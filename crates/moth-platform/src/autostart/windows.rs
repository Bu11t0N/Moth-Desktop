use std::path::Path;
use windows_sys::Win32::{Foundation::*, System::Registry::*};
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
const PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const NAME: &str = "Moth Desktop";
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
pub(super) fn registration(executable: &Path) -> Result<String, &'static str> {
    let path = executable
        .to_str()
        .filter(|s| {
            executable.is_absolute()
                && s.len() < 16000
                && !s.chars().any(char::is_control)
                && !s.contains('"')
        })
        .ok_or("Не удалось создать запись автозапуска")?;
    Ok(format!("\"{path}\" --autostart"))
}
pub(super) fn read() -> Result<Option<String>, &'static str> {
    let mut key = std::ptr::null_mut();
    let code = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(PATH).as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if code != ERROR_SUCCESS {
        return Err("Не удалось прочитать пользовательский автозапуск");
    }
    let key = Key(key);
    let mut kind = 0;
    let mut size = 0;
    let code = unsafe {
        RegQueryValueExW(
            key.0,
            wide(NAME).as_ptr(),
            std::ptr::null(),
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if code != ERROR_SUCCESS || kind != REG_SZ || size > 32768 || size % 2 != 0 {
        return Err("Неизвестная запись автозапуска сохранена без изменений");
    }
    let mut bytes = vec![0u16; size as usize / 2];
    if unsafe {
        RegQueryValueExW(
            key.0,
            wide(NAME).as_ptr(),
            std::ptr::null(),
            &mut kind,
            bytes.as_mut_ptr().cast(),
            &mut size,
        )
    } != ERROR_SUCCESS
        || kind != REG_SZ
    {
        return Err("Запись автозапуска изменилась во время чтения");
    }
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    if bytes.contains(&0) {
        return Err("Неизвестная запись автозапуска сохранена без изменений");
    }
    String::from_utf16(&bytes)
        .map(Some)
        .map_err(|_| "Некорректная запись автозапуска сохранена без изменений")
}
pub(super) fn write(value: Option<&str>) -> Result<(), &'static str> {
    let mut key = std::ptr::null_mut();
    if unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(PATH).as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    } != ERROR_SUCCESS
    {
        return Err("Не удалось изменить пользовательский автозапуск");
    }
    let key = Key(key);
    let code = if let Some(value) = value {
        let bytes = wide(value);
        unsafe {
            RegSetValueExW(
                key.0,
                wide(NAME).as_ptr(),
                0,
                REG_SZ,
                bytes.as_ptr().cast(),
                (bytes.len() * 2) as u32,
            )
        }
    } else {
        unsafe { RegDeleteValueW(key.0, wide(NAME).as_ptr()) }
    };
    if code != ERROR_SUCCESS && !(value.is_none() && code == ERROR_FILE_NOT_FOUND) {
        return Err("Не удалось изменить пользовательский автозапуск");
    }
    Ok(())
}
