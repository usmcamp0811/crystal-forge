//! System-specific UI components.
//!
//! Components for displaying and interacting with system data,
//! including info cards, status displays, and system-specific forms.

pub mod cards;
pub mod helpers;
pub mod info_row;
pub mod system_card;
pub mod tabs;

pub use cards::{AgentCard, HardwareCard, NetworkCard, SecurityCard, SystemInfoCard};
pub use helpers::{
    EnvStyle, deployment_policy_label, environment_style, format_memory, format_uptime,
};
pub use info_row::{BooleanRow, InfoRow, InfoRowMono, StatusBadge};
pub use system_card::SystemCard;
pub use tabs::{LogLine, LogsTab};
