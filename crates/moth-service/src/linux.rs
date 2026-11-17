use moth_ipc::{
    linux::ServiceClient, Command, EngineInfo, Health, IpcError, Request, Response,
    PROTOCOL_VERSION,
};
use std::{
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::UnixListener,
    sync::{watch, Mutex, Semaphore},
    task::JoinSet,
    time::timeout,
};

pub fn main() -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|_| "Runtime unavailable")?;
    match std::env::args().nth(1).as_deref() {
        Some("--version") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--restore-guard") => {
            require_root().map_err(|e| e.to_string())?;
            moth_engine::guard::restore().map_err(|_| "Could not restore Moth egress guard".into())
        }
        Some("--release-guard") => {
            require_root().map_err(|e| e.to_string())?;
            let _lease = moth_ipc::maintenance::UpdateLease::acquire()
                .map_err(|_| "Stop the service/VPN before administrator recovery")?;
            moth_engine::guard::release().map_err(|_| "Could not remove Moth egress rules".into())
        }
        Some("--check-install" | "--check-installed") => {
            moth_ipc::linux::validate_installation().map_err(|e| e.to_string())
        }
        Some("--cleanup-network") => {
            require_root().map_err(|e| e.to_string())?;
            let _lock = lock_runtime().map_err(|e| e.to_string())?;
            moth_engine::recover_network().map_err(|e| e.to_string())
        }
        Some("--prepare-install") => {
            require_root().map_err(|e| e.to_string())?;
            moth_ipc::maintenance::initialize().map_err(|e| e.to_string())
        }
        Some("--service") => {
            require_root().map_err(|e| e.to_string())?;
            runtime.block_on(async {
                let (tx, rx) = watch::channel(false);
                tokio::spawn(async move {
                    let mut terminate =
                        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                            .expect("signal");
                    tokio::select! { _=terminate.recv()=>{}, _=tokio::signal::ctrl_c()=>{} }
                    let _ = tx.send(true);
                });
                run(rx).await.map_err(|e| e.to_string())
            })
        }
        Some("--status") => runtime.block_on(async {
            let health = ServiceClient::production()
                .health()
                .await
                .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string(&health).map_err(|_| "Serialization failed")?
            );
            Ok(())
        }),
        Some("--tunnel-status" | "--disconnect" | "--connect") => {
            let command = match std::env::args().nth(1).as_deref() {
                Some("--connect") => {
                    use std::io::Read;
                    let mut link = String::new();
                    std::io::stdin()
                        .take(16385)
                        .read_to_string(&mut link)
                        .map_err(|_| "Invalid profile input")?;
                    if link.len() > 16384 {
                        return Err("Invalid profile input".into());
                    }
                    Command::Connect { link }
                }
                Some("--disconnect") => Command::Disconnect {},
                _ => Command::TunnelStatus {},
            };
            runtime.block_on(async {
                let response = ServiceClient::production()
                    .call(command)
                    .await
                    .map_err(|e| e.to_string())?;
                println!(
                    "{}",
                    serde_json::to_string(&response.result).map_err(|_| "Serialization failed")?
                );
                Ok(())
            })
        }
        _ => Err(
            "Usage: moth-service --service | --status | --tunnel-status | --connect | --disconnect"
                .into(),
        ),
    }
}
fn require_root() -> Result<(), IpcError> {
    // SAFETY: a read-only libc call without pointers.
    if unsafe { libc::geteuid() } != 0 {
        return Err(IpcError::AccessDenied);
    }
    moth_ipc::linux::validate_installation()?;
    if std::env::current_exe().map_err(|_| IpcError::Unavailable)?
        != Path::new("/usr/lib/moth-desktop/moth-service")
    {
        return Err(IpcError::UntrustedPeer);
    }
    Ok(())
}
fn lock_runtime() -> Result<fs::File, IpcError> {
    for path in ["/run", "/run/moth-desktop"] {
        let metadata = fs::symlink_metadata(path).map_err(|_| IpcError::Unavailable)?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(IpcError::UntrustedPeer);
        }
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open("/run/moth-desktop/daemon.lock")
        .map_err(|_| IpcError::Unavailable)?;
    let metadata = file.metadata().map_err(|_| IpcError::Unavailable)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o077 != 0 {
        return Err(IpcError::UntrustedPeer);
    }
    file.try_lock().map_err(|_| IpcError::AccessDenied)?;
    Ok(file)
}
async fn run(mut shutdown: watch::Receiver<bool>) -> Result<(), IpcError> {
    let _lock = lock_runtime()?;
    moth_engine::recover_network().map_err(|_| IpcError::Unavailable)?;
    let path = moth_ipc::linux::SOCKET;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.uid() == 0 && metadata.file_type().is_socket() => {
            fs::remove_file(path).map_err(|_| IpcError::Unavailable)?
        }
        Ok(_) => return Err(IpcError::UntrustedPeer),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(IpcError::Unavailable),
    }
    let engines = Arc::new(moth_engine::EngineSet::new(
        Path::new(moth_ipc::linux::INSTALL_ROOT).join("engines"),
    ));
    let status = engines.status();
    if status.iter().any(|engine| !engine.available) {
        return Err(IpcError::Unavailable);
    }
    let health = Arc::new(Health {
        version: env!("CARGO_PKG_VERSION").into(),
        protocol: PROTOCOL_VERSION,
        system_tunnel: true,
        engines: status
            .into_iter()
            .map(|engine| EngineInfo {
                name: engine.name,
                version: engine.version,
                available: engine.available,
            })
            .collect(),
    });
    let listener = UnixListener::bind(path).map_err(|_| IpcError::Unavailable)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o666))
        .map_err(|_| IpcError::Unavailable)?;
    let capacity = Arc::new(Semaphore::new(8));
    let gate = Arc::new(Mutex::new(()));
    let tunnel = Arc::new(crate::tunnel::Controller::default());
    tunnel.restore_guard().await?;
    let mut tasks = JoinSet::new();
    let mut monitor = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            _=shutdown.changed()=>{if *shutdown.borrow(){break;}},
            _=monitor.tick()=>{let tunnel=tunnel.clone();let engines=engines.clone();tasks.spawn(async move{tunnel.monitor(&engines).await;Ok::<(),IpcError>(())});},
            accepted=listener.accept()=>{
                let (mut stream,_)=accepted.map_err(|_|IpcError::Unavailable)?;
                let Ok(permit)=capacity.clone().try_acquire_owned() else {continue;};
                let Ok(caller)=moth_ipc::linux::authorize_client(&stream) else {continue;};
                let engines=engines.clone();let health=health.clone();let gate=gate.clone();let tunnel=tunnel.clone();
                tasks.spawn(async move {
                    let _permit=permit;
                    let request:Request=timeout(Duration::from_secs(2),moth_ipc::read_frame(&mut stream)).await.map_err(|_|IpcError::Timeout)??;
                    let result=crate::control::dispatch(&request,&caller,&engines,&health,&gate,&tunnel,false).await;
                    let response=Response {version:PROTOCOL_VERSION,id:request.id,result};
                    timeout(Duration::from_secs(2),moth_ipc::write_frame(&mut stream,&response)).await.map_err(|_|IpcError::Timeout)??;
                    Ok::<_,IpcError>(())
                });
            },
            Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    tunnel.shutdown().await;
    fs::remove_file(path).map_err(|_| IpcError::Unavailable)?;
    Ok(())
}
