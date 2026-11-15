use moth_update::{UpdateSummary, VerifiedUpdate};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub(super) phase: String,
    update: Option<UpdateSummary>,
    downloaded: u64,
    total: u64,
    error: Option<String>,
}
#[derive(Default)]
struct Inner {
    status: Status,
    update: Option<VerifiedUpdate>,
    package: Option<PathBuf>,
}
#[derive(Default)]
pub struct Updater {
    inner: Mutex<Inner>,
    operation: tokio::sync::Mutex<()>,
    monitor: Option<Uuid>,
    restarting: std::sync::atomic::AtomicBool,
}
fn status(app: &AppHandle, change: impl FnOnce(&mut Status)) {
    if let Ok(mut inner) = app.state::<Updater>().inner.lock() {
        change(&mut inner.status);
        let _ = app.emit("moth-update-progress", inner.status.clone());
    }
}
fn error(app: &AppHandle, message: String) -> String {
    status(app, |status| {
        status.phase = "error".into();
        status.error = Some(message.clone())
    });
    message
}
pub fn monitor_argument() -> Result<Option<Uuid>, String> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|argument| argument == "--update-monitor")
    {
        if arguments.len() != 2 {
            return Err("Некорректные параметры обновления".into());
        }
        return Uuid::parse_str(&arguments[1])
            .map(Some)
            .map_err(|_| "Некорректные параметры обновления".into());
    }
    Ok(None)
}
pub fn state(monitor: Option<Uuid>) -> Updater {
    Updater {
        monitor,
        ..Default::default()
    }
}
#[tauri::command]
pub fn update_monitor_mode(state: tauri::State<Updater>) -> bool {
    state.monitor.is_some()
}
fn root_job(id: Uuid) -> Result<PathBuf, String> {
    let state =
        moth_ipc::maintenance::state_directory().map_err(|_| "Состояние обновления недоступно")?;
    let jobs = state.join("updates");
    let job = jobs.join(id.to_string());
    for path in [&state, &jobs, &job] {
        moth_ipc::maintenance::validate_protected_path(path)
            .map_err(|_| "Состояние обновления ещё не создано")?;
    }
    Ok(job)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    phase: String,
    version: String,
}
fn root_status(id: Uuid) -> Result<Report, String> {
    let path = root_job(id)?.join("status.json");
    moth_ipc::maintenance::validate_protected_path(&path)
        .map_err(|_| "Состояние обновления недоступно")?;
    let metadata = fs::symlink_metadata(&path).map_err(|_| "Состояние обновления недоступно")?;
    if !metadata.is_file() || metadata.len() > 4096 {
        return Err("Некорректное состояние обновления".into());
    }
    let report: Report =
        serde_json::from_slice(&fs::read(path).map_err(|_| "Состояние обновления недоступно")?)
            .map_err(|_| "Некорректное состояние обновления")?;
    if ![
        "verifying",
        "preparing",
        "installing",
        "rollingBack",
        "complete",
        "failed",
        "recoveryFailed",
    ]
    .contains(&report.phase.as_str())
        || report.version.len() > 64
    {
        return Err("Некорректное состояние обновления".into());
    }
    Ok(report)
}
#[tauri::command]
pub fn update_status(state: tauri::State<Updater>) -> Result<Status, String> {
    if let Some(id) = state.monitor {
        let report = root_status(id)?;
        return Ok(Status {
            phase: report.phase,
            update: Some(UpdateSummary {
                version: report.version,
                notes: String::new(),
                file: String::new(),
                size: 0,
            }),
            ..Default::default()
        });
    }
    state
        .inner
        .lock()
        .map(|inner| inner.status.clone())
        .map_err(|_| "Состояние обновления недоступно".into())
}
#[tauri::command]
pub async fn check_updates(app: AppHandle) -> Result<Status, String> {
    let updater = app.state::<Updater>();
    if updater.monitor.is_some() {
        return Err("Дождитесь завершения установки".into());
    }
    let _guard = updater
        .operation
        .try_lock()
        .map_err(|_| "Дождитесь завершения действия")?;
    status(&app, |status| {
        status.phase = "checking".into();
        status.error = None
    });
    match moth_update::check(env!("CARGO_PKG_VERSION")).await {
        Ok(update) => {
            let summary = update
                .as_ref()
                .map(VerifiedUpdate::summary)
                .transpose()
                .map_err(|e| error(&app, e.to_string()))?;
            let mut inner = updater
                .inner
                .lock()
                .map_err(|_| "Состояние обновления недоступно")?;
            inner.update = update;
            inner.package = None;
            inner.status = Status {
                phase: if summary.is_some() {
                    "available"
                } else {
                    "current"
                }
                .into(),
                update: summary,
                ..Default::default()
            };
            Ok(inner.status.clone())
        }
        Err(e) => Err(error(&app, e.to_string())),
    }
}
fn cache_directory(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    let test_cache = crate::updater_acceptance::cache_directory();
    #[cfg(not(debug_assertions))]
    let test_cache: Option<PathBuf> = None;
    let cache = match test_cache {
        Some(path) => path,
        None => app
            .path()
            .app_cache_dir()
            .map_err(|_| "Папка обновления недоступна")?
            .join("updates"),
    };
    fs::create_dir_all(&cache).map_err(|_| "Не удалось создать папку обновления")?;
    let request = cache.join(Uuid::new_v4().to_string());
    fs::create_dir(&request).map_err(|_| "Не удалось создать папку обновления")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&request, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Не удалось защитить папку обновления")?;
    }
    Ok(request)
}
#[tauri::command]
pub async fn download_update(app: AppHandle) -> Result<Status, String> {
    let updater = app.state::<Updater>();
    if updater.monitor.is_some() {
        return Err("Дождитесь завершения установки".into());
    }
    let _guard = updater
        .operation
        .try_lock()
        .map_err(|_| "Дождитесь завершения действия")?;
    let update = updater
        .inner
        .lock()
        .map_err(|_| "Состояние обновления недоступно")?
        .update
        .clone()
        .ok_or("Сначала проверьте обновления")?;
    let directory = cache_directory(&app)?;
    let package = directory.join("package.bin");
    status(&app, |status| {
        status.phase = "downloading".into();
        status.error = None;
        status.downloaded = 0
    });
    if let Err(e) = update
        .download(&package, |downloaded, total| {
            status(&app, |status| {
                status.downloaded = downloaded;
                status.total = total
            })
        })
        .await
    {
        return Err(error(&app, e.to_string()));
    }
    updater
        .inner
        .lock()
        .map_err(|_| "Состояние обновления недоступно")?
        .package = Some(package);
    status(&app, |status| status.phase = "ready".into());
    update_status(updater.clone())
}
#[cfg(windows)]
fn installed_gui() -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath},
    };
    unsafe {
        let mut pointer = std::ptr::null_mut();
        if SHGetKnownFolderPath(
            &FOLDERID_ProgramFiles,
            0,
            std::ptr::null_mut(),
            &mut pointer,
        ) < 0
            || pointer.is_null()
        {
            return Err("Установленное приложение недоступно".into());
        }
        let mut length = 0;
        while *pointer.add(length) != 0 {
            length += 1
        }
        let path = PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(
            pointer, length,
        )));
        CoTaskMemFree(pointer.cast());
        Ok(path.join("Moth Desktop/moth-desktop.exe"))
    }
}
#[cfg(target_os = "linux")]
fn installed_gui() -> Result<PathBuf, String> {
    Ok("/usr/bin/moth-desktop".into())
}
fn helper() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let path = moth_ipc::maintenance::state_directory()
        .map_err(|_| "Updater недоступен")?
        .join("maintenance")
        .join(env!("CARGO_PKG_VERSION"))
        .join("moth-maintenance.exe");
    #[cfg(target_os = "linux")]
    let path = PathBuf::from("/usr/lib/moth-desktop/moth-maintenance");
    moth_ipc::maintenance::validate_protected_path(&path)
        .map_err(|_| "Установите полный пакет Moth для обновления из приложения")?;
    Ok(path)
}
#[cfg(windows)]
fn elevate(helper: &Path, request: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW},
    };
    let file: Vec<u16> = helper.as_os_str().encode_wide().chain(Some(0)).collect();
    let arguments = format!("--apply \"{}\"", request.display());
    let arguments: Vec<u16> = arguments.encode_utf16().chain(Some(0)).collect();
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of_val(&info) as u32;
        info.fMask = 0x40;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = arguments.as_ptr();
        info.nShow = 0;
        if ShellExecuteExW(&mut info) == 0 {
            return Err("Установка не разрешена или не удалось запустить updater".into());
        }
        if !info.hProcess.is_null() {
            CloseHandle(info.hProcess);
        }
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn elevate(helper: &Path, request: &Path) -> Result<(), String> {
    std::process::Command::new("/usr/bin/pkexec")
        .arg(helper)
        .arg("--apply")
        .arg(request)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Не удалось запросить разрешение на установку".into())
}
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.state::<Updater>();
    if updater.monitor.is_some() {
        return Err("Установка уже выполняется".into());
    }
    let _operation = updater
        .operation
        .try_lock()
        .map_err(|_| "Дождитесь завершения действия")?;
    let installed = installed_gui()?;
    if std::env::current_exe().map_err(|_| "Не удалось проверить установленное приложение")?
        != installed
    {
        return Err("Обновление доступно после установки полного пакета Moth".into());
    }
    let helper = helper()?;
    let (update, package) = {
        let inner = updater
            .inner
            .lock()
            .map_err(|_| "Состояние обновления недоступно")?;
        (
            inner.update.clone().ok_or("Сначала проверьте обновления")?,
            inner.package.clone().ok_or("Сначала скачайте обновление")?,
        )
    };
    update
        .verify_package(&package)
        .map_err(|e| error(&app, e.to_string()))?;
    let request = cache_directory(&app)?;
    fs::write(request.join("manifest.json"), update.manifest_bytes())
        .map_err(|_| "Не удалось подготовить обновление")?;
    fs::write(request.join("manifest.sig"), update.manifest_signature())
        .map_err(|_| "Не удалось подготовить обновление")?;
    fs::copy(package, request.join("package.bin"))
        .map_err(|_| "Не удалось подготовить обновление")?;
    let id = Uuid::parse_str(
        request
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Некорректная папка обновления")?,
    )
    .map_err(|_| "Некорректная папка обновления")?;
    let lifecycle = app.state::<crate::tray::DesktopLifecycle>();
    if lifecycle.busy.swap(true, Ordering::AcqRel) {
        return Err("Дождитесь завершения отключения VPN".into());
    }
    let _controls = lifecycle.controls.write().await;
    let result = async {
        status(&app, |status| {
            status.phase = "authorizing".into();
            status.error = None
        });
        crate::tray::stop_owned()
            .await
            .map_err(|_| "Не удалось подтвердить отключение VPN")?;
        tauri::async_runtime::spawn_blocking(move || {
            #[cfg(debug_assertions)]
            if let Some(result) = crate::updater_acceptance::authorize(&request) {
                return result;
            }
            elevate(&helper, &request)
        })
        .await
        .map_err(|_| "Не удалось запустить updater")??;
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if let Ok(report) = root_status(id) {
                match report.phase.as_str() {
                    "preparing" => break,
                    "failed" | "recoveryFailed" => {
                        return Err(
                            "Updater отклонил обновление; установленная версия сохранена".into(),
                        )
                    }
                    _ => {}
                }
            }
            if Instant::now() >= deadline {
                return Err(
                    "Updater не подтвердил готовность. Приложение остаётся открытым".into(),
                );
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let directory = cache_directory(&app)?;
        let monitor = directory.join(if cfg!(windows) {
            "moth-monitor.exe"
        } else {
            "moth-monitor"
        });
        fs::copy(&installed, &monitor).map_err(|_| "Не удалось открыть окно установки")?;
        let mut command = std::process::Command::new(monitor);
        command.arg("--update-monitor").arg(id.to_string());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
            .spawn()
            .map_err(|_| "Не удалось открыть окно установки")?;
        lifecycle.allow_exit.store(true, Ordering::Release);
        app.exit(0);
        Ok(())
    }
    .await;
    if let Err(message) = result {
        lifecycle.busy.store(false, Ordering::Release);
        return Err(error(&app, message));
    }
    Ok(())
}
#[tauri::command]
pub fn restart_updated_app(app: AppHandle, state: tauri::State<Updater>) -> Result<(), String> {
    let id = state.monitor.ok_or("Окно установки недоступно")?;
    if !matches!(root_status(id)?.phase.as_str(), "complete" | "failed") {
        return Err("Дождитесь завершения установки или восстановления".into());
    }
    if state.restarting.swap(true, Ordering::AcqRel) {
        return Err("Перезапуск уже выполняется".into());
    }
    let result = (|| {
        let path = installed_gui()?;
        moth_ipc::maintenance::validate_protected_path(&path)
            .map_err(|_| "Установленное приложение недоступно")?;
        let mut command = std::process::Command::new(path);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
            .spawn()
            .map_err(|_| "Не удалось открыть приложение")?;
        app.exit(0);
        Ok(())
    })();
    if result.is_err() {
        state.restarting.store(false, Ordering::Release);
    }
    result
}
