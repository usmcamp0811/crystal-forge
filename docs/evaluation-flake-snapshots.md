# Evaluation and Flake Snapshot Architecture

## Purpose

Crystal Forge stores reusable, revision-specific read models for the System
Config view and the flake explorer. These read models expose evaluated NixOS
options and flake outputs without running Nix, Git, or network operations from
an HTTP read request.

This document defines the ownership, identity, security, lifecycle, retention,
and API contracts for those snapshots.

## Ownership and Data Flow

The existing authorized commit evaluation path owns extraction. The bulk
`nix-eval-jobs` expression emits two types of metadata:

- One options-tree result for each evaluated `nixosConfiguration`.
- One configuration-independent flake-output carrier for declared systems,
  exported modules, module declarations and consumers, and resolved lock
  inputs. The carrier exists even when the flake has no NixOS configurations.

Evaluation finalization redacts and persists the metadata in the same database
transaction that finalizes the successful evaluation attempt. A failed
configuration stores only a safe failed lifecycle and diagnostic. The server
does not serialize the NixOS `config` tree.

Migration `0248_immutable_evaluation_artifacts.sql` separates immutable attempt
artifacts from the mutable current selector for each exact commit and
configuration. Each success or failure inserts a new artifact and advances the
selector atomically. A retained generation points to the exact successful
artifact and derivation that produced it. A later success or failure does not
rewrite that retained artifact.

Pre-0248 retained metadata remains queryable after upgrade. Migration validates
the complete copied artifact and marks valid content readable. The mutable
legacy schema cannot prove exact deployment/store lineage, so migration marks
these rows unverified. This flag does not affect Config validity or comparison;
it makes the generation ineligible for rollback only.

When a deployment request binds its full commit and exact derivation target, it
also captures the current successful evaluation artifact. Generation retention
uses this captured identity. It does not consult the mutable selector when the
agent reports the generation later. Reciprocal retention considers only
observations timestamped at or after the bound deployment was issued. Commit
rollback uses the resolved commit's artifact. Generation rollback carries the
retained artifact into the new
deployment instead of selecting the current attempt for the commit. If the
deployment, derivation, store path,
configuration, and artifact lineage cannot be matched exactly, retention fails
closed and the generation is not advertised as rollback-eligible.
Migration marks pre-0248 deployments as not expecting an artifact binding.
Only deployments created after migration participate in reciprocal binding.
Unavailable artifacts, including snapshots over the content limit, advance the
current selector but do not bind deployments or create generation retention.
Pending and succeeded deployments can create retention. An expired deployment
can also create retention until 24 hours after its terminal completion. The row
remains `expired`; a delayed successful activation creates a correlated
`cf_deployment_succeeded` event instead of rewriting terminal status. Failed and
superseded deployments cannot create retention or successful-activation
correlation. The server selects the newest same-path deployment issued no later
than the observation before it checks eligibility. It does not skip a newer
failed or superseded request to attach the observation to older work.

Migration `0247_authoritative_snapshot_metrics.sql` performs the historical
host-delta backfill. In normal operation,
the application persists the complete configuration corpus with deferred
per-row recomputation, then recomputes commit-wide host deltas once before the
finalization transaction becomes visible. Replacement and failure paths use
the same transaction-level recomputation contract.

For an available configuration, `module_count` is the exact count of distinct
`(source_input, source_revision, source_path)` tuples in the persisted option
definitions. The server computes this scalar after redaction and per-option
bounding. Response-only tracked identities do not affect it. Existing snapshots
are backfilled with the same tuple semantics.

Snapshot GET handlers and query functions are database-only. They MUST NOT
invoke Nix, inspect Git, fetch a repository, enqueue work, or perform per-host
evaluation. A missing snapshot remains a read result. Only the explicit,
authorized evaluation action can queue or reuse evaluation work.

This design does not add an agent or builder protocol field. Deployed agents
continue to report state and generations through the existing protocol.
Deployed builders continue to use the existing server-issued job authorization
and evaluation path. Snapshot extraction is server-owned evaluator metadata,
not a new database or API responsibility for an API-only builder.

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

Evaluation and flake-output reads use these states:

| State | Meaning |
| --- | --- |
| `queued` | An authorized mutation placed the commit in the existing evaluation queue. |
| `running` | The existing evaluator is processing the commit. |
| `failed` | Evaluation ended and a redacted diagnostic is available. |
| `available` | A complete, schema-valid persisted snapshot can be read. |
| `unavailable` | No reusable snapshot exists, or persisted content is missing, corrupt, incompatible, or over a storage/response bound. |

The read API derives queued and running states from the active commit attempt
when no integrity-valid reusable snapshot exists. This active state overrides a
failed or corrupt snapshot left by an earlier attempt. Corrupt content and an
unsupported snapshot schema otherwise degrade to `unavailable`; the API does
not return a partial corpus as available. The explicit action treats corrupt
content as non-reusable and can therefore queue its reconstruction.

The explicit evaluation action requires administrator authority because the
current evaluator processes a complete commit and can cross configuration and
environment boundaries. It locks the commit and reuses available, queued, or
running work. It queues only a missing terminal evaluation and sends a queue
wakeup only when the database transition occurred. The commit transition and
its new claimable `evaluation_attempts` lineage row commit atomically.

## Persistence, Bounds, and Reclamation

Option content is redacted and canonicalized before the server computes its
SHA-256 digest. `evaluation_option_contents` stores one payload per digest.
Snapshot rows store only option paths and digest references, so identical
content can be shared across option paths, hosts, and revisions. Flake output
payloads use the same content-addressed pattern at revision scope.

Production option persistence uses parameterized set-oriented batches of at
most 500 rows for content and references. It does not issue two SQL statements
for each option. Digest conflicts update no content and fail the transaction;
the selector advances only after every batch succeeds. Snapshot writers, both
deployment-creation paths, generation retention, and artifact/content reclamation
use one transaction advisory lock. Transactions acquire this advisory lock
before POA&M, system, or deployment row locks. Heartbeat/state ingestion uses
the same order. Therefore deployment creation either
binds an already-published Available artifact or commits first and lets snapshot
finalization bind the exact deployment. If state ingestion commits before
deployment creation, deployment binding reciprocally retains the existing
generation observation. Rollback leaves the old selector and all prior artifacts
unchanged.

The current hard bounds are:

- 256 KiB for one encoded option payload. An over-limit value becomes an
  explicit opaque value and loses oversized provenance.
- 64 MiB for one complete configuration snapshot. An over-limit snapshot
  becomes unavailable.
- 8 MiB for one persisted flake-output payload. An over-limit snapshot becomes
  unavailable.
- 2 MiB for one encoded flake-output API response. An over-limit response
  becomes unavailable rather than returning partial unmarked data.
- 16 KiB of searchable text per option after redaction.
- 16 option-tree levels before a non-empty deeper subtree becomes an explicit
  `over_depth` failed value. This guard bounds cyclic or recursively generated
  attribute sets; it does not silently truncate the ordinary option count.

Foreign keys and immutability triggers prevent mutation or direct removal of
artifact content and references. Server startup and the 15-minute maintenance
loop first releases at most 100 terminal deployment rows that have a snapshot or
derivation binding and whose completion is at least 24 hours old. This interval
lets delayed agent state ingestion retain
the deployed artifact. Active deployment work and retained generation rows are
never released by this step. The loop then removes at most 100 artifacts that
are neither current, retained, nor bound to a deployment and deletes unreferenced
option and flake content in batches of 1,000 rows.
Each pass reports binding, artifact, and content-row progress. The maintenance
loop stops only when all four counts are zero, so more than 100 orphan artifacts
drain in successive bounded transactions. The maintenance transaction uses the
same advisory lock as writers. After a terminal deployment's 24-hour ingestion
window, maintenance releases both its snapshot and exact derivation bindings.
It then removes archived derivations and commits in bounded pages only when no
retained generation, live deployment artifact, system target, or durable request
reservation still references them.

## Retention

An observed deployment generation records its system, generation, derivation,
commit, store path, and snapshot identity. Restrictive foreign keys keep that
snapshot, derivation, and commit while the generation reference exists. Nix
store garbage collection does not affect the database snapshot.

Generation rollback accepts a retained generation UUID or system-local
generation number and resolves the exact derivation and store path on the
server. Composite authorization remains constrained to that derivation even if
another commit or duplicate derivation has the same store path. A supplied
legacy store path can only narrow that exact retained lookup. A path by itself, a
foreign system's retained UUID, a failed artifact, or mismatched derivation
lineage does not authorize rollback. Post-migration deployment rows persist the
exact requested derivation ID when the server resolves one. Rollback deployment
creation carries the retained derivation ID unchanged. Ordinary path-only or
legacy deployments can leave this field null, but such a row cannot create
verified generation retention. When multiple same-path deployments exist, an
observation uses the newest deployment issued no later than the observation.
The generation-list response exposes the retained UUID and `rollback_eligible`.
Clients MUST offer rollback only when eligibility is true. `store_path` remains
optional in the rollback request and does not replace retained identity.

Flake timeline snapshots remain attached to their commit records. A branch
rewrite does not reinterpret an old full SHA as a new revision. Rewrite
acceptance archives every old-lineage commit before it removes unretained
history. Restrictive snapshot or generation references can preserve a commit,
but the archived commit is not available through active-revision APIs.
Derivations referenced by retained generations also remain available. A flake
source reset archives commits required by retained generations, exact
deployment-bound derivations, or deployment-bound artifacts and preserves their
derivations. It also archives commits referenced by durable explicit request
reservations and by all deployment rows, including pre-0248 and path-only rows
with no exact artifact or derivation binding. Bounded maintenance later releases
terminal deployment identities after the 24-hour ingestion window. A
derivation-only binding remains authoritative when the evaluation artifact is unavailable.
Source reset removes those
commits from active revision APIs. Generation reads use the retained
identity directly, so the archived snapshot remains queryable without exposing
the old revision as part of the replacement source. Source reset is serialized
with snapshot publication, deployment binding, retention, and reclamation. The
global advisory-lock order is snapshot writer, per-flake sync, then attention.
After the final deployment binding releases, bounded maintenance removes an
otherwise unreferenced archived derivation and commit. A terminal deployment's
commit identity remains protected for the same 24-hour ingestion window. A
durable explicit request reservation protects immutable request intent without a
time limit. Source reset and history rewrite archive every deployment-referenced
commit first. Only bounded maintenance releases an eligible terminal identity;
an `ON DELETE` action never decides eligibility. Other commit snapshots follow
commit timeline retention.

Missing, corrupt, or schema-incompatible content is reported as unavailable.
Migration and successful snapshot finalization recursively validate every
persisted safe-value and provenance variant before setting the artifact's
immutable integrity marker. Option references and content cannot change after
certification. Each Config read checks this marker in the same read-only
`REPEATABLE READ` transaction that selects authoritative first-parent or nearest
preceding usable-generation comparison identity and reads the bounded page.
Scalar variants accept only JSON string, number, Boolean, or null values. Arrays
and objects tagged as scalar are malformed. Malformed content outside the requested page therefore prevents certification
and cannot produce a partially available response. Read cost and response size
remain bounded independently of the complete option corpus.
The server MUST NOT silently re-evaluate during a read to reconstruct it.

## Safe-Value and Redaction Policy

Redaction runs before persistence, content hashing, search indexing, diffing,
logging of evaluator-controlled diagnostics, or API serialization. The policy
covers option values, nested collection and submodule values, package fields,
module defaults, evaluator errors, source metadata, lock metadata, winner
notes, and repository URLs.

The policy replaces all scalar leaves under a sensitive option path. It also
removes nested fields whose normalized names indicate passwords, secrets,
tokens, credentials, private keys, access keys, passphrases, signing keys,
netrc, askpass, API keys, or PATs. Text scanning removes authorization values,
common secret assignments, provider token prefixes, JWT-like values,
high-entropy token-like values, URL user information for any syntactically
valid URL scheme, URL queries and fragments, and SCP-style repository user
information. Redacted names and values are not searchable.

Ordinary strings, booleans, numbers, package metadata, diagnostics, and source
paths remain available when the policy classifies them as safe. Opaque and
failed values remain explicitly typed; the server does not fabricate a value.

Redaction is a deterministic safety boundary, not a general secret-detection
proof. A low-entropy secret under a neutral option path and neutral field name
can evade lexical detection. A safe value can also match a token heuristic and
be redacted. URL credentials, queries, and fragments do not depend on a fixed
scheme allowlist. Operators MUST keep secrets out of evaluator-visible option
metadata and repository URLs. Callers MUST NOT persist an unredacted alternate
copy or log evaluator output before applying this boundary.

## Flake Outputs and Count Authority

The independent carrier extracts each revision once. Browsing does not evaluate
each managed host. Exported-module analysis uses module-system checking with
unmatched definitions disabled; genuinely unevaluable modules carry a safe
module-analysis error instead of failing the complete carrier.

System reconciliation joins the selected revision's declared configuration
names with active managed systems:

- `managed` means the output is declared and has a visible managed system.
- `declared_unmanaged` means the output is declared without a visible managed
  system.
- `managed_undeclared` means a managed configuration is absent at the selected
  revision.

Multiple managed hosts with one configuration name set `output_collapsed`.
`managed_system_count` is the authoritative count of visible active managed
systems and is revision-independent. `declared_system_count` comes from the
selected snapshot. Fleet subtitles, rollout denominators, removal warnings,
and managed totals MUST use the managed-system relationship, not declared
output count or the length of a bounded API page.

The Systems pane supports `all`, `declared_unmanaged`, and
`managed_undeclared` filters. The server applies the filter before the bounded
offset and limit. `pagination.system_total` is the visible total for the active
filter, and `pagination.systems_has_more` describes that filtered sequence.
Revision-global reconciliation counts and warnings do not change with the
filter or page.

An exported module's `source_input`, `source_revision`, and `source_path`
identify the location of its `nixosModules` attribute binding. The evaluator
uses the Nix attribute position and requires one unambiguous longest matching
input root. The path is relative to that root. Missing positions and ambiguous
roots produce null fields. These fields do not identify value provenance and
do not grant source navigation. Declaration `source_paths` identify the
declaration locations.

The Inputs pane lists direct root inputs. For each direct root,
`direct_descendant_count` counts immediate lock-graph children and
`transitive_descendant_count` counts all unique recursive descendants. Both
counts use the complete lock graph, not a bounded API page. They are null for a
node that is not a direct root or when the count is not available. The UI uses
the transitive count for its `+N transitive` label.

For non-admin callers, reconciliation removes hidden managed systems and also
filters configuration names and module consumers that would disclose a hidden
environment. A flake with no visible active managed system returns the same
not-found response as an unknown flake. System snapshot endpoints likewise use
not-found for an unknown system, a hidden environment, a revision from another
flake, and an inactive archived source revision. This non-disclosure contract
takes precedence over lifecycle detail.

## Deployment Queue Contract

Manual deployment accepts `deploy`, `continue_auto_latest`, and
`convert_to_manual`. An `auto_latest` system requires an explicit choice.
Conversion to manual commits independently before deployment queueing. A later
queue failure therefore returns partial success: the response reports the
persisted manual policy, the conversion result, and a failed deployment state.
A conversion failure queues no deployment.

New clients send a UUID `request_id` and reuse it until the request reaches its
reported result. The server reserves that immutable system, full commit SHA,
and action before conversion. Matching retries reuse partial state or the
deployment ID. Reusing the UUID for another intent returns conflict before
policy mutation. Legacy clients that omit `request_id` receive a stable derived
identity with a 24-hour replay window; after that window the same target can be
deployed intentionally again. Queueing serializes on the system row and reuses
matching pending work.

## API and URL State

The endpoint contract and bounds are listed in the
[backend API specification](specs/02-backend-api.md#evaluation-and-flake-snapshot-api).
Option search, filter, counts, comparison, and pagination are server-side.
Option pages clamp `limit` to 1-100, `offset` to 0-100,000, and search to 256
characters. Counts are revision-global; `total` reflects the active search and
filter. Evaluated-options, module-source, and summary responses return the same
opaque token for the selected artifact, selected retained identity, exact
comparison artifact, and comparison retained identity. A positive option or
module-source offset requires the page-one token. Summary requests can supply
the token to bind independently loaded Config cards to the same artifact.
Replaced current artifacts return HTTP 409 `snapshot_changed` without rows or
summary data. When a request supplies a token, a failed, unavailable, or absent
replacement also returns `snapshot_changed` before lifecycle or no-selection
data. A generation-mode options or summary response exposes
`baseline_generation` when comparison is available. Integrity, counts, totals,
rows, baseline, provenance, and summary state are read in read-only
`REPEATABLE READ` transactions.

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

- `host_delta_count` counts option paths whose complete safe content digest
  differs from the deterministic modal state across usable configuration
  snapshots at the same commit. Missing options participate as a state. Ties
  use bytewise state identity. Definition-provenance changes affect the digest.
  A usable one-configuration corpus has a zero delta. Null means that no usable
  materialized result exists.
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

The module-source endpoint groups the complete persisted definition corpus by
the same exact tuple. It returns bounded pages ordered by winning count
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

## TASK-440 Design Audit

The Config side column preserves the reference order: Modules, Evaluation, and
Drift. Modules incrementally loads bounded source rows with input, path, and
winning/defined counts while displaying the exact snapshot-wide total.
Evaluation contains selected-revision completion, duration, option total,
source total, toplevel store path, closure package count, and comparison
identity when those persisted facts exist. Drift compares the exact selected
and running store paths. Each card has separate loading, error, lifecycle,
empty, and unavailable states. The layout uses the reference 7:5 wide split and
one-column narrow layout.

The Flake Modules pane orders modules by authoritative consumer count, shows a
proportional blast-radius indicator, and renders declarations in an explicit
Option/Type/Default table. Declaration continuation, replacement, loading,
retry, error, and unavailable states remain local to the expanded module.

The implementation intentionally differs from the reference in these places:

- The Inputs pane retains additional authoritative lock-resolution metadata.
  The column hierarchy therefore is not an exact copy of the reference.
- Browser fixtures use deterministic values instead of the reference's random
  fixture values. The changed reference `.thumbnail` is also excluded. These
  fixture differences are not product behavior.

Config pagination is geometry-sensitive. The browser measures the natural
height of the three side cards and their rendered gaps. It subtracts table
chrome and header height, divides by the rendered row height, and clamps the
request limit to 10-80 rows. Invalid measurements retain the previous limit;
the initial fallback is 24. A material limit change resets the offset to zero.
The server still enforces its 1-100 response bound.

Deterministic screenshots in dark and light themes at 1920x1080 and 900x900,
together with semantic and geometry assertions, are the visual evidence. The
assertions cover ratios, columns, stacking, clipping, overlap, reachability, and
inner scrolling. Screenshot baseline and rendered-design comparisons remain
advisory and non-blocking. They are not a strict automated pixel-baseline gate.

## Verification Expectations

Changes to this architecture require targeted evidence for:

- options-tree and configuration-independent carrier extraction, including a
  flake with zero configurations and an exported module with unmatched options;
- pre-persistence redaction across values, defaults, errors, paths, URLs,
  storage, search, diffs, logs, and API-shaped reads;
- content deduplication, all size bounds, corrupt-content degradation, and
  bounded orphan reclamation;
- full-SHA prefix collisions, Git root/missing-parent behavior, generation
  baselines, branch rewrite, source reset, retained generations, and store GC
  independence;
- environment non-disclosure and authoritative reconciliation/counts across
  endpoints;
- explicit queue authorization and reuse, concurrent deployment reservations,
  conflicting intents, conversion failure, partial success, retry replay, and
  legacy compatibility;
- API pagination, search/filter counts, response bounds, stale browser response
  rejection, hard reload, and browser back/forward URL restoration;
- supported agent and builder compatibility without a protocol migration; and
- applicable Rust tests, rustdoc, formatting, SQLx/migration checks, web UI
  build, authoritative browser workflows, and broader Nix checks when affected.
