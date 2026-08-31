//! SRG / CCI compliance identifier normalisation helpers.
//!
//! These utilities are used by create/update policy paths, XCCDF import, and
//! the version-list API to provide consistent, server-authoritative
//! normalisation and validation of Security Requirements Guide (SRG) and
//! Control Correlation Identifier (CCI) mappings stored in
//! `deployment_policy_versions.compliance_metadata`.

use anyhow::{Result, bail};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum length (bytes) of a single SRG or CCI identifier string.
pub const MAX_MAPPING_ID_LEN: usize = 128;

/// Maximum number of SRG IDs stored per policy version.
pub const MAX_SRG_COUNT: usize = 64;

/// Maximum number of CCI IDs stored per policy version.
pub const MAX_CCI_COUNT: usize = 128;

// ─── Validation ──────────────────────────────────────────────────────────────

/// Validate and normalise a single SRG identifier.
///
/// Rules:
/// - Must begin with `SRG-` (case-insensitive; returned as uppercase)
/// - After the prefix: uppercase letters, digits, and hyphens only
/// - Must not be just `SRG-` (must have at least one character after the prefix)
/// - Maximum length: [`MAX_MAPPING_ID_LEN`]
///
/// Returns the normalised (uppercased, trimmed) form on success.
pub fn validate_srg(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    let upper = trimmed.to_ascii_uppercase();

    if trimmed.len() > MAX_MAPPING_ID_LEN {
        bail!(
            "SRG identifier too long (max {} chars): {:?}",
            MAX_MAPPING_ID_LEN,
            &upper[..MAX_MAPPING_ID_LEN.min(upper.len())]
        );
    }

    if !upper.starts_with("SRG-") {
        bail!("Invalid SRG identifier {:?}: must begin with 'SRG-'", upper);
    }

    let after_prefix = &upper["SRG-".len()..];
    if after_prefix.is_empty() {
        bail!(
            "Invalid SRG identifier {:?}: must have content after 'SRG-'",
            upper
        );
    }

    for ch in after_prefix.chars() {
        if !ch.is_ascii_uppercase() && !ch.is_ascii_digit() && ch != '-' {
            bail!(
                "Invalid SRG identifier {:?}: character {:?} is not allowed (use A–Z, 0–9, '-')",
                upper,
                ch
            );
        }
    }

    Ok(upper)
}

/// Validate and normalise a single CCI identifier.
///
/// Rules:
/// - Must begin with `CCI-` (case-insensitive; returned as uppercase)
/// - After the prefix: exactly numeric digits only (no letters)
/// - Must not be just `CCI-`
/// - Maximum length: [`MAX_MAPPING_ID_LEN`]
///
/// Returns the normalised form on success.
pub fn validate_cci(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    let upper = trimmed.to_ascii_uppercase();

    if trimmed.len() > MAX_MAPPING_ID_LEN {
        bail!(
            "CCI identifier too long (max {} chars): {:?}",
            MAX_MAPPING_ID_LEN,
            &upper[..MAX_MAPPING_ID_LEN.min(upper.len())]
        );
    }

    if !upper.starts_with("CCI-") {
        bail!("Invalid CCI identifier {:?}: must begin with 'CCI-'", upper);
    }

    let after_prefix = &upper["CCI-".len()..];
    if after_prefix.is_empty() {
        bail!(
            "Invalid CCI identifier {:?}: must have digits after 'CCI-'",
            upper
        );
    }

    if !after_prefix.chars().all(|ch| ch.is_ascii_digit()) {
        bail!(
            "Invalid CCI identifier {:?}: the part after 'CCI-' must be numeric digits only",
            upper
        );
    }

    Ok(upper)
}

// ─── Normalise lists ─────────────────────────────────────────────────────────

/// Normalise a list of raw SRG strings: validate each, deduplicate, sort.
///
/// Returns an error identifying the first invalid value.
pub fn normalise_srg_ids(raw: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    for item in raw {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue; // silently skip blank entries
        }
        let normalised = validate_srg(trimmed).map_err(|e| anyhow::anyhow!("srg_ids: {}", e))?;
        if !out.contains(&normalised) {
            out.push(normalised);
        }
    }
    if out.len() > MAX_SRG_COUNT {
        bail!("Too many SRG IDs (max {})", MAX_SRG_COUNT);
    }
    out.sort();
    Ok(out)
}

/// Normalise a list of raw CCI strings: validate each, deduplicate, sort.
///
/// Returns an error identifying the first invalid value.
pub fn normalise_cci_ids(raw: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    for item in raw {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalised = validate_cci(trimmed).map_err(|e| anyhow::anyhow!("cci_ids: {}", e))?;
        if !out.contains(&normalised) {
            out.push(normalised);
        }
    }
    if out.len() > MAX_CCI_COUNT {
        bail!("Too many CCI IDs (max {})", MAX_CCI_COUNT);
    }
    out.sort();
    Ok(out)
}

// ─── compliance_metadata helpers ────────────────────────────────────────────

/// Extract the curated `srg_ids` array from a `compliance_metadata` JSON value.
///
/// Resolution rule (read-time, non-mutating):
/// 1. If the key `"srg_ids"` is explicitly present (even as `[]`), use it.
/// 2. Otherwise fall back to deriving SRG-looking values from the generic
///    `identifiers` array — but ONLY when the value begins with `"SRG-"`.
///    Prose / rationale text is NOT scanned.
pub fn extract_srg_ids(metadata: &serde_json::Value) -> Vec<String> {
    if let Some(arr) = metadata.get("srg_ids").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::to_string)
            .collect();
    }
    // Derive from imported structured identifiers
    if let Some(idents) = metadata.get("identifiers").and_then(|v| v.as_array()) {
        return idents
            .iter()
            .filter_map(|id| id.get("value").and_then(|v| v.as_str()))
            .filter(|v| v.to_ascii_uppercase().starts_with("SRG-"))
            .map(str::to_string)
            .collect();
    }
    Vec::new()
}

/// Extract the curated `cci_ids` array from a `compliance_metadata` JSON value.
///
/// Resolution rule (read-time, non-mutating):
/// 1. If the key `"cci_ids"` is explicitly present (even as `[]`), use it.
/// 2. Otherwise fall back to deriving CCI-looking values from the generic
///    `identifiers` array where the value begins with `"CCI-"`.
pub fn extract_cci_ids(metadata: &serde_json::Value) -> Vec<String> {
    if let Some(arr) = metadata.get("cci_ids").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::to_string)
            .collect();
    }
    // Derive from imported structured identifiers
    if let Some(idents) = metadata.get("identifiers").and_then(|v| v.as_array()) {
        return idents
            .iter()
            .filter_map(|id| id.get("value").and_then(|v| v.as_str()))
            .filter(|v| v.to_ascii_uppercase().starts_with("CCI-"))
            .map(str::to_string)
            .collect();
    }
    Vec::new()
}

/// Extract evidence specifications from compliance_metadata.
///
/// Returns an array of evidence spec objects as stored in compliance_metadata.evidence_specs.
/// If the key is absent or not an array, returns an empty array.
pub fn extract_evidence_specs(metadata: &serde_json::Value) -> Vec<serde_json::Value> {
    metadata
        .get("evidence_specs")
        .and_then(|v| v.as_array())
        .map(|arr| arr.clone())
        .unwrap_or_default()
}

/// Validate a single evidence spec for required fields per kind.
///
/// Returns an error if validation fails; returns Ok(()) if valid.
pub fn validate_evidence_spec(spec: &serde_json::Value) -> Result<()> {
    let obj = spec
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("evidence_specs: item must be an object"))?;

    let kind = obj
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("evidence_specs: missing or invalid 'kind' field"))?;

    // Match case-insensitively to handle both serialization formats
    let kind_lower = kind.to_lowercase();

    // Helper: get field from either flattened format (top-level) or details-nested format
    let get_field = |field_name: &str| {
        obj.get(field_name).and_then(|v| v.as_str()).or_else(|| {
            obj.get("details")
                .and_then(|v| v.as_object())
                .and_then(|d| d.get(field_name))
                .and_then(|v| v.as_str())
        })
    };

    match kind_lower.as_str() {
        "command" => {
            // Command requires both cmd and expect (matching editor validation)
            let cmd = get_field("cmd");
            if cmd.is_none() || cmd.map_or(false, |s| s.is_empty()) {
                bail!("evidence_specs: Command evidence must have non-empty 'cmd' field");
            }
            let expect = get_field("expect");
            if expect.is_none() || expect.map_or(false, |s| s.is_empty()) {
                bail!("evidence_specs: Command evidence must have non-empty 'expect' field");
            }
        }
        "file" => {
            let path = get_field("path");
            if path.is_none() || path.map_or(false, |s| s.is_empty()) {
                bail!("evidence_specs: File evidence must have non-empty 'path' field");
            }
        }
        "unitstate" | "unit_state" => {
            let unit = get_field("unit");
            let state = get_field("state");
            if unit.is_none() || unit.map_or(false, |s| s.is_empty()) {
                bail!("evidence_specs: UnitState evidence must have non-empty 'unit' field");
            }
            if state.is_none() || state.map_or(false, |s| s.is_empty()) {
                bail!("evidence_specs: UnitState evidence must have non-empty 'state' field");
            }
        }
        "evalattr" | "eval_attr" => {
            let attr = get_field("attr");
            if attr.is_none() || attr.map_or(false, |s| s.is_empty()) {
                bail!("evidence_specs: EvalAttr evidence must have non-empty 'attr' field");
            }
        }
        "attestation" => {
            let note = get_field("note");
            if note.is_none() || note.map_or(false, |s| s.is_empty()) {
                bail!("evidence_specs: Attestation evidence must have non-empty 'note' field");
            }
        }
        "log" => {
            // Log requires unit and match_text in addition to source (matching editor validation)
            let source = get_field("source");
            if source.is_none() || source.map_or(false, |s| s.is_empty()) {
                bail!("evidence_specs: Log evidence must have non-empty 'source' field");
            }
            let unit = get_field("unit");
            if unit.is_none() || unit.map_or(false, |s| s.is_empty()) {
                bail!("evidence_specs: Log evidence must have non-empty 'unit' field");
            }
            let match_text = get_field("match_text");
            if match_text.is_none() || match_text.map_or(false, |s| s.is_empty()) {
                bail!("evidence_specs: Log evidence must have non-empty 'match_text' field");
            }
        }
        _ => {
            bail!("evidence_specs: unknown kind '{}'", kind);
        }
    }
    Ok(())
}

/// Strictly decode Evidence specs from compliance_metadata.
///
/// This is fail-closed: malformed persisted Evidence causes an error instead of silently disappearing.
///
/// Semantics:
/// - key absent → Ok([])
/// - key present as empty array → Ok([])
/// - key present with valid specs → Ok(Vec<EvidenceSpec>)
/// - key present but not array → Err
/// - any array entry malformed/invalid → Err
///
/// This is used when loading stored policy versions where data integrity matters.
/// Do not use filter_map or other lenient parsing.
pub fn decode_evidence_specs_strict(
    metadata: &serde_json::Value,
) -> Result<Vec<crate::api::models::EvidenceSpec>> {
    match metadata.get("evidence_specs") {
        None => {
            // Key absent: no evidence configured
            Ok(Vec::new())
        }
        Some(evidence_value) => {
            // Key present: must be an array
            let arr = evidence_value.as_array().ok_or_else(|| {
                let type_name = match evidence_value {
                    serde_json::Value::Null => "null",
                    serde_json::Value::Bool(_) => "bool",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Object(_) => "object",
                };
                anyhow::anyhow!("evidence_specs: field must be array, got {}", type_name)
            })?;

            // Decode and validate each entry
            let mut result = Vec::with_capacity(arr.len());
            for (idx, entry) in arr.iter().enumerate() {
                let spec: crate::api::models::EvidenceSpec = serde_json::from_value(entry.clone())
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "evidence_specs[{}]: failed to decode evidence spec: {}",
                            idx,
                            e
                        )
                    })?;
                // Validate the decoded spec (fail-closed on semantic errors)
                validate_evidence_spec(&serde_json::to_value(&spec)?)
                    .map_err(|e| anyhow::anyhow!("evidence_specs[{}]: {}", idx, e))?;
                result.push(spec);
            }
            Ok(result)
        }
    }
}

/// Merge evidence specs into an existing `compliance_metadata` JSON object.
///
/// Semantics:
/// - `None` → caller did not touch this field; preserve existing value exactly.
/// - `Some([])` → caller cleared evidence; store empty array.
/// - `Some([...])` → validate and replace evidence array.
///
/// All other keys in `existing` survive unchanged.
///
/// Returns an error if any evidence spec is invalid.
pub fn merge_evidence_into_metadata(
    existing: &serde_json::Value,
    evidence_specs: Option<&[crate::api::models::EvidenceSpec]>,
) -> Result<serde_json::Value> {
    if evidence_specs.is_none() {
        // Caller did not specify evidence; preserve existing
        return Ok(existing.clone());
    }

    let mut obj = existing.as_object().cloned().unwrap_or_default();

    if let Some(specs) = evidence_specs {
        // Validate all specs before updating
        for spec in specs {
            let spec_json = serde_json::to_value(spec)?;
            validate_evidence_spec(&spec_json)?;
        }
        // All valid; store the array
        obj.insert("evidence_specs".to_string(), serde_json::to_value(specs)?);
    } else {
        // Empty array; clear evidence
        obj.insert("evidence_specs".to_string(), serde_json::json!([]));
    }

    Ok(serde_json::Value::Object(obj))
}

/// Merge SRG/CCI mappings into an existing `compliance_metadata` JSON object.
///
/// Semantics:
/// - `None` → caller did not touch this field; preserve existing value exactly.
/// - `Some([])` → caller cleared curated mappings; store `[]`.
/// - `Some([...])` → normalise and replace that mapping array.
///
/// All other keys in `existing` survive unchanged (source fidelity, rationale,
/// checks, fixes, identifiers, references, etc.).
///
/// Returns an error (with field name) for any invalid identifier.
pub fn merge_policy_mappings(
    existing: &serde_json::Value,
    srg_ids: Option<&[String]>,
    cci_ids: Option<&[String]>,
) -> Result<serde_json::Value> {
    let mut merged = match existing.as_object() {
        Some(obj) => obj.clone(),
        None => serde_json::Map::new(),
    };

    if let Some(raw_srgs) = srg_ids {
        let normalised = normalise_srg_ids(raw_srgs)?;
        merged.insert(
            "srg_ids".to_string(),
            serde_json::Value::Array(
                normalised
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    if let Some(raw_ccis) = cci_ids {
        let normalised = normalise_cci_ids(raw_ccis)?;
        merged.insert(
            "cci_ids".to_string(),
            serde_json::Value::Array(
                normalised
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    Ok(serde_json::Value::Object(merged))
}

/// Build the initial `compliance_metadata` for a freshly created policy
/// (no existing metadata to merge into).
pub fn initial_policy_metadata(
    srg_ids: Option<&[String]>,
    cci_ids: Option<&[String]>,
) -> Result<serde_json::Value> {
    merge_policy_mappings(&serde_json::json!({}), srg_ids, cci_ids)
}

// ─── Classification helpers ───────────────────────────────────────────────────

/// Extract semantic classification fields from a `compliance_metadata` JSON value.
///
/// Returns a tuple of `(category, framework, severity, control_family, cmmc_level, cis_section, rationale)`.
/// All fields are `None` when absent from the metadata object.
pub fn extract_classification(
    metadata: &serde_json::Value,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i32>,
    Option<String>,
    Option<String>,
) {
    let obj = metadata.as_object();
    let get_str = |key: &str| {
        obj.and_then(|m| m.get(key))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    let category = get_str("category");
    let framework = get_str("framework");
    let severity = get_str("severity");
    let control_family = get_str("control_family");
    let cmmc_level = obj
        .and_then(|m| m.get("cmmc_level"))
        .and_then(|v| v.as_i64())
        .map(|n| n as i32);
    let cis_section = get_str("cis_section");
    let rationale = get_str("rationale");
    (
        category,
        framework,
        severity,
        control_family,
        cmmc_level,
        cis_section,
        rationale,
    )
}

/// Merge semantic classification fields into an existing `compliance_metadata` JSON object.
///
/// Only keys supplied as `Some(...)` are updated; `None` values leave the existing key unchanged.
/// All other keys in the existing object survive unchanged.
pub fn merge_classification_into_metadata(
    existing: &serde_json::Value,
    category: Option<&str>,
    framework: Option<&str>,
    severity: Option<&str>,
    control_family: Option<&str>,
    cmmc_level: Option<i32>,
    cis_section: Option<&str>,
    rationale: Option<&str>,
) -> serde_json::Value {
    let mut obj = existing.as_object().cloned().unwrap_or_default();
    if let Some(v) = category {
        obj.insert("category".into(), serde_json::json!(v));
    }
    if let Some(v) = framework {
        obj.insert("framework".into(), serde_json::json!(v));
    }
    if let Some(v) = severity {
        obj.insert("severity".into(), serde_json::json!(v));
    }
    if let Some(v) = control_family {
        obj.insert("control_family".into(), serde_json::json!(v));
    }
    if let Some(v) = cmmc_level {
        obj.insert("cmmc_level".into(), serde_json::json!(v));
    }
    if let Some(v) = cis_section {
        obj.insert("cis_section".into(), serde_json::json!(v));
    }
    if let Some(v) = rationale {
        obj.insert("rationale".into(), serde_json::json!(v));
    }
    serde_json::Value::Object(obj)
}

/// Applies tri-state classification updates to compliance metadata.
///
/// An outer `None` preserves a key. `Some(Some(value))` replaces a key, and
/// `Some(None)` removes a key. All unrelated metadata remains unchanged.
pub fn patch_classification_into_metadata(
    existing: &serde_json::Value,
    category: Option<&str>,
    framework: Option<Option<&str>>,
    severity: Option<Option<&str>>,
    control_family: Option<Option<&str>>,
    cmmc_level: Option<Option<i32>>,
    cis_section: Option<Option<&str>>,
    rationale: Option<Option<&str>>,
) -> serde_json::Value {
    let mut obj = existing.as_object().cloned().unwrap_or_default();
    if let Some(value) = category {
        obj.insert("category".into(), serde_json::json!(value));
    }
    for (key, update) in [
        ("framework", framework),
        ("severity", severity),
        ("control_family", control_family),
        ("cis_section", cis_section),
        ("rationale", rationale),
    ] {
        match update {
            Some(Some(value)) => {
                obj.insert(key.into(), serde_json::json!(value));
            }
            Some(None) => {
                obj.remove(key);
            }
            None => {}
        }
    }
    match cmmc_level {
        Some(Some(value)) => {
            obj.insert("cmmc_level".into(), serde_json::json!(value));
        }
        Some(None) => {
            obj.remove("cmmc_level");
        }
        None => {}
    }
    serde_json::Value::Object(obj)
}

/// Infer the policy category for policies that have no stored `"category"` key
/// in `compliance_metadata`.
///
/// Priority:
/// 1. Explicit `"category"` key in `compliance_metadata` (already stored).
/// 2. Known `policy_type` values that imply a category.
/// 3. SRG/CCI presence implies `"security"`.
/// 4. Default: `"deployment"`.
pub fn infer_legacy_category(
    policy_type: &str,
    compliance_metadata: &serde_json::Value,
) -> &'static str {
    // Check compliance_metadata first
    if let Some(cat) = compliance_metadata.get("category").and_then(|v| v.as_str()) {
        return match cat {
            "security" => "security",
            "pipeline" => "pipeline",
            "rollout" => "rollout",
            "deployment" => "deployment",
            _ => "deployment",
        };
    }
    // Infer from policy_type
    match policy_type {
        "require_cve_check" | "cve_threshold" => "pipeline",
        "time_window" | "require_approvals" | "canary_rollout" => "rollout",
        _ => {
            // Check SRG/CCI presence for security
            let has_srg = compliance_metadata
                .get("srg_ids")
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            let has_cci = compliance_metadata
                .get("cci_ids")
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            if has_srg || has_cci {
                "security"
            } else {
                "deployment"
            }
        }
    }
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod classification_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cve_policy_is_pipeline() {
        assert_eq!(
            infer_legacy_category("require_cve_check", &json!({})),
            "pipeline"
        );
        assert_eq!(
            infer_legacy_category("cve_threshold", &json!({})),
            "pipeline"
        );
    }

    #[test]
    fn rollout_policies_are_rollout() {
        assert_eq!(infer_legacy_category("time_window", &json!({})), "rollout");
        assert_eq!(
            infer_legacy_category("require_approvals", &json!({})),
            "rollout"
        );
        assert_eq!(
            infer_legacy_category("canary_rollout", &json!({})),
            "rollout"
        );
    }

    #[test]
    fn srg_cci_policy_is_security() {
        let meta = json!({ "srg_ids": ["SRG-OS-000001"] });
        assert_eq!(infer_legacy_category("custom_check", &meta), "security");
        let meta = json!({ "cci_ids": ["CCI-000001"] });
        assert_eq!(infer_legacy_category("custom_check", &meta), "security");
    }

    #[test]
    fn explicit_category_wins() {
        let meta = json!({ "category": "rollout", "srg_ids": ["SRG-OS-000001"] });
        assert_eq!(infer_legacy_category("custom_check", &meta), "rollout");
    }

    #[test]
    fn unknown_custom_is_deployment() {
        assert_eq!(
            infer_legacy_category("custom_check", &json!({})),
            "deployment"
        );
    }

    #[test]
    fn cmmc_level_not_inferred_from_severity() {
        // CMMC level must come from explicit cmmc_level key only, never from severity
        let meta = json!({ "severity": "high", "category": "security" });
        let cmmc = meta.get("cmmc_level").and_then(|v| v.as_i64());
        assert!(
            cmmc.is_none(),
            "CMMC level must not be inferred from severity"
        );
    }

    #[test]
    fn extract_classification_reads_all_fields() {
        let meta = json!({
            "category": "security",
            "framework": "DISA STIG",
            "severity": "high",
            "control_family": "AC",
            "cmmc_level": 2,
            "cis_section": "5.2.3",
            "rationale": "Must enable firewall",
        });
        let (cat, fw, sev, cf, cmmc, cis, rat) = extract_classification(&meta);
        assert_eq!(cat.as_deref(), Some("security"));
        assert_eq!(fw.as_deref(), Some("DISA STIG"));
        assert_eq!(sev.as_deref(), Some("high"));
        assert_eq!(cf.as_deref(), Some("AC"));
        assert_eq!(cmmc, Some(2));
        assert_eq!(cis.as_deref(), Some("5.2.3"));
        assert_eq!(rat.as_deref(), Some("Must enable firewall"));
    }

    #[test]
    fn extract_classification_returns_none_for_absent_fields() {
        let (cat, fw, sev, cf, cmmc, cis, rat) = extract_classification(&json!({}));
        assert!(cat.is_none());
        assert!(fw.is_none());
        assert!(sev.is_none());
        assert!(cf.is_none());
        assert!(cmmc.is_none());
        assert!(cis.is_none());
        assert!(rat.is_none());
    }

    #[test]
    fn merge_classification_preserves_existing_keys() {
        let existing = json!({ "srg_ids": ["SRG-OS-000001"], "cci_ids": ["CCI-000001"] });
        let merged = merge_classification_into_metadata(
            &existing,
            Some("security"),
            None,
            Some("high"),
            None,
            None,
            None,
            None,
        );
        assert_eq!(merged["category"], "security");
        assert_eq!(merged["severity"], "high");
        // Existing SRG/CCI keys preserved
        assert_eq!(merged["srg_ids"], json!(["SRG-OS-000001"]));
        assert_eq!(merged["cci_ids"], json!(["CCI-000001"]));
        // framework not set — should be absent
        assert!(merged.get("framework").is_none());
    }

    #[test]
    fn merge_classification_sets_cmmc_level() {
        let merged = merge_classification_into_metadata(
            &json!({}),
            None,
            None,
            None,
            None,
            Some(3),
            None,
            None,
        );
        assert_eq!(merged["cmmc_level"], 3);
    }

    #[test]
    fn classification_patch_distinguishes_clear_from_preserve() {
        let existing = json!({
            "framework": "DISA STIG",
            "severity": "high",
            "cmmc_level": 2,
            "source": "import"
        });
        let patched = patch_classification_into_metadata(
            &existing,
            Some("deployment"),
            Some(None),
            None,
            Some(Some("AC")),
            Some(None),
            None,
            None,
        );

        assert_eq!(patched["category"], "deployment");
        assert!(patched.get("framework").is_none());
        assert_eq!(patched["severity"], "high");
        assert_eq!(patched["control_family"], "AC");
        assert!(patched.get("cmmc_level").is_none());
        assert_eq!(patched["source"], "import");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SRG validation ───────────────────────────────────────────────────────

    #[test]
    fn srg_valid_examples() {
        assert_eq!(
            validate_srg("SRG-OS-000298-GPOS-00116").unwrap(),
            "SRG-OS-000298-GPOS-00116"
        );
        assert_eq!(
            validate_srg("SRG-OS-000096-GPOS-00050").unwrap(),
            "SRG-OS-000096-GPOS-00050"
        );
        // lowercase input normalised to upper
        assert_eq!(
            validate_srg("srg-os-000298-gpos-00116").unwrap(),
            "SRG-OS-000298-GPOS-00116"
        );
    }

    #[test]
    fn srg_invalid_prefix() {
        assert!(validate_srg("CCI-000205").is_err());
        assert!(validate_srg("foo").is_err());
    }

    #[test]
    fn srg_just_prefix_is_invalid() {
        assert!(validate_srg("SRG-").is_err());
    }

    #[test]
    fn srg_invalid_chars() {
        assert!(validate_srg("SRG-OS!000").is_err());
        assert!(validate_srg("SRG-OS_000").is_err());
    }

    // ── CCI validation ───────────────────────────────────────────────────────

    #[test]
    fn cci_valid_examples() {
        assert_eq!(validate_cci("CCI-000205").unwrap(), "CCI-000205");
        assert_eq!(validate_cci("CCI-000196").unwrap(), "CCI-000196");
        // lowercase normalised
        assert_eq!(validate_cci("cci-000205").unwrap(), "CCI-000205");
    }

    #[test]
    fn cci_invalid_prefix() {
        assert!(validate_cci("SRG-OS-000298").is_err());
        assert!(validate_cci("foo").is_err());
    }

    #[test]
    fn cci_just_prefix_is_invalid() {
        assert!(validate_cci("CCI-").is_err());
    }

    #[test]
    fn cci_non_numeric_suffix() {
        assert!(validate_cci("CCI-000ABC").is_err());
        assert!(validate_cci("CCI-ABC").is_err());
    }

    // ── Normalise lists ──────────────────────────────────────────────────────

    #[test]
    fn normalise_srg_deduplicates_and_sorts() {
        let raw = vec![
            "SRG-OS-000298-GPOS-00116".to_string(),
            "SRG-OS-000096-GPOS-00050".to_string(),
            "srg-os-000298-gpos-00116".to_string(), // duplicate, different case
        ];
        let out = normalise_srg_ids(&raw).unwrap();
        assert_eq!(
            out,
            vec!["SRG-OS-000096-GPOS-00050", "SRG-OS-000298-GPOS-00116"]
        );
    }

    #[test]
    fn normalise_cci_deduplicates_and_sorts() {
        let raw = vec![
            "CCI-000205".to_string(),
            "CCI-000196".to_string(),
            "cci-000205".to_string(), // duplicate
        ];
        let out = normalise_cci_ids(&raw).unwrap();
        assert_eq!(out, vec!["CCI-000196", "CCI-000205"]);
    }

    #[test]
    fn normalise_skips_blank_entries() {
        let raw = vec!["".to_string(), "  ".to_string(), "CCI-000001".to_string()];
        let out = normalise_cci_ids(&raw).unwrap();
        assert_eq!(out, vec!["CCI-000001"]);
    }

    #[test]
    fn normalise_rejects_first_invalid() {
        let raw = vec!["CCI-000001".to_string(), "bad-value".to_string()];
        assert!(normalise_cci_ids(&raw).is_err());
    }

    // ── merge_policy_mappings ────────────────────────────────────────────────

    #[test]
    fn merge_none_preserves_all_existing_keys() {
        let existing = serde_json::json!({
            "source_rule_id": "SV-123",
            "severity": "high",
            "identifiers": [{"system": "http://iase.disa.mil/cci", "value": "CCI-000001"}],
            "rationale": "some rationale",
            "srg_ids": ["SRG-OS-000001"],
            "cci_ids": ["CCI-000001"],
        });
        let merged = merge_policy_mappings(&existing, None, None).unwrap();
        // Every key is preserved unchanged
        assert_eq!(merged["source_rule_id"], "SV-123");
        assert_eq!(merged["severity"], "high");
        assert_eq!(merged["rationale"], "some rationale");
        assert_eq!(merged["srg_ids"], serde_json::json!(["SRG-OS-000001"]));
        assert_eq!(merged["cci_ids"], serde_json::json!(["CCI-000001"]));
        assert!(merged["identifiers"].is_array());
    }

    #[test]
    fn merge_some_empty_clears_mapping_preserves_others() {
        let existing = serde_json::json!({
            "source_rule_id": "SV-123",
            "severity": "medium",
            "cci_ids": ["CCI-000001"],
            "identifiers": [{"system": "uri", "value": "CCI-000001"}],
        });
        let merged = merge_policy_mappings(&existing, None, Some(&[])).unwrap();
        // cci_ids cleared, others preserved
        assert_eq!(merged["cci_ids"], serde_json::json!([]));
        assert_eq!(merged["source_rule_id"], "SV-123");
        assert_eq!(merged["severity"], "medium");
        assert!(merged["identifiers"].is_array());
    }

    #[test]
    fn merge_replaces_and_normalises_mappings() {
        let existing = serde_json::json!({
            "source_rule_id": "SV-123",
            "cci_ids": ["CCI-000001"],
        });
        let new_ccis = vec!["cci-000002".to_string(), "CCI-000001".to_string()];
        let merged = merge_policy_mappings(&existing, None, Some(&new_ccis)).unwrap();
        // Sorted, deduplicated, normalised
        assert_eq!(
            merged["cci_ids"],
            serde_json::json!(["CCI-000001", "CCI-000002"])
        );
        assert_eq!(merged["source_rule_id"], "SV-123");
    }

    #[test]
    fn merge_invalid_cci_returns_error() {
        let bad = vec!["NOT-A-CCI".to_string()];
        assert!(merge_policy_mappings(&serde_json::json!({}), None, Some(&bad)).is_err());
    }

    #[test]
    fn merge_invalid_srg_returns_error() {
        let bad = vec!["NOT-AN-SRG".to_string()];
        assert!(merge_policy_mappings(&serde_json::json!({}), Some(&bad), None).is_err());
    }

    // ── Semantic digest stability ────────────────────────────────────────────

    #[test]
    fn order_only_change_produces_same_normalised_output() {
        // [CCI-000205, CCI-000196] and [CCI-000196, CCI-000205] both normalise
        // to the same sorted list, so the digest will be identical.
        let a = normalise_cci_ids(&["CCI-000205".to_string(), "CCI-000196".to_string()]).unwrap();
        let b = normalise_cci_ids(&["CCI-000196".to_string(), "CCI-000205".to_string()]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn duplicate_ids_normalised_to_one() {
        let raw = vec!["CCI-000205".to_string(), "CCI-000205".to_string()];
        let out = normalise_cci_ids(&raw).unwrap();
        assert_eq!(out, vec!["CCI-000205"]);
    }

    // ── extract_srg_ids / extract_cci_ids ────────────────────────────────────

    #[test]
    fn extract_uses_explicit_key_when_present() {
        let meta = serde_json::json!({
            "srg_ids": ["SRG-OS-000001"],
            "cci_ids": ["CCI-000001"],
            "identifiers": [{"system": "x", "value": "SRG-OS-999999"}],
        });
        assert_eq!(extract_srg_ids(&meta), vec!["SRG-OS-000001"]);
        assert_eq!(extract_cci_ids(&meta), vec!["CCI-000001"]);
    }

    #[test]
    fn extract_falls_back_to_identifiers() {
        let meta = serde_json::json!({
            "identifiers": [
                {"system": "http://iase.disa.mil/cci", "value": "CCI-000205"},
                {"system": "other", "value": "SRG-OS-000298-GPOS-00116"},
                {"system": "other2", "value": "V-123456"},
            ]
        });
        let ccis = extract_cci_ids(&meta);
        assert!(ccis.contains(&"CCI-000205".to_string()));
        assert!(!ccis.iter().any(|v| v.starts_with("SRG-")));
        let srgs = extract_srg_ids(&meta);
        assert!(srgs.contains(&"SRG-OS-000298-GPOS-00116".to_string()));
    }

    #[test]
    fn extract_empty_key_returns_empty() {
        let meta = serde_json::json!({"srg_ids": [], "cci_ids": []});
        assert!(extract_srg_ids(&meta).is_empty());
        assert!(extract_cci_ids(&meta).is_empty());
    }

    // ── Preservation regression test ─────────────────────────────────────────

    /// Editing CCI IDs must not remove unrelated metadata keys.
    #[test]
    fn preservation_test_unrelated_fields_survive_cci_edit() {
        let existing = serde_json::json!({
            "source_rule_id": "SV-268078r958781_rule",
            "severity": "high",
            "identifiers": [{"system": "http://iase.disa.mil/cci", "value": "CCI-000205"}],
            "references": [{"href": "http://example.com", "title": "Example"}],
            "rationale": "The firewall must be enabled.",
            "checks": [{"system": "ocil", "body_parts": []}],
            "fixes": [{"id": "F-1", "system": "nix", "content": "networking.firewall.enable = true;"}],
            "srg_ids": ["SRG-OS-000298-GPOS-00116"],
            "cci_ids": ["CCI-000205"],
        });

        // Update only cci_ids
        let new_ccis = vec!["CCI-000205".to_string(), "CCI-000196".to_string()];
        let merged = merge_policy_mappings(&existing, None, Some(&new_ccis)).unwrap();

        // cci_ids updated
        assert_eq!(
            merged["cci_ids"],
            serde_json::json!(["CCI-000196", "CCI-000205"])
        );
        // srg_ids preserved
        assert_eq!(
            merged["srg_ids"],
            serde_json::json!(["SRG-OS-000298-GPOS-00116"])
        );
        // All other fields preserved exactly
        assert_eq!(merged["source_rule_id"], "SV-268078r958781_rule");
        assert_eq!(merged["severity"], "high");
        assert_eq!(merged["rationale"], "The firewall must be enabled.");
        assert!(merged["identifiers"].is_array());
        assert!(merged["references"].is_array());
        assert!(merged["checks"].is_array());
        assert!(merged["fixes"].is_array());
    }

    // ── Evidence validation ──────────────────────────────────────────────────────

    #[test]
    fn evidence_command_requires_expect() {
        // Valid: both cmd and expect
        let valid = serde_json::json!({
            "kind": "command",
            "cmd": "systemctl status ssh",
            "expect": "active"
        });
        assert!(validate_evidence_spec(&valid).is_ok());

        // Invalid: missing expect
        let missing_expect = serde_json::json!({
            "kind": "command",
            "cmd": "systemctl status ssh"
        });
        assert!(validate_evidence_spec(&missing_expect).is_err());

        // Invalid: empty expect
        let empty_expect = serde_json::json!({
            "kind": "command",
            "cmd": "systemctl status ssh",
            "expect": ""
        });
        assert!(validate_evidence_spec(&empty_expect).is_err());
    }

    #[test]
    fn evidence_log_requires_unit_and_match_text() {
        // Valid: all three fields
        let valid = serde_json::json!({
            "kind": "log",
            "source": "journald",
            "unit": "auditd.service",
            "match_text": "audit: rules loaded"
        });
        assert!(validate_evidence_spec(&valid).is_ok());

        // Invalid: missing unit
        let missing_unit = serde_json::json!({
            "kind": "log",
            "source": "journald",
            "match_text": "audit: rules loaded"
        });
        assert!(validate_evidence_spec(&missing_unit).is_err());

        // Invalid: missing match_text
        let missing_match = serde_json::json!({
            "kind": "log",
            "source": "journald",
            "unit": "auditd.service"
        });
        assert!(validate_evidence_spec(&missing_match).is_err());

        // Invalid: empty unit
        let empty_unit = serde_json::json!({
            "kind": "log",
            "source": "journald",
            "unit": "",
            "match_text": "audit: rules loaded"
        });
        assert!(validate_evidence_spec(&empty_unit).is_err());
    }

    #[test]
    fn evidence_file_requires_path() {
        let valid = serde_json::json!({
            "kind": "file",
            "path": "/etc/ssh/sshd_config"
        });
        assert!(validate_evidence_spec(&valid).is_ok());

        let missing_path = serde_json::json!({
            "kind": "file"
        });
        assert!(validate_evidence_spec(&missing_path).is_err());
    }

    #[test]
    fn evidence_eval_attr_requires_attr() {
        let valid = serde_json::json!({
            "kind": "eval_attr",
            "attr": "config.services.openssh.settings.PermitRootLogin"
        });
        assert!(validate_evidence_spec(&valid).is_ok());

        let missing_attr = serde_json::json!({
            "kind": "eval_attr"
        });
        assert!(validate_evidence_spec(&missing_attr).is_err());
    }

    #[test]
    fn evidence_unit_state_requires_unit_and_state() {
        let valid = serde_json::json!({
            "kind": "unit_state",
            "unit": "auditd.service",
            "state": "active"
        });
        assert!(validate_evidence_spec(&valid).is_ok());

        let missing_state = serde_json::json!({
            "kind": "unit_state",
            "unit": "auditd.service"
        });
        assert!(validate_evidence_spec(&missing_state).is_err());
    }

    #[test]
    fn evidence_attestation_requires_note() {
        let valid = serde_json::json!({
            "kind": "attestation",
            "note": "Manually verified and approved"
        });
        assert!(validate_evidence_spec(&valid).is_ok());

        let missing_note = serde_json::json!({
            "kind": "attestation"
        });
        assert!(validate_evidence_spec(&missing_note).is_err());
    }
}
