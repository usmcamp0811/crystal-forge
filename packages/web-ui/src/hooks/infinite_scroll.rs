//! Infinite-scroll hook for Dioxus WASM.
//!
//! Mirrors `useInfiniteScroll(resetKey, pageSize = 30)` from
//! `docs/design/CrystalForge/components/Shell.jsx`.
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

/// Lead-in pixels before the sentinel hits the viewport bottom.
#[cfg(target_arch = "wasm32")]
const LEAD_PX: f64 = 400.0;

/// Next unique sentinel number, used to give each instance a distinct DOM attribute.
static SENTINEL_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Handle returned by [`use_infinite_scroll`].
#[derive(Clone, Copy)]
pub struct InfiniteScroll {
    count: Signal<usize>,
    #[allow(dead_code)] // used inside #[cfg(target_arch = "wasm32")] blocks
    page_size: usize,
    /// Unique numeric id for locating the sentinel in the DOM.
    id: u32,
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

    /// Attach to the sentinel's `onmounted` event.  Fires an immediate
    /// viewport check and registers a passive `scroll` listener on `window`
    /// that grows `count` by `page_size` whenever the sentinel is within
    /// `LEAD_PX` of the visible bottom edge.
    pub fn check_and_register(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use wasm_bindgen::prelude::*;

            let page_size = self.page_size;
            let mut count = self.count;
            let id = self.id;

            let Some(window) = web_sys::window() else {
                return;
            };

            let win_clone = window.clone();
            let check_fn = Closure::<dyn Fn()>::new(move || {
                let Some(doc) = win_clone.document() else {
                    return;
                };
                let selector = format!("[data-sentinel='s{}']", id);
                let Some(el) = doc.query_selector(&selector).ok().flatten() else {
                    return; // sentinel unmounted (tab switch); listener will become a no-op
                };
                let rect = el.get_bounding_client_rect();
                let vh = win_clone
                    .inner_height()
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(768.0);
                if rect.top() < vh + LEAD_PX {
                    count.with_mut(|c| *c += page_size);
                }
            });

            // Fire immediately (after current microtask queue settles).
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                check_fn.as_ref().unchecked_ref(),
                0,
            );

            // Persistent scroll listener.
            let _ = window
                .add_event_listener_with_callback("scroll", check_fn.as_ref().unchecked_ref());

            // The closure must stay alive for the lifetime of the listener.
            // Each sentinel is mounted once per view lifetime, so this is a
            // small bounded allocation.
            check_fn.forget();
        }
    }
}

/// Paginate a client-side list for infinite scroll.
///
/// - `reset_key`: a string that changes whenever the visible list should reset
///   to the first page (e.g., tab switch, search query, filter change).
/// - `page_size`: items rendered initially and added per scroll trigger.
///
/// Render `list[..handle.count()]` items, and place a sentinel element below
/// the list calling `handle.check_and_register()` on `onmounted`.
pub fn use_infinite_scroll(reset_key: String, page_size: usize) -> InfiniteScroll {
    // Stable unique id for this hook instance.
    let id = use_signal(|| SENTINEL_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed));

    let mut count = use_signal(|| page_size);
    let mut prev_key = use_signal(|| reset_key.clone());

    // Reset to first page when the key changes.
    use_effect(move || {
        let key = reset_key.clone();
        if *prev_key.read() != key {
            prev_key.set(key);
            count.set(page_size);
        }
    });

    InfiniteScroll {
        count,
        page_size,
        id: *id.read(),
    }
}
