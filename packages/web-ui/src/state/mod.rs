//! Application state management for Crystal Forge Web UI.
//!
//! Uses Dioxus signals for reactive state. As the app grows,
//! shared state (e.g. cached dashboard data, selected filters)
//! will live here as context providers.

pub mod app_state;
pub mod auth;
