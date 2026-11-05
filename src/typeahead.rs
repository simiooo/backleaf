//! Transient filename navigation; independent from search and every text editor.
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct State {
    pub token: u64,
    prefix: String,
    last: Option<Instant>,
    pub pending: bool,
    pub cycle: usize,
}
impl State {
    pub fn reset(&mut self) {
        self.token = self.token.wrapping_add(1);
        self.prefix.clear();
        self.last = None;
        self.pending = false;
        self.cycle = 0;
    }
    pub fn continuing(&self, now: Instant) -> bool {
        self.last
            .is_some_and(|t| now.saturating_duration_since(t) <= Duration::from_secs(1))
    }
    pub fn push(&mut self, text: &str, now: Instant) -> Option<(u64, String)> {
        if text.is_empty() || text.chars().any(char::is_control) || text.chars().count() != 1 {
            return None;
        }
        if self
            .last
            .is_none_or(|t| now.saturating_duration_since(t) > Duration::from_secs(1))
        {
            self.prefix.clear();
            self.cycle = 0;
        }
        if self.prefix.len() + text.len() > 1024 {
            self.prefix.clear();
        }
        if self.prefix.chars().count() == 1 && self.prefix.to_lowercase() == text.to_lowercase() {
            self.cycle = self.cycle.saturating_add(1);
        } else {
            self.prefix.push_str(text);
            self.cycle = 0;
        }
        self.last = Some(now);
        self.token = self.token.wrapping_add(1);
        self.pending = true;
        Some((self.token, self.prefix.to_lowercase()))
    }
}
/// Smallest Unicode string greater than every string beginning with `prefix`.
pub fn successor(prefix: &str) -> Option<String> {
    let mut chars = prefix.chars().collect::<Vec<_>>();
    while let Some(ch) = chars.pop() {
        let mut next = ch as u32 + 1;
        if (0xD800..=0xDFFF).contains(&next) {
            next = 0xE000;
        }
        if let Some(next) = char::from_u32(next) {
            chars.push(next);
            return Some(chars.into_iter().collect());
        }
    }
    None
}
pub fn native_composing(owner: usize) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        UI::Input::Ime::{GCS_COMPSTR, ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext},
    };
    unsafe {
        let hwnd = HWND(owner as *mut _);
        let context = ImmGetContext(hwnd);
        if context.is_invalid() {
            return false;
        }
        let composing = ImmGetCompositionStringW(context, GCS_COMPSTR, None, 0) > 0;
        let _ = ImmReleaseContext(hwnd, context);
        composing
    }
}
#[cfg(test)]
mod state_tests {
    use super::*;
    #[test]
    fn repeats_cycle_but_distinct_letters_extend_the_prefix() {
        let mut state = State::default();
        let now = Instant::now();
        assert_eq!(state.push("A", now).unwrap().1, "a");
        assert_eq!(state.push("a", now).unwrap().1, "a");
        assert_eq!(state.cycle, 1);
        assert_eq!(state.push("A", now).unwrap().1, "a");
        assert_eq!(state.cycle, 2);
        assert_eq!(state.push("l", now).unwrap().1, "al");
        assert_eq!(state.cycle, 0);
        assert_eq!(state.push("l", now).unwrap().1, "all");
        state.push("a", now + Duration::from_secs(2));
        assert_eq!(state.cycle, 0);
    }
    #[test]
    fn prefix_timeout_reset_and_unicode_range_do_not_leak_between_sessions() {
        let mut state = State::default();
        let now = Instant::now();
        assert_eq!(state.push("A", now).unwrap().1, "a");
        assert_eq!(
            state.push("b", now + Duration::from_millis(100)).unwrap().1,
            "ab"
        );
        assert_eq!(
            state.push("文", now + Duration::from_secs(2)).unwrap().1,
            "文"
        );
        state.reset();
        assert_eq!(
            state.push("X", now + Duration::from_secs(2)).unwrap().1,
            "x"
        );
        assert!(state.push("\n", now).is_none());
        assert_eq!(successor("ab"), Some("ac".into()));
        assert_eq!(successor("\u{d7ff}"), Some("\u{e000}".into()));
        assert_eq!(successor("\u{10ffff}"), None);
    }
}
