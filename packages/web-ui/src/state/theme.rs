//! UI theme state and browser persistence helpers.

use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "cf.ui.theme";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiTheme {
    Dark,
    Light,
}

impl UiTheme {
    pub fn toggle(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    pub fn as_attr(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    pub fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(value)) = storage.get_item(STORAGE_KEY) {
                        if value == "light" {
                            return Self::Light;
                        }
                        if value == "dark" {
                            return Self::Dark;
                        }
                    }
                }
            }
        }

        Self::Dark
    }
}

pub fn apply(theme: UiTheme) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = theme;

    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(root) = document.document_element() {
                    let _ = root.set_attribute("data-theme", theme.as_attr());
                }
            }
        }
    }
}

pub fn persist(theme: UiTheme) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = theme;

    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item(STORAGE_KEY, theme.as_attr());
            }
        }
    }
}
