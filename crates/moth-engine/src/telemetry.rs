//! Read-only sampling of the pinned sing-box Clash API. The ephemeral controller
//! is loopback-only; its random bearer key never leaves the native service.
use crate::EngineError;
use moth_ipc::{ConnectionSummary, TrafficSnapshot};
use serde_json::Value;
use std::{
    net::TcpListener,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use uuid::Uuid;
use zeroize::Zeroizing;
pub struct Api {
    port: u16,
    secret: Zeroizing<String>,
    session: String,
    started: Instant,
    previous: Mutex<Option<(Instant, u64, u64)>>,
}
impl Api {
    pub fn configure(config: &mut Value) -> Result<(Arc<Self>, TcpListener), EngineError> {
        let reservation = TcpListener::bind("127.0.0.1:0").map_err(|_| EngineError::Process)?;
        let port = reservation
            .local_addr()
            .map_err(|_| EngineError::Process)?
            .port();
        let secret = Zeroizing::new(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ));
        config["experimental"]["clash_api"] = serde_json::json!({"external_controller":format!("127.0.0.1:{port}"),"secret":secret.as_str(),"access_control_allow_origin":["http://moth.invalid"],"access_control_allow_private_network":false});
        Ok((
            Arc::new(Self {
                port,
                secret,
                session: Uuid::new_v4().simple().to_string(),
                started: Instant::now(),
                previous: Mutex::new(None),
            }),
            reservation,
        ))
    }
    pub fn session(&self) -> &str {
        &self.session
    }
    #[cfg(test)]
    pub(crate) async fn assert_requires_authentication(&self) {
        let response = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://127.0.0.1:{}/connections", self.port))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    }
    pub async fn snapshot(&self) -> Result<TrafficSnapshot, EngineError> {
        let mut previous = self.previous.lock().await;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|_| EngineError::Probe)?;
        let mut response = client
            .get(format!("http://127.0.0.1:{}/connections", self.port))
            .bearer_auth(self.secret.as_str())
            .send()
            .await
            .map_err(|_| EngineError::Probe)?;
        if response.status() != 200 || response.content_length().is_some_and(|n| n > 1024 * 1024) {
            return Err(EngineError::Probe);
        }
        let mut bytes = Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await.map_err(|_| EngineError::Probe)? {
            if bytes.len() + chunk.len() > 1024 * 1024 {
                return Err(EngineError::Probe);
            }
            bytes.extend_from_slice(&chunk);
        }
        let document: Value = serde_json::from_slice(&bytes).map_err(|_| EngineError::Probe)?;
        let mut result = parse(&document)?;
        result.session = self.session.clone();
        result.elapsed_seconds = self.started.elapsed().as_secs();
        let now = Instant::now();
        if let Some((at, up, down)) = *previous {
            let millis = now.duration_since(at).as_millis();
            if (500..=5000).contains(&millis)
                && result.uploaded_bytes >= up
                && result.downloaded_bytes >= down
            {
                let rate = |bytes: u64| {
                    u64::try_from(u128::from(bytes) * 1000 / millis).unwrap_or(u64::MAX)
                };
                result.upload_per_second = Some(rate(result.uploaded_bytes - up));
                result.download_per_second = Some(rate(result.downloaded_bytes - down));
            }
        }
        *previous = Some((now, result.uploaded_bytes, result.downloaded_bytes));
        Ok(result)
    }
}
fn parse(document: &Value) -> Result<TrafficSnapshot, EngineError> {
    let upload = document["uploadTotal"].as_u64().ok_or(EngineError::Probe)?;
    let download = document["downloadTotal"]
        .as_u64()
        .ok_or(EngineError::Probe)?;
    let rows = match document.get("connections").ok_or(EngineError::Probe)? {
        Value::Null => &[][..],
        Value::Array(rows) => rows.as_slice(),
        _ => return Err(EngineError::Probe),
    };
    let connections = rows
        .iter()
        .take(128)
        .map(|row| {
            let m = &row["metadata"];
            let host = m["host"]
                .as_str()
                .filter(|s| {
                    !s.is_empty()
                        && s.len() <= 253
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"._-:[]".contains(&b))
                })
                .map(str::to_owned);
            let process = m["processPath"]
                .as_str()
                .filter(|s| s.contains('/') || s.contains('\\'))
                .and_then(|s| s.split(" (").next())
                .and_then(|s| s.rsplit(['/', '\\']).next())
                .filter(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
                .map(str::to_owned);
            let via = row["chains"]
                .as_array()
                .and_then(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .find(|s| ["proxy", "direct"].contains(s))
                })
                .unwrap_or("unknown")
                .to_owned();
            Ok(ConnectionSummary {
                network: match m["network"].as_str() {
                    Some("tcp") => "tcp",
                    Some("udp") => "udp",
                    _ => "unknown",
                }
                .into(),
                destination_ip: m["destinationIP"].as_str().and_then(|s| s.parse().ok()),
                host,
                port: m["destinationPort"].as_str().and_then(|s| s.parse().ok()),
                process,
                uploaded_bytes: row["upload"].as_u64().ok_or(EngineError::Probe)?,
                downloaded_bytes: row["download"].as_u64().ok_or(EngineError::Probe)?,
                via,
            })
        })
        .collect::<Result<Vec<_>, EngineError>>()?;
    Ok(TrafficSnapshot {
        session: String::new(),
        elapsed_seconds: 0,
        uploaded_bytes: upload,
        downloaded_bytes: download,
        upload_per_second: None,
        download_per_second: None,
        connections_total: rows.len(),
        connections,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summaries_omit_api_ids_paths_users_and_raw_rules() {
        let raw = serde_json::json!({"uploadTotal":123,"downloadTotal":456,"connections":[{"id":"synthetic-api-uuid","upload":10,"download":20,"rule":"secret-rule","metadata":{"network":"tcp","destinationIP":"192.0.2.2","destinationPort":"443","host":"example.com","processPath":"C:\\Users\\synthetic-user\\App.exe (synthetic-user)"},"chains":["proxy"]}]});
        let parsed = parse(&raw).unwrap();
        assert_eq!(parsed.uploaded_bytes, 123);
        assert_eq!(parsed.connections[0].process.as_deref(), Some("App.exe"));
        let json = serde_json::to_string(&parsed).unwrap();
        assert!(
            !json.contains("synthetic-user")
                && !json.contains("synthetic-api-uuid")
                && !json.contains("secret-rule")
        );
        assert!(parse(&serde_json::json!({"connections":[]})).is_err());
    }
}
