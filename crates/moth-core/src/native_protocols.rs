//! Typed sing-box families. Android URI conventions were reviewed against
//! Bu11t0N/Moth ExtraProtocols.kt / ProxyProtocols.kt, GPL-3.0, b529c62.
//! No raw outbound or user-selected engine configuration crosses the service.
use crate::{Profile, Protocol, ProxyOptions, Tls, Transport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use url::Url;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum NativeProtocolOptions {
    Hysteria2 {
        obfs_password: Option<String>,
    },
    Tuic {
        uuid: Uuid,
        congestion_control: String,
        udp_relay_mode: String,
        disable_sni: bool,
    },
    AnyTls {},
}
impl Protocol {
    pub fn uses_sing_box(&self) -> bool {
        matches!(self, Self::Hysteria2 | Self::Tuic | Self::AnyTls)
    }
}
fn decode(text: &str) -> Result<String, &'static str> {
    percent_encoding::percent_decode_str(text)
        .decode_utf8()
        .map(|s| s.into_owned())
        .map_err(|_| "Некорректная кодировка ссылки")
}
fn secret(text: &str) -> bool {
    !text.is_empty() && text.len() <= 4096 && !text.chars().any(char::is_control)
}
fn query(url: &Url) -> Result<BTreeMap<String, String>, &'static str> {
    let mut query = BTreeMap::new();
    for item in url
        .query()
        .unwrap_or("")
        .split('&')
        .filter(|s| !s.is_empty())
    {
        let (key, value) = item.split_once('=').ok_or("Параметр ссылки без значения")?;
        let (key, value) = (decode(key)?, decode(value)?);
        if query.insert(key, value).is_some() {
            return Err("Повторяющийся параметр ссылки");
        }
    }
    Ok(query)
}
pub(crate) fn parse(text: &str) -> Result<Profile, &'static str> {
    let url = Url::parse(text).map_err(|_| "Некорректная ссылка")?;
    let protocol = match url.scheme() {
        "hy2" | "hysteria2" => Protocol::Hysteria2,
        "tuic" => Protocol::Tuic,
        "anytls" => Protocol::AnyTls,
        _ => return Err("Протокол не поддерживается"),
    };
    if !matches!(url.path(), "" | "/") {
        return Err("Путь в ссылке этого протокола не поддерживается");
    }
    let host = url
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or("Нет адреса сервера")?
        .trim_matches(['[', ']'])
        .to_lowercase();
    let port = url.port().unwrap_or(443);
    if port == 0 {
        return Err("Некорректный порт сервера");
    }
    let q = query(&url)?;
    let allowed = match protocol {
        Protocol::Hysteria2 => vec![
            "sni",
            "peer",
            "alpn",
            "insecure",
            "allowInsecure",
            "obfs",
            "obfs-password",
        ],
        Protocol::Tuic => vec![
            "sni",
            "peer",
            "alpn",
            "insecure",
            "allowInsecure",
            "congestion_control",
            "udp_relay_mode",
            "disable_sni",
        ],
        _ => vec![
            "sni",
            "peer",
            "alpn",
            "fp",
            "insecure",
            "allowInsecure",
            "allowinsecure",
        ],
    };
    if q.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("Неподдерживаемые параметры протокола; импорт отменён без потери настроек");
    }
    for key in ["insecure", "allowInsecure", "allowinsecure"] {
        if q.get(key)
            .is_some_and(|v| !matches!(v.as_str(), "0" | "false"))
        {
            return Err("Отключение проверки TLS запрещено");
        }
    }
    if q.contains_key("sni") && q.contains_key("peer") {
        return Err("Укажите только один параметр SNI");
    }
    let sni = q
        .get("sni")
        .or_else(|| q.get("peer"))
        .cloned()
        .unwrap_or_else(|| host.clone());
    if sni.is_empty() || sni.len() > 253 || sni.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err("Некорректный SNI");
    }
    let alpn = q
        .get("alpn")
        .map(|v| v.split(',').map(str::to_owned).collect::<Vec<_>>())
        .unwrap_or_default();
    if alpn.len() > 8
        || alpn
            .iter()
            .any(|a| a.is_empty() || a.len() > 255 || a.chars().any(char::is_control))
    {
        return Err("Некорректный ALPN");
    }
    let fingerprint = q.get("fp").cloned();
    if fingerprint.as_ref().is_some_and(|fp| {
        ![
            "chrome",
            "firefox",
            "safari",
            "ios",
            "android",
            "edge",
            "random",
            "randomized",
        ]
        .contains(&fp.as_str())
    }) {
        return Err("Неподдерживаемый отпечаток TLS");
    }
    let mut credential = decode(url.username())?;
    if let Some(password) = url.password() {
        credential.push(':');
        credential.push_str(&decode(password)?);
    }
    let options = match protocol {
        Protocol::Hysteria2 => {
            let obfs_password = if let Some(obfs) = q.get("obfs") {
                if obfs != "salamander" {
                    return Err("Поддерживается только Hysteria2 salamander");
                }
                Some(
                    q.get("obfs-password")
                        .filter(|s| secret(s))
                        .cloned()
                        .ok_or("Нет пароля salamander")?,
                )
            } else {
                if q.contains_key("obfs-password") {
                    return Err("Пароль обфускации без salamander");
                }
                None
            };
            NativeProtocolOptions::Hysteria2 { obfs_password }
        }
        Protocol::Tuic => {
            let (id, password) = credential
                .split_once(':')
                .ok_or("TUIC v5 требует UUID и пароль")?;
            let uuid = Uuid::parse_str(id).map_err(|_| "Некорректный UUID TUIC")?;
            if uuid.is_nil() {
                return Err("Некорректный UUID TUIC");
            }
            let password = password.to_owned();
            let congestion_control = q
                .get("congestion_control")
                .cloned()
                .unwrap_or("cubic".into());
            let udp_relay_mode = q.get("udp_relay_mode").cloned().unwrap_or("native".into());
            if !["cubic", "new_reno", "bbr"].contains(&congestion_control.as_str())
                || !["native", "quic"].contains(&udp_relay_mode.as_str())
            {
                return Err("Неподдерживаемая настройка TUIC v5");
            }
            let disable_sni = match q.get("disable_sni").map(String::as_str) {
                None | Some("false" | "0") => false,
                Some("true" | "1") => true,
                _ => return Err("Некорректный disable_sni"),
            };
            credential = password;
            NativeProtocolOptions::Tuic {
                uuid,
                congestion_control,
                udp_relay_mode,
                disable_sni,
            }
        }
        _ => NativeProtocolOptions::AnyTls {},
    };
    if !secret(&credential) {
        return Err("Некорректный пароль сервера");
    }
    let name = decode(url.fragment().unwrap_or(""))?;
    let name = if name.is_empty() { host.clone() } else { name };
    if name.chars().count() > 160 || name.chars().any(char::is_control) {
        return Err("Некорректное имя сервера");
    }
    Ok(Profile {
        id: Uuid::new_v4(),
        name,
        host,
        port,
        protocol,
        credential,
        tls: Tls::Tls {
            server_name: sni,
            fingerprint,
            alpn,
        },
        transport: Transport::Tcp,
        flow: None,
        options: ProxyOptions {
            native: Some(options),
            ..Default::default()
        },
    })
}
pub(crate) fn share(profile: &Profile) -> Result<String, &'static str> {
    if !matches!(profile.transport, Transport::Tcp)
        || profile.flow.is_some()
        || !profile.options.username.is_empty()
        || !profile.options.method.is_empty()
    {
        return Err("Параметры другого протокола не переносятся");
    }
    let Tls::Tls {
        server_name,
        fingerprint,
        alpn,
    } = &profile.tls
    else {
        return Err("Этот протокол требует TLS с проверкой сертификата");
    };
    let scheme = match profile.protocol {
        Protocol::Hysteria2 => "hysteria2",
        Protocol::Tuic => "tuic",
        Protocol::AnyTls => "anytls",
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
    let credentials = match &profile.options.native {
        Some(NativeProtocolOptions::Tuic { uuid, .. }) => format!("{uuid}:{}", profile.credential),
        _ => profile.credential.clone(),
    };
    url.set_username(&credentials)
        .map_err(|_| "Некорректные данные доступа")?;
    url.set_fragment(Some(&profile.name));
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("sni", server_name);
        if !alpn.is_empty() {
            q.append_pair("alpn", &alpn.join(","));
        }
        if let Some(fp) = fingerprint {
            if profile.protocol != Protocol::AnyTls {
                return Err("uTLS не поддерживается QUIC протоколом");
            }
            q.append_pair("fp", fp);
        }
        match (&profile.protocol, &profile.options.native) {
            (Protocol::Hysteria2, Some(NativeProtocolOptions::Hysteria2 { obfs_password })) => {
                if let Some(password) = obfs_password {
                    q.append_pair("obfs", "salamander")
                        .append_pair("obfs-password", password);
                }
            }
            (
                Protocol::Tuic,
                Some(NativeProtocolOptions::Tuic {
                    congestion_control,
                    udp_relay_mode,
                    disable_sni,
                    ..
                }),
            ) => {
                q.append_pair("congestion_control", congestion_control)
                    .append_pair("udp_relay_mode", udp_relay_mode)
                    .append_pair("disable_sni", if *disable_sni { "true" } else { "false" });
            }
            (Protocol::AnyTls, Some(NativeProtocolOptions::AnyTls {})) => {}
            _ => return Err("Параметры не соответствуют протоколу"),
        }
    }
    let result = url.to_string();
    let parsed = parse(&result)?;
    if !profile.same_server(&parsed) {
        return Err("Параметры профиля нельзя перенести без потери");
    }
    Ok(result)
}
pub fn sing_box_native_config(profile: &Profile) -> Result<Value, &'static str> {
    share(profile)?;
    let Tls::Tls {
        server_name,
        fingerprint,
        alpn,
    } = &profile.tls
    else {
        return Err("TLS обязателен");
    };
    let mut tls = json!({"enabled":true,"server_name":server_name});
    if !alpn.is_empty() {
        tls["alpn"] = json!(alpn);
    }
    if let Some(fp) = fingerprint {
        tls["utls"] = json!({"enabled":true,"fingerprint":fp});
    }
    let mut outbound = json!({"type":match profile.protocol{Protocol::Hysteria2=>"hysteria2",Protocol::Tuic=>"tuic",_=>"anytls"},"tag":"proxy","server":profile.host,"server_port":profile.port,"password":profile.credential,"tls":tls});
    match profile
        .options
        .native
        .as_ref()
        .ok_or("Нет параметров протокола")?
    {
        NativeProtocolOptions::Hysteria2 { obfs_password } => {
            if let Some(password) = obfs_password {
                outbound["obfs"] = json!({"type":"salamander","password":password});
            }
        }
        NativeProtocolOptions::Tuic {
            uuid,
            congestion_control,
            udp_relay_mode,
            disable_sni,
        } => {
            outbound["uuid"] = json!(uuid);
            outbound["congestion_control"] = json!(congestion_control);
            outbound["udp_relay_mode"] = json!(udp_relay_mode);
            outbound["tls"]["disable_sni"] = json!(disable_sni);
        }
        NativeProtocolOptions::AnyTls {} => {}
    }
    Ok(json!({"log":{"disabled":true},"outbounds":[outbound],"route":{"final":"proxy"}}))
}

pub(crate) fn document(v: &Value, clash: bool) -> Result<Profile, &'static str> {
    let field = |key: &str| -> Result<String, &'static str> {
        match v.get(key) {
            None => Ok(String::new()),
            Some(Value::String(s)) => Ok(s.clone()),
            _ => Err("Ожидается строковый параметр протокола"),
        }
    };
    let kind = field("type")?;
    let scheme = match kind.as_str() {
        "hy2" | "hysteria2" => "hysteria2",
        "tuic" => "tuic",
        "anytls" => "anytls",
        _ => return Err("Протокол не поддерживается"),
    };
    let mut allowed = if clash {
        vec![
            "name",
            "type",
            "server",
            "port",
            "password",
            "sni",
            "servername",
            "alpn",
            "skip-cert-verify",
            "client-fingerprint",
            "udp",
        ]
    } else {
        vec!["tag", "type", "server", "server_port", "password", "tls"]
    };
    match scheme {
        "hysteria2" => {
            if clash {
                allowed.extend(["obfs", "obfs-password"]);
            } else {
                allowed.push("obfs");
            }
        }
        "tuic" => {
            if clash {
                allowed.extend([
                    "uuid",
                    "congestion-controller",
                    "udp-relay-mode",
                    "disable-sni",
                ]);
            } else {
                allowed.extend(["uuid", "congestion_control", "udp_relay_mode"]);
            }
        }
        _ => {}
    }
    let object = v.as_object().ok_or("Ожидается объект протокола")?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("Неподдерживаемые параметры протокола; импорт отменён без потери настроек");
    }
    let mut url =
        Url::parse(&format!("{scheme}://placeholder:443")).map_err(|_| "Некорректный профиль")?;
    let host = field("server")?;
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host
    };
    url.set_host(Some(&host))
        .map_err(|_| "Некорректный сервер")?;
    let key = if clash { "port" } else { "server_port" };
    let port = v
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        })
        .and_then(|n| u16::try_from(n).ok())
        .filter(|p| *p > 0)
        .ok_or("Некорректный порт")?;
    url.set_port(Some(port)).map_err(|_| "Некорректный порт")?;
    let credential = if scheme == "tuic" {
        format!("{}:{}", field("uuid")?, field("password")?)
    } else {
        field("password")?
    };
    url.set_username(&credential)
        .map_err(|_| "Некорректные данные доступа")?;
    url.set_fragment(Some(&field(if clash { "name" } else { "tag" })?));
    let mut q = Vec::<(String, String)>::new();
    let mut add = |key: &str, value: String| {
        if !value.is_empty() {
            q.push((key.into(), value));
        }
    };
    if clash {
        if v.get("skip-cert-verify").is_some_and(|v| v != false)
            || v.get("udp").is_some_and(|v| v != true)
        {
            return Err("Небезопасный TLS или неподдерживаемое ограничение UDP");
        }
        let sni = field("sni")?;
        let other = field("servername")?;
        if !sni.is_empty() && !other.is_empty() {
            return Err("Укажите только один параметр SNI");
        }
        add("sni", if sni.is_empty() { other } else { sni });
        add("fp", field("client-fingerprint")?);
    } else {
        let tls = v
            .get("tls")
            .and_then(Value::as_object)
            .ok_or("Протокол требует TLS");
        let tls = tls?;
        let allowed = if scheme == "tuic" {
            vec!["enabled", "server_name", "alpn", "insecure", "disable_sni"]
        } else if scheme == "anytls" {
            vec!["enabled", "server_name", "alpn", "insecure", "utls"]
        } else {
            vec!["enabled", "server_name", "alpn", "insecure"]
        };
        if tls.keys().any(|key| !allowed.contains(&key.as_str()))
            || tls.get("enabled") != Some(&Value::Bool(true))
            || tls.get("insecure").is_some_and(|v| v != false)
        {
            return Err("Параметры TLS не поддерживаются или проверка отключена");
        }
        if let Some(sni) = tls.get("server_name") {
            add("sni", sni.as_str().ok_or("SNI должен быть строкой")?.into());
        }
        if let Some(disabled) = tls.get("disable_sni") {
            add(
                "disable_sni",
                disabled
                    .as_bool()
                    .ok_or("disable_sni должен быть boolean")?
                    .to_string(),
            );
        }
        if let Some(utls) = tls.get("utls") {
            let object = utls.as_object().ok_or("Некорректный uTLS")?;
            if object
                .keys()
                .any(|key| !["enabled", "fingerprint"].contains(&key.as_str()))
                || utls["enabled"] != true
            {
                return Err("Явные неподдерживаемые параметры uTLS");
            }
            add(
                "fp",
                utls["fingerprint"]
                    .as_str()
                    .ok_or("Нет отпечатка uTLS")?
                    .into(),
            );
        }
    }
    let alpn = if clash {
        v.get("alpn")
    } else {
        v.get("tls").and_then(|t| t.get("alpn"))
    };
    if let Some(alpn) = alpn {
        let alpn = alpn
            .as_array()
            .ok_or("ALPN должен быть списком строк")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or("ALPN должен быть строкой")
            })
            .collect::<Result<Vec<_>, _>>()?;
        add("alpn", alpn.join(","));
    }
    match scheme {
        "hysteria2" => {
            if clash {
                add("obfs", field("obfs")?);
                add("obfs-password", field("obfs-password")?);
            } else if let Some(obfs) = v.get("obfs") {
                let object = obfs.as_object().ok_or("Некорректный salamander")?;
                if object
                    .keys()
                    .any(|key| !["type", "password"].contains(&key.as_str()))
                {
                    return Err("Неподдерживаемый salamander");
                }
                add(
                    "obfs",
                    obfs["type"].as_str().ok_or("Нет типа обфускации")?.into(),
                );
                add(
                    "obfs-password",
                    obfs["password"]
                        .as_str()
                        .ok_or("Нет пароля обфускации")?
                        .into(),
                );
            }
        }
        "tuic" => {
            add(
                "congestion_control",
                field(if clash {
                    "congestion-controller"
                } else {
                    "congestion_control"
                })?,
            );
            add(
                "udp_relay_mode",
                field(if clash {
                    "udp-relay-mode"
                } else {
                    "udp_relay_mode"
                })?,
            );
            if clash {
                if let Some(disabled) = v.get("disable-sni") {
                    add(
                        "disable_sni",
                        disabled
                            .as_bool()
                            .ok_or("disable-sni должен быть boolean")?
                            .to_string(),
                    );
                }
            }
        }
        _ => {}
    }
    url.query_pairs_mut().extend_pairs(q);
    parse(url.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_native_profiles_preserve_authentication_and_reject_lossy_options() {
        let sing = r#"{"outbounds":[{"type":"hysteria2","server":"example.com","server_port":443,"password":"pass+word","obfs":{"type":"salamander","password":"obfs+word"},"tls":{"enabled":true,"server_name":"tls.example.com"}},{"type":"tuic","server":"example.com","server_port":443,"uuid":"00000000-0000-4000-8000-000000000001","password":"secret","congestion_control":"bbr","udp_relay_mode":"quic","tls":{"enabled":true,"alpn":["h3"]}},{"type":"anytls","server":"example.com","server_port":443,"password":"secret","tls":{"enabled":true,"utls":{"enabled":true,"fingerprint":"chrome"}}}]}"#;
        let profiles = crate::import_links(sing).unwrap();
        assert_eq!(profiles.len(), 3);
        assert_eq!(profiles[0].credential, "pass+word");
        assert!(
            matches!(&profiles[0].options.native,Some(NativeProtocolOptions::Hysteria2{obfs_password:Some(password)}) if password=="obfs+word")
        );
        for p in profiles {
            assert!(p.same_server(&crate::import_links(&share(&p).unwrap()).unwrap()[0]));
        }
        let clash="proxies:\n  - name: HY2\n    type: hysteria2\n    server: example.com\n    port: 443\n    password: secret\n    sni: tls.example.com\n    obfs: salamander\n    obfs-password: obfs-secret\n";
        assert_eq!(crate::import_links(clash).unwrap().len(), 1);
        for bad in [
            sing.replace(
                "\"congestion_control\":\"bbr\"",
                "\"zero_rtt_handshake\":true",
            ),
            sing.replace("\"enabled\":true", "\"enabled\":false"),
            clash.to_owned() + "    up: 100 Mbps\n",
            clash.to_owned() + "    skip-cert-verify: true\n",
        ] {
            assert!(crate::import_links(&bad).is_err());
        }
        let p = parse("hy2://secret@example.com?obfs=salamander&obfs-password=a+b").unwrap();
        assert!(
            matches!(p.options.native,Some(NativeProtocolOptions::Hysteria2{obfs_password:Some(password)}) if password=="a+b")
        );
    }
    #[test]
    fn native_profiles_roundtrip_and_reject_unsupported_security() {
        for link in ["hy2://pass%3Aword@example.com?sni=example.com&obfs=salamander&obfs-password=secret", "tuic://00000000-0000-4000-8000-000000000001:password@example.com:443?congestion_control=bbr&udp_relay_mode=quic&alpn=h3", "anytls://password@example.com:443?sni=tls.example.com&fp=chrome"] {
            let p=parse(link).unwrap();let next=parse(&share(&p).unwrap()).unwrap();assert!(p.same_server(&next));assert!(sing_box_native_config(&p).is_ok());
        }
        for link in ["hy2://password@example.com?insecure=1","hy2://password@example.com?mport=443-500","hy2://password@example.com?obfs-password=orphan","tuic://bad:password@example.com","tuic://00000000-0000-4000-8000-000000000001:password@example.com?zero_rtt_handshake=true","anytls://password@example.com?padding=arbitrary","anytls://password@example.com?sni=a&sni=b"] {assert!(parse(link).is_err(),"{link}");}
    }
}
