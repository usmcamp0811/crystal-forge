//! MCP server status card of Admin → Server (TASK-534.4, remediation spec
//! section 10.6).
//!
//! The card is read-only. Server enablement and the external URL come from
//! the declarative NixOS configuration (`services.crystal-forge.server.mcp`),
//! so the card renders no toggle and no suspend control: a control that
//! cannot persist would mislead the operator. The external URL is displayed
//! only when the server reports one; the card never derives a URL from the
//! browser location, because the browser origin can differ from the
//! configured public endpoint.

use dioxus::prelude::*;

use crate::api::client::fetch_admin_mcp_status;
use crate::api::models::McpServerStatusResponse;
use crate::components::icon::{Icon, IconName};

/// Text displayed for a value that the server did not report.
const NOT_REPORTED: &str = "Not reported";

/// Returns the label, chip class, and icon of the enablement status.
///
/// The label and icon carry the state, so the chip does not depend on colour
/// alone.
fn mcp_status_presentation(enabled: bool) -> (&'static str, &'static str, IconName) {
    if enabled {
        ("Enabled", "chip chip-healthy", IconName::Check)
    } else {
        ("Disabled", "chip chip-unknown", IconName::X)
    }
}

/// Returns the display label of a login source wire value.
fn login_source_label(source: &str) -> &str {
    match source {
        "oidc" => "OIDC",
        "local" => "Local",
        "" => NOT_REPORTED,
        other => other,
    }
}

/// Returns `value`, or "Not reported" when it is empty.
fn or_not_reported(value: &str) -> &str {
    if value.is_empty() {
        NOT_REPORTED
    } else {
        value
    }
}

/// Returns the display text of the maximum token lifetime.
fn token_lifetime_text(days: Option<u32>) -> String {
    match days {
        Some(1) => "1 day".to_string(),
        Some(days) => format!("{days} days"),
        None => NOT_REPORTED.to_string(),
    }
}

/// Copies `value` to the browser clipboard. A failure is returned and never
/// reported as a successful copy; the URL stays visible for manual copy.
async fn copy_url(value: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen_futures::JsFuture;
        let window = web_sys::window().ok_or_else(|| {
            "Clipboard access is unavailable in this browser context.".to_string()
        })?;
        JsFuture::from(window.navigator().clipboard().write_text(value))
            .await
            .map_err(|_| "Copy failed. Select the URL and copy it manually.".to_string())?;
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = value;
        Err("Clipboard access is available only in a browser.".to_string())
    }
}

/// Renders the read-only MCP server status card. The card loads
/// `GET /admin/mcp/status` itself and offers a retry after a failure.
#[component]
pub fn McpServerCard() -> Element {
    let mut status = use_signal(|| None::<McpServerStatusResponse>);
    let mut loading = use_signal(|| true);
    let mut load_error = use_signal(|| None::<String>);
    let mut reload = use_signal(|| 0_u32);
    // Holds the confirmation or the failure message of the last Copy.
    let mut copy_result = use_signal(|| None::<Result<String, String>>);

    use_effect(move || {
        let _ = *reload.read();
        loading.set(true);
        load_error.set(None);
        spawn(async move {
            match fetch_admin_mcp_status().await {
                Ok(next) => {
                    status.set(Some(next));
                    load_error.set(None);
                }
                Err(error) => {
                    load_error.set(Some(format!("Failed to load MCP server status: {error}")));
                }
            }
            loading.set(false);
        });
    });

    let current = status.read().clone();
    let status_chip = current
        .as_ref()
        .map(|info| mcp_status_presentation(info.enabled));
    let protocol_versions = current
        .as_ref()
        .filter(|info| !info.protocol_versions.is_empty())
        .map(|info| info.protocol_versions.join(", "))
        .unwrap_or_else(|| NOT_REPORTED.to_string());
    let login_source = current
        .as_ref()
        .map(|info| login_source_label(&info.login_source).to_string())
        .unwrap_or_else(|| NOT_REPORTED.to_string());

    rsx! {
        div {
            class: "card",
            style: "padding:16px;grid-column:1 / -1;",
            "data-testid": "mcp-server-card",
            div { style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:12px;gap:12px;flex-wrap:wrap;",
                h3 { style: "margin:0;font-size:13px;font-weight:600;", "MCP server" }
                if let Some((label, class, icon)) = status_chip {
                    span { class: "{class}", "data-testid": "mcp-server-status",
                        Icon { name: icon, size: 11 }
                        "{label}"
                    }
                }
            }

            if *loading.read() {
                div {
                    style: "font-size:12px;color:var(--cf-text-muted);",
                    role: "status",
                    "data-testid": "mcp-server-loading",
                    "Loading MCP server status…"
                }
            } else if let Some(error) = load_error.read().clone() {
                div {
                    class: "sd-callout sd-callout-danger",
                    style: "font-size:12px;display:flex;align-items:center;gap:10px;",
                    role: "alert",
                    "data-testid": "mcp-server-load-error",
                    span { style: "flex:1;", "{error}" }
                    button {
                        class: "btn btn-ghost focus-ring",
                        "aria-label": "Retry loading MCP server status",
                        onclick: move |_| reload += 1,
                        "Retry"
                    }
                }
            } else if let Some(info) = current {
                dl { class: "kv-grid mcp-kv",
                    dt { "External MCP URL" }
                    dd {
                        if let Some(url) = info.public_url.clone() {
                            div { class: "mcp-url-row",
                                span { class: "mono mcp-url", "data-testid": "mcp-server-url", "{url}" }
                                button {
                                    class: "btn btn-ghost focus-ring xs",
                                    r#type: "button",
                                    "aria-label": "Copy external MCP URL",
                                    onclick: move |_| {
                                        let value = url.clone();
                                        spawn(async move {
                                            let outcome = copy_url(&value)
                                                .await
                                                .map(|_| "MCP URL copied to clipboard.".to_string());
                                            copy_result.set(Some(outcome));
                                        });
                                    },
                                    Icon { name: IconName::File, size: 11 }
                                    "Copy"
                                }
                            }
                        } else {
                            span { class: "chip chip-unknown", "data-testid": "mcp-server-url-missing", "URL not configured" }
                        }
                    }
                    dt { "Authentication" }
                    dd {
                        "Crystal Forge access token"
                        if !info.authentication.is_empty() {
                            span { style: "color:var(--cf-text-muted);", " · {info.authentication}" }
                        }
                    }
                    dt { "Login source" }
                    dd { "{login_source}" }
                    dt { "Protocol versions" }
                    dd { "{protocol_versions}" }
                    dt { "Transport" }
                    dd { "{or_not_reported(&info.transport)}" }
                    dt { "Phase" }
                    dd { "{or_not_reported(&info.phase)}" }
                    dt { "Tools" }
                    dd {
                        if info.tools.is_empty() {
                            "{NOT_REPORTED}"
                        } else {
                            ul { class: "mcp-tools", "aria-label": "MCP tools",
                                for tool in info.tools.iter() {
                                    li { key: "{tool}", class: "chip chip-unknown mono", "{tool}" }
                                }
                            }
                        }
                    }
                    dt { "Access tokens" }
                    dd {
                        if info.access_tokens_enabled { "Enabled" } else { "Disabled" }
                    }
                    dt { "Max token lifetime" }
                    dd { "{token_lifetime_text(info.max_token_lifetime_days)}" }
                    dt { "Active tokens" }
                    dd {
                        match info.active_token_count {
                            Some(count) => rsx! { "{count}" },
                            None => rsx! { "{NOT_REPORTED}" },
                        }
                    }
                }

                div { class: "mcp-live", role: "status", aria_live: "polite",
                    match copy_result() {
                        Some(Ok(text)) => rsx! { "{text}" },
                        Some(Err(text)) => rsx! { span { class: "mcp-live-error", "{text}" } },
                        None => rsx! {},
                    }
                }

                for warning in info.warnings.iter() {
                    div {
                        key: "{warning}",
                        class: "sd-callout sd-callout-warn",
                        style: "font-size:12px;margin-top:10px;",
                        "data-testid": "mcp-server-warning",
                        Icon { name: IconName::Warn, size: 13 }
                        span { "{warning}" }
                    }
                }
            } else {
                div { style: "font-size:12px;color:var(--cf-text-muted);", "MCP server status unavailable." }
            }

            p { class: "help", style: "margin:12px 0 0;",
                "Server enablement and the external URL are managed in NixOS configuration ("
                span { class: "mono", "services.crystal-forge.server.mcp" }
                ")."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_status_presentation_uses_text_and_icon() {
        let (label, class, icon) = mcp_status_presentation(true);
        assert_eq!((label, class), ("Enabled", "chip chip-healthy"));
        assert!(matches!(icon, IconName::Check));
        let (label, class, icon) = mcp_status_presentation(false);
        assert_eq!((label, class), ("Disabled", "chip chip-unknown"));
        assert!(matches!(icon, IconName::X));
    }

    #[test]
    fn login_source_label_maps_wire_values() {
        assert_eq!(login_source_label("oidc"), "OIDC");
        assert_eq!(login_source_label("local"), "Local");
        assert_eq!(login_source_label(""), "Not reported");
        assert_eq!(login_source_label("ldap"), "ldap");
    }

    #[test]
    fn or_not_reported_replaces_only_empty_values() {
        assert_eq!(or_not_reported(""), "Not reported");
        assert_eq!(or_not_reported("read-only"), "read-only");
    }

    #[test]
    fn token_lifetime_text_handles_singular_and_missing() {
        assert_eq!(token_lifetime_text(Some(1)), "1 day");
        assert_eq!(token_lifetime_text(Some(90)), "90 days");
        assert_eq!(token_lifetime_text(None), "Not reported");
    }
}
