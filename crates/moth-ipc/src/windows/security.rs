use crate::IpcError;
use std::{
    os::windows::{fs::MetadataExt, io::AsRawHandle},
    path::Path,
};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    System::{Pipes::*, Threading::*},
};

pub(super) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Local(*mut core::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn token_info(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Result<Vec<usize>, IpcError> {
    unsafe {
        let mut len = 0;
        GetTokenInformation(token, class, std::ptr::null_mut(), 0, &mut len);
        if len == 0 {
            return Err(IpcError::AccessDenied);
        }
        let mut buffer = vec![0usize; (len as usize).div_ceil(std::mem::size_of::<usize>())];
        if GetTokenInformation(token, class, buffer.as_mut_ptr().cast(), len, &mut len) == 0 {
            return Err(IpcError::AccessDenied);
        }
        Ok(buffer)
    }
}
fn sid_text(sid: PSID) -> Result<String, IpcError> {
    unsafe {
        let mut ptr = std::ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut ptr) == 0 {
            return Err(IpcError::AccessDenied);
        }
        let _memory = Local(ptr.cast());
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        Ok(String::from_utf16_lossy(std::slice::from_raw_parts(
            ptr, len,
        )))
    }
}
fn user_sid(token: HANDLE) -> Result<String, IpcError> {
    let data = token_info(token, TokenUser)?;
    unsafe { sid_text((*(data.as_ptr() as *const TOKEN_USER)).User.Sid) }
}
pub fn current_sid() -> Result<String, IpcError> {
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(IpcError::AccessDenied);
        }
        let token = Handle(token);
        user_sid(token.0)
    }
}
pub(super) fn process_sid(pid: u32) -> Result<String, IpcError> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return Err(IpcError::UntrustedPeer);
        }
        let process = Handle(process);
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(process.0, TOKEN_QUERY, &mut token) == 0 {
            return Err(IpcError::UntrustedPeer);
        }
        let token = Handle(token);
        user_sid(token.0)
    }
}
fn member(token: HANDLE, text: &str) -> Result<bool, IpcError> {
    unsafe {
        let mut sid = std::ptr::null_mut();
        if ConvertStringSidToSidW(wide(text).as_ptr(), &mut sid) == 0 {
            return Err(IpcError::AccessDenied);
        }
        let _memory = Local(sid);
        let mut yes = 0;
        if CheckTokenMembership(token, sid, &mut yes) == 0 {
            return Err(IpcError::AccessDenied);
        }
        Ok(yes != 0)
    }
}

pub fn authorize_client(
    pipe: &NamedPipeServer,
    development_sid: Option<&str>,
) -> Result<String, IpcError> {
    // Synchronous scope only: no await/thread migration while impersonating.
    unsafe {
        if ImpersonateNamedPipeClient(pipe.as_raw_handle().cast()) == 0 {
            return Err(IpcError::AccessDenied);
        }
        let result = (|| {
            let mut token = std::ptr::null_mut();
            if OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) == 0 {
                return Err(IpcError::AccessDenied);
            }
            let token = Handle(token);
            let app = token_info(token.0, TokenIsAppContainer)?;
            if *(app.as_ptr() as *const u32) != 0 {
                return Err(IpcError::AccessDenied);
            }
            let integrity = token_info(token.0, TokenIntegrityLevel)?;
            let sid = (*(integrity.as_ptr() as *const TOKEN_MANDATORY_LABEL))
                .Label
                .Sid;
            let count = *GetSidSubAuthorityCount(sid);
            if count == 0 || *GetSidSubAuthority(sid, (count - 1) as u32) < 0x2000 {
                return Err(IpcError::AccessDenied);
            }
            if let Some(expected) = development_sid {
                if user_sid(token.0)? != expected {
                    return Err(IpcError::AccessDenied);
                }
            } else if !member(token.0, "S-1-5-4")? && !member(token.0, "S-1-5-32-544")? {
                return Err(IpcError::AccessDenied);
            }
            user_sid(token.0)
        })();
        // Continuing on a thread with an unknown security context is forbidden.
        if RevertToSelf() == 0 {
            std::process::abort();
        }
        result
    }
}

pub fn create_server(
    name: &str,
    first: bool,
    development_sid: Option<&str>,
) -> Result<NamedPipeServer, IpcError> {
    let acl = if let Some(sid) = development_sid {
        format!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{sid})")
    } else {
        "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x00100083;;;IU)".into()
    };
    unsafe {
        let mut descriptor = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide(&acl).as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(IpcError::AccessDenied);
        }
        let _memory = Local(descriptor);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .max_instances(16)
            .create_with_security_attributes_raw(
                name,
                (&attributes as *const SECURITY_ATTRIBUTES)
                    .cast_mut()
                    .cast(),
            )
            .map_err(|_| IpcError::AccessDenied)
    }
}

/// Fail closed before a privileged service executes anything from its installation.
/// Installer uses this exact fixed Program Files layout and administrator-owned ACLs.
pub fn validate_installation(root: &Path) -> Result<(), IpcError> {
    let program_files = std::env::var_os("ProgramFiles").ok_or(IpcError::AccessDenied)?;
    let expected = std::path::PathBuf::from(program_files).join("Moth Desktop");
    if std::fs::symlink_metadata(&expected)
        .map_err(|_| IpcError::AccessDenied)?
        .file_attributes()
        & 0x400
        != 0
    {
        return Err(IpcError::AccessDenied);
    }
    let canonical = std::fs::canonicalize(root).map_err(|_| IpcError::AccessDenied)?;
    if canonical != std::fs::canonicalize(&expected).map_err(|_| IpcError::AccessDenied)? {
        return Err(IpcError::AccessDenied);
    }
    for relative in [
        "",
        "engines",
        "moth-service.exe",
        "engines/xray.exe",
        "engines/sing-box.exe",
    ] {
        let path = root.join(relative);
        let meta = std::fs::symlink_metadata(&path).map_err(|_| IpcError::AccessDenied)?;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(IpcError::AccessDenied);
        }
        check_acl(&path)?;
    }
    Ok(())
}
fn check_acl(path: &Path) -> Result<(), IpcError> {
    const WRITE_RIGHTS: u32 = 0x0002
        | 0x0004
        | 0x0010
        | 0x0040
        | 0x0100
        | 0x10000
        | 0x40000
        | 0x80000
        | 0x40000000
        | 0x10000000;
    unsafe {
        let name: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut owner = std::ptr::null_mut();
        let mut dacl = std::ptr::null_mut();
        let mut descriptor = std::ptr::null_mut();
        if GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        ) != 0
        {
            return Err(IpcError::AccessDenied);
        }
        let _memory = Local(descriptor);
        if !matches!(sid_text(owner)?.as_str(), "S-1-5-18" | "S-1-5-32-544") || dacl.is_null() {
            return Err(IpcError::AccessDenied);
        }
        for index in 0..(*dacl).AceCount {
            let mut ptr = std::ptr::null_mut();
            if GetAce(dacl, index as u32, &mut ptr) == 0 {
                return Err(IpcError::AccessDenied);
            }
            let header = &*(ptr as *const ACE_HEADER);
            if header.AceFlags as u32 & INHERIT_ONLY_ACE != 0 {
                continue;
            }
            if header.AceType as u32
                == windows_sys::Win32::System::SystemServices::ACCESS_DENIED_ACE_TYPE
            {
                continue;
            }
            if header.AceType as u32
                != windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE
            {
                return Err(IpcError::AccessDenied);
            }
            let ace = &*(ptr as *const ACCESS_ALLOWED_ACE);
            if ace.Mask & WRITE_RIGHTS != 0 {
                let sid = sid_text((&ace.SidStart as *const u32).cast_mut().cast())?;
                if !matches!(sid.as_str(), "S-1-5-18" | "S-1-5-32-544") {
                    return Err(IpcError::AccessDenied);
                }
            }
        }
        Ok(())
    }
}
