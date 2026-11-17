//! Opt-in aggregate observations, never destinations, domains or process names.
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TrafficHistory {
    pub enabled: bool,
    pub entries: Vec<TrafficRecord>,
    pub baseline_next: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrafficRecord {
    pub session: String,
    pub first_seen: u64,
    pub last_seen: u64,
    pub uploaded_bytes: u64,
    pub downloaded_bytes: u64,
    pub origin_uploaded: u64,
    pub origin_downloaded: u64,
}
impl TrafficHistory {
    pub fn set_enabled(&mut self, enabled: bool) {
        if enabled && !self.enabled {
            self.baseline_next = true;
        }
        self.enabled = enabled;
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.baseline_next = true;
    }
    pub fn observe(
        &mut self,
        session: &str,
        uploaded: u64,
        downloaded: u64,
        now: u64,
    ) -> Result<(), &'static str> {
        if !self.enabled {
            return Ok(());
        }
        if session.len() != 32 || !session.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Некорректный идентификатор измерения");
        }
        if let Some(entry) = self.entries.iter_mut().find(|e| e.session == session) {
            entry.uploaded_bytes = entry
                .uploaded_bytes
                .max(uploaded.saturating_sub(entry.origin_uploaded));
            entry.downloaded_bytes = entry
                .downloaded_bytes
                .max(downloaded.saturating_sub(entry.origin_downloaded));
            entry.last_seen = now.max(entry.last_seen);
        } else {
            let (origin_uploaded, origin_downloaded) = if self.baseline_next {
                (uploaded, downloaded)
            } else {
                (0, 0)
            };
            self.baseline_next = false;
            self.entries.push(TrafficRecord {
                session: session.into(),
                first_seen: now,
                last_seen: now,
                uploaded_bytes: uploaded - origin_uploaded,
                downloaded_bytes: downloaded - origin_downloaded,
                origin_uploaded,
                origin_downloaded,
            });
        }
        self.entries
            .retain(|e| now.saturating_sub(e.last_seen) <= 90 * 86400);
        if self.entries.len() > 128 {
            self.entries.drain(..self.entries.len() - 128);
        }
        self.validate()
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        let mut ids = std::collections::HashSet::new();
        if self.entries.len() > 128
            || self.entries.iter().any(|e| {
                e.session.len() != 32
                    || !e.session.bytes().all(|b| b.is_ascii_hexdigit())
                    || !ids.insert(&e.session)
                    || e.first_seen > e.last_seen
            })
        {
            return Err("Некорректная история расхода");
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opt_in_and_clear_do_not_reintroduce_old_totals_or_destinations() {
        let id = "00000000000000000000000000000001";
        let mut h = TrafficHistory::default();
        h.observe(id, 100, 200, 1).unwrap();
        assert!(h.entries.is_empty());
        h.set_enabled(true);
        h.observe(id, 100, 200, 2).unwrap();
        h.observe(id, 130, 250, 3).unwrap();
        assert_eq!(h.entries[0].uploaded_bytes, 30);
        h.clear();
        h.observe(id, 150, 300, 4).unwrap();
        h.observe(id, 160, 310, 5).unwrap();
        assert_eq!(h.entries[0].uploaded_bytes, 10);
        h.set_enabled(false);
        h.observe(id, 200, 400, 6).unwrap();
        assert_eq!(h.entries[0].uploaded_bytes, 10);
        let fields = serde_json::to_value(&h).unwrap();
        assert!(fields["entries"][0].get("host").is_none());
    }
}
