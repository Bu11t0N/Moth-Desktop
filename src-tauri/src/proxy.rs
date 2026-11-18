use crate::{mutate, AppState, Snapshot};
use tauri::Manager;

#[tauri::command]
pub fn save_proxy_preferences(
    input: moth_core::ProxyPreferences,
    state: tauri::State<AppState>,
) -> Result<Snapshot, String> {
    input.validate().map_err(str::to_owned)?;
    mutate(&state, |library| {
        library.proxy_preferences = input;
        Ok(())
    })
}
#[tauri::command]
pub fn system_proxy_status(
    state: tauri::State<AppState>,
) -> Result<moth_platform::system_proxy::Status, String> {
    let library = state.library.lock().map_err(|_| "Хранилище недоступно")?;
    moth_platform::system_proxy::status(library.system_proxy_ownership.as_deref())
        .map_err(str::to_owned)
}
#[tauri::command]
pub async fn proxy_access() -> Result<moth_ipc::ProxyAccess, String> {
    match moth_ipc::ServiceClient::production()
        .call(moth_ipc::Command::ProxyAccess {})
        .await
        .map_err(|e| e.to_string())?
        .result
    {
        moth_ipc::Reply::ProxyAccess(access) => Ok(access),
        _ => Err("Данные доступны только владельцу активного HTTP/SOCKS прокси".into()),
    }
}
/// Call only after confirming that the caller's connection is stopped.
pub async fn restore_owned(app: &tauri::AppHandle) -> Result<(), String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let _gate = state
            .system_proxy_gate
            .lock()
            .map_err(|_| "Действие с прокси недоступно")?;
        let text = state
            .library
            .lock()
            .map_err(|_| "Хранилище недоступно")?
            .system_proxy_ownership
            .clone();
        if let Some(text) = text {
            let text = zeroize::Zeroizing::new(text);
            moth_platform::system_proxy::restore(&text).map_err(str::to_owned)?;
        }
        let needed = {
            let library = state.library.lock().map_err(|_| "Хранилище недоступно")?;
            library.proxy_session_requested || library.system_proxy_ownership.is_some()
        };
        if needed {
            mutate(&state, |l| {
                l.system_proxy_ownership = None;
                l.proxy_session_requested = false;
                Ok(())
            })?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|_| "Восстановление системного прокси прервано")?
}
fn text_pending(state: &AppState) -> bool {
    state
        .library
        .lock()
        .map(|l| l.system_proxy_ownership.is_some())
        .unwrap_or(true)
}
pub async fn apply_system(app: &tauri::AppHandle, port: u16) -> Result<(), String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let _gate = state
            .system_proxy_gate
            .lock()
            .map_err(|_| "Действие с прокси недоступно")?;
        if text_pending(&state) {
            return Err("Сначала восстановите прежний системный прокси".into());
        }
        let journal = zeroize::Zeroizing::new(
            moth_platform::system_proxy::prepare(port).map_err(str::to_owned)?,
        );
        mutate(&state, |l| {
            l.system_proxy_ownership = Some(journal.to_string());
            Ok(())
        })?;
        moth_platform::system_proxy::apply(&journal).map_err(str::to_owned)
    })
    .await
    .map_err(|_| "Изменение системного прокси прервано")?
}
#[tauri::command]
pub async fn restore_system_proxy(app: tauri::AppHandle) -> Result<Snapshot, String> {
    crate::disconnect_vpn(app.clone()).await?;
    crate::get_snapshot(app.state())
}
#[tauri::command]
pub async fn acknowledge_proxy_recovery(app: tauri::AppHandle) -> Result<Snapshot, String> {
    crate::tray::stop_owned().await.map_err(|e| e.to_string())?;
    let lifecycle = app.state::<crate::tray::DesktopLifecycle>();
    let _controls = lifecycle.controls.write().await;
    crate::tray::stop_owned().await.map_err(|e| e.to_string())?;
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move||{
        let state=handle.state::<AppState>();
        let _gate=state.system_proxy_gate.lock().map_err(|_|"Действие недоступно")?;
        let owned={let library=state.library.lock().map_err(|_|"Хранилище недоступно")?;moth_platform::system_proxy::status(library.system_proxy_ownership.as_deref()).map_err(str::to_owned)?.owned};
        if owned{return Err("Собственный прокси ещё установлен; сначала восстановите его или измените параметры ОС вручную".into());}
        mutate(&state,|l|{l.system_proxy_ownership=None;l.proxy_session_requested=false;Ok(())})
    }).await.map_err(|_|"Подтверждение восстановления прервано")?
}
/// Native downloads ignore environment/OS proxies. An explicit service route
/// prevents silently leaking them after proxy-only crashes or service restarts.
pub async fn native_route(
    app: &tauri::AppHandle,
    require_proxy: bool,
) -> Result<Option<moth_ipc::DownloadProxy>, String> {
    let intent = app
        .state::<AppState>()
        .library
        .lock()
        .map_err(|_| "Хранилище недоступно")?
        .proxy_session_requested;
    match moth_ipc::ServiceClient::production()
        .call(moth_ipc::Command::NativeRoute {})
        .await
    {
        Ok(reply) => match reply.result {
            moth_ipc::Reply::NativeRoute(moth_ipc::NativeRoute::PrivateProxy(p)) => {
                return Ok(Some(p))
            }
            moth_ipc::Reply::NativeRoute(moth_ipc::NativeRoute::Direct)
                if !intent && !require_proxy =>
            {
                return Ok(None)
            }
            moth_ipc::Reply::NativeRoute(_) => return Err(
                "Защищённый канал недоступен; подключитесь заново или явно отключите соединение"
                    .into(),
            ),
            _ => {}
        },
        Err(moth_ipc::IpcError::Unavailable) if !intent && !require_proxy => return Ok(None),
        Err(moth_ipc::IpcError::Protocol) => {} // Older authenticated service: legacy command below.
        Err(e) => return Err(e.to_string()),
    }
    if intent {
        return Err(
            "Для прокси-режима нужна служба 0.3.5 или новее; прямой запрос запрещён".into(),
        );
    }
    if require_proxy {
        if let Ok(reply) = moth_ipc::ServiceClient::production()
            .call(moth_ipc::Command::SubscriptionProxy {})
            .await
        {
            if let moth_ipc::Reply::SubscriptionProxy(p) = reply.result {
                return Ok(Some(p));
            }
        }
        return Err("VPN-only: защищённый канал службы недоступен; прямой запрос запрещён".into());
    }
    Ok(None)
}
pub fn reqwest_proxy(
    proxy: Option<&moth_ipc::DownloadProxy>,
) -> Result<Option<reqwest::Proxy>, String> {
    proxy
        .map(|p| {
            reqwest::Proxy::all(format!("http://127.0.0.1:{}", p.port))
                .map(|proxy| proxy.basic_auth(&p.username, &p.password))
                .map_err(|_| "Не удалось создать защищённый канал".into())
        })
        .transpose()
}
