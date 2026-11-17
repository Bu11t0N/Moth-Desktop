use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
fn path() -> Result<PathBuf, &'static str> {
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .filter(|p| p.is_absolute())
        .ok_or("Каталог настроек пользователя недоступен")?;
    Ok(root.join("autostart/org.mothvpn.desktop.desktop"))
}
pub(super) fn registration(executable: &Path) -> Result<String, &'static str> {
    super::desktop_entry(executable)
}
pub(super) fn read() -> Result<Option<String>, &'static str> {
    let path = path()?;
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Не удалось прочитать пользовательский автозапуск"),
    };
    let metadata = file
        .metadata()
        .map_err(|_| "Не удалось прочитать пользовательский автозапуск")?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o022 != 0
        || metadata.nlink() != 1
        || metadata.len() > 32768
    {
        return Err("Неизвестная запись автозапуска сохранена без изменений");
    }
    let mut body = String::new();
    file.take(32769)
        .read_to_string(&mut body)
        .map_err(|_| "Не удалось прочитать пользовательский автозапуск")?;
    Ok(Some(body))
}
pub(super) fn write(value: Option<&str>) -> Result<(), &'static str> {
    let path = path()?;
    let Some(value) = value else {
        return match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err("Не удалось удалить запись автозапуска"),
        };
    };
    let parent = path.parent().ok_or("Каталог автозапуска недоступен")?;
    if !parent.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|_| "Каталог автозапуска недоступен")?;
    }
    let metadata = fs::metadata(parent).map_err(|_| "Каталог автозапуска недоступен")?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o022 != 0
    {
        return Err("Небезопасные права каталога автозапуска");
    }
    let mut stage = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "Не удалось создать запись автозапуска")?;
    stage
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| "Не удалось защитить запись автозапуска")?;
    stage
        .write_all(value.as_bytes())
        .map_err(|_| "Не удалось создать запись автозапуска")?;
    stage
        .as_file()
        .sync_all()
        .map_err(|_| "Не удалось сохранить запись автозапуска")?;
    stage
        .persist(&path)
        .map_err(|_| "Не удалось сохранить запись автозапуска")?;
    Ok(())
}
