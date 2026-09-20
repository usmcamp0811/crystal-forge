# Sidebar badges vs. the notification bell

Crystal Forge surfaces "something needs your attention" in two places that look similar
but answer different questions. This is the decision on what each is for, so they don't
drift back into overlapping/duplicating each other.

## Sidebar badges — "is this section OK right now?"

A live **rollup of current, unresolved state**, scoped to one nav section. Recomputed
from the underlying data on every render — it's a mirror, not a log.

- **Systems**: systems with critical/offline health, + pending deploy approvals, +
  unresolved attestation attention items (unauthorized/unknown artifact, invalid identity).
- **Flakes**: flakes currently failing to sync.
- **Environments**: environments containing at least one critical/offline system.
- **Evaluations / Builds**: failed evals/builds in the last 24h.
- **CVEs**: open critical CVEs across the fleet.

Rules:
- The badge **disappears the instant the count reaches zero** — no action required, no memory.
- It also disappears once the operator **visits that section** (acknowledged), even if the
  underlying issue is still technically open, so it doesn't nag on every return visit.
- No history and no per-item dismissal — you can't "mark one system read" independently
  of the others. It's a gauge, not an inbox.
- Every badge has a tooltip spelling out exactly what it's counting.

Use a sidebar badge for: **standing conditions** that are true about a section's data
until someone fixes them (failing pipeline, unhealthy system, sync error).

## Notification bell — "what happened, and did I deal with it?"

A **chronological event log**. Each entry is a discrete thing that occurred at a point
in time — not a live count — with its own read/unread state.

Current sources:
- Deploys newly awaiting approval (policy-gated).
- Unauthorized/unknown/invalid-identity attestation findings.
- Build failures, new critical CVEs, lost heartbeats, completed evaluations.

Rules:
- Entries are **timestamped** and persist until the operator reads/dismisses
  them, regardless of whether the underlying condition later resolves itself.
  The feed can remove an entry when the current account is no longer
  authorized to see its subject.
- Multiple entries can exist for the same underlying issue over time (e.g. a build that
  fails twice logs twice) — the bell is additive, the sidebar badge is not.
- Clicking an entry routes to the relevant view/system.
- The production bell uses the durable, per-user notification API. It keeps
  read and dismissal mutations distinct from resolution of the underlying
  attention occurrence.
- A head refresh captures the oldest displayed `(created_at, id)` key. The
  client fetches bounded pages from the new head until it reaches or passes
  that key, or the server is exhausted. The fetched range then replaces the
  prior loaded range. No unvalidated suffix or old cursor is retained.
- Reconciliation has a fixed page bound. If the client reaches the bound
  before the old key, it adopts the bounded fetched range and its cursor. The
  client tells the user that older loaded history was reset.
- A successful head or range reconciliation replaces the pagination cursor and
  clears every prior append failure. Retry never sends a cursor from the
  replaced range.
- A successful mutation invalidates an in-flight feed response as a whole.
  The client discards both its rows and unread count and queues one fresh head
  request.
- The client permits one in-flight mutation for each durable operation identity:
  `read(id)`, `dismiss(id)`, or `mark_all`. It disables the matching control
  until the current request succeeds, fails, or loses account ownership.
- Dismissal restores focus only when the current account coordinator applies
  the response and the same account still owns the open notification panel.
- The Shell fixture is the visual authority. Production adapts that design to
  the durable, per-user API. The Web UI integration scenarios use an API mock
  only to exercise loading, pagination, failure, and delayed-response states.
  The fixture and mock do not define durable API or event-materialization
  behavior.

Use a bell entry for: **things that happened** that the operator should know about even
after the fact, and that benefit from an explicit "I've seen this."

## Rule of thumb when adding a new signal

Ask: *"Does this describe a standing condition of the fleet right now, or a thing that
just happened?"*
- Standing condition (would still be true if no one looked at it in a week) → sidebar badge.
- Discrete event (has a moment it occurred, worth logging even after it's resolved) → bell.

Some conditions warrant both — e.g. a pending deploy approval is a standing condition
(Systems badge) *and* worth logging as an event when it was first requested (bell).
