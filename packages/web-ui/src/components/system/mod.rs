//! System-specific UI components.
//!
//! Components for displaying and interacting with system data,
//! including info cards, status displays, and system-specific forms.

pub mod cards;
pub mod deploy_system_modal;
pub mod edit_system_modal;
pub mod helpers;
pub mod info_row;
pub mod pending_deploy_banner;
pub mod system_card;
pub mod system_card_v2;
pub mod tabs;

pub use cards::{AgentCard, HardwareCard, NetworkCard, SecurityCard, SystemInfoCard};
pub use edit_system_modal::EditSystemModal;
pub use helpers::{
    deployment_policy_label, deployment_state_label, environment_style, format_memory,
    format_uptime, EnvStyle,
};
pub use info_row::{BooleanRow, InfoRow, InfoRowMono, StatusBadge};
pub use pending_deploy_banner::PendingDeployBanner;
pub use system_card::SystemCard;
pub use system_card_v2::SystemCardV2;
pub use tabs::{LogLine, LogsTab};
