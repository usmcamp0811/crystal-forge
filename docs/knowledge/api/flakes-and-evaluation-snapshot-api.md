---
type: API
title: "Flakes API and Evaluation and Flake Snapshot API"
description: "Specifies the flakes endpoints and the database-only snapshot reads for evaluated options, evaluation summary, module sources, generations, rollback, Config inspections, and flake revision outputs, including tokens, pagination bounds, and 409 snapshot_changed behavior."
tags:
  - crystal-forge
  - api
  - flakes
  - evaluation
  - snapshots
  - config-inspector
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/02-backend-api.md at commit 3b23d36f"
    title: "Backend API Specification"
---

# Flakes API and Evaluation and Flake Snapshot API

## Flakes API

Flakes are git repositories that contain NixOS configurations.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/flakes` | Viewer+ | List registered flakes |
| POST | `/flakes` | Operator+ | Add flake to registry |
| GET | `/flakes/:id` | Viewer+ | Get flake details |
| PATCH | `/flakes/:id` | Operator+ | Update flake |
| DELETE | `/flakes/:id` | Operator+ | Remove from registry |
| POST | `/flakes/:id/sync` | Operator+ | Trigger git sync |
| GET | `/flakes/:id/commits` | Viewer+ | Get commit timeline |
| GET | `/flakes/:id/revisions/:revision/outputs` | Viewer+ | Read cached revision outputs |
| GET | `/flakes/:id/revisions/:revision/modules/:module/declarations` | Viewer+ | Read cached exported-module declarations |

### Example: Get Commit Timeline

**Request:**
```bash
GET /api/v1/flakes/flake-456/commits
```

**Response:**
```json
{
  "data": [
    {
      "sha": "abc1234def5678",
      "sha_short": "abc1234",
      "message": "Update nginx config",
      "author": "john@example.com",
      "date": "2024-01-15T10:00:00Z",
      "changed_files": 2
    }
  ]
}
```

## Evaluation and Flake Snapshot API

These endpoints read persisted snapshots only. GET requests do not evaluate
Nix, inspect Git, fetch repositories, enqueue work, or perform per-host work.
All `revision` values are complete 40- or 64-character hexadecimal SHAs.

### GET `/systems/:id/evaluated-options`

Query parameters:

| Parameter | Contract |
| --- | --- |
| `revision` | Required full SHA in commit mode. |
| `mode` | `commit` or `generation`; defaults to `commit`. |
| `generation` | Required retained generation number in generation mode. |
| `search` | Case-insensitive redacted search text; truncated to 256 characters. |
| `filter` | `all`, `overridden`, or `changed`. |
| `limit` | Clamped to 1-100; defaults to 50. |
| `offset` | Clamped to 0-100,000; defaults to 0. |
| `snapshot_token` | Optional on offset 0; required and a 64-character hexadecimal digest when `offset` is greater than 0. |

The response lifecycle is `queued`, `running`, `failed`, `available`, or
`unavailable`. `counts` is revision-global and independent of search/filter.
Every response includes `option_inventory_state`, which is `complete`,
`partial`, or `unavailable`, and bounded `option_inventory_diagnostics`. Each
partial diagnostic contains redacted `path_components`, a stable `code`, and a
redacted `message`. The server canonicalizes and deduplicates path components
after redaction. `option_inventory_diagnostics_truncated` is true when the
128-entry bound or redaction collisions omit diagnostic detail. Traversal
continues after the detail budget is full. A partial available response contains
only options observed outside unreadable prefixes. Its counts, total, and module
totals describe that observed corpus.
Commit mode selects only schema-V2 Config Inspector artifacts through
`config_snapshot_selections`. It does not fall back to a schema-V1 commit
artifact. Generation mode retains schema-V1 selection through the exact retained
generation identity.
Generation-mode Config validity is independent of rollback lineage. A complete
pre-0248 retained artifact remains readable after migration even though its
unverified deployment/store lineage makes rollback ineligible.
`total` is the number of rows for the active search/filter. Changed data and
`counts.changed` are absent when the selected inventory is partial or when no
valid first-parent or preceding retained generation snapshot exists. A
`changed` filter over a partial inventory returns no rows. Drift and other
selected-versus-baseline facts are unavailable for a partial inventory.
`module_count` is the exact count of distinct
`(source_input, source_revision, source_path)` tuples after redaction and
per-option bounding; it is not derived from the bounded option page.
An available response includes an opaque `snapshot_token`. In commit mode, the
token binds the selected and first-parent V2 artifacts, first-parent state, and
the selected and first-parent flake-output digests used for tracked provenance.
It also binds inventory completeness, retained diagnostics, and the certified
truncation state.
In generation mode, the token binds the exact selected artifact, retained
identity, and comparison baseline identity. Generation responses also return
`baseline_generation` when comparison is available. Continuations
send page one's token. A replaced selected artifact, replaced baseline, or
removed retained identity returns HTTP 409 `snapshot_changed`; counts, total,
rows, baseline, and provenance are read from one read-only `REPEATABLE READ`
transaction.
A request revalidates the system-local selected generation or exact commit and
selects its mode-specific first-parent V2 or nearest preceding usable-generation
V1 baseline inside
that transaction. It requires the immutable integrity marker computed by
recursive full-artifact validation before publication, then decodes only the
bounded page. Malformed content outside the requested search, offset, or limit
prevents certification and returns lifecycle `unavailable`, zero counts and
total, no token, and no rows. The response limit remains 100 rows.
The scalar safe-value variant accepts JSON strings, numbers, Booleans, and null.
It rejects arrays and objects; collections require their declared structured
variant.
A supplied token also returns `snapshot_changed` when the replacement is
failed, unavailable, or absent. The endpoint does not return replacement
lifecycle data before it rejects the stale token.

### GET `/systems/:id/evaluation-summary`

This endpoint uses the same mode-specific `revision`, `mode`, and `generation` selection and
non-disclosing system authorization as evaluated-options. The response is
scalar. It does not contain module-source or definition rows.
Unverified retained generation lineage does not affect a valid Config summary;
it affects rollback eligibility only.
The optional `snapshot_token` query parameter binds the summary to an artifact
selected by another Config response. An available response returns the same
token. The token also binds the exact comparison baseline. Generation responses
return `baseline_generation` when comparison is available. A stale token or
replaced selected/baseline identity returns HTTP 409 `snapshot_changed`.
A supplied stale token takes precedence over failed, unavailable, or absent
replacement lifecycle responses.
Snapshot integrity, derivation facts, latest state, and
seven-day observations use one read-only `REPEATABLE READ` transaction.

The response returns lifecycle, safe error, persisted completion time,
evaluation duration, option total, `module_source_total`, exact selected NixOS
toplevel store path, existing closure package count, exact latest running store
path, agent-reported profile match, and drift. `module_source_total` is the exact
count of distinct `(source_input, source_revision, source_path)` tuples after
redaction and per-option bounding. Response-only tracked identities do not
affect the count. Drift is `matches` only when selected and running store paths
are exactly equal, `differs` only when both paths exist and differ, and
`unavailable` otherwise. A partial option inventory always reports drift and
comparison-derived summary facts as unavailable. Scalar facts that do not
require a complete inventory remain available.

In generation mode, `host_delta_count` is materialized from the schema-V1 usable
configuration snapshots at the selected commit. For each option path, the server
selects the most frequent complete safe content digest, including definition
provenance; missing is also a state, and bytewise state identity breaks ties. The
count is the selected snapshot's differences from that modal corpus. A usable
one-configuration corpus returns zero. Commit-mode V2 snapshots remain outside
that corpus and return null. Null otherwise means no usable materialized result
exists.

`closure_size_bytes` is the sum of `narSize` for every unique store path from
one successful complete recursive Nix query of the selected toplevel output.
Null means no complete local measurement was persisted. The server does not
substitute derivation size, snapshot size, or a partial query.

`agent_fingerprint` compares the exact selected and latest agent-reported store
paths. It is `matches`, `differs`, or `unavailable` when either path is absent.
`seven_day_drift` is `no_observed_drift` or `observed_drift` only when persisted
state and heartbeat observations span the full trailing seven days, every
boundary or adjacent gap is at most four hours, and all observations have an
exact store path. The observation before the window establishes coverage but
does not contribute drift. Otherwise it is `insufficient_coverage`.

Completion time, duration, selected and running paths, closure counts, profile
match, and other optional facts are null when their named persisted source is
absent. A non-available lifecycle returns no summary facts and zero totals.
Clients MUST render unavailable states. They MUST NOT infer one metric from
another field or replace null, unavailable, failed, or insufficient coverage
with zero or success.

### GET `/systems/:id/evaluation-module-sources`

This endpoint uses the same selected-revision and non-disclosure contract as
evaluated-options.

| Parameter | Contract |
| --- | --- |
| `revision` | Required full SHA in commit mode. |
| `mode` | `commit` or `generation`; defaults to `commit`. |
| `generation` | Required retained generation number in generation mode. |
| `limit` | Clamped to 1-100; defaults to 50. |
| `offset` | Clamped to 0-100,000; defaults to 0. |
| `snapshot_token` | Optional on offset 0; required and a 64-character hexadecimal digest when `offset` is greater than 0. |

The response lifecycle is `queued`, `running`, `failed`, `available`, or
`unavailable`. Non-available responses contain no token or rows and return a
zero total. An available response returns one bounded page and a
snapshot-version token. `total` is the exact complete-snapshot tuple count even
when `sources` is empty because the offset is past the final row.
Unverified retained generation lineage does not affect module-source reads from
a valid artifact; it affects rollback eligibility only.

Rows are ordered by `won_count` descending, `defined_count` descending, then
`source_input`, `source_revision`, and `source_path` in ascending bytewise
order. Null input and revision values sort last. Each row contains the exact
tuple, snapshot-wide counts for that tuple, and optional server-issued
`tracked_flake` identity.

The first request omits `snapshot_token`. Every continuation request sends the
token from the first page. If the persisted snapshot is replaced, the endpoint
returns HTTP 409 with `snapshot_changed` and no rows. The client discards all
loaded rows and restarts at offset 0.

The module-source token uses the same selected-and-baseline identity as the
options and summary endpoints. A baseline replacement therefore also returns
HTTP 409 instead of mixing Config data from different comparisons.
Failed, unavailable, and absent replacements use the same precedence when the
request supplies a stale token.

### GET `/systems/:id/generations`

Each generation row includes `generation_snapshot_id` and `rollback_eligible`.
Eligibility is true only when the retained row resolves an available immutable
artifact, exact derivation lineage, and non-empty server-side source store path.
Legacy retained rows with unverifiable deployment/store lineage remain
queryable but are not rollback-eligible.
Clients MUST NOT advertise rollback for an ineligible row.

### POST `/systems/:id/rollback-generation`

The request MUST contain `generation_snapshot_id` or the system-local
`generation`. `store_path` is optional and, when present, only narrows the
retained lookup. A store path alone never authorizes rollback. The server carries
the retained derivation identity into
composite authorization; a newer derivation with the same store path cannot
replace it. Foreign retained
identities, failed artifacts, and mismatched artifact/derivation lineage fail
closed.

`tracked_flake` is response-only and is never persisted in evaluator content.
For `self`, the source revision must equal the page's exact active context
revision. For an external input, the context revision's persisted lock snapshot
must match the exact input name, repository URL, and full locked revision. The
identity is returned only when this mapping resolves unambiguously to one
non-deleted registered flake and non-archived commit visible through an active
managed system. Hidden, stale, unmatched, deleted, archived, and ambiguous
identities remain absent. Repository URLs are sanitized before serialization.

The same response-only resolver decorates every selected and baseline
definition returned by `/evaluated-options`, using the selected or baseline
revision as that definition's context. The browser independently loads summary,
module-source, and option pages. It MUST NOT infer identities or derive a
snapshot-wide module count from a bounded page.

This GET is database-only. It does not evaluate Nix, inspect Git, fetch a
repository, enqueue work, mutate snapshot state, or perform per-host work.

### POST `/systems/:id/config-inspections/:revision`

This mutation requires administrator authority and matching CSRF credentials.
Authorization and environment visibility checks occur before revision
validation or resolution. The server atomically resolves the system's exact
active flake commit, effective configuration name, completed NixOS derivation,
and non-empty carrier `.drv` path. It then queues or reuses only the exact
Config Inspector target. A queued or running job is reused, and terminal history
permits a retry. An available complete V2 artifact suppresses work only when it
is comparison-ready. A certified partial V2 artifact also suppresses work
because retrying cannot make its observed corpus more complete without a source
change. A complete artifact with unavailable global provenance or Stage 2 does
not suppress a retry. In both reusable states, the carrier path must match
exactly. The enqueue decision acquires the
snapshot-writer transaction lock before target row locks and readiness checks.
If active work has a different derivation ID or carrier path, the endpoint
returns retryable HTTP 409 with `error: config_inspection_target_conflict` and
does not mutate that work.

If the exact completed carrier is absent, the endpoint returns HTTP 409 with
`error: config_inspection_prerequisite`. This response does not queue primary
evaluation. The endpoint does not change commit evaluation status or attempts,
notify primary evaluator or build queues, invoke Nix, or inspect another
configuration. Unknown systems and revisions outside the system's flake return
the same non-disclosing not-found response.

### POST `/systems/:id/evaluations/:revision` (explicit prerequisite)

This mutation requires administrator authority because the evaluator processes
the complete commit, and it requires matching CSRF credentials. It queues a
missing terminal evaluation or reuses
available, queued, or running work. The `queued` response field is true only
when this request performed the queue transition. The System Config UI does not
call this route. A caller uses it only as an explicitly named whole-commit
prerequisite when the targeted Config inspection route reports a missing
carrier. Completion does not guarantee carrier reconstruction: the primary
evaluator must discover and persist the exact successful NixOS target.

### GET `/flakes/:id/revisions/:revision/outputs`

Query parameters:

| Parameter | Contract |
| --- | --- |
| `system_filter` | `all`, `declared_unmanaged`, or `managed_undeclared`; defaults to `all`. |
| `limit` | Clamped to 1-100; applies independently to each top-level collection and to filtered reconciliation. |
| `offset` | Clamped to 0-100,000; applies independently to each top-level collection and to filtered reconciliation. |
| `snapshot_token` | Optional opaque token returned by the endpoint. When supplied, it binds the request to the selected output and usable first-parent comparison state. |

The server applies `system_filter` before the reconciliation offset and limit.
`pagination.system_total` is the visible total for the active filter, and
`pagination.systems_has_more` reports whether that filtered sequence has a next
row. The aggregate reconciliation counts, collapse count, pinned count, and
stale-input count remain revision-global. Clients request continuation pages
and retain these authoritative totals. A response larger than the 2 MiB safe
response bound is `unavailable` rather than silently truncated.

Token-aware clients send the first page's `snapshot_token` on continuation
requests. The server returns `409 snapshot_changed` if a supplied token is
stale or malformed because the selected output, first-parent identity or state,
or usable first-parent output changed. The client then discards accumulated
rows and restarts at offset 0. For compatibility with existing clients, a
positive offset without `snapshot_token` retains the prior bounded offset
semantics and does not receive this replacement guarantee. HTTP 409 applies
only when the request supplied a stale or malformed token.

`managed_system_count` is the authoritative visible active fleet count. It can
exceed the bounded `systems` array. Non-admin responses remove hidden systems,
configuration names, and module consumers. A caller without a visible active
managed system for the flake receives not-found.

Exported-module entries in this response are summaries. `declaration_count`
remains authoritative. `declarations` is empty, and `declarations_complete` is
false when declaration details exist. Clients use the dedicated declaration
endpoint instead of treating the summary as a complete nested collection.

An exported module's `source_input`, `source_revision`, and `source_path`
describe only the location of its `nixosModules` attribute binding. The
evaluator uses the Nix attribute position and requires one unambiguous longest
matching input root; `source_path` is relative to that root. Missing positions
and ambiguous roots produce null. These fields are not module value provenance
and do not authorize navigation. Declaration `source_paths` are the declaration
locations.

Input rows expose `direct_descendant_count` for immediate lock-graph children
and `transitive_descendant_count` for all unique recursive descendants of a
direct root input. Both counts use the complete lock graph, not the response
page. They are null for non-direct nodes or unavailable counts. Clients that
describe transitive reach MUST use `transitive_descendant_count`.

### GET `/flakes/:id/revisions/:revision/modules/:module/declarations`

This endpoint returns declarations for one exact exported module from one
persisted flake-output JSONB snapshot. `limit` is clamped to 1-100 and `offset`
to 0-100,000. The response contains the authoritative `total`, applied
`offset` and `limit`, deterministic declaration rows, explicit snapshot
`lifecycle` and safe `error`, and a content-digest `snapshot_token`.

The first request omits `snapshot_token`. Every continuation request sends the
token returned by page one. If re-evaluation replaces the selected snapshot,
the endpoint returns `409 snapshot_changed`. The client must discard loaded
rows and restart at offset 0. Unknown active revisions and module names return
not-found. Unauthorized or hidden flakes use the same non-disclosing behavior
as the top-level output endpoint. The query is database-only and does not
mutate evaluation or snapshot state.

See [Evaluation and Flake Snapshot
Architecture](../evaluation-flake-snapshots.md) for extraction ownership,
identity, comparison, persistence, retention, redaction, and verification
requirements.

## Related concepts

- [Systems API](systems-api.md)
- [Backend API overview, error codes, and WebSocket streaming](api-overview-errors-and-streaming.md)
- [API authentication, sessions, and role-based authorization](../security/api-authentication-and-authorization.md)
