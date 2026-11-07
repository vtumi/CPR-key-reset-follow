//! 通过固定宿主的连接测试路由激活账号周期，不读取凭据或构造模型请求
use crate::host::fault;
use gateway_plugin_sdk::{
    PluginFault,
    call::middleware::http::Version,
    client::{HostClient, HttpBody, HttpFrame, HttpRequest, HttpResponse},
};
use serde::Deserialize;
use std::time::Duration;

const LIMIT: usize = 256 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

fn query(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn request(uri: String) -> HttpRequest {
    HttpRequest {
        settings: serde_json::Value::Null,
        method: "GET".into(),
        uri,
        version: Version::Http11,
        headers: vec![],
        timeout_ms: Some(30_000),
        body: HttpBody::empty(),
    }
}
async fn body(response: &mut HttpResponse) -> Result<Vec<u8>, PluginFault> {
    let result = tokio::time::timeout(TIMEOUT, async {
        if response.status != 200 {
            return Err(fault("宿主连接测试接口拒绝请求"));
        }
        let mut bytes = Vec::new();
        while let Some(frame) = response.body.read().await? {
            if let HttpFrame::Data(data) = frame {
                if bytes.len().saturating_add(data.len()) > LIMIT {
                    return Err(fault("宿主连接测试响应过大"));
                }
                bytes.extend(data);
            }
        }
        Ok(bytes)
    })
    .await
    .unwrap_or_else(|_| Err(fault("宿主连接测试响应超时")));
    if result.is_err() {
        // 提前退出也关闭惰性正文，关闭失败不重发测试
        let _ = tokio::time::timeout(Duration::from_secs(1), response.body.close()).await;
    }
    result
}
async fn get(host: &HostClient, uri: String) -> Result<Vec<u8>, PluginFault> {
    let mut response = tokio::time::timeout(TIMEOUT, host.dispatch_http(request(uri)))
        .await
        .map_err(|_| fault("宿主连接测试请求超时"))??;
    body(&mut response).await
}
#[derive(Deserialize)]
struct Catalog {
    code: u32,
    data: Models,
}
#[derive(Deserialize)]
struct Models {
    models: Vec<Model>,
}
#[derive(Deserialize)]
struct Model {
    id: String,
}
fn model(bytes: &[u8]) -> Result<String, PluginFault> {
    let catalog: Catalog = serde_json::from_slice(bytes).map_err(|_| fault("宿主模型目录无效"))?;
    if catalog.code != 200 {
        return Err(fault("宿主模型目录读取失败"));
    }
    // 跟随宿主连接测试默认的目录顺序，不硬编码或猜测模型名
    catalog
        .data
        .models
        .into_iter()
        .next()
        .filter(|model| !model.id.trim().is_empty() && !model.id.chars().any(char::is_control))
        .map(|model| model.id)
        .ok_or_else(|| fault("账号没有可用于连接测试的模型"))
}
fn completed(bytes: &[u8]) -> Result<(), PluginFault> {
    let text = std::str::from_utf8(bytes).map_err(|_| fault("连接测试响应格式无效"))?;
    let normalized = text.replace("\r\n", "\n");
    let mut success = false;
    let mut events = normalized.split("\n\n").peekable();
    while let Some(event) = events.next() {
        if events.peek().is_none() && !event.trim().is_empty() {
            return Err(fault("连接测试响应未完整结束"));
        }
        let data = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(|data| data.strip_prefix(' ').unwrap_or(data))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(&data).map_err(|_| fault("连接测试事件格式无效"))?;
        match value.get("type").and_then(|v| v.as_str()) {
            Some("error") => return Err(fault("账号连接测试失败")),
            Some("test_complete")
                if value.get("success").and_then(|v| v.as_bool()) == Some(true) =>
            {
                success = true
            }
            Some("test_complete") => return Err(fault("账号连接测试未成功")),
            Some("test_start" | "request" | "content") => {}
            _ => return Err(fault("连接测试事件格式无效")),
        }
    }
    if success {
        Ok(())
    } else {
        Err(fault("连接测试缺少成功完成事件"))
    }
}
pub async fn activate(host: &HostClient, account: &str) -> Result<(), PluginFault> {
    let account = query(account);
    let catalog = get(
        host,
        format!("/api/admin/accounts/models?accountId={account}"),
    )
    .await?;
    let model = query(&model(&catalog)?);
    let stream = get(
        host,
        format!("/api/admin/accounts/connection-test?accountId={account}&modelId={model}"),
    )
    .await?;
    completed(&stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_requires_complete_success_and_rejects_errors() {
        assert!(completed(b"data: {\"type\":\"test_complete\",\"success\":true}\n\n").is_ok());
        assert!(
            completed(
                b": keep-alive\r\n\r\ndata: {\"type\":\"test_complete\",\"success\":true}\r\n\r\n"
            )
            .is_ok()
        );
        for bytes in [b"".as_slice(), b"data: {\"type\":\"content\",\"text\":\"hello\"}\n\n", b"data: {\"type\":\"test_complete\",\"success\":true}", b"data: {\"type\":\"test_complete\",\"success\":false}\n\n", b"data: not-json\n\n", b"data: {\"type\":\"test_complete\",\"success\":true}\n\ndata: {\"type\":\"error\"}\n\n"] {
            assert!(completed(bytes).is_err());
        }
    }
    #[test]
    fn catalog_order_and_query_encoding() {
        assert_eq!(
            model(br#"{"code":200,"data":{"models":[{"id":"configured-model"},{"id":"other"}]}}"#)
                .unwrap(),
            "configured-model"
        );
        assert!(model(br#"{"code":200,"data":{"models":[]}}"#).is_err());
        assert!(model(br#"{"code":1,"data":{"models":[{"id":"other"}]}}"#).is_err());
        assert_eq!(query("a&b=模型"), "a%26b%3D%E6%A8%A1%E5%9E%8B");
    }
    #[tokio::test]
    async fn response_status_and_size_are_bounded() {
        assert!(
            body(&mut HttpResponse::new(403, HttpBody::empty()))
                .await
                .is_err()
        );
        assert!(
            body(&mut HttpResponse::new(
                200,
                HttpBody::from_bytes(vec![b'x'; LIMIT + 1])
            ))
            .await
            .is_err()
        );
    }
}
