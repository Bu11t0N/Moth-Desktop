//! Typed connection policy; service revalidates it before generating engine JSON.
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingRule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub kind: RuleKind,
    pub value: String,
    pub target: RuleTarget,
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuleKind {
    Domain,
    Suffix,
    Keyword,
    Cidr,
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuleTarget {
    Proxy,
    Direct,
    Block,
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SplitMode {
    #[default]
    Bypass,
    Only,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ConnectionSettings {
    pub mtu: u16,
    pub ipv6: bool,
    pub dns: String,
    pub bootstrap_dns: String,
    pub rules: Vec<RoutingRule>,
    pub split_mode: SplitMode,
    /// Native executable paths, never Android package identifiers.
    pub split_apps: Vec<String>,
    pub health_enabled: bool,
    pub health_interval_seconds: u32,
    pub health_url: String,
    pub auto_reconnect: bool,
    pub failover: bool,
    pub auto_connect: bool,
    pub theme: String,
    pub language: String,
}
impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            mtu: 1500,
            ipv6: true,
            dns: "https://1.1.1.1/dns-query".into(),
            bootstrap_dns: "https://1.1.1.1/dns-query".into(),
            rules: Vec::new(),
            split_mode: SplitMode::Bypass,
            split_apps: Vec::new(),
            health_enabled: false,
            health_interval_seconds: 300,
            health_url: "https://cp.cloudflare.com/generate_204".into(),
            auto_reconnect: false,
            failover: false,
            auto_connect: false,
            theme: "system".into(),
            language: "ru".into(),
        }
    }
}
pub fn dns_server(address: &str) -> Result<serde_json::Value, &'static str> {
    use serde_json::json;
    if let Ok(ip) = address.parse::<IpAddr>() {
        return Ok(json!({"type":"udp","server":ip.to_string(),"server_port":53}));
    }
    let u = url::Url::parse(address).map_err(|_| "DNS: нужен IP или HTTPS URL")?;
    if u.scheme() != "https"
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
        || u.query().is_some()
    {
        return Err("DNS: нужен IP или HTTPS URL без авторизации и query");
    }
    let host = u.host_str().unwrap();
    let mut result = json!({"type":"https","server":host,"server_port":u.port_or_known_default().unwrap_or(443),"path":if u.path().is_empty(){"/dns-query"}else{u.path()}});
    if host.parse::<IpAddr>().is_err() {
        result["domain_resolver"] = json!("bootstrap");
    }
    Ok(result)
}
impl ConnectionSettings {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.failover && !self.auto_reconnect {
            return Err("Для автопереключения включите восстановление подключения");
        }
        if serde_json::to_vec(self)
            .map_err(|_| "Некорректные настройки")?
            .len()
            > 64 * 1024
        {
            return Err("Настройки превышают лимит 64 КБ");
        }
        if !(1280..=9000).contains(&self.mtu) {
            return Err("MTU должен быть от 1280 до 9000");
        }
        dns_server(&self.dns)?;
        let bootstrap = dns_server(&self.bootstrap_dns)?;
        if bootstrap["server"]
            .as_str()
            .is_none_or(|s| s.parse::<IpAddr>().is_err())
        {
            return Err("Bootstrap DNS должен использовать числовой IP");
        }
        let health = url::Url::parse(&self.health_url).map_err(|_| "Некорректный URL проверки")?;
        if health.scheme() != "https"
            || !health.username().is_empty()
            || health.password().is_some()
            || health.host_str().is_none()
            || health.query().is_some()
            || health.fragment().is_some()
            || self.health_url.len() > 2048
        {
            return Err("Проверка требует HTTPS URL без авторизации");
        }
        if ![60, 120, 300, 600, 900].contains(&self.health_interval_seconds) {
            return Err("Недопустимый интервал проверки");
        }
        if !["system", "light", "dark"].contains(&self.theme.as_str())
            || !["ru", "en"].contains(&self.language.as_str())
        {
            return Err("Неизвестная тема или язык");
        }
        if self.rules.len() > 128 || self.split_apps.len() > 128 {
            return Err("Слишком много правил или приложений");
        }
        let mut ids = std::collections::HashSet::new();
        for rule in &self.rules {
            if !ids.insert(rule.id)
                || rule.name.chars().count() > 80
                || rule.name.chars().any(char::is_control)
                || rule.value.is_empty()
                || rule.value.len() > 255
                || rule.value.chars().any(char::is_control)
            {
                return Err("Некорректное правило маршрутизации");
            }
            match rule.kind {
                RuleKind::Cidr => {
                    let (ip, prefix) = rule
                        .value
                        .split_once('/')
                        .ok_or("CIDR требует адрес/префикс")?;
                    let ip: IpAddr = ip.parse().map_err(|_| "Некорректный IP в правиле")?;
                    let n = prefix.parse::<u8>().map_err(|_| "Некорректный CIDR")?;
                    if n > if ip.is_ipv4() { 32 } else { 128 } {
                        return Err("Некорректный CIDR");
                    }
                }
                RuleKind::Domain | RuleKind::Suffix => {
                    if rule
                        .value
                        .bytes()
                        .any(|c| !c.is_ascii_alphanumeric() && c != b'.' && c != b'-')
                    {
                        return Err("Используйте ASCII/punycode домен без URL");
                    }
                }
                RuleKind::Keyword => {}
            }
        }
        for path in &self.split_apps {
            if path.len() > 4096
                || path.chars().any(char::is_control)
                || !(path.starts_with('/')
                    || (path.len() > 3
                        && path.as_bytes()[1] == b':'
                        && matches!(path.as_bytes()[2], b'\\' | b'/')))
            {
                return Err("Для разделения нужен абсолютный путь исполняемого файла");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_rejects_invalid_dns_routes_and_paths() {
        let mut p = ConnectionSettings::default();
        assert!(p.validate().is_ok());
        p.bootstrap_dns = "https://dns.example/dns-query".into();
        assert!(p.validate().is_err());
        p = ConnectionSettings::default();
        p.split_apps.push("org.android.application".into());
        assert!(p.validate().is_err());
        p = ConnectionSettings::default();
        p.rules.push(RoutingRule {
            id: Uuid::new_v4(),
            name: "Rule".into(),
            enabled: true,
            kind: RuleKind::Cidr,
            value: "1.2.3.4/64".into(),
            target: RuleTarget::Direct,
        });
        assert!(p.validate().is_err());
    }
}
