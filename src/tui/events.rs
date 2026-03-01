//! Event handling utilities for the TUI dashboard.
//!
//! Provides keyboard input polling with async-compatible interface.
//! Most event loop logic lives in `mod.rs`; this module provides helpers.

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};

/// Non-blocking keyboard poll.
///
/// Returns `Some(KeyCode)` if a key was pressed within the timeout,
/// or `None` if no key event occurred.
pub fn poll_keyboard(timeout: Duration) -> Option<KeyCode> {
    if event::poll(timeout).ok()? {
        if let Event::Key(key) = event::read().ok()? {
            if key.kind == KeyEventKind::Press {
                return Some(key.code);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poll_keyboard_no_input() {
        // With zero timeout and no terminal input, should return None
        let result = poll_keyboard(Duration::from_millis(0));
        // In test environment (no TTY), this may return None or error gracefully
        // We just verify it doesn't panic
        let _ = result;
    }
}
