//! A fixed root-owned kernel lease coordinates service connections and package
//! maintenance without adding a privileged command interpreter to IPC.
use crate::IpcError;
use std::{fs::File, path::PathBuf};
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
