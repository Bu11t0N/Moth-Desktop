mod security;
use crate::{Command, Health, IpcError, Reply, Request, Response, PIPE_NAME, SERVICE_NAME};
pub use security::{authorize_client, create_server, current_sid, validate_installation};
use std::time::Duration;
use tokio::{net::windows::named_pipe::NamedPipeClient, time::timeout};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{Pipes::*, Services::*},
};

pub fn development_pipe(pid: u32) -> String {
    format!(r"\\.\pipe\Moth.Desktop.Dev.{pid}")
}
pub struct ServiceClient {
    development_pid: Option<u32>,
}
impl ServiceClient {
    pub fn production() -> Self {
        Self {
            development_pid: None,
        }
    }
    /// Explicit test harness only; production GUI never selects this transport.
    pub fn development(pid: u32) -> Self {
        Self {
            development_pid: Some(pid),
        }
    }
    async fn connect(&self) -> Result<NamedPipeClient, IpcError> {
        let expected = match self.development_pid {
            Some(pid) => pid,
            None => service_pid()?,
        };
        let name = self
            .development_pid
            .map(development_pipe)
            .unwrap_or_else(|| PIPE_NAME.into());
        let wide = security::wide(&name);
        let start = std::time::Instant::now();
        let pipe = loop {
            // Identification prevents a spoof server impersonating the GUI with full rights.
            let raw = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    FILE_READ_DATA | FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    std::ptr::null_mut(),
                )
            };
            if raw != INVALID_HANDLE_VALUE {
                // SAFETY: sole owner of an overlapped named-pipe handle; Tokio takes ownership.
                break unsafe { NamedPipeClient::from_raw_handle(raw.cast()) }
                    .map_err(|_| IpcError::Unavailable)?;
            }
            let error = unsafe { GetLastError() };
            if error == ERROR_PIPE_BUSY && start.elapsed() < Duration::from_secs(2) {
                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            }
            return Err(if error == ERROR_ACCESS_DENIED {
                IpcError::AccessDenied
            } else {
                IpcError::Unavailable
            });
        };
        use std::os::windows::io::AsRawHandle;
        let mut actual = 0;
        if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle().cast(), &mut actual) } == 0
            || actual != expected
        {
            return Err(IpcError::UntrustedPeer);
        }
        match self.development_pid {
            Some(_) => {
                if security::process_sid(actual)? != current_sid()? {
                    return Err(IpcError::UntrustedPeer);
                }
            }
            None => {
                if service_pid()? != actual {
                    return Err(IpcError::UntrustedPeer);
                }
            }
        }
        Ok(pipe)
    }
    pub async fn call(&self, command: Command) -> Result<Response, IpcError> {
        let mut pipe = timeout(Duration::from_secs(3), self.connect())
            .await
            .map_err(|_| IpcError::Timeout)??;
        // Authentication completed before serializing or writing any profile/credential.
        let request = Request::new(command);
        timeout(
            Duration::from_secs(2),
            crate::write_frame(&mut pipe, &request),
        )
        .await
        .map_err(|_| IpcError::Timeout)??;
        let response: Response = timeout(Duration::from_secs(30), crate::read_frame(&mut pipe))
            .await
            .map_err(|_| IpcError::Timeout)??;
        crate::validate_response(&request, &response)?;
        Ok(response)
    }
    pub async fn health(&self) -> Result<Health, IpcError> {
        match self.call(Command::Status {}).await?.result {
            Reply::Status(health) => Ok(health),
            _ => Err(IpcError::Protocol),
        }
    }
}

/// Used by graceful GUI exit. A missing pipe does not prove that the service
/// stopped: it may still own an active tunnel while its pipe is busy/broken.
pub fn service_is_stopped() -> Result<bool, IpcError> {
    unsafe {
        let manager = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if manager.is_null() {
            return Err(IpcError::AccessDenied);
        }
        let service = OpenServiceW(
            manager,
            security::wide(SERVICE_NAME).as_ptr(),
            SERVICE_QUERY_STATUS,
        );
        let error = GetLastError();
        CloseServiceHandle(manager);
        if service.is_null() {
            return if error == ERROR_SERVICE_DOES_NOT_EXIST {
                Ok(true)
            } else {
                Err(IpcError::AccessDenied)
            };
        }
        let mut status: SERVICE_STATUS_PROCESS = std::mem::zeroed();
        let mut size = 0;
        let ok = QueryServiceStatusEx(
            service,
            SC_STATUS_PROCESS_INFO,
            (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
            std::mem::size_of_val(&status) as u32,
            &mut size,
        );
        CloseServiceHandle(service);
        if ok == 0 {
            Err(IpcError::Unavailable)
        } else {
            Ok(status.dwCurrentState == SERVICE_STOPPED)
        }
    }
}

fn service_pid() -> Result<u32, IpcError> {
    // SCM is the trust anchor; unprivileged users cannot replace its service registration.
    unsafe {
        let manager = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if manager.is_null() {
            return Err(if GetLastError() == ERROR_ACCESS_DENIED {
                IpcError::AccessDenied
            } else {
                IpcError::Unavailable
            });
        }
        let name = security::wide(SERVICE_NAME);
        let service = OpenServiceW(
            manager,
            name.as_ptr(),
            SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
        );
        let service_error = if service.is_null() { GetLastError() } else { 0 };
        CloseServiceHandle(manager);
        if service.is_null() {
            return Err(if service_error == ERROR_SERVICE_DOES_NOT_EXIST {
                IpcError::Unavailable
            } else {
                IpcError::AccessDenied
            });
        }
        let mut status: SERVICE_STATUS_PROCESS = std::mem::zeroed();
        let mut size = 0;
        let ok = QueryServiceStatusEx(
            service,
            SC_STATUS_PROCESS_INFO,
            (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
            std::mem::size_of_val(&status) as u32,
            &mut size,
        );
        if ok == 0 || status.dwCurrentState != SERVICE_RUNNING || status.dwProcessId == 0 {
            CloseServiceHandle(service);
            return Err(IpcError::Unavailable);
        }
        // QUERY_CONFIG is available to ordinary authenticated users. Opening a
        // LocalSystem process token would require SeDebugPrivilege on their GUI.
        let mut needed = 0;
        QueryServiceConfigW(service, std::ptr::null_mut(), 0, &mut needed);
        if needed == 0 || needed > 16384 {
            CloseServiceHandle(service);
            return Err(IpcError::UntrustedPeer);
        }
        let mut buffer = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        let config = buffer.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW;
        let ok = QueryServiceConfigW(service, config, needed, &mut needed);
        CloseServiceHandle(service);
        if ok == 0
            || (*config).dwServiceType != SERVICE_WIN32_OWN_PROCESS
            || (*config).lpServiceStartName.is_null()
        {
            return Err(IpcError::UntrustedPeer);
        }
        let account = (*config).lpServiceStartName;
        let mut len = 0;
        while len < 64 && *account.add(len) != 0 {
            len += 1;
        }
        if len == 64
            || !String::from_utf16_lossy(std::slice::from_raw_parts(account, len))
                .eq_ignore_ascii_case("LocalSystem")
        {
            return Err(IpcError::UntrustedPeer);
        }
        Ok(status.dwProcessId)
    }
}
