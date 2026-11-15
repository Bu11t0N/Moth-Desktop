//! Debug-only real signed-update driver. A disposable CI harness performs the
//! administrator authorization; the unmodified privileged verifier still checks
//! the actual signed metadata/package. No production trust/elevation bypass.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Duration,
};
use tauri::Manager;
fn enabled() -> bool {
    std::env::var("MOTH_NATIVE_UPDATE_TEST").as_deref() == Ok("1")
        && std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
        && matches!(
            std::env::var("RUNNER_OS").as_deref(),
            Ok("Windows" | "Linux")
        )
}
fn evidence() -> PathBuf {
    std::env::var_os("MOTH_NATIVE_UPDATE_EVIDENCE")
        .expect("isolated update evidence")
        .into()
}
pub fn cache_directory() -> Option<PathBuf> {
    enabled().then(|| {
        PathBuf::from(std::env::var_os("MOTH_DEV_DATA_DIR").expect("isolated update data"))
            .join("updates")
    })
}
pub fn authorize(request: &Path) -> Option<Result<(), String>> {
    if !enabled() {
        return None;
    }
    let identifier = request
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    Some(
        fs::write(evidence().join("privileged-request"), identifier)
            .map_err(|_| "Acceptance authorization handoff failed".into()),
    )
}
fn ordinary_user() -> bool {
    #[cfg(windows)]
    {
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() == 0 }
    }
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::geteuid() != 0 }
    }
}
fn fingerprint(app: &tauri::AppHandle) -> Result<Vec<u8>, &'static str> {
    let state = app.state::<super::AppState>();
    let library = state.library.lock().map_err(|_| "library lock")?;
    if library.profiles.len() != 1 || library.selected != Some(library.profiles[0].id) {
        return Err("profile retention");
    }
    let mut bytes = serde_json::to_vec(&*library).map_err(|_| "library serialization")?;
    let digest = Sha256::digest(&bytes).to_vec();
    bytes.fill(0);
    Ok(digest)
}
async fn check(app: &tauri::AppHandle) -> Result<(), &'static str> {
    if !ordinary_user() {
        return Err("GUI must run as an ordinary user");
    }
    let directory = evidence();
    let expectation =
        std::env::var("MOTH_NATIVE_UPDATE_EXPECT").map_err(|_| "missing expected update result")?;
    let restarted = directory.join("attempted").exists();
    let empty = app
        .state::<super::AppState>()
        .library
        .lock()
        .map_err(|_| "library lock")?
        .profiles
        .is_empty();
    if empty {
        if restarted {
            return Err("restart lost profiles");
        }
        let mut text = String::new();
        std::io::stdin()
            .take(65536)
            .read_to_string(&mut text)
            .map_err(|_| "fixture stdin")?;
        let copy = app.clone();
        tauri::async_runtime::spawn_blocking(move || super::import_servers(text, copy.state()))
            .await
            .map_err(|_| "fixture import task")?
            .map_err(|_| "fixture import")?;
    }
    let digest = fingerprint(app)?;
    let data = PathBuf::from(std::env::var_os("MOTH_DEV_DATA_DIR").ok_or("isolated profile data")?);
    let original = data.join("updater-profile-fingerprint");
    if !original.exists() {
        fs::write(&original, &digest).map_err(|_| "retention fingerprint")?;
    }
    if fs::read(&original).map_err(|_| "retention fingerprint")? != digest {
        return Err("profile contents changed");
    }
    let expected = if restarted && expectation == "success" {
        "0.3.3"
    } else {
        "0.3.2"
    };
    if env!("CARGO_PKG_VERSION") != expected {
        return Err("restarted GUI version");
    }
    let health = super::service_status()
        .await
        .map_err(|_| "installed daemon health")?;
    if health.version != expected
        || health.protocol != 2
        || health.engines.len() != 2
        || !health.engines.iter().all(|engine| engine.available)
    {
        return Err("coordinated daemon/core versions");
    }
    // Both the original and restarted GUI use their retained real profile to
    // establish a daemon-owned HTTPS-verified TUN, not a simulated busy state.
    let selected = app
        .state::<super::AppState>()
        .library
        .lock()
        .map_err(|_| "library lock")?
        .selected
        .ok_or("selected profile")?;
    let connected = super::connect_vpn(selected, app.state(), app.clone())
        .await
        .map_err(|_| "real VPN before/after update")?;
    if connected.phase != moth_ipc::TunnelPhase::Connected || !connected.owned_by_caller {
        return Err("real owned VPN was not established");
    }
    if restarted {
        fs::write(directory.join("result.json"),b"{\"restarted\":true,\"ordinaryUser\":true,\"profilesUnchanged\":true,\"componentsHealthy\":true}").map_err(|_|"restart evidence")?;
        super::tray::request_action(app, true);
        return Ok(());
    }
    let checked = super::updater::check_updates(app.clone()).await;
    if expectation == "metadata-rejection" {
        if checked.err().as_deref() != Some("Подпись обновления не прошла проверку")
        {
            return Err("changed metadata was not rejected by signature verification");
        }
    } else {
        checked.map_err(|_| "signed update discovery")?;
        let downloaded = super::updater::download_update(app.clone()).await;
        if expectation == "download-rejection" {
            if downloaded.err().as_deref() != Some("Файл обновления повреждён или изменён")
            {
                return Err("changed package was not rejected by integrity verification");
            }
        } else {
            downloaded.map_err(|_| "signed package download")?;
            fs::write(directory.join("attempted"), b"1").map_err(|_| "handoff evidence")?;
            super::updater::install_update(app.clone())
                .await
                .map_err(|_| "signed package preparation")?;
            return Ok(());
        }
    }
    let tunnel = super::tunnel_status()
        .await
        .map_err(|_| "VPN after rejection")?;
    if tunnel.phase != moth_ipc::TunnelPhase::Connected || !tunnel.owned_by_caller {
        return Err("rejected update interrupted VPN");
    }
    fs::write(
        directory.join("result.json"),
        b"{\"rejected\":true,\"ordinaryUser\":true,\"profilesUnchanged\":true}",
    )
    .map_err(|_| "rejection evidence")?;
    super::tray::request_action(app, true);
    Ok(())
}
pub fn start(app: &tauri::AppHandle) {
    if !enabled() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(reason) = check(&app).await {
            let _ = fs::write(evidence().join("error.txt"), reason);
            app.state::<super::tray::DesktopLifecycle>()
                .allow_exit
                .store(true, Ordering::Release);
            app.exit(1);
        }
    });
}
pub fn start_monitor(app: &tauri::AppHandle) {
    if !enabled() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for _ in 0..1800 {
            if let Ok(status) = super::updater::update_status(app.state()) {
                if matches!(
                    status.phase.as_str(),
                    "complete" | "failed" | "recoveryFailed"
                ) {
                    let expected = std::env::var("MOTH_NATIVE_UPDATE_EXPECT").unwrap_or_default();
                    let correct = status.phase
                        == if expected == "success" {
                            "complete"
                        } else {
                            "failed"
                        };
                    if !correct {
                        let _ = fs::write(
                            evidence().join("error.txt"),
                            "unexpected installer recovery result",
                        );
                        app.exit(1);
                        return;
                    }
                    let _ = fs::write(
                        evidence().join("monitor-result.json"),
                        format!(
                            "{{\"phase\":\"{}\",\"ordinaryUser\":{}}}",
                            status.phase,
                            ordinary_user()
                        ),
                    );
                    let _ = super::updater::restart_updated_app(app.clone(), app.state());
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let _ = fs::write(
            evidence().join("error.txt"),
            "native update monitor timeout",
        );
        app.exit(1);
    });
}
