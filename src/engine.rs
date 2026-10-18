use crate::{
    cycle::{self, Decision, Sample},
    host::{Host, fault},
    model::{Link, Pending, Phase, State},
};
use gateway_plugin_sdk::PluginFault;

pub struct Engine<'a, H> {
    pub host: &'a H,
    pub state: State,
    pub version: Option<u64>,
}
impl<'a, H: Host> Engine<'a, H> {
    pub async fn load(host: &'a H) -> Result<Self, PluginFault> {
        let (state, version) = host.load().await?;
        Ok(Self {
            host,
            state,
            version,
        })
    }
    pub async fn save(&mut self) -> Result<(), PluginFault> {
        self.version = Some(self.host.save(&self.state, self.version).await?);
        Ok(())
    }
    pub async fn fresh(&self, account: &str) -> Result<Sample, PluginFault> {
        let facts = self.host.refresh(account).await?;
        if facts.account_id != account {
            return Err(fault("上游返回了其他账号的样本"));
        }
        cycle::sample(&facts).map_err(fault)
    }
    pub async fn add(&mut self, key: &str, account: &str, now: i64) -> Result<(), PluginFault> {
        if self.state.links.contains_key(key) {
            return Err(fault("该密钥已有关联"));
        }
        self.host.budget(key).await?;
        let baseline = self.fresh(account).await?;
        let phase = if cycle::floating(&baseline) {
            Phase::default()
        } else {
            Phase::Tracking
        };
        self.state.accounts.entry(account.into()).or_default();
        self.state.links.insert(
            key.into(),
            Link {
                key_id: key.into(),
                account_id: account.into(),
                baseline,
                paused: false,
                early_auto: None,
                pending: None,
                events: Default::default(),
                retry_at: 0,
                phase,
            },
        );
        self.state
            .links
            .get_mut(key)
            .unwrap()
            .event(now, "建立基线，不清零");
        self.save().await
    }
    pub async fn set_early_auto(&mut self, key: &str, enabled: bool) -> Result<(), PluginFault> {
        let legacy_default = self.state.early_auto;
        let link = self
            .state
            .links
            .get_mut(key)
            .ok_or_else(|| fault("关联不存在，请刷新列表"))?;
        if link.early_reset_enabled(legacy_default) != enabled {
            link.early_auto = Some(enabled);
            self.save().await?;
        }
        Ok(())
    }

    pub async fn check(&mut self, account_id: &str, now: i64) -> Result<(), PluginFault> {
        let facts = self.host.refresh(account_id).await;
        if let Ok(facts) = &facts {
            let changed = facts.windows.iter().any(|w| {
                w.key.starts_with("codex:604800s")
                    && (w.key != "codex:604800s" || w.window_seconds != Some(604800))
            });
            if facts.account_id == account_id && changed {
                for link in self
                    .state
                    .links
                    .values_mut()
                    .filter(|l| l.account_id == account_id && !l.paused && l.pending.is_none())
                {
                    if facts
                        .observed_at_ms
                        .is_some_and(|t| t > link.baseline.observed)
                    {
                        link.pending = Some(Pending::Changed {
                            windows: facts.windows.clone(),
                        });
                    }
                }
            }
        }
        let result = facts.and_then(|facts| {
            if facts.account_id != account_id {
                return Err(fault("账号样本不匹配"));
            }
            cycle::sample(&facts).map_err(fault)
        });
        let account = self.state.accounts.entry(account_id.into()).or_default();
        let sample = match result {
            Ok(s)
                if account
                    .sample
                    .as_ref()
                    .is_none_or(|prev| s.observed > prev.observed) =>
            {
                s
            }
            result => {
                account.failure_since.get_or_insert(now);
                account.failures = account.failures.saturating_add(1);
                account.next_check =
                    now + (30_000_i64.saturating_mul(1 << account.failures.min(6))).min(900_000);
                // 不透传可能包含代理地址或上游响应正文的错误详情。
                account.error = Some(match result {
                    Ok(_) => "上游仍返回旧样本".into(),
                    Err(e) => format!("无法刷新账号额度（{:?}）", e.code),
                });
                return self.save().await;
            }
        };
        account.sample = Some(sample.clone());
        account.next_check = now + 300_000;
        account.failure_since = None;
        account.failures = 0;
        account.error = None;
        // 旧版关闭提前清零时可能留下人工确认；仅使用新的有效样本安全地跳过该周期。
        // 不能清除结果未知等其他待处理状态，以免再次执行不可逆重置。
        let legacy_default = self.state.early_auto;
        {
            for link in self.state.links.values_mut().filter(|l| {
                !l.early_reset_enabled(legacy_default)
                    && l.account_id == account_id
                    && !l.paused
                    && matches!(
                        l.pending,
                        Some(Pending::Confirm {
                            reason: Decision::Early,
                            ..
                        })
                    )
            }) {
                if cycle::classify(&link.baseline, &sample) == Decision::Early {
                    link.baseline = sample.clone();
                    link.phase = if cycle::floating(&sample) {
                        Phase::default()
                    } else {
                        Phase::Tracking
                    };
                    link.pending = None;
                    link.event(now, "提前重置已忽略，未清零");
                }
            }
        }
        for link in self
            .state
            .links
            .values_mut()
            .filter(|l| l.account_id == account_id && !l.paused && l.pending.is_none())
        {
            if let Phase::Settling { candidate } = &mut link.phase {
                // 下一个完整周周期已越过上次结束时间，可独立处理正常换周。
                if cycle::classify(&link.baseline, &sample) == Decision::Normal {
                    link.pending = Some(Pending::Ready {
                        sample: sample.clone(),
                        reason: "正常换周".into(),
                    });
                    continue;
                }
                if cycle::floating(&sample) {
                    *candidate = None;
                } else if let Some(first) = candidate {
                    let reset_shift = i128::from(sample.reset) - i128::from(first.reset);
                    let stable = sample.observed - first.observed >= cycle::SETTLE_INTERVAL
                        && reset_shift.abs() <= 1_000;
                    if stable {
                        link.baseline = sample.clone();
                        link.phase = Phase::Tracking;
                    } else if reset_shift.abs() > 1_000 {
                        *candidate = Some(sample.clone());
                    }
                } else {
                    *candidate = Some(sample.clone());
                }
                continue;
            }
            let decision = cycle::classify(&link.baseline, &sample);
            match decision {
                Decision::Same => link.baseline.observed = sample.observed,
                Decision::Normal | Decision::Early
                    if decision == Decision::Normal || link.early_reset_enabled(legacy_default) =>
                {
                    link.pending = Some(Pending::Ready {
                        sample: sample.clone(),
                        reason: if decision == Decision::Normal {
                            "正常换周"
                        } else {
                            "提前重置"
                        }
                        .into(),
                    });
                }
                Decision::Early => {
                    // 管理员关闭提前清零后，新周期属于正常观察结果，不进入人工处理。
                    // 只更新基线，不能调用 reset；浮动窗口继续等待结束时间固定。
                    link.baseline = sample.clone();
                    link.phase = if cycle::floating(&sample) {
                        Phase::default()
                    } else {
                        Phase::Tracking
                    };
                    link.event(now, "提前重置已忽略，未清零");
                }
                Decision::Unexplained => {
                    link.pending = Some(Pending::Confirm {
                        sample: sample.clone(),
                        reason: decision,
                    });
                }
                // Normal 已在上面的条件分支处理；显式分支满足 Rust 穷尽性检查。
                Decision::Stale | Decision::Normal => {}
            }
        }
        self.save().await?;
        self.drain(now).await
    }
    pub async fn drain(&mut self, now: i64) -> Result<(), PluginFault> {
        let keys: Vec<_> = self
            .state
            .links
            .iter()
            .filter(|(_, l)| {
                !l.paused && l.retry_at <= now && matches!(l.pending, Some(Pending::Ready { .. }))
            })
            .take(8)
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            self.execute(&key, now).await?;
        }
        Ok(())
    }
    pub async fn execute(&mut self, key: &str, now: i64) -> Result<(), PluginFault> {
        let Some(Pending::Ready { sample, reason }) =
            self.state.links.get(key).and_then(|l| l.pending.clone())
        else {
            return Ok(());
        };
        // 停机或预算读取失败后，不能用过期的排队样本补做旧周期清零。
        if i128::from(now) - i128::from(sample.observed) > 300_000 {
            let link = self.state.links.get_mut(key).unwrap();
            link.pending = None;
            link.event(now, "排队样本已过期，等待重新检查当前周期");
            self.state
                .accounts
                .entry(link.account_id.clone())
                .or_default()
                .next_check = now;
            return self.save().await;
        }
        let before = match self.host.budget(key).await {
            Ok(b) => b,
            Err(_) => {
                self.state.links.get_mut(key).unwrap().retry_at = now + 300_000;
                self.state
                    .links
                    .get_mut(key)
                    .unwrap()
                    .event(now, "读取预算失败，尚未调用清零");
                return self.save().await;
            }
        };
        // 未完成意图就是未知结果，重启无需猜测调用是否曾发送。
        self.state.links.get_mut(key).unwrap().pending = Some(Pending::Unknown {
            sample: sample.clone(),
            reason: reason.clone(),
            before,
            error: None,
        });
        self.save().await?;
        let result = self.host.reset(key).await;
        let link = self.state.links.get_mut(key).unwrap();
        match result {
            Ok(()) => {
                link.baseline = sample;
                link.phase = Phase::default();
                link.pending = None;
                link.event(now, reason);
            }
            Err(e) => {
                if let Some(Pending::Unknown { error, .. }) = &mut link.pending {
                    *error = Some(format!("{:?}", e.code));
                }
                link.event(now, "结果未知，已停止自动重试");
            }
        }
        self.save().await
    }
    pub async fn action(
        &mut self,
        key: &str,
        action: &str,
        acknowledge: bool,
        now: i64,
    ) -> Result<(), PluginFault> {
        let link = self
            .state
            .links
            .get(key)
            .cloned()
            .ok_or_else(|| fault("关联不存在，请刷新页面"))?;
        match action {
            "pause" => {
                self.state.links.get_mut(key).unwrap().paused = true;
            }
            "delete" if acknowledge => {
                self.state.links.remove(key);
                self.state
                    .accounts
                    .retain(|id, _| self.state.links.values().any(|l| &l.account_id == id));
            }
            "resume" => {
                if !link.paused {
                    return Err(fault("关联未暂停"));
                }
                // 清零结果未知时不允许借由禁用/启用清除保护标记，避免重复清零。
                if matches!(link.pending, Some(Pending::Unknown { .. })) {
                    return Err(fault("清零结果未知，禁止重新启用此关联以免重复清零"));
                }
                let fresh = self.fresh(&link.account_id).await?;
                let l = self.state.links.get_mut(key).unwrap();
                l.baseline = fresh;
                l.phase = if cycle::floating(&l.baseline) {
                    Phase::default()
                } else {
                    Phase::Tracking
                };
                l.pending = None;
                l.paused = false;
                l.event(now, "启用关联并重新建立基线，未清零");
            }
            _ => return Err(fault("无效操作或缺少删除确认")),
        }
        self.save().await
    }
}
