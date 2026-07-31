//! User preferences state and browser persistence helpers.
//!
//! Manages UI preferences beyond theme: density, sidebar mode, default view,
//! and notification settings. All preferences persist to localStorage.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Density Preference
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
const DENSITY_KEY: &str = "cf.ui.density";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Comfortable,
    Compact,
}

impl Density {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Comfortable => "Comfort",
            Self::Compact => "Compact",
        }
    }

    pub fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(value)) = storage.get_item(DENSITY_KEY) {
                        if value == "compact" {
                            return Self::Compact;
                        }
                    }
                }
            }
        }

        Self::Comfortable
    }

    pub fn persist(self) {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self;

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    let _ = storage.set_item(DENSITY_KEY, self.as_str());
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sidebar Mode Preference
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
const SIDEBAR_MODE_KEY: &str = "cf.ui.sidebarMode";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarMode {
    Full,
    Rail,
}

impl SidebarMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Rail => "rail",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Rail => "Rail",
        }
    }

    pub fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(value)) = storage.get_item(SIDEBAR_MODE_KEY) {
                        if value == "rail" {
                            return Self::Rail;
                        }
                    }
                }
            }
        }

        Self::Full
    }

    pub fn persist(self) {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self;

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    let _ = storage.set_item(SIDEBAR_MODE_KEY, self.as_str());
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Default View Preference
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
const DEFAULT_VIEW_KEY: &str = "cf.ui.defaultView";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultView {
    Cards,
    Table,
}

impl DefaultView {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cards => "cards",
            Self::Table => "table",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cards => "Cards",
            Self::Table => "Table",
        }
    }

    pub fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(value)) = storage.get_item(DEFAULT_VIEW_KEY) {
                        if value == "table" {
                            return Self::Table;
                        }
                    }
                }
            }
        }

        Self::Cards
    }

    pub fn persist(self) {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self;

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    let _ = storage.set_item(DEFAULT_VIEW_KEY, self.as_str());
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Notification Channel Preference
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationChannel {
    InApp,
    Email,
    Both,
}

impl NotificationChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InApp => "in-app",
            Self::Email => "email",
            Self::Both => "both",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::InApp => "In-app",
            Self::Email => "Email",
            Self::Both => "Both",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Notification Preferences
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
const NOTIFICATIONS_KEY: &str = "cf.ui.notifications";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPreferences {
    pub deploy_failed: bool,
    pub build_failed: bool,
    pub critical_cve: bool,
    pub policy_fail: bool,
    pub heartbeat_lost: bool,
    pub weekly_digest: bool,
    pub channel: NotificationChannel,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            deploy_failed: true,
            build_failed: true,
            critical_cve: true,
            policy_fail: true,
            heartbeat_lost: false,
            weekly_digest: true,
            channel: NotificationChannel::InApp,
        }
    }
}

impl NotificationPreferences {
    pub fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(json)) = storage.get_item(NOTIFICATIONS_KEY) {
                        if let Ok(prefs) = serde_json::from_str(&json) {
                            return prefs;
                        }
                    }
                }
            }
        }

        Self::default()
    }

    pub fn persist(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self;

        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(json) = serde_json::to_string(self) {
                if let Some(window) = web_sys::window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        let _ = storage.set_item(NOTIFICATIONS_KEY, &json);
                    }
                }
            }
        }
    }
}
