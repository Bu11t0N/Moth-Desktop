//! Ordinary-user login registration. It never creates a service, elevation
//! task or arbitrary command; external modifications are preserved.
use serde::Serialize;
use std::path::Path;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(windows)]
use windows as platform;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub supported: bool,
    pub registered: bool,
    pub owned: bool,
    pub current_target: bool,
    pub conflict: bool,
}
pub struct Change {
    previous: Option<String>,
    next: Option<String>,
}
impl Change {
    pub fn ownership(&self) -> Option<&str> {
        self.next.as_deref()
    }
    pub fn undo(&self) -> Result<(), &'static str> {
        if platform::read()? == self.next {
            platform::write(self.previous.as_deref())?;
        }
        Ok(())
    }
}
pub fn status(executable: &Path, owner: Option<&str>) -> Result<Status, &'static str> {
    let expected = platform::registration(executable)?;
    let value = platform::read()?;
    let owned = value
        .as_deref()
        .is_some_and(|v| Some(v) == owner || v == expected);
    Ok(Status {
        supported: true,
        registered: value.is_some(),
        owned,
        current_target: value.as_deref() == Some(expected.as_str()),
        conflict: value.is_some() && !owned,
    })
}
pub fn set(enabled: bool, executable: &Path, owner: Option<&str>) -> Result<Change, &'static str> {
    let expected = platform::registration(executable)?;
    let previous = platform::read()?;
    if previous
        .as_deref()
        .is_some_and(|v| Some(v) != owner && v != expected)
    {
        return Err("Запись автозапуска изменена вне Moth; она сохранена без изменений");
    }
    let next = enabled.then_some(expected);
    if previous != next {
        platform::write(next.as_deref())?;
    }
    Ok(Change { previous, next })
}
#[cfg(any(target_os = "linux", test))]
fn desktop_entry(executable: &Path) -> Result<String, &'static str> {
    let path = executable
        .to_str()
        .filter(|s| s.starts_with('/') && s.len() < 16300 && !s.chars().any(char::is_control))
        .ok_or("Не удалось создать запись автозапуска")?;
    let mut argument = String::new();
    for c in path.chars() {
        match c {
            '\\' => argument.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                argument.push_str("\\\\");
                argument.push(c);
            }
            '%' => argument.push_str("%%"),
            _ => argument.push(c),
        }
    }
    Ok(format!("[Desktop Entry]\nType=Application\nName=Moth Desktop\nExec=\"{argument}\" --autostart\nTerminal=false\nX-GNOME-Autostart-enabled=true\nX-Moth-Owner=org.mothvpn.desktop\n"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn login_registration_cannot_expand_field_codes_or_shell_fragments() {
        let body = desktop_entry(Path::new("/opt/Moth $literal/100%/moth")).unwrap();
        assert!(body.contains("\\\\$literal") && body.contains("100%%"));
        assert!(!body.contains("sudo") && !body.contains("sh -c"));
        assert!(desktop_entry(Path::new("/opt/moth\nHidden=false")).is_err());
    }
}
