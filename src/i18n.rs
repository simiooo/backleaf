//! Minimal two-language (简体中文 / English) runtime localization.
//!
//! Source strings stay in Chinese and are looked up in a compact table. English
//! falls back to the Chinese source for any string not translated yet, so a
//! missing entry never breaks the UI. `init()` is called once by the native entry
//! point; tests keep the deterministic Chinese default.
use std::sync::atomic::{AtomicU8, Ordering};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Zh,
    En,
}
static CURRENT: AtomicU8 = AtomicU8::new(0);
pub fn current() -> Language {
    if CURRENT.load(Ordering::Acquire) == 1 {
        Language::En
    } else {
        Language::Zh
    }
}
pub fn set(language: Language) {
    CURRENT.store(
        match language {
            Language::Zh => 0,
            Language::En => 1,
        },
        Ordering::Release,
    );
}
pub fn toggle() -> Language {
    let next = match current() {
        Language::Zh => Language::En,
        Language::En => Language::Zh,
    };
    set(next);
    next
}
/// Resolve language from an explicit env override, then the persisted choice,
/// then the Windows UI language.
pub fn init_with_dir(directory: &std::path::Path) {
    if init_from_env() {
        return;
    }
    if let Some(language) = load(directory) {
        set(language);
        return;
    }
    set(system_language());
}
fn init_from_env() -> bool {
    let Ok(value) = std::env::var("BACKLEAF_LANG") else {
        return false;
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "en" | "en-us" | "en-gb" | "english" => {
            set(Language::En);
            true
        }
        "zh" | "zh-cn" | "zh-hans" | "chinese" | "中文" => {
            set(Language::Zh);
            true
        }
        _ => false,
    }
}
pub fn load(directory: &std::path::Path) -> Option<Language> {
    let value = std::fs::read_to_string(directory.join("language.txt")).ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "en" => Some(Language::En),
        "zh" => Some(Language::Zh),
        _ => None,
    }
}
pub fn save(directory: &std::path::Path) {
    if std::fs::create_dir_all(directory).is_err() {
        return;
    }
    let value = match current() {
        Language::Zh => "zh",
        Language::En => "en",
    };
    let _ = std::fs::write(directory.join("language.txt"), value);
}
fn system_language() -> Language {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::GetUserDefaultUILanguage;
        let id = unsafe { GetUserDefaultUILanguage() } as u32;
        if id & 0x3ff == 0x04 {
            return Language::Zh;
        }
        return Language::En;
    }
    #[allow(unreachable_code)]
    Language::Zh
}
pub fn tr(zh: &'static str) -> &'static str {
    if current() == Language::Zh {
        return zh;
    }
    lookup(zh).unwrap_or(zh)
}
pub fn tr_owned(text: &str) -> String {
    if current() == Language::Zh {
        return text.to_owned();
    }
    lookup(text)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| text.to_owned())
}
/// Replace positional `{}` placeholders in a translated template.
pub fn fill(template: &str, args: &[String]) -> String {
    let mut result =
        String::with_capacity(template.len() + args.iter().map(String::len).sum::<usize>());
    let mut rest = template;
    let mut index = 0;
    while let Some(at) = rest.find("{}") {
        result.push_str(&rest[..at]);
        if let Some(arg) = args.get(index) {
            result.push_str(arg);
            index += 1;
        } else {
            result.push_str("{}");
        }
        rest = &rest[at + 2..];
    }
    result.push_str(rest);
    result
}
#[macro_export]
macro_rules! trf {
    ($template:literal $(, $arg:expr)* $(,)?) => {
        $crate::i18n::fill($crate::i18n::tr($template), &[$(format!("{}", $arg)),*])
    };
}
/// `tr!` resolves a static source string; `trf!` does the same for formatting.
#[macro_export]
macro_rules! tr {
    ($text:literal) => {
        $crate::i18n::tr($text)
    };
}
fn lookup(text: &str) -> Option<&'static str> {
    EN.iter().find_map(|(zh, en)| (*zh == text).then_some(*en))
}
const EN: &[(&str, &str)] = &[
    // ---- context menu ----
    ("打开", "Open"),
    ("复制", "Copy"),
    ("剪切", "Cut"),
    ("粘贴", "Paste"),
    ("粘贴到文件夹", "Paste into folder"),
    ("复制路径", "Copy path"),
    ("重命名", "Rename"),
    ("移至回收站", "Move to Recycle Bin"),
    ("详情", "Details"),
    ("新建文件夹", "New folder"),
    ("刷新", "Refresh"),
    ("编辑路径", "Edit path"),
    ("压缩为 ZIP…", "Compress to ZIP…"),
    ("解压到…", "Extract to…"),
    ("固定到侧栏", "Pin to sidebar"),
    ("移除入口", "Remove entry"),
    ("重命名视图", "Rename view"),
    ("上移", "Move up"),
    ("下移", "Move down"),
    // ---- main chrome ----
    ("后退", "Back"),
    ("上一级", "Up one level"),
    ("最小化", "Minimize"),
    ("最大化或还原", "Maximize or restore"),
    ("关闭", "Close"),
    ("文件操作", "File operations"),
    ("搜索文件名，Ctrl+F", "Search file names, Ctrl+F"),
    ("搜索文件名 · Ctrl+F", "Search file names · Ctrl+F"),
    ("搜索文件名", "Search file names"),
    ("浏览当前目录层级", "Browse folder levels"),
    ("显示折叠的上级路径", "Show folded parent path"),
    ("编辑当前位置", "Edit current location"),
    ("当前位置", "Current location"),
    ("展开或折叠渲染样本", "Expand or collapse render sample"),
    ("文件", "Files"),
    // ---- quick access / locations ----
    ("个人文件夹", "Home"),
    ("桌面", "Desktop"),
    ("下载", "Downloads"),
    ("文档", "Documents"),
    ("图片", "Pictures"),
    ("音乐", "Music"),
    ("视频", "Videos"),
    ("此文件夹", "This folder"),
    ("此电脑", "This PC"),
    ("快速访问", "Quick access"),
    ("固定", "Pinned"),
    ("最近", "Recent"),
    // ---- search / views ----
    ("筛选", "Filters"),
    ("搜索", "Search"),
    ("视图名称", "View name"),
    ("内容搜索", "Content search"),
    (" · 已修改", " · modified"),
    ("移出索引范围", "Remove from indexed scope"),
    ("索引当前文件夹", "Index current folder"),
    ("保存视图", "Save view"),
    ("更新视图", "Update view"),
    ("另存为", "Save as"),
    ("内容", "Content"),
    ("内容搜索", "Content search"),
    ("类型", "Type"),
    ("最近天数", "Recent days"),
    ("最小容量", "Minimum size"),
    ("最大容量", "Maximum size"),
    ("如 jpg, png, pdf", "e.g. jpg, png, pdf"),
    ("如 7", "e.g. 7"),
    ("如 10MB、500KB、1024", "e.g. 10MB, 500KB, 1024"),
    ("排序", "Sort"),
    ("分组", "Group"),
    ("范围", "Scope"),
    ("布局", "Layout"),
    ("顺序", "Order"),
    ("位置", "Location"),
    ("名称", "Name"),
    ("修改日期", "Date modified"),
    ("大小", "Size"),
    ("无", "None"),
    ("日期", "Date"),
    ("含子文件夹", "Include subfolders"),
    ("当前文件夹", "Current folder"),
    ("列表", "List"),
    ("网格", "Grid"),
    ("升序", "Ascending"),
    ("降序", "Descending"),
    ("已索引位置", "Indexed locations"),
    ("当前路径", "Current path"),
    ("没有匹配内容", "No matching content"),
    ("正在扫描文件内容…", "Scanning file contents…"),
    (
        "内容搜索中 · 已检查 {} 个文件",
        "Searching content · checked {} files",
    ),
    ("{} 个文件包含“{}”{}", "{} files contain “{}”{}"),
    (" · 结果已截断", " · results truncated"),
    (
        "内容 · {} · 含子文件夹",
        "Content · {} · include subfolders",
    ),
    ("第 {} 行 · {}", "Line {} · {}"),
    ("没有匹配项", "No matches"),
    ("搜索中 · 已检查 {} 项", "Searching · checked {} items"),
    ("{} 项", "{} items"),
    ("在文件内容中搜索", "Search file contents"),
    (
        "在文件内容中搜索（递归检查文本文件）",
        "Search file contents (recursive text scan)",
    ),
    (
        "递归检查文本文件内容；可尝试更短、更具体的关键词",
        "Recursively checks text files; try a shorter or more specific keyword",
    ),
    (
        "内容 · {} · 含子文件夹",
        "Content · {} · include subfolders",
    ),
    ("正在扫描文件内容…", "Scanning file contents…"),
    // ---- fields/validation ----
    ("最多输入 64 个类型", "At most 64 types"),
    (
        "类型只能包含字母和数字，多个用逗号分隔",
        "Types may contain letters and numbers, separated by commas",
    ),
    (
        "请输入 1–365000 的整数天数",
        "Enter a whole number of days from 1 to 365000",
    ),
    (
        "天数需要在 1–365000 之间",
        "Days must be between 1 and 365000",
    ),
    (
        "最大容量不能小于最小容量",
        "Maximum size cannot be less than the minimum",
    ),
    (
        "请输入容量，如 10MB、500KB 或 1024",
        "Enter a size, e.g. 10MB, 500KB or 1024",
    ),
    ("请输入数字容量，如 10MB", "Enter a numeric size, e.g. 10MB"),
    (
        "容量必须是大于等于 0 的数字",
        "Size must be a number greater than or equal to 0",
    ),
    ("容量过大", "Size is too large"),
    (
        "无法识别的容量单位“{}”；可用 B、KB、MB、GB、TB",
        "Unknown size unit “{}”; use B, KB, MB, GB or TB",
    ),
    ("请先完成输入法候选", "Finish the IME candidate first"),
    ("请先完成输入", "Finish composing text first"),
    (
        "正在复制路径，请稍后重试",
        "Copying a path, please try again later",
    ),
    (
        "目录正在更新，请稍后重试",
        "The folder is updating, please try again later",
    ),
    ("正在读取剪贴板", "Reading the clipboard"),
    ("粘贴的文字过长", "Pasted text is too long"),
    ("文字过长", "Text is too long"),
    // ---- operations ----
    ("移动", "Move"),
    ("复制到剪贴板", "Copy to clipboard"),
    ("压缩", "Compress"),
    ("解压", "Extract"),
    ("文件操作，{} 项任务", "File operations, {} tasks"),
    ("文件操作 · {} 项任务", "File operations · {} tasks"),
    ("还有 {} 项任务", "{} more tasks"),
    ("继续处理", "Continue"),
    ("取消任务并退出", "Cancel tasks and exit"),
    ("正在准备", "Preparing"),
    ("正在确认结果", "Confirming result"),
    ("已完成", "Completed"),
    ("部分完成", "Partially completed"),
    ("已取消", "Cancelled"),
    ("结果尚未确认", "Result not confirmed"),
    ("未处理", "Pending"),
    ("处理中", "Processing"),
    ("确认中", "Confirming"),
    ("失败", "Failed"),
    ("已跳过", "Skipped"),
    ("{} / {} 项", "{} / {} items"),
    ("当前进度", "Current progress"),
    ("上一页未完成项目", "Previous unfinished items"),
    ("下一页未完成项目", "Next unfinished items"),
    ("取消任务并退出", "Cancel tasks and exit"),
    (
        "文件尚未加载，请重试",
        "The file is not loaded yet, please retry",
    ),
    ("当前列表尚未就绪", "The current list is not ready yet"),
    ("目录尚未准备好", "The folder is not ready yet"),
    ("文件名称", "File name"),
    ("无法重试：{}", "Cannot retry: {}"),
    ("{}未完成", "{} unfinished"),
    ("复制 → {}", "Copy → {}"),
    ("移动 → {}", "Move → {}"),
    ("回收站 → {}", "Recycle → {}"),
    ("压缩 → {}", "Compress → {}"),
    ("解压 → {}", "Extract → {}"),
    ("{} → {}", "{} → {}"),
    ("已排队", "Queued"),
    ("准备中", "Preparing"),
    ("进行中", "In progress"),
    ("取消中", "Cancelling"),
    ("收尾中", "Finishing"),
    ("完成", "Completed"),
    ("未完成", "Unfinished"),
    ("{} 项未完成", "{} unfinished"),
    ("{}未完成", "{} unfinished"),
    ("重试", "Retry"),
    ("取消任务", "Cancel task"),
    ("正在取消", "Cancelling"),
    ("关闭任务结果", "Close task result"),
    ("没有可用的菜单项", "No available menu items"),
    ("扩展暂不可用", "Extension temporarily unavailable"),
    ("读取扩展…", "Reading extensions…"),
    ("应用操作", "Actions"),
    ("没有可用的应用操作", "No available actions"),
    // ---- status / formatted ----
    ("正在选择…", "Selecting…"),
    ("已选 {} 项", "{} selected"),
    ("已选 {} 项{}", "{} selected{}"),
    ("无法打开详情：{}", "Cannot open details: {}"),
    ("拖放未完成", "Drop did not finish"),
    (
        "无法读取上次窗口状态",
        "Cannot read the previous window state",
    ),
    ("搜索中 · 已检查 {} 项", "Searching · checked {} items"),
    (" · 索引不可用：{}", " · index unavailable: {}"),
    (" · 已索引 {} 项{}{}", " · indexed {} items{}{}"),
    (" · 正在建立索引", " · building index"),
    (" · {} 个位置不可访问", " · {} locations inaccessible"),
    (" · {} 个位置无法访问", " · {} locations inaccessible"),
    (" · 未展开 {} 个链接", " · {} links not expanded"),
    (" · 含子文件夹", " · include subfolders"),
    (" · 结果已截断", " · results truncated"),
    (
        "内容搜索中 · 已检查 {} 个文件",
        "Searching content · checked {} files",
    ),
    ("{} 个文件包含“{}”{}", "{} files contain “{}”{}"),
    (
        "内容 · {} · 含子文件夹",
        "Content · {} · include subfolders",
    ),
    ("第 {} 行 · {}", "Line {} · {}"),
    // ---- dialogs / drag ----
    ("解压文件", "Extract file"),
    ("归档", "Archive"),
    ("压缩包名称", "Archive name"),
    ("解压文件夹名称", "Extract folder name"),
    ("请选择压缩包文件", "Select an archive file"),
    (
        "此位置暂时不可用",
        "This location is temporarily unavailable",
    ),
    ("主文件夹", "Home folder"),
    (
        "无法保存窗口状态，请重试",
        "Cannot save window state, please retry",
    ),
    ("目录已改变，请重新拖放", "The folder changed, drag again"),
    ("正在检查目标，请重试", "Checking the target, please retry"),
    ("暂不支持创建链接", "Creating links is not supported yet"),
    ("移动 {} 项 → {}", "Move {} items → {}"),
    ("无法移到此位置", "Cannot move here"),
    ("正在检查目标…", "Checking target…"),
    ("移动 {} 项", "Move {} items"),
    // ---- empty/status text ----
    ("关闭详情", "Close details"),
    ("复制完整名称", "Copy full name"),
    ("复制完整路径", "Copy full path"),
    ("逻辑大小", "Logical size"),
    ("部分项目未统计", "Some items not counted"),
    ("创建时间", "Created"),
    ("修改时间", "Modified"),
    ("详情滚动条", "Details scrollbar"),
    ("{} · 详情", "{} · Details"),
    ("{} 个文件 · {} 个文件夹", "{} files · {} folders"),
    ("{} 字节", "{} bytes"),
    ("等待统计", "Waiting to count"),
    ("正在统计", "Counting"),
    ("正在统计 · {}", "Counting · {}"),
    ("尚未获得大小", "Size not available yet"),
    (
        "{} 项无法读取 · {} 项未展开",
        "{} items unreadable · {} links not expanded",
    ),
    ("位置无效", "Invalid location"),
    ("类型或搜索条件无效", "Invalid type or search criteria"),
    ("分组视图使用列表布局", "Grouped views use list layout"),
    ("天数需要1–365000", "Days must be 1–365000"),
    (
        "最小大小不能大于最大大小",
        "Minimum size cannot exceed maximum size",
    ),
    ("文件夹", "Folder"),
    ("文件", "File"),
    ("没有文件", "No files"),
    ("空文件夹", "Empty folder"),
    ("当前文件夹为空", "This folder is empty"),
    ("正在读取…", "Reading…"),
    (
        "更多文件夹可在当前位置浏览",
        "More folders can be browsed from the current location",
    ),
    ("无法加载此位置", "Cannot load this location"),
    ("无法读取此位置", "Cannot read this location"),
];
#[cfg(test)]
mod tests {
    use super::{Language, fill, lookup, set, tr};
    #[test]
    fn english_lookup_and_placeholders_are_stable() {
        assert_eq!(lookup("打开"), Some("Open"));
        assert_eq!(lookup("不存在"), None);
        assert_eq!(
            fill("Searching · checked {} items", &["12".into()]),
            "Searching · checked 12 items"
        );
    }
    #[test]
    fn switching_language_changes_static_translation() {
        set(Language::En);
        assert_eq!(tr("打开"), "Open");
        set(Language::Zh);
        assert_eq!(tr("打开"), "打开");
    }
}
