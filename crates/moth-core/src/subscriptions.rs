//! Subscription lifecycle. Android Moth format/header behavior is the reference;
//! see THIRD_PARTY.md. URLs and device identifiers are credential-bearing data.
use crate::{Library, Profile};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Quota {
    pub used_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub expires: Option<u64>,
}
impl Quota {
    pub fn parse(header: &str) -> Self {
        let fields: BTreeMap<_, _> = header
            .split(';')
            .filter_map(|part| {
                let (k, v) = part.trim().split_once('=')?;
                Some((k.trim(), v.trim().parse::<u64>().ok()?))
            })
            .collect();
        Self {
            used_bytes: if fields.contains_key("upload") || fields.contains_key("download") {
                Some(
                    fields
                        .get("upload")
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(fields.get("download").copied().unwrap_or(0)),
                )
            } else {
                None
            },
            total_bytes: fields.get("total").copied(),
            expires: fields.get("expire").copied(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub profiles: Vec<Uuid>,
    #[serde(default)]
    pub send_hwid: bool,
    #[serde(default)]
    pub send_device_info: bool,
    #[serde(default = "default_agent")]
    pub user_agent: String,
    #[serde(default)]
    pub refresh_hours: u32,
    #[serde(default)]
    pub vpn_only: bool,
    #[serde(default)]
    pub allow_http: bool,
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub last_attempt: Option<u64>,
    #[serde(default)]
    pub last_success: Option<u64>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub quota: Quota,
}
pub fn default_agent() -> String {
    format!("Moth/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInput {
    pub id: Option<Uuid>,
    pub name: String,
    /// Empty when editing means keep the stored secret URL.
    pub url: String,
    pub send_hwid: bool,
    pub send_device_info: bool,
    pub user_agent: String,
    pub refresh_hours: u32,
    pub vpn_only: bool,
    #[serde(default)]
    pub allow_http: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionSummary {
    pub id: Uuid,
    pub name: String,
    pub profiles: Vec<Uuid>,
    pub send_hwid: bool,
    pub send_device_info: bool,
    pub user_agent: String,
    pub refresh_hours: u32,
    pub vpn_only: bool,
    pub allow_http: bool,
    pub last_attempt: Option<u64>,
    pub last_success: Option<u64>,
    pub last_error: Option<String>,
    pub quota: Quota,
}
impl Subscription {
    pub fn summary(&self) -> SubscriptionSummary {
        SubscriptionSummary {
            id: self.id,
            name: self.name.clone(),
            profiles: self.profiles.clone(),
            send_hwid: self.send_hwid,
            send_device_info: self.send_device_info,
            user_agent: self.user_agent.clone(),
            refresh_hours: self.refresh_hours,
            vpn_only: self.vpn_only,
            allow_http: self.allow_http,
            last_attempt: self.last_attempt,
            last_success: self.last_success,
            last_error: self.last_error.clone(),
            quota: self.quota.clone(),
        }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        subscription_url(&self.url, self.allow_http)?;
        if self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.name.chars().any(char::is_control)
        {
            return Err("Укажите название подписки до 80 символов");
        }
        if !matches!(self.refresh_hours, 0 | 6 | 12 | 24) {
            return Err("Недопустимый интервал обновления");
        }
        if self.user_agent.is_empty()
            || self.user_agent.len() > 160
            || !self.user_agent.bytes().all(|b| (32..127).contains(&b))
        {
            return Err("User-Agent должен содержать печатные ASCII-символы");
        }
        Ok(())
    }
    /// Device headers follow Android's same-origin rule on every redirect.
    /// Explicit source opt-in replaces Android's historical send-to-all default.
    pub fn headers(
        &self,
        destination: &Url,
        hwid: Uuid,
    ) -> Result<BTreeMap<String, String>, &'static str> {
        let original = subscription_url(&self.url, self.allow_http)?;
        let mut out = BTreeMap::from([
            ("User-Agent".into(), self.user_agent.clone()),
            (
                "Accept".into(),
                "text/plain, application/yaml, application/json, application/octet-stream".into(),
            ),
        ]);
        if self.send_hwid
            && original.origin() == destination.origin()
            && destination.scheme() == "https"
        {
            out.insert("x-hwid".into(), hwid.to_string());
            if self.send_device_info {
                out.insert(
                    "x-device-os".into(),
                    if cfg!(windows) { "Windows" } else { "Linux" }.into(),
                );
                // Deliberately no machine name, serial number or account name.
                out.insert("x-ver-os".into(), std::env::consts::OS.into());
                out.insert(
                    "x-device-model".into(),
                    format!("Desktop {}", std::env::consts::ARCH),
                );
            }
        }
        Ok(out)
    }
}
pub fn subscription_url(text: &str, allow_http: bool) -> Result<Url, &'static str> {
    if text.len() > 16384 || text.chars().any(char::is_control) {
        return Err("Некорректный URL подписки");
    }
    let url = Url::parse(text.trim()).map_err(|_| "Некорректный URL подписки")?;
    if !(url.scheme() == "https" || (allow_http && url.scheme() == "http"))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Нужна HTTPS-ссылка без логина и фрагмента; HTTP требует явного разрешения");
    }
    Ok(url)
}
impl Library {
    pub fn put_subscription(&mut self, input: SubscriptionInput) -> Result<Uuid, &'static str> {
        let id = input.id.unwrap_or_else(Uuid::new_v4);
        let previous = self.subscriptions.iter().find(|s| s.id == id);
        if input.id.is_some() && previous.is_none() {
            return Err("Подписка не найдена");
        }
        let source = Subscription {
            id,
            name: input.name.trim().into(),
            url: if input.url.trim().is_empty() {
                previous
                    .map(|s| s.url.clone())
                    .ok_or("Укажите URL подписки")?
            } else {
                input.url.trim().into()
            },
            profiles: previous.map(|s| s.profiles.clone()).unwrap_or_default(),
            send_hwid: input.send_hwid,
            send_device_info: input.send_device_info,
            user_agent: input.user_agent,
            refresh_hours: input.refresh_hours,
            vpn_only: input.vpn_only,
            allow_http: input.allow_http,
            generation: previous
                .map(|s| s.generation.saturating_add(1))
                .unwrap_or(0),
            last_attempt: previous.and_then(|s| s.last_attempt),
            last_success: previous.and_then(|s| s.last_success),
            last_error: None,
            quota: previous.map(|s| s.quota.clone()).unwrap_or_default(),
        };
        source.validate()?;
        if self
            .subscriptions
            .iter()
            .any(|s| s.id != id && s.url == source.url)
        {
            return Err("Такая подписка уже добавлена");
        }
        if let Some(old) = self.subscriptions.iter_mut().find(|s| s.id == id) {
            *old = source;
        } else {
            if self.subscriptions.len() >= 100 {
                return Err("Не более 100 подписок");
            }
            self.subscriptions.push(source);
        }
        Ok(id)
    }
    pub fn remove_subscription(&mut self, id: Uuid) -> Result<(), &'static str> {
        let source = self
            .subscriptions
            .iter()
            .find(|s| s.id == id)
            .ok_or("Подписка не найдена")?;
        let ids: HashSet<_> = source.profiles.iter().copied().collect();
        self.profiles.retain(|p| !ids.contains(&p.id));
        self.favorites.retain(|p| !ids.contains(p));
        self.subscriptions.retain(|s| s.id != id);
        if self.selected.is_some_and(|p| ids.contains(&p)) {
            self.selected = self.profiles.first().map(|p| p.id);
        }
        Ok(())
    }
    pub fn apply_subscription(
        &mut self,
        id: Uuid,
        generation: u64,
        mut profiles: Vec<Profile>,
        quota: Quota,
        now: u64,
    ) -> Result<(), &'static str> {
        let source = self
            .subscriptions
            .iter()
            .find(|s| s.id == id && s.generation == generation)
            .ok_or("Подписка изменилась во время обновления")?;
        let old: HashSet<_> = source.profiles.iter().copied().collect();
        let mut seen = Vec::<Profile>::new();
        for mut p in profiles.drain(..) {
            if seen.iter().any(|v| v.same_server(&p)) {
                continue;
            }
            if let Some(existing) = self
                .profiles
                .iter()
                .find(|v| old.contains(&v.id) && v.same_server(&p))
            {
                p.id = existing.id;
            }
            seen.push(p);
        }
        if seen.is_empty() {
            return Err("В ответе нет поддерживаемых серверов");
        }
        let new_ids: Vec<_> = seen.iter().map(|p| p.id).collect();
        self.profiles.retain(|p| !old.contains(&p.id));
        self.profiles.extend(seen);
        let source = self.subscriptions.iter_mut().find(|s| s.id == id).unwrap();
        source.profiles = new_ids;
        source.quota = quota;
        source.last_success = Some(now);
        source.last_error = None;
        self.favorites
            .retain(|id| self.profiles.iter().any(|p| p.id == *id));
        if self
            .selected
            .is_none_or(|id| !self.profiles.iter().any(|p| p.id == id))
        {
            self.selected = self.profiles.first().map(|p| p.id);
        }
        self.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> SubscriptionInput {
        SubscriptionInput {
            id: None,
            name: "Provider".into(),
            url: "https://example.com/secret".into(),
            send_hwid: false,
            send_device_info: false,
            user_agent: default_agent(),
            refresh_hours: 6,
            vpn_only: false,
            allow_http: false,
        }
    }
    #[test]
    fn private_defaults_and_origin_bound_device_headers() {
        let mut lib = Library::default();
        let id = lib.put_subscription(input()).unwrap();
        let hwid = lib.device_id;
        let source = &mut lib.subscriptions[0];
        let same = Url::parse("https://example.com/next").unwrap();
        assert!(!source.headers(&same, hwid).unwrap().contains_key("x-hwid"));
        source.send_hwid = true;
        source.send_device_info = true;
        assert_eq!(
            source.headers(&same, hwid).unwrap()["x-hwid"],
            hwid.to_string()
        );
        for destination in [
            "https://other.example/",
            "https://example.com:8443/",
            "http://example.com/",
        ] {
            assert!(!source
                .headers(&Url::parse(destination).unwrap(), hwid)
                .unwrap()
                .contains_key("x-hwid"));
        }
        let summary = serde_json::to_string(
            &lib.subscriptions
                .iter()
                .find(|s| s.id == id)
                .unwrap()
                .summary(),
        )
        .unwrap();
        assert!(!summary.contains("secret"));
        assert!(!summary.contains(&hwid.to_string()));
    }
    #[test]
    fn refresh_retains_identity_selection_and_favorite_and_rejects_stale() {
        let mut lib = Library::default();
        let id = lib.put_subscription(input()).unwrap();
        let parse = || crate::import_links("trojan://synthetic@example.com:443#one").unwrap();
        lib.apply_subscription(id, 0, parse(), Quota::default(), 1)
            .unwrap();
        let p = lib.selected.unwrap();
        lib.favorites.push(p);
        lib.apply_subscription(id, 0, parse(), Quota::default(), 2)
            .unwrap();
        assert_eq!(lib.selected, Some(p));
        assert_eq!(lib.favorites, vec![p]);
        let mut edit = input();
        edit.id = Some(id);
        edit.url = "".into();
        lib.put_subscription(edit).unwrap();
        assert!(lib
            .apply_subscription(id, 0, parse(), Quota::default(), 3)
            .is_err());
        lib.remove_subscription(id).unwrap();
        assert!(lib
            .apply_subscription(id, 1, parse(), Quota::default(), 4)
            .is_err());
    }
    #[test]
    fn old_library_migrates_without_loss() {
        let lib: Library = serde_json::from_str(r#"{"profiles":[],"selected":null}"#).unwrap();
        assert!(lib.subscriptions.is_empty());
        assert!(!lib.device_id.is_nil());
        let saved = serde_json::to_string(&lib).unwrap();
        let next: Library = serde_json::from_str(&saved).unwrap();
        assert_eq!(lib.device_id, next.device_id);
    }
}
