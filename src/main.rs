use gateway_plugin_sdk::{
    PluginFault,
    call::{
        host::{AuthListRequest, AuthListResult, KeyListRequest},
        management::*,
    },
    client::{Empty, PluginBuilder, PluginSession, SessionConfig, TypedCall, TypedReply, methods},
};
use key_reset_follow::{
    engine::Engine,
    host::{Host, SdkHost, fault},
    model::State,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    action: String,
    key_id: Option<String>,
    account_id: Option<String>,
    enabled: Option<bool>,
    #[serde(default)]
    acknowledge: bool,
}
// 后台对账可频繁推进整个 State 的版本；操作只需确认用户选择的关联身份未变。
// 具体持久化仍由 host.state.put 的 expected_version 原子比较保护。
fn ensure_current_link(
    state: &State,
    key: &str,
    expected_account: Option<&str>,
) -> Result<(), PluginFault> {
    let link = state
        .links
        .get(key)
        .ok_or_else(|| fault("关联已不存在，请刷新列表"))?;
    if Some(link.account_id.as_str()) != expected_account {
        return Err(fault("关联账号已变化，请刷新列表"));
    }
    Ok(())
}
fn registration() -> ManagementRegistration {
    ManagementRegistration {
        routes: vec![
            ManagementRoute {
                method: "GET".into(),
                path: "state".into(),
                request_content_types: vec![],
                response_content_types: vec!["application/json".into()],
            },
            ManagementRoute {
                method: "POST".into(),
                path: "action".into(),
                request_content_types: vec!["application/json".into()],
                response_content_types: vec!["application/json".into()],
            },
        ],
        resources: ["web/index.html", "web/app.js", "web/app.css"]
            .into_iter()
            .map(|path| ManagementResource {
                path: path.into(),
                public: false,
            })
            .collect(),
        pages: vec![ManagementPage {
            id: "follow".into(),
            title: "跟随重置".into(),
            description: Some("密钥额度跟随账号重置".into()),
            entry: "web/index.html".into(),
            icon: None,
        }],
        callbacks: vec![],
    }
}
async fn handle(call: TypedCall<ManagementRequest>) -> Result<Value, PluginFault> {
    let host = SdkHost(call.host);
    let mut engine = Engine::load(&host).await?;
    if call.request.method == "POST" {
        let cmd: Command =
            serde_json::from_slice(&call.payload).map_err(|_| fault("请求格式不正确"))?;
        let key = cmd.key_id.as_deref().unwrap_or("");
        if matches!(
            cmd.action.as_str(),
            "pause" | "resume" | "delete" | "early_auto"
        ) {
            ensure_current_link(&engine.state, key, cmd.account_id.as_deref())?;
        }
        match cmd.action.as_str() {
            "add" => {
                engine
                    .add(
                        key,
                        cmd.account_id
                            .as_deref()
                            .ok_or_else(|| fault("请选择账号"))?,
                        now(),
                    )
                    .await?
            }
            "early_auto" => {
                let enabled = cmd.enabled.ok_or_else(|| fault("缺少开关值"))?;
                engine.set_early_auto(key, enabled).await?;
            }
            action => engine.action(key, action, cmd.acknowledge, now()).await?,
        }
        return Ok(json!({"ok": true}));
    }
    let mut keys = vec![];
    let mut cursor = None;
    loop {
        let page = host
            .0
            .list_keys(KeyListRequest { cursor, limit: 200 })
            .await?;
        keys.extend(page.keys);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let mut accounts = vec![];
    let mut cursor = None;
    loop {
        let reply = host
            .0
            .call(
                "host.auth.list",
                json!({}),
                serde_json::to_vec(&AuthListRequest {
                    provider_id: None,
                    cursor,
                    limit: 200,
                })
                .map_err(|_| fault("账号查询编码失败"))?,
            )
            .await
            .map_err(gateway_plugin_sdk::client::SessionError::into_plugin_fault)?;
        let page: AuthListResult =
            serde_json::from_slice(&reply.payload).map_err(|_| fault("账号目录解析失败"))?;
        accounts.extend(page.accounts.into_iter().map(|a| {
            json!({
                "account_id": a.account_id, "name": a.name, "email": a.email,
                "provider_id": a.provider_id, "authentication_kind": a.authentication_kind,
                "plan_type": a.plan_type,
            })
        }));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let mut budgets = serde_json::Map::new();
    for key in &keys {
        if let Ok(budget) = host.budget(&key.id).await {
            budgets.insert(
                key.id.clone(),
                json!({
                    "weekly_limit_usd": budget.weekly_limit_usd,
                    "weekly_used_usd": budget.weekly_used_usd,
                }),
            );
        }
    }
    // 管理页面只接收展示和操作所需的字段；周期、事件和未知清零记录保留在持久状态中。
    let links: serde_json::Map<String, Value> = engine
        .state
        .links
        .iter()
        .map(|(id, link)| {
            let pending = link.pending.as_ref().map(|pending| match pending {
                key_reset_follow::model::Pending::Changed { .. } => "changed",
                key_reset_follow::model::Pending::Confirm { .. } => "confirm",
                key_reset_follow::model::Pending::Ready { .. } => "ready",
                key_reset_follow::model::Pending::Unknown { .. } => "unknown",
            });
            (
                id.clone(),
                json!({
                    "key_id": link.key_id,
                    "account_id": link.account_id,
                    "baseline": { "reset": link.baseline.reset },
                    "paused": link.paused,
                    "early_auto": link.early_auto,
                    "pending": pending.map(|kind| json!({ "kind": kind })),
                }),
            )
        })
        .collect();
    let observations: serde_json::Map<String, Value> = engine
        .state
        .accounts
        .iter()
        .map(|(id, account)| {
            (
                id.clone(),
                json!({ "failure_since": account.failure_since }),
            )
        })
        .collect();
    let keys: Vec<Value> = keys
        .iter()
        .map(|key| json!({ "id": key.id, "name": key.name }))
        .collect();
    Ok(json!({
        "state": {
            "early_auto": engine.state.early_auto,
            "links": links,
            "accounts": observations,
        },
        "keys": keys,
        "accounts": accounts,
        "budgets": budgets,
    }))
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lock = Arc::new(Mutex::new(()));
    let management_lock = lock.clone();
    let plugin = PluginBuilder::from_json(include_bytes!("../plugin.json"))?
        .management(registration(), move |call| {
            let lock = management_lock.clone();
            async move {
                let _guard = lock.lock().await;
                let (status, body) = match handle(call).await {
                    Ok(v) => (200, v),
                    Err(e) => (400, json!({"error": e.message})),
                };
                Ok(TypedReply::new(ManagementResponse {
                    status,
                    content_type: "application/json".into(),
                    headers: vec![],
                })
                .with_payload(serde_json::to_vec(&body).unwrap()))
            }
        })?
        .on(methods::RECONCILE, move |call| {
            let lock = lock.clone();
            async move {
                let _guard = lock.lock().await;
                let host = SdkHost(call.host);
                let mut engine = Engine::load(&host).await?;
                engine.drain(now()).await?;
                let mut due: Vec<_> = engine
                    .state
                    .accounts
                    .iter()
                    .filter(|(id, a)| {
                        a.next_check <= now()
                            && engine
                                .state
                                .links
                                .values()
                                .any(|l| &l.account_id == *id && !l.paused)
                    })
                    .map(|(id, a)| (a.next_check, id.clone()))
                    .collect();
                due.sort();
                for (_, id) in due.into_iter().take(2) {
                    engine.check(&id, now()).await?;
                }
                Ok(TypedReply::new(Empty {}))
            }
        })?
        .build()?;
    PluginSession::accept(
        tokio::io::stdin(),
        tokio::io::stdout(),
        SessionConfig::default(),
    )
    .await?
    .run(plugin)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use key_reset_follow::{
        cycle::Sample,
        model::{Link, Phase},
    };

    #[test]
    fn association_target_stays_bound_to_original_account() {
        let mut state = State::default();
        state.links.insert(
            "key-1".into(),
            Link {
                key_id: "key-1".into(),
                account_id: "account-a".into(),
                baseline: Sample {
                    observed: 1,
                    reset: 2,
                    used: None,
                },
                paused: false,
                early_auto: None,
                pending: None,
                events: Default::default(),
                retry_at: 0,
                phase: Phase::Tracking,
            },
        );
        assert!(ensure_current_link(&state, "key-1", Some("account-a")).is_ok());
        assert!(ensure_current_link(&state, "key-1", Some("account-b")).is_err());
        assert!(ensure_current_link(&state, "key-1", None).is_err());
        assert!(ensure_current_link(&state, "missing", Some("account-a")).is_err());
        // 后台调整观测时间不改变关联身份，不应阻止操作。
        state.links.get_mut("key-1").unwrap().baseline.observed += 1;
        assert!(ensure_current_link(&state, "key-1", Some("account-a")).is_ok());
    }
}
