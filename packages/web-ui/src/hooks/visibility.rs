//! Document visibility gate for background work.
//!
//! A hidden tab must not poll, tick or measure. Callers that run a timed loop
//! replace their sleep with [`visible_tick`]: it waits for the interval, then,
//! when the document is hidden, parks until the document is visible again and
//! returns at once. The loop body therefore refreshes stale data immediately on
//! return and runs no timer, request or render work in between.
//!
//! # Ownership
//!
//! One [`VisibilityGate`] exists per thread, and one `visibilitychange`
//! listener feeds it. The listener belongs to the application and lives as long
//! as the page. Waiters hold no listener of their own, so the number of
//! listeners does not grow with the number of polling loops.
//!
//! The gate is plain Rust and is tested natively. The browser glue is compiled
//! for `wasm32` only; native builds report a visible document.

use std::cell::{Cell, RefCell};
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};

/// Tracks whether the document is hidden and parks tasks that wait for it.
#[derive(Default)]
pub(crate) struct VisibilityGate {
    hidden: Cell<bool>,
    waiters: RefCell<Vec<Waker>>,
}

impl VisibilityGate {
    /// Returns whether the document is hidden.
    pub(crate) fn is_hidden(&self) -> bool {
        self.hidden.get()
    }

    /// Records the document state and, on a hidden-to-visible transition, wakes
    /// every parked waiter exactly once. Other transitions wake nothing.
    pub(crate) fn set_hidden(&self, hidden: bool) {
        let was_hidden = self.hidden.replace(hidden);
        if was_hidden && !hidden {
            // Take the list first: a woken task may park again re-entrantly.
            let parked = std::mem::take(&mut *self.waiters.borrow_mut());
            for waker in parked {
                waker.wake();
            }
        }
    }

    /// Returns the number of parked waiters.
    #[cfg(test)]
    pub(crate) fn parked(&self) -> usize {
        self.waiters.borrow().len()
    }

    /// Completes when the document is visible. Completes immediately when it
    /// already is. A task that is polled again while still parked is recorded
    /// once.
    pub(crate) async fn wait_visible(&self) {
        poll_fn(|cx| {
            if !self.hidden.get() {
                return Poll::Ready(());
            }
            let mut waiters = self.waiters.borrow_mut();
            if !waiters.iter().any(|parked| parked.will_wake(cx.waker())) {
                waiters.push(cx.waker().clone());
            }
            Poll::Pending
        })
        .await
    }
}

thread_local! {
    static GATE: Rc<VisibilityGate> = Rc::new(VisibilityGate::default());
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// The application-lifetime listener. It is stored, not forgotten, so its
    /// owner is explicit; it is never removed because the page owns it.
    static LISTENER: RefCell<Option<wasm_bindgen::prelude::Closure<dyn FnMut()>>> =
        const { RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn document_hidden() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| document.hidden())
}

/// Returns the shared gate, installing the browser listener on first use.
fn gate() -> Rc<VisibilityGate> {
    let gate = GATE.with(Rc::clone);
    #[cfg(target_arch = "wasm32")]
    LISTENER.with(|slot| {
        use wasm_bindgen::JsCast;
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return;
        }
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        gate.set_hidden(document.hidden());
        let target = Rc::clone(&gate);
        let listener = wasm_bindgen::prelude::Closure::<dyn FnMut()>::new(move || {
            target.set_hidden(document_hidden());
        });
        let _ = document.add_event_listener_with_callback(
            "visibilitychange",
            listener.as_ref().unchecked_ref(),
        );
        *slot = Some(listener);
    });
    gate
}

/// Returns whether the document is currently hidden.
pub fn is_hidden() -> bool {
    gate().is_hidden()
}

/// Waits until the document is visible. Returns at once when it already is.
pub async fn wait_visible() {
    gate().wait_visible().await;
}

/// Waits `interval_ms`, then, if the document is hidden, waits until it is
/// visible again.
///
/// While hidden, the caller holds no timer and performs no work. The first
/// call after the document becomes visible returns without further delay, so a
/// polling loop refreshes stale data as soon as the tab is shown.
pub async fn visible_tick(interval_ms: u32) {
    gloo_timers::future::TimeoutFuture::new(interval_ms).await;
    wait_visible().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Wake};

    struct CountingWaker(AtomicUsize);

    impl Wake for CountingWaker {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn poll_once(
        future: &mut Pin<Box<dyn Future<Output = ()> + '_>>,
        waker: &Arc<CountingWaker>,
    ) -> Poll<()> {
        let waker = Waker::from(Arc::clone(waker));
        future.as_mut().poll(&mut Context::from_waker(&waker))
    }

    fn counter() -> Arc<CountingWaker> {
        Arc::new(CountingWaker(AtomicUsize::new(0)))
    }

    #[test]
    fn visible_document_does_not_park() {
        let gate = VisibilityGate::default();
        let waker = counter();
        let mut wait: Pin<Box<dyn Future<Output = ()> + '_>> = Box::pin(gate.wait_visible());
        assert_eq!(poll_once(&mut wait, &waker), Poll::Ready(()));
        assert_eq!(gate.parked(), 0);
    }

    #[test]
    fn hidden_document_parks_until_visible_and_then_wakes_once() {
        let gate = VisibilityGate::default();
        gate.set_hidden(true);
        let waker = counter();
        let mut wait: Pin<Box<dyn Future<Output = ()> + '_>> = Box::pin(gate.wait_visible());
        assert_eq!(poll_once(&mut wait, &waker), Poll::Pending);
        assert_eq!(waker.0.load(Ordering::SeqCst), 0, "hidden tab does no work");
        gate.set_hidden(false);
        assert_eq!(waker.0.load(Ordering::SeqCst), 1);
        assert_eq!(poll_once(&mut wait, &waker), Poll::Ready(()));
        assert_eq!(gate.parked(), 0, "the parked list is released on resume");
    }

    #[test]
    fn every_parked_waiter_is_woken_on_resume() {
        let gate = VisibilityGate::default();
        gate.set_hidden(true);
        let wakers: Vec<_> = (0..5).map(|_| counter()).collect();
        let mut waits: Vec<Pin<Box<dyn Future<Output = ()> + '_>>> =
            (0..5).map(|_| Box::pin(gate.wait_visible()) as _).collect();
        for (wait, waker) in waits.iter_mut().zip(&wakers) {
            assert_eq!(poll_once(wait, waker), Poll::Pending);
        }
        assert_eq!(gate.parked(), 5);
        gate.set_hidden(false);
        assert!(wakers.iter().all(|w| w.0.load(Ordering::SeqCst) == 1));
        assert_eq!(gate.parked(), 0);
    }

    #[test]
    fn repolling_a_parked_task_registers_it_once() {
        let gate = VisibilityGate::default();
        gate.set_hidden(true);
        let waker = counter();
        let mut wait: Pin<Box<dyn Future<Output = ()> + '_>> = Box::pin(gate.wait_visible());
        for _ in 0..10 {
            assert_eq!(poll_once(&mut wait, &waker), Poll::Pending);
        }
        assert_eq!(gate.parked(), 1);
    }

    #[test]
    fn only_a_hidden_to_visible_transition_wakes_waiters() {
        let gate = VisibilityGate::default();
        let waker = counter();
        // Visible -> visible and visible -> hidden wake nothing.
        gate.set_hidden(false);
        gate.set_hidden(true);
        let mut wait: Pin<Box<dyn Future<Output = ()> + '_>> = Box::pin(gate.wait_visible());
        assert_eq!(poll_once(&mut wait, &waker), Poll::Pending);
        // Hidden -> hidden is not a resume.
        gate.set_hidden(true);
        assert_eq!(waker.0.load(Ordering::SeqCst), 0);
        gate.set_hidden(false);
        assert_eq!(waker.0.load(Ordering::SeqCst), 1);
    }
}
