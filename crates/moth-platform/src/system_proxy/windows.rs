use super::FAILED;
use serde::{Deserialize, Serialize};
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{Foundation::GlobalFree, Networking::WinInet::*};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    flags: u32,
    server: String,
    bypass: String,
    pac: String,
}
impl Drop for Config {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.server.zeroize();
        self.bypass.zeroize();
        self.pac.zeroize();
    }
}
fn wide(s: &str) -> Result<Vec<u16>, &'static str> {
    if s.len() > 16384 || s.contains('\0') {
        return Err(FAILED);
    }
    Ok(s.encode_utf16().chain(Some(0)).collect())
}
unsafe fn owned_string(pointer: *mut u16) -> Result<String, &'static str> {
    if pointer.is_null() {
        return Ok(String::new());
    }
    let result = (|| {
        let len = (0..16385)
            .find(|&i| unsafe { *pointer.add(i) } == 0)
            .ok_or(FAILED)?;
        if len > 16384 {
            return Err(FAILED);
        }
        String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, len) }).map_err(|_| FAILED)
    })();
    unsafe {
        GlobalFree(pointer.cast());
    }
    result
}
pub(super) fn read() -> Result<Config, &'static str> {
    let mut options = [INTERNET_PER_CONN_OPTIONW::default(); 4];
    for (option, kind) in options.iter_mut().zip([
        INTERNET_PER_CONN_FLAGS_UI,
        INTERNET_PER_CONN_PROXY_SERVER,
        INTERNET_PER_CONN_PROXY_BYPASS,
        INTERNET_PER_CONN_AUTOCONFIG_URL,
    ]) {
        option.dwOption = kind;
    }
    let mut list = INTERNET_PER_CONN_OPTION_LISTW {
        dwSize: size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
        pszConnection: null_mut(),
        dwOptionCount: 4,
        dwOptionError: 0,
        pOptions: options.as_mut_ptr(),
    };
    let mut size = list.dwSize;
    if unsafe {
        InternetQueryOptionW(
            null(),
            INTERNET_OPTION_PER_CONNECTION_OPTION,
            (&mut list as *mut INTERNET_PER_CONN_OPTION_LISTW).cast(),
            &mut size,
        )
    } == 0
    {
        return Err(FAILED);
    }
    // Read/free all allocated strings even if one is malformed.
    let server = unsafe { owned_string(options[1].Value.pszValue) };
    let bypass = unsafe { owned_string(options[2].Value.pszValue) };
    let pac = unsafe { owned_string(options[3].Value.pszValue) };
    Ok(Config {
        flags: unsafe { options[0].Value.dwValue },
        server: server?,
        bypass: bypass?,
        pac: pac?,
    })
}
pub(super) fn desired(port: u16) -> Result<Config, &'static str> {
    Ok(Config {
        flags: PROXY_TYPE_PROXY,
        server: format!("http=127.0.0.1:{port};https=127.0.0.1:{port}"),
        bypass: "localhost;127.0.0.1;[::1]".into(),
        pac: String::new(),
    })
}
pub(super) fn write(config: &Config) -> Result<(), &'static str> {
    if config.flags
        & !(PROXY_TYPE_DIRECT
            | PROXY_TYPE_PROXY
            | PROXY_TYPE_AUTO_PROXY_URL
            | PROXY_TYPE_AUTO_DETECT)
        != 0
    {
        return Err(FAILED);
    }
    let mut strings = [
        wide(&config.server)?,
        wide(&config.bypass)?,
        wide(&config.pac)?,
    ];
    let mut options = [INTERNET_PER_CONN_OPTIONW::default(); 4];
    options[0].dwOption = INTERNET_PER_CONN_FLAGS;
    options[0].Value.dwValue = config.flags;
    for (i, kind) in [
        INTERNET_PER_CONN_PROXY_SERVER,
        INTERNET_PER_CONN_PROXY_BYPASS,
        INTERNET_PER_CONN_AUTOCONFIG_URL,
    ]
    .into_iter()
    .enumerate()
    {
        options[i + 1].dwOption = kind;
        options[i + 1].Value.pszValue = strings[i].as_mut_ptr();
    }
    let list = INTERNET_PER_CONN_OPTION_LISTW {
        dwSize: size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
        pszConnection: null_mut(),
        dwOptionCount: 4,
        dwOptionError: 0,
        pOptions: options.as_mut_ptr(),
    };
    unsafe {
        if InternetSetOptionW(
            null(),
            INTERNET_OPTION_PER_CONNECTION_OPTION,
            (&list as *const INTERNET_PER_CONN_OPTION_LISTW).cast(),
            list.dwSize,
        ) == 0
        {
            return Err(FAILED);
        }
        if InternetSetOptionW(null(), INTERNET_OPTION_SETTINGS_CHANGED, null(), 0) == 0
            || InternetSetOptionW(null(), INTERNET_OPTION_REFRESH, null(), 0) == 0
        {
            return Err(FAILED);
        }
    }
    Ok(())
}
pub(super) fn restore(before: &Config, installed: &Config) -> Result<(), &'static str> {
    let current = read()?;
    if current == *before {
        return Ok(());
    }
    if current != *installed {
        return Err("Прокси изменён вне Moth; изменения сохранены. Проверьте параметры ОС и журнал восстановления");
    }
    write(before)?;
    if read()? != *before {
        return Err(FAILED);
    }
    Ok(())
}
