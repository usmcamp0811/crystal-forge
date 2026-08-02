---
id: TASK-415
title: Implement deployment approvals, signed running-state attestations, and attention UX
status: In Progress
assignee: []
created_date: '2026-08-01 10:07'
updated_date: '2026-08-01 10:07'
labels:

- design
- backend
- frontend
- web-ui
- api
- database
- deployment
- approvals
- attestation
- agent
- security
- attention
- notifications
- testing
  dependencies:
- TASK-412
- TASK-414
  references:
- 'design commit: 17d8ffe18e954824831543dc5b5684c5de4d30b9'
- docs/design/CrystalForge/app.jsx
- docs/design/CrystalForge/components/DashboardView.jsx
- docs/design/CrystalForge/components/EnvironmentsView.jsx
- docs/design/CrystalForge/components/Shell.jsx
- docs/design/CrystalForge/components/SystemDetail.jsx
- docs/design/CrystalForge/components/Systems.jsx
- docs/design/CrystalForge/data-attestations.js
- docs/design/CrystalForge/data-dashboard.js
- docs/design/CrystalForge/docs/alerts-and-notifications.md
- docs/design/CrystalForge/styles.css
- packages/default/crates/cf-server/src/models/deployment_policies.rs
- packages/default/crates/cf-server/src/queries/attention.rs
- packages/default/crates/cf-protocol/src/agent.rs
- packages/default/crates/cf-agent/src/system_state.rs
- packages/web-ui/src/views/dashboard.rs
- packages/web-ui/src/views/environments.rs
- packages/web-ui/src/views/environments_list.rs
- packages/web-ui/src/views/system_detail.rs
- packages/web-ui/src/views/systems.rs
- packages/web-ui/src/views/systems_list.rs
- packages/web-ui/src/components/layout/
- packages/web-ui/src/components/notifications/
  modified_files:
- migrations/
- packages/default/crates/cf-protocol/src/
- packages/default/crates/cf-agent/src/
- packages/default/crates/cf-server/src/api/
- packages/default/crates/cf-server/src/models/
- packages/default/crates/cf-server/src/queries/
- packages/default/crates/cf-server/src/tasks/
- packages/web-ui/src/components/dashboard/
- packages/web-ui/src/components/layout/
- packages/web-ui/src/components/notifications/
- packages/web-ui/src/components/system/
- packages/web-ui/src/views/
- docs/
  priority: high
  type: feature
  ordinal: 415000

---

<!-- SECTION:DESCRIPTION:BEGIN -->

## Description

Implement the deployment-approval and running-state attestation changes shown in design commit `17d8ffe18e954824831543dc5b5684c5de4d30b9`.

The design commit is a UI prototype. It uses local JavaScript arrays and client-side state for approval requests, approval decisions, attestation records, trust classifications, and notification events. Do not copy those implementation details into production code.

Implement the same product behavior with:

- Durable PostgreSQL records.
- Server-side authorization and validation.
- Exact deployment-target binding.
- Signed agent attestations.
- Immutable audit records.
- Existing Crystal Forge attention infrastructure.
- TASK-414 account notifications.
- Rust API types in `cf-protocol`.
- Dioxus components in the production web UI.
- Transaction-safe approval and resolution operations.

This task does not add a separate Attestations page. Approval and attestation work must appear in the existing Systems, System Detail, Environments, Dashboard, sidebar, and notification interfaces.

## Product outcome

After this task:

1. A deployment that requires approval cannot start until the required approvals exist.
2. Each approval applies to one exact system, target artifact, policy version, and authorization request.
3. Crystal Forge can determine whether the artifact currently running on a system was authorized.
4. The agent signs periodic running-state attestations with its enrolled identity key.
5. The server verifies and classifies each attestation.
6. Operators can review suspicious running state from the System Detail Deploy tab.
7. Dashboard and environment views show pending approvals and attestation trust conditions.
8. The Systems sidebar badge remains a live unresolved-condition count.
9. Notifications remain a chronological event log.
10. All decisions have durable actor, timestamp, target, reason, and audit-note records.

## Design changes covered by this task

Implement these changes from the design commit.

### Systems view

- Add a `Needs attention` system filter.
- Make the `N needing attention` summary interactive.
- Show a pending-approval chip on a system card or row.
- Open the selected system on the Deploy tab when the user selects the approval chip.
- Support deep links that open System Detail on a specific tab.
- Preserve all current search, status, environment, selection, and pagination behavior.

### System Detail Deploy tab

- Show a pending deployment-approval banner.
- Show the requested target, requester, policy, approval progress, and request time.
- Add a Review action.
- Add an approval-review modal.
- Allow authorized users to approve or reject.
- Allow an optional audit note.
- Show existing approvals and approver identities.
- Show a running-state trust banner when the current state needs a decision.
- Add a Resolve action for suspicious running state.
- Add resolution choices for:
  - Adopt as authorized.
  - Replace with an authorized build.
  - Mark as under investigation.

- Require an audit note where this task specifies one.
- Do not treat `under investigation` as resolution of the underlying condition.

### Environments view

- Replace the existing `Auto-sync off` summary statistic with `Awaiting approval`.
- Show pending approval counts in environment rows and cards.
- Show pending approval requests in the environment detail panel.
- Open the related system on the Deploy tab when the user selects a request.
- Use server-provided aggregate counts. Do not fetch each system separately.

### Dashboard

Add these widgets:

- `Deploy Approvals`
- `Attestation Trust`

The Deploy Approvals widget must show:

- Total pending requests.
- Requests waiting for more than one hour.
- Requests that require more than one approver.
- Requests that have at least one approval but are not complete.
- A Review action that opens the relevant Systems results.

The Attestation Trust widget must show:

- Unresolved flagged systems.
- Systems with current authorized state.
- Systems with stale evidence.
- A Review action that opens the relevant Systems results.

Add both widgets to:

- The default dashboard layout.
- Existing saved dashboard layouts that do not contain them.

Do not duplicate a widget when a saved layout already contains it.

### Shell, sidebar, and notifications

- Make the sidebar navigation region scrollable.
- Keep the profile/account area pinned at the bottom.
- Remove the top-bar Tweaks action only after its required functions remain accessible through the existing account or settings interface.
- Include pending approvals in the Systems sidebar attention count.
- Include unresolved flagged attestations in the Systems sidebar attention count.
- Preserve critical and offline system counts.
- Show a tooltip that explains each component of the total.
- Produce chronological notification events for approval and attestation events.
- Use TASK-414 notification storage and delivery.
- Do not create hard-coded sample notifications.
- Do not generate notification events during UI rendering.

## Important product distinction

The attention count and notification list have different purposes.

### Attention

Attention represents a current unresolved condition.

Examples:

- A system is offline.
- A system is critical.
- A deployment request still needs approval.
- A system reports an unauthorized artifact.
- An agent identity cannot be verified.

Attention remains open until the underlying condition changes or an authorized resolution action closes it.

A per-user dismissal hides an attention occurrence for that user. A dismissal does not change the underlying deployment, approval, attestation, or investigation state.

### Notifications

Notifications represent discrete events in time.

Examples:

- A deployment approval was requested.
- A user approved a deployment.
- A user rejected a deployment.
- An approval request expired.
- A system entered an unauthorized-artifact state.
- An operator adopted an observed artifact.
- An operator requested replacement of an observed artifact.

A condition can create both:

- An open attention occurrence.
- One chronological notification event.

Do not repeatedly create notifications while the condition remains open.

## Goals

- Enforce deployment approval policies at deployment time.
- Bind every approval to an exact immutable deployment target.
- Support one or more distinct approvers.
- Support approver-role restrictions.
- Support approval expiration.
- Prevent stale approvals from authorizing a changed target.
- Record approval and rejection decisions.
- Create immutable deployment authorizations.
- Collect signed running-state attestations from agents.
- Verify agent identity and signature.
- detect replayed or out-of-order attestations.
- Compare observed running state with Crystal Forge authorization records.
- Classify running state with stable reason codes.
- Expose approval and trust information through aggregate APIs.
- Integrate with existing attention reconciliation.
- Integrate with TASK-414 notifications.
- Implement all design surfaces without client-side mock data.
- Add tests for concurrency, authorization, signatures, state transitions, and UI routing.

## Out of scope

Do not include these items:

- A separate top-level Attestations view.
- TPM quotes.
- Secure Boot measurement.
- IMA measurement.
- Full remote measured-boot attestation.
- A new SBOM system.
- A new binary-cache trust model.
- A new general notification framework.
- Replacement of TASK-414 notifications.
- Automatic trust of unknown artifacts.
- Browser-side signature verification as the authoritative check.
- Browser-side trust classification.
- A rewrite of the deployment scheduler.
- A rewrite of all deployment-policy types.
- A mobile application.
- Automatic approval based only on a user viewing a request.
- Treating a dismissed alert as an approved deployment.
- Treating `under investigation` as a resolved trust condition.

## Dependencies

### TASK-412

TASK-412 defines versioned deployment and compliance policy behavior.

This task must reference the exact policy version used when Crystal Forge creates an approval request. The approval request must retain that policy snapshot after a newer policy version exists.

Do not bind an approval only to a mutable policy name.

### TASK-414

TASK-414 implements persistent account notifications and sessions.

This task must:

- Add approval and attestation notification producers.
- Use existing notification recipient, read, and dismissal behavior.
- Use stable idempotency keys.
- Add deep-link metadata.
- Not create a second notification subsystem.

If TASK-414 changes the final table or API names, use its final interfaces.

## Existing systems to extend

Use these existing systems before creating new equivalents:

- `DeploymentPolicy::RequireApprovals`
- `ApprovalConfig`
- Agent enrollment and signing keys.
- Agent request-signature helpers.
- System state and heartbeat processing.
- Deployment execution records.
- `attention_occurrences`
- Per-user attention dismissals.
- Existing environment and dashboard aggregate queries.
- Existing Dioxus modal, banner, chip, card, table, and routing components.
- TASK-414 notification records and delivery.

Do not create a second deployment-policy parser.

Do not create a second user or role model.

Do not create a second system-health alert table.

## Source-of-truth rules

The following rules are mandatory.

1. PostgreSQL is the source of truth for approval requests and decisions.
2. The server is the source of truth for approval status.
3. The server is the source of truth for attestation verification.
4. The server is the source of truth for trust classification.
5. The agent reports observed state. The agent does not decide whether that state is authorized.
6. The browser does not supply authoritative actor IDs.
7. The browser does not supply authoritative approval counts.
8. The browser does not supply authoritative trust classifications.
9. An approval is valid only for the exact target stored in the request.
10. Changing the desired target invalidates or supersedes the old request.
11. An attestation row is immutable after insertion.
12. A resolution action does not modify the original signed attestation.
13. `Investigate` keeps the trust attention occurrence open.
14. A notification event does not replace an attention occurrence.
15. A user dismissal does not alter shared system state.

## Domain model

### Deployment approval request

A deployment approval request represents permission to deploy one exact target to one exact system.

A request must contain:

- Request ID.
- System ID.
- Environment ID at request time.
- Target store path.
- Target derivation path when available.
- Target commit ID or commit hash when available.
- Flake ID when available.
- Deployment policy ID.
- Deployment policy version ID.
- Requester type.
- Requester user ID when a user created the request.
- Requester automation identifier when automation created the request.
- Request timestamp.
- Required approval count.
- Required role.
- Whether approvers must be distinct.
- Whether the requester may approve.
- Expiration timestamp.
- Current status.
- Superseded request ID where applicable.
- Deployment authorization ID after approval.
- Created and updated timestamps.

Persist a snapshot of the approval requirements on the request.

Do not recalculate the required count from a newer mutable policy when the request is reviewed.

### Approval request statuses

Use one enum or constrained text field with these values:

- `pending`
- `approved`
- `rejected`
- `expired`
- `cancelled`
- `superseded`
- `consumed`

Definitions:

#### pending

The request can accept decisions.

#### approved

The required approval count exists. Crystal Forge issued an immutable deployment authorization.

#### rejected

An authorized approver rejected the request. The request cannot receive more approvals.

#### expired

The configured expiration time passed before approval completed.

#### cancelled

The requester or an authorized administrator cancelled the request.

#### superseded

The system target, policy version, or relevant deployment request changed.

#### consumed

A deployment execution used the authorization for this request.

### Approval decision

Each decision must contain:

- Decision ID.
- Request ID.
- Actor user ID.
- Decision type.
- Optional note.
- Actor role snapshot.
- Decision timestamp.
- Request target digest or fingerprint.
- Request status before the decision.
- Request status after the decision.

Decision types:

- `approve`
- `reject`

Add a unique constraint that prevents the same user from approving the same request more than once.

A repeated identical API request must return the existing result. It must not create a duplicate decision.

### Requester self-approval

Default behavior:

- The requester does not count as an approver.

Only permit requester approval when the stored policy snapshot explicitly enables it.

Do not infer this permission from a broad administrator role.

### Distinct approvers

When `distinct = true`:

- Count each user at most once.
- Require the configured number of different user IDs.
- Reject duplicate decisions from the same user.
- Keep the first durable decision.
- Return an idempotent success response for a repeated identical decision.

### Rejection behavior

One valid rejection makes the request terminal.

After rejection:

- Do not expose the target as deployable.
- Do not create a deployment authorization.
- Resolve the pending-approval attention occurrence.
- Create a rejection notification for the requester.
- Preserve all prior approvals and the rejection record.

### Approval expiration

A request expires at its stored `expires_at` value.

Implement expiration in both places:

- Synchronous validation during every read or decision operation.
- A periodic reconciliation task that updates expired requests and related attention.

An expired request cannot become approved.

The expiration operation must be idempotent.

### Supersession

Supersede a pending request when any of these values change:

- System ID.
- Target store path.
- Target derivation.
- Target commit.
- Policy version.
- Required approval configuration.

The new request must have a new request ID.

Do not transfer approvals from the old request to the new request.

### Deployment authorization

An approved request must create one immutable deployment authorization.

The authorization must contain:

- Authorization ID.
- System ID.
- Exact target store path.
- Target derivation path when available.
- Target commit ID when available.
- Policy version ID.
- Source approval request ID.
- Issued timestamp.
- Issued-by actor or automation.
- Valid-from timestamp.
- Expiration timestamp when applicable.
- Revocation timestamp when applicable.
- Consumption timestamp when applicable.
- Deployment execution ID when consumed.

The deployment agent may receive the target only when a valid authorization permits that exact target.

Do not authorize a different target because it has the same commit, flake, or display name.

## Approval state transition rules

Implement transitions in one server-side service or query module.

Allowed transitions:

```text
pending -> approved
pending -> rejected
pending -> expired
pending -> cancelled
pending -> superseded
approved -> consumed
approved -> cancelled
approved -> superseded
```

Do not allow other transitions.

In particular:

```text
rejected -> approved
expired -> approved
cancelled -> approved
superseded -> approved
consumed -> pending
```

must fail.

Use a database transaction and row lock for every transition.

The final approval and deployment-authorization creation must occur in one transaction.

Two concurrent final approvals must create:

- One final request transition.
- One deployment authorization.
- One approval-completed event.
- One resolved attention occurrence.

## Approval request creation

When a deployment target is selected:

1. Resolve the exact system and target.
2. Evaluate deployment-time policies.
3. Determine whether `RequireApprovals` applies.
4. Persist the policy-version snapshot.
5. Search for an active request with the same exact request fingerprint.
6. Reuse the active request when all immutable inputs are equal.
7. Supersede incompatible active requests for the same system.
8. Create a pending request when approval is required.
9. Open or observe one pending-approval attention occurrence.
10. Create one approval-requested notification event.
11. Do not expose the target to the agent as deployable.
12. Return the pending approval summary to the caller.

When approval is not required:

1. Create the normal deployment authorization.
2. Continue through the existing deployment path.
3. Do not create a pending approval request.

## Running-state attestation

A running-state attestation is a signed statement from one enrolled agent about the system state that the agent currently observes.

Use a dedicated protocol DTO.

Do not treat the complete ordinary heartbeat body as the signed attestation contract.

### Attestation payload

The canonical signed payload must contain:

- Protocol version.
- Attestation ID.
- System ID or enrolled agent identity.
- Agent key ID.
- Agent session ID when available.
- Boot ID.
- Boot timestamp when available.
- Observation timestamp.
- Monotonic attestation counter.
- Current system store path.
- Current system NAR hash when available.
- Current system profile target.
- Booted generation number when available.
- Kernel version.
- Nix version.
- Agent version.
- Agent build hash.
- Deployment authorization ID when known.
- Deployment execution ID when known.
- Activation source when known.
- Payload digest.

The request envelope must contain:

- Canonical payload.
- Agent signature.
- Key ID.
- Signature algorithm version.

Use the existing enrolled Ed25519 agent identity unless the current agent-signing implementation uses a newer approved mechanism.

Do not transmit the private key.

### Canonical encoding

Define one deterministic canonical encoding.

The signer and verifier must use the same bytes.

Do not sign non-canonical JSON generated by an unordered map.

Acceptable implementations include:

- A fixed field-order binary encoding.
- A fixed field-order JSON serializer with no optional-field ambiguity.
- An existing canonical serialization helper already used by Crystal Forge.

Add protocol-version tests with fixed signing vectors.

### Agent attestation cadence

The agent must send an attestation:

- After agent startup and enrollment.
- After a boot ID change.
- After a current-system store-path change.
- After a deployment activation result.
- On a configurable periodic interval.

Use these defaults:

- Attestation interval: 6 hours.
- Evidence freshness interval: 12 hours.

Validate that the configured attestation interval is shorter than the configured freshness interval.

Do not create an attestation on every normal heartbeat unless the agent configuration explicitly sets that interval.

### Replay protection

The server must reject or mark invalid:

- A repeated attestation ID.
- A repeated monotonic counter for the same agent identity and boot session.
- A lower counter than the latest accepted counter for the same boot session.
- A timestamp outside the configured allowed clock-skew window.
- A signature from an unknown key.
- A signature from a revoked key.
- A signature that does not match the canonical payload.
- A payload whose digest does not match its canonical bytes.
- A system identity that does not match the enrolled key.
- A session ID that is invalid or superseded when session validation applies.

Use a database uniqueness constraint where possible.

Do not rely only on an in-memory counter cache.

### Attestation immutability

After insertion, do not update:

- Signed payload.
- Signature.
- Agent key ID.
- Observation time.
- Counter.
- Store path.
- Digest.
- Verification result.

Store later classification and operator actions in separate tables.

## Attestation verification state

Use a stable enum or constrained field with these values:

- `verified`
- `invalid_signature`
- `unknown_key`
- `revoked_key`
- `identity_mismatch`
- `invalid_session`
- `replay`
- `stale_timestamp`
- `malformed`

Store a safe reason code.

Do not store private key material.

Do not include full signatures in ordinary logs.

## Running-state trust classifications

Support these classifications from the design:

- `authorized_current`
- `authorized_but_evidence_stale`
- `authorized_previous_generation`
- `deployment_pending_reboot`
- `activation_failed`
- `unauthorized_artifact`
- `unknown_artifact`
- `agent_attestation_stale`
- `agent_identity_invalid`

Use stable machine-readable values and separate display labels.

### Classification rules

Apply the following precedence.

#### agent_identity_invalid

Use this classification when the latest relevant attestation cannot establish a valid enrolled agent identity.

Examples:

- Invalid signature.
- Unknown key.
- Revoked key.
- Key and system mismatch.
- Invalid or superseded session.
- Replay attack.

This classification creates an open flagged-attestation attention occurrence.

#### activation_failed

Use this classification when the latest authorized deployment execution failed and the observed system has not reached the authorized target.

Use the existing deployment failure attention and notification path where possible.

Do not include this value in the Attestation Trust widget's `flagged artifacts` number unless the product already counts the same occurrence there. Avoid double counting.

#### deployment_pending_reboot

Use this classification when:

- The authorized system profile points to the approved target.
- The currently booted generation does not match that target.
- The deployment result indicates that activation completed but reboot is required.

#### authorized_current

Use this classification when:

- The signature and identity are valid.
- The observed current-system store path matches a valid authorization for this system.
- The observed state represents the current expected target.
- The evidence is within the freshness interval.

#### authorized_previous_generation

Use this classification when:

- The observed artifact is known.
- Crystal Forge previously authorized it for this system.
- A newer authorized generation is expected.
- The system has not reported an unknown or unauthorized artifact.

#### unauthorized_artifact

Use this classification when:

- The observed store path is known to Crystal Forge.
- Crystal Forge has build, evaluation, or artifact metadata for it.
- No valid deployment authorization permits that artifact on this system.

This classification creates an open flagged-attestation attention occurrence.

#### unknown_artifact

Use this classification when:

- The observed store path is not present in authoritative Crystal Forge artifact records.
- No valid deployment authorization permits it for the system.

This classification creates an open flagged-attestation attention occurrence.

#### authorized_but_evidence_stale

Use this projected current state when:

- The last verified state was authorized.
- No newer verified attestation exists within the freshness interval.

Do not rewrite the old attestation classification.

#### agent_attestation_stale

Use this projected current state when:

- No usable attestation exists within the freshness interval.
- The last usable record cannot support `authorized_but_evidence_stale`.

### Classification implementation

Store two concepts separately:

1. The immutable assessment of an individual attestation.
2. The current projected trust state for a system.

The current projected state can change because time passes, even when no new attestation arrives.

Implement periodic stale-state reconciliation.

Reconcile when:

- A new attestation arrives.
- A deployment authorization is issued.
- A deployment starts.
- A deployment succeeds.
- A deployment fails.
- An authorization is revoked.
- A target changes.
- The stale-state timer runs.

## Attestation resolution actions

Support these operator actions.

### Adopt as authorized

This action creates a deliberate authorization for the exact observed artifact.

Requirements:

- Administrator authorization.
- A required audit note.
- Exact system binding.
- Exact store-path binding.
- Existing known artifact metadata when available.
- A clear warning when the artifact is unknown.
- A new immutable authorization record.
- A new resolution-action record.
- Reclassification after the transaction commits.
- A notification event.

Do not:

- Modify the original attestation.
- Mark every copy of the artifact as trusted.
- Authorize the artifact for another system.
- Auto-adopt because an administrator opened the modal.
- Hide a failed signature through adoption.

An identity-invalid attestation cannot be adopted until Crystal Forge has a verified agent identity and a trustworthy observed store path.

### Replace with an authorized build

This action starts the normal deployment-request path for a known authorized target.

Requirements:

- Select the current desired or latest authorized target according to existing deployment policy.
- Run all normal deployment gates.
- Create a new approval request when approval is required.
- Create a normal deployment authorization when approval is not required.
- Record the operator and note.
- Keep the trust condition open until a new verified attestation reports an authorized state.

Do not mark the condition resolved immediately after the user selects Replace.

### Mark as under investigation

This action creates or updates an investigation case.

Requirements:

- A required audit note.
- Actor ID.
- Created timestamp.
- Optional owner.
- Optional follow-up state.
- Durable case status.

The trust attention occurrence must remain open.

The UI must show `Under investigation`.

The sidebar count must continue to include the condition.

### Investigation resolution

Provide a server-side way to close an investigation only when an authorized operator supplies:

- A resolution reason.
- An audit note.
- A final disposition.

A later verified authorized attestation can also resolve the underlying trust occurrence. Preserve the investigation history.

## Database changes

Create additive migrations.

Use the next available migration numbers after rebasing on `dev`.

Do not edit an already-applied migration.

### Table: deployment_approval_requests

Required columns:

```text
id UUID PRIMARY KEY
system_id UUID NOT NULL
environment_id UUID NULL
target_store_path TEXT NOT NULL
target_derivation_path TEXT NULL
target_commit_id UUID NULL
target_commit_hash TEXT NULL
flake_id UUID NULL
deployment_policy_id UUID NULL
deployment_policy_version_id UUID NULL
requester_kind TEXT NOT NULL
requested_by_user_id UUID NULL
requested_by_automation TEXT NULL
requested_at TIMESTAMPTZ NOT NULL
required_approvals INTEGER NOT NULL
required_role TEXT NULL
distinct_approvers BOOLEAN NOT NULL
requester_may_approve BOOLEAN NOT NULL DEFAULT FALSE
expires_at TIMESTAMPTZ NULL
status TEXT NOT NULL
request_fingerprint TEXT NOT NULL
deployment_authorization_id UUID NULL
superseded_by_id UUID NULL
decided_at TIMESTAMPTZ NULL
created_at TIMESTAMPTZ NOT NULL
updated_at TIMESTAMPTZ NOT NULL
```

Add constraints:

- `required_approvals > 0`
- Valid status values.
- Valid requester kinds.
- A user requester requires `requested_by_user_id`.
- An automation requester requires `requested_by_automation`.
- `superseded_by_id` cannot equal `id`.
- Approved requests require `deployment_authorization_id`.
- Terminal timestamps must be internally consistent.

Add indexes:

- Pending by system.
- Pending by environment.
- Pending by requested time.
- Pending by expiration time.
- Request fingerprint.
- Policy version.
- Deployment authorization.

Add a partial unique index or equivalent transaction rule that prevents two active requests with the same request fingerprint.

### Table: deployment_approval_decisions

Required columns:

```text
id UUID PRIMARY KEY
request_id UUID NOT NULL
actor_user_id UUID NOT NULL
decision TEXT NOT NULL
note TEXT NULL
actor_role_snapshot TEXT NULL
request_fingerprint TEXT NOT NULL
status_before TEXT NOT NULL
status_after TEXT NOT NULL
created_at TIMESTAMPTZ NOT NULL
```

Add:

- Foreign keys.
- Valid decision constraint.
- Unique `(request_id, actor_user_id)`.
- Index by request and timestamp.

### Table: deployment_authorizations

Create this table only if TASK-412 does not already provide an equivalent durable model.

Required information:

```text
id UUID PRIMARY KEY
system_id UUID NOT NULL
target_store_path TEXT NOT NULL
target_derivation_path TEXT NULL
target_commit_id UUID NULL
policy_version_id UUID NULL
source_approval_request_id UUID NULL
authorization_source TEXT NOT NULL
issued_by_user_id UUID NULL
issued_by_automation TEXT NULL
issued_at TIMESTAMPTZ NOT NULL
valid_from TIMESTAMPTZ NOT NULL
expires_at TIMESTAMPTZ NULL
revoked_at TIMESTAMPTZ NULL
consumed_at TIMESTAMPTZ NULL
deployment_execution_id UUID NULL
created_at TIMESTAMPTZ NOT NULL
```

Add indexes for:

- System and target.
- Active authorizations.
- Approval request.
- Deployment execution.

### Table: running_state_attestations

Required columns:

```text
id UUID PRIMARY KEY
attestation_id UUID NOT NULL
system_id UUID NOT NULL
agent_key_id UUID NOT NULL
agent_session_id UUID NULL
protocol_version INTEGER NOT NULL
boot_id TEXT NOT NULL
boot_timestamp TIMESTAMPTZ NULL
observed_at TIMESTAMPTZ NOT NULL
received_at TIMESTAMPTZ NOT NULL
monotonic_counter BIGINT NOT NULL
current_system_store_path TEXT NOT NULL
current_system_nar_hash TEXT NULL
system_profile_store_path TEXT NULL
booted_generation BIGINT NULL
kernel_version TEXT NULL
nix_version TEXT NULL
agent_version TEXT NOT NULL
agent_build_hash TEXT NULL
reported_authorization_id UUID NULL
reported_execution_id UUID NULL
activation_source TEXT NULL
canonical_payload BYTEA NOT NULL
payload_digest BYTEA NOT NULL
signature BYTEA NOT NULL
verification_status TEXT NOT NULL
verification_reason_code TEXT NULL
created_at TIMESTAMPTZ NOT NULL
```

Add:

- Unique `attestation_id`.
- Unique `(agent_key_id, boot_id, monotonic_counter)`.
- Index `(system_id, observed_at DESC)`.
- Index `(system_id, received_at DESC)`.
- Index by verification status.
- Payload-size constraints where practical.

Do not add an update path for signed fields.

### Table: running_state_attestation_assessments

Required columns:

```text
id UUID PRIMARY KEY
attestation_id UUID NOT NULL
system_id UUID NOT NULL
classification TEXT NOT NULL
reason_code TEXT NOT NULL
matched_authorization_id UUID NULL
matched_deployment_execution_id UUID NULL
matched_artifact_id UUID NULL
classifier_version INTEGER NOT NULL
assessed_at TIMESTAMPTZ NOT NULL
created_at TIMESTAMPTZ NOT NULL
```

Add:

- Unique `attestation_id`.
- Valid classification constraint.
- Index by system and classification.
- Index by classification and assessment time.

### Table: attestation_investigations

Required columns:

```text
id UUID PRIMARY KEY
system_id UUID NOT NULL
source_attestation_id UUID NOT NULL
status TEXT NOT NULL
opened_by_user_id UUID NOT NULL
owner_user_id UUID NULL
opening_note TEXT NOT NULL
opened_at TIMESTAMPTZ NOT NULL
resolved_by_user_id UUID NULL
resolution_reason TEXT NULL
resolution_note TEXT NULL
resolved_at TIMESTAMPTZ NULL
created_at TIMESTAMPTZ NOT NULL
updated_at TIMESTAMPTZ NOT NULL
```

Allow at most one open investigation for the same system and trust episode.

### Table: attestation_resolution_actions

Required columns:

```text
id UUID PRIMARY KEY
system_id UUID NOT NULL
attestation_id UUID NOT NULL
actor_user_id UUID NOT NULL
action TEXT NOT NULL
note TEXT NOT NULL
created_authorization_id UUID NULL
created_deployment_request_id UUID NULL
investigation_id UUID NULL
created_at TIMESTAMPTZ NOT NULL
```

Valid actions:

- `adopt`
- `replace`
- `investigate`
- `close_investigation`

## Request fingerprint

Calculate the approval request fingerprint on the server.

Include:

- System ID.
- Target store path.
- Target derivation path.
- Target commit ID or stable commit hash.
- Policy version ID.
- Required approval count.
- Required role.
- Distinct-approver flag.
- Requester-self-approval flag.
- Expiration policy inputs.

Use a deterministic encoding and digest.

Do not include mutable display text.

## Server implementation

Create cohesive modules. Do not put all logic in HTTP handlers.

Suggested module responsibilities:

```text
models/deployment_approvals.rs
    Domain enums and API-safe models.

queries/deployment_approvals.rs
    Request creation, locking, decisions, expiration, supersession,
    authorization issuance, and list queries.

models/running_state_attestations.rs
    Verification and classification models.

queries/running_state_attestations.rs
    Immutable insert, replay checks, latest state, assessments,
    investigations, and resolution actions.

services/attestation_verification.rs
    Canonical payload verification and identity checks.

services/running_state_classification.rs
    Deterministic classification rules.

tasks/approval_expiration.rs
    Periodic expiration reconciliation.

tasks/attestation_freshness.rs
    Periodic stale-state and attention reconciliation.
```

Use existing repository naming and module layout when a direct equivalent exists.

### Transaction boundaries

Use one transaction for each operation:

- Create or reuse approval request.
- Approve request.
- Reject request.
- Expire request.
- Supersede request.
- Issue authorization.
- Consume authorization.
- Insert and assess attestation.
- Adopt observed artifact.
- Request replacement.
- Open investigation.
- Close investigation.

Use row locks or transaction-scoped advisory locks where competing operations can affect the same system or request.

Use a consistent lock order:

1. System-level deployment or trust lock.
2. Approval request row.
3. Deployment authorization row.
4. Attention occurrence rows.
5. Notification idempotency row or producer state.

Document any deviation.

### Error responses

Return stable error codes.

Approval errors:

- `approval_request_not_found`
- `approval_request_not_pending`
- `approval_request_expired`
- `approval_request_superseded`
- `approval_request_rejected`
- `approval_duplicate_actor`
- `approval_role_required`
- `approval_requester_not_allowed`
- `approval_target_mismatch`
- `approval_conflict`

Attestation errors:

- `attestation_malformed`
- `attestation_unknown_key`
- `attestation_revoked_key`
- `attestation_invalid_signature`
- `attestation_identity_mismatch`
- `attestation_invalid_session`
- `attestation_replay`
- `attestation_stale_timestamp`
- `attestation_counter_conflict`
- `attestation_resolution_not_allowed`

Do not return raw database errors to clients.

## Protocol and agent implementation

### cf-protocol

Add shared DTOs for:

- Attestation payload.
- Signed attestation envelope.
- Attestation receipt response.
- Deployment authorization reference where needed.
- Stable enum serialization.

Keep protocol DTOs independent from database models.

Do not add server query types to `cf-protocol`.

### cf-agent

Add a running-state attestation producer.

The producer must:

1. Read the current system store path.
2. Read the system-profile target.
3. Read boot ID.
4. Read the booted generation when available.
5. Read kernel, Nix, agent version, and build hash.
6. Load the current agent key.
7. Load and atomically advance the monotonic counter.
8. Build the canonical payload.
9. Calculate the payload digest.
10. Sign the canonical payload.
11. Send the signed envelope.
12. Persist the accepted counter state.
13. Retry transient failures with bounded backoff.
14. Not reuse an attestation ID.
15. Not reset the counter during an ordinary agent restart in the same boot.

Store counter state in a durable agent state location with appropriate file permissions.

Use atomic file replacement or an existing durable state helper.

Do not allow concurrent attestation jobs to emit the same counter.

### Trigger integration

Trigger attestation sending from:

- Successful startup.
- Boot change detection.
- Running store-path change detection.
- Deployment completion.
- Deployment failure where observed state can still be reported.
- Periodic timer.

Coalesce duplicate triggers.

Do not block normal heartbeat processing for the full network retry period.

### Server receipt

The server endpoint must:

1. Apply request-size limits.
2. Parse the envelope.
3. Resolve the enrolled key.
4. Verify system and session identity.
5. Calculate canonical bytes.
6. Verify the payload digest.
7. Verify the signature.
8. Check timestamp limits.
9. Check replay constraints.
10. Insert the immutable row.
11. Calculate the assessment.
12. Reconcile attention.
13. Commit.
14. Produce notifications after durable state exists.
15. Return the stored attestation ID and classification.

## API requirements

Follow existing API version and route conventions. Use these route semantics even if the final path prefix differs.

### List approval requests

```text
GET /api/v1/deployment-approvals
```

Filters:

- `status`
- `system_id`
- `environment_id`
- `requested_before`
- `requested_after`
- `requires_multiple_approvers`
- `partially_approved`
- Pagination cursor or repository-standard pagination.

Default:

- Pending requests first.
- Oldest pending request first within the pending group.

### Get one approval request

```text
GET /api/v1/deployment-approvals/{request_id}
```

Response must include:

- Target.
- Policy snapshot.
- Requester.
- Approval progress.
- Existing decisions.
- Expiration.
- Current status.
- Current-user permissions.
- Allowed actions.

### Approve

```text
POST /api/v1/deployment-approvals/{request_id}/approve
```

Request:

```json
{
  "note": "Optional audit note"
}
```

The server obtains the actor from the authenticated session.

### Reject

```text
POST /api/v1/deployment-approvals/{request_id}/reject
```

Request:

```json
{
  "note": "Required rejection reason"
}
```

Require a non-empty rejection note.

### Cancel

```text
POST /api/v1/deployment-approvals/{request_id}/cancel
```

Require requester ownership or an authorized administrative role.

### Approval summary

```text
GET /api/v1/deployment-approvals/summary
```

Return:

```json
{
  "pending": 0,
  "waiting_more_than_one_hour": 0,
  "requires_multiple_approvers": 0,
  "partially_approved": 0
}
```

Apply the current user's environment and system visibility rules.

### Submit agent attestation

```text
POST /api/v1/agent/running-state-attestations
```

This endpoint uses agent authentication, not an account session.

### Latest system trust state

```text
GET /api/v1/systems/{system_id}/running-state-trust
```

Return:

- Latest attestation summary.
- Verification status.
- Current projected classification.
- Stable reason code.
- Observed target.
- Expected authorized target.
- Matched authorization.
- Evidence age.
- Investigation state.
- Current-user allowed actions.

### Attestation history

```text
GET /api/v1/systems/{system_id}/running-state-attestations
```

Use bounded pagination.

Do not return canonical payload bytes or full signatures by default.

### Resolve or investigate

```text
POST /api/v1/running-state-attestations/{attestation_id}/actions
```

Request:

```json
{
  "action": "adopt",
  "note": "Required audit note"
}
```

Supported action values:

- `adopt`
- `replace`
- `investigate`
- `close_investigation`

Validate each action independently.

### Attestation trust summary

```text
GET /api/v1/running-state-attestations/summary
```

Return:

```json
{
  "flagged_unresolved": 0,
  "authorized_current": 0,
  "stale_evidence": 0
}
```

For `flagged_unresolved`, count unresolved current states with:

- `unauthorized_artifact`
- `unknown_artifact`
- `agent_identity_invalid`

For `stale_evidence`, count:

- `authorized_but_evidence_stale`
- `agent_attestation_stale`

### Aggregate list DTOs

Extend existing system and environment list DTOs.

System summaries must include:

- Pending approval count.
- Oldest pending approval timestamp.
- Current trust classification.
- Whether flagged trust attention exists.
- Current investigation status.

Environment summaries must include:

- Pending approval count.
- Number of systems with pending approval.
- Flagged trust count where required by the detail panel.

Calculate these values in aggregate queries.

Do not add one API request per card or row.

## Attention integration

Use `attention_occurrences`.

Add stable categories or subjects for:

- Pending deployment approval.
- Unauthorized artifact.
- Unknown artifact.
- Invalid agent identity.

Use a stable subject key.

Suggested subject keys:

```text
deployment_approval_request:{request_id}
running_state_trust:{system_id}:{episode_id}
```

### Pending approval attention

Open when:

- A request enters `pending`.

Resolve when:

- The request becomes approved.
- The request becomes rejected.
- The request expires.
- The request is cancelled.
- The request is superseded.

Do not create one occurrence per approval vote.

### Attestation trust attention

Open when current projected state enters:

- `unauthorized_artifact`
- `unknown_artifact`
- `agent_identity_invalid`

Keep the same occurrence open during one continuous trust episode.

Resolve when:

- A verified attestation reports an authorized state.
- A valid adopt action authorizes the exact observed artifact and reclassification succeeds.
- A completed replacement deployment is followed by a verified authorized attestation.
- An authorized operator closes the issue with a valid final resolution, where policy permits it.

Do not resolve when:

- A user dismisses the occurrence.
- An operator selects `Investigate`.
- An operator selects `Replace` but the system has not reported the replacement state.
- An approval request is created for the replacement.

### Sidebar Systems count

Return component counts:

```json
{
  "critical_or_offline": 0,
  "pending_approvals": 0,
  "flagged_attestations": 0,
  "total": 0
}
```

Calculate:

```text
total =
    critical_or_offline
  + pending_approvals
  + flagged_attestations
```

This is an occurrence count. It is not a unique-system count.

The tooltip must state this distinction and show each component.

Use one aggregate API response.

## Notification integration

Use TASK-414.

Create idempotent events for:

### Approval events

- `deployment_approval_requested`
- `deployment_approval_partially_approved`
- `deployment_approval_approved`
- `deployment_approval_rejected`
- `deployment_approval_expired`
- `deployment_approval_cancelled`
- `deployment_approval_superseded`

### Attestation events

- `running_state_trust_flagged`
- `running_state_trust_changed`
- `running_state_artifact_adopted`
- `running_state_replacement_requested`
- `running_state_investigation_opened`
- `running_state_investigation_closed`

### Notification idempotency

Use a stable key such as:

```text
deployment_approval_requested:{request_id}
deployment_approval_decision:{decision_id}
deployment_approval_terminal:{request_id}:{status}
running_state_trust_flagged:{system_id}:{episode_id}:{classification}
attestation_resolution:{resolution_action_id}
```

Do not emit the same event on every reconciliation pass.

### Recipients

At minimum:

- Notify the requester about approval decisions and terminal states.
- Notify eligible approvers about a new pending request.
- Notify authorized environment operators about flagged running-state trust.
- Notify the acting operator only when the existing notification policy includes self-events.
- Apply account and environment visibility rules.

### Deep links

Approval notifications must link to:

```text
Systems -> selected system -> Deploy tab -> approval request
```

Attestation notifications must link to:

```text
Systems -> selected system -> Deploy tab -> running-state trust banner
```

The route must remain valid after a browser refresh.

## Web UI requirements

Use production Dioxus components.

Do not copy React state or mock arrays from the design directory.

### Routing

Extend the existing system-detail route or route state to support:

- Selected system ID.
- Selected tab.
- Optional approval request ID.
- Optional attestation ID.

A refresh must preserve the selected Deploy tab.

Use stable URL parameters where practical.

### Systems list

Add `Needs attention` to the status filter.

For the initial design behavior, this filter includes system-health states:

- `warning`
- `drifted`
- `critical`
- `offline`

Also support server query parameters for:

- Pending approval.
- Flagged attestation.

Dashboard and notification Review actions must use the appropriate filtered route.

Make the `N needing attention` summary a real button or link.

Do not use a non-keyboard-accessible clickable text span.

### System card and row approval chip

When a system has pending approvals:

- Show one compact chip.
- Use singular or plural text correctly.
- Show `Awaiting approval` where space permits.
- Show the count when more than one request exists.
- Open System Detail on the Deploy tab.
- Focus or scroll to the approval section when an approval request ID exists.

Use a real button.

Stop click propagation so card selection and approval navigation do not both execute.

### Deploy approval banner

Show:

- Pending state.
- Target commit or store-path identifier.
- Policy name.
- `current approvals / required approvals`.
- Requester.
- Request age.
- Expiration when present.
- Review action.

Show a disabled state or explanatory text when the current user cannot review the request.

### Approval modal

Show:

- System hostname.
- Environment.
- Flake.
- Commit.
- Store path, with copy action.
- Requester.
- Request time.
- Policy name and version.
- Required role.
- Required approval count.
- Existing approvals.
- Expiration.
- Optional approval note.
- Required rejection note.

Actions:

- Cancel.
- Reject.
- Approve.

Disable invalid actions.

After a successful action:

- Refresh approval details.
- Refresh the system summary.
- Refresh environment aggregate counts.
- Refresh dashboard aggregate counts where mounted.
- Refresh sidebar attention count.
- Refresh notifications through the existing notification mechanism.
- Close the modal only after the server confirms success.
- Show a success toast.

For a conflict response:

- Keep the modal open.
- Refresh current request state.
- Explain that another actor changed the request.

### Running-state trust banner

Display the current projected classification.

For flagged states, show:

- Classification label.
- Reason.
- Observed store path.
- Observation time.
- Evidence age.
- Agent key or identity summary.
- Investigation state.
- Decide or Review action.

Do not show raw signatures.

### Attestation decision modal

Show:

- System.
- Current observed artifact.
- Current expected artifact.
- Classification.
- Reason.
- Observation time.
- Verification status.
- Matching authorization when one exists.
- Existing investigation state.
- Audit note.

Actions depend on server-provided permissions.

#### Adopt

- Show a high-impact warning.
- Require note.
- Confirm exact system and store path.
- Disable for identity-invalid records.
- Do not allow wildcard authorization.

#### Replace

- Show the proposed authorized target.
- Explain that normal deployment policies still apply.
- Require note.
- Do not display the condition as resolved after the API accepts the request.

#### Investigate

- Require note.
- Permit optional owner selection when current user permissions allow it.
- Keep the trust banner and attention state visible.

### Environments list

Replace the existing summary field with `Awaiting approval`.

For each environment:

- Show pending request count.
- Show an approval chip when count is greater than zero.
- Keep existing system-health counts.
- Preserve table and card layouts.
- Preserve sorting and filtering.

### Environment detail panel

Add a pending approvals section.

Each row must show:

- Hostname.
- Target commit or short target identifier.
- Current approvals and required approvals.
- Request age.
- Expiration warning when near expiration.

Selecting a row opens System Detail on the Deploy tab with the request selected.

Show a proper empty state when no requests are pending.

### Dashboard Deploy Approvals widget

Show four values:

- Pending.
- Waiting more than one hour.
- Requires multiple approvers.
- Partially approved.

The Review action must open Systems with the pending-approval filter.

Use the server summary endpoint.

Do not calculate these values from a limited client-side list.

### Dashboard Attestation Trust widget

Show:

- Flagged unresolved.
- Authorized current.
- Stale evidence.

The Review action must open Systems with the flagged-attestation filter.

Use the server summary endpoint.

### Saved dashboard layout migration

When loading a saved layout:

1. Check whether `deployApprovals` exists.
2. Check whether `attestationTrust` exists.
3. Insert only missing widgets.
4. Preserve the user's existing widget order and sizes where possible.
5. Save the upgraded layout through the existing layout persistence path.
6. Do not insert duplicates on the next load.

Add a dashboard layout schema version if the existing model supports one.

### Sidebar

Make only the navigation section scrollable.

Keep:

- Logo/header fixed.
- Profile/account section pinned.
- Notification controls accessible.
- Current collapse behavior.
- Keyboard navigation.

Do not create nested page-level horizontal scrollbars.

### Top bar

Remove the Tweaks action only when:

- Its functions exist in another production location.
- The replacement location is reachable.
- Existing tests and documentation are updated.

If the action still exposes unique production behavior, move that behavior before removing the action.

### Notifications

Render TASK-414 notifications.

Approval items must show:

- Request type.
- System.
- Requester where permitted.
- Approval progress.
- Relative time.
- Read state.

Attestation items must show:

- Trust classification.
- System.
- Relative time.
- Resolution state where applicable.

Selecting an item must open the correct System Detail Deploy context.

## Accessibility requirements

- Use buttons for actions.
- Use links for navigation.
- Support keyboard activation.
- Show visible focus.
- Give icon-only controls accessible names.
- Trap focus inside modals.
- Restore focus when a modal closes.
- Support Escape to close non-destructive modals.
- Do not close a modal while a destructive request is in progress.
- Associate validation errors with the relevant fields.
- Do not rely on color alone for trust state.
- Use text labels for approval and trust status.
- Preserve acceptable contrast in light and dark themes.

## Authorization requirements

All server mutations must enforce authorization.

### Approval review

The server must verify:

- Authenticated account session.
- User can access the system and environment.
- User has the required policy role.
- User is allowed to approve or reject.
- User is not a prohibited requester.
- Request is pending.
- Request is not expired.
- Request target still matches the stored fingerprint.

Do not rely on disabled UI controls.

### Attestation actions

Suggested minimum roles:

- View trust status: users with system read access.
- Investigate: environment operator or administrator.
- Replace: user who can request deployment to the system.
- Adopt: administrator or a dedicated artifact-trust role.
- Close investigation: environment operator or administrator.

Use existing role and environment membership models.

Do not add a string comparison against a display role name when a canonical permission check exists.

### Agent endpoint

The agent endpoint must not accept an account-session identity as a substitute for an enrolled agent identity.

Apply:

- Agent key validation.
- Session validation where available.
- System binding.
- Request-size limits.
- Timestamp limits.
- Signature verification.
- Replay protection.
- Rate limits or bounded ingestion where supported.

## Audit requirements

Record every state-changing operation.

Required audit fields:

- Actor.
- Actor type.
- System.
- Environment.
- Request or attestation.
- Exact target.
- Previous state.
- New state.
- Reason code.
- Human note.
- Timestamp.
- Request correlation ID where available.

Never write private keys to audit data.

Do not truncate a note without informing the caller.

Use a maximum note size of 2,000 Unicode scalar values or the repository-standard lower limit.

Render notes as plain text.

## Performance requirements

- Use aggregate SQL for dashboard counts.
- Use aggregate SQL for environment counts.
- Use aggregate SQL for sidebar counts.
- Avoid N+1 queries.
- Paginate approval and attestation history.
- Index active approval and latest-attestation queries.
- Keep signed payload bytes out of ordinary list responses.
- Do not load full attestation history for a system list card.
- Reconcile stale states in bounded batches.
- Reconcile expired requests in bounded batches.
- Use stable ordering and cursors.

## Observability

Add structured logs for:

- Approval request creation.
- Approval request reuse.
- Approval request supersession.
- Approval and rejection.
- Approval expiration.
- Authorization issuance.
- Authorization consumption.
- Attestation accepted.
- Attestation verification failure.
- Replay rejection.
- Trust classification change.
- Investigation open and close.
- Adopt action.
- Replace action.
- Attention reconciliation failure.
- Notification-production failure.

Include IDs and reason codes.

Do not include:

- Private keys.
- Full signatures.
- Entire canonical payloads.
- Account session tokens.
- Agent bearer credentials.

Add metrics where the existing metrics framework supports them:

- Pending approval requests.
- Oldest pending request age.
- Approval decision latency.
- Expired approval count.
- Attestations accepted.
- Invalid signatures.
- Replay rejections.
- Current flagged trust states.
- Current stale evidence states.
- Attestation processing latency.
- Notification producer failures.

## Error handling

### Approval request no longer pending

When the UI submits a decision after another user changed the request:

- Return HTTP conflict or the repository-standard conflict response.
- Include the current status.
- Do not create a decision.
- Let the UI refresh the modal.

### Attestation verification failure

Persist enough safe evidence to support an identity-invalid trust event where permitted.

Do not treat unverified agent-reported authorization IDs as trustworthy.

Return a non-success response to the agent for malformed, invalid, or replayed data.

### Notification failure

The core approval or attestation transaction must remain durable when asynchronous notification delivery fails.

Use the TASK-414 durable producer or outbox pattern.

Do not roll back a valid approval because an email, toast, or account-notification delivery attempt fails after commit.

### Attention failure

Attention reconciliation must be part of the durable transaction where the existing attention API supports it.

If attention reconciliation cannot be in the same transaction, record a durable retry requirement.

Do not silently lose the unresolved condition.

## File-level implementation guidance

The final file list can differ after repository inspection, but use these ownership boundaries.

### Server

```text
packages/default/crates/cf-server/src/models/deployment_policies.rs
```

- Reuse `ApprovalConfig`.
- Add only fields required for explicit requester approval behavior or policy snapshot behavior.
- Preserve compatible serialization where possible.

```text
packages/default/crates/cf-server/src/models/deployment_approvals.rs
packages/default/crates/cf-server/src/queries/deployment_approvals.rs
```

- Add durable approval domain and query logic.
- Keep handlers thin.
- Centralize transitions.

```text
packages/default/crates/cf-server/src/queries/deployment.rs
```

- Gate deployment target exposure on authorization.
- Bind executions to authorizations.
- Supersede stale approval requests when the target changes.

```text
packages/default/crates/cf-server/src/models/running_state_attestations.rs
packages/default/crates/cf-server/src/queries/running_state_attestations.rs
```

- Store immutable envelopes.
- Store assessments separately.
- Implement latest trust queries.

```text
packages/default/crates/cf-server/src/queries/attention.rs
```

- Add approval and trust reconciliation helpers.
- Reuse existing locking and episode behavior.
- Do not weaken existing dismissal behavior.

```text
packages/default/crates/cf-server/src/queries/dashboard.rs
packages/default/crates/cf-server/src/queries/environments.rs
packages/default/crates/cf-server/src/queries/navigation.rs
packages/default/crates/cf-server/src/queries/systems.rs
```

- Add aggregate counts and summary fields.
- Avoid per-row queries.

### Protocol

```text
packages/default/crates/cf-protocol/src/agent.rs
```

- Add versioned signed attestation DTOs.
- Add stable serialization tests.
- Keep wire models independent from SQLx.

### Agent

```text
packages/default/crates/cf-agent/src/system_state.rs
```

- Reuse current state inspection.

Add a focused module for:

- Counter state.
- Canonical payload generation.
- Signing.
- Trigger coalescing.
- Submission and retry.

### Web UI

```text
packages/web-ui/src/views/systems.rs
packages/web-ui/src/views/systems_list.rs
```

- Add attention and approval filters.
- Add approval chips.
- Add deep-link handling.

```text
packages/web-ui/src/views/system_detail.rs
packages/web-ui/src/components/system/
```

- Add approval banner and modal.
- Add trust banner and action modal.
- Use typed API clients.

```text
packages/web-ui/src/views/environments.rs
packages/web-ui/src/views/environments_list.rs
```

- Add approval aggregate display.
- Add detail-panel request list.

```text
packages/web-ui/src/views/dashboard.rs
packages/web-ui/src/components/dashboard/
```

- Add both widgets.
- Add saved-layout migration.

```text
packages/web-ui/src/components/layout/
packages/web-ui/src/components/notifications/
```

- Add sidebar scroll behavior.
- Add component count tooltip.
- Use TASK-414 notifications and deep links.

## Testing requirements

Add tests before marking this task complete.

### Approval unit tests

Test:

- Request fingerprint stability.
- Different targets produce different fingerprints.
- Different policy versions produce different fingerprints.
- Request reuse for identical inputs.
- Request supersession after target change.
- Request supersession after policy-version change.
- One required approval.
- Multiple required approvals.
- Distinct approvers.
- Duplicate actor.
- Requester self-approval denied by default.
- Requester self-approval allowed by explicit policy.
- Role mismatch.
- Approval expiration.
- Approval after expiration.
- Rejection after partial approval.
- Approval after rejection.
- Cancellation.
- Authorization issuance.
- Exact target binding.
- Authorization consumption.
- Invalid state transitions.

### Approval database and concurrency tests

Test with PostgreSQL:

- Two simultaneous first approvals by the same user.
- Two simultaneous final approvals by different users.
- Approval racing expiration.
- Approval racing supersession.
- Target change racing final approval.
- One authorization created.
- One terminal notification event created.
- One attention occurrence resolved.
- No approvals copied to a replacement request.

### Attestation protocol tests

Test:

- Canonical payload fixed vector.
- Valid signature.
- Modified field invalidates signature.
- Modified digest is rejected.
- Optional field encoding is deterministic.
- Protocol version mismatch.
- Large payload rejection.
- Stable enum serialization.

### Agent tests

Test:

- Counter persists across process restart.
- Counter does not repeat during concurrent triggers.
- Boot-ID change starts the correct boot-session behavior.
- Store-path change triggers an attestation.
- Periodic trigger sends an attestation.
- Duplicate triggers are coalesced.
- Failed submission retries.
- Permanent authentication failure does not retry without limit.
- Private-key file permissions are enforced.
- No private key enters logs.

### Attestation server tests

Test:

- Valid enrolled key.
- Unknown key.
- Revoked key.
- Key and system mismatch.
- Invalid session.
- Invalid signature.
- Repeated attestation ID.
- Repeated counter.
- Lower counter.
- Timestamp too old.
- Timestamp too far in the future.
- Immutable row behavior.
- Assessment insertion.
- Classification precedence.

### Classification tests

Create one explicit test for each classification:

- `authorized_current`
- `authorized_but_evidence_stale`
- `authorized_previous_generation`
- `deployment_pending_reboot`
- `activation_failed`
- `unauthorized_artifact`
- `unknown_artifact`
- `agent_attestation_stale`
- `agent_identity_invalid`

Also test:

- Known artifact authorized for another system is unauthorized here.
- Old authorization does not authorize a new target.
- Revoked authorization does not authorize current state.
- Stale projection does not mutate the original assessment.
- A new verified attestation clears stale projection.
- Invalid agent identity takes precedence over artifact matching.

### Resolution tests

Test:

- Adopt exact known artifact.
- Adopt unknown artifact with required warning and note.
- Adopt blocked for identity-invalid attestation.
- Adopt does not change the original attestation.
- Replace uses normal policy evaluation.
- Replace can create a new approval request.
- Replace keeps trust attention open.
- Investigate keeps trust attention open.
- Investigation can be closed with a reason.
- Later authorized attestation resolves the trust episode.
- One resolution notification per action.

### Attention tests

Test:

- Pending request opens attention.
- Repeated reconciliation does not duplicate attention.
- Partial approval keeps attention open.
- Full approval resolves attention.
- Rejection resolves approval attention.
- Expiration resolves approval attention.
- Flagged classification opens trust attention.
- Same continuous episode reuses occurrence.
- New later episode creates a new occurrence.
- Investigate does not resolve attention.
- User dismissal does not resolve shared state.
- Authorized attestation resolves trust attention.

### Notification tests

Test:

- Approval request event.
- Partial approval event.
- Final approval event.
- Rejection event.
- Expiration event.
- Flagged trust event.
- Classification transition event.
- Adopt event.
- Replace event.
- Investigation events.
- Idempotency keys.
- Recipient authorization.
- Deep-link metadata.
- No notification created during a read-only query.

### API tests

Test:

- Authentication.
- Authorization.
- Environment scoping.
- Pagination.
- Filters.
- Current-user action permissions.
- Conflict responses.
- Validation errors.
- Rejection-note requirement.
- Adoption-note requirement.
- Agent authentication cannot use an account session.
- Account authentication cannot impersonate an agent.

### Web UI tests

Test:

- Needs-attention filter.
- Approval chip navigation.
- Deep link opens Deploy tab after refresh.
- Approval banner content.
- Approval modal approve path.
- Approval modal reject path.
- Concurrent-state conflict refresh.
- Trust banner classification.
- Adopt validation.
- Replace validation.
- Investigate state remains visible.
- Environment approval counts.
- Environment detail deep link.
- Dashboard approval summary.
- Dashboard trust summary.
- Existing layout gets missing widgets.
- Existing layout does not get duplicate widgets.
- Sidebar total and tooltip.
- Notification deep links.
- Keyboard modal behavior.
- No hard-coded design data is used.

## Verification commands

Run the repository-standard checks.

At minimum:

```bash
nix develop -c cargo fmt --all -- --check
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo test --workspace
nix flake check
```

Also run:

- Database-backed server tests with the repository test PostgreSQL workflow.
- Web UI build.
- Web UI component tests.
- Playwright or repository-standard browser tests for the main approval and trust flows.
- SQLx offline metadata generation or verification when query metadata changes.

Do not update SQLx metadata without executing the relevant queries against the supported PostgreSQL version.

## Required implementation sequence

Use this sequence unless a dependency requires a small adjustment.

### Phase 1: Schema and domain models

- Rebase on current `dev`.
- Confirm TASK-412 and TASK-414 interfaces.
- Add migrations.
- Add enums and domain models.
- Add transition and fingerprint tests.

### Phase 2: Approval backend

- Implement request creation.
- Implement target supersession.
- Implement approve, reject, cancel, and expire.
- Implement authorization issuance.
- Gate deployment exposure.
- Add attention reconciliation.
- Add notification producers.
- Complete concurrency tests.

### Phase 3: Attestation protocol and agent

- Add protocol DTO.
- Add canonical encoding.
- Add signing vectors.
- Add durable counter.
- Add trigger coalescing.
- Submit attestations.
- Add agent tests.

### Phase 4: Verification and classification backend

- Verify enrolled identity.
- Add replay protection.
- Store immutable attestations.
- Add assessments.
- Add current projected trust state.
- Add freshness reconciliation.
- Add trust attention.
- Add trust notifications.
- Complete classification tests.

### Phase 5: Resolution backend

- Add adopt.
- Add replace.
- Add investigate.
- Add investigation closure.
- Add authorization and audit tests.

### Phase 6: Aggregate APIs

- Add system summaries.
- Add environment summaries.
- Add dashboard summaries.
- Add sidebar component counts.
- Verify query plans and indexes.

### Phase 7: Web UI

- Add routing.
- Add Systems changes.
- Add System Detail banners and modals.
- Add Environment changes.
- Add Dashboard widgets.
- Add sidebar changes.
- Add notification rendering.
- Add accessibility behavior.

### Phase 8: Final verification and documentation

- Run all tests.
- Run Nix checks.
- Review migration safety.
- Review authorization.
- Review audit behavior.
- Review concurrency.
- Review screenshots against the design commit.
- Update documentation.

## Documentation requirements

Add or update production documentation.

Document:

- Approval request lifecycle.
- Approval policy snapshot behavior.
- Exact-target binding.
- Requester self-approval rule.
- Distinct approver behavior.
- Expiration and supersession.
- Deployment authorization lifecycle.
- Agent attestation payload.
- Canonical signing behavior.
- Counter and replay protection.
- Trust classifications.
- Trust resolution actions.
- Attention versus notification semantics.
- Configuration values.
- Operator troubleshooting.
- Security limitations.

Port the useful doctrine from:

```text
docs/design/CrystalForge/docs/alerts-and-notifications.md
```

into the production documentation area.

Do not make the design directory the permanent product documentation.

## Acceptance criteria

### Deployment approval enforcement

- [ ] A policy that requires approval creates a durable approval request.
- [ ] The request references an exact policy version.
- [ ] The request references one exact system and target.
- [ ] The target is not deployable before approval completes.
- [ ] The requester does not count as an approver by default.
- [ ] Explicit policy configuration can permit requester approval.
- [ ] Required approver roles are enforced on the server.
- [ ] Distinct approvers are enforced.
- [ ] Duplicate decisions are idempotent.
- [ ] A rejection makes the request terminal.
- [ ] An expired request cannot be approved.
- [ ] A changed target supersedes the previous request.
- [ ] A changed policy version supersedes the previous request.
- [ ] Approvals do not transfer to a superseding request.
- [ ] The final approval creates one immutable authorization.
- [ ] Two concurrent final approvals cannot create duplicate authorizations.
- [ ] A deployment execution consumes the exact authorization.
- [ ] An authorization cannot be used for another system or target.

### Signed running-state attestations

- [ ] The agent sends a versioned signed attestation.
- [ ] The agent uses its enrolled identity key.
- [ ] The payload has deterministic canonical encoding.
- [ ] The server verifies the digest and signature.
- [ ] The server verifies key and system binding.
- [ ] The server validates the agent session where applicable.
- [ ] The server enforces timestamp limits.
- [ ] The server rejects replayed attestation IDs.
- [ ] The server rejects replayed or lower counters.
- [ ] Counter state persists across an ordinary agent restart.
- [ ] Signed attestation rows are immutable.
- [ ] Classification data is stored separately.
- [ ] Full signatures and payloads are excluded from normal logs and list APIs.

### Trust classification

- [ ] All nine required classifications exist.
- [ ] Each classification has a stable reason code.
- [ ] Known but unauthorized artifacts are distinguished from unknown artifacts.
- [ ] Invalid identity takes precedence over artifact matching.
- [ ] Stale current state is projected without modifying old attestations.
- [ ] Fresh verified evidence clears a stale projection.
- [ ] Classification runs after relevant authorization and deployment changes.
- [ ] Periodic reconciliation detects stale evidence.

### Attestation actions

- [ ] Adopt creates an exact system-and-artifact authorization.
- [ ] Adopt requires an audit note.
- [ ] Adopt cannot hide an invalid agent identity.
- [ ] Replace uses the normal deployment-policy path.
- [ ] Replace can create a new approval request.
- [ ] Replace does not immediately resolve trust attention.
- [ ] Investigate creates a durable case.
- [ ] Investigate does not resolve trust attention.
- [ ] Closing an investigation requires a reason and note.
- [ ] Original attestations remain unchanged after all actions.

### Attention and notifications

- [ ] Pending approval creates one open attention occurrence.
- [ ] Terminal request status resolves pending-approval attention.
- [ ] A flagged trust episode creates one open attention occurrence.
- [ ] Reconciliation does not create duplicate occurrences.
- [ ] User dismissal does not alter shared state.
- [ ] Systems sidebar total includes health, approval, and trust components.
- [ ] Sidebar tooltip shows each component.
- [ ] Notifications use TASK-414.
- [ ] Notification producers use stable idempotency keys.
- [ ] Read-only rendering does not create events.
- [ ] Approval and trust notifications deep-link to the Deploy tab.

### Systems and System Detail UI

- [ ] Systems has a keyboard-accessible Needs attention filter.
- [ ] The needing-attention summary is interactive.
- [ ] Pending approval chips appear on affected systems.
- [ ] Approval chips open the Deploy tab.
- [ ] Deep links survive browser refresh.
- [ ] The Deploy tab shows pending request details.
- [ ] Authorized users can approve or reject.
- [ ] Rejection requires a note.
- [ ] The UI handles concurrent conflicts.
- [ ] Flagged trust state appears on the Deploy tab.
- [ ] The decision modal supports adopt, replace, and investigate.
- [ ] Server permissions control all enabled actions.
- [ ] Trust state does not rely only on color.

### Environments UI

- [ ] Environment summary shows Awaiting approval.
- [ ] Rows and cards show real pending counts.
- [ ] Environment detail lists pending requests.
- [ ] Selecting a request opens the correct system and Deploy tab.
- [ ] Counts come from aggregate server queries.
- [ ] No N+1 system requests are added.

### Dashboard UI

- [ ] Deploy Approvals widget exists.
- [ ] Attestation Trust widget exists.
- [ ] Both use server aggregate endpoints.
- [ ] Both appear in the default layout.
- [ ] Missing widgets are added to saved layouts.
- [ ] Existing widgets are not duplicated.
- [ ] Review actions open filtered Systems results.

### Shell and notifications UI

- [ ] Sidebar navigation scrolls independently.
- [ ] Profile area remains pinned.
- [ ] Existing sidebar collapse behavior works.
- [ ] Tweaks is removed only after its functions remain accessible.
- [ ] Notification items use persistent data.
- [ ] No sample notification arrays remain in production paths.

### Security and quality

- [ ] All mutations enforce server-side authorization.
- [ ] Browser-supplied actor IDs are ignored.
- [ ] Agent-supplied classifications are ignored.
- [ ] Agent-supplied authorization IDs are treated only as claims until verified.
- [ ] Audit records exist for all decisions and resolution actions.
- [ ] Migrations are additive and reversible where practical.
- [ ] Database constraints protect major invariants.
- [ ] Concurrency tests pass.
- [ ] API tests pass.
- [ ] Agent tests pass.
- [ ] Web UI tests pass.
- [ ] PostgreSQL integration tests pass.
- [ ] `cargo fmt` passes.
- [ ] `cargo clippy` passes with warnings denied.
- [ ] Workspace tests pass.
- [ ] `nix flake check` passes.
- [ ] SQLx offline metadata is current.
- [ ] Production code contains no approval or attestation mock arrays.

## Definition of done

This task is done only when:

1. Deployment approval is enforced by the server.
2. The approved target is exact and immutable.
3. The agent submits signed running-state attestations.
4. The server verifies and classifies those attestations.
5. Operators can review and act through the existing Deploy tab.
6. Attention and notifications follow their separate semantics.
7. Dashboard, Environments, Systems, System Detail, sidebar, and notifications match the design behavior.
8. All state survives a server restart and browser refresh.
9. All important state transitions have audit records.
10. Concurrency and security tests pass.
11. No prototype-only state remains in the production implementation.
12. The merge request includes screenshots or recordings of each changed UI surface.
13. The merge request description maps completed work to every acceptance-criteria section.

<!--
SECTION:DESCRIPTION:END
-->
