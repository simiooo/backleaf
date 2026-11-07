//! Bounded recursive content search over text files.
//!
//! A file is treated as text only after content sniffing: UTF-8 (optionally with
//! BOM), UTF-16 with a BOM, no NULs and no more than a tiny control-byte ratio.
//! Results carry a context snippet and byte ranges for highlighting.
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SCANNED_BYTES: u64 = 256 * 1024 * 1024;
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
pub fn spawn(
    token: u64,
    root: PathBuf,
    query: String,
    limit: usize,
    tx: async_channel::Sender<crate::model::Event>,
    latest: Arc<AtomicU64>,
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
                            .and_then(|_| std::fs::read(&path).ok())
                            .and_then(|bytes| {
                                decode_text(&bytes)
                                    .and_then(|text| find_match(&path, &text, &query_lower))
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
    use super::{decode_text, find_match};
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
}
