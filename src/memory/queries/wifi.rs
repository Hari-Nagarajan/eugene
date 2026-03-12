use chrono::Utc;
use tokio_rusqlite::Connection;

use crate::memory::MemoryError;
use crate::wifi::types::WifiAccessPoint;

/// Insert or replace a wifi access point record.
///
/// Uses INSERT OR REPLACE with UNIQUE(run_id, bssid) to handle rescan updates.
/// Preserves `first_seen` from the existing row if one exists (via COALESCE sub-select).
/// Returns the rowid of the inserted/replaced row.
#[allow(clippy::too_many_arguments)]
pub async fn insert_wifi_ap(
    conn: &Connection,
    run_id: Option<i64>,
    bssid: String,
    essid: Option<String>,
    channel: Option<i32>,
    frequency: Option<i32>,
    encryption: Option<String>,
    cipher: Option<String>,
    auth: Option<String>,
    signal_dbm: Option<i32>,
) -> Result<i64, MemoryError> {
    let now = Utc::now().to_rfc3339();

    conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO wifi_access_points
                (run_id, bssid, essid, channel, frequency, encryption, cipher, auth, signal_dbm, first_seen, last_seen)
             VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                COALESCE((SELECT first_seen FROM wifi_access_points WHERE run_id IS ?1 AND bssid = ?2), ?10),
                ?10
             )",
            rusqlite::params![run_id, bssid, essid, channel, frequency, encryption, cipher, auth, signal_dbm, now],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .map_err(MemoryError::from)
}

/// Retrieve all wifi access points for a given run, ordered by signal strength (strongest first).
pub async fn get_wifi_aps(
    conn: &Connection,
    run_id: i64,
) -> Result<Vec<WifiAccessPoint>, MemoryError> {
    conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, run_id, bssid, essid, channel, frequency, encryption, cipher, auth, signal_dbm, client_count, wps_enabled, first_seen, last_seen
             FROM wifi_access_points
             WHERE run_id = ?1
             ORDER BY signal_dbm DESC",
        )?;

        let aps = stmt
            .query_map(rusqlite::params![run_id], |row| {
                let wps_raw: Option<i32> = row.get(11)?;
                Ok(WifiAccessPoint {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    bssid: row.get(2)?,
                    essid: row.get(3)?,
                    channel: row.get(4)?,
                    frequency: row.get(5)?,
                    encryption: row.get(6)?,
                    cipher: row.get(7)?,
                    auth: row.get(8)?,
                    signal_dbm: row.get(9)?,
                    client_count: row.get(10)?,
                    wps_enabled: wps_raw.map(|v| v != 0),
                    first_seen: row.get(12)?,
                    last_seen: row.get(13)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(aps)
    })
    .await
    .map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{create_run, init_schema, open_memory_store};

    async fn setup() -> (std::sync::Arc<Connection>, i64) {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();
        (conn, run_id)
    }

    #[tokio::test]
    async fn test_insert_wifi_ap_creates_row_and_returns_id() {
        let (conn, run_id) = setup().await;
        let id = insert_wifi_ap(
            &conn,
            Some(run_id),
            "AA:BB:CC:DD:EE:FF".to_string(),
            Some("TestNetwork".to_string()),
            Some(6),
            Some(2437),
            Some("WPA2".to_string()),
            Some("CCMP".to_string()),
            Some("PSK".to_string()),
            Some(-42),
        )
        .await
        .unwrap();

        assert!(id > 0, "insert_wifi_ap should return valid ID");
    }

    #[tokio::test]
    async fn test_insert_or_replace_same_bssid_updates() {
        let (conn, run_id) = setup().await;

        // First insert
        let _id1 = insert_wifi_ap(
            &conn,
            Some(run_id),
            "AA:BB:CC:DD:EE:FF".to_string(),
            Some("OldName".to_string()),
            Some(6),
            None,
            None,
            None,
            None,
            Some(-50),
        )
        .await
        .unwrap();

        // Second insert with same (run_id, bssid) - should replace
        let id2 = insert_wifi_ap(
            &conn,
            Some(run_id),
            "AA:BB:CC:DD:EE:FF".to_string(),
            Some("NewName".to_string()),
            Some(11),
            None,
            None,
            None,
            None,
            Some(-30),
        )
        .await
        .unwrap();

        // Should be a new row id (INSERT OR REPLACE deletes + inserts)
        assert!(id2 > 0);

        // Verify only one row exists for this bssid
        let aps = get_wifi_aps(&conn, run_id).await.unwrap();
        let matching: Vec<_> = aps
            .iter()
            .filter(|ap| ap.bssid == "AA:BB:CC:DD:EE:FF")
            .collect();
        assert_eq!(matching.len(), 1, "Should have exactly one row after replace");
        assert_eq!(matching[0].essid.as_deref(), Some("NewName"));
        assert_eq!(matching[0].channel, Some(11));
        assert_eq!(matching[0].signal_dbm, Some(-30));
    }

    #[tokio::test]
    async fn test_insert_or_replace_preserves_first_seen() {
        let (conn, run_id) = setup().await;

        // First insert
        insert_wifi_ap(
            &conn,
            Some(run_id),
            "11:22:33:44:55:66".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        // Grab first_seen from the first insert
        let first_seen_original: String = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT first_seen FROM wifi_access_points WHERE bssid = '11:22:33:44:55:66'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();

        // Small delay to ensure timestamp differs
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Second insert (rescan update)
        insert_wifi_ap(
            &conn,
            Some(run_id),
            "11:22:33:44:55:66".to_string(),
            Some("Appeared".to_string()),
            None,
            None,
            None,
            None,
            None,
            Some(-60),
        )
        .await
        .unwrap();

        let (first_seen_after, last_seen_after): (String, String) = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT first_seen, last_seen FROM wifi_access_points WHERE bssid = '11:22:33:44:55:66'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .await
            .unwrap();

        assert_eq!(
            first_seen_original, first_seen_after,
            "first_seen should be preserved on rescan update"
        );
        assert_ne!(
            first_seen_after, last_seen_after,
            "last_seen should differ from first_seen after update"
        );
    }

    #[tokio::test]
    async fn test_get_wifi_aps_returns_all_for_run() {
        let (conn, run_id) = setup().await;

        // Insert two APs
        insert_wifi_ap(
            &conn,
            Some(run_id),
            "AA:AA:AA:AA:AA:AA".to_string(),
            Some("Net1".to_string()),
            Some(1),
            None,
            None,
            None,
            None,
            Some(-40),
        )
        .await
        .unwrap();

        insert_wifi_ap(
            &conn,
            Some(run_id),
            "BB:BB:BB:BB:BB:BB".to_string(),
            Some("Net2".to_string()),
            Some(6),
            None,
            None,
            None,
            None,
            Some(-70),
        )
        .await
        .unwrap();

        let aps = get_wifi_aps(&conn, run_id).await.unwrap();
        assert_eq!(aps.len(), 2);
        // Ordered by signal_dbm DESC: -40 first, -70 second
        assert_eq!(aps[0].bssid, "AA:AA:AA:AA:AA:AA");
        assert_eq!(aps[1].bssid, "BB:BB:BB:BB:BB:BB");
    }

    #[tokio::test]
    async fn test_get_wifi_aps_empty_run_returns_empty_vec() {
        let (conn, run_id) = setup().await;
        let aps = get_wifi_aps(&conn, run_id).await.unwrap();
        assert!(aps.is_empty(), "Should return empty vec for run with no APs");
    }
}
