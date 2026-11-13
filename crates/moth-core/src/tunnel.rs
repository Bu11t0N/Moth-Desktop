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

pub fn xray_bridge_config(profile: &crate::Profile, bridge: &LocalBridge) -> Value {
    let mut config = crate::xray_check_config(profile);
    config["inbounds"] = json!([{"tag":"moth-bridge","listen":"127.0.0.1","port":bridge.port,"protocol":"vless","settings":{"decryption":"none","clients":[{"id":bridge.credential.to_string()}]}}]);
    config
}
