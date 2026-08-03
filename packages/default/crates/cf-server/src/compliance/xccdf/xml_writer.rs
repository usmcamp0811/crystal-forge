//! XCCDF 1.2 XML writer for CF-XCCDF bundle export.
//!
//! Produces a standards-valid XCCDF 1.2 Benchmark with Crystal Forge extension
//! elements embedded inside XCCDF `<metadata>` extension points. All XML text
//! is escaped automatically by quick-xml; no CDATA sections are constructed
//! via string concatenation.
//!
//! ## XCCDF 1.2 Rule content model (element sequence)
//!
//! Per the XCCDF 1.2 schema `ruleType` extends `selectableItemType`:
//! ```text
//! title, description?, warning*, question*, reference*, metadata?,
//! rationale?, platform*, requires*, conflicts*, ident*, impact-metric?,
//! profile-note*, fixtext*, fix*, (check | complex-check), signature?
//! ```
//!
//! CF extension elements (`cf:policy-identity`, `cf:policy`, `cf:source-mappings`)
//! are placed inside `<metadata>`, which accepts `xs:any` content. Standard
//! `<reference>` elements precede `<metadata>`, and `<ident>` elements follow it.
//!
//! ## XCCDF ID conventions
//!
//! All IDs use the underscore-segmented form required by the schema:
//! `xccdf_<namespace-word>_<type>_<suffix>`.
//!
//! ## CF-XCCDF `cf:bundle` attributes (required by cf-xccdf-1.xsd)
//!
//! `schema-version`, `bundle-id` (urn:uuid:…), `bundle-version-id` (urn:uuid:…),
//! `publication-state`. Children: `cf:framework?`, `cf:layer?`, `cf:owner?`,
//! `cf:content-digest` (required).

use std::collections::BTreeMap;
use std::io::Cursor;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

use super::super::canonical::{ImplementationState, PublicationState};
use super::super::interchange::{
    CANONICALIZATION_VERSION, CF_NIX_FIX_SYSTEM, CF_POLICY_CHECK_SYSTEM, CF_XCCDF_NAMESPACE,
    DIGEST_ALGORITHM, XCCDF_1_2_NAMESPACE,
};
use super::export_models::{
    XccdfBundleExport, XccdfCheckBody, XccdfGroupExport, XccdfPolicyExport, XccdfSourceMapping,
    XccdfStandardCheck,
};

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors returned by the XCCDF writer.
#[derive(Debug)]
pub enum XccdfWriterError {
    /// A required configuration field is absent or has the wrong type.
    MissingConfig {
        policy_type: String,
        field: &'static str,
    },
    /// An execution phase value is not in the CF schema enumeration.
    InvalidExecutionPhase {
        policy_version_id: uuid::Uuid,
        phase: String,
    },
    UnsupportedDigestMetadata {
        object: &'static str,
        algorithm: String,
        canonicalization_version: String,
    },
    /// An imported standard check object is structurally invalid.
    MalformedImportedCheck {
        policy_version_id: uuid::Uuid,
        reason: String,
    },
    /// An imported standard fix object is structurally invalid.
    MalformedImportedFix {
        policy_version_id: uuid::Uuid,
        reason: String,
    },
    /// A policy configuration could not be serialized without loss.
    Json(serde_json::Error),
    /// An I/O error from the underlying quick-xml writer.
    Io(std::io::Error),
}

impl std::fmt::Display for XccdfWriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingConfig { policy_type, field } => {
                write!(
                    f,
                    "policy type {policy_type:?} requires config field {field:?}"
                )
            }
            Self::InvalidExecutionPhase {
                policy_version_id,
                phase,
            } => {
                write!(
                    f,
                    "policy version {policy_version_id} has invalid execution phase {phase:?}; \
                     must be one of: nix-evaluation, post-build, pre-deployment, \
                     deployment-orchestration, continuous-assessment"
                )
            }
            Self::Io(e) => write!(f, "XML I/O error: {e}"),
            Self::Json(e) => write!(f, "JSON serialization error: {e}"),
            Self::UnsupportedDigestMetadata {
                object,
                algorithm,
                canonicalization_version,
            } => write!(
                f,
                "{object} digest uses unsupported algorithm {algorithm:?} or canonical model {canonicalization_version:?}"
            ),
            Self::MalformedImportedCheck {
                policy_version_id,
                reason,
            } => write!(
                f,
                "policy version {policy_version_id} has malformed compliance_metadata.check: {reason}"
            ),
            Self::MalformedImportedFix {
                policy_version_id,
                reason,
            } => write!(
                f,
                "policy version {policy_version_id} has malformed compliance_metadata.fix: {reason}"
            ),
        }
    }
}

impl std::error::Error for XccdfWriterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for XccdfWriterError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for XccdfWriterError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

// ── XCCDF executionPhase whitelist (from cf-xccdf-1.xsd) ─────────────────────

const VALID_EXECUTION_PHASES: &[&str] = &[
    "nix-evaluation",
    "post-build",
    "pre-deployment",
    "deployment-orchestration",
    "continuous-assessment",
];

fn validate_execution_phase(
    policy_version_id: uuid::Uuid,
    phase: &str,
) -> Result<(), XccdfWriterError> {
    if VALID_EXECUTION_PHASES.contains(&phase) {
        Ok(())
    } else {
        Err(XccdfWriterError::InvalidExecutionPhase {
            policy_version_id,
            phase: phase.to_owned(),
        })
    }
}

// ── publication_state_str ─────────────────────────────────────────────────────

/// Map [`PublicationState`] to the exact string accepted by the CF-XCCDF schema
/// and used as the XCCDF `<status>` value.
///
/// The CF schema enumerates: `incomplete`, `draft`, `interim`, `accepted`,
/// `deprecated`.  XCCDF 1.2 `<status>` also accepts `draft`, `interim`,
/// `accepted`, `deprecated`; `incomplete` is passed through and rejected by
/// schema validators, which is correct — an incomplete bundle version should
/// never be exported.
fn publication_state_str(s: PublicationState) -> &'static str {
    match s {
        PublicationState::Incomplete => "incomplete",
        PublicationState::Draft => "draft",
        PublicationState::Interim => "interim",
        PublicationState::Accepted => "accepted",
        PublicationState::Deprecated => "deprecated",
    }
}

/// XCCDF 1.2 does not define `incomplete` as a Benchmark status. The exact
/// CF lifecycle state is retained on `cf:bundle`; only the standard status
/// needs this compatibility projection.
fn xccdf_status_str(s: PublicationState) -> &'static str {
    match s {
        PublicationState::Incomplete => "draft",
        other => publication_state_str(other),
    }
}

fn implementation_state_str(s: ImplementationState) -> &'static str {
    match s {
        ImplementationState::Native => "native",
        ImplementationState::Manual => "manual",
        ImplementationState::External => "external",
        ImplementationState::Unbound => "unbound",
        ImplementationState::Opaque => "opaque",
    }
}

// ── Severity and group helpers ────────────────────────────────────────────────

fn severity_for_type(policy_type: &str) -> &'static str {
    match policy_type {
        "require_cve_check" | "require_approvals" | "cve_threshold" => "high",
        _ => "medium",
    }
}

fn standard_severity(pv: &XccdfPolicyExport) -> &str {
    pv.compliance_metadata
        .get("severity")
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| severity_for_type(&pv.policy_type))
}

fn group_title_for_type(policy_type: &str) -> &'static str {
    match policy_type {
        "require_cf_agent" => "Crystal Forge Agent Requirement",
        "require_packages" => "Required Package Verification",
        "custom_check" => "Custom Policy Check",
        "require_cve_check" => "CVE Check Gate",
        "cve_threshold" => "CVE Threshold Gate",
        "time_window" => "Time Window Policy",
        "require_approvals" => "Approval Gate",
        "canary_rollout" => "Canary Rollout",
        _ => "Policy Group",
    }
}

fn group_id_for_type(policy_type: &str) -> String {
    let slug = policy_type.replace('_', "-");
    format!("xccdf_crystalforge_group_{slug}")
}

// ── Low-level writer helpers ──────────────────────────────────────────────────

fn el(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    name: &str,
    text: &str,
) -> Result<(), XccdfWriterError> {
    writer.write_event(Event::Start(BytesStart::new(name)))?;
    writer.write_event(Event::Text(BytesText::new(text)))?;
    writer.write_event(Event::End(BytesEnd::new(name)))?;
    Ok(())
}

fn cf_el(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    tag: &str,
    text: &str,
) -> Result<(), XccdfWriterError> {
    let full = format!("cf:{tag}");
    el(writer, &full, text)
}

fn cf_empty(writer: &mut Writer<Cursor<&mut Vec<u8>>>, tag: &str) -> Result<(), XccdfWriterError> {
    let full = format!("cf:{tag}");
    writer.write_event(Event::Empty(BytesStart::new(&full)))?;
    Ok(())
}

// ── CF extension sub-writers ──────────────────────────────────────────────────

fn write_source_mappings(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    mappings: &[XccdfSourceMapping],
) -> Result<(), XccdfWriterError> {
    if mappings.is_empty() {
        return Ok(());
    }
    writer.write_event(Event::Start(BytesStart::new("cf:source-mappings")))?;
    for m in mappings {
        writer.write_event(Event::Start(BytesStart::new("cf:source")))?;
        cf_el(writer, "object-kind", &m.object_kind)?;
        cf_el(writer, "source-identity", &m.source_identity)?;
        cf_el(writer, "fidelity", &m.fidelity)?;
        writer.write_event(Event::End(BytesEnd::new("cf:source")))?;
    }
    writer.write_event(Event::End(BytesEnd::new("cf:source-mappings")))?;
    Ok(())
}

fn write_policy_identity(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    let policy_urn = format!("urn:uuid:{}", pv.policy_id);
    let version_urn = format!("urn:uuid:{}", pv.policy_version_id);

    let mut elem = BytesStart::new("cf:policy-identity");
    elem.push_attribute(("policy-id", policy_urn.as_str()));
    elem.push_attribute(("policy-version-id", version_urn.as_str()));
    elem.push_attribute((
        "publication-state",
        publication_state_str(pv.publication_state),
    ));
    elem.push_attribute((
        "enabled-default",
        if pv.enabled_default { "true" } else { "false" },
    ));
    elem.push_attribute((
        "implementation-state",
        implementation_state_str(pv.implementation_state),
    ));
    elem.push_attribute(("selected", if pv.selected { "true" } else { "false" }));
    elem.push_attribute(("policy-order", pv.policy_order.to_string().as_str()));
    writer.write_event(Event::Start(elem))?;

    cf_el(writer, "policy-version", &pv.version)?;

    let mut digest = BytesStart::new("cf:content-digest");
    digest.push_attribute(("algorithm", pv.digest_algorithm.as_str()));
    digest.push_attribute(("canonical-model", pv.canonicalization_version.as_str()));
    writer.write_event(Event::Start(digest))?;
    writer.write_event(Event::Text(BytesText::new(&pv.semantic_digest)))?;
    writer.write_event(Event::End(BytesEnd::new("cf:content-digest")))?;

    writer.write_event(Event::End(BytesEnd::new("cf:policy-identity")))?;
    Ok(())
}

/// Write `<cf:policy>` for Native implementation state.
///
/// For Manual, External, Unbound, and Opaque states, no `<cf:policy>` is
/// emitted because CF cannot guarantee that the typed implementation content
/// model is accurate; the implementation state is captured in
/// `<cf:policy-identity>` via the surrounding XCCDF check text.
fn write_cf_policy(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    let mut policy = BytesStart::new("cf:policy");
    policy.push_attribute(("schema-version", "1"));
    writer.write_event(Event::Start(policy))?;

    let mut exec = BytesStart::new("cf:execution");
    exec.push_attribute(("phase", pv.execution_phase.as_str()));
    exec.push_attribute(("strict", "true"));
    writer.write_event(Event::Empty(exec))?;

    write_implementation(writer, pv)?;

    write_cf_dependencies(writer, pv)?;

    write_json_element(writer, "config-json", &pv.config)?;
    write_json_element(writer, "compliance-metadata-json", &pv.compliance_metadata)?;
    write_json_element(writer, "dependencies-json", &pv.dependencies)?;

    writer.write_event(Event::End(BytesEnd::new("cf:policy")))?;
    Ok(())
}

/// Write `<cf:implementation>` for a policy with `Native` implementation state.
///
/// Returns `XccdfWriterError::MissingConfig` when a required configuration
/// field is absent or has the wrong type rather than substituting a default.
fn write_implementation(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    let mut implementation = BytesStart::new("cf:implementation");
    implementation.push_attribute(("state", implementation_state_str(pv.implementation_state)));
    writer.write_event(Event::Start(implementation))?;

    if pv.implementation_state != ImplementationState::Native {
        cf_empty(writer, "unsupported")?;
        writer.write_event(Event::End(BytesEnd::new("cf:implementation")))?;
        return Ok(());
    }

    match pv.policy_type.as_str() {
        "require_cf_agent" => {
            cf_empty(writer, "require-crystal-forge-agent")?;
        }
        "require_packages" => {
            let pkgs = pv
                .config
                .get("packages")
                .and_then(|v| v.as_array())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "packages",
                })?;
            if pkgs.is_empty() {
                return Err(XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "packages (must contain at least one item)",
                });
            }
            writer.write_event(Event::Start(BytesStart::new("cf:require-packages")))?;
            for pkg in pkgs {
                let name = pkg
                    .as_str()
                    .ok_or_else(|| XccdfWriterError::MissingConfig {
                        policy_type: pv.policy_type.clone(),
                        field: "packages[] (must be strings)",
                    })?;
                cf_el(writer, "package", name)?;
            }
            writer.write_event(Event::End(BytesEnd::new("cf:require-packages")))?;
        }
        "custom_check" => {
            write_custom_check(writer, pv)?;
        }
        "require_cve_check" => {
            let max_crit = pv
                .config
                .get("max_critical")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "max_critical",
                })?;
            let req_just = pv
                .config
                .get("require_high_justification")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "require_high_justification",
                })?;
            let no_scan = pv
                .config
                .get("when_no_scan")
                .and_then(|v| v.as_str())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "when_no_scan",
                })?;
            let mut elem = BytesStart::new("cf:require-cve-check");
            elem.push_attribute(("max-critical", max_crit.to_string().as_str()));
            elem.push_attribute(("require-high-justification", req_just.to_string().as_str()));
            elem.push_attribute(("when-no-scan", no_scan));
            writer.write_event(Event::Start(elem))?;
            if let Some(max_high) = pv.config.get("max_high").and_then(|v| v.as_u64()) {
                cf_el(writer, "max-high", &max_high.to_string())?;
            }
            writer.write_event(Event::End(BytesEnd::new("cf:require-cve-check")))?;
        }
        "time_window" => {
            let start = pv
                .config
                .get("start_time")
                .and_then(|v| v.as_str())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "start_time",
                })?;
            let end = pv
                .config
                .get("end_time")
                .and_then(|v| v.as_str())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "end_time",
                })?;
            let tz = pv
                .config
                .get("timezone")
                .and_then(|v| v.as_str())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "timezone",
                })?;
            let action = pv
                .config
                .get("action")
                .and_then(|v| v.as_str())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "action",
                })?;
            let days = pv
                .config
                .get("days")
                .and_then(|v| v.as_array())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "days",
                })?;
            if days.is_empty() || days.len() > 7 {
                return Err(XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "days (must contain 1 to 7 items)",
                });
            }
            for day in days {
                let value = day
                    .as_str()
                    .ok_or_else(|| XccdfWriterError::MissingConfig {
                        policy_type: pv.policy_type.clone(),
                        field: "days[] (must be strings)",
                    })?;
                if !matches!(value, "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun") {
                    return Err(XccdfWriterError::MissingConfig {
                        policy_type: pv.policy_type.clone(),
                        field: "days[] (invalid weekday)",
                    });
                }
            }
            let mut elem = BytesStart::new("cf:time-window");
            elem.push_attribute(("start-time", start));
            elem.push_attribute(("end-time", end));
            elem.push_attribute(("timezone", tz));
            elem.push_attribute(("action", action));
            writer.write_event(Event::Start(elem))?;
            if let Some(desc) = pv.config.get("description").and_then(|v| v.as_str()) {
                cf_el(writer, "description", desc)?;
            }
            for day in days {
                if let serde_json::Value::String(d) = day {
                    cf_el(writer, "day", d)?;
                }
            }
            writer.write_event(Event::End(BytesEnd::new("cf:time-window")))?;
        }
        "require_approvals" => {
            let count = pv
                .config
                .get("count")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "count",
                })?;
            let role = pv
                .config
                .get("role")
                .and_then(|v| v.as_str())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "role",
                })?;
            let distinct = pv
                .config
                .get("distinct")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "distinct",
                })?;
            let mut elem = BytesStart::new("cf:require-approvals");
            if count == 0 {
                return Err(XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "count (must be positive)",
                });
            }
            elem.push_attribute(("count", count.to_string().as_str()));
            elem.push_attribute(("role", role));
            elem.push_attribute(("distinct", distinct.to_string().as_str()));
            writer.write_event(Event::Start(elem))?;
            if let Some(desc) = pv.config.get("description").and_then(|v| v.as_str()) {
                cf_el(writer, "description", desc)?;
            }
            if let Some(hours) = pv
                .config
                .get("expires_after_hours")
                .and_then(|v| v.as_u64())
            {
                cf_el(writer, "expires-after-hours", &hours.to_string())?;
            }
            writer.write_event(Event::End(BytesEnd::new("cf:require-approvals")))?;
        }
        "canary_rollout" => {
            let pct = pv
                .config
                .get("percentage")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "percentage",
                })?;
            let dur = pv
                .config
                .get("observe_duration_minutes")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "observe_duration_minutes",
                })?;
            let strat = pv
                .config
                .get("selection_strategy")
                .and_then(|v| v.as_str())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "selection_strategy",
                })?;
            let hc =
                pv.config
                    .get("health_check")
                    .ok_or_else(|| XccdfWriterError::MissingConfig {
                        policy_type: pv.policy_type.clone(),
                        field: "health_check",
                    })?;
            let hc_type = hc.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
                XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "health_check.type",
                }
            })?;
            let fail = hc
                .get("fail_threshold")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "health_check.fail_threshold",
                })?;
            let mut elem = BytesStart::new("cf:canary-rollout");
            if pct == 0 {
                return Err(XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "percentage (must be positive)",
                });
            }
            elem.push_attribute(("percentage", pct.to_string().as_str()));
            elem.push_attribute(("observe-duration-minutes", dur.to_string().as_str()));
            elem.push_attribute(("selection-strategy", strat));
            writer.write_event(Event::Start(elem))?;
            if let Some(desc) = pv.config.get("description").and_then(|v| v.as_str()) {
                cf_el(writer, "description", desc)?;
            }
            let mut hc_elem = BytesStart::new("cf:health-check");
            hc_elem.push_attribute(("type", hc_type));
            hc_elem.push_attribute(("fail-threshold", fail.to_string().as_str()));
            writer.write_event(Event::Empty(hc_elem))?;
            writer.write_event(Event::End(BytesEnd::new("cf:canary-rollout")))?;
        }
        "cve_threshold" => {
            let no_scan = pv
                .config
                .get("no_scan_behavior")
                .and_then(|v| v.as_str())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "no_scan_behavior",
                })?;
            let allow_just = pv
                .config
                .get("allow_justifications")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "allow_justifications",
                })?;
            let req_ack = pv
                .config
                .get("require_acknowledgment")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "require_acknowledgment",
                })?;
            let thresholds = pv
                .config
                .get("thresholds")
                .and_then(|v| v.as_array())
                .ok_or_else(|| XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "thresholds",
                })?;
            if thresholds.is_empty() {
                return Err(XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "thresholds (must contain at least one item)",
                });
            }
            let mut elem = BytesStart::new("cf:cve-threshold");
            elem.push_attribute(("no-scan-behavior", no_scan));
            elem.push_attribute(("allow-justifications", allow_just.to_string().as_str()));
            elem.push_attribute(("require-acknowledgment", req_ack.to_string().as_str()));
            writer.write_event(Event::Start(elem))?;
            if let Some(desc) = pv.config.get("description").and_then(|v| v.as_str()) {
                cf_el(writer, "description", desc)?;
            }
            for t in thresholds {
                let sev = t.get("severity").and_then(|v| v.as_str()).ok_or_else(|| {
                    XccdfWriterError::MissingConfig {
                        policy_type: pv.policy_type.clone(),
                        field: "thresholds[].severity",
                    }
                })?;
                let max = t.get("max").and_then(|v| v.as_u64()).ok_or_else(|| {
                    XccdfWriterError::MissingConfig {
                        policy_type: pv.policy_type.clone(),
                        field: "thresholds[].max",
                    }
                })?;
                let act = t.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
                    XccdfWriterError::MissingConfig {
                        policy_type: pv.policy_type.clone(),
                        field: "thresholds[].action",
                    }
                })?;
                let mut t_elem = BytesStart::new("cf:threshold");
                t_elem.push_attribute(("severity", sev));
                t_elem.push_attribute(("max", max.to_string().as_str()));
                t_elem.push_attribute(("action", act));
                writer.write_event(Event::Empty(t_elem))?;
            }
            writer.write_event(Event::End(BytesEnd::new("cf:cve-threshold")))?;
        }
        other => {
            return Err(XccdfWriterError::MissingConfig {
                policy_type: other.to_owned(),
                field: "<unknown policy type for native implementation>",
            });
        }
    }

    writer.write_event(Event::End(BytesEnd::new("cf:implementation")))?;
    Ok(())
}

fn write_custom_check(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    let rules = pv.config.get("rules").and_then(|v| v.as_array());
    let has_rules = rules.map(|r| !r.is_empty()).unwrap_or(false);

    let mode = pv
        .config
        .get("mode")
        .and_then(|v| v.as_str())
        .ok_or_else(|| XccdfWriterError::MissingConfig {
            policy_type: pv.policy_type.clone(),
            field: "mode",
        })?;
    if !matches!(mode, "all" | "any") {
        return Err(XccdfWriterError::MissingConfig {
            policy_type: pv.policy_type.clone(),
            field: "mode (must be all or any)",
        });
    }
    let context = pv
        .config
        .get("context")
        .and_then(|v| v.as_str())
        .ok_or_else(|| XccdfWriterError::MissingConfig {
            policy_type: pv.policy_type.clone(),
            field: "context",
        })?;
    let binding = pv
        .config
        .get("binding")
        .and_then(|v| v.as_str())
        .ok_or_else(|| XccdfWriterError::MissingConfig {
            policy_type: pv.policy_type.clone(),
            field: "binding",
        })?;

    let mut elem = BytesStart::new("cf:custom-check");
    elem.push_attribute(("mode", mode));
    elem.push_attribute(("context", context));
    elem.push_attribute(("binding", binding));
    writer.write_event(Event::Start(elem))?;

    if has_rules {
        for rule in rules.unwrap() {
            write_custom_check_rule(writer, pv, rule)?;
        }
    } else {
        // Legacy single-expression form.
        let field_name = pv
            .config
            .get("field_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| XccdfWriterError::MissingConfig {
                policy_type: pv.policy_type.clone(),
                field: "field_name",
            })?;
        let strict = pv
            .config
            .get("strict")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| XccdfWriterError::MissingConfig {
                policy_type: pv.policy_type.clone(),
                field: "strict",
            })?;
        let expr = pv
            .config
            .get("expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| XccdfWriterError::MissingConfig {
                policy_type: pv.policy_type.clone(),
                field: "expression",
            })?;
        let mut rule_elem = BytesStart::new("cf:rule");
        rule_elem.push_attribute(("field-name", field_name));
        rule_elem.push_attribute(("strict", strict.to_string().as_str()));
        writer.write_event(Event::Start(rule_elem))?;
        if let Some(desc) = pv.config.get("description").and_then(|v| v.as_str()) {
            cf_el(writer, "description", desc)?;
        }
        let mut expr_elem = BytesStart::new("cf:expression");
        expr_elem.push_attribute(("language", "nix"));
        writer.write_event(Event::Start(expr_elem))?;
        writer.write_event(Event::Text(BytesText::new(expr)))?;
        writer.write_event(Event::End(BytesEnd::new("cf:expression")))?;
        writer.write_event(Event::End(BytesEnd::new("cf:rule")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("cf:custom-check")))?;
    Ok(())
}

fn write_custom_check_rule(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
    rule: &serde_json::Value,
) -> Result<(), XccdfWriterError> {
    let field_name = rule
        .get("field_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| XccdfWriterError::MissingConfig {
            policy_type: pv.policy_type.clone(),
            field: "rules[].field_name",
        })?;
    let strict = rule
        .get("strict")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| XccdfWriterError::MissingConfig {
            policy_type: pv.policy_type.clone(),
            field: "rules[].strict",
        })?;
    let expr = rule
        .get("expression")
        .and_then(|v| v.as_str())
        .ok_or_else(|| XccdfWriterError::MissingConfig {
            policy_type: pv.policy_type.clone(),
            field: "rules[].expression",
        })?;
    let mut elem = BytesStart::new("cf:rule");
    elem.push_attribute(("field-name", field_name));
    elem.push_attribute(("strict", strict.to_string().as_str()));
    writer.write_event(Event::Start(elem))?;
    if let Some(desc) = rule.get("description").and_then(|v| v.as_str()) {
        cf_el(writer, "description", desc)?;
    }
    let mut expr_elem = BytesStart::new("cf:expression");
    expr_elem.push_attribute(("language", "nix"));
    writer.write_event(Event::Start(expr_elem))?;
    writer.write_event(Event::Text(BytesText::new(expr)))?;
    writer.write_event(Event::End(BytesEnd::new("cf:expression")))?;
    writer.write_event(Event::End(BytesEnd::new("cf:rule")))?;
    Ok(())
}

/// Write `<cf:dependencies>` if the policy has non-empty dependencies.
///
/// The CF schema allows two child types:
/// - `<cf:nix-option path="..."/>` — a self-closing element with a `path`
///   attribute (not text content).
/// - `<cf:module-ref uri="..." optional="..."/>` — a self-closing element.
///
/// An array of strings in `dependencies` is treated as nix-option paths.
fn write_cf_dependencies(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    let deps = match &pv.dependencies {
        serde_json::Value::Array(a) if !a.is_empty() => a,
        _ => return Ok(()),
    };
    writer.write_event(Event::Start(BytesStart::new("cf:dependencies")))?;
    for item in deps {
        match item {
            serde_json::Value::String(path) => {
                let mut opt = BytesStart::new("cf:nix-option");
                opt.push_attribute(("path", path.as_str()));
                writer.write_event(Event::Empty(opt))?;
            }
            serde_json::Value::Object(object) => {
                let kind = object
                    .get("kind")
                    .or_else(|| object.get("type"))
                    .and_then(|value| value.as_str());
                match kind {
                    Some("nix_option") | Some("nix-option") => {
                        let path = object
                            .get("path")
                            .and_then(|value| value.as_str())
                            .ok_or_else(|| XccdfWriterError::MissingConfig {
                                policy_type: pv.policy_type.clone(),
                                field: "dependencies[].path",
                            })?;
                        let mut opt = BytesStart::new("cf:nix-option");
                        opt.push_attribute(("path", path));
                        writer.write_event(Event::Empty(opt))?;
                    }
                    Some("module_ref") | Some("module-ref") => {
                        let uri = object
                            .get("uri")
                            .and_then(|value| value.as_str())
                            .ok_or_else(|| XccdfWriterError::MissingConfig {
                                policy_type: pv.policy_type.clone(),
                                field: "dependencies[].uri",
                            })?;
                        let optional = object
                            .get("optional")
                            .and_then(|value| value.as_bool())
                            .ok_or_else(|| XccdfWriterError::MissingConfig {
                                policy_type: pv.policy_type.clone(),
                                field: "dependencies[].optional",
                            })?;
                        let mut module = BytesStart::new("cf:module-ref");
                        module.push_attribute(("uri", uri));
                        module.push_attribute(("optional", optional.to_string().as_str()));
                        writer.write_event(Event::Empty(module))?;
                    }
                    _ => {
                        return Err(XccdfWriterError::MissingConfig {
                            policy_type: pv.policy_type.clone(),
                            field: "dependencies[] (unknown dependency type)",
                        });
                    }
                }
            }
            _ => {
                return Err(XccdfWriterError::MissingConfig {
                    policy_type: pv.policy_type.clone(),
                    field: "dependencies[] (must be a string or typed object)",
                });
            }
        }
    }
    writer.write_event(Event::End(BytesEnd::new("cf:dependencies")))?;
    Ok(())
}

fn write_json_element(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    tag: &str,
    value: &serde_json::Value,
) -> Result<(), XccdfWriterError> {
    let serialized = serde_json::to_string(value)?;
    cf_el(writer, tag, &serialized)
}

// ── XCCDF Rule element writers ────────────────────────────────────────────────

/// Write XCCDF `<check>` (and `<fix>` for native implementations).
///
/// Rule content model sequence (from ruleType):
///   … ident*, fixtext*, fix*, (check | complex-check), …
///
/// `fix` must precede `check`.
fn write_check_and_fix(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    // Parse the imported standard check. A malformed check object is an error
    // rather than a silent replacement with a synthesized CF check.
    let standard_check =
        pv.parse_standard_check()
            .map_err(|e| XccdfWriterError::MalformedImportedCheck {
                policy_version_id: pv.policy_version_id,
                reason: e.to_string(),
            })?;

    // Parse the imported standard fix. A malformed fix object is an error.
    let imported_fix =
        pv.parse_standard_fix()
            .map_err(|e| XccdfWriterError::MalformedImportedFix {
                policy_version_id: pv.policy_version_id,
                reason: e.to_string(),
            })?;

    // fix MUST precede check in the XCCDF Rule sequence. All imported fix
    // attributes (system, id, complexity, disruption) have been validated by
    // parse_standard_fix; emit them unconditionally when present.
    if let Some(fix_data) = imported_fix {
        let generated_fix_id = format!("xccdf_crystalforge_fix_{}", pv.policy_version_id.simple());
        let fix_id = fix_data.id.as_deref().unwrap_or(generated_fix_id.as_str());
        let mut fix = BytesStart::new("fix");
        if let Some(system) = fix_data.system.as_deref() {
            fix.push_attribute(("system", system));
        } else {
            fix.push_attribute(("system", CF_NIX_FIX_SYSTEM));
        }
        fix.push_attribute(("id", fix_id));
        if let Some(complexity) = fix_data.complexity.as_deref() {
            fix.push_attribute(("complexity", complexity));
        }
        if let Some(disruption) = fix_data.disruption.as_deref() {
            fix.push_attribute(("disruption", disruption));
        }
        writer.write_event(Event::Start(fix))?;
        writer.write_event(Event::Text(BytesText::new(&fix_data.content)))?;
        writer.write_event(Event::End(BytesEnd::new("fix")))?;
    } else if pv.implementation_state == ImplementationState::Native {
        let fix_id = format!("xccdf_crystalforge_fix_{}", pv.policy_version_id.simple());
        let mut fix = BytesStart::new("fix");
        fix.push_attribute(("system", CF_NIX_FIX_SYSTEM));
        fix.push_attribute(("id", fix_id.as_str()));
        writer.write_event(Event::Start(fix))?;
        let fix_text = format!("Apply {} via Crystal Forge Nix evaluation", pv.policy_type);
        writer.write_event(Event::Text(BytesText::new(&fix_text)))?;
        writer.write_event(Event::End(BytesEnd::new("fix")))?;
    }

    // XCCDF allows multiple <check> elements in a Rule when each uses a
    // different system URI. When a native policy also has an imported standard
    // check, emit the standard check first (for standards consumers), then the
    // CF executable check (for Crystal Forge consumers). When the imported
    // check body is reference-only, parse_standard_check rejects it because
    // the current single-document export cannot include the referenced file.
    let is_native = pv.implementation_state == ImplementationState::Native;

    if is_native {
        // For native policies: always emit the CF executable check.
        // If an imported standard check is also present, emit it first so that
        // standards consumers (OVAL/XCCDF scanners) can evaluate the original
        // check without understanding Crystal Forge.
        if let Some(ref std_check) = standard_check {
            write_single_standard_check(writer, std_check)?;
        }
        write_cf_executable_check(writer, pv, standard_check.as_ref())?;
    } else if let Some(standard_check) = standard_check {
        // Non-native policy with imported standard check: preserve it exactly.
        write_single_standard_check(writer, &standard_check)?;
    } else {
        // Non-native policy with no imported check: emit CF explanatory check.
        write_cf_executable_check(writer, pv, None)?;
    }

    Ok(())
}

/// Emit a single imported standard XCCDF `<check>` element.
///
/// Preserves all behavior-affecting attributes: `system`, `selector`,
/// `multi-check`, and `negate`. Unknown attributes cause export rejection
/// at parse time (see `parse_standard_check`), so this function only needs
/// to emit the typed fields.
fn write_single_standard_check(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    std_check: &XccdfStandardCheck,
) -> Result<(), XccdfWriterError> {
    let mut check = BytesStart::new("check");
    check.push_attribute(("system", std_check.system.as_str()));
    if let Some(selector) = std_check.selector.as_deref() {
        check.push_attribute(("selector", selector));
    }
    if let Some(multi_check) = std_check.multi_check {
        check.push_attribute(("multi-check", if multi_check { "true" } else { "false" }));
    }
    if let Some(negate) = std_check.negate {
        check.push_attribute(("negate", if negate { "true" } else { "false" }));
    }
    writer.write_event(Event::Start(check))?;
    match &std_check.body {
        XccdfCheckBody::Inline { content } => {
            writer.write_event(Event::Start(BytesStart::new("check-content")))?;
            writer.write_event(Event::Text(BytesText::new(content)))?;
            writer.write_event(Event::End(BytesEnd::new("check-content")))?;
        }
        XccdfCheckBody::Reference { href, name } => {
            let mut content_ref = BytesStart::new("check-content-ref");
            content_ref.push_attribute(("href", href.as_str()));
            if let Some(name) = name.as_deref() {
                content_ref.push_attribute(("name", name));
            }
            writer.write_event(Event::Empty(content_ref))?;
        }
    }
    writer.write_event(Event::End(BytesEnd::new("check")))?;
    Ok(())
}

/// Emit the Crystal Forge executable `<check>` element containing `<cf:policy>`.
fn write_cf_executable_check(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
    standard_check: Option<&XccdfStandardCheck>,
) -> Result<(), XccdfWriterError> {
    let mut check = BytesStart::new("check");
    check.push_attribute(("system", CF_POLICY_CHECK_SYSTEM));
    writer.write_event(Event::Start(check))?;
    writer.write_event(Event::Start(BytesStart::new("check-content")))?;
    // Emit a human-readable description before the typed cf:policy.
    if standard_check.is_none() {
        let text = match pv.implementation_state {
            ImplementationState::Native => {
                format!("Crystal Forge {} policy check", pv.policy_type)
            }
            ImplementationState::Manual => format!(
                "Manual ({}) – user must provide evidence of compliance",
                pv.policy_type
            ),
            ImplementationState::Unbound => format!(
                "Unbound ({}) – requirement exists but has no implementation",
                pv.policy_type
            ),
            ImplementationState::External => {
                format!("External ({}) – checked by external system", pv.policy_type)
            }
            ImplementationState::Opaque => format!(
                "Opaque ({}) – CF preserves rule but cannot model check",
                pv.policy_type
            ),
        };
        writer.write_event(Event::Text(BytesText::new(&text)))?;
    }
    write_cf_policy(writer, pv)?;
    writer.write_event(Event::End(BytesEnd::new("check-content")))?;
    writer.write_event(Event::End(BytesEnd::new("check")))?;
    Ok(())
}

fn write_standard_rationale(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    if let Some(rationale) = pv
        .compliance_metadata
        .get("rationale")
        .and_then(|v| v.as_str())
    {
        el(writer, "rationale", rationale)?;
    }
    Ok(())
}

fn write_standard_platforms(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    if let Some(platforms) = pv
        .compliance_metadata
        .get("platforms")
        .and_then(|v| v.as_array())
    {
        for platform in platforms {
            if let Some(idref) = platform.as_str() {
                let mut element = BytesStart::new("platform");
                element.push_attribute(("idref", idref));
                writer.write_event(Event::Empty(element))?;
            }
        }
    }
    Ok(())
}

fn write_standard_idents(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    if let Some(idents) = pv
        .compliance_metadata
        .get("identifiers")
        .and_then(|v| v.as_array())
    {
        for ident in idents {
            let system = ident.get("system").and_then(|v| v.as_str()).unwrap_or("");
            let value = ident.get("value").and_then(|v| v.as_str()).unwrap_or("");
            if !system.is_empty() && !value.is_empty() {
                let mut elem = BytesStart::new("ident");
                elem.push_attribute(("system", system));
                writer.write_event(Event::Start(elem))?;
                writer.write_event(Event::Text(BytesText::new(value)))?;
                writer.write_event(Event::End(BytesEnd::new("ident")))?;
            }
        }
    }
    Ok(())
}

fn write_standard_references(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    if let Some(refs) = pv
        .compliance_metadata
        .get("references")
        .and_then(|v| v.as_array())
    {
        for r in refs {
            let href = r.get("href").and_then(|v| v.as_str()).unwrap_or("");
            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("");
            if !href.is_empty() {
                let mut elem = BytesStart::new("reference");
                elem.push_attribute(("href", href));
                writer.write_event(Event::Start(elem))?;
                if !title.is_empty() {
                    writer.write_event(Event::Text(BytesText::new(title)))?;
                }
                writer.write_event(Event::End(BytesEnd::new("reference")))?;
            }
        }
    }
    Ok(())
}

/// Write the complete content of an XCCDF `<Rule>` element.
///
/// XCCDF 1.2 Rule content model (sequence):
/// ```text
/// title, description?, warning*, question*, reference*, metadata?,
/// rationale?, platform*, requires*, conflicts*, ident*, impact-metric?,
/// profile-note*, fixtext*, fix*, (check | complex-check), signature?
/// ```
///
/// CF extension elements are placed inside `<metadata>` (xs:any). Standard
/// `<reference>` precedes `<metadata>`, and `<ident>` follows it. No `<version>`
/// element is emitted (not part of the Rule content model).
fn write_rule_content(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), XccdfWriterError> {
    // 1. title (required)
    el(writer, "title", &pv.name)?;

    // 2. description? (optional)
    if let Some(ref desc) = pv.description {
        el(writer, "description", desc)?;
    }

    // 3. reference* (before metadata and ident)
    write_standard_references(writer, pv)?;

    // 4. metadata? — CF extension elements live here (xs:any content)
    {
        writer.write_event(Event::Start(BytesStart::new("metadata")))?;

        // CF policy identity (always present)
        write_policy_identity(writer, pv)?;

        // CF source-object mappings (when present)
        if !pv.source_mappings.is_empty() {
            write_source_mappings(writer, &pv.source_mappings)?;
        }

        // Opaque preserved content (when present)
        if let Some(ref opaque) = pv.opaque_xml {
            cf_el(writer, "opaque-xml", opaque)?;
        }

        writer.write_event(Event::End(BytesEnd::new("metadata")))?;
    }

    // 5. ident* (after metadata, before fix/check)
    write_standard_rationale(writer, pv)?;
    write_standard_platforms(writer, pv)?;
    write_standard_idents(writer, pv)?;

    // 6. fix? + check (fix MUST precede check per XCCDF Rule content model)
    write_check_and_fix(writer, pv)?;

    Ok(())
}

fn write_group_tree(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    group: &XccdfGroupExport,
    policies: &std::collections::HashMap<uuid::Uuid, &XccdfPolicyExport>,
) -> Result<(), XccdfWriterError> {
    let mut element = BytesStart::new("Group");
    element.push_attribute(("id", group.generated_id.as_str()));
    writer.write_event(Event::Start(element))?;
    el(writer, "title", &group.title)?;
    if let Some(description) = &group.description {
        el(writer, "description", description)?;
    }
    if let Some(source_id) = &group.source_id {
        writer.write_event(Event::Start(BytesStart::new("metadata")))?;
        cf_el(writer, "source-group-id", source_id)?;
        writer.write_event(Event::End(BytesEnd::new("metadata")))?;
    }
    let mut ordered_children: Vec<&XccdfGroupExport> = group.children.iter().collect();
    ordered_children.sort_by_key(|child| child.order);
    for child in ordered_children {
        write_group_tree(writer, child, policies)?;
    }
    let mut ordered_policies = group.policies.clone();
    ordered_policies.sort_by_key(|policy_id| {
        policies
            .get(policy_id)
            .map(|policy| policy.policy_order)
            .unwrap_or(i32::MAX)
    });
    for policy_id in ordered_policies {
        if let Some(pv) = policies.get(&policy_id) {
            let rule_id = pv.rule_id();
            let mut rule = BytesStart::new("Rule");
            rule.push_attribute(("id", rule_id.as_str()));
            rule.push_attribute(("selected", if pv.selected { "true" } else { "false" }));
            rule.push_attribute(("weight", "10.0"));
            rule.push_attribute(("severity", standard_severity(pv)));
            writer.write_event(Event::Start(rule))?;
            write_rule_content(writer, pv)?;
            writer.write_event(Event::End(BytesEnd::new("Rule")))?;
        }
    }
    writer.write_event(Event::End(BytesEnd::new("Group")))?;
    Ok(())
}

// ── Public writer entry point ─────────────────────────────────────────────────

/// Write a complete XCCDF 1.2 Benchmark for a bundle version export.
///
/// Returns `XccdfWriterError::InvalidExecutionPhase` if any policy has an
/// execution phase not in the CF schema enumeration, and
/// `XccdfWriterError::MissingConfig` if a native policy is missing a required
/// configuration field. These are returned before any bytes are written.
pub fn write_bundle_xccdf_export(snapshot: &XccdfBundleExport) -> Result<String, XccdfWriterError> {
    if snapshot.digest_algorithm != DIGEST_ALGORITHM
        || snapshot.canonicalization_version != CANONICALIZATION_VERSION
    {
        return Err(XccdfWriterError::UnsupportedDigestMetadata {
            object: "bundle",
            algorithm: snapshot.digest_algorithm.clone(),
            canonicalization_version: snapshot.canonicalization_version.clone(),
        });
    }
    // Pre-validate all policies before writing any bytes.
    for pv in &snapshot.policies {
        if pv.digest_algorithm != DIGEST_ALGORITHM
            || pv.canonicalization_version != CANONICALIZATION_VERSION
        {
            return Err(XccdfWriterError::UnsupportedDigestMetadata {
                object: "policy",
                algorithm: pv.digest_algorithm.clone(),
                canonicalization_version: pv.canonicalization_version.clone(),
            });
        }
        validate_execution_phase(pv.policy_version_id, &pv.execution_phase)?;
        // Validate any imported standard check before writing XML.
        pv.parse_standard_check()
            .map_err(|e| XccdfWriterError::MalformedImportedCheck {
                policy_version_id: pv.policy_version_id,
                reason: e.to_string(),
            })?;
    }

    let mut buf = Vec::new();
    let mut writer = Writer::new(Cursor::new(&mut buf));

    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

    let benchmark_id = snapshot.benchmark_id();
    let mut bench = BytesStart::new("Benchmark");
    bench.push_attribute(("xmlns", XCCDF_1_2_NAMESPACE));
    bench.push_attribute(("xmlns:cf", CF_XCCDF_NAMESPACE));
    bench.push_attribute(("id", benchmark_id.as_str()));
    bench.push_attribute(("resolved", "false"));
    bench.push_attribute(("xml:lang", "en"));
    writer.write_event(Event::Start(bench))?;

    // XCCDF Benchmark-level elements (sequence: status, title, description, version, metadata, …)
    el(
        &mut writer,
        "status",
        xccdf_status_str(snapshot.publication_state),
    )?;
    el(&mut writer, "title", &snapshot.name)?;
    if let Some(ref desc) = snapshot.description {
        el(&mut writer, "description", desc)?;
    }
    // Bundle version (e.g. "2.3.0"), NOT the framework revision.
    el(&mut writer, "version", &snapshot.version)?;

    // Benchmark-level metadata: cf:bundle with required attributes.
    {
        writer.write_event(Event::Start(BytesStart::new("metadata")))?;

        let bundle_urn = format!("urn:uuid:{}", snapshot.bundle_id);
        let bundle_version_urn = format!("urn:uuid:{}", snapshot.bundle_version_id);

        let mut bundle_elem = BytesStart::new("cf:bundle");
        bundle_elem.push_attribute(("schema-version", "1"));
        bundle_elem.push_attribute(("bundle-id", bundle_urn.as_str()));
        bundle_elem.push_attribute(("bundle-version-id", bundle_version_urn.as_str()));
        bundle_elem.push_attribute((
            "publication-state",
            publication_state_str(snapshot.publication_state),
        ));
        writer.write_event(Event::Start(bundle_elem))?;

        // cf:framework (optional child)
        if let Some(ref fw_ver) = snapshot.framework_version {
            let mut fw_elem = BytesStart::new("cf:framework");
            fw_elem.push_attribute(("name", snapshot.framework.as_str()));
            fw_elem.push_attribute(("version", fw_ver.as_str()));
            writer.write_event(Event::Empty(fw_elem))?;
        } else {
            let mut fw_elem = BytesStart::new("cf:framework");
            fw_elem.push_attribute(("name", snapshot.framework.as_str()));
            writer.write_event(Event::Empty(fw_elem))?;
        }

        // cf:layer (optional child)
        cf_el(&mut writer, "layer", &snapshot.layer)?;

        // cf:owner (optional child)
        cf_el(&mut writer, "owner", &snapshot.owner)?;

        // cf:content-digest (required child)
        let mut digest_elem = BytesStart::new("cf:content-digest");
        digest_elem.push_attribute(("algorithm", snapshot.digest_algorithm.as_str()));
        digest_elem.push_attribute((
            "canonical-model",
            snapshot.canonicalization_version.as_str(),
        ));
        writer.write_event(Event::Start(digest_elem))?;
        writer.write_event(Event::Text(BytesText::new(&snapshot.semantic_digest)))?;
        writer.write_event(Event::End(BytesEnd::new("cf:content-digest")))?;

        writer.write_event(Event::End(BytesEnd::new("cf:bundle")))?;
        writer.write_event(Event::End(BytesEnd::new("metadata")))?;
    }

    // Profile: one baseline profile with a select entry per policy.
    let profile_id = snapshot.profile_id();
    let mut prof = BytesStart::new("Profile");
    prof.push_attribute(("id", profile_id.as_str()));
    writer.write_event(Event::Start(prof))?;
    el(&mut writer, "title", "Crystal Forge Baseline")?;
    for policy in &snapshot.policies {
        let rid = policy.rule_id();
        let mut sel = BytesStart::new("select");
        sel.push_attribute(("idref", rid.as_str()));
        sel.push_attribute(("selected", if policy.selected { "true" } else { "false" }));
        writer.write_event(Event::Empty(sel))?;
    }
    writer.write_event(Event::Start(BytesStart::new("metadata")))?;
    cf_el(&mut writer, "profile-role", "baseline")?;
    writer.write_event(Event::End(BytesEnd::new("metadata")))?;
    writer.write_event(Event::End(BytesEnd::new("Profile")))?;

    if !snapshot.groups.is_empty() {
        let policy_map: std::collections::HashMap<uuid::Uuid, &XccdfPolicyExport> = snapshot
            .policies
            .iter()
            .map(|policy| (policy.policy_version_id, policy))
            .collect();
        let mut ordered_groups: Vec<&XccdfGroupExport> = snapshot.groups.iter().collect();
        ordered_groups.sort_by_key(|group| group.order);
        for group in ordered_groups {
            write_group_tree(&mut writer, group, &policy_map)?;
        }
    } else {
        // Authored policies fall back to deterministic type groups.
        let mut groups: BTreeMap<String, (String, Vec<&XccdfPolicyExport>)> = BTreeMap::new();
        for policy in &snapshot.policies {
            let group_id = policy
                .compliance_metadata
                .get("group_id")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| group_id_for_type(&policy.policy_type));
            let group_title = policy
                .compliance_metadata
                .get("group_title")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| group_title_for_type(&policy.policy_type).to_owned());
            groups
                .entry(group_id)
                .or_insert_with(|| (group_title, Vec::new()))
                .1
                .push(policy);
        }

        for (gid, (group_title, policies)) in &groups {
            let mut group = BytesStart::new("Group");
            group.push_attribute(("id", gid.as_str()));
            writer.write_event(Event::Start(group))?;
            el(&mut writer, "title", group_title)?;

            for pv in policies {
                let rid = pv.rule_id();
                let mut rule = BytesStart::new("Rule");
                rule.push_attribute(("id", rid.as_str()));
                rule.push_attribute(("selected", if pv.selected { "true" } else { "false" }));
                rule.push_attribute(("weight", "10.0"));
                rule.push_attribute(("severity", standard_severity(pv)));
                writer.write_event(Event::Start(rule))?;

                write_rule_content(&mut writer, pv)?;

                writer.write_event(Event::End(BytesEnd::new("Rule")))?;
            }

            writer.write_event(Event::End(BytesEnd::new("Group")))?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("Benchmark")))?;
    drop(writer);

    Ok(String::from_utf8(buf).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::super::export_models::{XccdfPolicyExport, XccdfSourceMapping};
    use super::*;
    use crate::compliance::canonical::{ImplementationState, PublicationState};
    use serde_json::json;
    use uuid::Uuid;

    fn test_policy(
        policy_type: &str,
        impl_state: ImplementationState,
        config: serde_json::Value,
    ) -> XccdfPolicyExport {
        let id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let vid = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        XccdfPolicyExport {
            policy_id: id,
            policy_version_id: vid,
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "abc123".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "Test Policy".into(),
            description: Some("A test policy".into()),
            policy_type: policy_type.into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: impl_state,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config,
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        }
    }

    fn test_snapshot() -> XccdfBundleExport {
        let bundle_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        let bundle_version_id = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();

        let p1_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let p1_vid = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let p2_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let p2_vid = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();

        XccdfBundleExport {
            bundle_id,
            bundle_version_id,
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "abc123".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "Test Bundle".into(),
            description: Some("A test bundle".into()),
            framework: "STIG".into(),
            framework_version: Some("V1R1".into()),
            layer: "os".into(),
            owner: "Team".into(),
            groups: vec![],
            policies: vec![
                XccdfPolicyExport {
                    policy_id: p1_id,
                    policy_version_id: p1_vid,
                    version: "1.0".into(),
                    publication_state: PublicationState::Accepted,
                    semantic_digest: "digest1".into(),
                    digest_algorithm: "sha-256".into(),
                    canonicalization_version: "cf-model-json-1".into(),
                    name: "CF Agent Policy".into(),
                    description: Some("Requires CF agent".into()),
                    policy_type: "require_cf_agent".into(),
                    execution_phase: "nix-evaluation".into(),
                    implementation_state: ImplementationState::Native,
                    enabled_default: true,
                    selected: true,
                    policy_order: 1,
                    config: json!({}),
                    compliance_metadata: json!({}),
                    dependencies: json!([]),
                    opaque_xml: None,
                    source_mappings: vec![XccdfSourceMapping {
                        object_kind: "policy".into(),
                        source_identity: "stig://1234".into(),
                        fidelity: "high".into(),
                    }],
                },
                XccdfPolicyExport {
                    policy_id: p2_id,
                    policy_version_id: p2_vid,
                    version: "2.0".into(),
                    publication_state: PublicationState::Draft,
                    semantic_digest: "digest2".into(),
                    digest_algorithm: "sha-256".into(),
                    canonicalization_version: "cf-model-json-1".into(),
                    name: "CVE Threshold".into(),
                    description: None,
                    policy_type: "cve_threshold".into(),
                    execution_phase: "deployment-orchestration".into(),
                    implementation_state: ImplementationState::Native,
                    enabled_default: true,
                    selected: true,
                    policy_order: 2,
                    config: json!({
                        "no_scan_behavior": "block",
                        "allow_justifications": false,
                        "require_acknowledgment": true,
                        "thresholds": [{"severity": "critical", "max": 0, "action": "block"}]
                    }),
                    compliance_metadata: json!({}),
                    dependencies: json!(["nixos/modules/cve.nix"]),
                    opaque_xml: None,
                    source_mappings: vec![],
                },
            ],
        }
    }

    #[test]
    fn produces_well_formed_xml() {
        let xml = write_bundle_xccdf_export(&test_snapshot()).unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<Benchmark"));
        assert!(xml.contains("</Benchmark>"));
        assert!(xml.contains("</Profile>"));
        assert!(xml.contains("</Group>"));
        assert!(xml.contains("</Rule>"));
        assert!(xml.contains("</metadata>"));
    }

    #[test]
    fn benchmark_id_uses_underscore_format() {
        let snap = test_snapshot();
        let expected = snap.benchmark_id();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        // ID must use underscore separator: xccdf_crystalforge_benchmark_<uuid>
        assert!(
            expected.starts_with("xccdf_crystalforge_benchmark_"),
            "ID: {expected}"
        );
        assert!(
            xml.contains(&expected),
            "Expected benchmark id {expected} in XML"
        );
    }

    #[test]
    fn rule_id_uses_underscore_format() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        for policy in &snap.policies {
            let rid = policy.rule_id();
            assert!(
                rid.starts_with("xccdf_crystalforge_rule_"),
                "Rule ID: {rid}"
            );
            assert!(xml.contains(&rid), "Missing rule id {rid}");
        }
    }

    #[test]
    fn profile_id_uses_underscore_format() {
        let snap = test_snapshot();
        let pid = snap.profile_id();
        assert!(
            pid.starts_with("xccdf_crystalforge_profile_"),
            "Profile ID: {pid}"
        );
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains(&pid), "Missing profile id {pid}");
    }

    #[test]
    fn benchmark_version_from_bundle_version_not_framework() {
        let mut snap = test_snapshot();
        snap.version = "2.3.0".into();
        snap.framework_version = Some("V9R99".into());
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("<version>2.3.0</version>"),
            "Bundle version must appear in <version>"
        );
        // Framework version should not appear in <version> element.
        let version_pos = xml.find("<version>").unwrap();
        let after_version = &xml[version_pos..version_pos + 30];
        assert!(
            !after_version.contains("V9R99"),
            "Framework version must not be in <version>: {after_version}"
        );
    }

    #[test]
    fn publication_state_preserved_exactly() {
        for (state, expected) in [
            (PublicationState::Incomplete, "incomplete"),
            (PublicationState::Draft, "draft"),
            (PublicationState::Interim, "interim"),
            (PublicationState::Accepted, "accepted"),
            (PublicationState::Deprecated, "deprecated"),
        ] {
            assert_eq!(publication_state_str(state), expected);
        }
    }

    #[test]
    fn accepted_bundle_exports_as_accepted_not_draft() {
        let mut snap = test_snapshot();
        snap.publication_state = PublicationState::Accepted;
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("<status>accepted</status>"),
            "Accepted bundle must export as accepted"
        );
        assert!(
            !xml.contains("<status>draft</status>"),
            "Accepted bundle must not export as draft"
        );
    }

    #[test]
    fn interim_bundle_exports_as_interim() {
        let mut snap = test_snapshot();
        snap.publication_state = PublicationState::Interim;
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("<status>interim</status>"),
            "Interim bundle must export as interim"
        );
    }

    #[test]
    fn accepted_policy_identity_exports_as_accepted() {
        let mut snap = test_snapshot();
        snap.policies[0].publication_state = PublicationState::Accepted;
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("publication-state=\"accepted\""),
            "Accepted policy must export as accepted"
        );
    }

    #[test]
    fn cf_bundle_has_required_attributes() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        // Required attributes on cf:bundle per cf-xccdf-1.xsd
        assert!(
            xml.contains("schema-version=\"1\""),
            "Missing schema-version attribute"
        );
        assert!(xml.contains(&format!("bundle-id=\"urn:uuid:{}\"", snap.bundle_id)));
        assert!(xml.contains(&format!(
            "bundle-version-id=\"urn:uuid:{}\"",
            snap.bundle_version_id
        )));
        assert!(
            xml.contains("publication-state=\"draft\""),
            "Missing publication-state on cf:bundle"
        );
        // Required child element: cf:content-digest
        assert!(
            xml.contains("cf:content-digest"),
            "Missing cf:content-digest child"
        );
        assert!(
            xml.contains("algorithm=\"sha-256\""),
            "Missing algorithm attribute on cf:content-digest"
        );
        assert!(xml.contains(&snap.semantic_digest), "Missing digest value");
    }

    #[test]
    fn cf_bundle_has_no_invented_child_elements() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        // Must NOT emit these non-schema children
        assert!(
            !xml.contains("cf:semantic-digest"),
            "cf:semantic-digest is not in the schema"
        );
        assert!(
            !xml.contains("cf:publication-state"),
            "cf:publication-state child is not in the schema"
        );
        assert!(
            !xml.contains("cf:bundle-id"),
            "cf:bundle-id standalone element is not in the schema"
        );
        assert!(
            !xml.contains("cf:bundle-version-id"),
            "cf:bundle-version-id standalone element is not in the schema"
        );
    }

    #[test]
    fn cf_extensions_are_inside_metadata_not_directly_in_rule() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        // All CF extensions inside a Rule must be inside <metadata>…</metadata>
        // Check that cf:policy-identity appears after <metadata> and before </metadata>
        let meta_start = xml.find("<metadata>").unwrap_or(0);
        // Find the Rule-level metadata (after a <Rule> tag)
        let rule_pos = xml.find("<Rule ").unwrap_or(0);
        let rule_meta = xml[rule_pos..]
            .find("<metadata>")
            .map(|p| p + rule_pos)
            .unwrap_or(0);
        let rule_meta_end = xml[rule_meta..]
            .find("</metadata>")
            .map(|p| p + rule_meta)
            .unwrap_or(0);
        let pi_pos = xml[rule_meta..]
            .find("cf:policy-identity")
            .map(|p| p + rule_meta)
            .unwrap_or(usize::MAX);
        assert!(
            pi_pos < rule_meta_end,
            "cf:policy-identity must be inside Rule <metadata>"
        );
        let _ = meta_start; // used to ensure variable is referenced
    }

    #[test]
    fn reference_before_metadata_in_rule() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "STIG Rule".into(),
            description: None,
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({
                "references": [{"href": "https://example.com/ref", "title": "Reference"}]
            }),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let rule_pos = xml.find("<Rule ").unwrap();
        let rule_xml = &xml[rule_pos..];
        let ref_pos = rule_xml.find("<reference ").unwrap();
        let meta_pos = rule_xml.find("<metadata>").unwrap();
        assert!(
            ref_pos < meta_pos,
            "<reference> must precede <metadata> in Rule"
        );
    }

    #[test]
    fn ident_after_metadata_in_rule() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "STIG Rule".into(),
            description: None,
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({
                "identifiers": [{"system": "http://cyber.mil/cci", "value": "CCI-001"}]
            }),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let rule_pos = xml.find("<Rule ").unwrap();
        let rule_xml = &xml[rule_pos..];
        let meta_end_pos = rule_xml.find("</metadata>").unwrap();
        let ident_pos = rule_xml.find("<ident ").unwrap();
        assert!(
            ident_pos > meta_end_pos,
            "<ident> must follow </metadata> in Rule"
        );
    }

    #[test]
    fn fix_before_check_in_rule() {
        let pv = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let rule_pos = xml.find("<Rule ").unwrap();
        let rule_xml = &xml[rule_pos..];
        let fix_pos = rule_xml.find("<fix ").unwrap();
        let check_pos = rule_xml.find("<check ").unwrap();
        assert!(fix_pos < check_pos, "<fix> must precede <check> in Rule");
    }

    #[test]
    fn no_version_element_in_rule() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let rule_pos = xml.find("<Rule ").unwrap();
        let rule_end = xml.find("</Rule>").unwrap();
        let rule_xml = &xml[rule_pos..rule_end];
        assert!(
            !rule_xml.contains("<version>"),
            "<version> must not appear inside a Rule"
        );
    }

    #[test]
    fn invalid_execution_phase_returns_error() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "Bad Phase".into(),
            description: None,
            policy_type: "require_cf_agent".into(),
            execution_phase: "deployment".into(), // invalid per schema
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let result = write_bundle_xccdf_export(&snap);
        assert!(
            matches!(result, Err(XccdfWriterError::InvalidExecutionPhase { .. })),
            "Expected InvalidExecutionPhase error, got: {:?}",
            result.err().map(|e| e.to_string())
        );
    }

    #[test]
    fn post_deployment_execution_phase_returns_error() {
        let pv = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        let mut pv = pv;
        pv.execution_phase = "post-deployment".into(); // invalid per schema
        let snap = make_single_policy_snapshot(vec![pv]);
        let result = write_bundle_xccdf_export(&snap);
        assert!(matches!(
            result,
            Err(XccdfWriterError::InvalidExecutionPhase { .. })
        ));
    }

    #[test]
    fn missing_required_config_field_returns_error() {
        let pv = test_policy(
            "require_packages",
            ImplementationState::Native,
            json!({}), // missing "packages"
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let result = write_bundle_xccdf_export(&snap);
        assert!(
            matches!(result, Err(XccdfWriterError::MissingConfig { .. })),
            "Expected MissingConfig error"
        );
    }

    #[test]
    fn profile_has_selects_for_all_policies() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        for policy in &snap.policies {
            let rid = policy.rule_id();
            assert!(
                xml.contains(&format!("idref=\"{rid}\"")),
                "Missing select for rule {rid}"
            );
        }
    }

    #[test]
    fn groups_policies_by_type_btree_order() {
        let xml = write_bundle_xccdf_export(&test_snapshot()).unwrap();
        assert!(
            xml.contains("xccdf_crystalforge_group_require-cf-agent"),
            "Missing require-cf-agent group"
        );
        assert!(
            xml.contains("xccdf_crystalforge_group_cve-threshold"),
            "Missing cve-threshold group"
        );
        let cve_pos = xml.find("cve-threshold").unwrap();
        let cf_pos = xml.find("require-cf-agent").unwrap();
        assert!(
            cve_pos < cf_pos,
            "Groups should be in BTreeMap alphabetical order (cve < require)"
        );
    }

    #[test]
    fn empty_snapshot_produces_benchmark_only() {
        let snap = XccdfBundleExport {
            bundle_id: Uuid::new_v4(),
            bundle_version_id: Uuid::new_v4(),
            version: "0.1".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "empty".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "Empty".into(),
            description: None,
            framework: "none".into(),
            framework_version: None,
            layer: "none".into(),
            owner: "nobody".into(),
            groups: vec![],
            policies: vec![],
        };
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("<Benchmark"));
        assert!(xml.contains("</Benchmark>"));
        assert!(xml.contains("<Profile"));
        assert!(!xml.contains("<Group"));
        assert!(!xml.contains("<Rule"));
        assert!(!xml.contains("<select"));
    }

    fn make_single_policy_snapshot(policies: Vec<XccdfPolicyExport>) -> XccdfBundleExport {
        XccdfBundleExport {
            bundle_id: Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap(),
            bundle_version_id: Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "Test".into(),
            description: None,
            framework: "STIG".into(),
            framework_version: Some("V1R1".into()),
            layer: "os".into(),
            owner: "team".into(),
            groups: vec![],
            policies,
        }
    }

    #[test]
    fn require_cf_agent_policy_type() {
        let pv = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:require-crystal-forge-agent"));
        assert!(xml.contains("cf:implementation"));
        assert!(xml.contains("cf:policy"));
        assert!(xml.contains("cf:execution"));
    }

    #[test]
    fn require_packages_policy_type() {
        let pv = test_policy(
            "require_packages",
            ImplementationState::Native,
            json!({"packages": ["nginx", "openssl"]}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:require-packages"));
        assert!(xml.contains("cf:package"));
        assert!(xml.contains("nginx"));
        assert!(xml.contains("openssl"));
    }

    #[test]
    fn require_packages_missing_config_is_error() {
        let pv = test_policy("require_packages", ImplementationState::Native, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        assert!(matches!(
            write_bundle_xccdf_export(&snap),
            Err(XccdfWriterError::MissingConfig { .. })
        ));
    }

    #[test]
    fn custom_check_single_expression() {
        let pv = test_policy(
            "custom_check",
            ImplementationState::Native,
            json!({
                "expression": "cfg.config.networking.firewall.enable",
                "description": "Firewall enabled",
                "field_name": "firewallEnabled",
                "strict": true,
                "mode": "all",
                "context": "nixos-configuration-v1",
                "binding": "cfg"
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:custom-check"));
        assert!(xml.contains("cf:rule"));
        assert!(xml.contains("cf:expression"));
        assert!(xml.contains("language=\"nix\""));
        assert!(xml.contains("cfg.config.networking.firewall.enable"));
        assert!(xml.contains("firewallEnabled"));
    }

    #[test]
    fn custom_check_multi_rule_all() {
        let pv = test_policy(
            "custom_check",
            ImplementationState::Native,
            json!({
                "mode": "all",
                "context": "nixos-configuration-v1",
                "binding": "cfg",
                "rules": [
                    {"expression": "a", "description": "Rule A", "field_name": "a", "strict": true},
                    {"expression": "b", "description": "Rule B", "field_name": "b", "strict": false}
                ]
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("mode=\"all\""));
        let rule_count = xml.matches("<cf:rule ").count();
        assert_eq!(rule_count, 2, "Expected 2 cf:rule elements");
        assert!(xml.contains("field-name=\"a\""));
        assert!(xml.contains("field-name=\"b\""));
    }

    #[test]
    fn custom_check_multi_rule_any() {
        let pv = test_policy(
            "custom_check",
            ImplementationState::Native,
            json!({
                "mode": "any",
                "context": "nixos-configuration-v1",
                "binding": "cfg",
                "rules": [{"expression": "x", "field_name": "x", "strict": true}]
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("mode=\"any\""));
    }

    #[test]
    fn require_cve_check_policy_type() {
        let pv = test_policy(
            "require_cve_check",
            ImplementationState::Native,
            json!({
                "max_critical": 0,
                "require_high_justification": true,
                "when_no_scan": "block",
                "max_high": 5
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:require-cve-check"));
        assert!(xml.contains("max-critical=\"0\""));
        assert!(xml.contains("require-high-justification=\"true\""));
        assert!(xml.contains("when-no-scan=\"block\""));
        assert!(xml.contains("cf:max-high"));
    }

    #[test]
    fn require_cve_check_missing_config_is_error() {
        // Missing required fields
        let pv = test_policy("require_cve_check", ImplementationState::Native, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        assert!(matches!(
            write_bundle_xccdf_export(&snap),
            Err(XccdfWriterError::MissingConfig { .. })
        ));
    }

    #[test]
    fn invalid_typed_configuration_values_are_rejected() {
        let cases = vec![
            ("require_packages", json!({"packages": [1]})),
            (
                "custom_check",
                json!({
                    "mode": "sometimes",
                    "context": "nixos-configuration-v1",
                    "binding": "cfg",
                    "expression": "true",
                    "field_name": "enabled",
                    "strict": true
                }),
            ),
            (
                "time_window",
                json!({
                    "start_time": "09:00",
                    "end_time": "17:00",
                    "timezone": "UTC",
                    "action": "block",
                    "days": ["monday"]
                }),
            ),
            (
                "require_approvals",
                json!({"count": 0, "role": "admin", "distinct": true}),
            ),
            (
                "canary_rollout",
                json!({
                    "percentage": 0,
                    "observe_duration_minutes": 30,
                    "selection_strategy": "random",
                    "health_check": {"type": "systemd", "fail_threshold": 1}
                }),
            ),
            (
                "cve_threshold",
                json!({
                    "no_scan_behavior": "block",
                    "allow_justifications": false,
                    "require_acknowledgment": true,
                    "thresholds": []
                }),
            ),
        ];

        for (policy_type, config) in cases {
            let policy = test_policy(policy_type, ImplementationState::Native, config);
            let snapshot = make_single_policy_snapshot(vec![policy]);
            assert!(
                matches!(
                    write_bundle_xccdf_export(&snapshot),
                    Err(XccdfWriterError::MissingConfig { .. })
                ),
                "invalid configuration for {policy_type} must be rejected"
            );
        }
    }

    #[test]
    fn standard_xccdf_projection_preserves_imported_fields() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.compliance_metadata = json!({
            "severity": "low",
            "group_id": "xccdf_crystalforge_group_imported",
            "group_title": "Imported Group",
            "rationale": "Imported rationale",
            "platforms": ["cpe:/o:example:nixos:1"],
            "check": {"system": "urn:example:check", "content": "Imported check"},
            "fix": {"system": "urn:example:fix", "content": "Imported fix"},
            "identifiers": [{"system": "urn:example:id", "value": "V-0001"}],
            "references": [{"href": "https://example.test/ref", "title": "Imported reference"}]
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(xml.contains("severity=\"low\""));
        assert!(xml.contains("xccdf_crystalforge_group_imported"));
        assert!(xml.contains("<rationale>Imported rationale</rationale>"));
        assert!(xml.contains("idref=\"cpe:/o:example:nixos:1\""));
        assert!(xml.contains("system=\"urn:example:fix\""));
        assert!(xml.contains("Imported check"));
    }

    #[test]
    fn time_window_policy_type() {
        let pv = test_policy(
            "time_window",
            ImplementationState::Native,
            json!({
                "start_time": "09:00",
                "end_time": "17:00",
                "timezone": "UTC",
                "action": "block",
                "description": "Business hours",
                "days": ["mon", "tue", "wed"]
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:time-window"));
        assert!(xml.contains("start-time=\"09:00\""));
        assert!(xml.contains("end-time=\"17:00\""));
        assert!(xml.contains("timezone=\"UTC\""));
        assert!(xml.contains("action=\"block\""));
        assert!(xml.contains("cf:day"));
        assert!(xml.contains("mon"));
        assert!(xml.contains("tue"));
        assert!(xml.contains("wed"));
    }

    #[test]
    fn require_approvals_policy_type() {
        let pv = test_policy(
            "require_approvals",
            ImplementationState::Native,
            json!({
                "count": 2,
                "role": "admin",
                "distinct": true,
                "description": "Two admins required",
                "expires_after_hours": 24
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:require-approvals"));
        assert!(xml.contains("count=\"2\""));
        assert!(xml.contains("role=\"admin\""));
        assert!(xml.contains("distinct=\"true\""));
        assert!(xml.contains("cf:expires-after-hours"));
        assert!(xml.contains("24"));
    }

    #[test]
    fn canary_rollout_policy_type() {
        let pv = test_policy(
            "canary_rollout",
            ImplementationState::Native,
            json!({
                "percentage": 25,
                "observe_duration_minutes": 30,
                "selection_strategy": "random",
                "description": "Gradual rollout",
                "health_check": {"type": "systemd", "fail_threshold": 3}
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:canary-rollout"));
        assert!(xml.contains("percentage=\"25\""));
        assert!(xml.contains("observe-duration-minutes=\"30\""));
        assert!(xml.contains("selection-strategy=\"random\""));
        assert!(xml.contains("cf:health-check"));
        assert!(xml.contains("type=\"systemd\""));
        assert!(xml.contains("fail-threshold=\"3\""));
    }

    #[test]
    fn cve_threshold_policy_type() {
        let pv = test_policy(
            "cve_threshold",
            ImplementationState::Native,
            json!({
                "no_scan_behavior": "block",
                "allow_justifications": false,
                "require_acknowledgment": true,
                "thresholds": [
                    {"severity": "critical", "max": 0, "action": "block"},
                    {"severity": "high", "max": 3, "action": "warn"}
                ]
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:cve-threshold"));
        assert!(xml.contains("no-scan-behavior=\"block\""));
        assert!(xml.contains("allow-justifications=\"false\""));
        assert!(xml.contains("require-acknowledgment=\"true\""));
        assert!(xml.contains("cf:threshold"));
        assert!(xml.contains("severity=\"critical\""));
        assert!(xml.contains("severity=\"high\""));
        assert!(xml.contains("max=\"0\""));
        assert!(xml.contains("max=\"3\""));
    }

    #[test]
    fn native_policies_emit_cf_check_and_fix() {
        let pv = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains(&format!("system=\"{CF_POLICY_CHECK_SYSTEM}\"")),
            "Missing CF check system"
        );
        assert!(xml.contains("<check-content>"), "Missing check-content");
        assert!(
            xml.contains("<cf:policy schema-version=\"1\">"),
            "Missing executable CF policy in check-content"
        );
        assert!(
            xml.contains(&format!("system=\"{CF_NIX_FIX_SYSTEM}\"")),
            "Missing CF fix system"
        );
        assert!(xml.contains("<fix"), "Missing fix element");
    }

    #[test]
    fn manual_policy_emits_explanatory_check_text() {
        let pv = test_policy("require_cf_agent", ImplementationState::Manual, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("user must provide evidence"),
            "Missing manual policy check text"
        );
        assert!(
            !xml.contains("check-content-ref"),
            "Manual should not have check-content-ref"
        );
        assert!(
            xml.contains("state=\"manual\""),
            "Manual implementation state must be preserved"
        );
    }

    #[test]
    fn unbound_policy_emits_explanatory_check_text() {
        let pv = test_policy("require_cf_agent", ImplementationState::Unbound, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("no implementation"),
            "Missing unbound policy check text"
        );
    }

    #[test]
    fn external_policy_emits_explanatory_check_text() {
        let pv = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("external system"),
            "Missing external policy check text"
        );
    }

    #[test]
    fn opaque_policy_emits_explanatory_check_text() {
        let pv = test_policy("require_cf_agent", ImplementationState::Opaque, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("cannot model check"),
            "Missing opaque policy check text"
        );
    }

    #[test]
    fn non_native_policies_preserve_state_and_config_in_cf_policy() {
        for state in [
            ImplementationState::Manual,
            ImplementationState::Unbound,
            ImplementationState::External,
            ImplementationState::Opaque,
        ] {
            let pv = test_policy("require_cf_agent", state, json!({}));
            let snap = make_single_policy_snapshot(vec![pv]);
            let xml = write_bundle_xccdf_export(&snap).unwrap();
            assert!(
                xml.contains("cf:implementation"),
                "Non-native state {state:?} must retain implementation state"
            );
            assert!(xml.contains(&format!("state=\"{}\"", implementation_state_str(state))));
            assert!(
                xml.contains("cf:config-json"),
                "Non-native state must retain config"
            );
        }
    }

    #[test]
    fn policy_identity_element_with_uuids_and_digest() {
        let pv = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:policy-identity"));
        assert!(xml.contains("policy-id=\"urn:uuid:"));
        assert!(xml.contains("policy-version-id=\"urn:uuid:"));
        assert!(xml.contains("cf:policy-version"));
        assert!(xml.contains("cf:content-digest"));
        assert!(xml.contains("algorithm=\"sha-256\""));
        assert!(xml.contains("canonical-model=\"cf-model-json-1\""));
        assert!(xml.contains("abc123"));
    }

    #[test]
    fn unsupported_canonical_model_is_rejected() {
        let mut snapshot = make_single_policy_snapshot(vec![test_policy(
            "require_cf_agent",
            ImplementationState::Native,
            json!({}),
        )]);
        snapshot.canonicalization_version = "unknown-model".into();
        assert!(matches!(
            write_bundle_xccdf_export(&snapshot),
            Err(XccdfWriterError::UnsupportedDigestMetadata {
                object: "bundle",
                ..
            })
        ));

        let mut policy = snapshot.policies.remove(0);
        policy.canonicalization_version = "unknown-model".into();
        snapshot.canonicalization_version = "cf-model-json-1".into();
        snapshot.policies.push(policy);
        assert!(matches!(
            write_bundle_xccdf_export(&snapshot),
            Err(XccdfWriterError::UnsupportedDigestMetadata {
                object: "policy",
                ..
            })
        ));
    }

    #[test]
    fn policy_identity_includes_enabled_default() {
        let pv = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("enabled-default=\"true\""),
            "enabled-default must be emitted"
        );
    }

    #[test]
    fn xml_escaping_of_special_chars() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "Policy <with> &special \"chars\"".into(),
            description: Some("Desc <html> & entities".into()),
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(!xml.contains("<with>"), "Angle bracket should be escaped");
        assert!(!xml.contains("&special"), "Ampersand should be escaped");
        assert!(xml.contains("&lt;with&gt;"), "Escaped angle brackets");
        assert!(xml.contains("&amp;special"), "Escaped ampersand");
    }

    #[test]
    fn literal_cdata_closing_in_config_does_not_break_xml() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "CDATA Test".into(),
            description: Some("Contains ]]>evil text".into()),
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            !xml.contains("]]>"),
            "Literal ]]> should not appear unescaped"
        );
        assert!(xml.contains("</Benchmark>"), "XML should be well-formed");
    }

    #[test]
    fn standard_ident_elements_from_compliance_metadata() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "STIG Rule".into(),
            description: None,
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({
                "identifiers": [
                    {"system": "http://cyber.mil/cci", "value": "CCI-002322"},
                    {"system": "http://example.com/ids", "value": "EX-001"}
                ]
            }),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("system=\"http://cyber.mil/cci\""));
        assert!(xml.contains("CCI-002322"));
        assert!(xml.contains("system=\"http://example.com/ids\""));
        assert!(xml.contains("EX-001"));
        let ident_count = xml.matches("<ident ").count();
        assert_eq!(ident_count, 2, "Expected 2 ident elements");
    }

    #[test]
    fn standard_reference_elements_from_compliance_metadata() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "STIG Rule".into(),
            description: None,
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({
                "references": [
                    {"href": "https://example.com/stig/V1R1", "title": "STIG V1R1"},
                    {"href": "https://example.com/blank", "title": ""}
                ]
            }),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("href=\"https://example.com/stig/V1R1\""));
        assert!(xml.contains("STIG V1R1"));
        assert!(xml.contains("href=\"https://example.com/blank\""));
        let ref_count = xml.matches("<reference ").count();
        assert_eq!(ref_count, 2, "Expected 2 reference elements");
    }

    #[test]
    fn implementation_state_metadata_element() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:policy"));
        assert!(xml.contains("cf:execution"));
        assert!(xml.contains("cf:implementation"));
    }

    #[test]
    fn execution_phase_metadata_element() {
        let pv = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:execution"));
        assert!(xml.contains("phase=\"nix-evaluation\""));
        assert!(xml.contains("strict=\"true\""));
    }

    #[test]
    fn source_mappings_emitted_when_present() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("cf:source-mappings"),
            "Missing source-mappings element"
        );
        assert!(xml.contains("cf:source"), "Missing cf:source element");
        assert!(
            xml.contains("cf:object-kind"),
            "Missing cf:object-kind element"
        );
    }

    #[test]
    fn opaque_xml_preserved() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "Opaque Rule".into(),
            description: None,
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Opaque,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml: Some("<custom:data>value</custom:data>".into()),
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:opaque-xml"));
        assert!(xml.contains("custom:data"));
    }

    #[test]
    fn dependencies_emitted_as_nix_option_attributes() {
        let pv = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        let mut pv = pv;
        pv.dependencies = json!(["nixos/modules/services/nginx.nix"]);
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("cf:dependencies"),
            "Missing cf:dependencies element"
        );
        // Must be path attribute, NOT text content
        assert!(
            xml.contains("path=\"nixos/modules/services/nginx.nix\""),
            "nix-option must use path attribute: {xml}"
        );
        assert!(
            xml.contains("cf:nix-option"),
            "Missing cf:nix-option element"
        );
    }

    #[test]
    fn module_ref_dependency_is_emitted_as_typed_cf_content() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.dependencies = json!([{
            "kind": "module_ref",
            "uri": "https://example.test/module",
            "optional": false
        }]);
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(xml.contains("cf:module-ref"));
        assert!(xml.contains("uri=\"https://example.test/module\""));
        assert!(xml.contains("optional=\"false\""));
    }

    #[test]
    fn malformed_dependency_is_rejected_instead_of_dropped() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.dependencies = json!([{"kind": "module_ref", "uri": "https://example.test/module"}]);
        assert!(matches!(
            write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])),
            Err(XccdfWriterError::MissingConfig { .. })
        ));
    }

    #[test]
    fn no_dependencies_when_empty_array() {
        let pv = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            !xml.contains("<cf:dependencies>"),
            "Empty dependencies array should not emit cf:dependencies"
        );
    }

    #[test]
    fn custom_check_expression_escaping() {
        let pv = test_policy(
            "custom_check",
            ImplementationState::Native,
            json!({
                "expression": "a < b && c > d",
                "description": "Test & escaping",
                "field_name": "test",
                "strict": true,
                "mode": "all",
                "context": "nixos-configuration-v1",
                "binding": "cfg"
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            !xml.contains("a < b && c > d"),
            "Expression should be XML-escaped"
        );
        assert!(xml.contains("a &lt; b &amp;&amp; c &gt; d"));
        assert!(xml.contains("Test &amp; escaping"));
    }

    #[test]
    fn severity_mapping_for_all_types() {
        let high_types = ["require_cve_check", "require_approvals", "cve_threshold"];
        let medium_types = [
            "require_cf_agent",
            "require_packages",
            "custom_check",
            "time_window",
            "canary_rollout",
        ];
        for t in &high_types {
            assert_eq!(severity_for_type(t), "high", "{t} should be high severity");
        }
        for t in &medium_types {
            assert_eq!(
                severity_for_type(t),
                "medium",
                "{t} should be medium severity"
            );
        }
    }

    // ── Round-trip tests ───────────────────────────────────────────────────────

    use super::super::models::CheckBody;
    use super::super::models::DocumentClass;
    use super::super::parser::parse_xccdf;
    use crate::compliance::interchange::InterchangeLimits;

    #[test]
    fn dual_checks_survive_real_writer_parser_round_trip() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "urn:example:oval",
                "selector": "oval:example:selector",
                "multi-check": "1",
                "negate": "0",
                "content": "Evaluate the imported standards check."
            }
        });
        let snapshot = make_single_policy_snapshot(vec![policy]);
        let xml = write_bundle_xccdf_export(&snapshot).unwrap();
        let parsed = parse_xccdf(
            xml.as_bytes(),
            Some("round-trip.xml"),
            &InterchangeLimits::default(),
        )
        .unwrap();
        assert!(
            parsed.errors.is_empty(),
            "unexpected parser errors: {:?}",
            parsed.errors
        );

        let checks = &parsed.rules[0].checks;
        assert_eq!(
            checks.len(),
            2,
            "dual-check export must parse as two checks"
        );

        let imported = &checks[0];
        assert_eq!(imported.system, "urn:example:oval");
        assert_eq!(imported.selector.as_deref(), Some("oval:example:selector"));
        assert_eq!(imported.multi_check, Some(true));
        assert_eq!(imported.negate, Some(false));
        assert!(matches!(
            &imported.body,
            CheckBody::Inline { content }
                if content == "Evaluate the imported standards check."
        ));

        let crystal_forge = &checks[1];
        assert_eq!(crystal_forge.system, CF_POLICY_CHECK_SYSTEM);
        assert_eq!(crystal_forge.selector, None);
        assert_eq!(crystal_forge.multi_check, None);
        assert_eq!(crystal_forge.negate, None);
        assert!(matches!(&crystal_forge.body, CheckBody::Inline { .. }));
    }

    fn full_test_snapshot() -> XccdfBundleExport {
        let bundle_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        let bundle_version_id = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();

        let p1_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let p1_vid = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let p2_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let p2_vid = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();
        let p3_id = Uuid::parse_str("55555555-5555-5555-5555-555555555555").unwrap();
        let p3_vid = Uuid::parse_str("66666666-6666-6666-6666-666666666666").unwrap();
        let p4_id = Uuid::parse_str("77777777-7777-7777-7777-777777777777").unwrap();
        let p4_vid = Uuid::parse_str("88888888-8888-8888-8888-888888888888").unwrap();

        XccdfBundleExport {
            bundle_id,
            bundle_version_id,
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "abc123".into(),
            digest_algorithm: "sha-256".into(),
            canonicalization_version: "cf-model-json-1".into(),
            name: "Test Bundle".into(),
            description: Some("A test bundle for round-trip".into()),
            framework: "STIG".into(),
            framework_version: Some("V1R1".into()),
            layer: "os".into(),
            owner: "Team".into(),
            groups: vec![],
            policies: vec![
                // CF agent policy – native, nix-evaluation
                XccdfPolicyExport {
                    policy_id: p1_id,
                    policy_version_id: p1_vid,
                    version: "1.0".into(),
                    publication_state: PublicationState::Accepted,
                    semantic_digest: "digest1".into(),
                    digest_algorithm: "sha-256".into(),
                    canonicalization_version: "cf-model-json-1".into(),
                    name: "CF Agent Policy".into(),
                    description: Some("Requires CF agent".into()),
                    policy_type: "require_cf_agent".into(),
                    execution_phase: "nix-evaluation".into(),
                    implementation_state: ImplementationState::Native,
                    enabled_default: true,
                    selected: true,
                    policy_order: 1,
                    config: json!({}),
                    compliance_metadata: json!({}),
                    dependencies: json!([]),
                    opaque_xml: None,
                    source_mappings: vec![XccdfSourceMapping {
                        object_kind: "policy".into(),
                        source_identity: "stig://1234".into(),
                        fidelity: "high".into(),
                    }],
                },
                // custom_check policy – native, nix-evaluation
                XccdfPolicyExport {
                    policy_id: p2_id,
                    policy_version_id: p2_vid,
                    version: "1.0".into(),
                    publication_state: PublicationState::Draft,
                    semantic_digest: "digest2".into(),
                    digest_algorithm: "sha-256".into(),
                    canonicalization_version: "cf-model-json-1".into(),
                    name: "Custom Check Policy".into(),
                    description: Some("Multi-rule custom check".into()),
                    policy_type: "custom_check".into(),
                    execution_phase: "nix-evaluation".into(),
                    implementation_state: ImplementationState::Native,
                    enabled_default: true,
                    selected: true,
                    policy_order: 2,
                    config: json!({
                        "mode": "all",
                        "context": "nixos-configuration-v1",
                        "binding": "cfg",
                        "rules": [
                            {
                                "expression": "cfg.config.networking.firewall.enable",
                                "description": "Firewall enabled",
                                "field_name": "firewallEnabled",
                                "strict": true
                            },
                            {
                                "expression": "cfg.config.services.openssh.enable",
                                "description": "SSH enabled",
                                "field_name": "sshEnabled",
                                "strict": false
                            }
                        ]
                    }),
                    compliance_metadata: json!({
                        "identifiers": [
                            {"system": "http://cyber.mil/cci", "value": "CCI-002322"}
                        ]
                    }),
                    dependencies: json!([]),
                    opaque_xml: None,
                    source_mappings: vec![],
                },
                // cve_threshold policy – native, deployment-orchestration
                XccdfPolicyExport {
                    policy_id: p3_id,
                    policy_version_id: p3_vid,
                    version: "2.0".into(),
                    publication_state: PublicationState::Draft,
                    semantic_digest: "digest3".into(),
                    digest_algorithm: "sha-256".into(),
                    canonicalization_version: "cf-model-json-1".into(),
                    name: "CVE Threshold".into(),
                    description: Some("Blocks deployment if CVEs exceed threshold".into()),
                    policy_type: "cve_threshold".into(),
                    execution_phase: "deployment-orchestration".into(),
                    implementation_state: ImplementationState::Native,
                    enabled_default: true,
                    selected: true,
                    policy_order: 3,
                    config: json!({
                        "no_scan_behavior": "block",
                        "allow_justifications": false,
                        "require_acknowledgment": true,
                        "thresholds": [
                            {"severity": "critical", "max": 0, "action": "block"},
                            {"severity": "high", "max": 3, "action": "warn"}
                        ]
                    }),
                    compliance_metadata: json!({}),
                    dependencies: json!(["nixos/modules/cve.nix"]),
                    opaque_xml: None,
                    source_mappings: vec![],
                },
                // manual policy – Manual state, pre-deployment
                XccdfPolicyExport {
                    policy_id: p4_id,
                    policy_version_id: p4_vid,
                    version: "1.0".into(),
                    publication_state: PublicationState::Incomplete,
                    semantic_digest: "digest4".into(),
                    digest_algorithm: "sha-256".into(),
                    canonicalization_version: "cf-model-json-1".into(),
                    name: "Manual Review".into(),
                    description: Some("Requires manual review".into()),
                    policy_type: "require_approvals".into(),
                    execution_phase: "pre-deployment".into(),
                    implementation_state: ImplementationState::Manual,
                    enabled_default: true,
                    selected: true,
                    policy_order: 4,
                    config: json!({
                        "count": 2,
                        "role": "admin",
                        "distinct": true,
                        "description": "Two admins required"
                    }),
                    compliance_metadata: json!({}),
                    dependencies: json!([]),
                    opaque_xml: None,
                    source_mappings: vec![],
                },
            ],
        }
    }

    /// Round-trip: write → parse → verify CF-native classification.
    #[test]
    fn round_trip_classifies_as_cf_native() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), Some("export.xml"), &limits).unwrap();
        assert!(
            matches!(
                parsed.class,
                DocumentClass::CfNativeExact | DocumentClass::CfNativeUnsupportedExtension
            ),
            "Expected CF-native classification, got {:?}",
            parsed.class
        );
    }

    /// Round-trip: benchmark_id survives.
    #[test]
    fn round_trip_preserves_benchmark_id() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let expected_bench_id = snap.benchmark_id();
        let bm = parsed.benchmark.expect("benchmark present");
        assert_eq!(bm.id, expected_bench_id);
    }

    /// Round-trip: benchmark title survives.
    #[test]
    fn round_trip_preserves_benchmark_title() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let bm = parsed.benchmark.expect("benchmark present");
        assert_eq!(bm.title.as_deref(), Some("Test Bundle"));
    }

    /// Round-trip: rule count matches.
    #[test]
    fn round_trip_preserves_rule_count() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert_eq!(parsed.rules.len(), snap.policies.len());
    }

    /// Round-trip: CF policy identity metadata is preserved for native rules.
    #[test]
    fn round_trip_preserves_cf_policy_meta() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let rules_with_meta: Vec<_> = parsed
            .rules
            .iter()
            .filter(|r| r.cf_policy_meta.is_some())
            .collect();
        assert!(
            !rules_with_meta.is_empty(),
            "At least one rule should have cf_policy_meta"
        );
        let meta = rules_with_meta[0].cf_policy_meta.as_ref().unwrap();
        let expected_policy = snap
            .policies
            .iter()
            .find(|p| p.policy_id == meta.policy_id)
            .expect("matching policy in snapshot");
        assert_eq!(meta.policy_id, expected_policy.policy_id);
        assert_eq!(meta.policy_version_id, expected_policy.policy_version_id);
    }

    /// Round-trip: standard ident elements are preserved.
    #[test]
    fn round_trip_preserves_identifiers() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let rule = parsed
            .rules
            .iter()
            .find(|r| !r.identifiers.is_empty())
            .expect("rule with identifiers");
        let ident = rule
            .identifiers
            .iter()
            .find(|i| i.system == "http://cyber.mil/cci")
            .expect("CCI ident");
        assert_eq!(ident.value, "CCI-002322");
    }

    /// Round-trip: profile with selects survives.
    #[test]
    fn round_trip_preserves_profile_selects() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(!parsed.profiles.is_empty(), "Profile should be present");
        let profile = &parsed.profiles[0];
        assert_eq!(profile.select_ids.len(), snap.policies.len());
    }

    /// Round-trip: no blocking errors on well-formed input.
    #[test]
    fn round_trip_has_no_blocking_errors() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let blocking: Vec<_> = parsed.errors.iter().filter(|e| e.blocking).collect();
        assert!(
            blocking.is_empty(),
            "No blocking errors expected, got: {blocking:?}"
        );
    }

    // ── Group tree tests ──────────────────────────────────────────────────────

    #[test]
    fn two_authored_policy_types_do_not_cause_infinite_recursion() {
        // Before the fix, two authored policies with no group_id both had
        // source_id = None, so None == None in the child-matching would cause
        // each to treat the other as a child, causing stack overflow.
        let p1 = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        let mut p2 = test_policy(
            "require_packages",
            ImplementationState::Native,
            json!({"packages": ["curl"]}),
        );
        p2.policy_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        p2.policy_version_id = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();
        // Both policies have no group_id metadata — this is the crash path.
        let snap = make_single_policy_snapshot(vec![p1, p2]);
        // Must not panic.
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("<Benchmark"), "Should produce valid output");
        // Both rules must be present.
        assert_eq!(xml.matches("<Rule ").count(), 2, "Both rules must appear");
    }

    #[test]
    fn orphaned_child_group_is_promoted_to_root() {
        // A policy with parent_group_id referencing a group that has no policies
        // should still appear in the output, not be silently dropped.
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.compliance_metadata = json!({
            "group_id": "xccdf_crystalforge_group_child",
            "group_title": "Child Group",
            "parent_group_id": "nonexistent-parent-that-has-no-policies"
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("<Rule "),
            "Orphaned child's rule must still appear"
        );
    }

    // ── Standard check/fix tests ──────────────────────────────────────────────

    #[test]
    fn imported_inline_check_content_is_preserved_for_non_native_policy() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the firewall is enabled."
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(xml.contains("http://oval.mitre.org/XMLSchema/oval-definitions-5"));
        assert!(xml.contains("Verify the firewall is enabled."));
        // Must use the imported system, not CF_POLICY_CHECK_SYSTEM.
        assert!(
            !xml.contains("urn:crystal-forge:check-system:policy:1"),
            "Non-native check must use imported system"
        );
    }

    #[test]
    fn reference_only_check_is_always_rejected() {
        // Reference-only checks are always rejected because the current writer
        // returns a single XML document, not a ZIP or SCAP package, so the
        // referenced external content cannot be included in the export.
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content_ref_href": "oval-definitions.xml",
                "content_ref_name": "oval:com.example:def:1"
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedCheck { .. })
            ),
            "Reference-only check must be rejected"
        );
    }

    #[test]
    fn reference_only_check_with_long_opaque_xml_is_still_rejected() {
        // Even with a long opaque_xml, reference-only checks are rejected
        // because opaque_xml is a CF extension element, not a usable inline
        // check body.
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content_ref_href": "oval-definitions.xml",
                "content_ref_name": "oval:com.example:def:1"
            }
        });
        policy.opaque_xml = Some("<oval-definitions xmlns=\"http://oval.mitre.org/schema/oval-definitions-5\"><definition id=\"oval:com.example:def:1\" class=\"compliance\"><metadata><title>Check firewall</title></metadata></definition></oval-definitions>".to_owned());
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedCheck { .. })
            ),
            "Reference-only check with long opaque_xml must still be rejected"
        );
    }

    #[test]
    fn reference_only_check_with_trivial_opaque_xml_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content_ref_href": "oval-definitions.xml",
                "content_ref_name": "oval:com.example:def:1"
            }
        });
        policy.opaque_xml = Some("<custom-xml/>".to_owned());
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedCheck { .. })
            ),
            "Reference check with trivial opaque_xml must be rejected"
        );
    }

    #[test]
    fn native_policy_with_imported_check_emits_both_standard_and_cf_checks() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify firewall is enabled."
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        // Must contain the imported standard check system
        assert!(
            xml.contains("http://oval.mitre.org/XMLSchema/oval-definitions-5"),
            "Standard check system must be preserved"
        );
        assert!(
            xml.contains("Verify firewall is enabled."),
            "Standard check content must be preserved"
        );
        // Must also contain the CF executable check
        assert!(
            xml.contains("urn:crystal-forge:check-system:policy:1"),
            "CF check system must also be present"
        );
        assert!(
            xml.contains("cf:policy"),
            "CF policy element must be present"
        );
    }

    #[test]
    fn standard_check_multi_check_and_negate_attributes_are_preserved() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the firewall is enabled.",
                "multi-check": "true",
                "negate": "true"
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(
            xml.contains("multi-check=\"true\""),
            "multi-check attribute must be preserved"
        );
        assert!(
            xml.contains("negate=\"true\""),
            "negate attribute must be preserved"
        );
    }

    #[test]
    fn standard_check_multi_check_false_and_negate_zero_are_preserved() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the setting.",
                "multi-check": "false",
                "negate": "0"
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(
            xml.contains("multi-check=\"false\""),
            "multi-check=false must be preserved"
        );
        assert!(
            xml.contains("negate=\"false\""),
            "negate=0 must be emitted as negate=false"
        );
    }

    #[test]
    fn check_with_both_inline_and_reference_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://example.com/check",
                "content": "Inline text",
                "content_ref_href": "some-ref.xml"
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedCheck { .. })
            ),
            "Ambiguous check body must be rejected"
        );
    }

    #[test]
    fn check_with_ref_name_without_href_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://example.com/check",
                "content_ref_name": "def:1"
                // no href
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedCheck { .. })
            ),
            "Ref name without href must be rejected"
        );
    }

    #[test]
    fn check_with_missing_system_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                // system is absent
                "content": "Some check"
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedCheck { .. })
            ),
            "Missing system must be rejected"
        );
    }

    #[test]
    fn imported_fix_complexity_and_disruption_are_preserved() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.compliance_metadata = json!({
            "fix": {
                "id": "F-001r1_fix",
                "system": "urn:example:fix",
                "content": "Apply the fix.",
                "complexity": "medium",
                "disruption": "low"
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(xml.contains("complexity=\"medium\""));
        assert!(xml.contains("disruption=\"low\""));
        assert!(
            xml.contains("id=\"F-001r1_fix\""),
            "Imported fix ID must be preserved"
        );
    }

    #[test]
    fn imported_fix_with_invalid_complexity_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.compliance_metadata = json!({
            "fix": {
                "system": "urn:example:fix",
                "content": "Apply the fix.",
                "complexity": "extreme"
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedFix { .. })
            ),
            "Invalid fix complexity must be rejected"
        );
    }

    #[test]
    fn imported_fix_with_invalid_disruption_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.compliance_metadata = json!({
            "fix": {
                "system": "urn:example:fix",
                "content": "Apply the fix.",
                "disruption": "catastrophic"
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedFix { .. })
            ),
            "Invalid fix disruption must be rejected"
        );
    }

    #[test]
    fn imported_fix_with_invalid_ncname_id_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.compliance_metadata = json!({
            "fix": {
                "id": "has spaces and special@chars!",
                "system": "urn:example:fix",
                "content": "Apply the fix."
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedFix { .. })
            ),
            "Fix ID with invalid NCName characters must be rejected"
        );
    }

    #[test]
    fn imported_fix_with_empty_system_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::Native, json!({}));
        policy.compliance_metadata = json!({
            "fix": {
                "system": "",
                "content": "Apply the fix."
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedFix { .. })
            ),
            "Fix with empty system must be rejected"
        );
    }

    // ── Boolean parsing tests ──────────────────────────────────────────────

    #[test]
    fn negate_one_is_emitted_as_true() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the setting.",
                "negate": "1"
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(
            xml.contains("negate=\"true\""),
            "negate=\"1\" must be emitted as negate=\"true\""
        );
    }

    #[test]
    fn negate_zero_is_emitted_as_false() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the setting.",
                "negate": "0"
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(
            xml.contains("negate=\"false\""),
            "negate=\"0\" must be emitted as negate=\"false\""
        );
    }

    #[test]
    fn multi_check_true_string_is_preserved() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the setting.",
                "multi-check": "true"
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(
            xml.contains("multi-check=\"true\""),
            "multi-check=\"true\" must be preserved"
        );
    }

    #[test]
    fn multi_check_false_string_is_preserved() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the setting.",
                "multi-check": "false"
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(
            xml.contains("multi-check=\"false\""),
            "multi-check=\"false\" must be preserved"
        );
    }

    #[test]
    fn negate_yes_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the setting.",
                "negate": "yes"
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedCheck { .. })
            ),
            "negate=\"yes\" must be rejected as invalid XSD boolean"
        );
    }

    #[test]
    fn multi_check_invalid_value_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the setting.",
                "multi-check": "on"
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedCheck { .. })
            ),
            "multi-check=\"on\" must be rejected as invalid XSD boolean"
        );
    }

    #[test]
    fn json_boolean_negate_is_accepted() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the setting.",
                "negate": true
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(
            xml.contains("negate=\"true\""),
            "JSON boolean negate=true must be accepted and emitted as negate=\"true\""
        );
    }

    #[test]
    fn json_boolean_multi_check_false_is_accepted() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the setting.",
                "multi-check": false
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(
            xml.contains("multi-check=\"false\""),
            "JSON boolean multi-check=false must be accepted and emitted"
        );
    }

    // ── Unsupported attribute tests ─────────────────────────────────────────

    #[test]
    fn unsupported_check_attribute_is_rejected() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the setting.",
                "unknown-attr": "some-value"
            }
        });
        let snap = make_single_policy_snapshot(vec![policy]);
        assert!(
            matches!(
                write_bundle_xccdf_export(&snap),
                Err(XccdfWriterError::MalformedImportedCheck { .. })
            ),
            "Unknown check attributes must cause export rejection"
        );
    }

    // ── Inline check content test ───────────────────────────────────────────

    #[test]
    fn inline_check_content_exports_successfully() {
        let mut policy = test_policy("require_cf_agent", ImplementationState::External, json!({}));
        policy.compliance_metadata = json!({
            "check": {
                "system": "http://oval.mitre.org/XMLSchema/oval-definitions-5",
                "content": "Verify the firewall is enabled."
            }
        });
        let xml = write_bundle_xccdf_export(&make_single_policy_snapshot(vec![policy])).unwrap();
        assert!(
            xml.contains("check-content"),
            "Inline check must emit check-content element"
        );
        assert!(
            xml.contains("Verify the firewall is enabled."),
            "Inline check content must be preserved"
        );
    }
}
