---
id: TASK-534.5
title: M4 MCP write tools with agent attribution
status: In Progress
assignee: []
created_date: '2026-11-20'
labels: [security, agent-access, mcp]
parent_task_id: TASK-534
dependencies:
  - TASK-534.4
references:
  - TASK-534
  - TASK-534.4
  - docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The MCP server (TASK-534.4) is read-only. An agent can find remediation work but
cannot request a scan, record progress on a POA&M, or ask the server to verify
it. Every existing write path records only the human user, so an action taken by
an agent would be indistinguishable from the user's own action.

## Goal

An agent holding a token with an explicit write scope can request a CVE scan,
read its result, update POA&M progress and run POA&M verification through MCP.
The ability to write is a property of the token: the owner selects write scopes
when creating it. Every write records the acting access token, and the Audit Log
and POA&M activity show that an agent did it.

## Decisions (owner, 2026-11-20)

- Stacked as the fourth MR on !347 (TASK-534.4).
- All three write capabilities in this slice: `scans:request`, `poams:write`,
  `poams:verify`. `poams:close` stays unreleased.
- Write capability is part of the token, not a server switch. The three scopes
  become selectable for Operator and Admin owners (Viewer: "Requires Operator").
  `mcp_enable` and `access_tokens_enable` remain the global kill switches. There
  is no `allowMutationTools` option.
- Every agent action is attributed to the access token in audit and activity
  records.

## Design

### Token policy

- `scans:request`, `poams:write`, `poams:verify` are released and require an
  Operator or Admin owner, at creation, rotation and on every call.
- A token that combines a write scope with an environment where
  `environments.is_production` is true needs `confirm_production_write: true` in
  the create request (400 `validation_failed`, field `confirm_production_write`,
  otherwise). Rotation keeps the original confirmation. The policy reports
  `is_production` per environment; the create dialog shows the design's
  production confirmation checkbox.

### Restricted write authorization

A write tool runs its domain mutation in one transaction that first refreshes
the actor: owner row `FOR SHARE`, token row `FOR SHARE` (not revoked, not
expired, same owner), owner roles and memberships `FOR SHARE`; the owner must be
Operator or Admin; effective environments are the grant intersected with the
owner's current environments (Admin owner: grant only), active environments
only; the tool's scope must be in the grant. The refreshed actor is never Admin
for domain checks. Whole-resource operations require every subject in the
effective set (`poam_visible_to_environments` plus
`require_restricted_poam_visibility_tx`); otherwise `not_found`.

### Tools

| Tool | Scope | Behavior |
| --- | --- | --- |
| `request_cve_scan` | `scans:request` | Inputs `system_id`, `expected_derivation_id`. Existing admission (`admit_system_cve_scan`) with the restricted actor. Returns `{status: accepted, scan_id, derivation_id}`; target changed, ineligible and executor unavailable are tool errors with codes. Repeated requests return the active scan for the derivation (existing dedupe), so no separate idempotency key. Acceptance is not a result. |
| `get_scan_result` | `cves:read` | Inputs `system_id`, `scan_id`. The scan must be the system's current authority scan or a scan of the system's current target derivation; otherwise `not_found`. Returns status, timestamps, counts. |
| `update_poam_progress` | `poams:write` | Inputs `poam_id`, `revision`, and exactly one `action`: `add_note {text}`, `set_status {status, note?}` (no `completed`), `add_milestone {title, target_date}`, `update_milestone {milestone_id, title?, target_date?, completed?}`, `remove_milestone {milestone_id}`. No risk, title, plan, owner, assignee, target date, finding links, waivers, closure or reopening. Returns the new revision and the changed item. |
| `verify_poam` | `poams:verify` | Inputs `poam_id`, `revision`. Existing `verify` checks. Never closes. Returns attempt id, outcome, revision. |

### Attribution

- Migration `0341_agent_action_attribution.sql` adds nullable
  `access_token_id uuid REFERENCES api_access_tokens(id) ON DELETE SET NULL` to
  `admin_audit_events`, `poam_activity` and `poam_verification_attempts`.
- Every write made through a token sets `access_token_id` and adds audit
  metadata `{"via": "mcp", "access_token_id", "access_token_name"}` (name is a
  snapshot that survives token deletion). The human owner stays the actor.
- Admin Audit Log shows "<actor> via agent token <name>" and lists the POA&M
  write events. POA&M activity shows "via agent token <name>".
- A write tool call denied by authorization writes one
  `mcp_write_denied` audit event (token id, tool, reason code; no arguments).

## Non-goals

`poams:close`, risk acceptance, waivers, finding links, reopening, MCP OAuth, a
per-token write rate limit beyond the existing 4 concurrent requests (TASK-536),
admin token tab (TASK-537).

## Verification

Per owner instruction: implement quickly, full affected test targets on the
farm before push, CI runs the rest. Then a live write check on the dev server
with a write-scoped token.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Operator and Admin owners can create tokens with `scans:request`, `poams:write` and `poams:verify`; Viewers cannot; write plus a production environment requires explicit confirmation.
- [ ] #2 The four write-related tools work through MCP only for tokens holding the scope, with in-transaction reauthorization against the current token row, owner role and environments.
- [ ] #3 POA&M writes and verification require every subject in scope; hidden resources are `not_found`; closure, risk and other excluded fields cannot change.
- [ ] #4 Every agent write records the access token in audit and activity rows, and the Audit Log and POA&M activity display it; denied write calls are audited.
- [ ] #5 Database tests cover each tool, scope and role denial, revocation between calls, cross-environment denial and attribution; docs describe the write tools.
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-534.5-mcp-write-tools

Branch `TASK-534.5-mcp-write-tools` starts at `882c4462` (TASK-534.4 head). Migration 0341 is the next free number (0337 to 0340 belong to the scan-cache-recovery work).
<!-- SECTION:NOTES:END -->

## Verification

Farm PostgreSQL runs (sandbox clusters, all at the branch heads named):

- `36e0f129`: `access_tokens` 45, `mcp_tools` 8, `mcp_write_poam` 7, `mcp_write_scan` 8, `poam_workflows` 133 passed; `mcp_transport` 9 passed and 1 failed (`admin_status_reports_configuration_and_requires_admin` still expected `phase: read-only` and 4 tools).
- `d3cdf293` (expectation fixed): `mcp_transport` 10 passed, 0 failed.
- Web UI derivation at `ad0a9114`: 965 passed, 0 failed.
- `nix build .#okf-knowledge` passed.

The integration-access scenario was updated for the released write scopes (and for the environment `color`/`is_production` fields added since TASK-534.3) but has not been run. No live write check on the dev server yet.

## Review !349 response (head `ce230da0`)

- P1 CI: the Access scenario now expects `production_write_confirmed: false` in the read-only creation audit metadata; `tests/access_tokens.rs` already asserts `true` for a confirmed production write grant.
- P1 race: restricted POA&M writes and verification lock every grant environment `FOR SHARE`, then every POA&M subject system `FOR SHARE`, in id order, before the owner/token refresh and the POA&M row; `is_active` and visibility are read after the locks. The order matches system move, scan admission, fleet triage, reopen and CVE linking. Tests prove a committed deactivation or system move during the wait is observed, and a write in progress blocks both until commit.
- P2 durable attribution: migration `0342_agent_action_attribution_snapshot.sql` adds `via_agent` and an `access_token_name` snapshot to POA&M activity and verification attempts. Audit Log `via_agent` also derives from `metadata.via` and `mcp_write_denied`. A token with POA&M history cannot be hard-deleted (immutability triggers reject the `SET NULL`); a token with only audit rows can, and the Audit Log keeps the marker and name.
- P2 no-op milestone: an unchanged update writes nothing and keeps the revision (MCP and browser).
- P2 HTTP: `tests/mcp_write_transport.rs` covers the write tools over `/mcp`, stale revision, scan request then result, demotion and missing scope with transport-written `mcp_write_denied`, revocation between calls, and the audit marker after token deletion.
- P2 production confirmation now names the selected production environments and write capabilities. Verification attempts and activity show "via agent token <name>" or "(deleted)".

Farm at `65a25838` (output `xc3izr5rf8q2a2ikzmg7jhmp7v6q2y5k-cf-mcp-regressions-0.3.0`): access_tokens 45, mcp_tools 8, mcp_transport 10, mcp_write_poam 12, mcp_write_scan 8, mcp_write_transport 8, poam_workflows 133 passed; 0 failed. Web UI 968 passed. OKF passed. Integration-access scenario not rerun locally (CI). Live dev-server check before the fixes: scan request accepted and deduplicated, note on POAM-0005 attributed to token "Write Test".

Follow-up review gaps closed at the head that adds them: `verify_poam_over_http_records_a_sealed_attributed_attempt` (successful verification over `/mcp`: sealed attempt, revision +1, token attribution on attempt, activity and audit, never completed) and `verification_serializes_with_a_real_system_move` (20 concurrent iterations against the real `PATCH /api/v1/systems/:id` move handler with no deadlock or timeout and only coherent outcomes, plus a deterministic run that holds the move's CVE key and environment locks and proves verification waits and then returns not found). Farm: `mcp_write_poam` 13 passed, `mcp_write_transport` 9 passed, 0 failed.
