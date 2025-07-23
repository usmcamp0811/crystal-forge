use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// use crate::models::{
//     cves::Cve, nix_packages::NixPackage, package_vulnerabilities::PackageVulnerability,
// };
//
// /// Database integration result - contains the data ready for database insertion
// /// This is different from vulnix_parser::VulnixScanResult which contains parsed models
// #[derive(Debug, Serialize, Deserialize)]
// pub struct DatabaseScanResult {
//     pub scan_id: Uuid,
//     pub evaluation_target_id: i32,
//     pub scanner_name: String,
//     pub scanner_version: Option<String>,
//     pub started_at: DateTime<Utc>,
//     pub completed_at: Option<DateTime<Utc>>,
//     pub total_packages: i32,
//     pub total_vulnerabilities: i32,
//     pub critical_count: i32,
//     pub high_count: i32,
//     pub medium_count: i32,
//     pub low_count: i32,
//     pub scan_duration_ms: Option<i32>,
//     pub metadata: Option<serde_json::Value>,
//     pub packages: Vec<NixPackage>,
//     pub cves: Vec<Cve>,
//     pub vulnerabilities: Vec<PackageVulnerability>,
// }
//
// impl DatabaseScanResult {
//     pub fn new(
//         evaluation_target_id: i32,
//         scanner_name: String,
//         scanner_version: Option<String>,
//     ) -> Self {
//         Self {
//             scan_id: Uuid::new_v4(),
//             evaluation_target_id,
//             scanner_name,
//             scanner_version,
//             started_at: Utc::now(),
//             completed_at: None,
//             total_packages: 0,
//             total_vulnerabilities: 0,
//             critical_count: 0,
//             high_count: 0,
//             medium_count: 0,
//             low_count: 0,
//             scan_duration_ms: None,
//             metadata: None,
//             packages: Vec::new(),
//             cves: Vec::new(),
//             vulnerabilities: Vec::new(),
//         }
//     }
//
//     /// Convert from the parser's VulnixScanResult to our database result
//     pub fn from_parser_result(
//         parser_result: crate::vulnix::vulnix_parser::VulnixScanResult,
//     ) -> Self {
//         Self {
//             scan_id: parser_result.scan.id,
//             evaluation_target_id: parser_result.scan.evaluation_target_id,
//             scanner_name: parser_result.scan.scanner_name,
//             scanner_version: parser_result.scan.scanner_version,
//             started_at: parser_result.scan.created_at.unwrap_or_else(|| Utc::now()),
//             completed_at: parser_result.scan.completed_at,
//             total_packages: parser_result.packages.len() as i32,
//             total_vulnerabilities: parser_result.package_vulnerabilities.len() as i32,
//             critical_count: parser_result.scan.critical_count,
//             high_count: parser_result.scan.high_count,
//             medium_count: parser_result.scan.medium_count,
//             low_count: parser_result.scan.low_count,
//             scan_duration_ms: parser_result.scan.scan_duration_ms,
//             metadata: parser_result.scan.scan_metadata,
//             packages: parser_result.packages,
//             cves: parser_result.cves,
//             vulnerabilities: parser_result.package_vulnerabilities,
//         }
//     }
//
//     pub fn complete(&mut self) {
//         self.completed_at = Some(Utc::now());
//         self.calculate_totals();
//     }
//
//     pub fn calculate_totals(&mut self) {
//         self.total_packages = self.packages.len() as i32;
//         self.total_vulnerabilities = self.vulnerabilities.len() as i32;
//
//         // Count vulnerabilities by severity
//         self.critical_count = 0;
//         self.high_count = 0;
//         self.medium_count = 0;
//         self.low_count = 0;
//
//         for vuln in &self.vulnerabilities {
//             if let Some(cve) = self.cves.iter().find(|c| c.id == vuln.cve_id) {
//                 match cve.severity() {
//                     crate::models::cves::CveSeverity::Critical => self.critical_count += 1,
//                     crate::models::cves::CveSeverity::High => self.high_count += 1,
//                     crate::models::cves::CveSeverity::Medium => self.medium_count += 1,
//                     crate::models::cves::CveSeverity::Low => self.low_count += 1,
//                     crate::models::cves::CveSeverity::Unknown => {
//                         // Count unknown as low for now
//                         self.low_count += 1;
//                     }
//                 }
//             }
//         }
//     }
//
//     pub fn summary(&self) -> DatabaseScanSummary {
//         DatabaseScanSummary {
//             total_packages: self.total_packages,
//             total_vulnerabilities: self.total_vulnerabilities,
//             critical_count: self.critical_count,
//             high_count: self.high_count,
//             medium_count: self.medium_count,
//             low_count: self.low_count,
//             scan_duration_ms: self.scan_duration_ms,
//         }
//     }
//
//     pub fn set_duration(&mut self, start_time: std::time::Instant) {
//         self.scan_duration_ms = Some(start_time.elapsed().as_millis() as i32);
//     }
// }
//
// #[derive(Debug, Serialize, Deserialize)]
// pub struct DatabaseScanSummary {
//     pub total_packages: i32,
//     pub total_vulnerabilities: i32,
//     pub critical_count: i32,
//     pub high_count: i32,
//     pub medium_count: i32,
//     pub low_count: i32,
//     pub scan_duration_ms: Option<i32>,
// }
//
// impl std::fmt::Display for DatabaseScanSummary {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(
//             f,
//             "{} packages, {} vulnerabilities (C:{} H:{} M:{} L:{})",
//             self.total_packages,
//             self.total_vulnerabilities,
//             self.critical_count,
//             self.high_count,
//             self.medium_count,
//             self.low_count
//         )
//     }
// }
