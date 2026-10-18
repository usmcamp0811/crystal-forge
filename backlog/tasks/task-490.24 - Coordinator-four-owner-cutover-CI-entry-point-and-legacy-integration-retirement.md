---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 19:29'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.1
  - TASK-490.2
  - TASK-490.3
  - TASK-490.4
  - TASK-490.5
  - TASK-490.6
  - TASK-490.7
  - TASK-490.8
  - TASK-490.9
  - TASK-490.10
  - TASK-490.11
  - TASK-490.12
  - TASK-490.13
  - TASK-490.14
  - TASK-490.15
  - TASK-490.16
  - TASK-490.17
  - TASK-490.18
  - TASK-490.19
  - TASK-490.20
  - TASK-490.21
  - TASK-490.22
  - TASK-490.23
  - TASK-490.25
  - TASK-490.26
  - TASK-490.27
  - TASK-490.28
  - TASK-490.29
modified_files:
  - .gitlab-ci.yml
  - packages/ci/integration-owner-build.sh
  - packages/ci/test_public_cache_build.py
  - .gitignore
  - AGENTS.md
  - checks/integration-access/default.nix
  - checks/integration-access/README.md
  - docs/knowledge/testing/integration-coverage-map.md
  - docs/knowledge/testing/integration-test-contract.md
  - docs/knowledge/testing/flake-checks.md
  - docs/knowledge/testing/web-ui-check.md
  - docs/knowledge/testing/test-plan.md
  - docs/agents/verification.md
parent_task_id: TASK-490
priority: high
type: task
ordinal: 536000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Coordinator-owned four-owner completion and source retirement. Final checks.x86_64-linux must contain exactly these FOUR TOTAL attributes: integration-access, integration-delivery, integration-assurance, integration-upgrade. Native Snowfall checks/<owner>/default.nix registration remains unchanged. Four check attributes does not mean only four verification commands.

The owner's source-retirement instruction explicitly supersedes both legacy-source-retention-until-coverage and the interim manual-job policy. Delete the nine named legacy suite directories now after inspecting their consumers. Their accepted product obligations remain in the coverage map as uncovered where no new scenario passes. Do not reproduce the old suites one-for-one or treat their inventory as requirements. The accepted contract/current CF architecture defines integration coverage.

Selected source-removal milestone: nine legacy suites and optional CI jobs removed; useful host browser/design assets relocated to packages/devScripts/web-ui; two XCCDF inputs moved to checks/integration-assurance/fixtures/xccdf without a placeholder owner; 17 non-integration definitions relocated as native package outputs (verify-* except ui-screenshots and okf-knowledge). Checks output decreases from 27 to 1, integration-access only. Its 16 cases remain intact. No final four-owner completeness claim.

Lower-level paths retain useful assertions: verify-server-regressions preserves the exact Rust integration/ignored selections and isolated postgresqlTestHook with doCheck=true. It remains automatic through package-verification. Existing cve-processing-test, dashboard-visibility-tests and state-machine-tests retain disposable database environments. Schema/documentation/packaging/tool verification runs as explicit packages, not flake checks. No custom registry/discovery/framework.

Remaining complex cleanup here: compare ignored commit selections in packages/verify-server-regressions/default.nix and packages/devScripts/default.nix state-machine-test. Choose one primary runner for exact overlaps; preserve distinct test environment evidence or record a reason. Dependency: complete exact selected-test mapping and validate the surviving isolated DB path before removing duplicates. Generic packages/cf-test-suite remains developer tooling; only accepted obligations justify new owner cases, not historical test inventory. No new audit task family.

Final construction remains six planned scenarios across four owners. Extend compatible same-scenario cases; add a VM boundary only for required machine configuration, lifecycle or isolation. Missing owners, forms, notifications, OIDC, cross-environment scope and closed registration remain open under the existing child tasks and their applicable decisions. TASK-492/493/494 do not authorize product fixes. No merge, deployment, production/schema/design change or Done transition.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Automatic integration-access builds its normal owner aggregate through the existing cache wrapper/resource limits and propagates failures; never use a .run output as the gate
- [x] #2 Source retirement removes checks/integration, oidc-auth, niks3-cache, niks3-write-probe, niks3-ui, attic-metrics, attic-publication, web-ui and web-ui-reconciliation after external consumers are relocated; obsolete legacy CI jobs and dead asset references are removed
- [ ] #3 Lower-level Rust/database/package/schema/documentation/tool verification has explicit working package or CI execution paths outside flake checks; preserve isolated PostgreSQL and normal package tests
- [x] #4 Owner evidence exports real dereferenced files with artifacts always; failed-build collection processes unterminated last records, preserves the original status after copy errors, never rebuilds .run, and a successful build with missing evidence fails
- [x] #5 Contract, retirement map, verification guide and task records distinguish executed coverage, required gaps, obsolete suites, and lower-level verification retained outside checks; retirement does not mark missing obligations covered
- [ ] #6 Evaluate the actual flake inventory and changed package/import/CI consumers; run retained tooling/schema/doc checks and normal Access CI execution, recording revision, result and evidence reuse/execution
- [ ] #7 FINAL: nix eval .#checks.x86_64-linux --apply builtins.attrNames --json returns exactly integration-access, integration-delivery, integration-assurance and integration-upgrade, with no other attributes
- [ ] #8 FINAL: all four native Snowfall owner aggregates build every explicitly registered scenario gate and prove the complete accepted coverage; no placeholder owners
- [x] #9 FINAL: all replaced legacy definitions and obsolete manual jobs are removed; preserve necessary assets at their owner/package locations; historical test inventory does not create mandatory product requirements
- [ ] #10 FINAL: useful assertions have primary execution ownership; repeated Rust/database work is consolidated or justified by distinct execution evidence; developer tools use packages/apps and no non-integration flake check attribute remains
- [ ] #11 FINAL: retain the six-scenario starting topology with each extra machine/lifecycle/isolation boundary justified; report measured runtime/resources and the actual final inventory, not merely CI job names or four wrapper aggregates
- [ ] #12 Coordinator owns shared harness, composition, registration and CI; owner-local case fragments reuse same-scenario receipts only; no root tests/, custom discovery, registry or new test framework
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Clean-slate consolidation iteration selected by latest owner request. Baseline2c34186e, clean existing worktree, same coordinator lock. Earlier one-for-one verify package relocation is rejected and superseded.
1. Review all15verify packages for unique assertions vs Rust/component/CI owners; measure exact duplicate selections and before drv inventory.
2. Eliminate verify public outputs. Keep unique component assertions in corresponding ordinary build/test mechanisms or explicit existing CI commands; no new registry/aggregator. Consolidate PostgreSQL regressions into existing isolated state-machine workflow, remove exact repeated work and preserve SQLx privileges/migration rehearsals.
3. Inventory actual API, Dioxus, configuration, agent/builder/cache/evidence/upgrade product surfaces against accepted authorities; update existing coverage map with truthful dispositions, exact decisions and no old suite authority.
4. Preserve and execute existing Access scenario. Check dependencies and implement one next runnable existing child coverage slice (Access form success if Delivery prerequisite roles cannot be completed safely in this bounded iteration).
5. Verify all real commands/statuses; record check/package/job counts, derivation reduction and repeated work removed. Commit bounded changes to draft !335; keep parent and final requirements incomplete.

Database worker scope: exclusively own packages/devScripts/default.nix and new packages/devScripts/tests database shell scripts; remove packages/verify-server-regressions only. Preserve six broad state-machine ignored filters, all 14 integration targets, both populated migration rehearsals, encryption fixture, one-case mock config and exact missing-test guards. Independently audit six duplicate ignored selections and 94 normal-lib selections before removing them. Use fresh disposable dedicated PostgreSQL path/port with LOGIN SUPERUSER CREATEDB and pg_stat_statements; export both database URLs in sqlx shell and share Cargo target compilation. Verify shell/Nix syntax and actual program evaluation, selection preservation and minimal real isolated DB smoke. Stage only owned files; no CI/docs edits or commits. Coordinator handles runtime assertion integration requests.

Remaining-eight delegated scope: remove owned verify outputs; preserve pure builder/STIG module assertions as explicit boolean evaluations under modules tests, package metadata assertions in nixos-options-metadata and OSCAL schema validation in oscal-fixture checkPhase. Preserve discovery as explicit isolated runtime command and packaging wrapper/evaluator/runtime-module assertions as explicit commands without replacement package derivations. Audit normal Rust tests; provide exact coordinator CI/devScripts build snippets. Stage only owned paths, no commit.

Delegated five-package slice in existing locked TASK-490.1 worktree: inspect actual Rust assertions; remove verify-config-inspector/config-observer/evaluator-snapshot-isolation/verified-source-evaluator-parity/niks3-ca-bundle public definitions. Preserve unique real Nix protocol tests as cf-server core-only postCheck fragments under existing component tests, retain locked source inputs, and move Go/Rust CA dataset compatibility into core check execution. Select cf-config normal library tests once in core (current cf-server-only selection does not run dependency tests), removing separate CA Rust runner. Evaluate component drvPaths and real dependency inventory; validate through Nix with persistent logs for expensive builds. Stage only this slice; no commit, CI/docs/devScripts or production changes.

Additional assigned XCCDF slice: exclusively delete verify-xccdf-schema definition/README and orphan verify-server-regressions README; add cf-server/tests/xccdf-schema.sh taking installed core, XCCDF schema directory and CF schema file. Retain actual writer-generated XCCDF/OpenSCAP validation and all five CF extension node types with non-vacuous extraction. Remove handwritten example documents and structural duplicates after writer/parser Rust audit. Verify actual installed fixture binary if available; no default.nix edits, stage only assigned paths.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Current source-retirement result (2026-10-18)

Implementation commit 0105e512baa80ace5f181918bbb72951bf506a50 in existing TASK-490.1-integration-foundation worktree/branch and draft MR !335. The owner's explicit source-retirement instruction supersedes all prior legacy-source and manual-coexistence retention rules. Task remains In Progress; no merge/deploy/Done claim. LOCK: openai on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation.

Actual inventory command `nix eval .#checks.x86_64-linux --apply builtins.attrNames --json`: ["integration-access"]. Before: 27. After: 1. The final exact four-name test is NOT satisfied: Delivery, Assurance and Upgrade have no default.nix. Retained Assurance XML assets do not register a placeholder.

Deleted all nine legacy suites: checks/{integration,oidc-auth,niks3-cache,niks3-write-probe,niks3-ui,attic-metrics,attic-publication,web-ui,web-ui-reconciliation}. Deleted orphan lib/cache-test-node and unused test-key placeholders after checking consumers. Old suite VM/test implementations, VM-only validators and baseline artifacts are removed, not copied into new owners. Historical source is in Git at 5fa1f4fa.

Assets retained: 15 host browser/design tooling files under packages/devScripts/web-ui (runner, monolithic developer workflow script and its imported modules, fixture/manifest, generator/tests, capture runbook). External devScripts/design-target package/app consumers updated. Both XCCDF XML revisions moved to checks/integration-assurance/fixtures/xccdf; the developer harness reads V1R2 there. Approved design sources remain unchanged. Retired OIDC realm copy is unnecessary: developer OIDC stack owns its separate realm.

All 17 nonintegration definitions moved to native package outputs. Fifteen are packages/verify-<former-name>; ui-screenshots and okf-knowledge keep their existing package names. The source directory table is in docs/knowledge/testing/flake-checks.md. No standalone nonintegration definition remains under checks. No registry/custom Snowfall discovery/new framework. Three assertion groups are forced in their existing build-command argument rather than before namespace construction, preserving expressions while avoiding recursive package lookup.

CI: removed legacy-flake-check (seven entries), web-ui-check and web-ui-baseline-candidates. Renamed retained six-entry matrix to package-verification with explicit package targets and unchanged runner/auth/local limits/cache-wrapper selection. Existing cve-processing-test, dashboard-visibility-tests, state-machine-tests retain their isolated DB launchers. Coverage/complexity/release unchanged.

Verification passed: all17 drvPaths; ten lightweight contract/tool package builds; CA compatibility (13/13 selected Rust tests and three matching identity reports); OSCAL and builder module validation; .#okf-knowledge; task-local nix develop, browser runner selftest, generator tests/static contracts, syntax; updated CI routing/cache-policy script; GitLab lint (valid, no errors/warnings, correct MR/main only rules); git diff --check. No stale executable moved-check path remains.

Access: `sh packages/ci/integration-owner-build.sh integration-access` exited0 at clean 0105e512. VM EXECUTED, not reused;16/16 cases passed and Viewer browser result passed. Existing assertions and registration unchanged. VM script24.23seconds. Evidence /nix/store/n68lcq6xxab1rhhfymp7yz4pj19hff95-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run plus integration-evidence/integration-access/. Documentation-only follow-up does not require rerunning VM.

Incomplete verification: full verify-server-regressions exceeded600seconds after isolated PG started, migration-prefix rehearsals succeeded, assignment_semantics13/13 and zombie-repair1/1 passed, and composite_policy continued. No full pass claimed. Its default.nix is byte-identical (Git blob8dbc8cfa410ad148f39def97a7e40c5f43374cce) and drvPath unchanged (cj9yfsfs54v821m2zv659lj8wbl9pxl1-crystal-forge-server-regressions-0.1.0.drv). Full builder-evaluator-packaging/XCCDF command also exceeded600seconds compiling core server dependency. Preserve these visible gaps; obtain longer build window or exact-head CI. AC#3/#6 remain unchecked for full verification; do not move task Review.

Remaining complex migrations (THIS task, no new task family): exact ignored-test overlap mapping between packages/verify-server-regressions/default.nix and packages/devScripts/default.nix state-machine-test; assign primary owner and validate surviving isolated DB environment before removing duplicate invocations. Existing Rust/crate test execution should own unit assertions. Package relocation removes check attrs, not duplicated execution. No historical Python test inventory is automatically a product requirement. Accepted owner gaps continue through existing .2-.23/.27-.29 tasks with six planned scenarios; no extra VMs added.

## Historical milestones

Access bootstrap first failed at fc930335; owner-authorized .25/.26 configuration repairs (52128b3a,76e51464) enabled bootstrap. Sessions passed at81d65be7 and CI-path2c494ac4. Authorization extended same scenario to16 cases at8a857bc8; hosted job passed at08a79037. No unrelated product repair occurred; TASK-492/493/494 remain separate. Collector fixes6d7e7a92 handle unterminated listing records and copy-error status preservation (15/15 temporary shim cases passed). Earlier manual-job and provisional check-attribute retention policies are superseded, not current.

Database consolidation worker handoff (2026-10-18, worktree TASK-490.1-integration-foundation, baseline 2c34186e). Owned changes staged only: packages/devScripts/default.nix; packages/devScripts/tests/{database-environment,database-regressions,state-machine-environment,state-machine-tests}.sh; removal of packages/verify-server-regressions/default.nix (Git detects extraction as rename). No commits or CI/docs edits by this worker. Other workers' staged changes preserved. Exclusive ownership of devScripts/default.nix retained; coordinator must integrate other package worker runtime-check requests after this handoff rather than allowing concurrent edits.

Independent source AND compiled libtest audit: existing six broad filters select 51 ignored tests (28 commits, 5 derivations, 5 finalize_attempt, 11 finalize_system, 1 ordering, 1 migration_0184). Exactly six ignored overlaps removed from extracted regression selections: four queries::commits::tests::derivation_count_* cases; queries::commits::tests::evaluation_start_waits_for_snapshot_writer_before_commit_lock; queries::derivations::tests::cross_commit_shared_path_keeps_distinct_idempotent_rows. Exactly 94 nonignored regression library selections removed because normal --lib --bins tests own them; compiled --list/--ignored confirms all 94 are present and nonignored. Regression library execution selections 333 -> 233; combined with broad groups 384 -> 284, with 284 unique ignored library tests retained. Removed two exact composite_policy selections from CVE runner; whole composite_policy target retains both. All 14 integration targets retained, including all seven exact register_export_selection cases. Both exact-success guard sites retained. Both populated migration rehearsals compare byte-for-byte with original after only shell interpolation unescaping and dedicated PGPORT URL correction. Authored public encryption fixture, one-case worker mock config, security/data-integrity/concurrency selections and captured Cargo stderr failure diagnostics retained. Audit command: nix run nixpkgs#python3 -- /tmp/opencode/task490-db-audit.py (exit0).

State-machine app remains existing process-compose CLI, without new exported runner/package/registry. Existing module override now uses port35433, loopback-only, fresh mktemp-owned data/state-machine-db, LOGIN SUPERUSER CREATEDB and pg_stat_statements preload. Launcher cleans only its temporary root after process-compose shutdown; inner sqlx shell exports both database URLs, checks actual server data_directory/role/preload before mutation and uses one target/database-tests cache for every selection. Runtime URLs have no passwords and are not placed in shell argv. Development port3042/database data untouched.

Verification exit0: bash -n four owned shell scripts; nix run nixpkgs#shellcheck -- four scripts (inside nix develop .#sqlx); git diff --cached --check on owned paths; nix eval .#devScripts.state-machine-test meta/program and drvPath; nix build .#devScripts.state-machine-test --no-link. Resulting drvPath /nix/store/bfnx0ws5jdssrn8iwzgjmy0sidfn8732-process-compose.drv. Occupied35433 invocation rejected exit1; live wrong-data-directory attempt rejected exit1 before migrations.

REAL smoke executed using existing generated PostgreSQL process-compose config plus temporary command override, same launcher and database-environment.sh within nix develop #sqlx. Command: nohup nix run .#devScripts.state-machine-test -- -f /nix/store/5x5hajyx1x2j55x0b4nl6zz8jb2wcmmg-process-compose-process-compose.json -f /tmp/opencode/task490-db-smoke.json -t=false up > /tmp/opencode/task490-db-smoke.log 2>&1 < /dev/null &. Actual status /tmp/opencode/task490-db-smoke.status =0. Proved LOGIN SUPERUSER CREATEDB, real createdb/dropdb, CREATE EXTENSION pg_stat_statements and query, all current migrations, and cargo test --offline -p cf-server --lib queries::commits::tests::derivation_count_markerless_legacy_is_unavailable -- --ignored --exact --test-threads=1 --color=never: 1 passed,0 failed,0 ignored,2594 filtered,4.39s. Initial compilation5m16s. Cluster shut down and temporary root cleaned.

FULL workflow launched detached without tool timeout: nohup bash /tmp/opencode/task490-db-full.sh > /tmp/opencode/task490-db-full.log 2>&1 < /dev/null &; script runs nix run .#devScripts.state-machine-test -- -t=false up and records final exit in /tmp/opencode/task490-db-full.status. Started19:03:23Z. Both populated upgrade rehearsals PASSED, phase exit0 in6s at19:03:39Z. Currently compiling first integration target assignment_semantics (process-compose PID17630). Full pass NOT claimed; coordinator should inspect status/log after completion. Keep TASK-490.24 In Progress and acceptance criteria incomplete pending full coordinated verification.

Five-package component slice staged in TASK-490.1-integration-foundation; no commit. Owned files: packages/default/default.nix; cf-server/tests/nix/{config-inspection,config-observation,primary-isolation,source-parity}.nix and relocated verified-source fixture (flake.lock force-added because global ignore); cf-config/tests/ca-bundle/{compatibility.nix,compare.go}; five retired verify directories. Existing production Rust source/tests unchanged. Removed unused inspector/fixture/flake.nix (no consumers) and retired CA package README.
Assertion mapping: Inspector retains real Nix >30000 jobs, capability failure, indexed lookup/no quadratic guards, shallow shapes, partial diagnostics/sibling isolation, 128 diagnostic truncation with continued traversal, root error, stage1/stage2 target/source/carrier/digest identity,3067 definition jobs,legacy25.05 unsupported adapter,poison value isolation,replay/hidden-module/modulesLocation evidence,and priority/condition/import/disabled/order/merge semantics. Rust models/config_inspector.rs tests expression_selects_one_configuration_and_uses_selected_system_library,reconciliation_is_order_independent_and_preserves_value_failure,partial_provenance_uses_exact_observed_set_beyond_diagnostic_budget,successful_carriers_must_share_one_drv_path,stage2_rejects_carrier_mismatch_even_when_stage2_is_self_consistent etc. inspect source/synthetic JSON, not the real Nix producer. All meaningful assertions retained core postCheck config-inspection.nix.
Observer retains real configured classification matrix(defaultOnly/ordinary/mkDefault/mkForce/generated/losing/tie/defaultless/throwingApply/nestedPoison),ambiguous diagnostic,laziness,no lock writes,unavailable sibling,root/prefix/option,nested failure redaction,512 pagination order,513->512 provenance truncation,locked narHash pure evaluation,common carrier. services/config_observations.rs generated_expression_embeds_validated_structured_selection_for_pure_eval,reconciliation_accepts_every_operation_payload_contract,classifier_error_records_stay_partial_diagnostics_with_a_usable_index,observer_command_uses_only_closed_server_owned_arguments,shallow_command_uses_direct_nix_eval_without_a_derivation_carrier inspect source/commands/reconciler,not real evaluator results. Retained core config-observation.nix.
Snapshot isolation retains actual pure nix-eval-jobs single managed result,untouched aborting metadata/options/graph/unmanaged sibling/module,architectureGate/cfAgentEnabled/revision metadata. deployment_policies.rs scoped_primary_expression_filters_before_attribute_evaluation and primary expression tests inspect strings; generated_evaluation_expression_parses_when_nix_is_available explicitly returns in NIX_BUILD_TOP. Retained core primary-isolation.nix.
Parity retains real canonical Git archive executable/symlink/export-subst,linked CLI/evaluator version,pure server job drvPath==direct CLI despite ambient environment/path,and impure drvPath difference. verified_store_source_uses_only_the_explicit_revision_override/fingerprint/archive unit tests do not execute both evaluators. Removed two tautologies: canonicalSource==itself and same path hash==itself. source-parity.nix retains locked fixture bytes under tests/nix/fixtures/verified-source.

CA audit: old runner selected --package cf-config --lib ca_bundle::tests; ordinary --package cf-server --lib --bins did NOT select dependency tests. Core now selects `cargo test --release --locked --package cf-server --lib --bins --package cf-config`, no feature flags. Embedded remains --package cf-server --features cf-server/embedded-ui --lib --bins, no new postCheck. Removed standalone filtered CA invocation. Normal core library run owns the 13 unchanged CA tests once.
Exact 13 tests: framing::{ordinary_roots_are_preserved_canonical_and_idempotent,ca_delimiters_reject_indentation_before_root_selection,ca_column_zero_delimiters_keep_go_compatible_whitespace,ca_raw_controls_and_bare_cr_cannot_promote_a_second_root,rejects_unknown_text_private_material_and_malformed_framing,errors_are_static_and_do_not_retain_input}; trust::{base64_shaped_nss_labels_require_a_complete_validated_paired_record,nss_matrix_preserves_122_plain_and_ignores_172_aux_roots,trusted_only_never_becomes_a_go_root_even_with_server_auth,rejects_metadata_certificate_and_trust_contradictions,rejects_trailing_der_noncanonical_lengths_and_unsupported_aux}; client_identity::{identity_pair_is_strict_separate_and_checks_first_chain_certificate,rsa_and_ec_identity_formats_match_but_encrypted_keys_fail}.
Distinct Go-vs-Rust full-DER identity equality on three pinned datasets remains core postCheck compatibility.nix. Builds normalizer example in same Cargo target and obtains executable from Cargo JSON. Condensed redundant Go passes:9->3 invocations; public-X509 OpenSSL parses1758->879, comparator assertions unchanged.
Removed all FIVE public definitions, no replacement public package/registry/aggregator. Removed7 explicit derivations:5 roots+CA runner+CA fixture. Retained28 local data helpers:6 runCommand fixtures(inspector current/legacy,observer unlocked/locked,primary fixture,canonical archive),22 writeText(11 inspection expressions,9 observation expressions,primary flake source,CA provenance). Reasons: error boundaries,locked pure-eval inputs,protocol expressions/provenance,not standalone gates. CA key/Go compilation now happens directly in component check.
Actual nix-store --query --requisites union baseline core+five roots:5428 drv paths; final core4432,reduction996. Baseline core alone3435(new core incorporates prior independent work,+997);old five roots alone4020. DAG counts,not runtime estimates. Embedded drvPath unchanged /nix/store/9n5295lsapwxw6qvsgpdii9jdb2d1pal-cf-server-0.3.0.drv. Final core /nix/store/mi1inp9dp6lqx9wq9s2ba40pc2qs1pfy-cf-server-core-0.3.0.drv. Explicit postCheck fixtures excluded from Cargo source identity; embedded and dependency-only artifacts not invalidated.

Verification: removed-attribute eval returned [];core/embedded drvPaths evaluated. Owned staged diff --check passed. `nix develop .#default.cf-server-core-drv -c bash -c 'printf "%s\n" "$postCheck" | bash -n'` exited0. One-off sandbox smoke importing all four Nix fragments passed(status0): /tmp/opencode/task490-config-protocol-smoke.{log,status}; ephemeral verification only,no repository output. Additional exact-core-native-input smoke running: /tmp/opencode/task490-config-exact-protocol.{log,status}.
Full exact staged core build running WITHOUT timeout PID18602: `nix build --no-link -L '/nix/store/mi1inp9dp6lqx9wq9s2ba40pc2qs1pfy-cf-server-core-0.3.0.drv^*'`. Logs/status /tmp/opencode/task490-config-core-staged.{log,status}. Still compiling; no full pass claimed. Earlier superseded builds interrupted during compilation,not successful evidence. Full cf-config/cf-server tests and CA dataset comparison remain unverified until final build succeeds. Cache401/503 warnings observed;continued. Coordinator CI successor: `nix build --no-link -L .#default.cf-server-core-drv`. Retain embedded build where required without repeating these postChecks or a filtered13-test CA invocation. No CI/docs/devScripts edits by this slice.

Remaining-eight worker update: removed all eight public verify outputs; actual nix eval packages filter returns []. Root verification derivations removed: 9 (8 public roots plus nested crystal-forge-proxy-module-validation). No replacement exported derivations. modules/nixos/crystal-forge/tests/{builder-cve,stig,packaging}.nix preserve pure assertions; packaging runtime=true renders a shell string, packaging.sh realizes only its input context and executes it; --module-only preserves real generated TOML/preStart/ExecStart probes without Rust builds. discovery.{sh,py} retains actual delayed private HTTP + isolated Nix stores + primary_evaluation.nix runtime behavior, not static inspection. Metadata assertions now ordinary nixos-options-metadata text/checkPhase; oscal-fixture postCheck validates compiled fixture AR/AP/SSP chain using moved validate.py. Existing host launcher selftests remain at devScripts source. Runner selftest now unsets inherited DB_HOST/PORT/USER/NAME; custom cases still set explicit values.
Coordinator wiring REQUIRED: existing CI should evaluate builder-cve.nix, stig.nix, packaging.nix independently with arguments inputs=f.inputs // {self=f;}, pkgs=import f.inputs.nixpkgs {system=builtins.currentSystem; overlays=[f.overlays.default];}, lib=pkgs.lib // {crystal-forge=f.lib;}, where f=builtins.getFlake ("git+file://" + toString ./.). Each returns true. Use git+file, not a path flake: path-flake copies untracked Cargo artifacts and caused setup timeouts.
Runtime CI commands: nix develop --command bash modules/nixos/crystal-forge/tests/discovery.sh ; nix shell --impure --expr 'let f=builtins.getFlake ("git+file://" + toString ./.); p=import f.inputs.nixpkgs {system=builtins.currentSystem;}; in [p.remarshal p.jq]' --command bash modules/nixos/crystal-forge/tests/packaging.sh . Existing package CI must build .#nixos-options-metadata and .#oscal-fixture. Packaging full command currently timed out at 200s waiting on the other worker's core-server build; module-only runtime PASSED. No required new packages/default postCheck: all unique installed wrapper/evaluator binary assertions remain in explicit packaging.sh. CA compatibility now belongs other worker's ordinary cf-config checks, not an extra dependency from packaging.
DB worker/coordinator devScripts integration request (do not lose writeShellApplication's existing check): change returned base to (full-stack.config.outputs.package.overrideAttrs (old: { nativeBuildInputs=(old.nativeBuildInputs or []) ++ [pkgs.bash pkgs.coreutils pkgs.iproute2 pkgs.gnugrep pkgs.gnused pkgs.nodejs]; checkPhase=(old.checkPhase or "") + ''\n bash ${./db-usability-check-test.sh} ${./db-usability-check.sh}\n bash ${./db-only-start-test.sh} ${./db-only-start.sh}\n bash ${./web-ui/tests/web-ui-test-runner-test.sh} ${./web-ui/tests/web-ui-test.sh} ${./web-ui/coverage-manifest.json}\n''; })) // { existing attrs }. Verify base runs checkPhase; alternatively put these same three explicit bash commands in existing CI under nix develop. All three direct shell selftests PASSED under nix develop; no actual dev DB used.
Verification passed: boolean builder-CVE, all 11 STIG groups, packaging boolean; packaging.sh --module-only (all original proxy-option acceptance/rejection, TOML, path binding and override probes); nix build --offline .#nixos-options-metadata .#oscal-fixture --no-link -L (schema chain all valid, no extra cargo compile from postCheck); discovery.sh full rerun exits0, cold65.067s warm0.029s policy0.048s, historical60s killed, 2 authorized/2 rejected/0 unexpected requests; git diff --check and shell syntax. Initial discovery assertions passed but cleanup failed on readonly fixture store; chmod restricted to task-owned temporary workspace fixed that and full rerun passed. Full packaging runtime remains incomplete pending core build. Owned files staged, no commit.
<!-- SECTION:NOTES:END -->
