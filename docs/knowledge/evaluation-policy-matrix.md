# Evaluation policy matrix capture

## Contract and ownership

`GET /api/v1/commits/:commit_id/eval/policy-matrix` retains `commit_id`,
`policies`, and `systems`. It adds `matrix: Option<EvalPolicyMatrixSnapshot>`.
The server emits `Some` for both captured and unavailable provenance. Older
server responses can omit the field. Viewer-or-above authorization runs before
the matrix query.

The server owns applicability, attempt matching, and policy result semantics.
Clients aggregate the structured matrix. A control means one exact
`policy_version_id`. A control count is **not** a framework requirement
compliance claim. The legacy fields include the global CF-agent invariant;
that invariant has no assigned policy-version identity and is not a synthetic
bundle or policy column in `matrix`.

Types are defined in `src/api/eval_policy_matrix.rs` in the `cf-server` crate:

- `EvalPolicyMatrixSnapshot`: `evaluation_attempt_id`, `attempt_number`,
  `mock_evidence`, `provenance`, `columns`, `bundles`,
  `empty_bundle_metadata_complete`, `rows`.
- `EvalPolicyColumn`: exact `policy_version_id`, lineage `policy_id`, version
  `name`, and `version`.
- `EvalBundleColumn`: exact `bundle_version_id`, lineage `bundle_id`, version
  `name`, `version`, `framework`, and optional `framework_version`.
- `EvalMatrixRow`: optional historical `system_id`, captured `system_name`,
  `configuration_name`, `resolution`, optional `evaluation_digest`, per-system
  `assigned_bundles`, `policies`, and `cells`/`details` parallel to `columns`.
  `details: Vec<Option<String>>` has `#[serde(default)]` for earlier captures.
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
attempt is the latest attempt number, matching queue/history selection. A queued retry is represented even
before the commit's started-attempt counter advances. Payload identity must match
both the selected attempt UUID and attempt number. Ambiguous attempt records fail
the read.

Every captured row remains present when no derivation exists. Outcomes join by
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
remain in legacy `systems`; no system identity or assignment is invented for
them in the captured plan.

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
exists, including mixed/ambiguous outcomes and legacy evaluations without a plan.
It does not depend on the current server execution mode. False means no isolated
mock evidence was found, not proof of native evaluation. Earlier JSON defaults
to false. Capture constructors initialize false, and capture insertion rejects
true. Reads recompute the flag in the same repeatable-read transaction and never
change the stored plan. A queued retry without its own mock records is false.

`view_evaluation_mock_counts` supplies queue/history metric overrides when the
represented attempt has isolated mock evidence. The view excludes configurations
with valid current native evidence because that evidence is ambiguous. It counts
mock passing and policy-failed configurations; mock execution does not fabricate
Nix evaluation errors. A queued retry after mock evidence has zero known counts
until that retry produces evidence. Native rows without a mock override retain
their existing count queries. The view does not change runnable derivation or
build queries. These reads use runtime SQLx queries; they introduce no
checked-query metadata.

## Derivation record counts

Queue and history responses add nullable `derivation_count`. The value counts
persisted derivation records with a valid numeric attempt marker matching the
represented attempt. It is not a closure-package total or a count of built
outputs. Older marked records do not contribute. Unattributable legacy records
make the value unavailable rather than producing a guessed total.

An isolated mock run with no derivation records reports zero. The drawer renders
the reported value or an em dash when unavailable; it never estimates a count
from the number of systems. Mock evidence does not create dependency inventory.

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
