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
`direct`. Configuration synchronization preserves operator metadata. These
fields do not introduce an inherited deployment policy: the existing three
policies remain separate from management mode.

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
write after re-enrollment, even when the enrollment key is reused.
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

Build creation/recovery in `queries/build_jobs.rs` and both peeks and all four
atomic claim variants in `queries/builders.rs` use this admission. Builder
session fencing, environment filters, verified-source prerequisites and cache
preflight remain enforced. Isolated database tests cover actual claims; full
remote evaluation/build/scan integration proof remains pending.

CVE scans reuse the existing derivation runner, cache verification, diagnostics,
and execution fencing. Fleet admission deduplicates derivations. Scheduled CVE
rescans include all modes: selected built external/reference head artifacts use
the existing `deployed_interval` cadence label. That scheduler label is not a
deployment claim. Recent/archived artifact cadences and post-build recovery
remain separate. Active scans and existing failure backoff still govern retries.
Hardening target selection also uses the exact selected nonmanaged branch head.
The Web UI offers its exact derivation/build history without requiring a CVE
scan. Hardening reads return exact-derivation evidence, not Current runtime
authority. CVE scans and hardening assessments retain independent availability
and diagnostics.
See [scan operations](../api/cve-scan-operations-api.md) for denominator details.

Current exact-CVE authority requires an active **managed** registration, its
latest consistent reported generation/store path, exactly one matching
registered derivation, and that derivation's newest completed schema-1 scan.
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
| Systems list | Active registrations across all modes; mode filter applies before totals/paging |
| Dashboard `total_systems`, fleet health, deployment partitions | Visible active managed machines only |
| Dashboard runtime CVE summary | Distinct Current-authorized CVEs per visible active managed machine; zero is not proof of full scan coverage |
| Environment `system_count` | Every registration, including inactive resources and all modes |
| Environment `rollup.active_system_count` and health partition | Active managed machines only |
| Environment flake names | Active registrations in all modes |
| Operational Scanning coverage | Distinct derivations in managed runtime, selected built nonmanaged head, recoverable-build, or active-scan populations |
| Hardening runtime fleet/Current evidence | Managed runtime population; exact configuration assessments are not guest health |

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

Existing enrolled rows remain managed; migration does not infer external or
reference resources. Constraint failures on legacy blank enrollment keys require
investigation of those rows, not fake credentials or a disabled constraint.
Migration-backed upgrade execution and old-agent integration are unverified in
this documentation pass.

- **Agentless build stays queued:** confirm the derivation's saved
  `artifactAdmission` result and that an active external/reference registration
  still matches the exact flake and configuration; then check builder session,
  environment filter and source prerequisites as for managed builds.
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
These are worktree observations, not immutable committed provenance.

The approved TASK-508 presentation is in `docs/design/CrystalForge` at commit
`18b47eed`. The owner reports the following results; this documentation pass did
not rerun application tests:

- Disposable SQLx database tests pass for CVE/POA&M enrollment authority,
  stale-signed state races, wall-clock stamping after row-lock acquisition,
  current Config reads/drift repair, and inactive managed health. Metadata API
  and Native declaration database tests also pass.
- Native frontend tests report 808 passed and one ignored on the latest owner
  run. This does not prove WASM browser behavior or visual parity.
- R3 remote managed-agent cases pass in Delivery and Assurance. Access keyless
  registration and denied cases pass. Delivery's new case failed because
  `cf-keygen` was absent from the target PATH; the fragment now uses builderA's
  existing tool, but its rerun remains pending.
- Assurance browser registration failed with `NoFullOutputSnapshot`. The Native
  Systems-facet fix is present and has database/pure-UI proof; the VM rerun
  remains pending. Earlier owner runs failed on a stale SQL assertion and the
  module loopback default; source fixes do not prove the final owner run.
- The second normal state-machine full run still failed later at
  `required_inventory_exact_owner_success_and_conflict` with
  `EvaluationSuperseded`. Twenty targeted diagnostic runs passed. The full-run
  failure is not classified as fixed.

Full exact-source integration-access, integration-delivery,
integration-assurance and upgrade results, final design comparison, dark/light
and empty/error-state parity, and final SQLx/source alignment remain pending.
See the [coverage map](../testing/integration-coverage-map.md) for proof scope.
No fixture observation or simulated Refresh is production evidence.
