//! Linux TUN owns a private link and two explicitly journaled policy rules.
//! It never flushes rule tables or rewrites physical adapter DNS/resolv.conf.
use super::*;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    process::Command as ProcessCommand,
};

const LINK: &str = "moth0";
const RUNTIME: &str = "/run/moth-desktop";
const LEASE: &str = "/run/moth-desktop/network-v1.json";
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    kind: String,
    auxiliary_priority: u32,
}
const TABLE: &str = "20220";
const PRIORITY: &str = "20220";
const PROTOCOL: &str = "242";
const MARK: u32 = 0x4d4f5448;
const MARK_TEXT: &str = "0x4d4f5448";

// Fixed trusted executables, no shell and no caller-selected arguments/paths.
// A reader thread drains bounded output while a deadline bounds each child.
fn command(program: &str, arguments: &[&str]) -> Result<Vec<u8>, EngineError> {
    let mut child = ProcessCommand::new(program)
        .args(arguments)
        .env_clear()
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| EngineError::Network)?;
    let stdout = child.stdout.take().ok_or(EngineError::Process)?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(1024 * 1024).read_to_end(&mut bytes)?;
        Ok::<_, std::io::Error>(bytes)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < Duration::from_secs(3) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| EngineError::Process)?
        .map_err(|_| EngineError::Network)?;
    if !matches!(status, Some(status) if status.success()) || bytes.len() >= 1024 * 1024 {
        return Err(EngineError::Network);
    }
    Ok(bytes)
}
fn ip(arguments: &[&str]) -> Result<Vec<u8>, EngineError> {
    command("/usr/sbin/ip", arguments)
}
fn array(bytes: &[u8]) -> Result<Vec<serde_json::Value>, EngineError> {
    serde_json::from_slice(bytes).map_err(|_| EngineError::Network)
}
fn number(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        let text = value.as_str()?;
        if let Some(hex) = text.strip_prefix("0x") {
            u64::from_str_radix(hex, 16).ok()
        } else {
            text.parse().ok()
        }
    })
}
fn rules(family: &str) -> Result<Vec<serde_json::Value>, EngineError> {
    array(&ip(&[family, "-N", "-j", "rule", "show"])?)
}
fn reserved(rule: &serde_json::Value) -> bool {
    number(&rule["priority"]) == Some(20220) || number(&rule["table"]) == Some(20220)
}
fn owned(rule: &serde_json::Value) -> bool {
    number(&rule["priority"]) == Some(20220)
        && number(&rule["table"]) == Some(20220)
        && number(&rule["protocol"]) == Some(242)
        && number(&rule["fwmark"]) == Some(MARK as u64)
        && (rule.get("fwmask").is_none() || number(&rule["fwmask"]) == Some(u32::MAX as u64))
        && rule.get("not").is_some()
        && rule["src"].as_str() == Some("all")
        && rule.as_object().is_some_and(|object| {
            object.keys().all(|key| {
                [
                    "priority", "table", "protocol", "fwmark", "fwmask", "not", "src",
                ]
                .contains(&key.as_str())
            })
        })
}
fn owned_auxiliary(rule: &serde_json::Value, priority: u32) -> bool {
    number(&rule["priority"]) == Some(priority as u64)
        && number(&rule["table"]) == Some(20220)
        && rule["src"].as_str() == Some("all")
        && rule["oif"].as_str() == Some(LINK)
        && rule
            .get("protocol")
            .is_none_or(|value| number(value) == Some(0))
        && rule.as_object().is_some_and(|object| {
            object.keys().all(|key| {
                [
                    "priority",
                    "src",
                    "oif",
                    "oif_detached",
                    "table",
                    "protocol",
                ]
                .contains(&key.as_str())
            })
        })
}
fn runtime() -> Result<(), EngineError> {
    for path in ["/run", RUNTIME] {
        let metadata = fs::symlink_metadata(path).map_err(|_| EngineError::Network)?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(EngineError::Network);
        }
    }
    Ok(())
}

/// Called on daemon startup and by systemd ExecStopPost, after the service cgroup
/// has been killed. Only exact journaled rules are removed; conflicts fail closed.
pub fn recover_network() -> Result<(), EngineError> {
    runtime()?;
    // Never clear the journal while sing-tun can still create its auxiliary
    // rule. Cancellation kills asynchronously; the caller retries after reap.
    if Path::new("/sys/class/net/moth0").exists() {
        return Err(EngineError::Network);
    }
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(LEASE)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(EngineError::Network),
    };
    let metadata = file.metadata().map_err(|_| EngineError::Network)?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.mode() & 0o077 != 0
        || metadata.len() > 256
    {
        return Err(EngineError::Network);
    }
    let mut bytes = Vec::new();
    file.take(257)
        .read_to_end(&mut bytes)
        .map_err(|_| EngineError::Network)?;
    let journal: Journal = serde_json::from_slice(&bytes).map_err(|_| EngineError::Network)?;
    if journal.version != 1
        || journal.kind != "moth-network"
        || journal.auxiliary_priority == 0
        || journal.auxiliary_priority >= u32::MAX - 1
        || journal.auxiliary_priority == 20220
    {
        return Err(EngineError::Network);
    }
    for family in ["-4", "-6"] {
        let entries = rules(family)?;
        let entries: Vec<_> = entries
            .iter()
            .filter(|rule| {
                reserved(rule)
                    || family == "-6"
                        && (rule["oif"].as_str() == Some(LINK)
                            || number(&rule["priority"]) == Some(journal.auxiliary_priority as u64))
            })
            .collect();
        let main: Vec<_> = entries.iter().filter(|rule| owned(rule)).collect();
        let auxiliary: Vec<_> = entries
            .iter()
            .filter(|rule| family == "-6" && owned_auxiliary(rule, journal.auxiliary_priority))
            .collect();
        if main.len() > 1 || auxiliary.len() > 1 || main.len() + auxiliary.len() != entries.len() {
            return Err(EngineError::Network);
        }
        if !auxiliary.is_empty() {
            ip(&[
                "-6",
                "rule",
                "del",
                "pref",
                &journal.auxiliary_priority.to_string(),
                "oif",
                LINK,
                "table",
                TABLE,
            ])?;
        }
        if !main.is_empty() {
            ip(&[
                family, "rule", "del", "pref", PRIORITY, "not", "fwmark", MARK_TEXT, "table",
                TABLE, "protocol", PROTOCOL,
            ])?;
        }
        if rules(family)?.iter().any(|rule| {
            reserved(rule)
                || family == "-6"
                    && (rule["oif"].as_str() == Some(LINK)
                        || number(&rule["priority"]) == Some(journal.auxiliary_priority as u64))
        }) {
            return Err(EngineError::Network);
        }
    }
    fs::remove_file(LEASE).map_err(|_| EngineError::Network)?;
    fs::File::open(RUNTIME)
        .and_then(|file| file.sync_all())
        .map_err(|_| EngineError::Network)?;
    Ok(())
}

struct NetworkLease;
impl NetworkLease {
    fn acquire() -> Result<Self, EngineError> {
        runtime()?;
        if Path::new("/sys/class/net/moth0").exists() {
            return Err(EngineError::Network);
        }
        for family in ["-4", "-6"] {
            if rules(family)?
                .iter()
                .any(|rule| reserved(rule) || rule["oif"].as_str() == Some(LINK))
            {
                return Err(EngineError::Network);
            }
            if array(&ip(&[family, "-N", "-j", "route", "show", "table", "all"])?)?
                .iter()
                .any(|route| number(&route["table"]) == Some(20220))
            {
                return Err(EngineError::Network);
            }
        }
        // sing-tun adds an IPv6 oif rule even with auto_route=false. Its exact
        // priority is one below the smallest positive existing IPv6 priority.
        // Record that ownership before spawning it, and use our reserved table.
        let auxiliary_priority = rules("-6")?
            .iter()
            .filter_map(|rule| number(&rule["priority"]))
            .filter(|priority| *priority > 0)
            .min()
            .map(|priority| priority.min(20220))
            .and_then(|priority| priority.checked_sub(1))
            .filter(|priority| {
                *priority > 0 && *priority != 20220 && *priority < u32::MAX as u64 - 1
            })
            .ok_or(EngineError::Network)? as u32;
        // Require the supported resolved stub setup before any network mutation.
        if fs::canonicalize("/etc/resolv.conf").map_err(|_| EngineError::Network)?
            != Path::new("/run/systemd/resolve/stub-resolv.conf")
        {
            return Err(EngineError::Network);
        }
        command(
            "/usr/bin/systemctl",
            &["is-active", "--quiet", "systemd-resolved.service"],
        )?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(LEASE)
            .map_err(|_| EngineError::Network)?;
        let journal = serde_json::to_vec(&Journal {
            version: 1,
            kind: "moth-network".into(),
            auxiliary_priority,
        })
        .map_err(|_| EngineError::Network)?;
        file.write_all(&journal)
            .and_then(|_| file.sync_all())
            .map_err(|_| EngineError::Network)?;
        fs::File::open(RUNTIME)
            .and_then(|file| file.sync_all())
            .map_err(|_| EngineError::Network)?;
        let lease = Self;
        // Install the journaled IPv6 selector before sing-tun starts. Otherwise
        // its nextIndex6 calculation could race activate() and choose a
        // different priority from the one recorded above. The empty table
        // falls through to the existing main routes until activation.
        ip(&[
            "-6", "rule", "add", "pref", PRIORITY, "not", "fwmark", MARK_TEXT, "table", TABLE,
            "protocol", PROTOCOL,
        ])?;
        Ok(lease)
    }
    fn activate(&self) -> Result<(), EngineError> {
        for family in ["-4", "-6"] {
            ip(&[
                family, "route", "add", "default", "dev", LINK, "table", TABLE, "proto", PROTOCOL,
            ])?;
            if family == "-4" {
                ip(&[
                    family, "rule", "add", "pref", PRIORITY, "not", "fwmark", MARK_TEXT, "table",
                    TABLE, "protocol", PROTOCOL,
                ])?;
            }
        }
        // Only this private link's resolver state changes. Link deletion makes
        // resolved discard it, including after SIGKILL; physical DNS stays intact.
        command("/usr/bin/resolvectl", &["dns", LINK, "172.19.0.2"])?;
        command("/usr/bin/resolvectl", &["domain", LINK, "~."])?;
        command("/usr/bin/resolvectl", &["default-route", LINK, "yes"])?;
        Ok(())
    }
}

pub struct Tunnel {
    tun: EngineProcess,
    upstream: EngineProcess,
    _lease: NetworkLease,
    _maintenance: moth_ipc::maintenance::TunnelLease,
}
impl Tunnel {
    pub fn alive(&mut self) -> bool {
        matches!(self.tun.child.try_wait(), Ok(None))
            && matches!(self.upstream.child.try_wait(), Ok(None))
    }
    pub async fn stop(mut self) {
        let _ = self.tun.stop().await;
        let _ = self.upstream.stop().await;
    }
}

impl EngineSet {
    pub async fn connect(&self, profile: &Profile) -> Result<Tunnel, EngineError> {
        let maintenance =
            moth_ipc::maintenance::TunnelLease::acquire().map_err(|_| EngineError::Maintenance)?;
        let address = timeout(
            Duration::from_secs(5),
            tokio::net::lookup_host((profile.host.as_str(), profile.port)),
        )
        .await
        .map_err(|_| EngineError::Timeout)?
        .map_err(|_| EngineError::Network)?
        .next()
        .ok_or(EngineError::Network)?;
        let family = if address.is_ipv4() { "-4" } else { "-6" };
        let routes = array(&ip(&[
            family,
            "-j",
            "route",
            "get",
            &address.ip().to_string(),
        ])?)?;
        let interface = routes
            .first()
            .and_then(|route| route["dev"].as_str())
            .ok_or(EngineError::Network)?;
        if interface.is_empty()
            || interface.len() > 15
            || !interface
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
            || interface == LINK
            || interface.starts_with("tun")
            || interface.starts_with("wg")
        {
            return Err(EngineError::Network);
        }
        let reservation =
            std::net::TcpListener::bind("127.0.0.1:0").map_err(|_| EngineError::Process)?;
        let bridge = LocalBridge {
            port: reservation
                .local_addr()
                .map_err(|_| EngineError::Process)?
                .port(),
            credential: Uuid::new_v4(),
        };
        let mut config = moth_core::xray_bridge_config(profile, &bridge);
        let outbound = &mut config["outbounds"][0];
        let key = if profile.protocol == moth_core::Protocol::Vless {
            "vnext"
        } else {
            "servers"
        };
        outbound["settings"][key][0]["address"] = serde_json::json!(address.ip().to_string());
        outbound["streamSettings"]["sockopt"] =
            serde_json::json!({"interface":interface,"mark":MARK});
        let mut tun_config = moth_core::sing_box_tun_config(&bridge);
        tun_config["inbounds"][0]["interface_name"] = serde_json::json!(LINK);
        tun_config["inbounds"][0]["auto_route"] = serde_json::json!(false);
        tun_config["inbounds"][0]["strict_route"] = serde_json::json!(false);
        tun_config["inbounds"][0]["iproute2_table_index"] = serde_json::json!(20220);
        self.validate(Engine::Xray, &config).await?;
        self.validate(Engine::SingBox, &tun_config).await?;
        let lease = NetworkLease::acquire()?;
        drop(reservation);
        let mut upstream = self.spawn_config(Engine::Xray, &config, false).await?;
        let start = Instant::now();
        loop {
            if upstream
                .child
                .try_wait()
                .map_err(|_| EngineError::Process)?
                .is_some()
            {
                return Err(EngineError::Process);
            }
            if tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, bridge.port))
                .await
                .is_ok()
            {
                break;
            }
            if start.elapsed() > Duration::from_secs(3) {
                return Err(EngineError::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let tun = self
            .spawn_config(Engine::SingBox, &tun_config, false)
            .await?;
        let mut session = Tunnel {
            tun,
            upstream,
            _lease: lease,
            _maintenance: maintenance,
        };
        let start = Instant::now();
        while !Path::new("/sys/class/net/moth0").exists() {
            if !session.alive() {
                return Err(EngineError::Process);
            }
            if start.elapsed() > Duration::from_secs(3) {
                return Err(EngineError::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        session._lease.activate()?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .local_address(IpAddr::V4(Ipv4Addr::new(172, 19, 0, 1)))
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(4))
            .build()
            .map_err(|_| EngineError::Probe)?;
        let start = Instant::now();
        loop {
            if !session.alive() {
                return Err(EngineError::Process);
            }
            if matches!(client.get("https://cp.cloudflare.com/generate_204").send().await,Ok(response) if response.status()==reqwest::StatusCode::NO_CONTENT)
                && session.alive()
            {
                return Ok(session);
            }
            if start.elapsed() > Duration::from_secs(12) {
                return Err(EngineError::Probe);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

impl Drop for NetworkLease {
    fn drop(&mut self) {
        // On failure keep the journal for ExecStopPost/startup recovery.
        let _ = recover_network();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auxiliary_rule_requires_exact_journaled_identity() {
        let rule = serde_json::json!({"priority":20219,"src":"all","oif":"moth0","oif_detached":null,"table":"20220"});
        assert!(owned_auxiliary(&rule, 20219));
        for (key, value) in [
            ("priority", serde_json::json!(20218)),
            ("table", serde_json::json!(254)),
            ("oif", serde_json::json!("eth0")),
            ("src", serde_json::json!("10.0.0.0/8")),
            ("protocol", serde_json::json!(242)),
            ("fwmark", serde_json::json!(1)),
        ] {
            let mut foreign = rule.clone();
            foreign[key] = value;
            assert!(!owned_auxiliary(&foreign, 20219));
        }
    }
}
