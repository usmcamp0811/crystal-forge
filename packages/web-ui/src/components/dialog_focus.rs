//! Focus management for modal drawers and dialogs.

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use std::rc::Rc;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

/// Selects the focusable edge to receive focus from a dialog sentinel.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum DialogFocusBoundary {
    /// Focuses the first enabled control.
    First,
    /// Focuses the last enabled control.
    Last,
}

#[cfg(target_arch = "wasm32")]
struct DialogFocusGuardInner {
    opener: Option<web_sys::HtmlElement>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
struct DialogFocusGuard {
    _inner: Rc<DialogFocusGuardInner>,
}

#[cfg(target_arch = "wasm32")]
impl DialogFocusGuard {
    fn capture() -> Self {
        Self {
            _inner: Rc::new(DialogFocusGuardInner {
                opener: web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.active_element())
                    .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok()),
            }),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for DialogFocusGuardInner {
    fn drop(&mut self) {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        if let Some(opener) = self.opener.as_ref() {
            let connected = js_sys::Reflect::get(opener.as_ref(), &"isConnected".into())
                .ok()
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if connected && !matches!(opener.tag_name().as_str(), "BODY" | "HTML") {
                if opener.focus().is_ok() {
                    return;
                }
            }
        }
        if let Ok(Some(element)) = document.query_selector("main h1, main [role='heading']")
            && let Ok(element) = element.dyn_into::<web_sys::HtmlElement>()
        {
            if element.tab_index() < 0 {
                let _ = element.set_attribute("tabindex", "-1");
            }
            let _ = element.focus();
        }
    }
}

/// Captures the active opener and restores focus when the owning dialog unmounts.
#[component]
pub(crate) fn DialogFocusRestore() -> Element {
    #[cfg(target_arch = "wasm32")]
    let _guard = use_hook(DialogFocusGuard::capture);

    rsx! {}
}

/// Moves focus into a modal dialog once the dialog element exists.
///
/// The `autofocus` content attribute is only honored for elements that are
/// present while a document performs its initial autofocus processing. A
/// dialog mounted later keeps focus on whatever opened it, so a modal that
/// declares `autofocus` alone never actually receives focus. Render this
/// component inside the dialog subtree to focus the dialog's first enabled
/// control after the dialog is inserted.
///
/// Render [`DialogFocusRestore`] in the same dialog to return focus to the
/// opener on unmount. `DialogFocusRestore` captures the opener while the
/// component renders, before this effect runs, so the two compose in any
/// order.
#[component]
pub(crate) fn DialogInitialFocus(dialog_id: String) -> Element {
    use_effect(move || focus_dialog_boundary(&dialog_id, DialogFocusBoundary::First));

    rsx! {}
}

fn focus_dialog_boundary(dialog_id: &str, boundary: DialogFocusBoundary) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(dialog) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(dialog_id))
        else {
            return;
        };
        let Ok(nodes) = dialog.query_selector_all(
            "button, [href], input, select, textarea, [tabindex]:not(.cf-focus-sentinel)",
        ) else {
            return;
        };
        let focusable = (0..nodes.length())
            .filter_map(|index| nodes.item(index))
            .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            .filter(|element| {
                element.tab_index() >= 0
                    && !element.has_attribute("disabled")
                    && element.get_attribute("aria-hidden").as_deref() != Some("true")
            })
            .collect::<Vec<_>>();
        let target = match boundary {
            DialogFocusBoundary::First => focusable.first(),
            DialogFocusBoundary::Last => focusable.last(),
        };
        if let Some(target) = target {
            let _ = target.focus();
        } else if let Ok(dialog) = dialog.dyn_into::<web_sys::HtmlElement>() {
            let _ = dialog.focus();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = (dialog_id, boundary);
}

#[derive(Clone, PartialEq, Props)]
pub(crate) struct DialogFocusSentinelProps {
    /// Identifies the dialog whose focus boundary is selected.
    pub dialog_id: String,
    /// Selects the boundary that receives focus.
    pub boundary: DialogFocusBoundary,
}

/// Wraps Tab focus at one edge of a modal dialog.
#[component]
pub(crate) fn DialogFocusSentinel(props: DialogFocusSentinelProps) -> Element {
    rsx! {
        span {
            class: "cf-focus-sentinel",
            tabindex: "0",
            aria_label: match props.boundary {
                DialogFocusBoundary::First => "End of dialog",
                DialogFocusBoundary::Last => "Start of dialog",
            },
            onfocus: move |_| focus_dialog_boundary(&props.dialog_id, props.boundary),
        }
    }
}
