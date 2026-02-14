//! Style guide view — displays all design tokens for visual review.

use dioxus::prelude::*;

use crate::api::models::{DeploymentStatus, HealthStatus, PipelineStage};
use crate::components::status_badge::{DeploymentBadge, HealthBadge};
use crate::theme::{self, presets};

/// Style guide page showing all design system tokens.
#[component]
pub fn StyleGuideView() -> Element {
    rsx! {
        div {
            class: "{theme::spacing::PAGE_PADDING}",
            h1 {
                class: "{theme::typography::PAGE_TITLE} mb-8",
                "Design System — Style Guide"
            }

            // ── Health Status ──────────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "Health Status" }
                div { class: "flex flex-wrap {theme::spacing::CARD_GAP}",
                    StatusSwatch { label: "Healthy", text_class: theme::health::HEALTHY_TEXT, bg_class: theme::health::HEALTHY_BG, dot_class: theme::health::HEALTHY_DOT }
                    StatusSwatch { label: "Warning", text_class: theme::health::WARNING_TEXT, bg_class: theme::health::WARNING_BG, dot_class: theme::health::WARNING_DOT }
                    StatusSwatch { label: "Critical", text_class: theme::health::CRITICAL_TEXT, bg_class: theme::health::CRITICAL_BG, dot_class: theme::health::CRITICAL_DOT }
                    StatusSwatch { label: "Offline", text_class: theme::health::OFFLINE_TEXT, bg_class: theme::health::OFFLINE_BG, dot_class: theme::health::OFFLINE_DOT }
                }
            }

            // ── Health Badges ──────────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "Health Badges (Component)" }
                div { class: "flex flex-wrap gap-3",
                    HealthBadge { status: HealthStatus::Healthy }
                    HealthBadge { status: HealthStatus::Warning }
                    HealthBadge { status: HealthStatus::Critical }
                    HealthBadge { status: HealthStatus::Offline }
                }
            }

            // ── Deployment Status ──────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "Deployment Status" }
                div { class: "flex flex-wrap {theme::spacing::CARD_GAP}",
                    ColorSwatch { label: "Up to Date", text_class: theme::deployment::UP_TO_DATE_TEXT, bg_class: theme::deployment::UP_TO_DATE_BG }
                    ColorSwatch { label: "Behind", text_class: theme::deployment::BEHIND_TEXT, bg_class: theme::deployment::BEHIND_BG }
                    ColorSwatch { label: "Ahead", text_class: theme::deployment::AHEAD_TEXT, bg_class: theme::deployment::AHEAD_BG }
                    ColorSwatch { label: "Never Deployed", text_class: theme::deployment::NEVER_DEPLOYED_TEXT, bg_class: theme::deployment::NEVER_DEPLOYED_BG }
                    ColorSwatch { label: "Unknown", text_class: theme::deployment::UNKNOWN_TEXT, bg_class: theme::deployment::UNKNOWN_BG }
                }
            }

            // ── Deployment Badges ──────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "Deployment Badges (Component)" }
                div { class: "flex flex-wrap gap-3",
                    DeploymentBadge { status: DeploymentStatus::UpToDate }
                    DeploymentBadge { status: DeploymentStatus::Behind }
                    DeploymentBadge { status: DeploymentStatus::Ahead }
                    DeploymentBadge { status: DeploymentStatus::NeverDeployed }
                    DeploymentBadge { status: DeploymentStatus::Unknown }
                }
            }

            // ── CVE Severity ───────────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "CVE Severity" }
                div { class: "flex flex-wrap {theme::spacing::CARD_GAP}",
                    ColorSwatch { label: "Critical", text_class: theme::cve::CRITICAL_TEXT, bg_class: theme::cve::CRITICAL_BG }
                    ColorSwatch { label: "High", text_class: theme::cve::HIGH_TEXT, bg_class: theme::cve::HIGH_BG }
                    ColorSwatch { label: "Medium", text_class: theme::cve::MEDIUM_TEXT, bg_class: theme::cve::MEDIUM_BG }
                    ColorSwatch { label: "Low", text_class: theme::cve::LOW_TEXT, bg_class: theme::cve::LOW_BG }
                }
            }

            // ── Pipeline Stages ────────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "Pipeline Stages" }
                div { class: "flex flex-wrap gap-3",
                    for stage in [PipelineStage::DryRun, PipelineStage::ReadyForBuild, PipelineStage::Building, PipelineStage::BuildComplete, PipelineStage::ReadyForDeploy, PipelineStage::Unknown] {
                        span {
                            class: "{presets::BADGE} {stage.color_class()} bg-gray-800",
                            "{stage.label()}"
                        }
                    }
                }
            }

            // ── Surface Colors ─────────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "Surface Colors" }
                div { class: "grid grid-cols-1 md:grid-cols-3 {theme::spacing::CARD_GAP}",
                    div {
                        class: "{theme::surface::PAGE_BG} border border-gray-700 rounded-lg p-4",
                        p { class: "{theme::text::SECONDARY}", "Page BG (gray-950)" }
                    }
                    div {
                        class: "{theme::surface::SIDEBAR_BG} border border-gray-700 rounded-lg p-4",
                        p { class: "{theme::text::SECONDARY}", "Sidebar BG (gray-900)" }
                    }
                    div {
                        class: "{theme::surface::SUBTLE_BG} border border-gray-700 rounded-lg p-4",
                        p { class: "{theme::text::SECONDARY}", "Subtle BG (gray-800/50)" }
                    }
                }
            }

            // ── Typography ─────────────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "Typography" }
                div { class: "{presets::CARD} space-y-4",
                    p { class: "{theme::typography::PAGE_TITLE}", "Page Title (text-2xl font-bold)" }
                    p { class: "{theme::typography::SECTION_TITLE}", "Section Title (text-lg font-semibold)" }
                    p { class: "{theme::typography::LABEL}", "Label (text-sm text-gray-400)" }
                    p { class: "{theme::typography::STAT_VALUE}", "42" }
                    p { class: "{theme::typography::TABLE_HEADER}", "Table Header" }
                    p { class: "{theme::typography::MONO}", "/nix/store/abc123-nixos-system-24.11" }
                    p { class: "{theme::typography::CAPTION}", "Caption — timestamps, minor info" }
                }
            }

            // ── Text Hierarchy ─────────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "Text Hierarchy" }
                div { class: "{presets::CARD} space-y-2",
                    p { class: "{theme::text::PRIMARY}", "Primary — headings, important values" }
                    p { class: "{theme::text::SECONDARY}", "Secondary — labels, descriptions" }
                    p { class: "{theme::text::MUTED}", "Muted — timestamps, versions" }
                    p { class: "{theme::text::DISABLED}", "Disabled — inactive elements" }
                }
            }

            // ── Interactive ────────────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "Interactive Elements" }
                div { class: "flex flex-wrap gap-3",
                    button {
                        class: "px-4 py-2 rounded-lg text-white font-medium transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                        "Primary"
                    }
                    button {
                        class: "px-4 py-2 rounded-lg text-white font-medium transition-colors {theme::interactive::DANGER_BTN} {theme::interactive::FOCUS_RING}",
                        "Danger"
                    }
                    button {
                        class: "px-4 py-2 rounded-lg text-white font-medium transition-colors {theme::interactive::SUCCESS_BTN} {theme::interactive::FOCUS_RING}",
                        "Success"
                    }
                    button {
                        class: "px-4 py-2 rounded-lg text-gray-400 font-medium transition-colors {theme::interactive::GHOST_BTN} {theme::interactive::FOCUS_RING}",
                        "Ghost"
                    }
                }
                div { class: "mt-4",
                    input {
                        class: "rounded-lg px-4 py-2 text-sm text-gray-300 placeholder-gray-600 {theme::interactive::INPUT} {theme::interactive::FOCUS_RING}",
                        r#type: "text",
                        placeholder: "Input field...",
                    }
                }
            }

            // ── Card Preset ────────────────────────────────────────
            section { class: "mb-10",
                h2 { class: "{theme::typography::SECTION_TITLE} mb-4", "Card Preset" }
                div { class: "{presets::CARD}",
                    p { class: "{theme::typography::SECTION_TITLE} mb-2", "Example Card" }
                    p { class: "{theme::text::SECONDARY}", "This card uses the presets::CARD class string." }
                }
            }
        }
    }
}

/// A color swatch showing text and background variants.
#[component]
fn ColorSwatch(label: &'static str, text_class: &'static str, bg_class: &'static str) -> Element {
    rsx! {
        div {
            class: "{presets::CARD} min-w-[140px]",
            span { class: "{text_class} font-medium text-sm", "{label}" }
            div { class: "mt-2 {bg_class} rounded px-3 py-1.5",
                span { class: "{text_class} text-xs", "Badge preview" }
            }
        }
    }
}

/// A status swatch with dot, text, and badge preview.
#[component]
fn StatusSwatch(label: &'static str, text_class: &'static str, bg_class: &'static str, dot_class: &'static str) -> Element {
    rsx! {
        div {
            class: "{presets::CARD} min-w-[140px]",
            div { class: "flex items-center gap-2 mb-2",
                span { class: "{presets::DOT} {dot_class}" }
                span { class: "{text_class} font-medium text-sm", "{label}" }
            }
            div { class: "{bg_class} rounded px-3 py-1.5",
                span { class: "{text_class} text-xs", "Badge preview" }
            }
        }
    }
}
