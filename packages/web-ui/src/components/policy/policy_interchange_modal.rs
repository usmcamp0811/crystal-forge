//! Preview-first JSON/TOML policy interchange flow.

use dioxus::prelude::*;

use crate::api::client::{import_policy_interchange, preview_policy_interchange};
use crate::api::models::{PolicyInterchangeImportResponse, PolicyInterchangePreviewResponse};

const MAX_POLICY_UPLOAD_BYTES: u64 = 50 * 1024 * 1024;

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
    let can_commit = preview.read().is_some() && !busy() && error.read().is_none();

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
                            let mut generation = generation;
                            generation += 1;
                            preview.set(None);
                            result.set(None);
                            error.set(None);
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
                                            Ok(bytes) => selected_file.set(Some(ImportFile { bytes: bytes.to_vec(), filename })),
                                            Err(read_error) => error.set(Some(format!("Could not read selected file: {read_error}"))),
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
                                            Err(api_error) => error.set(Some(api_error.to_string())),
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
                                busy.set(true);
                                error.set(None);
                                spawn(async move {
                                    match import_policy_interchange(&file.bytes, &file.filename, &preview_response.source_sha256).await {
                                        Ok(response) => {
                                            result.set(Some(response));
                                            busy.set(false);
                                            on_success.call(());
                                        }
                                        Err(api_error) => {
                                            if api_error.to_string().contains("POLICY_SOURCE_DIGEST_MISMATCH") {
                                                preview.set(None);
                                            }
                                            error.set(Some(api_error.to_string()));
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
