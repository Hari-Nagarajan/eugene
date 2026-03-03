//! Orchestrated CVE lookup with cache, OSV primary, NVD CVSS enrichment, and NVD CPE fallback.
//!
//! This is the main entry point for all CVE queries. Downstream code calls
//! `lookup_cves(osv, nvd, conn, service, version)` and gets cached, rate-limited,
//! structured vulnerability data.
//!
//! # Lookup flow
//!
//! 1. Normalize inputs and build cache key
//! 2. Check SQLite cache (7-day TTL)
//! 3. Try OSV.dev (primary, no rate limits)
//! 4. Enrich missing CVSS from NVD (at most 5 calls, rate-limited)
//! 5. Fall back to NVD CPE query if OSV returned nothing
//! 6. Store results in cache
//! 7. Return results

use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::memory::{get_cached_cves, store_cached_cves};
use crate::vuln::cpe::{build_cpe, extract_version, service_to_cpe, service_to_osv};
use crate::vuln::nvd::NvdClient;
use crate::vuln::osv::OsvClient;
use crate::vuln::types::CveRecord;

/// Maximum number of NVD get_cve calls for CVSS enrichment per lookup.
/// At 6 seconds/call (unauthenticated), this caps at 30 seconds.
const MAX_CVSS_ENRICHMENT: usize = 5;

/// Cache TTL in days.
const CACHE_TTL_DAYS: i64 = 7;

/// Orchestrated CVE lookup: cache -> OSV -> CVSS enrichment -> NVD fallback -> cache store.
///
/// Returns `Vec<CveRecord>` -- empty if no vulnerabilities found or all lookups failed.
/// Never panics or returns errors; all failures are logged as warnings.
pub async fn lookup_cves(
    osv: &OsvClient,
    nvd: &NvdClient,
    conn: &Arc<Connection>,
    service: &str,
    version: &str,
) -> Vec<CveRecord> {
    let clean_version = extract_version(version);
    let key = cache_key(service, &clean_version);

    // 1. Check cache
    match get_cached_cves(conn, key.clone(), CACHE_TTL_DAYS).await {
        Ok(Some(records)) => {
            log::debug!("Cache hit for {}: {} records", key, records.len());
            return records;
        }
        Ok(None) => {
            log::debug!("Cache miss for {}", key);
        }
        Err(e) => {
            log::warn!("Cache lookup failed for {}: {}", key, e);
            // Continue -- don't let cache errors block lookup
        }
    }

    // 2. Try OSV primary
    let mut records = Vec::new();
    if let Some(pkg) = service_to_osv(service) {
        records = osv.query(pkg.name, pkg.ecosystem, &clean_version).await;
        log::debug!(
            "OSV returned {} results for {}/{}",
            records.len(),
            pkg.ecosystem,
            pkg.name
        );
    }

    // 3. Enrich missing CVSS from NVD (at most MAX_CVSS_ENRICHMENT calls)
    if !records.is_empty() {
        let cve_ids_needing_cvss: Vec<String> = records
            .iter()
            .filter(|r| r.cvss_score.is_none() && r.cve_id.starts_with("CVE-"))
            .take(MAX_CVSS_ENRICHMENT)
            .map(|r| r.cve_id.clone())
            .collect();

        for cve_id in &cve_ids_needing_cvss {
            if let Some(nvd_record) = nvd.get_cve(cve_id).await {
                if nvd_record.cvss_score.is_some() {
                    // Find and update the matching OSV record
                    if let Some(record) = records.iter_mut().find(|r| &r.cve_id == cve_id) {
                        record.cvss_score = nvd_record.cvss_score;
                        record.cvss_vector = nvd_record.cvss_vector;
                        record.severity = nvd_record.severity;
                    }
                }
            }
        }
    }

    // 4. NVD CPE fallback (if OSV returned nothing or no OSV mapping exists)
    if records.is_empty() {
        if let Some(mapping) = service_to_cpe(service) {
            let cpe = build_cpe(mapping.vendor, mapping.product, &clean_version);
            records = nvd.search_by_cpe(&cpe).await;
            log::debug!("NVD CPE fallback returned {} results for {}", records.len(), cpe);
        }
    }

    // 5. Store in cache
    if !records.is_empty() {
        if let Err(e) = store_cached_cves(conn, key.clone(), records.clone()).await {
            log::warn!("Cache store failed for {}: {}", key, e);
        }
    }

    records
}

/// Normalize a cache key from service name and version.
///
/// Lowercases, replaces spaces and hyphens with underscores.
/// Format: "service_name:version"
///
/// # Examples
/// - ("Apache httpd", "2.4.49") -> "apache_httpd:2.4.49"
/// - ("OpenSSH", "8.4p1") -> "openssh:8.4p1"
/// - ("My-Service", "1.0") -> "my_service:1.0"
fn cache_key(service: &str, version: &str) -> String {
    let normalized = service
        .to_lowercase()
        .replace(' ', "_")
        .replace('-', "_");
    format!("{}:{}", normalized, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_basic() {
        assert_eq!(cache_key("openssh", "8.4p1"), "openssh:8.4p1");
    }

    #[test]
    fn test_cache_key_with_spaces() {
        assert_eq!(cache_key("Apache httpd", "2.4.49"), "apache_httpd:2.4.49");
    }

    #[test]
    fn test_cache_key_with_hyphens() {
        assert_eq!(cache_key("My-Service", "1.0"), "my_service:1.0");
    }

    #[test]
    fn test_cache_key_uppercase() {
        assert_eq!(cache_key("OpenSSH", "8.4p1"), "openssh:8.4p1");
    }

    #[test]
    fn test_cache_key_mixed() {
        assert_eq!(
            cache_key("Apache HTTP Server", "2.4.49"),
            "apache_http_server:2.4.49"
        );
    }

    #[test]
    fn test_cache_key_empty_version() {
        assert_eq!(cache_key("nginx", ""), "nginx:");
    }

    #[cfg(feature = "live-tests")]
    #[tokio::test]
    async fn test_lookup_cves_integration() {
        use crate::memory::{init_schema, open_memory_store};
        use crate::vuln::{OsvClient, NvdClient, RateLimiter};

        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let osv = OsvClient::new();
        let rate_limiter = RateLimiter::new(false);
        let nvd = NvdClient::new(None, rate_limiter);

        // First call: should hit APIs
        let results = lookup_cves(&osv, &nvd, &conn, "openssh", "8.4p1").await;
        assert!(!results.is_empty(), "Should find CVEs for openssh 8.4p1");

        // At least one should have a CVE ID
        let has_cve = results.iter().any(|r| r.cve_id.starts_with("CVE-"));
        assert!(has_cve, "At least one result should have a CVE ID");

        // At least some should have CVSS scores (from OSV or NVD enrichment)
        let has_cvss = results.iter().any(|r| r.cvss_score.is_some());
        assert!(has_cvss, "At least some results should have CVSS scores");

        // Second call: should hit cache (much faster)
        let cached_results = lookup_cves(&osv, &nvd, &conn, "openssh", "8.4p1").await;
        assert_eq!(
            cached_results.len(),
            results.len(),
            "Cached results should match original"
        );
    }
}
