//! Crystal Forge design system — color tokens, typography, and spacing constants.
//!
//! This module centralizes all visual design tokens so that components use a
//! consistent visual language. Colors are expressed as Tailwind CSS class fragments.
//!
//! # Color System
//!
//! Tailwind class fragments in this module provide stable semantic tokens.
//! Shared CSS in `assets/app.css` provides theme-variable mappings for
//! dark/light/custom themes where component-specific styling is required.

// ─────────────────────────────────────────────────────────────────────────────
// Health Status Colors
// ─────────────────────────────────────────────────────────────────────────────

/// Color tokens for system health status indicators.
pub mod health {
    /// Healthy — system reported heartbeat within 15 minutes.
    pub const HEALTHY_TEXT: &str = "text-emerald-400";
    pub const HEALTHY_BG: &str = "bg-emerald-400/10";
    pub const HEALTHY_BORDER: &str = "border-emerald-400/30";
    pub const HEALTHY_DOT: &str = "bg-emerald-400";

    /// Warning — system reported heartbeat within 1 hour.
    pub const WARNING_TEXT: &str = "text-amber-400";
    pub const WARNING_BG: &str = "bg-amber-400/10";
    pub const WARNING_BORDER: &str = "border-amber-400/30";
    pub const WARNING_DOT: &str = "bg-amber-400";

    /// Critical — system reported heartbeat within 4 hours.
    pub const CRITICAL_TEXT: &str = "text-red-400";
    pub const CRITICAL_BG: &str = "bg-red-400/10";
    pub const CRITICAL_BORDER: &str = "border-red-400/30";
    pub const CRITICAL_DOT: &str = "bg-red-400";

    /// Offline — system has not reported for 4+ hours (or never).
    pub const OFFLINE_TEXT: &str = "text-gray-500";
    pub const OFFLINE_BG: &str = "bg-gray-500/10";
    pub const OFFLINE_BORDER: &str = "border-gray-500/30";
    pub const OFFLINE_DOT: &str = "bg-gray-500";
}

// ─────────────────────────────────────────────────────────────────────────────
// Deployment Status Colors
// ─────────────────────────────────────────────────────────────────────────────

/// Color tokens for deployment status indicators.
pub mod deployment {
    pub const UP_TO_DATE_TEXT: &str = "text-emerald-400";
    pub const UP_TO_DATE_BG: &str = "bg-emerald-400/10";

    pub const BEHIND_TEXT: &str = "text-amber-400";
    pub const BEHIND_BG: &str = "bg-amber-400/10";

    pub const AHEAD_TEXT: &str = "text-blue-400";
    pub const AHEAD_BG: &str = "bg-blue-400/10";

    pub const NEVER_DEPLOYED_TEXT: &str = "text-gray-500";
    pub const NEVER_DEPLOYED_BG: &str = "bg-gray-500/10";

    pub const NO_COMMITS_TEXT: &str = "text-gray-500";
    pub const NO_COMMITS_BG: &str = "bg-gray-500/10";

    pub const UNKNOWN_TEXT: &str = "text-gray-500";
    pub const UNKNOWN_BG: &str = "bg-gray-500/10";
}

// ─────────────────────────────────────────────────────────────────────────────
// CVE Severity Colors
// ─────────────────────────────────────────────────────────────────────────────

/// Color tokens for CVE severity levels.
pub mod cve {
    pub const CRITICAL_TEXT: &str = "text-red-500";
    pub const CRITICAL_BG: &str = "bg-red-500/10";

    pub const HIGH_TEXT: &str = "text-orange-400";
    pub const HIGH_BG: &str = "bg-orange-400/10";

    pub const MEDIUM_TEXT: &str = "text-yellow-400";
    pub const MEDIUM_BG: &str = "bg-yellow-400/10";

    pub const LOW_TEXT: &str = "text-blue-400";
    pub const LOW_BG: &str = "bg-blue-400/10";
}

// ─────────────────────────────────────────────────────────────────────────────
// Pipeline Stage Colors
// ─────────────────────────────────────────────────────────────────────────────

/// Color tokens for build/deploy pipeline stages.
pub mod pipeline {
    pub const DRY_RUN_TEXT: &str = "text-gray-400";
    pub const READY_FOR_BUILD_TEXT: &str = "text-blue-400";
    pub const BUILDING_TEXT: &str = "text-indigo-400";
    pub const BUILD_COMPLETE_TEXT: &str = "text-violet-400";
    pub const READY_FOR_DEPLOY_TEXT: &str = "text-emerald-400";
    pub const UNKNOWN_TEXT: &str = "text-gray-500";
}

// ─────────────────────────────────────────────────────────────────────────────
// Surface / Layout Colors
// ─────────────────────────────────────────────────────────────────────────────

/// Color tokens for page surfaces, cards, and borders.
pub mod surface {
    /// Page background.
    pub const PAGE_BG: &str = "bg-gray-950";
    /// Sidebar / elevated surface.
    pub const SIDEBAR_BG: &str = "bg-gray-900";
    /// Card background.
    pub const CARD_BG: &str = "bg-gray-900";
    /// Card border.
    pub const CARD_BORDER: &str = "border-gray-800";
    /// Divider lines.
    pub const DIVIDER: &str = "divide-gray-800";
    /// Subtle surface (table header, hover).
    pub const SUBTLE_BG: &str = "bg-gray-800/50";
}

// ─────────────────────────────────────────────────────────────────────────────
// Text Colors
// ─────────────────────────────────────────────────────────────────────────────

/// Color tokens for text hierarchy.
pub mod text {
    /// Primary text — headings, important values.
    pub const PRIMARY: &str = "text-gray-100";
    /// Secondary text — labels, descriptions.
    pub const SECONDARY: &str = "text-gray-400";
    /// Muted text — timestamps, version numbers.
    pub const MUTED: &str = "text-gray-500";
    /// Disabled text.
    pub const DISABLED: &str = "text-gray-600";
}

// ─────────────────────────────────────────────────────────────────────────────
// Interactive Colors
// ─────────────────────────────────────────────────────────────────────────────

/// Color tokens for buttons and interactive elements.
pub mod interactive {
    // Theme button colors are defined in `src/main.rs` CSS variables/classes.
    // Update both this token mapping and the CSS vars together.
    /// Primary action button.
    pub const PRIMARY_BTN: &str = "cf-primary-btn";
    /// Danger action button.
    pub const DANGER_BTN: &str = "cf-danger-btn";
    /// Success action button.
    pub const SUCCESS_BTN: &str = "bg-emerald-600 hover:bg-emerald-700";
    /// Ghost / subtle button.
    pub const GHOST_BTN: &str = "hover:bg-gray-800";
    /// Hover background for interactive elements.
    pub const HOVER_BG: &str = "hover:bg-gray-800/50";
    /// Focus ring.
    pub const FOCUS_RING: &str = "focus:outline-none focus:ring-2 focus:ring-blue-500/50";
    /// Input field.
    pub const INPUT: &str = "bg-gray-900 border-gray-700 focus:border-blue-500";
}

// ─────────────────────────────────────────────────────────────────────────────
// Typography
// ─────────────────────────────────────────────────────────────────────────────

/// Typography class fragments. Uses Tailwind defaults (system font stack).
/// Monospace is used for hashes, store paths, and commit SHAs.
pub mod typography {
    /// Page title (h1).
    pub const PAGE_TITLE: &str = "text-2xl font-bold";
    /// Section title (h2).
    pub const SECTION_TITLE: &str = "text-lg font-semibold";
    /// Card label / stat label.
    pub const LABEL: &str = "text-sm text-gray-400";
    /// Large numeric value (stat cards).
    pub const STAT_VALUE: &str = "text-3xl font-bold";
    /// Table header.
    pub const TABLE_HEADER: &str = "text-xs font-medium text-gray-400 uppercase tracking-wider";
    /// Monospace for hashes, paths, versions.
    pub const MONO: &str = "font-mono text-sm";
    /// Small caption text.
    pub const CAPTION: &str = "text-xs text-gray-500";
}

// ─────────────────────────────────────────────────────────────────────────────
// Spacing
// ─────────────────────────────────────────────────────────────────────────────

/// Spacing tokens (Tailwind's 4px base scale).
pub mod spacing {
    /// Page content padding.
    pub const PAGE_PADDING: &str = "p-8";
    /// Card internal padding.
    pub const CARD_PADDING: &str = "p-6";
    /// Gap between cards in a grid.
    pub const CARD_GAP: &str = "gap-4";
    /// Gap between sections.
    pub const SECTION_GAP: &str = "gap-6";
    /// Table cell padding.
    pub const TABLE_CELL: &str = "px-6 py-3";
}

// ─────────────────────────────────────────────────────────────────────────────
// Component Presets
// ─────────────────────────────────────────────────────────────────────────────

/// Pre-composed class strings for common component patterns.
pub mod presets {
    /// Standard card container.
    pub const CARD: &str = "bg-gray-900 border border-gray-800 rounded-xl p-6";
    /// Badge (pill) base.
    pub const BADGE: &str =
        "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium";
    /// Status dot (small circle indicator).
    pub const DOT: &str = "w-2 h-2 rounded-full";
    /// Table container.
    pub const TABLE_CONTAINER: &str =
        "bg-gray-900 border border-gray-800 rounded-xl overflow-hidden";
}
