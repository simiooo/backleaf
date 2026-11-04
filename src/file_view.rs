//! Local deterministic search contract; paths remain lossless UTF-16 on disk.
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WinPath(pub Vec<u16>);
impl WinPath {
    pub fn new(path: &Path) -> Self {
        Self(path.as_os_str().encode_wide().collect())
    }
    pub fn path(&self) -> PathBuf {
        OsString::from_wide(&self.0).into()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sort {
    #[default]
    Name,
    Modified,
    Size,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Group {
    #[default]
    None,
    Type,
    Date,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    #[default]
    List,
    Grid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileView {
    pub root: WinPath,
    pub query: String,
    pub recursive: bool,
    pub indexed: bool,
    pub extensions: Vec<String>,
    pub modified_days: Option<u32>,
    pub min_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
    pub sort: Sort,
    pub descending: bool,
    pub group: Group,
    pub layout: Layout,
}
impl FileView {
    pub fn new(root: &Path) -> Self {
        Self {
            root: WinPath::new(root),
            query: String::new(),
            recursive: true,
            indexed: false,
            extensions: vec![],
            modified_days: None,
            min_bytes: None,
            max_bytes: None,
            sort: Sort::Name,
            descending: false,
            group: Group::None,
            layout: Layout::List,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.root.0.is_empty() || self.root.0.contains(&0) {
            return Err("位置无效".into());
        }
        if self.query.chars().count() > 1024
            || self.extensions.len() > 64
            || self
                .extensions
                .iter()
                .any(|s| s.is_empty() || s.len() > 64 || s.chars().any(|c| !c.is_alphanumeric()))
        {
            return Err("类型或搜索条件无效".into());
        }
        if self.group != Group::None && self.layout != Layout::List {
            return Err("分组视图使用列表布局".into());
        }
        if self.modified_days.is_some_and(|d| d == 0 || d > 365000) {
            return Err("天数需要1–365000".into());
        }
        if self
            .min_bytes
            .zip(self.max_bytes)
            .is_some_and(|(a, b)| a > b)
        {
            return Err("最小大小不能大于最大大小".into());
        }
        Ok(())
    }
    pub fn metadata_needed(&self) -> bool {
        self.modified_days.is_some()
            || self.min_bytes.is_some()
            || self.max_bytes.is_some()
            || self.sort != Sort::Name
            || self.group == Group::Date
    }
    pub fn accepts(
        &self,
        path: &Path,
        name: &str,
        directory: bool,
        metadata: Option<&std::fs::Metadata>,
        now: std::time::SystemTime,
    ) -> bool {
        if !name.to_lowercase().contains(&self.query.to_lowercase()) {
            return false;
        }
        if !self.extensions.is_empty()
            && (directory
                || !path.extension().is_some_and(|e| {
                    self.extensions
                        .contains(&e.to_string_lossy().to_lowercase())
                }))
        {
            return false;
        }
        if self.metadata_needed() {
            let Some(m) = metadata else {
                return false;
            };
            if self.min_bytes.is_some_and(|v| directory || m.len() < v)
                || self.max_bytes.is_some_and(|v| directory || m.len() > v)
            {
                return false;
            }
            if let Some(days) = self.modified_days {
                let Ok(modified) = m.modified() else {
                    return false;
                };
                let today = chrono::DateTime::<chrono::Local>::from(now).date_naive();
                let day = chrono::DateTime::<chrono::Local>::from(modified).date_naive();
                if today.signed_duration_since(day).num_days() < 0
                    || today.signed_duration_since(day).num_days() >= i64::from(days)
                {
                    return false;
                }
            }
        }
        true
    }
    pub fn group_key(&self, path: &Path, directory: bool, modified: Option<u64>) -> String {
        match self.group {
            Group::None => String::new(),
            Group::Type => {
                if directory {
                    "文件夹".into()
                } else {
                    path.extension()
                        .map(|x| x.to_string_lossy().to_uppercase())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "其他".into())
                }
            }
            Group::Date => modified
                .and_then(|t| chrono::DateTime::from_timestamp(t as i64, 0))
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format("%Y-%m-%d")
                        .to_string()
                })
                .unwrap_or_else(|| "未知日期".into()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fileview_roundtrip_keeps_lossless_paths_and_relative_filters() {
        use std::os::windows::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_wide(&[
            67, 58, 92, 0xd800, 92, 0x6587,
        ]));
        let mut view = FileView::new(&path);
        view.query = "报告".into();
        view.extensions = vec!["pdf".into()];
        view.modified_days = Some(7);
        view.min_bytes = Some(100);
        view.max_bytes = Some(200);
        let encoded = serde_json::to_string(&view).unwrap();
        let restored: FileView = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored, view);
        assert_eq!(restored.root.path(), path);
        assert!(restored.validate().is_ok());
        let mut invalid = restored;
        invalid.min_bytes = Some(300);
        assert!(invalid.validate().is_err());
    }
}
