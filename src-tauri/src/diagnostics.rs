//! Structured stage events only. No API accepts arbitrary log text, URLs,
//! profile identifiers, HWID, raw configurations, OS usernames or file paths.
use crate::AppState;
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Code {
    ConnectStarted,
    ConnectVerified,
    ConnectFailed,
    Disconnected,
    DisconnectStarted,
    Recovering,
    ServiceUnavailable,
    SubscriptionStarted,
    SubscriptionSucceeded,
    SubscriptionFailed,
    UpdateCheck,
    UpdateDownload,
    UpdateInstall,
    UpdateSuccess,
    UpdateFailed,
    StorageFailed,
    GuardEnable,
    GuardRelease,
    GuardVerified,
    GuardFailed,
    AutostartEnabled,
    AutostartDisabled,
    AutostartFailed,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub at: u64,
    pub level: &'static str,
    pub category: &'static str,
    pub code: Code,
    pub reason: Option<moth_ipc::ErrorCode>,
}
#[derive(Default)]
struct Inner {
    events: VecDeque<Event>,
    last_tunnel: Option<(moth_ipc::TunnelPhase, Option<moth_ipc::ErrorCode>)>,
    available: Option<bool>,
    last_update: Option<Code>,
}
#[derive(Default)]
pub struct Journal(Mutex<Inner>);
impl Journal {
    pub fn record(&self, code: Code, reason: Option<moth_ipc::ErrorCode>) {
        if let Ok(mut inner) = self.0.lock() {
            push(&mut inner, code, reason);
        }
    }
    pub fn observe(&self, state: Option<&moth_ipc::TunnelState>) {
        let Ok(mut inner) = self.0.lock() else {
            return;
        };
        if let Some(state) = state {
            let current = (state.phase, state.error);
            if inner.last_tunnel != Some(current) || inner.available != Some(true) {
                let code = if state.phase == moth_ipc::TunnelPhase::Failed && state.owned_by_caller
                {
                    Code::Recovering
                } else if state.error.is_some() {
                    Code::ConnectFailed
                } else {
                    match state.phase {
                        moth_ipc::TunnelPhase::Connected => Code::ConnectVerified,
                        moth_ipc::TunnelPhase::Connecting => Code::ConnectStarted,
                        moth_ipc::TunnelPhase::Failed => Code::Recovering,
                        moth_ipc::TunnelPhase::Disconnecting => Code::DisconnectStarted,
                        _ => Code::Disconnected,
                    }
                };
                push(&mut inner, code, state.error);
                inner.last_tunnel = Some(current);
            }
            inner.available = Some(true);
        } else if inner.available != Some(false) {
            push(&mut inner, Code::ServiceUnavailable, None);
            inner.available = Some(false);
        }
    }
}
fn push(inner: &mut Inner, code: Code, reason: Option<moth_ipc::ErrorCode>) {
    let category = match code {
        Code::SubscriptionStarted | Code::SubscriptionSucceeded | Code::SubscriptionFailed => {
            "subscription"
        }
        Code::UpdateCheck
        | Code::UpdateDownload
        | Code::UpdateInstall
        | Code::UpdateSuccess
        | Code::UpdateFailed => "update",
        Code::StorageFailed => "storage",
        Code::AutostartEnabled | Code::AutostartDisabled | Code::AutostartFailed => "startup",
        Code::GuardEnable | Code::GuardRelease | Code::GuardVerified | Code::GuardFailed => "guard",
        _ => "connection",
    };
    let level = match code {
        Code::ConnectFailed
        | Code::SubscriptionFailed
        | Code::UpdateFailed
        | Code::StorageFailed
        | Code::GuardFailed
        | Code::AutostartFailed => "error",
        Code::ServiceUnavailable | Code::Recovering => "warning",
        _ => "info",
    };
    inner.events.push_back(Event {
        at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        level,
        category,
        code,
        reason,
    });
    while inner.events.len() > 256 {
        inner.events.pop_front();
    }
}
pub fn record(app: &tauri::AppHandle, code: Code) {
    if let Some(state) = app.try_state::<AppState>() {
        state.diagnostics.record(code, None);
    }
}
pub fn observe(app: &tauri::AppHandle, state: Option<&moth_ipc::TunnelState>) {
    if let Some(app_state) = app.try_state::<AppState>() {
        app_state.diagnostics.observe(state);
    }
}
pub fn update(app: &tauri::AppHandle, phase: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let code = match phase {
        "checking" => Code::UpdateCheck,
        "downloading" => Code::UpdateDownload,
        "installing" => Code::UpdateInstall,
        "complete" | "installed" | "success" => Code::UpdateSuccess,
        "error" => Code::UpdateFailed,
        _ => return,
    };
    if let Ok(mut inner) = state.diagnostics.0.lock() {
        if inner.last_update != Some(code) {
            push(&mut inner, code, None);
            inner.last_update = Some(code);
        }
    };
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: &'static str,
    pub platform: &'static str,
    pub events: Vec<Event>,
}
#[tauri::command]
pub fn diagnostic_snapshot(state: tauri::State<AppState>) -> Result<Snapshot, String> {
    let inner = state
        .diagnostics
        .0
        .lock()
        .map_err(|_| "Журнал недоступен")?;
    Ok(Snapshot {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        events: inner.events.iter().cloned().collect(),
    })
}
#[tauri::command]
pub fn clear_diagnostics(state: tauri::State<AppState>) -> Result<Snapshot, String> {
    state
        .diagnostics
        .0
        .lock()
        .map_err(|_| "Журнал недоступен")?
        .events
        .clear();
    diagnostic_snapshot(state)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_events_have_no_free_text_or_sensitive_context() {
        let journal = Journal::default();
        for _ in 0..300 {
            journal.record(Code::SubscriptionFailed, None);
        }
        let inner = journal.0.lock().unwrap();
        assert_eq!(inner.events.len(), 256);
        let event = serde_json::to_value(inner.events.front().unwrap()).unwrap();
        let fields = event.as_object().unwrap();
        assert_eq!(fields.len(), 5);
        assert!(
            fields.get("url").is_none()
                && fields.get("hwid").is_none()
                && fields.get("path").is_none()
        );
    }
}
