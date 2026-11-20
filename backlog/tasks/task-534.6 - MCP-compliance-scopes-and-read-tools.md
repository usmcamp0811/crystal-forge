---
id: TASK-534.6
title: MCP compliance scopes and read tools
status: In Progress
assignee: []
created_date: '2026-11-21'
labels: [security, agent-access, mcp, compliance]
parent_task_id: TASK-534
dependencies:
  - TASK-534.5
references:
  - TASK-534
  - TASK-534.5
  - docs/knowledge/security/mcp-server.md
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Agents connected through MCP cannot discover Crystal Forge security policies,
compliance bundles or framework requirements. Later slices (TASK-534.7 policy
drafts and imports, TASK-534.8 bundle drafts and impact preview) need the scope
catalog and read tools first.

## Delivery plan (owner decisions, 2026-11-21)

The policy and compliance-bundle authoring feature is delivered as three
stacked slices: TASK-534.6 (this task), TASK-534.7, TASK-534.8.

- Authoring scopes (`policies:draft`, `policies:import`, `bundles:draft`) are
  issued only to tokens whose owner is an Admin. Operators and Viewers may hold
  `policies:read`, `bundles:read` and `compliance:preview`. Browser role
  permissions do not change.
- An agent may edit only a draft version whose `created_by` is the token owner.
- MCP supports the JSON/TOML policy interchange. XCCDF/STIG import through MCP
  is a follow-up (staged artifact upload).
- Trust, publication, assignment, enforcement-mode changes and deletion stay
  outside MCP for every token, including Admin-owned tokens.

## Scope of this slice

1. Migration `0343_compliance_token_scopes.sql` replaces the
   `api_access_token_scopes.scope` CHECK with the existing seven scopes plus
   `policies:read`, `policies:draft`, `policies:import`, `bundles:read`,
   `bundles:draft`, `compliance:preview`.
2. Scope catalog: add `requires_admin` (reason "Requires Admin"). Release
   `policies:read` and `bundles:read` (Viewer+). Register the other four as
   unreleased ("Not supported yet") until their slice ships. Every scope check
   that applies at creation, rotation and each call applies to the new scopes;
   an Admin-only scope stops working when its owner is no longer Admin.
3. Read tools, global catalog data only (no assignment, usage, system or
   evidence data):
   - `list_compliance_policies` (`policies:read`): search by name and
     description, filter by policy type, publication state and trust state;
     cursor pagination (25 default, 100 max).
   - `get_compliance_policy` (`policies:read`): lineage or exact version;
     lineage id, version id, version number, publication and trust state, policy
     type, execution phase, implementation state, config, requirement mappings,
     semantic digest, provenance (created by, derived from, source artifact),
     version list, and `can_edit_draft` / `can_derive_draft` for this token.
   - `list_compliance_bundles` (`bundles:read`): name search, version and
     publication metadata, cursor pagination.
   - `get_compliance_bundle` (`bundles:read`): lineage or exact version; ordered
     exact policy-version membership, requirement membership, framework version,
     trust and publication state, digests, provenance.
   - `list_compliance_requirements` (`policies:read`): framework and framework
     version discovery, requirement search by identifier or text within a
     framework version, and existing policy mappings for a requirement.
4. Token create UI explains that read, draft and import are separate and that
   no capability publishes, trusts or assigns.
5. Docs: MCP server tools and authorization matrix, access-token scope list.

## Non-goals

Draft creation, editing, validation, import, bundle authoring and impact
preview (TASK-534.7, TASK-534.8). XCCDF through MCP.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The six scopes are accepted by the database; read scopes are selectable by Viewer, Operator and Admin; authoring scopes show "Requires Admin" to non-Admins and "Not supported yet" until released.
- [ ] #2 The five read tools return the documented data for tokens holding the scope, reject calls without it (also when tools/list is bypassed) and expose no assignment, usage, system or evidence data.
- [ ] #3 Exact version and lineage reads return immutable identities; pagination is bounded; unknown ids are not_found.
- [ ] #4 Database and HTTP tests cover the tools and scopes; docs describe the tools and the scope matrix.
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-534.6-mcp-compliance-read

Branch `TASK-534.6-mcp-compliance-read` starts at `83062654` (TASK-534.5 head, MR !349). Stack: !345, !346, !347, !349 unmerged. Next free migration 0343.
<!-- SECTION:NOTES:END -->
