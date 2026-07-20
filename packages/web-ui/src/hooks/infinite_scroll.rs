//! Infinite-scroll hook for Dioxus WASM.
//!
//! Mirrors `useInfiniteScroll(resetKey, pageSize = 30)` from
//! `docs/design/CrystalForge/components/Shell.jsx`, with the following
//! behavioural differences from the v1 implementation:
//!
//! * Listens on the `.content` scroll container (where Crystal Forge's
//!   vertical scroll actually occurs) via `IntersectionObserver`, not on
//!   `window`.
//! * The observer is disconnected in effect cleanup so listeners never
//!   multiply across sentinel remounts.
//! * The caller explicitly requests a re-evaluation (via [`InfiniteScroll::recheck`])
//!   after the rendered list grows, which avoids the unbounded re-registration
//!   loop that would occur if the hook re-registered on every `count` change.
//!
//! ## Usage
//!
//! ```rust,ignore
//! let paging = use_infinite_scroll(tab_key + "|" + &search_q, 20);
//! let paged  = full_list.iter().take(paging.count()).cloned().collect::<Vec<_>>();
//! let has_more = paging.count() < full_list.len();
//! // Beneath the list table in rsx!:
//! if has_more {
//!     div {
//!         class: "infinite-sentinel",
//!         "data-sentinel": paging.sentinel_id(),
//!         onmounted: move |_| paging.check_and_register(),
//!         "Loading more…"
//!     }
//! }
//! ```

use dioxus::prelude::*;

/// Next unique sentinel number, used to give each instance a distinct DOM attribute.
static SENTINEL_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Handle returned by [`use_infinite_scroll`].
#[derive(Clone, Copy, PartialEq)]
pub struct InfiniteScroll {
    count: Signal<usize>,
    #[allow(dead_code)] // used inside #[cfg(target_arch = "wasm32")] blocks
    page_size: usize,
    /// Unique numeric id for locating the sentinel in the DOM.
    id: u32,
    /// Monotonically-increasing version bumped by [`recheck`]; a `use_effect`
    /// inside [`use_infinite_scroll`] watches this and re-registers the
    /// observer when it changes.
    recheck_version: Signal<usize>,
    /// The rendered-list length from the last successful recheck.  Only when
    /// `min(count, rendered_len)` exceeds this value does [`recheck`] bump
    /// `recheck_version`, preventing the feedback loop where re-registration
    /// causes an initial IntersectionObserver fire that increments `count`.
    last_render_len: Signal<usize>,
}

impl InfiniteScroll {
    /// Current number of items to display.
    pub fn count(&self) -> usize {
        *self.count.read()
    }

    /// Value for the `data-sentinel` attribute on the sentinel div.
    pub fn sentinel_id(&self) -> String {
        format!("s{}", self.id)
    }

    /// Call from the sentinel's `onmounted` event.
    ///
    /// Creates an `IntersectionObserver` rooted on the nearest `.content`
    /// ancestor (Crystal Forge's scroll container) with a 400 px root
    /// margin.  When the sentinel enters that expanded viewport the observer
    /// callback increments `count` by `page_size`.  The observer is stored
    /// on the sentinel element itself so the owning `use_effect` can
    /// disconnect it during cleanup.
    pub fn check_and_register(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::prelude::*;
            use wasm_bindgen::JsCast;

            let page_size = self.page_size;
            let count = self.count;
            let id = self.id;

            let Some(window) = web_sys::window() else {
                return;
            };
            let Some(doc) = window.document() else {
                return;
            };

            // Locate the sentinel element.
            let selector = format!("[data-sentinel='s{}']", id);
            let Some(sentinel) = doc.query_selector(&selector).ok().flatten() else {
                return;
            };

            // Walk up to the nearest .content ancestor to use as the
            // IntersectionObserver root.  Fall back to None (= viewport) if
            // no such ancestor exists.
            let scroll_root: Option<web_sys::Element> = {
                let mut node = sentinel.parent_element();
                let mut found = None;
                while let Some(el) = node {
                    if el.class_list().contains("content") {
                        found = Some(el.clone());
                        break;
                    }
                    node = el.parent_element();
                }
                found
            };

            // Build IntersectionObserver callback.
            let cb = Closure::<dyn Fn(js_sys::Array)>::new(move |entries: js_sys::Array| {
                for entry in entries.iter() {
                    let entry: web_sys::IntersectionObserverEntry = entry.unchecked_into();
                    if entry.is_intersecting() {
                        let mut count = count;
                        count.with_mut(|c| *c += page_size);
                    }
                }
            });

            // Configure: 400 px root margin so loading triggers before the
            // sentinel is fully in view, matching the design reference.
            let mut opts = web_sys::IntersectionObserverInit::new();
            opts.root_margin("400px");
            if let Some(ref root) = scroll_root {
                opts.root(Some(root));
            }

            let observer =
                web_sys::IntersectionObserver::new_with_options(cb.as_ref().unchecked_ref(), &opts)
                    .expect("IntersectionObserver construction failed");

            observer.observe(&sentinel);

            // Store the observer reference in a thread-local so the cleanup
            // effect can retrieve and disconnect it.  If a previous observer
            // for this sentinel id already exists (e.g. the sentinel was
            // remounted without a full key reset), disconnect it first.
            OBSERVER_REGISTRY.with(|reg| {
                let mut reg = reg.borrow_mut();
                if let Some((old_observer, _)) = reg.insert(id, (observer, cb)) {
                    old_observer.disconnect();
                }
            });
        }
    }

    /// Re-evaluate the sentinel after the rendered list has grown.
    ///
    /// `rendered_count` should be `min(count, total_list_len)` — the actual
    /// number of items currently displayed.  Only when this value has
    /// increased since the last call is a re-registration triggered, which
    /// prevents the feedback loop described in the module docs.
    ///
    /// Call from the server-response or advancement `use_effect` after
    /// updating the backing list signal.
    pub fn recheck(&self, rendered_count: usize) {
        use std::cmp::min;
        let current_display = min(*self.count.read(), rendered_count);
        let last = *self.last_render_len.read();
        if current_display > last {
            // Copy the signals out of `&self` so we can call `.set()` on
            // the owned copies (Signal<T> is Copy).
            let mut last_render_len = self.last_render_len;
            let mut recheck_version = self.recheck_version;
            last_render_len.set(current_display);
            // Bump the version so the internal use_effect re-registers.
            let next_version = *recheck_version.read() + 1;
            recheck_version.set(next_version);
        }
    }

    /// Disconnect any active observer for this sentinel.
    /// Called by the `use_effect` cleanup so old listeners don't survive
    /// key resets or unmounts.
    pub fn disconnect(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            OBSERVER_REGISTRY.with(|reg| {
                if let Some((observer, _cb)) = reg.borrow_mut().remove(&self.id) {
                    observer.disconnect();
                }
            });
        }
    }
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// Maps sentinel id → (IntersectionObserver, owning Closure).
    /// Keeping the Closure here prevents it from being dropped while the
    /// observer is active.  Entries are removed (and the observer disconnected)
    /// in `InfiniteScroll::disconnect`.
    static OBSERVER_REGISTRY: std::cell::RefCell<
        std::collections::HashMap<u32, (web_sys::IntersectionObserver, wasm_bindgen::prelude::Closure<dyn Fn(js_sys::Array)>)>
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Paginate a client-side list for infinite scroll.
///
/// - `reset_key`: a string that changes whenever the visible list should reset
///   to the first page (e.g., tab switch, search query, filter change).
/// - `page_size`: items rendered initially and added per scroll trigger.
///
/// Render `list[..handle.count()]` items, and place a sentinel element below
/// the list calling `handle.check_and_register()` on `onmounted`.  The
/// sentinel disappears when `count >= list.len()`, which disconnects the
/// observer automatically (sentinel is no longer in the DOM).
pub fn use_infinite_scroll(reset_key: String, page_size: usize) -> InfiniteScroll {
    // Stable unique id for this hook instance.
    let id = use_signal(|| SENTINEL_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed));

    let mut count = use_signal(|| page_size);
    let mut prev_key = use_signal(|| reset_key.clone());
    let mut recheck_version = use_signal(|| 0_usize);
    let mut last_render_len = use_signal(|| page_size);
    #[allow(dead_code)]
    let handle_id = *id.read();

    // Reset count when the key changes.
    // Runs synchronously on every render (not inside a use_effect) so the
    // comparison is always against the latest reset_key value and Dioxus
    // does not need to track signal dependencies.
    //
    // The IntersectionObserver is intentionally left connected across
    // resets: it targets the same sentinel element and the same count
    // signal.  Disconnecting here would require the sentinel to remount
    // and invoke onmounted again, but Dioxus may reuse the DOM node and
    // onmounted only fires once per element (review finding #2).
    if *prev_key.read() != reset_key {
        prev_key.set(reset_key);
        count.set(page_size);
        last_render_len.set(page_size);
        // Also bump the version so the sentinel gets a fresh observer
        // after the reset, even if the caller does not explicitly recheck.
        // Without this the observer from before the reset may target an
        // element that no longer matches the current sentinel id.
        recheck_version.set(recheck_version() + 1);
    }

    // Re-register the observer when recheck_version is bumped.
    // This effect fires *at most once per explicit recheck() call*, never
    // as a side-effect of the observer incrementing count, so there is no
    // feedback loop.
    use_effect(move || {
        let _ = recheck_version();
        self::InfiniteScroll {
            count,
            page_size,
            id: handle_id,
            recheck_version,
            last_render_len,
        }
        .check_and_register();
    });

    // Disconnect observer on component unmount.
    use_drop(move || {
        #[cfg(target_arch = "wasm32")]
        OBSERVER_REGISTRY.with(|reg| {
            if let Some((observer, _cb)) = reg.borrow_mut().remove(&handle_id) {
                observer.disconnect();
            }
        });
    });

    InfiniteScroll {
        count,
        page_size,
        id: *id.read(),
        recheck_version,
        last_render_len,
    }
}
