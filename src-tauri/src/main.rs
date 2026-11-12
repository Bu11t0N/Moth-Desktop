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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    profiles: Vec<ProfileSummary>,
    selected: Option<Uuid>,
    capabilities: moth_platform::Capabilities,
    version: &'static str,
}
fn snapshot(library: &Library) -> Snapshot {
    Snapshot {
        profiles: library.profiles.iter().map(|p| p.summary()).collect(),
        selected: library.selected,
        capabilities: moth_platform::capabilities(),
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[tauri::command]
fn get_snapshot(state: tauri::State<AppState>) -> Result<Snapshot, String> {
    let library = state.library.lock().map_err(|_| "Хранилище недоступно")?;
    Ok(snapshot(&library))
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
    Ok(snapshot(&library))
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
            app.manage(AppState {
                store,
                library: Mutex::new(library),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            import_servers,
            select_server,
            remove_server
        ])
        .run(tauri::generate_context!())
        .expect("Не удалось запустить Moth Desktop");
}
