//! Standard Shell clipboard formats on one message-pumping STA; no private paste-only fallback.
use super::{
    ledger::Ledger,
    windows::{self as shell, identity},
};
use std::{
    ffi::OsString,
    marker::PhantomData,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM},
        System::{
            Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, TYMED_HGLOBAL},
            DataExchange::*,
            LibraryLoader::GetModuleHandleW,
            Memory::*,
            Ole::{
                CF_HDROP, CF_UNICODETEXT, OleGetClipboard, OleInitialize, OleSetClipboard,
                OleUninitialize, ReleaseStgMedium,
            },
        },
        UI::{
            Shell::{DROPFILES, DragQueryFileW, HDROP},
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, Error, w},
};

pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024 * 1024;
struct OleApartment(PhantomData<Rc<()>>);
impl OleApartment {
    fn new() -> windows::core::Result<Self> {
        unsafe {
            OleInitialize(None)?;
        }
        Ok(Self(PhantomData))
    }
}
impl Drop for OleApartment {
    fn drop(&mut self) {
        unsafe { OleUninitialize() }
    }
}
struct Global(HGLOBAL);
impl Global {
    fn new(bytes: usize) -> windows::core::Result<Self> {
        unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes).map(Self) }
    }
    fn handle(&self) -> HANDLE {
        HANDLE(self.0.0)
    }
    fn disown(&mut self) {
        self.0 = HGLOBAL::default();
    }
}
impl Drop for Global {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            unsafe {
                let _ = GlobalFree(Some(self.0));
            }
        }
    }
}
struct Unlock(HGLOBAL);
impl Drop for Unlock {
    fn drop(&mut self) {
        unsafe {
            let _ = GlobalUnlock(self.0);
        }
    }
}
struct Medium(STGMEDIUM);
impl Drop for Medium {
    fn drop(&mut self) {
        unsafe {
            ReleaseStgMedium(&mut self.0);
        }
    }
}
struct ClipboardOpen;
impl Drop for ClipboardOpen {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}
struct Payload {
    block: Option<Global>,
    pointer: *mut u8,
    cursor: usize,
    size: usize,
}
impl Payload {
    fn new(size: usize) -> anyhow::Result<Self> {
        anyhow::ensure!(
            (std::mem::size_of::<DROPFILES>() + 2..=MAX_PAYLOAD_BYTES).contains(&size),
            "无法准备剪贴板，请减少选择后重试"
        );
        let block = Global::new(size)?;
        let pointer = unsafe { GlobalLock(block.0) }.cast::<u8>();
        anyhow::ensure!(!pointer.is_null(), "Clipboard allocation cannot be locked");
        unsafe {
            pointer.cast::<DROPFILES>().write(DROPFILES {
                pFiles: std::mem::size_of::<DROPFILES>() as u32,
                fWide: BOOL(1),
                ..Default::default()
            });
        }
        Ok(Self {
            block: Some(block),
            pointer,
            cursor: std::mem::size_of::<DROPFILES>(),
            size,
        })
    }
    fn push(&mut self, path: &[u16]) -> anyhow::Result<()> {
        anyhow::ensure!(
            path.len() > 1 && path.last() == Some(&0) && !path[..path.len() - 1].contains(&0),
            "Invalid clipboard path"
        );
        anyhow::ensure!(
            self.cursor + path.len() * 2 + 2 <= self.size,
            "Clipboard manifest changed during rendering"
        );
        unsafe {
            std::ptr::copy_nonoverlapping(
                path.as_ptr().cast::<u8>(),
                self.pointer.add(self.cursor),
                path.len() * 2,
            );
        }
        self.cursor += path.len() * 2;
        Ok(())
    }
    fn finish(mut self) -> anyhow::Result<Global> {
        anyhow::ensure!(
            self.cursor + 2 == self.size,
            "Clipboard manifest changed during rendering"
        );
        unsafe {
            self.pointer
                .add(self.cursor)
                .cast::<u16>()
                .write_unaligned(0);
            let _ = GlobalUnlock(self.block.as_ref().unwrap().0);
        }
        self.pointer = std::ptr::null_mut();
        Ok(self.block.take().unwrap())
    }
}
impl Drop for Payload {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            unsafe {
                let _ = GlobalUnlock(self.block.as_ref().unwrap().0);
            }
        }
    }
}
fn format(id: u32) -> anyhow::Result<FORMATETC> {
    Ok(FORMATETC {
        cfFormat: u16::try_from(id)?,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    })
}
fn data(object: &IDataObject, id: u32) -> anyhow::Result<Medium> {
    let value = Medium(unsafe { object.GetData(&format(id)?) }?);
    anyhow::ensure!(
        value.0.tymed == TYMED_HGLOBAL.0 as u32,
        "Clipboard medium is not global memory"
    );
    Ok(value)
}
fn dword(value: u32) -> anyhow::Result<Global> {
    let block = Global::new(4)?;
    let pointer = unsafe { GlobalLock(block.0) }.cast::<u32>();
    anyhow::ensure!(!pointer.is_null(), "Cannot lock clipboard effect");
    unsafe {
        pointer.write_unaligned(value);
        let _ = GlobalUnlock(block.0);
    }
    Ok(block)
}
/// Validate the bounded array before asking DragQueryFileW to parse it.
pub(crate) fn checked_drop_count(handle: HGLOBAL) -> anyhow::Result<u32> {
    let size = unsafe { GlobalSize(handle) };
    anyhow::ensure!(
        (std::mem::size_of::<DROPFILES>()..=MAX_PAYLOAD_BYTES).contains(&size),
        "无法读取剪贴板，请减少选择后重试"
    );
    let pointer = unsafe { GlobalLock(handle) }.cast::<u8>();
    anyhow::ensure!(!pointer.is_null(), "Cannot lock clipboard data");
    let _unlock = Unlock(handle);
    let header = unsafe { pointer.cast::<DROPFILES>().read_unaligned() };
    let offset = header.pFiles as usize;
    anyhow::ensure!(
        offset >= std::mem::size_of::<DROPFILES>() && offset < size,
        "Malformed clipboard header"
    );
    let bytes = unsafe { std::slice::from_raw_parts(pointer.add(offset), size - offset) };
    let width = if header.fWide.as_bool() { 2 } else { 1 };
    anyhow::ensure!(
        offset.is_multiple_of(width) && bytes.len() >= width * 2,
        "Malformed clipboard array"
    );
    let mut position = 0;
    let mut count = 0;
    let mut ended = false;
    while position + width <= bytes.len() {
        if bytes[position..position + width].iter().all(|v| *v == 0) {
            ended = true;
            break;
        }
        let start = position;
        while position + width <= bytes.len()
            && bytes[position..position + width].iter().any(|v| *v != 0)
        {
            position += width;
        }
        anyhow::ensure!(
            position + width <= bytes.len() && (position - start) / width <= 32767,
            "Malformed clipboard path"
        );
        count += 1;
        position += width;
    }
    anyhow::ensure!(
        ended && unsafe { DragQueryFileW(HDROP(handle.0), u32::MAX, None) } == count,
        "Incomplete clipboard array"
    );
    Ok(count)
}
/// Import a drop object on its receiving STA. No clipboard state is read or changed.
pub fn import_object(
    object: &IDataObject,
    ledger: &Ledger,
    canceled: impl Fn() -> bool,
) -> anyhow::Result<(u64, u64, bool)> {
    let medium = data(object, CF_HDROP.0 as u32)?;
    let handle = unsafe { medium.0.u.hGlobal };
    let count = checked_drop_count(handle)?;
    anyhow::ensure!(count > 0, "拖放中没有文件");
    let mut bytes = 0u64;
    let mut directories = false;
    ledger.begin()?;
    for index in 0..count {
        anyhow::ensure!(!canceled(), "已取消");
        let length = unsafe { DragQueryFileW(HDROP(handle.0), index, None) } as usize;
        anyhow::ensure!(length > 0 && length <= 32767, "拖放路径无效");
        let mut text = vec![0u16; length + 1];
        anyhow::ensure!(
            unsafe { DragQueryFileW(HDROP(handle.0), index, Some(&mut text)) } as usize == length,
            "拖放数据已改变"
        );
        let path = PathBuf::from(OsString::from_wide(&text[..length]));
        anyhow::ensure!(path.is_absolute(), "拖放路径必须是绝对路径");
        let metadata = identity::capture(&path)?;
        bytes = bytes.saturating_add(metadata.bytes.unwrap_or(0));
        directories |= metadata.directory;
        ledger.add_root(index as u64, &path, Some(&metadata))?;
        if index % 256 == 255 {
            ledger.commit()?;
            ledger.begin()?;
        }
    }
    ledger.commit()?;
    Ok((count as u64, bytes, directories))
}
/// Build standard CF_HDROP only when an external target requests it, off the GPUI thread.
pub fn render_drop(ledger: &Ledger, canceled: impl Fn() -> bool) -> anyhow::Result<HGLOBAL> {
    let mut size = std::mem::size_of::<DROPFILES>() + 2;
    let mut count = 0;
    ledger.visit_roots(false, |root| {
        anyhow::ensure!(!canceled(), "已取消");
        anyhow::ensure!(
            root.identity.as_ref() == Some(&identity::capture(&root.path)?.identity),
            "源文件已改变"
        );
        let path = shell::clipboard_path(&root.path)?;
        size = size
            .checked_add(path.len() * 2)
            .ok_or_else(|| anyhow::anyhow!("拖放数据过大"))?;
        anyhow::ensure!(size <= MAX_PAYLOAD_BYTES, "拖放路径超过256 MiB，请减少选择");
        count += 1;
        Ok(())
    })?;
    anyhow::ensure!(count > 0, "没有可拖放的文件");
    let mut payload = Payload::new(size)?;
    ledger.visit_roots(false, |root| {
        anyhow::ensure!(!canceled(), "已取消");
        anyhow::ensure!(
            root.identity.as_ref() == Some(&identity::capture(&root.path)?.identity),
            "源文件已改变"
        );
        payload.push(&shell::clipboard_path(&root.path)?)
    })?;
    let mut block = payload.finish()?;
    let result = block.0;
    block.disown();
    Ok(result)
}
unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let creation = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, creation.lpCreateParams as isize);
        }
        if message == WM_CLIPBOARDUPDATE {
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const AtomicBool;
            if !pointer.is_null() {
                (*pointer).store(true, Ordering::Release);
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}
pub struct Input {
    pub _origin: IDataObject,
    pub sequence: u32,
    pub move_requested: bool,
    pub count: u64,
    pub known_file_bytes: u64,
    pub has_directories: bool,
    _apartment: PhantomData<Rc<()>>,
}
pub struct Clipboard {
    owner: HWND,
    changed: Box<AtomicBool>,
    preferred: u32,
    _ole: OleApartment,
}
impl Clipboard {
    pub fn read_text_limited(&self, max_utf16: usize) -> anyhow::Result<Option<String>> {
        let _opened = self.open(|| false)?;
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32) }.is_err() {
            return Ok(None);
        }
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT.0 as u32) }?;
        let handle = HGLOBAL(handle.0);
        let size = unsafe { GlobalSize(handle) };
        anyhow::ensure!(size <= max_utf16.saturating_add(1) * 2, "粘贴的文字过长");
        let pointer = unsafe { GlobalLock(handle) }.cast::<u16>();
        anyhow::ensure!(!pointer.is_null(), "无法读取剪贴板");
        let _unlock = Unlock(handle);
        let text = unsafe { std::slice::from_raw_parts(pointer, size / 2) };
        let end = text
            .iter()
            .position(|c| *c == 0)
            .ok_or_else(|| anyhow::anyhow!("剪贴板文字格式无效"))?;
        Ok(Some(String::from_utf16(&text[..end])?))
    }
    pub fn read_own_files(
        &self,
        ledger: &Ledger,
        source: &Ledger,
        expected_sequence: u32,
        cut: bool,
        canceled: impl Fn() -> bool,
        mut progress: impl FnMut(u64, u64),
    ) -> anyhow::Result<Input> {
        anyhow::ensure!(
            Self::sequence() == expected_sequence,
            "剪贴板已改变，请重新粘贴"
        );
        let origin = unsafe { OleGetClipboard() }?;
        anyhow::ensure!(
            Self::sequence() == expected_sequence,
            "剪贴板已改变，请重新粘贴"
        );
        let mut count = 0;
        source.visit_roots(cut, |_| {
            count += 1;
            Ok(())
        })?;
        anyhow::ensure!(count > 0, "剪贴板中没有文件");
        let mut prepared = 0;
        let mut known_file_bytes = 0u64;
        let mut has_directories = false;
        ledger.begin()?;
        source.visit_roots(cut, |root| {
            if canceled() {
                anyhow::bail!("已取消");
            }
            let metadata = root.identity.map(|identity| identity::Metadata {
                identity,
                directory: root.directory,
                bytes: root.bytes,
                reparse: false,
                modified: 0,
                modified_raw: 0,
                attributes: 0,
            });
            ledger.add_root(root.key, &root.path, metadata.as_ref())?;
            has_directories |= root.directory;
            known_file_bytes = known_file_bytes
                .checked_add(root.bytes.unwrap_or(0))
                .ok_or_else(|| anyhow::anyhow!("总大小超出支持范围，请减少选择"))?;
            prepared += 1;
            if prepared % 256 == 0 {
                ledger.commit()?;
                progress(prepared, count);
                ledger.begin()?;
            }
            Ok(())
        })?;
        ledger.commit()?;
        progress(prepared, count);
        Ok(Input {
            _origin: origin,
            sequence: expected_sequence,
            move_requested: cut,
            count,
            known_file_bytes,
            has_directories,
            _apartment: PhantomData,
        })
    }
    pub fn new() -> anyhow::Result<Self> {
        let ole = OleApartment::new()?;
        let changed = Box::new(AtomicBool::new(true));
        unsafe {
            let module = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: module.into(),
                lpszClassName: w!("BackleafClipboardNotifications"),
                ..Default::default()
            };
            RegisterClassW(&class);
            let owner = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class.lpszClassName,
                w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(module.into()),
                Some(changed.as_ref() as *const AtomicBool as *const _),
            )?;
            if let Err(error) = AddClipboardFormatListener(owner) {
                let _ = DestroyWindow(owner);
                return Err(error.into());
            }
            let preferred = RegisterClipboardFormatW(w!("Preferred DropEffect"));
            if preferred == 0 {
                let error = Error::from_thread();
                let _ = RemoveClipboardFormatListener(owner);
                let _ = DestroyWindow(owner);
                return Err(error.into());
            }
            Ok(Self {
                owner,
                changed,
                preferred,
                _ole: ole,
            })
        }
    }
    pub fn pump(&self) -> bool {
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        self.changed.swap(false, Ordering::AcqRel)
    }
    pub fn sequence() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }
    pub fn available() -> bool {
        unsafe { IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_ok() }
    }
    fn open(&self, canceled: impl Fn() -> bool) -> anyhow::Result<ClipboardOpen> {
        for _ in 0..20 {
            if canceled() {
                anyhow::bail!("已取消");
            }
            if unsafe { OpenClipboard(Some(self.owner)) }.is_ok() {
                return Ok(ClipboardOpen);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        anyhow::bail!("剪贴板正忙，请重试")
    }
    pub fn write_files(
        &self,
        ledger: &Ledger,
        cut: bool,
        remaining: bool,
        expected_sequence: Option<u32>,
        canceled: impl Fn() -> bool,
    ) -> anyhow::Result<u32> {
        let mut size = std::mem::size_of::<DROPFILES>() + 2;
        let mut count = 0;
        ledger.visit_roots(remaining, |root| {
            if canceled() {
                anyhow::bail!("已取消");
            }
            let path = shell::clipboard_path(&root.path)?;
            size = size
                .checked_add(path.len() * 2)
                .ok_or_else(|| anyhow::anyhow!("Clipboard overflow"))?;
            anyhow::ensure!(
                size <= MAX_PAYLOAD_BYTES,
                "无法准备剪贴板，请减少选择后重试"
            );
            count += 1;
            Ok(())
        })?;
        anyhow::ensure!(count > 0, "没有可复制的文件");
        let mut payload = Payload::new(size)?;
        ledger.visit_roots(remaining, |root| {
            if canceled() {
                anyhow::bail!("已取消");
            }
            payload.push(&shell::clipboard_path(&root.path)?)?;
            Ok(())
        })?;
        let mut payload = payload.finish()?;
        let mut effect = dword(if cut { 2 } else { 1 })?;
        // All allocation/rendering completes before EmptyClipboard. Keep an OLE reference for exceptional publication failure.
        let previous = unsafe { OleGetClipboard() }.ok();
        let opened = self.open(&canceled)?;
        anyhow::ensure!(
            expected_sequence.is_none_or(|sequence| Self::sequence() == sequence),
            "剪贴板已改变，未覆盖当前内容"
        );
        let result = (|| -> windows::core::Result<()> {
            unsafe {
                EmptyClipboard()?;
                SetClipboardData(CF_HDROP.0 as u32, Some(payload.handle()))?;
                payload.disown();
                SetClipboardData(self.preferred, Some(effect.handle()))?;
                effect.disown();
            }
            Ok(())
        })();
        drop(opened);
        if let Err(error) = result {
            if let Some(previous) = previous {
                let restored = unsafe { OleSetClipboard(&previous) };
                anyhow::ensure!(restored.is_ok(), "无法写入或恢复剪贴板：{error}");
            }
            return Err(error.into());
        }
        Ok(Self::sequence())
    }
    pub fn write_text(&self, text: &[u16], expected_sequence: u32) -> anyhow::Result<()> {
        anyhow::ensure!(
            !text.contains(&0) && text.len() <= 32767,
            "路径文字过长或格式无效"
        );
        let mut block = Global::new((text.len() + 1) * 2)?;
        let pointer = unsafe { GlobalLock(block.0) }.cast::<u16>();
        anyhow::ensure!(!pointer.is_null(), "无法准备剪贴板文字");
        let unlock = Unlock(block.0);
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), pointer, text.len());
            pointer.add(text.len()).write(0);
        }
        drop(unlock);
        let previous = unsafe { OleGetClipboard() }.ok();
        let opened = self.open(|| false)?;
        anyhow::ensure!(
            Self::sequence() == expected_sequence,
            "剪贴板已改变，请重试"
        );
        let result = (|| -> windows::core::Result<()> {
            unsafe {
                EmptyClipboard()?;
                SetClipboardData(CF_UNICODETEXT.0 as u32, Some(block.handle()))?;
                block.disown();
            }
            Ok(())
        })();
        drop(opened);
        if let Err(error) = result {
            if let Some(previous) = previous {
                unsafe { OleSetClipboard(&previous) }
                    .map_err(|_| anyhow::anyhow!("无法写入或恢复剪贴板"))?;
            }
            return Err(error.into());
        }
        Ok(())
    }
    pub fn write_text_paths(
        &self,
        ledger: &Ledger,
        expected_sequence: Option<u32>,
        canceled: impl Fn() -> bool,
    ) -> anyhow::Result<u32> {
        let text = |path: &std::path::Path| -> Vec<u16> {
            shell::clipboard_path(path).unwrap_or_else(|_| {
                use std::os::windows::ffi::OsStrExt;
                path.as_os_str().encode_wide().chain(Some(0)).collect()
            })
        };
        let mut bytes = 2usize;
        let mut count = 0usize;
        ledger.visit_roots(false, |root| {
            if canceled() {
                anyhow::bail!("已取消");
            }
            let path = text(&root.path);
            bytes = bytes
                .checked_add((path.len() - 1) * 2 + 8)
                .ok_or_else(|| anyhow::anyhow!("Clipboard overflow"))?;
            count += 1;
            Ok(())
        })?;
        anyhow::ensure!(count > 0, "没有可复制的路径");
        bytes -= 4;
        anyhow::ensure!(
            bytes <= MAX_PAYLOAD_BYTES,
            "无法准备剪贴板，请减少选择后重试"
        );
        let mut block = Global::new(bytes)?;
        let pointer = unsafe { GlobalLock(block.0) }.cast::<u16>();
        anyhow::ensure!(!pointer.is_null(), "Cannot lock clipboard text");
        let unlock = Unlock(block.0);
        let mut cursor = 0usize;
        let mut index = 0usize;
        ledger.visit_roots(false, |root| {
            if canceled() {
                anyhow::bail!("已取消");
            }
            let path = text(&root.path);
            let path = &path[..path.len() - 1];
            let suffix = if index + 1 < count { 4 } else { 2 };
            anyhow::ensure!(
                (cursor + path.len() + suffix + 1) * 2 <= bytes,
                "Clipboard manifest changed during rendering"
            );
            unsafe {
                pointer.add(cursor).write(b'"' as u16);
                cursor += 1;
                std::ptr::copy_nonoverlapping(path.as_ptr(), pointer.add(cursor), path.len());
                cursor += path.len();
                pointer.add(cursor).write(b'"' as u16);
                cursor += 1;
                if index + 1 < count {
                    pointer.add(cursor).write(13);
                    pointer.add(cursor + 1).write(10);
                    cursor += 2;
                }
            }
            index += 1;
            Ok(())
        })?;
        anyhow::ensure!(
            (cursor + 1) * 2 == bytes,
            "Clipboard manifest changed during rendering"
        );
        unsafe {
            pointer.add(cursor).write(0);
        }
        drop(unlock);
        let previous = unsafe { OleGetClipboard() }.ok();
        let opened = self.open(&canceled)?;
        anyhow::ensure!(
            expected_sequence.is_none_or(|sequence| Self::sequence() == sequence),
            "剪贴板已改变，未覆盖当前内容"
        );
        let result = (|| -> windows::core::Result<()> {
            unsafe {
                EmptyClipboard()?;
                SetClipboardData(CF_UNICODETEXT.0 as u32, Some(block.handle()))?;
                block.disown();
            }
            Ok(())
        })();
        drop(opened);
        if let Err(error) = result {
            if let Some(previous) = previous {
                let restored = unsafe { OleSetClipboard(&previous) };
                anyhow::ensure!(restored.is_ok(), "无法写入或恢复剪贴板：{error}");
            }
            return Err(error.into());
        }
        Ok(Self::sequence())
    }
    pub fn read_files(
        &self,
        ledger: &Ledger,
        expected_sequence: u32,
        canceled: impl Fn() -> bool,
        mut progress: impl FnMut(u64, u64),
    ) -> anyhow::Result<Input> {
        anyhow::ensure!(
            Self::sequence() == expected_sequence,
            "剪贴板已改变，请重新粘贴"
        );
        let origin = unsafe { OleGetClipboard() }?;
        let medium = data(&origin, CF_HDROP.0 as u32)?;
        let handle = unsafe { medium.0.u.hGlobal };
        let count = checked_drop_count(handle)?;
        let move_requested = if let Ok(effect) = data(&origin, self.preferred) {
            let block = unsafe { effect.0.u.hGlobal };
            if unsafe { GlobalSize(block) } >= 4 {
                let pointer = unsafe { GlobalLock(block) }.cast::<u32>();
                if pointer.is_null() {
                    false
                } else {
                    let value = unsafe { pointer.read_unaligned() };
                    unsafe {
                        let _ = GlobalUnlock(block);
                    }
                    value == 2
                }
            } else {
                false
            }
        } else {
            false
        };
        anyhow::ensure!(
            Self::sequence() == expected_sequence,
            "剪贴板已改变，请重新粘贴"
        );
        anyhow::ensure!(count > 0, "剪贴板中没有文件");
        let mut known_file_bytes = 0u64;
        let mut has_directories = false;
        let mut since = std::time::Instant::now();
        ledger.begin()?;
        for index in 0..count {
            if canceled() {
                ledger.commit()?;
                anyhow::bail!("已取消");
            }
            let length = unsafe { DragQueryFileW(HDROP(handle.0), index, None) } as usize;
            anyhow::ensure!(length > 0 && length <= 32767, "Invalid clipboard filename");
            let mut text = vec![0u16; length + 1];
            anyhow::ensure!(
                unsafe { DragQueryFileW(HDROP(handle.0), index, Some(&mut text)) } as usize
                    == length,
                "Clipboard filename changed"
            );
            let path = PathBuf::from(OsString::from_wide(&text[..length]));
            anyhow::ensure!(path.is_absolute(), "剪贴板路径必须是绝对路径");
            let metadata = identity::capture(&path);
            ledger.add_root(index as u64, &path, metadata.as_ref().ok())?;
            match metadata {
                Ok(metadata) => {
                    known_file_bytes = known_file_bytes.saturating_add(metadata.bytes.unwrap_or(0));
                    has_directories |= metadata.directory;
                }
                Err(error) => ledger.preparation_error(index as u64, error.code().0)?,
            }
            if index % 256 == 255 || since.elapsed() >= Duration::from_millis(50) {
                ledger.commit()?;
                progress((index + 1) as u64, count as u64);
                ledger.begin()?;
                since = std::time::Instant::now();
            }
        }
        ledger.commit()?;
        progress(count as u64, count as u64);
        drop(medium);
        Ok(Input {
            _origin: origin,
            sequence: expected_sequence,
            move_requested,
            count: count as u64,
            known_file_bytes,
            has_directories,
            _apartment: PhantomData,
        })
    }
    pub fn clear_if_sequence(&self, expected: u32) -> anyhow::Result<bool> {
        if Self::sequence() != expected {
            return Ok(false);
        }
        let _opened = self.open(|| false)?;
        if Self::sequence() != expected {
            return Ok(false);
        }
        unsafe {
            EmptyClipboard()?;
        }
        Ok(true)
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            let _ = RemoveClipboardFormatListener(self.owner);
            let _ = DestroyWindow(self.owner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hdrop_round_trip_keeps_unicode_and_has_a_bounded_double_null() {
        let paths = [
            PathBuf::from("D:\\资料\\城市_👩🏽‍💻_é.txt"),
            PathBuf::from("C:\\Users\\Example\\文档"),
        ];
        let encoded = paths
            .iter()
            .map(|p| shell::clipboard_path(p).unwrap())
            .collect::<Vec<_>>();
        let bytes = std::mem::size_of::<DROPFILES>()
            + 2
            + encoded.iter().map(|s| s.len() * 2).sum::<usize>();
        let mut builder = Payload::new(bytes).unwrap();
        for path in &encoded {
            builder.push(path).unwrap();
        }
        let block = builder.finish().unwrap();
        assert_eq!(checked_drop_count(block.0).unwrap(), 2);
        for (index, expected) in paths.iter().enumerate() {
            let len = unsafe { DragQueryFileW(HDROP(block.0.0), index as u32, None) };
            let mut value = vec![0; len as usize + 1];
            unsafe {
                DragQueryFileW(HDROP(block.0.0), index as u32, Some(&mut value));
            }
            assert_eq!(
                PathBuf::from(OsString::from_wide(&value[..len as usize])),
                *expected
            );
        }
    }

    #[test]
    fn real_shell_data_object_import_preserves_paths_and_captured_identity() {
        let _apartment = OleApartment::new().unwrap();
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!(
            "Backleaf-hdrop-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("资料_👩🏽‍💻_é.txt");
        std::fs::write(&path, b"native hdrop fixture").unwrap();
        let source = Ledger::open(&root.join("source.sqlite")).unwrap();
        let captured = identity::capture(&path).unwrap();
        source.begin().unwrap();
        source.add_root(0, &path, Some(&captured)).unwrap();
        source.commit().unwrap();
        let h = render_drop(&source, || false).unwrap();
        let object: IDataObject = unsafe {
            windows::Win32::UI::Shell::SHCreateDataObject(None, None, None::<&IDataObject>)
        }
        .unwrap();
        let mut medium = STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            ..Default::default()
        };
        medium.u.hGlobal = h;
        unsafe { object.SetData(&format(CF_HDROP.0 as u32).unwrap(), &medium, true) }.unwrap();
        let imported = Ledger::open(&root.join("imported.sqlite")).unwrap();
        assert_eq!(
            import_object(&object, &imported, || false).unwrap(),
            (1, 20, false)
        );
        imported
            .visit_roots(false, |r| {
                assert_eq!(r.path.canonicalize().unwrap(), path.canonicalize().unwrap());
                assert_eq!(r.identity, Some(captured.identity.clone()));
                Ok(())
            })
            .unwrap();
        std::fs::rename(&path, root.join("original.txt")).unwrap();
        std::fs::write(&path, b"replacement").unwrap();
        assert!(render_drop(&imported, || false).is_err());
        drop(imported);
        drop(source);
        drop(object);
        let resolved = root.canonicalize().unwrap();
        assert!(
            resolved.starts_with(&base)
                && resolved
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("Backleaf-hdrop-test-")
        );
        std::fs::remove_dir_all(resolved).unwrap();
    }
    #[test]
    fn oversized_payload_fails_before_any_clipboard_publication() {
        assert!(Payload::new(MAX_PAYLOAD_BYTES + 1).is_err());
    }
}
