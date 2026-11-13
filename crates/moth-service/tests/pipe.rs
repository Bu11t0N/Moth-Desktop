#![cfg(windows)]
use moth_ipc::{
    windows::{create_server, current_sid, development_pipe, ServiceClient},
    Command, ErrorCode, IpcError, Reply, Request, Response,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command as Process},
    time::{timeout, Duration},
};

async fn daemon() -> Child {
    let mut child = Process::new(env!("CARGO_BIN_EXE_moth-service"))
        .arg("--console")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    timeout(Duration::from_secs(10), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(line.trim(), format!("MOTH_READY {}", child.id().unwrap()));
    child
}

#[tokio::test]
async fn real_console_pipe_health_version_validation_and_recovery() {
    let mut child = daemon().await;
    let pid = child.id().unwrap();
    let client = ServiceClient::development(pid);
    let health = client.health().await.unwrap();
    assert!(!health.system_tunnel);
    assert!(health.engines.iter().all(|e| e.available));
    // Console mode must NEVER start system routing, including in local tests.
    assert!(matches!(
        client
            .call(Command::Connect {
                link: "SECRET".into()
            })
            .await
            .unwrap()
            .result,
        Reply::Failure {
            code: ErrorCode::Unsupported
        }
    ));
    assert!(create_server(&development_pipe(pid), true, Some(&current_sid().unwrap())).is_err());
    let response = client
        .call(Command::Probe {
            link: "SECRET-not-a-profile".into(),
        })
        .await
        .unwrap();
    assert!(matches!(
        response.result,
        Reply::Failure {
            code: ErrorCode::InvalidProfile
        }
    ));
    assert!(!serde_json::to_string(&response).unwrap().contains("SECRET"));
    let mut raw = tokio::net::windows::named_pipe::ClientOptions::new()
        .security_qos_flags(0x10000)
        .open(development_pipe(pid))
        .unwrap();
    let mut request = Request::new(Command::Status {});
    request.version = 99;
    moth_ipc::write_frame(&mut raw, &request).await.unwrap();
    let response: Response = moth_ipc::read_frame(&mut raw).await.unwrap();
    assert!(matches!(
        response.result,
        Reply::Failure {
            code: ErrorCode::VersionMismatch
        }
    ));
    drop(raw);
    let mut raw = tokio::net::windows::named_pipe::ClientOptions::new()
        .security_qos_flags(0x10000)
        .open(development_pipe(pid))
        .unwrap();
    raw.write_u32_le((moth_ipc::MAX_FRAME + 1) as u32)
        .await
        .unwrap();
    let mut byte = [0];
    let read = timeout(Duration::from_secs(3), raw.read(&mut byte))
        .await
        .unwrap();
    assert!(read.is_err() || read.unwrap() == 0);
    assert!(client.health().await.is_ok());
    child.kill().await.unwrap();
}

#[tokio::test]
async fn rejects_spoofed_server_before_sending_credentials() {
    let fake_pid = std::process::id() + 10000000;
    let mut fake = create_server(
        &development_pipe(fake_pid),
        true,
        Some(&current_sid().unwrap()),
    )
    .unwrap();
    let task = tokio::spawn(async move {
        fake.connect().await.unwrap();
        let mut buf = [0u8; 128];
        timeout(Duration::from_secs(3), fake.read(&mut buf))
            .await
            .unwrap()
            .unwrap_or(0)
    });
    let result = ServiceClient::development(fake_pid)
        .call(Command::Probe {
            link: "VERY_SECRET".into(),
        })
        .await;
    assert!(matches!(result, Err(IpcError::UntrustedPeer)));
    assert_eq!(task.await.unwrap(), 0);
}
