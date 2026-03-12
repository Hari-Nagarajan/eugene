use regex::Regex;
use std::sync::LazyLock;

/// FTS5 query sanitization regex - strips non-alphanumeric/non-space characters.
/// Shared by memories and scripts modules for FTS5 search queries.
pub(crate) static FTS_SANITIZER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[^\w\s]").unwrap()
});

mod runs;
pub use runs::{RunSummary, Finding, create_run, log_finding, log_task, update_task, update_run, get_findings_by_host, get_run_summary};

mod scores;
pub use scores::{ScoreSummary, ScoreEvent, points_for_action, log_score_event, get_score_summary, weighted_vuln_points, log_weighted_vuln_event};

mod scripts;
pub use scripts::{Script, save_script, search_scripts, get_script_by_name, update_script_usage};

mod memories;
pub use memories::{Memory, save_memory, search_memories};

mod sessions;
pub use sessions::{load_session, save_session, clear_session};

mod schedules;
pub use schedules::{ScheduledTask, create_schedule, list_schedules, delete_schedule, pause_schedule, resume_schedule, get_due_schedules, advance_schedule};

mod cve;
pub use cve::{get_cached_cves, store_cached_cves, delete_stale_cves};

mod wifi;
pub use wifi::{insert_wifi_ap, get_wifi_aps, insert_wifi_client, insert_client_probe, get_wifi_clients, get_matched_probes, migrate_wifi_schema, WifiClient, MatchedProbe};
