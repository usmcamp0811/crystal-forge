---
type: Concept
title: Management modes and configuration assurance
description: Defines managed, external, and reference registrations, deployment authority transitions, exact configuration evidence, parent observations, metric populations, and TASK-508 upgrade limits.
tags:
  - crystal-forge
  - management-mode
  - assurance
  - deployment
  - evidence
implementation_status: partial
---

# Management modes and configuration assurance

**Status: partial.** This contract describes source inspected in the TASK-508
worktree, including uncommitted changes. The owner reports passing targeted
database and frontend checks and selected VM cases. Full exact-source owner
runs, upgrade proof, and final browser/design parity remain pending. Source
inspection is not test proof; see [remaining proof](#source-inspection-and-remaining-proof).
The current gaps below prevent a claim that the complete feature is delivered.

## Registration and identity

`systems.management_mode` is a typed server/API value:

| Mode | Resource meaning | CF deployment authority | Agent enrollment |
| --- | --- | --- | --- |
| `managed` | Machine managed by Crystal Forge | Subject to deployment policy and final authorization | Valid Ed25519 public key required |
| `external` | Machine deployed by another operator or tool | None | No agent key |
| `reference` | Descriptive configuration baseline, not a machine | Not applicable | No agent key |

Existing rows and create requests that omit the mode default to `managed`.
Invalid modes are rejected. An update that omits the mode preserves it.
`deployment_policy` (`manual`, `auto_latest`, `pinned`) is a separate field;
storing a policy on a nonmanaged registration does not grant authority.
`is_active` is registry activity, not mode, machine health, or observation age.

Migration `0319_system_registration_metadata.sql` adds registration-owned
`tags` (`text[]`, non-null, default `{}`) and nullable `description` for operator
notes. Create and edit persist these fields in every mode. Tags are trimmed,
nonempty, case-sensitive, and deduplicated in first-occurrence order. Notes are
trimmed; blank notes become null. Managed create also persists `fqdn` and
`reachability` (`direct` or `pull`). PATCH omission preserves these fields;
explicit null clears tags, notes, or address, and resets reachability to
`direct`. Configuration synchronization preserves operator metadata.

There is no live policy inheritance. Create/edit requests may send the
request-only token `environment_default`; under the environment row lock the
server copies that environment's current `default_policy` and stores the
concrete `manual`, `auto_latest`, or `pinned` value. An unsupported environment
default is rejected (400). Later environment-default changes never update the
registration.

Migration `0324_system_pinned_commit.sql` adds `systems.pinned_commit_id`, the
declared pin of a managed `pinned` system. PATCH `pinned_commit` (full SHA of
an unarchived commit in the registered flake) sets it; omission preserves it;
null clears it. Saving a pin never sets `desired_target` or queues a
deployment; automatic selection already ignores `pinned` systems. A trigger
clears the pin whenever the registration is no longer managed with policy
`pinned`, or its flake no longer contains the commit, so conversion,
convert-to-manual and flake reassignment cannot leave a stale pin.

The UUID identifies the registration. The legacy field `hostname` holds its
unique resource name; a reference can use a descriptive name rather than a DNS
hostname. External/reference registration requires a real registered flake and
an explicit nonblank `system_configuration_name`. It rejects supplied keys,
including empty strings. The server generates no placeholder enrollment key.
Multiple resource names/UUIDs can share one flake/configuration. Evaluation and
artifact identity remain configuration-scoped, not resource-name-scoped.

Registration is insert-only. A name conflict cannot overwrite another resource.
The server rechecks the active actor, roles, and environment memberships inside
the write transaction. Operators need source and destination environment scope;
administrators can manage unassigned resources. Viewer reads remain scoped.
Registration and metadata edits write credential-free audit records in the same
transaction, including old/new modes and parent associations.

Only an external resource can have `parent_host_id`. The parent must be active,
managed, distinct from the child, and in the same non-null environment. Clear
child associations before moving a parent to another environment or changing
its mode. Association does not grant deployment or evidence authority.

## Deployment authority and transitions

Automatic selection and rollout groups admit active managed systems only.
Manual deployment, rollback, desired-target writes, pending-target delivery,
and final composite authorization independently check persisted authority.
Agent authentication rejects inactive/nonmanaged registrations or missing keys.
Configuration success cannot bypass these checks.

Metadata transitions share writer and system-row locking with deployment and
evidence writers. Moving from managed to external/reference atomically:

- removes the enrollment public key and desired target, derivation, and set time;
- marks pending deployments `superseded`;
- tombstones request reservations as `invalidated`;
- removes the legacy desired-target bridge;
- records the metadata audit while retaining scan, deployment, and event history.

Re-enrollment cannot revive an invalidated request ID. Use a new request ID for
new deployment intent. Revocation prevents subsequent server delivery; it is not
a remote cancellation of an activation already received by an agent.

Initial conversion from external/reference to managed requires an explicit
`management_mode: "managed"`, fresh valid enrollment credentials, and explicit
`deployment_policy: "manual"`. The conversion queues no deployment. A later
authorized edit can explicitly select `auto_latest` or `pinned`; normal policy
processing then applies. External/reference transitions retain configuration
evidence and do not enroll an agent.

Migration `0318_agent_enrollment_epoch.sql` records a database-owned
`systems.agent_enrolled_at` when a nonmanaged registration becomes managed.
The trigger takes wall-clock time after the system-row lock is acquired.
Transaction-start time could precede an old report committed while the
transition waited for that lock. Existing managed rows retain a null boundary
and preserve their legacy semantics. Key rotation, metadata edits, and config
sync cannot reset or erase the boundary. Leaving managed mode clears it.

Re-enrollment remains pending until the new identity reports. The filtered
`view_agent_current_system_states` admits only active managed observations at
or after the enrollment boundary. Current CVE authority, runtime scan selection,
active list health, deployment status, and POA&M runtime verification use that
projection. Migration `0320_inactive_managed_detail_observations.sql` makes
detail telemetry mode/epoch-filtered without requiring registry activity.
Deactivation alone does not invent an offline health state; Current authority
and agent writes remain active-only.
Retained full states and heartbeats remain history. A heartbeat attached to an
excluded old state cannot restore health or authority. The first new-key report
inserts a full state even when its runtime fields equal the old state; normal
heartbeat deduplication resumes afterward. Both agent write endpoints recheck
the current key, enrollment epoch, managed mode, and activity under the
transaction's system-row lock. A request authenticated before revocation cannot
write after re-enrollment.

**Revoked keys stay revoked.** Leaving managed records the system's key in
`system_revoked_agent_keys` (migration `0333`) in the same transaction that
clears it. A later conversion into managed, or a key rotation, that presents a
recorded key for that system is refused with HTTP 400 ("This agent key was
revoked when the system left managed mode; enroll a new key"). Keys compare in
canonical base64 form. Re-enrollment therefore always uses a fresh keypair, and
the revoked agent identity cannot authenticate again.
Changing the mode back to managed alone cannot restore Current CVE or POA&M
authority. A fresh report authenticated under the new enrollment is required.

See [Systems API](../api/systems-api.md) for field and response details and
[Deployment flow](../deployment/deployment-flow.md) for managed delivery.

## Configuration evidence is not runtime assurance

Flake Explorer and registration use persisted native declaration inventory when
the optional full module/input output snapshot is absent. The API's nullable
`declared_system_names` page distinguishes unavailable discovery from a known
empty set. Native declaration availability does not claim that module or input
inspection completed. See the [flake snapshot API](../api/flakes-and-evaluation-snapshot-api.md).

Exact configuration evidence identifies the registered flake, branch revision,
effective configuration name, derivation, build attempt, and scan. It does not
establish that a machine runs that output. A reference has no running generation.
A parent observation does not manufacture one.

External/reference scan selection uses position 0 of the registered flake's
ready branch snapshot. It does not substitute an older successful/cached build
when that revision was removed, failed evaluation, or failed building. Native
discovery proves removal; absent discovery means unavailable. Historical scans
remain separately identified. A latest failed scan and an older sealed good
scan can both be returned. Missing/unsealed counts are null, not zero CVEs.

The evaluator records `global.artifactAdmission.passed` separately from
`cf_agent_enabled` and `policy_requirements_met`. An active external/reference
registration for the exact flake/configuration can admit an agentless artifact.
The evaluated agent fact remains false when the agent is disabled. Missing
agent metadata, missing assigned outcomes, strict assigned failures, and build
scope exclusion do not become passes. Explicit assigned agent policies retain
their enforcement semantics. Shared registrations do not fan out build jobs for
one derivation. Older results without artifact-admission evidence need a new
evaluation; the server does not rewrite historical policy results.

### One configuration in several environments

A configuration may have several registrations. The SQL function
`derivation_environment(commit, name)` is the single rule that build admission,
builder claims, exact-artifact reuse and cache publication use to find the
environment that owns the artifact. Migration `0331` replaces the `0330`
definition:

- Only active registrations in the commit's flake count. A registration without
  an environment is ignored.
- Each registration counts by its effective configuration identity: its trimmed,
  non-empty `system_configuration_name`, otherwise its hostname (legacy
  fallback). An explicit name and a hostname fallback that select the same
  configuration are the same identity. A hostname does not count when the
  registration's explicit name selects another configuration.
- If all counted registrations are in one environment, that environment owns the
  job and the cache credentials. Two registrations in one environment still give
  one deduplicated build.
- If they are in more than one environment, the result is **ambiguous**.

`0330` ranked explicit-name matches above hostname matches and examined only the
best rank. A reference registration that named `web` in environment A therefore
hid a legacy managed registration with hostname `web` in environment B, and A
was returned as the unique owner. `0331` removes the ranking.

### Environment moves and queued work

A build job records its environment when it is queued. Builder authorization and
cache destination selection both depend on that environment, so the job
environment MUST equal the current owner when work is selected or credentials
are released:

- Every builder peek and claim requires `JOB_ENVIRONMENT_IS_CURRENT` in addition
  to `ARTIFACT_POLICY_PREDICATE`. After a configuration's only registration moves
  from environment A to B, no builder can claim the queued job while it still
  carries A. This includes builders restricted to A and wildcard builders.
- `retire_unclaimable_queued_jobs` moves a still-admitted queued job to the
  current owner environment. It cancels a job that no longer satisfies artifact
  admission. A queued job has no claim, cache dispatch or credentials, so the
  move changes no recorded authorization. The system update handler runs this
  reconciliation after its last registration write, in the same transaction.
  The builder recovery loop runs it for every other writer.
- `record_job_cache_dispatch` refuses a selected cache destination when the
  claimed job's environment is not the current owner. The check runs under the
  job row lock before any credential read. A registration move after the claim
  therefore cannot release the new environment's destination to that claim.
  The builder API reports this as a transient dispatch failure.

**Temporary restriction (TASK-518).** Sharing one configuration across
environments is not supported. Safe support needs an explicit multi-environment
build authorization and per-environment cache publication, so that no builder
holds another environment's cache credentials. Until then the server fails
closed:

- `ARTIFACT_POLICY_PREDICATE` rejects an ambiguous derivation. Build admission,
  recovery, every builder peek and claim, and exact-artifact reuse refuse it.
  It never becomes a NULL-environment job. Every builder can claim a NULL
  environment, and NULL publication selects global destinations.
- Activation records the preparation error "Build refused for configuration X:
  its registrations are in more than one environment" with the action to take.
- A job that was queued before the registrations spread is cancelled by the
  revalidation sweep (`retire_unclaimable_queued_jobs`, run by the builder
  recovery loop) and by every system update, including a management-mode
  transition. A cancelled attempt is
  history. After the registrations share one environment, retry the build
  manually. A derivation that was refused before it had any attempt becomes a
  build-preparation recovery candidate once one environment owns it. Recovery
  applies its normal retry backoff.
- A job already building when the registrations spread keeps running. A cache
  publication step for it answers HTTP 409, not 500.

Build, cache and scan evidence for configurations that share one environment is
unchanged. Per-registration access control on scans and evidence is unchanged.

Build creation/recovery in `queries/build_jobs.rs` and both peeks and all four
atomic claim variants in `queries/builders.rs` use this admission. Builder
session fencing, environment filters, verified-source prerequisites and cache
preflight remain enforced. Isolated database tests cover actual claims; full
remote evaluation/build/scan integration proof remains pending.

### Exact derivation reuse

A new commit can evaluate a configuration to a `.drv` that an earlier commit
already owns, for example after a documentation-only change. Migration
`0322_exact_derivation_reuse.sql` keeps the new evaluation as its own carrier
row (commit, attempt, policy results, real `.drv` and expected output) and
records `reused_from_derivation_id`, a flat reference to the physical origin.
Database triggers enforce:

- the binding is flat, native-only, and requires an identical `.drv` and
  expected output; an expected output alone never creates a binding;
- a referenced or job-bound physical identity is immutable;
- the origin path stays unique (`derivations_origin_path_unique`), and at
  most one queued, building, or cancelling job exists per physical artifact;
- `reused_build_job_id` adopts only a real active or successful job of the
  same flake/configuration. No job, success, publication, or scan is copied.

`view_derivation_execution_evidence` exposes the evaluation carrier separately
from the actual execution source. Configuration assurance reports the selected
revision and evaluation as the new commit, and the build/publication/scan as the
real origin with its own revision. A blocked or failed origin does not prevent
one real job under the new carrier's own authorized policy; a rejected carrier
cannot cancel shared work. Evidence joins require the same flake and
configuration (`derivations_share_evidence_scope`). Physical equality never
grants another scope's evidence or cache credentials.

A successful origin job is adopted, and a reused head is admitted for scanning,
only when `derivation_publication_eligible` finds a completed publication of the
exact output. Three publication provenances can qualify:

- `database`: to an enabled destination that the carrier's environment may use.
- `legacy`: only while exactly one destination answers to the recorded name or
  URL. A second matching destination, enabled or not, makes it ambiguous.
- `static` (migration `0328`): only when the recorded destination equals the
  destination of the **currently effective static cache configuration**. That is
  `cache.push_to` while `cache.push_after_build` is true. It is none when
  publication is disabled, when `push_to` is missing or blank, when the
  destination was changed after the publication, or when the configuration
  cannot be loaded. The database cannot read the configuration, so the server
  passes the destination in (`cache_push::effective_static_destination`). It
  applies the same rule as the static job resolver and the scanner's
  materialization check; a test asserts the two never disagree.

Admission is a yes/no decision and grants nothing. A scan claim carries no cache
source or credential. The server-local scanner re-resolves a static source
against the configuration in force when it runs and refuses a record whose
destination no longer matches. Delivery to an agent resolves read sources from
database destinations only, so a static publication yields no read source, even
when a database destination has the same URL. A failed, pending or in-progress
publication, a publication of another output, and a publication of another
derivation never qualify. Queued or running origin work is adopted regardless of
publication. Anything that does not qualify fails closed: the carrier builds
under its own policy and a scan of a reused head is refused with
`scan_ineligible`.

Current exact-CVE authority counts enforced aliases as one physical artifact,
so a reused head does not become ambiguous. Two different `.drv`s with the same
output remain ambiguous. The returned commit is scan provenance, never inferred
runtime Git revision. Migration `0323_external_reconciliation_reuse_epoch.sql`
applies the same alias rule, and the enrollment epoch from `0318`, to external
generation reconciliation. Before `0323` that trigger required a scan on the
carrier itself and read raw observations.

CVE scans reuse the existing derivation runner, cache verification, diagnostics,
and execution fencing. Fleet admission deduplicates derivations. Scheduled CVE
rescans include all modes: selected built external/reference head artifacts use
the existing `deployed_interval` cadence label. That scheduler label is not a
deployment claim. Recent/archived artifact cadences and post-build recovery
remain separate. Active scans and existing failure backoff still govern retries.
Hardening target selection also uses the exact selected nonmanaged branch head.
For an exact-reuse head, scan admission, hardening selection, and Scanning
coverage resolve the head to its execution source derivation. A scoped scan
response returns that source as `derivation_id` and the requested carrier as
`configuration_derivation_id`; the Web UI matches its request by the carrier.
The Web UI offers its exact derivation/build history without requiring a CVE
scan. Hardening reads return exact-derivation evidence, not Current runtime
authority. CVE scans and hardening assessments retain independent availability
and diagnostics.
See [scan operations](../api/cve-scan-operations-api.md) for denominator details.

Current exact-CVE authority requires an active **managed** registration, its
latest consistent observation in the current enrollment epoch, exactly one
matching physical artifact (enforced aliases count once), and the newest
completed schema-1 scan of that artifact within the same configuration scope.
Retained observations after conversion do not preserve Current authority.
Configuration-only scans and parent unit reports cannot authorize runtime CVE
triage, remediation verification, POA&M closure, or deployment attestation.
Hardening/composite runtime mutation and verification paths also check managed
mode. Configuration assessments can remain evidence without granting those
runtime actions. See [exact-CVE authority](../cves/exact-cve-evidence-authority-and-inventory-reads.md)
and [POA&M closure](../poam/cve-poam-verification-and-closure.md).

An authorized configuration Re-scan uses the scoped system endpoint with the
displayed artifact's `expected_derivation_id` query parameter. The server
resolves the target independently. A changed eligible target returns HTTP 409
`scan_target_changed` and queues no substitute. Nonmanaged admission uses the
existing durable queue without requiring local vulnix; managed scans keep the
immediate local path. An accepted request is not proof of execution.

Durable manual and periodic admission attaches an eligible successful producing
build job for the exact artifact. That job must have the exact output, a
completed cache publication, a matching environment, scanner capability, and the
current builder session. Remote claims verify that producer and keep session
fencing. If no executor can run the scan, a scoped request returns HTTP 409
`scan_executor_unavailable`, and queued rows without an executor are failed with
retained evidence. They do not remain pending. Periodic target selection and
admission run even when the server has no local vulnix; only local execution is
skipped. Producer affinity is bound to the builder **session** that produced
the job, as in the pre-existing remote claim design. In a remote-only deployment
without a server-local scanner, a rescan of an artifact built in an earlier
builder session therefore fails as `scan_executor_unavailable` until the
artifact is rebuilt or a local scanner is available; TASK-513 tracks a
session-independent trust model. Authorization, expected-ID validation,
admission, and audit share one transaction. Its lock order is derivation, canonical CVE, environment, POA&M
sentinel, then system and actor scope.

Per-system scan statistics classify a selected external/reference head on the
same `deployed_interval` cadence as the scheduler and aggregate statistics.
The Systems list derives selected-scan freshness from one schedule read with the
same rule. A missing or unparseable schedule leaves freshness unknown.

### Agent report identity and delivery

Both agent write paths reject a report whose `hostname` differs from the
authenticated registration. Each path then rechecks the locked registration's
UUID, hostname, key, enrollment epoch, mode, and activity before writing. Final
deployment delivery receives the authenticated registration snapshot. It
rechecks the enrollment inside the authorization transaction before claiming a
target or returning publication-read credentials, so a heartbeat from a revoked
enrollment cannot claim work queued for a later one. Duplicate-key cleanup runs
only after that recheck, in the same transaction as the observation, behind a
savepoint. Migration `0321_current_detail_cve_counts.sql` makes managed detail
CVE counts use exact Current authority instead of name-based historical scan
views.

An ISO-oriented reference still names a `nixosConfigurations` closure. Scanning
that closure is not inspection of a generated ISO, VM image, boot medium, or
deployed guest. Image distribution and guest attestation are not implemented by
this model.

## Signed parent observations

An upgraded managed parent receives explicit `guest_observation_targets` in its
normal heartbeat response. Its next signed heartbeat can include
`guest_observations`. Older agents omit the optional list and keep working.
Agents communicate with the server, not builders or PostgreSQL.

Targets bind guest UUID, registered guest identity, and the fixed unit
`microvm@<guest_identity>.service`. Identities allow an initial alphanumeric
character followed by alphanumerics, `-`, `_`, or `.`; the unit is at most 255
bytes. The collector runs only:

```sh
systemctl show --no-pager --property=ActiveState -- microvm@<guest_identity>.service
```

Collection has a five-second per-unit timeout. It reads only `ActiveState`;
failure produces an unknown/null state. The current collector always reports
`runner_store_path: null`. The protocol permits a validated runner store-object
root, but a runner is not a guest current-system closure or a configuration
match. The collector does not inspect arbitrary `ExecStart` arguments, discover
guests, execute guest commands, or orchestrate VMs.

Ingestion rechecks the parent's current active managed identity/key, association,
environment, unit, and collection time under transaction locks. An invalid or
revoked parent identity rejects the heartbeat. Unauthorized, duplicate,
future-dated, or mismatched guest reports discard the whole observation batch;
the authenticated parent's heartbeat still succeeds and returns its current
authorized target list.
This lets a pull-only parent recover after a guest association changes instead
of repeatedly sending an obsolete cached target. No rejected observation is
stored. Accepted rows are append-only with distinct `observed_at` and
`received_at` timestamps.
They never update guest `system_states`, scans, policy, or Current authority.

The latest-observation API reads persisted evidence for the current association.
Inactive, detached or reference resources, revoked parents, renamed identities, or environment
mismatches return null while history remains stored. HTTP success does not make
an old observation fresh. Missing/stale observations do not mean guest outage;
`ActiveState=active` does not prove guest health or attestation. The UI's reload
reads this latest stored row; it is not synchronous Refresh/contact with an
agent. The inspected runtime card has no reporting freshness policy and labels
freshness unknown. No automatic guest configuration match is established.

## Metric populations

| Surface | Population and unit |
| --- | --- |
| Systems list | Active registrations across all modes. Each filter (management, environment, status, flake, tag) is multi-select: values are OR within a filter and AND across filters, and the "N shown" count follows the result. "Group guests" (on by default) lists an external resource under its managed parent host when both match; a guest whose host is filtered out stays top level. Grouping and collapse are display state only and change no count The table CVE cell for an external or reference resource shows non-zero severities only, and `clean` only for a zero total. A marker states provenance by scan identity, not by status: none for an exact selected scan inside its freshness window, `stale` or `age unknown` for an exact selected scan with expired or unclassifiable freshness, `last good` for the retained fallback after the selected scan failed, and `historical` for a scan of another revision, artifact or configuration. Only `last good` and `historical` counts are dimmed. A keyboard- and touch-accessible information control discloses the status, full revision SHA, scan ID and every severity count, and evidence or runtime read errors |
| Dashboard `total_systems`, fleet health, deployment partitions | Visible active managed machines only |
| Dashboard runtime CVE summary | Distinct Current-authorized CVEs per visible active managed machine; zero is not proof of full scan coverage |
| Environment `system_count` | Every registration, including inactive resources and all modes |
| Environment `rollup.active_system_count` and health partition | Active managed machines only |
| Environment flake names | Active registrations in all modes |
| Environment card, table and panel counts ("systems") | Active managed machines only (`rollup.active_system_count`), as in the approved design; registrations of all modes are listed on the Systems page |
| Operational Scanning coverage | Distinct scannable derivations: managed runtime, selected *built* nonmanaged head, recoverable-build, or active-scan populations. A registration whose evaluation or build failed, or whose head is unbuilt, has nothing to scan and is not in the denominator; it appears as an evidence issue on the Systems page instead |
| Admin CVE dashboard (summary, drill-down, top systems, scan freshness) | Active managed machines only. These endpoints report per-host runtime exposure from the hostname-joined legacy view; external and reference registrations are excluded so artifact evidence never becomes affected-system runtime exposure |
| Hardening fleet summary and top vulnerable services | Active managed machines only (migration `0332`). Each machine counts its configuration's latest completed assessment; exact per-configuration assessments stay available on the configuration surfaces and are not guest health |
| Compliance bundle evaluation | CVE and runtime controls use evidence-based evaluation that requires a deployed managed context; an external or reference registration has none, so those controls are *not checked*, never passed |

Migrations `0316` (list and rollup views) and `0317` (detail view) mask
nonmanaged runtime fields and set SQL health, deployment status and pipeline
stage to `unknown`. The Systems API `HealthStatus` has an `unknown` variant
that preserves this value, and the Web UI shows it as a neutral state that
enters neither the online nor the outage filters. Reference configurations are
never counted as offline machines.
Legacy list CVE integers also remain zero-compatible; use configuration-assurance
nullable counts to distinguish missing scans from clean evidence.

## Upgrade and troubleshooting

Apply normal migrations in order; do not edit applied migrations:

| Migration | Change |
| --- | --- |
| `0311_management_modes.sql` | Managed default for existing rows; mode/key/desired-target constraints; optional parent FK |
| `0312_management_reservation_revocation.sql` | Retained `invalidated` reservation state |
| `0314_current_managed_authority.sql` | Replaces Current CVE authority in place with active-managed restriction |
| `0315_guest_observations.sql` | Append-only associated parent evidence and latest index |
| `0316_management_metric_views.sql` | Replaces list/environment/fleet/deployment views in place, preserving dependent column shapes |
| `0317_management_detail_health.sql` | Replaces `view_system_detail` in place so nonmanaged health, deployment status and pipeline stage are `unknown`, never `offline` |
| `0318_agent_enrollment_epoch.sql` | Adds the DB-owned nullable epoch, current-enrollment state projection, and Current/POA&M SQL bindings; existing null epochs retain legacy semantics |
| `0319_system_registration_metadata.sql` | Adds persistent tags and description independently of runtime mode |
| `0320_inactive_managed_detail_observations.sql` | Preserves inactive managed detail telemetry within its enrollment epoch; Current authority remains active-only |
| `0321_current_detail_cve_counts.sql` | Detail CVE counts require current enrollment and exact Current authority, never configuration history |
| `0322_exact_derivation_reuse.sql` | Exact `.drv` reuse (`reused_from_derivation_id`, `reused_build_job_id`), `derivations_share_evidence_scope`, `derivation_publication_eligible`, `view_derivation_execution_evidence`, reuse-aware Current authority |
| `0323_external_reconciliation_reuse_epoch.sql` | External generation reconciliation applies the same alias and enrollment-epoch rules as Current authority |
| `0324_system_pinned_commit.sql` | Pinned commit display metadata; a trigger clears it when mode, policy or flake no longer permit it; it never deploys |
| `0325_current_authority_scan_lookup.sql` | Restores the indexed newest-scan lookup in `view_current_cve_authority` (0322 made it a scan of every completed scan per managed system) without changing its result |
| `0326_evidence_peer_lookup.sql` | `derivation_evidence_peers(T)` replaces `x = T OR derivations_share_evidence_scope(T, x)` in hot paths; evidence view calls the execution-source function only for adopted jobs; restores the `derivation_path` index and indexes `reused_build_job_id` |
| `0327_enrollment_epoch_on_registration_and_rename.sql` | A managed registration or rename that claims a hostname with earlier reports starts a new observation boundary, so those reports are history |
| `0328_static_publication_reuse.sql` | `derivation_publication_eligible` gains a 4-argument overload that also accepts a completed static publication of the exact output whose destination equals the currently effective static cache destination; the 3-argument rule is unchanged |
| `0329_guest_observation_indexes.sql` | Composite index for the current-parent observation read (277 ms to 0.24 ms with 400,000 former-parent rows) and indexes for the `parent_host_id` and `environment_id` foreign keys |
| `0330_derivation_environment_resolution.sql` | `derivation_environment(commit, name)` returns the one owner environment of a derivation or reports ambiguity; build admission, claims, reuse and cache publication share it |
| `0331_derivation_environment_effective_identity.sql` | Replaces the `0330` resolver: registrations count by effective configuration identity (explicit name, else hostname), so an explicit name and a hostname fallback in different environments are ambiguous |
| `0332_hardening_fleet_managed_population.sql` | `view_hardening_fleet_summary` and `view_hardening_top_vulnerable_services` count active managed machines only; columns unchanged |
| `0333_system_revoked_agent_keys.sql` | `system_revoked_agent_keys` records keys revoked by leaving managed; conversion and rotation refuse them |

Existing enrolled rows remain managed; migration does not infer external or
reference resources. Constraint failures on legacy blank enrollment keys require
investigation of those rows, not fake credentials or a disabled constraint.
Migration-backed upgrade execution and old-agent integration are unverified in
this documentation pass.

- **Agentless build stays queued:** confirm the derivation's saved
  `artifactAdmission` result and that an active external/reference registration
  still matches the exact flake and configuration; then check builder session,
  environment filter and source prerequisites as for managed builds.
- **Build refused or cancelled with "more than one environment":** two active
  registrations of the configuration are in different environments. Move or
  remove one so that all share one environment, then retry the build if its
  earlier attempt was cancelled.
- **No selected artifact/scan:** inspect branch snapshot readiness, exact
  configuration discovery, evaluation outcome, build status and cache push.
  Historical evidence is not a substitute for the selected head.
- **Parent edit rejected:** clear child associations first; verify managed parent
  activity, same environment, and the actor's memberships.
- **No observation:** verify the exact safe guest identity/unit and upgraded
  parent heartbeat sequence. Reload cannot request new collection.
- **Nonmanaged resource appears offline:** verify that migrations `0316` and
  `0317` are applied and reload the current UI bundle. Absence of an agent
  heartbeat is not an outage for that mode.
- **Conversion rejected:** initial managed conversion requires new credentials
  and manual policy. Change automated policy in a later explicit edit.

The server owns persistence, authorization and authoritative evaluation. API-only
builders obtain work and report results through authenticated server APIs and do
not access PostgreSQL. The Web UI reads server APIs, not the database. These
boundaries remain unchanged by configuration-only assurance.

## Source inspection and remaining proof

Source inspected: migrations above; server `models/systems.rs`,
`models/deployment_policies.rs`, `models/evaluate_with_policies.rs`,
`handlers/api/systems.rs`, configuration-assurance and guest-observation handlers
and queries, scan selectors, dashboard/environment queries, runtime authority
and POA&M gates; protocol `agent.rs`; agent `guest_observations.rs`; Web UI mode
components, DTOs, forms, adapters and System Detail evidence/runtime views.

The approved TASK-508 presentation is in `docs/design/CrystalForge` at commit
`18b47eed`. Results below name the commit at which each check ran. A result is
not evidence for a later commit.

Verified on the branch (see the [coverage map](../testing/integration-coverage-map.md)
for commands and scope):

- At `4df19a7b`: GitLab pipeline 3025454179 passed all 10 jobs, including the
  four integration owners and `state-machine-tests`, and local `nix flake check`
  passed (Access 40, Assurance 67, Delivery 17, Upgrade 13 cases).
- At `24aff108`, pipeline 3040327484 passed Access, Delivery, Upgrade,
  state-machine, component, dashboard-visibility, CVE-processing and documentation
  jobs. Assurance failed during keyless browser registration: its managed-mode
  locator still expected `Configuration` after the approved rail label changed
  to `Flake assignment`. The structured artifacts identify that failure; the
  truncated CI trace does not. These results do not establish merge readiness.
- Final-head verification after the registration-locator and key-rotation
  concurrency corrections remains pending. The MR description records the
  exact source revision for each completed check. Earlier results are not
  evidence for a later head.
- PostgreSQL regressions added by the finalization run in the existing isolated
  harness selections (`management_mode_`, `system_key_rotation_` and the named
  environment-ownership tests). The key-rotation concurrency regression observes
  the actual PostgreSQL row-lock wait before committing conversion, then checks
  that the revoked key cannot overwrite the fresh enrollment key.

Known limits that remain documented decisions, not hidden defects:

- A configuration registered in more than one environment cannot be built
  (TASK-518). The server refuses it with an explicit reason instead of choosing
  an environment. See [One configuration in several
  environments](#one-configuration-in-several-environments). No complete
  shared-environment build, publication and scan workflow exists, so none is
  claimed.

- Remote-only CVE rescans of an artifact built in an earlier builder session fail
  with `scan_executor_unavailable` after a builder restart (TASK-513). The user
  accepted this limit for TASK-508.
- Static publication reuse (TASK-514) is implemented and decided: see
  [Exact derivation reuse](#exact-derivation-reuse). It is verified by six
  PostgreSQL tests and two unit tests, and by an Assurance case that re-records
  the real publication as static (an injected prerequisite, named in the case)
  and then exercises the real evaluator, scanner and rescan. No real static
  publication exists in that topology, which publishes through a database
  destination.

Not verified: independent design reference/actual/difference images (the
browser comparison was made against the mock in a live preview), and
old-agent integration with an older agent binary. No integration scenario covers an environment move or a
mixed explicit-name and hostname registration; PostgreSQL tests cover them. No fixture observation or simulated Refresh is
production evidence.
