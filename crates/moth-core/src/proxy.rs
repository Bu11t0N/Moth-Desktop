use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionMode {
    #[default]
    Tun,
    Proxy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicProxyOptions {
    pub listen: IpAddr,
    pub port: u16,
    pub allowed_clients: Vec<String>,
}
impl Default for PublicProxyOptions {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1".parse().unwrap(),
            port: 10808,
            allowed_clients: Vec::new(),
        }
    }
}
fn private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local(),
    }
}
fn private_range(text: &str) -> bool {
    let (address, bits) = match text.split_once('/') {
        Some((a, b)) => (a, b.parse::<u32>().ok()),
        None => (text, None),
    };
    let Ok(ip) = address.parse::<IpAddr>() else {
        return false;
    };
    let (value, width) = match ip {
        IpAddr::V4(ip) => (u128::from(u32::from(ip)), 32),
        IpAddr::V6(ip) => (u128::from(ip), 128),
    };
    let bits = bits.unwrap_or(width);
    if bits > width
        || bits == 0
        || (text.contains('/')
            && text
                .rsplit('/')
                .next()
                .and_then(|b| b.parse::<u32>().ok())
                .is_none())
    {
        return false;
    }
    let mask = if bits == width {
        0
    } else {
        u128::MAX >> (128 - (width - bits))
    };
    let first = value & !mask;
    let last = value | mask;
    let convert = |value: u128| {
        if width == 32 {
            IpAddr::V4((value as u32).into())
        } else {
            IpAddr::V6(value.into())
        }
    };
    private(convert(first)) && private(convert(last))
}
impl PublicProxyOptions {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.port < 1024
            || !private(self.listen)
            || self.allowed_clients.len() > 16
            || (!self.listen.is_loopback() && self.allowed_clients.is_empty())
            || self
                .allowed_clients
                .iter()
                .any(|s| s.len() > 64 || !private_range(s))
        {
            return Err("Для прокси нужен локальный адрес, порт 1024–65535 и явный список частных адресов LAN");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyPreferences {
    pub mode: ConnectionMode,
    pub share_with_tun: bool,
    pub system_proxy: bool,
    pub listener: PublicProxyOptions,
}
impl ProxyPreferences {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.listener.validate()?;
        if self.system_proxy && self.mode == ConnectionMode::Tun && !self.share_with_tun {
            return Err("Для системного прокси нужен включённый HTTP/SOCKS listener");
        }
        Ok(())
    }
    pub fn public_listener(&self) -> Option<&PublicProxyOptions> {
        (self.mode == ConnectionMode::Proxy || self.share_with_tun).then_some(&self.listener)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_private_lan_only() {
        let mut options = PublicProxyOptions::default();
        assert!(options.validate().is_ok());
        options.listen = "0.0.0.0".parse().unwrap();
        assert!(options.validate().is_err());
        options.listen = "192.168.1.5".parse().unwrap();
        assert!(options.validate().is_err());
        for bad in [
            "0.0.0.0/0",
            "172.16.0.0/8",
            "192.168.0.0/no",
            "::/0",
            "1.1.1.1",
        ] {
            options.allowed_clients = vec![bad.into()];
            assert!(options.validate().is_err(), "{bad}");
        }
        for good in ["192.168.1.0/24", "10.0.0.2", "172.16.0.0/12", "fd01::/64"] {
            options.allowed_clients = vec![good.into()];
            assert!(options.validate().is_ok(), "{good}");
        }
    }
}
