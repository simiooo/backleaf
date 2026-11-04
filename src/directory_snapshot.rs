//! NTFS/ReFS enumerate names and identities together, without opening every file.
use std::{
    collections::VecDeque,
    ffi::OsString,
    io,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE},
        Storage::FileSystem::*,
    },
    core::PCWSTR,
};
pub struct Entry {
    pub name: OsString,
    pub directory: bool,
    pub bytes: Option<u64>,
    pub identity: [u8; 33],
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
enum Backend {
    Native {
        handle: Handle,
        volume: u64,
        queue: VecDeque<Entry>,
        done: bool,
    },
    Fallback(Box<std::fs::ReadDir>),
}
pub struct Reader {
    backend: Backend,
    parent: PathBuf,
}
impl Reader {
    pub fn new(path: &Path) -> io::Result<Self> {
        let parent = crate::operations::windows::identity::capture(path)
            .map_err(|e| io::Error::other(e.to_string()))?;
        if parent.identity.wide_id {
            let raw = path
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            if let Ok(handle) = unsafe {
                CreateFileW(
                    PCWSTR(raw.as_ptr()),
                    FILE_LIST_DIRECTORY.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS,
                    None,
                )
            } {
                let mut reader = Self {
                    backend: Backend::Native {
                        handle: Handle(handle),
                        volume: parent.identity.volume,
                        queue: VecDeque::new(),
                        done: false,
                    },
                    parent: path.to_owned(),
                };
                if reader.fill().is_ok() {
                    return Ok(reader);
                }
            }
        }
        Ok(Self {
            backend: Backend::Fallback(Box::new(std::fs::read_dir(path)?)),
            parent: path.to_owned(),
        })
    }
    fn fill(&mut self) -> io::Result<()> {
        let Backend::Native {
            handle,
            volume,
            queue,
            done,
        } = &mut self.backend
        else {
            return Ok(());
        };
        if *done {
            return Ok(());
        }
        let mut buffer = vec![0u64; 8192];
        if let Err(error) = unsafe {
            GetFileInformationByHandleEx(
                handle.0,
                FileIdExtdDirectoryInfo,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
            )
        } {
            if error.code() == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0) {
                *done = true;
                return Ok(());
            }
            return Err(io::Error::other(error.to_string()));
        }
        let limit = buffer.len() * 8;
        let mut offset = 0usize;
        loop {
            let name_offset = std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileName);
            if offset + std::mem::size_of::<FILE_ID_EXTD_DIR_INFO>() > limit {
                return Err(io::Error::other("directory record overflow"));
            }
            let item = unsafe {
                std::ptr::read_unaligned(
                    buffer
                        .as_ptr()
                        .cast::<u8>()
                        .add(offset)
                        .cast::<FILE_ID_EXTD_DIR_INFO>(),
                )
            };
            let n = item.FileNameLength as usize;
            if item.FileId.Identifier == [0; 16] {
                return Err(io::Error::other("directory identity unsupported"));
            }
            if n == 0 || !n.is_multiple_of(2) || offset + name_offset + n > limit {
                return Err(io::Error::other("invalid directory name"));
            }
            let text = unsafe {
                std::slice::from_raw_parts(
                    buffer
                        .as_ptr()
                        .cast::<u8>()
                        .add(offset + name_offset)
                        .cast::<u16>(),
                    n / 2,
                )
            };
            let name = OsString::from_wide(text);
            if name != "." && name != ".." {
                let mut identity = [0u8; 33];
                identity[0] = 1;
                identity[1..9].copy_from_slice(&volume.to_le_bytes());
                identity[9..25].copy_from_slice(&item.FileId.Identifier);
                identity[25..].copy_from_slice(&(item.CreationTime as u64).to_le_bytes());
                let directory = item.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
                queue.push_back(Entry {
                    name,
                    directory,
                    bytes: (!directory).then_some(item.EndOfFile.max(0) as u64),
                    identity,
                });
            }
            let step = item.NextEntryOffset as usize;
            if step == 0 {
                break;
            }
            if step < name_offset
                || !step.is_multiple_of(8)
                || offset + step <= offset
                || offset + step >= limit
            {
                return Err(io::Error::other("invalid directory offset"));
            }
            offset += step;
        }
        Ok(())
    }
}
impl Iterator for Reader {
    type Item = io::Result<Entry>;
    fn next(&mut self) -> Option<Self::Item> {
        if let Backend::Fallback(entries) = &mut self.backend {
            return entries.next().map(|entry| {
                let entry = entry?;
                let metadata = crate::operations::windows::identity::capture(
                    &self.parent.join(entry.file_name()),
                )
                .map_err(|e| io::Error::other(e.to_string()))?;
                Ok(Entry {
                    name: entry.file_name(),
                    directory: metadata.directory,
                    bytes: metadata.bytes,
                    identity: metadata.identity.encode(),
                })
            });
        }
        loop {
            if let Backend::Native { queue, done, .. } = &mut self.backend {
                if let Some(entry) = queue.pop_front() {
                    return Some(Ok(entry));
                }
                if *done {
                    return None;
                }
            }
            if let Err(error) = self.fill() {
                if let Backend::Native { done, .. } = &mut self.backend {
                    *done = true;
                }
                return Some(Err(error));
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enumeration_captures_matching_native_identity_and_long_unicode_names() {
        let f = crate::test_support::Fixture::new("snapshot");
        let name = format!("图像-{}-尾.txt", "长".repeat(70));
        std::fs::write(f.0.join(&name), b"abc").unwrap();
        std::fs::create_dir(f.0.join("folder")).unwrap();
        let rows = Reader::new(&f.0)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows.len(), 2);
        for row in rows {
            let m = crate::operations::windows::identity::capture(&f.0.join(&row.name)).unwrap();
            assert_eq!(row.identity, m.identity.encode());
            assert_eq!(row.directory, m.directory);
            assert_eq!(row.bytes, m.bytes);
        }
    }
}
