//! Per-user OS proxy ownership journal. Keep the serialized journal exclusively
//! in protected storage: previous PAC URLs or authentication can contain secrets.
use serde::{Deserialize, Serialize};
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(windows)]
use windows as platform;

pub const FAILED: &str = "Системный прокси недоступен; проверьте настройки прокси ОС";
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub supported: bool,
    pub recovery_pending: bool,
    pub owned: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    platform: String,
    before: platform::Config,
    installed: platform::Config,
}
fn decode(text: &str) -> Result<Journal, &'static str> {
    if text.len() > 65536 {
        return Err(FAILED);
    }
    let j: Journal = serde_json::from_str(text).map_err(|_| FAILED)?;
    if j.version != 1 || j.platform != std::env::consts::OS {
        return Err(FAILED);
    }
    Ok(j)
}
/// Prepare before changing the OS. Caller must persist this exact journal first.
pub fn prepare(port: u16) -> Result<String, &'static str> {
    if port < 1024 {
        return Err(FAILED);
    }
    let before = platform::read()?;
    let installed = platform::desired(port)?;
    serde_json::to_string(&Journal {
        version: 1,
        platform: std::env::consts::OS.into(),
        before,
        installed,
    })
    .map_err(|_| FAILED)
}
pub fn apply(text: &str) -> Result<(), &'static str> {
    let j = decode(text)?;
    if platform::read()? != j.before {
        return Err("Параметры прокси изменены вне Moth; повторите подключение");
    }
    platform::write(&j.installed)?;
    if platform::read()? != j.installed {
        return Err(FAILED);
    }
    Ok(())
}
/// Restore only values still equal to our installed values. External changes
/// take precedence, including changes between two application launches.
pub fn restore(text: &str) -> Result<(), &'static str> {
    let j = decode(text)?;
    platform::restore(&j.before, &j.installed)
}
pub fn status(journal: Option<&str>) -> Result<Status, &'static str> {
    let current = platform::read()?;
    let owned = match journal {
        Some(text) => current == decode(text)?.installed,
        None => false,
    };
    Ok(Status {
        supported: true,
        recovery_pending: journal.is_some(),
        owned,
    })
}
