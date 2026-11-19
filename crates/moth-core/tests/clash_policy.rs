use moth_core::inspect_clash_policy;

const DOCUMENT: &str = r#"
proxies:
  - {name: primary, type: socks5, server: example.com, port: 1080}
  - {name: secondary, type: http, server: example.net, port: 8080, dialer-proxy: primary}
proxy-groups:
  - {name: Manual, type: select, proxies: [primary, secondary]}
rules:
  - DOMAIN,one.example,primary
  - DOMAIN-SUFFIX,two.example,secondary
  - MATCH,Manual
dns:
  enable: true
  enhanced-mode: fake-ip
  default-nameserver: [1.1.1.1]
  nameserver: [https://1.1.1.1/dns-query]
"#;

#[test]
fn whole_profile_keeps_rule_order_and_named_chain() {
    let result = inspect_clash_policy(DOCUMENT).unwrap().unwrap();
    assert!(result.preview.can_apply);
    assert_eq!(result.preview.groups[0].name, "Manual");
    assert_eq!(result.preview.rules.len(), 3);
    assert_eq!(result.preview.dns_mode.as_deref(), Some("fake-ip"));
    let plan = result.policy.unwrap();
    let config = plan.configuration().unwrap();
    assert_eq!(config["proxies"][1]["dialer-proxy"], "primary");
    assert_eq!(config["rules"][0], "DOMAIN,one.example,primary");
    assert_eq!(config["rules"][2], "MATCH,Manual");
}

#[test]
fn unsupported_settings_block_whole_profile_but_offer_explicit_nodes() {
    let text =
        format!("{DOCUMENT}\nexternal-ui: 'C:/private/value'\nscript: {{code: secret-value}}\n");
    let result = inspect_clash_policy(&text).unwrap().unwrap();
    assert!(!result.preview.can_apply);
    assert_eq!(result.preview.node_only_count, 1);
    assert!(result.policy.is_none());
    let summary = serde_json::to_string(&result.preview).unwrap();
    assert!(summary.contains("external-ui") && summary.contains("script"));
    assert!(!summary.contains("C:/private/value") && !summary.contains("secret-value"));
}

#[test]
fn removed_relay_and_unknown_nested_fields_are_reported() {
    let text = DOCUMENT
        .replace("type: select", "type: relay")
        .replace("dialer-proxy: primary", "ca-cert: private.pem");
    let result = inspect_clash_policy(&text).unwrap().unwrap();
    assert!(!result.preview.can_apply);
    assert!(result
        .preview
        .issues
        .iter()
        .any(|i| i.field == "proxy-groups[1].type"));
    assert!(result
        .preview
        .issues
        .iter()
        .any(|i| i.field == "proxies[2].ca-cert"));
}

#[test]
fn provider_cache_paths_are_replaced_and_urls_are_not_in_preview() {
    let text = format!("{DOCUMENT}\nproxy-providers:\n  pool:\n    type: http\n    url: 'https://example.com/nodes?token=private&mode=full'\n    path: ./pool.yaml\n    interval: 3600\nrule-providers:\n  ruleset:\n    type: inline\n    behavior: domain\n    payload: ['+.fixture.example']\n");
    let result = inspect_clash_policy(&text).unwrap().unwrap();
    assert!(result.preview.can_apply);
    assert_eq!(result.preview.providers.len(), 2);
    let summary = serde_json::to_string(&result.preview).unwrap();
    assert!(!summary.contains("token=private") && !summary.contains("./pool.yaml"));
    assert!(result.policy.unwrap().configuration().is_err());
}

#[test]
fn malformed_clash_is_not_a_candidate_for_silent_node_import() {
    assert!(inspect_clash_policy("proxies: [\nproxy-groups: []").is_err());
    assert!(inspect_clash_policy("vless://not-a-clash-policy")
        .unwrap()
        .is_none());
    assert!(inspect_clash_policy("proxies: &source []\nproxy-groups: *source").is_err());
}

#[test]
fn provider_refresh_rejects_invalid_data_atomically() {
    let text = format!("{DOCUMENT}\nproxy-providers:\n  pool: {{type: http, url: 'https://example.com/pool', interval: 3600}}\n");
    let mut policy = inspect_clash_policy(&text)
        .unwrap()
        .unwrap()
        .policy
        .unwrap();
    let request = policy.providers().remove(0);
    let before = policy.stored_yaml().unwrap();
    assert!(policy
        .with_provider(&request, "proxies: [{type: http, name: Incomplete}]")
        .is_err());
    assert_eq!(policy.stored_yaml().unwrap(), before);
    policy
        .with_provider(
            &request,
            "proxies: [{type: http, name: Valid, server: example.com, port: 80}]",
        )
        .unwrap();
    let config = policy.configuration().unwrap();
    assert_eq!(config["proxy-providers"]["pool"]["type"], "inline");
    assert!(config["proxy-providers"]["pool"].get("url").is_none());
    assert_eq!(
        config["proxy-providers"]["pool"]["payload"][0]["name"],
        "Valid"
    );
}

#[test]
fn plain_import_cannot_drop_rules_from_a_policy() {
    assert!(moth_core::import_links(DOCUMENT).is_err());
    assert!(moth_core::import_links(
        "proxies: [{name: A, type: socks5, server: example.com, port: 1080}]"
    )
    .is_ok());
}
