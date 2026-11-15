use crate::IpcError;
use std::{
    fs::File,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
        io::FromRawHandle,
    },
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::Com::CoTaskMemFree,
    UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath},
};

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
struct Local(*mut core::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn admin() -> Result<(), IpcError> {
    unsafe {
        let text: Vec<u16> = "S-1-5-32-544".encode_utf16().chain(Some(0)).collect();
        let mut sid = std::ptr::null_mut();
        if ConvertStringSidToSidW(text.as_ptr(), &mut sid) == 0 {
            return Err(IpcError::AccessDenied);
        }
        let _memory = Local(sid);
        let mut member = 0;
        if CheckTokenMembership(std::ptr::null_mut(), sid, &mut member) == 0 || member == 0 {
            return Err(IpcError::AccessDenied);
        }
        Ok(())
    }
}
pub(super) fn directory() -> Result<PathBuf, IpcError> {
    unsafe {
        let mut pointer = std::ptr::null_mut();
        if SHGetKnownFolderPath(&FOLDERID_ProgramData, 0, std::ptr::null_mut(), &mut pointer) < 0
            || pointer.is_null()
        {
            return Err(IpcError::Unavailable);
        }
        let mut length = 0;
        while *pointer.add(length) != 0 {
            length += 1;
        }
        let path = PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(
            pointer, length,
        )));
        CoTaskMemFree(pointer.cast());
        Ok(path.join("Moth Desktop"))
    }
}
fn validate_directory() -> Result<(), IpcError> {
    let directory = directory()?;
    let parent = directory.parent().ok_or(IpcError::UntrustedPeer)?;
    crate::windows::check_parent_acl(parent)?;
    for path in [parent, directory.as_path()] {
        let metadata = std::fs::symlink_metadata(path).map_err(|_| IpcError::Unavailable)?;
        if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(IpcError::UntrustedPeer);
        }
    }
    crate::windows::check_acl(&directory)
}
fn attributes(sddl: &str) -> Result<(Local, SECURITY_ATTRIBUTES), IpcError> {
    unsafe {
        let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(IpcError::AccessDenied);
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        Ok((Local(descriptor), attributes))
    }
}
pub(super) fn validate_path(path: &Path) -> Result<(), IpcError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| IpcError::Unavailable)?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(IpcError::UntrustedPeer);
    }
    crate::windows::check_acl(path)
}
pub(super) fn create_directory(path: &Path) -> Result<(), IpcError> {
    admin()?;
    validate_path(path.parent().ok_or(IpcError::UntrustedPeer)?)?;
    let (_memory, attributes) =
        attributes("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FRFX;;;BU)")?;
    unsafe {
        if CreateDirectoryW(wide(path).as_ptr(), &attributes) == 0 {
            return Err(IpcError::AccessDenied);
        }
    }
    validate_path(path)
}
pub(super) fn create_file(path: &Path, readable: bool) -> Result<File, IpcError> {
    admin()?;
    validate_path(path.parent().ok_or(IpcError::UntrustedPeer)?)?;
    let (_memory, attributes) = attributes(if readable {
        "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;BU)"
    } else {
        "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)"
    })?;
    unsafe {
        let handle = CreateFileW(
            wide(path).as_ptr(),
            FILE_GENERIC_WRITE,
            0,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(IpcError::AccessDenied);
        }
        Ok(File::from_raw_handle(handle.cast()))
    }
}
pub(super) fn initialize() -> Result<(), IpcError> {
    admin()?;
    let directory = directory()?;
    crate::windows::check_parent_acl(directory.parent().ok_or(IpcError::UntrustedPeer)?)?;
    let (_memory, attributes) =
        attributes("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FRFX;;;BU)")?;
    unsafe {
        if CreateDirectoryW(wide(&directory).as_ptr(), &attributes) == 0
            && GetLastError() != ERROR_ALREADY_EXISTS
        {
            return Err(IpcError::AccessDenied);
        }
    }
    validate_directory()?;
    let path = directory.join("maintenance.lock");
    let (_memory, attributes) = self::attributes("O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)")?;
    unsafe {
        let handle = CreateFileW(
            wide(&path).as_ptr(),
            FILE_GENERIC_READ,
            FILE_SHARE_READ,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            if !matches!(GetLastError(), ERROR_FILE_EXISTS | ERROR_ALREADY_EXISTS) {
                return Err(IpcError::AccessDenied);
            }
        } else {
            CloseHandle(handle);
        }
    }
    crate::windows::check_acl(&path)?;
    Ok(())
}
pub(super) fn open(exclusive: bool) -> Result<File, IpcError> {
    admin()?;
    validate_directory()?;
    let path = directory()?.join("maintenance.lock");
    crate::windows::check_acl(&path)?;
    unsafe {
        let handle = CreateFileW(
            wide(&path).as_ptr(),
            FILE_GENERIC_READ,
            if exclusive { 0 } else { FILE_SHARE_READ },
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(IpcError::AccessDenied);
        }
        let file = File::from_raw_handle(handle.cast());
        let metadata = file.metadata().map_err(|_| IpcError::UntrustedPeer)?;
        let mut information = std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>();
        if !metadata.is_file()
            || metadata.len() != 0
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || GetFileInformationByHandle(handle, &mut information) == 0
            || information.nNumberOfLinks != 1
        {
            return Err(IpcError::UntrustedPeer);
        }
        Ok(file)
    }
}
