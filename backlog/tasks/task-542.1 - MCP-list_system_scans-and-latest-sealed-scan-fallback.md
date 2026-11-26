---
id: TASK-542.1
title: MCP list_system_scans and latest sealed scan fallback
status: In Progress
assignee: []
created_date: '2026-11-24T14:39:00Z'
updated_date: '2026-11-26T21:18:41Z'
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
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-542.1-mcp-list-system-scans

## Verification (2026-11-24, uncommitted worktree head on ef3a14bb+task commits)

- Farm, PostgreSQL-backed targets in full (throwaway override of cf-server-core-drv check phase with a sandbox PostgreSQL, /tmp/opencode/cf/dbtests.nix, SQLX_OFFLINE=true): mcp_compliance_read 7, mcp_policy_authoring 17, mcp_scan_findings 6 (new: system_scans_list_sealed_history_and_head_fallback), mcp_tools 8, mcp_transport 10, mcp_write_transport 9 passed, 0 failed, exit 0. The run predates a comment-only edit in scan_inventory.rs.
- Farm nix build .#default.cf-server-core-drv: lib tests 1634 passed, 1 failed in each of two runs; the new unit tests commit_filter_accepts_hex_prefixes_only and scan_cursor_is_bound_to_system_and_commit passed. The single failure was a different unrelated fake-executable test each run (ETXTBSY fixture race, tracked as TASK-543). The package build is therefore NOT green; post-check fixtures did not run.
- nix build .#okf-knowledge -L: exit 0, 202 concepts validated (after replacing an ASCII arrow chain in TASK-542 Goal with a numbered list).
- rustfmt 1.9.0 (flake nixpkgs) --edition 2024 on changed Rust files: clean. The default dev shell has no cargo fmt.
- clippy: not run; no clippy gate exists in the flake or CI.
- No migration, no SQLx offline metadata change (runtime query_as only), no browser-visible change, integration-access not affected (no scope change).

Design note: is_head uses HEAD's proven execution derivation (same rule as configuration_head_cve_scan), not commit equality, because execution reuse can make the HEAD scan's commit earlier than HEAD.

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/356 (Draft, targets dev). Lock released: awaiting review. Pushed via ssh://altssh.gitlab.com:443 because port 22 to gitlab.com timed out from vm-agent.

2026-11-24 Returned to In Progress for MR !356 maintainer review findings (8 items). LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-542.1-mcp-list-system-scans

## Verification record, review round 2 (2026-11-24)

Code and tests verified at commit `4436e8696ecc215615c17bde705a44a9817689a8`. Later commits on this branch change only this task file. Farm = the configured Nix build farm; nothing compiled locally.

### Passed
- **MCP database targets, all nine, in full, single-threaded as in `database-regressions.sh`**: `mcp_bundle_authoring` 12, `mcp_compliance_read` 7, `mcp_policy_authoring` 17, `mcp_scan_findings` 8, `mcp_tools` 8, `mcp_transport` 10, `mcp_write_poam` 13, `mcp_write_scan` 8, `mcp_write_transport` 9: 92 passed, 0 failed. Plus `cargo test --lib -- mcp::` 26 passed. Exit 0. Command: a throwaway farm derivation (an override of `.#default.cf-server-core-drv` that starts a sandbox PostgreSQL, sets `SQLX_OFFLINE=true`, runs `cargoWithProfile test --locked --package cf-server --test <target> ... -- --test-threads=1`). It is not committed and the source tree is the committed tree.
- **Embedded package** `nix build .#default.cf-server-drv --no-link -L`: exit 0, library tests 1637 passed, 0 failed. This package has no post-check fixtures.
- **`nix build .#okf-knowledge --no-link -L`**: exit 0, 202 concepts validated, UNCLASSIFIED=0.
- **rustfmt 1.9.0 (flake nixpkgs) `--edition 2024`** on the changed Rust files: clean after formatting.
- **clippy, on the farm**, same lint flags as `packages/code-metrics` (`-Wclippy::complexity -Wclippy::cognitive_complexity -Wclippy::too_many_arguments -Wclippy::too_many_lines -Wclippy::type_complexity -Wclippy::fn_params_excessive_bools -Wclippy::vec_box`), `cargo clippy --locked --package cf-server --all-targets --message-format=json`: exit 0, build-finished success, no errors, at head and at base `ef3a14bb`. Unique complexity diagnostics crate-wide: 675 at base, 675 at head. Diagnostics in the files this slice changes are identical at base and head except line numbers: `mcp/mod.rs` cognitive_complexity 36/25 and collapsible_if, `mcp/tools.rs` cognitive_complexity 26/25, `mcp_write_transport.rs` too_many_lines 105/100, `scan_inventory.rs` `list_scan_findings` too_many_lines **178/100 at base, 194/100 at head** (already over the limit; this slice adds 16 lines to it). No new lint. How clippy was run: see the exception below.
- **Admin MCP Server card** (review item 6): see the MR description. Rendered from the farm-built embedded server at `4436e869` (isolated PostgreSQL on port 55433, server on 3446, both stopped afterwards) with Playwright against `/admin` > Server: 25 tool chips including `list_system_scans`, 6 wrapped rows, 0 chips outside the list, dark and light themes correct. The card source is unchanged by this MR.

### Not passed: native core package `nix build .#default.cf-server-core-drv --no-link -L`
Exit 1 in every run at this head. Results are infrastructure failures in tests that this slice does not touch; no MCP test failed. I did not weaken, skip or retry-until-green these tests, and did not build on the local host.
- Run C (first attempt aborted by a farm transport error, "Socket closed", which restarted the build): second attempt, library tests 1619 passed, 16 failed. All 16 returned `503 Nix syntax validation unavailable: the Nix parser did not finish within 2 seconds`. Tracked as TASK-526.
- Run D (retry): library tests 1616 passed, 19 failed: the same 16 Nix-parser-deadline tests, plus `derivations::cache::publication_signing_tests::selected_publication_key_signs_resolved_root_and_propagates_failure`, `builder::cve_worker::tests::scan_inputs_restore_independently_across_cache_sources` and `handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition` (`Text file busy (os error 26)`, `NotFound`, `Failed to spawn nix copy`): the fake-executable race of TASK-543.
- Earlier runs on this branch (before this round): 1634 passed, 1 failed, a different TASK-543 test each time.
- The 16 failing policy and interchange tests: `handlers::api::deployment_policies::tests::{validate_policy_config_accepts_non_reserved_field_names, validate_policy_config_accepts_valid_custom_check, validate_policy_config_auto_fixes_legacy_cfg_prefix, validate_policy_config_handles_complex_expression, validate_policy_config_ignores_superseded_single_expression_fields, validate_policy_config_normalizes_multi_rule_expressions}`, `models::deployment_policies::tests::{ac3_validation_matrix_accepts_and_rejects_each_exposed_kind_discriminately, async_policy_validation_matches_sync_semantics_without_panicking, composite_missing_evaluator_evidence_is_error_and_reporting_is_not_checked, composite_round_trip_preserves_ids_order_kinds_and_semantic_values, composite_single_expression_helper_returns_explicit_error, composite_validation_rejects_non_pname_package_contract_and_bad_custom_syntax, strict_report_only_nix_failure_retains_evidence_without_blocking}`, `handlers::api::compliance::tests::composite_json_toml_and_cf_native_interchange_preserve_all_supported_rules`, `queries::compliance_interchange::tests::{authentic_v1_digest_is_verified_before_current_normalization, v2_custom_check_accepts_bare_paths_and_uri_literals}`.
- Likely contributor, not proven: other sessions were building on the same farm at the same time (a TASK-544.1 worktree), and my own clippy run overlapped Run C. The post-check fixtures of the core package did not complete in any run.
- Impact: the native core package check is **unverified** at this head. The new unit tests `commit_filter_accepts_hex_prefixes_only`, `scan_cursor_is_bound_to_system_and_commit` passed in these runs and in the throwaway `mcp::` run.

### Clippy exception
The flake and CI define no farm-run clippy gate. `.#code-metrics.complexity-report` is a shell application (`bin/complexity-report`); building it only builds the script, and running it calls `cargo clippy` on the local host, which AGENTS.md forbids. I ran clippy through a throwaway farm derivation with the repository's lint flags and recorded base-versus-head results above. That derivation is not an approved gate and is not committed. Decision needed from the maintainer: accept this run as the clippy evidence, or define a farm clippy gate in a separate task.

### Remaining limitations
- Native core package check not green (above). The embedded package and the MCP database targets are green.
- Exact-head GitLab pipeline result: see the MR description.
- The card check used a mock-mode isolated server, not the dev server, and its screenshots were not attached to the MR (the capture directory in `/tmp` was cleared during the session). No UI code changed.
- No live call of the new tool on the dev server yet.

### Correction to the clippy statements in this task (2026-11-24)
Earlier notes say no clippy gate exists in the flake or CI. That is wrong. CI job `complexity-check` (stage `metrics`, .gitlab-ci.yml) runs `.#code-metrics.complexity-report`, which runs `cargo clippy --all-targets --all-features` with the complexity lints on `packages/default` and `packages/web-ui` on the CI runner. That job is the declared clippy gate. It cannot run on the agent host (AGENTS.md forbids local compilation) and building `complexity-report` on the farm only builds the shell script, so the hosted job is the evidence.
Hosted pipeline results for MR !356 (GitLab API, https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/<id>):
- 3110275427 at 1bb3219e: component-tests, state-machine-tests, integration-access, integration-assurance, docs-site-check, dashboard-visibility-tests, cve-processing-test success; integration-delivery and integration-upgrade failed; complexity-check skipped (an earlier job failed). Failure reasons not examined.
- 3110276535 at fb11f971 (same code as 1bb3219e, task-file commit only): all 10 jobs success, including complexity-check, component-tests (native core package) and state-machine-tests (MCP database targets).
- 3111546242 at 6b7f7f4e (this round): running when this note was written.
The clippy exception in the previous note is withdrawn: the farm clippy run (base 675 and head 675 unique complexity diagnostics, none new) is supplemental evidence, and the gate result is the `complexity-check` job of the pipeline for the final head.

## Owner scope exception (2026-11-26)
The owner explicitly extended MR !356 to include a CI performance and infrastructure refactor, as an exception to the AGENTS.md one-slice-per-MR rule (asked and answered in session). The CI work goes on this branch in separate logical commits (cache policy, pipeline scheduling, Rust compilation reuse, integration scenario scheduling). The original TASK-542.1 acceptance criteria are unchanged. !356 stays Draft until both the original criteria and the CI verification requirements are met.
Cache trust decision (owner, 2026-11-26): all CI jobs may substitute from Niks3 and cache.nixos.org; only pipelines on protected refs with a protected CI variable may publish to Niks3. MR pipelines never publish.

## CI simplification (2026-11-26), head 342a8458

Removed: ci-selftest job; packages/ci/nix-ci.sh, packages/ci/tests/*, packages/ci/pipeline-timings.py, packages/ci/integration-pipeline.sh; the prebuilt test-executable derivation, prebuilt-cargo-test.sh and its test, the three Rust fixture path changes; committed ci-metrics/nix.jsonl; the pre-existing public-cache-build.sh and its unused 363-line test. CI infrastructure vs base b760157a: 8 files, +194 / -684. Product code: unchanged by CI work.

Kept/changed: plain nix (runner daemons already use Niks3 + keys + farm); cf-server-drv doCheck=false; integration-pipeline builds cf-server-drv/agent/builder then generates the scenario child pipeline (one nix eval, explicit rules); preferLocalBuild on scenario VM runs; CVE+dashboard share one job (database-launcher-tests); state-machine runs alone (database-tests); interruptible jobs + auto-cancel; complexity-check needs [].

Measured (GitLab API):
- Baseline successful 3110276535 / 3100779917 / 3099949246: wall 205 / 186 / 183 min; component-tests 38 / 44 / 37; state-machine 43 / 42 / 41; complexity 13 / 16 / 23.
- 3120511772 (warm): component-tests 7.0, complexity 9.4, docs 0.3, integration-pipeline 0.3, database-launcher-tests 8.8 (baseline CVE+dashboard 12 runner-min); scenarios access 7.5, assurance 25, delivery-lifecycle 31.9, failure-reporting 7.6, upgrade 30.3; all scenarios finished 52 min after creation. database-tests on webb: timeout at 122 min -> pipeline failed.
- 3120173363 (warm): same picture; database-tests on webb timeout at 120 min.
- 3119599541 (cf-server-drv cold): 119 min, failed by TASK-546 driver flake; database merged job 53 min on sledge.
Cross-runner substitution: cf-server-core-0.3.0 (drv s4px..., out yma0...) built on runner 31882425 (pipeline 3111548109), substituted from Niks3 (sig niks3-campground-1) by runner 40527293 in 3119599541.
Blocker: webb (runner 59818941) runs the state-machine suite ~10x slower (access_tokens run ~29 min vs ~2 min; migration phase 104-107 s vs 6 s). All previous state-machine runs were on sledge/lucas (41-53 min). Infrastructure, not changed here.
<!-- SECTION:NOTES:END -->
