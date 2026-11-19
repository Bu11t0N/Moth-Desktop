//! Hosted VM only. Exercise the actual service, TUN DNS and owner-only API.
use moth_ipc::{Command, ErrorCode, NativeRoute, Reply, ServiceClient, TunnelPhase};
use std::time::Duration;

const FIXTURE: &str = r#"
proxy-groups:
  - {name: Manual, type: select, proxies: [DIRECT, REJECT]}
rules:
  - DOMAIN,cp.cloudflare.com,DIRECT
  - MATCH,Manual
dns:
  enable: true
  enhanced-mode: fake-ip
  default-nameserver: [1.1.1.1]
  nameserver: [https://1.1.1.1/dns-query]
"#;

async fn request(command: Command) -> Reply {
    ServiceClient::production()
        .call(command)
        .await
        .unwrap()
        .result
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let command = Command::BeginClashPolicy {
        document: FIXTURE.into(),
        settings: Default::default(),
        mode: moth_core::ConnectionMode::Tun,
        listener: None,
    };
    if std::env::args().nth(1).as_deref() == Some("guard") {
        assert!(
            matches!(request(Command::GuardStatus {}).await, Reply::Guard(g) if g.enabled && g.filtering)
        );
        assert!(matches!(
            request(command).await,
            Reply::Failure {
                code: ErrorCode::InvalidProfile
            }
        ));
        assert!(
            matches!(request(Command::GuardStatus {}).await, Reply::Guard(g) if g.enabled && g.filtering)
        );
        println!("Enabled guard refused a direct Clash policy and retained kernel filtering");
        return;
    }

    assert!(
        matches!(request(command).await, Reply::Connection(c) if c.phase == TunnelPhase::Connected && c.owned_by_caller)
    );
    assert!(matches!(
        request(Command::NativeRoute {}).await,
        Reply::NativeRoute(NativeRoute::Blocked)
    ));
    let socket = tokio::net::UdpSocket::bind("172.19.0.1:0").await.unwrap();
    let query =
        b"\x82\x02\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x03one\x07fixture\x00\x00\x01\x00\x01";
    socket.send_to(query, "172.19.0.2:53").await.unwrap();
    let mut data = [0; 512];
    let (count, _) = tokio::time::timeout(Duration::from_secs(3), socket.recv_from(&mut data))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&data[..2], &query[..2]);
    assert_eq!(&data[6..8], &[0, 1]);
    assert!(data[..count].windows(2).any(|w| w == [198, 18]));
    let selected = request(Command::ChoosePolicyMember {
        group: "Manual".into(),
        member: "REJECT".into(),
    })
    .await;
    assert!(
        matches!(selected, Reply::PolicyGroups { groups } if groups.len() == 1 && groups[0].selected.as_deref() == Some("REJECT"))
    );
    assert!(
        matches!(request(Command::Disconnect {}).await, Reply::Tunnel(s) if s.phase == TunnelPhase::Disconnected)
    );
    println!(
        "Real service Clash TUN, fake-IP DNS, group selection and private download denial passed"
    );
}
