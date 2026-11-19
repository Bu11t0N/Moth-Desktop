#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use moth_core::{Library, ProfileSummary};
use moth_platform::ProfileStore;
use serde::Serialize;
use std::sync::Mutex;
use tauri::Manager;
use uuid::Uuid;
mod about;
mod autostart;
mod backup;
mod clash;
mod diagnostics;
mod guard;
#[cfg(all(any(windows, target_os = "linux"), debug_assertions))]
mod native_acceptance;
mod proxy;
mod subscriptions;
mod traffic;
mod tray;
mod updater;
#[cfg(all(any(windows, target_os = "linux"), debug_assertions))]
mod updater_acceptance;

struct AppState {
    autostart_gate: Mutex<()>,
    system_proxy_gate: Mutex<()>,
    diagnostics: diagnostics::Journal,
    store: ProfileStore,
    library: Mutex<Library>,
    engines: moth_engine::EngineSet,
    engine_status: Vec<moth_engine::EngineStatus>,
    probe_gate: tokio::sync::Mutex<()>,
    subscription_gate: tokio::sync::Mutex<()>,
    policy_preview: Mutex<Option<clash::Staged>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    clash_policies: Vec<serde_json::Value>,
    profiles: Vec<ProfileSummary>,
    subscriptions: Vec<moth_core::SubscriptionSummary>,
    favorites: Vec<Uuid>,
    archived: Vec<Uuid>,
    settings: moth_core::ConnectionSettings,
    proxy_preferences: moth_core::ProxyPreferences,
    selected: Option<Uuid>,
    capabilities: moth_platform::Capabilities,
    version: &'static str,
    engines: Vec<moth_engine::EngineStatus>,
}
fn snapshot(library: &Library, engines: &[moth_engine::EngineStatus]) -> Snapshot {
    Snapshot {
        clash_policies: library
            .clash_policies
            .iter()
            .filter_map(|p| serde_json::to_value(p.summary()).ok())
            .collect(),
        profiles: library.profiles.iter().map(|p| p.summary()).collect(),
        subscriptions: library.subscriptions.iter().map(|s| s.summary()).collect(),
        favorites: library.favorites.clone(),
        archived: library.archived.clone(),
        settings: library.settings.clone(),
        proxy_preferences: library.proxy_preferences.clone(),
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
    state.store.save(&next).map_err(|e| {
        state
            .diagnostics
            .record(diagnostics::Code::StorageFailed, None);
        e.to_string()
    })?;
    *library = next;
    Ok(snapshot(&library, &state.engine_status))
}
#[tauri::command]
fn import_servers(text: String, state: tauri::State<AppState>) -> Result<Snapshot, String> {
    if let Some(policy) = moth_core::inspect_clash_policy(&text).map_err(str::to_owned)? {
        if policy.preview.issues.is_empty()
            || !policy.preview.groups.is_empty()
            || !policy.preview.rules.is_empty()
            || !policy.preview.providers.is_empty()
            || policy.preview.dns_mode.is_some()
        {
            return Err(
                "Use the Clash profile preview to choose how to import this document".into(),
            );
        }
    }
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
fn save_connection_settings(
    input: moth_core::ConnectionSettings,
    state: tauri::State<AppState>,
) -> Result<Snapshot, String> {
    input.validate().map_err(str::to_owned)?;
    mutate(&state, |library| {
        library.settings = input;
        Ok(())
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
    #[cfg(any(windows, target_os = "linux"))]
    {
        let link = moth_core::share_link(&profile).map_err(str::to_owned)?;
        match moth_ipc::ServiceClient::production()
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
    #[cfg(any(windows, target_os = "linux"))]
    {
        moth_ipc::ServiceClient::production()
            .health()
            .await
            .map_err(|e| e.to_string())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err("Системная служба этой платформы ещё не реализована".into())
    }
}
async fn tunnel_call(command: moth_ipc::Command) -> Result<moth_ipc::ConnectionState, String> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        let status_query = matches!(command, moth_ipc::Command::ConnectionStatus {});
        let response = moth_ipc::ServiceClient::production().call(command).await;
        let response = match response {
            Err(moth_ipc::IpcError::Protocol) if status_query => {
                moth_ipc::ServiceClient::production()
                    .call(moth_ipc::Command::TunnelStatus {})
                    .await
            }
            other => other,
        };
        match response.map_err(|e| e.to_string())?.result {
            moth_ipc::Reply::Tunnel(state) => Ok(state.into()),
            moth_ipc::Reply::Connection(state) => Ok(state),
            moth_ipc::Reply::Failure { code } => Err(match code {
                moth_ipc::ErrorCode::PortUnavailable => "Адрес прокси недоступен или порт уже занят. Измените адрес/порт и подключитесь заново",
                moth_ipc::ErrorCode::NotOwner => "Подключением управляет другая учётная запись",
                moth_ipc::ErrorCode::Busy => "Служба уже выполняет действие; дождитесь завершения",
                moth_ipc::ErrorCode::InvalidProfile => "Служба отклонила параметры сервера",
                _ => "Не удалось выполнить действие с VPN",
            }
            .into()),
            _ => Err("Некорректный ответ службы".into()),
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        drop(command);
        Err("Системный туннель Linux ещё не реализован".into())
    }
}
#[tauri::command]
async fn tunnel_status(app: tauri::AppHandle) -> Result<moth_ipc::ConnectionState, String> {
    let outcome = tunnel_call(moth_ipc::Command::ConnectionStatus {}).await;
    diagnostics::observe(
        &app,
        outcome.as_ref().ok().map(|s| s.tunnel_state()).as_ref(),
    );
    let status = outcome?;
    #[cfg(any(windows, target_os = "linux"))]
    if status.owned_by_caller && status.phase == moth_ipc::TunnelPhase::Connected {
        let failover = app
            .state::<AppState>()
            .library
            .lock()
            .map(|l| l.settings.failover)
            .unwrap_or(false);
        if failover {
            if let Ok(response) = moth_ipc::ServiceClient::production()
                .call(moth_ipc::Command::ActiveProfile {})
                .await
            {
                if let moth_ipc::Reply::ActiveProfile(Some(profile)) = response.result {
                    let handle = app.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        let state = handle.state::<AppState>();
                        let changed = state
                            .library
                            .lock()
                            .map(|l| {
                                l.selected != Some(profile.id)
                                    && l.profiles.iter().any(|p| p.id == profile.id)
                            })
                            .unwrap_or(false);
                        if changed
                            && mutate(&state, |l| {
                                l.select(profile.id);
                                Ok(())
                            })
                            .is_ok()
                        {
                            use tauri::Emitter;
                            let _ = handle.emit("moth-library-changed", ());
                        }
                    })
                    .await;
                }
            }
        }
    }
    Ok(status)
}
#[tauri::command]
async fn disconnect_vpn(app: tauri::AppHandle) -> Result<moth_ipc::ConnectionState, String> {
    tray::stop_owned().await.map_err(|e| e.to_string())?;
    let lifecycle = app.state::<tray::DesktopLifecycle>();
    let _controls = lifecycle.controls.write().await;
    tray::stop_owned().await.map_err(|e| e.to_string())?;
    proxy::restore_owned(&app).await?;
    if matches!(moth_ipc::service_is_stopped(), Ok(true)) {
        return Ok(moth_ipc::TunnelState::default().into());
    }
    tunnel_call(moth_ipc::Command::ConnectionStatus {}).await
}
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    tray::request_action(&app, true);
}
#[tauri::command]
async fn connect_vpn(
    id: Uuid,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<moth_ipc::ConnectionState, String> {
    let lifecycle = app.state::<tray::DesktopLifecycle>();
    let _lease = lifecycle.controls.read().await;
    if lifecycle.busy.load(std::sync::atomic::Ordering::Acquire) {
        return Err("Дождись завершения отключения VPN".into());
    }
    let (link, policy, candidates, preferences) = {
        let library = state.library.lock().map_err(|_| "Хранилище недоступно")?;
        let profile = library
            .profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or("Сервер не найден")?;
        let mut backups: Vec<_> = library
            .profiles
            .iter()
            .filter(|p| p.id != id && !library.archived.contains(&p.id))
            .collect();
        backups.sort_by_key(|p| !library.favorites.contains(&p.id));
        let candidates = if library.settings.failover {
            backups
                .into_iter()
                .take(7)
                .map(|p| {
                    moth_core::share_link(p).map(|link| moth_ipc::ProfileLink { id: p.id, link })
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(str::to_owned)?
        } else {
            Vec::new()
        };
        (
            moth_core::share_link(profile).map_err(str::to_owned)?,
            library.settings.clone(),
            candidates,
            library.proxy_preferences.clone(),
        )
    };
    preferences.validate().map_err(str::to_owned)?;
    if preferences.mode == moth_core::ConnectionMode::Proxy || preferences.system_proxy {
        let handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let state = handle.state::<AppState>();
            mutate(&state, |l| {
                if l.system_proxy_ownership.is_some() {
                    return Err("Сначала восстановите прежний системный прокси".into());
                }
                l.proxy_session_requested = true;
                Ok(())
            })
        })
        .await
        .map_err(|_| "Не удалось сохранить режим подключения")??;
    }
    diagnostics::record(&app, diagnostics::Code::ConnectStarted);
    let result = if preferences.public_listener().is_some() {
        tunnel_call(moth_ipc::Command::BeginConnection {
            link,
            policy: Box::new(policy),
            profile_id: id,
            candidates,
            mode: preferences.mode,
            listener: preferences.public_listener().cloned(),
        })
        .await
    } else if policy == moth_core::ConnectionSettings::default() {
        tunnel_call(moth_ipc::Command::Connect { link }).await
    } else {
        tunnel_call(moth_ipc::Command::ConnectWithPolicy {
            link,
            policy: Box::new(policy),
            profile_id: id,
            candidates,
        })
        .await
    };
    match &result {
        Ok(status) => diagnostics::observe(&app, Some(&status.tunnel_state())),
        Err(_) => diagnostics::record(&app, diagnostics::Code::ConnectFailed),
    };
    if result
        .as_ref()
        .is_ok_and(|s| s.phase == moth_ipc::TunnelPhase::Connected && s.owned_by_caller)
        && preferences.system_proxy
    {
        if let Err(error) = proxy::apply_system(&app, preferences.listener.port).await {
            // The selected OS mode failed. Keep the recovery journal until the
            // owned settings are restored; never report a successful OS proxy.
            if tunnel_call(moth_ipc::Command::Disconnect {})
                .await
                .is_ok_and(|s| s.phase == moth_ipc::TunnelPhase::Disconnected)
            {
                let _ = proxy::restore_owned(&app).await;
            }
            return Err(error);
        }
    }
    result
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let monitor = match updater::monitor_argument() {
        Ok(value) => value,
        Err(_) => std::process::exit(1),
    };
    if monitor.is_some() {
        // A separate GTK application identity lets the monitor coexist with
        // the exiting GUI and the subsequently restarted installed instance.
        // It never initializes profile storage or the single-instance plugin.
        let mut context = tauri::generate_context!();
        context.config_mut().identifier.push_str(".updater");
        if let Some(window) = context.config_mut().app.windows.first_mut() {
            window.title = "Обновление Moth".into();
        }
        tauri::Builder::default()
            .manage(updater::state(monitor))
            .invoke_handler(tauri::generate_handler![
                updater::update_monitor_mode,
                updater::update_status,
                updater::restart_updated_app
            ])
            .build(context)
            .expect("Не удалось открыть окно установки")
            .run(|_app, _event| {
                #[cfg(debug_assertions)]
                if matches!(_event, tauri::RunEvent::Ready) {
                    updater_acceptance::start_monitor(_app);
                }
            });
        return;
    }
    tauri::Builder::default()
        .manage(updater::state(None))
        .manage(backup::BackupState::default())
        .manage(traffic::TrafficState::default())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::show_main(app)
        }))
        .setup(|app| {
            // Resolve explicit test storage before querying a Windows known folder:
            // a temporary CI account may not have initialized shell folders yet.
            #[cfg(debug_assertions)]
            let directory = match std::env::var_os("MOTH_DEV_DATA_DIR") {
                Some(path) => path.into(),
                None => app.path().app_local_data_dir()?,
            };
            #[cfg(not(debug_assertions))]
            let directory = app.path().app_local_data_dir()?;
            let store = ProfileStore::new(directory);
            let library = store.load()?;
            let automatic = library
                .settings
                .auto_connect
                .then_some(library.selected)
                .flatten();
            let engine_directory = if cfg!(debug_assertions) {
                moth_engine::development_directory()?
            } else {
                app.path().resource_dir()?.join("engines")
            };
            let engines = moth_engine::EngineSet::new(engine_directory);
            let mut engine_status = engines.status();
            engine_status.push(engines.mihomo_status());
            app.manage(AppState {
                autostart_gate: Mutex::new(()),
                system_proxy_gate: Mutex::new(()),
                diagnostics: diagnostics::Journal::default(),
                store,
                library: Mutex::new(library),
                engines,
                engine_status,
                probe_gate: tokio::sync::Mutex::new(()),
                subscription_gate: tokio::sync::Mutex::new(()),
                policy_preview: Mutex::new(None),
            });
            tray::setup(app)?;
            autostart::reconcile(app.handle());
            subscriptions::schedule(app.handle().clone());
            if let Some(id) = automatic {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = connect_vpn(id, handle.state::<AppState>(), handle.clone()).await;
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            clash::preview_clash,
            clash::apply_clash,
            clash::refresh_clash,
            clash::remove_clash,
            clash::connect_clash,
            clash::clash_groups,
            proxy::save_proxy_preferences,
            proxy::system_proxy_status,
            proxy::proxy_access,
            proxy::restore_system_proxy,
            proxy::acknowledge_proxy_recovery,
            autostart::autostart_status,
            autostart::set_autostart,
            about::license_document,
            diagnostics::diagnostic_snapshot,
            diagnostics::clear_diagnostics,
            guard::egress_guard_status,
            traffic::traffic_snapshot,
            traffic::traffic_history,
            traffic::set_traffic_history,
            guard::set_egress_guard,
            backup::export_backup,
            backup::preview_backup,
            backup::confirm_backup,
            backup::discard_backup,
            backup::undo_backup,
            save_connection_settings,
            subscriptions::save_subscription,
            subscriptions::delete_subscription,
            subscriptions::refresh_subscription,
            subscriptions::get_device_id,
            subscriptions::toggle_favorite,
            import_servers,
            select_server,
            remove_server,
            probe_server,
            service_status,
            tunnel_status,
            connect_vpn,
            disconnect_vpn,
            quit_app,
            updater::update_monitor_mode,
            updater::update_status,
            updater::check_updates,
            updater::download_update,
            updater::install_update,
            updater::restart_updated_app
        ])
        .build(tauri::generate_context!())
        .expect("Не удалось запустить Moth Desktop")
        .run(tray::run_event);
}
