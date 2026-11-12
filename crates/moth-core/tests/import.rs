use base64::Engine;
use moth_core::{import_links, xray_check_config, Library};

const VLESS: &str = "vless://c6b3a07a-8bf9-4a18-905b-ea41642633c3@example.com:443?security=tls&sni=example.com&type=ws&path=%2Fvpn#%D0%A2%D0%B5%D1%81%D1%82";

#[test]
fn imports_encoded_names_and_transports() {
    let p = import_links(VLESS).unwrap().remove(0);
    assert_eq!(p.name, "Тест");
    let c = xray_check_config(&p);
    assert_eq!(
        c["outbounds"][0]["streamSettings"]["wsSettings"]["path"],
        "/vpn"
    );
    assert_eq!(
        c["outbounds"][0]["streamSettings"]["tlsSettings"]["allowInsecure"],
        false
    );
}

#[test]
fn supports_base64_lists_and_ipv6() {
    let text = format!("{VLESS}\ntrojan://password@[2001:db8::1]:443?sni=example.com#IPv6");
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let p = import_links(&encoded).unwrap();
    assert_eq!(p.len(), 2);
    assert_eq!(p[1].host, "2001:db8::1");
}

#[test]
fn import_is_atomic_and_errors_do_not_reveal_secrets() {
    let err = import_links(&format!(
        "{VLESS}\ntrojan://VERY_SECRET@example.com:443?allowInsecure=1"
    ))
    .err()
    .unwrap();
    assert_eq!(err.line, 2);
    assert!(!err.to_string().contains("VERY_SECRET"));
}

#[test]
fn fails_closed_on_unsupported_or_ambiguous_parameters() {
    for suffix in [
        "&extra=secret",
        "&type=grpc",
        "&flow=xtls-rprx-vision",
        "&security=none",
        "&insecure=true",
    ] {
        let link = format!("{}{suffix}", VLESS.split('#').next().unwrap());
        assert!(
            import_links(&link).is_err(),
            "unexpectedly accepted {suffix}"
        );
    }
    assert!(import_links("vless://not-a-uuid@example.com:443").is_err());
    assert!(import_links("https://example.com/subscription?token=secret").is_err());
}

#[test]
fn library_deduplicates_and_repairs_selection_on_delete() {
    let mut lib = Library::default();
    assert_eq!(lib.add(import_links(VLESS).unwrap()), 1);
    assert_eq!(lib.add(import_links(VLESS).unwrap()), 0);
    let id = lib.selected.unwrap();
    assert!(!lib.select(uuid::Uuid::new_v4()));
    assert!(lib.remove(id));
    assert!(lib.selected.is_none());
    assert!(lib.validate().is_ok());
}

#[test]
fn reality_preserves_public_key_and_vision() {
    let key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([7u8; 32]);
    let p=import_links(&format!("vless://c6b3a07a-8bf9-4a18-905b-ea41642633c3@example.com:443?security=reality&sni=example.com&pbk={key}&sid=abcd&flow=xtls-rprx-vision")).unwrap().remove(0);
    let config = xray_check_config(&p);
    assert_eq!(
        config["outbounds"][0]["streamSettings"]["realitySettings"]["publicKey"],
        key
    );
    assert_eq!(
        config["outbounds"][0]["settings"]["vnext"][0]["users"][0]["flow"],
        "xtls-rprx-vision"
    );
}

#[test]
fn rejects_oversize_empty_and_insecure_trojan() {
    assert!(import_links(&"a".repeat(2 * 1024 * 1024 + 1)).is_err());
    assert!(import_links("").is_err());
    assert!(import_links("trojan://secret@example.com:443?security=none").is_err());
}

#[test]
fn rejects_malformed_escapes_and_unused_security_options() {
    for link in [
        "trojan://password@example.com:443?path=%QQ",
        "trojan://password@example.com:443?type=tcp&path=%2Fvpn",
        "trojan://password@example.com:443?pbk=ignored-key",
        "trojan://password@example.com:443?type=ws&path=%FF",
        "trojan://password@example.com:443?type=grpc&serviceName=%0A",
        "trojan://password@example.com:443?sni=a.example&peer=b.example",
    ] {
        assert!(import_links(link).is_err());
    }
}

#[test]
fn unencrypted_vless_is_limited_to_private_destinations() {
    let base = "vless://c6b3a07a-8bf9-4a18-905b-ea41642633c3@";
    assert!(import_links(&format!("{base}example.com:443?security=none")).is_err());
    assert!(import_links(&format!("{base}127.0.0.1:443?security=none")).is_ok());
}
