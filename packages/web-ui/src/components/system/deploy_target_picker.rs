//! Deploy tab "Deploy target" card.
//!
//! A vertical lane that mirrors the flake-position lane: newer commits above
//! the running commit, older commits below. Generations use the same rail.
//! Design reference: `docs/design/CrystalForge/components/DeployTargetPicker.jsx`
//! at dev commit `bd8d068d`.

use crate::api::models::{FlakePositionBuildState, FlakePositionCommit, SystemGeneration};
use crate::components::system::flake_position::short_ago;
use crate::components::{Icon, IconName};
use chrono::Utc;
use dioxus::prelude::*;
use wasm_bindgen::JsCast;

/// DOM id of the scrollable picker list. One picker exists per page.
const LIST_ID: &str = "deploy-target-list";

/// Minimum number of commit rows shown before "Show older commits".
const MIN_VISIBLE_COMMITS: usize = 8;

/// Number of rows shown below the running commit before collapsing.
const ROWS_AFTER_RUNNING: usize = 4;

/// Rows of the commit list after the collapse rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerRows {
    /// Visible commit indexes in display order.
    pub rows: Vec<usize>,
    /// Number of commits that the collapse hides.
    pub hidden: usize,
    /// Whether the list is long enough to offer "Show older commits".
    pub collapsible: bool,
}

/// Applies the design collapse rule to a commit list of length `len`.
///
/// The list shows `max(8, running + 4)` rows. The selected commit stays
/// visible even when it is older than that limit. `show_older` shows all rows.
pub fn picker_rows(
    len: usize,
    running: Option<usize>,
    selected: Option<usize>,
    show_older: bool,
) -> PickerRows {
    let limit = MIN_VISIBLE_COMMITS.max(running.unwrap_or(0) + ROWS_AFTER_RUNNING);
    let rows: Vec<usize> = (0..len)
        .filter(|index| show_older || *index < limit || Some(*index) == selected)
        .collect();
    PickerRows {
        hidden: len - rows.len(),
        rows,
        collapsible: len > limit,
    }
}

/// Scrolls the selected row into view inside the list and refreshes the
/// edge-fade state.
///
/// The function scrolls only the list container. It does not call
/// `scrollIntoView`, because that would also scroll the page.
fn sync_list(mut fade: Signal<(bool, bool)>, reveal_selected: bool) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(list) = document.get_element_by_id(LIST_ID) else {
        return;
    };
    if reveal_selected {
        if let Some(row) = list
            .query_selector(".tp-row.sel")
            .ok()
            .flatten()
            .and_then(|row| row.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let top = row.offset_top();
            let bottom = top + row.offset_height();
            if top < list.scroll_top() + 8 {
                list.set_scroll_top((top - 24).max(0));
            } else if bottom > list.scroll_top() + list.client_height() - 8 {
                list.set_scroll_top(bottom - list.client_height() + 24);
            }
        }
    }
    let at_top = list.scroll_top() > 2;
    let at_bottom = list.scroll_top() + list.client_height() < list.scroll_height() - 2;
    if *fade.peek() != (at_top, at_bottom) {
        fade.set((at_top, at_bottom));
    }
}

/// Deploy target card with Commits and Generations tabs.
///
/// `running` is the index of the running commit in `commits`. Events report a
/// full commit SHA or a generation number; the parent owns the selection.
#[component]
pub fn DeployTargetPicker(
    hostname: String,
    flake_name: String,
    commits: Vec<FlakePositionCommit>,
    running: Option<usize>,
    #[props(default)] degraded: bool,
    generations: Vec<SystemGeneration>,
    generation_mode: bool,
    selected_commit: String,
    selected_generation: Option<i32>,
    on_mode: EventHandler<bool>,
    on_select_commit: EventHandler<String>,
    on_select_generation: EventHandler<i32>,
    on_open_sha: EventHandler<String>,
) -> Element {
    let mut show_older = use_signal(|| false);
    let fade = use_signal(|| (false, false));
    let now = Utc::now();

    let selected_idx = commits.iter().position(|c| c.sha == selected_commit);
    let collapse = picker_rows(commits.len(), running, selected_idx, show_older());
    let newer = running.unwrap_or(0);

    use_effect(use_reactive(
        (&selected_commit, &selected_generation, &generation_mode),
        move |_| sync_list(fade, true),
    ));
    use_effect(move || {
        let _ = show_older();
        sync_list(fade, true);
    });

    let (fade_top, fade_bottom) = fade();
    let list_class = format!(
        "tp-list{}{}",
        if fade_top { " fade-t" } else { "" },
        if fade_bottom { " fade-b" } else { "" }
    );
    let row_count = collapse.rows.len();
    let generation_count = generations.len();

    rsx! {
        section { class: "card sd-card tp-card", "data-testid": "deploy-target-picker",
            div { class: "sd-card-head tp-head",
                h2 { "Deploy target" }
                div { class: "tp-switch", role: "tablist",
                    button {
                        role: "tab",
                        "aria-selected": "{!generation_mode}",
                        class: if generation_mode { "" } else { "on" },
                        onclick: move |_| on_mode.call(false),
                        Icon { name: IconName::Git, size: 12 }
                        " Commits"
                    }
                    button {
                        role: "tab",
                        "aria-selected": "{generation_mode}",
                        class: if generation_mode { "on" } else { "" },
                        onclick: move |_| on_mode.call(true),
                        Icon { name: IconName::Rollback, size: 12 }
                        " Generations"
                    }
                }
            }
            div { class: "tp-sub",
                if generation_mode {
                    "Generations on {hostname}. Switching needs no build."
                } else if newer > 0 {
                    "{flake_name} · "
                    strong { class: "tp-ahead-n", "{newer} newer" }
                    " than what's deployed"
                } else if degraded {
                    "{flake_name} · ordered by commit time; branch position unavailable"
                } else if running.is_some() {
                    "{flake_name} · deployed commit is HEAD"
                } else {
                    "{flake_name}"
                }
            }
            div {
                id: LIST_ID,
                class: "{list_class}",
                onscroll: move |_| sync_list(fade, false),
                if !generation_mode {
                    if commits.is_empty() {
                        div { class: "tp-empty", "No commits available." }
                    }
                    for (k, index) in collapse.rows.iter().copied().enumerate() {
                        {
                            let commit = commits[index].clone();
                            let previous = k.checked_sub(1).map(|p| collapse.rows[p]);
                            let gap_above = previous.filter(|p| *p + 1 != index).map(|p| index - p - 1);
                            let is_sel = selected_commit == commit.sha;
                            let is_cur = running == Some(index);
                            let ahead = running.is_some_and(|r| r > 0 && index < r);
                            let class = format!(
                                "tp-row{}{}{}{}{}{}",
                                if is_sel { " sel" } else { "" },
                                if is_cur { " cur" } else { "" },
                                if ahead { " ahead" } else { "" },
                                if index == 0 { " head" } else { "" },
                                if k == 0 { " first" } else { "" },
                                if k + 1 == row_count && collapse.hidden == 0 { " last" } else { "" },
                            );
                            let sha_select = commit.sha.clone();
                            let sha_open = commit.sha.clone();
                            let short: String = commit.sha.chars().take(7).collect();
                            let building = !is_cur && commit.build_state == FlakePositionBuildState::Building;
                            let unpublished = !is_cur && commit.build_state == FlakePositionBuildState::Unpublished;
                            rsx! {
                                if let Some(count) = gap_above {
                                    div { class: "tp-gap",
                                        span { class: "tp-lane", i {} }
                                        span { "{count} commits" }
                                    }
                                }
                                // ACCESSIBILITY: Selecting the target and opening the commit
                                // are separate buttons. The select button stretches over the
                                // whole row with a pseudo-element (see `.tp-select::after`);
                                // the SHA button sits above it, so both keep their own
                                // keyboard focus and a pointer click anywhere selects the row.
                                div {
                                    class: "{class}",
                                    "data-sha": "{commit.sha}",
                                    span { class: "tp-lane", "aria-hidden": "true", i {} b {} }
                                    span { class: "tp-main",
                                        button {
                                            class: "tp-select focus-ring",
                                            "aria-pressed": "{is_sel}",
                                            title: "{commit.message}",
                                            onclick: move |_| on_select_commit.call(sha_select.clone()),
                                            span { class: "tp-msg", "{commit.message}" }
                                        }
                                        span { class: "tp-meta",
                                            button {
                                                class: "mono tp-sha focus-ring",
                                                title: "Open {short} in Flakes",
                                                "aria-label": "Open {short} in Flakes",
                                                onclick: move |_| on_open_sha.call(sha_open.clone()),
                                                "{short}"
                                            }
                                            span { "{commit.author}" }
                                            span { "{short_ago(now, commit.committed_at)}" }
                                        }
                                    }
                                    span { class: "tp-side",
                                        if is_cur {
                                            span { class: "tp-tag tp-tag-cur", Icon { name: IconName::Server, size: 10 } " {hostname}" }
                                        } else if index == 0 {
                                            span { class: "tp-tag tp-tag-head", "HEAD" }
                                        }
                                        if building {
                                            span { class: "tp-build", title: "Building",
                                                span { class: "deploy-pending-spinner fp-chip-spin", "aria-hidden": "true" }
                                                " building"
                                            }
                                        }
                                        if unpublished {
                                            span {
                                                class: "tp-build tp-unpublished",
                                                title: "Built, but not yet confirmed in the deployment cache. A deployment of this commit can wait or fail until it is published.",
                                                " awaiting cache"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if collapse.collapsible {
                        button {
                            class: "tp-more focus-ring",
                            onclick: move |_| show_older.set(!show_older()),
                            span { class: "tp-lane", i {} }
                            if show_older() {
                                "Show fewer"
                            } else {
                                "Show {collapse.hidden} older commits"
                            }
                        }
                    }
                } else {
                    if generations.is_empty() {
                        div { class: "tp-empty", "No historical generations available." }
                    }
                    for (k, generation) in generations.iter().cloned().enumerate() {
                        {
                            // Origin labels use only server fields. A missing
                            // commit hash means Crystal Forge cannot map the
                            // generation to a tracked commit ("local"). A
                            // generation without retained rollback lineage
                            // cannot be switched to ("unknown").
                            let origin = if generation.commit_hash.is_none() {
                                "local"
                            } else if !generation.rollback_eligible && !generation.is_current {
                                "unknown"
                            } else {
                                "cf"
                            };
                            let origin_label = match origin {
                                "local" => Some("no tracked commit"),
                                "unknown" => Some("not retained"),
                                _ => None,
                            };
                            let is_sel = selected_generation == Some(generation.generation);
                            let class = format!(
                                "tp-row tp-gen o-{origin}{}{}{}{}",
                                if is_sel { " sel" } else { "" },
                                if generation.is_current { " cur" } else { "" },
                                if k == 0 { " first" } else { "" },
                                if k + 1 == generation_count { " last" } else { "" },
                            );
                            let title = match origin {
                                "local" => "The generation does not map to a tracked commit",
                                "unknown" => "No retained rollback lineage for this generation",
                                _ => "",
                            };
                            let number = generation.generation;
                            let sha_open = generation.commit_hash.clone();
                            let short = generation.commit_hash.as_ref().map(|sha| sha.chars().take(7).collect::<String>());
                            let message = if generation.is_current { "Active generation" } else { "Generation rollback" };
                            rsx! {
                                div {
                                    key: "gen-{number}",
                                    class: "{class}",
                                    title: "{title}",
                                    "data-generation": "{number}",
                                    span { class: "tp-lane", "aria-hidden": "true", i {} b {} }
                                    span { class: "tp-main",
                                        button {
                                            class: "tp-select focus-ring",
                                            "aria-pressed": "{is_sel}",
                                            onclick: move |_| on_select_generation.call(number),
                                            span { class: "tp-msg", "{message}" }
                                        }
                                        span { class: "tp-meta",
                                            span { class: "mono tp-gen-id", "#{number}" }
                                            if let (Some(short), Some(sha)) = (short, sha_open) {
                                                button {
                                                    class: "mono tp-sha focus-ring",
                                                    title: "Open {short} in Flakes",
                                                    "aria-label": "Open {short} in Flakes",
                                                    onclick: move |_| on_open_sha.call(sha.clone()),
                                                    "{short}"
                                                }
                                            } else {
                                                span { class: "tp-nosha", "no commit" }
                                            }
                                            span { "{short_ago(now, generation.timestamp)}" }
                                            if let Some(label) = origin_label {
                                                span { class: "tp-origin o-{origin}", Icon { name: IconName::Warn, size: 9 } " {label}" }
                                            }
                                        }
                                    }
                                    span { class: "tp-side",
                                        if generation.is_current {
                                            span { class: "tp-tag tp-tag-cur", Icon { name: IconName::Server, size: 10 } " active" }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_list_shows_every_row() {
        let rows = picker_rows(5, Some(2), None, false);
        assert_eq!(rows.rows, vec![0, 1, 2, 3, 4]);
        assert_eq!(rows.hidden, 0);
        assert!(!rows.collapsible);
    }

    #[test]
    fn long_list_collapses_to_running_plus_four() {
        let rows = picker_rows(30, Some(10), None, false);
        assert_eq!(rows.rows, (0..14).collect::<Vec<_>>());
        assert_eq!(rows.hidden, 16);
        assert!(rows.collapsible);
    }

    #[test]
    fn collapse_keeps_at_least_eight_rows_and_the_selected_row() {
        let rows = picker_rows(30, None, Some(20), false);
        assert_eq!(rows.rows, vec![0, 1, 2, 3, 4, 5, 6, 7, 20]);
        assert_eq!(rows.hidden, 21);
    }

    #[test]
    fn show_older_expands_every_row() {
        let rows = picker_rows(30, Some(1), None, true);
        assert_eq!(rows.rows.len(), 30);
        assert_eq!(rows.hidden, 0);
        assert!(rows.collapsible);
    }
}
