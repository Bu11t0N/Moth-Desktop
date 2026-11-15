use super::*;
use std::os::windows::io::AsRawHandle;
use std::os::windows::{ffi::OsStringExt, process::CommandExt};
use windows_sys::Win32::{
    Foundation::*,
    System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
};
use windows_sys::Win32::{
    System::{Com::CoTaskMemFree, SystemInformation::GetWindowsDirectoryW},
    UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath},
};
pub(super) struct ChildGroup(HANDLE);
pub(super) fn finish(group: &ChildGroup) -> Result<()> {
    unsafe {
        if TerminateJobObject(group.0, 1) == 0 {
            return Err(());
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            if QueryInformationJobObject(
                group.0,
                JobObjectBasicAccountingInformation,
                (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                std::mem::size_of_val(&accounting) as u32,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(());
            }
            if accounting.ActiveProcesses == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for ChildGroup {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn resume(child: &std::process::Child) -> Result<()> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(());
        }
        let snapshot = ChildGroup(snapshot);
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut more = Thread32First(snapshot.0, &mut entry) != 0;
        while more {
            if entry.th32OwnerProcessID == child.id() {
                let thread = OpenThread(
                    THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
                    0,
                    entry.th32ThreadID,
                );
                if thread.is_null() {
                    return Err(());
                }
                let thread = ChildGroup(thread);
                if GetProcessIdOfThread(thread.0) != child.id()
                    || ResumeThread(thread.0) == u32::MAX
                {
                    return Err(());
                }
                return Ok(());
            }
            more = Thread32Next(snapshot.0, &mut entry) != 0;
        }
        Err(())
    }
}
pub(super) fn spawn(mut command: Command) -> Result<(std::process::Child, ChildGroup)> {
    unsafe {
        let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if handle.is_null() {
            return Err(());
        }
        let group = ChildGroup(handle);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
        {
            return Err(());
        }
        // The process cannot spawn an escaping child between creation and job
        // assignment: its initial thread remains suspended until contained.
        let mut child = command
            .creation_flags(0x08000000 | CREATE_SUSPENDED)
            .spawn()
            .map_err(|_| ())?;
        if AssignProcessToJobObject(handle, child.as_raw_handle().cast()) == 0
            || resume(&child).is_err()
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(());
        }
        Ok((child, group))
    }
}
pub(super) const PACKAGE: &str = "update.exe";
pub(super) fn validate_package(_job: &Path, package: &Path, expected: &str) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::*;
    let path: Vec<u16> = package.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(path.as_ptr(), std::ptr::null_mut());
        if size == 0 || size > 1024 * 1024 {
            return Err(());
        }
        let mut buffer = vec![0u8; size as usize];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, buffer.as_mut_ptr().cast()) == 0 {
            return Err(());
        }
        let block: Vec<u16> = "\\\0".encode_utf16().collect();
        let mut pointer = std::ptr::null_mut();
        let mut length = 0;
        if VerQueryValueW(
            buffer.as_ptr().cast(),
            block.as_ptr(),
            &mut pointer,
            &mut length,
        ) == 0
            || pointer.is_null()
            || length < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
        {
            return Err(());
        }
        let info = std::ptr::read_unaligned(pointer.cast::<VS_FIXEDFILEINFO>());
        let actual = format!(
            "{}.{}.{}",
            info.dwProductVersionMS >> 16,
            info.dwProductVersionMS & 0xffff,
            info.dwProductVersionLS >> 16
        );
        if info.dwSignature != 0xfeef04bd
            || info.dwProductVersionLS & 0xffff != 0
            || actual != expected
        {
            return Err(());
        }
    }
    Ok(())
}
fn program_files() -> Result<PathBuf> {
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
            return Err(());
        }
        let mut length = 0;
        while *pointer.add(length) != 0 {
            length += 1
        }
        let value = PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(
            pointer, length,
        )));
        CoTaskMemFree(pointer.cast());
        Ok(value)
    }
}
fn windows() -> Result<PathBuf> {
    let mut buffer = vec![0; 32768];
    let length = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(());
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(
        &buffer[..length],
    )))
}
pub(super) fn service() -> Result<PathBuf> {
    Ok(program_files()?.join("Moth Desktop/moth-service.exe"))
}
pub(super) fn environment(command: &mut Command) -> Result<()> {
    let windows = windows()?;
    command
        .env("SystemRoot", &windows)
        .env("WINDIR", &windows)
        .env("ProgramFiles", program_files()?)
        .env("PATH", windows.join("System32"))
        .creation_flags(0x08000000);
    Ok(())
}
fn helper() -> Result<PathBuf> {
    Ok(secure::state_directory()
        .map_err(|_| ())?
        .join("maintenance")
        .join(VERSION)
        .join("moth-maintenance.exe"))
}
pub(super) fn validate_self() -> Result<()> {
    let expected = helper()?;
    if std::env::current_exe().map_err(|_| ())? != expected {
        return Err(());
    }
    let state = secure::state_directory().map_err(|_| ())?;
    for path in [
        &state,
        &state.join("maintenance"),
        expected.parent().ok_or(())?,
        &expected,
    ] {
        secure::validate_protected_path(path).map_err(|_| ())?;
    }
    // Acquiring the protected lease checks the actual elevated token.
    Ok(())
}
pub(super) fn install() -> Result<()> {
    secure::initialize().map_err(|_| ())?;
    let current = std::env::current_exe().map_err(|_| ())?;
    let expected = program_files()?.join("Moth Desktop/moth-maintenance.exe");
    if current != expected {
        return Err(());
    }
    for path in [current.parent().ok_or(())?, current.as_path()] {
        secure::validate_protected_path(path).map_err(|_| ())?;
    }
    let parent = secure::state_directory()
        .map_err(|_| ())?
        .join("maintenance");
    directory(&parent)?;
    let versioned = parent.join(VERSION);
    directory(&versioned)?;
    let target = helper()?;
    if target.exists() {
        secure::validate_protected_path(&target).map_err(|_| ())?;
        // Retry of the same build cannot overwrite a running helper. Compare
        // bytes; changed same-version builds are rejected instead of replaced.
        if bytes(&current, 64 * 1024 * 1024)? != bytes(&target, 64 * 1024 * 1024)? {
            return Err(());
        }
        return Ok(());
    }
    let temporary = versioned.join(format!("{}.tmp", Uuid::new_v4()));
    copy(&current, &temporary, 64 * 1024 * 1024, false)?;
    fs::rename(temporary, target).map_err(|_| ())
}
fn wait_gui(job: &Path) -> Result<()> {
    let tasklist = windows()?.join("System32/tasklist.exe");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let output = command(
            job,
            &tasklist,
            &[
                "/FI".as_ref(),
                "IMAGENAME eq moth-desktop.exe".as_ref(),
                "/FO".as_ref(),
                "CSV".as_ref(),
                "/NH".as_ref(),
            ],
            Duration::from_secs(5),
        )?;
        if !String::from_utf8_lossy(&output)
            .to_ascii_lowercase()
            .contains("moth-desktop.exe")
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
pub(super) fn apply(
    job: &Path,
    package: &Path,
    previous: &str,
    next: &str,
    running: bool,
) -> Result<()> {
    wait_gui(job)?;
    report(job, "installing", next)?;
    let installed = command(job, package, &["/S".as_ref()], Duration::from_secs(300));
    if !children_clean() {
        report(job, "recoveryFailed", previous)?;
        return Err(());
    }
    if installed.is_ok() && healthy(job, next).is_ok() {
        return report(job, "complete", next);
    }
    // The NSIS transaction restores the complete previous GUI/service/core tree
    // and prior service state on failure. Independently confirm that recovery.
    report(job, "rollingBack", previous)?;
    if restored_previous(job, previous, running).is_ok() {
        report(job, "failed", previous)?;
    } else {
        report(job, "recoveryFailed", previous)?;
    }
    Err(())
}
