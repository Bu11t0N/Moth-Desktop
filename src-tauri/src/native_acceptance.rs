//! Debug-only native lifecycle driver for disposable Windows/Linux CI VMs.
//! No production flag, authentication bypass, fake state or host test routing.
use std::{io::Read, path::PathBuf, sync::atomic::Ordering, time::Duration};
use tauri::Manager;

fn enabled() -> bool {
    std::env::var("MOTH_NATIVE_TRAY_TEST").as_deref() == Ok("1")
        && ((std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
            && matches!(
                std::env::var("RUNNER_OS").as_deref(),
                Ok("Windows" | "Linux")
            ))
            || (cfg!(windows) && std::env::var("MOTH_DISPOSABLE_VM").as_deref() == Ok("1")))
}
fn output() -> PathBuf {
    PathBuf::from(std::env::var_os("MOTH_DEV_DATA_DIR").expect("isolated test data directory"))
        .join("native-tray-result.json")
}
pub fn record_exit() {
    if enabled() {
        assert!(
            output().is_file(),
            "native lifecycle assertions must finish before exit"
        );
        let fallback =
            cfg!(target_os = "linux") && std::env::var("MOTH_NATIVE_NO_TRAY").as_deref() == Ok("1");
        std::fs::write(output(),format!("{{\"trayRegistered\":{},\"trayFallback\":{},\"closeKeptVpn\":true,\"secondLaunchRestored\":true,\"exitDisconnected\":true}}",!fallback,fallback)).expect("write native evidence");
    }
}
async fn visible(window: &tauri::WebviewWindow, wanted: bool) -> Result<(), &'static str> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if window.is_visible().ok() == Some(wanted) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "window visibility did not settle")
}
async fn exercise(app: &tauri::AppHandle) -> Result<(), &'static str> {
    let fallback =
        cfg!(target_os = "linux") && std::env::var("MOTH_NATIVE_NO_TRAY").as_deref() == Ok("1");
    #[cfg(target_os = "linux")]
    tokio::time::timeout(Duration::from_secs(10), async {
        while app
            .state::<super::tray::DesktopLifecycle>()
            .tray_available
            .load(Ordering::Acquire)
            == fallback
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "real tray host availability did not match the acceptance session")?;
    if !fallback && app.tray_by_id("moth-tray").is_none() {
        return Err("native tray not registered");
    }
    let mut link = String::new();
    std::io::stdin()
        .take(16385)
        .read_to_string(&mut link)
        .map_err(|_| "stdin fixture input failed")?;
    let profiles = moth_core::import_links(&link).map_err(|_| "invalid fixture link")?;
    if link.len() > 16384 || profiles.len() != 1 || profiles[0].host != "127.0.0.1" {
        return Err("fixture must be a local-only profile");
    }
    let handle = app.clone();
    let snapshot =
        tauri::async_runtime::spawn_blocking(move || super::import_servers(link, handle.state()))
            .await
            .map_err(|_| "native import task failed")?
            .map_err(|_| "native import failed")?;
    let id = snapshot.selected.ok_or("fixture selection missing")?;
    let state = super::connect_vpn(id, app.state(), app.clone())
        .await
        .map_err(|_| "native connect command failed")?;
    if state.phase != moth_ipc::TunnelPhase::Connected || !state.owned_by_caller {
        return Err("native connect did not verify real TUN");
    }
    let window = app
        .get_webview_window("main")
        .ok_or("native window missing")?;
    window.close().map_err(|_| "native close request failed")?;
    close_state(&window, fallback).await?;
    if app.get_webview_window("main").is_none() {
        return Err("close destroyed webview");
    }
    let state = super::tunnel_status(app.clone())
        .await
        .map_err(|_| "service status unavailable after close")?;
    if state.phase != moth_ipc::TunnelPhase::Connected || !state.owned_by_caller {
        return Err("window close interrupted VPN");
    }
    // Real second process: the official plugin must exit it and restore this window.
    let mut child = tokio::process::Command::new(
        std::env::current_exe().map_err(|_| "current exe unavailable")?,
    )
    .env_remove("MOTH_NATIVE_TRAY_TEST")
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .kill_on_drop(true)
    .spawn()
    .map_err(|_| "second instance failed to launch")?;
    let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .map_err(|_| "second instance remained running")?
        .map_err(|_| "second instance wait failed")?;
    if !status.success() {
        return Err("second instance did not exit cleanly");
    }
    visible(&window, true).await?;
    window.close().map_err(|_| "second close request failed")?;
    close_state(&window, fallback).await?;
    std::fs::write(output(),r#"{"trayRegistered":true,"closeKeptVpn":true,"secondLaunchRestored":true,"exitDisconnected":false}"#).map_err(|_|"native evidence write failed")?;
    // The actual menu handler; graceful exit records evidence only after stop confirmation.
    if fallback {
        super::quit_app(app.clone());
    } else {
        super::tray::menu_action(app, "moth-quit");
    }
    Ok(())
}
async fn close_state(window: &tauri::WebviewWindow, fallback: bool) -> Result<(), &'static str> {
    if fallback {
        tokio::time::timeout(Duration::from_secs(5), async {
            while window.is_minimized().ok() != Some(true) || window.is_visible().ok() != Some(true)
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| "no-tray close did not retain a minimized taskbar window")
    } else {
        visible(window, false).await
    }
}
pub fn start(app: &tauri::AppHandle) {
    if !enabled() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(reason) = exercise(&app).await {
            let _ = std::fs::write(output(), format!("{{\"error\":\"{reason}\"}}"));
            app.state::<super::tray::DesktopLifecycle>()
                .allow_exit
                .store(true, Ordering::Release);
            app.exit(1);
        }
    });
}
