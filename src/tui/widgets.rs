//! Dashboard layout and widget rendering for the TUI.
//!
//! Provides the `draw_dashboard()` function that renders the full-screen
//! dashboard with banner, status bar, progress gauge, findings table,
//! activity log, and help bar.
//!
//! ## Layout
//!
//! ```text
//! +------------------------------------------+
//! | Eugene - Autonomous Recon Agent   [target]|  3 rows - Banner
//! +------------------------------------------+
//! | Phase: Discovery      Score: 145         |  3 rows - Status bar
//! +------------------------------------------+
//! | [=============>          ] 60%           |  3 rows - Progress gauge
//! +------------------------------------------+
//! |  # | Type    | Finding                   |  Min 8 rows - Findings table
//! |  1 | port    | 22 SSH open               |
//! |  2 | service | OpenSSH 8.9               |
//! +------------------------------------------+
//! | > Finding: Host 192.168.1.1 found        |  6 rows - Activity log
//! | > Task complete: nmap scan               |
//! +------------------------------------------+
//! | q: quit                                  |  1 row - Help bar
//! +------------------------------------------+
//! ```

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table, Wrap};

use super::App;

/// Status color based on app state.
fn status_color(status: &str) -> Color {
    if status.starts_with("Error") {
        Color::Red
    } else if status == "Complete" {
        Color::Cyan
    } else {
        Color::Green
    }
}

/// Gauge color based on app state.
fn gauge_color(app: &App) -> Color {
    if app.status.starts_with("Error") {
        Color::Red
    } else if app.status == "Complete" {
        Color::Cyan
    } else {
        Color::Green
    }
}

/// Score color: yellow for positive, red for negative, dark gray for zero.
fn score_color(score: i64) -> Color {
    if score > 0 {
        Color::Yellow
    } else if score < 0 {
        Color::Red
    } else {
        Color::DarkGray
    }
}

/// Draw the full TUI dashboard with 6-section vertical layout.
pub fn draw_dashboard(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Main vertical layout: banner, status, progress, findings, activity, help
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Banner
            Constraint::Length(3), // Status bar
            Constraint::Length(3), // Progress gauge
            Constraint::Min(8),   // Findings table
            Constraint::Length(6), // Activity log
            Constraint::Length(1), // Help bar
        ])
        .split(area);

    draw_banner(frame, app, chunks[0]);
    draw_status_bar(frame, app, chunks[1]);
    draw_progress(frame, app, chunks[2]);
    draw_findings_table(frame, app, chunks[3]);
    draw_activity_log(frame, app, chunks[4]);
    draw_help_bar(frame, app, chunks[5]);
}

/// Banner: agent name and target info.
fn draw_banner(frame: &mut Frame, app: &App, area: Rect) {
    let banner_text = Line::from(vec![
        Span::styled(
            "Eugene",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" - Autonomous Recon Agent  "),
        Span::styled(
            format!("[{}]", app.target),
            Style::default().fg(Color::White),
        ),
    ]);

    let banner = Paragraph::new(banner_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title("Eugene"),
        )
        .alignment(Alignment::Center);

    frame.render_widget(banner, area);
}

/// Status bar: phase, status, score, task counts in a 2-column layout.
fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    // Split into left and right halves
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left: phase and status
    let left_text = vec![Line::from(vec![
        Span::raw("Phase: "),
        Span::styled(&app.phase, Style::default().fg(Color::Yellow)),
        Span::raw("  Status: "),
        Span::styled(&app.status, Style::default().fg(status_color(&app.status))),
    ])];

    let left = Paragraph::new(left_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Status"),
    );
    frame.render_widget(left, halves[0]);

    // Right: score and task counts
    let right_text = vec![Line::from(vec![
        Span::raw("Score: "),
        Span::styled(
            format!("{}", app.score),
            Style::default().fg(score_color(app.score)),
        ),
        Span::raw("  Tasks: "),
        Span::styled(
            format!("{}/{}", app.tasks_completed, app.tasks_total),
            Style::default().fg(Color::Yellow),
        ),
    ])];

    let right = Paragraph::new(right_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Metrics"),
    );
    frame.render_widget(right, halves[1]);
}

/// Progress gauge showing campaign completion percentage.
fn draw_progress(frame: &mut Frame, app: &App, area: Rect) {
    let pct = (app.progress * 100.0) as u16;
    let label = format!("{}%", pct.min(100));

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Campaign Progress"),
        )
        .gauge_style(Style::default().fg(gauge_color(app)).bg(Color::DarkGray))
        .ratio(app.progress.clamp(0.0, 1.0))
        .label(label);

    frame.render_widget(gauge, area);
}

/// Findings table showing discovered hosts, ports, services.
fn draw_findings_table(frame: &mut Frame, app: &App, area: Rect) {
    // Header row
    let header = Row::new(vec![
        Cell::from("#").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Finding").style(Style::default().add_modifier(Modifier::BOLD)),
    ])
    .style(Style::default().fg(Color::Cyan));

    // Calculate how many findings fit in the visible area
    // (area height - 2 for borders - 1 for header)
    let visible_rows = area.height.saturating_sub(3) as usize;

    // Show the most recent findings that fit
    let skip = if app.findings.len() > visible_rows {
        app.findings.len() - visible_rows
    } else {
        0
    };

    let rows: Vec<Row> = app
        .findings
        .iter()
        .enumerate()
        .skip(skip)
        .map(|(i, finding)| {
            let style = if i % 2 == 0 {
                Style::default()
            } else {
                Style::default().bg(Color::Rgb(30, 30, 30))
            };
            Row::new(vec![
                Cell::from(format!("{}", i + 1)),
                Cell::from(finding.as_str()),
            ])
            .style(style)
        })
        .collect();

    let widths = [Constraint::Length(4), Constraint::Min(20)];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Findings ({})", app.findings.len())),
        );

    frame.render_widget(table, area);
}

/// Activity log showing recent events.
fn draw_activity_log(frame: &mut Frame, app: &App, area: Rect) {
    // Show the last N log lines that fit (area height - 2 for borders)
    let visible_lines = area.height.saturating_sub(2) as usize;
    let skip = if app.log_lines.len() > visible_lines {
        app.log_lines.len() - visible_lines
    } else {
        0
    };

    let lines: Vec<Line> = app
        .log_lines
        .iter()
        .enumerate()
        .skip(skip)
        .map(|(i, line)| {
            let style = if i == app.log_lines.len() - 1 {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Line::from(Span::styled(format!("> {line}"), style))
        })
        .collect();

    let log = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Activity"),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(log, area);
}

/// Help bar showing keyboard shortcuts and campaign status.
fn draw_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let help_text = if app.status == "Complete" {
        "q: quit | Complete! Press q to exit."
    } else if app.status.starts_with("Error") {
        "q: quit | Error occurred. Press q to exit."
    } else {
        "q: quit | Campaign running..."
    };

    let help = Paragraph::new(Span::styled(
        help_text,
        Style::default().fg(Color::DarkGray),
    ));

    frame.render_widget(help, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// Helper to create a test terminal and render the dashboard.
    fn render_test_dashboard(app: &App) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| draw_dashboard(frame, app))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn test_dashboard_renders_without_panic() {
        let app = App::new("10.0.0.0/24".to_string());
        let _buffer = render_test_dashboard(&app);
        // If we get here, the dashboard rendered without panic
    }

    #[test]
    fn test_dashboard_shows_target() {
        let app = App::new("192.168.1.0/24".to_string());
        let buffer = render_test_dashboard(&app);
        let content = buffer_to_string(&buffer);
        assert!(
            content.contains("192.168.1.0/24"),
            "Dashboard should show target. Got: {content}"
        );
    }

    #[test]
    fn test_dashboard_shows_banner() {
        let app = App::new("target".to_string());
        let buffer = render_test_dashboard(&app);
        let content = buffer_to_string(&buffer);
        assert!(
            content.contains("Eugene"),
            "Dashboard should show Eugene banner. Got: {content}"
        );
    }

    #[test]
    fn test_dashboard_shows_score() {
        let mut app = App::new("target".to_string());
        app.score = 42;
        let buffer = render_test_dashboard(&app);
        let content = buffer_to_string(&buffer);
        assert!(
            content.contains("42"),
            "Dashboard should show score. Got: {content}"
        );
    }

    #[test]
    fn test_dashboard_shows_findings() {
        let mut app = App::new("target".to_string());
        app.findings.push("Host 10.0.0.1 found".to_string());
        let buffer = render_test_dashboard(&app);
        let content = buffer_to_string(&buffer);
        assert!(
            content.contains("10.0.0.1"),
            "Dashboard should show findings. Got: {content}"
        );
    }

    #[test]
    fn test_dashboard_shows_activity() {
        let mut app = App::new("target".to_string());
        app.log_lines.push("Task complete: nmap scan".to_string());
        let buffer = render_test_dashboard(&app);
        let content = buffer_to_string(&buffer);
        assert!(
            content.contains("nmap scan"),
            "Dashboard should show activity. Got: {content}"
        );
    }

    #[test]
    fn test_dashboard_shows_progress_percentage() {
        let mut app = App::new("target".to_string());
        app.progress = 0.6;
        let buffer = render_test_dashboard(&app);
        let content = buffer_to_string(&buffer);
        assert!(
            content.contains("60%"),
            "Dashboard should show 60% progress. Got: {content}"
        );
    }

    #[test]
    fn test_dashboard_complete_state() {
        let mut app = App::new("target".to_string());
        app.status = "Complete".to_string();
        app.progress = 1.0;
        let buffer = render_test_dashboard(&app);
        let content = buffer_to_string(&buffer);
        assert!(
            content.contains("Complete"),
            "Dashboard should show Complete status. Got: {content}"
        );
    }

    #[test]
    fn test_dashboard_small_terminal() {
        // Should not panic even with a very small terminal
        let app = App::new("target".to_string());
        let backend = TestBackend::new(40, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| draw_dashboard(frame, &app))
            .unwrap();
    }

    /// Convert a test buffer to a string for assertion matching.
    fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
        let area = buffer.area();
        let mut result = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                let cell = &buffer[(x, y)];
                result.push_str(cell.symbol());
            }
            result.push('\n');
        }
        result
    }
}
