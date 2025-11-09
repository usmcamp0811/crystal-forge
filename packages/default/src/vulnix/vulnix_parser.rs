use crate::models::derivations::utils::get_store_path_from_drv;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub cvssv3_basescore: HashMap<String, f32>,
}

impl VulnixEntry {
    /// Get all CVE IDs (affected + whitelisted)
    pub fn all_cve_ids(&self) -> Vec<String> {
        let mut all_cves = self.affected_by.clone();
        all_cves.extend(self.whitelisted.clone());
        all_cves.sort();
        all_cves.dedup();
        all_cves
    }

    /// Check if this entry has any vulnerabilities
    pub fn has_vulnerabilities(&self) -> bool {
        !self.affected_by.is_empty()
    }

    /// Get the highest CVSS score for this entry
    pub fn max_cvss_score(&self) -> Option<f32> {
        self.cvssv3_basescore
            .values()
            .copied()
            .fold(None, |acc, score| Some(acc.map_or(score, |a| a.max(score))))
    }
    /// Count vulnerabilities by severity for this entry
    pub fn severity_counts(&self) -> SeverityCounts {
        let mut counts = SeverityCounts::default();

        for cve_id in &self.affected_by {
            match self.cvssv3_basescore.get(cve_id) {
                Some(score) => match *score {
                    s if s >= 9.0 => counts.critical += 1,
                    s if s >= 7.0 => counts.high += 1,
                    s if s >= 4.0 => counts.medium += 1,
                    s if s > 0.0 => counts.low += 1,
                    _ => counts.unknown += 1,
                },
                None => counts.unknown += 1,
            }
        }

        counts
    }
}

/// Array of VulnixEntry - this is what vulnix outputs as JSON
pub type VulnixScanOutput = Vec<VulnixEntry>;

/// Parser for vulnix JSON output
pub struct VulnixParser;

impl VulnixParser {
    /// Parse vulnix JSON output into array of VulnixEntry structs
    pub fn parse_json(json_data: &str) -> Result<VulnixScanOutput> {
        if json_data.trim().is_empty() {
            return Ok(Vec::new());
        }

        serde_json::from_str(json_data).with_context(|| {
            format!(
                "Failed to parse vulnix JSON. First 200 chars: {}",
                &json_data.chars().take(200).collect::<String>()
            )
        })
    }

    /// Calculate aggregate statistics from vulnix entries
    pub fn calculate_stats(entries: &VulnixScanOutput) -> ScanStats {
        let mut total_counts = SeverityCounts::default();
        let mut unique_cves = std::collections::HashSet::new();

        for entry in entries {
            let entry_counts = entry.severity_counts();
            total_counts.critical += entry_counts.critical;
            total_counts.high += entry_counts.high;
            total_counts.medium += entry_counts.medium;
            total_counts.low += entry_counts.low;
            total_counts.unknown += entry_counts.unknown;

            for cve_id in entry.all_cve_ids() {
                unique_cves.insert(cve_id);
            }
        }

        ScanStats {
            total_packages: entries.len(),
            total_cves: unique_cves.len(),
            total_vulnerabilities: (total_counts.critical
                + total_counts.high
                + total_counts.medium
                + total_counts.low) as usize,
            critical_count: total_counts.critical as usize,
            high_count: total_counts.high as usize,
            medium_count: total_counts.medium as usize,
            low_count: total_counts.low as usize,
            unknown_count: total_counts.unknown as usize,
        }
    }
}

#[derive(Debug, Default)]
pub struct SeverityCounts {
    pub critical: i32,
    pub high: i32,
    pub medium: i32,
    pub low: i32,
    pub unknown: i32,
}

#[derive(Debug)]
pub struct ScanStats {
    pub total_packages: usize,
    pub total_cves: usize,
    pub total_vulnerabilities: usize,
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub low_count: usize,
    pub unknown_count: usize,
}

impl std::fmt::Display for ScanStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} packages, {} CVEs (C:{} H:{} M:{} L:{})",
            self.total_packages,
            self.total_cves,
            self.critical_count,
            self.high_count,
            self.medium_count,
            self.low_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vulnix_json() {
        let json_data = r#"[
            {
                "name": "openssl-1.1.1w",
                "pname": "openssl",
                "version": "1.1.1w",
                "affected_by": ["CVE-2023-1234", "CVE-2023-5678"],
                "whitelisted": ["CVE-2023-5678"],
                "derivation": "/nix/store/abc123-openssl-1.1.1w.drv",
                "cvssv3_basescore": {
                    "CVE-2023-1234": 7.5,
                    "CVE-2023-5678": 5.3
                }
            },
            {
                "name": "curl-8.0.1",
                "pname": "curl",
                "version": "8.0.1",
                "affected_by": [],
                "whitelisted": [],
                "derivation": "/nix/store/def456-curl-8.0.1.drv",
                "cvssv3_basescore": {}
            }
        ]"#;

        let entries = VulnixParser::parse_json(json_data).unwrap();

        assert_eq!(entries.len(), 2);

        let openssl = &entries[0];
        assert_eq!(openssl.name, "openssl-1.1.1w");
        assert_eq!(openssl.pname, "openssl");
        assert_eq!(openssl.version, "1.1.1w");
        assert_eq!(openssl.affected_by.len(), 2);
        assert_eq!(openssl.whitelisted.len(), 1);
        assert!(openssl.has_vulnerabilities());
        assert_eq!(openssl.max_cvss_score(), Some(7.5));

        let curl = &entries[1];
        assert_eq!(curl.name, "curl-8.0.1");
        assert!(!curl.has_vulnerabilities());
        assert_eq!(curl.max_cvss_score(), None);
    }

    #[test]
    fn test_empty_json() {
        let entries = VulnixParser::parse_json("").unwrap();
        assert_eq!(entries.len(), 0);

        let entries = VulnixParser::parse_json("   ").unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_severity_counts() {
        let entry = VulnixEntry {
            name: "test-1.0".to_string(),
            pname: "test".to_string(),
            version: "1.0".to_string(),
            affected_by: vec!["CVE-2023-1234".to_string(), "CVE-2023-5678".to_string()],
            whitelisted: vec![],
            derivation: "/nix/store/test".to_string(),
            cvssv3_basescore: [
                ("CVE-2023-1234".to_string(), 9.5), // Critical
                ("CVE-2023-5678".to_string(), 6.0), // Medium
            ]
            .into_iter()
            .collect(),
        };

        let counts = entry.severity_counts();
        assert_eq!(counts.critical, 1);
        assert_eq!(counts.high, 0);
        assert_eq!(counts.medium, 1);
        assert_eq!(counts.low, 0);
        assert_eq!(counts.unknown, 0);
    }

    #[test]
    fn test_calculate_stats() {
        let entries = vec![
            VulnixEntry {
                name: "pkg1-1.0".to_string(),
                pname: "pkg1".to_string(),
                version: "1.0".to_string(),
                affected_by: vec!["CVE-2023-1234".to_string()],
                whitelisted: vec![],
                derivation: "/nix/store/pkg1".to_string(),
                cvssv3_basescore: [("CVE-2023-1234".to_string(), 9.5)].into_iter().collect(),
            },
            VulnixEntry {
                name: "pkg2-2.0".to_string(),
                pname: "pkg2".to_string(),
                version: "2.0".to_string(),
                affected_by: vec!["CVE-2023-1234".to_string(), "CVE-2023-5678".to_string()],
                whitelisted: vec![],
                derivation: "/nix/store/pkg2".to_string(),
                cvssv3_basescore: [
                    ("CVE-2023-1234".to_string(), 9.5),
                    ("CVE-2023-5678".to_string(), 7.5),
                ]
                .into_iter()
                .collect(),
            },
        ];

        let stats = VulnixParser::calculate_stats(&entries);

        assert_eq!(stats.total_packages, 2);
        assert_eq!(stats.total_cves, 2); // CVE-2023-1234 and CVE-2023-5678
        assert_eq!(stats.total_vulnerabilities, 3); // 1 + 2 vulnerabilities
        assert_eq!(stats.critical_count, 2); // Both CVE-2023-1234 instances
        assert_eq!(stats.high_count, 1); // CVE-2023-5678
    }

    #[test]
    fn test_all_cve_ids() {
        let entry = VulnixEntry {
            name: "test-1.0".to_string(),
            pname: "test".to_string(),
            version: "1.0".to_string(),
            affected_by: vec!["CVE-2023-1234".to_string(), "CVE-2023-5678".to_string()],
            whitelisted: vec!["CVE-2023-5678".to_string(), "CVE-2023-9999".to_string()],
            derivation: "/nix/store/test".to_string(),
            cvssv3_basescore: HashMap::new(),
        };

        let all_cves = entry.all_cve_ids();
        assert_eq!(all_cves.len(), 3);
        assert!(all_cves.contains(&"CVE-2023-1234".to_string()));
        assert!(all_cves.contains(&"CVE-2023-5678".to_string()));
        assert!(all_cves.contains(&"CVE-2023-9999".to_string()));
    }
}
