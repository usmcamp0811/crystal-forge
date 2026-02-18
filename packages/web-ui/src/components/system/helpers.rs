//! Helper functions for system components.
//!
//! Provides utility functions for formatting system data.

/// Format memory size in GB, converting to TB if >= 1000 GB.
pub fn format_memory(gb: f64) -> String {
    if gb >= 1000.0 {
        format!("{:.0} GB", gb / 1000.0)
    } else {
        format!("{:.1} GB", gb)
    }
}

/// Format uptime in seconds to a human-readable string (days, hours, minutes).
pub fn format_uptime(seconds: i64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

/// Get a human-readable label for deployment policy.
pub fn deployment_policy_label(policy: &str) -> String {
    match policy {
        "Immediate" => "Auto-deploy: Immediate".to_string(),
        "Boot Only" => "Auto-deploy: On reboot".to_string(),
        _ => policy.to_string(),
    }
}

/// Style information for environment chips/badges.
pub struct EnvStyle {
    pub chip_bg: &'static str,
    pub chip_text: &'static str,
}

/// Get style classes for environment display based on name.
pub fn environment_style(environment: &str) -> EnvStyle {
    match environment.to_lowercase().as_str() {
        "production" => EnvStyle {
            chip_bg: "bg-emerald-500/20",
            chip_text: "text-emerald-300",
        },
        "staging" => EnvStyle {
            chip_bg: "bg-amber-500/20",
            chip_text: "text-amber-300",
        },
        "development" => EnvStyle {
            chip_bg: "bg-blue-500/20",
            chip_text: "text-blue-300",
        },
        _ => EnvStyle {
            chip_bg: "bg-gray-500/20",
            chip_text: "text-gray-300",
        },
    }
}
