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
    if let Some(failure) = parsed {
        view.reason_status = "classified".into();
        if matches!(role, Role::Operator | Role::Admin) {
            view.exit_code = failure.exit_code;
            view.attempt_count = failure.attempt_count;
            view.max_attempts = failure.max_attempts;
        }
        if role == Role::Admin {
            // SECURITY: Older/future captures may carry explicit completeness
            // flags that serde's protocol type would otherwise ignore.
            let raw = structured.and_then(|value| value.get("diagnostics"));
            let incomplete = raw.is_some_and(|value| {
                ["incomplete", "truncated"]
                    .iter()
                    .any(|key| value.get(key) == Some(&json!(true)))
                    || ["complete", "capture_complete"]
                        .iter()
                        .any(|key| value.get(key) == Some(&json!(false)))
            });
            if !incomplete {
                view.diagnostics = failure.diagnostics.and_then(sanitize_diagnostics);
            }
        }
    }
    view
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
}
