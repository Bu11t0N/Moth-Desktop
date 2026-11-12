#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use moth_core::{Library, ProfileSummary};
use moth_platform::ProfileStore;
use serde::Serialize;
use std::sync::Mutex;
use tauri::Manager;
use uuid::Uuid;

struct AppState {
    store: ProfileStore,
    library: Mutex<Library>,
    engines: moth_engine::EngineSet,
    engine_status: Vec<moth_engine::EngineStatus>,
    probe_gate: tokio::sync::Mutex<()>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    profiles: Vec<ProfileSummary>,
    selected: Option<Uuid>,
    capabilities: moth_platform::Capabilities,
    version: &'static str,
    engines: Vec<moth_engine::EngineStatus>,
}
fn snapshot(library: &Library, engines: &[moth_engine::EngineStatus]) -> Snapshot {
    Snapshot {
        profiles: library.profiles.iter().map(|p| p.summary()).collect(),
        selected: library.selected,
        capabilities: moth_platform::capabilities(),
        version: env!("CARGO_PKG_VERSION"),
        engines: engines.to_vec(),
    }
}

#[tauri::command]
fn get_snapshot(state: tauri::State<AppState>) -> Result<Snapshot, String> {
    let library = state.library.lock().map_err(|_| "Хранилище недоступно")?;
    Ok(snapshot(&library, &state.engine_status))
}
fn mutate(
    state: &AppState,
    change: impl FnOnce(&mut Library) -> Result<(), String>,
) -> Result<Snapshot, String> {
    let mut library = state.library.lock().map_err(|_| "Хранилище недоступно")?;
    let mut next = library.clone();
    change(&mut next)?;
    state.store.save(&next).map_err(|e| e.to_string())?;
    *library = next;
    Ok(snapshot(&library, &state.engine_status))
}
#[tauri::command]
fn import_servers(text: String, state: tauri::State<AppState>) -> Result<Snapshot, String> {
    let profiles = moth_core::import_links(&text).map_err(|e| e.to_string())?;
    mutate(&state, |library| {
        library.add(profiles);
        library.validate().map_err(str::to_owned)
    })
}
#[tauri::command]
fn select_server(id: Uuid, state: tauri::State<AppState>) -> Result<Snapshot, String> {
    mutate(&state, |library| {
        if library.select(id) {
            Ok(())
        } else {
            Err("Сервер не найден".into())
        }
    })
}
#[tauri::command]
fn remove_server(id: Uuid, state: tauri::State<AppState>) -> Result<Snapshot, String> {
    mutate(&state, |library| {
        if library.remove(id) {
            Ok(())
        } else {
            Err("Сервер не найден".into())
        }
    })
}
#[tauri::command]
async fn probe_server(
    id: Uuid,
    state: tauri::State<'_, AppState>,
) -> Result<moth_engine::ProbeResult, String> {
    let _probe = state
        .probe_gate
        .try_lock()
        .map_err(|_| "Другая проверка ещё выполняется")?;
    let profile = {
        let library = state.library.lock().map_err(|_| "Хранилище недоступно")?;
        library
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or("Сервер не найден")?
    };
    #[cfg(windows)]
    {
        let link = moth_core::share_link(&profile).map_err(str::to_owned)?;
        match moth_ipc::windows::ServiceClient::production()
            .call(moth_ipc::Command::Probe { link })
            .await
        {
            Ok(response) => {
                return match response.result {
                    moth_ipc::Reply::Probe { latency_ms } => Ok(moth_engine::ProbeResult {
                        profile_id: id,
                        latency_ms,
                    }),
                    moth_ipc::Reply::Failure { code } => Err(match code {
                        moth_ipc::ErrorCode::Busy => "Служба уже выполняет проверку",
                        moth_ipc::ErrorCode::EngineUnavailable => {
                            "Сетевые компоненты службы недоступны"
                        }
                        moth_ipc::ErrorCode::InvalidProfile => "Служба отклонила параметры сервера",
                        _ => "Проверка через службу не завершилась успешно",
                    }
                    .into()),
                    _ => Err("Некорректный ответ службы".into()),
                }
            }
            Err(moth_ipc::IpcError::Unavailable) => {} // Explicit preview support without installation.
            Err(error) => return Err(error.to_string()), // Never fall back after an authentication failure.
        }
    }
    state
        .engines
        .probe(&profile)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn service_status() -> Result<moth_ipc::Health, String> {
    #[cfg(windows)]
    {
        moth_ipc::windows::ServiceClient::production()
            .health()
            .await
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        Err("Системная служба этой платформы ещё не реализована".into())
    }
}
fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let mut directory = app.path().app_local_data_dir()?;
            // Development/test override only; release cannot be redirected by env vars.
            if cfg!(debug_assertions) {
                if let Some(path) = std::env::var_os("MOTH_DEV_DATA_DIR") {
                    directory = path.into();
                }
            }
            let store = ProfileStore::new(directory);
            let library = store.load()?;
            let engine_directory = if cfg!(debug_assertions) {
                moth_engine::development_directory()?
            } else {
                app.path().resource_dir()?.join("engines")
            };
            let engines = moth_engine::EngineSet::new(engine_directory);
            let engine_status = engines.status();
            app.manage(AppState {
                store,
                library: Mutex::new(library),
                engines,
                engine_status,
                probe_gate: tokio::sync::Mutex::new(()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            import_servers,
            select_server,
            remove_server,
            probe_server,
            service_status
        ])
        .run(tauri::generate_context!())
        .expect("Не удалось запустить Moth Desktop");
}
