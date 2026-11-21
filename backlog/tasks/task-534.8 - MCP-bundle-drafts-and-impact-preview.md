---
id: TASK-534.8
title: MCP bundle drafts and impact preview
status: In Progress
assignee: []
created_date: '2026-11-21'
labels: [security, agent-access, mcp, compliance]
parent_task_id: TASK-534
dependencies:
  - TASK-534.7
references:
  - TASK-534.6
  - TASK-534.7
  - docs/knowledge/security/mcp-server.md
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Agents can author policy drafts (TASK-534.7) but cannot assemble them into a
compliance bundle or see what a bundle would change on a system or environment.
Bundle authoring is Admin-only browser logic that accepts policy lineage ids
(the trigger picks a version), and the assignment preview accepts only accepted,
trusted bundle versions and returns no difference against the current state.

## Owner decisions (2026-11-21, see TASK-534.6)

- `bundles:draft` requires an Admin owner, at issue, rotation, per call and in
  the write transaction. `compliance:preview` is open to every role.
- An agent edits only a bundle draft version whose `created_by` is the owner.
- No MCP tool trusts, publishes, assigns, changes enforcement or deletes.

## Scope

1. Release `bundles:draft`.
2. Migration `0345`: `draft_revision` on `compliance_bundle_versions` (trigger
   bump on any real change of a mutable version, including membership rows),
   and `created_via_agent`, `created_access_token_id`
   (`ON DELETE SET NULL`), `created_access_token_name` for version attribution.
3. Shared services (browser behavior unchanged):
   - create a bundle lineage with an initial draft (extend
     `queries::compliance::create_bundle` with an actor and audit);
   - derive a draft through `ensure_bundle_draft`;
   - set the exact ordered membership of a draft bundle version from explicit
     policy version ids. Never resolve a lineage to its newest version. Reject
     duplicates, unknown versions, two versions of one lineage, and versions
     that the existing publish rules would never accept (report untrusted and
     draft members as warnings, since publication stays a human step). Respect
     the membership immutability triggers and keep `compliance_bundle_policies`
     consistent with the version membership.
4. Candidate impact preview (read-only, rolled-back transaction or pure reads):
   resolve the effective policy set of a target system or environment as it is
   now (the real resolver) and as it would be with the candidate bundle version
   assigned in a given enforcement mode, and return the difference. The
   resolver gets an explicit candidate mode that accepts a draft or untrusted
   version only for preview; committed assignments keep the current checks.
5. MCP tools:
   - `create_bundle_draft` (`bundles:draft`): `mode: new_lineage` (name,
     description, framework fields, optional initial `policy_version_ids`) or
     `mode: derive` (`bundle_id`).
   - `update_bundle_draft` (`bundles:draft`): `version_id`, `draft_revision`,
     optional metadata changes and an optional complete ordered
     `policy_version_ids` list.
   - `preview_bundle_impact` (`compliance:preview`): `bundle_version_id`,
     exactly one of `system_id` or `environment_id` (must be in the token's
     effective environments), `enforcement_mode` (`enforce` or
     `report_only`). Returns the candidate version, current and proposed
     effective policies, added, removed and changed policies, conflicts,
     requirement coverage (existing coverage service), untrusted or draft
     members, warnings, and explicit `facts` versus `requires_evaluation`
     fields. Never claims pass or fail. Never writes, assigns, publishes,
     trusts or schedules work.
6. Attribution as in TASK-534.7 (audit with token id, `via: mcp`, name snapshot;
   version attribution columns; denied writes through `mcp_write_denied`). The
   Compliance view shows "via agent token <name>" on agent-created bundle
   versions.
7. Docs (tools, examples for assembling a bundle and previewing impact, operator
   review note) and tests (service, database, concurrency, HTTP through `/mcp`
   including the full agent workflow from the owner's brief).

## Non-goals

Trust, publish, assign, enforcement changes, XCCDF through MCP.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An Admin-owned token with `bundles:draft` creates a new bundle draft and a derived draft; derived drafts keep the exact membership; non-Admin owners cannot hold the scope.
- [ ] #2 Membership updates use exact policy version ids, reject invalid or duplicate references and stale revisions, write nothing for no-ops, and never change published versions or other users' drafts.
- [ ] #3 The impact preview uses the real resolver, respects environment isolation, separates facts from outcomes that need evaluation, and writes nothing.
- [ ] #4 Every mutation is attributed to the owner and token; denied calls are audited without content.
- [ ] #5 An HTTP test drives the full agent workflow: list and inspect policies, create and update a policy draft, preview and import, create a bundle draft with an exact version, preview it on an authorized system, confirm nothing was assigned or scheduled, revoke the token and confirm mutations stop.
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-534.8-mcp-bundle-drafts

Branch `TASK-534.8-mcp-bundle-drafts` starts at `3a676f20` (TASK-534.7, MR !351). Next free migration 0345.

Implementation (uncompiled, untested; no build, test or preview was run):

- Service `services/bundle_authoring.rs`; tools `mcp/bundle_authoring.rs`; migration `0345_bundle_draft_revision_and_agent_attribution.sql`; tests `tests/mcp_bundle_authoring.rs`.
- Exact membership: lineage rows in `compliance_bundle_policies` are written first (the `sync_bundle_version_membership` trigger runs), then the draft's `compliance_bundle_version_policies` rows are replaced with the requested versions in order; digest recomputed from the stored rows.
- Revision: BEFORE UPDATE trigger on `compliance_bundle_versions` plus an AFTER row trigger on `compliance_bundle_version_policies` and `compliance_bundle_version_requirements` that increments the parent draft's counter.
- Resolver: private `candidate_preview` flag on the single-assignment resolver and public `resolve_system_effective_policies_with_candidate_in_tx` with `CandidateAssignment`; the candidate replaces an active assignment of the same bundle lineage at the same target. Preview runs in a `REPEATABLE READ, READ ONLY` transaction that is rolled back; environments use the batch resolver for the current set and at most 100 systems.
- Browser `create_bundle` now takes an actor, sets `created_by` on the initial draft and writes `bundle_draft_created`; `update_bundle` writes `bundle_draft_updated`.
<!-- SECTION:NOTES:END -->

## Verification

Farm PostgreSQL run at `170bfd2f`: `access_tokens` 47, `assignment_semantics` 13, `compliance_assignment_zombie_repair` 1, `composite_policy` 38, `mcp_compliance_read` 7, `mcp_policy_authoring` 17, `mcp_transport` 10, `resolver_enforcement` 0 run (8 ignored by design), `task433_assignment_visibility` 1, `task433_csrf` 4 passed; `mcp_bundle_authoring` 8 passed and 1 failed (`membership_updates_are_exact_validated_and_revisioned`: the fixture deprecated a published policy version whose `mapping_digest` was still `pending`, which the existing immutability trigger only allows as a digest backfill). The fixture now computes the mapping digest before publishing, as the publish path does. At that fix (`mcp_bundle_authoring` only): 9 passed, 0 failed, including `full_agent_workflow_over_http`. Web UI at `170bfd2f`: 970 passed. `nix build .#okf-knowledge` passed.

Not run: integration-access scenario (farm builder aborted NixOS unit builds twice during TASK-534.6); CI pending.
