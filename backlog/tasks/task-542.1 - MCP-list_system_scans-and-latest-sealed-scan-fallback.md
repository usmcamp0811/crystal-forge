---
id: TASK-542.1
title: MCP list_system_scans and latest sealed scan fallback
status: Review
assignee: []
created_date: '2026-11-24T14:39:00Z'
updated_date: '2026-11-28T02:50:10Z'
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
- is_runtime_authoritative comes only from view_current_cve_authority. is_head is true for every scan of the proven execution derivation of HEAD from configuration_cve_head (evidence identity, not selection); flake_head selects the newest such scan, the first one with is_head true in list order. With execution reuse a HEAD scan can belong to an earlier commit.
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

## Final record (2026-11-27), head 28dedaef2ab64adc4b71c1bfc498c54971c0f756

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/356 (targets dev).

### Scope
- TASK-542.1: new MCP read tool `list_system_scans` (scope `cves:read`) and the `latest_sealed_scan` reference in `list_scan_findings` (flake_head mode). Code: `cf-server/src/mcp/{scan_inventory,tools,mod}.rs`; tests: `tests/mcp_scan_findings.rs` (3 new database tests) and tool-list expectations in `mcp_tools`, `mcp_transport`, `mcp_write_transport`; docs: `docs/knowledge/security/mcp-server.md`, `agent-access-tokens-mcp-remediation-context.md`. No migration, no new scope, no SQLx metadata change.
- Owner-authorized CI simplification (scope exception recorded 2026-11-26): see the MR description.
- Backlog records: TASK-542, TASK-543, TASK-546, TASK-547, TASK-548.

### Acceptance criteria and evidence (database tests run in CI job database-tests, pipeline 3121240685)
1. Scope and visibility: `list_system_scans` uses `scoped_read` (live token/owner recheck, one REPEATABLE READ snapshot). Tests: hidden and unknown system give not_found; a `poams:read`-only token gives forbidden.
2. Sealed history, newest first, with commit, completion time, scanner, counts, is_head, is_runtime_authoritative. Same relation as explicit `scan_id` in `list_scan_findings`. Tests exclude legacy, mock, pending, failed, other-configuration and other-flake scans.
3. Commit filter: full hash or 7-40 hex digits, any case, no whitespace (matches the JSON Schema). Tests: prefix, full hash, short, non-hex, `%`, leading/trailing/inner whitespace, newline.
4. Cursor: keyset on (completed_at DESC, scan_id DESC), bound to system and commit filter. Tests: paging without loss or duplication including three scans with equal completion time across a page boundary; rejection for another commit filter and another visible system; malformed cursor.
5. `latest_sealed_scan`: newest sealed scan or null; never changes state, selection, findings or planning eligibility. Tests follow the returned `scan_id` and assert exact findings, versions, counts and provenance, and that other scans' findings are absent; null for empty history.
6. Authority: `is_runtime_authoritative` only from `view_current_cve_authority`; tests cover configuration-only history, HEAD, positive runtime authority on HEAD, and historical runtime authority independent of HEAD. `is_head` = evidence of HEAD's proven execution derivation; tests cover several scans of the HEAD derivation and execution reuse from an earlier commit. Planning baseline and POA&M closure rules are unchanged (no write path touched).
7. Docs and tool descriptions match behavior; Admin MCP card shows the tool (browser check below).

Results at 28dedaef (pipeline 3121240685, child 3121288108, all jobs success):
- database-tests (retry job 18417436572): state-machine 273 selections, 847 passed, 0 failed; every mcp_* target exit 0 (incl. mcp_scan_findings).
- database-launcher-tests: CVE + dashboard 35 selections, 47 passed, 0 failed; 16/16 fault-harness cases.
- component-tests (retry job 18417436709): core library 1637 passed, 0 failed, incl. `handlers::ui_paths` tests.
- complexity-check, docs-site-check, integration-pipeline and all 5 integration scenarios: success.
- First attempts that failed (not product failures, not retried by the agent): component-tests 18415029361, TASK-543 ETXTBSY flake in `scan_inputs_restore_independently_across_cache_sources`; database-tests 18415029362, 2-hour timeout on runner webb (TASK-547).
- Admin MCP card, browser check on the farm-built embedded server at 28dedaef (isolated PostgreSQL, mock mode, stopped after): 25 tools incl. `list_system_scans`, 6 rows, no overflow, dark and light screenshots attached to the MR.
- okf-knowledge: 202 concepts validated (last run on the docs at 8867853f; later commits change one guide paragraph and code comments).
- Coverage configuration: all four component `.coverage.profile` outputs evaluate; cf-keygen's built on the farm with `profile.profdata`; coverage-check inputs match their producers.

### Known limits
- TASK-543 (ETXTBSY test-fixture race) and TASK-547 (webb runner ~10x slower, no Niks3 publication, single-job runners) caused the two retried failures.
- TASK-546: intermittent VM test driver `Bad file descriptor`.
- TASK-548: core and embedded server builds still compile three shared crates separately.

<!-- SECTION:NOTES:END -->
