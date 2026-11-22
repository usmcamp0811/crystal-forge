---
id: TASK-534.7
title: MCP policy drafts, validation and interchange import
status: In Progress
assignee: []
created_date: '2026-11-21'
labels: [security, agent-access, mcp, compliance]
parent_task_id: TASK-534
dependencies:
  - TASK-534.6
references:
  - TASK-534.6
  - docs/knowledge/security/mcp-server.md
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Agents can read the policy catalog (TASK-534.6) but cannot propose policy
content. Policy draft creation and interchange import exist only as Admin
browser routes whose logic is partly inline in HTTP handlers.

## Owner decisions (2026-11-21, see TASK-534.6)

- `policies:draft` and `policies:import` require an Admin owner, at issue, at
  rotation, per call and inside each write transaction.
- An agent edits only a draft version whose `created_by` is the token owner.
- JSON/TOML interchange only; XCCDF through MCP is a follow-up.
- No MCP tool trusts, publishes, assigns, enables or deletes.

## Scope

1. Release `policies:draft`, `policies:import` and `compliance:preview`.
2. Extract the reusable domain logic from the Admin handlers into services that
   both the browser handlers and MCP call, without changing browser behavior:
   policy draft creation (new lineage with an initial draft; derived draft of an
   existing lineage through `ensure_policy_draft`), draft update, validation,
   interchange preview and interchange import (the about 430-line inline import
   in `handlers/api/compliance.rs`).
3. Optimistic concurrency for draft edits: migration `0344` adds a
   `draft_revision` counter to `deployment_policy_versions` (forward
   compatible, default 1). An MCP update must send the current revision; a
   stale revision fails without writing. A no-op update writes nothing. Browser
   edits increment the revision but keep their current contract.
4. Version-level agent attribution: migration `0344` also adds
   `created_via_agent boolean NOT NULL DEFAULT false`,
   `created_access_token_id uuid REFERENCES api_access_tokens(id) ON DELETE SET NULL`
   and `created_access_token_name text` to `deployment_policy_versions`, so the
   Policies view can show "via agent token <name>" next to the author.
5. MCP tools:
   - `create_policy_draft` (`policies:draft`): `mode: new_lineage` with name,
     description, policy type, config, execution phase, enabled-by-default,
     compliance metadata and requirement mappings; or `mode: derive` with
     `policy_id` (and optional source version). Returns lineage id, version id,
     version number, draft revision. A lineage whose open draft belongs to
     another user gives a conflict that does not reveal more than the draft
     version id.
   - `update_policy_draft` (`policies:draft`): `version_id`, `draft_revision`,
     replacement fields (description, config, execution phase, enabled by
     default, compliance metadata, requirement mappings). Only mutable drafts
     created by the owner. Published or accepted versions are immutable.
   - `validate_policy_draft` (`policies:draft`): validates a proposed policy
     (inline definition or an existing draft version) with the server validator;
     structured errors and warnings; never publishes; never evaluates Nix (the
     existing validator only parses with a 2 s deadline); states that syntax
     validity does not prove effectiveness.
   - `preview_policy_import` (`compliance:preview`): JSON or TOML document text
     plus format; returns source SHA-256, recognized policies and versions,
     identities, reconciliation decisions (reuse, create lineage, create
     version), conflicts, name collisions, blocking errors and warnings. No
     writes, no evaluation or deployment work.
   - `import_policy_draft` (`policies:import`): document, format and
     `expected_source_sha256`; reparse, revalidate, reconcile in one
     transaction with the existing advisory locks; imported versions are
     untrusted drafts; never overwrites an immutable version; all or nothing.
6. Attribution: every mutation writes `admin_audit_events` with the owner as
   actor, `access_token_id`, metadata `via: mcp`, token id and name snapshot,
   operation, target lineage and exact version, revision, and the source
   SHA-256 for imports (never the document). Denied write calls use the
   existing `mcp_write_denied` path. Browser policy draft creation and import
   keep their audit events (draft creation becomes audited).
7. UI: the Policies drawer shows "via agent token <name>" for versions created
   through MCP; MCP drafts use the same catalog and editor.
8. Docs and tests (service, database, concurrency, HTTP through `/mcp`).

## Non-goals

Bundles and impact preview (TASK-534.8). XCCDF through MCP. Trust, publish,
assign.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An Admin-owned token with `policies:draft` creates a new lineage draft and a derived draft; an Operator- or Viewer-owned token cannot hold the scope, and a demoted owner's call is rejected inside the transaction.
- [ ] #2 Draft updates require the current revision, reject stale revisions, write nothing for no-ops, and never modify published versions or other users' drafts.
- [ ] #3 Validation reports structured errors and warnings without persistence or Nix evaluation.
- [ ] #4 Import preview writes nothing; import verifies the source digest, reconciles with existing identities, creates untrusted drafts atomically, rejects digest conflicts and rolls back fully on failure, including under concurrent imports.
- [ ] #5 Every mutation is attributed to the owner and token; denied calls are audited without content; browser behavior is unchanged.
- [ ] #6 Tests and docs cover the tools, the authorization rules and the import workflow end to end over `/mcp`.
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-534.7-mcp-policy-drafts

Branch `TASK-534.7-mcp-policy-drafts` starts at `3b330b87` (TASK-534.6, MR !350). Next free migration 0344.
<!-- SECTION:NOTES:END -->

## Verification at `f0bde6b5`

Farm PostgreSQL run: `access_tokens` 47, `composite_policy` 38, `mcp_compliance_read` 7, `mcp_policy_authoring` 17, `mcp_transport` 10, `policy_editor_phase2` 5 passed; 0 failed. Web UI derivation (at `1d089a14`, before two non-UI commits; the later `compliance:preview` read-scope change in `logic.rs` is unit-tested but not rebuilt): 969 passed. `nix build .#okf-knowledge` passed after replacing semicolons in one Mermaid message.

Defect found while testing: an interchange policy without `enabled_by_default` made the import insert fail with a database error (HTTP 500 in the browser route as well). The field is NOT NULL and part of the semantic digest, so the import validator now rejects such a document with a validation error. The ignored browser interchange tests in `handlers/api/compliance.rs` built documents without the field; their documents now include it. Those tests are `#[ignore]` and not run by any CI job.

Decision: `compliance:preview` is not a write scope (no production confirmation), in the server and the token dialog.

Integration-access scenario: not run; two farm attempts for TASK-534.6 aborted on NixOS unit builds (builder signal 6) before any case started.
