use serde::{Deserialize, Serialize};

use super::searchsploit::ExploitEntry;

/// Unified CVE record consumed by all downstream code (enrichment, scoring, reporting).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveRecord {
    /// CVE identifier (e.g., "CVE-2021-44228")
    pub cve_id: String,
    /// Description truncated to ~500 chars for LLM context
    pub description: String,
    /// CVSS v3+ base score, None if unavailable
    pub cvss_score: Option<f64>,
    /// CVSS vector string (e.g., "CVSS:3.1/AV:N/AC:L/...")
    pub cvss_vector: Option<String>,
    /// Severity derived from CVSS score
    pub severity: CveSeverity,
    /// Advisory/patch references
    pub references: Vec<CveReference>,
    /// Publication date as ISO 8601 string
    pub published: Option<String>,
    /// Which API provided this record
    pub source: CveSource,
    /// Whether a public exploit is known to exist (via searchsploit)
    #[serde(default)]
    pub has_public_exploit: bool,
    /// Public exploit entries from searchsploit/Exploit-DB
    #[serde(default)]
    pub exploits: Vec<ExploitEntry>,
}

/// CVSS v3.1 severity thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CveSeverity {
    /// 9.0 - 10.0
    Critical,
    /// 7.0 - 8.9
    High,
    /// 4.0 - 6.9
    Medium,
    /// 0.1 - 3.9
    Low,
    /// No CVSS data available
    Unknown,
}

impl CveSeverity {
    /// Classify a CVSS v3.1 base score into a severity level.
    pub fn from_score(score: f64) -> Self {
        if score >= 9.0 {
            CveSeverity::Critical
        } else if score >= 7.0 {
            CveSeverity::High
        } else if score >= 4.0 {
            CveSeverity::Medium
        } else if score > 0.0 {
            CveSeverity::Low
        } else {
            CveSeverity::Unknown
        }
    }

    /// Return uppercase string for SQLite storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            CveSeverity::Critical => "CRITICAL",
            CveSeverity::High => "HIGH",
            CveSeverity::Medium => "MEDIUM",
            CveSeverity::Low => "LOW",
            CveSeverity::Unknown => "UNKNOWN",
        }
    }
}

/// A reference URL attached to a CVE record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveReference {
    pub url: String,
    pub source: Option<String>,
}

/// Which upstream API provided the CVE data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CveSource {
    Osv,
    Nvd,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_from_score_critical() {
        assert_eq!(CveSeverity::from_score(9.5), CveSeverity::Critical);
        assert_eq!(CveSeverity::from_score(10.0), CveSeverity::Critical);
        assert_eq!(CveSeverity::from_score(9.0), CveSeverity::Critical);
    }

    #[test]
    fn test_severity_from_score_high() {
        assert_eq!(CveSeverity::from_score(7.0), CveSeverity::High);
        assert_eq!(CveSeverity::from_score(8.9), CveSeverity::High);
    }

    #[test]
    fn test_severity_from_score_medium() {
        assert_eq!(CveSeverity::from_score(4.0), CveSeverity::Medium);
        assert_eq!(CveSeverity::from_score(6.9), CveSeverity::Medium);
    }

    #[test]
    fn test_severity_from_score_low() {
        assert_eq!(CveSeverity::from_score(2.0), CveSeverity::Low);
        assert_eq!(CveSeverity::from_score(0.1), CveSeverity::Low);
        assert_eq!(CveSeverity::from_score(3.9), CveSeverity::Low);
    }

    #[test]
    fn test_severity_from_score_unknown() {
        assert_eq!(CveSeverity::from_score(0.0), CveSeverity::Unknown);
    }

    #[test]
    fn test_severity_as_str() {
        assert_eq!(CveSeverity::Critical.as_str(), "CRITICAL");
        assert_eq!(CveSeverity::High.as_str(), "HIGH");
        assert_eq!(CveSeverity::Medium.as_str(), "MEDIUM");
        assert_eq!(CveSeverity::Low.as_str(), "LOW");
        assert_eq!(CveSeverity::Unknown.as_str(), "UNKNOWN");
    }

    #[test]
    fn test_cve_record_serde_roundtrip() {
        let record = CveRecord {
            cve_id: "CVE-2021-44228".to_string(),
            description: "Log4j RCE vulnerability".to_string(),
            cvss_score: Some(10.0),
            cvss_vector: Some("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H".to_string()),
            severity: CveSeverity::Critical,
            references: vec![CveReference {
                url: "https://nvd.nist.gov/vuln/detail/CVE-2021-44228".to_string(),
                source: Some("NVD".to_string()),
            }],
            published: Some("2021-12-10".to_string()),
            source: CveSource::Nvd,
            has_public_exploit: false,
            exploits: vec![],
        };

        let json = serde_json::to_string(&record).expect("serialize");
        let deserialized: CveRecord = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.cve_id, "CVE-2021-44228");
        assert_eq!(deserialized.cvss_score, Some(10.0));
        assert_eq!(deserialized.severity, CveSeverity::Critical);
        assert_eq!(deserialized.source, CveSource::Nvd);
        assert_eq!(deserialized.references.len(), 1);
    }

    #[test]
    fn test_cvss_crate_parses_known_vector() {
        use std::str::FromStr;
        let vector = cvss::v3::Base::from_str("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H")
            .expect("parse CVSS vector");
        let score = vector.score().value();
        assert!((score - 10.0).abs() < 0.01, "Expected 10.0, got {}", score);
    }
}
