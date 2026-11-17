use crate::{mutate, AppState};
use tauri::Manager;
fn status(state: &AppState) -> Result<moth_platform::autostart::Status, String> {
    let owner = state
        .library
        .lock()
        .map_err(|_| "Хранилище недоступно")?
        .autostart_ownership
        .clone();
    let executable = std::env::current_exe().map_err(|_| "Каталог приложения недоступен")?;
    moth_platform::autostart::status(&executable, owner.as_deref()).map_err(str::to_owned)
}
#[tauri::command]
pub fn autostart_status(
    state: tauri::State<AppState>,
) -> Result<moth_platform::autostart::Status, String> {
    status(&state)
}
fn configure(enabled: bool, state: &AppState) -> Result<moth_platform::autostart::Status, String> {
    let _gate = state
        .autostart_gate
        .lock()
        .map_err(|_| "Настройка автозапуска занята")?;
    let owner = state
        .library
        .lock()
        .map_err(|_| "Хранилище недоступно")?
        .autostart_ownership
        .clone();
    let executable = std::env::current_exe().map_err(|_| "Каталог приложения недоступен")?;
    let change = moth_platform::autostart::set(enabled, &executable, owner.as_deref())
        .map_err(str::to_owned)?;
    let next = change.ownership().map(str::to_owned);
    if let Err(error) = mutate(state, |library| {
        library.autostart_ownership = next;
        library.validate().map_err(str::to_owned)
    }) {
        let _ = change.undo();
        return Err(error);
    }
    status(state)
}
#[tauri::command]
pub fn set_autostart(
    enabled: bool,
    state: tauri::State<AppState>,
) -> Result<moth_platform::autostart::Status, String> {
    let result = configure(enabled, &state);
    use crate::diagnostics::Code;
    state.diagnostics.record(
        if result.is_err() {
            Code::AutostartFailed
        } else if enabled {
            Code::AutostartEnabled
        } else {
            Code::AutostartDisabled
        },
        None,
    );
    result
}
pub fn reconcile(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    if !state
        .library
        .lock()
        .is_ok_and(|l| l.autostart_ownership.is_some())
    {
        return;
    }
    if let Ok(status) = status(&state) {
        if status.owned
            && status.registered
            && !status.current_target
            && configure(true, &state).is_err()
        {
            state
                .diagnostics
                .record(crate::diagnostics::Code::AutostartFailed, None);
        }
    }
}
