//! Load exported Crystal Forge artifacts into the generator's domain model.
//!
//! Every input is treated as untrusted. Parsing is delegated entirely to the
//! shared `cf-compliance` interchange layer:
//!
//! * policy JSON/TOML goes through [`cf_compliance::policy_document`], which
//!   applies the server's normalization, compatibility, and digest rules,
//!   followed by the shared policy configuration validator;
//! * XCCDF XML/ZIP goes through [`cf_compliance::xccdf::package`] (DTDs,
//!   external entities, network retrieval, and archive attacks disabled) and
//!   then [`cf_compliance::xccdf::importer::validate_cf_native_document`],
//!   which verifies every per-rule and whole-bundle semantic digest.
//!
//! No Nix contained in an export is evaluated. Policy validation uses the
//! same bounded `nix-instantiate --parse` syntax check as server imports, so
//! `nix-instantiate` must be available on `PATH` for custom expressions.

use std::fmt;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use cf_compliance::interchange::InterchangeLimits;
use cf_compliance::policy_document::{parse_policy_document, validate_policy_interchange_document};
use cf_compliance::xccdf::importer::validate_cf_native_document;
use cf_compliance::xccdf::models::DocumentClass;
use cf_compliance::xccdf::package::{ProcessingError, process_xccdf_bytes};
use cf_compliance::xccdf::zip_extractor::{PackageKind, detect_package_kind};
use sha2::{Digest, Sha256};

use crate::model::{PolicyOrigin, ResolvedBundle, ResolvedPolicy};

/// Everything one input artifact contributed.
#[derive(Debug, Clone)]
pub struct LoadedInput {
    pub label: String,
    /// SHA-256 of the artifact the user actually supplied. For a ZIP package
    /// this identifies the archive, not the XML member selected from it.
    pub source_sha256: String,
    /// SHA-256 of the XCCDF member selected from a ZIP package, when the input
    /// was an archive. `None` for direct XML and policy documents.
    pub selected_xml_sha256: Option<String>,
    pub bundle: Option<ResolvedBundle>,
    pub policies: Vec<ResolvedPolicy>,
}

/// A failure to load one input artifact.
#[derive(Debug, Clone)]
pub struct InputError {
    pub label: String,
    pub message: String,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.label, self.message)
    }
}

impl std::error::Error for InputError {}

impl InputError {
    fn new(label: &str, message: impl Into<String>) -> Self {
        Self {
            label: label.to_string(),
            message: message.into(),
        }
    }
}

/// Detected artifact kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// A policy-set or single-policy JSON/TOML export.
    PolicyDocument,
    /// An XCCDF 1.2 benchmark, optionally packaged in a ZIP.
    XccdfBundle,
}

/// Classify an artifact from its bytes and filename.
///
/// Content is authoritative: an XML or ZIP magic prefix selects the XCCDF path
/// regardless of extension, so a mislabelled file cannot be routed to the
/// wrong parser.
pub fn detect_input_kind(bytes: &[u8], label: &str) -> InputKind {
    let leading = bytes
        .iter()
        .copied()
        .skip_while(|b| b.is_ascii_whitespace() || *b == 0xEF || *b == 0xBB || *b == 0xBF)
        .take(1)
        .next();

    if leading == Some(b'<') || bytes.starts_with(b"PK\x03\x04") {
        return InputKind::XccdfBundle;
    }

    let lowered = label.to_ascii_lowercase();
    if lowered.ends_with(".xml") || lowered.ends_with(".zip") {
        return InputKind::XccdfBundle;
    }

    InputKind::PolicyDocument
}

/// Loads an exported artifact from a bounded byte slice.
///
/// Plain policy documents and XML share the interchange plain-document limit
/// (10 MiB); ZIP packages use its archive limit (50 MiB). Size is checked before
/// hashing, parsing, or cloning the input for XCCDF processing.
///
/// # Errors
/// Returns an error for oversized input, invalid policy configuration or
/// interchange metadata, unsupported bundle types, or mutable lifecycle state.
pub fn load_input(bytes: &[u8], label: &str) -> Result<LoadedInput, InputError> {
    check_input_size(bytes.len() as u64, input_byte_limit(bytes), label)?;
    match detect_input_kind(bytes, label) {
        InputKind::PolicyDocument => load_policy_document(bytes, label),
        InputKind::XccdfBundle => load_xccdf_bundle(bytes, label),
    }
}

/// Loads an exported file without allocating an unbounded input buffer.
///
/// Uses the same size limits as [`load_input`], selected from the file's content
/// signature rather than its extension. Metadata rejects oversized regular
/// files before accumulation. A bounded read also handles growth after the
/// metadata check and files whose metadata does not report their full length.
///
/// # Errors
/// Returns an error for file I/O failures and all errors from [`load_input`].
pub fn load_input_file(path: &Path) -> Result<LoadedInput, InputError> {
    let label = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let file = std::fs::File::open(path)
        .map_err(|error| InputError::new(&label, format!("could not read input: {error}")))?;
    let length = file
        .metadata()
        .map_err(|error| InputError::new(&label, format!("could not inspect input: {error}")))?
        .len();
    let mut reader = BufReader::new(file);
    let prefix = reader
        .fill_buf()
        .map_err(|error| InputError::new(&label, format!("could not read input: {error}")))?;
    let maximum = input_byte_limit(prefix);
    check_input_size(length, maximum, &label)?;
    let mut bytes = Vec::new();
    reader
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| InputError::new(&label, format!("could not read input: {error}")))?;
    check_input_size(bytes.len() as u64, maximum, &label)?;
    load_input(&bytes, &label)
}

fn input_byte_limit(prefix: &[u8]) -> usize {
    let limits = InterchangeLimits::default();
    if detect_package_kind(prefix) == Some(PackageKind::Zip) {
        limits.max_zip_bytes
    } else {
        // Reuse the existing plain interchange document bound for JSON/TOML.
        limits.max_xml_bytes
    }
}

fn check_input_size(length: u64, maximum: usize, label: &str) -> Result<(), InputError> {
    if length > maximum as u64 {
        return Err(InputError::new(
            label,
            format!("input is {length} bytes, exceeding the {maximum} byte limit"),
        ));
    }
    Ok(())
}

fn source_digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn load_policy_document(bytes: &[u8], label: &str) -> Result<LoadedInput, InputError> {
    let source_sha256 = source_digest(bytes);

    // Supplying the source digest makes compatibility identities deterministic,
    // so a document without explicit portable IDs still generates identical
    // output on every run.
    let normalized = parse_policy_document(bytes, Some(label), Some(&source_sha256))
        .map_err(|message| InputError::new(label, message))?;
    for policy in &normalized {
        if !matches!(policy.publication_state.as_str(), "accepted" | "deprecated") {
            return Err(InputError::new(
                label,
                format!(
                    "policy version {} has publication_state '{}'; standalone generation requires an immutable accepted or deprecated policy version",
                    policy.version_id, policy.publication_state
                ),
            ));
        }
    }
    validate_policy_interchange_document(&normalized)
        .map_err(|message| InputError::new(label, message))?;
    for policy in &normalized {
        let composite = cf_compliance::policy_config::validate_policy_type_config(
            &policy.policy_type,
            &policy.config,
        )
        .map_err(|message| {
            InputError::new(
                label,
                format!("policy version {}: {message}", policy.version_id),
            )
        })?;
        if composite.is_some() && policy.execution_phase != "multi-phase" {
            return Err(InputError::new(
                label,
                format!(
                    "composite policy version {} must use execution_phase multi-phase",
                    policy.version_id
                ),
            ));
        }
    }

    let policies = normalized
        .into_iter()
        .map(|policy| ResolvedPolicy {
            policy_id: policy.lineage_id,
            policy_version_id: policy.version_id,
            version: policy.version,
            publication_state: policy.publication_state,
            name: policy.name,
            description: policy.description,
            policy_type: policy.policy_type,
            implementation_state: policy.implementation_state,
            execution_phase: policy.execution_phase,
            config: policy.config,
            compliance_metadata: policy.compliance_metadata,
            semantic_digest: policy.semantic_digest,
            origin: PolicyOrigin::new(label.to_string(), source_sha256.clone(), None),
        })
        .collect();

    Ok(LoadedInput {
        label: label.to_string(),
        source_sha256,
        selected_xml_sha256: None,
        bundle: None,
        policies,
    })
}

fn load_xccdf_bundle(bytes: &[u8], label: &str) -> Result<LoadedInput, InputError> {
    let limits = InterchangeLimits::default();
    let processed = process_xccdf_bytes(bytes.to_vec(), Some(label.to_string()), &limits)
        .map_err(|error| InputError::new(label, describe_processing_error(&error)))?;

    // `provenance.sha256` is the digest of the artifact the user supplied; for
    // a ZIP that is the archive itself. `parsed.source_sha256` is only the
    // digest of the selected XML member, so it must not be reported as the
    // source export digest.
    let source_sha256 = processed.provenance.sha256.clone();
    let selected_xml_sha256 = processed.provenance.selected_xml_sha256.clone();
    let parsed = processed.parsed;

    if !matches!(parsed.class, DocumentClass::CfNativeExact) {
        return Err(InputError::new(
            label,
            format!(
                "only CF-native XCCDF bundle exports can be converted; this document classified as {:?}. \
                 A foreign STIG or XCCDF benchmark must first be imported into Crystal Forge and exported as a bundle version.",
                parsed.class
            ),
        ));
    }

    let bundle_meta = parsed
        .cf_bundle_meta
        .as_ref()
        .ok_or_else(|| InputError::new(label, "CF-native document is missing bundle metadata"))?;

    if bundle_meta.publication_state != "accepted" {
        return Err(InputError::new(
            label,
            format!(
                "bundle version {} has publication_state '{}'; standalone generation requires an accepted bundle version",
                bundle_meta.bundle_version_id, bundle_meta.publication_state
            ),
        ));
    }
    for rule in &parsed.rules {
        let Some(meta) = &rule.cf_policy_meta else {
            continue;
        };
        if !matches!(meta.publication_state.as_str(), "accepted" | "deprecated") {
            return Err(InputError::new(
                label,
                format!(
                    "policy version {} has publication_state '{}'; standalone generation requires an immutable accepted or deprecated policy version",
                    meta.policy_version_id, meta.publication_state
                ),
            ));
        }
    }

    // Authoritative CF-native validation: verifies bundle and per-rule semantic
    // digests, portable identities, digest contract, and membership ordering.
    let (_validated, records) = validate_cf_native_document(&parsed)
        .map_err(|error| InputError::new(label, format!("{error:?}")))?;

    // Use the exact membership this immutable bundle version froze. Ordering
    // comes from `policy_order`, never from document order or "latest".
    let mut membership: Vec<_> = records
        .iter()
        .filter(|record| record.selected)
        .map(|record| (record.policy_order, record.policy_version_id))
        .collect();
    membership.sort_by_key(|(order, version_id)| (*order, *version_id));
    let selected_policy_version_ids = membership.iter().map(|(_, id)| *id).collect();

    let bundle = ResolvedBundle {
        bundle_id: bundle_meta.bundle_id,
        bundle_version_id: bundle_meta.bundle_version_id,
        name: parsed
            .benchmark
            .as_ref()
            .and_then(|benchmark| benchmark.title.clone())
            .unwrap_or_else(|| bundle_meta.bundle_id.to_string()),
        version: parsed
            .benchmark
            .as_ref()
            .and_then(|benchmark| benchmark.version.clone())
            .unwrap_or_else(|| "1".into()),
        framework: bundle_meta.framework.clone(),
        framework_version: bundle_meta.framework_version.clone(),
        publication_state: bundle_meta.publication_state.clone(),
        semantic_digest: bundle_meta.digest.clone(),
        source_sha256s: vec![source_sha256.clone()],
        input_labels: vec![label.to_string()],
        selected_policy_version_ids,
    };

    let policies = records
        .into_iter()
        .filter(|record| record.selected)
        .map(|record| ResolvedPolicy {
            policy_id: record.policy_id,
            policy_version_id: record.policy_version_id,
            version: record.version.unwrap_or_else(|| "0.1.0".into()),
            publication_state: record
                .publication_state
                .clone()
                .unwrap_or_else(|| "accepted".into()),
            name: record.name,
            description: record.description,
            policy_type: record.policy_type,
            implementation_state: record.implementation_state,
            execution_phase: record.execution_phase,
            config: record.config,
            compliance_metadata: record.compliance_metadata,
            semantic_digest: record.semantic_digest.unwrap_or_default(),
            origin: PolicyOrigin::new(
                label.to_string(),
                source_sha256.clone(),
                Some(bundle_meta.bundle_version_id),
            ),
        })
        .collect();

    Ok(LoadedInput {
        label: label.to_string(),
        source_sha256,
        selected_xml_sha256,
        bundle: Some(bundle),
        policies,
    })
}

fn describe_processing_error(error: &ProcessingError) -> String {
    match error {
        ProcessingError::UnknownContentType => {
            "content is neither XCCDF XML nor a ZIP package".to_string()
        }
        ProcessingError::ContentExtensionMismatch { reason } => {
            format!("file extension contradicts its content: {reason}")
        }
        ProcessingError::TooLarge {
            subject,
            actual,
            maximum,
        } => format!("{subject} is {actual} bytes, which exceeds the {maximum} byte limit"),
        ProcessingError::ZipExtraction { code, message, .. } => {
            format!("ZIP extraction failed ({code}): {message}")
        }
        ProcessingError::BlockingDiagnostics { parsed } => {
            let details = parsed
                .errors
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.summary))
                .collect::<Vec<_>>()
                .join("; ");
            format!("document has blocking validation errors: {details}")
        }
        other => format!("could not process the document: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_xml_by_content_even_with_a_json_extension() {
        assert_eq!(
            detect_input_kind(b"<?xml version=\"1.0\"?><Benchmark/>", "bundle.json"),
            InputKind::XccdfBundle
        );
    }

    #[test]
    fn detects_zip_by_magic_bytes() {
        assert_eq!(
            detect_input_kind(b"PK\x03\x04rest", "package.bin"),
            InputKind::XccdfBundle
        );
    }

    #[test]
    fn detects_json_policy_documents() {
        assert_eq!(
            detect_input_kind(b"{\"policies\":[]}", "policy-set.json"),
            InputKind::PolicyDocument
        );
    }

    #[test]
    fn oversized_in_memory_input_is_rejected_before_parsing_or_cloning() {
        let maximum = InterchangeLimits::default().max_xml_bytes;
        let bytes = vec![b'<'; maximum + 1];
        let error = load_input(&bytes, "large.xml").expect_err("reject before XCCDF processing");
        assert!(
            error
                .message
                .contains(&format!("exceeding the {maximum} byte limit")),
            "{error}"
        );
        assert_eq!(
            input_byte_limit(b"PK\x03\x04"),
            InterchangeLimits::default().max_zip_bytes
        );
    }

    #[test]
    fn detects_toml_policy_documents() {
        assert_eq!(
            detect_input_kind(b"[[policies]]\n", "policy-set.toml"),
            InputKind::PolicyDocument
        );
    }

    #[test]
    fn loads_a_policy_set_document() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema": "urn:crystal-forge:policy-set:1",
            "policies": [{
                "lineage_id": "11111111-1111-1111-1111-111111111111",
                "version_id": "22222222-2222-2222-2222-222222222222",
                "publication_state": "accepted",
                "name": "firewall",
                "policy_type": "custom_check",
                "config": {"expression": "config.networking.firewall.enable == true"},
            }],
        }))
        .expect("serialize");
        let loaded = load_input(&bytes, "policy-set.json").expect("loads");
        assert!(loaded.bundle.is_none());
        assert_eq!(loaded.policies.len(), 1);
        assert_eq!(loaded.policies[0].name, "firewall");
        assert_eq!(loaded.source_sha256.len(), 64);
    }

    #[test]
    fn direct_exports_reject_invalid_custom_check_configuration() {
        let expression = "config.networking.firewall.enable == true";
        for (config, expected) in [
            (
                serde_json::json!({"rules": [{"expression": expression}]}),
                "field_name",
            ),
            (
                serde_json::json!({"expression": expression, "strict": "false"}),
                "strict",
            ),
            (
                serde_json::json!({"rules": [{"expression": expression, "field_name": "firewall", "strict": 1}]}),
                "strict",
            ),
            (
                serde_json::json!({"expression": expression, "rules": {}}),
                "rules",
            ),
            (
                serde_json::json!({"mode": "invalid", "rules": [{"expression": expression, "field_name": "firewall"}]}),
                "mode",
            ),
        ] {
            // Each invalid configuration still contains a convertible assertion.
            // The shared validator, not extraction, must reject its metadata.
            cf_compliance::policy_config::validate_policy_type_config("custom_check", &config)
                .expect_err("authoritative validator rejects this configuration");
            let document = serde_json::json!({
                "name": "invalid-native-policy",
                "publication_state": "accepted",
                "policy_type": "custom_check",
                "config": config,
            });
            for (label, bytes) in [
                ("policy.json", serde_json::to_vec(&document).unwrap()),
                (
                    "policy.toml",
                    toml::to_string(&document).unwrap().into_bytes(),
                ),
            ] {
                let error = load_input(&bytes, label)
                    .expect_err("invalid config must not generate assignments");
                assert!(error.message.contains(expected), "{label}: {error}");
            }
        }
    }

    #[test]
    fn direct_exports_with_valid_rule_metadata_still_generate_assignments() {
        let document = serde_json::json!({
            "name": "valid-native-policy", "publication_state": "accepted",
            "policy_type": "custom_check",
            "config": {"rules": [{
                "expression": "config.networking.firewall.enable == true",
                "field_name": "firewall", "strict": true,
            }]},
        });
        for (label, bytes) in [
            ("policy.json", serde_json::to_vec(&document).unwrap()),
            (
                "policy.toml",
                toml::to_string(&document).unwrap().into_bytes(),
            ),
        ] {
            let loaded = load_input(&bytes, label).expect("valid configuration");
            let selection = crate::select::select_policies(&[loaded]).unwrap();
            let generated = crate::generate::generate(&selection, "valid").unwrap();
            assert!(generated.skipped.is_empty());
            assert_eq!(generated.implemented.len(), 1);
            assert_eq!(generated.implemented[0].assignments.len(), 1);
        }
    }

    #[test]
    fn identical_bytes_produce_identical_identities() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "policies": [{
                "name": "no-portable-ids",
                "publication_state": "accepted",
                "policy_type": "custom_check",
                "config": {"expression": "config.a.b == true"},
            }],
        }))
        .expect("serialize");
        let first = load_input(&bytes, "p.json").expect("loads");
        let second = load_input(&bytes, "p.json").expect("loads");
        assert_eq!(
            first.policies[0].policy_version_id,
            second.policies[0].policy_version_id
        );
    }

    #[test]
    fn malformed_json_is_rejected() {
        let error = load_input(b"{not json", "p.json").expect_err("must fail");
        assert!(error.message.contains("invalid"), "got: {}", error.message);
    }

    #[test]
    fn a_tampered_policy_digest_is_rejected() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "policies": [{
                "name": "tampered",
                "publication_state": "accepted",
                "policy_type": "custom_check",
                "config": {"expression": "config.a.b == true"},
                "semantic_digest": "0".repeat(64),
            }],
        }))
        .expect("serialize");
        let error = load_input(&bytes, "p.json").expect_err("must fail");
        assert!(
            error.message.contains("semantic_digest"),
            "got: {}",
            error.message
        );
    }

    #[test]
    fn mutable_direct_policy_export_is_rejected() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "policies": [{
                "name": "draft-policy",
                "publication_state": "draft",
                "policy_type": "custom_check",
                "config": {"expression": "config.a.b == true"},
            }],
        }))
        .expect("serialize");
        let error = load_input(&bytes, "draft.json").expect_err("draft must be rejected");
        assert!(error.message.contains("publication_state"), "{error}");
        assert!(error.message.contains("draft"), "{error}");
    }

    #[test]
    fn mutable_selected_policy_in_accepted_bundle_is_rejected() {
        let xml = crate::fixture::bundle_xccdf_xml().replace(
            "policy-version-id=\"urn:uuid:22222222-0000-0000-0000-0000000000a1\" publication-state=\"accepted\"",
            "policy-version-id=\"urn:uuid:22222222-0000-0000-0000-0000000000a1\" publication-state=\"draft\"",
        );
        assert!(xml.contains("<cf:bundle "));
        assert!(xml.contains("publication-state=\"accepted\""));
        let error = load_input(xml.as_bytes(), "draft-policy.xml")
            .expect_err("draft selected policy must fail");
        assert!(error.message.contains("publication_state"), "{error}");
        assert!(error.message.contains("draft"), "{error}");
        assert!(
            error
                .message
                .contains("22222222-0000-0000-0000-0000000000a1"),
            "{error}"
        );
    }

    #[test]
    fn a_non_cf_native_xccdf_document_is_rejected_with_guidance() {
        let foreign = br#"<?xml version="1.0" encoding="UTF-8"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="xccdf_mil.disa_benchmark_test">
  <status>accepted</status>
  <title>Foreign benchmark</title>
  <version>1</version>
</Benchmark>"#;
        let error = load_input(foreign, "foreign.xml").expect_err("must fail");
        assert!(
            error.message.contains("CF-native"),
            "got: {}",
            error.message
        );
    }

    #[test]
    fn empty_input_is_rejected() {
        let error = load_input(b"", "empty.xml").expect_err("must fail");
        assert!(!error.message.is_empty());
    }
}
