use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrafficSnapshot {
    pub session: String,
    pub elapsed_seconds: u64,
    pub uploaded_bytes: u64,
    pub downloaded_bytes: u64,
    pub upload_per_second: Option<u64>,
    pub download_per_second: Option<u64>,
    pub connections_total: usize,
    pub connections: Vec<ConnectionSummary>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionSummary {
    pub network: String,
    pub destination_ip: Option<std::net::IpAddr>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub process: Option<String>,
    pub uploaded_bytes: u64,
    pub downloaded_bytes: u64,
    pub via: String,
}
