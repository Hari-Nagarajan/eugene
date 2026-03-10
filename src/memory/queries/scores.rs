use chrono::Utc;
use tokio_rusqlite::Connection;

use crate::memory::MemoryError;

/// Score event record
#[derive(Debug, serde::Serialize)]
pub struct ScoreEvent {
    pub action: String,
    pub points: i64,
    pub risk_level: String,
    pub detected: bool,
    pub timestamp: String,
}

/// Score summary for a run
#[derive(Debug, serde::Serialize)]
pub struct ScoreSummary {
    pub total_score: i64,
    pub detection_count: i64,
    pub recent_events: Vec<ScoreEvent>,
}

/// Look up point value for a scoring action
pub fn points_for_action(action: &str) -> Option<i64> {
    match action {
        "host_discovered" => Some(10),
        "port_found" => Some(5),
        "service_identified" => Some(15),
        "os_fingerprinted" => Some(20),
        "vuln_detected" => Some(25),
        "credential_captured" => Some(50),
        "successful_login" => Some(75),
        "privilege_escalation" => Some(150),
        "rce_achieved" => Some(200),
        "data_exfiltrated" => Some(100),
        "detection" => Some(-100),
        _ => None,
    }
}

/// Log a score event and return its ID
pub async fn log_score_event(
    conn: &Connection,
    run_id: Option<i64>,
    action: String,
    risk_level: String,
    detected: bool,
) -> Result<i64, MemoryError> {
    let points = points_for_action(&action)
        .ok_or_else(|| MemoryError::Query(format!("Unknown action type: {}", action)))?;
    let timestamp = Utc::now().to_rfc3339();
    let detected_int: i64 = if detected { 1 } else { 0 };

    let err_result = conn.call(move |conn| {
        conn.execute(
            "INSERT INTO score_events (run_id, action, points, risk_level, detected, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![run_id, action, points, risk_level, detected_int, timestamp],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await;
    err_result.map_err(MemoryError::from)
}

/// Calculate CVSS-weighted points for a vulnerability detection event.
///
/// Base points = 25 (same as flat `vuln_detected`). Multiplier by severity:
/// - Critical (>= 9.0): 2.0x -> 50 points
/// - High (>= 7.0): 1.5x -> 38 points
/// - Medium (>= 4.0): 1.0x -> 25 points
/// - Low (> 0.0): 0.5x -> 13 points
/// - Unknown / None / 0.0: 1.0x -> 25 points
pub fn weighted_vuln_points(cvss_score: Option<f64>) -> i64 {
    let base: f64 = 25.0;
    let multiplier = match cvss_score {
        Some(s) if s >= 9.0 => 2.0,
        Some(s) if s >= 7.0 => 1.5,
        Some(s) if s >= 4.0 => 1.0,
        Some(s) if s > 0.0 => 0.5,
        _ => 1.0, // None or 0.0 -> unknown
    };
    (base * multiplier).round() as i64
}

/// Log a CVSS-weighted vulnerability detection event.
///
/// Unlike [`log_score_event`] which uses the flat 25-point value for `vuln_detected`,
/// this function applies CVSS severity weighting so critical CVEs score higher.
pub async fn log_weighted_vuln_event(
    conn: &Connection,
    run_id: Option<i64>,
    cvss_score: Option<f64>,
    risk_level: String,
) -> Result<i64, MemoryError> {
    let points = weighted_vuln_points(cvss_score);
    let timestamp = Utc::now().to_rfc3339();

    let err_result = conn
        .call(move |conn| {
            conn.execute(
                "INSERT INTO score_events (run_id, action, points, risk_level, detected, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![run_id, "vuln_detected", points, risk_level, 0i64, timestamp],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await;
    err_result.map_err(MemoryError::from)
}

/// Get score summary for a run
pub async fn get_score_summary(
    conn: &Connection,
    run_id: i64,
) -> Result<ScoreSummary, MemoryError> {
    conn.call(move |conn| {
        let (total_score, detection_count): (i64, i64) = conn.query_row(
            "SELECT COALESCE(SUM(points), 0), COALESCE(SUM(CASE WHEN detected = 1 THEN 1 ELSE 0 END), 0) FROM score_events WHERE run_id = ?1",
            rusqlite::params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut stmt = conn.prepare(
            "SELECT action, points, risk_level, detected, timestamp FROM score_events WHERE run_id = ?1 ORDER BY id DESC LIMIT 5"
        )?;
        let recent_events = stmt.query_map(rusqlite::params![run_id], |row| {
            let detected_int: i64 = row.get(3)?;
            Ok(ScoreEvent {
                action: row.get(0)?,
                points: row.get(1)?,
                risk_level: row.get(2)?,
                detected: detected_int != 0,
                timestamp: row.get(4)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(ScoreSummary {
            total_score,
            detection_count,
            recent_events,
        })
    })
    .await
    .map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store, create_run};

    #[test]
    fn test_points_for_action_known_actions() {
        assert_eq!(points_for_action("host_discovered"), Some(10));
        assert_eq!(points_for_action("port_found"), Some(5));
        assert_eq!(points_for_action("service_identified"), Some(15));
        assert_eq!(points_for_action("os_fingerprinted"), Some(20));
        assert_eq!(points_for_action("vuln_detected"), Some(25));
        assert_eq!(points_for_action("credential_captured"), Some(50));
        assert_eq!(points_for_action("successful_login"), Some(75));
        assert_eq!(points_for_action("privilege_escalation"), Some(150));
        assert_eq!(points_for_action("rce_achieved"), Some(200));
        assert_eq!(points_for_action("data_exfiltrated"), Some(100));
        assert_eq!(points_for_action("detection"), Some(-100));
    }

    #[test]
    fn test_points_for_action_unknown_returns_none() {
        assert_eq!(points_for_action("bogus_action"), None);
        assert_eq!(points_for_action(""), None);
        assert_eq!(points_for_action("HOST_DISCOVERED"), None);
    }

    #[tokio::test]
    async fn test_log_score_event_inserts_and_returns_id() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let event_id = log_score_event(&conn, Some(run_id), "host_discovered".to_string(), "low".to_string(), false).await.unwrap();
        assert!(event_id > 0);

        let points: i64 = conn
            .call(move |conn| {
                Ok(conn.query_row("SELECT points FROM score_events WHERE id = ?1", rusqlite::params![event_id], |row| row.get(0))?)
            }).await.unwrap();
        assert_eq!(points, 10);
    }

    #[tokio::test]
    async fn test_log_score_event_rejects_unknown_action() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let result = log_score_event(&conn, None, "bogus_action".to_string(), "low".to_string(), false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_score_summary_empty_run() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let summary = get_score_summary(&conn, run_id).await.unwrap();
        assert_eq!(summary.total_score, 0);
        assert_eq!(summary.detection_count, 0);
        assert!(summary.recent_events.is_empty());
    }

    #[test]
    fn test_weighted_vuln_points_critical() {
        assert_eq!(weighted_vuln_points(Some(9.5)), 50); // 25 * 2.0
    }

    #[test]
    fn test_weighted_vuln_points_high() {
        assert_eq!(weighted_vuln_points(Some(7.5)), 38); // 25 * 1.5, rounded
    }

    #[test]
    fn test_weighted_vuln_points_medium() {
        assert_eq!(weighted_vuln_points(Some(5.0)), 25); // 25 * 1.0
    }

    #[test]
    fn test_weighted_vuln_points_low() {
        assert_eq!(weighted_vuln_points(Some(2.0)), 13); // 25 * 0.5, rounded
    }

    #[test]
    fn test_weighted_vuln_points_none() {
        assert_eq!(weighted_vuln_points(None), 25); // unknown default
    }

    #[test]
    fn test_weighted_vuln_points_zero() {
        assert_eq!(weighted_vuln_points(Some(0.0)), 25); // zero treated as unknown
    }

    #[tokio::test]
    async fn test_log_weighted_vuln_event_critical() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let event_id = log_weighted_vuln_event(&conn, Some(run_id), Some(9.8), "high".to_string()).await.unwrap();
        assert!(event_id > 0);

        let points: i64 = conn
            .call(move |conn| {
                Ok(conn.query_row("SELECT points FROM score_events WHERE id = ?1", rusqlite::params![event_id], |row| row.get(0))?)
            }).await.unwrap();
        assert_eq!(points, 50); // critical CVSS stores 50 not flat 25
    }

    #[tokio::test]
    async fn test_log_weighted_vuln_event_backward_compat() {
        // Existing points_for_action still returns 25 for vuln_detected
        assert_eq!(points_for_action("vuln_detected"), Some(25));
    }

    #[tokio::test]
    async fn test_get_score_summary_with_events() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        log_score_event(&conn, Some(run_id), "host_discovered".to_string(), "low".to_string(), false).await.unwrap();
        log_score_event(&conn, Some(run_id), "port_found".to_string(), "low".to_string(), false).await.unwrap();
        log_score_event(&conn, Some(run_id), "detection".to_string(), "high".to_string(), true).await.unwrap();

        let summary = get_score_summary(&conn, run_id).await.unwrap();
        assert_eq!(summary.total_score, -85);
        assert_eq!(summary.detection_count, 1);
        assert_eq!(summary.recent_events.len(), 3);
    }
}
