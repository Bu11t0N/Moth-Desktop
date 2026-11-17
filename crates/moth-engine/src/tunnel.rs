//! System routing is only started by the privileged service, never by probes.
use super::*;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use windows_sys::Win32::{
    NetworkManagement::{IpHelper::*, Ndis::NET_LUID_LH},
    Networking::WinSock::*,
};

/// Owns both engines and their Windows jobs. Dropping it tears down the private
/// Wintun adapter/routes and the dynamic WFP session, including on cancellation.
pub struct Tunnel {
    telemetry: Option<std::sync::Arc<crate::telemetry::Api>>,
    endpoint: SocketAddr,
    subscription_proxy: moth_ipc::DownloadProxy,
    tun: EngineProcess,
    upstream: EngineProcess,
    _maintenance: Option<moth_ipc::maintenance::TunnelLease>,
}
impl Tunnel {
    pub fn telemetry(&self) -> Option<std::sync::Arc<crate::telemetry::Api>> {
        self.telemetry.clone()
    }
    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }
    pub fn subscription_proxy(&self) -> moth_ipc::DownloadProxy {
        self.subscription_proxy.clone()
    }
    pub fn alive(&mut self) -> bool {
        matches!(self.tun.child.try_wait(), Ok(None))
            && matches!(self.upstream.child.try_wait(), Ok(None))
    }
    pub async fn stop(mut self) {
        // Wintun owns its routes/DNS, not the physical adapter. No baseline
        // routes or physical DNS are rewritten/restored by Moth.
        let _ = self.tun.stop().await;
        let _ = self.upstream.stop().await;
    }
}

/// Read-only route discovery before TUN activation. The upstream socket must
/// stay bound to this interface: sing-box auto_detect does NOT bind Xray.
pub(super) fn upstream_interface(ip: IpAddr) -> Result<String, EngineError> {
    unsafe {
        let mut destination: SOCKADDR_INET = std::mem::zeroed();
        match ip {
            IpAddr::V4(ip) => {
                destination.Ipv4.sin_family = AF_INET;
                destination.Ipv4.sin_addr.S_un.S_addr = u32::from_ne_bytes(ip.octets());
            }
            IpAddr::V6(ip) => {
                destination.Ipv6.sin6_family = AF_INET6;
                destination.Ipv6.sin6_addr.u.Byte = ip.octets();
            }
        }
        let mut index = 0;
        if GetBestInterfaceEx((&destination as *const SOCKADDR_INET).cast(), &mut index) != 0 {
            return Err(EngineError::Network);
        }
        let mut luid: NET_LUID_LH = std::mem::zeroed();
        let mut alias = [0u16; 257];
        if ConvertInterfaceIndexToLuid(index, &mut luid) != 0
            || ConvertInterfaceLuidToAlias(&luid, alias.as_mut_ptr(), alias.len()) != 0
        {
            return Err(EngineError::Network);
        }
        let name = String::from_utf16_lossy(
            &alias[..alias.iter().position(|&c| c == 0).unwrap_or(alias.len())],
        );
        if name.is_empty() || name.eq_ignore_ascii_case("Moth") {
            return Err(EngineError::Network);
        }
        Ok(name)
    }
}

impl EngineSet {
    /// A successful return requires actual HTTPS sourced from the TUN address,
    /// not merely a spawned process. Startup failure/cancellation drops both jobs.
    pub async fn connect(&self, profile: &Profile) -> Result<Tunnel, EngineError> {
        self.connect_with_policy(profile, &moth_core::ConnectionSettings::default())
            .await
    }
    pub async fn connect_with_policy(
        &self,
        profile: &Profile,
        policy: &moth_core::ConnectionSettings,
    ) -> Result<Tunnel, EngineError> {
        self.connect_shared(profile, policy, None).await
    }
    pub(super) async fn connect_shared(
        &self,
        profile: &Profile,
        policy: &moth_core::ConnectionSettings,
        public: Option<&crate::proxy::PublicListener>,
    ) -> Result<Tunnel, EngineError> {
        policy.validate().map_err(|_| EngineError::InvalidConfig)?;
        let lease =
            moth_ipc::maintenance::TunnelLease::acquire().map_err(|_| EngineError::Maintenance)?;
        let mut tunnel = self
            .connect_checked_policy(profile, &policy.health_url, policy, public)
            .await?;
        tunnel._maintenance = Some(lease);
        Ok(tunnel)
    }
    #[cfg(test)]
    async fn connect_checked(
        &self,
        profile: &Profile,
        check_url: &str,
    ) -> Result<Tunnel, EngineError> {
        self.connect_checked_policy(
            profile,
            check_url,
            &moth_core::ConnectionSettings::default(),
            None,
        )
        .await
    }
    async fn connect_checked_policy(
        &self,
        profile: &Profile,
        check_url: &str,
        policy: &moth_core::ConnectionSettings,
        public: Option<&crate::proxy::PublicListener>,
    ) -> Result<Tunnel, EngineError> {
        // Resolve the server before changing DNS/routes, then pin this connection
        // to one numeric address. TLS SNI/Reality/WS host remain unchanged.
        let address = guard::resolve(profile).await?;
        let interface = upstream_interface(address.ip())?;
        let reservation =
            std::net::TcpListener::bind("127.0.0.1:0").map_err(|_| EngineError::Process)?;
        let bridge = LocalBridge {
            port: reservation
                .local_addr()
                .map_err(|_| EngineError::Process)?
                .port(),
            credential: Uuid::new_v4(),
        };
        let proxy_reservation =
            std::net::TcpListener::bind("127.0.0.1:0").map_err(|_| EngineError::Process)?;
        let subscription_proxy = moth_ipc::DownloadProxy {
            port: proxy_reservation
                .local_addr()
                .map_err(|_| EngineError::Process)?
                .port(),
            username: Uuid::new_v4().to_string(),
            password: Uuid::new_v4().to_string(),
        };
        let mut config = moth_core::xray_bridge_config(profile, &bridge);
        let outbound = &mut config["outbounds"][0];
        let key = if matches!(
            profile.protocol,
            moth_core::Protocol::Vless | moth_core::Protocol::Vmess
        ) {
            "vnext"
        } else {
            "servers"
        };
        outbound["settings"][key][0]["address"] = serde_json::json!(address.ip().to_string());
        outbound["streamSettings"]["sockopt"] = serde_json::json!({"interface":interface});
        let mut tun_config = moth_core::sing_box_tun_policy(&bridge, policy)
            .map_err(|_| EngineError::InvalidConfig)?;
        tun_config["outbounds"][1]["bind_interface"] = serde_json::json!(interface);
        protect_verifier(&mut tun_config, policy)?;
        tun_config["inbounds"].as_array_mut().ok_or(EngineError::Process)?.push(serde_json::json!({"type":"mixed","tag":"subscription-in","listen":"127.0.0.1","listen_port":subscription_proxy.port,"users":[{"username":subscription_proxy.username,"password":subscription_proxy.password}]}));
        let public_reservation = public.map(|p| p.add(&mut tun_config)).transpose()?;
        self.validate(Engine::Xray, &config).await?;
        let (telemetry, telemetry_reservation) = self.attach_telemetry(&mut tun_config).await?;
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
        #[cfg(test)]
        if check_url == "http://198.18.0.123:18080/generate_204" {
            fixture_bridge(&bridge, Ipv4Addr::new(198, 18, 0, 123), 18080).await;
            eprintln!("Bound Xray bridge passed before TUN activation");
        }
        drop(public_reservation);
        drop(proxy_reservation);
        drop(telemetry_reservation);
        let tun = self
            .spawn_config(Engine::SingBox, &tun_config, false)
            .await?;
        let mut session = Tunnel {
            telemetry,
            endpoint: address,
            subscription_proxy,
            tun,
            upstream,
            _maintenance: None,
        };
        let client = reqwest::Client::builder()
            .no_proxy()
            .local_address(IpAddr::V4(Ipv4Addr::new(172, 19, 0, 1)))
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(4))
            .build()
            .map_err(|_| EngineError::Probe)?;
        let start = Instant::now();
        // Wintun is created by the child after spawn. Update the persistent
        // scoped interface permit before performing the real TUN verifier.
        if guard::load()?.is_some() {
            loop {
                let mut luid = windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH::default();
                let alias: Vec<u16> = "Moth".encode_utf16().chain(Some(0)).collect();
                if unsafe { ConvertInterfaceAliasToLuid(alias.as_ptr(), &mut luid) } == 0 {
                    break;
                }
                if start.elapsed() > Duration::from_secs(3) {
                    return Err(EngineError::Network);
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            guard::refresh_interface()?;
        }
        loop {
            if !session.alive() {
                return Err(EngineError::Process);
            }
            match client.get(check_url).send().await {
                Ok(response)
                    if response.status() == reqwest::StatusCode::NO_CONTENT && session.alive() =>
                {
                    return Ok(session)
                }
                #[cfg(test)]
                Err(error) => eprintln!(
                    "TUN verification: timeout={}, connect={}",
                    error.is_timeout(),
                    error.is_connect()
                ),
                _ => {}
            }
            if start.elapsed() > Duration::from_secs(12) {
                return Err(EngineError::Probe);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

#[cfg(test)]
pub(super) async fn fixture_bridge(bridge: &LocalBridge, ip: Ipv4Addr, port: u16) {
    use tokio::io::AsyncReadExt;
    timeout(Duration::from_secs(3), async {
        let mut socket = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, bridge.port))
            .await
            .unwrap();
        let mut header = vec![0];
        header.extend_from_slice(bridge.credential.as_bytes());
        header.extend_from_slice(&[0, 1]);
        header.extend_from_slice(&port.to_be_bytes());
        header.push(1);
        header.extend_from_slice(&ip.octets());
        socket.write_all(&header).await.unwrap();
        socket
            .write_all(
                b"GET /generate_204 HTTP/1.1\r\nHost: fixture.invalid\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let mut prefix = [0; 2];
        socket.read_exact(&mut prefix).await.unwrap();
        assert_eq!(prefix, [0, 0]);
        let mut status = [0; 12];
        socket.read_exact(&mut status).await.unwrap();
        assert_eq!(&status, b"HTTP/1.1 204");
    })
    .await
    .expect("Bound bridge must reach the fixture before TUN starts");
}

#[cfg(test)]
mod acceptance {
    use super::*;
    /// Explicitly invoked ONLY by the disposable Windows CI job. Normal local
    /// --include-ignored remains read-only and does not start a system tunnel.
    #[tokio::test]
    #[ignore = "system TUN: disposable Windows CI only"]
    async fn windows_system_tun_roundtrip() {
        if std::env::var("MOTH_TUN_ACCEPTANCE").as_deref() != Ok("1") {
            return;
        }
        assert!(
            (std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
                && std::env::var("RUNNER_OS").as_deref() == Ok("Windows"))
                || std::env::var("MOTH_DISPOSABLE_VM").as_deref() == Ok("1")
        );
        let engines = EngineSet::new(development_directory().unwrap());
        let fixture = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_port = fixture.local_addr().unwrap().port();
        let udp = tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, target_port))
            .await
            .unwrap();
        let udp_endpoint = tokio::spawn(async move {
            // DNS-over-HTTPS/QUIC probes can also reach the redirected fixture.
            // Receive a complete UDP datagram; only the exact fixture payload
            // below receives an echo. A larger unrelated packet must not kill
            // this task with WSAEMSGSIZE before the UDP assertions run.
            let mut buffer = [0; 65536];
            loop {
                let (n, peer) = udp.recv_from(&mut buffer).await.unwrap();
                if &buffer[..n] == b"moth-tun-fixture" {
                    let _ = udp.send_to(b"moth-udp-ok", peer).await;
                }
            }
        });
        let endpoint = tokio::spawn(async move {
            loop {
                let (mut socket, _) = fixture.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut bytes = [0u8; 4096];
                    let _ = socket.read(&mut bytes).await;
                    let _=socket.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                });
            }
        });
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let id = Uuid::new_v4();
        let config = serde_json::json!({"log":{"loglevel":"none"},"inbounds":[{"listen":"127.0.0.1","port":port,"protocol":"vless","settings":{"decryption":"none","clients":[{"id":id.to_string()}]}}],"outbounds":[{"protocol":"freedom","settings":{"redirect":format!("127.0.0.1:{target_port}"),"finalRules":[{"action":"allow"}]}}]});
        let mut server = engines
            .spawn_config(Engine::Xray, &config, false)
            .await
            .unwrap();
        let profile = moth_core::import_links(&format!(
            "vless://{id}@127.0.0.1:{port}?security=none&type=tcp"
        ))
        .unwrap()
        .remove(0);
        // This address has no direct endpoint. Response can only be produced by
        // TUN -> authenticated bridge -> Xray server -> loopback fixture.
        let url = "http://198.18.0.123:18080/generate_204";
        let curl = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/curl.exe");
        for split in [
            None,
            Some(moth_core::SplitMode::Bypass),
            Some(moth_core::SplitMode::Only),
        ] {
            let mut tunnel = if let Some(mode) = split {
                let policy = moth_core::ConnectionSettings {
                    split_mode: mode,
                    split_apps: vec![curl.to_string_lossy().into_owned()],
                    ..Default::default()
                };
                engines
                    .connect_checked_policy(&profile, url, &policy, None)
                    .await
                    .unwrap()
            } else {
                engines.connect_checked(&profile, url).await.unwrap()
            };
            assert!(tunnel.alive());
            let selected = Command::new(&curl)
                .args(["--noproxy", "*", "--silent", "--max-time", "3", url])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .status()
                .await
                .unwrap();
            assert!(selected.success() != (split == Some(moth_core::SplitMode::Bypass)), "selected process did not follow its per-app policy (bypass={}, only={}, exit={selected})", split == Some(moth_core::SplitMode::Bypass), split == Some(moth_core::SplitMode::Only));
            if split == Some(moth_core::SplitMode::Only) {
                let other=Command::new("pwsh").args(["-NoProfile","-Command","try { $null=Invoke-WebRequest -NoProxy -TimeoutSec 3 -Uri 'http://198.18.0.123:18080/generate_204'; exit 0 } catch { exit 1 }"]).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).status().await.unwrap();
                assert!(!other.success());
            }
            let credentials = tunnel.subscription_proxy();
            let proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{}", credentials.port))
                .unwrap()
                .basic_auth(&credentials.username, &credentials.password);
            let subscription_client = reqwest::Client::builder()
                .no_proxy()
                .proxy(proxy)
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            assert!(subscription_client.get(url).send().await.unwrap().status() == 204);
            let denied = reqwest::Client::builder()
                .no_proxy()
                .proxy(
                    reqwest::Proxy::all(format!("http://127.0.0.1:{}", credentials.port)).unwrap(),
                )
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap()
                .get(url)
                .send()
                .await;
            assert!(denied.is_err() || denied.unwrap().status() != 204);
            let response = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap()
                .get(url)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 204);
            let ipv6 = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap()
                .get("http://[2001:db8::123]:18080/generate_204")
                .send()
                .await
                .unwrap();
            assert_eq!(ipv6.status(), 204);
            for (bind, target) in [
                ("0.0.0.0:0", "198.18.0.123:18080"),
                ("[::]:0", "[2001:db8::123]:18080"),
            ] {
                let socket = tokio::net::UdpSocket::bind(bind).await.unwrap();
                socket.send_to(b"moth-tun-fixture", target).await.unwrap();
                let mut bytes = [0; 64];
                let (n, _) = timeout(Duration::from_secs(5), socket.recv_from(&mut bytes))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(&bytes[..n], b"moth-udp-ok");
            }
            tunnel.stop().await;
            assert!(subscription_client.get(url).send().await.is_err());
            tokio::time::sleep(Duration::from_secs(2)).await;
            assert!(reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap()
                .get(url)
                .send()
                .await
                .is_err());
        }
        server.stop().await.unwrap();
        endpoint.abort();
        udp_endpoint.abort();
    }
}
