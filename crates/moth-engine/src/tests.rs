use super::*;
use tokio::io::AsyncReadExt;

fn engines() -> EngineSet {
    EngineSet::new(development_directory().unwrap())
}
fn fixture_links() -> Vec<String> {
    let base = "vless://c6b3a07a-8bf9-4a18-905b-ea41642633c3@example.com:443";
    let mut links = vec![];
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
