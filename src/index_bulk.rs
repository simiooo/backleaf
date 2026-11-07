//! Bulk directory enumeration for the metadata index.
//!
//! Opens each directory once and reads batches of `FILE_ID_*_DIR_INFO` records via
//! `GetFileInformationByHandleEx`, which include file identity, size, times and
//! attributes without opening every child file. The index writer falls back to
//! `std::fs::read_dir` + `identity::capture` whenever the bulk class is unavailable
//! or errors.
use crate::operations::windows::identity;
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_INFO_BY_HANDLE_CLASS,
            FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, FileIdBothDirectoryInfo, FileIdExtdDirectoryInfo,
            GetFileInformationByHandleEx, OPEN_EXISTING,
        },
    },
    core::PCWSTR,
};

const BUFFER_BYTES: usize = 64 * 1024;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
/// Fixed FILE_ID_EXTD_DIR_INFO portion before FileName.
const EXTD_HEADER: usize = 88;
/// Fixed FILE_ID_BOTH_DIR_INFO portion before FileName.
const BOTH_HEADER: usize = 104;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub struct DirectoryReader {
    handle: Handle,
    parent: PathBuf,
    volume: u64,
    identity_is_wide: bool,
    class: FILE_INFO_BY_HANDLE_CLASS,
    buffer: Vec<u8>,
    pending: std::vec::IntoIter<DirEntryInfo>,
    finished: bool,
}

pub struct DirEntryInfo {
    pub path: PathBuf,
    pub metadata: identity::Metadata,
}

impl DirectoryReader {
    /// Opens a directory for bulk enumeration. Returns `Err` when the bulk classes
    /// are unavailable so the caller can fall back to the ordinary filesystem walk.
    pub fn open(parent: &Path) -> anyhow::Result<Self> {
        let wide = parent
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let raw = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )?
        };
        let metadata = match unsafe { identity::capture_handle(raw) } {
            Ok(metadata) => metadata,
            Err(error) => {
                unsafe {
                    let _ = CloseHandle(raw);
                }
                return Err(error.into());
            }
        };
        let volume = metadata.identity.volume;
        let identity_is_wide = metadata.identity.wide_id;
        let classes: &[(FILE_INFO_BY_HANDLE_CLASS, bool)] = if identity_is_wide {
            &[
                (FileIdExtdDirectoryInfo, true),
                (FileIdBothDirectoryInfo, true),
            ]
        } else {
            &[(FileIdBothDirectoryInfo, false)]
        };
        let mut last_error = None;
        for &(class, wide_identity) in classes {
            let mut buffer = vec![0u8; BUFFER_BYTES];
            match fetch_batch(raw, class, &mut buffer, parent, volume, wide_identity) {
                Ok(Some(batch)) => {
                    return Ok(Self {
                        handle: Handle(raw),
                        parent: parent.to_owned(),
                        volume,
                        identity_is_wide: wide_identity,
                        class,
                        buffer,
                        pending: batch.into_iter(),
                        finished: false,
                    });
                }
                Ok(None) => {
                    return Ok(Self {
                        handle: Handle(raw),
                        parent: parent.to_owned(),
                        volume,
                        identity_is_wide: wide_identity,
                        class,
                        buffer,
                        pending: Vec::new().into_iter(),
                        finished: true,
                    });
                }
                Err(error) => last_error = Some(error),
            }
        }
        unsafe {
            let _ = CloseHandle(raw);
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("bulk directory enumeration unavailable")))
    }

    pub fn next(&mut self) -> anyhow::Result<Option<DirEntryInfo>> {
        loop {
            if let Some(item) = self.pending.next() {
                return Ok(Some(item));
            }
            if self.finished {
                return Ok(None);
            }
            let batch = fetch_batch(
                self.handle.0,
                self.class,
                &mut self.buffer,
                &self.parent,
                self.volume,
                self.identity_is_wide,
            )?;
            match batch {
                Some(batch) => self.pending = batch.into_iter(),
                None => self.finished = true,
            }
        }
    }
}

fn fetch_batch(
    handle: HANDLE,
    class: FILE_INFO_BY_HANDLE_CLASS,
    buffer: &mut [u8],
    parent: &Path,
    volume: u64,
    wide_id: bool,
) -> anyhow::Result<Option<Vec<DirEntryInfo>>> {
    let result = unsafe {
        GetFileInformationByHandleEx(
            handle,
            class,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
        )
    };
    if let Err(error) = result {
        if error.code() == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0) {
            return Ok(None);
        }
        return Err(error.into());
    }
    Ok(Some(parse_batch(parent, class, buffer, volume, wide_id)?))
}

fn parse_batch(
    parent: &Path,
    class: FILE_INFO_BY_HANDLE_CLASS,
    buffer: &[u8],
    volume: u64,
    wide_id: bool,
) -> anyhow::Result<Vec<DirEntryInfo>> {
    let mut entries = Vec::new();
    let mut offset = 0usize;
    let header = if class == FileIdExtdDirectoryInfo {
        EXTD_HEADER
    } else {
        BOTH_HEADER
    };
    let file_is_extended = class == FileIdExtdDirectoryInfo;
    loop {
        if offset + header > buffer.len() {
            anyhow::bail!("truncated bulk directory buffer");
        }
        let next = read_u32(buffer, offset) as usize;
        let name_len = read_u32(buffer, offset + 60) as usize;
        if !name_len.is_multiple_of(2) || offset + header + name_len > buffer.len() {
            anyhow::bail!("invalid bulk directory entry name");
        }
        let name = OsString::from_wide(
            &buffer[offset + header..offset + header + name_len]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect::<Vec<_>>(),
        );
        let name_text = name.to_string_lossy();
        if name_text != "." && name_text != ".." {
            let path = parent.join(&name);
            let attributes = read_u32(buffer, offset + 56);
            let directory = attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
            let creation = read_i64(buffer, offset + 8) as u64;
            let last_write = read_i64(buffer, offset + 24).max(0) as u64;
            let end_of_file = read_i64(buffer, offset + 40).max(0) as u64;
            let file = if file_is_extended {
                buffer[offset + 72..offset + 88]
                    .try_into()
                    .expect("16-byte file id")
            } else {
                let raw = read_i64(buffer, offset + 96) as u64;
                let mut id = [0u8; 16];
                id[..8].copy_from_slice(&raw.to_le_bytes());
                id
            };
            entries.push(DirEntryInfo {
                path,
                metadata: identity::Metadata {
                    identity: identity::Identity {
                        volume,
                        file,
                        created: creation,
                        wide_id,
                    },
                    directory,
                    bytes: (!directory).then_some(end_of_file),
                    reparse: attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0,
                    modified: (last_write / 10_000_000).saturating_sub(11_644_473_600),
                    modified_raw: last_write,
                    attributes,
                },
            });
        }
        if next == 0 {
            break;
        }
        if next < header || !next.is_multiple_of(8) || offset + next > buffer.len() {
            anyhow::bail!("invalid bulk directory entry offset");
        }
        offset += next;
    }
    Ok(entries)
}

fn read_u32(buffer: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(buffer[offset..offset + 4].try_into().expect("u32 range"))
}

fn read_i64(buffer: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(buffer[offset..offset + 8].try_into().expect("i64 range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_enumeration_matches_per_file_identity_capture() {
        let fixture = crate::test_support::Fixture::new("index-bulk");
        let root = fixture.0.join("files");
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        for name in ["alpha.txt", "报告.txt", "beta.log"] {
            std::fs::write(root.join(name), b"data").unwrap();
        }
        std::fs::write(nested.join("deep.txt"), b"data").unwrap();
        std::fs::create_dir(root.join("empty")).unwrap();
        let root = root.canonicalize().unwrap();
        let mut reader = DirectoryReader::open(&root).unwrap();
        let mut seen = Vec::new();
        while let Some(entry) = reader.next().unwrap() {
            let expected = identity::capture(&entry.path).unwrap();
            assert_eq!(entry.metadata.identity, expected.identity);
            assert_eq!(entry.metadata.directory, expected.directory);
            assert_eq!(entry.metadata.reparse, expected.reparse);
            assert_eq!(entry.metadata.bytes, expected.bytes);
            // The directory-entry LastWriteTime can lag the file handle by up to a
            // second when a file was just written (NTFS flushes the directory entry
            // lazily). The index stores whole seconds; validation below tolerates
            // the one-second discrepancy while identity/bytes remain exact.
            assert!(
                entry.metadata.modified.abs_diff(expected.modified) <= 1,
                "modified {} vs {}",
                entry.metadata.modified,
                expected.modified
            );
            seen.push(
                entry
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        seen.sort();
        assert_eq!(
            seen,
            vec!["alpha.txt", "beta.log", "empty", "nested", "报告.txt"]
        );
        let empty = root.join("empty");
        let mut reader = DirectoryReader::open(&empty).unwrap();
        assert!(reader.next().unwrap().is_none());
    }

    #[test]
    fn malformed_batch_offsets_are_rejected() {
        let mut buffer = vec![0u8; EXTD_HEADER + 2];
        // NextEntryOffset stays 0; name length is zero, so this is a valid bare
        // entry and must parse. Then a corrupted offset must be rejected.
        assert!(parse_batch(Path::new("root"), FileIdExtdDirectoryInfo, &buffer, 1, true).is_ok());
        buffer[0..4].copy_from_slice(&4u32.to_le_bytes());
        assert!(parse_batch(Path::new("root"), FileIdExtdDirectoryInfo, &buffer, 1, true).is_err());
    }
}
