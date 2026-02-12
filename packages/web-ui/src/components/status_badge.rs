//! Status badge components for health and deployment status indicators.

use dioxus::prelude::*;

use crate::api::models::{DeploymentStatus, HealthStatus};

/// A colored badge displaying a health status.
#[component]
pub fn HealthBadge(status: HealthStatus) -> Element {
    rsx! {
        span {
            class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium {status.color_class()} {status.bg_class()}",
            "{status.label()}"
        }
    }
}

/// A colored badge displaying a deployment status.
#[component]
pub fn DeploymentBadge(status: DeploymentStatus) -> Element {
    rsx! {
        span {
            class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium {status.color_class()}",
            "{status.label()}"
        }
    }
}
