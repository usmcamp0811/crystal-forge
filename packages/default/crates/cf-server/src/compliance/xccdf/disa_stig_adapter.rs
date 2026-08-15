//! DISA STIG framework adapter.
//!
//! Separates DISA-specific knowledge from the generic XCCDF parser.  The
//! adapter is responsible for determining:
//!
//! - Whether a parsed XCCDF document is a DISA STIG.
//! - The canonical framework and release identities.
//! - The canonical requirement key for each rule.
//! - The requirement hierarchy and supplementary metadata (CCI, SRG, …).
//!
//! The adapter never mutates the database; it only produces typed identities
//! and classification outputs that the import pipeline uses.

use serde_json::{Value, json};

use super::models::{BenchmarkMeta, ParsedRule, ParsedXccdf, StandardIdentifier};
use crate::compliance::requirement_model::{
    FrameworkReconciliation, FrameworkReconciliationState, RequirementVersionCanonical,
};

// ── System URIs that identify DISA STIG identifier types ─────────────────────

/// XCCDF `<ident>` system URI for DISA STIG vulnerability IDs (V-xxxxxx).
pub const DISA_STIG_ID_SYSTEM: &str = "http://cyber.mil/cci";
pub const DISA_VULN_ID_SYSTEM: &str = "http://cyber.mil/stigs/stig";

/// Any `<ident>` whose value starts with `V-` and whose system is a
/// recognized DISA identifier system is treated as a stable DISA vulnerability ID.
/// Recognized systems include:
/// - DISA STIG: `http://cyber.mil/stigs/stig`
/// - DISA CCI: `http://cyber.mil/cci`
/// - Any system containing `"cyber.mil"` (authoritative DISA namespace)
fn is_stig_vuln_id(ident: &StandardIdentifier) -> bool {
    ident.value.starts_with("V-")
        && (ident.system.contains("cyber.mil") || ident.system.to_lowercase().contains("stig"))
}

/// Any `<ident>` starting with `CCI-` is a DISA CCI identifier.
fn is_cci_id(ident: &StandardIdentifier) -> bool {
    ident.value.starts_with("CCI-")
}

/// Any `<ident>` starting with `SRG-` is a DISA SRG identifier.
fn is_srg_id(ident: &StandardIdentifier) -> bool {
    ident.value.starts_with("SRG-")
}

// ── Framework detection ───────────────────────────────────────────────────────

/// Returns `true` if the parsed XCCDF document appears to be a DISA STIG.
///
/// Strong attribution evidence is required: an official DISA benchmark
/// namespace/prefix or a V-ID carried by a recognized DISA/STIG identifier
/// system. Display text alone is deliberately not authoritative.
pub fn is_disa_stig(parsed: &ParsedXccdf) -> bool {
    if let Some(bm) = &parsed.benchmark {
        if bm.id.starts_with("xccdf_mil.disa.stig_benchmark_")
            || bm.id.starts_with("xccdf_mil.disa.fso_benchmark_")
        {
            return true;
        }
    }
    parsed
        .rules
        .iter()
        .any(|r| r.identifiers.iter().any(is_stig_vuln_id))
}

// ── Canonical framework identity ──────────────────────────────────────────────

/// Computed framework identity for a DISA STIG document.
#[derive(Debug, Clone)]
pub struct DisaStigFrameworkIdentity {
    /// A stable machine key for the framework lineage.
    /// Derived from the benchmark ID with STIG-specific normalisation.
    /// Example: `"disa-anduril-nixos-stig"`.
    pub canonical_source_key: String,
    /// A stable release identifier within the lineage.
    /// Derived from the benchmark version string, e.g. `"V1R1"`.
    pub canonical_release_key: String,
    /// Human-readable version string, e.g. `"V1R1"`.
    pub version: String,
    /// Display title for the framework version.
    pub title: Option<String>,
    /// Publisher (always `"DISA"` for this adapter).
    pub publisher: String,
}

/// Compute the framework identity for a DISA STIG.
///
/// Returns `None` if the adapter cannot determine a stable identity (e.g.
/// the benchmark has no ID or version).  The caller should treat `None` as
/// a degraded/foreign document and fall back to the generic XCCDF path.
pub fn identify_framework(parsed: &ParsedXccdf) -> Option<DisaStigFrameworkIdentity> {
    let bm = parsed.benchmark.as_ref()?;

    // Canonical source key: normalise the benchmark ID into a slug.
    // Example: "xccdf_mil.disa.stig_benchmark_Anduril_NixOS_STIG" →
    //          "disa-anduril-nixos-stig"
    let canonical_source_key = derive_canonical_source_key(&bm.id);

    // Canonical release key: extract and normalise the version string.
    // Common forms: "V1R1", "Version 1 Release 1", "1.1".
    let version_str = bm.version.as_deref().unwrap_or("").trim().to_string();
    let canonical_release_key = normalise_release_key(&version_str);

    if canonical_source_key.is_empty() || canonical_release_key.is_empty() {
        return None;
    }

    Some(DisaStigFrameworkIdentity {
        canonical_source_key,
        canonical_release_key: canonical_release_key.clone(),
        version: if version_str.is_empty() {
            canonical_release_key
        } else {
            version_str
        },
        title: bm.title.clone(),
        publisher: "DISA".to_string(),
    })
}

/// Derive a stable slug from an XCCDF benchmark ID.
///
/// Strips the XCCDF reverse-DNS prefix, lowercases, replaces non-alphanumeric
/// characters with hyphens, and collapses consecutive hyphens.
fn derive_canonical_source_key(benchmark_id: &str) -> String {
    // Strip well-known XCCDF prefix patterns.
    let stripped = benchmark_id
        .trim_start_matches("xccdf_")
        .trim_start_matches("mil.disa.stig_benchmark_")
        .trim_start_matches("mil.disa.fso_benchmark_")
        // Fall back: strip everything up to the last underscore-delimited segment.
        .to_string();

    // Normalise to a slug.
    let slug: String = stripped
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    // Collapse consecutive hyphens; strip leading/trailing hyphens.
    let mut result = String::new();
    let mut last_hyphen = true;
    for ch in slug.chars() {
        if ch == '-' {
            if !last_hyphen {
                result.push(ch);
                last_hyphen = true;
            }
        } else {
            result.push(ch);
            last_hyphen = false;
        }
    }
    result.trim_end_matches('-').to_string()
}

/// Normalise a STIG version string into a canonical release key.
///
/// Recognised patterns:
/// - `"V1R1"` → `"V1R1"` (already canonical)
/// - `"Version 1 Release 1"` → `"V1R1"`
/// - `"1"` or `"1.0"` → `"V1R1"` (minimal form)
/// - `"3"` → `"V3R1"` (assumed release 1)
///
/// Returns the input uppercased and trimmed if no pattern matches.
fn normalise_release_key(version: &str) -> String {
    let v = version.trim().to_uppercase();

    // Already canonical: e.g. "V1R1", "V2R3".
    if v.len() >= 4
        && v.starts_with('V')
        && v[1..].chars().next().map_or(false, |c| c.is_ascii_digit())
        && v.contains('R')
    {
        return v;
    }

    // "Version 1 Release 1" → V1R1
    if let Some(rest) = v.strip_prefix("VERSION ") {
        // rest = "1 RELEASE 1" or "1"
        let parts: Vec<&str> = rest.splitn(2, " RELEASE ").collect();
        if parts.len() == 2 {
            let ver = parts[0].trim();
            let rel = parts[1].trim();
            return format!("V{ver}R{rel}");
        }
        return format!("V{}R1", rest.trim());
    }

    // Bare number like "1" or "1.0" → "V1R1"
    let major = v
        .split('.')
        .next()
        .unwrap_or(&v)
        .trim()
        .trim_start_matches('V');
    if major.chars().all(|c| c.is_ascii_digit()) && !major.is_empty() {
        return format!("V{major}R1");
    }

    // Fallback: return as-is.
    v
}

// ── Canonical requirement key ─────────────────────────────────────────────────

/// Derive the canonical requirement key for a DISA STIG rule.
///
/// Preference order:
/// 1. A stable STIG vulnerability ID from `<ident>` (e.g. `V-268137`).
/// 2. A SRG-based identifier from `<ident>` (e.g. `SRG-OS-000109-GPOS-00051`).
/// 3. The XCCDF rule id (fallback, may not be stable across releases).
pub fn canonical_key_for_rule(rule: &ParsedRule) -> String {
    // Prefer V-ID identifiers.
    if let Some(vid) = rule.identifiers.iter().find(|i| is_stig_vuln_id(i)) {
        return vid.value.clone();
    }
    // Next: SRG identifier.
    if let Some(srg) = rule.identifiers.iter().find(|i| is_srg_id(i)) {
        return srg.value.clone();
    }
    // Fallback: XCCDF rule id.
    rule.id.clone()
}

// ── Requirement metadata extraction ──────────────────────────────────────────

/// Build the `metadata` JSONB value for a requirement version from a DISA STIG
/// rule.  This is supplementary data — it does NOT affect the canonical key or
/// hierarchy but is required for CCI/SRG-based requirement search.
pub fn requirement_metadata(rule: &ParsedRule) -> Value {
    let cci_ids: Vec<&str> = rule
        .identifiers
        .iter()
        .filter(|i| is_cci_id(i))
        .map(|i| i.as_str())
        .collect();

    let srg_ids: Vec<&str> = rule
        .identifiers
        .iter()
        .filter(|i| is_srg_id(i))
        .map(|i| i.as_str())
        .collect();

    let other_ids: Vec<Value> = rule
        .identifiers
        .iter()
        .filter(|i| !is_cci_id(i) && !is_srg_id(i) && !is_stig_vuln_id(i))
        .map(|i| json!({ "system": i.system, "value": i.value }))
        .collect();

    let refs: Vec<Value> = rule
        .references
        .iter()
        .map(|r| {
            json!({
                "href": r.href,
                "title": r.title,
            })
        })
        .collect();

    json!({
        "cci_ids": cci_ids,
        "srg_ids": srg_ids,
        "other_identifiers": other_ids,
        "references": refs,
        "platforms": rule.platforms,
        "version": rule.version,
        "weight": rule.weight,
    })
}

/// Build the canonical `RequirementVersionCanonical` for a single DISA STIG rule.
pub fn canonical_for_rule(
    rule: &ParsedRule,
    canonical_requirement_key: &str,
) -> RequirementVersionCanonical {
    RequirementVersionCanonical {
        canonical_requirement_key: canonical_requirement_key.to_string(),
        external_id: canonical_requirement_key.to_string(),
        title: rule.title.clone(),
        description: rule.description.clone(),
        kind: "rule".to_string(),
        severity: rule.severity.clone(),
        check_text: rule.checks.first().and_then(|c| {
            c.body_parts.iter().find_map(|bp| {
                if let super::models::CheckBodyPart::Inline { content } = bp {
                    Some(content.clone())
                } else {
                    None
                }
            })
        }),
        fix_text: rule.fix.as_ref().map(|f| f.content.clone()),
        metadata: requirement_metadata(rule),
    }
}

/// Return the complete authoritative requirement set represented by a parsed
/// DISA framework source. This intentionally does not depend on policy import
/// decisions or selected implementation records.
pub fn canonical_requirements_for_framework(
    parsed: &ParsedXccdf,
) -> Vec<RequirementVersionCanonical> {
    parsed
        .rules
        .iter()
        .map(|rule| {
            let key = canonical_key_for_rule(rule);
            canonical_for_rule(rule, &key)
        })
        .collect()
}

/// Requirement hierarchy node produced by the adapter for a single rule.
///
/// For DISA STIGs the hierarchy is: Group → Rule.
/// Group information comes from `ParsedRule::group_id`.
#[derive(Debug, Clone)]
pub struct StigHierarchyNode {
    /// The canonical key of this node.
    pub canonical_key: String,
    /// Node kind: `"group"` for group-level nodes, `"rule"` for leaf rules.
    pub kind: String,
    /// Title for this node.
    pub title: Option<String>,
    /// Canonical key of the parent, or `None` for root-level nodes.
    pub parent_canonical_key: Option<String>,
}

/// Build the hierarchy nodes for a DISA STIG rule.
///
/// Returns a `Vec` of nodes to upsert, in parent-first order:
/// 1. The group node (if `rule.group_id` is set).
/// 2. The rule node itself.
pub fn hierarchy_nodes_for_rule(
    rule: &ParsedRule,
    rule_canonical_key: &str,
) -> Vec<StigHierarchyNode> {
    let mut nodes = Vec::with_capacity(2);

    let group_key = rule.group_id.as_deref().map(str::to_string);

    if let Some(ref gk) = group_key {
        // Find the matching group in the parsed document to get its title.
        // This function does not have access to the full ParsedXccdf, so the
        // caller is expected to pass a title through `StigHierarchyContext`.
        nodes.push(StigHierarchyNode {
            canonical_key: gk.clone(),
            kind: "group".to_string(),
            title: None, // caller fills this in from parsed.groups
            parent_canonical_key: None,
        });
    }

    nodes.push(StigHierarchyNode {
        canonical_key: rule_canonical_key.to_string(),
        kind: "rule".to_string(),
        title: rule.title.clone(),
        parent_canonical_key: group_key,
    });

    nodes
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::xccdf::models::StandardIdentifier;

    fn ident(system: &str, value: &str) -> StandardIdentifier {
        StandardIdentifier {
            system: system.to_string(),
            value: value.to_string(),
        }
    }

    // Helper to construct a minimal ParsedXccdf for testing.
    fn minimal_parsed_xccdf(
        benchmark_id: &str,
        benchmark_title: &str,
        version: &str,
        rules: Vec<ParsedRule>,
    ) -> ParsedXccdf {
        use crate::compliance::xccdf::models::{DocumentClass, Fidelity};
        ParsedXccdf {
            class: DocumentClass::ForeignXccdf,
            fidelity: Fidelity::NativeExact,
            fidelity_losses: vec![],
            source_filename: Some("test.xml".to_string()),
            source_bytes: vec![],
            source_sha256: "test".to_string(),
            xccdf_namespace_version: Some("1.2"),
            xccdf_version: Some("1.2".to_string()),
            benchmark: Some(BenchmarkMeta {
                id: benchmark_id.to_string(),
                title: Some(benchmark_title.to_string()),
                description: None,
                version: Some(version.to_string()),
                status: None,
                status_date: None,
                platforms: vec![],
                publisher: None,
                references: vec![],
            }),
            profiles: vec![],
            rules,
            groups: vec![],
            values: vec![],
            cf_bundle_meta: None,
            signature_info: None,
            errors: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn canonical_key_prefers_v_id() {
        let rule = ParsedRule {
            id: "xccdf_mil.disa.stig_rule_SV-268137r1_rule".to_string(),
            title: Some("SSH root login".to_string()),
            identifiers: vec![
                ident("http://cyber.mil/stigs/stig", "V-268137"),
                ident("http://cyber.mil/cci", "CCI-000770"),
            ],
            description: None,
            rationale: None,
            severity: Some("high".to_string()),
            weight: None,
            version: None,
            checks: vec![],
            fix: None,
            references: vec![],
            platforms: vec![],
            group_id: Some("V-268137".to_string()),
            rule_order: None,
            cf_policy_meta: None,
            preserved_xml: None,
        };
        assert_eq!(canonical_key_for_rule(&rule), "V-268137");
    }

    #[test]
    fn canonical_key_falls_back_to_rule_id() {
        let rule = ParsedRule {
            id: "xccdf_mil.disa.stig_rule_SV-999999r1_rule".to_string(),
            identifiers: vec![ident("http://cyber.mil/cci", "CCI-000001")],
            title: None,
            description: None,
            rationale: None,
            severity: None,
            weight: None,
            version: None,
            checks: vec![],
            fix: None,
            references: vec![],
            platforms: vec![],
            group_id: None,
            rule_order: None,
            cf_policy_meta: None,
            preserved_xml: None,
        };
        assert_eq!(
            canonical_key_for_rule(&rule),
            "xccdf_mil.disa.stig_rule_SV-999999r1_rule"
        );
    }

    #[test]
    fn normalise_release_key_v1r1() {
        assert_eq!(normalise_release_key("V1R1"), "V1R1");
    }

    #[test]
    fn normalise_release_key_long_form() {
        assert_eq!(normalise_release_key("Version 1 Release 1"), "V1R1");
    }

    #[test]
    fn normalise_release_key_bare_number() {
        assert_eq!(normalise_release_key("1"), "V1R1");
    }

    #[test]
    fn derive_source_key_strips_prefix() {
        let key = derive_canonical_source_key("xccdf_mil.disa.stig_benchmark_Anduril_NixOS_STIG");
        assert!(key.contains("anduril"), "expected anduril in key: {key}");
        assert!(!key.contains('_'), "expected no underscores in key: {key}");
    }

    #[test]
    fn requirement_metadata_includes_cci_and_srg() {
        let rule = ParsedRule {
            id: "rule-1".to_string(),
            title: None,
            identifiers: vec![
                ident("http://cyber.mil/stigs/stig", "V-268137"),
                ident("http://cyber.mil/cci", "CCI-000770"),
                ident("http://cyber.mil/srg", "SRG-OS-000109"),
            ],
            description: None,
            rationale: None,
            severity: None,
            weight: None,
            version: None,
            checks: vec![],
            fix: None,
            references: vec![],
            platforms: vec![],
            group_id: None,
            rule_order: None,
            cf_policy_meta: None,
            preserved_xml: None,
        };
        let meta = requirement_metadata(&rule);
        let ccis = meta["cci_ids"].as_array().unwrap();
        let srgs = meta["srg_ids"].as_array().unwrap();
        assert_eq!(ccis.len(), 1);
        assert_eq!(ccis[0].as_str().unwrap(), "CCI-000770");
        assert_eq!(srgs.len(), 1);
        assert_eq!(srgs[0].as_str().unwrap(), "SRG-OS-000109");
    }

    // ── DISA detection boundary tests ────────────────────────────────────────

    #[test]
    fn is_stig_vuln_id_official_disa_benchmark_prefix() {
        // Official DISA benchmark ID from xccdf_mil.disa.stig_benchmark_* prefix.
        // This is strong evidence and should always return true via is_disa_stig.
        let parsed = minimal_parsed_xccdf(
            "xccdf_mil.disa.stig_benchmark_Anduril_NixOS_STIG",
            "NixOS STIG",
            "V1R1",
            vec![],
        );
        assert!(is_disa_stig(&parsed));
    }

    #[test]
    fn is_stig_vuln_id_official_disa_fso_benchmark_prefix() {
        // Official DISA FSO benchmark ID.
        let parsed = minimal_parsed_xccdf(
            "xccdf_mil.disa.fso_benchmark_Some_FSO_Benchmark",
            "FSO Benchmark",
            "V1R1",
            vec![],
        );
        assert!(is_disa_stig(&parsed));
    }

    #[test]
    fn is_stig_vuln_id_recognized_cyber_mil_system() {
        // Recognized DISA identifier system: http://cyber.mil/stigs/stig
        let rule = ParsedRule {
            id: "rule-1".to_string(),
            title: Some("Test Rule".to_string()),
            identifiers: vec![ident("http://cyber.mil/stigs/stig", "V-268137")],
            description: None,
            rationale: None,
            severity: None,
            weight: None,
            version: None,
            checks: vec![],
            fix: None,
            references: vec![],
            platforms: vec![],
            group_id: None,
            rule_order: None,
            cf_policy_meta: None,
            preserved_xml: None,
        };
        let parsed =
            minimal_parsed_xccdf("generic-benchmark", "Generic Benchmark", "1.0", vec![rule]);
        assert!(is_disa_stig(&parsed));
    }

    #[test]
    fn is_stig_vuln_id_cyber_mil_namespace() {
        // Recognized DISA namespace via cyber.mil domain.
        let rule = ParsedRule {
            id: "rule-1".to_string(),
            title: Some("Test Rule".to_string()),
            identifiers: vec![ident("http://cyber.mil/custom/system", "V-999999")],
            description: None,
            rationale: None,
            severity: None,
            weight: None,
            version: None,
            checks: vec![],
            fix: None,
            references: vec![],
            platforms: vec![],
            group_id: None,
            rule_order: None,
            cf_policy_meta: None,
            preserved_xml: None,
        };
        let parsed =
            minimal_parsed_xccdf("generic-benchmark", "Generic Benchmark", "1.0", vec![rule]);
        assert!(is_disa_stig(&parsed));
    }

    #[test]
    fn is_disa_stig_rejects_arbitrary_vuln_uri() {
        // Arbitrary vulnerability URI without recognized DISA evidence.
        // This should NOT be classified as DISA STIG.
        let rule = ParsedRule {
            id: "rule-1".to_string(),
            title: Some("Test Rule".to_string()),
            identifiers: vec![ident("http://example.com/vulnerability", "V-999999")],
            description: None,
            rationale: None,
            severity: None,
            weight: None,
            version: None,
            checks: vec![],
            fix: None,
            references: vec![],
            platforms: vec![],
            group_id: None,
            rule_order: None,
            cf_policy_meta: None,
            preserved_xml: None,
        };
        let parsed = minimal_parsed_xccdf(
            "generic-cve-benchmark",
            "Generic CVE Benchmark",
            "1.0",
            vec![rule],
        );
        assert!(!is_disa_stig(&parsed));
    }

    #[test]
    fn is_disa_stig_rejects_title_only() {
        // Title/description containing "STIG" alone is not authoritative.
        let rule = ParsedRule {
            id: "rule-1".to_string(),
            title: Some("STIG-like rule".to_string()),
            identifiers: vec![
                ident("http://example.com/custom", "V-999999"),
                ident("http://example.com/cci", "CCI-000001"),
            ],
            description: None,
            rationale: None,
            severity: None,
            weight: None,
            version: None,
            checks: vec![],
            fix: None,
            references: vec![],
            platforms: vec![],
            group_id: None,
            rule_order: None,
            cf_policy_meta: None,
            preserved_xml: None,
        };
        let parsed = minimal_parsed_xccdf(
            "generic-benchmark-stig-like",
            "My STIG Benchmark",
            "1.0",
            vec![rule],
        );
        assert!(!is_disa_stig(&parsed));
    }

    #[test]
    fn is_disa_stig_accepts_stig_system_string() {
        // System string containing "stig" (case-insensitive) is recognized.
        let rule = ParsedRule {
            id: "rule-1".to_string(),
            title: Some("Test Rule".to_string()),
            identifiers: vec![ident("http://example.com/stig-id", "V-123456")],
            description: None,
            rationale: None,
            severity: None,
            weight: None,
            version: None,
            checks: vec![],
            fix: None,
            references: vec![],
            platforms: vec![],
            group_id: None,
            rule_order: None,
            cf_policy_meta: None,
            preserved_xml: None,
        };
        let parsed = minimal_parsed_xccdf("generic-benchmark", "Test", "1.0", vec![rule]);
        assert!(is_disa_stig(&parsed));
    }

    #[test]
    fn is_disa_stig_rejects_generic_vulnerability_without_disa_evidence() {
        // Generic CVE or vulnerability identifier without DISA evidence.
        // "vuln" substring is no longer accepted.
        let rule = ParsedRule {
            id: "rule-1".to_string(),
            title: Some("Generic Rule".to_string()),
            identifiers: vec![ident("http://example.com/vuln", "CVE-2021-12345")],
            description: None,
            rationale: None,
            severity: None,
            weight: None,
            version: None,
            checks: vec![],
            fix: None,
            references: vec![],
            platforms: vec![],
            group_id: None,
            rule_order: None,
            cf_policy_meta: None,
            preserved_xml: None,
        };
        let parsed = minimal_parsed_xccdf(
            "generic-benchmark",
            "Generic Vulnerability Benchmark",
            "1.0",
            vec![rule],
        );
        assert!(!is_disa_stig(&parsed));
    }
}

// Helper to make `StandardIdentifier` usable as a str in maps.
trait IdentAsStr {
    fn as_str(&self) -> &str;
}
impl IdentAsStr for StandardIdentifier {
    fn as_str(&self) -> &str {
        &self.value
    }
}
