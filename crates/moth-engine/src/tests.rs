use super::*;
#[cfg(windows)]
#[test]
fn windows_process_paths_use_anchored_literal_case_insensitive_patterns() {
    assert_eq!(
        windows_process_pattern(r"C:\Windows/System32/curl.exe"),
        r"(?i)^C:\\Windows\\System32\\curl\.exe$"
    );
    assert_eq!(
        windows_process_pattern(r"\\?\C:\Apps\Client (1).exe"),
        r"(?i)^C:\\Apps\\Client \(1\)\.exe$"
    );
}
use tokio::io::AsyncReadExt;

fn engines() -> EngineSet {
    EngineSet::new(development_directory().unwrap())
}
fn fixture_links() -> Vec<String> {
    let base = "vless://c6b3a07a-8bf9-4a18-905b-ea41642633c3@example.com:443";
    let mut links = vec![];
    let additional = r#"{"outbounds":[{"type":"vmess","tag":"VMess","server":"example.com","server_port":443,"uuid":"00000000-0000-4000-8000-000000000001","tls":{"enabled":true}},{"type":"shadowsocks","tag":"SS","server":"example.com","server_port":443,"method":"aes-256-gcm","password":"synthetic"},{"type":"socks","tag":"SOCKS","server":"example.com","server_port":1080,"username":"synthetic","password":"synthetic"},{"type":"http","tag":"HTTP","server":"example.com","server_port":8080,"username":"synthetic","password":"synthetic"}]}"#;
    for profile in moth_core::import_links(additional).unwrap() {
        links.push(moth_core::share_link(&profile).unwrap());
    }
    for transport in [
        "type=tcp",
        "type=ws&path=%2Fvpn&host=example.com",
        "type=grpc&serviceName=moth",
        "type=xhttp&mode=auto&path=%2Fvpn",
    ] {
        links.push(format!(
            "{base}?security=tls&sni=example.com&fp=chrome&{transport}"
        ));
        links.push(format!(
            "trojan://fixture-secret@example.com:443?security=tls&sni=example.com&{transport}"
        ));
    }
    links.push(format!("{base}?security=reality&sni=example.com&fp=chrome&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sid=abcd&flow=xtls-rprx-vision"));
    links.push(
        "vless://c6b3a07a-8bf9-4a18-905b-ea41642633c3@127.0.0.1:443?security=none&type=tcp".into(),
    );
    links
}

#[tokio::test]
#[ignore = "requires pinned engines: npm run engines:fetch"]
async fn pinned_engines_accept_supported_profile_matrix_and_tun_plan() {
    for (index, link) in fixture_links().into_iter().enumerate() {
        let p = moth_core::import_links(&link).unwrap().remove(0);
        engines()
            .validate_profile(&p)
            .await
            .unwrap_or_else(|error| panic!("matrix row {index}: {error}"));
    }
    let bridge = LocalBridge {
        port: 15491,
        credential: Uuid::new_v4(),
    };
    let policy = moth_core::ConnectionSettings {
        mtu: 1400,
        ipv6: false,
        dns: "9.9.9.9".into(),
        split_apps: vec![if cfg!(windows) {
            "C:\\Windows\\System32\\curl.exe"
        } else {
            "/usr/bin/curl"
        }
        .into()],
        rules: vec![moth_core::RoutingRule {
            id: Uuid::new_v4(),
            name: "Fixture".into(),
            enabled: true,
            kind: moth_core::RuleKind::Suffix,
            value: "example.com".into(),
            target: moth_core::RuleTarget::Block,
        }],
        ..Default::default()
    };
    let mut config = moth_core::sing_box_tun_policy(&bridge, &policy).unwrap();
    protect_verifier(&mut config, &policy).unwrap();
    let (telemetry, reservation) = engines().attach_telemetry(&mut config).await.unwrap();
    assert!(
        telemetry.is_some(),
        "Pinned engine must support the configured telemetry API"
    );
    drop(reservation);
    engines().validate(Engine::SingBox, &config).await.unwrap();
    // A regression in config generation must not be hidden by a zero exit assumption.
    assert!(matches!(
        engines()
            .validate(
                Engine::Xray,
                &serde_json::json!({"outbounds":[{"protocol":"moth-invalid"}]})
            )
            .await,
        Err(EngineError::InvalidConfig)
    ));
    assert!(matches!(
        engines()
            .validate(
                Engine::SingBox,
                &serde_json::json!({"outbounds":[{"type":"moth-invalid"}]})
            )
            .await,
        Err(EngineError::InvalidConfig)
    ));
}

#[tokio::test]
#[ignore = "requires pinned engines: npm run engines:fetch"]
async fn real_vless_roundtrip_and_wrong_credential_failure() {
    // Only loopback sockets. No routes, DNS/firewall settings or Internet access.
    let endpoint = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint_port = endpoint.local_addr().unwrap().port();
    let endpoint_task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = endpoint.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = [0; 1024];
                let _ = socket.read(&mut buf).await;
                let _=socket.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            });
        }
    });
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let id = Uuid::new_v4();
    let server = serde_json::json!({"log":{"loglevel":"none"},"inbounds":[{"listen":"127.0.0.1","port":port,"protocol":"vless","settings":{"decryption":"none","clients":[{"id":id.to_string()}]}}],"outbounds":[{"protocol":"freedom","settings":{"finalRules":[{"action":"allow"}]}}]});
    let mut server = engines()
        .spawn_config(Engine::Xray, &server, false)
        .await
        .unwrap();
    let profile = moth_core::import_links(&format!(
        "vless://{id}@127.0.0.1:{port}?security=none&type=tcp"
    ))
    .unwrap()
    .remove(0);
    let url = format!("http://127.0.0.1:{endpoint_port}/generate_204");
    let response = engines().probe_url(&profile, &url).await.unwrap();
    assert_eq!(response.profile_id, profile.id);
    // Exercise the production mixed-listener path with only loopback fixtures.
    // No global maintenance lease, TUN or OS proxy setting is created here.
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let public_port = reserved.local_addr().unwrap().port();
    drop(reserved);
    let plan = SessionPlan::new(
        moth_core::ConnectionMode::Proxy,
        Some(moth_core::PublicProxyOptions {
            port: public_port,
            ..Default::default()
        }),
    )
    .unwrap();
    let mut proxy_session = engines()
        .connect_proxy_inner(
            &profile,
            &Default::default(),
            &plan,
            None,
            format!("127.0.0.1:{port}").parse().unwrap(),
            &url,
        )
        .await
        .unwrap();
    let access = &plan.public.as_ref().unwrap().access;
    for scheme in ["http", "socks5h"] {
        let local = reqwest::Proxy::all(format!("{scheme}://127.0.0.1:{public_port}"))
            .unwrap()
            .basic_auth(&access.username, &access.password);
        let client = reqwest::Client::builder()
            .no_proxy()
            .proxy(local)
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        assert_eq!(
            client.get(&url).send().await.unwrap().status(),
            reqwest::StatusCode::NO_CONTENT
        );
    }
    for password in [None, Some("wrong-password")] {
        let mut proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{public_port}")).unwrap();
        if let Some(password) = password {
            proxy = proxy.basic_auth("moth", password);
        }
        let response = reqwest::Client::builder()
            .no_proxy()
            .proxy(proxy)
            .build()
            .unwrap()
            .get(&url)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::PROXY_AUTHENTICATION_REQUIRED
        );
    }
    let api = proxy_session
        .telemetry()
        .expect("Pinned sing-box must support measured traffic");
    api.assert_requires_authentication().await;
    let measured = api.snapshot().await.unwrap();
    assert!(measured.uploaded_bytes > 0 && measured.downloaded_bytes > 0);
    assert!(proxy_session.alive());
    proxy_session.stop().await;
    assert!(
        tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, public_port))
            .await
            .is_err()
    );
    #[cfg(windows)]
    {
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let bridge = LocalBridge {
            port: reservation.local_addr().unwrap().port(),
            credential: Uuid::new_v4(),
        };
        drop(reservation);
        let mut config = moth_core::xray_bridge_config(&profile, &bridge);
        config["outbounds"][0]["streamSettings"]["sockopt"] = serde_json::json!({"interface":super::tunnel::upstream_interface("127.0.0.1".parse().unwrap()).unwrap()});
        let mut process = engines()
            .spawn_config(Engine::Xray, &config, false)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        super::tunnel::fixture_bridge(&bridge, std::net::Ipv4Addr::LOCALHOST, endpoint_port).await;
        process.stop().await.unwrap();
    }
    let mut bad = profile.clone();
    bad.credential = Uuid::new_v4().to_string();
    assert!(engines().probe_url(&bad, &url).await.is_err());
    server.stop().await.unwrap();
    endpoint_task.abort();
    // The owned listening process was actually reaped, not just hidden in UI state.
    assert!(server.child.try_wait().unwrap().is_some());
}

#[test]
fn missing_or_modified_engines_never_launch() {
    let set = EngineSet::new(std::env::temp_dir().join(format!("moth-missing-{}", Uuid::new_v4())));
    assert!(matches!(
        set.verified_path(Engine::Xray),
        Err(EngineError::Missing)
    ));
    let directory = tempfile::tempdir().unwrap();
    let file = if cfg!(windows) { "xray.exe" } else { "xray" };
    std::fs::write(directory.path().join(file), b"modified executable").unwrap();
    let set = EngineSet::new(directory.path().into());
    assert!(matches!(
        set.verified_path(Engine::Xray),
        Err(EngineError::Integrity)
    ));
}
