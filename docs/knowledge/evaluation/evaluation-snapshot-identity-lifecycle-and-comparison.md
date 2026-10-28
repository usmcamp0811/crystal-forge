---
type: Design Specification
title: "Evaluation snapshot identity, comparison, and lifecycle"
description: "Specifies full-SHA snapshot identity, first-parent resolution and Changed comparison rules, and the queued/running/failed/available/unavailable lifecycle for Config snapshot reads and the targeted Config inspection mutation."
tags:
  - crystal-forge
  - evaluation
  - snapshot
  - identity
  - lifecycle
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/evaluation-flake-snapshots.md at commit 3b23d36f"
    title: "Evaluation and Flake Snapshot Architecture"
---

# Evaluation snapshot identity, comparison, and lifecycle

## Identity and Comparison

Persisted snapshots, API lookups, cache keys, comparisons, and URL state use the
complete immutable SHA-1 or SHA-256 commit identity. A seven-character SHA is
presentation only and MUST NOT identify a row or revision.

Git synchronization records the complete first-parent SHA and a separate
`first_parent_resolved` flag. These values have distinct meanings:

- `first_parent_resolved = true` with a parent SHA means Git identified the
  first parent.
- `first_parent_resolved = true` without a parent SHA means the commit is a
  root commit.
- `first_parent_resolved = false` means ancestry is unknown.

Commit-mode Changed results compare the selected configuration snapshot with
the same configuration at the Git first parent. Generation-mode Changed
results compare the selected retained generation with the highest lower
retained generation that has an available snapshot. The server does not infer
another ancestor or compare by timestamp. A root, unresolved parent, missing
parent snapshot, or missing preceding retained snapshot produces no comparison.
It MUST NOT produce a zero-change result.

Flake output deltas use the same Git first-parent rule. They compare declared
systems, exported module names, and resolved lock input revisions.

## Lifecycle

Config snapshot reads use these states:

| State | Meaning |
| --- | --- |
| `queued` | An exact Config Inspector target is waiting for the inspection worker. |
| `running` | The Config Inspector worker owns the exact target. |
| `failed` | Targeted inspection ended and a redacted diagnostic is available. |
| `available` | A schema-valid complete or explicitly partial persisted snapshot can be read. |
| `unavailable` | No reusable snapshot exists, or persisted content is missing, corrupt, incompatible, or over a storage/response bound. |

Commit-mode Config reads derive `queued` and `running` from the exact active
Config Inspector job when no integrity-valid reusable V2 snapshot exists. An
active job does not become reusable when its derivation ID or carrier path
differs from the newly resolved target. The targeted mutation returns the
retryable `409 config_inspection_target_conflict` response and does not mutate
that job. Corrupt content and an unsupported snapshot schema degrade to
`unavailable`. An unreadable non-root option prefix produces an explicitly
partial available artifact when healthy options remain. An unreadable
option-tree root produces `unavailable` because no meaningful inventory exists.

Primary evaluation fallback and flake-output reads have a separate lifecycle.
Their `queued`, `running`, and `failed` states come from the commit evaluation
attempt, not from a Config Inspector job. An active primary attempt overrides a
failed or corrupt primary artifact left by an earlier attempt. These states do
not imply that configuration-scoped V2 inspection is queued or running.

The Config UI calls only the targeted Config inspection mutation. This Admin
mutation applies system and environment authorization before revision
disclosure. In one transaction, it resolves the exact immutable commit,
effective configuration, completed NixOS derivation, and non-empty carrier
`.drv` path. It reuses exactly matching queued or running work, retries after
terminal history, and suppresses work for an integrity-valid available V2
artifact only when the carrier matches. This includes a reusable partial
artifact. Enqueue takes the snapshot-writer
transaction lock before target row locks or readiness checks, so same-carrier
publication and enqueue cannot both commit a redundant job. A missing carrier
returns `409 config_inspection_prerequisite`
without changing primary commit state, creating an evaluation attempt, sending
an evaluator or build wakeup, or invoking Nix.

The separate whole-commit evaluation mutation remains an explicitly named
prerequisite option for callers that choose to request primary evaluation. It
requires
administrator authority because the primary evaluator processes a complete
commit and can cross configuration and environment boundaries. A completed
primary attempt produces a carrier only when that evaluation discovers and
persists the exact successful NixOS target. The generic route does not guarantee
carrier reconstruction. After applicable primary success, automatic scheduling
and the targeted mutation use the same exact Config Inspector enqueue state
machine. The separate worker claims that job,
persists the V2 artifact and selector atomically, and records a redacted
terminal failure when inspection cannot produce a reusable artifact.

## Related concepts

* [Evaluation and Flake Snapshot Architecture](evaluation-flake-snapshot-architecture.md) - Defines ownership and data flow for revision-specific evaluation and flake-output snapshots: what PRIMARY and the Config Inspector worker own, immutable artifacts versus the mutable selector, deployment binding, and database-only reads.
* [Evaluation snapshot persistence, bounds, reclamation, and redaction](evaluation-snapshot-persistence-bounds-and-redaction.md) - Describes content-addressed snapshot persistence, advisory-lock ordering, hard size bounds, the Stage 2 indexed membership cost, orphan reclamation, and the safe-value and redaction policy applied before persistence.
* [Evaluation snapshot API and URL state](../api/evaluation-snapshot-api-and-url-state.md) - Describes server-side option search, filter, pagination and snapshot tokens (409 snapshot_changed), flake output paging, module declaration and module-source endpoints, summary field meanings, tracked provenance, and System Detail URL state.
