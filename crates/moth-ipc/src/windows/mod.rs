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
        let sid = security::process_sid(actual)?;
        match self.development_pid {
            Some(_) => {
                if sid != current_sid()? {
                    return Err(IpcError::UntrustedPeer);
                }
            }
            None => {
                if sid != "S-1-5-18" || service_pid()? != actual {
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
        let response: Response = timeout(Duration::from_secs(25), crate::read_frame(&mut pipe))
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

fn service_pid() -> Result<u32, IpcError> {
    // SCM is the trust anchor; unprivileged users cannot replace its service registration.
    unsafe {
        let manager = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if manager.is_null() {
            return Err(IpcError::Unavailable);
        }
        let name = security::wide(SERVICE_NAME);
        let service = OpenServiceW(manager, name.as_ptr(), SERVICE_QUERY_STATUS);
        CloseServiceHandle(manager);
        if service.is_null() {
            return Err(IpcError::Unavailable);
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
        if ok == 0 || status.dwCurrentState != SERVICE_RUNNING || status.dwProcessId == 0 {
            return Err(IpcError::Unavailable);
        }
        Ok(status.dwProcessId)
    }
}
