//! File identity is independent of the spelling accepted by Shell parsers.
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_FUNCTION, ERROR_NOT_SUPPORTED, HANDLE},
        Storage::FileSystem::*,
    },
    core::{Error, PCWSTR, Result},
};
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Identity {
    pub volume: u64,
    pub file: [u8; 16],
    pub created: u64,
    pub wide_id: bool,
}
#[derive(serde::Serialize)]
pub struct Metadata {
    pub identity: Identity,
    pub directory: bool,
    pub bytes: Option<u64>,
    pub reparse: bool,
    pub modified: u64,
    pub modified_raw: u64,
    pub attributes: u32,
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub fn capture(path: &Path) -> Result<Metadata> {
    let text: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if text[..text.len() - 1].contains(&0) {
        return Err(Error::from_hresult(
            windows::Win32::Foundation::E_INVALIDARG,
        ));
    }
    unsafe {
        let handle = Handle(CreateFileW(
            PCWSTR(text.as_ptr()),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )?);
        capture_handle(handle.0)
    }
}
/// # Safety
/// The caller must keep a valid readable file/directory handle alive for this call.
pub unsafe fn capture_handle(handle: HANDLE) -> Result<Metadata> {
    unsafe {
        let mut basic = BY_HANDLE_FILE_INFORMATION::default();
        GetFileInformationByHandle(handle, &mut basic)?;
        let mut extended = FILE_ID_INFO::default();
        let result = GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut extended as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        );
        let (volume, file, wide_id) = match result {
            Ok(()) => (
                extended.VolumeSerialNumber,
                extended.FileId.Identifier,
                true,
            ),
            Err(error)
                if error.code() == windows::core::HRESULT::from_win32(ERROR_NOT_SUPPORTED.0)
                    || error.code()
                        == windows::core::HRESULT::from_win32(ERROR_INVALID_FUNCTION.0) =>
            {
                let mut id = [0; 16];
                id[..8].copy_from_slice(
                    &((basic.nFileIndexHigh as u64) << 32 | basic.nFileIndexLow as u64)
                        .to_le_bytes(),
                );
                (basic.dwVolumeSerialNumber as u64, id, false)
            }
            Err(error) => return Err(error),
        };
        let directory = basic.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
        Ok(Metadata {
            identity: Identity {
                volume,
                file,
                wide_id,
                created: (basic.ftCreationTime.dwHighDateTime as u64) << 32
                    | basic.ftCreationTime.dwLowDateTime as u64,
            },
            directory,
            bytes: (!directory)
                .then_some((basic.nFileSizeHigh as u64) << 32 | basic.nFileSizeLow as u64),
            reparse: basic.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0,
            modified: (((basic.ftLastWriteTime.dwHighDateTime as u64) << 32
                | basic.ftLastWriteTime.dwLowDateTime as u64)
                / 10_000_000)
                .saturating_sub(11_644_473_600),
            attributes: basic.dwFileAttributes,
            modified_raw: ((basic.ftLastWriteTime.dwHighDateTime as u64) << 32)
                | basic.ftLastWriteTime.dwLowDateTime as u64,
        })
    }
}
impl Identity {
    pub fn encode(&self) -> [u8; 33] {
        let mut value = [0; 33];
        value[0] = u8::from(self.wide_id);
        value[1..9].copy_from_slice(&self.volume.to_le_bytes());
        value[9..25].copy_from_slice(&self.file);
        value[25..].copy_from_slice(&self.created.to_le_bytes());
        value
    }
    pub fn decode(value: &[u8]) -> Option<Self> {
        if value.len() != 33 || value[0] > 1 {
            return None;
        }
        Some(Self {
            wide_id: value[0] != 0,
            volume: u64::from_le_bytes(value[1..9].try_into().ok()?),
            file: value[9..25].try_into().ok()?,
            created: u64::from_le_bytes(value[25..].try_into().ok()?),
        })
    }
}
