//! Fixed systemd Unix socket transport. Credentials never precede peer authentication.
use crate::{Command, Health, IpcError, Reply, Request, Response};
use std::{
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::Path,
    time::Duration,
};
use tokio::{net::UnixStream, time::timeout};

pub const SOCKET: &str = "/run/moth-desktop/control.sock";
pub const INSTALL_ROOT: &str = "/usr/lib/moth-desktop";

fn root_owned(path: &Path, directory: bool) -> Result<(), IpcError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| IpcError::Unavailable)?;
    if metadata.uid() != 0
        || metadata.mode() & 0o022 != 0
        || metadata.file_type().is_symlink()
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(IpcError::UntrustedPeer);
    }
    Ok(())
}

/// Both the daemon and package acceptance verify the fixed, root-controlled tree.
/// The daemon separately checks baked-in core hashes before every launch.
pub fn validate_installation() -> Result<(), IpcError> {
    for directory in ["/", "/usr", "/usr/lib", INSTALL_ROOT] {
        root_owned(Path::new(directory), true)?;
    }
    fn tree(path: &Path) -> Result<(), IpcError> {
        let metadata = fs::symlink_metadata(path).map_err(|_| IpcError::Unavailable)?;
        root_owned(path, metadata.is_dir())?;
        if metadata.is_dir() {
            for entry in fs::read_dir(path).map_err(|_| IpcError::Unavailable)? {
                tree(&entry.map_err(|_| IpcError::Unavailable)?.path())?;
            }
        }
        Ok(())
    }
    tree(Path::new(INSTALL_ROOT))?;
    for file in ["moth-service", "engines/xray", "engines/sing-box"] {
        root_owned(&Path::new(INSTALL_ROOT).join(file), false)?;
    }
    Ok(())
}

fn validate_socket() -> Result<(), IpcError> {
    for directory in ["/", "/run", "/run/moth-desktop"] {
        root_owned(Path::new(directory), true)?;
    }
    let metadata = fs::symlink_metadata(SOCKET).map_err(|_| IpcError::Unavailable)?;
    if metadata.uid() != 0 || !metadata.file_type().is_socket() {
        return Err(IpcError::UntrustedPeer);
    }
    Ok(())
}

fn authenticate_service(stream: &UnixStream) -> Result<(), IpcError> {
    let peer = stream.peer_cred().map_err(|_| IpcError::UntrustedPeer)?;
    if peer.uid() != 0 || !matches!(peer.pid(), Some(pid) if pid > 1) {
        return Err(IpcError::UntrustedPeer);
    }
    Ok(())
}

/// Local root maintenance or a desktop account with a real login runtime directory.
/// Ownership comes only from SO_PEERCRED, never a caller-supplied UID in a frame.
pub fn authorize_client(stream: &UnixStream) -> Result<String, IpcError> {
    let peer = stream.peer_cred().map_err(|_| IpcError::AccessDenied)?;
    let uid = peer.uid();
    if !matches!(peer.pid(), Some(pid) if pid > 0) {
        return Err(IpcError::AccessDenied);
    }
    if uid != 0 {
        if !(1000..65534).contains(&uid) {
            return Err(IpcError::AccessDenied);
        }
        root_owned(Path::new("/run/user"), true)?;
        let runtime =
            fs::symlink_metadata(format!("/run/user/{uid}")).map_err(|_| IpcError::AccessDenied)?;
        if !runtime.is_dir() || runtime.uid() != uid || runtime.mode() & 0o077 != 0 {
            return Err(IpcError::AccessDenied);
        }
    }
    Ok(format!("uid:{uid}"))
}

pub struct ServiceClient;

/// Missing/broken IPC alone never authorizes an exit. Require an inactive unit
/// and absence of both the network journal and private interface.
pub fn service_is_stopped() -> Result<bool, IpcError> {
    use std::{
        io::Read,
        process::{Command, Stdio},
        time::Instant,
    };
    let mut process = Command::new("/usr/bin/systemctl")
        .args([
            "show",
            "moth-desktop.service",
            "--property=ActiveState",
            "--value",
        ])
        .env_clear()
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| IpcError::Unavailable)?;
    let start = Instant::now();
    loop {
        match process.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(None) if start.elapsed() < Duration::from_secs(2) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = process.kill();
                let _ = process.wait();
                return Err(IpcError::Unavailable);
            }
        }
    }
    let mut state = String::new();
    process
        .stdout
        .take()
        .ok_or(IpcError::Unavailable)?
        .take(64)
        .read_to_string(&mut state)
        .map_err(|_| IpcError::Unavailable)?;
    if !matches!(state.trim(), "inactive" | "failed") {
        return Ok(false);
    }
    for path in ["/run/moth-desktop/network-v1.json", "/sys/class/net/moth0"] {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => return Ok(false),
            Err(_) => return Err(IpcError::Unavailable),
        }
    }
    Ok(true)
}
impl ServiceClient {
    pub fn production() -> Self {
        Self
    }
    pub async fn call(&self, command: Command) -> Result<Response, IpcError> {
        validate_socket()?;
        let mut stream = timeout(Duration::from_secs(3), UnixStream::connect(SOCKET))
            .await
            .map_err(|_| IpcError::Timeout)?
            .map_err(|_| IpcError::Unavailable)?;
        authenticate_service(&stream)?;
        validate_socket()?;
        let request = Request::new(command);
        timeout(
            Duration::from_secs(2),
            crate::write_frame(&mut stream, &request),
        )
        .await
        .map_err(|_| IpcError::Timeout)??;
        let response = timeout(Duration::from_secs(30), crate::read_frame(&mut stream))
            .await
            .map_err(|_| IpcError::Timeout)??;
        crate::validate_response(&request, &response)?;
        Ok(response)
    }
    pub async fn health(&self) -> Result<Health, IpcError> {
        match self.call(Command::Status {}).await?.result {
            Reply::Status(health) => Ok(health),
            _ => Err(IpcError::Protocol),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn root_peer_is_required_before_a_credential_can_be_written() {
        let (client, _server) = UnixStream::pair().unwrap();
        // The CI normal user must not be accepted as a privileged daemon.
        let uid = client.peer_cred().unwrap().uid();
        assert_eq!(authenticate_service(&client).is_ok(), uid == 0);
    }
}
