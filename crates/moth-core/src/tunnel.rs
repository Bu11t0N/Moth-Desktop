//! Private configurations for the service-owned TUN and authenticated local bridge.
use serde_json::{json, Value};
use uuid::Uuid;

/// Local authenticated bridge used between sing-box TUN and Xray.
/// Never log the UUID or serialize it to a UI snapshot.
pub struct LocalBridge {
    pub port: u16,
    pub credential: Uuid,
}

pub fn sing_box_tun_config(bridge: &LocalBridge) -> Value {
    json!({
        "log":{"disabled":true},
        "dns":{"servers":[{"type":"https","tag":"dns-remote","server":"1.1.1.1","detour":"proxy"}],"final":"dns-remote"},
        "inbounds":[{"type":"tun","tag":"tun-in","interface_name":"Moth","address":["172.19.0.1/30","fdfe:dcba:9876::1/126"],"mtu":1500,"auto_route":true,"strict_route":true,"stack":"gvisor"}],
        "outbounds":[{"type":"vless","tag":"proxy","server":"127.0.0.1","inet4_bind_address":"127.0.0.1","server_port":bridge.port,"uuid":bridge.credential.to_string(),"packet_encoding":"xudp"}],
        "route":{"auto_detect_interface":true,"final":"proxy","rules":[{"inbound":"tun-in","action":"sniff"},{"protocol":"dns","action":"hijack-dns"}]}
    })
}

pub fn sing_box_tun_policy(
    bridge: &LocalBridge,
    settings: &crate::ConnectionSettings,
) -> Result<Value, &'static str> {
    use crate::{RuleKind, RuleTarget, SplitMode};
    settings.validate()?;
    let mut config = sing_box_tun_config(bridge);
    config["inbounds"][0]["mtu"] = json!(settings.mtu);
    let mut dns = crate::dns_server(&settings.dns)?;
    dns["tag"] = json!("dns-remote");
    dns["detour"] = json!("proxy");
    let mut bootstrap = crate::dns_server(&settings.bootstrap_dns)?;
    bootstrap["tag"] = json!("bootstrap");
    // Upstream engines are already numerically pinned before TUN activation.
    // This resolver bootstraps a hostname-based VPN DNS server, not the VPN
    // endpoint; it can and must travel through the existing private bridge.
    bootstrap["detour"] = json!("proxy");
    config["dns"] = json!({"servers":[dns,bootstrap],"final":"dns-remote","strategy":if settings.ipv6{"prefer_ipv4"}else{"ipv4_only"}});
    config["outbounds"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"direct","tag":"direct"}));
    config["route"]["default_domain_resolver"] = json!("bootstrap");
    let mut rules = vec![
        json!({"inbound":"subscription-in","action":"route","outbound":"proxy"}),
        json!({"inbound":"tun-in","action":"sniff"}),
        json!({"protocol":"dns","action":"hijack-dns"}),
    ];
    // Retain IPv6 capture routes and reject IPv6 rather than silently sending it
    // through a physical adapter when the user disables IPv6 through the VPN.
    if !settings.ipv6 {
        rules.push(json!({"ip_version":6,"action":"reject"}));
    }
    if !settings.split_apps.is_empty() {
        rules.push(json!({"process_path":settings.split_apps,"invert":settings.split_mode==SplitMode::Only,"action":"route","outbound":"direct"}));
    }
    for rule in settings.rules.iter().filter(|r| r.enabled) {
        let key = match rule.kind {
            RuleKind::Domain => "domain",
            RuleKind::Suffix => "domain_suffix",
            RuleKind::Keyword => "domain_keyword",
            RuleKind::Cidr => "ip_cidr",
        };
        let mut value = json!({key:[rule.value]});
        if rule.target == RuleTarget::Block {
            value["action"] = json!("reject");
        } else {
            value["action"] = json!("route");
            value["outbound"] = json!(if rule.target == RuleTarget::Direct {
                "direct"
            } else {
                "proxy"
            });
        }
        rules.push(value);
    }
    config["route"]["rules"] = json!(rules);
    Ok(config)
}

pub fn xray_bridge_config(profile: &crate::Profile, bridge: &LocalBridge) -> Value {
    let mut config = crate::xray_check_config(profile);
    config["inbounds"] = json!([{"tag":"moth-bridge","listen":"127.0.0.1","port":bridge.port,"protocol":"vless","settings":{"decryption":"none","clients":[{"id":bridge.credential.to_string()}]}}]);
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostname_dns_bootstrap_has_no_direct_network_dependency() {
        let bridge = LocalBridge {
            port: 15491,
            credential: Uuid::new_v4(),
        };
        let policy = crate::ConnectionSettings {
            dns: "https://dns.example.com/dns-query".into(),
            ..Default::default()
        };
        let config = sing_box_tun_policy(&bridge, &policy).unwrap();
        assert_eq!(config["route"]["default_domain_resolver"], "bootstrap");
        assert!(config["dns"]["servers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["detour"] == "proxy"));
        assert_eq!(config["outbounds"][0]["server"], "127.0.0.1");
        assert_eq!(config["outbounds"][0]["server_port"], bridge.port);
    }
}
