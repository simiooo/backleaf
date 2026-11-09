//! Administrator handoff for locations the medium-integrity process cannot read.
//!
//! Windows elevation is a per-process token: a running process cannot acquire the
//! administrator token later and there is no TTL. Backleaf therefore keeps the
//! normal window untouched, writes the window intent to a one-shot request file
//! and starts another Backleaf process through `ShellExecuteExW("runas")`. That
//! elevated window keeps the permission for its lifetime; closing it ends the
//! elevated process. See `docs/elevation.md`.
use crate::{file_view::WinPath, selection::SelectionMask, window_state::State};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
/// Request schema version. Bump together with `Request::validate`.
pub const REQUEST_VERSION: u32 = 1;
/// The supported window-state schema (see `window_state::State`).
const STATE_VERSION: u32 = 1;
/// Restoring huge selections would make the handoff file unbounded and the
/// elevated window fragile; larger selections open without a carried selection.
pub const MAX_RESTORED_SELECTION: usize = 4096;
/// Directory worker access denial (ERROR_ACCESS_DENIED) and privilege denial.
const ACCESS_DENIED_TEXT: [&str; 3] = ["拒绝访问", "Access is denied", "access denied"];
/// A one-shot handoff written to the state directory and consumed by the
/// elevated process. Paths are lossless UTF-16 (see [`WinPath`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub state: State,
    #[serde(default)]
    pub selection: Vec<WinPath>,
    #[serde(default)]
    pub scroll_row: u64,
}
/// The part of a request that must survive into the newly opened window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RestoreState {
    pub selection: Vec<WinPath>,
    pub scroll_row: u64,
}
/// Snapshot pieces needed to turn a live selection into restartable paths.
/// Captured on the UI thread without touching SQLite; the launch thread reads it.
pub struct SelectionHandoff {
    pub database: PathBuf,
    pub parent: PathBuf,
    pub mask: SelectionMask,
    pub count: usize,
}
impl Request {
    pub fn new(state: State, selection: Vec<WinPath>, scroll_row: u64) -> Self {
        Self {
            version: REQUEST_VERSION,
            state,
            selection,
            scroll_row,
        }
    }
    pub fn restore(&self) -> RestoreState {
        RestoreState {
            selection: self.selection.clone(),
            scroll_row: self.scroll_row,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != REQUEST_VERSION {
            return Err(crate::i18n::tr_owned("管理员请求版本不兼容"));
        }
        if self.state.version != STATE_VERSION {
            return Err(crate::i18n::tr_owned("管理员请求版本不兼容"));
        }
        if self.state.bounds.iter().any(|value| !value.is_finite()) {
            return Err(crate::i18n::tr_owned("无法读取管理员请求"));
        }
        if self.state.path.0.is_empty() || self.state.path.0.contains(&0) {
            return Err(crate::i18n::tr_owned("无法读取管理员请求"));
        }
        if self.selection.len() > MAX_RESTORED_SELECTION {
            return Err(crate::i18n::tr_owned("无法读取管理员请求"));
        }
        for selected in &self.selection {
            if selected.0.is_empty()
                || selected.0.contains(&0)
                || selected.path().file_name().is_none()
            {
                return Err(crate::i18n::tr_owned("无法读取管理员请求"));
            }
        }
        Ok(())
    }
}
pub fn is_elevated() -> bool {
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut length = 0u32;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        );
        let _ = CloseHandle(token);
        result.is_ok() && elevation.TokenIsElevated != 0
    }
}
/// HRESULTs that mean "this token lacks access", not "the file is busy".
/// Kept narrow on purpose: privilege-not-held (0x80070522) is not necessarily
/// fixed by an administrator token and must not raise the handoff prompt.
pub fn hresult_access_denied(code: i32) -> bool {
    code as u32 == 0x8007_0005
}
pub fn io_access_denied(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(5)
        || (error.kind() == std::io::ErrorKind::PermissionDenied && error.raw_os_error().is_none())
}
/// Keep the Win32 code when a Shell/file API only exposes a formatted message.
pub fn io_from_windows(error: windows::core::Error) -> std::io::Error {
    let code = error.code().0 as u32;
    if code & 0xffff_0000 == 0x8007_0000 {
        std::io::Error::from_raw_os_error((code & 0xffff) as i32)
    } else {
        std::io::Error::other(error.to_string())
    }
}
/// Walk the whole context chain: a snapshot failure can be wrapped several times.
pub fn error_access_denied(error: &anyhow::Error) -> bool {
    for cause in error.chain() {
        if let Some(io) = cause.downcast_ref::<std::io::Error>()
            && io_access_denied(io)
        {
            return true;
        }
        if let Some(windows) = cause.downcast_ref::<windows::core::Error>()
            && hresult_access_denied(windows.code().0)
        {
            return true;
        }
    }
    let text = error.to_string().to_lowercase();
    ACCESS_DENIED_TEXT
        .iter()
        .any(|needle| text.contains(needle.to_lowercase().as_str()))
}
/// Create a new request file under `<state_dir>/elevation` without exposing a
/// partially written file to the elevated process.
pub fn write_request(state_dir: &Path, request: &Request) -> Result<PathBuf, String> {
    request.validate()?;
    let state_dir = absolute(state_dir);
    let directory = state_dir.join("elevation");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    prune_requests(&directory);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = directory.join(format!("request-{}-{stamp}.json", std::process::id()));
    let temporary = directory.join(format!("request-{}-{stamp}.tmp", std::process::id()));
    let bytes = serde_json::to_vec(request).map_err(|error| error.to_string())?;
    std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, &path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        error.to_string()
    })?;
    Ok(path)
}
fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}
/// UAC cancellation or a crash before the elevated process starts must not
/// leave handoff files around forever.
fn prune_requests(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("request-") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
            continue;
        };
        if now.duration_since(modified).unwrap_or_default()
            > std::time::Duration::from_secs(24 * 60 * 60)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
pub fn read_request(path: &Path) -> Result<Request, String> {
    let bytes = std::fs::read(path).map_err(|_| crate::i18n::tr_owned("无法读取管理员请求"))?;
    let request: Request =
        serde_json::from_slice(&bytes).map_err(|_| crate::i18n::tr_owned("无法读取管理员请求"))?;
    request.validate()?;
    Ok(request)
}
/// Command-line arguments for the elevated instance. Values are Win32-quoted so
/// a state directory containing spaces survives ShellExecuteEx parsing.
pub fn request_arguments(request: &Path, state_dir: &Path) -> String {
    format!(
        "--elevate-request {} --state-dir {}",
        quote_argument(&request.to_string_lossy()),
        quote_argument(&state_dir.to_string_lossy())
    )
}
pub fn quote_argument(value: &str) -> String {
    if !value.is_empty() && !value.chars().any(|c| matches!(c, ' ' | '\t' | '"')) {
        return value.to_owned();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for character in value.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                // Backslashes before a quote must be doubled, then escape the quote.
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(character);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}
/// Ask Windows for an administrator token and start a second Backleaf process
/// for `request`. Runs on a background thread because the UAC prompt blocks.
pub fn launch_elevated(request_file: &Path, state_dir: &Path) -> Result<(), String> {
    #[cfg(test)]
    {
        // Headless tests must never open a real UAC prompt; the request file and
        // event roundtrip are still exercised.
        let _ = (request_file, state_dir);
        Ok(())
    }
    #[cfg(not(test))]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::{
                Foundation::{CloseHandle, ERROR_CANCELLED},
                UI::{
                    Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
                    WindowsAndMessaging::SW_SHOWNORMAL,
                },
            },
            core::PCWSTR,
        };
        let executable = std::env::current_exe()
            .map_err(|error| crate::trf!("无法打开管理员窗口：{}", error))?;
        let arguments = request_arguments(request_file, &absolute(state_dir));
        let executable_wide = executable.as_os_str().encode_wide().collect::<Vec<_>>();
        let arguments_wide = arguments.encode_utf16().collect::<Vec<_>>();
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: windows::core::w!("runas"),
            lpFile: PCWSTR(executable_wide.as_ptr()),
            lpParameters: PCWSTR(arguments_wide.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        match unsafe { ShellExecuteExW(&mut info) } {
            Ok(()) => {
                if !info.hProcess.is_invalid() {
                    let _ = unsafe { CloseHandle(info.hProcess) };
                }
                Ok(())
            }
            Err(error) => {
                if error.code() == windows::core::HRESULT::from_win32(ERROR_CANCELLED.0) {
                    Err(crate::i18n::tr_owned("已取消管理员权限请求"))
                } else {
                    Err(crate::trf!("无法打开管理员窗口：{}", error.message()))
                }
            }
        }
    }
}
/// Resolve selected snapshot ids to lossless absolute paths on a worker thread.
/// A selection larger than [`MAX_RESTORED_SELECTION`] is deliberately dropped.
pub fn capture_selection(handoff: &SelectionHandoff) -> Vec<WinPath> {
    if handoff.mask.count() > MAX_RESTORED_SELECTION {
        return Vec::new();
    }
    let connection = match rusqlite::Connection::open_with_flags(
        &handoff.database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(connection) => connection,
        Err(_) => return Vec::new(),
    };
    if connection
        .busy_timeout(std::time::Duration::from_millis(250))
        .is_err()
    {
        return Vec::new();
    }
    let mut statement = match connection.prepare("SELECT raw FROM files WHERE id=?1") {
        Ok(statement) => statement,
        Err(_) => return Vec::new(),
    };
    let mut paths = Vec::new();
    for id in handoff.mask.ids_from(0, handoff.count) {
        if paths.len() >= MAX_RESTORED_SELECTION {
            return Vec::new();
        }
        let raw: Vec<u8> = match statement.query_row([id as i64], |row| row.get(0)) {
            Ok(raw) => raw,
            Err(_) => return Vec::new(),
        };
        let name = crate::model::decode_name(&raw);
        let path = handoff.parent.join(&name);
        if path.parent() != Some(handoff.parent.as_path()) {
            return Vec::new();
        }
        paths.push(WinPath::new(&path));
    }
    paths
}
/// Map the carried paths back to ids in the newly enumerated snapshot.
/// Names are queried exactly as stored (lossless UTF-16 bytes).
pub fn resolve_selection(database: &Path, selection: &[WinPath]) -> Vec<usize> {
    if selection.is_empty() {
        return Vec::new();
    }
    let connection = match rusqlite::Connection::open_with_flags(
        database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(connection) => connection,
        Err(_) => return Vec::new(),
    };
    if connection
        .busy_timeout(std::time::Duration::from_millis(250))
        .is_err()
    {
        return Vec::new();
    }
    let mut statement = match connection.prepare("SELECT id FROM files WHERE raw=?1") {
        Ok(statement) => statement,
        Err(_) => return Vec::new(),
    };
    let mut ids = Vec::new();
    for selected in selection {
        let Some(name) = selected.path().file_name().map(std::ffi::OsString::from) else {
            continue;
        };
        let raw = crate::model::encode_name(&name);
        if let Ok(id) = statement.query_row([raw], |row| row.get::<_, i64>(0)) {
            ids.push(id as usize);
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_view::WinPath;
    fn state(path: &Path) -> State {
        State {
            version: STATE_VERSION,
            path: WinPath::new(path),
            search: None,
            bounds: [10., 20., 900., 600.],
            maximized: false,
        }
    }
    #[test]
    fn access_denied_is_narrower_than_any_io_error() {
        assert!(io_access_denied(&std::io::Error::from_raw_os_error(5)));
        assert!(!io_access_denied(&std::io::Error::from_raw_os_error(32)));
        assert!(!io_access_denied(&std::io::Error::other("拒绝访问")));
        assert!(hresult_access_denied(0x8007_0005u32 as i32));
        assert!(!hresult_access_denied(0x8007_0522u32 as i32));
        assert!(!hresult_access_denied(0x8007_0020u32 as i32));
        assert!(error_access_denied(&anyhow::anyhow!(
            std::io::Error::from_raw_os_error(5)
        )));
        assert!(!error_access_denied(&anyhow::anyhow!(
            std::io::Error::from_raw_os_error(32)
        )));
    }
    #[test]
    fn arguments_keep_spaces_trailing_backslashes_and_quotes() {
        assert_eq!(quote_argument(r"C:\plain"), r"C:\plain");
        assert_eq!(quote_argument(r"C:\two words\x"), r#""C:\two words\x""#);
        // A bare argument needs no quoting, so its trailing slash stays as is.
        assert_eq!(quote_argument(r"C:\trailing\"), r"C:\trailing\");
        assert_eq!(quote_argument(r"C:\two words\"), r#""C:\two words\\""#);
        assert_eq!(quote_argument(r#"C:\a"b"#), r#""C:\a\"b""#);
        let arguments = request_arguments(
            Path::new(r"C:\state dir\request-1.json"),
            Path::new(r"C:\state dir"),
        );
        assert_eq!(
            arguments,
            r#"--elevate-request "C:\state dir\request-1.json" --state-dir "C:\state dir""#
        );
    }
    #[test]
    fn request_roundtrip_keeps_lossless_paths_and_rejects_invalid_payload() {
        let fixture = crate::test_support::Fixture::new("elevation-request");
        let parent = fixture.0.join("protected");
        std::fs::create_dir(&parent).unwrap();
        let selected = parent.join("报告.txt");
        let mut request = Request::new(state(&parent), vec![WinPath::new(&selected)], 7);
        request.validate().unwrap();
        let path = write_request(&fixture.0, &request).unwrap();
        assert_eq!(read_request(&path).unwrap(), request);
        std::fs::remove_file(&path).unwrap();
        request.state.bounds = [0., 0., f32::NAN, 600.];
        assert!(request.validate().is_err());
        request.state.bounds = [0., 0., 900., 600.];
        request.selection = vec![WinPath(Vec::new())];
        assert!(request.validate().is_err());
        request.selection.clear();
        request.version = REQUEST_VERSION + 1;
        assert!(request.validate().is_err());
    }
    #[test]
    fn selection_capture_and_resolve_roundtrip_by_identity() {
        let fixture = crate::test_support::Fixture::new("elevation-selection");
        let parent = fixture.0.join("folder");
        std::fs::create_dir(&parent).unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(parent.join(name), name).unwrap();
        }
        let (tx, rx) = async_channel::unbounded();
        let source = crate::model::Source::directory(parent.clone(), 2, tx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let count = loop {
            assert!(std::time::Instant::now() < deadline, "directory timeout");
            match rx.try_recv() {
                Ok(crate::model::Event::Count {
                    count,
                    done: true,
                    error: None,
                    ..
                }) => break count,
                Ok(crate::model::Event::Count {
                    error: Some(error), ..
                }) => panic!("{error}"),
                Ok(_) | Err(async_channel::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(async_channel::TryRecvError::Closed) => panic!("enumerator disconnected"),
            }
        };
        let mut values = roaring::RoaringTreemap::new();
        values.insert(1);
        let handoff = source
            .selection_handoff(&crate::selection::SelectionMask::from_bitmap(values), count)
            .expect("selection handoff");
        let selected = capture_selection(&handoff);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].path().parent(), Some(handoff.parent.as_path()));
        assert_eq!(resolve_selection(&handoff.database, &selected), vec![1]);
    }
}
