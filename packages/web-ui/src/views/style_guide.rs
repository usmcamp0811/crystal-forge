//! Component isolation surface for frontend visual development.

use dioxus::prelude::*;

use crate::api::models::{DeploymentStatus, HealthStatus};
use crate::components::charts::{DonutChartWithLegend, DonutSegment};
use crate::components::dashboard::{BuildQueueRow, BuildSummaryPanel, RecentDeploymentRow};
use crate::components::filters::{ViewMode, ViewToggle};
use crate::components::stat_card::StatCard;
use crate::components::status_badge::{DeploymentBadge, HealthBadge};
use crate::components::system::SystemCard;
use crate::showcase::fixtures::{
    build_queue_item_fixtures, build_queue_summary_fixture, recent_deployment_fixtures,
    stat_card_fixtures, system_summary_fixtures, timeline_fixtures,
};
use crate::showcase::shell::{
    DESKTOP_WIDTH, MOBILE_WIDTH, ResponsiveGrid, ResponsivePreview, ShowcaseSection, StateMatrix,
    StateTile, TABLET_WIDTH, VariantGroup, WIDE_WIDTH,
};
use crate::theme::{self, presets};

#[component]
pub fn StyleGuideView() -> Element {
    let stat_fixtures = stat_card_fixtures();
    let timeline = timeline_fixtures();

    rsx! {
        div {
            class: "{theme::spacing::PAGE_PADDING}",
            h1 {
                class: "{theme::typography::PAGE_TITLE} mb-2",
                "Component Isolation Surface"
            }
            p {
                class: "{theme::text::SECONDARY} mb-8",
                "Develop and review UI primitives, composite components, and page widgets in isolation."
            }

            div { class: "flex flex-wrap gap-2 mb-8",
                TaxonomyChip { label: "Primitives" }
                TaxonomyChip { label: "Composites" }
                TaxonomyChip { label: "Page Widgets" }
            }

            ShowcaseSection {
                title: "Primitives",
                description: "Small visual building blocks that should stay stateless and reusable.",
                div { class: "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-3",
                    PrimitiveCard {
                        title: "Health Badge",
                        content: rsx!(
                            div { class: "flex gap-2 flex-wrap",
                                HealthBadge { status: HealthStatus::Healthy }
                                HealthBadge { status: HealthStatus::Warning }
                                HealthBadge { status: HealthStatus::Critical }
                            }
                        )
                    }
                    PrimitiveCard {
                        title: "Deployment Badge",
                        content: rsx!(
                            div { class: "flex gap-2 flex-wrap",
                                DeploymentBadge { status: DeploymentStatus::UpToDate }
                                DeploymentBadge { status: DeploymentStatus::Behind }
                                DeploymentBadge { status: DeploymentStatus::NeverDeployed }
                            }
                        )
                    }
                    PrimitiveCard {
                        title: "Color Tokens",
                        content: rsx!(
                            div { class: "space-y-2",
                                ColorSwatch { label: "Healthy", text_class: theme::health::HEALTHY_TEXT, bg_class: theme::health::HEALTHY_BG }
                                ColorSwatch { label: "Warning", text_class: theme::health::WARNING_TEXT, bg_class: theme::health::WARNING_BG }
                                ColorSwatch { label: "Critical", text_class: theme::health::CRITICAL_TEXT, bg_class: theme::health::CRITICAL_BG }
                            }
                        )
                    }
                    PrimitiveCard {
                        title: "Buttons",
                        content: rsx!(
                            div { class: "flex flex-wrap gap-2",
                                button { class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}", "Primary" }
                                button { class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::DANGER_BTN}", "Danger" }
                            }
                        )
                    }
                }
            }

            ShowcaseSection {
                title: "Design Tokens",
                description: "Legacy token coverage retained so the isolation surface is additive, not reductive.",
                div { class: "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3",
                    PrimitiveCard {
                        title: "Health Status Swatches",
                        content: rsx!(
                            div { class: "space-y-2",
                                StatusSwatch { label: "Healthy", text_class: theme::health::HEALTHY_TEXT, bg_class: theme::health::HEALTHY_BG, dot_class: theme::health::HEALTHY_DOT }
                                StatusSwatch { label: "Warning", text_class: theme::health::WARNING_TEXT, bg_class: theme::health::WARNING_BG, dot_class: theme::health::WARNING_DOT }
                                StatusSwatch { label: "Critical", text_class: theme::health::CRITICAL_TEXT, bg_class: theme::health::CRITICAL_BG, dot_class: theme::health::CRITICAL_DOT }
                            }
                        )
                    }
                    PrimitiveCard {
                        title: "Deployment Swatches",
                        content: rsx!(
                            div { class: "space-y-2",
                                ColorSwatch { label: "Up to Date", text_class: theme::deployment::UP_TO_DATE_TEXT, bg_class: theme::deployment::UP_TO_DATE_BG }
                                ColorSwatch { label: "Behind", text_class: theme::deployment::BEHIND_TEXT, bg_class: theme::deployment::BEHIND_BG }
                                ColorSwatch { label: "Never Deployed", text_class: theme::deployment::NEVER_DEPLOYED_TEXT, bg_class: theme::deployment::NEVER_DEPLOYED_BG }
                            }
                        )
                    }
                    PrimitiveCard {
                        title: "Typography Tokens",
                        content: rsx!(
                            div { class: "space-y-2",
                                p { class: "{theme::typography::SECTION_TITLE}", "Section Title" }
                                p { class: "{theme::typography::LABEL}", "Label text token" }
                                p { class: "{theme::typography::MONO}", "/nix/store/example-system" }
                            }
                        )
                    }
                }
            }

            ShowcaseSection {
                title: "Composites",
                description: "Prop-driven UI components composed from primitives and shared in multiple views.",
                StateMatrix { title: "Stat Card Matrix",
                    StateTile { label: "loading", SkeletonStatCard {} }
                    StateTile { label: "empty", EmptyStatCard {} }
                    StateTile { label: "success", DemoStatCard { label: stat_fixtures[0].label, value: stat_fixtures[0].value, caption: stat_fixtures[0].caption } }
                    StateTile { label: "error", ErrorStatCard {} }
                    StateTile { label: "overflow", DemoStatCard { label: "Very Long Label For Build Queue Processing", value: "123,456,789", caption: "caption with a very long explanation to validate text wrapping" } }
                }
            }

            ShowcaseSection {
                title: "Page Widgets",
                description: "Larger sections used within pages. These should be presentational and fixture-driven in the showcase.",
                ResponsivePreview {
                    label: "mobile (375px)",
                    width_class: "max-w-[375px]",
                    WidgetPanel {
                        title: "Flake Timeline",
                        for item in timeline.iter() {
                            TimelineRow {
                                title: item.title,
                                meta: item.meta,
                                status: item.status,
                            }
                        }
                    }
                }
                ResponsivePreview {
                    label: "desktop (960px)",
                    width_class: "max-w-[960px]",
                    WidgetPanel {
                        title: "Flake Timeline",
                        for item in timeline.iter() {
                            TimelineRow {
                                title: item.title,
                                meta: item.meta,
                                status: item.status,
                            }
                        }
                    }
                }
            }

            ShowcaseSection {
                title: "Interactive Components",
                description: "Components with user interaction patterns, demonstrated with state management.",
                VariantGroup { title: "View Toggle States",
                    StateMatrix { title: "ViewToggle - Interactive State Demo",
                        StateTile { label: "table active",
                            ViewToggleDemo { initial_mode: ViewMode::Table }
                        }
                        StateTile { label: "cards active",
                            ViewToggleDemo { initial_mode: ViewMode::Cards }
                        }
                    }
                }

                ResponsiveGrid {
                    ResponsivePreview {
                        label: "mobile (375px)",
                        width_class: MOBILE_WIDTH,
                        div { class: "p-4 space-y-3",
                            p { class: "text-xs {theme::text::MUTED}", "View toggle in mobile context" }
                            ViewToggleDemo { initial_mode: ViewMode::Table }
                        }
                    }
                    ResponsivePreview {
                        label: "tablet (768px)",
                        width_class: TABLET_WIDTH,
                        div { class: "p-4 space-y-3",
                            p { class: "text-xs {theme::text::MUTED}", "View toggle in tablet context" }
                            ViewToggleDemo { initial_mode: ViewMode::Cards }
                        }
                    }
                }
            }

            ShowcaseSection {
                title: "Dashboard Components",
                description: "High-value reusable components extracted from dashboard, builds, and systems views with complete state coverage.",

                StateMatrix { title: "BuildQueueRow - All States",
                    {
                        let queue_items = build_queue_item_fixtures();
                        rsx! {
                            StateTile { label: "building (active)",
                                BuildQueueRow {
                                    item: queue_items[0].clone(),
                                    position_label: Some("Active".to_string())
                                }
                            }
                            StateTile { label: "queued (next)",
                                BuildQueueRow {
                                    item: queue_items[1].clone(),
                                    position_label: Some("Next".to_string())
                                }
                            }
                            StateTile { label: "queued (#2)",
                                BuildQueueRow {
                                    item: queue_items[2].clone(),
                                    position_label: Some("Queued #2".to_string())
                                }
                            }
                            StateTile { label: "overflow (long text)",
                                BuildQueueRow {
                                    item: queue_items[3].clone(),
                                    position_label: Some("Active".to_string())
                                }
                            }
                            StateTile { label: "empty message",
                                BuildQueueRow {
                                    item: queue_items[4].clone(),
                                    position_label: None
                                }
                            }
                        }
                    }
                }

                ResponsiveGrid {
                    ResponsivePreview {
                        label: "mobile (375px)",
                        width_class: MOBILE_WIDTH,
                        {
                            let queue_items = build_queue_item_fixtures();
                            rsx! {
                                div { class: "space-y-2",
                                    BuildQueueRow {
                                        item: queue_items[0].clone(),
                                        position_label: Some("Active".to_string())
                                    }
                                    BuildQueueRow {
                                        item: queue_items[1].clone(),
                                        position_label: Some("Next".to_string())
                                    }
                                }
                            }
                        }
                    }
                    ResponsivePreview {
                        label: "desktop (1024px)",
                        width_class: DESKTOP_WIDTH,
                        {
                            let queue_items = build_queue_item_fixtures();
                            rsx! {
                                div { class: "space-y-2",
                                    BuildQueueRow {
                                        item: queue_items[0].clone(),
                                        position_label: Some("Active".to_string())
                                    }
                                    BuildQueueRow {
                                        item: queue_items[1].clone(),
                                        position_label: Some("Next".to_string())
                                    }
                                    BuildQueueRow {
                                        item: queue_items[3].clone(),
                                        position_label: Some("Queued #3".to_string())
                                    }
                                }
                            }
                        }
                    }
                }

                StateMatrix { title: "StatCard - Semantic States",
                    {
                        rsx! {
                            StateTile { label: "default (neutral)",
                                StatCard {
                                    label: "Total Systems".to_string(),
                                    value: "24".to_string(),
                                    color_class: "".to_string()
                                }
                            }
                            StateTile { label: "success (green)",
                                StatCard {
                                    label: "Healthy Systems".to_string(),
                                    value: "18".to_string(),
                                    color_class: "text-green-400".to_string()
                                }
                            }
                            StateTile { label: "warning (amber)",
                                StatCard {
                                    label: "Policy Failures".to_string(),
                                    value: "3".to_string(),
                                    color_class: "text-amber-400".to_string()
                                }
                            }
                            StateTile { label: "danger (red)",
                                StatCard {
                                    label: "Critical CVEs".to_string(),
                                    value: "12".to_string(),
                                    color_class: "text-red-400".to_string()
                                }
                            }
                            StateTile { label: "info (blue)",
                                StatCard {
                                    label: "Active Builds".to_string(),
                                    value: "6".to_string(),
                                    color_class: "text-blue-400".to_string()
                                }
                            }
                            StateTile { label: "large value",
                                StatCard {
                                    label: "Total Deployments".to_string(),
                                    value: "1,247".to_string(),
                                    color_class: "".to_string()
                                }
                            }
                        }
                    }
                }

                ResponsiveGrid {
                    ResponsivePreview {
                        label: "mobile (375px)",
                        width_class: MOBILE_WIDTH,
                        {
                            rsx! {
                                div { class: "grid grid-cols-2 gap-2",
                                    StatCard {
                                        label: "Systems".to_string(),
                                        value: "24".to_string(),
                                        color_class: "".to_string()
                                    }
                                    StatCard {
                                        label: "Healthy".to_string(),
                                        value: "18".to_string(),
                                        color_class: "text-green-400".to_string()
                                    }
                                }
                            }
                        }
                    }
                    ResponsivePreview {
                        label: "desktop (1024px)",
                        width_class: DESKTOP_WIDTH,
                        {
                            rsx! {
                                div { class: "grid grid-cols-4 gap-3",
                                    StatCard {
                                        label: "Total Systems".to_string(),
                                        value: "24".to_string(),
                                        color_class: "".to_string()
                                    }
                                    StatCard {
                                        label: "Healthy".to_string(),
                                        value: "18".to_string(),
                                        color_class: "text-green-400".to_string()
                                    }
                                    StatCard {
                                        label: "Policy Failures".to_string(),
                                        value: "3".to_string(),
                                        color_class: "text-amber-400".to_string()
                                    }
                                    StatCard {
                                        label: "CVE Alerts".to_string(),
                                        value: "12".to_string(),
                                        color_class: "text-red-400".to_string()
                                    }
                                }
                            }
                        }
                    }
                }

                StateMatrix { title: "SystemCard - Health & Deployment States",
                    {
                        let systems = system_summary_fixtures();
                        rsx! {
                            StateTile { label: "healthy + up-to-date",
                                SystemCard {
                                    system: systems[0].clone(),
                                    on_remove: move |_| {},
                                    on_update_key: move |_| {}
                                }
                            }
                            StateTile { label: "warning + behind",
                                SystemCard {
                                    system: systems[1].clone(),
                                    on_remove: move |_| {},
                                    on_update_key: move |_| {}
                                }
                            }
                            StateTile { label: "critical + never deployed",
                                SystemCard {
                                    system: systems[2].clone(),
                                    on_remove: move |_| {},
                                    on_update_key: move |_| {}
                                }
                            }
                            StateTile { label: "offline + unknown",
                                SystemCard {
                                    system: systems[3].clone(),
                                    on_remove: move |_| {},
                                    on_update_key: move |_| {}
                                }
                            }
                            StateTile { label: "overflow (long hostname)",
                                SystemCard {
                                    system: systems[4].clone(),
                                    on_remove: move |_| {},
                                    on_update_key: move |_| {}
                                }
                            }
                            StateTile { label: "building state",
                                SystemCard {
                                    system: systems[5].clone(),
                                    on_remove: move |_| {},
                                    on_update_key: move |_| {}
                                }
                            }
                        }
                    }
                }

                ResponsiveGrid {
                    ResponsivePreview {
                        label: "mobile (375px)",
                        width_class: MOBILE_WIDTH,
                        {
                            let systems = system_summary_fixtures();
                            rsx! {
                                div { class: "space-y-3",
                                    SystemCard {
                                        system: systems[0].clone(),
                                        on_remove: move |_| {},
                                        on_update_key: move |_| {}
                                    }
                                }
                            }
                        }
                    }
                    ResponsivePreview {
                        label: "tablet (768px)",
                        width_class: TABLET_WIDTH,
                        {
                            let systems = system_summary_fixtures();
                            rsx! {
                                div { class: "grid grid-cols-2 gap-3",
                                    SystemCard {
                                        system: systems[0].clone(),
                                        on_remove: move |_| {},
                                        on_update_key: move |_| {}
                                    }
                                    SystemCard {
                                        system: systems[1].clone(),
                                        on_remove: move |_| {},
                                        on_update_key: move |_| {}
                                    }
                                }
                            }
                        }
                    }
                    ResponsivePreview {
                        label: "desktop (1024px)",
                        width_class: DESKTOP_WIDTH,
                        {
                            let systems = system_summary_fixtures();
                            rsx! {
                                div { class: "grid grid-cols-3 gap-3",
                                    SystemCard {
                                        system: systems[0].clone(),
                                        on_remove: move |_| {},
                                        on_update_key: move |_| {}
                                    }
                                    SystemCard {
                                        system: systems[1].clone(),
                                        on_remove: move |_| {},
                                        on_update_key: move |_| {}
                                    }
                                    SystemCard {
                                        system: systems[2].clone(),
                                        on_remove: move |_| {},
                                        on_update_key: move |_| {}
                                    }
                                }
                            }
                        }
                    }
                }

                StateMatrix { title: "RecentDeploymentRow - Deployment States",
                    {
                        let deployments = recent_deployment_fixtures();
                        rsx! {
                            StateTile { label: "up-to-date (recent)",
                                RecentDeploymentRow {
                                    deployment: deployments[0].clone()
                                }
                            }
                            StateTile { label: "behind (older)",
                                RecentDeploymentRow {
                                    deployment: deployments[1].clone()
                                }
                            }
                            StateTile { label: "overflow (long message)",
                                RecentDeploymentRow {
                                    deployment: deployments[2].clone()
                                }
                            }
                            StateTile { label: "no commit message",
                                RecentDeploymentRow {
                                    deployment: deployments[3].clone()
                                }
                            }
                            StateTile { label: "just deployed",
                                RecentDeploymentRow {
                                    deployment: deployments[4].clone()
                                }
                            }
                        }
                    }
                }

                StateMatrix { title: "BuildSummaryPanel - Queue States",
                    {
                        rsx! {
                            StateTile { label: "active builds + queue",
                                div { class: "w-96 h-64",
                                    BuildSummaryPanel {
                                        queue: build_queue_summary_fixture(),
                                        flake_filter: None
                                    }
                                }
                            }
                            StateTile { label: "with flake filter",
                                div { class: "w-96 h-64",
                                    BuildSummaryPanel {
                                        queue: build_queue_summary_fixture(),
                                        flake_filter: Some("infrastructure".to_string())
                                    }
                                }
                            }
                        }
                    }
                }

                StateMatrix { title: "DonutChartWithLegend - Visual Data",
                    {
                        rsx! {
                            StateTile { label: "health distribution",
                                div { class: "w-96 h-48",
                                    DonutChartWithLegend {
                                        segments: vec![
                                            DonutSegment {
                                                percent: 60.0,
                                                color: "#10b981",
                                                label: "Healthy",
                                                count: 18,
                                                systems: vec!["web-server-1".to_string(), "web-server-2".to_string(), "db-primary".to_string()],
                                            },
                                            DonutSegment {
                                                percent: 25.0,
                                                color: "#f59e0b",
                                                label: "Warning",
                                                count: 5,
                                                systems: vec!["staging-app".to_string(), "dev-machine".to_string()],
                                            },
                                            DonutSegment {
                                                percent: 15.0,
                                                color: "#ef4444",
                                                label: "Critical",
                                                count: 1,
                                                systems: vec!["legacy-server".to_string()],
                                            },
                                        ],
                                        center_value: 24,
                                        center_label: "SYSTEMS"
                                    }
                                }
                            }
                            StateTile { label: "build queue",
                                div { class: "w-96 h-48",
                                    DonutChartWithLegend {
                                        segments: vec![
                                            DonutSegment {
                                                percent: 40.0,
                                                color: "#42ff65",
                                                label: "Building",
                                                count: 2,
                                                systems: vec!["web-server-1".to_string(), "db-primary".to_string()],
                                            },
                                            DonutSegment {
                                                percent: 60.0,
                                                color: "#e57c00",
                                                label: "Queued",
                                                count: 3,
                                                systems: vec!["staging-app".to_string(), "api-gateway".to_string(), "worker-01".to_string()],
                                            },
                                        ],
                                        center_value: 5,
                                        center_label: "BUILDS"
                                    }
                                }
                            }
                        }
                    }
                }

                StateMatrix { title: "Status Badges - All States",
                    {
                        rsx! {
                            StateTile { label: "health badges",
                                div { class: "flex flex-wrap gap-2",
                                    HealthBadge { status: HealthStatus::Healthy }
                                    HealthBadge { status: HealthStatus::Warning }
                                    HealthBadge { status: HealthStatus::Critical }
                                    HealthBadge { status: HealthStatus::Offline }
                                }
                            }
                            StateTile { label: "deployment badges",
                                div { class: "flex flex-wrap gap-2",
                                    DeploymentBadge { status: DeploymentStatus::UpToDate }
                                    DeploymentBadge { status: DeploymentStatus::Behind }
                                    DeploymentBadge { status: DeploymentStatus::Ahead }
                                    DeploymentBadge { status: DeploymentStatus::NeverDeployed }
                                    DeploymentBadge { status: DeploymentStatus::NoCommitsAvailable }
                                    DeploymentBadge { status: DeploymentStatus::Unknown }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TaxonomyChip(label: &'static str) -> Element {
    rsx! {
        span {
            class: "rounded-full border {theme::surface::CARD_BORDER} px-3 py-1 text-xs font-semibold {theme::text::SECONDARY}",
            "{label}"
        }
    }
}

#[component]
fn PrimitiveCard(title: &'static str, content: Element) -> Element {
    rsx! {
        div { class: "{presets::CARD}",
            p { class: "text-sm font-semibold {theme::text::SECONDARY} mb-3", "{title}" }
            {content}
        }
    }
}

#[component]
fn ColorSwatch(label: &'static str, text_class: &'static str, bg_class: &'static str) -> Element {
    rsx! {
        div { class: "{bg_class} rounded px-2 py-1",
            span { class: "{text_class} text-xs font-medium", "{label}" }
        }
    }
}

#[component]
fn StatusSwatch(
    label: &'static str,
    text_class: &'static str,
    bg_class: &'static str,
    dot_class: &'static str,
) -> Element {
    rsx! {
        div { class: "rounded border {theme::surface::CARD_BORDER} px-2 py-1.5",
            div { class: "flex items-center gap-2",
                span { class: "{presets::DOT} {dot_class}" }
                span { class: "{text_class} text-xs font-medium", "{label}" }
            }
            div { class: "mt-1 {bg_class} rounded px-2 py-1",
                span { class: "{text_class} text-xs", "Badge preview" }
            }
        }
    }
}

#[component]
fn DemoStatCard(label: &'static str, value: &'static str, caption: &'static str) -> Element {
    rsx! {
        div { class: "rounded-lg border {theme::surface::CARD_BORDER} p-3 bg-gray-900/40",
            p { class: "text-xs uppercase tracking-wide {theme::text::MUTED}", "{label}" }
            p { class: "text-2xl font-bold {theme::text::PRIMARY} mt-1", "{value}" }
            p { class: "text-xs {theme::text::SECONDARY} mt-1", "{caption}" }
        }
    }
}

#[component]
fn SkeletonStatCard() -> Element {
    rsx! {
        div { class: "rounded-lg border {theme::surface::CARD_BORDER} p-3 bg-gray-900/40 animate-pulse",
            div { class: "h-3 w-20 rounded bg-gray-700 mb-2" }
            div { class: "h-7 w-14 rounded bg-gray-700 mb-2" }
            div { class: "h-3 w-24 rounded bg-gray-700" }
        }
    }
}

#[component]
fn EmptyStatCard() -> Element {
    rsx! {
        div { class: "rounded-lg border {theme::surface::CARD_BORDER} p-3 bg-gray-900/40",
            p { class: "text-sm {theme::text::SECONDARY}", "No data available" }
        }
    }
}

#[component]
fn ErrorStatCard() -> Element {
    rsx! {
        div { class: "rounded-lg border border-red-500/30 bg-red-500/10 p-3",
            p { class: "text-xs text-red-300 uppercase tracking-wide", "error" }
            p { class: "text-sm text-red-200 mt-1", "Unable to load widget data" }
        }
    }
}

#[component]
fn WidgetPanel(title: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "{presets::CARD}",
            p { class: "text-sm font-semibold {theme::text::SECONDARY} mb-3", "{title}" }
            div { class: "space-y-2", {children} }
        }
    }
}

#[component]
fn TimelineRow(title: &'static str, meta: &'static str, status: &'static str) -> Element {
    let (status_text, status_bg) = timeline_status_style(status);

    rsx! {
        div { class: "rounded-lg border {theme::surface::CARD_BORDER} p-3 bg-gray-900/30",
            div { class: "flex items-start justify-between gap-3",
                div {
                    p { class: "text-sm font-medium {theme::text::PRIMARY}", "{title}" }
                    p { class: "text-xs {theme::text::MUTED} mt-1", "{meta}" }
                }
                span { class: "rounded-full px-2 py-0.5 text-xs {status_text} {status_bg}", "{status}" }
            }
        }
    }
}

fn timeline_status_style(status: &str) -> (&'static str, &'static str) {
    match status {
        "evaluating" => (theme::health::WARNING_TEXT, theme::health::WARNING_BG),
        "ready for build" => (theme::deployment::AHEAD_TEXT, theme::deployment::AHEAD_BG),
        "building" => (theme::deployment::AHEAD_TEXT, theme::deployment::AHEAD_BG),
        "build complete" => (
            theme::deployment::UP_TO_DATE_TEXT,
            theme::deployment::UP_TO_DATE_BG,
        ),
        "policy failed" => (theme::health::CRITICAL_TEXT, theme::health::CRITICAL_BG),
        _ => (
            theme::deployment::UNKNOWN_TEXT,
            theme::deployment::UNKNOWN_BG,
        ),
    }
}

#[component]
fn ViewToggleDemo(initial_mode: ViewMode) -> Element {
    let mut view_mode = use_signal(|| initial_mode);
    let mode_text = match *view_mode.read() {
        ViewMode::Table => "Table",
        ViewMode::Cards => "Cards",
    };

    rsx! {
        div { class: "space-y-2",
            ViewToggle {
                view_mode: *view_mode.read(),
                on_change: move |mode| view_mode.set(mode)
            }
            p { class: "text-xs {theme::text::MUTED}",
                "Current: {mode_text}"
            }
        }
    }
}
