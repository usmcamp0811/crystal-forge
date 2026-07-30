use std::collections::HashSet;
use std::hash::Hash;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LatestFilterState {
    enabled: bool,
}

impl LatestFilterState {
    pub fn enabled(self) -> bool {
        self.enabled
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn clear(&mut self) {
        self.enabled = false;
    }
}

pub fn normalized_filter(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub fn request_state(value: &str, latest: LatestFilterState) -> (Option<String>, bool) {
    (normalized_filter(value), latest.enabled())
}

pub fn reset_key(domain: &str, criteria: &[&str], latest_only: bool) -> String {
    format!("{domain}|{}|latest={latest_only}", criteria.join("|"))
}

pub fn marker_matches(latest_only: bool, is_latest_per_flake: bool) -> bool {
    !latest_only || is_latest_per_flake
}

pub fn retain_visible<T>(selected: &mut HashSet<T>, visible: impl IntoIterator<Item = T>) -> bool
where
    T: Eq + Hash,
{
    let visible = visible.into_iter().collect::<HashSet<_>>();
    let previous_len = selected.len();
    selected.retain(|id| visible.contains(id));
    selected.len() != previous_len
}

pub fn replace_unique_by<T, K>(items: impl IntoIterator<Item = T>, key: impl Fn(&T) -> K) -> Vec<T>
where
    K: Eq + Hash,
{
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(key(item)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_toggle_persists_independently_of_tab_keys() {
        let mut state = LatestFilterState::default();
        state.toggle();

        assert!(state.enabled());
        assert_ne!(
            reset_key("active", &["needle"], state.enabled()),
            reset_key("history", &["needle"], state.enabled())
        );
        assert!(state.enabled());
    }

    #[test]
    fn marker_filter_only_restricts_rows_when_enabled() {
        assert!(marker_matches(false, false));
        assert!(marker_matches(false, true));
        assert!(!marker_matches(true, false));
        assert!(marker_matches(true, true));
    }

    #[test]
    fn reset_key_tracks_all_effective_criteria() {
        let base = reset_key("history", &["failed", "flake-a", "needle"], false);
        assert_ne!(
            base,
            reset_key("history", &["all", "flake-a", "needle"], false)
        );
        assert_ne!(
            base,
            reset_key("history", &["failed", "flake-b", "needle"], false)
        );
        assert_ne!(
            base,
            reset_key("history", &["failed", "flake-a", "other"], false)
        );
        assert_ne!(
            base,
            reset_key("history", &["failed", "flake-a", "needle"], true)
        );
    }

    #[test]
    fn request_state_combines_normalized_search_and_latest_toggle() {
        let mut latest = LatestFilterState::default();
        latest.toggle();

        assert_eq!(
            request_state("  flake-a  ", latest),
            (Some("flake-a".to_string()), true)
        );
        assert_eq!(request_state("   ", latest), (None, true));
    }

    #[test]
    fn live_replacement_deduplicates_and_selection_drops_hidden_rows() {
        let replacement = replace_unique_by([3, 2, 2, 1], |id| *id);
        assert_eq!(replacement, vec![3, 2, 1]);

        let mut selected = HashSet::from([1, 2, 4]);
        assert!(retain_visible(&mut selected, replacement));
        assert_eq!(selected, HashSet::from([1, 2]));
    }
}
