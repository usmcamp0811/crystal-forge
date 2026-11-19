//! Projects typed agent failures across the API disclosure boundary.
//!
//! Legacy text is never evidence of redaction, including administrator reads.
use crate::auth::models::Role;
use cf_protocol::deployment_failure::{
    DeploymentFailure, FailureCode, FailureDiagnostics, FailureStage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Describes a deployment failure with fields restricted to the caller's role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentFailureView {
    /// Gives the fixed operation label.
    pub stage: String,
    /// Gives a fixed summary derived from the typed cause.
    pub summary: String,
    /// Distinguishes classified, legacy_agent and incomplete_report data.
    pub reason_status: String,
    /// Gives the typed cause to operators and administrators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Gives fixed operator guidance, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation_hint: Option<String>,
    /// Gives the child exit code to operators and administrators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Counts started attempts for operators and administrators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_count: Option<u32>,
    /// Gives the attempt limit to operators and administrators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    /// Contains bounded, sanitized diagnostics for administrators only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<FailureDiagnostics>,
}

impl DeploymentFailureView {
    /// Formats role-permitted summary context without duplicating diagnostics.
    pub(crate) fn message(&self) -> String {
        let mut message = if self.summary.starts_with("Deployment failed") {
            if self.stage == "Deployment" {
                self.summary.clone()
            } else {
                format!("{}: {}", self.stage, self.summary)
            }
        } else {
            format!("{} failed: {}", self.stage, self.summary)
        };
        if let Some(code) = &self.code {
            message.push_str(&format!(" [code={code}"));
            if let Some(exit) = self.exit_code {
                message.push_str(&format!(", exit={exit}"));
            }
            if let Some(attempt) = self.attempt_count {
                message.push_str(&format!(", attempts={attempt}"));
                if let Some(max) = self.max_attempts {
                    message.push_str(&format!("/{max}"));
                }
            } else if let Some(max) = self.max_attempts {
                message.push_str(&format!(", max_attempts={max}"));
            }
            message.push(']');
            if let Some(hint) = &self.remediation_hint {
                message.push_str(&format!(" {hint}"));
            }
        }
        message
    }
}

/// Returns fixed failure context and only the fields permitted for the role.
///
/// Missing structured data means a legacy agent. Invalid or partial structured
/// data produces an incomplete-report result without using legacy text. A valid
/// stage and cause remain usable even when optional report fields are malformed.
pub(crate) fn project_failure(metadata: &Value, role: Role) -> DeploymentFailureView {
    let structured = metadata.get("deployment_failure");
    let parsed =
        structured.and_then(|v| serde_json::from_value::<DeploymentFailure>(v.clone()).ok());
    // COMPATIBILITY: Recover only enum-valid values independently. Malformed
    // optional fields must not erase a known operation or fixed cause summary,
    // and a missing cause must never be inferred as the Unknown category.
    let stage = structured
        .and_then(|value| value.get("stage"))
        .and_then(|value| serde_json::from_value::<FailureStage>(value.clone()).ok());
    let code = structured
        .and_then(|value| value.get("code"))
        .and_then(|value| serde_json::from_value::<FailureCode>(value.clone()).ok());
    let mut view = DeploymentFailureView {
        stage: stage.map_or("Deployment", FailureStage::label).into(),
        summary: if structured.is_none() || structured == Some(&Value::Null) {
            "Failure reason unavailable: agent does not support structured reporting"
        } else {
            "Failure reason unavailable: agent supplied an incomplete structured report"
        }
        .into(),
        reason_status: if structured.is_none() || structured == Some(&Value::Null) {
            "legacy_agent"
        } else {
            "incomplete_report"
        }
        .into(),
        code: None,
        remediation_hint: None,
        exit_code: None,
        attempt_count: None,
        max_attempts: None,
        diagnostics: None,
    };
    if let (Some(_), Some(code)) = (stage, code) {
        view.summary = code.summary().into();
        if matches!(role, Role::Operator | Role::Admin) {
            view.code = Some(code.as_str().into());
            view.remediation_hint = code.remediation().map(str::to_owned);
        }
    }
    // A report marked incomplete stays `incomplete_report` even though its typed
    // fields would parse as a complete one. Ingress normalization uses the same
    // test, so the stored and reported forms project identically.
    if let (Some(failure), false) = (parsed, structured.is_some_and(marked_incomplete)) {
        view.reason_status = "classified".into();
        if matches!(role, Role::Operator | Role::Admin) {
            view.exit_code = failure.exit_code;
            view.attempt_count = failure.attempt_count;
            view.max_attempts = failure.max_attempts;
        }
        if role == Role::Admin {
            let raw = structured.and_then(|value| value.get("diagnostics"));
            if !raw.is_some_and(diagnostics_flagged_incomplete) {
                view.diagnostics = failure.diagnostics.and_then(sanitize_diagnostics);
            }
        }
    }
    view
}

/// Returns whether a report declares itself incomplete with a top-level marker.
///
/// Only the JSON boolean `true` counts. Any other value is an unknown field,
/// which normalization drops and projection ignores, so both still agree.
fn marked_incomplete(report: &Value) -> bool {
    report.get("incomplete").and_then(Value::as_bool) == Some(true)
}

// SECURITY: Older or future captures may carry explicit completeness flags that
// serde's protocol type would otherwise ignore. Diagnostics so flagged are never
// displayed or stored.
fn diagnostics_flagged_incomplete(raw: &Value) -> bool {
    ["incomplete", "truncated"]
        .iter()
        .any(|key| raw.get(key) == Some(&json!(true)))
        || ["complete", "capture_complete"]
            .iter()
            .any(|key| raw.get(key) == Some(&json!(false)))
}

/// Bounds the serialized failure report that the server persists.
///
/// A valid report is under 2 KiB: at most three 300-character lines, one
/// 300-character endpoint and a few fixed fields. A larger report carries
/// content that the server would never display, so it is not stored.
pub(crate) const MAX_REPORTED_FAILURE_BYTES: usize = 8 * 1024;

/// A reported failure after ingress normalization.
pub(crate) struct NormalizedFailure {
    /// The only failure JSON that is persisted, under `deployment_failure`.
    ///
    /// `Null` marks a legacy agent. `{"incomplete": true, ...}` carries only an
    /// enum-valid stage and cause. Otherwise it is the typed report with
    /// sanitized diagnostics.
    pub(crate) stored: Value,
    /// The complete typed cause, used for terminal-state decisions. `None` for a
    /// legacy or incomplete report.
    pub(crate) typed: Option<DeploymentFailure>,
}

/// Normalizes an agent's reported failure before it is persisted.
///
/// SECURITY: An authenticated agent is not trusted to bound or sanitize what it
/// stores. Unknown fields, free-form strings and oversized diagnostics are
/// dropped here, not only at display time. Persisted content is limited to the
/// stage and cause enums, numeric fields, and diagnostics that pass
/// [`sanitize_diagnostics`]. The role projection of the stored form equals that
/// of the reported form, except that an oversized report becomes incomplete.
pub(crate) fn normalize_reported_failure(reported: Option<&Value>) -> NormalizedFailure {
    let Some(raw) = reported.filter(|value| !value.is_null()) else {
        return NormalizedFailure {
            stored: Value::Null,
            typed: None,
        };
    };
    let oversized =
        serde_json::to_vec(raw).map_or(true, |bytes| bytes.len() > MAX_REPORTED_FAILURE_BYTES);
    // SECURITY: Check the marker before typed deserialization. Serde ignores
    // unknown fields, so a typed round trip would silently drop it and turn a
    // self-declared incomplete report into a complete one with a typed cause,
    // which could then authorize a terminal-state override.
    if oversized || marked_incomplete(raw) {
        return incomplete_failure(raw);
    }
    let Ok(mut failure) = serde_json::from_value::<DeploymentFailure>(raw.clone()) else {
        return incomplete_failure(raw);
    };
    failure.diagnostics = if raw
        .get("diagnostics")
        .is_some_and(diagnostics_flagged_incomplete)
    {
        None
    } else {
        failure.diagnostics.and_then(sanitize_diagnostics)
    };
    match serde_json::to_value(&failure) {
        Ok(stored) => NormalizedFailure {
            stored,
            typed: Some(failure),
        },
        Err(_) => incomplete_failure(raw),
    }
}

// Keeps only enum-valid values, recovered independently, so that a known
// operation or fixed cause survives while arbitrary invalid content does not.
fn incomplete_failure(raw: &Value) -> NormalizedFailure {
    let mut stored = json!({ "incomplete": true });
    if let Some(stage) = raw
        .get("stage")
        .and_then(|value| serde_json::from_value::<FailureStage>(value.clone()).ok())
    {
        stored["stage"] = json!(stage);
    }
    if let Some(code) = raw
        .get("code")
        .and_then(|value| serde_json::from_value::<FailureCode>(value.clone()).ok())
    {
        stored["code"] = json!(code);
    }
    NormalizedFailure {
        stored,
        typed: None,
    }
}

// SECURITY: Only protocol-sized masked diagnostics are eligible for display.
// Agent-side secret masking remains required; this removes generic URL secrets
// and independently suppresses provider response bodies and incomplete capture.
fn sanitize_diagnostics(mut diagnostics: FailureDiagnostics) -> Option<FailureDiagnostics> {
    if let Some(reason) = diagnostics.suppression_reason.as_deref() {
        if !matches!(
            reason,
            "provider_response_body" | "incomplete_capture" | "credential_inventory_unavailable"
        ) {
            return None;
        }
        diagnostics.lines.clear();
        diagnostics.cache_endpoint = None;
        return Some(diagnostics);
    }
    if diagnostics.lines.len() > 3
        || diagnostics.lines.iter().any(|line| {
            line.chars().count() > 300
                || !line.trim_start().starts_with("error:")
                || line.to_ascii_lowercase().contains("response body:")
                || line.to_ascii_lowercase().contains("incomplete")
                || line.chars().any(char::is_control)
        })
    {
        return None;
    }
    diagnostics.lines = diagnostics
        .lines
        .iter()
        .map(|line| sanitize_urls(line))
        .collect();
    if diagnostics
        .lines
        .iter()
        .any(|line| line.chars().count() > 300)
    {
        return None;
    }
    diagnostics.cache_endpoint = diagnostics
        .cache_endpoint
        .filter(|value| value.chars().count() <= 300 && !value.chars().any(char::is_control))
        .and_then(|value| {
            let mut url = url::Url::parse(&value).ok()?;
            url.set_username("").ok()?;
            url.set_password(None).ok()?;
            url.set_query(None);
            url.set_fragment(None);
            let endpoint = url.to_string();
            (endpoint.chars().count() <= 300).then_some(endpoint)
        });
    Some(diagnostics)
}

fn sanitize_urls(line: &str) -> String {
    line.split_whitespace()
        .map(|word| {
            if let Some(start) = word.find("://") {
                let scheme_start = word[..start]
                    .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '+' && c != '-' && c != '.')
                    .map_or(0, |i| i + 1);
                if let Ok(mut url) = url::Url::parse(&word[scheme_start..]) {
                    let _ = url.set_username("");
                    let _ = url.set_password(None);
                    url.set_query(None);
                    url.set_fragment(None);
                    return format!("{}{}", &word[..scheme_start], url);
                }
                return "****".into();
            }
            word.to_owned()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whitelists correlation metadata; arbitrary stored strings never cross APIs.
pub(crate) fn failure_metadata(metadata: &Value) -> Value {
    let mut safe = json!({});
    if let Some(id) = metadata
        .get("pending_deployment_id")
        .and_then(Value::as_str)
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
    {
        safe["pending_deployment_id"] = json!(id);
    }
    safe
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> Value {
        json!({
            "deployment_failure": {
                "stage": "cache_acquisition", "code": "dns", "exit_code": 1,
                "attempt_count": 3, "max_attempts": 3,
                "diagnostics": {"lines": ["error: lookup failed"],
                    "cache_endpoint": "https://user:password@cache.test/path?token=secret",
                    "suppression_reason": null}
            },
            "display_failure_message": "legacy-secret", "failure_message": "raw-secret",
            "unknown_future_key": "future-secret"
        })
    }

    #[test]
    fn whole_failure_json_enforces_role_matrix() {
        let viewer = json!({
            "stage": "Cache acquisition", "summary": "DNS resolution of the cache host failed",
            "reason_status": "classified"
        });
        assert_eq!(
            serde_json::to_value(project_failure(&metadata(), Role::Viewer)).unwrap(),
            viewer
        );
        let mut operator = viewer.clone();
        operator["code"] = json!("dns");
        operator["exit_code"] = json!(1);
        operator["attempt_count"] = json!(3);
        operator["max_attempts"] = json!(3);
        operator["remediation_hint"] = json!("Check DNS configuration and cache hostname.");
        assert_eq!(
            serde_json::to_value(project_failure(&metadata(), Role::Operator)).unwrap(),
            operator
        );
        let mut admin = operator;
        admin["diagnostics"] = json!({"lines": ["error: lookup failed"],
            "cache_endpoint": "https://cache.test/path", "suppression_reason": null});
        assert_eq!(
            serde_json::to_value(project_failure(&metadata(), Role::Admin)).unwrap(),
            admin
        );
        assert_eq!(failure_metadata(&metadata()), json!({}));
    }

    #[test]
    fn all_api_dtos_serialize_role_projection_without_legacy_or_hidden_fields() {
        use crate::api::models::{SystemAgentEvent, SystemDeploymentProgress, SystemHistoryEntry};
        for role in [Role::Viewer, Role::Operator, Role::Admin] {
            let metadata = metadata();
            let failure = project_failure(&metadata, role);
            let expected_failure = serde_json::to_value(&failure).unwrap();
            let input = json!({
                "id":"00000000-0000-0000-0000-000000000001",
                "timestamp":"2026-11-17T00:00:00Z", "issued_at":"2026-11-17T00:00:00Z",
                "stage":"failed", "kind":"deployment", "source":"agent_report",
                "target_store_path":"/nix/store/test-system", "event_type":"cf_deployment_failed",
                "change_reason":failure.message(), "title":failure.message(),
                "actor":"agent", "outcome":"failed", "event_kind":"cf_deployment", "reconciled":false,
                "level":"error", "message":failure.message(), "failure_message":failure.message(),
                "deployment_related":true, "metadata":failure_metadata(&metadata), "failure":failure
            });
            let history: SystemHistoryEntry = serde_json::from_value(input.clone()).unwrap();
            let events: SystemAgentEvent = serde_json::from_value(input.clone()).unwrap();
            let progress: SystemDeploymentProgress = serde_json::from_value(input).unwrap();
            for response in [
                serde_json::to_value(history).unwrap(),
                serde_json::to_value(events).unwrap(),
                serde_json::to_value(progress).unwrap(),
            ] {
                assert_eq!(response["failure"], expected_failure);
                let text = response.to_string();
                for forbidden in [
                    "legacy-secret",
                    "raw-secret",
                    "future-secret",
                    "password",
                    "token=",
                ] {
                    assert!(!text.contains(forbidden));
                }
                if role != Role::Admin {
                    assert!(!text.contains("diagnostics"));
                    assert!(!text.contains("cache.test"));
                    assert!(!text.contains("error: lookup"));
                }
                if role == Role::Viewer {
                    for field in [
                        "code",
                        "exit_code",
                        "attempt_count",
                        "max_attempts",
                        "remediation_hint",
                    ] {
                        assert!(!text.contains(&format!("\"{field}\"")));
                    }
                }
            }
        }
    }

    #[test]
    fn legacy_partial_and_unknown_have_fixed_distinct_results() {
        for role in [Role::Viewer, Role::Operator, Role::Admin] {
            assert_eq!(
                serde_json::to_value(project_failure(
                    &json!({"display_failure_message":"secret"}),
                    role
                ))
                .unwrap(),
                json!({"stage":"Deployment", "summary":"Failure reason unavailable: agent does not support structured reporting", "reason_status":"legacy_agent"})
            );
            for (partial, stage) in [
                (json!({}), "Deployment"),
                (json!({"stage":"activation"}), "Activation"),
                (json!({"stage":"bogus","code":"dns"}), "Deployment"),
                (json!({"stage":"activation","code":"bogus"}), "Activation"),
            ] {
                assert_eq!(
                    serde_json::to_value(project_failure(
                        &json!({"deployment_failure":partial}),
                        role
                    ))
                    .unwrap(),
                    json!({"stage":stage, "summary":"Failure reason unavailable: agent supplied an incomplete structured report", "reason_status":"incomplete_report"})
                );
            }
            let unknown = project_failure(
                &json!({"deployment_failure":{"stage":"activation","code":"unknown"}}),
                role,
            );
            assert_eq!(unknown.stage, "Activation");
            assert_eq!(
                unknown.summary,
                "Deployment failed for an unrecognized reason; details are redacted"
            );
            assert_eq!(unknown.reason_status, "classified");
        }
    }

    #[test]
    fn malformed_diagnostics_preserve_known_stage_and_cause_for_every_role() {
        for role in [Role::Viewer, Role::Operator, Role::Admin] {
            for diagnostics in [
                json!("private"),
                json!({"lines":42}),
                json!({"lines":[false]}),
            ] {
                let view = project_failure(
                    &json!({"deployment_failure":{
                        "stage":"cache_acquisition", "code":"dns", "diagnostics":diagnostics
                    }}),
                    role,
                );
                let mut expected = json!({
                    "stage":"Cache acquisition", "summary":FailureCode::Dns.summary(),
                    "reason_status":"incomplete_report"
                });
                if role != Role::Viewer {
                    expected["code"] = json!("dns");
                    expected["remediation_hint"] = json!(FailureCode::Dns.remediation());
                }
                assert_eq!(serde_json::to_value(view).unwrap(), expected);
            }
        }
    }

    #[test]
    fn unknown_summary_is_not_prefixed_with_a_redundant_failure_phrase() {
        for role in [Role::Viewer, Role::Operator, Role::Admin] {
            for (stage, prefix) in [("deployment", ""), ("activation", "Activation: ")] {
                let view = project_failure(
                    &json!({"deployment_failure":{
                        "stage":stage, "code":"unknown"
                    }}),
                    role,
                );
                let suffix = if role == Role::Viewer {
                    ""
                } else {
                    " [code=unknown]"
                };
                assert_eq!(
                    view.message(),
                    format!("{prefix}{}{suffix}", FailureCode::Unknown.summary())
                );
                assert!(!view.message().contains("failed: Deployment failed"));
            }
        }
    }

    #[test]
    fn diagnostics_reject_body_incomplete_oversized_and_unknown_suppression() {
        for diagnostics in [
            json!({"lines":["error: response BODY: secret"]}),
            json!({"lines":["error: incomplete capture"]}),
            json!({"lines":["error: one", "error: two", "error: three", "error: four"]}),
            json!({"lines":[format!("error: {}", "x".repeat(300))]}),
            json!({"lines":["error: secret"], "incomplete":true}),
            json!({"lines":["error: secret"], "capture_complete":false}),
            json!({"lines":["error: secret"], "suppression_reason":"secret"}),
        ] {
            let mut metadata = metadata();
            metadata["deployment_failure"]["diagnostics"] = diagnostics;
            assert!(
                project_failure(&metadata, Role::Admin)
                    .diagnostics
                    .is_none()
            );
        }
        for reason in [
            "provider_response_body",
            "incomplete_capture",
            "credential_inventory_unavailable",
        ] {
            let mut metadata = metadata();
            metadata["deployment_failure"]["diagnostics"]["suppression_reason"] = json!(reason);
            let diagnostics = project_failure(&metadata, Role::Admin).diagnostics.unwrap();
            assert_eq!(diagnostics.lines, Vec::<String>::new());
            assert!(diagnostics.cache_endpoint.is_none());
            assert_eq!(diagnostics.suppression_reason.as_deref(), Some(reason));
        }
        assert_eq!(
            sanitize_urls("error: https://user:pass@cache.test/p?token=secret#fragment failed"),
            "error: https://cache.test/p failed"
        );
    }

    fn view(raw: &Value, role: Role) -> Value {
        serde_json::to_value(project_failure(&json!({ "deployment_failure": raw }), role)).unwrap()
    }

    // Normalization must not change what any role sees. Oversized reports are the
    // one deliberate exception and are covered separately.
    #[test]
    fn normalization_preserves_every_role_projection() {
        let samples = [
            metadata()["deployment_failure"].clone(),
            json!({"stage":"activation","code":"activation_execution","exit_code":17,
                   "attempt_count":1,"max_attempts":1}),
            json!({"stage":"cache_acquisition","code":"dns","future_field":"x",
                   "diagnostics":{"lines":["error: lookup"],"cache_endpoint":null,
                                  "suppression_reason":null,"extra":"y"}}),
            json!({"stage":"cache_acquisition","code":"dns",
                   "diagnostics":{"lines":["error: a","error: b","error: c","error: d"],
                                  "cache_endpoint":null,"suppression_reason":null}}),
            json!({"stage":"cache_acquisition","code":"dns",
                   "diagnostics":{"lines":["error: secret"],"cache_endpoint":null,
                                  "suppression_reason":null,"incomplete":true}}),
            json!({"stage":"cache_acquisition","code":"dns",
                   "diagnostics":{"lines":[],"cache_endpoint":null,
                                  "suppression_reason":"provider_response_body"}}),
            // Valid stage and cause with malformed optional content stay incomplete.
            json!({"stage":"cache_acquisition","code":"dns","diagnostics":"not-an-object"}),
            json!({"stage":"activation","code":"activation_execution","exit_code":"seventeen"}),
            json!({"stage":"activation"}),
            // A self-declared incomplete report, and markers that do not count.
            json!({"stage":"activation","code":"activation_execution","incomplete":true}),
            json!({"stage":"activation","code":"activation_execution","exit_code":17,
                   "incomplete":true}),
            json!({"stage":"activation","code":"activation_execution","incomplete":false}),
            json!({"stage":"activation","code":"activation_execution","incomplete":"yes"}),
            json!({"code":"dns"}),
            json!({"stage":"bogus","code":"dns"}),
            json!({"stage":"activation","code":"bogus"}),
            json!({}),
            json!("a string"),
            json!(["an", "array"]),
            json!(42),
        ];
        for raw in samples {
            let normalized = normalize_reported_failure(Some(&raw));
            for role in [Role::Viewer, Role::Operator, Role::Admin] {
                assert_eq!(
                    view(&raw, role),
                    view(&normalized.stored, role),
                    "{role:?} projection changed for {raw}"
                );
            }
        }
    }

    #[test]
    fn legacy_reports_normalize_to_null() {
        for reported in [None, Some(&Value::Null)] {
            let normalized = normalize_reported_failure(reported);
            assert_eq!(normalized.stored, Value::Null);
            assert!(normalized.typed.is_none());
            assert_eq!(
                view(&normalized.stored, Role::Admin)["reason_status"],
                "legacy_agent"
            );
        }
    }

    #[test]
    fn unknown_fields_and_free_form_strings_are_not_stored() {
        let raw = json!({
            "stage": "cache_acquisition", "code": "dns", "future_field": "SECRET-FUTURE",
            "diagnostics": {"lines": ["error: lookup"], "cache_endpoint": null,
                            "suppression_reason": null, "extra": "SECRET-DIAGNOSTIC"},
            "error": "SECRET-ERROR"
        });
        let normalized = normalize_reported_failure(Some(&raw));
        let stored = normalized.stored.to_string();
        for private in [
            "SECRET-FUTURE",
            "SECRET-DIAGNOSTIC",
            "SECRET-ERROR",
            "future_field",
        ] {
            assert!(!stored.contains(private), "{private} stored in {stored}");
        }
        // Only the typed report's own fields remain.
        let keys: std::collections::BTreeSet<_> = normalized
            .stored
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let expected: std::collections::BTreeSet<_> = [
            "stage",
            "code",
            "exit_code",
            "attempt_count",
            "max_attempts",
            "diagnostics",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        assert_eq!(keys, expected);
        assert!(normalized.typed.is_some());
    }

    #[test]
    fn stored_diagnostics_are_sanitized_before_persistence() {
        let raw = json!({"stage":"cache_acquisition","code":"dns","diagnostics":{
            "lines": ["error: https://user:pw@cache.test/p?token=secret#frag failed"],
            "cache_endpoint": "https://user:pw@cache.test/path?token=secret",
            "suppression_reason": null}});
        let stored = normalize_reported_failure(Some(&raw)).stored.to_string();
        for private in ["user:pw", "pw@", "token=secret", "#frag"] {
            assert!(!stored.contains(private), "{private} stored in {stored}");
        }
        assert!(stored.contains("cache.test"));
    }

    #[test]
    fn oversized_reports_are_stored_as_a_small_fixed_incomplete_marker() {
        let huge = "x".repeat(MAX_REPORTED_FAILURE_BYTES * 4);
        for raw in [
            json!({"stage":"cache_acquisition","code":"dns","diagnostics":{
                "lines":[format!("error: {huge}")],"cache_endpoint":null,"suppression_reason":null}}),
            json!({"stage":"cache_acquisition","code":"dns","unknown": huge}),
        ] {
            let normalized = normalize_reported_failure(Some(&raw));
            let stored = normalized.stored.to_string();
            assert!(
                stored.len() < 200,
                "oversized report stored {} bytes",
                stored.len()
            );
            assert!(!stored.contains("xxxx"));
            assert!(
                normalized.typed.is_none(),
                "an oversized report decides no state"
            );
            // The known operation and cause survive; nothing else does.
            let admin = view(&normalized.stored, Role::Admin);
            assert_eq!(admin["reason_status"], "incomplete_report");
            assert_eq!(admin["stage"], "Cache acquisition");
            assert_eq!(admin["summary"], "DNS resolution of the cache host failed");
            assert!(admin.get("diagnostics").is_none());
        }
    }

    #[test]
    fn incomplete_reports_never_carry_a_typed_cause_for_terminal_decisions() {
        // A complete activation_execution report may override a succeeded request.
        let complete = json!({"stage":"activation","code":"activation_execution","exit_code":17});
        assert!(normalize_reported_failure(Some(&complete)).typed.is_some());
        // The same cause with malformed optional content must not.
        // A report that declares itself incomplete keeps that status, even with
        // every typed field valid.
        for declared in [
            json!({"stage":"activation","code":"activation_execution","incomplete":true}),
            json!({"stage":"activation","code":"activation_execution","exit_code":17,
                   "incomplete":true}),
        ] {
            let normalized = normalize_reported_failure(Some(&declared));
            assert!(normalized.typed.is_none(), "{declared}");
            assert_eq!(
                normalized.stored,
                json!({"incomplete":true,"stage":"activation","code":"activation_execution"})
            );
            assert_eq!(
                view(&normalized.stored, Role::Admin)["reason_status"],
                "incomplete_report"
            );
            assert_eq!(
                view(&declared, Role::Admin)["reason_status"],
                "incomplete_report"
            );
        }
        // Only the boolean `true` is a marker; other values are dropped as unknown.
        let not_marked =
            json!({"stage":"activation","code":"activation_execution","incomplete":"yes"});
        assert!(
            normalize_reported_failure(Some(&not_marked))
                .typed
                .is_some()
        );
        let malformed =
            json!({"stage":"activation","code":"activation_execution","diagnostics":"x"});
        let normalized = normalize_reported_failure(Some(&malformed));
        assert!(normalized.typed.is_none());
        assert_eq!(normalized.stored["incomplete"], true);
        assert_eq!(
            view(&normalized.stored, Role::Viewer)["reason_status"],
            "incomplete_report"
        );
    }
}
