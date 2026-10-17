---
type: API
title: Evaluation policy matrix capture
description: Defines evaluation-time policy matrix capture, exact-attempt outcome projection, isolated mock evidence, and derivation record count semantics.
tags:
  - crystal-forge
  - evaluation
  - policy-matrix
implementation_status: implemented
---

# Evaluation policy matrix capture

## Contract and ownership

`GET /api/v1/commits/:commit_id/eval/policy-matrix` retains `commit_id`,
`policies`, and `systems`. It adds `matrix: Option<EvalPolicyMatrixSnapshot>`.
The server emits `Some` for both captured and unavailable provenance. Older
server responses can omit the field. Viewer-or-above authorization runs before
the matrix query.

### Latest and pinned endpoint reads

`GET /api/v1/commits/:commit_id/eval/policy-matrix` selects the latest attempt
number, including a queued retry. `GET /api/v1/commits/:commit_id/eval/attempts/:attempt_id/policy-matrix`
selects only that UUID belonging to that commit, with no fallback to latest.
Both return the same envelope. Missing authentication or viewer-or-above role
returns 403. Missing commits, inaccessible commits, and missing or foreign pinned
attempts return the same nondisclosing 404. Database errors and invalid captured
plans return 500. An accessible selection without a capture returns 200 with
`legacy_unavailable` provenance.

Authorization, selected attempt context, capture, outcomes, and scope projection
share one read-only repeatable-read transaction. Admins retain the full projection.
Other readers require a current system on the commit's flake in an environment
where the reader has membership. Each returned captured row must match that
system's UUID and current configuration key (hostname when the configured key is
blank). Deleted systems, moved-out systems, configuration changes, and revoked
memberships fail closed; a replacement with the same name cannot inherit access.
If no current system authorizes the commit, the read returns 404. An authorized
commit can still return an empty projection when no captured rows match.

Scope filtering also removes policy columns and bundle metadata not reachable
from retained rows, including assigned empty-bundle contexts. Restricted flat
results use each retained host's captured policy bindings, not the shared
configuration's full policy union. Hidden-only labels and details are excluded.
Legacy rows have no captured system UUID and are omitted for restricted readers;
current configuration names alone cannot authorize historical evidence.
`mock_evidence` is likewise limited to retained configurations for these readers.

The snapshot adds nullable, default-absent `commit_sha`, `attempt_status`,
`started_at`, and `completed_at`. These transient fields come from the selected
commit and attempt, never the commit-wide error. `cancelling` overrides an
`in_progress` status only for the latest, currently started attempt when the
commit indicates cancellation. Capture initializes these fields to null and
rejects nonnull read-projected context; reads never update the stored payload.

The server owns applicability, attempt matching, and policy result semantics.
Clients aggregate the structured matrix. A control means one exact
`policy_version_id`. A control count is **not** a framework requirement
compliance claim. The legacy fields include the global CF-agent invariant;
that invariant has no assigned policy-version identity and is not a synthetic
bundle or policy column in `matrix`.

Types are defined in `src/api/eval_policy_matrix.rs` in the `cf-server` crate:

- `EvalPolicyMatrixSnapshot`: `evaluation_attempt_id`, `attempt_number`,
  `commit_sha`, `attempt_status`, `started_at`, `completed_at`,
  `mock_evidence`, `provenance`, `columns`, `bundles`,
  `empty_bundle_metadata_complete`, `rows`.
- `EvalPolicyColumn`: exact `policy_version_id`, lineage `policy_id`, version
  `name`, `version`, `identifiers: Vec<String>`, and nullable `description`,
  `assertion`, and `attribute_path`. Missing identifiers default to empty;
  missing optional display fields default to null for older captures.
- `EvalBundleColumn`: exact `bundle_version_id`, lineage `bundle_id`, version
  `name`, `version`, `framework`, and optional `framework_version`.
- `EvalMatrixRow`: optional historical `system_id`, captured `system_name`,
  `configuration_name`, `resolution`, optional `evaluation_digest`, per-system
  `assigned_bundles`, `policies`, and `cells`/`details` parallel to `columns`.
  `details: Vec<Option<String>>` has `#[serde(default)]` for earlier captures.
  `greenfield: Option<bool>` records explicit operator authority at capture.
  True means greenfield, false means established, and null means unknown.
  Missing fields in older snapshots default to null, never false.
- `EvalPolicyBinding`: `policy_version_id`, `report_only`, `executable`, and
  `memberships`.
- `EvalBundleMembership`: `bundle_version_id`, optional `assignment_id`, optional
  immutable `assignment_version_id`, and `source` (`assignment` or
  `virtual_baseline`). Persisted assignments always have both UUIDs. Only a
  genuine virtual baseline has neither UUID; no assignment UUID is invented.
  Earlier JSON membership UUID strings deserialize as `Some`. Missing `source`
  defaults to `assignment`; missing row `assigned_bundles` defaults to empty.

All enum wire values use snake case. `provenance` is `captured` or
`legacy_unavailable`. `resolution` is `resolved`, `conflict`, `skipped`, or
`unavailable`. Cells are `pass`, `warn`, `fail`, `unknown`, or `not_assigned`.

## Capture transaction

The production loader opens one repeatable-read transaction. It takes share
locks on the active commit and attempt, checks cancellation and the attempt
counter, and rejects an existing capture. Resolution, native execution
eligibility, enabled-state reads, exact-version metadata, and capture insertion
use that transaction. The evaluator receives the existing deduplicated
per-configuration input only after the transaction commits.

The loader preserves the existing resolver merge, executable parser,
report-only behavior, semantic digest, and shared-configuration safety check.
Registered active systems are captured separately even when their evaluator
configuration is shared. A resolver conflict still skips the configuration's
compliance gates, as before. The conflicting system is `conflict`; its other
resolved systems are `skipped`. A missing resolver outcome is `unavailable`.
A successful eligible set with no policies is `resolved` with empty `policies`.
Invalid report-only implementations remain applicable with `executable=false`.

Each complete row records the digest of the actual sorted executable and
skipped report-only inputs. This digest contains no executable source. A loader
safety error records unavailable rows when its transaction remains usable, then
returns the original error. A database error can prevent capture entirely; the
API then reports unavailable provenance. No evaluator runs after a capture
failure. This capture is metadata for the resolved execution plan, not a replay
archive of executable configuration.

Migration `0303_evaluation_policy_plan_capture.sql` stores a compact JSONB
payload keyed by `(commit_id, attempt_number)` and the exact attempt UUID. The
insert trigger validates attempt ownership. Updates and direct deletes are
rejected. Commit/attempt cleanup can cascade. Historical system IDs inside the
payload have no FK, so capture adds no system-deletion restriction. Capture uses
the existing cancellation and attempt-ownership checks. It does not enqueue
build/deployment work, perform cache admission, or grant evaluator authorization.
A duplicate capture is rejected rather than executing newly resolved inputs
under an existing attempt identity.

## Provenance and metadata boundary

Only authoritative resolver contributions create policy-to-bundle edges. A superseded
policy version cannot claim a separate pass. Equal-semantics authoritative
sources can retain many-to-many overlap. Direct policies can have no bundle
memberships. Counts must deduplicate exact policy identities, not bundle edges.

The resolver exports applicable bundle contexts from its existing work items,
using the same authoritative assignment query and virtual-baseline construction.
The export occurs before exclusions, merging, or execution eligibility can remove
all controls. Capture consumes these exact assignment-version identities without
another assignment query. Bundle and policy names
come from those exact version rows at capture, not mutable lineage labels.
Framework names/releases come from exact bundle versions. The public payload
does not copy configuration, expressions, overrides, opaque source documents,
arbitrary provenance JSON, or arbitrary compliance metadata.

### Historical control display metadata

The loader captures the exact policy version's `description` and a closed
identifier whitelist from `compliance_metadata`: string `source_rule_id`,
`source_group_id`, and `cis_section`; string entries in `srg_ids` and `cci_ids`.
The XCCDF importer declares the source rule/group and mapping arrays; policy CRUD
declares the CIS section. Capture also includes `external_id` from immutable
`compliance_requirement_versions` joined through trusted exact-policy-version
`policy_requirement_mappings`. Suggested mappings are excluded. The loader reads
all sources in the same repeatable-read transaction. Identifier labels are sorted
and deduplicated; no identifier is inferred from policy or system names.

Mappings supply display labels only. They do not create additional controls,
change policy-version-based counts, or prove requirement compliance. A mapped
requirement can need multiple policies. The generic metadata `identifiers`
objects, XCCDF check/fix bodies, expressions, configuration, overlays, and opaque
payloads are excluded. No canonical safe display-only assertion or attribute-path
source currently exists. Capture therefore stores `assertion=null` and
`attribute_path=null`. Actual saved outcome detail strings remain independent
and unchanged. Older captures are never enriched from today's catalog.

### Explicit evaluation lifecycle authority

`PUT /api/v1/systems/:id/evaluation-lifecycle` requires the JSON body
`{"greenfield": true|false|null}`. The key is required; unknown keys and nonboolean,
nonnull values are rejected. The response is `{"system_id": UUID, "greenfield":
true|false|null}` with status 200. The authenticated actor must be an operator or
admin and satisfy the existing CSRF contract. Viewers receive 403. Operators
require membership in the system's current environment. Missing systems and
systems outside that scope return the same 404. Admins retain their existing
full-environment access policy.

Migration `0305_system_evaluation_lifecycle.sql` adds a dedicated classification
table with the system UUID, nullable `greenfield`, authenticated `classified_by`
user UUID, and `classified_at` timestamp. Absence means unknown. No classification
is inferred from names, pass ratios, system-state absence, or deployment history.
System deletion cascades classification cleanup. User cleanup can clear the
actor FK; the audit event retains its UUID string as `actor_identifier`.

Each PUT locks the current system and the operator's membership until commit.
Classification and the existing `admin_audit_events` record commit atomically.
The audit action is `system_evaluation_lifecycle_updated`; metadata contains only
`system_id` and `greenfield`. Audit failure rolls back the classification. Headers,
credentials, and arbitrary request payloads are not copied into the audit event.
The loader copies only this explicit classification in its existing repeatable-read
transaction. A later operator edit cannot change a prior evaluation's value.

Greenfield is a column-trim display annotation only. It must never change control
counts, applicability, evaluation execution, outcomes, or deployment policy.

New complete captures have `empty_bundle_metadata_complete=true` and per-system
`assigned_bundles`. The global bundle list includes the exact versions for empty,
fully excluded, disabled-only, and non-Nix-only applicable contexts. Contexts are
also retained for conflicts and skipped rows. Context presence does not prove
successful resolution or a passing control. Per-policy memberships still come
only from authoritative surviving contributions; an assigned context must not
restore a superseded version's control or result.

Virtual contexts remain confined to the resolver's existing exact-version request
path. Ordinary evaluation does not request virtual baselines and gains no implicit
assignments. When actual resolver output includes a virtual baseline, capture
preserves the exact bundle version with `source=virtual_baseline` and nullable
assignment identities, even though the resolver uses nil sentinels internally.

Earlier immutable captures retain `empty_bundle_metadata_complete=false` and
default-empty `assigned_bundles`. Neither absence can establish that no empty
bundle was assigned. Never backfill them from current catalog state. Migration
`0303` is unchanged; the JSON additions require no schema migration.

## Outcome reads

One read-only repeatable-read transaction selects the represented attempt,
capture, native derivation outcomes, and isolated mock outcomes. The represented
attempt is the latest attempt number unless the request pins an attempt UUID.
Latest selection matches queue/history. A queued retry is represented even
before the commit's started-attempt counter advances. Payload identity must match
both the selected attempt UUID and attempt number. Ambiguous attempt records fail
the read.

Every scope-authorized captured row remains present when no derivation exists. Outcomes join by
the exact captured configuration key. Duplicate outcome rows for that key are
unknown. Each outcome must have an integer
`policy_results.evaluation_attempt` equal to the represented attempt number.
Missing, stale, malformed, skipped, or evaluation-failed outcomes are unknown.
The projection accepts only the recognized `evaluated` outcome state; missing
or unrecognized state strings remain unknown and supply no detail.
SQL marks only `DryRunComplete` and known downstream build states as evaluated.
Pending, NULL, or unknown derivation status IDs remain unavailable even when
their result document contains a matching attempt marker. A downstream build
failure does not invalidate otherwise matching evaluation policy evidence.
Applicable missing results never become `not_assigned`. Only a fully resolved
row that explicitly excludes a column's exact policy version is `not_assigned`.
Report-only failed checks with valid nonblocking evidence are warnings; they
never become blocking failures. Contradictory evidence is unknown.

Each read projects `rows[].details` alongside `cells` in the exact column order.
Only applicable executable checks with valid matching-attempt outcomes supply
the existing UUID-keyed result's `details` string, without rewriting the string.
Missing/non-string details and unknown/nonapplicable cells produce `null`.
Projection never copies raw result objects or configuration. Capture constructors
leave both `cells` and `details` empty. Read projection changes only the returned
in-memory snapshot; persisted immutable plan payloads are never updated.
The capture writer rejects nonempty read-projected cells or details before SQL.

Valid Boolean result metadata follows the legacy classifier: a successful check
is a pass unless `blocking=true` contradicts success; `blocking=false` failures
are warnings; enforce-mode `blocking=true` failures fail. Without `blocking`,
the captured report-only mode or a valid intrinsic `strict` Boolean determines
failure severity. Missing/invalid `passed`, an invalid present `blocking`, or
unknown strictness for an enforce-mode failure remains unknown. A report-only
binding with blocking failure metadata is contradictory and remains unknown.
These projection rules do not change evaluator or deployment-gate decisions.

The ordinary finalizer stamps confirmed Nix failures with the expected attempt,
a Boolean `evaluation_failure` flag, and the diagnostic in the same guarded
transaction. The reader accepts that diagnostic only when all three are valid.
The compatibility response retains `nix_eval_failure`, including when the
derivation's irreversible build state must remain intact. Individual controls
remain unknown because a Nix failure does not prove that each policy failed.
Stale or malformed failure evidence cannot expose an earlier diagnostic.

The legacy label/result fields remain a compatibility projection and are not
authoritative applicability or bundle data. Stale marked evidence is also masked
from the legacy projection. With a capture, markerless evidence is masked too.
An absent legacy result now displays
`legacy_unknown`, not inferred nonassignment. A planless evaluation has
`legacy_unavailable` provenance, no inferred policy/bundle columns or memberships,
and outcome-only rows with `system_id=None` and `resolution=unavailable`.
New attempts that have not captured a plan use the same unavailable contract.
Newly discovered flake configurations without a registered planned system
remain in the unrestricted legacy `systems`; no system identity or assignment
is invented for them in the captured plan. Restricted readers receive only the
captured-host projection described above.

Pinned reads preserve captured applicability but cannot restore overwritten
native results. Native derivation evidence is mutable: if its marker no longer
matches the selected attempt, controls remain unknown and flat results are
masked. Pinned reads also mask markerless legacy evidence; only the unrestricted
latest legacy compatibility projection can retain markerless results. Isolated
mock evidence remains readable by its exact retained attempt UUID. No read
reconstructs historical outcomes or memberships from the current catalog.

## Isolated mock evidence

Migration `0304_isolated_mock_evaluation_outcomes.sql` adds
`evaluation_mock_outcomes`. Its primary key is
`(evaluation_attempt_id, configuration_name)`. A composite foreign key validates
the attempt UUID, commit ID, and attempt number together. The table contains
policy JSON, the agent/requirements flags, and a capture timestamp. It has no
derivation, store, output, or path fields. It is not a build-admission source.

The mock-only writer locks the commit and active attempt with `FOR UPDATE`.
It checks ownership and lifecycle state before cancellation. Missing ownership
or a newer attempt returns typed supersession; matching cancellation returns
typed cancellation. The transaction writes only isolated evidence. The first
write for an attempt/configuration wins; repeated writes do nothing. Updates
and direct deletes are rejected. Parent cleanup can cascade.

`policy_results_json` retains the native result document shape, UUID-keyed
assigned outcomes, enforcement mode, and blocking semantics. Mock documents add
`execution_mode="mock"` and the integer `evaluation_attempt` marker. The schema
requires both markers to match the isolated record. Assigned/global detail
strings start with `[MOCK]`. Synthetic results do not establish Nix evaluation,
configuration observations, composite assessments, or deployment eligibility.
The mock evaluator returns no native successful-system carriers, derivation
paths, output paths, or option snapshots for ordinary finalization to publish.

Matrix reads select mock rows by the represented UUID, commit, and number in the
same repeatable-read snapshot as native outcomes and the plan. A stale or
markerless native row cannot hide a matching mock row. Two valid current-attempt
sources for one configuration are ambiguous: both the structured cells and flat
compatibility fields expose no known outcome. Missing mock configurations remain
unknown in the captured plan. The flat projection retains mock global CF-agent
results through its existing result-document contract.

The additive `matrix.mock_evidence` Boolean is a transient read projection of
durable, exact represented-attempt mock records. It is true when any such record
exists in the authorized projection, including mixed/ambiguous outcomes. Admin
reads also retain the flag for legacy evaluations without a plan; restricted
readers receive no legacy rows and therefore no legacy mock-evidence flag.
It does not depend on the current server execution mode. False means no isolated
mock evidence was found in the authorized projection, not proof of native
evaluation. Earlier JSON defaults
to false. Capture constructors initialize false, and capture insertion rejects
true. Reads recompute the flag in the same repeatable-read transaction and never
change the stored plan. A queued retry without its own mock records is false.

## Queue and history policy counters

Queue and history use the shared lateral query in `queries/commits/policy_counts.rs`,
not a mock-only override of unchanged native counters. Both select the latest
attempt as `ea`, including queued retries. Native NixOS derivations require a
positive integer JSON `evaluation_attempt` marker equal to that attempt number.
Isolated mock rows must match commit ID, attempt number, and attempt UUID.
Counts group evidence by configuration, not captured host or control identity.
Multiple valid current carriers for one configuration, including a native/mock
pair, contribute no known count.

Native evidence requires status ID 5, 6, 7, 8, 10, or 12, or a confirmed
evaluation failure. A confirmed failure requires Boolean `evaluation_failure=true`
and a nonblank string `evaluation_failure_error`. A present but incomplete or
invalid failure document is excluded. Status 6 or confirmed failure contributes
to `eval_failed_count`; otherwise both agent/requirements flags true contribute
to `passed_count`, and either flag false contributes to `policy_failed_count`.
Other flag combinations contribute no known count. Mock evidence uses the same
agent/requirements classification and does not fabricate Nix evaluation errors.
Stale native evidence does not contribute current counts. Pending native evidence
is excluded unless it carries a confirmed matching-attempt evaluation failure.

The additive, default-false `policy_counts_available` Boolean distinguishes
attributable counts from unavailable counts while retaining integer counter
fields. True means a represented attempt exists, no current configuration has
multiple evidence carriers, and no native configuration has an unattributable
marker unless exact current mock evidence replaces that configuration. Missing,
null, string, fractional, and nonpositive native markers are unattributable.
True does not prove completion, complete coverage, known policy flags, or a pass.
A partial attempt can report known counts with true; a queued retry containing
only attributable older evidence reports zero with true. Ambiguous current
carriers or uncovered unattributable legacy records make the flag false even if
other configurations supply integer counts. The UI shows unavailable rather
than treating false as a clean zero. These runtime SQLx reads introduce no
checked-query metadata and do not change runnable derivation or build queries.

## Derivation record counts

Queue and history responses add nullable `derivation_count`. The value counts
persisted derivation records with a valid numeric attempt marker matching the
represented attempt. It is not a closure-package total or a count of built
outputs. Older marked records do not contribute. Unattributable legacy records
make the value unavailable rather than producing a guessed total.

An isolated mock run with no derivation records reports zero. The drawer renders
the reported value or an em dash when unavailable; it never estimates a count
from the number of systems. Mock evidence does not create dependency inventory.

## UI navigation and presentation

`Open system` uses the captured system UUID and full evaluated commit SHA to open
SystemDetail Config in commit mode. `View in compliance evidence` offers one action per captured
contributing bundle and carries commit ID/SHA, attempt UUID/number, system UUID,
bundle lineage/version UUIDs, and policy lineage/version UUIDs into Compliance.
Compliance fetches the pinned endpoint and validates every identity plus the
actual per-system policy-to-bundle membership. Missing, mismatched, ambiguous,
or unauthorized context is explicitly unavailable. This read-only check is not
framework requirement satisfaction or deployed compliance and does not substitute
current catalog membership or deployed evidence.

Receiver handoffs and return state are in memory; refresh persistence is not
promised. Back to Evaluation validates the pinned commit SHA and attempt identity
before restoring the original queue/history target, search/status/flake/latest
filters, drawer tab and size. Matrix presentation is stored by commit ID and
attempt UUID: mode, row filter, sort, trim, bundle focus, expanded row, issue,
assertion card, and row page survive same-attempt refreshes and explicit request
errors. Errors replace displayed evidence rather than presenting cached success.
A different attempt uses its own saved presentation or defaults. Scroll and
invoking focus restoration is best effort after validation and mounting. A pinned
return does not follow newer retry summaries; latest-only header policy counters
and derivation counts are marked unavailable. Latest evaluation actions require
opening the latest evaluations view.

More than 14 distinct controls defaults to Bundles with column trim enabled;
smaller matrices default to Controls without trim. Mode changes clear bundle
focus but preserve row filter, sort, and trim. Bundle drilldown selects that exact
bundle's Controls without changing those choices. Row filtering and sorting run
before pagination. Rendering targets 8192 body cells, including the identity,
health, and layout filler cells, with `clamp(8192 / (visible_columns + 3), 1, 100)`
systems per page.
Aggregation, issue counts, and trim use the full relevant row sets, not one page;
horizontal scrolling retains control columns even when one row exceeds the budget.

Column trim retains failures, warnings, and unknown outcomes. Its baseline is the
filtered row set, excluding only explicit `greenfield=true` rows when other rows
remain. Unknown classifications remain in the baseline; an all-greenfield set
retains all rows. An empty row filter falls back to all planned rows. If trim
would hide every scoped control, all scoped controls remain visible. Greenfield
never changes health, rollups, applicability, or control counts.

### Matrix layout and failure-first detail

The captured matrix uses the design geometry: System 180 px, Health 110 px, and
bundle columns at their min-content width with a 104 px minimum. Bundle names stay
on one line with their exact version beside the name; long names truncate and keep
the full identity in the heading title. A hidden trailing filler column is the only
auto-width column, so one bundle and six bundles keep identical column widths and
38 px rows. Bundle cells show compact `passed/applicable`, `N/A`, or `Unknown`
values; the cell title states the full applicability state.

The toolbar has three levels: Top Issues (only when real failed controls exist),
row filters with the Bundles/Controls selector, then the control summary, column
options, and sort. Expanded rows list failed, warning, and unknown controls first
as collapsible cards. Passed and not-assigned controls stay behind an explicit
toggle. An open card shows captured assertion and attribute path when present,
otherwise an explicit unavailable statement, plus the saved outcome detail,
enforcement mode, exact version and lineage identifiers, the registry action, and
one Compliance evidence action per contributing bundle. Expanded details stay
pinned to the visible scroll width.

The header Systems value counts evaluated flake configurations; the header Policy
value counts configurations that pass or fail the CF-agent and assigned-requirement
gate. The matrix counts registered systems with a captured plan visible to the
reader, and their controls. When the populations differ, or when global CF-agent
failures exist, a `Matrix scope` note states both counts. The global CF-agent
result stays in the collapsed Platform evidence panel and never enters bundle or
control counts.

## Verification handoff

Pure tests cover missing/stale outcomes, typed statuses, report-only checks,
partial rows, overlap retention, empty/conflict/skipped/unavailable resolution,
legacy provenance, and unauthenticated endpoint rejection. Ignored
`sqlx::test(migrations = "./migrations")` cases cover transaction rollback,
first-write protection, immutable payloads, attempt ownership, retry isolation,
and parent cleanup. New `server::policy_capture_tests` cases exercise published
policy/bundle fixtures through the real resolver and production loader. They
cover authoritative overlap, per-system empty/disabled-only contexts, exact
version IDs, unchanged executable inputs/digests, historical catalog/assignment
mutation, and virtual baselines without assignment UUIDs. They require the
authoritative disposable PostgreSQL sandbox.

Mock persistence regressions verify exact ownership, immutable outcomes,
cancellation/supersession, and absence of derivations, buildable rows, jobs,
observations, and assessments. Projection cases verify current/old attempt
isolation, ambiguous native/mock evidence, persistent mock labels, queued retry
counts, and unchanged plan payloads. Count regressions distinguish current
derivation records from stale or unattributable legacy records.

Run migration-backed cases in disposable SQLx databases through a verified
isolated control URL. Do not point test bodies at a review preview or a shared
development database. `checks/server-regressions/default.nix` lists the exact
required ignored cases and requires each selected test to execute successfully.
The authoritative browser workflow is `26e-evaluation-policy-bundle-controls`;
CI owns its VM execution. Preview inspection is separate evidence and must use
the actual API response rather than an intercepted response when reporting live
data readiness. Record exact commands, outcomes, and remaining verification gaps
in the task ledger, not as timeless claims in this contract.
