//! Approved standalone policy core. The service owns its listeners and API.
use super::*;
use moth_core::{ClashPolicy, ConnectionSettings, GroupState};
use serde_json::{json, Value};
use std::{
    net::{SocketAddr, TcpListener, UdpSocket},
    sync::Arc,
};

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
struct Pins {
    version: String,
    platforms: BTreeMap<String, Asset>,
    geo: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    file: String,
    sha256: String,
}
fn pins() -> Result<Pins, EngineError> {
    serde_json::from_str(include_str!("../../../policy-assets.lock.json"))
        .map_err(|_| EngineError::Integrity)
}
impl EngineSet {
    pub(super) fn verified_mihomo(&self) -> Result<PathBuf, EngineError> {
        let lock = pins()?;
        let pin = lock
            .platforms
            .get(Self::platform()?)
            .ok_or(EngineError::Unsupported)?;
        self.verify_asset(&pin.file, &pin.sha256)
    }
    pub fn mihomo_status(&self) -> EngineStatus {
        let result = self.verified_mihomo().and_then(|_| {
            for geo in pins()?.geo {
                self.verify_asset(&geo.file, &geo.sha256)?;
            }
            Ok(())
        });
        EngineStatus {
            name: "mihomo".into(),
            version: pins().map(|p| p.version).unwrap_or_default(),
            available: result.is_ok(),
            message: result.err().map(|e| e.to_string()),
        }
    }
    pub async fn validate_clash(&self, policy: &ClashPolicy) -> Result<(), EngineError> {
        let bridge = LocalBridge {
            port: 18080,
            credential: Uuid::new_v4(),
        };
        let prepared = Prepared::new(policy, &Default::default(), &bridge, None, false)?;
        let home = self.policy_home(&std::env::temp_dir())?;
        self.check_policy(&prepared.config, home.path()).await
    }
    fn policy_home(&self, parent: &Path) -> Result<tempfile::TempDir, EngineError> {
        let home = tempfile::Builder::new()
            .prefix("moth-policy-")
            .tempdir_in(parent)
            .map_err(|_| EngineError::Process)?;
        for geo in pins()?.geo {
            let source = self.verify_asset(&geo.file, &geo.sha256)?;
            std::fs::copy(source, home.path().join(&geo.file))
                .map_err(|_| EngineError::Integrity)?;
        }
        Ok(home)
    }
    async fn check_policy(&self, config: &Value, home: &Path) -> Result<(), EngineError> {
        let mut process = self
            .spawn_config_at(Engine::Mihomo, config, true, Some(home))
            .await?;
        let result = timeout(Duration::from_secs(8), process.child.wait())
            .await
            .map_err(|_| EngineError::Timeout)?
            .map_err(|_| EngineError::Process)?;
        if !result.success() {
            return Err(EngineError::InvalidConfig);
        }
        Ok(())
    }
}

pub(super) struct Prepared {
    config: Value,
    dns: UdpSocket,
    control_port: TcpListener,
    control: Arc<Control>,
    bridge_port: u16,
    bridge_password: Zeroizing<String>,
}
impl Prepared {
    pub(super) fn new(
        policy: &ClashPolicy,
        settings: &ConnectionSettings,
        bridge: &LocalBridge,
        interface: Option<&str>,
        mark: bool,
    ) -> Result<Self, EngineError> {
        let mut config = policy
            .configuration()
            .map_err(|_| EngineError::InvalidConfig)?;
        let dns = UdpSocket::bind("127.0.0.1:0").map_err(|_| EngineError::Process)?;
        let dns_address = dns.local_addr().map_err(|_| EngineError::Process)?;
        let control_port = TcpListener::bind("127.0.0.1:0").map_err(|_| EngineError::Process)?;
        let addr = control_port
            .local_addr()
            .map_err(|_| EngineError::Process)?;
        let token = Zeroizing::new(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ));
        let bridge_password = Zeroizing::new(bridge.credential.simple().to_string());
        config["mode"] = json!("rule");
        config["ipv6"] = json!(settings.ipv6);
        config["log-level"] = json!("silent");
        config["geo-auto-update"] = json!(false);
        config["geodata-mode"] = json!(true);
        config["geox-url"] =
            json!({"geoip":"http://127.0.0.1:1/disabled","geosite":"http://127.0.0.1:1/disabled"});
        config["find-process-mode"] = json!("off");
        config["profile"] = json!({"store-selected":false,"store-fake-ip":false});
        config["mixed-port"] = json!(bridge.port);
        config["allow-lan"] = json!(false);
        config["bind-address"] = json!("127.0.0.1");
        config["authentication"] = json!([format!("moth:{}", bridge_password.as_str())]);
        config["external-controller"] = json!(addr.to_string());
        config["secret"] = json!(token.as_str());
        if let Some(interface) = interface {
            config["interface-name"] = json!(interface);
        }
        if mark {
            config["routing-mark"] = json!(0x4d4f5448u32);
        }
        if config.get("dns").is_none() {
            config["dns"] = json!({
                "enable":true,"nameserver":[settings.dns],"default-nameserver":[settings.bootstrap_dns],
                "proxy-server-nameserver":[settings.bootstrap_dns]
            });
        }
        config["dns"]["use-system-hosts"] = json!(false);
        config["dns"]["listen"] = json!(dns_address.to_string());
        if config["dns"].get("ipv6").is_none() {
            config["dns"]["ipv6"] = json!(settings.ipv6);
        }
        let control = Arc::new(Control {
            address: addr,
            groups: policy.groups(),
            allows_downloads: !policy.has_direct_exit(),
            token,
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(2))
                .build()
                .map_err(|_| EngineError::Process)?,
        });
        Ok(Self {
            config,
            dns,
            control_port,
            control,
            bridge_port: bridge.port,
            bridge_password,
        })
    }
    pub(super) fn configure_capture(
        &self,
        config: &mut Value,
        dns_capture: bool,
    ) -> Result<(), EngineError> {
        config["outbounds"][0] = json!({
            "type":"socks","tag":"proxy","server":"127.0.0.1","server_port":self.bridge_port,
            "inet4_bind_address":"127.0.0.1","version":"5","username":"moth","password":self.bridge_password.as_str()
        });
        if dns_capture {
            let port = self
                .dns
                .local_addr()
                .map_err(|_| EngineError::Process)?
                .port();
            config["dns"] = json!({"servers":[{"type":"udp","tag":"policy-dns","server":"127.0.0.1",
                "server_port":port,"inet4_bind_address":"127.0.0.1"}],"final":"policy-dns","disable_cache":true});
        }
        if let Some(route) = config["route"].as_object_mut() {
            route.remove("default_domain_resolver");
        }
        Ok(())
    }
    pub(super) async fn start(
        self,
        engines: &EngineSet,
        protected: bool,
    ) -> Result<(EngineProcess, Arc<Control>), EngineError> {
        let parent = if protected {
            let path =
                moth_ipc::maintenance::state_directory().map_err(|_| EngineError::Integrity)?;
            moth_ipc::maintenance::validate_protected_path(&path)
                .map_err(|_| EngineError::Integrity)?;
            path
        } else {
            std::env::temp_dir()
        };
        let home = engines.policy_home(&parent)?;
        engines.check_policy(&self.config, home.path()).await?;
        drop(self.dns);
        drop(self.control_port);
        let mut process = engines
            .spawn_config_at(Engine::Mihomo, &self.config, false, Some(home.path()))
            .await?;
        process._data = Some(home);
        let start = Instant::now();
        loop {
            if process
                .child
                .try_wait()
                .map_err(|_| EngineError::Process)?
                .is_some()
            {
                return Err(EngineError::Process);
            }
            if self.control.probe().await
                && tokio::net::TcpStream::connect(("127.0.0.1", self.bridge_port))
                    .await
                    .is_ok()
            {
                return Ok((process, self.control));
            }
            if start.elapsed() > Duration::from_secs(4) {
                return Err(EngineError::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

pub struct Control {
    address: SocketAddr,
    groups: Vec<moth_core::GroupPreview>,
    pub allows_downloads: bool,
    token: Zeroizing<String>,
    client: reqwest::Client,
}
impl Control {
    fn url(&self, path: &[&str]) -> reqwest::Url {
        let mut url = reqwest::Url::parse(&format!("http://{}", self.address))
            .expect("local controller address");
        url.path_segments_mut().expect("HTTP URL").extend(path);
        url
    }
    async fn probe(&self) -> bool {
        self.client
            .get(self.url(&["version"]))
            .bearer_auth(self.token.as_str())
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
    }
    async fn read(&self, path: &[&str]) -> Result<Value, EngineError> {
        let mut response = self
            .client
            .get(self.url(path))
            .bearer_auth(self.token.as_str())
            .send()
            .await
            .map_err(|_| EngineError::Process)?;
        if !response.status().is_success() {
            return Err(EngineError::Process);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| EngineError::Process)? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(EngineError::Process);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| EngineError::Process)
    }
    pub async fn groups(&self) -> Result<Vec<GroupState>, EngineError> {
        let current = self.read(&["proxies"]).await?;
        self.groups
            .iter()
            .map(|group| {
                let status = &current["proxies"][&group.name];
                Ok(GroupState {
                    name: group.name.clone(),
                    kind: group.kind.clone(),
                    selected: status["now"]
                        .as_str()
                        .filter(|s| s.len() <= 160)
                        .map(str::to_owned),
                    members: status["all"]
                        .as_array()
                        .ok_or(EngineError::Process)?
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|s| s.len() <= 160)
                        .map(str::to_owned)
                        .collect(),
                })
            })
            .collect()
    }
    pub async fn select(&self, group: &str, member: &str) -> Result<(), EngineError> {
        if !self
            .groups
            .iter()
            .any(|g| g.name == group && g.kind == "select")
        {
            return Err(EngineError::InvalidConfig);
        }
        let group_state = self.read(&["proxies", group]).await?;
        if !group_state["all"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row == member))
        {
            return Err(EngineError::InvalidConfig);
        }
        let result = self
            .client
            .put(self.url(&["proxies", group]))
            .bearer_auth(self.token.as_str())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(json!({"name":member}).to_string())
            .send()
            .await
            .map_err(|_| EngineError::Process)?;
        if !result.status().is_success() {
            return Err(EngineError::InvalidConfig);
        }
        Ok(())
    }
}
