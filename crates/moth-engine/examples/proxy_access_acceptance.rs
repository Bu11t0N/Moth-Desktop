//! Disposable CI only. Export stdout is captured as private fixture IPC, never
//! displayed or uploaded. Passwords are passed to the peer client through stdin.
use std::io::Read;
fn main() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        if std::env::args().nth(1).as_deref() == Some("export") {
            let reply = moth_ipc::ServiceClient::production()
                .call(moth_ipc::Command::ProxyAccess {})
                .await
                .unwrap();
            let moth_ipc::Reply::ProxyAccess(access) = reply.result else {
                panic!("Owner-only access unavailable");
            };
            println!("{}", serde_json::to_string(&access).unwrap());
            return;
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            access: moth_ipc::ProxyAccess,
            source: std::net::IpAddr,
            expected: bool,
        }
        let mut text = zeroize::Zeroizing::new(String::new());
        std::io::stdin()
            .take(8193)
            .read_to_string(&mut text)
            .unwrap();
        assert!(text.len() <= 8192);
        let input: Input = serde_json::from_str(&text).unwrap();
        let address = std::net::SocketAddr::new(input.access.address, input.access.port);
        for scheme in ["http", "socks5h"] {
            let proxy = reqwest::Proxy::all(format!("{scheme}://{address}"))
                .unwrap()
                .basic_auth(&input.access.username, &input.access.password);
            let client = reqwest::Client::builder()
                .no_proxy()
                .proxy(proxy)
                .local_address(input.source)
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap();
            let success = client
                .get("http://10.231.90.2:18080/lan-client")
                .send()
                .await
                .is_ok_and(|r| r.status() == 204);
            assert_eq!(
                success, input.expected,
                "LAN allowlist/authentication result differs"
            );
        }
        for password in [None, Some("incorrect-fixture-password")] {
            let mut proxy = reqwest::Proxy::all(format!("http://{address}")).unwrap();
            if let Some(password) = password {
                proxy = proxy.basic_auth(&input.access.username, password);
            }
            let result = reqwest::Client::builder()
                .no_proxy()
                .proxy(proxy)
                .local_address(input.source)
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap()
                .get("http://10.231.90.2:18080/no-auth")
                .send()
                .await;
            assert!(
                result.is_ok_and(|r| r.status() == 407),
                "Unauthenticated LAN request was not rejected"
            );
        }
        println!("Actual LAN HTTP/SOCKS authentication and source allowlist verified.");
    });
}
