//! Lightweight cross-view focus state for deep-link navigation.

use dioxus::prelude::*;
use std::collections::BTreeMap;

/// A System Detail tab that can be represented in a deep link.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SystemDetailTab {
    /// Shows the system summary.
    #[default]
    Overview,
    /// Shows deployment target selection.
    Deploy,
    /// Shows deployment history.
    History,
    /// Shows deployment logs.
    Logs,
    /// Shows evaluated configuration.
    Config,
    /// Shows vulnerabilities.
    Cves,
    /// Shows hardening results.
    Hardening,
    /// Shows compliance results.
    Compliance,
}

impl SystemDetailTab {
    /// Returns the stable query value for the tab.
    pub fn as_query_value(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Deploy => "deploy",
            Self::History => "history",
            Self::Logs => "logs",
            Self::Config => "config",
            Self::Cves => "cves",
            Self::Hardening => "hardening",
            Self::Compliance => "compliance",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "overview" => Some(Self::Overview),
            "deploy" => Some(Self::Deploy),
            "history" => Some(Self::History),
            "logs" => Some(Self::Logs),
            "config" => Some(Self::Config),
            "cves" => Some(Self::Cves),
            "hardening" => Some(Self::Hardening),
            "compliance" => Some(Self::Compliance),
            _ => None,
        }
    }
}

/// Identifies the exact revision selected on the Config tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigRevision {
    /// Uses the system's current deployed generation.
    Current,
    /// Uses an exact retained generation number.
    Generation(i32),
    /// Uses an exact immutable commit SHA.
    Commit(String),
}

/// URL-backed state for System Detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemDetailNavigation {
    /// Selected tab.
    pub tab: SystemDetailTab,
    /// Selected Config revision.
    pub config_revision: ConfigRevision,
    /// Exact generation preselected after a History rollback action.
    pub deploy_generation: Option<i32>,
}

impl Default for SystemDetailNavigation {
    fn default() -> Self {
        Self {
            tab: SystemDetailTab::Overview,
            config_revision: ConfigRevision::Current,
            deploy_generation: None,
        }
    }
}

impl SystemDetailNavigation {
    /// Parses System Detail state from a URL query string.
    pub fn from_query(search: &str) -> Self {
        let query = parse_query(search);
        let tab = query
            .get("tab")
            .and_then(|value| SystemDetailTab::parse(value))
            .unwrap_or_default();
        let config_revision = match query.get("config_mode").map(String::as_str) {
            Some("commit") => query
                .get("revision")
                .filter(|sha| is_full_commit_sha(sha))
                .cloned()
                .map(ConfigRevision::Commit)
                .unwrap_or(ConfigRevision::Current),
            Some("generation") => query
                .get("generation")
                .and_then(|value| value.parse::<i32>().ok())
                .map(ConfigRevision::Generation)
                .unwrap_or(ConfigRevision::Current),
            _ => ConfigRevision::Current,
        };
        let deploy_generation = query
            .get("deploy_generation")
            .and_then(|value| value.parse::<i32>().ok());

        Self {
            tab,
            config_revision,
            deploy_generation,
        }
    }

    /// Writes System Detail state while preserving unrelated query parameters.
    pub fn to_query(&self, current_search: &str) -> String {
        let mut query = parse_query(current_search);
        for key in [
            "tab",
            "config_mode",
            "revision",
            "generation",
            "deploy_generation",
        ] {
            query.remove(key);
        }
        if self.tab != SystemDetailTab::Overview {
            query.insert("tab".to_string(), self.tab.as_query_value().to_string());
        }
        if self.tab == SystemDetailTab::Config {
            match &self.config_revision {
                ConfigRevision::Current => {}
                ConfigRevision::Generation(generation) => {
                    query.insert("config_mode".to_string(), "generation".to_string());
                    query.insert("generation".to_string(), generation.to_string());
                }
                ConfigRevision::Commit(sha) => {
                    if is_full_commit_sha(sha) {
                        query.insert("config_mode".to_string(), "commit".to_string());
                        query.insert("revision".to_string(), sha.clone());
                    }
                }
            }
        }
        if self.tab == SystemDetailTab::Deploy {
            if let Some(generation) = self.deploy_generation {
                query.insert("deploy_generation".to_string(), generation.to_string());
            }
        }
        render_query(&query)
    }
}

/// A revision-scoped pane in the flake tray.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlakePane {
    /// Shows commits and file changes.
    #[default]
    Commits,
    /// Shows declared and managed systems.
    Systems,
    /// Shows exported modules.
    Modules,
    /// Shows resolved inputs.
    Inputs,
}

impl FlakePane {
    /// Returns the stable query value for the pane.
    pub fn as_query_value(self) -> &'static str {
        match self {
            Self::Commits => "commits",
            Self::Systems => "systems",
            Self::Modules => "modules",
            Self::Inputs => "inputs",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "commits" => Some(Self::Commits),
            "systems" => Some(Self::Systems),
            "modules" => Some(Self::Modules),
            "inputs" => Some(Self::Inputs),
            _ => None,
        }
    }
}

/// URL-backed state for an open flake tray.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlakeNavigation {
    /// Exact registry identifier for the open flake.
    pub flake_id: Option<i32>,
    /// Flake name fallback used by cross-surface links without a registry ID.
    pub flake_name: Option<String>,
    /// Selected tray pane.
    pub pane: FlakePane,
    /// Exact immutable selected commit SHA.
    pub revision: Option<String>,
    /// Environment panel to restore when the tray closes.
    pub return_environment: Option<String>,
}

impl FlakeNavigation {
    /// Parses flake tray state from a URL query string.
    pub fn from_query(search: &str) -> Self {
        let query = parse_query(search);
        Self {
            flake_id: query.get("flake").and_then(|value| value.parse().ok()),
            flake_name: query
                .get("flake_name")
                .filter(|value| !value.is_empty())
                .cloned(),
            pane: query
                .get("pane")
                .and_then(|value| FlakePane::parse(value))
                .unwrap_or_default(),
            revision: query
                .get("revision")
                .filter(|value| is_full_commit_sha(value))
                .cloned(),
            return_environment: query
                .get("return_environment")
                .filter(|value| !value.is_empty())
                .cloned(),
        }
    }

    /// Writes flake tray state while preserving unrelated query parameters.
    pub fn to_query(&self, current_search: &str) -> String {
        let mut query = parse_query(current_search);
        for key in [
            "flake",
            "flake_name",
            "pane",
            "revision",
            "return_environment",
        ] {
            query.remove(key);
        }
        if let Some(id) = self.flake_id {
            query.insert("flake".to_string(), id.to_string());
        } else if let Some(name) = self.flake_name.as_ref() {
            query.insert("flake_name".to_string(), name.clone());
        }
        if self.flake_id.is_some() || self.flake_name.is_some() {
            query.insert("pane".to_string(), self.pane.as_query_value().to_string());
            if let Some(revision) = self.revision.as_ref() {
                if is_full_commit_sha(revision) {
                    query.insert("revision".to_string(), revision.clone());
                }
            }
            if let Some(environment) = self.return_environment.as_ref() {
                query.insert("return_environment".to_string(), environment.clone());
            }
        }
        render_query(&query)
    }

    /// Returns state with all tray and return context removed.
    pub fn cleared() -> Self {
        Self::default()
    }
}

fn is_full_commit_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_query(search: &str) -> BTreeMap<String, String> {
    search
        .trim_start_matches('?')
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| part.split_once('=').unwrap_or((part, "")))
        .map(|(key, value)| (percent_decode(key), percent_decode(value)))
        .collect()
}

fn render_query(query: &BTreeMap<String, String>) -> String {
    if query.is_empty() {
        String::new()
    } else {
        format!(
            "?{}",
            query
                .iter()
                .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
                .collect::<Vec<_>>()
                .join("&")
        )
    }
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Returns the current browser query string.
pub fn current_query() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|window| window.location().search().ok())
            .unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        String::new()
    }
}

/// Updates the current URL without reloading the Dioxus application.
pub fn update_query(search: &str, push: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(window) = web_sys::window() else {
            return;
        };
        let pathname = window.location().pathname().ok().unwrap_or_default();
        let hash = window.location().hash().ok().unwrap_or_default();
        if let Ok(history) = window.history() {
            let url = format!("{pathname}{search}{hash}");
            if push {
                let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
            } else {
                let _ =
                    history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (search, push);
    }
}

/// Target view for a cross-surface focus request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    /// Builds view focus.
    Builds,
    /// Evaluations view focus.
    Evaluations,
    /// Policies view focus.
    Policies,
    /// Systems view focus.
    Systems,
}

impl Default for FocusTarget {
    fn default() -> Self {
        Self::Builds
    }
}

/// Shared navigation focus payload.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NavigationFocus {
    pub target: FocusTarget,
    pub commit_sha: Option<String>,
    pub flake_name: Option<String>,
    pub status: Option<String>,
    pub policy_name: Option<String>,
}

/// Provide the global navigation focus signal.
pub fn provide_navigation_focus() {
    use_context_provider(|| Signal::new(None::<NavigationFocus>));
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigRevision, FlakeNavigation, FlakePane, SystemDetailNavigation, SystemDetailTab,
    };

    #[test]
    fn system_detail_navigation_round_trips_exact_revision_and_clears_stale_fields() {
        let state = SystemDetailNavigation {
            tab: SystemDetailTab::Config,
            config_revision: ConfigRevision::Commit(
                "abcdef0123456789abcdef0123456789abcdef01".to_string(),
            ),
            deploy_generation: Some(41),
        };
        let query = state.to_query("?notice=keep&generation=9&deploy_generation=8");
        let parsed = SystemDetailNavigation::from_query(&query);

        assert_eq!(parsed.tab, SystemDetailTab::Config);
        assert_eq!(parsed.config_revision, state.config_revision);
        assert_eq!(parsed.deploy_generation, None);
        assert!(query.contains("notice=keep"));
        assert!(!query.contains("generation=9"));
        assert!(!query.contains("deploy_generation"));
    }

    #[test]
    fn flake_navigation_round_trips_full_sha_pane_and_return_context() {
        let state = FlakeNavigation {
            flake_id: Some(17),
            flake_name: None,
            pane: FlakePane::Inputs,
            revision: Some("1234567890abcdef1234567890abcdef12345678".to_string()),
            return_environment: Some("26ee295d-7f12-48ae-99b5-2ccf07716782".to_string()),
        };
        let query = state.to_query("?stale=yes");
        assert_eq!(FlakeNavigation::from_query(&query), state);
        assert_eq!(FlakeNavigation::cleared().to_query(&query), "?stale=yes");
    }

    #[test]
    fn invalid_or_incomplete_navigation_values_fall_back_without_aliasing() {
        let system = SystemDetailNavigation::from_query(
            "?tab=config&config_mode=commit&revision=&generation=12",
        );
        assert_eq!(system.config_revision, ConfigRevision::Current);

        let flake = FlakeNavigation::from_query("?flake=nope&pane=unknown&revision=abcdef0");
        assert_eq!(flake.flake_id, None);
        assert_eq!(flake.pane, FlakePane::Commits);
        assert_eq!(flake.revision, None);
    }

    #[test]
    fn flake_names_are_url_encoded_and_decoded() {
        let state = FlakeNavigation {
            flake_name: Some("Production & edge".to_string()),
            ..Default::default()
        };
        let query = state.to_query("");
        assert!(query.contains("flake_name=Production%20%26%20edge"));
        assert_eq!(FlakeNavigation::from_query(&query), state);
    }

    #[test]
    fn revision_navigation_accepts_complete_40_or_64_hex_shas() {
        for invalid in [
            "abcdef0",
            "abcdef0123456789abcdef0123456789abcdef0g",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg",
        ] {
            let system = SystemDetailNavigation::from_query(&format!(
                "?tab=config&config_mode=commit&revision={invalid}"
            ));
            assert_eq!(system.config_revision, ConfigRevision::Current);
            assert_eq!(
                FlakeNavigation::from_query(&format!("?flake=1&revision={invalid}")).revision,
                None
            );
        }

        for full in [
            "ABCDEF0123456789abcdef0123456789abcdef01",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ] {
            assert_eq!(
                FlakeNavigation::from_query(&format!("?flake=1&revision={full}"))
                    .revision
                    .as_deref(),
                Some(full)
            );
            assert_eq!(
                SystemDetailNavigation::from_query(&format!(
                    "?tab=config&config_mode=commit&revision={full}"
                ))
                .config_revision,
                ConfigRevision::Commit(full.to_string())
            );
        }
    }
}
