use crate::models::{
    cve_scans::CveScan, cves::Cve, nix_packages::NixPackage,
    package_vulnerabilities::PackageVulnerability, scan_packages::ScanPackage,
};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
            true, // Assume runtime dependency
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
        serde_json::from_str(json_data).context("Failed to parse vulnix JSON output")
    }

    /// Convert vulnix derivations to Crystal Forge models for database insertion
    pub fn to_scan_result(
        derivations: Vec<VulnixDerivation>,
        system_state_id: i32,
        scanner_version: Option<String>,
    ) -> VulnixScanResult {
        let mut scan = CveScan::new(system_state_id, "vulnix".to_string());
        scan.scanner_version = scanner_version;

        let mut result = VulnixScanResult {
            scan,
            packages: Vec::new(),
            scan_packages: Vec::new(),
            cves: Vec::new(),
            package_vulnerabilities: Vec::new(),
        };

        let mut unique_cves: HashMap<String, Cve> = HashMap::new();
        let mut severity_counts = SeverityCounts::default();

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

            // Add CVEs (deduplicating)
            for cve in models.cves {
                let severity = cve.severity();
                match severity {
                    crate::models::cves::CveSeverity::Critical => severity_counts.critical += 1,
                    crate::models::cves::CveSeverity::High => severity_counts.high += 1,
                    crate::models::cves::CveSeverity::Medium => severity_counts.medium += 1,
                    crate::models::cves::CveSeverity::Low => severity_counts.low += 1,
                    crate::models::cves::CveSeverity::Unknown => {}
                }
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
            severity_counts.critical,
            severity_counts.high,
            severity_counts.medium,
            severity_counts.low,
        );

        // Add unique CVEs to result
        result.cves = unique_cves.into_values().collect();

        result
    }

    /// Parse and convert in one step
    pub fn parse_and_convert(
        json_data: &str,
        system_state_id: i32,
        scanner_version: Option<String>,
    ) -> Result<VulnixScanResult> {
        let derivations = Self::parse_json(json_data)?;
        Ok(Self::to_scan_result(
            derivations,
            system_state_id,
            scanner_version,
        ))
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

#[derive(Debug, Default)]
struct SeverityCounts {
    critical: i32,
    high: i32,
    medium: i32,
    low: i32,
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
}
