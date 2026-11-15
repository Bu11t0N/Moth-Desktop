//! A fixed root-owned kernel lease coordinates service connections and package
//! maintenance without adding a privileged command interpreter to IPC.
use crate::IpcError;
use std::{
    fs::File,
    path::{Path, PathBuf},
};
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(windows)]
use windows as platform;

pub fn state_directory() -> Result<PathBuf, IpcError> {
    platform::directory()
}
/// Explicit elevated installation only; service Connect never creates state.
pub fn initialize() -> Result<(), IpcError> {
    platform::initialize()
}

/// Fixed-path maintenance tools use this only for descendants of state_directory.
/// The existing parent must be administrator/root owned and non-writable by users.
pub fn create_protected_directory(path: &Path) -> Result<(), IpcError> {
    platform::create_directory(path)
}
pub fn create_protected_file(path: &Path) -> Result<File, IpcError> {
    platform::create_file(path, true)
}
/// Unverified input must stay administrator/root-only even if verification fails.
pub fn create_private_file(path: &Path) -> Result<File, IpcError> {
    platform::create_file(path, false)
}
pub fn validate_protected_path(path: &Path) -> Result<(), IpcError> {
    platform::validate_path(path)
}

pub struct TunnelLease {
    _file: File,
}
impl TunnelLease {
    pub fn acquire() -> Result<Self, IpcError> {
        Ok(Self {
            _file: platform::open(false)?,
        })
    }
}
pub struct UpdateLease {
    file: File,
}
impl UpdateLease {
    pub fn acquire() -> Result<Self, IpcError> {
        Ok(Self {
            file: platform::open(true)?,
        })
    }
    pub fn file(&self) -> &File {
        &self.file
    }
}
