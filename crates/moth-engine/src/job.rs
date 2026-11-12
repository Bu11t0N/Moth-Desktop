/// Windows job owns every engine process. Closing the parent or dropping the job
/// kills its engines, including after an ungraceful GUI exit. No system service yet.
#[cfg(windows)]
pub struct ProcessJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for ProcessJob {}
#[cfg(windows)]
impl ProcessJob {
    pub fn attach(child: &tokio::process::Child) -> Result<Self, crate::EngineError> {
        use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
        // SAFETY: job handle is owned here, Win32 reads an initialized POD struct.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(crate::EngineError::Process);
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            );
            let process = child.raw_handle().ok_or(crate::EngineError::Process);
            let assigned = process.map(|p| AssignProcessToJobObject(handle, p.cast()));
            if configured == 0 || !matches!(assigned,Ok(value) if value!=0) {
                CloseHandle(handle);
                return Err(crate::EngineError::Process);
            }
            Ok(Self(handle))
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(not(windows))]
pub struct ProcessJob;
#[cfg(not(windows))]
impl ProcessJob {
    pub fn attach(_child: &tokio::process::Child) -> Result<Self, crate::EngineError> {
        Ok(Self)
    }
}
