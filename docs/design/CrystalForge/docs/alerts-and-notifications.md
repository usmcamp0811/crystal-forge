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
- Entries are **timestamped** and persist until the operator reads/dismisses them,
  regardless of whether the underlying condition later resolves itself.
- Multiple entries can exist for the same underlying issue over time (e.g. a build that
  fails twice logs twice) — the bell is additive, the sidebar badge is not.
- Clicking an entry routes to the relevant view/system.

Use a bell entry for: **things that happened** that the operator should know about even
after the fact, and that benefit from an explicit "I've seen this."

## Rule of thumb when adding a new signal

Ask: *"Does this describe a standing condition of the fleet right now, or a thing that
just happened?"*
- Standing condition (would still be true if no one looked at it in a week) → sidebar badge.
- Discrete event (has a moment it occurred, worth logging even after it's resolved) → bell.

Some conditions warrant both — e.g. a pending deploy approval is a standing condition
(Systems badge) *and* worth logging as an event when it was first requested (bell).
