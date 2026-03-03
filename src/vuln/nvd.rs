//! NVD API v2.0 client -- CVE fallback and CVSS enrichment source.
//!
//! Queries `services.nvd.nist.gov/rest/json/cves/2.0` (GET) with CPE name or CVE ID.
//! Rate-limited via shared `RateLimiter` to respect NVD's 5 req/30s (or 50/30s with key).

use serde::Deserialize;

use crate::vuln::rate_limiter::RateLimiter;
use crate::vuln::types::{CveRecord, CveReference, CveSeverity, CveSource};

const NVD_API_URL: &str = "https://services.nvd.nist.gov/rest/json/cves/2.0";
const USER_AGENT: &str = "eugene/0.1";
const TIMEOUT_SECS: u64 = 30;
const MAX_DESCRIPTION_LEN: usize = 500;
const MAX_REFERENCES: usize = 5;
const RESULTS_PER_PAGE: u32 = 20;

// --- Response types ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NvdResponse {
    #[allow(dead_code)]
    results_per_page: u32,
    #[allow(dead_code)]
    start_index: u32,
    #[allow(dead_code)]
    total_results: u32,
    vulnerabilities: Vec<NvdVulnerability>,
}

#[derive(Deserialize, Clone)]
struct NvdVulnerability {
    cve: NvdCve,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NvdCve {
    id: String,
    descriptions: Vec<NvdDescription>,
    metrics: Option<NvdMetrics>,
    references: Option<Vec<NvdReference>>,
    published: Option<String>,
    #[allow(dead_code)]
    last_modified: Option<String>,
}

#[derive(Deserialize, Clone)]
struct NvdDescription {
    lang: String,
    value: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NvdMetrics {
    cvss_metric_v31: Option<Vec<NvdCvssMetric>>,
    cvss_metric_v30: Option<Vec<NvdCvssMetric>>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NvdCvssMetric {
    cvss_data: NvdCvssData,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NvdCvssData {
    base_score: f64,
    #[allow(dead_code)]
    base_severity: String,
    vector_string: String,
}

#[derive(Deserialize, Clone)]
struct NvdReference {
    url: String,
    source: Option<String>,
}

/// Client for the NIST National Vulnerability Database API v2.0.
///
/// Used as a fallback when OSV.dev lacks coverage, and for CVSS enrichment
/// when OSV results are missing CVSS vectors.
pub struct NvdClient {
    client: reqwest::Client,
    api_key: Option<String>,
    rate_limiter: RateLimiter,
}

impl NvdClient {
    /// Create a new NVD client.
    ///
    /// `api_key`: Optional NVD API key for higher rate limits.
    /// `rate_limiter`: Shared rate limiter (should match the API key status).
    pub fn new(api_key: Option<String>, rate_limiter: RateLimiter) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .user_agent(USER_AGENT)
            .build()
            .expect("reqwest client build should not fail");
        Self {
            client,
            api_key,
            rate_limiter,
        }
    }

    /// Search NVD for CVEs matching a CPE string.
    ///
    /// Returns up to 20 results. Rate-limited via the shared `RateLimiter`.
    /// Returns empty `Vec` on any error (network, parse, rate limit).
    pub async fn search_by_cpe(&self, cpe: &str) -> Vec<CveRecord> {
        self.rate_limiter.wait().await;

        let url = format!(
            "{}?cpeName={}&resultsPerPage={}",
            NVD_API_URL, cpe, RESULTS_PER_PAGE
        );

        let mut request = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            request = request.header("apiKey", key);
        }

        let response = match request.send().await {
            Ok(resp) => {
                if resp.status() == reqwest::StatusCode::FORBIDDEN {
                    log::warn!("NVD rate limit hit (HTTP 403), backing off");
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    return Vec::new();
                }
                resp
            }
            Err(e) => {
                log::warn!("NVD CPE search failed for {}: {}", cpe, e);
                return Vec::new();
            }
        };

        let nvd_response: NvdResponse = match response.json().await {
            Ok(parsed) => parsed,
            Err(e) => {
                log::warn!("NVD response parse failed for CPE {}: {}", cpe, e);
                return Vec::new();
            }
        };

        nvd_response
            .vulnerabilities
            .into_iter()
            .map(|v| nvd_cve_to_record(v.cve))
            .collect()
    }

    /// Fetch a single CVE by ID for CVSS enrichment.
    ///
    /// Rate-limited. Returns `None` on any error.
    pub async fn get_cve(&self, cve_id: &str) -> Option<CveRecord> {
        self.rate_limiter.wait().await;

        let url = format!("{}?cveId={}", NVD_API_URL, cve_id);

        let mut request = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            request = request.header("apiKey", key);
        }

        let response = match request.send().await {
            Ok(resp) => {
                if resp.status() == reqwest::StatusCode::FORBIDDEN {
                    log::warn!("NVD rate limit hit (HTTP 403) for CVE {}, backing off", cve_id);
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    return None;
                }
                resp
            }
            Err(e) => {
                log::warn!("NVD get_cve failed for {}: {}", cve_id, e);
                return None;
            }
        };

        let nvd_response: NvdResponse = match response.json().await {
            Ok(parsed) => parsed,
            Err(e) => {
                log::warn!("NVD response parse failed for CVE {}: {}", cve_id, e);
                return None;
            }
        };

        nvd_response
            .vulnerabilities
            .into_iter()
            .next()
            .map(|v| nvd_cve_to_record(v.cve))
    }
}

/// Convert an NVD CVE entry to a unified CveRecord.
///
/// NVD provides CVSS scores directly (no parsing needed, unlike OSV).
fn nvd_cve_to_record(cve: NvdCve) -> CveRecord {
    let description = cve
        .descriptions
        .iter()
        .find(|d| d.lang == "en")
        .map(|d| truncate_description(&d.value))
        .unwrap_or_default();

    let (cvss_score, cvss_vector) = extract_nvd_cvss(&cve.metrics);

    let severity = match cvss_score {
        Some(score) => CveSeverity::from_score(score),
        None => CveSeverity::Unknown,
    };

    let references = cve
        .references
        .unwrap_or_default()
        .into_iter()
        .take(MAX_REFERENCES)
        .map(|r| CveReference {
            url: r.url,
            source: r.source,
        })
        .collect();

    CveRecord {
        cve_id: cve.id,
        description,
        cvss_score,
        cvss_vector,
        severity,
        references,
        published: cve.published,
        source: CveSource::Nvd,
    }
}

/// Extract CVSS score and vector from NVD metrics.
///
/// Prefers v3.1 over v3.0. NVD provides the score directly, no crate parsing needed.
fn extract_nvd_cvss(metrics: &Option<NvdMetrics>) -> (Option<f64>, Option<String>) {
    let metrics = match metrics {
        Some(m) => m,
        None => return (None, None),
    };

    // Prefer CVSS v3.1, fall back to v3.0
    let cvss_data = metrics
        .cvss_metric_v31
        .as_ref()
        .and_then(|v| v.first())
        .map(|m| &m.cvss_data)
        .or_else(|| {
            metrics
                .cvss_metric_v30
                .as_ref()
                .and_then(|v| v.first())
                .map(|m| &m.cvss_data)
        });

    match cvss_data {
        Some(data) => (Some(data.base_score), Some(data.vector_string.clone())),
        None => (None, None),
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

    fn make_nvd_cve_with_v31() -> NvdCve {
        NvdCve {
            id: "CVE-2021-41773".to_string(),
            descriptions: vec![
                NvdDescription {
                    lang: "en".to_string(),
                    value: "Path traversal in Apache HTTP Server 2.4.49".to_string(),
                },
                NvdDescription {
                    lang: "es".to_string(),
                    value: "Descripcion en espanol".to_string(),
                },
            ],
            metrics: Some(NvdMetrics {
                cvss_metric_v31: Some(vec![NvdCvssMetric {
                    cvss_data: NvdCvssData {
                        base_score: 9.8,
                        base_severity: "CRITICAL".to_string(),
                        vector_string: "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"
                            .to_string(),
                    },
                }]),
                cvss_metric_v30: None,
            }),
            references: Some(vec![
                NvdReference {
                    url: "https://httpd.apache.org/security/vulnerabilities_24.html".to_string(),
                    source: Some("Apache".to_string()),
                },
                NvdReference {
                    url: "https://nvd.nist.gov/vuln/detail/CVE-2021-41773".to_string(),
                    source: Some("NVD".to_string()),
                },
            ]),
            published: Some("2021-10-05T10:15:00.000".to_string()),
            last_modified: Some("2021-10-12T12:00:00.000".to_string()),
        }
    }

    fn make_nvd_cve_no_metrics() -> NvdCve {
        NvdCve {
            id: "CVE-2024-99999".to_string(),
            descriptions: vec![NvdDescription {
                lang: "en".to_string(),
                value: "A vulnerability without CVSS metrics".to_string(),
            }],
            metrics: None,
            references: None,
            published: None,
            last_modified: None,
        }
    }

    fn make_nvd_cve_v30_only() -> NvdCve {
        NvdCve {
            id: "CVE-2020-12345".to_string(),
            descriptions: vec![NvdDescription {
                lang: "en".to_string(),
                value: "A v3.0-only vulnerability".to_string(),
            }],
            metrics: Some(NvdMetrics {
                cvss_metric_v31: None,
                cvss_metric_v30: Some(vec![NvdCvssMetric {
                    cvss_data: NvdCvssData {
                        base_score: 7.5,
                        base_severity: "HIGH".to_string(),
                        vector_string: "CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N"
                            .to_string(),
                    },
                }]),
            }),
            references: None,
            published: Some("2020-06-15T00:00:00.000".to_string()),
            last_modified: None,
        }
    }

    #[test]
    fn test_nvd_cve_id_preserved() {
        let cve = make_nvd_cve_with_v31();
        let record = nvd_cve_to_record(cve);
        assert_eq!(record.cve_id, "CVE-2021-41773");
    }

    #[test]
    fn test_nvd_english_description_selected() {
        let cve = make_nvd_cve_with_v31();
        let record = nvd_cve_to_record(cve);
        assert!(record.description.contains("Path traversal"));
        assert!(!record.description.contains("espanol"));
    }

    #[test]
    fn test_nvd_cvss_v31_extracted() {
        let cve = make_nvd_cve_with_v31();
        let record = nvd_cve_to_record(cve);
        assert_eq!(record.cvss_score, Some(9.8));
        assert!(record.cvss_vector.as_ref().unwrap().starts_with("CVSS:3.1"));
        assert_eq!(record.severity, CveSeverity::Critical);
    }

    #[test]
    fn test_nvd_cvss_v30_fallback() {
        let cve = make_nvd_cve_v30_only();
        let record = nvd_cve_to_record(cve);
        assert_eq!(record.cvss_score, Some(7.5));
        assert!(record.cvss_vector.as_ref().unwrap().starts_with("CVSS:3.0"));
        assert_eq!(record.severity, CveSeverity::High);
    }

    #[test]
    fn test_nvd_no_metrics_unknown_severity() {
        let cve = make_nvd_cve_no_metrics();
        let record = nvd_cve_to_record(cve);
        assert!(record.cvss_score.is_none());
        assert!(record.cvss_vector.is_none());
        assert_eq!(record.severity, CveSeverity::Unknown);
    }

    #[test]
    fn test_nvd_source_is_nvd() {
        let cve = make_nvd_cve_with_v31();
        let record = nvd_cve_to_record(cve);
        assert_eq!(record.source, CveSource::Nvd);
    }

    #[test]
    fn test_nvd_references_limited_to_five() {
        let mut refs = Vec::new();
        for i in 0..10 {
            refs.push(NvdReference {
                url: format!("https://example.com/ref{}", i),
                source: None,
            });
        }
        let cve = NvdCve {
            id: "CVE-2024-00001".to_string(),
            descriptions: vec![NvdDescription {
                lang: "en".to_string(),
                value: "Many references".to_string(),
            }],
            metrics: None,
            references: Some(refs),
            published: None,
            last_modified: None,
        };
        let record = nvd_cve_to_record(cve);
        assert_eq!(
            record.references.len(),
            5,
            "References should be capped at 5"
        );
    }

    #[test]
    fn test_nvd_description_truncation() {
        let long_desc = "B".repeat(600);
        let cve = NvdCve {
            id: "CVE-2024-00002".to_string(),
            descriptions: vec![NvdDescription {
                lang: "en".to_string(),
                value: long_desc,
            }],
            metrics: None,
            references: None,
            published: None,
            last_modified: None,
        };
        let record = nvd_cve_to_record(cve);
        assert_eq!(record.description.len(), 503);
        assert!(record.description.ends_with("..."));
    }

    #[test]
    fn test_nvd_published_preserved() {
        let cve = make_nvd_cve_with_v31();
        let record = nvd_cve_to_record(cve);
        assert_eq!(
            record.published,
            Some("2021-10-05T10:15:00.000".to_string())
        );
    }

    #[cfg(feature = "live-tests")]
    #[tokio::test]
    async fn test_nvd_search_by_cpe_live() {
        let rate_limiter = crate::vuln::RateLimiter::new(false);
        let client = NvdClient::new(None, rate_limiter);
        let results = client
            .search_by_cpe("cpe:2.3:a:apache:http_server:2.4.49:*:*:*:*:*:*:*")
            .await;
        assert!(
            !results.is_empty(),
            "NVD should return CVEs for Apache 2.4.49"
        );
        assert!(results.iter().any(|r| r.cve_id == "CVE-2021-41773"));
    }

    #[cfg(feature = "live-tests")]
    #[tokio::test]
    async fn test_nvd_get_cve_live() {
        let rate_limiter = crate::vuln::RateLimiter::new(false);
        let client = NvdClient::new(None, rate_limiter);
        let result = client.get_cve("CVE-2021-41773").await;
        assert!(result.is_some(), "NVD should return CVE-2021-41773");
        let record = result.unwrap();
        assert_eq!(record.cve_id, "CVE-2021-41773");
        assert!(record.cvss_score.is_some());
    }
}
