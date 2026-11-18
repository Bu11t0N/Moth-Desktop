use crate::{Profile, Protocol, Tls, Transport};
use url::Url;

/// Internal transport to the service, never exposed to the webview or logs.
/// Re-import before sending to prevent invalid persisted models crossing the boundary.
pub fn share_link(profile: &Profile) -> Result<String, &'static str> {
    if profile.protocol.uses_sing_box() {
        return crate::native_protocols::share(profile);
    }
    if profile.options.native.is_some() {
        return Err("Параметры не соответствуют протоколу");
    }
    if !matches!(profile.protocol, Protocol::Vless | Protocol::Trojan) {
        return crate::extra_protocols::share(profile);
    }
    let scheme = match profile.protocol {
        Protocol::Vless => "vless",
        Protocol::Trojan => "trojan",
        _ => unreachable!(),
    };
    let mut url =
        Url::parse(&format!("{scheme}://placeholder:443")).map_err(|_| "Некорректный профиль")?;
    let host = if profile.host.contains(':') {
        format!("[{}]", profile.host)
    } else {
        profile.host.clone()
    };
    url.set_host(Some(&host))
        .map_err(|_| "Некорректный адрес сервера")?;
    url.set_port(Some(profile.port))
        .map_err(|_| "Некорректный порт")?;
    url.set_username(&profile.credential)
        .map_err(|_| "Некорректные данные доступа")?;
    url.set_fragment(Some(&profile.name));
    {
        let mut query = url.query_pairs_mut();
        match &profile.tls {
            Tls::None => {
                query.append_pair("security", "none");
            }
            Tls::Tls {
                server_name,
                fingerprint,
                alpn,
            } => {
                query
                    .append_pair("security", "tls")
                    .append_pair("sni", server_name);
                if let Some(fp) = fingerprint {
                    query.append_pair("fp", fp);
                }
                if !alpn.is_empty() {
                    query.append_pair("alpn", &alpn.join(","));
                }
            }
            Tls::Reality {
                server_name,
                fingerprint,
                public_key,
                short_id,
            } => {
                query
                    .append_pair("security", "reality")
                    .append_pair("sni", server_name)
                    .append_pair("fp", fingerprint)
                    .append_pair("pbk", public_key)
                    .append_pair("sid", short_id);
            }
        }
        match &profile.transport {
            Transport::Tcp => {
                query.append_pair("type", "tcp");
            }
            Transport::Ws { path, host } => {
                query.append_pair("type", "ws").append_pair("path", path);
                if let Some(host) = host {
                    query.append_pair("host", host);
                }
            }
            Transport::Grpc { service_name } => {
                query
                    .append_pair("type", "grpc")
                    .append_pair("serviceName", service_name);
            }
            Transport::Xhttp { path, host, mode } => {
                query
                    .append_pair("type", "xhttp")
                    .append_pair("path", path)
                    .append_pair("mode", mode);
                if let Some(host) = host {
                    query.append_pair("host", host);
                }
            }
        }
        if let Some(flow) = &profile.flow {
            query.append_pair("flow", flow);
        }
    }
    let link = url.to_string();
    crate::import_links(&link).map_err(|e| e.reason)?;
    Ok(link)
}
