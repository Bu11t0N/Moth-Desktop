use super::*;
use moth_core::{ClashPolicy, ConnectionSettings};
use std::{net::SocketAddr, sync::Arc};

#[derive(Clone, Copy)]
pub enum ConnectionSource<'a> {
    Profile(&'a Profile),
    Clash(&'a ClashPolicy),
}
impl ConnectionSource<'_> {
    pub(super) async fn address(self) -> Result<SocketAddr, EngineError> {
        match self {
            Self::Profile(p) => crate::guard::resolve(p).await,
            Self::Clash(_) if crate::guard::load()?.is_some() => Err(EngineError::InvalidConfig),
            // A numeric route lookup identifies the default egress interface.
            // Mihomo resolves its actual upstreams under the imported DNS policy.
            Self::Clash(_) => Ok(SocketAddr::from(([1, 1, 1, 1], 443))),
        }
    }
    pub(super) fn prepare(
        self,
        bridge: &LocalBridge,
        endpoint: SocketAddr,
        interface: Option<&str>,
        mark: bool,
        settings: &ConnectionSettings,
    ) -> Result<Prepared, EngineError> {
        match self {
            Self::Profile(p) => {
                let (engine, mut config) = config(p, Some(bridge))?;
                pin(&mut config, engine, p, endpoint.ip(), interface, mark);
                Ok(Prepared::Legacy(engine, config))
            }
            Self::Clash(profile) => {
                policy::Prepared::new(profile, settings, bridge, interface, mark)
                    .map(|p| Prepared::Policy(Box::new(p)))
            }
        }
    }
}

pub(super) enum Prepared {
    Legacy(Engine, serde_json::Value),
    Policy(Box<policy::Prepared>),
}
impl Prepared {
    pub(super) fn configure_capture(
        &self,
        config: &mut serde_json::Value,
        dns: bool,
    ) -> Result<(), EngineError> {
        if let Self::Policy(policy) = self {
            policy.configure_capture(config, dns)?;
        }
        Ok(())
    }
    pub(super) async fn start(
        self,
        set: &EngineSet,
        protected: bool,
    ) -> Result<(EngineProcess, Option<Arc<policy::Control>>), EngineError> {
        match self {
            Self::Legacy(engine, config) => {
                set.validate(engine, &config).await?;
                Ok((set.spawn_config(engine, &config, false).await?, None))
            }
            Self::Policy(policy) => policy
                .start(set, protected)
                .await
                .map(|(process, control)| (process, Some(control))),
        }
    }
}

pub fn profile_engine(profile: &Profile) -> Engine {
    if profile.protocol.uses_sing_box() {
        Engine::SingBox
    } else {
        Engine::Xray
    }
}
pub(super) fn config(
    profile: &Profile,
    bridge: Option<&LocalBridge>,
) -> Result<(Engine, serde_json::Value), EngineError> {
    let engine = profile_engine(profile);
    let mut config = if engine == Engine::SingBox {
        moth_core::sing_box_native_config(profile).map_err(|_| EngineError::InvalidConfig)?
    } else if let Some(bridge) = bridge {
        moth_core::xray_bridge_config(profile, bridge)
    } else {
        moth_core::xray_check_config(profile)
    };
    if engine == Engine::SingBox {
        if let Some(bridge) = bridge {
            config["inbounds"] = serde_json::json!([{"type":"vless","tag":"moth-bridge","listen":"127.0.0.1","listen_port":bridge.port,"users":[{"uuid":bridge.credential}]}]);
        }
    }
    Ok((engine, config))
}
pub(super) fn pin(
    config: &mut serde_json::Value,
    engine: Engine,
    profile: &Profile,
    address: std::net::IpAddr,
    interface: Option<&str>,
    mark: bool,
) {
    let outbound = &mut config["outbounds"][0];
    if engine == Engine::SingBox {
        outbound["server"] = serde_json::json!(address.to_string());
        if let Some(interface) = interface {
            outbound["bind_interface"] = serde_json::json!(interface);
        }
        if mark {
            outbound["routing_mark"] = serde_json::json!(0x4d4f5448_u32);
        }
    } else {
        let key = if matches!(
            profile.protocol,
            moth_core::Protocol::Vless | moth_core::Protocol::Vmess
        ) {
            "vnext"
        } else {
            "servers"
        };
        outbound["settings"][key][0]["address"] = serde_json::json!(address.to_string());
        if let Some(interface) = interface {
            outbound["streamSettings"]["sockopt"]["interface"] = serde_json::json!(interface);
        }
        if mark {
            outbound["streamSettings"]["sockopt"]["mark"] = serde_json::json!(0x4d4f5448_u32);
        }
    }
}
