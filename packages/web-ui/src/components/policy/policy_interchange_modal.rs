//! Preview-first JSON/TOML policy interchange flow.

use dioxus::prelude::*;

use crate::api::client::{import_policy_interchange, preview_policy_interchange, ApiClientError};
use crate::api::models::{PolicyInterchangeImportResponse, PolicyInterchangePreviewResponse};

const MAX_POLICY_UPLOAD_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
enum PolicyImportDiagnosticSeverity {
    Error,
    Warning,
    Information,
}

#[derive(Clone, Debug, PartialEq)]
struct PolicyImportDiagnostic {
    severity: PolicyImportDiagnosticSeverity,
    code: String,
    message: String,
    policy_index: Option<usize>,
    policy_name: Option<String>,
    field_path: Option<String>,
}

impl PolicyImportDiagnostic {
    fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: PolicyImportDiagnosticSeverity::Error,
            code: code.into(),
            message: message.into(),
            policy_index: None,
            policy_name: None,
            field_path: None,
        }
    }

    fn blocks_commit(&self) -> bool {
        self.severity == PolicyImportDiagnosticSeverity::Error
    }
}

fn normalize_policy_import_error(error: &ApiClientError) -> Vec<PolicyImportDiagnostic> {
    let ApiClientError::Status { code, body } = error else {
        return vec![PolicyImportDiagnostic::error(
            "NETWORK_ERROR",
            error.to_string(),
        )];
    };

    if *code >= 500 {
        return vec![PolicyImportDiagnostic::error(
            "SERVER_ERROR",
            "The server could not process the policy interchange request.",
        )];
    }

    let payload = serde_json::from_str::<serde_json::Value>(body).ok();
    let error_code = payload
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("HTTP_{code}"));
    let message = payload
        .as_ref()
        .and_then(|value| value.get("message").or_else(|| value.get("error")))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| "The policy interchange request was rejected.".to_string());

    if *code == 409 && error_code == "POLICY_SOURCE_DIGEST_MISMATCH" {
        return vec![PolicyImportDiagnostic::error(
            error_code,
            "The selected file changed after preview. Preview it again before importing.",
        )];
    }

    if let Some(conflicts) = payload
        .as_ref()
        .and_then(|value| value.get("conflicts"))
        .and_then(serde_json::Value::as_array)
    {
        let diagnostics = conflicts
            .iter()
            .filter_map(|conflict| {
                let message = conflict.get("message")?.as_str()?.to_string();
                let code = conflict
                    .get("code")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("POLICY_IMPORT_CONFLICT")
                    .to_string();
                Some(PolicyImportDiagnostic::error(code, message))
            })
            .collect::<Vec<_>>();
        if !diagnostics.is_empty() {
            return diagnostics;
        }
    }

    let (code, message) = match *code {
        403 => (
            "AUTHORIZATION_REQUIRED".to_string(),
            "Administrator permission is required to import policies.".to_string(),
        ),
        413 => ("POLICY_FILE_TOO_LARGE".to_string(), message),
        415 => ("POLICY_FORMAT_UNSUPPORTED".to_string(), message),
        422 => (error_code, message),
        _ => (error_code, message),
    };
    vec![PolicyImportDiagnostic::error(code, message)]
}

#[derive(Clone, Debug, PartialEq)]
struct ImportFile {
    bytes: Vec<u8>,
    filename: String,
}

#[component]
pub fn PolicyInterchangeModal(on_close: EventHandler<()>, on_success: EventHandler<()>) -> Element {
    let mut selected_file = use_signal(|| None::<ImportFile>);
    let mut preview = use_signal(|| None::<PolicyInterchangePreviewResponse>);
    let mut result = use_signal(|| None::<PolicyInterchangeImportResponse>);
    let mut error = use_signal(|| None::<String>);
    let mut diagnostics = use_signal(Vec::<PolicyImportDiagnostic>::new);
    let mut busy = use_signal(|| false);
    let mut generation = use_signal(|| 0u64);

    let file_label = selected_file
        .read()
        .as_ref()
        .map(|file| file.filename.clone())
        .unwrap_or_else(|| "No file selected".to_string());
    let file_size = selected_file
        .read()
        .as_ref()
        .map(|file| file.bytes.len() as u64)
        .unwrap_or(0);
    let can_preview =
        selected_file.read().is_some() && !busy() && file_size <= MAX_POLICY_UPLOAD_BYTES;
    let diagnostics_snapshot = diagnostics.read().clone();
    let can_commit = preview.read().is_some()
        && !busy()
        && error.read().is_none()
        && !diagnostics_snapshot
            .iter()
            .any(PolicyImportDiagnostic::blocks_commit);

    rsx! {
        div {
            class: "modal-backdrop cf-modal-overlay-z50",
            role: "presentation",
            onclick: move |_| if !busy() { on_close.call(()) },
            div {
                class: "modal cf-policy-modal-panel",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "policy-interchange-title",
                onclick: |event| event.stop_propagation(),
                div { class: "modal-head",
                    div {
                        h2 { id: "policy-interchange-title", "Import policies" }
                        p { "Preview the exact JSON or TOML file before it is persisted." }
                    }
                    button {
                        class: "btn-icon focus-ring",
                        aria_label: "Close policy import",
                        disabled: busy(),
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }
                div { class: "modal-body", style: "display:flex;flex-direction:column;gap:14px;overflow-y:auto;",
                    label { class: "text-sm font-medium", "Policy interchange file" }
                    input {
                        r#type: "file",
                        accept: ".json,.toml,application/json,application/toml",
                        aria_label: "Choose a JSON or TOML policy file",
                        onchange: move |event| {
                            let mut selected_file = selected_file;
                            let mut preview = preview;
                            let mut result = result;
                            let mut error = error;
                            let mut diagnostics = diagnostics;
                            let mut generation = generation;
                            generation += 1;
                            let file_generation = generation();
                            busy.set(false);
                            preview.set(None);
                            result.set(None);
                            error.set(None);
                            diagnostics.set(Vec::new());
                            let files = event.files();
                            if let Some(file) = files.into_iter().next() {
                                let filename = file.name();
                                let size = file.size();
                                if size > MAX_POLICY_UPLOAD_BYTES {
                                    error.set(Some(format!("File exceeds the 50 MiB upload limit ({size} bytes).")));
                                    selected_file.set(None);
                                } else {
                                    spawn(async move {
                                        match file.read_bytes().await {
                                            Ok(bytes) if generation() == file_generation => selected_file.set(Some(ImportFile { bytes: bytes.to_vec(), filename })),
                                            Err(read_error) => error.set(Some(format!("Could not read selected file: {read_error}"))),
                                            _ => {}
                                        }
                                    });
                                }
                            } else {
                                selected_file.set(None);
                            }
                        },
                    }
                    div { class: "text-xs text-gray-400", "{file_label} · {file_size} bytes · maximum 50 MiB" }
                    if let Some(ref err) = *error.read() {
                        div { class: "sd-callout sd-callout-danger", role: "alert", "{err}" }
                    }
                    for diagnostic in diagnostics_snapshot.iter() {
                        div {
                            class: if diagnostic.severity == PolicyImportDiagnosticSeverity::Error {
                                "sd-callout sd-callout-danger"
                            } else if diagnostic.severity == PolicyImportDiagnosticSeverity::Warning {
                                "sd-callout sd-callout-warning"
                            } else {
                                "sd-callout sd-callout-info"
                            },
                            role: if diagnostic.blocks_commit() { "alert" } else { "status" },
                            strong { "{diagnostic.code}: " }
                            "{diagnostic.message}"
                            if let Some(index) = diagnostic.policy_index {
                                " (policy {index})"
                            }
                            if let Some(name) = diagnostic.policy_name.as_deref() {
                                " — {name}"
                            }
                            if let Some(field) = diagnostic.field_path.as_deref() {
                                " · {field}"
                            }
                        }
                    }
                    div { class: "flex gap-2",
                        button {
                            class: "btn btn-primary focus-ring",
                            disabled: !can_preview,
                            onclick: move |_| {
                                let Some(file) = selected_file.read().clone() else { return; };
                                let mut busy = busy;
                                let mut preview = preview;
                                let mut result = result;
                                let mut error = error;
                                let mut diagnostics = diagnostics;
                                let request_generation = generation();
                                busy.set(true);
                                preview.set(None);
                                result.set(None);
                                error.set(None);
                                spawn(async move {
                                    let response = preview_policy_interchange(&file.bytes, &file.filename).await;
                                    if generation() == request_generation {
                                        match response {
                                            Ok(response) => preview.set(Some(response)),
                                            Err(api_error) => {
                                                diagnostics.set(normalize_policy_import_error(
                                                    &api_error,
                                                ));
                                            }
                                        }
                                        busy.set(false);
                                    }
                                });
                            },
                            if busy() { "Previewing…" } else { "Preview import" }
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            disabled: !can_commit,
                            onclick: move |_| {
                                let Some(file) = selected_file.read().clone() else { return; };
                                let Some(preview_response) = preview.read().clone() else { return; };
                                let mut busy = busy;
                                let mut result = result;
                                let mut error = error;
                                let mut diagnostics = diagnostics;
                                busy.set(true);
                                error.set(None);
                                spawn(async move {
                                    match import_policy_interchange(&file.bytes, &file.filename, &preview_response.source_sha256).await {
                                        Ok(response) => {
                                            result.set(Some(response));
                                            selected_file.set(None);
                                            preview.set(None);
                                            busy.set(false);
                                            on_success.call(());
                                        }
                                        Err(api_error) => {
                                            let next_diagnostics = normalize_policy_import_error(&api_error);
                                            if next_diagnostics.iter().any(|diagnostic| {
                                                diagnostic.code == "POLICY_SOURCE_DIGEST_MISMATCH"
                                            }) {
                                                preview.set(None);
                                            }
                                            diagnostics.set(next_diagnostics);
                                            busy.set(false);
                                        }
                                    }
                                });
                            },
                            if busy() { "Importing…" } else { "Commit import" }
                        }
                    }
                    if let Some(response) = preview.read().clone() {
                        div { class: "cf-policy-interchange-preview",
                            div { class: "text-xs text-gray-400", "Source SHA-256: " span { class: "mono", "{response.source_sha256}" } }
                            div { class: "text-xs text-gray-400", "{response.policy_count} policies · proposed draft · disabled · untrusted" }
                            div { class: "flex flex-col gap-2",
                                for policy in response.policies {
                                    div { class: "rounded border border-gray-700 px-3 py-2",
                                        div { class: "font-medium", "{policy.name}" }
                                        div { class: "text-xs text-gray-400", "{policy.policy_type} · {policy.implementation_state}" }
                                        div { class: "text-[11px] text-gray-500 mono", "lineage {policy.lineage_id} · version {policy.version_id}" }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(response) = result.read().clone() {
                        div { class: "sd-callout sd-callout-success", role: "status",
                            "Imported {response.created_policy_count} policies; reused {response.reused_policy_count} exact versions."
                        }
                    }
                }
                div { class: "modal-foot",
                    button { class: "btn btn-ghost focus-ring", disabled: busy(), onclick: move |_| on_close.call(()), "Close" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_policy_import_error, PolicyImportDiagnosticSeverity};
    use crate::api::client::ApiClientError;

    #[test]
    fn digest_mismatch_is_a_blocking_typed_diagnostic() {
        let diagnostics = normalize_policy_import_error(&ApiClientError::Status {
            code: 409,
            body: r#"{"error":"POLICY_SOURCE_DIGEST_MISMATCH","expected":"a","actual":"b"}"#
                .to_string(),
        });

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "POLICY_SOURCE_DIGEST_MISMATCH");
        assert_eq!(
            diagnostics[0].severity,
            PolicyImportDiagnosticSeverity::Error
        );
        assert!(diagnostics[0].blocks_commit());
    }

    #[test]
    fn validation_diagnostic_preserves_available_fields_without_raw_internals() {
        let diagnostics = normalize_policy_import_error(&ApiClientError::Status {
            code: 422,
            body: r#"{"error":"POLICY_INTERCHANGE_INVALID","message":"config.rules is required","details":{"internal":"secret"}}"#
                .to_string(),
        });

        assert_eq!(diagnostics[0].code, "POLICY_INTERCHANGE_INVALID");
        assert_eq!(diagnostics[0].message, "config.rules is required");
        assert!(!diagnostics[0].message.contains("secret"));
    }

    #[test]
    fn server_errors_are_generic() {
        let diagnostics = normalize_policy_import_error(&ApiClientError::Status {
            code: 500,
            body: r#"{"error":"Internal Server Error","message":"sql details"}"#.to_string(),
        });

        assert_eq!(diagnostics[0].code, "SERVER_ERROR");
        assert_eq!(
            diagnostics[0].message,
            "The server could not process the policy interchange request."
        );
        assert!(!diagnostics[0].message.contains("sql"));
    }

    #[test]
    fn conflict_list_becomes_separate_blocking_diagnostics() {
        let diagnostics = normalize_policy_import_error(&ApiClientError::Status {
            code: 422,
            body: r#"{"error":"Assignment resolution conflict","conflicts":[{"code":"POLICY_VERSION_DIGEST_CONFLICT","message":"digest differs"}]}"#
                .to_string(),
        });

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "POLICY_VERSION_DIGEST_CONFLICT");
        assert!(diagnostics[0].blocks_commit());
    }
}
