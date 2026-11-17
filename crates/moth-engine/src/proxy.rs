//! Authenticated mixed listener. This path creates no TUN, route or DNS setting.
use super::*;
use moth_core::{ConnectionMode, PublicProxyOptions};
use std::{
    net::{SocketAddr, TcpListener},
    sync::Arc,
};

#[derive(Clone, Default)]
pub struct SessionPlan {
    pub mode: ConnectionMode,
    pub public: Option<PublicListener>,
}
#[derive(Clone)]
pub struct PublicListener {
    pub options: PublicProxyOptions,
    pub access: moth_ipc::ProxyAccess,
}
impl SessionPlan {
    pub fn new(
        mode: ConnectionMode,
        options: Option<PublicProxyOptions>,
    ) -> Result<Self, EngineError> {
        if mode == ConnectionMode::Proxy && options.is_none() {
            return Err(EngineError::InvalidConfig);
        }
        let public = options
            .map(|options| {
                options.validate().map_err(|_| EngineError::InvalidConfig)?;
                Ok(PublicListener {
                    access: moth_ipc::ProxyAccess {
                        address: options.listen,
                        port: options.port,
                        username: "moth".into(),
                        password: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
                    },
                    options,
                })
            })
            .transpose()?;
        Ok(Self { mode, public })
    }
    pub fn lan(&self) -> bool {
        self.public
            .as_ref()
            .is_some_and(|p| !p.options.listen.is_loopback())
    }
}
impl PublicListener {
    pub(super) fn add(
        &self,
        config: &mut serde_json::Value,
    ) -> Result<Vec<TcpListener>, EngineError> {
        self.options
            .validate()
            .map_err(|_| EngineError::InvalidConfig)?;
        let reservation =
            TcpListener::bind(SocketAddr::new(self.options.listen, self.options.port))
                .map_err(|_| EngineError::PortUnavailable)?;
        config["inbounds"].as_array_mut().ok_or(EngineError::InvalidConfig)?.push(serde_json::json!({
            "type":"mixed","tag":"public-in","listen":self.options.listen.to_string(),"listen_port":self.options.port,
            "users":[{"username":self.access.username,"password":self.access.password}]
        }));
        if !self.options.listen.is_loopback() {
            let mut clients = self.options.allowed_clients.clone();
            // Native readiness probes still authenticate and use the actual
            // bound address. No wildcard or global established-flow exemption.
            clients.push(format!(
                "{}/{}",
                self.options.listen,
                if self.options.listen.is_ipv4() {
                    32
                } else {
                    128
                }
            ));
            config["route"]["rules"].as_array_mut().ok_or(EngineError::InvalidConfig)?.insert(0,serde_json::json!({
                "type":"logical","mode":"and","rules":[{"inbound":"public-in"},{"source_ip_cidr":clients,"invert":true}],"action":"reject"
            }));
        }
        let mut reservations = vec![reservation];
        if self.options.listen.to_string() != "127.0.0.1" {
            reservations.push(
                TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, self.options.port))
                    .map_err(|_| EngineError::PortUnavailable)?,
            );
            config["inbounds"].as_array_mut().ok_or(EngineError::InvalidConfig)?.push(serde_json::json!({
                "type":"mixed","tag":"public-local","listen":"127.0.0.1","listen_port":self.options.port,
                "users":[{"username":self.access.username,"password":self.access.password}]
            }));
        }
        Ok(reservations)
    }
}

pub enum Session {
    Tun(Tunnel),
    Proxy(ProxySession),
}
impl Session {
    pub fn telemetry(&self) -> Option<Arc<telemetry::Api>> {
        match self {
            Self::Tun(s) => s.telemetry(),
            Self::Proxy(s) => s.telemetry.clone(),
        }
    }
    pub fn endpoint(&self) -> SocketAddr {
        match self {
            Self::Tun(s) => s.endpoint(),
            Self::Proxy(s) => s.endpoint,
        }
    }
    pub fn subscription_proxy(&self) -> moth_ipc::DownloadProxy {
        match self {
            Self::Tun(s) => s.subscription_proxy(),
            Self::Proxy(s) => s.private.clone(),
        }
    }
    pub fn alive(&mut self) -> bool {
        match self {
            Self::Tun(s) => s.alive(),
            Self::Proxy(s) => s.alive(),
        }
    }
    pub async fn stop(self) {
        match self {
            Self::Tun(s) => s.stop().await,
            Self::Proxy(s) => s.stop().await,
        }
    }
}
pub struct ProxySession {
    telemetry: Option<Arc<telemetry::Api>>,
    endpoint: SocketAddr,
    private: moth_ipc::DownloadProxy,
    listener: EngineProcess,
    upstream: EngineProcess,
    _maintenance: Option<moth_ipc::maintenance::TunnelLease>,
}
impl ProxySession {
    #[cfg(test)]
    pub(super) fn telemetry(&self) -> Option<Arc<telemetry::Api>> {
        self.telemetry.clone()
    }
    pub(super) fn alive(&mut self) -> bool {
        matches!(self.listener.child.try_wait(), Ok(None))
            && matches!(self.upstream.child.try_wait(), Ok(None))
    }
    pub(super) async fn stop(mut self) {
        let _ = self.listener.stop().await;
        let _ = self.upstream.stop().await;
    }
}
impl EngineSet {
    pub async fn connect_session(
        &self,
        profile: &Profile,
        policy: &moth_core::ConnectionSettings,
        plan: &SessionPlan,
    ) -> Result<Session, EngineError> {
        if plan.mode == ConnectionMode::Tun {
            self.connect_shared(profile, policy, plan.public.as_ref())
                .await
                .map(Session::Tun)
        } else {
            self.connect_proxy(profile, policy, plan)
                .await
                .map(Session::Proxy)
        }
    }
    async fn connect_proxy(
        &self,
        profile: &Profile,
        policy: &moth_core::ConnectionSettings,
        plan: &SessionPlan,
    ) -> Result<ProxySession, EngineError> {
        let maintenance =
            moth_ipc::maintenance::TunnelLease::acquire().map_err(|_| EngineError::Maintenance)?;
        let endpoint = guard::resolve(profile).await?;
        self.connect_proxy_inner(
            profile,
            policy,
            plan,
            Some(maintenance),
            endpoint,
            &policy.health_url,
        )
        .await
    }
    pub(super) async fn connect_proxy_inner(
        &self,
        profile: &Profile,
        policy: &moth_core::ConnectionSettings,
        plan: &SessionPlan,
        maintenance: Option<moth_ipc::maintenance::TunnelLease>,
        endpoint: SocketAddr,
        check_url: &str,
    ) -> Result<ProxySession, EngineError> {
        let reservation = TcpListener::bind("127.0.0.1:0").map_err(|_| EngineError::Process)?;
        let bridge = LocalBridge {
            port: reservation
                .local_addr()
                .map_err(|_| EngineError::Process)?
                .port(),
            credential: Uuid::new_v4(),
        };
        let private_reservation =
            TcpListener::bind("127.0.0.1:0").map_err(|_| EngineError::Process)?;
        let private = moth_ipc::DownloadProxy {
            port: private_reservation
                .local_addr()
                .map_err(|_| EngineError::Process)?
                .port(),
            username: Uuid::new_v4().to_string(),
            password: Uuid::new_v4().to_string(),
        };
        let mut upstream = moth_core::xray_bridge_config(profile, &bridge);
        let key = if matches!(
            profile.protocol,
            moth_core::Protocol::Vless | moth_core::Protocol::Vmess
        ) {
            "vnext"
        } else {
            "servers"
        };
        upstream["outbounds"][0]["settings"][key][0]["address"] =
            serde_json::json!(endpoint.ip().to_string());
        #[cfg(windows)]
        {
            let interface = crate::tunnel::upstream_interface(endpoint.ip())?;
            upstream["outbounds"][0]["streamSettings"]["sockopt"] =
                serde_json::json!({"interface":interface});
        }
        #[cfg(target_os = "linux")]
        if maintenance.is_some() {
            upstream["outbounds"][0]["streamSettings"]["sockopt"] =
                serde_json::json!({"mark":0x4d4f5448_u32});
        }
        let mut config = moth_core::sing_box_tun_policy(&bridge, policy)
            .map_err(|_| EngineError::InvalidConfig)?;
        // Proxy routing is force-upstream. TUN-only per-app/DNS/capture policy
        // never silently turns this listener into a direct network escape.
        config["inbounds"] = serde_json::json!([{"type":"mixed","tag":"subscription-in","listen":"127.0.0.1","listen_port":private.port,"users":[{"username":private.username,"password":private.password}]}]);
        config["route"]["rules"] = serde_json::json!([]);
        config["outbounds"]
            .as_array_mut()
            .ok_or(EngineError::InvalidConfig)?
            .retain(|v| v["tag"] == "proxy");
        config["route"]
            .as_object_mut()
            .ok_or(EngineError::InvalidConfig)?
            .remove("default_domain_resolver");
        config
            .as_object_mut()
            .ok_or(EngineError::InvalidConfig)?
            .remove("dns");
        let public = plan.public.as_ref().ok_or(EngineError::InvalidConfig)?;
        let public_reservation = public.add(&mut config)?;
        self.validate(Engine::Xray, &upstream).await?;
        let (telemetry, telemetry_reservation) = self.attach_telemetry(&mut config).await?;
        drop(reservation);
        let upstream = self.spawn_config(Engine::Xray, &upstream, false).await?;
        drop(private_reservation);
        drop(public_reservation);
        drop(telemetry_reservation);
        let listener = self.spawn_config(Engine::SingBox, &config, false).await?;
        let mut session = ProxySession {
            telemetry,
            endpoint,
            private,
            listener,
            upstream,
            _maintenance: maintenance,
        };
        let public_proxy = reqwest::Proxy::all(format!(
            "http://{}",
            SocketAddr::new(public.access.address, public.access.port)
        ))
        .map_err(|_| EngineError::Probe)?
        .basic_auth(&public.access.username, &public.access.password);
        let client = reqwest::Client::builder()
            .no_proxy()
            .proxy(public_proxy)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(4))
            .build()
            .map_err(|_| EngineError::Probe)?;
        let start = Instant::now();
        loop {
            if !session.alive() {
                return Err(EngineError::Process);
            }
            if health_through_proxy(&session.private, check_url)
                .await
                .is_ok()
                && client
                    .get(check_url)
                    .send()
                    .await
                    .is_ok_and(|r| r.status().is_success())
                && session.alive()
            {
                return Ok(session);
            }
            if start.elapsed() > Duration::from_secs(12) {
                return Err(EngineError::Probe);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
