//! Bounded server extraction from subscription documents. Infrastructure entries
//! are not servers; every actual proxy entry must convert without losing options.
use crate::{Profile, Protocol, ProxyOptions, Tls, Transport};
use serde_json::Value;
use uuid::Uuid;
type Result<T> = std::result::Result<T, &'static str>;
fn keys(v: &Value, allowed: &[&str]) -> Result<()> {
    let object = v.as_object().ok_or("Ожидается объект параметров")?;
    if object.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("Документ содержит неподдерживаемые параметры сервера");
    }
    Ok(())
}
fn text(v: &Value, k: &str) -> String {
    v.get(k)
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_default()
}
fn port(v: &Value, k: &str) -> Result<u16> {
    text(v, k)
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .ok_or("Некорректный порт сервера")
}
fn yes(v: &Value, k: &str) -> bool {
    matches!(v.get(k), Some(Value::Bool(true))) || text(v, k) == "true" || text(v, k) == "1"
}
fn list(v: &Value) -> Result<Vec<String>> {
    if v.is_null() {
        return Ok(Vec::new());
    }
    v.as_array()
        .ok_or("Ожидается список строк")?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or("Ожидается строка"))
        .collect()
}
fn single(v: &Value) -> Result<&Value> {
    let rows = v.as_array().ok_or("Ожидается список серверов")?;
    if rows.len() != 1 {
        return Err("Несколько пользователей или узлов в одном профиле не поддерживаются");
    }
    Ok(&rows[0])
}
fn blank() -> Profile {
    Profile {
        id: Uuid::new_v4(),
        name: String::new(),
        host: String::new(),
        port: 0,
        protocol: Protocol::Vless,
        credential: String::new(),
        tls: Tls::None,
        transport: Transport::Tcp,
        flow: None,
        options: ProxyOptions::default(),
    }
}
fn protocol(s: &str) -> Result<Protocol> {
    match s {
        "vless" => Ok(Protocol::Vless),
        "trojan" => Ok(Protocol::Trojan),
        "vmess" => Ok(Protocol::Vmess),
        "ss" | "shadowsocks" => Ok(Protocol::Shadowsocks),
        "socks" | "socks5" => Ok(Protocol::Socks),
        "http" => Ok(Protocol::Http),
        _ => Err("Протокол в документе пока не поддерживается; обновление отменено целиком"),
    }
}
fn canonical(mut p: Profile) -> Result<Profile> {
    p.host = p.host.trim_matches(['[', ']']).to_lowercase();
    if matches!(p.protocol, Protocol::Vless | Protocol::Vmess) {
        p.credential = Uuid::parse_str(&p.credential)
            .map_err(|_| "Некорректный UUID сервера")?
            .to_string();
    }
    if p.name.is_empty() {
        p.name = p.host.clone();
    }
    if p.name.chars().any(char::is_control) || p.name.chars().count() > 160 {
        return Err("Некорректное имя сервера");
    }
    let link = crate::share_link(&p)?;
    let parsed = crate::import::parse(&link)?;
    if !p.same_server(&parsed) {
        return Err("Параметры сервера нельзя перенести без потери; импорт отменён");
    }
    Ok(parsed)
}
pub(crate) fn parse(input: &str) -> Result<Vec<Profile>> {
    if input.len() > 2 * 1024 * 1024 {
        return Err("Документ больше 2 МБ");
    }
    let trimmed = input.trim_start();
    let root: Value = if trimmed.starts_with('{') || trimmed.starts_with('[') {
        serde_json::from_str(input).map_err(|_| "Некорректный JSON")?
    } else {
        // Subscription YAML does not need alias expansion; reject it explicitly
        // before deserialization to bound adversarial expansion.
        if input.contains('&') || input.contains('*') || input.contains("<<:") {
            return Err("YAML anchors и aliases не поддерживаются");
        }
        serde_yaml_ng::from_str(input).map_err(|_| "Некорректный Clash YAML")?
    };
    let mut rows = Vec::new();
    documents(&root, &mut rows, 0)?;
    if rows.is_empty() {
        return Err("В документе нет поддерживаемых серверов");
    }
    if rows.len() > 1000 {
        return Err("В документе больше 1000 серверов");
    }
    Ok(rows)
}
fn documents(v: &Value, out: &mut Vec<Profile>, depth: usize) -> Result<()> {
    if depth > 12 || out.len() > 1000 {
        return Err("Слишком большой документ подписки");
    }
    if let Some(rows) = v.as_array() {
        for row in rows {
            documents(row, out, depth + 1)?;
        }
        return Ok(());
    }
    if let Some(link) = v.as_str() {
        out.push(crate::import::parse(link)?);
        return Ok(());
    }
    if v.get("proxy-providers")
        .is_some_and(|p| p.as_object().is_none_or(|v| !v.is_empty()))
    {
        return Err("Вложенные Clash providers требуют отдельного источника и не импортированы");
    }
    if let Some(proxies) = v.get("proxies") {
        for proxy in proxies.as_array().ok_or("Ожидается список Clash proxies")? {
            out.push(clash(proxy)?);
        }
        return Ok(());
    }
    if v.get("endpoints")
        .is_some_and(|p| p.as_array().is_none_or(|v| !v.is_empty()))
    {
        return Err("Этот тип sing-box endpoints пока не поддерживается");
    }
    if let Some(rows) = v.get("outbounds") {
        for row in rows.as_array().ok_or("Ожидается список outbounds")? {
            let kind = text(
                row,
                if row.get("protocol").is_some() {
                    "protocol"
                } else {
                    "type"
                },
            );
            if matches!(
                kind.as_str(),
                "direct" | "freedom" | "blackhole" | "block" | "dns"
            ) {
                continue;
            }
            if row.get("protocol").is_some() {
                out.push(xray(row)?);
            } else {
                out.push(sing(row)?);
            }
        }
        return Ok(());
    }
    if v.get("protocol").is_some() {
        out.push(xray(v)?);
        return Ok(());
    }
    if v.get("type").is_some() {
        out.push(sing(v)?);
        return Ok(());
    }
    Err("Неизвестный формат документа подписки")
}
fn xray(v: &Value) -> Result<Profile> {
    keys(
        v,
        &["tag", "protocol", "settings", "streamSettings", "remarks"],
    )?;
    let mut p = blank();
    p.protocol = protocol(&text(v, "protocol"))?;
    p.name = text(v, "remarks");
    if p.name.is_empty() {
        p.name = text(v, "tag");
    }
    let settings = &v["settings"];
    let row = if matches!(p.protocol, Protocol::Vless | Protocol::Vmess) {
        keys(settings, &["vnext"])?;
        single(&settings["vnext"])?
    } else {
        keys(settings, &["servers"])?;
        single(&settings["servers"])?
    };
    keys(row, &["address", "port", "users", "password", "method"])?;
    p.host = text(row, "address");
    p.port = port(row, "port")?;
    match p.protocol {
        Protocol::Vless | Protocol::Vmess => {
            let user = single(&row["users"])?;
            keys(
                user,
                &[
                    "id",
                    "encryption",
                    "flow",
                    "security",
                    "alterId",
                    "email",
                    "level",
                ],
            )?;
            if yes(user, "alterId")
                || (!text(user, "alterId").is_empty() && text(user, "alterId") != "0")
            {
                return Err("VMess alterId не поддерживается");
            }
            if !matches!(text(user, "encryption").as_str(), "" | "none") {
                return Err("Неизвестное шифрование VLESS");
            }
            p.credential = text(user, "id");
            p.flow = (!text(user, "flow").is_empty()).then(|| text(user, "flow"));
            if p.protocol == Protocol::Vmess {
                p.options.method = if text(user, "security").is_empty() {
                    "auto".into()
                } else {
                    text(user, "security")
                };
            }
        }
        Protocol::Trojan => p.credential = text(row, "password"),
        Protocol::Shadowsocks => {
            p.credential = text(row, "password");
            p.options.method = text(row, "method");
        }
        Protocol::Http | Protocol::Socks => {
            if !row["users"].is_null() {
                let user = single(&row["users"])?;
                keys(user, &["user", "pass"])?;
                p.options.username = text(user, "user");
                p.credential = text(user, "pass");
            }
        }
        _ => return Err("Этот протокол не является Xray outbound"),
    }
    let stream = &v["streamSettings"];
    if !stream.is_null() {
        keys(
            stream,
            &[
                "network",
                "security",
                "tlsSettings",
                "realitySettings",
                "wsSettings",
                "grpcSettings",
                "xhttpSettings",
                "tcpSettings",
            ],
        )?;
        let network = text(stream, "network");
        let transport_key = match network.as_str() {
            "ws" => "wsSettings",
            "grpc" => "grpcSettings",
            "xhttp" | "splithttp" => "xhttpSettings",
            _ => "tcpSettings",
        };
        for key in ["wsSettings", "grpcSettings", "xhttpSettings", "tcpSettings"] {
            if key != transport_key
                && stream
                    .get(key)
                    .is_some_and(|v| !v.is_null() && v.as_object().is_none_or(|m| !m.is_empty()))
            {
                return Err("Параметры не соответствуют транспорту Xray");
            }
        }
        let security = text(stream, "security");
        if security != "tls" && stream.get("tlsSettings").is_some_and(|v| !v.is_null()) {
            return Err("Параметры TLS не соответствуют security");
        }
        if security != "reality" && stream.get("realitySettings").is_some_and(|v| !v.is_null()) {
            return Err("Параметры Reality не соответствуют security");
        }
        p.transport = match network.as_str() {
            "" | "tcp" | "raw" => {
                if !stream["tcpSettings"].is_null()
                    && stream["tcpSettings"]
                        .as_object()
                        .is_none_or(|v| !v.is_empty())
                {
                    return Err("TCP header settings не поддерживаются");
                }
                Transport::Tcp
            }
            "ws" => {
                let ws = &stream["wsSettings"];
                keys(ws, &["path", "host", "headers"])?;
                let mut host = text(ws, "host");
                if !ws["headers"].is_null() {
                    keys(&ws["headers"], &["Host", "host"])?;
                    if host.is_empty() {
                        host = text(&ws["headers"], "Host");
                        if host.is_empty() {
                            host = text(&ws["headers"], "host");
                        }
                    }
                }
                Transport::Ws {
                    path: if text(ws, "path").is_empty() {
                        "/".into()
                    } else {
                        text(ws, "path")
                    },
                    host: (!host.is_empty()).then_some(host),
                }
            }
            "grpc" => {
                let grpc = &stream["grpcSettings"];
                keys(grpc, &["serviceName"])?;
                Transport::Grpc {
                    service_name: text(grpc, "serviceName"),
                }
            }
            "xhttp" | "splithttp" => {
                let x = &stream["xhttpSettings"];
                keys(x, &["path", "host", "mode"])?;
                Transport::Xhttp {
                    path: if text(x, "path").is_empty() {
                        "/".into()
                    } else {
                        text(x, "path")
                    },
                    host: (!text(x, "host").is_empty()).then(|| text(x, "host")),
                    mode: if text(x, "mode").is_empty() {
                        "auto".into()
                    } else {
                        text(x, "mode")
                    },
                }
            }
            _ => return Err("Транспорт Xray не поддерживается"),
        };
        p.tls = match text(stream, "security").as_str() {
            "" | "none" => {
                if !stream["tlsSettings"].is_null() || !stream["realitySettings"].is_null() {
                    return Err("TLS settings без security");
                }
                Tls::None
            }
            "tls" => {
                let t = &stream["tlsSettings"];
                keys(
                    t,
                    &[
                        "serverName",
                        "fingerprint",
                        "alpn",
                        "allowInsecure",
                        "insecure",
                    ],
                )?;
                if yes(t, "allowInsecure") || yes(t, "insecure") {
                    return Err("Небезопасный TLS запрещён");
                }
                Tls::Tls {
                    server_name: if text(t, "serverName").is_empty() {
                        p.host.clone()
                    } else {
                        text(t, "serverName")
                    },
                    fingerprint: (!text(t, "fingerprint").is_empty())
                        .then(|| text(t, "fingerprint")),
                    alpn: list(&t["alpn"])?,
                }
            }
            "reality" => {
                let t = &stream["realitySettings"];
                keys(t, &["serverName", "fingerprint", "publicKey", "shortId"])?;
                Tls::Reality {
                    server_name: text(t, "serverName"),
                    fingerprint: if text(t, "fingerprint").is_empty() {
                        "chrome".into()
                    } else {
                        text(t, "fingerprint")
                    },
                    public_key: text(t, "publicKey"),
                    short_id: text(t, "shortId"),
                }
            }
            _ => return Err("Неизвестная защита Xray"),
        };
    }
    canonical(p)
}
fn sing(v: &Value) -> Result<Profile> {
    if matches!(
        v["type"].as_str(),
        Some("hysteria2" | "hy2" | "tuic" | "anytls")
    ) {
        return crate::native_protocols::document(v, false);
    }
    keys(
        v,
        &[
            "type",
            "tag",
            "server",
            "server_port",
            "uuid",
            "password",
            "username",
            "method",
            "security",
            "alter_id",
            "flow",
            "tls",
            "transport",
        ],
    )?;
    let mut p = blank();
    p.protocol = protocol(&text(v, "type"))?;
    p.name = text(v, "tag");
    p.host = text(v, "server");
    p.port = port(v, "server_port")?;
    p.credential = if matches!(p.protocol, Protocol::Vless | Protocol::Vmess) {
        text(v, "uuid")
    } else {
        text(v, "password")
    };
    p.options.username = text(v, "username");
    p.options.method = if p.protocol == Protocol::Vmess {
        if text(v, "security").is_empty() {
            "auto".into()
        } else {
            text(v, "security")
        }
    } else {
        text(v, "method")
    };
    if !matches!(text(v, "alter_id").as_str(), "" | "0") {
        return Err("VMess alter_id не поддерживается");
    }
    p.flow = (!text(v, "flow").is_empty()).then(|| text(v, "flow"));
    let t = &v["tls"];
    if !t.is_null() {
        keys(
            t,
            &[
                "enabled",
                "server_name",
                "insecure",
                "alpn",
                "utls",
                "reality",
            ],
        )?;
        if yes(t, "insecure") {
            return Err("Небезопасный TLS запрещён");
        }
        if !yes(t, "enabled") {
            return Err("Явно отключённый TLS не переносится с параметрами защиты");
        }
        let fp = if t["utls"].is_null() {
            None
        } else {
            keys(&t["utls"], &["enabled", "fingerprint"])?;
            if yes(&t["utls"], "enabled") {
                Some(text(&t["utls"], "fingerprint"))
            } else {
                None
            }
        };
        let sni = if text(t, "server_name").is_empty() {
            p.host.clone()
        } else {
            text(t, "server_name")
        };
        p.tls = if yes(&t["reality"], "enabled") {
            let reality = &t["reality"];
            keys(reality, &["enabled", "public_key", "short_id"])?;
            if !t["alpn"].is_null() {
                return Err("Reality ALPN не поддерживается");
            }
            Tls::Reality {
                server_name: sni,
                fingerprint: fp
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "chrome".into()),
                public_key: text(reality, "public_key"),
                short_id: text(reality, "short_id"),
            }
        } else {
            Tls::Tls {
                server_name: sni,
                fingerprint: fp,
                alpn: list(&t["alpn"])?,
            }
        };
    }
    let transport = &v["transport"];
    if !transport.is_null() {
        keys(transport, &["type", "path", "headers", "service_name"])?;
        if text(transport, "type") == "grpc"
            && (transport.get("path").is_some() || transport.get("headers").is_some())
        {
            return Err("Неиспользуемые параметры транспорта sing-box");
        }
        if text(transport, "type") == "ws" && transport.get("service_name").is_some() {
            return Err("Неиспользуемые параметры транспорта sing-box");
        }
        p.transport = match text(transport, "type").as_str() {
            "ws" => {
                let h = &transport["headers"];
                if !h.is_null() {
                    keys(h, &["Host", "host"])?;
                }
                let host = if text(h, "Host").is_empty() {
                    text(h, "host")
                } else {
                    text(h, "Host")
                };
                Transport::Ws {
                    path: if text(transport, "path").is_empty() {
                        "/".into()
                    } else {
                        text(transport, "path")
                    },
                    host: (!host.is_empty()).then_some(host),
                }
            }
            "grpc" => Transport::Grpc {
                service_name: text(transport, "service_name"),
            },
            _ => return Err("Транспорт sing-box не поддерживается"),
        };
    }
    canonical(p)
}
fn clash(v: &Value) -> Result<Profile> {
    if matches!(
        v["type"].as_str(),
        Some("hysteria2" | "hy2" | "tuic" | "anytls")
    ) {
        return crate::native_protocols::document(v, true);
    }
    keys(
        v,
        &[
            "name",
            "type",
            "server",
            "port",
            "uuid",
            "password",
            "username",
            "cipher",
            "alterId",
            "cipher",
            "tls",
            "servername",
            "sni",
            "client-fingerprint",
            "alpn",
            "skip-cert-verify",
            "network",
            "ws-opts",
            "grpc-opts",
            "reality-opts",
            "flow",
            "udp",
        ],
    )?;
    if yes(v, "skip-cert-verify") {
        return Err("Небезопасный TLS запрещён");
    }
    if v.get("udp") == Some(&Value::Bool(false)) {
        return Err("Ограничение UDP этого Clash-профиля не поддерживается");
    }
    let mut p = blank();
    p.protocol = protocol(&text(v, "type"))?;
    p.name = text(v, "name");
    p.host = text(v, "server");
    p.port = port(v, "port")?;
    p.credential = if matches!(p.protocol, Protocol::Vless | Protocol::Vmess) {
        text(v, "uuid")
    } else {
        text(v, "password")
    };
    p.options.username = text(v, "username");
    p.options.method = if p.protocol == Protocol::Vmess {
        if text(v, "cipher").is_empty() {
            "auto".into()
        } else {
            text(v, "cipher")
        }
    } else {
        text(v, "cipher")
    };
    if !matches!(text(v, "alterId").as_str(), "" | "0") {
        return Err("VMess alterId не поддерживается");
    }
    p.flow = (!text(v, "flow").is_empty()).then(|| text(v, "flow"));
    let mut sni = text(v, "servername");
    if sni.is_empty() {
        sni = text(v, "sni");
    }
    if sni.is_empty() {
        sni = p.host.clone();
    }
    if !v["reality-opts"].is_null() {
        if !v["alpn"].is_null() {
            return Err("Clash Reality ALPN не поддерживается");
        }
        let t = &v["reality-opts"];
        keys(t, &["public-key", "short-id"])?;
        p.tls = Tls::Reality {
            server_name: sni,
            fingerprint: if text(v, "client-fingerprint").is_empty() {
                "chrome".into()
            } else {
                text(v, "client-fingerprint")
            },
            public_key: text(t, "public-key"),
            short_id: text(t, "short-id"),
        };
    } else if yes(v, "tls") || p.protocol == Protocol::Trojan {
        p.tls = Tls::Tls {
            server_name: sni,
            fingerprint: (!text(v, "client-fingerprint").is_empty())
                .then(|| text(v, "client-fingerprint")),
            alpn: list(&v["alpn"])?,
        };
    }
    p.transport = match text(v, "network").as_str() {
        "" | "tcp" => Transport::Tcp,
        "ws" => {
            let t = &v["ws-opts"];
            keys(t, &["path", "headers"])?;
            let h = &t["headers"];
            if !h.is_null() {
                keys(h, &["Host"])?;
            }
            Transport::Ws {
                path: if text(t, "path").is_empty() {
                    "/".into()
                } else {
                    text(t, "path")
                },
                host: (!text(h, "Host").is_empty()).then(|| text(h, "Host")),
            }
        }
        "grpc" => {
            let t = &v["grpc-opts"];
            keys(t, &["grpc-service-name"])?;
            Transport::Grpc {
                service_name: text(t, "grpc-service-name"),
            }
        }
        _ => return Err("Транспорт Clash не поддерживается"),
    };
    canonical(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_formats_keep_tls_and_identity() {
        let yaml="proxies:\n  - name: Test\n    type: trojan\n    server: example.com\n    port: 443\n    password: synthetic\n    sni: sni.example.com\n";
        let p = parse(yaml).unwrap().remove(0);
        assert!(matches!(p.tls,Tls::Tls{server_name,..} if server_name=="sni.example.com"));
        let sing = r#"{"outbounds":[{"type":"trojan","tag":"Test","server":"example.com","server_port":443,"password":"synthetic","tls":{"enabled":true,"server_name":"sni.example.com"}}]}"#;
        assert_eq!(parse(sing).unwrap().len(), 1);
        let xray = r#"{"outbounds":[{"protocol":"trojan","tag":"Test","settings":{"servers":[{"address":"example.com","port":443,"password":"synthetic"}]},"streamSettings":{"network":"tcp","security":"tls","tlsSettings":{"serverName":"sni.example.com"}}}]}"#;
        assert_eq!(parse(xray).unwrap().len(), 1);
    }
    #[test]
    fn unsupported_member_rejects_whole_response() {
        let doc = r#"{"outbounds":[{"type":"trojan","server":"example.com","server_port":443,"password":"synthetic","tls":{"enabled":true}},{"type":"unknown","server":"example.com","server_port":443}]}"#;
        assert!(parse(doc).is_err());
        assert!(parse("proxies: &a [*a]").is_err());
    }
}
