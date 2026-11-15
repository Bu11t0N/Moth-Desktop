use crate::IpcError;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};

pub(super) fn directory() -> Result<PathBuf, IpcError> {
    Ok(PathBuf::from("/var/lib/moth-desktop"))
}
fn root() -> Result<(), IpcError> {
    // SAFETY: read-only syscall without pointers.
    if unsafe { libc::geteuid() } == 0 {
        Ok(())
    } else {
        Err(IpcError::AccessDenied)
    }
}
fn validate_directory() -> Result<(), IpcError> {
    for path in ["/", "/var", "/var/lib", "/var/lib/moth-desktop"] {
        let metadata = fs::symlink_metadata(path).map_err(|_| IpcError::Unavailable)?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(IpcError::UntrustedPeer);
        }
    }
    Ok(())
}
fn validate_file(file: &File) -> Result<(), IpcError> {
    let metadata = file.metadata().map_err(|_| IpcError::Unavailable)?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.mode() & 0o077 != 0
        || metadata.len() != 0
        || metadata.nlink() != 1
    {
        return Err(IpcError::UntrustedPeer);
    }
    Ok(())
}
pub(super) fn initialize() -> Result<(), IpcError> {
    root()?;
    let directory = directory()?;
    match fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))
            .map_err(|_| IpcError::AccessDenied)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(IpcError::AccessDenied),
    }
    validate_directory()?;
    let path = directory.join("maintenance.lock");
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
    {
        Ok(file) => file.sync_all().map_err(|_| IpcError::Unavailable)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(IpcError::AccessDenied),
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| IpcError::AccessDenied)?;
    validate_file(&file)
}
pub(super) fn open(exclusive: bool) -> Result<File, IpcError> {
    root()?;
    validate_directory()?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory()?.join("maintenance.lock"))
        .map_err(|_| IpcError::AccessDenied)?;
    validate_file(&file)?;
    let result = if exclusive {
        file.try_lock()
    } else {
        file.try_lock_shared()
    };
    result.map_err(|_| IpcError::AccessDenied)?;
    Ok(file)
}
