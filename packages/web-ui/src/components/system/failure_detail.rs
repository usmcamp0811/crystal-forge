//! Presents server-projected failure fields without making authorization decisions.

use crate::api::models::DeploymentFailureView;
use dioxus::prelude::*;

/// Renders approved summary, operational context, and separate diagnostic evidence.
#[component]
pub(crate) fn FailureDetail(failure: DeploymentFailureView) -> Element {
    rsx! {
        div { class: "sd-failure-detail",
            dl {
                div { dt { "Stage" } dd { "{failure.stage}" } }
                div { dt { "Summary" } dd { "{failure.summary}" } }
                div { dt { "Reason status" } dd { "{failure.reason_status}" } }
                if let Some(code) = &failure.code {
                    div { dt { "Code" } dd { "{code}" } }
                }
                if let Some(hint) = &failure.remediation_hint {
                    div { dt { "Remediation" } dd { "{hint}" } }
                }
                if let Some(exit) = failure.exit_code {
                    div { dt { "Exit code" } dd { "{exit}" } }
                }
                if let Some(attempts) = failure.attempt_count {
                    div { dt { "Attempts" } dd { "{attempts}" } }
                }
                if let Some(max) = failure.max_attempts {
                    div { dt { "Maximum attempts" } dd { "{max}" } }
                }
            }
            if let Some(diagnostics) = &failure.diagnostics {
                details {
                    summary { class: "focus-ring", "Diagnostic details" }
                    if let Some(endpoint) = &diagnostics.cache_endpoint {
                        p { "Cache endpoint: {endpoint}" }
                    }
                    if let Some(reason) = &diagnostics.suppression_reason {
                        p { "Diagnostics suppressed: {reason}" }
                    }
                    for (index, line) in diagnostics.lines.iter().enumerate() {
                        pre { key: "{index}", "{line}" }
                    }
                }
            }
        }
    }
}

/// Formats approved structured fields once for the rendered and downloaded log.
pub(crate) fn failure_log_message(failure: &DeploymentFailureView) -> String {
    let summary = if failure.summary.starts_with("Deployment failed") {
        if failure.stage == "Deployment" {
            failure.summary.clone()
        } else {
            format!("{}: {}", failure.stage, failure.summary)
        }
    } else {
        format!("{} failed: {}", failure.stage, failure.summary)
    };
    let mut lines = vec![summary, format!("Reason status: {}", failure.reason_status)];
    if let Some(code) = &failure.code {
        lines.push(format!("Code: {code}"));
    }
    if let Some(hint) = &failure.remediation_hint {
        lines.push(format!("Remediation: {hint}"));
    }
    if let Some(exit) = failure.exit_code {
        lines.push(format!("Exit code: {exit}"));
    }
    if let Some(attempts) = failure.attempt_count {
        lines.push(format!("Attempts: {attempts}"));
    }
    if let Some(max) = failure.max_attempts {
        lines.push(format!("Maximum attempts: {max}"));
    }
    if let Some(diagnostics) = &failure.diagnostics {
        if let Some(endpoint) = &diagnostics.cache_endpoint {
            lines.push(format!("Cache endpoint: {endpoint}"));
        }
        if let Some(reason) = &diagnostics.suppression_reason {
            lines.push(format!("Diagnostics suppressed: {reason}"));
        }
        lines.extend(diagnostics.lines.iter().cloned());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_failure_does_not_repeat_the_failure_prefix() {
        for (stage, expected) in [
            (
                "Deployment",
                "Deployment failed for an unrecognized reason; details are redacted",
            ),
            (
                "Activation",
                "Activation: Deployment failed for an unrecognized reason; details are redacted",
            ),
        ] {
            let failure: DeploymentFailureView = serde_json::from_value(serde_json::json!({
                "stage": stage,
                "summary": "Deployment failed for an unrecognized reason; details are redacted",
                "reason_status": "classified", "code": "unknown"
            }))
            .unwrap();
            let log = failure_log_message(&failure);
            assert_eq!(log.lines().next(), Some(expected));
            assert_eq!(log.matches("Deployment failed").count(), 1);
            assert!(log.contains("Reason status: classified\nCode: unknown"));
        }
    }
}
