//! Dashboard layout and widget rendering for the TUI.
//!
//! Provides the `draw_dashboard()` function that renders the full-screen
//! dashboard with banner, status, progress, findings table, activity log,
//! and help bar.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use super::App;

/// Draw the TUI dashboard. Placeholder for Task 1 compilation.
/// Full implementation in Task 2.
pub fn draw_dashboard(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let text = format!(
        "Eugene - {} | Status: {} | Score: {}",
        app.target, app.status, app.score
    );
    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Eugene"));
    frame.render_widget(paragraph, area);
}
