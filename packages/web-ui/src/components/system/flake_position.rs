//! Flake-position lane for System Detail.
//!
//! The card answers "where is this system in its flake's history?". It draws
//! the tracked branch as a horizontal lane: older commits on the left, HEAD on
//! the right, the host pin on the commit the system runs, and dashed segments
//! for the commits that are newer than the running commit. On the Deploy tab
//! the operator can pick a target from the lane, and a running deployment is
//! drawn as a stage branch under the lane.
//!
//! Design reference: `docs/design/CrystalForge/components/FlakePosition.jsx`
//! at dev commit `bd8d068d`. All positions, peers, and build states come from
//! `GET /api/v1/systems/:id/flake-position`. The card does not invent
//! positions for commits that the server did not place.

use crate::api::models::{
    FlakePositionCommit, FlakePositionUnknownReason, SystemDeploymentProgress, SystemFlakePosition,
};
use crate::components::system::pending_deploy_banner::{
    DEPLOY_STAGES, stage_index, stage_label, stage_subtext,
};
use crate::components::{Icon, IconName};
use chrono::{DateTime, Utc};
use dioxus::prelude::*;

/// Index the unknown-position lane anchors on when the host has no last-tracked
/// commit. The lane shows the commits through two past this index.
const UNKNOWN_LANE_COMMITS: usize = 6;

/// Distance from HEAD after which the lane collapses commits into gap pills.
///
/// The value matches `fpWindow` in the design: up to ten commits fit without
/// horizontal scrolling at the default card width.
const LANE_COLLAPSE_DISTANCE: usize = 9;

/// Fixed height of the commit row in deploy mode, in pixels.
///
/// INVARIANT: The row holds the 48px pin area, the 20px rail, the SHA label,
/// and an optional peers pill. A fixed height puts every commit dot at a known
/// offset from the stage row, so the connector needs no DOM measurement. Keep
/// it in sync with `.fp-deploying .fp-node .fp-top` and `.fp-rail` in app.css.
const DEPLOY_LANE_ROW_PX: i32 = 104;

/// Gap between the commit row and the stage row in deploy mode, in pixels.
const DEPLOY_ROW_GAP_PX: i32 = 14;

/// Vertical center of a commit dot inside the commit row: the 48px pin area
/// plus half of the 20px rail.
const DOT_CENTER_PX: i32 = 58;

/// Vertical center of the stage dots inside the stage row: half of the 20px
/// stage rail.
const STAGE_CENTER_PX: i32 = 10;

/// Offset of a commit dot center from the top of the stage row (negative:
/// above it).
const DOT_FROM_STAGE_ROW_PX: i32 = DOT_CENTER_PX - (DEPLOY_LANE_ROW_PX + DEPLOY_ROW_GAP_PX);

/// Returns the SVG path for a same-commit redeploy, in a 600-unit-wide viewBox.
///
/// The branch spans `cols` equal columns and the shared commit sits in column
/// `dot_col` (0-based). The loop leaves the left side of that dot, drops to
/// the stage row (y=70), and returns to the right side of the dot, as in the
/// design's `fp-uloop`. Each side reaches one column width from the dot, but
/// never past the branch edge, so a commit in the first or last column (for
/// example HEAD) still gets a loop centered on its dot.
pub fn same_commit_loop_path(dot_col: usize, cols: usize) -> String {
    let cols = cols.max(1) as f64;
    let width = 600.0 / cols;
    let cx = (dot_col as f64 + 0.5) * width;
    let half = width.min(cx - 10.0).min(590.0 - cx).max(24.0);
    let (l, r) = (cx - half, cx + half);
    format!(
        "M {a} 0 C {b} 0 {l} 12 {l} 35 L {l} 58 Q {l} 70 {l2} 70 L {r2} 70 Q {r} 70 {r} 58 L {r} 35 C {r} 12 {c} 0 {d} 0",
        a = cx - 12.0,
        b = cx - 46.0,
        c = cx + 46.0,
        d = cx + 12.0,
        l2 = l + 12.0,
        r2 = r - 12.0,
    )
}

/// Why the card cannot place the system on its tracked branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneUnknown {
    /// Server reason from the flake-position API.
    Server(FlakePositionUnknownReason),
    /// External/Reference configuration was removed from the flake.
    ConfigurationRemoved,
}

/// Prepared lane data for [`FlakePositionCard`].
#[derive(Clone, Debug, PartialEq)]
pub struct LaneModel {
    /// Registered flake name.
    pub flake_name: String,
    /// Tracked branch name.
    pub branch: String,
    /// Branch commits from HEAD (index 0) to older commits.
    pub commits: Vec<FlakePositionCommit>,
    /// Index of the commit that the system runs, declares, or tracks.
    pub at: Option<usize>,
    /// Reason `at` is unknown.
    pub unknown: Option<LaneUnknown>,
    /// Index of the newest commit that the system has run.
    pub last_tracked: Option<usize>,
}

impl LaneModel {
    /// Builds the model for a managed system from the server response.
    pub fn managed(position: &SystemFlakePosition) -> Self {
        let at = position
            .running_position
            .and_then(|value| usize::try_from(value).ok())
            .filter(|index| *index < position.commits.len());
        Self {
            flake_name: position.flake_name.clone().unwrap_or_default(),
            branch: position
                .branch
                .clone()
                .unwrap_or_else(|| "main".to_string()),
            commits: position.commits.clone(),
            at,
            unknown: match (at, position.unknown_reason) {
                (Some(_), _) => None,
                (None, Some(reason)) => Some(LaneUnknown::Server(reason)),
                (None, None) => Some(LaneUnknown::Server(FlakePositionUnknownReason::Untracked)),
            },
            last_tracked: position
                .last_tracked_position
                .and_then(|value| usize::try_from(value).ok()),
        }
    }

    /// Builds the model for an External or Reference system.
    ///
    /// These systems have no agent observation. The lane places them at the
    /// revision whose evidence the server selected (`evidence_sha`).
    ///
    /// When the configuration was removed, the evidence revision is the last
    /// revision that held it. The lane keeps that revision as the
    /// `last_tracked` marker instead of dropping it, so the card can show where
    /// the configuration last existed.
    pub fn evidence(
        position: &SystemFlakePosition,
        evidence_sha: Option<&str>,
        configuration_removed: bool,
    ) -> Self {
        let mut model = Self::managed(position);
        let evidence_index = evidence_sha.and_then(|sha| commit_index(&model.commits, sha));
        model.at = if configuration_removed {
            None
        } else {
            evidence_index
        };
        model.last_tracked = if configuration_removed {
            evidence_index
        } else {
            None
        };
        model.unknown = if configuration_removed {
            Some(LaneUnknown::ConfigurationRemoved)
        } else if model.commits.is_empty() {
            Some(LaneUnknown::Server(FlakePositionUnknownReason::NoHistory))
        } else if model.at.is_none() {
            Some(LaneUnknown::Server(FlakePositionUnknownReason::OffBranch))
        } else {
            None
        };
        model
    }
}

/// One node of the horizontal lane, ordered from left (oldest) to right (HEAD).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneNode {
    /// A drawn commit at this branch index.
    Commit(usize),
    /// Collapsed commits. `newest` is the smallest branch index in the gap.
    Gap { newest: usize, count: usize },
    /// Untracked running configuration drawn next to the deploy target.
    Orphan,
}

/// Returns the lane nodes from oldest to HEAD.
///
/// The window always shows HEAD, the `at` commit, every `targets` commit (the
/// deploy target and the picked commit), and two older commits after the
/// farthest of them. When the farthest of `at` and
/// `targets` is more than [`LANE_COLLAPSE_DISTANCE`] from HEAD, only the three
/// newest commits and the neighbors of `at` and `targets` are drawn, and the
/// other commits collapse into [`LaneNode::Gap`] nodes.
pub fn lane_window(len: usize, at: usize, targets: &[usize]) -> Vec<LaneNode> {
    if len == 0 {
        return Vec::new();
    }
    let far = targets.iter().copied().fold(at, usize::max);
    let hi = (far + 2).min(len - 1);
    let keep = |index: usize| -> bool {
        if far <= LANE_COLLAPSE_DISTANCE {
            return true;
        }
        let near = |anchor: usize| index + 2 >= anchor && index <= anchor + 2;
        index <= 2 || near(at) || targets.iter().any(|t| index + 1 >= *t && index <= t + 1)
    };
    let mut nodes = Vec::new();
    for index in (0..=hi).rev() {
        if keep(index) {
            nodes.push(LaneNode::Commit(index));
        } else if let Some(LaneNode::Gap { newest, count }) = nodes.last_mut() {
            *newest = index;
            *count += 1;
        } else {
            nodes.push(LaneNode::Gap {
                newest: index,
                count: 1,
            });
        }
    }
    nodes
}

/// Returns the index of the commit whose full SHA starts with `sha`, or that
/// `sha` starts with.
pub fn commit_index(commits: &[FlakePositionCommit], sha: &str) -> Option<usize> {
    if sha.is_empty() {
        return None;
    }
    commits
        .iter()
        .position(|commit| commit.sha.starts_with(sha) || sha.starts_with(&commit.sha))
}

/// Returns whether the lane can draw `progress` as a deployment.
///
/// The lane draws a deployment only when it places both ends: the source (the
/// running commit, or an untracked configuration) and the target commit inside
/// the returned branch window. A target with no commit (for example a
/// generation that maps to no tracked commit) or outside the window has no
/// node to pin, so the lane would show stages with no connector.
///
/// Callers MUST show another progress presentation (the progress banner) when
/// this returns `false`. Otherwise the deployment, its failure message, and its
/// Logs action are invisible.
pub fn can_draw_deployment(lane: &LaneModel, progress: &SystemDeploymentProgress) -> bool {
    let untracked = matches!(
        lane.unknown,
        Some(LaneUnknown::Server(FlakePositionUnknownReason::Untracked))
    );
    let source_known = lane.at.is_some() || untracked;
    let target_placed = progress
        .target_commit
        .as_deref()
        .is_some_and(|sha| commit_index(&lane.commits, sha).is_some());
    source_known && target_placed
}

/// Describes `index` relative to the host at `at`, as the design focus row does.
pub fn relative_label(index: usize, at: usize, host: &str, verb: &str) -> String {
    if index == at {
        format!("the commit {host} is {verb}")
    } else if index == 0 {
        format!("HEAD · {at} ahead of {host}")
    } else if index < at {
        format!("{} ahead of {host}", at - index)
    } else {
        format!("{} before {host}", index - at)
    }
}

/// Describes a picked target relative to the host at `at`.
pub fn picked_label(pick: usize, at: usize, host: &str) -> String {
    if pick < at {
        format!("selected · {} ahead of {host}", at - pick)
    } else {
        format!("selected · {} before {host}", pick - at)
    }
}

/// Describes the deploy distance from `at` to `target`.
pub fn deploy_distance_label(at: usize, target: usize) -> Option<String> {
    let distance = at.abs_diff(target);
    if distance == 0 {
        return None;
    }
    let direction = if target < at { "forward" } else { "back" };
    let plural = if distance == 1 { "" } else { "s" };
    Some(format!("{distance} commit{plural} {direction}"))
}

/// Returns whether deployment stages read left to right, from the source
/// column toward the target column.
pub fn stages_flow_right(source_col: usize, target_col: usize) -> bool {
    target_col >= source_col
}

/// Formats a timestamp as a short age such as `5m ago`, `3h ago`, or `2d ago`.
pub fn short_ago(now: DateTime<Utc>, at: DateTime<Utc>) -> String {
    let elapsed = now.signed_duration_since(at);
    if elapsed.num_minutes() < 1 {
        "just now".to_string()
    } else if elapsed.num_hours() < 1 {
        format!("{}m ago", elapsed.num_minutes())
    } else if elapsed.num_days() < 1 {
        format!("{}h ago", elapsed.num_hours())
    } else {
        format!("{}d ago", elapsed.num_days())
    }
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        format!("{count} {word}")
    } else {
        format!("{count} {word}s")
    }
}

/// Title and explanation for an unknown position.
fn unknown_copy(
    unknown: LaneUnknown,
    host: &str,
    flake: &str,
    branch: &str,
    configuration: &str,
    has_last_tracked: bool,
) -> (String, String) {
    use FlakePositionUnknownReason as R;
    let reference = if has_last_tracked {
        "The last tracked commit is shown for reference."
    } else {
        "The newest branch commits are shown for reference."
    };
    match unknown {
        LaneUnknown::Server(R::Untracked) => (
            format!("{host} is running a configuration Crystal Forge never evaluated"),
            format!(
                "The active store path doesn't match the output of any commit on this flake. It was most likely built on the host with nixos-rebuild. {reference}"
            ),
        ),
        LaneUnknown::ConfigurationRemoved => (
            format!("nixosConfigurations.{configuration} no longer exists in {flake}"),
            format!(
                "The configuration was removed or renamed, so there is no current commit to compare against. {reference}"
            ),
        ),
        LaneUnknown::Server(R::NoHistory) => (
            format!("No history is available for {flake}"),
            "The flake hasn't synced, so commits can't be placed yet.".to_string(),
        ),
        LaneUnknown::Server(R::NotObserved) => (
            format!("{host} has not reported a running configuration"),
            "Crystal Forge places the system on the lane after its agent reports a store path."
                .to_string(),
        ),
        LaneUnknown::Server(R::OffBranch) => (
            format!("{host} is on a commit that is not on {flake}/{branch}"),
            format!("The commit is tracked, but it is not in the branch history. {reference}"),
        ),
        LaneUnknown::Server(R::BeyondWindow) => (
            format!("{host} is far behind {flake}/{branch}"),
            format!("The commit is older than the commits shown. {reference}"),
        ),
    }
}

/// Card shown in place of the lane while the flake-position read is loading or
/// after it failed.
///
/// Surfaces that show the lane MUST render this instead of dropping the
/// section, so a failed read is visible and derived fallback data never looks
/// like authoritative branch position, peers, or build state. `hint` adds a
/// surface-specific sentence to the unavailable text.
#[component]
pub fn FlakePositionNotice(
    loading: bool,
    hostname: String,
    #[props(default)] hint: Option<String>,
) -> Element {
    rsx! {
        section {
            class: "card sd-card fp-card",
            "data-testid": "flake-position-card",
            "data-fp-state": if loading { "loading" } else { "unavailable" },
            div { class: "fp-summary", role: "status",
                if loading {
                    div { class: "fp-headline", "Loading flake position…" }
                } else {
                    div { class: "fp-headline", "Flake position unavailable" }
                    div { class: "fp-sub",
                        "The server could not load the branch position, other hosts, or build states for {hostname}."
                        if let Some(hint) = hint {
                            " {hint}"
                        }
                    }
                }
            }
        }
    }
}

/// Deploy-mode state derived from a deployment progress record.
#[derive(Clone, Debug, PartialEq)]
struct DeployView {
    rollback: bool,
    landed: bool,
    failed: bool,
    target: Option<usize>,
    stage_idx: usize,
    sub: String,
    chip_text: String,
}

/// Flake-position card.
///
/// `verb` is `running` for managed systems, `declared` for External systems,
/// and `tracked` for Reference systems. `pick` and `on_pick` are set only on
/// the Deploy tab. `deploy` switches the card to deploy mode; it is ignored
/// for an unknown position that is not untracked, because the lane cannot
/// draw a deployment without a source.
#[component]
pub fn FlakePositionCard(
    lane: LaneModel,
    hostname: String,
    verb: String,
    configuration_name: String,
    store_path: Option<String>,
    #[props(default)] heartbeat_interval_secs: i64,
    #[props(default)] heartbeat_next_in_secs: Option<f64>,
    #[props(default)] pick: Option<String>,
    #[props(default)] on_pick: Option<EventHandler<FlakePositionCommit>>,
    #[props(default)] deploy: Option<SystemDeploymentProgress>,
    #[props(default)] on_view_logs: Option<EventHandler<()>>,
    #[props(default)] on_dismiss_deploy: Option<EventHandler<()>>,
    on_open_commit: EventHandler<FlakePositionCommit>,
    #[props(default)] on_view_history: Option<EventHandler<()>>,
    #[props(default)] on_capture: Option<EventHandler<()>>,
) -> Element {
    let mut hovered: Signal<Option<usize>> = use_signal(|| None);
    let now = Utc::now();
    let host = hostname.as_str();
    let flake = lane.flake_name.as_str();
    let branch = lane.branch.as_str();
    let untracked = matches!(
        lane.unknown,
        Some(LaneUnknown::Server(FlakePositionUnknownReason::Untracked))
    );
    let deploy = deploy.filter(|progress| can_draw_deployment(&lane, progress));

    let deploy_view = deploy.as_ref().map(|progress| {
        let rollback = progress.kind == "rollback";
        let landed = progress.stage == "activated";
        let failed = progress.stage == "failed";
        let target = progress
            .target_commit
            .as_deref()
            .and_then(|sha| commit_index(&lane.commits, sha));
        let target_label = progress
            .target_generation
            .map(|generation| format!("gen #{generation}"))
            .or_else(|| progress.target_commit.as_deref().map(short_sha))
            .unwrap_or_else(|| "the target".to_string());
        DeployView {
            rollback,
            landed,
            failed,
            target,
            stage_idx: stage_index(if landed { "activated" } else { &progress.stage }),
            sub: stage_subtext(
                &progress.stage,
                rollback,
                host,
                heartbeat_interval_secs,
                heartbeat_next_in_secs,
                &target_label,
                progress.failure_message.as_deref(),
            ),
            chip_text: if landed {
                "deployed".to_string()
            } else if failed {
                "failed".to_string()
            } else if rollback {
                "rolling back".to_string()
            } else {
                "deploying".to_string()
            },
        }
    });

    let chips = {
        let deploy_view = deploy_view.clone();
        let unknown = lane.unknown;
        let at = lane.at;
        rsx! {
            div { class: "fp-chips",
                if let Some(view) = deploy_view {
                    if view.landed {
                        span { class: "chip chip-healthy", Icon { name: IconName::Check, size: 10 } " deployed" }
                    } else if view.failed {
                        span { class: "chip chip-critical", Icon { name: IconName::Warn, size: 10 } " failed" }
                    } else {
                        span { class: "chip chip-info",
                            span { class: "deploy-pending-spinner fp-chip-spin", "aria-hidden": "true" }
                            " {view.chip_text}"
                        }
                    }
                } else if unknown.is_some() {
                    span { class: "chip chip-unknown", Icon { name: IconName::Warn, size: 10 } " not in history" }
                } else if at == Some(0) {
                    span { class: "chip chip-healthy", Icon { name: IconName::Check, size: 10 } " on HEAD" }
                } else if let Some(behind) = at {
                    span { class: "chip chip-warning", "{behind} behind" }
                }
            }
        }
    };

    let store_value = store_path.clone().unwrap_or_else(|| "—".to_string());

    // ── Unknown position without a deployment: faded reference lane. ──
    if lane.at.is_none() && deploy_view.is_none() {
        let unknown = lane
            .unknown
            .unwrap_or(LaneUnknown::Server(FlakePositionUnknownReason::Untracked));
        let (title, copy) = unknown_copy(
            unknown,
            host,
            flake,
            branch,
            &configuration_name,
            lane.last_tracked.is_some(),
        );
        // On the Deploy tab the reference lane is pickable too: a host on an
        // untracked configuration is exactly the case where the operator
        // chooses a known commit to deploy.
        let unknown_pick = on_pick
            .and(pick.as_deref())
            .and_then(|sha| commit_index(&lane.commits, sha));
        // The reference lane uses the same bounded window as the normal lane:
        // the newest commits, the last-tracked commit, and the picked commit
        // stay visible, and the commits between them collapse into gap pills.
        // A last-tracked or picked commit far from HEAD therefore adds a few
        // nodes, not one node per commit.
        let anchor = lane
            .last_tracked
            .unwrap_or(UNKNOWN_LANE_COMMITS - 1)
            .min(lane.commits.len().saturating_sub(1));
        let picks: Vec<usize> = unknown_pick.into_iter().collect();
        let nodes = lane_window(lane.commits.len(), anchor, &picks);
        let count = nodes.len();
        return rsx! {
            section { class: "card sd-card fp-card", "data-testid": "flake-position-card", "data-fp-state": "unknown",
                div { class: "fp-toprow",
                    div { class: "fp-summary",
                        div { class: "fp-headline", "{title}" }
                        div { class: "fp-sub", "{copy}" }
                    }
                    {chips}
                }
                if count > 0 {
                    div { class: "fp-lane-wrap",
                        div { class: "fp-lane fp-lane-faded",
                            for (i, node) in nodes.iter().copied().enumerate() {
                                {
                                    match node {
                                        LaneNode::Gap { newest, count: hidden } => rsx! {
                                            div { key: "gap-{newest}", class: "fp-node fp-gap",
                                                div { class: "fp-top" }
                                                div { class: "fp-rail",
                                                    span { class: "fp-seg" }
                                                    span { class: "fp-gap-pill", "+{hidden}" }
                                                    span { class: "fp-seg" }
                                                }
                                                div { class: "fp-bot", "commits" }
                                            }
                                        },
                                        LaneNode::Orphan => rsx! {},
                                        LaneNode::Commit(index) => {
                                            let commit = lane.commits[index].clone();
                                            let last = lane.last_tracked == Some(index);
                                            let is_pick = unknown_pick == Some(index);
                                            let commit_for_open = commit.clone();
                                            let title = if on_pick.is_some() {
                                                format!("Select {} as deploy target", short_sha(&commit.sha))
                                            } else {
                                                format!("{} · {}", short_sha(&commit.sha), commit.message)
                                            };
                                            let node_class = format!(
                                                "fp-node{}{}{}",
                                                if last { " fp-lastknown" } else { "" },
                                                if is_pick { " fp-picked" } else { "" },
                                                if on_pick.is_some() { " fp-pickable" } else { "" },
                                            );
                                            rsx! {
                                                div {
                                                    key: "{commit.sha}",
                                                    class: "{node_class}",
                                                    "data-fp-idx": "{index}",
                                                    div { class: "fp-top fp-top-sha mono",
                                                        if is_pick {
                                                            span { class: "fp-pin fp-pin-target", Icon { name: IconName::Deploy, size: 11 } " selected" }
                                                        }
                                                        if index == 0 { span { class: "fp-head-tag", "HEAD" } }
                                                        "{short_sha(&commit.sha)}"
                                                        if last { span { class: "fp-lk", "last tracked" } }
                                                    }
                                                    div { class: "fp-rail",
                                                        span { class: if i == 0 { "fp-seg fp-none" } else { "fp-seg" } }
                                                        button {
                                                            class: "fp-dot focus-ring",
                                                            title: "{title}",
                                                            onclick: move |_| match on_pick {
                                                                Some(handler) => handler.call(commit_for_open.clone()),
                                                                None => on_open_commit.call(commit_for_open.clone()),
                                                            },
                                                        }
                                                        span { class: if i + 1 == count { "fp-seg fp-none" } else { "fp-seg" } }
                                                    }
                                                    if last && untracked {
                                                        div { class: "fp-orphan",
                                                            span { class: "fp-orphan-line" }
                                                            span { class: "fp-orphan-dot", "?" }
                                                            span { class: "fp-pin fp-pin-unknown", Icon { name: IconName::Server, size: 11 } " {host}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if on_pick.is_some() && count > 0 {
                    div { class: "fp-focus",
                        span { class: "fp-pick-hint", "Click a commit to select it as the deploy target" }
                    }
                }
                div { class: "fp-store-row",
                    div { class: "fp-store",
                        span { class: "fp-store-k", "Store path" }
                        span { class: "mono fp-store-v", title: "{store_value}", "{store_value}" }
                        span { class: "fp-store-arrow", "→" }
                        span { class: "fp-store-unknown", "no matching evaluation" }
                    }
                    div { class: "fp-actions",
                        if untracked {
                            if let Some(on_capture) = on_capture {
                                button {
                                    class: "btn btn-ghost xs focus-ring",
                                    onclick: move |_| on_capture.call(()),
                                    Icon { name: IconName::Git, size: 11 }
                                    " Capture to flake"
                                }
                            }
                        }
                        if let Some(on_view_history) = on_view_history {
                            button {
                                class: "btn btn-ghost xs focus-ring",
                                onclick: move |_| on_view_history.call(()),
                                "View history"
                            }
                        }
                    }
                }
            }
        };
    }

    // ── Known position, or a deployment from an untracked configuration. ──
    let orphan = lane.at.is_none();
    let at = lane.at.unwrap_or(0);
    // Picking stays available while a deployment is in progress. A real
    // deployment can sit in a stage for a long time, and the operator still
    // selects the next target in the picker; the lane must show that choice.
    // (The prototype disables picking in deploy mode because its simulated
    // deployments finish in seconds.)
    let picking = on_pick.is_some();
    let deploy_target = deploy_view.as_ref().and_then(|view| view.target);
    let pick_idx = if picking {
        pick.as_deref()
            .and_then(|sha| commit_index(&lane.commits, sha))
            .filter(|index| *index != at || orphan)
            // The deploy target already carries its own pin.
            .filter(|index| Some(*index) != deploy_target)
    } else {
        None
    };
    let window_anchor = if orphan {
        deploy_target.unwrap_or(0)
    } else {
        at
    };
    let anchors: Vec<usize> = deploy_target.into_iter().chain(pick_idx).collect();
    let mut window = lane_window(lane.commits.len(), window_anchor, &anchors);
    if orphan {
        // The untracked source is drawn as an extra node immediately older
        // than the deploy target, so the branch connects the two.
        let target_col = window
            .iter()
            .position(|node| *node == LaneNode::Commit(deploy_target.unwrap_or(0)))
            .unwrap_or(0);
        window.insert(target_col, LaneNode::Orphan);
    }
    let landed = deploy_view.as_ref().is_some_and(|view| view.landed);
    let pin_at = if landed {
        deploy_target
    } else if orphan {
        None
    } else {
        Some(at)
    };

    // Deploy-mode grid geometry. The branch row spans from the source column
    // to the target column; for a same-commit redeploy it spans one column on
    // each side of the shared commit.
    let column_of = |wanted: LaneNode| window.iter().position(|node| *node == wanted);
    let source_col = if orphan {
        column_of(LaneNode::Orphan)
    } else {
        column_of(LaneNode::Commit(at))
    };
    let target_col = deploy_target.and_then(|index| column_of(LaneNode::Commit(index)));
    let span = match (deploy_view.as_ref(), source_col, target_col) {
        (Some(_), Some(source), Some(target)) => {
            let (c0, c1) = (source.min(target), source.max(target));
            let same = c0 == c1;
            let b0 = if same { c0.saturating_sub(1) } else { c0 };
            let b1 = if same {
                (c1 + 1).min(window.len() - 1)
            } else {
                c1
            };
            // Columns run from oldest (left) to HEAD (right). Stages flow from
            // the running commit toward the target, so a target to the right
            // reads left to right (design: `fwd = tCol >= fCol`).
            Some((b0, b1, same, stages_flow_right(source, target)))
        }
        (Some(_), _, _) => Some((0, window.len().saturating_sub(1), false, true)),
        _ => None,
    };
    let lane_style = span.map(|(b0, b1, same, _)| {
        let columns = (b1 - b0 + 1).max(1);
        let min_width = 64.max((5 * 72 + 40) / columns).min(200);
        let template = (0..window.len())
            .map(|col| {
                if col >= b0 && col <= b1 {
                    if same {
                        "minmax(130px, 3fr)".to_string()
                    } else {
                        format!("minmax({min_width}px, 4fr)")
                    }
                } else {
                    "minmax(56px, 1fr)".to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "display:grid;grid-template-columns:{template};grid-template-rows:{DEPLOY_LANE_ROW_PX}px auto;row-gap:{DEPLOY_ROW_GAP_PX}px;align-items:start;"
        )
    });

    let focus_idx = (*hovered.read())
        .or(pick_idx)
        .or(if orphan { deploy_target } else { None })
        .unwrap_or(at)
        .min(lane.commits.len().saturating_sub(1));
    let focus = lane.commits.get(focus_idx).cloned();
    let focus_rel = if orphan {
        if Some(focus_idx) == deploy_target {
            format!("deploy target for {host}")
        } else {
            format!("on {branch}")
        }
    } else if pick_idx == Some(focus_idx) {
        picked_label(focus_idx, at, host)
    } else {
        relative_label(focus_idx, at, host, &verb)
    };

    let summary = if let Some(view) = deploy_view.clone() {
        let verb_done = if view.rollback {
            "Rollback"
        } else {
            "Deployment"
        };
        let verb_doing = if view.rollback {
            "Rolling back"
        } else {
            "Deploying"
        };
        let headline = if view.landed {
            format!("{verb_done} complete")
        } else if view.failed {
            format!("{verb_done} failed")
        } else {
            format!("{verb_doing} {host}")
        };
        let progress = deploy.clone();
        let target_sha = progress
            .as_ref()
            .and_then(|p| p.target_commit.as_deref().map(short_sha));
        let target_chip = match (
            view.rollback,
            progress.as_ref().and_then(|p| p.target_generation),
            target_sha,
        ) {
            (true, Some(generation), Some(sha)) => format!("#{generation} · {sha}"),
            (true, Some(generation), None) => format!("#{generation}"),
            (_, _, Some(sha)) => sha,
            (_, _, None) => "uncommitted".to_string(),
        };
        let distance = if orphan {
            Some("from an untracked config".to_string())
        } else {
            view.target
                .and_then(|target| deploy_distance_label(at, target))
        };
        rsx! {
            div { class: "fp-summary fp-dep-summary",
                div { class: "fp-dep-text",
                    div { class: "fp-headline",
                        "{headline} "
                        span { class: "mono fp-dep-sha", "{target_chip}" }
                        if let Some(distance) = distance {
                            span { class: "fp-dep-dist", "{distance}" }
                        }
                    }
                    div { class: "fp-sub", title: "{view.sub}", "{view.sub}" }
                }
                if let Some(on_view_logs) = on_view_logs {
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        onclick: move |_| on_view_logs.call(()),
                        Icon { name: IconName::Terminal, size: 11 }
                        " Logs"
                    }
                }
                if view.landed || view.failed {
                    if let Some(on_dismiss_deploy) = on_dismiss_deploy {
                        button {
                            class: "btn-icon focus-ring",
                            "aria-label": "Dismiss",
                            onclick: move |_| on_dismiss_deploy.call(()),
                            Icon { name: IconName::X, size: 13 }
                        }
                    }
                }
            }
        }
    } else {
        let head = lane.commits.first().cloned();
        let at_commit = lane.commits.get(at).cloned();
        let behind_text = plural(at, "commit");
        rsx! {
            div { class: "fp-summary",
                if at == 0 {
                    div { class: "fp-headline",
                        "{host} is on the latest commit of "
                        span { class: "mono", "{flake}/{branch}" }
                        "."
                    }
                    if let Some(head) = head {
                        div { class: "fp-sub",
                            "The {verb} store path was built from HEAD, "
                            span { class: "mono", "{short_sha(&head.sha)}" }
                            " ({short_ago(now, head.committed_at)})."
                        }
                    }
                } else {
                    div { class: "fp-headline",
                        "{host} is "
                        strong { class: "fp-behind-n", "{behind_text}" }
                        " behind "
                        span { class: "mono", "{flake}/{branch}" }
                        "."
                    }
                    if let (Some(head), Some(at_commit)) = (head, at_commit) {
                        div { class: "fp-sub",
                            "The {verb} store path was built from "
                            span { class: "mono", "{short_sha(&at_commit.sha)}" }
                            " ({short_ago(now, at_commit.committed_at)}). HEAD is "
                            span { class: "mono", "{short_sha(&head.sha)}" }
                            " ({short_ago(now, head.committed_at)})."
                        }
                    }
                }
            }
        }
    };

    let lane_class = format!(
        "fp-lane{}{}",
        if deploy_view.is_some() {
            " fp-deploying"
        } else {
            ""
        },
        // Deploy mode keeps its fixed 48px pin area (see DEPLOY_LANE_ROW_PX).
        if picking && deploy_view.is_none() {
            " fp-picking"
        } else {
            ""
        }
    );
    let window_len = window.len();
    let stage_nodes: Vec<(String, bool, bool)> = deploy_view
        .as_ref()
        .map(|view| {
            DEPLOY_STAGES
                .iter()
                .enumerate()
                .map(|(k, stage)| {
                    let past = k < view.stage_idx || view.landed;
                    let cur = k == view.stage_idx && !view.landed && !view.failed;
                    (stage_label(stage, view.rollback).to_string(), past, cur)
                })
                .collect()
        })
        .unwrap_or_default();

    rsx! {
        section {
            class: "card sd-card fp-card",
            "data-testid": "flake-position-card",
            "data-fp-state": if deploy_view.is_some() { "deploying" } else if at == 0 { "head" } else { "behind" },
            div { class: "fp-toprow", {summary} {chips} }
            div { class: "fp-lane-wrap",
                div { class: "{lane_class}", style: lane_style.clone().unwrap_or_default(),
                    for (col, node) in window.iter().copied().enumerate() {
                        {
                            let grid = if lane_style.is_some() {
                                format!("grid-row:1;grid-column:{};", col + 1)
                            } else {
                                String::new()
                            };
                            let first = col == 0;
                            let last = col + 1 == window_len;
                            match node {
                                LaneNode::Orphan => rsx! {
                                    div { key: "orphan-{col}", class: "fp-node fp-orphan-node", style: "{grid}",
                                        div { class: "fp-top",
                                            if landed {
                                                span { class: "fp-from-tag", "was here" }
                                            } else {
                                                span { class: "fp-pin fp-pin-unknown", Icon { name: IconName::Server, size: 11 } " {host}" }
                                            }
                                        }
                                        div { class: "fp-rail",
                                            span { class: if first { "fp-seg fp-orphan-seg fp-none" } else { "fp-seg fp-orphan-seg" } }
                                            button { class: "fp-dot fp-qdot focus-ring", title: "Untracked · {store_value}", "?" }
                                            span { class: if last { "fp-seg fp-none" } else { "fp-seg" } }
                                        }
                                        div { class: "fp-bot", "untracked" }
                                    }
                                },
                                LaneNode::Gap { newest, count } => {
                                    let left_ahead = !orphan && newest + count - 1 < at;
                                    let right_ahead = !orphan && newest <= at && newest > 0;
                                    rsx! {
                                        div { key: "gap-{newest}", class: "fp-node fp-gap", style: "{grid}",
                                            div { class: "fp-top" }
                                            div { class: "fp-rail",
                                                span { class: if left_ahead { "fp-seg fp-ahead" } else { "fp-seg" } }
                                                span { class: "fp-gap-pill", "+{count}" }
                                                span { class: if right_ahead { "fp-seg fp-ahead" } else { "fp-seg" } }
                                            }
                                            div { class: "fp-bot", "commits" }
                                        }
                                    }
                                }
                                LaneNode::Commit(index) => {
                                    let commit = lane.commits[index].clone();
                                    let is_sys = pin_at == Some(index);
                                    let is_head = index == 0;
                                    let is_target = deploy_view.is_some() && !landed && deploy_target == Some(index) && !is_sys;
                                    let is_from = deploy_view.is_some() && !orphan && index == at && !is_sys;
                                    let is_pick = pick_idx == Some(index);
                                    let left_ahead = !orphan && index < at;
                                    let right_ahead = !orphan && index <= at && index > 0;
                                    let rollback = deploy_view.as_ref().is_some_and(|view| view.rollback);
                                    let class = format!(
                                        "fp-node{}{}{}{}{}{}{}{}",
                                        if is_pick { " fp-picked" } else { "" },
                                        if picking { " fp-pickable" } else { "" },
                                        if is_sys { " fp-sys" } else { "" },
                                        if is_target { " fp-target" } else { "" },
                                        if is_from { " fp-from" } else { "" },
                                        if is_head { " fp-head" } else { "" },
                                        if focus_idx == index { " fp-sel" } else { "" },
                                        if !orphan && index < at { " fp-is-ahead" } else { "" },
                                    );
                                    let title = if picking {
                                        format!("Select {} as deploy target", short_sha(&commit.sha))
                                    } else {
                                        format!("{} · {}", short_sha(&commit.sha), commit.message)
                                    };
                                    let peers = commit.peers.clone();
                                    let peers_title = peers.join(", ");
                                    let peers_text = if peers.len() == 1 { "+1 host".to_string() } else { format!("+{} hosts", peers.len()) };
                                    let commit_for_click = commit.clone();
                                    rsx! {
                                        div {
                                            key: "{commit.sha}",
                                            class: "{class}",
                                            style: "{grid}",
                                            "data-fp-idx": "{index}",
                                            onmouseenter: move |_| hovered.set(Some(index)),
                                            onmouseleave: move |_| hovered.set(None),
                                            div { class: "fp-top",
                                                if is_sys {
                                                    span { class: "fp-pin", Icon { name: IconName::Server, size: 11 } " {host}" }
                                                }
                                                if is_target {
                                                    span { class: "fp-pin fp-pin-target",
                                                        if rollback {
                                                            Icon { name: IconName::Rollback, size: 11 }
                                                            " rollback"
                                                        } else {
                                                            Icon { name: IconName::Deploy, size: 11 }
                                                            " target"
                                                        }
                                                    }
                                                }
                                                if is_pick {
                                                    span { class: "fp-pin fp-pin-target", Icon { name: IconName::Deploy, size: 11 } " selected" }
                                                }
                                                if is_from {
                                                    span { class: "fp-from-tag", "was here" }
                                                }
                                                if is_head && !(is_sys && deploy_view.is_none()) {
                                                    span { class: "fp-head-tag", "HEAD" }
                                                }
                                            }
                                            div { class: "fp-rail",
                                                span { class: format!("fp-seg{}{}", if first { " fp-none" } else { "" }, if left_ahead { " fp-ahead" } else { "" }) }
                                                button {
                                                    class: "fp-dot focus-ring",
                                                    title: "{title}",
                                                    onfocus: move |_| hovered.set(Some(index)),
                                                    onblur: move |_| hovered.set(None),
                                                    onclick: move |_| {
                                                        match (picking, on_pick) {
                                                            (true, Some(handler)) => handler.call(commit_for_click.clone()),
                                                            _ => on_open_commit.call(commit_for_click.clone()),
                                                        }
                                                    },
                                                }
                                                span { class: format!("fp-seg{}{}", if last { " fp-none" } else { "" }, if right_ahead { " fp-ahead" } else { "" }) }
                                            }
                                            div { class: "fp-bot mono", "{short_sha(&commit.sha)}" }
                                            if !peers.is_empty() {
                                                div { class: "fp-peers", title: "{peers_title}", "{peers_text}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let (Some(view), Some((b0, b1, same, forward))) = (deploy_view.clone(), span) {
                        {
                            let columns = b1 - b0 + 1;
                            // Column centers inside the branch: every spanned
                            // column has the same track size, so the source
                            // and target centers sit half a column from each
                            // edge of the branch.
                            let edge = format!("calc(100% / {})", columns * 2);
                            let branch_class = format!(
                                "fp-branch{}{}{}{}",
                                if forward { "" } else { " back" },
                                if same { " same" } else { "" },
                                if view.landed { " landed" } else { "" },
                                if view.rollback { " rb" } else { "" },
                            );
                            let has_connector = target_col.is_some() && source_col.is_some();
                            // The U starts 2px below the larger (16px) target dot.
                            let u_top = DOT_FROM_STAGE_ROW_PX + 8 + 2;
                            let u_height = STAGE_CENTER_PX - u_top;
                            let loop_height = STAGE_CENTER_PX - DOT_FROM_STAGE_ROW_PX + 2;
                            rsx! {
                                div {
                                    class: "{branch_class}",
                                    style: "grid-row:2;grid-column:{b0 + 1} / {b1 + 2};",
                                    if has_connector && same {
                                        // A redeploy of the running commit loops
                                        // out of that commit's dot and back.
                                        svg {
                                            class: "fp-uloop",
                                            style: "top:{DOT_FROM_STAGE_ROW_PX}px;width:100%;height:{loop_height}px;",
                                            "viewBox": "0 0 600 {loop_height}",
                                            "preserveAspectRatio": "none",
                                            "aria-hidden": "true",
                                            path { d: same_commit_loop_path(source_col.unwrap_or(b0).saturating_sub(b0), b1 - b0 + 1), "vector-effect": "non-scaling-stroke" }
                                        }
                                    } else if has_connector {
                                        span { class: "fp-u", style: "left:{edge};right:{edge};top:{u_top}px;height:{u_height}px;" }
                                    }
                                    div {
                                        class: "fp-branch-row",
                                        style: "padding-left:calc({edge} + 10px);padding-right:calc({edge} + 10px);",
                                        for (k, (label, past, cur)) in stage_nodes.iter().cloned().enumerate() {
                                            {
                                                let in_on = past || cur;
                                                let (left_on, right_on) = if forward { (in_on, past) } else { (past, in_on) };
                                                rsx! {
                                                    div {
                                                        key: "stage-{k}",
                                                        class: format!("fp-stage{}{}", if past { " past" } else { "" }, if cur { " cur" } else { "" }),
                                                        title: "{label}",
                                                        div { class: "fp-rail",
                                                            span { class: format!("fp-seg{}{}", if left_on { " on" } else { "" }, if cur && forward { " live" } else { "" }) }
                                                            span { class: "fp-sdot",
                                                                if past {
                                                                    Icon { name: IconName::Check, size: 9 }
                                                                } else if cur {
                                                                    span { class: "deploy-step-pulse" }
                                                                }
                                                            }
                                                            span { class: format!("fp-seg{}{}", if right_on { " on" } else { "" }, if cur && !forward { " live" } else { "" }) }
                                                        }
                                                        div { class: "fp-bot", "{label}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(focus) = focus {
                div { class: "fp-focus",
                    span { class: "mono fp-focus-sha", "{short_sha(&focus.sha)}" }
                    span { class: "fp-focus-msg", "{focus.message}" }
                    span { class: "fp-focus-meta", "{focus.author} · {short_ago(now, focus.committed_at)} · {focus_rel}" }
                    if picking {
                        span { class: "fp-pick-hint", "Click a commit to select it as the deploy target" }
                    }
                }
            }
            div { class: "fp-store-row",
                div { class: "fp-store",
                    span { class: "fp-store-k", "Store path" }
                    span { class: "mono fp-store-v", title: "{store_value}", "{store_value}" }
                    span { class: "fp-store-arrow", "←" }
                    if orphan {
                        span { class: "fp-store-unknown", "no matching evaluation" }
                    } else if let Some(at_commit) = lane.commits.get(at) {
                        span { class: "mono", "{short_sha(&at_commit.sha)} · nixosConfigurations.{configuration_name}" }
                    }
                }
                if at > 0 && deploy_view.is_none() && !picking {
                    if let Some(head) = lane.commits.first().cloned() {
                        div { class: "fp-actions",
                            button {
                                class: "btn btn-ghost xs focus-ring",
                                onclick: move |_| on_open_commit.call(head.clone()),
                                Icon { name: IconName::Git, size: 11 }
                                " Open HEAD"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commits(len: usize) -> Vec<FlakePositionCommit> {
        (0..len)
            .map(|index| FlakePositionCommit {
                position: index as i64,
                sha: format!("{index:040x}"),
                message: format!("commit {index}"),
                author: "dev".to_string(),
                committed_at: Utc::now(),
                build_state: crate::api::models::FlakePositionBuildState::Ready,
                peers: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn short_lane_draws_every_commit_from_oldest_to_head() {
        assert_eq!(
            lane_window(20, 3, &[]),
            vec![
                LaneNode::Commit(5),
                LaneNode::Commit(4),
                LaneNode::Commit(3),
                LaneNode::Commit(2),
                LaneNode::Commit(1),
                LaneNode::Commit(0),
            ]
        );
    }

    #[test]
    fn lane_stops_at_the_oldest_commit() {
        assert_eq!(
            lane_window(2, 1, &[]),
            vec![LaneNode::Commit(1), LaneNode::Commit(0)]
        );
        assert!(lane_window(0, 0, &[]).is_empty());
    }

    #[test]
    fn far_lane_collapses_into_gap_pills() {
        assert_eq!(
            lane_window(40, 20, &[]),
            vec![
                LaneNode::Commit(22),
                LaneNode::Commit(21),
                LaneNode::Commit(20),
                LaneNode::Commit(19),
                LaneNode::Commit(18),
                LaneNode::Gap {
                    newest: 3,
                    count: 15
                },
                LaneNode::Commit(2),
                LaneNode::Commit(1),
                LaneNode::Commit(0),
            ]
        );
    }

    #[test]
    fn lane_keeps_deploy_target_and_picked_commit_visible() {
        // A deployment to HEAD is in progress while the operator picks an
        // older commit (index 30) in the picker.
        let window = lane_window(60, 25, &[0, 30]);
        for index in [0, 25, 29, 30, 31] {
            assert!(
                window.contains(&LaneNode::Commit(index)),
                "commit {index} drawn"
            );
        }
    }

    #[test]
    fn far_lane_keeps_target_neighbors() {
        let window = lane_window(40, 20, &[10]);
        assert!(window.contains(&LaneNode::Commit(9)));
        assert!(window.contains(&LaneNode::Commit(10)));
        assert!(window.contains(&LaneNode::Commit(11)));
        assert!(window.contains(&LaneNode::Gap {
            newest: 3,
            count: 6
        }));
        assert!(window.contains(&LaneNode::Gap {
            newest: 12,
            count: 6
        }));
    }

    #[test]
    fn relative_labels_match_design_copy() {
        assert_eq!(
            relative_label(3, 3, "web-1", "running"),
            "the commit web-1 is running"
        );
        assert_eq!(
            relative_label(0, 3, "web-1", "running"),
            "HEAD · 3 ahead of web-1"
        );
        assert_eq!(relative_label(1, 3, "web-1", "running"), "2 ahead of web-1");
        assert_eq!(relative_label(5, 3, "web-1", "running"), "2 before web-1");
        assert_eq!(picked_label(1, 3, "web-1"), "selected · 2 ahead of web-1");
        assert_eq!(picked_label(4, 3, "web-1"), "selected · 1 before web-1");
    }

    #[test]
    fn deploy_distance_reports_direction() {
        assert_eq!(
            deploy_distance_label(3, 0).as_deref(),
            Some("3 commits forward")
        );
        assert_eq!(
            deploy_distance_label(0, 1).as_deref(),
            Some("1 commit back")
        );
        assert_eq!(deploy_distance_label(2, 2), None);
    }

    #[test]
    fn commit_index_accepts_short_and_full_shas() {
        let list = commits(4);
        assert_eq!(commit_index(&list, &list[2].sha), Some(2));
        assert_eq!(commit_index(&list, ""), None);
    }

    fn position_with(commits_len: usize) -> SystemFlakePosition {
        SystemFlakePosition {
            flake_id: Some(1),
            flake_name: Some("fleet".to_string()),
            branch: Some("main".to_string()),
            ordering: crate::api::models::FlakePositionOrdering::BranchSnapshot,
            commits: commits(commits_len),
            running_commit: None,
            running_position: None,
            unknown_reason: Some(FlakePositionUnknownReason::NotObserved),
            last_tracked_position: None,
        }
    }

    fn deploy_to(commit: Option<&str>) -> SystemDeploymentProgress {
        SystemDeploymentProgress {
            id: uuid::Uuid::from_u128(1),
            stage: "queued".to_string(),
            kind: "deploy".to_string(),
            target_store_path: "/nix/store/x".to_string(),
            target_commit: commit.map(str::to_string),
            target_generation: None,
            source: "manual".to_string(),
            issued_at: Utc::now(),
            delivered_at: None,
            applying_at: None,
            completed_at: None,
            failed_at: None,
            failure_message: None,
        }
    }

    #[test]
    fn removed_configuration_keeps_its_last_revision_far_from_head() {
        let position = position_with(60);
        let last_valid = position.commits[20].sha.clone();
        let model = LaneModel::evidence(&position, Some(&last_valid), true);
        assert_eq!(model.at, None);
        assert_eq!(model.unknown, Some(LaneUnknown::ConfigurationRemoved));
        assert_eq!(model.last_tracked, Some(20));
        // The unknown-state lane must draw that commit, bounded by gap pills.
        let nodes = lane_window(model.commits.len(), 20, &[]);
        assert!(nodes.contains(&LaneNode::Commit(20)));
        assert!(
            nodes
                .iter()
                .any(|node| matches!(node, LaneNode::Gap { .. })),
            "the 20 newer commits collapse into gap pills"
        );
        assert!(
            nodes.len() < 15,
            "bounded, not one node per commit: {}",
            nodes.len()
        );
    }

    #[test]
    fn unknown_lane_stays_bounded_for_a_far_pick() {
        // A picked commit at index 400 of 500 adds nodes only around itself.
        let nodes = lane_window(500, 5, &[400]);
        assert!(nodes.contains(&LaneNode::Commit(400)));
        assert!(nodes.len() < 25, "{} nodes", nodes.len());
    }

    #[test]
    fn lane_draws_a_deployment_only_when_it_places_both_ends() {
        let mut position = position_with(10);
        position.running_position = Some(3);
        position.unknown_reason = None;
        let known = LaneModel::managed(&position);
        let on_branch = position.commits[1].sha.clone();
        assert!(can_draw_deployment(&known, &deploy_to(Some(&on_branch))));
        // No commit (for example a generation that maps to no tracked commit).
        assert!(!can_draw_deployment(&known, &deploy_to(None)));
        // A commit outside the returned branch window.
        assert!(!can_draw_deployment(
            &known,
            &deploy_to(Some(&"f".repeat(40)))
        ));

        // Unknown source states other than `untracked` cannot be drawn.
        for reason in [
            FlakePositionUnknownReason::OffBranch,
            FlakePositionUnknownReason::BeyondWindow,
            FlakePositionUnknownReason::NotObserved,
            FlakePositionUnknownReason::NoHistory,
        ] {
            let mut unknown = position.clone();
            unknown.running_position = None;
            unknown.unknown_reason = Some(reason);
            let lane = LaneModel::managed(&unknown);
            assert!(
                !can_draw_deployment(&lane, &deploy_to(Some(&on_branch))),
                "{reason:?}"
            );
        }

        // An untracked configuration is a drawable source.
        let mut untracked = position.clone();
        untracked.running_position = None;
        untracked.unknown_reason = Some(FlakePositionUnknownReason::Untracked);
        let lane = LaneModel::managed(&untracked);
        assert!(can_draw_deployment(&lane, &deploy_to(Some(&on_branch))));
    }

    #[test]
    fn evidence_model_uses_selected_revision_and_removed_state() {
        let position = SystemFlakePosition {
            flake_id: Some(1),
            flake_name: Some("fleet".to_string()),
            branch: Some("main".to_string()),
            ordering: crate::api::models::FlakePositionOrdering::BranchSnapshot,
            commits: commits(5),
            running_commit: None,
            running_position: None,
            unknown_reason: Some(FlakePositionUnknownReason::NotObserved),
            last_tracked_position: None,
        };
        let sha = position.commits[2].sha.clone();
        let model = LaneModel::evidence(&position, Some(&sha), false);
        assert_eq!(model.at, Some(2));
        assert_eq!(model.unknown, None);
        let removed = LaneModel::evidence(&position, Some(&sha), true);
        assert_eq!(removed.at, None);
        assert_eq!(removed.unknown, Some(LaneUnknown::ConfigurationRemoved));
    }

    #[test]
    fn stages_start_at_the_running_commit() {
        // Deploying newer (target column to the right) reads left to right.
        assert!(stages_flow_right(2, 5));
        // A rollback to an older commit (target to the left) reads right to left.
        assert!(!stages_flow_right(5, 2));
        assert!(stages_flow_right(3, 3));
    }

    #[test]
    fn same_commit_loop_reaches_the_stage_row() {
        // The loop starts at the dot center and its lowest point (y=70 in the
        // path) must sit on the stage rail center.
        assert_eq!(DOT_FROM_STAGE_ROW_PX, -60);
        assert_eq!(STAGE_CENTER_PX - DOT_FROM_STAGE_ROW_PX, 70);
        assert!(same_commit_loop_path(1, 3).contains("Q 100 70 112 70"));
        // HEAD in the last of two columns: the loop stays centered on the dot.
        let edge = same_commit_loop_path(1, 2);
        assert!(edge.starts_with("M 438 0"), "{edge}");
        assert!(edge.contains("Q 590 70 590 58"), "{edge}");
    }

    #[test]
    fn short_ago_uses_compact_units() {
        let now = Utc::now();
        assert_eq!(short_ago(now, now), "just now");
        assert_eq!(short_ago(now, now - chrono::Duration::minutes(5)), "5m ago");
        assert_eq!(short_ago(now, now - chrono::Duration::hours(3)), "3h ago");
        assert_eq!(short_ago(now, now - chrono::Duration::days(2)), "2d ago");
    }
}
