use chrono::Utc;
use serde::Serialize;
use tokio_rusqlite::Connection;

use crate::memory::MemoryError;
use crate::wifi::types::WifiAccessPoint;

/// Migrate wifi schema for existing databases.
///
/// Checks for the existence of `client_count` and `wps_enabled` columns on
/// `wifi_access_points` and adds them if missing. This handles the case where
/// the database was created before these columns were added to schema.sql.
pub async fn migrate_wifi_schema(conn: &Connection) -> Result<(), MemoryError> {
    conn.call(|conn| {
        // Check if client_count column exists
        let has_client_count = {
            let mut stmt = conn.prepare("PRAGMA table_info(wifi_access_points)")?;
            let cols: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            cols.iter().any(|c| c == "client_count")
        };

        if !has_client_count {
            conn.execute(
                "ALTER TABLE wifi_access_points ADD COLUMN client_count INTEGER",
                [],
            )?;
        }

        // Check if wps_enabled column exists
        let has_wps_enabled = {
            let mut stmt = conn.prepare("PRAGMA table_info(wifi_access_points)")?;
            let cols: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            cols.iter().any(|c| c == "wps_enabled")
        };

        if !has_wps_enabled {
            conn.execute(
                "ALTER TABLE wifi_access_points ADD COLUMN wps_enabled INTEGER",
                [],
            )?;
        }

        Ok(())
    })
    .await
    .map_err(MemoryError::from)
}

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
    client_count: Option<i32>,
) -> Result<i64, MemoryError> {
    let now = Utc::now().to_rfc3339();

    conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO wifi_access_points
                (run_id, bssid, essid, channel, frequency, encryption, cipher, auth, signal_dbm, client_count, first_seen, last_seen)
             VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                COALESCE((SELECT first_seen FROM wifi_access_points WHERE run_id IS ?1 AND bssid = ?2), ?11),
                ?11
             )",
            rusqlite::params![run_id, bssid, essid, channel, frequency, encryption, cipher, auth, signal_dbm, client_count, now],
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

/// A wifi client station record.
#[derive(Debug, Clone, Serialize)]
pub struct WifiClient {
    pub id: Option<i64>,
    pub run_id: Option<i64>,
    pub mac: String,
    pub associated_bssid: Option<String>,
    pub signal_dbm: Option<i32>,
    pub packets: Option<i32>,
    pub first_seen: String,
    pub last_seen: String,
}

/// Insert or update a wifi client station record.
///
/// Uses INSERT OR REPLACE with UNIQUE(run_id, mac) to handle rescan updates.
/// Preserves `first_seen` from the existing row if one exists (via COALESCE sub-select).
/// Returns the rowid of the inserted/replaced row.
pub async fn insert_wifi_client(
    conn: &Connection,
    run_id: Option<i64>,
    mac: String,
    associated_bssid: Option<String>,
    signal_dbm: Option<i32>,
    packets: Option<i32>,
) -> Result<i64, MemoryError> {
    let now = Utc::now().to_rfc3339();

    conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO wifi_clients
                (run_id, mac, associated_bssid, signal_dbm, packets, first_seen, last_seen)
             VALUES (
                ?1, ?2, ?3, ?4, ?5,
                COALESCE((SELECT first_seen FROM wifi_clients WHERE run_id IS ?1 AND mac = ?2), ?6),
                ?6
             )",
            rusqlite::params![run_id, mac, associated_bssid, signal_dbm, packets, now],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .map_err(MemoryError::from)
}

/// Insert a probed SSID for a wifi client. Uses INSERT OR IGNORE to handle
/// the UNIQUE(run_id, client_mac, probed_ssid) constraint -- duplicates are silently skipped.
pub async fn insert_client_probe(
    conn: &Connection,
    run_id: Option<i64>,
    client_mac: String,
    probed_ssid: String,
) -> Result<i64, MemoryError> {
    let now = Utc::now().to_rfc3339();

    conn.call(move |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO wifi_client_probes
                (run_id, client_mac, probed_ssid, first_seen)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![run_id, client_mac, probed_ssid, now],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .map_err(MemoryError::from)
}

/// Retrieve all wifi clients for a given run, ordered by signal strength (strongest first).
pub async fn get_wifi_clients(
    conn: &Connection,
    run_id: i64,
) -> Result<Vec<WifiClient>, MemoryError> {
    conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, run_id, mac, associated_bssid, signal_dbm, packets, first_seen, last_seen
             FROM wifi_clients
             WHERE run_id = ?1
             ORDER BY signal_dbm DESC",
        )?;

        let clients = stmt
            .query_map(rusqlite::params![run_id], |row| {
                Ok(WifiClient {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    mac: row.get(2)?,
                    associated_bssid: row.get(3)?,
                    signal_dbm: row.get(4)?,
                    packets: row.get(5)?,
                    first_seen: row.get(6)?,
                    last_seen: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(clients)
    })
    .await
    .map_err(MemoryError::from)
}

/// A matched probe result: a client whose probed SSID matches a visible AP in the same run.
#[derive(Debug, Clone, Serialize)]
pub struct MatchedProbe {
    pub client_mac: String,
    pub probed_ssid: String,
    pub matched_ap_bssid: String,
    pub channel: Option<i32>,
    pub encryption: Option<String>,
    pub ap_signal: Option<i32>,
    pub client_signal: Option<i32>,
    pub associated_bssid: Option<String>,
}

/// Get clients whose probed SSIDs match visible APs in the same scan run.
///
/// Joins wifi_client_probes against wifi_access_points on probed_ssid = essid
/// within the same run_id. These are high-value deauth target candidates:
/// the client is looking for a network that's actually present.
pub async fn get_matched_probes(
    conn: &Connection,
    run_id: i64,
) -> Result<Vec<MatchedProbe>, MemoryError> {
    conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT cp.client_mac, cp.probed_ssid, ap.bssid AS matched_ap_bssid,
                    ap.channel, ap.encryption, ap.signal_dbm AS ap_signal,
                    c.signal_dbm AS client_signal, c.associated_bssid
             FROM wifi_client_probes cp
             JOIN wifi_access_points ap ON cp.probed_ssid = ap.essid AND cp.run_id = ap.run_id
             JOIN wifi_clients c ON cp.client_mac = c.mac AND cp.run_id = c.run_id
             WHERE cp.run_id = ?1
             ORDER BY ap.signal_dbm DESC",
        )?;

        let probes = stmt
            .query_map(rusqlite::params![run_id], |row| {
                Ok(MatchedProbe {
                    client_mac: row.get(0)?,
                    probed_ssid: row.get(1)?,
                    matched_ap_bssid: row.get(2)?,
                    channel: row.get(3)?,
                    encryption: row.get(4)?,
                    ap_signal: row.get(5)?,
                    client_signal: row.get(6)?,
                    associated_bssid: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(probes)
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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

    #[tokio::test]
    async fn test_insert_wifi_ap_with_client_count() {
        let (conn, run_id) = setup().await;
        let id = insert_wifi_ap(
            &conn,
            Some(run_id),
            "AA:BB:CC:DD:EE:FF".to_string(),
            Some("TestNet".to_string()),
            Some(6),
            None,
            None,
            None,
            None,
            Some(-42),
            Some(5),
        )
        .await
        .unwrap();

        assert!(id > 0);
        let aps = get_wifi_aps(&conn, run_id).await.unwrap();
        assert_eq!(aps.len(), 1);
        assert_eq!(aps[0].client_count, Some(5));
    }

    // -- Wifi client tests --

    #[tokio::test]
    async fn test_insert_wifi_client_persists_and_returns_id() {
        let (conn, run_id) = setup().await;
        let id = insert_wifi_client(
            &conn,
            Some(run_id),
            "CC:DD:EE:FF:00:11".to_string(),
            Some("AA:BB:CC:DD:EE:FF".to_string()),
            Some(-55),
            Some(20),
        )
        .await
        .unwrap();

        assert!(id > 0, "insert_wifi_client should return valid ID");
    }

    #[tokio::test]
    async fn test_insert_wifi_client_upsert_preserves_first_seen() {
        let (conn, run_id) = setup().await;

        // First insert
        insert_wifi_client(
            &conn,
            Some(run_id),
            "CC:DD:EE:FF:00:11".to_string(),
            Some("AA:BB:CC:DD:EE:FF".to_string()),
            Some(-55),
            Some(20),
        )
        .await
        .unwrap();

        let first_seen_original: String = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT first_seen FROM wifi_clients WHERE mac = 'CC:DD:EE:FF:00:11'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Second insert -- same (run_id, mac), different signal/bssid
        insert_wifi_client(
            &conn,
            Some(run_id),
            "CC:DD:EE:FF:00:11".to_string(),
            Some("11:22:33:44:55:66".to_string()),
            Some(-40),
            Some(50),
        )
        .await
        .unwrap();

        let first_seen_after: String = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT first_seen FROM wifi_clients WHERE mac = 'CC:DD:EE:FF:00:11'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();

        assert_eq!(
            first_seen_original, first_seen_after,
            "first_seen should be preserved on client rescan update"
        );
    }

    #[tokio::test]
    async fn test_insert_client_probe_persists_ssid() {
        let (conn, run_id) = setup().await;
        let id = insert_client_probe(
            &conn,
            Some(run_id),
            "CC:DD:EE:FF:00:11".to_string(),
            "MyNetwork".to_string(),
        )
        .await
        .unwrap();

        assert!(id > 0, "insert_client_probe should return valid ID");
    }

    #[tokio::test]
    async fn test_insert_client_probe_duplicate_does_not_error() {
        let (conn, run_id) = setup().await;

        // First insert
        insert_client_probe(
            &conn,
            Some(run_id),
            "CC:DD:EE:FF:00:11".to_string(),
            "MyNetwork".to_string(),
        )
        .await
        .unwrap();

        // Duplicate insert -- should not error (INSERT OR IGNORE)
        let result = insert_client_probe(
            &conn,
            Some(run_id),
            "CC:DD:EE:FF:00:11".to_string(),
            "MyNetwork".to_string(),
        )
        .await;

        assert!(result.is_ok(), "Duplicate probe insert should not error");
    }

    #[tokio::test]
    async fn test_get_wifi_clients_returns_ordered_by_signal() {
        let (conn, run_id) = setup().await;

        // Insert two clients with different signal strengths
        insert_wifi_client(
            &conn,
            Some(run_id),
            "AA:AA:AA:AA:AA:AA".to_string(),
            None,
            Some(-70),
            None,
        )
        .await
        .unwrap();

        insert_wifi_client(
            &conn,
            Some(run_id),
            "BB:BB:BB:BB:BB:BB".to_string(),
            None,
            Some(-40),
            None,
        )
        .await
        .unwrap();

        let clients = get_wifi_clients(&conn, run_id).await.unwrap();
        assert_eq!(clients.len(), 2);
        // Ordered by signal_dbm DESC: -40 first, -70 second
        assert_eq!(clients[0].mac, "BB:BB:BB:BB:BB:BB");
        assert_eq!(clients[1].mac, "AA:AA:AA:AA:AA:AA");
    }

    #[tokio::test]
    async fn test_migrate_wifi_schema_is_idempotent() {
        let (conn, _run_id) = setup().await;

        // Schema already has client_count and wps_enabled from CREATE TABLE.
        // migrate_wifi_schema should be a no-op and not error.
        migrate_wifi_schema(&conn).await.unwrap();
        migrate_wifi_schema(&conn).await.unwrap();
    }

    // -- Matched probes tests --

    #[tokio::test]
    async fn test_get_matched_probes_empty_when_no_matches() {
        let (conn, run_id) = setup().await;

        // Insert an AP
        insert_wifi_ap(
            &conn, Some(run_id), "AA:BB:CC:DD:EE:FF".to_string(),
            Some("HomeNetwork".to_string()), Some(6), None,
            Some("WPA2".to_string()), None, None, Some(-42), None,
        ).await.unwrap();

        // Insert a client with a probe that does NOT match any AP
        insert_wifi_client(
            &conn, Some(run_id), "11:22:33:44:55:66".to_string(),
            None, Some(-50), None,
        ).await.unwrap();
        insert_client_probe(
            &conn, Some(run_id), "11:22:33:44:55:66".to_string(),
            "NonExistentNetwork".to_string(),
        ).await.unwrap();

        let matched = get_matched_probes(&conn, run_id).await.unwrap();
        assert!(matched.is_empty(), "No probes should match when SSID differs");
    }

    #[tokio::test]
    async fn test_get_matched_probes_returns_match_when_probe_equals_essid() {
        let (conn, run_id) = setup().await;

        // Insert an AP with essid "TargetNet"
        insert_wifi_ap(
            &conn, Some(run_id), "AA:BB:CC:DD:EE:FF".to_string(),
            Some("TargetNet".to_string()), Some(11), None,
            Some("WPA2".to_string()), None, None, Some(-35), None,
        ).await.unwrap();

        // Insert a client that probes "TargetNet"
        insert_wifi_client(
            &conn, Some(run_id), "11:22:33:44:55:66".to_string(),
            Some("AA:BB:CC:DD:EE:FF".to_string()), Some(-45), None,
        ).await.unwrap();
        insert_client_probe(
            &conn, Some(run_id), "11:22:33:44:55:66".to_string(),
            "TargetNet".to_string(),
        ).await.unwrap();

        let matched = get_matched_probes(&conn, run_id).await.unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].client_mac, "11:22:33:44:55:66");
        assert_eq!(matched[0].probed_ssid, "TargetNet");
        assert_eq!(matched[0].matched_ap_bssid, "AA:BB:CC:DD:EE:FF");
        assert_eq!(matched[0].channel, Some(11));
        assert_eq!(matched[0].encryption.as_deref(), Some("WPA2"));
        assert_eq!(matched[0].ap_signal, Some(-35));
        assert_eq!(matched[0].client_signal, Some(-45));
        assert_eq!(matched[0].associated_bssid.as_deref(), Some("AA:BB:CC:DD:EE:FF"));
    }

    #[tokio::test]
    async fn test_get_matched_probes_no_cross_run_matching() {
        let (conn, run_id1) = setup().await;
        let run_id2 = create_run(&conn, "test2".to_string(), None).await.unwrap();

        // Run 1: AP with essid "SharedNet"
        insert_wifi_ap(
            &conn, Some(run_id1), "AA:BB:CC:DD:EE:FF".to_string(),
            Some("SharedNet".to_string()), Some(6), None,
            None, None, None, Some(-40), None,
        ).await.unwrap();

        // Run 2: client probing "SharedNet" -- should NOT match run 1's AP
        insert_wifi_client(
            &conn, Some(run_id2), "11:22:33:44:55:66".to_string(),
            None, Some(-50), None,
        ).await.unwrap();
        insert_client_probe(
            &conn, Some(run_id2), "11:22:33:44:55:66".to_string(),
            "SharedNet".to_string(),
        ).await.unwrap();

        // Run 1 should have no matches (no clients in run 1)
        let matched1 = get_matched_probes(&conn, run_id1).await.unwrap();
        assert!(matched1.is_empty(), "Run 1 has AP but no matching client probes");

        // Run 2 should have no matches (no APs in run 2)
        let matched2 = get_matched_probes(&conn, run_id2).await.unwrap();
        assert!(matched2.is_empty(), "Run 2 has probe but no matching AP");
    }
}
