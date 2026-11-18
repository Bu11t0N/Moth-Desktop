use super::{EngineError, Policy};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const TABLE: &str = "moth_egress_guard";
const OWNER: &str = "Moth egress guard v1";

fn nft(args: &[&str], input: Option<&str>) -> Result<Vec<u8>, EngineError> {
    let mut child = Command::new("/usr/sbin/nft")
        .args(args)
        .env_clear()
        .env("LANG", "C")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| EngineError::Network)?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .ok_or(EngineError::Process)?
            .write_all(input.as_bytes())
            .map_err(|_| EngineError::Network)?;
    }
    let output = child.stdout.take().ok_or(EngineError::Process)?;
    let reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        output.take(1024 * 1024).read_to_end(&mut b).map(|_| b)
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
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
    if !status.is_some_and(|s| s.success()) || bytes.len() >= 1024 * 1024 {
        return Err(EngineError::Network);
    }
    Ok(bytes)
}
fn exists() -> Result<bool, EngineError> {
    let document: serde_json::Value =
        serde_json::from_slice(&nft(&["-j", "list", "tables"], None)?)
            .map_err(|_| EngineError::Network)?;
    Ok(document["nftables"]
        .as_array()
        .ok_or(EngineError::Network)?
        .iter()
        .any(|v| v["table"]["family"] == "inet" && v["table"]["name"] == TABLE))
}
fn owned() -> Result<bool, EngineError> {
    if !exists()? {
        return Ok(false);
    }
    let document: serde_json::Value =
        serde_json::from_slice(&nft(&["-j", "list", "table", "inet", TABLE], None)?)
            .map_err(|_| EngineError::Network)?;
    if !document["nftables"]
        .as_array()
        .ok_or(EngineError::Network)?
        .iter()
        .any(|v| v["table"]["name"] == TABLE && v["table"]["comment"] == OWNER)
    {
        return Err(EngineError::Network);
    }
    Ok(true)
}
fn rules(policy: &Policy) -> String {
    let mut rules = format!("add table inet {TABLE} {{ comment \"{OWNER}\"; }}\nadd chain inet {TABLE} output {{ type filter hook output priority -150; policy drop; }}\nadd chain inet {TABLE} forward {{ type filter hook forward priority -150; policy drop; }}\n");
    // No established/related blanket exemption: pre-existing direct TCP, UDP,
    // DNS and forwarded flows must also be denied when the guard is armed.
    for expression in [
        "oifname \"lo\" accept",
        "oifname \"moth0\" accept",
        "meta skuid 0 udp sport 68 udp dport 67 ip daddr 255.255.255.255 accept",
        "meta skuid 0 udp sport 546 udp dport 547 ip6 daddr ff02::1:2 accept",
        "ip6 daddr { fe80::/10, ff02::/16 } icmpv6 type { nd-router-solicit, nd-neighbor-solicit, nd-neighbor-advert } accept",
    ] {rules.push_str(&format!("add rule inet {TABLE} output {expression}\n"));}
    rules.push_str(&format!(
        "add rule inet {TABLE} forward oifname \"moth0\" accept\n"
    ));
    for endpoint in &policy.endpoints {
        let family = if endpoint.address.is_ipv4() {
            "ip"
        } else {
            "ip6"
        };
        // Only the privileged engine can set the existing protected routing
        // mark. Restrict this escape further to its pinned upstream endpoint.
        rules.push_str(&format!("add rule inet {TABLE} output meta skuid 0 meta mark 0x4d4f5448 {family} daddr {} meta l4proto {{ tcp, udp }} th dport {} accept\n",endpoint.address,endpoint.port));
    }
    rules
}
pub(super) fn apply(policy: &Policy) -> Result<(), EngineError> {
    let mut batch = String::new();
    if owned()? {
        batch.push_str(&format!("delete table inet {TABLE}\n"));
    }
    batch.push_str(&rules(policy));
    // One nft netlink transaction: replacement cannot create an unfiltered gap.
    nft(&["-f", "-"], Some(&batch))?;
    if !active()? {
        return Err(EngineError::Network);
    }
    Ok(())
}
pub(super) fn remove() -> Result<(), EngineError> {
    if owned()? {
        nft(&["delete", "table", "inet", TABLE], None)?;
    }
    Ok(())
}
pub(super) fn active() -> Result<bool, EngineError> {
    if !owned()? {
        return Ok(false);
    }
    let document: serde_json::Value =
        serde_json::from_slice(&nft(&["-j", "list", "table", "inet", TABLE], None)?)
            .map_err(|_| EngineError::Network)?;
    let rows = document["nftables"]
        .as_array()
        .ok_or(EngineError::Network)?;
    Ok(["output", "forward"].iter().all(|name| {
        rows.iter().any(|v| {
            v["chain"]["name"] == *name
                && v["chain"]["hook"] == *name
                && v["chain"]["policy"] == "drop"
        })
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firewall_batch_contains_no_global_flush_or_established_escape() {
        let policy = Policy::new(
            "1000".into(),
            vec![super::super::Endpoint {
                engine: crate::Engine::Xray,
                host: "vpn.example".into(),
                address: "192.0.2.1".parse().unwrap(),
                port: 443,
            }],
        )
        .unwrap();
        let text = rules(&policy);
        assert!(
            !text.contains("flush") && !text.contains("ct state") && !text.contains("53 accept")
        );
        assert!(
            text.contains("policy drop")
                && text.contains("meta skuid 0 meta mark 0x4d4f5448 ip daddr 192.0.2.1")
        );
    }
}
