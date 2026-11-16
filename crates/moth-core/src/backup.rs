//! Versioned portable backups using standard AES-GCM/PBKDF2 components.
//! Android moth-backup v1 is read without copying its HWID or package IDs.
//! Format reference: Bu11t0N/Moth Backup.kt and CustomRouting.kt, GPL-3.0,
//! commit b529c62ff2e97da35bdb989237a2dd994f993cce (Android 1.0.3).
use crate::{Library, Quota, SubscriptionInput};
use aes_gcm::{
    aead::{rand_core::RngCore, Aead, AeadCore, OsRng, Payload},
    Aes256Gcm, KeyInit,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;
const FORMAT: &str = "moth-desktop-backup";
const MAX: usize = 8 * 1024 * 1024;
const ITERATIONS: u32 = 600_000;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub servers: usize,
    pub subscriptions: usize,
    pub favorites: usize,
    pub rules: usize,
    pub warnings: Vec<String>,
}
pub struct DecodedBackup {
    pub library: Library,
    pub warnings: Vec<String>,
}
impl DecodedBackup {
    pub fn summary(&self) -> BackupSummary {
        BackupSummary {
            servers: self.library.profiles.len(),
            subscriptions: self.library.subscriptions.len(),
            favorites: self.library.favorites.len(),
            rules: self.library.settings.rules.len(),
            warnings: self.warnings.clone(),
        }
    }
}
fn key(password: &str, salt: &[u8], iterations: u32) -> Zeroizing<[u8; 32]> {
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut *key);
    key
}
pub fn export_backup(library: &Library, password: &str) -> Result<String, &'static str> {
    library.validate()?;
    if password.chars().count() < 8 || password.len() > 1024 {
        return Err("Для резервной копии нужен пароль от 8 символов");
    }
    let mut data =
        serde_json::to_value(library).map_err(|_| "Не удалось создать резервную копию")?;
    data.as_object_mut().unwrap().remove("deviceId");
    let plain = Zeroizing::new(
        serde_json::to_vec(&json!({"library":data,"appVersion":env!("CARGO_PKG_VERSION")}))
            .map_err(|_| "Не удалось создать резервную копию")?,
    );
    if plain.len() > MAX / 2 {
        return Err("Резервная копия слишком большая");
    }
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let derived = key(password, &salt, ITERATIONS);
    let cipher = Aes256Gcm::new_from_slice(derived.as_ref())
        .map_err(|_| "Не удалось создать резервную копию")?;
    let aad = format!("{FORMAT}/1");
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plain.as_ref(),
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "Не удалось зашифровать резервную копию")?;
    serde_json::to_string(&json!({"format":FORMAT,"version":1,"encrypted":true,"kdf":"PBKDF2WithHmacSHA256","iterations":ITERATIONS,"salt":STANDARD.encode(salt),"iv":STANDARD.encode(nonce),"data":STANDARD.encode(ciphertext)})).map_err(|_|"Не удалось создать резервную копию")
}
pub fn read_backup(text: &str, password: &str) -> Result<DecodedBackup, &'static str> {
    if text.len() > MAX || password.len() > 1024 {
        return Err("Резервная копия слишком большая");
    }
    let root: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|_| "Некорректная резервная копия")?;
    let format = root["format"]
        .as_str()
        .ok_or("Это не резервная копия Moth")?;
    if ![FORMAT, "moth-backup"].contains(&format) || root["version"].as_u64() != Some(1) {
        return Err("Неизвестная версия резервной копии");
    }
    let payload = if root["encrypted"] == true {
        if password.is_empty() {
            return Err("Для этой резервной копии нужен пароль");
        }
        if root["kdf"] != "PBKDF2WithHmacSHA256" {
            return Err("Неизвестный формат шифрования");
        }
        let iterations = root["iterations"]
            .as_u64()
            .filter(|n| (10_000..=5_000_000).contains(n))
            .ok_or("Некорректные параметры KDF")? as u32;
        let decode = |field: &str| {
            STANDARD
                .decode(root[field].as_str().ok_or("Некорректная резервная копия")?)
                .map_err(|_| "Некорректная резервная копия")
        };
        let salt = decode("salt")?;
        let nonce = decode("iv")?;
        let ciphertext = decode("data")?;
        if !(8..=64).contains(&salt.len()) || nonce.len() != 12 {
            return Err("Некорректные параметры шифрования");
        }
        let derived = key(password, &salt, iterations);
        let cipher = Aes256Gcm::new_from_slice(derived.as_ref())
            .map_err(|_| "Некорректная резервная копия")?;
        let aad = format!("{format}/1");
        let plain = Zeroizing::new(
            cipher
                .decrypt(
                    aes_gcm::Nonce::from_slice(&nonce),
                    Payload {
                        msg: &ciphertext,
                        aad: aad.as_bytes(),
                    },
                )
                .map_err(|_| "Неверный пароль или повреждённая резервная копия")?,
        );
        serde_json::from_slice::<Value>(&plain).map_err(|_| "Некорректная резервная копия")?
    } else {
        if format != "moth-backup" {
            return Err("Незашифрованный desktop backup не поддерживается");
        }
        root["payload"].clone()
    };
    let mut decoded = if format == FORMAT {
        DecodedBackup {
            library: serde_json::from_value(payload["library"].clone())
                .map_err(|_| "Некорректная библиотека в резервной копии")?,
            warnings: Vec::new(),
        }
    } else {
        android(&payload)?
    };
    decoded.library.device_id = Uuid::new_v4();
    decoded.library.settings.auto_connect = false;
    for source in &mut decoded.library.subscriptions {
        source.send_hwid = false;
        source.send_device_info = false;
        source.refresh_hours = 0;
        source.generation = source.generation.saturating_add(1);
        source.last_attempt = None;
    }
    decoded.warnings.push("HWID не переносится; передача данных устройства, расписание и автоподключение выключены до явного выбора".into());
    decoded.library.validate()?;
    for profile in &decoded.library.profiles {
        let link = crate::share_link(profile)?;
        let parsed =
            crate::import_links(&link).map_err(|_| "Неподдерживаемый профиль в резервной копии")?;
        if parsed.len() != 1 || !parsed[0].same_server(profile) {
            return Err("Параметры резервной копии нельзя перенести без потери");
        }
    }
    Ok(decoded)
}

fn android(payload: &Value) -> Result<DecodedBackup, &'static str> {
    let mut library = Library::default();
    let mut mapping = std::collections::BTreeMap::new();
    let book = &payload["subscriptions"];
    let sources = if let Some(rows) = book["sources"].as_array() {
        rows.clone()
    } else if book.get("profiles").is_some() {
        vec![book.clone()]
    } else {
        Vec::new()
    };
    for source in sources {
        let url = source["url"].as_str().unwrap_or("moth:local");
        let mut profiles = Vec::new();
        for entry in source["profiles"]
            .as_array()
            .ok_or("Некорректный список Android-профилей")?
        {
            let name = entry["name"].as_str().unwrap_or("Server");
            let mut parsed = if let Some(link) = entry["link"].as_str().filter(|s| !s.is_empty()) {
                crate::import_links(link).map_err(|_| {
                    "Android backup содержит неподдерживаемый профиль; импорт отменён целиком"
                })?
            } else {
                let text = entry["outbound"]
                    .as_str()
                    .ok_or("В Android-профиле нет конфигурации")?;
                let mut outbound: Value =
                    serde_json::from_str(text).map_err(|_| "Некорректный Android-профиль")?;
                outbound["tag"] = json!(name);
                crate::formats::parse(
                    &serde_json::to_string(&outbound)
                        .map_err(|_| "Некорректный Android-профиль")?,
                )
                .map_err(|_| {
                    "Android backup содержит неподдерживаемый профиль; импорт отменён целиком"
                })?
            };
            if parsed.len() != 1 {
                return Err("Неоднозначный Android-профиль");
            }
            let mut profile = parsed.remove(0);
            profile.name = name.into();
            mapping.insert(
                entry["id"].as_str().unwrap_or("").to_owned(),
                (profile.clone(), url.to_owned()),
            );
            profiles.push(profile);
        }
        if url == "moth:local" {
            library.add(profiles);
            continue;
        }
        let name = source["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| source["providerName"].as_str().filter(|s| !s.is_empty()))
            .unwrap_or("Imported subscription")
            .chars()
            .take(80)
            .collect();
        let id = library.put_subscription(SubscriptionInput {
            id: None,
            name,
            url: url.into(),
            send_hwid: false,
            send_device_info: false,
            user_agent: crate::default_agent(),
            refresh_hours: 0,
            vpn_only: false,
            allow_http: false,
        })?;
        if !profiles.is_empty() {
            library.apply_subscription(
                id,
                0,
                profiles,
                Quota {
                    used_bytes: source["usedBytes"].as_u64(),
                    total_bytes: source["totalBytes"].as_u64(),
                    expires: source["expires"].as_u64(),
                },
                source["updatedAt"].as_u64().unwrap_or(0),
            )?;
        }
    }
    // Import/deduplication can replace generated UUIDs. Resolve Android IDs only
    // after the final desktop membership is known.
    let mapping: std::collections::BTreeMap<_, _> = mapping
        .into_iter()
        .filter_map(|(old, (p, url))| {
            library
                .profiles
                .iter()
                .find(|actual| {
                    actual.same_server(&p)
                        && (url == "moth:local"
                            || library
                                .subscriptions
                                .iter()
                                .any(|s| s.url == url && s.profiles.contains(&actual.id)))
                })
                .map(|actual| (old, actual.id))
        })
        .collect();
    if let Some(id) = book["selected"].as_str().and_then(|id| mapping.get(id)) {
        library.select(*id);
    }
    if let Some(favorites) = payload["settings"]["favorites"].as_array() {
        for id in favorites
            .iter()
            .filter_map(|v| v.as_str())
            .filter_map(|id| mapping.get(id))
        {
            if library.profiles.iter().any(|p| p.id == *id) && !library.favorites.contains(id) {
                library.favorites.push(*id);
            }
        }
    }
    for (android, desktop) in [("dns", "dns"), ("bootstrapDns", "bootstrapDns")] {
        if let Some(value) = payload["settings"][android].as_str() {
            if desktop == "dns" {
                library.settings.dns = value.into();
            } else {
                library.settings.bootstrap_dns = value.into();
            }
        }
    }
    if let Some(mtu) = payload["settings"]["mtu"].as_u64() {
        library.settings.mtu =
            u16::try_from(mtu).map_err(|_| "Некорректный MTU в Android backup")?;
    }
    if let Some(ipv6) = payload["settings"]["ipv6"].as_bool() {
        library.settings.ipv6 = ipv6;
    }
    if let Some(rules) = payload["settings"]["routingRules"].as_array() {
        for rule in rules {
            use crate::settings::{RoutingRule, RuleKind, RuleTarget};
            let kind = match rule["type"].as_str() {
                Some("domain") => RuleKind::Domain,
                Some("domain_suffix") => RuleKind::Suffix,
                Some("domain_keyword") => RuleKind::Keyword,
                Some("ip_cidr") => RuleKind::Cidr,
                _ => return Err("Неподдерживаемое правило Android; импорт отменён"),
            };
            let target = match rule["action"].as_str() {
                Some("proxy") => RuleTarget::Proxy,
                Some("direct") => RuleTarget::Direct,
                Some("block") => RuleTarget::Block,
                _ => return Err("Неподдерживаемое действие Android; импорт отменён"),
            };
            library.settings.rules.push(RoutingRule {
                id: Uuid::new_v4(),
                name: rule["name"].as_str().unwrap_or("").into(),
                enabled: rule["enabled"].as_bool().unwrap_or(true),
                kind,
                value: rule["value"]
                    .as_str()
                    .ok_or("Нет значения правила Android")?
                    .into(),
                target,
            });
        }
    }
    Ok(DecodedBackup{library,warnings:vec!["Android package IDs и Android-специфичные настройки не переносятся; проверьте сетевые настройки перед подключением".into()]})
}

pub fn merge_backup(
    current: &Library,
    mut restored: Library,
    replace: bool,
) -> Result<Library, &'static str> {
    restored.device_id = current.device_id;
    if replace {
        restored.validate()?;
        return Ok(restored);
    }
    let mut merged = current.clone();
    let mut ids = std::collections::BTreeMap::new();
    let owners: std::collections::BTreeMap<_, _> = restored
        .subscriptions
        .iter()
        .flat_map(|source| source.profiles.iter().map(|id| (*id, source.url.as_str())))
        .collect();
    for mut profile in restored.profiles {
        if let Some(existing) = merged.profiles.iter().find(|p| {
            let owner = merged
                .subscriptions
                .iter()
                .find(|s| s.profiles.contains(&p.id));
            p.same_server(&profile)
                && owner.map(|s| s.url.as_str()) == owners.get(&profile.id).copied()
                && !ids.values().any(|id| *id == p.id)
        }) {
            ids.insert(profile.id, existing.id);
            continue;
        }
        let old = profile.id;
        if merged.profiles.iter().any(|p| p.id == profile.id) {
            profile.id = Uuid::new_v4();
        }
        ids.insert(old, profile.id);
        merged.profiles.push(profile);
    }
    for mut source in restored.subscriptions {
        source.profiles = source
            .profiles
            .iter()
            .filter_map(|id| ids.get(id).copied())
            .collect();
        if let Some(existing) = merged
            .subscriptions
            .iter_mut()
            .find(|s| s.url == source.url)
        {
            for id in source.profiles {
                if !existing.profiles.contains(&id) {
                    existing.profiles.push(id);
                }
            }
        } else {
            if merged.subscriptions.iter().any(|s| s.id == source.id) {
                source.id = Uuid::new_v4();
            }
            // Keep each server owned by at most one subscription.
            source
                .profiles
                .retain(|id| !merged.subscriptions.iter().any(|s| s.profiles.contains(id)));
            merged.subscriptions.push(source);
        }
    }
    for id in restored.favorites.iter().filter_map(|id| ids.get(id)) {
        if !merged.favorites.contains(id) {
            merged.favorites.push(*id);
        }
    }
    if merged.selected.is_none() {
        merged.selected = restored.selected.and_then(|id| ids.get(&id).copied());
    }
    for id in restored.archived.iter().filter_map(|id| ids.get(id)) {
        if !merged.archived.contains(id) {
            merged.archived.push(*id);
        }
    }
    merged
        .archived
        .retain(|id| !merged.subscriptions.iter().any(|s| s.profiles.contains(id)));
    merged.validate()?;
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_independent_android_encryption_fixture() {
        let text = include_str!("../tests/fixtures/android-backup-v1.json");
        let decoded = read_backup(text, "synthetic-android-password").unwrap();
        assert_eq!(decoded.library.profiles.len(), 1);
        assert_eq!(decoded.library.favorites.len(), 1);
        assert!(!decoded.library.settings.rules[0].enabled);
        assert!(!serde_json::to_string(&decoded.library)
            .unwrap()
            .contains("synthetic-phone-id"));
        assert!(read_backup(text, "wrong-password").is_err());
    }
    #[test]
    fn merge_preserves_distinct_subscription_ownership_and_archived_favorites() {
        let text = |url: &str| {
            json!({"format":"moth-backup","version":1,"encrypted":false,"payload":{"settings":{},"subscriptions":{"sources":[{"url":url,"profiles":[{"id":"one","name":"Test","link":"trojan://synthetic@example.com:443"}]}]}}}).to_string()
        };
        let first = read_backup(&text("https://example.com/one"), "")
            .unwrap()
            .library;
        let second = read_backup(&text("https://example.com/two"), "")
            .unwrap()
            .library;
        let merged = merge_backup(&first, second, false).unwrap();
        assert_eq!(merged.profiles.len(), 2);
        assert_eq!(merged.subscriptions.len(), 2);
        assert_ne!(
            merged.subscriptions[0].profiles,
            merged.subscriptions[1].profiles
        );
        let unchanged = merge_backup(&merged, first, false).unwrap();
        assert_eq!(unchanged.profiles.len(), 2);
        let mut archived = Library::default();
        archived.add(crate::import_links("trojan://other@example.com:443").unwrap());
        let id = archived.profiles[0].id;
        archived.archived.push(id);
        archived.favorites.push(id);
        let restored = merge_backup(&merged, archived, false).unwrap();
        assert_eq!(restored.archived, vec![id]);
        assert!(restored.favorites.contains(&id));
    }
    #[test]
    fn android_duplicate_aliases_and_ordered_rules_are_preserved() {
        let text = json!({"format":"moth-backup","version":1,"encrypted":false,"payload":{"settings":{"favorites":["alias"],"routingRules":[{"type":"domain_suffix","value":"example.com","action":"direct","enabled":false,"name":"First"},{"type":"ip_cidr","value":"192.0.2.0/24","action":"block"}]},"subscriptions":{"selected":"alias","sources":[{"url":"moth:local","profiles":[{"id":"first","name":"First","link":"trojan://synthetic@example.com:443"},{"id":"alias","name":"Alias","link":"trojan://synthetic@example.com:443"}]}]}}}).to_string();
        let decoded = read_backup(&text, "").unwrap();
        assert_eq!(decoded.library.profiles.len(), 1);
        assert_eq!(
            decoded.library.favorites,
            vec![decoded.library.selected.unwrap()]
        );
        assert_eq!(decoded.library.settings.rules.len(), 2);
        assert!(!decoded.library.settings.rules[0].enabled);
        assert_eq!(decoded.library.settings.rules[1].value, "192.0.2.0/24");
        assert!(read_backup(&text.replace("domain_suffix", "unknown"), "").is_err());
    }
    #[test]
    fn encrypted_roundtrip_and_tamper_refusal_keep_local_identity() {
        let mut original = Library::default();
        original.add(crate::import_links("trojan://synthetic@example.com:443#Test").unwrap());
        let encrypted = export_backup(&original, "synthetic-password").unwrap();
        assert!(
            !encrypted.contains("synthetic@example")
                && !encrypted.contains(&original.device_id.to_string())
        );
        assert!(read_backup(&encrypted, "wrong-password").is_err());
        let decoded = read_backup(&encrypted, "synthetic-password").unwrap();
        assert!(decoded.library.profiles[0].same_server(&original.profiles[0]));
        let current = Library::default();
        let id = current.device_id;
        let restored = merge_backup(&current, decoded.library, true).unwrap();
        assert!(restored.device_id == id);
        let mut changed: Value = serde_json::from_str(&encrypted).unwrap();
        changed["iterations"] = json!(700_000);
        assert!(read_backup(&changed.to_string(), "synthetic-password").is_err());
    }
    #[test]
    fn android_plain_backup_does_not_copy_device_id() {
        let text = r#"{"format":"moth-backup","version":1,"encrypted":false,"payload":{"settings":{"hwid":"phone-id","favorites":["old"]},"subscriptions":{"selected":"old","sources":[{"id":"local","url":"moth:local","profiles":[{"id":"old","name":"Test","link":"trojan://synthetic@example.com:443","outbound":"{}"}]}]}}}"#;
        let result = read_backup(text, "").unwrap();
        assert!(result.library.profiles.len() == 1 && result.library.favorites.len() == 1);
        assert!(!serde_json::to_string(&result.library)
            .unwrap()
            .contains("phone-id"));
    }
}
