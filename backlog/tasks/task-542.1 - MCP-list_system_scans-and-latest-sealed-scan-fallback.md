---
id: TASK-542.1
title: MCP list_system_scans and latest sealed scan fallback
status: In Progress
assignee: []
created_date: '2026-11-24T14:39:00Z'
updated_date: '2026-11-24T14:40:00Z'
labels: []
dependencies: []
parent_task_id: TASK-542
priority: high
type: feature
ordinal: 578000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
TASK-542 finding 1. The UI shows a system's last good CVE scan, but MCP never returns its scan ID. list_scan_findings accepts only HEAD or an exact scan_id, and get_scan_result needs an ID. When HEAD has no sealed scan the agent is stuck.

## Goal
An agent can discover the sealed configuration scans of a system and fall back to the newest one when HEAD has no sealed scan.

## Non-goals
- No failed or pending scan listing (TASK-542 finding 2 covers attempt and evaluation state).
- No change to Current authority, get_scan_result relation rules, POA&M planning admission, or scan admission.
- No migration; no new scope.

## Architectural constraints
- New read tool in cf-server/src/mcp/scan_inventory.rs, scope cves:read, using scoped_read (one REPEATABLE READ snapshot) and SCOPED_SYSTEMS. Hidden and unknown systems return not found.
- Source: the existing sealed_configuration_cve_scans view, filtered by the system's flake_id and effective configuration name, the same relation that list_scan_findings uses for an explicit scan_id. Every listed scan_id is therefore accepted by list_scan_findings(scan_id).
- Keyset pagination on (completed_at DESC, scan_id DESC) with an opaque cursor bound to the filters; bounded limit (existing limit_schema).
- is_runtime_authoritative comes only from view_current_cve_authority. is_head compares the scan commit with configuration_cve_head.
- latest_sealed_scan in list_scan_findings is a reference only. It never changes state, selection, findings, or planning eligibility.

## Verification plan
Full runs of the cf-server targets mcp_scan_findings, mcp_tools, mcp_transport against an isolated PostgreSQL via the Nix dev environment; cargo fmt and clippy; nix build .#okf-knowledge -L.

## Impact areas
cf-server mcp/scan_inventory.rs, mcp/tools.rs (registry), mcp/mod.rs (tool names), tests/mcp_scan_findings.rs, docs/knowledge/security/mcp-server.md.

## Risk
Low. Additive read tool over an existing view.

## Dependencies
TASK-534.9 (merged).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 list_system_scans(system_id, commit?, cursor?, limit?) requires cves:read and returns not found for hidden or unknown systems
- [ ] #2 It pages the system's sealed configuration scans newest first, each with scan_id, commit_id, commit_hash, completed_at, scanner name and version, raw severity counts, is_head and is_runtime_authoritative
- [ ] #3 commit filters by full commit hash or a hex prefix of at least 7 characters; an invalid value is an argument error
- [ ] #4 The cursor is opaque, keyset-based, and rejected when reused with different system or commit arguments
- [ ] #5 list_scan_findings in flake_head mode returns latest_sealed_scan (scan_id, commit_hash, completed_at, is_head) or null, and passing that scan_id to list_scan_findings returns its findings
- [ ] #6 Tool descriptions name the fallback path, and docs/knowledge/security/mcp-server.md documents the tool and field
- [ ] #7 Database-backed regression tests cover ordering, commit filter, cursor, environment scoping, and the fallback reference
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-542.1-mcp-list-system-scans
<!-- SECTION:NOTES:END -->
