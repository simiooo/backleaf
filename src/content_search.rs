//! Bounded recursive content search over text files.
//!
//! A file is treated as text only after content sniffing: UTF-8 (optionally with
//! BOM), UTF-16 with a BOM, no NULs and no more than a tiny control-byte ratio.
//! Results carry a context snippet and byte ranges for highlighting.
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex},
};
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const PREFIX_BYTES: usize = 8 * 1024;
const MAX_SCANNED_BYTES: u64 = 256 * 1024 * 1024;
const CACHE_TEXT_BYTES: usize = 32 * 1024 * 1024;
const CACHE_ENTRIES: usize = 1024;
const MAX_FILES: usize = 20_000;
const MAX_DIRS: usize = 8_000;
pub const DEFAULT_LIMIT: usize = 200;
const BATCH_SIZE: usize = 8;
const CONTEXT_CHARS: usize = 120;
#[derive(Clone, Debug)]
pub struct Found {
    pub path: PathBuf,
    pub name: String,
    pub line: usize,
    pub snippet: String,
    pub highlights: Vec<std::ops::Range<usize>>,
    pub matches: usize,
}
#[derive(Debug)]
pub enum Notice {
    Batch {
        token: u64,
        matches: Vec<Found>,
        scanned: usize,
        done: bool,
        truncated: bool,
        error: Option<String>,
    },
}
struct CachedText {
    len: u64,
    modified: Option<std::time::SystemTime>,
    text: Option<Arc<str>>,
    used: u64,
}
#[derive(Default)]
struct CacheState {
    entries: std::collections::HashMap<PathBuf, CachedText>,
    bytes: usize,
    used: u64,
}
/// Session lifetime cache of sniffed file text. Repeated queries skip disk reads
/// for unchanged files and also remember files already proven non-text.
#[derive(Default)]
pub struct TextCache {
    state: Mutex<CacheState>,
}
impl TextCache {
    pub fn new() -> Self {
        Self::default()
    }
    fn get(
        &self,
        path: &Path,
        len: u64,
        modified: Option<std::time::SystemTime>,
    ) -> Option<Option<Arc<str>>> {
        let mut state = self.state.lock().unwrap();
        state.used += 1;
        let used = state.used;
        let entry = state.entries.get_mut(path)?;
        if entry.len != len || entry.modified != modified {
            return None;
        }
        entry.used = used;
        Some(entry.text.clone())
    }
    fn put(
        &self,
        path: PathBuf,
        len: u64,
        modified: Option<std::time::SystemTime>,
        text: Option<Arc<str>>,
    ) {
        let mut state = self.state.lock().unwrap();
        state.used += 1;
        let used = state.used;
        let bytes = text.as_ref().map_or(0, |text| text.len());
        if let Some(old) = state.entries.remove(&path) {
            state.bytes = state
                .bytes
                .saturating_sub(old.text.as_ref().map_or(0, |text| text.len()));
        }
        state.bytes = state.bytes.saturating_add(bytes);
        state.entries.insert(
            path,
            CachedText {
                len,
                modified,
                text,
                used,
            },
        );
        while (state.bytes > CACHE_TEXT_BYTES || state.entries.len() > CACHE_ENTRIES)
            && !state.entries.is_empty()
        {
            let Some(key) = state
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(path, _)| path.clone())
            else {
                break;
            };
            if let Some(entry) = state.entries.remove(&key) {
                state.bytes = state
                    .bytes
                    .saturating_sub(entry.text.as_ref().map_or(0, |text| text.len()));
            }
        }
    }
}
pub fn spawn(
    token: u64,
    root: PathBuf,
    query: String,
    limit: usize,
    tx: async_channel::Sender<crate::model::Event>,
    latest: Arc<AtomicU64>,
    cache: Arc<TextCache>,
) {
    let _ = std::thread::Builder::new()
        .name("backleaf-content-search".into())
        .spawn(move || {
            let query_lower = query.to_ascii_lowercase();
            let mut batch = Vec::new();
            let mut scanned = 0usize;
            let mut scanned_bytes = 0u64;
            let mut dirs = 0usize;
            let mut results = 0usize;
            let mut truncated = false;
            let cancel = || latest.load(Ordering::Acquire) != token;
            let mut stack = vec![root];
            let mut seen_dirs = std::collections::HashSet::new();
            'scan: while let Some(directory) = stack.pop() {
                if cancel() {
                    return;
                }
                if dirs >= MAX_DIRS || scanned >= MAX_FILES || scanned_bytes >= MAX_SCANNED_BYTES {
                    truncated = true;
                    break;
                }
                dirs += 1;
                let Ok(read) = std::fs::read_dir(&directory) else {
                    continue;
                };
                let mut entries = read.flatten().collect::<Vec<_>>();
                entries.sort_by_key(|entry| entry.file_name());
                for entry in entries {
                    if cancel() {
                        return;
                    }
                    if results >= limit {
                        truncated = true;
                        break 'scan;
                    }
                    let Ok(kind) = entry.file_type() else {
                        continue;
                    };
                    if kind.is_symlink() {
                        continue;
                    }
                    let path = entry.path();
                    if kind.is_dir() {
                        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                        if seen_dirs.insert(canonical) {
                            stack.push(path);
                        }
                    } else if kind.is_file() {
                        if scanned >= MAX_FILES {
                            truncated = true;
                            break 'scan;
                        }
                        scanned += 1;
                        let metadata = std::fs::metadata(&path).ok();
                        if let Some(meta) = &metadata {
                            scanned_bytes = scanned_bytes.saturating_add(meta.len());
                        }
                        let found = metadata
                            .filter(|meta| meta.len() <= MAX_FILE_BYTES)
                            .and_then(|meta| {
                                let len = meta.len();
                                let modified = meta.modified().ok();
                                let text = cache.get(&path, len, modified).unwrap_or_else(|| {
                                    let text = read_text_file(&path, len);
                                    cache.put(path.clone(), len, modified, text.clone());
                                    text
                                });
                                text.and_then(|text| find_match(&path, &text, &query_lower))
                            });
                        if let Some(found) = found {
                            batch.push(found);
                            results += 1;
                            if batch.len() >= BATCH_SIZE
                                && !send(
                                    &tx,
                                    token,
                                    std::mem::take(&mut batch),
                                    scanned,
                                    false,
                                    false,
                                    None,
                                )
                            {
                                return;
                            }
                        } else if scanned.is_multiple_of(256)
                            && batch.is_empty()
                            && !send(&tx, token, Vec::new(), scanned, false, false, None)
                        {
                            return;
                        }
                    }
                }
            }
            if cancel() {
                return;
            }
            let _ = send(&tx, token, batch, scanned, true, truncated, None);
        });
}
fn send(
    tx: &async_channel::Sender<crate::model::Event>,
    token: u64,
    matches: Vec<Found>,
    scanned: usize,
    done: bool,
    truncated: bool,
    error: Option<String>,
) -> bool {
    tx.send_blocking(crate::model::Event::ContentSearch(Notice::Batch {
        token,
        matches,
        scanned,
        done,
        truncated,
        error,
    }))
    .is_ok()
}
/// Read the prefix first so obvious binaries are rejected without reading the
/// whole file; text files are then read and decoded exactly as before.
fn read_text_file(path: &Path, len: u64) -> Option<Arc<str>> {
    if len == 0 || len > MAX_FILE_BYTES {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let mut prefix = vec![0u8; PREFIX_BYTES.min(len as usize)];
    let read = file.read(&mut prefix).ok()?;
    prefix.truncate(read);
    if !looks_text_prefix(&prefix) {
        return None;
    }
    let mut bytes = prefix;
    if file.read_to_end(&mut bytes).is_err() || bytes.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    decode_text(&bytes).map(Arc::from)
}
fn looks_text_prefix(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if bytes.is_empty() {
        return true;
    }
    if bytes.contains(&0) {
        return false;
    }
    let controls = bytes
        .iter()
        .filter(|byte| **byte < 0x20 && !matches!(**byte, b'\t' | b'\n' | b'\r' | 0x0c))
        .count();
    if controls * 200 > bytes.len() {
        return false;
    }
    // An 8 KiB cut can split a multi-byte UTF-8 character; trim up to three
    // trailing bytes before deciding that the prefix is not valid text.
    let mut end = bytes.len();
    for _ in 0..4 {
        if std::str::from_utf8(&bytes[..end]).is_ok() {
            return true;
        }
        if end == 0 {
            return false;
        }
        end -= 1;
    }
    false
}
/// Conservative text sniffing: never trust the extension alone.
pub fn decode_text(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, false);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if bytes.is_empty() || bytes.contains(&0) {
        return None;
    }
    let controls = bytes
        .iter()
        .filter(|byte| **byte < 0x20 && !matches!(**byte, b'\t' | b'\n' | b'\r' | 0x0c))
        .count();
    if controls * 200 > bytes.len() {
        return None;
    }
    std::str::from_utf8(bytes).ok().map(ToOwned::to_owned)
}
fn decode_utf16(bytes: &[u8], little_endian: bool) -> Option<String> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes(*pair)
            } else {
                u16::from_be_bytes(*pair)
            }
        })
        .collect::<Vec<_>>();
    let text = String::from_utf16(&units).ok()?;
    (!text.is_empty()).then_some(text)
}
fn find_match(path: &Path, text: &str, query_lower: &str) -> Option<Found> {
    if query_lower.is_empty() {
        return None;
    }
    let lines = text.split('\n').collect::<Vec<_>>();
    let mut matches = 0usize;
    let mut first = None;
    for (index, line) in lines.iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        let count = lower.match_indices(query_lower).count();
        if count > 0 {
            matches += count;
            first.get_or_insert((index, lower.find(query_lower).unwrap_or(0)));
        }
    }
    let (line_index, byte_index) = first?;
    let previous = line_index
        .checked_sub(1)
        .and_then(|index| lines.get(index))
        .map(|line| truncate_line(line, CONTEXT_CHARS));
    let current = window_around(lines[line_index], byte_index, query_lower.len());
    let next = lines
        .get(line_index + 1)
        .map(|line| truncate_line(line, CONTEXT_CHARS));
    let mut snippet = String::new();
    for line in previous
        .iter()
        .chain(std::iter::once(&current))
        .chain(next.iter())
    {
        if !snippet.is_empty() {
            snippet.push('\n');
        }
        snippet.push_str(line);
    }
    let highlights = highlights(&snippet, query_lower);
    Some(Found {
        path: path.to_path_buf(),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        line: line_index + 1,
        snippet,
        highlights,
        matches,
    })
}
fn truncate_line(line: &str, limit: usize) -> String {
    let trimmed = line.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_owned();
    }
    let mut value = trimmed.chars().take(limit).collect::<String>();
    value.push('…');
    value
}
fn window_around(line: &str, byte_index: usize, query_len: usize) -> String {
    let start_target = byte_index.saturating_sub(CONTEXT_CHARS * 3);
    let start = floor_boundary(line, start_target);
    let end_target = (byte_index + query_len + CONTEXT_CHARS * 3).min(line.len());
    let end = ceil_boundary(line, end_target);
    let mut value = String::new();
    if start > 0 {
        value.push('…');
    }
    value.push_str(&line[start..end]);
    if end < line.len() {
        value.push('…');
    }
    value
}
fn floor_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}
fn ceil_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index += 1;
    }
    index
}
fn highlights(text: &str, query_lower: &str) -> Vec<std::ops::Range<usize>> {
    let lower = text.to_ascii_lowercase();
    let mut ranges = Vec::new();
    let mut offset = 0;
    while let Some(found) = lower[offset..].find(query_lower) {
        let start = offset + found;
        let end = start + query_lower.len();
        ranges.push(start..end);
        offset = end;
    }
    ranges
}
#[cfg(test)]
mod tests {
    use super::{TextCache, decode_text, find_match, read_text_file};
    use std::path::Path;
    #[test]
    fn text_sniffing_rejects_binary_and_accepts_utf8_and_utf16() {
        assert!(decode_text(b"hello\nworld\n").is_some());
        assert!(decode_text(&[0xEF, 0xBB, 0xBF, b'h', b'i']).is_some());
        assert!(decode_text(&[0xFF, 0xFE, b'h', 0, b'i', 0]).is_some());
        assert!(decode_text(b"hello\0world").is_none());
        assert!(decode_text(&[0x89, b'P', b'N', b'G', 0, 1, 2, 3]).is_none());
        assert!(decode_text(b"\x01\x02\x03\x04\x05").is_none());
    }
    #[test]
    fn match_snippet_keeps_context_and_highlight_ranges() {
        let text = "alpha\nbeta unique Gamma\ndelta\n";
        let found = find_match(Path::new("notes.txt"), text, "unique").unwrap();
        assert_eq!(found.name, "notes.txt");
        assert_eq!(found.line, 2);
        assert_eq!(found.matches, 1);
        assert!(found.snippet.contains("unique"));
        assert_eq!(&found.snippet[found.highlights[0].clone()], "unique");
        assert!(found.snippet.contains("beta"));
        assert!(found.snippet.contains("delta"));
    }
    #[test]
    fn text_cache_reuses_unchanged_files_and_invalidates_length_changes() {
        let fixture = crate::test_support::Fixture::new("content-text-cache");
        let path = fixture.0.join("notes.txt");
        std::fs::write(&path, b"cached needle").unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        let text = read_text_file(&path, meta.len()).expect("text");
        assert_eq!(&*text, "cached needle");
        let cache = TextCache::new();
        cache.put(
            path.clone(),
            meta.len(),
            meta.modified().ok(),
            Some(text.clone()),
        );
        assert_eq!(
            cache.get(&path, meta.len(), meta.modified().ok()),
            Some(Some(text))
        );
        std::fs::write(&path, b"cached needle changed").unwrap();
        let newer = std::fs::metadata(&path).unwrap();
        assert_eq!(cache.get(&path, newer.len(), newer.modified().ok()), None);
    }
}
