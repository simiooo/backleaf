//! Bounded plans and an isolated stream-only 7-Zip host. The final destination is never overwritten.
use super::{
    ledger::{Ledger, decode_path, encode_path},
    windows::identity,
};
use crate::model::FrozenSelection;
use anyhow::Context;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Read, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Storage::FileSystem::*,
        System::JobObjects::*,
    },
    core::PCWSTR,
};

// The Win32 wrapper rejects a relative RootDirectory on some Windows builds.
// Keep the destination anchored to the held parent rather than resolving its path again.
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSetInformationFile(
        file: HANDLE,
        status: *mut windows::Win32::System::IO::IO_STATUS_BLOCK,
        information: *const std::ffi::c_void,
        length: u32,
        class: u32,
    ) -> windows::Win32::Foundation::NTSTATUS;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    CreateZip,
    Extract,
}
#[derive(Clone, Deserialize, Default)]
pub struct Notice {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub done: u64,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub index: u64,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub directory: bool,
    #[serde(default)]
    pub size: u64,
}
pub fn validate_target(parent: &Path, name: &str) -> Result<(), String> {
    super::windows::names::validate(name).map_err(str::to_owned)?;
    if std::fs::symlink_metadata(parent.join(name)).is_ok() {
        return Err("名称已存在，请换一个名称".into());
    }
    Ok(())
}
pub fn temporary_name(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .and_then(|s| s.strip_prefix(".backleaf-"))
        .and_then(|s| s.strip_suffix(".partial"))
        .and_then(|s| s.split_once('-'))
        .is_some_and(|(pid, nonce)| pid.parse::<u32>().is_ok() && nonce.parse::<u128>().is_ok())
}
fn same_parent(parent: &Path, expected: &[u8; 33]) -> anyhow::Result<()> {
    anyhow::ensure!(
        identity::capture(parent)?.identity.encode() == *expected,
        "目标文件夹已改变"
    );
    Ok(())
}
fn lock_read(path: &Path) -> anyhow::Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(path)?)
}
fn id_hex(path: &Path) -> anyhow::Result<String> {
    Ok(identity::capture(path)?
        .identity
        .encode()
        .iter()
        .map(|n| format!("{n:02x}"))
        .collect())
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn open_delete(path: &Path) -> anyhow::Result<Handle> {
    let text = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let h = unsafe {
        CreateFileW(
            PCWSTR(text.as_ptr()),
            DELETE.0 | FILE_READ_ATTRIBUTES.0 | SYNCHRONIZE.0,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    }?;
    Ok(Handle(h))
}
struct Stage {
    path: PathBuf,
    id: [u8; 33],
    directory: bool,
    committed: bool,
}
impl Stage {
    fn new(parent: &Path, directory: bool) -> anyhow::Result<Self> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path = parent.join(format!(".backleaf-{}-{nonce}.partial", std::process::id()));
        if directory {
            std::fs::create_dir(&path)?;
        } else {
            File::create_new(&path)?;
        }
        let id = identity::capture(&path)?.identity.encode();
        Ok(Self {
            path,
            id,
            directory,
            committed: false,
        })
    }
    fn commit(&mut self, parent: &Path, name: &str, expected: &[u8; 33]) -> anyhow::Result<()> {
        same_parent(parent, expected)?;
        validate_target(parent, name).map_err(anyhow::Error::msg)?;
        let held = open_delete(&self.path)?;
        anyhow::ensure!(
            unsafe { identity::capture_handle(held.0)? }
                .identity
                .encode()
                == self.id,
            "暂存结果已改变"
        );
        let raw = parent
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let parent = Handle(unsafe {
            CreateFileW(
                PCWSTR(raw.as_ptr()),
                FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        }?);
        anyhow::ensure!(
            unsafe { identity::capture_handle(parent.0)? }
                .identity
                .encode()
                == *expected,
            "目标文件夹已改变"
        );
        let name = name.encode_utf16().collect::<Vec<_>>();
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let bytes = std::mem::size_of::<FILE_RENAME_INFO>() + name.len() * 2;
        let mut aligned = vec![
            0u64;
            bytes
                .div_ceil(8)
                .max(std::mem::size_of::<FILE_RENAME_INFO>().div_ceil(8))
        ];
        let info = aligned.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        unsafe {
            info.write(FILE_RENAME_INFO::default());
            (*info).RootDirectory = parent.0;
            (*info).FileNameLength = (name.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                info.cast::<u8>().add(offset).cast(),
                name.len(),
            );
            let mut status = windows::Win32::System::IO::IO_STATUS_BLOCK::default();
            let result = NtSetInformationFile(held.0, &mut status, info.cast(), bytes as u32, 10);
            if result.0 < 0 {
                return Err(windows::core::Error::from_hresult(
                    windows::core::HRESULT::from_win32(
                        windows::Win32::Foundation::RtlNtStatusToDosError(result),
                    ),
                ))
                .context("提交归档结果时重命名失败");
            }
        }
        self.committed = true;
        Ok(())
    }
    fn cleanup(&self) -> anyhow::Result<()> {
        if self.committed {
            return Ok(());
        }
        let held = open_delete(&self.path)?;
        anyhow::ensure!(
            unsafe { identity::capture_handle(held.0)? }
                .identity
                .encode()
                == self.id,
            "暂存目录已改变，未清理"
        );
        if self.directory {
            for entry in std::fs::read_dir(&self.path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    std::fs::remove_dir_all(entry.path())?;
                } else {
                    std::fs::remove_file(entry.path())?;
                }
            }
        }
        let mut info = FILE_DISPOSITION_INFO { DeleteFile: true };
        unsafe {
            SetFileInformationByHandle(
                held.0,
                FileDispositionInfo,
                (&mut info as *mut FILE_DISPOSITION_INFO).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
            )?;
        }
        Ok(())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn prepared_files(
    selection: &FrozenSelection,
    db: &Connection,
    canceled: &impl Fn() -> bool,
) -> anyhow::Result<()> {
    let parent = selection
        .snapshot
        .parent
        .get()
        .ok_or_else(|| anyhow::anyhow!("源目录未就绪"))?;
    anyhow::ensure!(
        selection
            .snapshot
            .parent_identity
            .get()
            .is_some_and(|id| identity::capture(parent).is_ok_and(|m| m.identity.encode() == *id)),
        "源目录已改变"
    );
    let source = Connection::open_with_flags(
        &selection.snapshot.database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let mut q = source.prepare("SELECT raw,identity FROM files WHERE id=?1")?;
    db.execute_batch("BEGIN")?;
    let mut n = 0;
    for id in selection.mask.ids_from(0, selection.count) {
        anyhow::ensure!(!canceled(), "已取消");
        let (raw, expected): (Vec<u8>, Vec<u8>) =
            q.query_row([id as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let path = parent.join(crate::model::decode_name(&raw));
        anyhow::ensure!(
            identity::capture(&path)?.identity.encode().as_slice() == expected.as_slice(),
            "源文件已改变，请重新选择"
        );
        db.execute(
            "INSERT INTO frontier(path,base)VALUES(?1,?2)",
            params![encode_path(&path), encode_path(parent)],
        )?;
        n += 1;
        if n % 128 == 0 {
            db.execute_batch("COMMIT;BEGIN")?;
        }
    }
    db.execute_batch("COMMIT")?;
    Ok(())
}
fn create_plan(
    selection: &FrozenSelection,
    kind: Kind,
    directory: &Path,
    ledger: &Ledger,
    canceled: &impl Fn() -> bool,
) -> anyhow::Result<TempFile> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let spill = TempFile(directory.join(format!("archive-plan-{nonce}.sqlite")));
    let db = Connection::open(&spill.0)?;
    db.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA cache_size=-2048; CREATE TABLE frontier(id INTEGER PRIMARY KEY,path BLOB NOT NULL,base BLOB NOT NULL);")?;
    prepared_files(selection, &db, canceled)?;
    let plan = TempFile(directory.join(format!("archive-plan-{nonce}.bin")));
    let mut file = File::create_new(&plan.0)?;
    file.write_all(b"BLARCH01")?;
    file.write_all(&0u64.to_le_bytes())?;
    let mut count = 0u64;
    let mut bytes = 0u64;
    let mut path_units = 0usize;
    ledger.begin()?;
    loop {
        anyhow::ensure!(!canceled(), "已取消");
        let next = db
            .query_row(
                "SELECT id,path,base FROM frontier ORDER BY id LIMIT 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        decode_path(&r.get::<_, Vec<u8>>(1)?),
                        decode_path(&r.get::<_, Vec<u8>>(2)?),
                    ))
                },
            )
            .optional()?;
        let Some((id, path, base)) = next else {
            break;
        };
        db.execute("DELETE FROM frontier WHERE id=?1", [id])?;
        let metadata = identity::capture(&path)?;
        anyhow::ensure!(
            kind != Kind::Extract || !metadata.directory,
            "请选择压缩包文件"
        );
        if metadata.directory {
            anyhow::ensure!(
                !directory.canonicalize()?.starts_with(path.canonicalize()?),
                "源目录包含归档暂存区，请更换位置"
            );
        }
        anyhow::ensure!(
            !metadata.reparse && metadata.attributes & (0x1000 | 0x40000 | 0x400000) == 0,
            "包含链接或脱机占位文件，无法安全压缩"
        );
        let relative = path.strip_prefix(&base)?;
        let mut name = Vec::new();
        for component in relative.components() {
            let std::path::Component::Normal(part) = component else {
                anyhow::bail!("源路径无效");
            };
            let text = part
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("文件名编码无效"))?;
            super::windows::names::validate(text).map_err(anyhow::Error::msg)?;
            if !name.is_empty() {
                name.push(b'/' as u16);
            }
            name.extend(part.encode_wide());
        }
        let source = path.as_os_str().encode_wide().collect::<Vec<_>>();
        path_units += source.len() + name.len();
        count += 1;
        anyhow::ensure!(
            count <= 1_000_000 && path_units < 64 * 1024 * 1024,
            "归档清单超过资源上限"
        );
        bytes = bytes
            .checked_add(metadata.bytes.unwrap_or(0))
            .ok_or_else(|| anyhow::anyhow!("大小溢出"))?;
        anyhow::ensure!(bytes <= 1 << 40, "归档超过 1 TiB 上限");
        file.write_all(&metadata.identity.encode())?;
        file.write_all(&metadata.bytes.unwrap_or(0).to_le_bytes())?;
        file.write_all(&metadata.modified_raw.to_le_bytes())?;
        file.write_all(&[u8::from(metadata.directory)])?;
        for text in [source, name] {
            anyhow::ensure!(!text.is_empty() && text.len() < 32760, "路径过长");
            file.write_all(&(text.len() as u32).to_le_bytes())?;
            for unit in text {
                file.write_all(&unit.to_le_bytes())?;
            }
        }
        ledger.add_root(count - 1, &path, Some(&metadata))?;
        if count.is_multiple_of(128) {
            ledger.commit()?;
            ledger.begin()?;
        }
        if metadata.directory {
            let canonical = path.canonicalize()?;
            anyhow::ensure!(
                canonical.starts_with(base.canonicalize()?),
                "源目录通过链接逃逸"
            );
            db.execute_batch("BEGIN")?;
            for (n, entry) in std::fs::read_dir(&path)?.enumerate() {
                anyhow::ensure!(!canceled(), "已取消");
                db.execute(
                    "INSERT INTO frontier(path,base)VALUES(?1,?2)",
                    params![encode_path(&entry?.path()), encode_path(&base)],
                )?;
                if n % 128 == 127 {
                    db.execute_batch("COMMIT;BEGIN")?;
                }
            }
            db.execute_batch("COMMIT")?;
        }
    }
    ledger.commit()?;
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(8))?;
    file.write_all(&count.to_le_bytes())?;
    file.sync_all()?;
    drop(file);
    drop(db);
    drop(spill);
    Ok(plan)
}
#[cfg(target_arch = "aarch64")]
const ARCHIVE_ENGINE: &[u8] = include_bytes!("../../assets/archive-engine/7z-arm64.dll");
#[cfg(not(target_arch = "aarch64"))]
const ARCHIVE_ENGINE: &[u8] = include_bytes!("../../assets/archive-engine/7z.dll");
fn engine(directory: &Path) -> anyhow::Result<(PathBuf, PathBuf, File, File)> {
    let executable = include_bytes!(concat!(env!("OUT_DIR"), "/BackleafArchive.exe"));
    let library = ARCHIVE_ENGINE;
    let mut hash = Sha256::new();
    hash.update(executable);
    hash.update(library);
    let key = format!("{:x}", hash.finalize());
    let root = directory.join("archive-engine").join(key);
    std::fs::create_dir_all(&root)?;
    let helper = root.join("BackleafArchive.exe");
    let dll = root.join("7z.dll");
    for (path, content) in [(&helper, executable.as_slice()), (&dll, library)] {
        match File::create_new(path) {
            Ok(mut f) => {
                f.write_all(content)?;
                f.sync_all()?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    // Packaged LGPL libraries remain replaceable independently of the application.
    // Resolve the explicit adjacent library, never a DLL search-path match.
    let distributed = std::env::current_exe()?
        .parent()
        .unwrap()
        .join("lib/7z.dll");
    let external = distributed.try_exists()?;
    let dll = if external {
        distributed.canonicalize()?
    } else {
        dll
    };
    let mut a = lock_read(&helper)?;
    let mut b = lock_read(&dll)?;
    let mut data = Vec::new();
    a.read_to_end(&mut data)?;
    anyhow::ensure!(data.as_slice() == executable, "归档组件已改变");
    if !external {
        data.clear();
        b.read_to_end(&mut data)?;
        anyhow::ensure!(data.as_slice() == library, "归档组件已改变");
    }
    Ok((helper, dll, a, b))
}
struct HelperInput<'a> {
    path: &'a Path,
    identity: String,
    modified: u64,
    size: u64,
}
fn run_helper(
    mode: &str,
    input: HelperInput<'_>,
    output: &Path,
    directory: &Path,
    canceled: &impl Fn() -> bool,
    report: &mut impl FnMut(Notice) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    use std::os::windows::process::CommandExt;
    let (helper, dll, _helper_lock, _library_lock) = engine(directory)?;
    let job = Handle(unsafe { CreateJobObjectW(None, None) }?);
    let mut process = Command::new(helper)
        .arg(mode)
        .arg(input.path)
        .arg(output)
        .arg(dll)
        .arg(input.identity)
        .arg(id_hex(output)?)
        .arg(input.modified.to_string())
        .arg(input.size.to_string())
        .creation_flags(0x08000000)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    limits.BasicLimitInformation.ActiveProcessLimit = 1;
    limits.ProcessMemoryLimit = 1024 * 1024 * 1024;
    let attached = unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        )
        .and_then(|_| AssignProcessToJobObject(job.0, HANDLE(process.as_raw_handle())))
    };
    if let Err(error) = attached {
        let _ = process.kill();
        let _ = process.wait();
        return Err(error.into());
    }
    let stdout = process.stdout.take().unwrap();
    let (tx, rx) = mpsc::sync_channel(64);
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut line = Vec::new();
        for byte in reader.bytes() {
            match byte {
                Ok(b'\n') => {
                    let result = serde_json::from_slice::<Notice>(&line).map_err(|e| e.to_string());
                    line.clear();
                    if tx.send(result).is_err() {
                        return;
                    }
                }
                Ok(b) => {
                    line.push(b);
                    if line.len() > 262144 {
                        let _ = tx.send(Err("归档消息超限".into()));
                        return;
                    }
                }
                Err(error) => {
                    let _ = tx.send(Err(error.to_string()));
                    return;
                }
            }
        }
    });
    let mut complete = false;
    let mut error = None;
    loop {
        if canceled() {
            let _ = process.kill();
            let _ = process.wait();
            anyhow::bail!("已取消");
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(notice)) => {
                if notice.kind == "error" {
                    error = Some(notice.message.clone());
                }
                if notice.kind == "complete" {
                    complete = true;
                }
                if let Err(error) = report(notice) {
                    let _ = process.kill();
                    let _ = process.wait();
                    return Err(error);
                }
            }
            Ok(Err(e)) => {
                let _ = process.kill();
                let _ = process.wait();
                anyhow::bail!(e);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    let exit = process.wait()?;
    anyhow::ensure!(
        exit.success() && complete,
        "{}",
        error.unwrap_or_else(|| "归档引擎异常退出或超过资源限制".into())
    );
    drop(job);
    Ok(())
}
pub struct Request {
    pub selection: FrozenSelection,
    pub kind: Kind,
    pub parent: PathBuf,
    pub name: String,
    pub parent_id: [u8; 33],
}
pub fn run(
    request: Request,
    directory: &Path,
    ledger: &Ledger,
    canceled: impl Fn() -> bool,
    mut report: impl FnMut(Notice) -> anyhow::Result<()>,
) -> anyhow::Result<PathBuf> {
    let Request {
        selection,
        kind,
        parent,
        name,
        parent_id,
    } = request;
    same_parent(&parent, &parent_id)?;
    validate_target(&parent, &name).map_err(anyhow::Error::msg)?;
    let plan = create_plan(&selection, kind, directory, ledger, &canceled)?;
    let mut stage = Stage::new(&parent, kind == Kind::Extract)?;
    if kind == Kind::CreateZip {
        run_helper(
            "create",
            HelperInput {
                path: &plan.0,
                identity: id_hex(&plan.0)?,
                modified: 0,
                size: 0,
            },
            &stage.path,
            directory,
            &canceled,
            &mut report,
        )?;
    } else {
        let mut source = File::open(&plan.0)?;
        let mut magic = [0u8; 8];
        source.read_exact(&mut magic)?;
        let mut raw_count = [0u8; 8];
        source.read_exact(&mut raw_count)?;
        let source_count = u64::from_le_bytes(raw_count);
        ledger.archive_reset()?;
        let mut offset_count = 0u64;
        let mut offset_bytes = 0u64;
        let mut pending_rows = 0u64;
        ledger.begin()?;
        for archive_index in 0..source_count {
            let mut expected = [0u8; 33];
            source.read_exact(&mut expected)?;
            let mut size = [0u8; 8];
            source.read_exact(&mut size)?;
            let mut modified = [0u8; 8];
            source.read_exact(&mut modified)?;
            let mut directory_flag = [0u8; 1];
            source.read_exact(&mut directory_flag)?;
            anyhow::ensure!(directory_flag[0] == 0, "请选择压缩包文件");
            let mut text = || -> anyhow::Result<std::ffi::OsString> {
                use std::os::windows::ffi::OsStringExt;
                let mut n = [0u8; 4];
                source.read_exact(&mut n)?;
                let n = u32::from_le_bytes(n) as usize;
                anyhow::ensure!(n < 32760, "路径过长");
                let mut data = vec![0u8; n * 2];
                source.read_exact(&mut data)?;
                Ok(std::ffi::OsString::from_wide(
                    &data
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| u16::from_le_bytes([b[0], b[1]]))
                        .collect::<Vec<_>>(),
                ))
            };
            let archive = PathBuf::from(text()?);
            let _ = text()?;
            let destination = if source_count > 1 {
                let stem = archive
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| anyhow::anyhow!("压缩包名称无效"))?;
                super::windows::names::validate(stem).map_err(anyhow::Error::msg)?;
                let p = stage.path.join(format!("{}-{stem}", archive_index + 1));
                std::fs::create_dir(&p)?;
                p
            } else {
                stage.path.clone()
            };
            let final_base = if source_count > 1 {
                parent.join(&name).join(destination.file_name().unwrap())
            } else {
                parent.join(&name)
            };
            let mut local_total = 0;
            let mut local_bytes = 0;
            run_helper(
                "extract",
                HelperInput {
                    path: &archive,
                    identity: expected.iter().map(|n| format!("{n:02x}")).collect(),
                    modified: u64::from_le_bytes(modified),
                    size: u64::from_le_bytes(size),
                },
                &destination,
                directory,
                &canceled,
                &mut |mut notice| {
                    if notice.kind == "plan" {
                        local_total = notice.count;
                        local_bytes = notice.bytes;
                        anyhow::ensure!(
                            local_total + offset_count <= 1_000_000
                                && local_bytes + offset_bytes <= 1 << 40,
                            "归档超过资源上限"
                        );
                        notice.count += offset_count;
                        notice.bytes += offset_bytes;
                        if archive_index + 1 < source_count {
                            notice.kind = "partial-plan".into();
                        }
                    } else if notice.kind == "item" {
                        let relative = Path::new(&notice.path);
                        anyhow::ensure!(
                            relative
                                .components()
                                .all(|c| matches!(c, std::path::Component::Normal(_))),
                            "归档含不安全路径"
                        );
                        ledger.add_archive_entry(
                            offset_count + notice.index,
                            &final_base.join(relative),
                            notice.directory,
                            notice.size,
                        )?;
                        pending_rows += 1;
                        if pending_rows.is_multiple_of(128) {
                            ledger.commit()?;
                            ledger.begin()?;
                        }
                    } else if notice.kind == "progress" {
                        notice.done += offset_count;
                        notice.bytes += offset_bytes;
                    }
                    report(notice)
                },
            )?;
            offset_count += local_total;
            offset_bytes += local_bytes;
        }
        ledger.commit()?;
    }
    anyhow::ensure!(!canceled(), "已取消");
    report(Notice {
        kind: "finishing".into(),
        ..Default::default()
    })?;
    if kind == Kind::CreateZip {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&stage.path)?
            .sync_all()?;
    }
    ledger.archive_intent(&super::ledger::ArchiveIntent {
        target: crate::file_view::WinPath::new(&parent.join(&name)),
        stage: crate::file_view::WinPath::new(&stage.path),
        identity: stage.id.to_vec(),
        extracted: kind == Kind::Extract,
        pid: std::process::id(),
        process_created: crate::window_state::process_created(std::process::id()).unwrap_or(0),
    })?;
    stage.commit(&parent, &name, &parent_id)?;
    let result = parent.join(name);
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn invoke(
        mode: &str,
        input: &Path,
        output: &Path,
        state: &Path,
        cancel: &impl Fn() -> bool,
    ) -> anyhow::Result<()> {
        let m = identity::capture(input)?;
        run_helper(
            mode,
            HelperInput {
                path: input,
                identity: id_hex(input)?,
                modified: m.modified_raw,
                size: m.bytes.unwrap_or(0),
            },
            output,
            state,
            cancel,
            &mut |_| Ok(()),
        )
    }
    #[test]
    fn native_zip_round_trip_preserves_nested_unicode_bytes_and_refuses_overwrite() {
        let f = crate::test_support::Fixture::new("archive");
        let source = f.0.join("source");
        std::fs::create_dir_all(source.join("项目/空目录")).unwrap();
        let data = (0..200_000).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        std::fs::write(source.join("项目/报告.bin"), &data).unwrap();
        std::fs::write(source.join("readme.txt"), b"hello").unwrap();
        let state = f.0.join("state");
        std::fs::create_dir(&state).unwrap();
        let ledger = Ledger::open(&state.join("compress.sqlite")).unwrap();
        let parent_id = identity::capture(&f.0).unwrap().identity.encode();
        let zip = run(
            Request {
                selection: f.freeze("source"),
                kind: Kind::CreateZip,
                parent: f.0.clone(),
                name: "output.zip".into(),
                parent_id,
            },
            &state,
            &ledger,
            || false,
            |_| Ok(()),
        )
        .unwrap();
        assert!(zip.is_file());
        let mut stage = Stage::new(&f.0, true).unwrap();
        invoke("extract", &zip, &stage.path, &state, &|| false).unwrap();
        assert_eq!(
            std::fs::read(stage.path.join("项目/报告.bin")).unwrap(),
            data
        );
        assert!(stage.path.join("项目/空目录").is_dir());
        assert_eq!(
            std::fs::read(stage.path.join("readme.txt")).unwrap(),
            b"hello"
        );
        std::fs::create_dir(f.0.join("occupied")).unwrap();
        assert!(stage.commit(&f.0, "occupied", &parent_id).is_err());
        assert!(stage.path.exists());
        stage.commit(&f.0, "restored", &parent_id).unwrap();
        assert_eq!(
            std::fs::read(f.0.join("restored/readme.txt")).unwrap(),
            b"hello"
        );
    }
    // A minimal ZIP with deliberately unsafe names. CRC is zero for empty payloads.
    fn empty_zip(path: &Path, names: &[&str]) {
        let mut bytes = Vec::new();
        let mut central = Vec::new();
        for name in names {
            let name = name.as_bytes();
            let offset = bytes.len() as u32;
            bytes.extend(0x04034b50u32.to_le_bytes());
            bytes.extend([20, 0, 0, 8, 0, 0, 0, 0, 0, 0]);
            bytes.extend([0u8; 12]);
            bytes.extend((name.len() as u16).to_le_bytes());
            bytes.extend(0u16.to_le_bytes());
            bytes.extend(name);
            central.extend(0x02014b50u32.to_le_bytes());
            central.extend([20, 0, 20, 0, 0, 8, 0, 0, 0, 0, 0, 0]);
            central.extend([0u8; 12]);
            central.extend((name.len() as u16).to_le_bytes());
            central.extend([0u8; 12]);
            central.extend(offset.to_le_bytes());
            central.extend(name);
        }
        let start = bytes.len() as u32;
        let length = central.len() as u32;
        bytes.extend(central);
        bytes.extend(0x06054b50u32.to_le_bytes());
        bytes.extend([0u8; 4]);
        bytes.extend((names.len() as u16).to_le_bytes());
        bytes.extend((names.len() as u16).to_le_bytes());
        bytes.extend(length.to_le_bytes());
        bytes.extend(start.to_le_bytes());
        bytes.extend([0u8; 2]);
        std::fs::write(path, bytes).unwrap();
    }
    #[test]
    fn native_extract_rejects_traversal_streams_devices_case_collisions_and_cancel() {
        let f = crate::test_support::Fixture::new("unsafe-archive");
        let state = f.0.join("state");
        std::fs::create_dir(&state).unwrap();
        for (i, names) in [
            vec!["../escape.txt"],
            vec!["a:stream"],
            vec!["CON.txt"],
            vec!["a.txt", "A.txt"],
        ]
        .into_iter()
        .enumerate()
        {
            let zip = f.0.join(format!("bad-{i}.zip"));
            empty_zip(&zip, &names);
            let stage = Stage::new(&f.0, true).unwrap();
            assert!(invoke("extract", &zip, &stage.path, &state, &|| false).is_err());
            assert_eq!(std::fs::read_dir(&stage.path).unwrap().count(), 0);
        }
        assert!(!f.0.join("escape.txt").exists());
        let zip = f.0.join("valid.zip");
        empty_zip(&zip, &["ok.txt"]);
        let stage = Stage::new(&f.0, true).unwrap();
        assert!(invoke("extract", &zip, &stage.path, &state, &|| true).is_err());
        assert!(!stage.path.join("ok.txt").exists());
    }
}
#[cfg(test)]
mod compatibility_tests {
    use super::*;
    #[test]
    fn official_corpus_extracts_7z_rar4_and_rar5_with_verified_content() {
        let f = crate::test_support::Fixture::new("archive-formats");
        let state = f.0.join("state");
        std::fs::create_dir(&state).unwrap();
        for (archive, files) in [
            ("test_read_format_7zip_lzma1.7z", vec![("file1", 2844)]),
            (
                "test_read_format_rar_binary_data.rar",
                vec![
                    ("random_data.bin", 1048576),
                    ("LibarchiveAddingTest.odt", 32618),
                ],
            ),
            (
                "test_read_format_rar5_compressed.rar",
                vec![("test.bin", 1200)],
            ),
        ] {
            let input = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/archives")
                .join(archive);
            let m = identity::capture(&input).unwrap();
            let stage = Stage::new(&f.0, true).unwrap();
            run_helper(
                "extract",
                HelperInput {
                    path: &input,
                    identity: id_hex(&input).unwrap(),
                    modified: m.modified_raw,
                    size: m.bytes.unwrap(),
                },
                &stage.path,
                &state,
                &|| false,
                &mut |_| Ok(()),
            )
            .unwrap();
            for (name, length) in files {
                assert_eq!(
                    std::fs::metadata(stage.path.join(name)).unwrap().len(),
                    length
                );
            }
            if archive.ends_with(".7z") {
                assert!(
                    std::fs::read(stage.path.join("file1"))
                        .unwrap()
                        .starts_with(b"The libarchive distribution ")
                );
            }
            if archive.contains("rar5") {
                let data = std::fs::read(stage.path.join("test.bin")).unwrap();
                for (i, chunk) in data.as_chunks::<4>().0.iter().enumerate() {
                    let k = i as i32 + 1;
                    assert_eq!(i32::from_le_bytes(*chunk), (k * k - 3 * k + 1).max(0));
                }
            }
            if archive.contains("binary_data") {
                assert!(
                    std::fs::read(stage.path.join("random_data.bin"))
                        .unwrap()
                        .ends_with(&[0x08, 0x74, 0xde, 0xec])
                );
            }
        }
    }
}
