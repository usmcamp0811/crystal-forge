use crate::models::{
    cve_scans::CveScan, cves::Cve, nix_packages::NixPackage,
    package_vulnerabilities::PackageVulnerability, scan_packages::ScanPackage,
};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};
use uuid::Uuid;

/// Vulnix JSON output structure - array of these objects
#[derive(Debug, Deserialize, Serialize)]
pub struct VulnixDerivation {
    /// Package name and version (e.g., "openssl-1.1.1w")
    pub name: String,

    /// Package name without version (e.g., "openssl")
    pub pname: String,

    /// Version only (e.g., "1.1.1w")
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

impl VulnixDerivation {
    /// Convert this vulnix derivation to Crystal Forge models
    pub fn to_models(&self, scan_id: Uuid) -> VulnixDerivationModels {
        let mut models = VulnixDerivationModels::new();

        // Create NixPackage
        let package = NixPackage::new(
            self.derivation.clone(),
            self.name.clone(),
            self.pname.clone(),
            self.version.clone(),
        );
        models.package = Some(package);

        // Create ScanPackage relationship
        let scan_package = ScanPackage::new(
            scan_id,
            self.derivation.clone(),
            true, // Assume runtime dependency for vulnix scans
            0,    // Vulnix doesn't provide dependency depth
        );
        models.scan_package = Some(scan_package);

        // Create CVEs and PackageVulnerabilities
        for cve_id in &self.affected_by {
            // Create CVE (basic info - will be enriched later)
            let cvss_score = self.cvssv3_basescore.get(cve_id).copied();
            let cve = Cve {
                id: cve_id.clone(),
                cvss_v3_score: cvss_score,
                cvss_v2_score: None,
                description: None, // Vulnix doesn't provide descriptions
                published_date: None,
                modified_date: None,
                vector: None,
                cwe_id: None,
                metadata: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            models.cves.push(cve);

            // Create PackageVulnerability
            let is_whitelisted = self.whitelisted.contains(cve_id);
            let mut vulnerability = PackageVulnerability::new(
                self.derivation.clone(),
                cve_id.clone(),
                "vulnix".to_string(),
            );

            if is_whitelisted {
                vulnerability.whitelist("Whitelisted in vulnix config".to_string(), None);
            }

            models.package_vulnerabilities.push(vulnerability);
        }

        models
    }

    /// Get all unique CVE IDs from this derivation
    pub fn get_all_cve_ids(&self) -> Vec<String> {
        let mut all_cves = self.affected_by.clone();
        all_cves.extend(self.whitelisted.clone());
        all_cves.sort();
        all_cves.dedup();
        all_cves
    }

    /// Check if this derivation has any vulnerabilities
    pub fn has_vulnerabilities(&self) -> bool {
        !self.affected_by.is_empty()
    }

    /// Get the highest CVSS score for this derivation
    pub fn max_cvss_score(&self) -> Option<f32> {
        self.cvssv3_basescore
            .values()
            .copied()
            .fold(None, |acc, score| Some(acc.map_or(score, |a| a.max(score))))
    }

    /// Get vulnerability count by severity
    pub fn vulnerability_counts(&self) -> SeverityCounts {
        let mut counts = SeverityCounts::default();

        for cve_id in &self.affected_by {
            if let Some(score) = self.cvssv3_basescore.get(cve_id) {
                match *score {
                    s if s >= 9.0 => counts.critical += 1,
                    s if s >= 7.0 => counts.high += 1,
                    s if s >= 4.0 => counts.medium += 1,
                    s if s > 0.0 => counts.low += 1,
                    _ => counts.unknown += 1,
                }
            } else {
                counts.unknown += 1;
            }
        }

        counts
    }
}

/// Collection of Crystal Forge models created from a single VulnixDerivation
#[derive(Debug)]
pub struct VulnixDerivationModels {
    pub package: Option<NixPackage>,
    pub scan_package: Option<ScanPackage>,
    pub cves: Vec<Cve>,
    pub package_vulnerabilities: Vec<PackageVulnerability>,
}

impl VulnixDerivationModels {
    fn new() -> Self {
        Self {
            package: None,
            scan_package: None,
            cves: Vec::new(),
            package_vulnerabilities: Vec::new(),
        }
    }
}

/// Parser for vulnix JSON output
pub struct VulnixParser;

impl VulnixParser {
    /// Parse vulnix JSON output into array of VulnixDerivation structs
    pub fn parse_json(json_data: &str) -> Result<Vec<VulnixDerivation>> {
        if json_data.trim().is_empty() {
            debug!("Empty vulnix output - no packages scanned or no vulnerabilities found");
            return Ok(Vec::new());
        }

        serde_json::from_str(json_data).with_context(|| {
            format!(
                "Failed to parse vulnix JSON output. First 200 chars: {}",
                &json_data.chars().take(200).collect::<String>()
            )
        })
    }

    /// Convert vulnix derivations to Crystal Forge models for database insertion
    pub fn to_scan_result(
        derivations: Vec<VulnixDerivation>,
        evaluation_target_id: i64,
        scanner_version: Option<String>,
    ) -> VulnixScanResult {
        let mut scan = CveScan::new(evaluation_target_id, "vulnix".to_string());
        scan.scanner_version = scanner_version;

        let mut result = VulnixScanResult {
            scan,
            packages: Vec::new(),
            scan_packages: Vec::new(),
            cves: Vec::new(),
            package_vulnerabilities: Vec::new(),
        };

        let mut unique_cves: HashMap<String, Cve> = HashMap::new();
        let mut total_severity_counts = SeverityCounts::default();

        for derivation in derivations {
            let models = derivation.to_models(result.scan.id);

            // Add package
            if let Some(package) = models.package {
                result.packages.push(package);
            }

            // Add scan package relationship
            if let Some(scan_package) = models.scan_package {
                result.scan_packages.push(scan_package);
            }

            // Aggregate severity counts for this derivation
            let deriv_counts = derivation.vulnerability_counts();
            total_severity_counts.critical += deriv_counts.critical;
            total_severity_counts.high += deriv_counts.high;
            total_severity_counts.medium += deriv_counts.medium;
            total_severity_counts.low += deriv_counts.low;
            total_severity_counts.unknown += deriv_counts.unknown;

            // Add CVEs (deduplicating)
            for cve in models.cves {
                unique_cves.insert(cve.id.clone(), cve);
            }

            // Add package vulnerabilities
            result
                .package_vulnerabilities
                .extend(models.package_vulnerabilities);
        }

        // Update scan counts
        result.scan.total_packages = result.packages.len() as i32;
        result.scan.update_counts(
            total_severity_counts.critical,
            total_severity_counts.high,
            total_severity_counts.medium,
            total_severity_counts.low,
        );

        // Add unique CVEs to result
        result.cves = unique_cves.into_values().collect();

        if total_severity_counts.unknown > 0 {
            warn!(
                "Found {} CVEs without CVSS scores",
                total_severity_counts.unknown
            );
        }

        result
    }

    /// Parse and convert in one step
    pub fn parse_and_convert(
        json_data: &str,
        evaluation_target_id: i64,
        scanner_version: Option<String>,
    ) -> Result<VulnixScanResult> {
        let derivations = Self::parse_json(json_data)?;
        Ok(Self::to_scan_result(
            derivations,
            evaluation_target_id,
            scanner_version,
        ))
    }

    /// Validate vulnix JSON output structure before parsing
    pub fn validate_json_structure(json_data: &str) -> Result<()> {
        if json_data.trim().is_empty() {
            return Ok(()); // Empty output is valid (no vulnerabilities)
        }

        // Basic JSON validation
        let value: serde_json::Value =
            serde_json::from_str(json_data).context("Invalid JSON format")?;

        // Check if it's an array
        if !value.is_array() {
            anyhow::bail!(
                "Expected JSON array, got {}",
                value
                    .as_object()
                    .map_or("non-object".to_string(), |_| "object".to_string())
            );
        }

        let array = value.as_array().unwrap();

        // Validate structure of first few items
        for (idx, item) in array.iter().take(3).enumerate() {
            let obj = item
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Item {} is not an object", idx))?;

            // Check required fields
            let required_fields = [
                "name",
                "pname",
                "version",
                "affected_by",
                "whitelisted",
                "derivation",
                "cvssv3_basescore",
            ];
            for field in &required_fields {
                if !obj.contains_key(*field) {
                    anyhow::bail!("Missing required field '{}' in item {}", field, idx);
                }
            }
        }

        Ok(())
    }
}

/// Result containing all models for database insertion
#[derive(Debug)]
pub struct VulnixScanResult {
    pub scan: CveScan,
    pub packages: Vec<NixPackage>,
    pub scan_packages: Vec<ScanPackage>,
    pub cves: Vec<Cve>,
    pub package_vulnerabilities: Vec<PackageVulnerability>,
}

impl VulnixScanResult {
    /// Get summary statistics
    pub fn summary(&self) -> ScanSummary {
        ScanSummary {
            total_packages: self.packages.len(),
            total_vulnerabilities: self.package_vulnerabilities.len(),
            total_cves: self.cves.len(),
            critical_count: self.scan.critical_count as usize,
            high_count: self.scan.high_count as usize,
            medium_count: self.scan.medium_count as usize,
            low_count: self.scan.low_count as usize,
            risk_level: self.scan.risk_level(),
        }
    }

    /// Get packages with critical vulnerabilities
    pub fn critical_packages(&self) -> Vec<&NixPackage> {
        let critical_derivations: std::collections::HashSet<&String> = self
            .package_vulnerabilities
            .iter()
            .filter(|vuln| {
                self.cves
                    .iter()
                    .find(|cve| cve.id == vuln.cve_id)
                    .map_or(false, |cve| cve.is_critical())
            })
            .map(|vuln| &vuln.derivation_path)
            .collect();

        self.packages
            .iter()
            .filter(|pkg| critical_derivations.contains(&pkg.derivation_path))
            .collect()
    }

    /// Check if scan has any actionable findings
    pub fn has_actionable_findings(&self) -> bool {
        self.scan.critical_count > 0 || self.scan.high_count > 0
    }
}

#[derive(Debug)]
pub struct ScanSummary {
    pub total_packages: usize,
    pub total_vulnerabilities: usize,
    pub total_cves: usize,
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub low_count: usize,
    pub risk_level: crate::models::cve_scans::SecurityRiskLevel,
}

impl std::fmt::Display for ScanSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} packages scanned, {} CVEs found ({} critical, {} high, {} medium, {} low) - Risk: {}",
            self.total_packages,
            self.total_cves,
            self.critical_count,
            self.high_count,
            self.medium_count,
            self.low_count,
            self.risk_level
        )
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

        let derivations = VulnixParser::parse_json(json_data).unwrap();

        assert_eq!(derivations.len(), 2);

        let openssl = &derivations[0];
        assert_eq!(openssl.name, "openssl-1.1.1w");
        assert_eq!(openssl.pname, "openssl");
        assert_eq!(openssl.version, "1.1.1w");
        assert_eq!(openssl.affected_by.len(), 2);
        assert_eq!(openssl.whitelisted.len(), 1);
        assert!(openssl.has_vulnerabilities());
        assert_eq!(openssl.max_cvss_score(), Some(7.5));

        let curl = &derivations[1];
        assert_eq!(curl.name, "curl-8.0.1");
        assert!(!curl.has_vulnerabilities());
        assert_eq!(curl.max_cvss_score(), None);
    }

    #[test]
    fn test_validate_json_structure() {
        let valid_json = r#"[{"name": "test", "pname": "test", "version": "1.0", "affected_by": [], "whitelisted": [], "derivation": "/nix/store/test", "cvssv3_basescore": {}}]"#;
        assert!(VulnixParser::validate_json_structure(valid_json).is_ok());

        let invalid_json = r#"{"not": "array"}"#;
        assert!(VulnixParser::validate_json_structure(invalid_json).is_err());

        let empty_json = "";
        assert!(VulnixParser::validate_json_structure(empty_json).is_ok());
    }

    #[test]
    fn test_to_models() {
        let derivation = VulnixDerivation {
            name: "test-package-1.0.0".to_string(),
            pname: "test-package".to_string(),
            version: "1.0.0".to_string(),
            affected_by: vec!["CVE-2023-1234".to_string()],
            whitelisted: vec![],
            derivation: "/nix/store/test123-test-package-1.0.0.drv".to_string(),
            cvssv3_basescore: [("CVE-2023-1234".to_string(), 8.5)].into_iter().collect(),
        };

        let scan_id = Uuid::new_v4();
        let models = derivation.to_models(scan_id);

        assert!(models.package.is_some());
        assert!(models.scan_package.is_some());
        assert_eq!(models.cves.len(), 1);
        assert_eq!(models.package_vulnerabilities.len(), 1);

        let cve = &models.cves[0];
        assert_eq!(cve.id, "CVE-2023-1234");
        assert_eq!(cve.cvss_v3_score, Some(8.5));

        let vuln = &models.package_vulnerabilities[0];
        assert_eq!(vuln.cve_id, "CVE-2023-1234");
        assert!(!vuln.is_whitelisted);
    }

    #[test]
    fn test_parse_and_convert() {
        let json_data = r#"[
            {
                "name": "test-1.0",
                "pname": "test",
                "version": "1.0",
                "affected_by": ["CVE-2023-1234"],
                "whitelisted": [],
                "derivation": "/nix/store/test123-test-1.0.drv",
                "cvssv3_basescore": {"CVE-2023-1234": 7.5}
            }
        ]"#;

        let result =
            VulnixParser::parse_and_convert(json_data, 1, Some("1.10.1".to_string())).unwrap();

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.cves.len(), 1);
        assert_eq!(result.package_vulnerabilities.len(), 1);
        assert_eq!(result.scan.scanner_version, Some("1.10.1".to_string()));

        let summary = result.summary();
        assert_eq!(summary.total_packages, 1);
        assert_eq!(summary.total_cves, 1);
        assert_eq!(summary.high_count, 1);
    }

    #[test]
    fn test_vulnerability_counts() {
        let derivation = VulnixDerivation {
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

        let counts = derivation.vulnerability_counts();
        assert_eq!(counts.critical, 1);
        assert_eq!(counts.high, 0);
        assert_eq!(counts.medium, 1);
        assert_eq!(counts.low, 0);
        assert_eq!(counts.unknown, 0);
    }
}
