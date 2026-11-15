//! Checkbox multi-select filter from the approved Systems design
//! (`docs/design/CrystalForge/components/MgmtParts.jsx`, `MultiFilter`).

use dioxus::prelude::*;

/// One selectable value of a [`MultiFilter`].
#[derive(Clone, Debug, PartialEq)]
pub struct FilterOption {
    /// Value stored in the selection.
    pub value: String,
    /// Text shown for the option and in the button summary.
    pub label: String,
    /// Optional resource count shown beside the option.
    pub count: Option<usize>,
    /// Optional CSS color of a dot shown before the label (design
    /// `MgmtParts.jsx` `MultiFilter` option `dot`).
    pub dot: Option<String>,
}

/// Returns the button text for a selection: `all_label` when nothing is
/// selected, otherwise the selected labels in option order.
pub fn multi_filter_summary(
    options: &[FilterOption],
    selected: &[String],
    all_label: &str,
) -> String {
    let labels: Vec<&str> = options
        .iter()
        .filter(|option| selected.contains(&option.value))
        .map(|option| option.label.as_str())
        .collect();
    if labels.is_empty() {
        all_label.to_owned()
    } else {
        labels.join(", ")
    }
}

/// Toggles `value` in `selected`, keeping the existing order of other values.
pub fn toggle_selection(selected: &[String], value: &str) -> Vec<String> {
    if selected.iter().any(|v| v == value) {
        selected.iter().filter(|v| *v != value).cloned().collect()
    } else {
        let mut next = selected.to_vec();
        next.push(value.to_owned());
        next
    }
}

/// Renders a filter button that opens a checkbox list.
///
/// An empty selection means no restriction. The menu closes on a click outside
/// it (a transparent backdrop receives that click) and on Escape. The `Clear`
/// action empties the selection and is disabled when nothing is selected.
#[component]
pub fn MultiFilter(
    label: String,
    all_label: String,
    options: Vec<FilterOption>,
    selected: Vec<String>,
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let mut open = use_signal(|| false);
    let summary = multi_filter_summary(&options, &selected, &all_label);
    let selected_count = options
        .iter()
        .filter(|o| selected.contains(&o.value))
        .count();
    let button_class = if selected_count > 0 {
        "input filter-select mf-btn focus-ring active"
    } else {
        "input filter-select mf-btn focus-ring"
    };
    let menu_label = label.clone();
    let clear_selected = selected.is_empty();

    rsx! {
        div {
            class: "mf",
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    open.set(false);
                }
            },
            button {
                r#type: "button",
                class: "{button_class}",
                "aria-haspopup": "listbox",
                "aria-expanded": "{open()}",
                "aria-label": "{label} filter",
                onclick: move |_| open.toggle(),
                span { class: "mf-text",
                    if selected_count > 0 {
                        span { class: "mf-label", "{label}:" }
                        " {summary}"
                    } else {
                        "{summary}"
                    }
                }
                if selected_count > 1 {
                    span { class: "mf-count", "{selected_count}" }
                }
            }
            if open() {
                div {
                    class: "mf-backdrop",
                    onclick: move |_| open.set(false),
                }
                div {
                    class: "mf-menu",
                    role: "listbox",
                    "aria-multiselectable": "true",
                    "aria-label": "{menu_label}",
                    div { class: "mf-head",
                        span { "{menu_label}" }
                        button {
                            r#type: "button",
                            disabled: clear_selected,
                            onclick: move |_| on_change.call(Vec::new()),
                            "Clear"
                        }
                    }
                    for option in options.iter().cloned() {
                        {
                            let checked = selected.contains(&option.value);
                            let next = toggle_selection(&selected, &option.value);
                            rsx! {
                                label { key: "{option.value}", class: "mf-opt",
                                    input {
                                        r#type: "checkbox",
                                        value: "{option.value}",
                                        checked,
                                        onchange: move |_| on_change.call(next.clone()),
                                    }
                                    if let Some(dot) = &option.dot {
                                        span { class: "chip-dot", style: "background: {dot};" }
                                    }
                                    span { class: "mf-opt-label", "{option.label}" }
                                    if let Some(count) = option.count {
                                        span { class: "mf-opt-count", "{count}" }
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

    fn options() -> Vec<FilterOption> {
        ["managed", "external", "reference"]
            .into_iter()
            .map(|v| FilterOption {
                value: v.into(),
                label: v.to_uppercase(),
                count: None,
                dot: None,
            })
            .collect()
    }

    #[test]
    fn summary_uses_all_label_then_option_order() {
        assert_eq!(
            multi_filter_summary(&options(), &[], "All management"),
            "All management"
        );
        let selected = vec!["reference".to_string(), "managed".to_string()];
        assert_eq!(
            multi_filter_summary(&options(), &selected, "All management"),
            "MANAGED, REFERENCE"
        );
    }

    #[test]
    fn toggle_adds_and_removes_one_value() {
        let added = toggle_selection(&["managed".into()], "external");
        assert_eq!(added, vec!["managed".to_string(), "external".to_string()]);
        assert_eq!(
            toggle_selection(&added, "managed"),
            vec!["external".to_string()]
        );
    }
}
