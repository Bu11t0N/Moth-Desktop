use crate::{mutate, AppState};
use serde::Serialize;
use std::{
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
#[derive(Default)]
pub struct TrafficState(Mutex<Option<Instant>>);
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummary {
    pub enabled: bool,
    pub entries: Vec<HistoryRow>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub first_seen: u64,
    pub last_seen: u64,
    pub uploaded_bytes: u64,
    pub downloaded_bytes: u64,
}
#[tauri::command]
pub fn traffic_history(state: tauri::State<AppState>) -> Result<HistorySummary, String> {
    let library = state.library.lock().map_err(|_| "Хранилище недоступно")?;
    Ok(HistorySummary {
        enabled: library.traffic_history.enabled,
        entries: library
            .traffic_history
            .entries
            .iter()
            .map(|e| HistoryRow {
                first_seen: e.first_seen,
                last_seen: e.last_seen,
                uploaded_bytes: e.uploaded_bytes,
                downloaded_bytes: e.downloaded_bytes,
            })
            .collect(),
    })
}
#[tauri::command]
pub fn set_traffic_history(
    enabled: bool,
    clear: bool,
    state: tauri::State<AppState>,
    traffic: tauri::State<TrafficState>,
) -> Result<HistorySummary, String> {
    mutate(&state, |library| {
        library.traffic_history.set_enabled(enabled);
        if clear {
            library.traffic_history.clear();
        }
        Ok(())
    })?;
    *traffic.0.lock().map_err(|_| "Хранилище недоступно")? = None;
    traffic_history(state)
}
#[tauri::command]
pub async fn traffic_snapshot(app: tauri::AppHandle) -> Result<moth_ipc::TrafficSnapshot, String> {
    if app
        .get_webview_window("main")
        .is_some_and(|w| w.is_visible().ok() != Some(true) || w.is_focused().ok() != Some(true))
    {
        return Err("Измерения приостановлены для неактивного окна".into());
    }
    #[cfg(any(windows, target_os = "linux"))]
    {
        let reply = moth_ipc::ServiceClient::production()
            .call(moth_ipc::Command::Traffic {})
            .await
            .map_err(|_| "Счётчики службы недоступны")?;
        let moth_ipc::Reply::Traffic(snapshot) = reply.result else {
            return Err("Нет доступных счётчиков вашей активной сессии".into());
        };
        let history_enabled = app
            .state::<AppState>()
            .library
            .lock()
            .map(|l| l.traffic_history.enabled)
            .unwrap_or(false);
        let save = if history_enabled {
            let state = app.state::<TrafficState>();
            let mut last = state.0.lock().map_err(|_| "Хранилище недоступно")?;
            if last.is_none_or(|t| t.elapsed() >= Duration::from_secs(30)) {
                *last = Some(Instant::now());
                true
            } else {
                false
            }
        } else {
            false
        };
        if save {
            let sample = snapshot.clone();
            let handle = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = handle.state::<AppState>();
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                mutate(&state, |library| {
                    library
                        .traffic_history
                        .observe(
                            &sample.session,
                            sample.uploaded_bytes,
                            sample.downloaded_bytes,
                            now,
                        )
                        .map_err(str::to_owned)
                })
                .map(|_| ())
            })
            .await
            .map_err(|_| "Не удалось сохранить измерение")??;
        }
        Ok(snapshot)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        drop(app);
        Err("Статистика недоступна на этой платформе".into())
    }
}
