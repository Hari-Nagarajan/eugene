use chrono::Utc;
use tokio_rusqlite::Connection;

use crate::memory::MemoryError;
use crate::vuln::{CveRecord, CveReference, CveSeverity, CveSource};

/// Retrieve cached CVE records for a given cache key, respecting TTL.
///
/// Returns `None` if no records found or all records are older than `ttl_days`.
/// Returns `Some(records)` if fresh cached data exists.
pub async fn get_cached_cves(
    conn: &Connection,
    cache_key: String,
    ttl_days: i64,
) -> Result<Option<Vec<CveRecord>>, MemoryError> {
    conn.call(move |conn| {
        let ttl_param = format!("-{ttl_days} days");
        let mut stmt = conn.prepare(
            "SELECT cve_id, description, cvss_score, cvss_vector, severity,
                    references_json, published, source
             FROM cve_cache
             WHERE cache_key = ?1
               AND fetched_at > datetime('now', ?2)",
        )?;

        let records: Vec<CveRecord> = stmt
            .query_map(rusqlite::params![cache_key, ttl_param], |row| {
                let severity_str: String = row.get(4)?;
                let refs_json: String = row.get(5)?;
                let source_str: String = row.get(7)?;

                Ok(CveRecord {
                    cve_id: row.get(0)?,
                    description: row.get(1)?,
                    cvss_score: row.get(2)?,
                    cvss_vector: row.get(3)?,
                    severity: match severity_str.as_str() {
                        "CRITICAL" => CveSeverity::Critical,
                        "HIGH" => CveSeverity::High,
                        "MEDIUM" => CveSeverity::Medium,
                        "LOW" => CveSeverity::Low,
                        _ => CveSeverity::Unknown,
                    },
                    references: serde_json::from_str::<Vec<CveReference>>(&refs_json)
                        .unwrap_or_default(),
                    published: row.get(6)?,
                    source: match source_str.as_str() {
                        "nvd" => CveSource::Nvd,
                        _ => CveSource::Osv,
                    },
                    has_public_exploit: false,
                    exploits: vec![],
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if records.is_empty() {
            Ok(None)
        } else {
            Ok(Some(records))
        }
    })
    .await
    .map_err(MemoryError::from)
}

/// Store CVE records in the cache, replacing any existing entries for the same cache key.
///
/// All records are stored atomically within a single database call.
pub async fn store_cached_cves(
    conn: &Connection,
    cache_key: String,
    records: Vec<CveRecord>,
) -> Result<(), MemoryError> {
    conn.call(move |conn| {
        // Clear old entries for this cache key
        conn.execute(
            "DELETE FROM cve_cache WHERE cache_key = ?1",
            rusqlite::params![cache_key],
        )?;

        let now = Utc::now().to_rfc3339();
        let mut stmt = conn.prepare(
            "INSERT INTO cve_cache (cache_key, cve_id, description, cvss_score, cvss_vector,
                                    severity, references_json, published, source, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;

        for record in &records {
            let refs_json =
                serde_json::to_string(&record.references).unwrap_or_else(|_| "[]".to_string());
            let source_str = match record.source {
                CveSource::Nvd => "nvd",
                CveSource::Osv => "osv",
            };

            stmt.execute(rusqlite::params![
                cache_key,
                record.cve_id,
                record.description,
                record.cvss_score,
                record.cvss_vector,
                record.severity.as_str(),
                refs_json,
                record.published,
                source_str,
                now,
            ])?;
        }

        Ok(())
    })
    .await
    .map_err(MemoryError::from)
}

/// Delete stale CVE cache entries older than `ttl_days`.
///
/// Returns the number of rows deleted. Call this opportunistically for cleanup.
pub async fn delete_stale_cves(
    conn: &Connection,
    ttl_days: i64,
) -> Result<u64, MemoryError> {
    conn.call(move |conn| {
        let ttl_param = format!("-{ttl_days} days");
        let deleted = conn.execute(
            "DELETE FROM cve_cache WHERE fetched_at < datetime('now', ?1)",
            rusqlite::params![ttl_param],
        )?;
        Ok(deleted as u64)
    })
    .await
    .map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    fn make_test_records() -> Vec<CveRecord> {
        vec![
            CveRecord {
                cve_id: "CVE-2021-44228".to_string(),
                description: "Log4j RCE".to_string(),
                cvss_score: Some(10.0),
                cvss_vector: Some(
                    "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H".to_string(),
                ),
                severity: CveSeverity::Critical,
                references: vec![CveReference {
                    url: "https://nvd.nist.gov/vuln/detail/CVE-2021-44228".to_string(),
                    source: Some("NVD".to_string()),
                }],
                published: Some("2021-12-10".to_string()),
                source: CveSource::Nvd,
                has_public_exploit: false,
                exploits: vec![],
            },
            CveRecord {
                cve_id: "CVE-2023-12345".to_string(),
                description: "Test vulnerability".to_string(),
                cvss_score: Some(7.5),
                cvss_vector: None,
                severity: CveSeverity::High,
                references: vec![],
                published: None,
                source: CveSource::Osv,
                has_public_exploit: false,
                exploits: vec![],
            },
        ]
    }

    #[tokio::test]
    async fn test_store_and_get_roundtrip() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let records = make_test_records();
        store_cached_cves(&conn, "apache:2.4.49".to_string(), records.clone())
            .await
            .unwrap();

        let cached = get_cached_cves(&conn, "apache:2.4.49".to_string(), 7)
            .await
            .unwrap();
        assert!(cached.is_some(), "Should return cached records");

        let cached = cached.unwrap();
        assert_eq!(cached.len(), 2);
        assert_eq!(cached[0].cve_id, "CVE-2021-44228");
        assert_eq!(cached[0].severity, CveSeverity::Critical);
        assert_eq!(cached[0].source, CveSource::Nvd);
        assert_eq!(cached[0].references.len(), 1);
        assert_eq!(cached[1].cve_id, "CVE-2023-12345");
        assert_eq!(cached[1].severity, CveSeverity::High);
        assert_eq!(cached[1].source, CveSource::Osv);
    }

    #[tokio::test]
    async fn test_get_unknown_cache_key_returns_none() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let cached = get_cached_cves(&conn, "nonexistent:1.0".to_string(), 7)
            .await
            .unwrap();
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_cache_key_isolation() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let records = make_test_records();
        store_cached_cves(&conn, "key_a".to_string(), records)
            .await
            .unwrap();

        let cached = get_cached_cves(&conn, "key_b".to_string(), 7)
            .await
            .unwrap();
        assert!(cached.is_none(), "Different cache key should return None");
    }

    #[tokio::test]
    async fn test_ttl_expiry() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let records = make_test_records();
        store_cached_cves(&conn, "old_key".to_string(), records)
            .await
            .unwrap();

        // Manually backdate fetched_at to 10 days ago
        conn.call(|conn| {
            conn.execute(
                "UPDATE cve_cache SET fetched_at = datetime('now', '-10 days') WHERE cache_key = 'old_key'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let cached = get_cached_cves(&conn, "old_key".to_string(), 7)
            .await
            .unwrap();
        assert!(cached.is_none(), "Stale entries should return None");
    }

    #[tokio::test]
    async fn test_store_replaces_old_entries() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        // Store first batch
        let records = make_test_records();
        store_cached_cves(&conn, "replace_key".to_string(), records)
            .await
            .unwrap();

        // Store second batch (should replace)
        let new_records = vec![CveRecord {
            cve_id: "CVE-2024-99999".to_string(),
            description: "Replacement record".to_string(),
            cvss_score: Some(5.0),
            cvss_vector: None,
            severity: CveSeverity::Medium,
            references: vec![],
            published: None,
            source: CveSource::Osv,
            has_public_exploit: false,
            exploits: vec![],
        }];
        store_cached_cves(&conn, "replace_key".to_string(), new_records)
            .await
            .unwrap();

        let cached = get_cached_cves(&conn, "replace_key".to_string(), 7)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cached.len(), 1, "Should only have the replacement record");
        assert_eq!(cached[0].cve_id, "CVE-2024-99999");
    }

    #[tokio::test]
    async fn test_delete_stale_cves() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let records = make_test_records();
        store_cached_cves(&conn, "stale_key".to_string(), records)
            .await
            .unwrap();

        // Backdate to 10 days ago
        conn.call(|conn| {
            conn.execute(
                "UPDATE cve_cache SET fetched_at = datetime('now', '-10 days') WHERE cache_key = 'stale_key'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let deleted = delete_stale_cves(&conn, 7).await.unwrap();
        assert_eq!(deleted, 2, "Should delete 2 stale records");

        // Verify they're gone
        let count: i64 = conn
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM cve_cache WHERE cache_key = 'stale_key'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
