use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

async fn request(port: u16, password: &str, destination: &str) -> Result<String, reqwest::Error> {
    let intermediary = reqwest::Proxy::all(format!("http://127.0.0.1:{port}"))
        .unwrap()
        .basic_auth("moth", password);
    reqwest::Client::builder()
        .no_proxy()
        .proxy(intermediary)
        .timeout(Duration::from_secs(4))
        .build()
        .unwrap()
        .get(destination)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
}

async fn http_proxy(listener: TcpListener, marker: &'static str, relay: Option<u16>) {
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(async move {
            let mut stream = BufReader::new(stream);
            let mut request = String::new();
            if stream.read_line(&mut request).await.unwrap_or_default() == 0 {
                return;
            }
            let mut line = String::new();
            loop {
                line.clear();
                if stream.read_line(&mut line).await.unwrap_or_default() == 0 || line == "\r\n" {
                    break;
                }
            }
            if let Some(port) =
                relay.filter(|port| request.starts_with(&format!("CONNECT 127.0.0.1:{port} ")))
            {
                let mut upstream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
                let mut stream = stream.into_inner();
                stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
                let _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream).await;
                return;
            }
            if request.starts_with("CONNECT ") {
                if stream
                    .get_mut()
                    .write_all(b"HTTP/1.1 200 OK\r\n\r\n")
                    .await
                    .is_err()
                {
                    return;
                }
                loop {
                    line.clear();
                    if stream.read_line(&mut line).await.unwrap_or_default() == 0 || line == "\r\n"
                    {
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{marker}",
                marker.len()
            );
            let _ = stream.get_mut().write_all(response.as_bytes()).await;
        });
    }
}

#[tokio::test]
#[ignore = "requires pinned Mihomo and geodata. Uses only local fixture sockets"]
async fn real_clash_rules_select_chain_and_fake_ip() {
    let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_port = first.local_addr().unwrap().port();
    let second_port = second.local_addr().unwrap().port();
    let first = tokio::spawn(http_proxy(first, "first", Some(second_port)));
    let second = tokio::spawn(http_proxy(second, "second", None));
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bridge = LocalBridge {
        port: reservation.local_addr().unwrap().port(),
        credential: Uuid::new_v4(),
    };
    let document = format!(
        r#"
proxies:
  - {{name: First, type: http, server: 127.0.0.1, port: {first_port}}}
  - {{name: Second, type: http, server: 127.0.0.1, port: {second_port}, dialer-proxy: First}}
proxy-groups:
  - {{name: Manual, type: select, proxies: [First, Second]}}
rule-providers:
  domains:
    type: inline
    behavior: domain
    payload: ['+.ruleset.invalid']
rules:
  - DOMAIN,one.fixture,First
  - DOMAIN,two.fixture,Second
  - RULE-SET,domains,Second
  - GEOSITE,github,First
  - GEOIP,private,Second,no-resolve
  - MATCH,Manual
dns:
  enable: true
  enhanced-mode: fake-ip
  default-nameserver: [1.1.1.1]
  nameserver: [https://1.1.1.1/dns-query]
"#
    );
    let inspection = moth_core::inspect_clash_policy(&document).unwrap().unwrap();
    assert!(inspection.preview.can_apply);
    let policy = inspection.policy.unwrap();
    let prepared = Prepared::new(&policy, &Default::default(), &bridge, None, false).unwrap();
    let dns = prepared.dns.local_addr().unwrap();
    let set = EngineSet::new(development_directory().unwrap());
    let mut capture = moth_core::sing_box_tun_config(&bridge);
    prepared.configure_capture(&mut capture, true).unwrap();
    set.validate(Engine::SingBox, &capture).await.unwrap();
    drop(reservation);
    let (mut process, control) = prepared.start(&set, false).await.unwrap();
    let password = bridge.credential.simple().to_string();
    for (host, expected) in [
        ("one.fixture", "first"),
        ("two.fixture", "second"),
        ("sub.ruleset.invalid", "second"),
        ("github.com", "first"),
        ("127.0.0.42", "second"),
        ("manual.fixture", "first"),
    ] {
        assert_eq!(
            request(bridge.port, &password, &format!("http://{host}/"))
                .await
                .unwrap(),
            expected,
            "{host}"
        );
    }
    control.select("Manual", "Second").await.unwrap();
    assert_eq!(
        control.groups().await.unwrap()[0].selected.as_deref(),
        Some("Second")
    );
    assert_eq!(
        request(bridge.port, &password, "http://manual.fixture/")
            .await
            .unwrap(),
        "second"
    );
    assert!(control.select("Manual", "not-a-member").await.is_err());
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    // One A query for one.fixture. Fake-IP must answer without an external resolver.
    let query =
        b"\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x03one\x07fixture\x00\x00\x01\x00\x01";
    socket.send_to(query, dns).await.unwrap();
    let mut response = [0; 512];
    let (len, _) = timeout(Duration::from_secs(2), socket.recv_from(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&response[..2], &query[..2]);
    assert_eq!(&response[6..8], &[0, 1]);
    assert!(response[..len].windows(2).any(|bytes| bytes == [198, 18]));
    process.stop().await.unwrap();
    first.abort();
    second.abort();
}

#[tokio::test]
#[ignore = "requires pinned Mihomo and geodata. Uses only local fixture sockets"]
async fn real_clash_fallback_uses_refreshed_provider_exit() {
    let failed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let failed_port = failed.local_addr().unwrap().port();
    // Retain the reservation. The health request times out instead of
    // racing another process for a supposedly unused port.
    let backup = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backup_port = backup.local_addr().unwrap().port();
    let backup = tokio::spawn(http_proxy(backup, "provider-exit", None));
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bridge = LocalBridge {
        port: reservation.local_addr().unwrap().port(),
        credential: Uuid::new_v4(),
    };
    let document = format!(
        r#"
proxies:
  - {{name: Failed, type: http, server: 127.0.0.1, port: {failed_port}}}
proxy-providers:
  pool:
    type: http
    url: https://example.com/fixture
    interval: 3600
    health-check: {{enable: true, url: 'http://check.fixture/', interval: 10, timeout: 500, lazy: false}}
proxy-groups:
  - {{name: Automatic, type: fallback, proxies: [Failed], use: [pool], url: 'http://check.fixture/', interval: 10, timeout: 500, lazy: false}}
rules: ['MATCH,Automatic']
"#
    );
    let mut policy = moth_core::inspect_clash_policy(&document)
        .unwrap()
        .unwrap()
        .policy
        .unwrap();
    let provider = policy.providers().remove(0);
    policy
        .with_provider(
            &provider,
            &format!(
                "proxies: [{{name: Provider, type: http, server: 127.0.0.1, port: {backup_port}}}]"
            ),
        )
        .unwrap();
    let set = EngineSet::new(development_directory().unwrap());
    let prepared = Prepared::new(&policy, &Default::default(), &bridge, None, false).unwrap();
    drop(reservation);
    let (mut process, control) = prepared.start(&set, false).await.unwrap();
    // Trigger Mihomo's own health tests and wait for both eligibility results.
    let mut url = control.url(&["group", "Automatic", "delay"]);
    url.query_pairs_mut()
        .append_pair("url", "http://check.fixture/")
        .append_pair("timeout", "1000");
    let probe = control
        .client
        .get(url)
        .bearer_auth(control.token.as_str())
        .send()
        .await
        .unwrap();
    assert!(probe.status().is_success());
    let group = control.groups().await.unwrap().remove(0);
    assert_eq!(group.selected.as_deref(), Some("Provider"));
    assert_eq!(
        request(
            bridge.port,
            &bridge.credential.simple().to_string(),
            "http://actual.fixture/"
        )
        .await
        .unwrap(),
        "provider-exit"
    );
    process.stop().await.unwrap();
    backup.abort();
    drop(failed);
}
