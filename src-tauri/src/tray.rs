#[cfg(windows)]
use moth_ipc::Reply;
use moth_ipc::{Command, IpcError, TunnelPhase, TunnelState};
use std::{
    future::Future,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tokio::sync::RwLock;

pub struct DesktopLifecycle {
    pub controls: RwLock<()>,
    pub busy: AtomicBool,
    pub(super) allow_exit: AtomicBool,
    tray: TrayIcon,
    status: MenuItem<tauri::Wry>,
    disconnect: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

async fn service_call(command: Command) -> Result<TunnelState, IpcError> {
    #[cfg(windows)]
    {
        match moth_ipc::windows::ServiceClient::production()
            .call(command)
            .await?
            .result
        {
            Reply::Tunnel(state) => Ok(state),
            _ => Err(IpcError::Protocol),
        }
    }
    #[cfg(not(windows))]
    {
        drop(command);
        Err(IpcError::Unavailable)
    }
}

/// No disconnect is sent for another account. Success must be confirmed by a
/// service reply; pipe errors are never silently treated as a disconnected VPN.
async fn stop_owned_with<F, Fut>(mut call: F) -> Result<(), IpcError>
where
    F: FnMut(Command) -> Fut,
    Fut: Future<Output = Result<TunnelState, IpcError>>,
{
    let state = call(Command::TunnelStatus {}).await?;
    if !state.owned_by_caller
        || matches!(state.phase, TunnelPhase::Disconnected | TunnelPhase::Failed)
    {
        return Ok(());
    }
    let stopped = call(Command::Disconnect {}).await?;
    if matches!(
        stopped.phase,
        TunnelPhase::Disconnected | TunnelPhase::Failed
    ) && !stopped.owned_by_caller
    {
        Ok(())
    } else {
        Err(IpcError::Protocol)
    }
}

async fn stop_owned() -> Result<(), IpcError> {
    match stop_owned_with(service_call).await {
        #[cfg(windows)]
        Err(IpcError::Unavailable)
            if matches!(moth_ipc::windows::service_is_stopped(), Ok(true)) =>
        {
            Ok(())
        }
        #[cfg(not(windows))]
        Err(IpcError::Unavailable) => Ok(()), // Linux has no service/tunnel yet.
        result => result,
    }
}

pub fn request_action(app: &AppHandle, exit: bool) {
    let lifecycle = app.state::<DesktopLifecycle>();
    if lifecycle.busy.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = lifecycle.disconnect.set_enabled(false);
    let _ = lifecycle.quit.set_enabled(false);
    let _ = lifecycle.status.set_text("Отключаем VPN…");
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let lifecycle = app.state::<DesktopLifecycle>();
        let result = tokio::time::timeout(Duration::from_secs(40), async {
            // Cancel any current connect, then wait for commands already in
            // flight. A second stop closes the race with a not-yet-sent connect.
            stop_owned().await?;
            let _fence = lifecycle.controls.write().await;
            stop_owned().await
        })
        .await;
        match result {
            Ok(Ok(())) if exit => {
                #[cfg(all(windows, debug_assertions))]
                crate::native_acceptance::record_exit();
                lifecycle.allow_exit.store(true, Ordering::Release);
                app.exit(0);
            }
            Ok(Ok(())) => {
                lifecycle.busy.store(false, Ordering::Release);
                refresh(&app).await;
            }
            _ => {
                lifecycle.busy.store(false, Ordering::Release);
                let _ = lifecycle.quit.set_enabled(true);
                let _ = lifecycle
                    .status
                    .set_text("Не удалось подтвердить отключение VPN");
                let _ = lifecycle
                    .tray
                    .set_tooltip(Some("Мотылёк — ошибка отключения VPN"));
                show_main(&app);
                let _=app.emit("moth-tray-error","Не удалось подтвердить отключение VPN. Moth остаётся запущен. Проверь связь со службой и повтори выход через трей.");
            }
        }
    });
}

async fn refresh(app: &AppHandle) {
    let state = service_call(Command::TunnelStatus {}).await;
    let lifecycle = app.state::<DesktopLifecycle>();
    if lifecycle.busy.load(Ordering::Acquire) {
        return;
    }
    let (text, can_disconnect) = match state {
        Ok(state) => {
            let text = match state.phase {
                TunnelPhase::Connected if !state.owned_by_caller => {
                    "VPN включён другой учётной записью"
                }
                TunnelPhase::Connected => "VPN подключён",
                TunnelPhase::Connecting => "Подключаем VPN…",
                TunnelPhase::Disconnecting => "Отключаем VPN…",
                TunnelPhase::Failed => "VPN отключён после ошибки",
                TunnelPhase::Disconnected => "VPN не подключён",
            };
            (
                text,
                state.owned_by_caller
                    && matches!(
                        state.phase,
                        TunnelPhase::Connected | TunnelPhase::Connecting
                    ),
            )
        }
        Err(_) => ("Служба VPN недоступна", false),
    };
    let _ = lifecycle.status.set_text(text);
    let _ = lifecycle
        .tray
        .set_tooltip(Some(format!("Мотылёк — {text}")));
    let _ = lifecycle.disconnect.set_enabled(can_disconnect);
    let _ = lifecycle.quit.set_enabled(true);
}

pub(super) fn menu_action(app: &AppHandle, id: &str) {
    match id {
        "moth-open" => show_main(app),
        "moth-disconnect" => request_action(app, false),
        "moth-quit" => request_action(app, true),
        _ => {}
    }
}

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "moth-open", "Открыть Мотылёк", true, None::<&str>)?;
    let status = MenuItem::with_id(
        app,
        "moth-status",
        "Проверяем состояние VPN…",
        false,
        None::<&str>,
    )?;
    let disconnect =
        MenuItem::with_id(app, "moth-disconnect", "Отключить VPN", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "moth-quit", "Выход", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &status, &disconnect, &separator, &quit])?;
    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::AssetNotFound("window icon".into()))?
        .clone();
    let tray = TrayIconBuilder::with_id("moth-tray")
        .icon(icon)
        .tooltip("Мотылёк — проверяем состояние VPN…")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| menu_action(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    app.manage(DesktopLifecycle {
        controls: RwLock::new(()),
        busy: AtomicBool::new(false),
        allow_exit: AtomicBool::new(false),
        tray,
        status,
        disconnect,
        quit,
    });
    let app = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        loop {
            refresh(&app).await;
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
    Ok(())
}

pub fn run_event(app: &AppHandle, event: tauri::RunEvent) {
    match event {
        #[cfg(all(windows, debug_assertions))]
        tauri::RunEvent::Ready => crate::native_acceptance::start(app),
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } if label == "main" => {
            api.prevent_close();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
        }
        tauri::RunEvent::ExitRequested { api, .. } => {
            if let Some(lifecycle) = app.try_state::<DesktopLifecycle>() {
                if !lifecycle.allow_exit.load(Ordering::Acquire) {
                    api.prevent_exit();
                    request_action(app, true);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn exit_disconnects_owner_and_requires_confirmation() {
        let mut calls = 0;
        stop_owned_with(|command| {
            calls += 1;
            let state = match command {
                Command::TunnelStatus {} => TunnelState {
                    phase: TunnelPhase::Connected,
                    owned_by_caller: true,
                    error: None,
                },
                Command::Disconnect {} => TunnelState::default(),
                _ => panic!("unexpected command"),
            };
            std::future::ready(Ok(state))
        })
        .await
        .unwrap();
        assert_eq!(calls, 2);
        assert!(stop_owned_with(|_| std::future::ready(Ok(TunnelState {
            phase: TunnelPhase::Connected,
            owned_by_caller: true,
            error: None
        })))
        .await
        .is_err());
    }
    #[tokio::test]
    async fn exit_leaves_other_accounts_alone_and_fails_closed_on_pipe_errors() {
        let mut calls = 0;
        stop_owned_with(|_| {
            calls += 1;
            std::future::ready(Ok(TunnelState {
                phase: TunnelPhase::Connected,
                owned_by_caller: false,
                error: None,
            }))
        })
        .await
        .unwrap();
        assert_eq!(calls, 1);
        assert!(
            stop_owned_with(|_| std::future::ready(Err(IpcError::Unavailable)))
                .await
                .is_err()
        );
    }
}
