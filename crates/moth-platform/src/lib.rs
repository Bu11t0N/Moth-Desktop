//! OS boundary. A future Iced UI can use the same domain and platform crates.
use moth_core::Library;
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf};
use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Не удалось прочитать или сохранить хранилище")]
    Io(#[from] std::io::Error),
    #[error("Хранилище повреждено или создано другой версией")]
    Invalid,
    #[error("Не удалось защитить данные учётной записью Windows")]
    Protection,
    #[error("Защищённое хранилище Linux ещё не реализовано")]
    Unsupported,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u32,
    library: Library,
}

pub struct ProfileStore {
    path: PathBuf,
}
impl ProfileStore {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            path: directory.join("profiles.dpapi"),
        }
    }
    pub fn load(&self) -> Result<Library, StoreError> {
        match fs::metadata(&self.path) {
            Ok(meta) if meta.len() > 8 * 1024 * 1024 => return Err(StoreError::Invalid),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        let encrypted = match fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Library::default()),
            Err(e) => return Err(e.into()),
        };
        if encrypted.len() > 8 * 1024 * 1024 {
            return Err(StoreError::Invalid);
        }
        let plain = Zeroizing::new(unseal(&encrypted)?);
        let e: Envelope = serde_json::from_slice(&plain).map_err(|_| StoreError::Invalid)?;
        if e.version != 1 || e.library.validate().is_err() {
            return Err(StoreError::Invalid);
        }
        Ok(e.library)
    }
    pub fn save(&self, library: &Library) -> Result<(), StoreError> {
        library.validate().map_err(|_| StoreError::Invalid)?;
        let plain = Zeroizing::new(
            serde_json::to_vec(&Envelope {
                version: 1,
                library: library.clone(),
            })
            .map_err(|_| StoreError::Invalid)?,
        );
        let encrypted = seal(&plain)?;
        let directory = self.path.parent().ok_or(StoreError::Invalid)?;
        fs::create_dir_all(directory)?;
        // Same-volume atomic replacement; a failed write never replaces the previous library.
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        file.write_all(&encrypted)?;
        file.as_file().sync_all()?;
        file.persist(&self.path)
            .map_err(|e| StoreError::Io(e.error))?;
        Ok(())
    }
}

#[cfg(windows)]
fn protect(input: &[u8], encrypt: bool) -> Result<Vec<u8>, StoreError> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let len = u32::try_from(input.len()).map_err(|_| StoreError::Invalid)?;
    let blob = CRYPT_INTEGER_BLOB {
        cbData: len,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: DPAPI borrows the input for the call and allocates output with LocalAlloc.
    // User scope (no LOCAL_MACHINE) prevents other accounts decrypting the file.
    unsafe {
        let ok = if encrypt {
            CryptProtectData(
                &blob,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &blob,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(StoreError::Protection);
        }
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        if !encrypt {
            std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        }
        LocalFree(output.pbData.cast());
        Ok(bytes)
    }
}
#[cfg(windows)]
fn seal(input: &[u8]) -> Result<Vec<u8>, StoreError> {
    protect(input, true)
}
#[cfg(windows)]
fn unseal(input: &[u8]) -> Result<Vec<u8>, StoreError> {
    protect(input, false)
}
#[cfg(not(windows))]
fn seal(_input: &[u8]) -> Result<Vec<u8>, StoreError> {
    Err(StoreError::Unsupported)
}
#[cfg(not(windows))]
fn unseal(_input: &[u8]) -> Result<Vec<u8>, StoreError> {
    Err(StoreError::Unsupported)
}

/// Explicit capabilities: the interface must never claim a VPN exists before
/// the privileged service, TUN, DNS and filtering are actually implemented.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub platform: &'static str,
    pub secure_storage: bool,
    pub system_tunnel: bool,
    pub kill_switch: bool,
}
pub fn capabilities() -> Capabilities {
    Capabilities {
        platform: std::env::consts::OS,
        secure_storage: cfg!(windows),
        system_tunnel: false,
        kill_switch: false,
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn dpapi_roundtrip_replace_and_corruption() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProfileStore::new(directory.path().to_path_buf());
        let mut lib = Library::default();
        lib.add(moth_core::import_links("trojan://SECRET@example.com:443#Test").unwrap());
        store.save(&lib).unwrap();
        let bytes = fs::read(&store.path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("SECRET"));
        assert_eq!(store.load().unwrap().profiles[0].credential, "SECRET");
        lib.remove(lib.selected.unwrap());
        store.save(&lib).unwrap();
        assert!(store.load().unwrap().profiles.is_empty());
        fs::write(&store.path, b"corrupt").unwrap();
        assert!(store.load().is_err());
    }
}
