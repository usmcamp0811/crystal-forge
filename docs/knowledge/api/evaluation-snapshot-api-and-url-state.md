---
type: API
title: "Evaluation snapshot API and URL state"
description: "Describes server-side option search, filter, pagination and snapshot tokens (409 snapshot_changed), flake output paging, module declaration and module-source endpoints, summary field meanings, tracked provenance, and System Detail URL state."
tags:
  - crystal-forge
  - evaluation
  - api
  - pagination
  - url-state
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/evaluation-flake-snapshots.md at commit 3b23d36f"
    title: "Evaluation and Flake Snapshot Architecture"
---

# Evaluation snapshot API and URL state

## API and URL State

The endpoint contract and bounds are listed in the
[backend API specification](flakes-and-evaluation-snapshot-api.md#evaluation-and-flake-snapshot-api).
Option search, filter, counts, comparison, and pagination are server-side.
Option pages clamp `limit` to 1-100, `offset` to 0-100,000, and search to 256
characters. Counts are revision-global; `total` reflects the active search and
filter. Generation mode selects immutable schema-V1 artifacts through retained
generation identity. Commit mode selects only schema-V2 artifacts through
`config_snapshot_selections`; it never falls back to the V1 commit selector.
Schema-V2 responses classify the inventory as `complete`, `partial`, or
`unavailable`. Partial responses expose bounded redacted diagnostics and the
healthy observed option and module rows. Their counts and totals cover only the
observed corpus. They never claim a complete option count.
Evaluated-options, module-source, and summary responses return the same opaque
token. The generation token binds the selected artifact, selected retained
identity, exact comparison artifact, and comparison retained identity. The
commit token binds the selected and first-parent V2 artifacts, first-parent
state, and selected and first-parent flake-output digests used for tracked
provenance. A positive option or
module-source offset requires the page-one token. Summary requests can supply
the token to bind independently loaded Config cards to the same artifact.
Replaced current artifacts return HTTP 409 `snapshot_changed` without rows or
summary data. When a request supplies a token, a failed, unavailable, or absent
replacement also returns `snapshot_changed` before lifecycle or no-selection
data. A generation-mode options or summary response exposes
`baseline_generation` when comparison is available. Integrity, counts, totals,
rows, baseline, provenance, and summary state are read in read-only
`REPEATABLE READ` transactions.

Comparison requires a complete selected inventory. A partial inventory has no
Changed count or Changed rows and cannot produce selected-versus-baseline drift.
Search and the All and Overridden filters remain available over observed rows.
Stage 2 excludes every unreadable Stage-1 prefix before provenance replay, so it
does not schedule definition-value jobs for identities that Stage 1 could not
observe.

Flake output pages apply one clamped 1-100 `limit` and 0-100,000 `offset` to
each top-level collection and reconciliation page. Clients merge continuation
pages but retain revision-wide authoritative totals. Exported-module rows are
summaries: they retain `declaration_count`, return an empty `declarations`
array, and set `declarations_complete` to false when declarations exist.
Page responses also return an opaque `snapshot_token` bound to both the
selected output and usable first-parent digest and state. Token-aware clients
send it on continuations. A supplied stale or malformed token returns HTTP 409
instead of mixing selected or comparison pages. Tokenless positive offsets
retain the endpoint's prior bounded compatibility semantics and do not receive
replacement detection. HTTP 409 applies only when a token was supplied.

The module declaration endpoint selects one exact module from one persisted
JSONB snapshot. It returns an authoritative total and a deterministic page
ordered by option path, declared type, canonical declaration content, and
persisted array position. `limit` is clamped to 1-100 and `offset` to
0-100,000. Page one returns the complete flake-output content digest as
`snapshot_token`. A continuation request MUST send that token. If
re-evaluation replaces the snapshot, the server returns conflict instead of
mixing pages. The endpoint performs one bounded SQL statement and does not
mutate snapshot or evaluation state.

System Detail URL state stores the exact system route, `tab`, `config_mode`,
full `revision` or retained `generation`, and optional `deploy_generation`.

The selected-evaluation summary is a scalar database-only projection. It joins
only existing snapshot, derivation, retained-generation, latest system-state,
and persisted observation facts. `module_source_total` is the snapshot's exact
distinct tuple count; the summary does not transfer module rows.

Summary fields have these authoritative meanings:

- In generation mode, `host_delta_count` counts option paths whose complete safe content digest
  differs from the deterministic modal state across usable configuration
  snapshots at the same commit. Missing options participate as a state. Ties
  use bytewise state identity. Definition-provenance changes affect the digest.
  A usable one-configuration corpus has a zero delta. Commit-mode V2 artifacts
  are outside this schema-V1 host corpus and always return null. Null otherwise
  means that no usable materialized result exists.
- `closure_size_bytes` is the sum of `narSize` for each unique store path from
  one successful complete recursive Nix query of the selected toplevel output.
  Null means that no complete local measurement was persisted. The server does
  not substitute derivation size, snapshot size, or a partial query.
- `agent_fingerprint` is `matches` or `differs` from exact equality of the
  selected and latest agent-reported store paths. It is `unavailable` when
  either path is absent. `drift` applies the same exact-store identity rule to
  the selected and running configuration fields.
- `seven_day_drift` is `no_observed_drift` or `observed_drift` only when exact
  persisted state and heartbeat observations cover the full trailing seven
  days, every boundary or adjacent gap is at most four hours, and every
  observation has a store path. The observation before the window establishes
  coverage but does not contribute drift. Any failed condition produces
  `insufficient_coverage`, not a no-drift result.

Other optional summary facts are null when their named persisted source does
not exist. Non-available lifecycle responses contain no summary facts and use
zero totals. The server and UI display unavailable states; they do not infer a
metric from another field or replace unknown data with zero.

The module-source endpoint groups the persisted observed definition corpus by
the same exact tuple. For a complete inventory this is the complete definition
corpus. For a partial inventory it excludes unreadable option prefixes.
`won_count` counts distinct options with at least one
surviving definition from the tuple; multiple surviving definitions for one
option count once. It returns bounded pages ordered by winning option count
descending, definition count descending, then input, revision, and path in
ascending bytewise order; null input and revision values sort last. `total` is
the complete snapshot-wide tuple count even when the requested page is empty.
The first page returns an opaque `snapshot_token`. Every request with a positive
offset MUST send that token. Snapshot replacement returns HTTP 409
`snapshot_changed`; the client discards accumulated rows and starts again at
offset 0. All lifecycle responses use `queued`, `running`, `failed`,
`available`, or `unavailable`. Non-available responses contain no token or rows
and have a zero total.

Tracked provenance is a response-only server projection for bounded module rows
and for every selected and baseline definition in a bounded option page. It is
never persisted in evaluator payloads. `self` requires the source revision to
equal the page's exact active context revision. An external input must map
through that context revision's flake-output lock snapshot to an exact input
name, repository URL, and full locked revision. The result must resolve to
exactly one non-deleted registered flake and non-archived commit visible to the
caller through an active managed system. Deleted, archived, hidden, stale,
unmatched, and ambiguous identities remain untracked. Repository URLs are
sanitized before serialization.

The UI loads scalar summary, module-source pages, and option pages independently
with selection-specific stale-response protection. It retains authoritative
totals while it merges continuation pages. It does not derive snapshot-wide
module totals from a bounded option page or infer navigation identities from
input names, revisions, paths, or repository text.
Flake tray state stores the flake identity, pane, full revision, and optional
return environment. State changes use browser history, popstate restores state,
and closing or unrelated navigation removes stale tray/return context. Unknown
or unavailable revisions render explicit states after authorization; they do
not fall back by abbreviated SHA.

## Related concepts

* [Evaluation and Flake Snapshot Architecture](../evaluation/evaluation-flake-snapshot-architecture.md) - Defines ownership and data flow for revision-specific evaluation and flake-output snapshots: what PRIMARY and the Config Inspector worker own, immutable artifacts versus the mutable selector, deployment binding, and database-only reads.
* [Evaluation snapshot identity, comparison, and lifecycle](../evaluation/evaluation-snapshot-identity-lifecycle-and-comparison.md) - Specifies full-SHA snapshot identity, first-parent resolution and Changed comparison rules, and the queued/running/failed/available/unavailable lifecycle for Config snapshot reads and the targeted Config inspection mutation.
* [Flake outputs, system reconciliation, and count authority](../evaluation/flake-outputs-and-count-authority.md) - Defines the PRIMARY flake-output projection, managed/declared_unmanaged/managed_undeclared reconciliation, which counts are authoritative, Systems and Inputs pane filters, and non-disclosure rules for hidden environments.
* [TASK-440 Config Explorer design audit](../historical/task-440-config-explorer-design-audit.md) - Records the TASK-440 design audit of the Config side column and Flake Modules pane against the reference design, the intentional differences, geometry-sensitive Config pagination, and the screenshot evidence policy.
