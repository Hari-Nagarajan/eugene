//! OSV.dev API client -- primary CVE data source.
//!
//! Queries `api.osv.dev/v1/query` (POST) with package/ecosystem/version.
//! No rate limiting needed -- OSV has no API limits.
//! Returns `Vec<CveRecord>` with CVE IDs extracted from the `aliases` field.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::vuln::types::{CveRecord, CveReference, CveSeverity, CveSource};

const OSV_API_URL: &str = "https://api.osv.dev/v1/query";
const USER_AGENT: &str = "eugene/0.1";
const TIMEOUT_SECS: u64 = 30;
const MAX_DESCRIPTION_LEN: usize = 500;

// --- Request types ---

#[derive(Serialize)]
struct OsvQuery {
    package: OsvQueryPackage,
    version: String,
}

#[derive(Serialize)]
struct OsvQueryPackage {
    name: String,
    ecosystem: String,
}

// --- Response types ---

#[derive(Deserialize)]
struct OsvResponse {
    vulns: Option<Vec<OsvVuln>>,
}

#[derive(Deserialize, Clone)]
struct OsvVuln {
    id: String,
    summary: Option<String>,
    details: Option<String>,
    severity: Option<Vec<OsvSeverityEntry>>,
    references: Option<Vec<OsvRef>>,
    aliases: Option<Vec<String>>,
    published: Option<String>,
    #[allow(dead_code)]
    modified: Option<String>,
}

#[derive(Deserialize, Clone)]
struct OsvSeverityEntry {
    #[serde(rename = "type")]
    severity_type: String,
    /// CVSS vector string (e.g., "CVSS:3.1/AV:N/AC:L/...")
    score: String,
}

#[derive(Deserialize, Clone)]
struct OsvRef {
    url: String,
    #[serde(rename = "type")]
    ref_type: Option<String>,
}

/// Client for the OSV.dev vulnerability database API.
///
/// OSV is the primary CVE data source. It has no rate limits and covers
/// most open-source packages via the Debian ecosystem.
pub struct OsvClient {
    client: reqwest::Client,
}

impl Default for OsvClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OsvClient {
    /// Create a new OSV client with standard timeout and user-agent.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .user_agent(USER_AGENT)
            .build()
            // reqwest build() only fails on TLS backend init — fatal system error
            .expect("reqwest TLS initialization failed");
        Self { client }
    }

    /// Query OSV.dev for vulnerabilities affecting a specific package version.
    ///
    /// Returns `Vec<CveRecord>` -- empty on any error (network, parse, etc.).
    /// Never returns `Err`; follows the project's error-as-value pattern.
    pub async fn query(
        &self,
        package_name: &str,
        ecosystem: &str,
        version: &str,
    ) -> Vec<CveRecord> {
        let body = OsvQuery {
            package: OsvQueryPackage {
                name: package_name.to_string(),
                ecosystem: ecosystem.to_string(),
            },
            version: version.to_string(),
        };

        let response = match self.client.post(OSV_API_URL).json(&body).send().await {
            Ok(resp) => resp,
            Err(e) => {
                log::warn!(
                    "OSV query failed for {}/{}: {}",
                    ecosystem,
                    package_name,
                    e
                );
                return Vec::new();
            }
        };

        let osv_response: OsvResponse = match response.json().await {
            Ok(parsed) => parsed,
            Err(e) => {
                log::warn!(
                    "OSV response parse failed for {}/{}: {}",
                    ecosystem,
                    package_name,
                    e
                );
                return Vec::new();
            }
        };

        osv_response
            .vulns
            .unwrap_or_default()
            .into_iter()
            .map(osv_vuln_to_cve_record)
            .collect()
    }
}

/// Convert an OSV vulnerability entry to a unified CveRecord.
///
/// Key behavior:
/// - CVE ID extracted from `aliases` (Debian entries have DSA/DLA IDs as primary `id`)
/// - CVSS v3/v4 vector parsed from `severity` array using the `cvss` crate
/// - Description truncated to 500 chars
fn osv_vuln_to_cve_record(vuln: OsvVuln) -> CveRecord {
    // Extract CVE ID from aliases (prefer CVE-* over DSA-*/DLA-*)
    let cve_id = vuln
        .aliases
        .as_ref()
        .and_then(|aliases| aliases.iter().find(|a| a.starts_with("CVE-")).cloned())
        .unwrap_or_else(|| vuln.id.clone());

    // Build description: prefer summary, fall back to truncated details
    let description = if let Some(ref summary) = vuln.summary {
        truncate_description(summary)
    } else if let Some(ref details) = vuln.details {
        truncate_description(details)
    } else {
        String::new()
    };

    // Parse CVSS from severity array
    let (cvss_score, cvss_vector) = extract_cvss_from_severity(&vuln.severity);

    let severity = match cvss_score {
        Some(score) => CveSeverity::from_score(score),
        None => CveSeverity::Unknown,
    };

    let references = vuln
        .references
        .unwrap_or_default()
        .into_iter()
        .map(|r| CveReference {
            url: r.url,
            source: r.ref_type,
        })
        .collect();

    CveRecord {
        cve_id,
        description,
        cvss_score,
        cvss_vector,
        severity,
        references,
        published: vuln.published,
        source: CveSource::Osv,
    }
}

/// Extract CVSS score and vector from OSV severity entries.
///
/// Looks for CVSS_V3 or CVSS_V4 entries. The `score` field in OSV is actually
/// the CVSS vector string, not a numeric score. We parse it with the `cvss` crate.
fn extract_cvss_from_severity(
    severity: &Option<Vec<OsvSeverityEntry>>,
) -> (Option<f64>, Option<String>) {
    let entries = match severity {
        Some(entries) => entries,
        None => return (None, None),
    };

    // Prefer CVSS_V3, accept CVSS_V4
    let entry = entries
        .iter()
        .find(|e| e.severity_type == "CVSS_V3")
        .or_else(|| entries.iter().find(|e| e.severity_type == "CVSS_V4"));

    match entry {
        Some(e) => parse_cvss_vector(&e.score),
        None => (None, None),
    }
}

/// Parse a CVSS vector string and extract the numeric score.
///
/// Returns (Some(score), Some(vector)) on success, (None, None) on failure.
fn parse_cvss_vector(vector: &str) -> (Option<f64>, Option<String>) {
    match cvss::v3::Base::from_str(vector) {
        Ok(base) => {
            let score = base.score().value();
            (Some(score), Some(vector.to_string()))
        }
        Err(_) => {
            // Try the unified Cvss parser for v4 vectors
            match cvss::Cvss::from_str(vector) {
                Ok(cvss) => {
                    let score = cvss.score();
                    (Some(score), Some(vector.to_string()))
                }
                Err(e) => {
                    log::warn!("Failed to parse CVSS vector '{}': {}", vector, e);
                    (None, None)
                }
            }
        }
    }
}

/// Truncate a string to MAX_DESCRIPTION_LEN chars, adding "..." if truncated.
fn truncate_description(s: &str) -> String {
    if s.len() <= MAX_DESCRIPTION_LEN {
        s.to_string()
    } else {
        let mut truncated = s[..MAX_DESCRIPTION_LEN].to_string();
        truncated.push_str("...");
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_osv_vuln_with_cve_alias() -> OsvVuln {
        OsvVuln {
            id: "DSA-5139-1".to_string(),
            summary: Some("A test vulnerability in OpenSSH".to_string()),
            details: None,
            severity: Some(vec![OsvSeverityEntry {
                severity_type: "CVSS_V3".to_string(),
                score: "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H".to_string(),
            }]),
            references: Some(vec![OsvRef {
                url: "https://security-tracker.debian.org/tracker/DSA-5139-1".to_string(),
                ref_type: Some("ADVISORY".to_string()),
            }]),
            aliases: Some(vec![
                "CVE-2022-12345".to_string(),
                "DSA-5139-1".to_string(),
            ]),
            published: Some("2022-05-01T00:00:00Z".to_string()),
            modified: Some("2022-06-01T00:00:00Z".to_string()),
        }
    }

    fn make_osv_vuln_no_cve_alias() -> OsvVuln {
        OsvVuln {
            id: "DLA-3456-1".to_string(),
            summary: None,
            details: Some("This is a long vulnerability description that provides details about the issue found in the package.".to_string()),
            severity: None,
            references: None,
            aliases: Some(vec!["DLA-3456-1".to_string()]),
            published: None,
            modified: None,
        }
    }

    fn make_osv_vuln_no_aliases() -> OsvVuln {
        OsvVuln {
            id: "GHSA-abcd-1234".to_string(),
            summary: Some("GitHub advisory".to_string()),
            details: None,
            severity: Some(vec![OsvSeverityEntry {
                severity_type: "CVSS_V3".to_string(),
                score: "CVSS:3.1/AV:N/AC:L/PR:L/UI:N/S:U/C:L/I:N/A:N".to_string(),
            }]),
            references: None,
            aliases: None,
            published: Some("2023-01-15T00:00:00Z".to_string()),
            modified: None,
        }
    }

    #[test]
    fn test_cve_id_extracted_from_aliases() {
        let vuln = make_osv_vuln_with_cve_alias();
        let record = osv_vuln_to_cve_record(vuln);
        assert_eq!(record.cve_id, "CVE-2022-12345");
    }

    #[test]
    fn test_cve_id_fallback_to_osv_id() {
        let vuln = make_osv_vuln_no_cve_alias();
        let record = osv_vuln_to_cve_record(vuln);
        assert_eq!(record.cve_id, "DLA-3456-1");
    }

    #[test]
    fn test_cve_id_no_aliases_uses_id() {
        let vuln = make_osv_vuln_no_aliases();
        let record = osv_vuln_to_cve_record(vuln);
        assert_eq!(record.cve_id, "GHSA-abcd-1234");
    }

    #[test]
    fn test_description_from_summary() {
        let vuln = make_osv_vuln_with_cve_alias();
        let record = osv_vuln_to_cve_record(vuln);
        assert_eq!(record.description, "A test vulnerability in OpenSSH");
    }

    #[test]
    fn test_description_fallback_to_details() {
        let vuln = make_osv_vuln_no_cve_alias();
        let record = osv_vuln_to_cve_record(vuln);
        assert!(record.description.starts_with("This is a long"));
    }

    #[test]
    fn test_description_truncation() {
        let long_desc = "A".repeat(600);
        let vuln = OsvVuln {
            id: "TEST-001".to_string(),
            summary: Some(long_desc),
            details: None,
            severity: None,
            references: None,
            aliases: None,
            published: None,
            modified: None,
        };
        let record = osv_vuln_to_cve_record(vuln);
        assert_eq!(record.description.len(), 503); // 500 + "..."
        assert!(record.description.ends_with("..."));
    }

    #[test]
    fn test_cvss_v3_parsed_from_severity() {
        let vuln = make_osv_vuln_with_cve_alias();
        let record = osv_vuln_to_cve_record(vuln);
        assert!(record.cvss_score.is_some());
        let score = record.cvss_score.unwrap();
        assert!(score >= 9.0, "Expected critical score, got {}", score);
        assert!(record.cvss_vector.is_some());
        assert!(record.cvss_vector.unwrap().starts_with("CVSS:3.1"));
    }

    #[test]
    fn test_no_cvss_when_no_severity() {
        let vuln = make_osv_vuln_no_cve_alias();
        let record = osv_vuln_to_cve_record(vuln);
        assert!(record.cvss_score.is_none());
        assert!(record.cvss_vector.is_none());
        assert_eq!(record.severity, CveSeverity::Unknown);
    }

    #[test]
    fn test_severity_from_cvss_score() {
        let vuln = make_osv_vuln_with_cve_alias();
        let record = osv_vuln_to_cve_record(vuln);
        assert_eq!(record.severity, CveSeverity::Critical);
    }

    #[test]
    fn test_source_is_osv() {
        let vuln = make_osv_vuln_with_cve_alias();
        let record = osv_vuln_to_cve_record(vuln);
        assert_eq!(record.source, CveSource::Osv);
    }

    #[test]
    fn test_references_mapped() {
        let vuln = make_osv_vuln_with_cve_alias();
        let record = osv_vuln_to_cve_record(vuln);
        assert_eq!(record.references.len(), 1);
        assert!(record.references[0]
            .url
            .contains("security-tracker.debian.org"));
        assert_eq!(record.references[0].source, Some("ADVISORY".to_string()));
    }

    #[test]
    fn test_published_preserved() {
        let vuln = make_osv_vuln_with_cve_alias();
        let record = osv_vuln_to_cve_record(vuln);
        assert_eq!(record.published, Some("2022-05-01T00:00:00Z".to_string()));
    }

    #[test]
    fn test_parse_cvss_vector_valid() {
        let (score, vector) =
            parse_cvss_vector("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H");
        assert!(score.is_some());
        assert!((score.unwrap() - 10.0).abs() < 0.01);
        assert!(vector.is_some());
    }

    #[test]
    fn test_parse_cvss_vector_invalid() {
        let (score, vector) = parse_cvss_vector("not-a-valid-vector");
        assert!(score.is_none());
        assert!(vector.is_none());
    }

    #[test]
    fn test_truncate_description_short() {
        let short = "Short description";
        assert_eq!(truncate_description(short), short);
    }

    #[test]
    fn test_truncate_description_long() {
        let long = "A".repeat(600);
        let result = truncate_description(&long);
        assert_eq!(result.len(), 503);
        assert!(result.ends_with("..."));
    }

    #[cfg(feature = "live-tests")]
    #[tokio::test]
    async fn test_osv_query_live_openssh() {
        let client = OsvClient::new();
        let results = client.query("openssh", "Debian", "8.4p1").await;
        assert!(
            !results.is_empty(),
            "OSV should return vulnerabilities for openssh:8.4p1"
        );
        // At least one should have a CVE ID
        let has_cve = results.iter().any(|r| r.cve_id.starts_with("CVE-"));
        assert!(has_cve, "At least one result should have a CVE ID");
    }

    #[cfg(feature = "live-tests")]
    #[tokio::test]
    async fn test_osv_query_live_apache() {
        let client = OsvClient::new();
        let results = client.query("apache2", "Debian", "2.4.49").await;
        assert!(
            !results.is_empty(),
            "OSV should return vulnerabilities for apache2:2.4.49"
        );
    }
}
