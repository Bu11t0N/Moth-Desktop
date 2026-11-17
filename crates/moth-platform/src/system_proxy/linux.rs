//! Fixed GNOME GSettings schemas, accessed in-process as the desktop user.
//! No password/PAC value is passed through argv or written to a plaintext file.
use super::FAILED;
use serde::{Deserialize, Serialize};
use std::{
    ffi::{c_char, c_int, c_void, CStr, CString},
    ptr::null_mut,
};
#[link(name = "gio-2.0")]
unsafe extern "C" {
    fn g_settings_schema_source_get_default() -> *mut c_void;
    fn g_settings_schema_source_lookup(
        source: *mut c_void,
        id: *const c_char,
        recursive: c_int,
    ) -> *mut c_void;
    fn g_settings_schema_has_key(schema: *mut c_void, key: *const c_char) -> c_int;
    fn g_settings_schema_unref(schema: *mut c_void);
    fn g_settings_new_full(
        schema: *mut c_void,
        backend: *mut c_void,
        path: *const c_char,
    ) -> *mut c_void;
    fn g_settings_get_value(settings: *mut c_void, key: *const c_char) -> *mut c_void;
    fn g_settings_is_writable(settings: *mut c_void, key: *const c_char) -> c_int;
    fn g_settings_set_value(settings: *mut c_void, key: *const c_char, value: *mut c_void)
        -> c_int;
    fn g_settings_sync();
}
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_variant_print(value: *mut c_void, annotate: c_int) -> *mut c_char;
    fn g_variant_parse(
        kind: *mut c_void,
        text: *const c_char,
        limit: *const c_char,
        end: *mut *const c_char,
        error: *mut *mut c_void,
    ) -> *mut c_void;
    fn g_variant_unref(value: *mut c_void);
    fn g_variant_take_ref(value: *mut c_void) -> *mut c_void;
    fn g_free(memory: *mut c_void);
}
#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_object_unref(object: *mut c_void);
}

// The final mode write activates manual proxy only after all endpoint values
// are committed. TUN-only capture, WinHTTP and unrelated desktop settings are
// deliberately outside the ordinary-user system-proxy adapter.
const KEYS: [(&str, &str); 12] = [
    ("org.gnome.system.proxy.http", "host"),
    ("org.gnome.system.proxy.http", "port"),
    ("org.gnome.system.proxy.http", "use-authentication"),
    ("org.gnome.system.proxy.http", "authentication-user"),
    ("org.gnome.system.proxy.http", "authentication-password"),
    ("org.gnome.system.proxy.https", "host"),
    ("org.gnome.system.proxy.https", "port"),
    ("org.gnome.system.proxy.socks", "host"),
    ("org.gnome.system.proxy.socks", "port"),
    ("org.gnome.system.proxy.ftp", "host"),
    ("org.gnome.system.proxy.ftp", "port"),
    ("org.gnome.system.proxy", "mode"),
];
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    values: Vec<String>,
}
impl Drop for Config {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        for value in &mut self.values {
            value.zeroize();
        }
    }
}
struct Setting(*mut c_void, CString);
impl Setting {
    fn open(index: usize) -> Result<Self, &'static str> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !desktop.split(':').any(|s| s == "gnome" || s == "unity") {
            return Err(
                "Системный прокси поддерживается в GNOME; задайте прокси вручную в вашей среде",
            );
        }
        let (schema, key) = KEYS[index];
        let schema = CString::new(schema).map_err(|_| FAILED)?;
        let key = CString::new(key).map_err(|_| FAILED)?;
        unsafe {
            let source = g_settings_schema_source_get_default();
            if source.is_null() {
                return Err(FAILED);
            }
            let schema = g_settings_schema_source_lookup(source, schema.as_ptr(), 1);
            if schema.is_null() {
                return Err(FAILED);
            }
            let settings = if g_settings_schema_has_key(schema, key.as_ptr()) != 0 {
                g_settings_new_full(schema, null_mut(), std::ptr::null())
            } else {
                null_mut()
            };
            g_settings_schema_unref(schema);
            if settings.is_null() {
                return Err(FAILED);
            }
            Ok(Self(settings, key))
        }
    }
    fn read(&self) -> Result<String, &'static str> {
        unsafe {
            let value = g_settings_get_value(self.0, self.1.as_ptr());
            if value.is_null() {
                return Err(FAILED);
            }
            let text = g_variant_print(value, 1);
            g_variant_unref(value);
            if text.is_null() {
                return Err(FAILED);
            }
            let result = CStr::from_ptr(text)
                .to_str()
                .map_err(|_| FAILED)
                .and_then(|s| {
                    if s.len() <= 16384 {
                        Ok(s.to_owned())
                    } else {
                        Err(FAILED)
                    }
                });
            g_free(text.cast());
            result
        }
    }
    fn writable(&self) -> bool {
        unsafe { g_settings_is_writable(self.0, self.1.as_ptr()) != 0 }
    }
    fn write(&self, text: &str) -> Result<(), &'static str> {
        if text.len() > 16384 {
            return Err(FAILED);
        }
        let text = CString::new(text).map_err(|_| FAILED)?;
        unsafe {
            let value = g_variant_parse(
                null_mut(),
                text.as_ptr(),
                std::ptr::null(),
                null_mut(),
                null_mut(),
            );
            if value.is_null() {
                return Err(FAILED);
            }
            g_variant_take_ref(value);
            let ok = g_settings_set_value(self.0, self.1.as_ptr(), value) != 0;
            g_variant_unref(value);
            if !ok {
                return Err(FAILED);
            }
        }
        Ok(())
    }
}
impl Drop for Setting {
    fn drop(&mut self) {
        unsafe {
            g_object_unref(self.0);
        }
    }
}
pub(super) fn read() -> Result<Config, &'static str> {
    Ok(Config {
        values: (0..KEYS.len())
            .map(|i| Setting::open(i)?.read())
            .collect::<Result<_, _>>()?,
    })
}
pub(super) fn desired(port: u16) -> Result<Config, &'static str> {
    // 'ignore-hosts' is not changed: it can contain administrator policy.
    // Existing bypasses are disclosed by UI; system proxy is never system VPN.
    Ok(Config {
        values: vec![
            "'127.0.0.1'".into(),
            port.to_string(),
            "false".into(),
            "''".into(),
            "''".into(),
            "'127.0.0.1'".into(),
            port.to_string(),
            "''".into(),
            "0".into(),
            "'127.0.0.1'".into(),
            port.to_string(),
            "'manual'".into(),
        ],
    })
}
pub(super) fn write(config: &Config) -> Result<(), &'static str> {
    if config.values.len() != KEYS.len() {
        return Err(FAILED);
    }
    let settings = (0..KEYS.len())
        .map(Setting::open)
        .collect::<Result<Vec<_>, _>>()?;
    if settings.iter().any(|s| !s.writable()) {
        return Err(FAILED);
    }
    for (s, value) in settings.iter().zip(&config.values) {
        s.write(value)?;
    }
    unsafe {
        g_settings_sync();
    }
    Ok(())
}
pub(super) fn restore(before: &Config, installed: &Config) -> Result<(), &'static str> {
    if before.values.len() != KEYS.len() || installed.values.len() != KEYS.len() {
        return Err(FAILED);
    }
    // Also handles a crash partway through applying the settings. Each key is
    // restored only while it still equals the value assigned by Moth.
    for i in (0..KEYS.len()).rev() {
        let setting = Setting::open(i)?;
        if before.values[i] != installed.values[i] && setting.read()? == installed.values[i] {
            setting.write(&before.values[i])?;
        }
    }
    unsafe {
        g_settings_sync();
    }
    Ok(())
}
