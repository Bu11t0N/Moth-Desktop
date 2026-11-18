use crate::{Profile, Protocol, Tls, Transport};
use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};
use percent_encoding::percent_decode_str;
use std::collections::BTreeMap;
use url::Url;
use uuid::Uuid;

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("Строка {line}: {reason}")]
pub struct ImportError {
    pub line: usize,
    pub reason: &'static str,
}

pub fn import_links(input: &str) -> Result<Vec<Profile>, ImportError> {
    let fail = |reason| ImportError { line: 0, reason };
    if input.len() > 2 * 1024 * 1024 {
        return Err(fail("Список больше 2 МБ"));
    }
    let input = input.trim().trim_start_matches('\u{feff}').trim();
    if input.starts_with('{')
        || input.starts_with('[')
        || input.starts_with("proxies:")
        || input.contains("\nproxies:")
        || input.contains("proxy-providers:")
    {
        return crate::formats::parse(input).map_err(fail);
    }
    let decoded;
    let text = if !input.contains("://") {
        let compact: String = input.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        let bytes = [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD]
            .iter()
            .find_map(|e| e.decode(&compact).ok())
            .ok_or_else(|| fail("Ожидается ссылка сервера или Base64-список"))?;
        decoded = String::from_utf8(bytes).map_err(|_| fail("Список должен быть в UTF-8"))?;
        if decoded.trim_start().starts_with('{')
            || decoded.trim_start().starts_with('[')
            || decoded.contains("proxies:")
        {
            return crate::formats::parse(&decoded).map_err(fail);
        }
        decoded.as_str()
    } else {
        input
    };
    let mut result = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if result.len() >= 1000 {
            return Err(ImportError {
                line: n + 1,
                reason: "Не более 1000 серверов за один импорт",
            });
        }
        result.push(parse(line).map_err(|reason| ImportError {
            line: n + 1,
            reason,
        })?);
    }
    if result.is_empty() {
        return Err(fail("В списке нет серверов"));
    }
    Ok(result)
}

fn decode(text: &str) -> Result<String, &'static str> {
    percent_decode_str(text)
        .decode_utf8()
        .map(|s| s.into_owned())
        .map_err(|_| "Неверная кодировка ссылки")
}

pub(crate) fn parse(line: &str) -> Result<Profile, &'static str> {
    if line.len() > 16384 {
        return Err("Ссылка слишком длинная");
    }
    if line.chars().any(char::is_control) {
        return Err("Ссылка содержит управляющие символы");
    }
    let bytes = line.as_bytes();
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return Err("Некорректное percent-кодирование");
        }
    }
    if ["hy2://", "hysteria2://", "tuic://", "anytls://"]
        .iter()
        .any(|prefix| line.starts_with(prefix))
    {
        return crate::native_protocols::parse(line);
    }
    if line.starts_with("vmess://")
        || line.starts_with("ss://")
        || line.starts_with("socks://")
        || line.starts_with("socks5://")
        || line.starts_with("socks5h://")
        || line.starts_with("http://")
        || line.starts_with("https://")
    {
        return crate::extra_protocols::parse(line);
    }
    let url = Url::parse(line).map_err(|_| "Некорректная ссылка")?;
    let protocol =
        match url.scheme() {
            "vless" => Protocol::Vless,
            "trojan" => Protocol::Trojan,
            "https" | "http" => return Err(
                "Загрузка подписок по URL появится на следующем этапе; вставьте ссылки серверов",
            ),
            _ => return Err("На этом этапе поддерживаются VLESS и Trojan"),
        };
    let host = url
        .host_str()
        .filter(|s| !s.is_empty())
        .ok_or("Не указан сервер")?
        .trim_matches(['[', ']'])
        .to_lowercase();
    let port = url
        .port()
        .filter(|p| *p > 0)
        .ok_or("Укажите порт сервера")?;
    if !url.path().is_empty() && url.path() != "/" {
        return Err("Путь транспорта задаётся параметром path");
    }
    if url.password().is_some() {
        return Err("Некорректные данные доступа");
    }
    let credential = decode(url.username())?;
    if credential.is_empty() {
        return Err("Не указаны данные доступа");
    }
    if protocol == Protocol::Vless {
        Uuid::parse_str(&credential).map_err(|_| "Некорректный UUID VLESS")?;
    }
    let mut q = BTreeMap::new();
    for (k, v) in url.query_pairs() {
        if v.chars().any(char::is_control) || v.contains('\u{fffd}') {
            return Err("Некорректная кодировка параметров");
        }
        if q.insert(k.into_owned(), v.into_owned()).is_some() {
            return Err("Повторяющийся параметр ссылки");
        }
    }
    let allowed = [
        "type",
        "security",
        "encryption",
        "sni",
        "peer",
        "fp",
        "pbk",
        "sid",
        "flow",
        "path",
        "host",
        "serviceName",
        "mode",
        "alpn",
        "allowInsecure",
        "insecure",
        "headerType",
    ];
    if q.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("Ссылка содержит пока неподдерживаемые параметры");
    }
    let get = |k: &str| q.get(k).map(String::as_str).unwrap_or("");
    if ["allowInsecure", "insecure"]
        .iter()
        .any(|k| !matches!(get(k), "" | "0" | "false"))
    {
        return Err("Отключение проверки сертификата не поддерживается");
    }
    if !matches!(get("encryption"), "" | "none") || !matches!(get("headerType"), "" | "none") {
        return Err("Этот вариант шифрования или транспорта пока не поддерживается");
    }
    let path = if get("path").is_empty() {
        "/"
    } else {
        get("path")
    }
    .to_owned();
    let transport = match get("type") {
        "" | "tcp" | "raw" => Transport::Tcp,
        "ws" => Transport::Ws {
            path,
            host: q.get("host").cloned(),
        },
        "grpc" => Transport::Grpc {
            service_name: get("serviceName").into(),
        },
        "xhttp" => {
            let mode = if get("mode").is_empty() {
                "auto"
            } else {
                get("mode")
            };
            if !matches!(mode, "auto" | "packet-up" | "stream-up" | "stream-one") {
                return Err("Неизвестный режим XHTTP");
            }
            Transport::Xhttp {
                path,
                host: q.get("host").cloned(),
                mode: mode.into(),
            }
        }
        _ => return Err("Этот транспорт пока не поддерживается"),
    };
    let server_name = q
        .get("sni")
        .or_else(|| q.get("peer"))
        .cloned()
        .unwrap_or_else(|| host.clone());
    let security = if get("security").is_empty() && protocol == Protocol::Trojan {
        "tls"
    } else {
        get("security")
    };
    let tls = match security {
        "" | "none" => Tls::None,
        "tls" => Tls::Tls {
            server_name,
            fingerprint: q.get("fp").cloned(),
            alpn: get("alpn")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        },
        "reality" => {
            if server_name.is_empty() || get("sni").is_empty() {
                return Err("Reality требует sni");
            }
            if URL_SAFE_NO_PAD.decode(get("pbk")).map(|b| b.len()).ok() != Some(32) {
                return Err("Некорректный открытый ключ Reality");
            }
            let sid = get("sid");
            if sid.len() > 16 || sid.len() % 2 != 0 || !sid.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("Некорректный short ID Reality");
            }
            Tls::Reality {
                server_name,
                fingerprint: if get("fp").is_empty() {
                    "chrome".into()
                } else {
                    get("fp").into()
                },
                public_key: get("pbk").into(),
                short_id: sid.into(),
            }
        }
        _ => return Err("Этот режим защиты пока не поддерживается"),
    };
    let unused: &[&str] = match transport {
        Transport::Tcp => &["path", "host", "serviceName", "mode"],
        Transport::Ws { .. } => &["serviceName", "mode"],
        Transport::Grpc { .. } => &["path", "host", "mode"],
        Transport::Xhttp { .. } => &["serviceName"],
    };
    if unused.iter().any(|k| !get(k).is_empty()) {
        return Err("Параметры не соответствуют выбранному транспорту");
    }
    let unused_tls: &[&str] = match tls {
        Tls::None => &["sni", "peer", "fp", "alpn", "pbk", "sid"],
        Tls::Tls { .. } => &["pbk", "sid"],
        Tls::Reality { .. } => &["alpn"],
    };
    if unused_tls.iter().any(|k| !get(k).is_empty())
        || (!get("sni").is_empty() && !get("peer").is_empty() && get("sni") != get("peer"))
    {
        return Err("Параметры не соответствуют выбранной защите");
    }
    if protocol == Protocol::Trojan && !matches!(tls, Tls::Tls { .. }) {
        return Err("Trojan требует TLS");
    }
    if protocol == Protocol::Vless && matches!(tls, Tls::None) {
        let private = host == "localhost"
            || host.parse::<std::net::IpAddr>().is_ok_and(|ip| match ip {
                std::net::IpAddr::V4(ip) => {
                    ip.is_private() || ip.is_loopback() || ip.is_link_local()
                }
                std::net::IpAddr::V6(ip) => {
                    ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
                }
            });
        if !private {
            return Err("Эта версия Xray требует TLS или Reality для публичного сервера VLESS");
        }
    }
    let flow = q.get("flow").filter(|s| !s.is_empty()).cloned();
    if let Some(flow) = &flow {
        if protocol != Protocol::Vless
            || flow != "xtls-rprx-vision"
            || !matches!(transport, Transport::Tcp)
            || matches!(tls, Tls::None)
        {
            return Err("Этот flow или его сочетание с транспортом пока не поддерживается");
        }
    }
    let name = decode(url.fragment().unwrap_or(""))?;
    let name = if name.is_empty() { host.clone() } else { name };
    if name.chars().count() > 160 || name.chars().any(char::is_control) {
        return Err("Название слишком длинное или содержит управляющие символы");
    }
    Ok(Profile {
        id: Uuid::new_v4(),
        name,
        host,
        port,
        protocol,
        credential,
        tls,
        transport,
        flow,
        options: Default::default(),
    })
}
