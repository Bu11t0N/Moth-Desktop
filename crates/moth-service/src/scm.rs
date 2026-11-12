use moth_ipc::SERVICE_NAME;
use std::{ffi::OsString, time::Duration};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};
define_windows_service!(ffi_service_main, service_main);
pub fn dispatch() -> Result<(), String> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|_| "Не удалось подключиться к Windows SCM".into())
}
fn status(state: ServiceState, failed: bool) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: if failed {
            ServiceExitCode::ServiceSpecific(1)
        } else {
            ServiceExitCode::Win32(0)
        },
        checkpoint: if state == ServiceState::StartPending {
            1
        } else {
            0
        },
        wait_hint: if state == ServiceState::StartPending {
            Duration::from_secs(15)
        } else {
            Duration::ZERO
        },
        process_id: None,
    }
}
fn service_main(_arguments: Vec<OsString>) {
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let Ok(handle) =
        service_control_handler::register(SERVICE_NAME, move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = shutdown_tx.send(true);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })
    else {
        return;
    };
    let _ = handle.set_service_status(status(ServiceState::StartPending, false));
    let Ok(runtime) = tokio::runtime::Runtime::new() else {
        let _ = handle.set_service_status(status(ServiceState::Stopped, true));
        return;
    };
    let result = runtime.block_on(async {
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(crate::server::run(false, shutdown_rx, Some(ready_tx)));
        if ready_rx.await.is_ok() {
            let _ = handle.set_service_status(status(ServiceState::Running, false));
        }
        task.await.map_err(|_| ()).and_then(|r| r.map_err(|_| ()))
    });
    let _ = handle.set_service_status(status(ServiceState::Stopped, result.is_err()));
}
