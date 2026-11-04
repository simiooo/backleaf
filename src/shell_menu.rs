//! Compatible Shell menus run on a separate STA. An invoked external command is never killed as cancellation.
use crate::{
    model::{Event, FrozenSelection},
    operations::{ledger::encode_path, windows::identity},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Read, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, process::CommandExt},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
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
pub fn open(
    selection: FrozenSelection,
    directory: PathBuf,
    owner: usize,
    screen: [i32; 2],
    tx: async_channel::Sender<Event>,
) -> Result<(), String> {
    if selection.mask.count_before(selection.count) > 2048 {
        return Err("系统菜单最多处理 2,048 项".into());
    }
    if ACTIVE
        .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < 4).then_some(n + 1)
        })
        .is_err()
    {
        return Err("仍有应用菜单操作在执行，请稍后重试".into());
    }
    std::thread::spawn(move || {
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
                    f.sync_all()?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            let mut locked = OpenOptions::new().read(true).share_mode(1).open(&helper)?;
            let mut bytes = Vec::new();
            locked.read_to_end(&mut bytes)?;
            anyhow::ensure!(bytes.as_slice() == source, "系统菜单组件已改变");
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
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos();
            let spec = Temp(directory.join(format!("shell-{}-{stamp}.bin", std::process::id())));
            let mut file = File::create_new(&spec.0)?;
            let count = selection.mask.count_before(selection.count);
            file.write_all(&(count as u32).to_le_bytes())?;
            write_path(&mut file, parent)?;
            file.write_all(&parent_id)?;
            let db = rusqlite::Connection::open_with_flags(
                &selection.snapshot.database,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let mut q = db.prepare("SELECT raw,identity FROM files WHERE id=?1")?;
            let mut units = parent.as_os_str().encode_wide().count();
            for id in selection.mask.ids_from(0, selection.count) {
                let (raw, expected): (Vec<u8>, Vec<u8>) =
                    q.query_row([id as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
                let path = parent.join(crate::model::decode_name(&raw));
                units += path.as_os_str().encode_wide().count();
                anyhow::ensure!(units <= 256 * 1024, "系统菜单路径超过 512 KiB 上限");
                let metadata = identity::capture(&path)?;
                anyhow::ensure!(
                    metadata.identity.encode().as_slice() == expected.as_slice(),
                    "目标已改变，请重新打开菜单"
                );
                write_path(&mut file, &path)?;
                file.write_all(&metadata.identity.encode())?;
            }
            file.sync_all()?;
            drop(file);
            let mut child = Command::new(helper)
                .arg(&spec.0)
                .arg(owner.to_string())
                .arg(std::process::id().to_string())
                .arg(screen[0].to_string())
                .arg(screen[1].to_string())
                .creation_flags(0x08000000)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
            let stdout = child.stdout.take().unwrap();
            let (sender, receiver) = mpsc::sync_channel(16);
            std::thread::spawn(move || {
                let mut line = Vec::new();
                for value in BufReader::new(stdout).bytes() {
                    let Ok(value) = value else {
                        break;
                    };
                    if value == b'\n' {
                        if let Ok(s) = std::str::from_utf8(&line)
                            && matches!(s, "shown" | "invoking" | "closed" | "error")
                            && sender.send(s.to_owned()).is_err()
                        {
                            return;
                        }
                        line.clear();
                    } else if line.len() < 4096 {
                        line.push(value);
                    }
                }
            });
            let started = Instant::now();
            let mut displayed = false;
            let mut error = false;
            loop {
                match receiver.recv_timeout(Duration::from_millis(50)) {
                    Ok(s) if s == "shown" || s == "invoking" => displayed = true,
                    Ok(s) if s == "error" => error = true,
                    Ok(_) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if !displayed && started.elapsed() > Duration::from_secs(5) {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!("系统菜单响应超时");
                }
            }
            let exit = child.wait()?;
            anyhow::ensure!(exit.success() && !error, "系统菜单不可用，或目标已改变");
            Ok(())
        })();
        let _ = tx.send_blocking(Event::ShellMenuClosed(result.map_err(|e| e.to_string())));
    });
    Ok(())
}
fn write_path(file: &mut File, path: &std::path::Path) -> anyhow::Result<()> {
    let raw = encode_path(path);
    anyhow::ensure!(!raw.is_empty() && raw.len() / 2 < 32760, "路径过长");
    file.write_all(&((raw.len() / 2) as u32).to_le_bytes())?;
    file.write_all(&raw)?;
    Ok(())
}
