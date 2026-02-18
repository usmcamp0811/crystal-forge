//! Reusable UI components for Crystal Forge Web UI.
//!
//! This module provides a comprehensive library of UI components organized
//! by domain. Components are designed to be reusable across views.
//!
//! # Organization
//!
//! - `charts` - Data visualization (donut charts, etc.)
//! - `dashboard` - Dashboard panels (fleet health, build queue, etc.)
//! - `filters` - Filter dropdowns, view toggles, filter bars
//! - `forms` - Form components for data entry
//! - `layout` - Application shell, cards, sidebar, topbar
//! - `modals` - Dialog components
//! - `tables` - Sortable headers, table containers
//! - `system` - System-specific components
//! - `flake` - Flake-specific components
//! - `builds` - Build control center components
//! - `policy` - Policy management components
//! - `diff` - Diff viewer components

// Domain-specific component modules
pub mod builds;
pub mod charts;
pub mod dashboard;
pub mod diff;
pub mod filters;
pub mod flake;
pub mod forms;
pub mod modals;
pub mod policy;
pub mod system;
pub mod tables;

// Top-level component modules
pub mod flake_timeline;
pub mod layout;
pub mod loading;
pub mod stat_card;
pub mod status_badge;
pub mod widget_grid;

// Re-exports for convenience
pub use charts::{DonutArc, DonutChartWithLegend, DonutSegment};
pub use filters::{
    DeploymentFilterDropdown, EnvironmentFilterDropdown, HealthFilterDropdown, MultiSelectDropdown,
    ViewToggle,
};
pub use layout::{AppShell, Card, SidebarNav, TopBar};
pub use loading::{ErrorMessage, LoadingSpinner};
pub use modals::ConfirmDialog;
pub use stat_card::StatCard;
pub use status_badge::{DeploymentBadge, HealthBadge};
pub use tables::{SortDirection, SortableHeader};
pub use widget_grid::{GridWidget, WidgetGrid};
