//! Owns attempt-bound producers, bounded replay, and atomic subscriptions.
//!
//! A state instance owns its registry. Producer state clones carry only an
//! immutable owner/generation token, never a sender. Rotation drops the sole
//! sender and closes old receivers. Publishing cannot create or retarget entries.

use std::collections::{HashMap, VecDeque};

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::handlers::agent_request::CFState;
use crate::queries::eval_logs::{EvalLogOwner, claimed_owner_in_tx};

/// Identifies one registry generation without retaining its broadcast sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProducerToken {
    /// Exact durable owner of this generation.
    pub(crate) owner: EvalLogOwner,
    generation: Uuid,
}

struct Entry {
    token: ProducerToken,
    sender: broadcast::Sender<String>,
    history: VecDeque<String>,
}

/// Stores the sole sender and replay history together under one state mutex.
#[derive(Default)]
pub(crate) struct EvalLogRegistry {
    entries: HashMap<i32, Entry>,
}

impl EvalLogRegistry {
    fn bind(&mut self, owner: EvalLogOwner) -> ProducerToken {
        if let Some(entry) = self.entries.get(&owner.commit_id) {
            if entry.token.owner == owner {
                return entry.token;
            }
        }
        let token = ProducerToken {
            owner,
            generation: Uuid::new_v4(),
        };
        if !self.entries.contains_key(&owner.commit_id)
            && self.entries.len() >= super::MAX_EVAL_LOG_CHANNELS
        {
            // Preserve evaluator progress when the replay cap is reached. This
            // detached token has an exact owner but cannot publish or create an
            // entry. Durable exact-owner logging remains available.
            tracing::warn!(
                commit_id = owner.commit_id,
                "Evaluation log registry is full; live logs unavailable"
            );
            return token;
        }
        let (sender, _) = broadcast::channel(super::EVAL_LOG_CHANNEL_BUFFER);
        self.entries.insert(
            owner.commit_id,
            Entry {
                token,
                sender,
                history: VecDeque::new(),
            },
        );
        token
    }

    /// Subscribes and snapshots replay at one publication boundary.
    ///
    /// Only the exact authorized owner can subscribe. The receiver does not
    /// keep the sender alive. No event can fall between replay and live delivery.
    pub(crate) fn subscribe(
        &self,
        owner: EvalLogOwner,
    ) -> Option<(broadcast::Receiver<String>, VecDeque<String>)> {
        let entry = self.entries.get(&owner.commit_id)?;
        (entry.token.owner == owner).then(|| (entry.sender.subscribe(), entry.history.clone()))
    }

    fn publish(&mut self, token: ProducerToken, json: String) {
        let Some(entry) = self.entries.get_mut(&token.owner.commit_id) else {
            return;
        };
        if entry.token != token {
            // SECURITY: Superseded diagnostics belong to the old owner. Drop
            // them rather than relabeling them or recreating a retired channel.
            return;
        }
        entry.history.push_back(json.clone());
        if entry.history.len() > super::EVAL_LOG_HISTORY_BUFFER {
            entry.history.pop_front();
        }
        let _ = entry.sender.send(json);
    }

    /// Retires only the producer generation that scheduled cleanup.
    pub(crate) fn remove(&mut self, token: ProducerToken) {
        if self.entries.get(&token.owner.commit_id).map(|e| e.token) == Some(token) {
            self.entries.remove(&token.owner.commit_id);
        }
    }
}

/// Returns a producer-state clone bound to the actual claimed expected attempt.
///
/// Database commit/attempt locks precede the registry mutex. Binding an already
/// bound state is permitted only for the same exact owner; an old worker cannot
/// acquire a successor's token. All helpers and spawned preparation tasks must
/// receive this clone. The base request state has no publication authority.
///
/// # Errors
/// Returns SQL errors, `RowNotFound` for a superseded generation/claim, or a
/// protocol error for an attempted producer-owner change. Registry capacity
/// suppresses live logs without failing evaluation or persistent logging.
pub(crate) async fn bind_eval_producer(
    state: &CFState,
    commit_id: i32,
    expected_attempt: i32,
) -> Result<CFState, sqlx::Error> {
    let mut tx = state.pool.begin().await?;
    let owner = claimed_owner_in_tx(&mut tx, commit_id, expected_attempt).await?;
    if state
        .eval_log_producer
        .is_some_and(|token| token.owner != owner)
    {
        return Err(sqlx::Error::Protocol(
            "evaluation producer owner changed".into(),
        ));
    }
    let token = {
        let mut registry = state.eval_log_registry.lock().await;
        if let Some(token) = state.eval_log_producer {
            // Do not resurrect a retired generation by rebinding an old clone.
            if registry
                .entries
                .get(&commit_id)
                .is_some_and(|e| e.token != token)
            {
                return Err(sqlx::Error::RowNotFound);
            }
            token
        } else {
            registry.bind(owner)
        }
    };
    tx.commit().await?;
    let mut producer = state.clone();
    producer.eval_log_producer = Some(token);
    Ok(producer)
}

/// Publishes serialized wire data only for the state's exact live capability.
pub(super) async fn publish(state: &CFState, commit_id: i32, json: String) {
    let Some(token) = state
        .eval_log_producer
        .filter(|t| t.owner.commit_id == commit_id)
    else {
        return;
    };
    state.eval_log_registry.lock().await.publish(token, json);
}

/// Binds an isolated pure-test registry without consulting its lazy database.
#[cfg(test)]
pub(super) async fn test_producer(state: &CFState, owner: EvalLogOwner) -> CFState {
    let mut state = state.clone();
    state.eval_log_producer = Some(state.eval_log_registry.lock().await.bind(owner));
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(number: i32) -> EvalLogOwner {
        EvalLogOwner {
            commit_id: 42,
            evaluation_attempt_id: Uuid::new_v4(),
            attempt_number: number,
        }
    }

    #[test]
    fn rotation_fences_producer_cleanup_and_closes_old_receivers() {
        let mut registry = EvalLogRegistry::default();
        let a = registry.bind(owner(1));
        registry.publish(a, "A-private".into());
        let (mut old, history) = registry.subscribe(a.owner).unwrap();
        assert_eq!(history[0], "A-private");
        let b = registry.bind(owner(2));
        assert!(matches!(
            old.try_recv(),
            Err(broadcast::error::TryRecvError::Closed)
        ));
        registry.publish(a, "A-late".into());
        registry.remove(a);
        assert!(registry.subscribe(a.owner).is_none());
        let (mut live, history) = registry.subscribe(b.owner).unwrap();
        assert!(history.is_empty());
        registry.publish(b, "B".into());
        assert_eq!(live.try_recv().unwrap(), "B");
        registry.remove(b);
        registry.publish(b, "B-retired".into());
        assert!(registry.subscribe(b.owner).is_none());
    }

    #[test]
    fn atomic_replay_live_boundary_delivers_each_event_once() {
        let mut registry = EvalLogRegistry::default();
        let token = registry.bind(owner(1));
        registry.publish(token, "before".into());
        let (mut rx, replay) = registry.subscribe(token.owner).unwrap();
        registry.publish(token, "after".into());
        assert_eq!(replay.into_iter().collect::<Vec<_>>(), ["before"]);
        assert_eq!(rx.try_recv().unwrap(), "after");
        assert!(matches!(
            rx.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        // A same-commit, same-number foreign UUID is still a foreign owner.
        assert!(registry.subscribe(owner(1)).is_none());
    }

    #[test]
    fn retired_generation_and_other_state_cannot_publish() {
        let owner = owner(1);
        let mut first = EvalLogRegistry::default();
        let mut second = EvalLogRegistry::default();
        let retired = first.bind(owner);
        first.remove(retired);
        let current = first.bind(owner);
        assert_ne!(current, retired);
        first.publish(retired, "retired".into());
        first.remove(retired);
        let other_state = second.bind(owner);
        assert_ne!(other_state, current);
        second.publish(current, "cross-state".into());
        assert!(first.subscribe(owner).unwrap().1.is_empty());
        assert!(second.subscribe(owner).unwrap().1.is_empty());
        first.publish(current, "first".into());
        second.publish(other_state, "second".into());
        assert_eq!(first.subscribe(owner).unwrap().1[0], "first");
        assert_eq!(second.subscribe(owner).unwrap().1[0], "second");
    }

    #[test]
    fn replay_and_channel_capacity_are_bounded_without_retargeting() {
        let mut registry = EvalLogRegistry::default();
        let token = registry.bind(owner(1));
        for n in 0..(super::super::EVAL_LOG_HISTORY_BUFFER + 3) {
            registry.publish(token, n.to_string());
        }
        let (_, replay) = registry.subscribe(token.owner).unwrap();
        assert_eq!(replay.len(), super::super::EVAL_LOG_HISTORY_BUFFER);
        assert_eq!(replay[0], "3");
        for commit_id in 1..=super::super::MAX_EVAL_LOG_CHANNELS as i32 {
            registry.bind(EvalLogOwner {
                commit_id,
                ..owner(1)
            });
        }
        assert_eq!(registry.entries.len(), super::super::MAX_EVAL_LOG_CHANNELS);
        let detached = registry.bind(EvalLogOwner {
            commit_id: i32::MAX,
            ..owner(1)
        });
        registry.publish(detached, "must-not-create".into());
        assert!(registry.subscribe(detached.owner).is_none());
        assert_eq!(registry.entries.len(), super::super::MAX_EVAL_LOG_CHANNELS);
        // An existing commit can rotate even at capacity; other owners cannot
        // retain its sender or use cleanup to remove the new generation.
        let old = registry.entries.get(&42).unwrap().token;
        let replacement = registry.bind(EvalLogOwner {
            commit_id: 42,
            ..owner(2)
        });
        registry.remove(old);
        assert!(registry.subscribe(replacement.owner).is_some());
    }
}
