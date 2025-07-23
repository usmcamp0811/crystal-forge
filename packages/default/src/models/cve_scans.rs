use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct CveScan {
    pub id: Uuid,
    pub evaluation_target_id: i32,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: ScanStatus,
    pub attempts: i32,
    pub scanner_name: String,
    pub scanner_version: Option<String>,
    pub total_packages: i32,
    pub total_vulnerabilities: i32,
    pub critical_count: i32,
    pub high_count: i32,
    pub medium_count: i32,
    pub low_count: i32,
    pub scan_duration_ms: Option<i32>,
    pub scan_metadata: Option<serde_json::Value>,
    pub created_at: Option<DateTime<Utc>>, // let Postgres default it
}

impl CveScan {
    /// Create a new CVE scan record
    pub fn new(evaluation_target_id: i32, scanner_name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            evaluation_target_id,
            scheduled_at: Some(now),
            completed_at: None,
            status: ScanStatus::Pending,
            attempts: 0,
            scanner_name,
            scanner_version: None,
            total_packages: 0,
            total_vulnerabilities: 0,
            critical_count: 0,
            high_count: 0,
            medium_count: 0,
            low_count: 0,
            scan_duration_ms: None,
            scan_metadata: None,
            created_at: Some(now),
        }
    }

    /// Start timing the scan
    pub fn start_timing(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    /// Finish timing and set duration
    pub fn finish_timing(&mut self, start_time: std::time::Instant) {
        self.scan_duration_ms = Some(start_time.elapsed().as_millis() as i32);
    }

    /// Update vulnerability counts
    pub fn update_counts(&mut self, critical: i32, high: i32, medium: i32, low: i32) {
        self.critical_count = critical;
        self.high_count = high;
        self.medium_count = medium;
        self.low_count = low;
        self.total_vulnerabilities = critical + high + medium + low;
    }

    /// Get security risk level based on findings
    pub fn risk_level(&self) -> SecurityRiskLevel {
        if self.critical_count > 0 {
            SecurityRiskLevel::Critical
        } else if self.high_count > 0 {
            SecurityRiskLevel::High
        } else if self.medium_count > 0 {
            SecurityRiskLevel::Medium
        } else if self.low_count > 0 {
            SecurityRiskLevel::Low
        } else {
            SecurityRiskLevel::Clean
        }
    }

    /// Check if scan found any vulnerabilities
    pub fn has_vulnerabilities(&self) -> bool {
        self.total_vulnerabilities > 0
    }

    /// Get scan efficiency (packages per second)
    pub fn packages_per_second(&self) -> Option<f64> {
        self.scan_duration_ms.map(|duration_ms| {
            if duration_ms > 0 {
                (self.total_packages as f64) / (duration_ms as f64 / 1000.0)
            } else {
                0.0
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityRiskLevel {
    Critical,
    High,
    Medium,
    Low,
    Clean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar")]
pub enum ScanStatus {
    #[sqlx(rename = "pending")]
    Pending,
    #[sqlx(rename = "in_progress")]
    InProgress,
    #[sqlx(rename = "completed")]
    Completed,
    #[sqlx(rename = "failed")]
    Failed,
}

impl std::fmt::Display for SecurityRiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityRiskLevel::Critical => write!(f, "Critical"),
            SecurityRiskLevel::High => write!(f, "High Risk"),
            SecurityRiskLevel::Medium => write!(f, "Medium Risk"),
            SecurityRiskLevel::Low => write!(f, "Low Risk"),
            SecurityRiskLevel::Clean => write!(f, "Clean"),
        }
    }
}

impl std::fmt::Display for CveScan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scan_time = self
            .scan_date
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "unknown time".to_string());

        write!(
            f,
            "Scan {} ({}) - {} packages, {} vulnerabilities ({})",
            self.scanner_name,
            scan_time,
            self.total_packages,
            self.total_vulnerabilities,
            self.risk_level()
        )
    }
}
