//! Machine-wide opt-in egress guard. Only bounded typed state crosses this
//! boundary; firewall calls are never made by unit tests or GUI processes.
use crate::EngineError;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::IpAddr,
};
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(windows)]
use windows as platform;
#[cfg(all(windows, test))]
pub(crate) use windows::DnsDropWatch;
const STATE: &str = "egress-guard-v1.json";

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    #[serde(default)]
    pub engine: crate::Engine,
    pub host: String,
    pub address: IpAddr,
    pub port: u16,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    version: u32,
    pub owner: String,
    pub endpoints: Vec<Endpoint>,
}
impl Policy {
    pub fn new(owner: String, endpoints: Vec<Endpoint>) -> Result<Self, EngineError> {
        let policy = Self {
            version: 1,
            owner,
            endpoints,
        };
        policy.validate()?;
        Ok(policy)
    }
    fn validate(&self) -> Result<(), EngineError> {
        if self.version != 1
            || self.owner.is_empty()
            || self.owner.len() > 184
            || self.owner.chars().any(char::is_control)
            || self.endpoints.is_empty()
            || self.endpoints.len() > 32
            || self.endpoints.iter().any(|e| {
                e.host.is_empty()
                    || e.host.len() > 253
                    || e.host.chars().any(char::is_control)
                    || e.port == 0
                    || e.address.is_unspecified()
                    || e.address.is_multicast()
            })
        {
            return Err(EngineError::InvalidConfig);
        }
        Ok(())
    }
    pub fn address(&self, host: &str, port: u16) -> Option<IpAddr> {
        self.endpoints
            .iter()
            .find(|e| e.host == host && e.port == port)
            .map(|e| e.address)
    }
    pub fn address_for(&self, profile: &moth_core::Profile) -> Option<IpAddr> {
        self.endpoints
            .iter()
            .find(|e| {
                e.host == profile.host
                    && e.port == profile.port
                    && e.engine == crate::profile_engine(profile)
            })
            .map(|e| e.address)
    }
}
fn path() -> Result<std::path::PathBuf, EngineError> {
    moth_ipc::maintenance::state_directory()
        .map(|p| p.join(STATE))
        .map_err(|_| EngineError::Network)
}
pub fn load() -> Result<Option<Policy>, EngineError> {
    let path = path()?;
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(EngineError::Network),
        Ok(m) if !m.is_file() || m.len() > 16384 => return Err(EngineError::Network),
        _ => {}
    }
    moth_ipc::maintenance::validate_protected_path(&path).map_err(|_| EngineError::Network)?;
    let mut data = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| EngineError::Network)?
        .take(16385)
        .read_to_end(&mut data)
        .map_err(|_| EngineError::Network)?;
    let policy: Policy = serde_json::from_slice(&data).map_err(|_| EngineError::InvalidConfig)?;
    policy.validate()?;
    Ok(Some(policy))
}
fn save(policy: &Policy) -> Result<(), EngineError> {
    let destination = path()?;
    let stage = destination.with_file_name(format!("guard-stage-{}", uuid::Uuid::new_v4()));
    let mut file =
        moth_ipc::maintenance::create_private_file(&stage).map_err(|_| EngineError::Network)?;
    let outcome = (|| {
        file.write_all(&serde_json::to_vec(policy).map_err(|_| EngineError::InvalidConfig)?)
            .map_err(|_| EngineError::Network)?;
        file.sync_all().map_err(|_| EngineError::Network)?;
        drop(file);
        // Updates are protected by the controller operation lock and installer
        // maintenance lease. The state is opt-in intent, never proof of filtering.
        #[cfg(windows)]
        windows::replace_file(&stage, &destination)?;
        #[cfg(target_os = "linux")]
        std::fs::rename(&stage, &destination).map_err(|_| EngineError::Network)?;
        #[cfg(target_os = "linux")]
        std::fs::File::open(destination.parent().ok_or(EngineError::Network)?)
            .map_err(|_| EngineError::Network)?
            .sync_all()
            .map_err(|_| EngineError::Network)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(stage);
    outcome
}
pub fn enable(policy: &Policy) -> Result<(), EngineError> {
    policy.validate()?;
    let old = load()?;
    save(policy)?;
    if let Err(error) = platform::apply(policy) {
        // Persist enabled intent after partial/crashed activation. A restart
        // retries default-deny rather than silently clearing protection.
        if old.is_some() {
            let _ = restore();
        }
        return Err(error);
    }
    Ok(())
}
pub fn restore() -> Result<(), EngineError> {
    if let Some(policy) = load()? {
        platform::apply(&policy)?;
    }
    Ok(())
}
pub fn active() -> Result<bool, EngineError> {
    platform::active()
}
pub fn release() -> Result<(), EngineError> {
    platform::remove()?;
    match std::fs::remove_file(path()?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(EngineError::Network),
    }
}
pub fn refresh_interface() -> Result<(), EngineError> {
    if let Some(policy) = load()? {
        platform::apply(&policy)?;
    }
    Ok(())
}
/// Resolve before routing changes. Once enabled, never perform a direct DNS
/// fallback: reconnect/failover use only the explicitly recorded endpoints.
pub async fn resolve(profile: &moth_core::Profile) -> Result<std::net::SocketAddr, EngineError> {
    if let Some(policy) = load()? {
        return policy
            .address_for(profile)
            .map(|ip| std::net::SocketAddr::new(ip, profile.port))
            .ok_or(EngineError::Network);
    }
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::lookup_host((profile.host.as_str(), profile.port)),
    )
    .await
    .map_err(|_| EngineError::Timeout)?
    .map_err(|_| EngineError::Network)?
    .next()
    .ok_or(EngineError::Network)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_policy_pins_only_explicit_endpoints() {
        let p = Policy::new(
            "owner".into(),
            vec![Endpoint {
                engine: crate::Engine::Xray,
                host: "vpn.example".into(),
                address: "192.0.2.1".parse().unwrap(),
                port: 443,
            }],
        )
        .unwrap();
        assert_eq!(
            p.address("vpn.example", 443),
            Some("192.0.2.1".parse().unwrap())
        );
        assert!(p.address("different.example", 443).is_none());
        let mut invalid = p.clone();
        invalid.endpoints[0].port = 0;
        assert!(invalid.validate().is_err());
        invalid = p;
        invalid.endpoints.resize(33, invalid.endpoints[0].clone());
        assert!(invalid.validate().is_err());
    }
}
