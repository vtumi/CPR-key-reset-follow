use gateway_plugin_sdk::{
    PluginFault,
    call::{
        data::{QuotaFacts, QuotaWindowFacts},
        key_budgets::KeyBudget,
    },
};
use key_reset_follow::{
    cycle::WEEK,
    engine::Engine,
    host::{Host, fault},
    model::{Pending, State},
};
use std::{collections::BTreeMap, sync::Mutex};
#[derive(Default)]
struct Storage {
    state: State,
    version: u64,
    resets: Vec<String>,
    refreshes: usize,
    time: i64,
    reset_at: i64,
    fail_save_at: Option<u64>,
    lost_response: Option<String>,
    hang_reset: bool,
    missing_window: bool,
    changed_window: bool,
    fail_budget: Option<String>,
    weekly_used: BTreeMap<String, String>,
    used_percent: f64,
}
struct Fake(Mutex<Storage>);
impl Fake {
    fn new() -> Self {
        Self(Mutex::new(Storage {
            time: 1000000,
            reset_at: WEEK,
            used_percent: 10.0,
            ..Default::default()
        }))
    }
}
impl Host for Fake {
    async fn load(&self) -> Result<(State, Option<u64>), PluginFault> {
        let s = self.0.lock().unwrap();
        Ok((s.state.clone(), (s.version > 0).then_some(s.version)))
    }
    async fn save(&self, state: &State, version: Option<u64>) -> Result<u64, PluginFault> {
        let mut s = self.0.lock().unwrap();
        if version.unwrap_or(0) != s.version || s.fail_save_at == Some(s.version + 1) {
            return Err(fault("injected state failure"));
        }
        s.version += 1;
        s.state = state.clone();
        Ok(s.version)
    }
    async fn refresh(&self, account: &str) -> Result<QuotaFacts, PluginFault> {
        let mut s = self.0.lock().unwrap();
        s.refreshes += 1;
        Ok(QuotaFacts {
            schema_version: 1,
            account_id: account.into(),
            observed_at_ms: Some(s.time),
            windows: if s.missing_window {
                vec![]
            } else {
                vec![QuotaWindowFacts {
                    key: if s.changed_window {
                        "codex:604800s:secondary_window"
                    } else {
                        "codex:604800s"
                    }
                    .into(),
                    window_seconds: Some(604800),
                    used_percent: Some(s.used_percent),
                    reset_at_ms: Some(s.reset_at),
                }]
            },
        })
    }
    async fn budget(&self, key: &str) -> Result<KeyBudget, PluginFault> {
        if self.0.lock().unwrap().fail_budget.as_deref() == Some(key) {
            return Err(fault("budget unavailable"));
        }
        Ok(KeyBudget {
            client_key_id: key.into(),
            daily_limit_usd: "0".into(),
            weekly_limit_usd: "60".into(),
            daily_used_usd: "0".into(),
            weekly_used_usd: self
                .0
                .lock()
                .unwrap()
                .weekly_used
                .get(key)
                .cloned()
                .unwrap_or_else(|| "12".into()),
            daily_resets_at_ms: None,
            weekly_resets_at_ms: Some(WEEK),
        })
    }
    async fn reset(&self, key: &str) -> Result<(), PluginFault> {
        let hang = self.0.lock().unwrap().hang_reset;
        if hang {
            std::future::pending::<()>().await;
        }
        let mut s = self.0.lock().unwrap();
        s.resets.push(key.into());
        s.weekly_used.insert(key.into(), "0".into());
        if s.lost_response.as_deref() == Some(key) {
            Err(fault("response lost after commit"))
        } else {
            Ok(())
        }
    }
}
async fn setup(h: &Fake) {
    let mut e = Engine::load(h).await.unwrap();
    e.add("key-a", "account-a", 1000000).await.unwrap();
    e.add("key-b", "account-a", 1000000).await.unwrap();
}
fn next(h: &Fake) {
    let mut s = h.0.lock().unwrap();
    s.time = WEEK + 1000;
    s.reset_at = WEEK * 2;
}
#[tokio::test]
async fn shared_refresh_and_partial_unknown_do_not_repeat_success() {
    let h = Fake::new();
    setup(&h).await;
    assert!(h.0.lock().unwrap().resets.is_empty());
    next(&h);
    h.0.lock().unwrap().lost_response = Some("key-b".into());
    let before = h.0.lock().unwrap().refreshes;
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().refreshes, before + 1);
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-a", "key-b"]);
    Engine::load(&h)
        .await
        .unwrap()
        .drain(WEEK + 2000)
        .await
        .unwrap();
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 2000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-b"].pending,
        Some(Pending::Unknown { .. })
    ));
}
#[tokio::test]
async fn completion_write_failure_cannot_repeat_reset() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    // check 保存待处理、保存意图后，故障发生于完成落盘。
    h.0.lock().unwrap().fail_save_at = Some(5);
    assert!(
        Engine::load(&h)
            .await
            .unwrap()
            .check("account-a", WEEK + 1000)
            .await
            .is_err()
    );
    h.0.lock().unwrap().fail_save_at = None;
    Engine::load(&h)
        .await
        .unwrap()
        .drain(WEEK + 2000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-a", "key-b"]);
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-a"].pending,
        Some(Pending::Unknown { .. })
    ));
}
#[tokio::test]
async fn failed_intent_write_does_not_call_reset() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().fail_save_at = Some(4);
    assert!(
        Engine::load(&h)
            .await
            .unwrap()
            .check("account-a", WEEK + 1000)
            .await
            .is_err()
    );
    assert!(h.0.lock().unwrap().resets.is_empty());
}
#[tokio::test]
async fn early_switch_skips_reset_without_manual_processing() {
    let h = Fake::new();
    setup(&h).await;
    {
        let mut storage = h.0.lock().unwrap();
        storage.state.early_auto = false;
        storage.time = 2000000;
        storage.reset_at = WEEK + 1500000;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", 2000000)
        .await
        .unwrap();
    let storage = h.0.lock().unwrap();
    assert!(storage.resets.is_empty());
    assert!(storage.state.links["key-a"].pending.is_none());
    assert!(storage.state.links["key-b"].pending.is_none());
    assert_eq!(storage.state.links["key-a"].baseline.reset, WEEK + 1500000);
}

#[tokio::test]
async fn concurrent_stale_state_cannot_claim_reset() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    let mut old = Engine::load(&h).await.unwrap();
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert!(old.check("account-a", WEEK + 1000).await.is_err());
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
}

#[tokio::test]
async fn termination_after_durable_intent_never_replays() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().hang_reset = true;
    let mut e = Engine::load(&h).await.unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            e.check("account-a", WEEK + 1000)
        )
        .await
        .is_err()
    );
    drop(e);
    h.0.lock().unwrap().hang_reset = false;
    Engine::load(&h)
        .await
        .unwrap()
        .drain(WEEK + 2000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b"]);
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-a"].pending,
        Some(Pending::Unknown { .. })
    ));
}
#[tokio::test]
async fn pause_resume_and_accounts_are_independent() {
    let h = Fake::new();
    setup(&h).await;
    Engine::load(&h)
        .await
        .unwrap()
        .add("key-c", "account-b", 1000000)
        .await
        .unwrap();
    Engine::load(&h)
        .await
        .unwrap()
        .action("key-a", "pause", false, 1000000)
        .await
        .unwrap();
    next(&h);
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b"]);
    Engine::load(&h)
        .await
        .unwrap()
        .action("key-a", "resume", false, WEEK + 2000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b"]);
    assert_eq!(
        h.0.lock().unwrap().state.links["key-c"].baseline.reset,
        WEEK
    );
}
#[tokio::test]
async fn missing_samples_keep_baseline_and_changed_identity_requires_confirmation() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().missing_window = true;
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert_eq!(
        h.0.lock().unwrap().state.links["key-a"].baseline.reset,
        WEEK
    );
    assert!(h.0.lock().unwrap().resets.is_empty());
    {
        let mut s = h.0.lock().unwrap();
        s.missing_window = false;
        s.changed_window = true;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 2000)
        .await
        .unwrap();
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-a"].pending,
        Some(Pending::Changed { .. })
    ));
    assert!(h.0.lock().unwrap().resets.is_empty());
}
#[tokio::test]
async fn budget_failure_leaves_other_keys_progressing_and_retries_only_unclaimed_key() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().fail_budget = Some("key-a".into());
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b"]);
    h.0.lock().unwrap().fail_budget = None;
    Engine::load(&h)
        .await
        .unwrap()
        .drain(WEEK + 301000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b", "key-a"]);
}
#[tokio::test]
async fn early_auto_is_enabled_by_default_and_unknown_retry_requires_acknowledgement() {
    let h = Fake::new();
    setup(&h).await;
    {
        let mut s = h.0.lock().unwrap();
        s.time = 2000000;
        s.reset_at = WEEK + 1500000;
        s.lost_response = Some("key-a".into());
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", 2000000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
    // 取消人工重试入口后，结果未知仍不得被重复执行。
    Engine::load(&h)
        .await
        .unwrap()
        .drain(2000000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
}

#[tokio::test]
async fn prolonged_outage_discards_queued_cycle_before_any_reset() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().fail_budget = Some("key-a".into());
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    h.0.lock().unwrap().fail_budget = None;
    Engine::load(&h)
        .await
        .unwrap()
        .drain(WEEK * 3)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b"]);
    assert!(h.0.lock().unwrap().state.links["key-a"].pending.is_none());
    {
        let mut s = h.0.lock().unwrap();
        s.time = WEEK * 3;
        s.reset_at = WEEK * 4;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK * 3)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b", "key-a", "key-b"]);
    assert_eq!(
        h.0.lock().unwrap().state.links["key-a"].baseline.reset,
        WEEK * 4
    );
}

#[tokio::test]
async fn early_auto_toggle_preserves_pending_reset_state() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().lost_response = Some("key-a".into());
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    let before = serde_json::to_value(&h.0.lock().unwrap().state).unwrap();
    let mut engine = Engine::load(&h).await.unwrap();
    engine.state.early_auto = false;
    engine.save().await.unwrap();
    let after = serde_json::to_value(&h.0.lock().unwrap().state).unwrap();
    assert_eq!(after["links"], before["links"]);
    assert_eq!(after["accounts"], before["accounts"]);
    assert_eq!(after["early_auto"], false);
    assert!(after.get("display_timezone").is_none());
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
}

#[tokio::test]
async fn rolling_unused_window_resets_once_and_records_one_event_per_key() {
    let h = Fake::new();
    setup(&h).await;
    let first = 2_000_000;
    {
        let mut s = h.0.lock().unwrap();
        s.time = first;
        s.reset_at = first + WEEK;
        s.used_percent = 0.0;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", first)
        .await
        .unwrap();
    {
        let mut s = h.0.lock().unwrap();
        assert_eq!(s.resets, ["key-a", "key-b"]);
        s.weekly_used.insert("key-a".into(), "7".into());
    }
    let first_event = h.0.lock().unwrap().state.links["key-a"]
        .events
        .back()
        .unwrap()
        .at;
    for i in 1..=12 {
        let t = first + i * 300_000;
        {
            let mut s = h.0.lock().unwrap();
            s.time = t;
            s.reset_at = t + WEEK;
        }
        Engine::load(&h)
            .await
            .unwrap()
            .check("account-a", t)
            .await
            .unwrap();
        let s = h.0.lock().unwrap();
        assert_eq!(s.resets, ["key-a", "key-b"], "poll {i}");
        assert_eq!(s.weekly_used["key-a"], "7");
        for key in ["key-a", "key-b"] {
            let events = &s.state.links[key].events;
            assert_eq!(events.iter().filter(|e| e.reason == "提前重置").count(), 1);
            assert_eq!(events.back().unwrap().at, first_event);
            assert!(matches!(
                s.state.links[key].phase,
                key_reset_follow::model::Phase::Settling { .. }
            ));
        }
    }
    let fixed_reset = h.0.lock().unwrap().reset_at;
    for i in 13..=15 {
        let t = first + i * 300_000;
        h.0.lock().unwrap().time = t;
        Engine::load(&h)
            .await
            .unwrap()
            .check("account-a", t)
            .await
            .unwrap();
    }
    {
        let s = h.0.lock().unwrap();
        assert_eq!(s.resets.len(), 2);
        assert_eq!(s.state.links["key-a"].baseline.reset, fixed_reset);
        assert!(matches!(
            s.state.links["key-a"].phase,
            key_reset_follow::model::Phase::Tracking
        ));
    }
    let t = first + 20 * 300_000;
    {
        let mut s = h.0.lock().unwrap();
        s.time = t;
        s.reset_at = fixed_reset + 1_500_000;
        s.used_percent = 2.0;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", t)
        .await
        .unwrap();
    let s = h.0.lock().unwrap();
    assert_eq!(s.resets.len(), 4);
    assert_eq!(
        s.state.links["key-a"]
            .events
            .iter()
            .filter(|e| e.reason == "提前重置")
            .count(),
        2
    );
}

#[tokio::test]
async fn legacy_link_without_phase_waits_for_anchor_without_replaying_reset() {
    let h = Fake::new();
    setup(&h).await;
    let first = 2_000_000;
    {
        let mut s = h.0.lock().unwrap();
        s.time = first;
        s.reset_at = first + WEEK;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", first)
        .await
        .unwrap();
    let mut old = serde_json::to_value(&h.0.lock().unwrap().state).unwrap();
    for link in old["links"].as_object_mut().unwrap().values_mut() {
        link.as_object_mut().unwrap().remove("phase");
    }
    h.0.lock().unwrap().state = serde_json::from_value(old).unwrap();
    for i in 1..=3 {
        let t = first + i * 300_000;
        {
            let mut s = h.0.lock().unwrap();
            s.time = t;
            s.reset_at = t + WEEK;
        }
        Engine::load(&h)
            .await
            .unwrap()
            .check("account-a", t)
            .await
            .unwrap();
    }
    let s = h.0.lock().unwrap();
    assert_eq!(s.resets, ["key-a", "key-b"]);
    assert_eq!(
        s.state.links["key-a"]
            .events
            .iter()
            .filter(|e| e.reason == "提前重置")
            .count(),
        1
    );
}

#[tokio::test]
async fn small_forward_shifts_do_not_accumulate_into_a_second_reset() {
    let h = Fake::new();
    setup(&h).await;
    let first = 2_000_000;
    {
        let mut s = h.0.lock().unwrap();
        s.time = first;
        s.reset_at = first + WEEK;
        s.used_percent = 0.0;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", first)
        .await
        .unwrap();
    for i in 1..=20 {
        let t = first + i * 300_000;
        {
            let mut s = h.0.lock().unwrap();
            s.time = t;
            s.reset_at = first + WEEK + i * 60_000;
        }
        Engine::load(&h)
            .await
            .unwrap()
            .check("account-a", t)
            .await
            .unwrap();
        let s = h.0.lock().unwrap();
        assert_eq!(s.resets.len(), 2, "poll {i}");
        assert!(matches!(
            s.state.links["key-a"].phase,
            key_reset_follow::model::Phase::Settling { .. }
        ));
        assert!(s.state.links["key-a"].pending.is_none());
    }
}

#[tokio::test]
async fn new_and_resumed_links_wait_for_a_floating_window_to_settle() {
    let h = Fake::new();
    let first = 2_000_000;
    {
        let mut s = h.0.lock().unwrap();
        s.time = first;
        s.reset_at = first + WEEK;
        s.used_percent = 0.0;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .add("key-a", "account-a", first)
        .await
        .unwrap();
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-a"].phase,
        key_reset_follow::model::Phase::Settling { .. }
    ));
    Engine::load(&h)
        .await
        .unwrap()
        .action("key-a", "pause", false, first)
        .await
        .unwrap();
    let resumed = first + 300_000;
    {
        let mut s = h.0.lock().unwrap();
        s.time = resumed;
        s.reset_at = resumed + WEEK;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .action("key-a", "resume", false, resumed)
        .await
        .unwrap();
    for i in 2..=5 {
        let t = first + i * 300_000;
        {
            let mut s = h.0.lock().unwrap();
            s.time = t;
            s.reset_at = t + WEEK;
        }
        Engine::load(&h)
            .await
            .unwrap()
            .check("account-a", t)
            .await
            .unwrap();
    }
    let s = h.0.lock().unwrap();
    assert!(s.resets.is_empty());
    assert!(s.state.links["key-a"].pending.is_none());
}

#[tokio::test]
async fn unknown_reset_cannot_be_cleared_by_pause_and_resume() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().lost_response = Some("key-a".into());
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-a"].pending,
        Some(Pending::Unknown { .. })
    ));
    Engine::load(&h)
        .await
        .unwrap()
        .action("key-a", "pause", false, WEEK + 2000)
        .await
        .unwrap();
    assert!(
        Engine::load(&h)
            .await
            .unwrap()
            .action("key-a", "resume", false, WEEK + 3000)
            .await
            .is_err()
    );
    let s = h.0.lock().unwrap();
    assert!(s.state.links["key-a"].paused);
    assert!(matches!(
        s.state.links["key-a"].pending,
        Some(Pending::Unknown { .. })
    ));
    assert_eq!(s.resets.len(), 2);
}

#[tokio::test]
async fn per_key_early_reset_can_be_disabled_independently() {
    let h = Fake::new();
    setup(&h).await;
    let mut engine = Engine::load(&h).await.unwrap();
    engine.set_early_auto("key-a", false).await.unwrap();
    assert!(!h.0.lock().unwrap().state.links["key-a"].early_reset_enabled(true));
    assert!(h.0.lock().unwrap().state.links["key-b"].early_reset_enabled(true));
    {
        let mut storage = h.0.lock().unwrap();
        storage.time = 2_000_000;
        storage.reset_at = WEEK + 1_500_000;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", 2_000_000)
        .await
        .unwrap();
    let storage = h.0.lock().unwrap();
    assert_eq!(storage.resets, ["key-b"]);
    assert_eq!(
        storage.state.links["key-a"].baseline.reset,
        WEEK + 1_500_000
    );
    assert!(storage.state.links["key-a"].pending.is_none());
    assert!(storage.state.links["key-b"].pending.is_none());
}

#[tokio::test]
async fn per_key_switch_does_not_clear_unknown_reset_or_change_other_links() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().lost_response = Some("key-a".into());
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    let before = serde_json::to_value(&h.0.lock().unwrap().state).unwrap();
    Engine::load(&h)
        .await
        .unwrap()
        .set_early_auto("key-a", false)
        .await
        .unwrap();
    let after = serde_json::to_value(&h.0.lock().unwrap().state).unwrap();
    assert_eq!(after["accounts"], before["accounts"]);
    assert_eq!(after["links"]["key-b"], before["links"]["key-b"]);
    assert_eq!(
        after["links"]["key-a"]["pending"],
        before["links"]["key-a"]["pending"]
    );
    assert_eq!(after["links"]["key-a"]["early_auto"], false);
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
}

#[test]
fn old_state_preserves_global_early_reset_default() {
    let old = serde_json::json!({
        "early_auto": false,
        "links": {
            "key-a": {
                "key_id": "key-a",
                "account_id": "account-a",
                "baseline": {"observed": 1, "reset": 604800000, "used": null},
                "paused": false,
                "pending": null,
                "events": [],
                "retry_at": 0,
                "phase": {"kind": "tracking"}
            }
        },
        "accounts": {}
    });
    let state: State = serde_json::from_value(old).unwrap();
    assert!(!state.links["key-a"].early_reset_enabled(state.early_auto));
    assert!(state.links["key-a"].early_auto.is_none());
    let restored: State = serde_json::from_value(serde_json::to_value(state).unwrap()).unwrap();
    assert!(!restored.links["key-a"].early_reset_enabled(restored.early_auto));
}

#[tokio::test]
async fn stale_resume_cannot_roll_back_a_completed_cycle() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    let mut engine = Engine::load(&h).await.unwrap();
    engine.check("account-a", WEEK + 1000).await.unwrap();
    engine
        .action("key-a", "pause", false, WEEK + 2000)
        .await
        .unwrap();
    let before = serde_json::to_value(&engine.state).unwrap();
    {
        let mut s = h.0.lock().unwrap();
        s.time = 1_000_000;
        s.reset_at = WEEK;
        s.weekly_used.insert("key-a".into(), "7".into());
    }
    let result = engine.action("key-a", "resume", false, WEEK + 3000).await;
    let after = serde_json::to_value(&engine.state).unwrap();
    {
        let mut s = h.0.lock().unwrap();
        s.time = WEEK + 4000;
        s.reset_at = WEEK * 2;
    }
    engine.check("account-a", WEEK + 4000).await.unwrap();
    let s = h.0.lock().unwrap();
    assert_eq!(s.resets, ["key-a", "key-b"]);
    assert_eq!(s.weekly_used["key-a"], "7");
    assert!(result.is_err());
    assert_eq!(after, before);
}

#[tokio::test]
async fn stale_changed_window_does_not_latch_pending_while_settling() {
    for stale_time in [WEEK + 2000, WEEK + 301000] {
        let h = Fake::new();
        setup(&h).await;
        next(&h);
        let mut engine = Engine::load(&h).await.unwrap();
        engine.check("account-a", WEEK + 1000).await.unwrap();
        h.0.lock().unwrap().time = WEEK + 301000;
        engine.check("account-a", WEEK + 301000).await.unwrap();
        let before = serde_json::to_value(&engine.state.links).unwrap();
        {
            let mut s = h.0.lock().unwrap();
            s.time = stale_time;
            s.changed_window = true;
        }
        engine.check("account-a", WEEK + 401000).await.unwrap();
        assert_eq!(serde_json::to_value(&engine.state.links).unwrap(), before);
        assert_eq!(engine.state.accounts["account-a"].failures, 1);
        {
            let mut s = h.0.lock().unwrap();
            s.time = WEEK + 601000;
            s.changed_window = false;
        }
        engine.check("account-a", WEEK + 601000).await.unwrap();
        assert!(matches!(
            engine.state.links["key-a"].phase,
            key_reset_follow::model::Phase::Tracking
        ));
        assert_eq!(h.0.lock().unwrap().resets, ["key-a", "key-b"]);
    }
}

#[tokio::test]
async fn baseline_refresh_rejects_regression_without_account_sample() {
    let h = Fake::new();
    setup(&h).await;
    let mut engine = Engine::load(&h).await.unwrap();
    engine
        .action("key-a", "pause", false, 1_000_000)
        .await
        .unwrap();
    assert!(engine.state.accounts["account-a"].sample.is_none());
    h.0.lock().unwrap().time = 999_999;
    let before = serde_json::to_value(&engine.state).unwrap();
    assert!(
        engine
            .action("key-a", "resume", false, 1_000_001)
            .await
            .is_err()
    );
    assert!(engine.add("key-c", "account-a", 1_000_001).await.is_err());
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}

#[tokio::test]
async fn newer_resume_can_recover_a_corrected_window_without_consuming_shared_sample() {
    let h = Fake::new();
    setup(&h).await;
    let mut engine = Engine::load(&h).await.unwrap();
    engine
        .action("key-a", "pause", false, 1_000_000)
        .await
        .unwrap();
    {
        let mut s = h.0.lock().unwrap();
        s.time = 2_000_000;
        s.reset_at = WEEK - 300_000;
    }
    engine
        .action("key-a", "resume", false, 2_000_000)
        .await
        .unwrap();
    assert_eq!(engine.state.links["key-a"].baseline.reset, WEEK - 300_000);
    assert!(engine.state.accounts["account-a"].sample.is_none());
    assert!(h.0.lock().unwrap().resets.is_empty());
    engine.check("account-a", 2_000_000).await.unwrap();
    assert!(matches!(
        engine.state.links["key-b"].pending,
        Some(Pending::Confirm { .. })
    ));
}

#[tokio::test]
async fn stale_link_sample_cannot_mutate_settling_after_resume() {
    let h = Fake::new();
    setup(&h).await;
    let mut engine = Engine::load(&h).await.unwrap();
    engine
        .action("key-a", "pause", false, 1_000_000)
        .await
        .unwrap();
    {
        let mut s = h.0.lock().unwrap();
        s.time = 3_000_000;
        s.reset_at = WEEK + 3_000_000;
    }
    engine
        .action("key-a", "resume", false, 3_000_000)
        .await
        .unwrap();
    let before = serde_json::to_value(&engine.state.links["key-a"]).unwrap();
    {
        let mut s = h.0.lock().unwrap();
        s.time = 2_000_000;
        s.reset_at = WEEK;
    }
    engine.check("account-a", 3_000_001).await.unwrap();
    assert_eq!(
        serde_json::to_value(&engine.state.links["key-a"]).unwrap(),
        before
    );
    assert!(h.0.lock().unwrap().resets.is_empty());
}

#[tokio::test]
async fn association_creation_time_survives_changes_and_restarts() {
    let h = Fake::new();
    let mut e = Engine::load(&h).await.unwrap();
    h.0.lock().unwrap().time = 1234567;
    e.add("key-a", "account-a", 1234567).await.unwrap();
    assert_eq!(e.state.links["key-a"].created_at, Some(1234567));
    h.0.lock().unwrap().time = 2000000;
    e.action("key-a", "pause", false, 2000000).await.unwrap();
    h.0.lock().unwrap().time = 3000000;
    e.action("key-a", "resume", false, 3000000).await.unwrap();
    e.set_early_auto("key-a", false).await.unwrap();
    let e = Engine::load(&h).await.unwrap();
    assert_eq!(e.state.links["key-a"].created_at, Some(1234567));
    let value = serde_json::to_value(&e.state).unwrap();
    let restored: State = serde_json::from_value(value).unwrap();
    assert_eq!(restored.links["key-a"].created_at, Some(1234567));
}

#[tokio::test]
async fn legacy_association_creation_time_remains_unknown() {
    let h = Fake::new();
    setup(&h).await;
    let mut value = serde_json::to_value(&h.0.lock().unwrap().state).unwrap();
    value["links"]["key-a"]
        .as_object_mut()
        .unwrap()
        .remove("created_at");
    let restored: State = serde_json::from_value(value).unwrap();
    assert_eq!(restored.links["key-a"].created_at, None);
    h.0.lock().unwrap().state = restored;
    let mut e = Engine::load(&h).await.unwrap();
    h.0.lock().unwrap().time = 2000000;
    e.action("key-a", "pause", false, 2000000).await.unwrap();
    h.0.lock().unwrap().time = 3000000;
    e.action("key-a", "resume", false, 3000000).await.unwrap();
    assert_eq!(e.state.links["key-a"].created_at, None);
    h.0.lock().unwrap().time = 4000000;
    e.action("key-a", "delete", true, 4000000).await.unwrap();
    h.0.lock().unwrap().time = 5000000;
    e.add("key-a", "account-a", 5000000).await.unwrap();
    assert_eq!(e.state.links["key-a"].created_at, Some(5000000));
}
