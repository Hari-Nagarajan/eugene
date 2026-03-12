//! TUI dashboard for `eugene run` command.
//!
//! Provides a full-screen ratatui dashboard showing live agent progress
//! with banner, progress gauge, findings table, and score bar.
//!
//! # Architecture
//!
//! The TUI runs an async event loop that:
//! 1. Spawns the agent campaign in a background tokio task
//! 2. Periodically polls the DB for progress updates (every 2 seconds)
//! 3. Handles keyboard input ('q' to quit)
//! 4. Renders the dashboard via `widgets::draw_dashboard()`
//!
//! Agent progress is communicated via `AgentEvent` enum over an mpsc channel.
//! Since rig's internal tool loop doesn't expose per-step callbacks, we also
//! poll `get_run_summary()` from the DB every 2 seconds for progress updates.

pub mod events;
pub mod widgets;

use std::sync::Arc;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use tokio_rusqlite::Connection;

use crate::agent::client::create_minimax_client;
use crate::agent::run_campaign;
use crate::agent::run_wifi_campaign;
use crate::config::Config;
use crate::memory::get_run_summary;
use crate::wifi::report::WifiReport;

/// Events sent from the agent task to the TUI for live updates.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A new campaign phase has started (e.g., "Orientation", "Discovery")
    PhaseStarted(String),
    /// A finding was logged (e.g., "Host 192.168.1.1 found")
    FindingLogged(String),
    /// Running total score updated
    ScoreUpdated(i64),
    /// A task was completed
    TaskCompleted(String),
    /// Agent campaign finished successfully
    AgentComplete(String),
    /// Agent encountered an error
    AgentError(String),
}

/// TUI application state, rendered each frame by `widgets::draw_dashboard()`.
pub struct App {
    /// Target network/host being scanned
    pub target: String,
    /// Current campaign phase name
    pub phase: String,
    /// Progress from 0.0 to 1.0
    pub progress: f64,
    /// Discovered findings
    pub findings: Vec<String>,
    /// Running score total
    pub score: i64,
    /// Number of completed tasks
    pub tasks_completed: usize,
    /// Total number of tasks (discovered from DB)
    pub tasks_total: usize,
    /// Current status: "Running", "Complete", "Error: ..."
    pub status: String,
    /// Whether the user requested quit
    pub should_quit: bool,
    /// Final campaign result (set on AgentComplete)
    pub final_result: Option<String>,
    /// Recent activity log lines
    pub log_lines: Vec<String>,
}

impl App {
    /// Create a new App state for a given target.
    pub fn new(target: String) -> Self {
        Self {
            target,
            phase: "Initializing".to_string(),
            progress: 0.0,
            findings: Vec::new(),
            score: 0,
            tasks_completed: 0,
            tasks_total: 0,
            status: "Running".to_string(),
            should_quit: false,
            final_result: None,
            log_lines: vec!["Starting campaign...".to_string()],
        }
    }

    /// Process an agent event, updating app state accordingly.
    pub fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::PhaseStarted(phase) => {
                self.log_lines.push(format!("Phase: {phase}"));
                self.phase = phase;
            }
            AgentEvent::FindingLogged(finding) => {
                self.log_lines
                    .push(format!("Finding: {finding}"));
                self.findings.push(finding);
            }
            AgentEvent::ScoreUpdated(score) => {
                self.score = score;
            }
            AgentEvent::TaskCompleted(name) => {
                self.tasks_completed += 1;
                self.progress =
                    self.tasks_completed as f64 / self.tasks_total.max(1) as f64;
                self.log_lines.push(format!("Task complete: {name}"));
            }
            AgentEvent::AgentComplete(result) => {
                self.status = "Complete".to_string();
                self.progress = 1.0;
                self.final_result = Some(result);
                self.log_lines.push("Campaign complete!".to_string());
            }
            AgentEvent::AgentError(err) => {
                self.status = format!("Error: {err}");
                self.log_lines.push(format!("Error: {err}"));
            }
        }
    }
}

/// Launch the TUI dashboard for a recon campaign.
///
/// This function:
/// 1. Initializes the terminal (raw mode, alternate screen)
/// 2. Spawns the agent campaign in a background task
/// 3. Runs the event loop (keyboard + agent events + DB polling)
/// 4. Restores the terminal on exit
///
/// Returns when the user presses 'q' or the agent completes and user quits.
pub async fn run_tui(
    target: Option<String>,
    config: Arc<Config>,
    db: Arc<Connection>,
) -> Result<(), anyhow::Error> {
    // Initialize terminal
    let mut terminal = ratatui::try_init()
        .map_err(|e| anyhow::anyhow!("Failed to initialize terminal: {e}"))?;

    // Create mpsc channel for agent events
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    // Initialize app state
    let display_target = target.clone().unwrap_or_else(|| "auto-discover".to_string());
    let mut app = App::new(display_target);

    // Create MiniMax client and spawn agent campaign
    let agent_config = config.clone();
    let agent_db = db.clone();
    let _agent_handle = tokio::spawn(async move {
        let client_result = create_minimax_client();
        let (client, model_name) = match client_result {
            Ok(pair) => pair,
            Err(e) => {
                let _ = tx.send(AgentEvent::AgentError(e.to_string())).await;
                return;
            }
        };
        let model = rig::prelude::CompletionClient::completion_model(&client, &model_name);

        let _ = tx
            .send(AgentEvent::PhaseStarted("Campaign".to_string()))
            .await;

        match run_campaign(model, agent_config, agent_db, target.as_deref()).await {
            Ok(summary) => {
                let _ = tx.send(AgentEvent::AgentComplete(summary)).await;
            }
            Err(e) => {
                let _ = tx.send(AgentEvent::AgentError(e.to_string())).await;
            }
        }
    });

    // DB polling: get the latest run_id for progress tracking
    // We poll the DB to get task counts since the agent loop doesn't emit per-step events
    let poll_db = db.clone();
    let mut last_poll = tokio::time::Instant::now();
    let poll_interval = Duration::from_secs(2);

    // Track the run_id (latest run in the DB)
    let mut tracked_run_id: Option<i64> = None;

    // Main event loop
    loop {
        // Draw UI
        terminal.draw(|frame| widgets::draw_dashboard(frame, &app))?;

        // Poll for keyboard events with 100ms timeout
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('q')
        {
            app.should_quit = true;
        }

        // Process agent events (non-blocking drain)
        while let Ok(agent_event) = rx.try_recv() {
            app.handle_event(agent_event);
        }

        // Periodic DB polling for progress updates
        if last_poll.elapsed() >= poll_interval {
            last_poll = tokio::time::Instant::now();

            // Find the latest run_id if we haven't tracked one yet
            if tracked_run_id.is_none() {
                let db_ref = poll_db.clone();
                if let Ok(rid) = db_ref
                    .call(|conn| {
                        let id: i64 = conn.query_row(
                            "SELECT MAX(id) FROM runs WHERE status = 'running'",
                            [],
                            |row| row.get(0),
                        )?;
                        Ok(id)
                    })
                    .await
                {
                    tracked_run_id = Some(rid);
                }
            }

            // Poll run summary for progress
            if let Some(run_id) = tracked_run_id
                && let Ok(summary) = get_run_summary(&poll_db, run_id).await
            {
                app.tasks_completed = summary.completed_task_count as usize;
                app.tasks_total = summary.task_count as usize;
                app.score = summary.total_score;
                if app.tasks_total > 0 {
                    app.progress =
                        app.tasks_completed as f64 / app.tasks_total as f64;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    ratatui::restore();

    // Print final result if available
    if let Some(result) = &app.final_result {
        println!("\nCampaign Result:\n{result}");
    }

    Ok(())
}

/// Launch the TUI dashboard for a wifi campaign.
///
/// Same structure as `run_tui()` but spawns `run_wifi_campaign()` instead
/// of `run_campaign()`. After the user presses 'q', generates and prints
/// the wifi audit report from the run_id returned by the campaign.
pub async fn run_tui_wifi(
    target: Option<String>,
    config: Arc<Config>,
    db: Arc<Connection>,
) -> Result<(), anyhow::Error> {
    // Initialize terminal
    let mut terminal = ratatui::try_init()
        .map_err(|e| anyhow::anyhow!("Failed to initialize terminal: {e}"))?;

    // Create mpsc channel for agent events
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    // Oneshot channel to receive run_id from the spawned wifi campaign
    let (run_id_tx, mut run_id_rx) = tokio::sync::oneshot::channel::<i64>();

    // Initialize app state
    let display_target = target.clone().unwrap_or_else(|| "auto-discover".to_string());
    let mut app = App::new(display_target);

    // Create MiniMax client and spawn wifi campaign
    let agent_config = config.clone();
    let agent_db = db.clone();
    let _agent_handle = tokio::spawn(async move {
        let client_result = create_minimax_client();
        let (client, model_name) = match client_result {
            Ok(pair) => pair,
            Err(e) => {
                let _ = tx.send(AgentEvent::AgentError(e.to_string())).await;
                return;
            }
        };
        let model = rig::prelude::CompletionClient::completion_model(&client, &model_name);

        let _ = tx
            .send(AgentEvent::PhaseStarted("Wifi Campaign".to_string()))
            .await;

        match run_wifi_campaign(model, agent_config, agent_db, target.as_deref()).await {
            Ok((summary, run_id)) => {
                let _ = run_id_tx.send(run_id);
                let _ = tx.send(AgentEvent::AgentComplete(summary)).await;
            }
            Err(e) => {
                let _ = tx.send(AgentEvent::AgentError(e.to_string())).await;
            }
        }
    });

    // DB polling for progress
    let poll_db = db.clone();
    let mut last_poll = tokio::time::Instant::now();
    let poll_interval = Duration::from_secs(2);
    let mut tracked_run_id: Option<i64> = None;

    // Main event loop
    loop {
        terminal.draw(|frame| widgets::draw_dashboard(frame, &app))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('q')
        {
            app.should_quit = true;
        }

        while let Ok(agent_event) = rx.try_recv() {
            app.handle_event(agent_event);
        }

        if last_poll.elapsed() >= poll_interval {
            last_poll = tokio::time::Instant::now();

            if tracked_run_id.is_none() {
                let db_ref = poll_db.clone();
                if let Ok(rid) = db_ref
                    .call(|conn| {
                        let id: i64 = conn.query_row(
                            "SELECT MAX(id) FROM runs WHERE status = 'running'",
                            [],
                            |row| row.get(0),
                        )?;
                        Ok(id)
                    })
                    .await
                {
                    tracked_run_id = Some(rid);
                }
            }

            if let Some(run_id) = tracked_run_id
                && let Ok(summary) = get_run_summary(&poll_db, run_id).await
            {
                app.tasks_completed = summary.completed_task_count as usize;
                app.tasks_total = summary.task_count as usize;
                app.score = summary.total_score;
                if app.tasks_total > 0 {
                    app.progress =
                        app.tasks_completed as f64 / app.tasks_total as f64;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    ratatui::restore();

    // Print final result if available
    if let Some(result) = &app.final_result {
        println!("\nCampaign Result:\n{result}");
    }

    // Print wifi audit report if we received a run_id
    if let Ok(run_id) = run_id_rx.try_recv() {
        if let Ok(report) = WifiReport::from_run(&db, run_id).await {
            println!("{}", report.format_cli());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_new() {
        let app = App::new("10.0.0.0/24".to_string());
        assert_eq!(app.target, "10.0.0.0/24");
        assert_eq!(app.phase, "Initializing");
        assert!((app.progress - 0.0).abs() < f64::EPSILON);
        assert!(app.findings.is_empty());
        assert_eq!(app.score, 0);
        assert_eq!(app.status, "Running");
        assert!(!app.should_quit);
        assert!(app.final_result.is_none());
        assert_eq!(app.log_lines.len(), 1);
    }

    #[test]
    fn test_app_handle_phase_started() {
        let mut app = App::new("target".to_string());
        app.handle_event(AgentEvent::PhaseStarted("Discovery".to_string()));
        assert_eq!(app.phase, "Discovery");
        assert!(app.log_lines.last().unwrap().contains("Discovery"));
    }

    #[test]
    fn test_app_handle_finding_logged() {
        let mut app = App::new("target".to_string());
        app.handle_event(AgentEvent::FindingLogged("Host 10.0.0.1 found".to_string()));
        assert_eq!(app.findings.len(), 1);
        assert_eq!(app.findings[0], "Host 10.0.0.1 found");
    }

    #[test]
    fn test_app_handle_score_updated() {
        let mut app = App::new("target".to_string());
        app.handle_event(AgentEvent::ScoreUpdated(42));
        assert_eq!(app.score, 42);
    }

    #[test]
    fn test_app_handle_task_completed() {
        let mut app = App::new("target".to_string());
        app.tasks_total = 5;
        app.handle_event(AgentEvent::TaskCompleted("nmap scan".to_string()));
        assert_eq!(app.tasks_completed, 1);
        assert!((app.progress - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_app_handle_agent_complete() {
        let mut app = App::new("target".to_string());
        app.handle_event(AgentEvent::AgentComplete("All done".to_string()));
        assert_eq!(app.status, "Complete");
        assert!((app.progress - 1.0).abs() < f64::EPSILON);
        assert_eq!(app.final_result.as_deref(), Some("All done"));
    }

    #[test]
    fn test_app_handle_agent_error() {
        let mut app = App::new("target".to_string());
        app.handle_event(AgentEvent::AgentError("Connection failed".to_string()));
        assert!(app.status.contains("Error"));
        assert!(app.log_lines.last().unwrap().contains("Connection failed"));
    }
}
