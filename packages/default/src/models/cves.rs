use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Cve {
    pub id: String, // CVE-YYYY-NNNNN
    pub cvss_v3_score: Option<f32>,
    pub cvss_v2_score: Option<f32>,
    pub description: Option<String>,
    pub published_date: Option<chrono::NaiveDate>,
    pub modified_date: Option<chrono::NaiveDate>,
    pub vector: Option<String>,              // CVSS vector string
    pub cwe_id: Option<String>,              // Common Weakness Enumeration
    pub metadata: Option<serde_json::Value>, // Additional CVE details
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Cve {
    /// Calculate severity from CVSS v3 score
    pub fn severity(&self) -> CveSeverity {
        match self.cvss_v3_score {
            Some(score) if score >= 9.0 => CveSeverity::Critical,
            Some(score) if score >= 7.0 => CveSeverity::High,
            Some(score) if score >= 4.0 => CveSeverity::Medium,
            Some(score) if score > 0.0 => CveSeverity::Low,
            _ => CveSeverity::Unknown,
        }
    }

    /// Check if this CVE is considered critical
    pub fn is_critical(&self) -> bool {
        matches!(self.severity(), CveSeverity::Critical)
    }

    /// Check if this CVE is high severity or above
    pub fn is_high_or_critical(&self) -> bool {
        matches!(self.severity(), CveSeverity::Critical | CveSeverity::High)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CveSeverity {
    Critical,
    High,
    Medium,
    Low,
    Unknown,
}

impl fmt::Display for CveSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CveSeverity::Critical => write!(f, "CRITICAL"),
            CveSeverity::High => write!(f, "HIGH"),
            CveSeverity::Medium => write!(f, "MEDIUM"),
            CveSeverity::Low => write!(f, "LOW"),
            CveSeverity::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

impl fmt::Display for Cve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}) - CVSS: {:.1}",
            self.id,
            self.severity(),
            self.cvss_v3_score.unwrap_or(0.0)
        )
    }
}
