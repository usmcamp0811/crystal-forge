use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Single entry from vulnix JSON output - represents one affected derivation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VulnixEntry {
    /// Package name and version
    pub name: String,
    /// Package name without version
    pub pname: String,
    /// Version only
    pub version: String,
    /// List of applicable CVE identifiers
    pub affected_by: Vec<String>,
    /// List of CVE identifiers which are masked by whitelist entries
    pub whitelisted: Vec<String>,
    /// Pathname of the scanned derivation file
    pub derivation: String,
    /// Dict of CVSS v3 impact base scores for each CVE found
    pub cvssv3_basescore: HashMap<String, f64>,
}

impl VulnixEntry {
    /// Get all CVEs (affected + whitelisted)
    pub fn all_cves(&self) -> Vec<&String> {
        let mut cves = Vec::new();
        cves.extend(&self.affected_by);
        cves.extend(&self.whitelisted);
        cves
    }

    /// Get the highest CVSS score for this package
    pub fn max_cvss_score(&self) -> Option<f64> {
        self.cvssv3_basescore
            .values()
            .copied()
            .fold(None, |acc, score| {
                Some(acc.map_or(score, |current_max| score.max(current_max)))
            })
    }

    /// Determine severity based on highest CVSS score
    pub fn severity(&self) -> CveSeverity {
        match self.max_cvss_score() {
            Some(score) if score >= 9.0 => CveSeverity::Critical,
            Some(score) if score >= 7.0 => CveSeverity::High,
            Some(score) if score >= 4.0 => CveSeverity::Medium,
            Some(score) if score > 0.0 => CveSeverity::Low,
            _ => CveSeverity::Unknown,
        }
    }
}

/// Severity levels based on CVSS scores
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CveSeverity {
    Critical, // 9.0-10.0
    High,     // 7.0-8.9
    Medium,   // 4.0-6.9
    Low,      // 0.1-3.9
    Unknown,  // No score or 0.0
}

/// Complete vulnix scan result - array of VulnixEntry
pub type VulnixScanOutput = Vec<VulnixEntry>;

/// Simplified scan result for database storage
#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub scan_id: Uuid,
    pub evaluation_target_id: i32,
    pub scanner_name: String,
    pub scanner_version: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub scan_duration_ms: Option<i32>,
    pub metadata: Option<serde_json::Value>,
    pub entries: Vec<VulnixEntry>,
}

impl ScanResult {
    pub fn new(
        evaluation_target_id: i32,
        scanner_name: String,
        scanner_version: Option<String>,
    ) -> Self {
        Self {
            scan_id: Uuid::new_v4(),
            evaluation_target_id,
            scanner_name,
            scanner_version,
            started_at: Utc::now(),
            completed_at: None,
            scan_duration_ms: None,
            metadata: None,
            entries: Vec::new(),
        }
    }

    /// Create from vulnix JSON output
    pub fn from_vulnix_output(
        evaluation_target_id: i32,
        scanner_name: String,
        scanner_version: Option<String>,
        vulnix_output: VulnixScanOutput,
    ) -> Self {
        Self {
            scan_id: Uuid::new_v4(),
            evaluation_target_id,
            scanner_name,
            scanner_version,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            scan_duration_ms: None,
            metadata: None,
            entries: vulnix_output,
        }
    }

    pub fn complete(&mut self) {
        self.completed_at = Some(Utc::now());
    }

    pub fn set_duration(&mut self, start_time: std::time::Instant) {
        self.scan_duration_ms = Some(start_time.elapsed().as_millis() as i32);
    }

    /// Calculate summary statistics
    pub fn summary(&self) -> ScanSummary {
        let mut summary = ScanSummary::default();

        // Count unique packages (by pname)
        let unique_packages: std::collections::HashSet<&String> =
            self.entries.iter().map(|e| &e.pname).collect();
        summary.total_packages = unique_packages.len() as i32;

        // Count vulnerabilities by severity
        for entry in &self.entries {
            for _cve in &entry.affected_by {
                match entry.severity() {
                    CveSeverity::Critical => summary.critical_count += 1,
                    CveSeverity::High => summary.high_count += 1,
                    CveSeverity::Medium => summary.medium_count += 1,
                    CveSeverity::Low => summary.low_count += 1,
                    CveSeverity::Unknown => summary.low_count += 1, // Count unknown as low
                }
            }
        }

        summary.total_vulnerabilities =
            summary.critical_count + summary.high_count + summary.medium_count + summary.low_count;
        summary.scan_duration_ms = self.scan_duration_ms;

        summary
    }

    /// Get all unique CVE IDs from the scan
    pub fn all_cve_ids(&self) -> Vec<String> {
        let mut cve_ids = std::collections::HashSet::new();
        for entry in &self.entries {
            for cve in entry.all_cves() {
                cve_ids.insert(cve.clone());
            }
        }
        cve_ids.into_iter().collect()
    }

    /// Filter entries by severity
    pub fn entries_by_severity(&self, severity: CveSeverity) -> Vec<&VulnixEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.severity() == severity)
            .collect()
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ScanSummary {
    pub total_packages: i32,
    pub total_vulnerabilities: i32,
    pub critical_count: i32,
    pub high_count: i32,
    pub medium_count: i32,
    pub low_count: i32,
    pub scan_duration_ms: Option<i32>,
}

impl std::fmt::Display for ScanSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} packages, {} vulnerabilities (C:{} H:{} M:{} L:{})",
            self.total_packages,
            self.total_vulnerabilities,
            self.critical_count,
            self.high_count,
            self.medium_count,
            self.low_count
        )
    }
}
