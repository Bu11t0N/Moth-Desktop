//! Typed URI adapters. Unsupported security/transport parameters fail atomically.
use crate::{Profile, Protocol, ProxyOptions, Tls, Transport};
use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};
use url::Url;
use uuid::Uuid;
pub(crate) fn unbase64(text: &str) -> Result<String, &'static str> {
    let bytes = [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD]
        .iter()
        .find_map(|e| e.decode(text).ok())
        .ok_or("Неверный Base64")?;
    String::from_utf8(bytes).map_err(|_| "Ожидается UTF-8")
}
fn dec(text: &str) -> Result<String, &'static str> {
    percent_encoding::percent_decode_str(text)
        .decode_utf8()
        .map(|s| s.into_owned())
        .map_err(|_| "Неверная кодировка ссылки")
}
fn name(text: &str, host: &str) -> Result<String, &'static str> {
    let s = dec(text)?;
    let s = if s.is_empty() { host.into() } else { s };
    if s.chars().any(char::is_control) || s.chars().count() > 160 {
        return Err("Некорректное имя сервера");
    }
    Ok(s)
}
fn endpoint(url: &Url) -> Result<(String, u16), &'static str> {
    let host = url
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or("Нет адреса сервера")?
        .trim_matches(['[', ']'])
        .to_lowercase();
    let port = url
        .port_or_known_default()
        .filter(|p| *p > 0)
        .ok_or("Нет порта сервера")?;
    Ok((host, port))
}
pub(crate) fn parse(text: &str) -> Result<Profile, &'static str> {
    if let Some(encoded) = text.strip_prefix("vmess://") {
        return vmess(&unbase64(encoded)?);
    }
    let mut text = text.to_owned();
    if text.starts_with("ss://") && !text.split('#').next().unwrap_or("").contains('@') {
        let raw = text
            .trim_start_matches("ss://")
            .split('#')
            .next()
            .unwrap_or("");
        let decoded = unbase64(raw)?;
        text = format!(
            "ss://{decoded}#{}",
            text.split_once('#').map(|(_, v)| v).unwrap_or("")
        );
    }
    let url = Url::parse(&text).map_err(|_| "Некорректная ссылка")?;
    if url.path() != "" && url.path() != "/" {
        return Err("Путь в ссылке прокси не поддерживается");
    }
    let mut query = std::collections::BTreeMap::new();
    for (k, v) in url.query_pairs() {
        if url.scheme() != "https"
            || !["sni", "fp", "alpn"].contains(&k.as_ref())
            || query.insert(k.into_owned(), v.into_owned()).is_some()
        {
            return Err("Параметры этого прокси, включая плагины, не поддерживаются");
        }
    }
    let (host, port) = endpoint(&url)?;
    let title = name(url.fragment().unwrap_or(""), &host)?;
    let mut result = Profile {
        id: Uuid::new_v4(),
        name: title,
        host: host.clone(),
        port,
        protocol: Protocol::Socks,
        credential: String::new(),
        tls: Tls::None,
        transport: Transport::Tcp,
        flow: None,
        options: ProxyOptions::default(),
    };
    match url.scheme() {
        "ss" => {
            let (method, password) = if let Some(password) = url.password() {
                (dec(url.username())?, dec(password)?)
            } else {
                let user = unbase64(&dec(url.username())?)?;
                let (m, p) = user
                    .split_once(':')
                    .ok_or("Shadowsocks: нет метода или пароля")?;
                (m.into(), p.into())
            };
            if ![
                "aes-128-gcm",
                "aes-256-gcm",
                "chacha20-ietf-poly1305",
                "chacha20-poly1305",
                "2022-blake3-aes-128-gcm",
                "2022-blake3-aes-256-gcm",
                "2022-blake3-chacha20-poly1305",
            ]
            .contains(&method.as_str())
            {
                return Err("Этот метод Shadowsocks не поддерживается");
            }
            if password.is_empty()
                || password.len() > 4096
                || password.chars().any(char::is_control)
            {
                return Err("Некорректный пароль Shadowsocks");
            }
            result.protocol = Protocol::Shadowsocks;
            result.credential = password;
            result.options.method = method;
        }
        "socks" | "socks5" | "socks5h" | "http" | "https" => {
            result.options.username = dec(url.username())?;
            result.credential = dec(url.password().unwrap_or(""))?;
            if result.options.username.chars().any(char::is_control)
                || result.credential.chars().any(char::is_control)
                || result.options.username.len() > 255
                || result.credential.len() > 4096
                || (!result.credential.is_empty() && result.options.username.is_empty())
            {
                return Err("Некорректная авторизация прокси");
            }
            if matches!(url.scheme(), "http" | "https") {
                result.protocol = Protocol::Http;
            }
            if url.scheme() == "https" {
                result.tls = Tls::Tls {
                    server_name: query
                        .get("sni")
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .unwrap_or(host),
                    fingerprint: query.get("fp").cloned(),
                    alpn: query
                        .get("alpn")
                        .map(|s| {
                            s.split(',')
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default(),
                };
            }
        }
        _ => return Err("Протокол не поддерживается"),
    }
    Ok(result)
}

pub(crate) fn vmess(text: &str) -> Result<Profile, &'static str> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "Некорректный VMess JSON")?;
    let obj = value.as_object().ok_or("Ожидается VMess JSON")?;
    let allowed = [
        "v", "ps", "add", "port", "id", "aid", "scy", "net", "type", "host", "path", "tls", "sni",
        "alpn", "fp",
    ];
    if obj.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("VMess содержит неподдерживаемые параметры");
    }
    let v = |k: &str| -> String {
        obj.get(k)
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
            })
            .unwrap_or_default()
    };
    if !matches!(v("aid").as_str(), "" | "0") || !matches!(v("type").as_str(), "" | "none") {
        return Err("VMess alterId и маскировка TCP не поддерживаются");
    }
    let method = if v("scy").is_empty() {
        "auto".into()
    } else {
        v("scy")
    };
    if !["auto", "aes-128-gcm", "chacha20-poly1305", "none", "zero"].contains(&method.as_str()) {
        return Err("Шифрование VMess не поддерживается");
    }
    let credential = Uuid::parse_str(&v("id"))
        .map_err(|_| "VMess требует корректный UUID")?
        .to_string();
    let host = v("add");
    let port = v("port")
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .ok_or("Некорректный порт VMess")?;
    let address = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.clone()
    };
    let u =
        Url::parse(&format!("vmess://{address}:{port}")).map_err(|_| "Некорректный адрес VMess")?;
    let (host, port) = endpoint(&u)?;
    let network = v("net");
    let transport = match network.as_str() {
        "" | "tcp" | "raw" => {
            if !v("path").is_empty() || !v("host").is_empty() {
                return Err("VMess TCP содержит параметры другого транспорта");
            }
            Transport::Tcp
        }
        "ws" => Transport::Ws {
            path: if v("path").is_empty() {
                "/".into()
            } else {
                v("path")
            },
            host: (!v("host").is_empty()).then(|| v("host")),
        },
        "grpc" => {
            if !v("host").is_empty() {
                return Err("Неподдерживаемый gRPC host");
            }
            Transport::Grpc {
                service_name: v("path"),
            }
        }
        _ => return Err("Этот транспорт VMess не поддерживается"),
    };
    let tls = match v("tls").as_str() {
        "" | "none" => {
            if ["sni", "alpn", "fp"].iter().any(|k| !v(k).is_empty()) {
                return Err("Параметры TLS без TLS");
            }
            Tls::None
        }
        "tls" => Tls::Tls {
            server_name: if v("sni").is_empty() {
                host.clone()
            } else {
                v("sni")
            },
            fingerprint: (!v("fp").is_empty()).then(|| v("fp")),
            alpn: v("alpn")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        },
        _ => return Err("Этот вариант TLS VMess не поддерживается"),
    };
    if text.chars().any(|c| c == '\u{0000}') {
        return Err("Некорректные параметры VMess");
    }
    Ok(Profile {
        id: Uuid::new_v4(),
        name: name(&v("ps"), &host)?,
        host,
        port,
        protocol: Protocol::Vmess,
        credential,
        tls,
        transport,
        flow: None,
        options: ProxyOptions {
            method,
            username: String::new(),
        },
    })
}

pub(crate) fn share(profile: &Profile) -> Result<String, &'static str> {
    if profile.flow.is_some() {
        return Err("Flow не поддерживается этим протоколом");
    }
    if profile.protocol == Protocol::Shadowsocks
        && (!matches!(profile.tls, Tls::None)
            || !matches!(profile.transport, Transport::Tcp)
            || !profile.options.username.is_empty())
    {
        return Err("Плагин или транспорт Shadowsocks не поддерживается");
    }
    if matches!(profile.protocol, Protocol::Socks | Protocol::Http)
        && (!matches!(profile.transport, Transport::Tcp) || !profile.options.method.is_empty())
    {
        return Err("Этот транспорт прокси не поддерживается");
    }
    if profile.protocol == Protocol::Socks && !matches!(profile.tls, Tls::None) {
        return Err("SOCKS5 поверх TLS не поддерживается");
    }
    if profile.protocol == Protocol::Http && matches!(profile.tls, Tls::Reality { .. }) {
        return Err("Reality не поддерживается HTTP-прокси");
    }
    if profile.protocol == Protocol::Vmess && !profile.options.username.is_empty() {
        return Err("VMess содержит параметры другого протокола");
    }
    if profile.protocol == Protocol::Vmess {
        let mut v = serde_json::json!({"v":"2","ps":profile.name,"add":profile.host,"port":profile.port,"id":profile.credential,"aid":0,"scy":profile.options.method});
        match &profile.transport {
            Transport::Tcp => v["net"] = "tcp".into(),
            Transport::Ws { path, host } => {
                v["net"] = "ws".into();
                v["path"] = path.clone().into();
                if let Some(host) = host {
                    v["host"] = host.clone().into();
                }
            }
            Transport::Grpc { service_name } => {
                v["net"] = "grpc".into();
                v["path"] = service_name.clone().into();
            }
            _ => return Err("Транспорт VMess не поддерживается"),
        }
        match &profile.tls {
            Tls::None => {}
            Tls::Tls {
                server_name,
                fingerprint,
                alpn,
            } => {
                v["tls"] = "tls".into();
                v["sni"] = server_name.clone().into();
                if let Some(fp) = fingerprint {
                    v["fp"] = fp.clone().into();
                }
                if !alpn.is_empty() {
                    v["alpn"] = alpn.join(",").into();
                }
            }
            _ => return Err("VMess Reality не поддерживается"),
        }
        let link = format!("vmess://{}", STANDARD.encode(v.to_string()));
        parse(&link)?;
        return Ok(link);
    }
    let scheme = match profile.protocol {
        Protocol::Shadowsocks => "ss",
        Protocol::Socks => "socks5",
        Protocol::Http => {
            if matches!(profile.tls, Tls::None) {
                "http"
            } else {
                "https"
            }
        }
        _ => return Err("Протокол не поддерживается"),
    };
    let mut url =
        Url::parse(&format!("{scheme}://placeholder:443")).map_err(|_| "Некорректный профиль")?;
    let host = if profile.host.contains(':') {
        format!("[{}]", profile.host)
    } else {
        profile.host.clone()
    };
    url.set_host(Some(&host))
        .map_err(|_| "Некорректный сервер")?;
    url.set_port(Some(profile.port))
        .map_err(|_| "Некорректный порт")?;
    if profile.protocol == Protocol::Shadowsocks {
        url.set_username(
            &STANDARD_NO_PAD.encode(format!("{}:{}", profile.options.method, profile.credential)),
        )
        .map_err(|_| "Некорректный профиль")?;
    } else if !profile.options.username.is_empty() {
        url.set_username(&profile.options.username)
            .map_err(|_| "Некорректный логин")?;
        url.set_password(Some(&profile.credential))
            .map_err(|_| "Некорректный пароль")?;
    }
    url.set_fragment(Some(&profile.name));
    if profile.protocol == Protocol::Http {
        if let Tls::Tls {
            server_name,
            fingerprint,
            alpn,
        } = &profile.tls
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("sni", server_name);
            if let Some(fp) = fingerprint {
                q.append_pair("fp", fp);
            }
            if !alpn.is_empty() {
                q.append_pair("alpn", &alpn.join(","));
            }
        }
    }
    let link = url.to_string();
    parse(&link)?;
    Ok(link)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extra_links_roundtrip_without_losing_auth_or_transport() {
        let vm=STANDARD.encode(r#"{"v":"2","ps":"VM","add":"example.com","port":"443","id":"00000000-0000-4000-8000-000000000001","aid":"0","net":"ws","path":"/vpn","tls":"tls","sni":"example.com"}"#);
        for link in [
            format!("vmess://{vm}"),
            format!(
                "ss://{}@example.com:443#SS",
                STANDARD_NO_PAD.encode("aes-256-gcm:synthetic")
            ),
            "socks5://user:synthetic@example.com:1080#Proxy".into(),
            "https://user:synthetic@example.com:443#HTTP".into(),
        ] {
            let p = parse(&link).unwrap();
            let q = parse(&share(&p).unwrap()).unwrap();
            assert!(p.same_server(&q));
        }
    }
    #[test]
    fn unsupported_settings_fail_explicitly() {
        assert!(parse("ss://YWVzLTI1Ni1nY206c3ludGhldGlj@example.com:443?plugin=obfs").is_err());
        let vm=STANDARD.encode(r#"{"add":"example.com","port":443,"id":"00000000-0000-4000-8000-000000000001","allowInsecure":true}"#);
        assert!(parse(&format!("vmess://{vm}")).is_err());
    }
}
