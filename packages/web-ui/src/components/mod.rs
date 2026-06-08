//! Reusable UI components for Crystal Forge Web UI.
//!
//! This module provides a comprehensive library of UI components organized
//! by domain. Components are designed to be reusable across views.
//!
//! # Organization
//!
//! - `charts` - Data visualization (donut charts, etc.)
//! - `cve` - CVE vulnerability display components
//! - `dashboard` - Dashboard panels (fleet health, build queue, etc.)
//! - `diff` - Diff viewer components
//! - `environments` - Environment management components
//! - `filters` - Filter dropdowns, view toggles, filter bars
//! - `forms` - Form components for data entry
//! - `layout` - Application shell, cards, sidebar, topbar
//! - `modals` - Dialog components
//! - `notifications` - Toast notifications
//! - `tables` - Sortable headers, table containers
//! - `system` - System-specific components
//! - `flake` - Flake-specific components
//! - `builds` - Build control center components
//! - `policy` - Policy management components

// Domain-specific component modules
pub mod builders;
pub mod builds;
pub mod charts;
pub mod cve;
pub mod dashboard;
pub mod diff;
pub mod environments;
pub mod eval_log_modal;
pub mod filters;
pub mod flake;
pub mod forms;
pub mod heartbeat_spinner;
pub mod modals;
pub mod notifications;
pub mod onboarding;
pub mod policy;
pub mod system;
pub mod tables;

// Top-level component modules
pub mod chips;
pub mod icon;
pub mod layout;
pub mod loading;
pub mod stat_card;
pub mod status_badge;
pub mod systems_stat_strip;
pub mod widget_grid;

// Re-exports for convenience
pub use charts::{DonutArc, DonutChartWithLegend, DonutSegment};
pub use chips::{Chip, ChipVariant, EnvBadge, StatusDot};
pub use cve::CvesTab;
pub use diff::DiffViewer;
pub use eval_log_modal::EvalLogModal;
pub use filters::{
    DeploymentFilterDropdown, EnvironmentFilterDropdown, HealthFilterDropdown, MultiSelectDropdown,
    ViewToggle,
};
pub use heartbeat_spinner::HeartbeatSpinner;
pub use icon::{Icon, IconName};
pub use layout::{AppShell, Card, SidebarNav, TopBar};
pub use loading::{DashboardLoadingSpinner, ErrorMessage, LoadingSpinner};
pub use modals::{ConfirmDialog, RollbackConfirmDialog, SyncConfirmDialog};
pub use notifications::Toast;
pub use onboarding::OnboardingCoachPanel;
pub use stat_card::StatCard;
pub use status_badge::{DeploymentBadge, HealthBadge};
pub use system::{
    AgentCard, BooleanRow, HardwareCard, InfoRow, InfoRowMono, LogLine, LogsTab, NetworkCard,
    SecurityCard, StatusBadge, SystemInfoCard,
};
pub use systems_stat_strip::{SystemsStatStrip, SystemsStats};
pub use tables::{SortDirection, SortableHeader};
pub use widget_grid::{GridWidget, WidgetGrid};
