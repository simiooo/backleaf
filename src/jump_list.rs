//! Custom Windows taskbar jump list (right-click menu on the taskbar button).
//!
//! The list mirrors Explorer's layout without recent-document tracking:
//! user-pinned folder entries and saved views from the sidebar, then the home
//! and known folders, then logical drives, and a "new window" task. Entries are
//! shell links to this executable with `--path` / `--view` / `--new-window`
//! arguments, so every item opens a Backleaf window rather than Explorer.
use crate::{
    i18n,
    quick_access::{self, Destination, Entry},
    window_state::{SearchState, State},
};
use anyhow::Context as _;
use std::{
    collections::HashSet,
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::core::Interface;
/// Stable AppUserModelID shared by the process, jump list and shortcuts.
pub const APP_ID: &str = "Simiooo.Backleaf";
const MAX_PINNED: usize = 10;
const MAX_VIEWS: usize = 10;
const MAX_COMMON: usize = 8;
const MAX_VOLUMES: usize = 8;
const PKEY_TITLE: windows::Win32::Foundation::PROPERTYKEY =
    windows::Win32::Foundation::PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0xf29f85e0_4ff9_1068_ab91_08002b27b3d9),
        pid: 2,
    };
const PKEY_APP_USER_MODEL_ID: windows::Win32::Foundation::PROPERTYKEY =
    windows::Win32::Foundation::PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 5,
    };
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Target {
    Path(PathBuf),
    View(String),
    NewWindow,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Link {
    pub title: String,
    pub target: Target,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Model {
    pub pinned: Vec<Link>,
    pub views: Vec<Link>,
    pub common: Vec<Link>,
    pub volumes: Vec<Link>,
}
impl Target {
    /// Command-line arguments for the new process, lossless UTF-16.
    pub fn arguments_wide(&self) -> Vec<u16> {
        match self {
            Self::NewWindow => "--new-window".encode_utf16().collect(),
            Self::View(id) => {
                let mut out: Vec<u16> = "--view ".encode_utf16().collect();
                push_quoted(&mut out, OsStr::new(id));
                out
            }
            Self::Path(path) => {
                let mut out: Vec<u16> = "--path ".encode_utf16().collect();
                push_quoted(&mut out, path.as_os_str());
                out
            }
        }
    }
    #[cfg(test)]
    pub fn arguments(&self) -> String {
        String::from_utf16_lossy(&self.arguments_wide())
    }
}
fn push_quoted(out: &mut Vec<u16>, value: &OsStr) {
    let value: Vec<u16> = value.encode_wide().collect();
    out.push(b'"' as u16);
    out.extend_from_slice(&value);
    // A trailing backslash would escape the closing quote for CommandLineToArgvW.
    if value.last() == Some(&(b'\\' as u16)) {
        out.push(b'\\' as u16);
    }
    out.push(b'"' as u16);
}
/// Sidebar entries split into jump-list categories: user folder pins and saved
/// views. The seeded system shortcuts live in the fixed "常用" category instead.
fn split_entries(entries: Vec<Entry>) -> (Vec<Link>, Vec<Link>) {
    let mut pinned = Vec::new();
    let mut views = Vec::new();
    for entry in entries {
        match &entry.destination {
            Destination::Folder(path) => {
                if !entry.id.starts_with("system-") {
                    pinned.push(Link {
                        title: entry.title,
                        target: Target::Path(path.path()),
                    });
                }
            }
            Destination::View(_) => views.push(Link {
                title: entry.title,
                target: Target::View(entry.id),
            }),
        }
    }
    pinned.truncate(MAX_PINNED);
    views.truncate(MAX_VIEWS);
    (pinned, views)
}
/// Assemble the visible list: sidebar pins and views, known folders and drives.
/// No recent/frequent tracking by design.
pub fn build_model(state_dir: &Path) -> Model {
    let (pinned, views) = split_entries(quick_access::read_entries(state_dir));
    let common: Vec<Link> = crate::navigation::common_locations()
        .into_iter()
        .take(MAX_COMMON)
        .map(|location| Link {
            title: location.title,
            target: Target::Path(location.path),
        })
        .collect();
    let volumes: Vec<Link> = crate::locations::read_volumes()
        .unwrap_or_default()
        .into_iter()
        .take(MAX_VOLUMES)
        .map(|location| Link {
            title: location.title,
            target: Target::Path(location.path),
        })
        .collect();
    Model {
        pinned,
        views,
        common,
        volumes,
    }
}
/// Build the window intent for a saved sidebar view (`--view <id>`).
pub fn saved_view_state(
    state_dir: &Path,
    id: &str,
    width: f32,
    height: f32,
) -> Result<State, String> {
    let entry = quick_access::read_entry(state_dir, id)
        .ok_or_else(|| i18n::tr_owned("无法打开保存的视图"))?;
    let Destination::View(view) = &entry.destination else {
        return Err(i18n::tr_owned("无法打开保存的视图"));
    };
    view.validate()?;
    Ok(State {
        version: 1,
        path: view.root.clone(),
        search: Some(SearchState {
            definition: view.clone(),
            saved: Some(entry),
        }),
        // Invalid sentinel: the window opens centered instead of at the origin.
        bounds: [-100_000., -100_000., width, height],
        maximized: false,
    })
}
pub fn set_app_id() {
    let app = wide(APP_ID);
    unsafe {
        let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
            windows::core::PCWSTR(app.as_ptr()),
        );
    }
}
/// Runs on the UI thread before the first window is created: initialize COM and
/// repair existing shortcuts synchronously so an already pinned taskbar button
/// groups with this process from its first frame.
pub fn prepare() {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }
    repair_shortcuts();
}
static PENDING: Mutex<Option<PathBuf>> = Mutex::new(None);
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Request a rebuild; concurrent requests from several windows coalesce into
/// one background pass with the latest state directory.
pub fn request_update(state_dir: PathBuf) {
    *PENDING.lock().unwrap() = Some(state_dir);
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    if std::thread::Builder::new()
        .name("backleaf-jump-list".into())
        .spawn(|| {
            let _apartment = crate::operations::windows::Apartment::new();
            loop {
                let Some(directory) = PENDING.lock().unwrap().take() else {
                    RUNNING.store(false, Ordering::Release);
                    if PENDING.lock().unwrap().is_some() && !RUNNING.swap(true, Ordering::AcqRel) {
                        continue;
                    }
                    return;
                };
                let model = build_model(&directory);
                // Diagnostic hook for packaging evidence; harmless when unset.
                if let Some(path) = std::env::var_os("BACKLEAF_JUMP_LIST_DUMP")
                    && let Ok(json) = serde_json::to_vec_pretty(&model)
                {
                    let _ = std::fs::write(path, json);
                }
                if let Err(error) = write(&model) {
                    eprintln!("jump list: {error:#}");
                }
            }
        })
        .is_err()
    {
        RUNNING.store(false, Ordering::Release);
    }
}
/// Explorer shows the taskbar jump list for the pinned shortcut's AppUserModelID.
/// Existing shortcuts created before the AUMID existed are repaired in place so
/// an already pinned button starts showing this list after one app launch.
fn repair_shortcuts() {
    let Some(app_data) = std::env::var_os("APPDATA").map(PathBuf::from) else {
        return;
    };
    let start_menu = app_data.join(r"Microsoft\Windows\Start Menu\Programs\Backleaf.lnk");
    let pinned =
        app_data.join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\Backleaf.lnk");
    for path in [start_menu, pinned] {
        if path.is_file()
            && let Err(error) = stamp_shortcut(&path)
        {
            eprintln!("jump list shortcut {}: {error:#}", path.display());
        }
    }
}
fn stamp_shortcut(path: &Path) -> windows::core::Result<()> {
    use windows::Win32::UI::Shell::PropertiesSystem::{
        GPS_READWRITE, IPropertyStore, SHGetPropertyStoreFromParsingName,
    };
    let wide_path = wide_os(path.as_os_str());
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreFromParsingName(
            windows::core::PCWSTR(wide_path.as_ptr()),
            None,
            GPS_READWRITE,
        )?;
        store.SetValue(&PKEY_APP_USER_MODEL_ID, &lpwstr_variant(APP_ID))?;
        store.Commit()?;
    }
    Ok(())
}
/// Shell properties expect `VT_LPWSTR` (not BSTR); the memory is owned by the
/// PROPVARIANT and released by its Drop through `PropVariantClear`.
fn lpwstr_variant(text: &str) -> windows::Win32::System::Com::StructuredStorage::PROPVARIANT {
    use windows::{
        Win32::System::{
            Com::{CoTaskMemAlloc, StructuredStorage::*},
            Variant::VT_LPWSTR,
        },
        core::PWSTR,
    };
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let pointer = unsafe { CoTaskMemAlloc(wide.len() * 2) } as *mut u16;
    unsafe { std::ptr::copy_nonoverlapping(wide.as_ptr(), pointer, wide.len()) };
    PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_LPWSTR,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    pwszVal: PWSTR(pointer),
                },
            }),
        },
    }
}
fn write(model: &Model) -> anyhow::Result<()> {
    use windows::{
        Win32::{
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            UI::Shell::{Common::IObjectArray, DestinationList, ICustomDestinationList},
        },
        core::PCWSTR,
    };
    let executable = std::env::current_exe()?;
    let executable_wide: Vec<u16> = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let executable_ptr = PCWSTR(executable_wide.as_ptr());
    unsafe {
        let list: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        let app = wide(APP_ID);
        list.SetAppID(PCWSTR(app.as_ptr())).context("SetAppID")?;
        let mut slots = 0u32;
        let removed: IObjectArray = list.BeginList(&mut slots).context("BeginList")?;
        let removed_arguments = removed_arguments(&removed);
        let result = (|| -> anyhow::Result<()> {
            let categories: [(String, &Vec<Link>); 4] = [
                (i18n::tr("固定").into(), &model.pinned),
                (i18n::tr("视图").into(), &model.views),
                (i18n::tr("常用").into(), &model.common),
                (i18n::tr("分区").into(), &model.volumes),
            ];
            for (title, links) in categories {
                if let Some(array) = links_array(links, executable_ptr, &removed_arguments)? {
                    let title = wide(&title);
                    list.AppendCategory(PCWSTR(title.as_ptr()), &array)
                        .context("AppendCategory")?;
                }
            }
            let tasks = [Link {
                title: i18n::tr("打开新窗口").into(),
                target: Target::NewWindow,
            }];
            if let Some(array) = links_array(&tasks, executable_ptr, &HashSet::new())? {
                list.AddUserTasks(&array).context("AddUserTasks")?;
            }
            list.CommitList().context("CommitList")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = list.AbortList();
        }
        result
    }
}
unsafe fn links_array(
    links: &[Link],
    executable: windows::core::PCWSTR,
    removed: &HashSet<Vec<u16>>,
) -> anyhow::Result<Option<windows::Win32::UI::Shell::Common::IObjectArray>> {
    use windows::{
        Win32::{
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            UI::Shell::{
                Common::{IObjectArray, IObjectCollection},
                EnumerableObjectCollection,
            },
        },
        core::IUnknown,
    };
    let kept: Vec<(&Link, Vec<u16>)> = links
        .iter()
        .map(|link| (link, link.target.arguments_wide()))
        .filter(|(_, arguments)| !removed.contains(arguments))
        .collect();
    if kept.is_empty() {
        return Ok(None);
    }
    let collection: IObjectCollection =
        unsafe { CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER) }?;
    for (link, arguments) in kept {
        let shell = unsafe { make_link(executable, &link.title, &arguments) }
            .with_context(|| format!("link {}", link.title))?;
        let unknown: IUnknown = shell.cast()?;
        unsafe { collection.AddObject(&unknown) }.context("AddObject")?;
    }
    let array: IObjectArray = collection.cast()?;
    Ok(Some(array))
}
unsafe fn make_link(
    executable: windows::core::PCWSTR,
    title: &str,
    arguments: &[u16],
) -> windows::core::Result<windows::Win32::UI::Shell::IShellLinkW> {
    use windows::{
        Win32::{
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            UI::Shell::{IShellLinkW, PropertiesSystem::IPropertyStore, ShellLink},
        },
        core::PCWSTR,
    };
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }?;
    unsafe { link.SetPath(executable) }?;
    let arguments: Vec<u16> = arguments.iter().copied().chain(Some(0)).collect();
    unsafe { link.SetArguments(PCWSTR(arguments.as_ptr())) }?;
    let description = wide(title);
    unsafe { link.SetDescription(PCWSTR(description.as_ptr())) }?;
    unsafe { link.SetIconLocation(executable, 0) }?;
    let store: IPropertyStore = link.cast()?;
    let title_value = lpwstr_variant(title);
    unsafe { store.SetValue(&PKEY_TITLE, &title_value) }?;
    unsafe { store.Commit() }?;
    Ok(link)
}
unsafe fn removed_arguments(
    removed: &windows::Win32::UI::Shell::Common::IObjectArray,
) -> HashSet<Vec<u16>> {
    use windows::Win32::UI::Shell::IShellLinkW;
    let mut set = HashSet::new();
    let Ok(count) = (unsafe { removed.GetCount() }) else {
        return set;
    };
    for index in 0..count {
        let Ok(link) = (unsafe { removed.GetAt::<IShellLinkW>(index) }) else {
            continue;
        };
        let mut buffer = [0u16; 1024];
        if unsafe { link.GetArguments(&mut buffer) }.is_ok() {
            let length = buffer.iter().position(|unit| *unit == 0).unwrap_or(0);
            if length > 0 {
                set.insert(buffer[..length].to_vec());
            }
        }
    }
    set
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn wide_os(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_view::{FileView, WinPath};
    fn folder(path: &str) -> Destination {
        Destination::Folder(WinPath::new(Path::new(path)))
    }
    fn entry(id: &str, title: &str, destination: Destination) -> Entry {
        Entry {
            id: id.into(),
            title: title.into(),
            destination,
            revision: 1,
        }
    }
    #[test]
    fn links_carry_lossless_quoted_arguments() {
        assert_eq!(
            Target::Path(PathBuf::from(r"C:\Users\例子\项目")).arguments(),
            r#"--path "C:\Users\例子\项目""#
        );
        assert_eq!(
            Target::Path(PathBuf::from(r"D:\work\")).arguments(),
            r#"--path "D:\work\\""#
        );
        assert_eq!(
            Target::View("view-1".into()).arguments(),
            r#"--view "view-1""#
        );
        assert_eq!(Target::NewWindow.arguments(), "--new-window");
    }
    #[test]
    fn sidebar_pins_and_views_become_separate_categories_without_system_entries() {
        let mut view = FileView::new(Path::new(r"C:\Users\me"));
        view.query = "报告".into();
        let (pinned, views) = split_entries(vec![
            entry("system-0", "个人文件夹", folder(r"C:\Users\me")),
            entry("folder-1", "work", folder(r"D:\work")),
            entry("view-2", "最近文档", Destination::View(view)),
        ]);
        assert_eq!(
            pinned,
            vec![Link {
                title: "work".into(),
                target: Target::Path(PathBuf::from(r"D:\work")),
            }]
        );
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].title, "最近文档");
        assert_eq!(views[0].target, Target::View("view-2".into()));
    }
    #[test]
    fn saved_view_state_restores_the_view_and_uses_centering_sentinel() {
        let fixture = crate::test_support::Fixture::new("jump-view");
        let mut view = FileView::new(&fixture.0);
        view.query = "报告".into();
        let payload = serde_json::to_string(&Destination::View(view.clone())).unwrap();
        let connection = rusqlite::Connection::open(fixture.0.join("quick-access.sqlite")).unwrap();
        connection
            .execute_batch("CREATE TABLE entries(id TEXT PRIMARY KEY,title TEXT NOT NULL,payload TEXT NOT NULL,revision INTEGER NOT NULL,position INTEGER NOT NULL);")
            .unwrap();
        connection
            .execute(
                "INSERT INTO entries VALUES('view-1','视图',?1,1,0)",
                [payload],
            )
            .unwrap();
        drop(connection);
        let state = saved_view_state(&fixture.0, "view-1", 1120., 720.).unwrap();
        assert_eq!(state.path, WinPath::new(&fixture.0));
        assert_eq!(
            state.search.as_ref().map(|search| &search.definition),
            Some(&view)
        );
        assert_eq!(state.bounds, [-100_000., -100_000., 1120., 720.]);
        assert!(saved_view_state(&fixture.0, "missing", 1., 1.).is_err());
    }
}
