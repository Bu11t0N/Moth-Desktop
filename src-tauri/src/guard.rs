//! Explicit native controls for machine-owned egress protection. No local
//! checkbox is treated as proof that kernel filtering is active.
#[tauri::command]
pub async fn egress_guard_status() -> Result<moth_ipc::GuardState, String> {
    call(moth_ipc::Command::GuardStatus {}).await
}
#[tauri::command]
pub async fn set_egress_guard(
    enabled: bool,
    app: tauri::AppHandle,
) -> Result<moth_ipc::GuardState, String> {
    use crate::diagnostics::{self, Code};
    diagnostics::record(
        &app,
        if enabled {
            Code::GuardEnable
        } else {
            Code::GuardRelease
        },
    );
    let result = call(moth_ipc::Command::SetGuard { enabled }).await;
    diagnostics::record(
        &app,
        if result.as_ref().is_ok_and(|s| !s.enabled || s.filtering) {
            Code::GuardVerified
        } else {
            Code::GuardFailed
        },
    );
    result
}
async fn call(command: moth_ipc::Command) -> Result<moth_ipc::GuardState, String> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        match moth_ipc::ServiceClient::production().call(command).await.map_err(|_|"Состояние kill switch не подтверждено. Нужна доступная служба 0.3.5 или новее")?.result {
            moth_ipc::Reply::Guard(state)=>Ok(state),
            moth_ipc::Reply::Failure{code}=>Err(match code {
                moth_ipc::ErrorCode::NotOwner=>"Kill switch включён другой учётной записью",
                moth_ipc::ErrorCode::Busy=>"Дождитесь завершения действия службы",
                moth_ipc::ErrorCode::InvalidProfile=>"Сначала подключите VPN. Отключите direct-правила и разделение приложений перед включением kill switch",
                _=>"Не удалось подтвердить правила kill switch. Блокировка может сохраняться; проверьте состояние или используйте восстановление администратора",
            }.into()),
            _=>Err("Состояние kill switch не подтверждено. Нужна служба 0.3.5 или новее".into()),
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        drop(command);
        Err("Kill switch недоступен на этой платформе".into())
    }
}
