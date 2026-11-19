//! Bounded Clash import. Only fixed fields cross the engine boundary.
use crate::Profile;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    net::IpAddr,
};

const LIMIT: usize = 192 * 1024;
const SECTIONS: &[&str] = &[
    "proxies",
    "proxy-groups",
    "proxy-providers",
    "rule-providers",
    "rules",
    "dns",
];
const BUILTINS: &[&str] = &["DIRECT", "REJECT", "REJECT-DROP"];
const MANAGED: &[&str] = &[
    "port",
    "socks-port",
    "mixed-port",
    "allow-lan",
    "bind-address",
    "log-level",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyPreview {
    pub can_apply: bool,
    pub node_only_count: usize,
    pub nodes: Vec<NodePreview>,
    pub groups: Vec<GroupPreview>,
    pub providers: Vec<ProviderPreview>,
    pub rules: Vec<RulePreview>,
    pub dns_mode: Option<String>,
    pub dns_fields: Vec<String>,
    pub issues: Vec<PolicyIssue>,
    pub managed_fields: Vec<String>,
}
#[derive(Serialize)]
pub struct NodePreview {
    pub name: String,
    pub kind: String,
    pub standalone: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupPreview {
    pub name: String,
    pub kind: String,
    pub members: Vec<String>,
    pub providers: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupState {
    pub name: String,
    pub kind: String,
    pub selected: Option<String>,
    pub members: Vec<String>,
}
#[derive(Serialize)]
pub struct ProviderPreview {
    pub name: String,
    pub kind: &'static str,
    pub remote: bool,
}
#[derive(Serialize)]
pub struct RulePreview {
    pub kind: String,
    pub selector: Option<String>,
    pub target: String,
}
#[derive(Serialize)]
pub struct PolicyIssue {
    pub field: String,
    pub reason: &'static str,
}
pub struct PolicyInspection {
    pub preview: PolicyPreview,
    pub nodes: Vec<Profile>,
    pub policy: Option<ClashPolicy>,
}
#[derive(Clone)]
pub struct ClashPolicy(Value);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedClashPolicy {
    pub id: uuid::Uuid,
    pub name: String,
    pub subscription: Option<uuid::Uuid>,
    pub original: String,
    pub resolved: String,
    pub refreshed_at: u64,
    pub last_attempt: u64,
    pub refresh_seconds: Option<u64>,
    pub refresh_failed: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedClashSummary<'a> {
    pub id: uuid::Uuid,
    pub name: &'a str,
    pub subscription: Option<uuid::Uuid>,
    pub refreshed_at: u64,
    pub refresh_failed: bool,
}
impl SavedClashPolicy {
    pub fn summary(&self) -> SavedClashSummary<'_> {
        SavedClashSummary {
            id: self.id,
            name: &self.name,
            subscription: self.subscription,
            refreshed_at: self.refreshed_at,
            refresh_failed: self.refresh_failed,
        }
    }
    pub fn valid(&self) -> bool {
        !self.id.is_nil()
            && label(&self.name)
            && self.original.len() <= LIMIT
            && self.resolved.len() <= LIMIT
            && self
                .refresh_seconds
                .is_none_or(|n| (60..=604800).contains(&n))
    }
}

#[derive(Clone)]
pub struct ProviderRequest {
    pub name: String,
    pub is_rule: bool,
    pub url: String,
    pub interval: u64,
}

fn label(text: &str) -> bool {
    !text.trim().is_empty()
        && text.len() <= 160
        && !text.contains(['/', '\\', ',', ':'])
        && !text.chars().any(char::is_control)
}
fn display(value: &Value, default: &str) -> String {
    value
        .as_str()
        .filter(|s| label(s))
        .unwrap_or(default)
        .into()
}
fn field_name(text: &str) -> &str {
    if text.len() <= 72
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        text
    } else {
        "<key>"
    }
}
fn https(value: &Value, allow_query: bool) -> bool {
    value
        .as_str()
        .and_then(|s| url::Url::parse(s).ok())
        .is_some_and(|url| {
            url.scheme() == "https"
                && url.host().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
                && (allow_query || url.query().is_none())
        })
}
fn health_url(value: &Value) -> bool {
    if https(value, false) {
        return true;
    }
    value
        .as_str()
        .and_then(|s| url::Url::parse(s).ok())
        .is_some_and(|url| {
            url.scheme() == "http"
                && url.host().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
        })
}
fn clean_text(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| s.len() <= 8192 && !s.chars().any(char::is_control))
}
fn strings(value: &Value) -> Option<Vec<&str>> {
    value
        .as_array()
        .filter(|v| v.len() <= 512)?
        .iter()
        .map(|v| v.as_str().filter(|s| !s.is_empty() && clean_text(v)))
        .collect()
}
fn cidr(text: &str) -> bool {
    text.split_once('/').is_some_and(|(ip, n)| {
        ip.parse::<IpAddr>().ok().is_some_and(|addr| {
            n.parse::<u8>()
                .is_ok_and(|bits| bits <= if addr.is_ipv4() { 32 } else { 128 })
        })
    })
}
fn parse(input: &str) -> Result<Value, &'static str> {
    if input.len() > LIMIT {
        return Err("Clash document exceeds 192 KiB");
    }
    // Reject YAML expansion tokens while allowing '&' within URL query strings.
    let mut previous = b' ';
    for byte in input.bytes() {
        if matches!(byte, b'&' | b'*' | b'!')
            && (previous.is_ascii_whitespace() || b"[{,:".contains(&previous))
        {
            return Err("YAML anchors, aliases and custom tags are unsupported");
        }
        previous = byte;
    }
    // Value first detects non-string/duplicate mapping keys before JSON conversion.
    let document: serde_yaml_ng::Value = serde_yaml_ng::from_str(input)
        .map_err(|_| "Invalid Clash YAML, including duplicate keys")?;
    serde_json::to_value(document).map_err(|_| "Unsupported YAML mapping")
}
fn push(issues: &mut Vec<PolicyIssue>, field: &str, reason: &'static str) {
    if issues.len() < 80 {
        issues.push(PolicyIssue {
            field: field.into(),
            reason,
        });
    }
}
fn keys(value: &Value, path: &str, allowed: &[&str], issues: &mut Vec<PolicyIssue>) {
    let Some(fields) = value.as_object() else {
        push(issues, path, "Expected a mapping");
        return;
    };
    for key in fields.keys() {
        if !allowed.contains(&key.as_str()) {
            push(
                issues,
                &format!("{path}.{}", field_name(key)),
                "Unsupported field",
            );
        }
    }
}
fn array<'a>(root: &'a Value, key: &str, issues: &mut Vec<PolicyIssue>) -> &'a [Value] {
    match root.get(key) {
        None => &[],
        Some(v) => v.as_array().filter(|rows| rows.len() <= 512).map_or_else(
            || {
                push(issues, key, "Expected at most 512 entries");
                &[] as &[Value]
            },
            Vec::as_slice,
        ),
    }
}
fn headers(v: &Value) -> bool {
    v.as_object().is_some_and(|m| {
        m.len() <= 24
            && m.iter().all(|(k, v)| {
                !k.is_empty()
                    && k.len() <= 80
                    && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    && clean_text(v)
            })
    })
}
fn node(node: &Value, path: &str, issues: &mut Vec<PolicyIssue>) {
    let extra: &[&str] = match node["type"].as_str() {
        Some("socks5" | "http") => &["username", "password", "tls", "sni", "headers"],
        Some("vless") => &[
            "uuid",
            "flow",
            "tls",
            "servername",
            "client-fingerprint",
            "network",
            "ws-opts",
            "grpc-opts",
            "reality-opts",
            "alpn",
            "packet-encoding",
        ],
        Some("vmess") => &[
            "uuid",
            "alterId",
            "cipher",
            "tls",
            "servername",
            "client-fingerprint",
            "network",
            "ws-opts",
            "grpc-opts",
            "alpn",
            "packet-encoding",
        ],
        Some("trojan") => &[
            "password",
            "sni",
            "client-fingerprint",
            "network",
            "ws-opts",
            "grpc-opts",
            "alpn",
        ],
        Some("ss") => &["password", "cipher"],
        Some("hysteria2") => &[
            "password",
            "sni",
            "obfs",
            "obfs-password",
            "alpn",
            "up",
            "down",
        ],
        Some("tuic") => &[
            "uuid",
            "password",
            "sni",
            "alpn",
            "congestion-controller",
            "udp-relay-mode",
            "disable-sni",
            "reduce-rtt",
        ],
        Some("anytls") => &["password", "sni", "alpn", "client-fingerprint"],
        _ => {
            push(
                issues,
                &format!("{path}.type"),
                "Unsupported proxy protocol",
            );
            return;
        }
    };
    let mut allowed = vec![
        "name",
        "type",
        "server",
        "port",
        "udp",
        "dialer-proxy",
        "skip-cert-verify",
    ];
    allowed.extend_from_slice(extra);
    keys(node, path, &allowed, issues);
    let Some(mapping) = node.as_object() else {
        return;
    };
    for required in ["name", "server", "port"] {
        if !mapping.contains_key(required) {
            push(
                issues,
                &format!("{path}.{required}"),
                "Required proxy field",
            );
        }
    }
    for (key, value) in mapping {
        let field = format!("{path}.{}", field_name(key));
        let valid = match key.as_str() {
            "name" | "dialer-proxy" => value.as_str().is_some_and(label),
            "server" => value.as_str().is_some_and(|host| {
                host.parse::<IpAddr>().is_ok()
                    || host.len() <= 253
                        && host.contains('.')
                        && host
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
            }),
            "port" => value.as_u64().is_some_and(|n| (1..=65535).contains(&n)),
            "udp" | "tls" | "disable-sni" => value.is_boolean(),
            "skip-cert-verify" | "reduce-rtt" => value == false,
            "network" => matches!(value.as_str(), Some("tcp" | "ws" | "grpc")),
            "alterId" => value.as_u64() == Some(0),
            "headers" => headers(value),
            "alpn" => strings(value).is_some(),
            "ws-opts" | "grpc-opts" | "reality-opts" => {
                let options = match key.as_str() {
                    "ws-opts" => &[
                        "path",
                        "headers",
                        "max-early-data",
                        "early-data-header-name",
                    ][..],
                    "grpc-opts" => &["grpc-service-name"],
                    _ => &["short-id", "public-key"],
                };
                keys(value, &field, options, issues);
                value.as_object().is_some_and(|m| {
                    m.iter().all(|(k, v)| match k.as_str() {
                        "headers" => headers(v),
                        "max-early-data" => v.as_u64().is_some_and(|n| n <= 65535),
                        _ => clean_text(v),
                    })
                })
            }
            "uuid" => value
                .as_str()
                .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok()),
            _ => clean_text(value),
        };
        if !valid {
            push(issues, &field, "Invalid or unsafe proxy option");
        }
    }
}

fn resolver(v: &Value, numeric: bool) -> bool {
    let Some(s) = v.as_str() else { return false };
    if s.parse::<IpAddr>().is_ok() {
        return true;
    }
    s.len() <= 1024
        && https(v, false)
        && (!numeric
            || url::Url::parse(s)
                .is_ok_and(|u| matches!(u.host(), Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)))))
}
fn resolvers(v: &Value, numeric: bool) -> bool {
    v.as_array().is_some_and(|list| {
        !list.is_empty() && list.len() <= 32 && list.iter().all(|v| resolver(v, numeric))
    })
}
fn dns(v: &Value, issues: &mut Vec<PolicyIssue>) -> (Option<String>, Vec<String>) {
    let allowed = &[
        "enable",
        "ipv6",
        "enhanced-mode",
        "fake-ip-range",
        "fake-ip-range6",
        "fake-ip-filter",
        "fake-ip-filter-mode",
        "fake-ip-ttl",
        "default-nameserver",
        "nameserver",
        "nameserver-policy",
        "proxy-server-nameserver",
        "proxy-server-nameserver-policy",
        "fallback",
        "direct-nameserver",
        "direct-nameserver-follow-policy",
        "respect-rules",
        "use-hosts",
        "use-system-hosts",
    ];
    keys(v, "dns", allowed, issues);
    let Some(map) = v.as_object() else {
        return (None, Vec::new());
    };
    for (k, value) in map {
        let valid = match k.as_str() {
            "enable" => value == true,
            "use-system-hosts" => value == false,
            "ipv6" | "respect-rules" | "use-hosts" | "direct-nameserver-follow-policy" => {
                value.is_boolean()
            }
            "enhanced-mode" => matches!(value.as_str(), Some("fake-ip" | "redir-host")),
            "fake-ip-range" | "fake-ip-range6" => value.as_str().is_some_and(cidr),
            "fake-ip-filter" => strings(value).is_some(),
            "fake-ip-filter-mode" => matches!(value.as_str(), Some("blacklist" | "whitelist")),
            "fake-ip-ttl" => value.as_u64().is_some_and(|n| n <= 86400),
            "nameserver-policy" | "proxy-server-nameserver-policy" => {
                value.as_object().is_some_and(|m| {
                    m.len() <= 512
                        && m.iter().all(|(k, v)| {
                            k.len() <= 512
                                && !k.chars().any(char::is_control)
                                && (resolver(v, false) || resolvers(v, false))
                        })
                })
            }
            "default-nameserver" => resolvers(value, true),
            _ => resolvers(value, false),
        };
        if !valid {
            push(
                issues,
                &format!("dns.{}", field_name(k)),
                "Unsupported DNS option or resolver",
            );
        }
    }
    if v["enable"] != true
        || !resolvers(&v["nameserver"], false)
        || !resolvers(&v["default-nameserver"], true)
    {
        push(
            issues,
            "dns",
            "Enabled DNS requires nameservers and explicit numeric bootstrap resolvers",
        );
    }
    (
        Some(
            if v["enhanced-mode"] == "fake-ip" {
                "fake-ip"
            } else {
                "redir-host"
            }
            .into(),
        ),
        map.keys()
            .filter(|k| allowed.contains(&k.as_str()))
            .cloned()
            .collect(),
    )
}

fn selector(kind: &str, value: &str) -> bool {
    if value.is_empty() || value.len() > 384 || value.chars().any(char::is_control) {
        return false;
    }
    match kind {
        "DOMAIN" | "DOMAIN-SUFFIX" | "DOMAIN-KEYWORD" => value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b)),
        "IP-CIDR" | "IP-CIDR6" => cidr(value),
        "GEOIP" => {
            value.eq_ignore_ascii_case("private")
                || value.len() == 2 && value.bytes().all(|b| b.is_ascii_alphabetic())
        }
        "GEOSITE" => value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@-_:".contains(&b)),
        "RULE-SET" => label(value),
        _ => false,
    }
}
fn rule(rule: &str) -> Option<(&str, &str, &str)> {
    let parts: Vec<_> = rule.split(',').map(str::trim).collect();
    match parts.as_slice() {
        ["MATCH", target] => Some(("MATCH", "", target)),
        [kind, value, target] if selector(kind, value) => Some((kind, value, target)),
        [kind @ ("IP-CIDR" | "IP-CIDR6" | "GEOIP" | "RULE-SET"), value, target, "no-resolve"]
            if selector(kind, value) =>
        {
            Some((kind, value, target))
        }
        _ => None,
    }
}
fn rule_payload(v: &Value, behavior: &str) -> bool {
    strings(v).is_some_and(|values| {
        !values.is_empty()
            && values.iter().all(|s| match behavior {
                "domain" => s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-_*+".contains(&b)),
                "ipcidr" => cidr(s),
                "classical" => match s.split(',').map(str::trim).collect::<Vec<_>>().as_slice() {
                    [k, v] if *k != "RULE-SET" => selector(k, v),
                    [k @ ("IP-CIDR" | "IP-CIDR6" | "GEOIP"), v, "no-resolve"] => selector(k, v),
                    _ => false,
                },
                _ => false,
            })
    })
}

fn health(v: &Value, path: &str, issues: &mut Vec<PolicyIssue>) {
    keys(
        v,
        path,
        &[
            "enable",
            "url",
            "interval",
            "timeout",
            "lazy",
            "expected-status",
        ],
        issues,
    );
    if let Some(map) = v.as_object() {
        for (k, v) in map {
            let valid = match k.as_str() {
                "enable" | "lazy" => v.is_boolean(),
                "url" => health_url(v),
                "interval" => v.as_u64().is_some_and(|n| (10..=604800).contains(&n)),
                "timeout" => v.as_u64().is_some_and(|n| (100..=15000).contains(&n)),
                "expected-status" => status_range(v),
                _ => false,
            };
            if !valid {
                push(
                    issues,
                    &format!("{path}.{}", field_name(k)),
                    "Invalid health check option",
                );
            }
        }
    }
    if v["enable"] == true && (!health_url(&v["url"]) || v["interval"].as_u64().is_none()) {
        push(
            issues,
            path,
            "Enabled health checks require a URL and an interval",
        );
    }
}
fn status_range(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() <= 64 && s.bytes().all(|b| b.is_ascii_digit() || b"/-".contains(&b))
    })
}
fn providers(root: &Value, issues: &mut Vec<PolicyIssue>) -> Vec<ProviderPreview> {
    let mut result = Vec::new();
    for (section, is_rule) in [("proxy-providers", false), ("rule-providers", true)] {
        let Some(value) = root.get(section) else {
            continue;
        };
        let Some(rows) = value.as_object().filter(|m| m.len() <= 32) else {
            push(issues, section, "Expected at most 32 named providers");
            continue;
        };
        for (index, (name, v)) in rows.iter().enumerate() {
            let path = format!("{section}[{}]", index + 1);
            let remote = v["type"] == "http";
            keys(
                v,
                &path,
                if is_rule {
                    &[
                        "type", "url", "path", "interval", "behavior", "format", "payload",
                    ]
                } else {
                    &["type", "url", "path", "interval", "payload", "health-check"]
                },
                issues,
            );
            if !label(name) || name == "default" {
                push(issues, &path, "Invalid provider name");
            }
            if remote {
                if !https(&v["url"], true)
                    || !v["interval"]
                        .as_u64()
                        .is_some_and(|n| (60..=604800).contains(&n))
                    || v.get("payload").is_some()
                    || v.get("path").is_some_and(|p| !clean_text(p))
                {
                    push(
                        issues,
                        &path,
                        "Remote provider requires HTTPS and a refresh interval",
                    );
                }
            } else if v["type"] != "inline"
                || ["url", "interval", "path"]
                    .iter()
                    .any(|k| v.get(k).is_some())
            {
                push(
                    issues,
                    &format!("{path}.type"),
                    "Only HTTPS or inline providers are supported. Local files are unavailable",
                );
            }
            if is_rule {
                let behavior = v["behavior"].as_str().unwrap_or_default();
                if !matches!(behavior, "domain" | "ipcidr" | "classical") {
                    push(
                        issues,
                        &format!("{path}.behavior"),
                        "Unknown rule provider behavior",
                    );
                }
                if v.get("format").is_some()
                    && !matches!(v["format"].as_str(), Some("yaml" | "text"))
                {
                    push(
                        issues,
                        &format!("{path}.format"),
                        "Only YAML and text rule sets are supported",
                    );
                }
                if !remote && !rule_payload(&v["payload"], behavior) {
                    push(
                        issues,
                        &format!("{path}.payload"),
                        "Invalid or unsupported rule-provider entries",
                    );
                }
            } else {
                if let Some(value) = v.get("health-check") {
                    health(value, &format!("{path}.health-check"), issues);
                }
                if !remote {
                    if let Some(rows) = v["payload"]
                        .as_array()
                        .filter(|a| !a.is_empty() && a.len() <= 512)
                    {
                        let mut names = HashSet::new();
                        for (index, value) in rows.iter().enumerate() {
                            let field = format!("{path}.payload[{}]", index + 1);
                            node(value, &field, issues);
                            if value.get("dialer-proxy").is_some()
                                || value["name"]
                                    .as_str()
                                    .is_none_or(|s| !names.insert(s) || BUILTINS.contains(&s))
                            {
                                push(issues, &field, "Provider proxies require unique names and cannot add dialer chains");
                            }
                        }
                    } else {
                        push(
                            issues,
                            &format!("{path}.payload"),
                            "Expected a nonempty list of proxies",
                        );
                    }
                }
            }
            result.push(ProviderPreview {
                name: if label(name) {
                    name.clone()
                } else {
                    "Provider".into()
                },
                kind: if is_rule { "rules" } else { "proxies" },
                remote,
            });
        }
    }
    result
}

fn acyclic<'a>(
    name: &'a str,
    edges: &BTreeMap<&str, Vec<&'a str>>,
    stack: &mut HashSet<&'a str>,
    done: &mut HashSet<&'a str>,
) -> bool {
    if done.contains(name) {
        return true;
    }
    if !stack.insert(name) {
        return false;
    }
    if edges
        .get(name)
        .is_some_and(|all| all.iter().any(|to| !acyclic(to, edges, stack, done)))
    {
        return false;
    }
    stack.remove(name);
    done.insert(name);
    true
}
fn inspect(root: Value) -> PolicyInspection {
    let mut issues = Vec::new();
    let mut managed_fields = Vec::new();
    let mut names: HashSet<&str> = BUILTINS.iter().copied().collect();
    let mut edges = BTreeMap::new();
    if let Some(fields) = root.as_object() {
        for (key, value) in fields {
            match key.as_str() {
                k if MANAGED.contains(&k) => managed_fields.push(key.clone()),
                "mode" if value == "rule" => {}
                "ipv6" | "unified-delay" | "tcp-concurrent" if value.is_boolean() => {}
                k if SECTIONS.contains(&k) => {}
                _ => push(
                    &mut issues,
                    field_name(key),
                    "Unsupported field. Full profile would change behavior",
                ),
            }
        }
    }
    let mut nodes = Vec::new();
    let mut node_previews = Vec::new();
    let proxy_rows = array(&root, "proxies", &mut issues);
    let group_rows = array(&root, "proxy-groups", &mut issues);
    for (section, rows) in [("proxies", proxy_rows), ("proxy-groups", group_rows)] {
        for (index, row) in rows.iter().enumerate() {
            if row["name"]
                .as_str()
                .is_none_or(|s| !label(s) || !names.insert(s))
            {
                push(
                    &mut issues,
                    &format!("{section}[{}].name", index + 1),
                    "Invalid, duplicate or reserved name",
                );
            }
        }
    }
    for (index, v) in proxy_rows.iter().enumerate() {
        let path = format!("proxies[{}]", index + 1);
        node(v, &path, &mut issues);
        if let Some(to) = v.get("dialer-proxy") {
            if let (Some(from), Some(to)) = (
                v["name"].as_str(),
                to.as_str()
                    .filter(|s| names.contains(s) && !BUILTINS.contains(s)),
            ) {
                edges.insert(from, vec![to]);
            } else {
                push(
                    &mut issues,
                    &format!("{path}.dialer-proxy"),
                    "Chain target must name a proxy or group",
                );
            }
        }
        let standalone = crate::formats::clash(v).ok();
        node_previews.push(NodePreview {
            name: display(&v["name"], "Proxy"),
            kind: display(&v["type"], "unknown"),
            standalone: standalone.is_some(),
        });
        nodes.extend(standalone);
    }
    let providers = providers(&root, &mut issues);
    let mut groups = Vec::new();
    let mut group_state_size = 0_usize;
    let encoded_name_size = |name: &str| 2 * name.len() + 3;
    for (index, v) in group_rows.iter().enumerate() {
        let path = format!("proxy-groups[{}]", index + 1);
        keys(
            v,
            &path,
            &[
                "name",
                "type",
                "proxies",
                "use",
                "url",
                "interval",
                "tolerance",
                "lazy",
                "timeout",
                "expected-status",
                "disable-udp",
            ],
            &mut issues,
        );
        if !matches!(v["type"].as_str(), Some("select" | "fallback" | "url-test")) {
            push(
                &mut issues,
                &format!("{path}.type"),
                "Unsupported group. Replace legacy relay with dialer-proxy",
            );
        }
        let members = strings(&v["proxies"]).unwrap_or_default();
        let uses = strings(&v["use"]).unwrap_or_default();
        if (members.is_empty() && uses.is_empty())
            || ["proxies", "use"]
                .iter()
                .any(|key| v.get(key).is_some_and(|v| strings(v).is_none()))
            || members.iter().any(|s| !names.contains(s))
            || uses
                .iter()
                .any(|s| root["proxy-providers"].get(s).is_none())
        {
            push(
                &mut issues,
                &path,
                "Group requires a nonempty list of valid proxy or provider references",
            );
        }
        if let Some(from) = v["name"].as_str() {
            edges.insert(from, members.clone());
        }
        // Budget the live group reply after inline providers have been resolved.
        // A name can expand when JSON escapes quotes, and "now" repeats one name.
        group_state_size += 768 + members.iter().map(|s| encoded_name_size(s)).sum::<usize>();
        for name in &uses {
            if let Some(payload) = root["proxy-providers"][*name]["payload"].as_array() {
                group_state_size += payload
                    .iter()
                    .filter_map(|node| node["name"].as_str())
                    .map(encoded_name_size)
                    .sum::<usize>();
            }
        }
        let automatic = v["type"] == "fallback" || v["type"] == "url-test";
        if (automatic || v.get("url").is_some()) && !health_url(&v["url"])
            || (automatic || v.get("interval").is_some())
                && !v["interval"]
                    .as_u64()
                    .is_some_and(|n| (10..=604800).contains(&n))
            || ["lazy", "disable-udp"]
                .iter()
                .any(|k| v.get(k).is_some_and(|x| !x.is_boolean()))
            || v.get("expected-status").is_some_and(|x| !status_range(x))
            || [("timeout", 100, 15000), ("tolerance", 0, 60000)]
                .iter()
                .any(|(k, min, max)| {
                    v.get(k)
                        .is_some_and(|x| !x.as_u64().is_some_and(|n| (*min..=*max).contains(&n)))
                })
        {
            push(&mut issues, &path, "Invalid group health settings");
        }
        groups.push(GroupPreview {
            name: display(&v["name"], "Group"),
            kind: display(&v["type"], "unknown"),
            members: members
                .into_iter()
                .filter(|s| names.contains(s))
                .map(str::to_owned)
                .collect(),
            providers: uses
                .into_iter()
                .filter(|s| root["proxy-providers"].get(s).is_some() && label(s))
                .map(str::to_owned)
                .collect(),
        });
    }
    if group_state_size > LIMIT {
        push(
            &mut issues,
            "proxy-groups",
            "Group status exceeds the supported size",
        );
    }
    let mut done = HashSet::new();
    if edges
        .keys()
        .any(|name| !acyclic(name, &edges, &mut HashSet::new(), &mut done))
    {
        push(
            &mut issues,
            "proxy-groups",
            "Group or dialer-proxy references form a cycle",
        );
    }
    let mut rules = Vec::new();
    let rule_rows = array(&root, "rules", &mut issues);
    for (index, v) in rule_rows.iter().enumerate() {
        match v.as_str().and_then(rule) {
            Some((k, selector, to))
                if names.contains(to)
                    && (k != "RULE-SET" || root["rule-providers"].get(selector).is_some())
                    && (k != "MATCH" || index + 1 == rule_rows.len()) =>
            {
                rules.push(RulePreview {
                    kind: k.into(),
                    selector: (!selector.is_empty()).then(|| selector.into()),
                    target: to.into(),
                });
            }
            _ => push(
                &mut issues,
                &format!("rules[{}]", index + 1),
                "Unsupported rule, missing target/provider, or MATCH before the end",
            ),
        }
    }
    if !rules.last().is_some_and(|r| r.kind == "MATCH") {
        push(
            &mut issues,
            "rules",
            "Final explicit MATCH required to avoid implicit direct traffic",
        );
    }
    let (dns_mode, dns_fields) = root
        .get("dns")
        .map_or((None, Vec::new()), |v| dns(v, &mut issues));
    let can_apply = issues.is_empty();
    PolicyInspection {
        preview: PolicyPreview {
            can_apply,
            node_only_count: nodes.len(),
            nodes: node_previews,
            groups,
            providers,
            rules,
            dns_mode,
            dns_fields,
            issues,
            managed_fields,
        },
        nodes,
        policy: can_apply.then_some(ClashPolicy(root)),
    }
}

/// None allows the ordinary link/JSON parser. A malformed Clash document
/// returns an error instead of falling back to node extraction.
pub fn inspect_clash_policy(input: &str) -> Result<Option<PolicyInspection>, &'static str> {
    let input = input.trim().trim_start_matches('\u{feff}').trim();
    let likely = input.lines().any(|line| {
        line.split_once(':').is_some_and(|(k, _)| {
            let k = k.trim().trim_matches(['"', '\'']);
            SECTIONS.contains(&k)
        })
    }) || (input.starts_with('{')
        && SECTIONS.iter().any(|s| input.contains(&format!("\"{s}\""))));
    if !likely {
        return Ok(None);
    }
    let root = parse(input)?;
    if !root
        .as_object()
        .is_some_and(|map| SECTIONS.iter().any(|s| map.contains_key(*s)))
    {
        return Ok(None);
    }
    Ok(Some(inspect(root)))
}

/// Node-only legacy import is safe only when the document has no policy fields.
pub(crate) fn has_clash_policy(value: &Value) -> bool {
    [
        "proxy-groups",
        "proxy-providers",
        "rule-providers",
        "rules",
        "dns",
        "tun",
        "sub-rules",
        "script",
        "external-controller",
    ]
    .iter()
    .any(|key| value.get(key).is_some())
}

impl ClashPolicy {
    /// Downloads in the private listener must not be silently redirected to a
    /// physical connection by an imported rule or a manual group selection.
    pub fn has_direct_exit(&self) -> bool {
        self.0["rules"].as_array().into_iter().flatten().any(|v| {
            v.as_str()
                .and_then(rule)
                .is_some_and(|(_, _, to)| to == "DIRECT")
        }) || self.0["proxy-groups"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|g| {
                g["proxies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|v| v == "DIRECT")
            })
    }
    pub fn groups(&self) -> Vec<GroupPreview> {
        inspect(self.0.clone()).preview.groups
    }
    pub fn validate_connection(
        &self,
        settings: &crate::ConnectionSettings,
        mode: crate::ConnectionMode,
    ) -> Result<(), &'static str> {
        settings.validate()?;
        if self.0["ipv6"].as_bool().is_some_and(|v| v != settings.ipv6)
            || self.0["dns"]["ipv6"]
                .as_bool()
                .is_some_and(|v| v && !settings.ipv6)
        {
            return Err("Clash IPv6 settings conflict with Moth IPv6");
        }
        if settings.auto_reconnect || settings.failover {
            return Err("Disable Moth reconnect and failover for Clash profiles. Use the profile's fallback groups");
        }
        if mode == crate::ConnectionMode::Proxy
            && (!settings.split_apps.is_empty() || settings.rules.iter().any(|r| r.enabled))
        {
            return Err("Moth app splitting and local routing overrides require TUN");
        }
        if self.0.get("dns").is_some() {
            let defaults = crate::ConnectionSettings::default();
            if settings.dns != defaults.dns || settings.bootstrap_dns != defaults.bootstrap_dns {
                return Err(
                    "Clash DNS conflicts with custom Moth DNS. Reset the Moth DNS overrides first",
                );
            }
        }
        Ok(())
    }
    pub fn stored_yaml(&self) -> Result<String, &'static str> {
        serde_json::to_string(&self.0).map_err(|_| "Invalid policy")
    }
    pub fn providers(&self) -> Vec<ProviderRequest> {
        let mut requests = Vec::new();
        for (section, is_rule) in [("proxy-providers", false), ("rule-providers", true)] {
            if let Some(rows) = self.0[section].as_object() {
                for (name, row) in rows {
                    if row["type"] == "http" {
                        requests.push(ProviderRequest {
                            name: name.clone(),
                            is_rule,
                            url: row["url"].as_str().unwrap().into(),
                            interval: row["interval"].as_u64().unwrap(),
                        });
                    }
                }
            }
        }
        requests
    }
    /// The GUI fetches provider data and replaces the descriptor only after
    /// validating the resulting policy. The worker receives inline data only.
    pub fn with_provider(
        &mut self,
        request: &ProviderRequest,
        body: &str,
    ) -> Result<(), &'static str> {
        if body.len() > LIMIT {
            return Err("Provider response exceeds 192 KiB");
        }
        let section = if request.is_rule {
            "rule-providers"
        } else {
            "proxy-providers"
        };
        let entry = self.0[section]
            .get(&request.name)
            .ok_or("Unknown provider")?;
        if entry["type"] != "http" || entry["url"] != request.url {
            return Err("Provider changed before refresh completed");
        }
        let payload = if request.is_rule && entry["format"] == "text" {
            Value::Array(
                body.lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && !s.starts_with('#'))
                    .map(|s| Value::String(s.into()))
                    .collect(),
            )
        } else {
            let key = if request.is_rule {
                "payload"
            } else {
                "proxies"
            };
            parse(body)?
                .get(key)
                .cloned()
                .ok_or("Provider response is missing its payload")?
        };
        let mut replacement = entry.as_object().ok_or("Invalid provider")?.clone();
        for key in ["url", "path", "interval", "format"] {
            replacement.remove(key);
        }
        replacement.insert("type".into(), "inline".into());
        replacement.insert("payload".into(), payload);
        let mut candidate = self.0.clone();
        candidate[section][&request.name] = Value::Object(replacement);
        if serde_json::to_vec(&candidate)
            .map_err(|_| "Invalid provider payload")?
            .len()
            > LIMIT
        {
            return Err("Combined policy exceeds 192 KiB");
        }
        *self = inspect(candidate)
            .policy
            .ok_or("Unsupported provider payload. Previous configuration retained")?;
        Ok(())
    }
    /// App-owned listeners, controller and egress settings are added separately.
    pub fn configuration(&self) -> Result<Value, &'static str> {
        if !self.providers().is_empty() {
            return Err("Fetch providers before applying policy");
        }
        let mut config = self.0.clone();
        let fields = config.as_object_mut().ok_or("Invalid policy")?;
        for key in MANAGED {
            fields.remove(*key);
        }
        Ok(config)
    }
}
