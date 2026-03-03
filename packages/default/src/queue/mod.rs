//! Event-driven queue notification system for Crystal Forge.
//!
//! This module provides an event-driven architecture for both evaluation and build queues,
//! replacing polling-based approaches with immediate notifications.
//!
//! ## Architecture
//!
//! - **Eval Queue**: Notifies eval loop when new commits are inserted
//! - **Build Queue**: Notifies build workers when new build jobs are created
//! - **FIFO Ordering**: MPSC channels maintain insertion order
//! - **Fallback Polling**: Periodic ticks catch any missed notifications
//!
//! ## Usage
//!
//! ```rust,no_run
//! # use crystal_forge::queue::QueueNotifier;
//! # use std::sync::Arc;
//! # use tokio::time;
//! # #[tokio::main]
//! # async fn main() {
//! // Initialize during server startup
//! let queue_notifier = Arc::new(QueueNotifier::new());
//!
//! // In eval loop
//! # let mut ticker = time::interval(tokio::time::Duration::from_secs(60));
//! tokio::select! {
//!     _ = ticker.tick() => { /* periodic fallback */ }
//!     _ = queue_notifier.wait_for_eval_work() => { /* immediate processing */ }
//! }
//!
//! // When inserting commits
//! queue_notifier.notify_eval_queue();
//! # }
//! ```

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::debug;

/// Event-driven queue notification system.
///
/// Uses unbounded MPSC channels to notify workers when new work arrives.
/// This eliminates polling delays and reduces CPU usage when queues are empty.
#[derive(Clone)]
pub struct QueueNotifier {
    eval_tx: mpsc::UnboundedSender<()>,
    eval_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<()>>>,
    build_tx: mpsc::UnboundedSender<()>,
    build_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<()>>>,
}

impl QueueNotifier {
    /// Create a new queue notifier system.
    pub fn new() -> Self {
        let (eval_tx, eval_rx) = mpsc::unbounded_channel();
        let (build_tx, build_rx) = mpsc::unbounded_channel();

        Self {
            eval_tx,
            eval_rx: Arc::new(tokio::sync::Mutex::new(eval_rx)),
            build_tx,
            build_rx: Arc::new(tokio::sync::Mutex::new(build_rx)),
        }
    }

    /// Notify the eval queue that new commits are available.
    ///
    /// This is called when:
    /// - New commits are inserted via flake polling
    /// - Commits are added via webhook
    /// - Manual commit insertion
    ///
    /// Fire-and-forget: errors are logged but don't propagate.
    pub fn notify_eval_queue(&self) {
        // send() on unbounded channel only fails if receiver is dropped
        // In that case, the server is shutting down, so we can ignore the error
        let _ = self.eval_tx.send(());
        debug!("🔔 Notified eval queue of new work");
    }

    /// Notify the build queue that new build jobs are available.
    ///
    /// This is called when:
    /// - Build jobs are created after successful evaluation
    /// - Build jobs are manually queued
    /// - Build jobs are re-queued after failure
    ///
    /// Fire-and-forget: errors are logged but don't propagate.
    pub fn notify_build_queue(&self) {
        let _ = self.build_tx.send(());
        debug!("🔔 Notified build queue of new work");
    }

    /// Wait for eval work notification (async).
    ///
    /// Returns immediately when notified, or never returns if no work arrives.
    /// Should be used in a `tokio::select!` with a fallback ticker.
    pub async fn wait_for_eval_work(&self) {
        let mut rx = self.eval_rx.lock().await;
        let _ = rx.recv().await;
    }

    /// Wait for build work notification (async).
    ///
    /// Returns immediately when notified, or never returns if no work arrives.
    /// Should be used in a `tokio::select!` with a fallback ticker.
    pub async fn wait_for_build_work(&self) {
        let mut rx = self.build_rx.lock().await;
        let _ = rx.recv().await;
    }
}

impl Default for QueueNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_eval_queue_notification() {
        let notifier = QueueNotifier::new();
        let notifier_clone = notifier.clone();

        // Spawn a task that waits for notification
        let handle = tokio::spawn(async move {
            notifier_clone.wait_for_eval_work().await;
        });

        // Give the receiver time to start waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send notification
        notifier.notify_eval_queue();

        // Verify the task completes
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            handle
        ).await;

        assert!(result.is_ok(), "Notification should wake up the receiver");
    }

    #[tokio::test]
    async fn test_build_queue_notification() {
        let notifier = QueueNotifier::new();
        let notifier_clone = notifier.clone();

        // Spawn a task that waits for notification
        let handle = tokio::spawn(async move {
            notifier_clone.wait_for_build_work().await;
        });

        // Give the receiver time to start waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send notification
        notifier.notify_build_queue();

        // Verify the task completes
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            handle
        ).await;

        assert!(result.is_ok(), "Notification should wake up the receiver");
    }

    #[tokio::test]
    async fn test_multiple_notifications_received() {
        let notifier = QueueNotifier::new();

        // Send multiple notifications before anyone is listening
        notifier.notify_eval_queue();
        notifier.notify_eval_queue();
        notifier.notify_eval_queue();

        // All notifications should be in the queue
        // We can consume them one by one
        notifier.wait_for_eval_work().await;
        notifier.wait_for_eval_work().await;
        notifier.wait_for_eval_work().await;

        // This should timeout since all notifications were consumed
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(50),
            notifier.wait_for_eval_work()
        ).await;

        assert!(result.is_err(), "Should timeout after all notifications consumed");
    }
}
