---
id: TASK-542.1
title: MCP list_system_scans and latest sealed scan fallback
status: In Progress
assignee: []
created_date: '2026-11-24T14:39:00Z'
updated_date: '2026-11-24T18:34:00Z'
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
- is_runtime_authoritative comes only from view_current_cve_authority. is_head is true when the scan is of the proven execution derivation of HEAD from configuration_cve_head, the scan that flake_head selection uses (with execution reuse its commit can be earlier than HEAD).
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
- [x] #1 list_system_scans(system_id, commit?, cursor?, limit?) requires cves:read and returns not found for hidden or unknown systems
- [x] #2 It pages the system's sealed configuration scans newest first, each with scan_id, commit_id, commit_hash, completed_at, scanner name and version, raw severity counts, is_head and is_runtime_authoritative
- [x] #3 commit filters by full commit hash or a hex prefix of at least 7 characters; an invalid value is an argument error
- [x] #4 The cursor is opaque, keyset-based, and rejected when reused with different system or commit arguments
- [x] #5 list_scan_findings in flake_head mode returns latest_sealed_scan (scan_id, commit_hash, completed_at, is_head) or null, and passing that scan_id to list_scan_findings returns its findings
- [x] #6 Tool descriptions name the fallback path, and docs/knowledge/security/mcp-server.md documents the tool and field
- [x] #7 Database-backed regression tests cover ordering, commit filter, cursor, environment scoping, and the fallback reference
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-542.1-mcp-list-system-scans

## Verification (2026-11-24, uncommitted worktree head on ef3a14bb+task commits)

- Farm, PostgreSQL-backed targets in full (throwaway override of cf-server-core-drv check phase with a sandbox PostgreSQL, /tmp/opencode/cf/dbtests.nix, SQLX_OFFLINE=true): mcp_compliance_read 7, mcp_policy_authoring 17, mcp_scan_findings 6 (new: system_scans_list_sealed_history_and_head_fallback), mcp_tools 8, mcp_transport 10, mcp_write_transport 9 passed, 0 failed, exit 0. The run predates a comment-only edit in scan_inventory.rs.
- Farm nix build .#default.cf-server-core-drv: lib tests 1634 passed, 1 failed in each of two runs; the new unit tests commit_filter_accepts_hex_prefixes_only and scan_cursor_is_bound_to_system_and_commit passed. The single failure was a different unrelated fake-executable test each run (ETXTBSY fixture race, tracked as TASK-543). The package build is therefore NOT green; post-check fixtures did not run.
- nix build .#okf-knowledge -L: exit 0, 202 concepts validated (after replacing an ASCII arrow chain in TASK-542 Goal with a numbered list).
- rustfmt 1.9.0 (flake nixpkgs) --edition 2024 on changed Rust files: clean. The default dev shell has no cargo fmt.
- clippy: not run; no clippy gate exists in the flake or CI.
- No migration, no SQLx offline metadata change (runtime query_as only), no browser-visible change, integration-access not affected (no scope change).

Design note: is_head uses HEAD's proven execution derivation (same rule as configuration_head_cve_scan), not commit equality, because execution reuse can make the HEAD scan's commit earlier than HEAD.
<!-- SECTION:NOTES:END -->
