#[cfg(windows)]
mod scm;
#[cfg(windows)]
mod server;
#[cfg(windows)]
mod tunnel;

#[cfg(windows)]
fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        Some("--service") => scm::dispatch(),
        Some("--version") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--check-install" | "--check-installed") => {
            let root = if std::env::args().nth(1).as_deref() == Some("--check-installed") {
                std::env::var_os("ProgramFiles")
                    .map(|p| std::path::PathBuf::from(p).join("Moth Desktop"))
            } else {
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            };
            match root {
                Some(root) => {
                    moth_ipc::windows::validate_installation(&root).map_err(|e| e.to_string())
                }
                None => Err("Не удалось определить каталог установки".into()),
            }
        }
        Some("--console") => {
            if matches!(moth_ipc::windows::current_sid(),Ok(sid) if sid=="S-1-5-18") {
                eprintln!("Console mode cannot run as LocalSystem");
                std::process::exit(1);
            }
            let runtime = tokio::runtime::Runtime::new().expect("runtime");
            runtime.block_on(async {
                let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
                tokio::spawn(async move {
                    let _ = tokio::signal::ctrl_c().await;
                    let _ = shutdown_tx.send(true);
                });
                server::run(true, shutdown_rx, None)
                    .await
                    .map_err(|e| e.to_string())
            })
        }
        Some("--status") => tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(async {
                let health = moth_ipc::windows::ServiceClient::production()
                    .health()
                    .await
                    .map_err(|e| e.to_string())?;
                println!(
                    "{}",
                    serde_json::to_string(&health).map_err(|_| "Serialization failed")?
                );
                Ok(())
            }),
        Some("--tunnel-status" | "--connect" | "--disconnect") => {
            let command = match std::env::args().nth(1).as_deref() {
                Some("--connect") => {
                    use std::io::Read;
                    let mut link = String::new();
                    if std::io::stdin()
                        .take(16385)
                        .read_to_string(&mut link)
                        .is_err()
                        || link.len() > 16384
                    {
                        eprintln!("Invalid profile input");
                        std::process::exit(2);
                    }
                    moth_ipc::Command::Connect { link }
                }
                Some("--disconnect") => moth_ipc::Command::Disconnect {},
                _ => moth_ipc::Command::TunnelStatus {},
            };
            tokio::runtime::Runtime::new()
                .expect("runtime")
                .block_on(async {
                    let response = moth_ipc::windows::ServiceClient::production()
                        .call(command)
                        .await
                        .map_err(|e| e.to_string())?;
                    println!(
                        "{}",
                        serde_json::to_string(&response.result)
                            .map_err(|_| "Serialization failed")?
                    );
                    Ok(())
                })
        }
        _ => {
            eprintln!("Usage: moth-service --service | --console | --status");
            std::process::exit(2);
        }
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Moth system service is not implemented on this platform");
    std::process::exit(2);
}
