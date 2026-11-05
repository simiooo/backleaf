//! A bounded binary protocol keeps Shell discovery/invocation isolated from self-drawn menus.
use crate::{
    model::{Event, FrozenSelection},
    operations::windows::{clipboard_path, identity},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Read, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Permit;
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[derive(Clone, Debug)]
pub struct Item {
    pub id: u32,
    pub label: String,
    pub hint: String,
    pub enabled: bool,
    pub checked: bool,
    pub radio: bool,
    pub submenu: bool,
    pub separator: bool,
    pub unavailable: bool,
}
pub struct Notice {
    pub session: u64,
    pub parent: u32,
    pub result: Result<Vec<Item>, String>,
}
enum Request {
    Menu(u32),
    Invoke(u32),
}
struct Flags {
    closed: AtomicBool,
    requested: AtomicBool,
}
pub struct Session {
    pub id: u64,
    sender: mpsc::SyncSender<Request>,
    flags: Arc<Flags>,
    #[cfg(test)]
    _test_requests: Option<std::sync::Mutex<mpsc::Receiver<Request>>>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.flags.closed.store(true, Ordering::Release);
    }
}
impl Session {
    pub fn new(
        selection: FrozenSelection,
        directory: PathBuf,
        owner: usize,
        tx: async_channel::Sender<Event>,
    ) -> Result<Arc<Self>, String> {
        if selection.mask.count_before(selection.count) > 2048 {
            return Err("扩展菜单最多处理2,048项".into());
        }
        if ACTIVE
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .is_err()
        {
            return Err("扩展菜单仍在处理中".into());
        }
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::sync_channel(16);
        let flags = Arc::new(Flags {
            closed: AtomicBool::new(false),
            requested: AtomicBool::new(false),
        });
        let work_flags = flags.clone();
        let spawn = std::thread::Builder::new()
            .name("backleaf-shell-session".into())
            .spawn(move || {
                let _permit = Permit;
                let result = (|| -> anyhow::Result<()> {
                    std::fs::create_dir_all(&directory)?;
                    let source = include_bytes!(concat!(env!("OUT_DIR"), "/BackleafShell.exe"));
                    let root = directory.join(format!("shell-host-{:x}", Sha256::digest(source)));
                    std::fs::create_dir_all(&root)?;
                    let helper = root.join("BackleafShell.exe");
                    match File::create_new(&helper) {
                        Ok(mut f) => {
                            f.write_all(source)?;
                            f.sync_all()?
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(e.into()),
                    }
                    let mut locked = OpenOptions::new().read(true).share_mode(1).open(&helper)?;
                    let mut bytes = Vec::new();
                    locked.read_to_end(&mut bytes)?;
                    anyhow::ensure!(bytes.as_slice() == source, "扩展宿主已改变");
                    let parent = selection
                        .snapshot
                        .parent
                        .get()
                        .ok_or_else(|| anyhow::anyhow!("源目录未就绪"))?;
                    let parent_id = identity::capture(parent)?.identity.encode();
                    anyhow::ensure!(
                        selection.snapshot.parent_identity.get() == Some(&parent_id),
                        "源目录已改变"
                    );
                    let spec =
                        Temp(directory.join(format!("shell-{}-{id}.bin", std::process::id())));
                    let mut file = File::create_new(&spec.0)?;
                    file.write_all(
                        &(selection.mask.count_before(selection.count) as u32).to_le_bytes(),
                    )?;
                    write_path(&mut file, parent)?;
                    file.write_all(&parent_id)?;
                    let db = rusqlite::Connection::open_with_flags(
                        &selection.snapshot.database,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                    )?;
                    let mut q = db.prepare("SELECT raw,identity FROM files WHERE id=?1")?;
                    let mut units = parent.as_os_str().encode_wide().count();
                    for row in selection.mask.ids_from(0, selection.count) {
                        anyhow::ensure!(!work_flags.closed.load(Ordering::Acquire), "已关闭");
                        let (raw, expected): (Vec<u8>, Vec<u8>) =
                            q.query_row([row as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
                        let path = parent.join(crate::model::decode_name(&raw));
                        units += path.as_os_str().encode_wide().count();
                        anyhow::ensure!(units <= 256 * 1024, "扩展菜单路径超过512 KiB");
                        let metadata = identity::capture(&path)?;
                        anyhow::ensure!(
                            metadata.identity.encode().as_slice() == expected.as_slice(),
                            "源文件已改变"
                        );
                        write_path(&mut file, &path)?;
                        file.write_all(&metadata.identity.encode())?
                    }
                    file.sync_all()?;
                    drop(file);
                    let mut child = Command::new(helper)
                        .arg(&spec.0)
                        .arg(owner.to_string())
                        .arg(std::process::id().to_string())
                        .creation_flags(0x08000000)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .spawn()?;
                    let mut input = child.stdin.take().unwrap();
                    let output = child.stdout.take().unwrap();
                    let (messages, answers) = mpsc::sync_channel(16);
                    std::thread::spawn(move || {
                        let mut reader = BufReader::new(output);
                        loop {
                            let response = read_response(&mut reader);
                            match response {
                                Ok(Some(r)) => {
                                    if messages.send(Ok(r)).is_err() {
                                        break;
                                    }
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    let _ = messages.send(Err(e.to_string()));
                                    break;
                                }
                            }
                        }
                    });
                    let mut waiting = Some((0u32, Instant::now()));
                    let mut invoking = false;
                    let mut command_sent = false;
                    let mut failed = false;
                    loop {
                        if work_flags.closed.load(Ordering::Acquire)
                            && !work_flags.requested.load(Ordering::Acquire)
                            && !invoking
                        {
                            let _ = writeln!(input, "close");
                            let _ = child.kill();
                            break;
                        }
                        if waiting.is_none() && !invoking {
                            match receiver.try_recv() {
                                Ok(Request::Menu(parent)) => {
                                    writeln!(input, "menu {parent}")?;
                                    input.flush()?;
                                    waiting = Some((parent, Instant::now()));
                                }
                                Ok(Request::Invoke(command)) => {
                                    writeln!(input, "invoke {command}")?;
                                    input.flush()?;
                                    command_sent = true;
                                    waiting = Some((0, Instant::now()));
                                }
                                Err(mpsc::TryRecvError::Empty) => {}
                                Err(mpsc::TryRecvError::Disconnected) => {
                                    let _ = writeln!(input, "close");
                                    break;
                                }
                            }
                        }
                        match answers.recv_timeout(Duration::from_millis(25)) {
                            Ok(Ok(Response::Rows(parent, rows))) => {
                                waiting = None;
                                let _ = tx.send_blocking(Event::ShellRows(Notice {
                                    session: id,
                                    parent,
                                    result: Ok(rows),
                                }));
                            }
                            Ok(Ok(Response::Invoking)) => {
                                invoking = true;
                                waiting = None;
                            }
                            Ok(Ok(Response::Closed)) => break,
                            Ok(Ok(Response::Error)) => {
                                failed = true;
                                break;
                            }
                            Ok(Err(_)) => {
                                failed = true;
                                if !invoking {
                                    let _ = child.kill();
                                }
                                break;
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                        }
                        if !invoking
                            && waiting.is_some_and(|(_, started)| {
                                started.elapsed() > Duration::from_secs(5)
                            })
                        {
                            let _ = child.kill();
                            let _ = child.wait();
                            anyhow::bail!("扩展菜单响应超时")
                        }
                    }
                    let exit = child.wait()?;
                    anyhow::ensure!(
                        !failed && exit.success()
                            || work_flags.closed.load(Ordering::Acquire) && !command_sent,
                        "扩展菜单不可用或源文件已改变"
                    );
                    Ok(())
                })();
                if let Err(e) = result {
                    let error = e.to_string();
                    if work_flags.requested.load(Ordering::Acquire) {
                        let _ = tx.send_blocking(Event::ShellMenuClosed(Err(error)));
                    } else if !work_flags.closed.load(Ordering::Acquire) {
                        let _ = tx.send_blocking(Event::ShellRows(Notice {
                            session: id,
                            parent: 0,
                            result: Err(error),
                        }));
                    }
                }
            });
        if let Err(e) = spawn {
            ACTIVE.fetch_sub(1, Ordering::AcqRel);
            return Err(e.to_string());
        }
        Ok(Arc::new(Self {
            id,
            sender,
            flags,
            #[cfg(test)]
            _test_requests: None,
        }))
    }
    #[cfg(test)]
    pub fn headless_fixture(id: u64) -> Arc<Self> {
        let (sender, receiver) = mpsc::sync_channel(16);
        Arc::new(Self {
            id,
            sender,
            flags: Arc::new(Flags {
                closed: AtomicBool::new(false),
                requested: AtomicBool::new(false),
            }),
            _test_requests: Some(std::sync::Mutex::new(receiver)),
        })
    }
    pub fn submenu(&self, parent: u32) -> Result<(), String> {
        self.sender
            .try_send(Request::Menu(parent))
            .map_err(|_| "扩展菜单正忙".into())
    }
    pub fn invoke(&self, command: u32) -> Result<(), String> {
        if self.flags.requested.swap(true, Ordering::AcqRel) {
            return Err("命令已提交".into());
        }
        if self.sender.try_send(Request::Invoke(command)).is_err() {
            self.flags.requested.store(false, Ordering::Release);
            return Err("扩展菜单已关闭".into());
        }
        Ok(())
    }
}
enum Response {
    Rows(u32, Vec<Item>),
    Invoking,
    Closed,
    Error,
}
fn read_response(reader: &mut impl Read) -> anyhow::Result<Option<Response>> {
    let mut line = Vec::new();
    loop {
        let mut b = [0];
        match reader.read(&mut b)? {
            0 if line.is_empty() => return Ok(None),
            0 => anyhow::bail!("扩展消息不完整"),
            _ if b[0] == b'\n' => break,
            _ => {
                anyhow::ensure!(line.len() < 4096, "扩展消息过长");
                line.push(b[0]);
            }
        }
    }
    let raw = std::str::from_utf8(&line)?;
    let s = raw.strip_suffix('\r').unwrap_or(raw);
    match s {
        "invoking" => Ok(Some(Response::Invoking)),
        "closed" => Ok(Some(Response::Closed)),
        "error" => Ok(Some(Response::Error)),
        _ => {
            let p = s.split_whitespace().collect::<Vec<_>>();
            anyhow::ensure!(p.len() == 3 && p[0] == "rows", "扩展消息无效");
            let parent = p[1].parse()?;
            let length: usize = p[2].parse()?;
            anyhow::ensure!(length <= 512 * 1024, "扩展菜单过大");
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes)?;
            Ok(Some(Response::Rows(parent, decode_rows(&bytes)?)))
        }
    }
}
pub fn decode_rows(bytes: &[u8]) -> anyhow::Result<Vec<Item>> {
    struct Cursor<'a> {
        data: &'a [u8],
        at: usize,
    }
    impl Cursor<'_> {
        fn number(&mut self) -> anyhow::Result<u32> {
            let value = self
                .data
                .get(self.at..self.at + 4)
                .ok_or_else(|| anyhow::anyhow!("菜单数据不完整"))?;
            self.at += 4;
            Ok(u32::from_le_bytes(value.try_into()?))
        }
        fn text(&mut self) -> anyhow::Result<String> {
            let n = self.number()? as usize;
            anyhow::ensure!(n <= 2048, "菜单文字过长");
            let size = n
                .checked_mul(2)
                .ok_or_else(|| anyhow::anyhow!("菜单溢出"))?;
            let value = self
                .data
                .get(self.at..self.at + size)
                .ok_or_else(|| anyhow::anyhow!("菜单文字不完整"))?;
            self.at += size;
            let units = value
                .as_chunks::<2>()
                .0
                .iter()
                .map(|u| u16::from_le_bytes([u[0], u[1]]))
                .collect::<Vec<_>>();
            Ok(String::from_utf16_lossy(&units))
        }
    }
    let mut cursor = Cursor { data: bytes, at: 0 };
    let count = cursor.number()? as usize;
    anyhow::ensure!(count <= 512, "菜单条目过多");
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let id = cursor.number()?;
        let flags = cursor.number()?;
        let raw = cursor.text()?;
        let _verb = cursor.text()?;
        let (label, hint) = raw.split_once('\t').unwrap_or((&raw, ""));
        let mut cleaned = String::new();
        let mut chars = label.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '&' {
                if chars.peek() == Some(&'&') {
                    cleaned.push('&');
                    chars.next();
                }
            } else {
                cleaned.push(ch)
            }
        }
        result.push(Item {
            id,
            label: cleaned,
            hint: hint.into(),
            enabled: flags & 34 == 0,
            checked: flags & 4 != 0,
            radio: flags & 8 != 0,
            submenu: flags & 16 != 0,
            separator: flags & 1 == 0,
            unavailable: flags & 32 != 0,
        });
    }
    anyhow::ensure!(cursor.at == bytes.len(), "菜单消息包含多余数据");
    Ok(result)
}
fn write_path(file: &mut File, path: &Path) -> anyhow::Result<()> {
    let raw = shell_path(path)?;
    anyhow::ensure!(!raw.is_empty() && raw.len() / 2 < 32760, "路径过长");
    file.write_all(&((raw.len() / 2) as u32).to_le_bytes())?;
    file.write_all(&raw)?;
    Ok(())
}
fn shell_path(path: &Path) -> anyhow::Result<Vec<u8>> {
    let mut units = clipboard_path(path)?;
    anyhow::ensure!(units.pop() == Some(0), "路径格式无效");
    Ok(units.into_iter().flat_map(u16::to_le_bytes).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::Path};
    #[test]
    fn native_protocol_accepts_crlf_and_rejects_noise_and_truncation() {
        for mut bytes in [b"closed\n".as_slice(), b"closed\r\n"] {
            assert!(matches!(
                read_response(&mut bytes).unwrap(),
                Some(Response::Closed)
            ));
        }
        for mut bytes in [b"unknown\n".as_slice(), b"rows 0 524289\n", b"rows 0 4\n\0"] {
            assert!(read_response(&mut bytes).is_err());
        }
    }
    #[test]
    fn binary_menu_preserves_flags_accelerators_tabs_and_unicode_without_overread() {
        let mut bytes = Vec::new();
        for n in [1u32, 42, 1 | 4 | 8 | 16] {
            bytes.extend(n.to_le_bytes());
        }
        for text in ["测试(&T) && archive\tCtrl+T", "7zip.test"] {
            let units = text.encode_utf16().collect::<Vec<_>>();
            bytes.extend((units.len() as u32).to_le_bytes());
            for u in units {
                bytes.extend(u.to_le_bytes());
            }
        }
        let rows = decode_rows(&bytes).unwrap();
        assert_eq!(rows[0].id, 42);
        assert_eq!(rows[0].label, "测试(T) & archive");
        assert_eq!(rows[0].hint, "Ctrl+T");
        assert!(rows[0].enabled && rows[0].checked && rows[0].radio && rows[0].submenu);
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_rows(&trailing).is_err());
        bytes.pop();
        assert!(decode_rows(&bytes).is_err());
        assert!(decode_rows(&513u32.to_le_bytes()).is_err());
    }
    #[test]
    fn shell_paths_preserve_utf16_and_normalize_only_safe_prefixes() {
        let mut source = r"\\?\C:\test\".encode_utf16().collect::<Vec<_>>();
        source.extend([0xd800, b'.' as u16, b't' as u16]);
        let path = OsString::from_wide(&source);
        let actual = shell_path(Path::new(&path)).unwrap();
        assert_eq!(
            actual,
            source[4..]
                .iter()
                .flat_map(|u| u.to_le_bytes())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            shell_path(Path::new(r"\\?\UNC\server\share\file")).unwrap(),
            r"\\server\share\file"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        );
        assert!(shell_path(Path::new(r"\\?\C:\test\unsafe.")).is_err());
        assert!(shell_path(Path::new(r"\\.\C:\test")).is_err());
    }
}
