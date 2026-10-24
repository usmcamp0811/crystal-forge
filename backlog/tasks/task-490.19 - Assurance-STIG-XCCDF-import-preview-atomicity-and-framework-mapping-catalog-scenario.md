---
id: TASK-490.19
title: >-
  Assurance: STIG XCCDF import, preview atomicity, and framework mapping catalog
  scenario
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:38'
updated_date: '2026-10-24 01:26'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.1
references:
  - docs/knowledge/testing/integration-coverage-map.md
  - docs/knowledge/compliance/cf-xccdf-interchange-operator-guide.md
  - docs/design/CrystalForge/components/ImportStigModal.jsx
  - >-
    checks/integration-assurance/fixtures/stig-import-U_Anduril_NixOS_STIG_V1R1_Manual-xccdf.xml
  - >-
    checks/integration-assurance/fixtures/stig-import-U_Anduril_NixOS_STIG_V1R2_Manual-xccdf.xml
modified_files:
  - checks/integration-assurance/scenarios/fragments/stig-import.py
  - checks/integration-assurance/scenarios/fragments/stig-import.cjs
  - checks/integration-assurance/fixtures/stig-import-*
parent_task_id: TASK-490
priority: high
type: task
ordinal: 531000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.policy.stig-import. Goal: prove input-derived import identities, preview atomicity, version provenance and framework mapping persistence. Risk: medium. Non-goals: enforcement (.18), export/round-trip assertions (.22), evaluation evidence, product changes and visual baselines without approved ImportStigModal state.

Writable paths (only):
- checks/integration-assurance/scenarios/fragments/stig-import.py
- checks/integration-assurance/scenarios/fragments/stig-import.cjs
- checks/integration-assurance/fixtures/stig-import-*

Composition target: checks/integration-assurance/scenarios/evidence-remediation-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs mapping and explicit registration/normal owner aggregation. Import cases need only the controller/browser interface but compose inside the shared Assurance VM/database. No standalone import gate or packet-specific NixOS derivation. Later same-scenario policy/remediation/export cases MAY consume real imported identities through receipts. Another derivation's live rows, outputs or receipts MUST NOT be consumed.

Adapt V1R1/V1R2 owner-local fixtures byte-faithfully from stig-import-U_Anduril_NixOS_STIG_V1R1_Manual-xccdf.xml and V1R2 counterpart (Benchmark Anduril_NixOS_STIG). Preserve provenance; legacy input retirement remains coordinator-owned. Independently compute namespace-aware XML rule count, rule IDs/stable distinct subset and version into authored facts (for example build-time XML parsing); never derive expectations from CF responses. Author truncated XML and non-XCCDF inputs locally.

Preview V1R1 shows exact independently computed counts/identities and persists no policies/framework requirements. Valid V1R1 import persists exact rules/provenance; distinguishable V1R2 does not rewrite V1R1. Malformed/non-XCCDF inputs are visibly rejected without partial rows. UI mapping of an imported rule to a framework requirement persists through reload. Cover success, validation and preview loading only under approved state authority.

Authority/read-only sources: docs/knowledge/compliance/cf-xccdf-interchange-operator-guide.md, cf-xccdf-interchange-profile.md and stig-modules.md are as-built evidence; the XCCDF design review draft is not acceptance authority. Approved ImportStigModal.jsx, PoliciesView.jsx and PolicyEditor.jsx control presentation. Read cf-server api/compliance.rs xccdf_preview/xccdf_import/MAX_XCCDF_MULTIPART_BYTES, compliance/xccdf/zip_extractor.rs PackageKind and disa_stig_adapter.rs, api/framework_requirements.rs framework/version/search/mapping handlers, /api/v1/compliance/xccdf/{preview,import}, frameworks* and /api/v1/policy-versions/:pv/requirement-mappings; web-ui import/Provenance/Mappings tabs. Uncertain semantics remain blocked: decision/design gap. Production, helpers, composition, registration, CI, docs and design are read-only.

Dependency/readiness: TASK-490.1 controller foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab passed with .25/.26. Use concrete case/assert/receipt/sql_json/http named-session/CSRF and independent run_browser contexts. Await selection, fixture facts, accepted import/mapping authority and coordinator Assurance composition; import proofs themselves do not need future target roles. No import fragment execution is claimed.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence. Retain correct failures and linked defects; escalate interface/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Expected rule counts, identities, stable distinct subset and versions are independently derived from byte-faithful owner-local V1R1/V1R2 XCCDF inputs, never CF responses
- [ ] #2 Preview shows exact input-derived facts and persists no policy/framework rows; valid import persists exact rules/provenance and V1R2 does not rewrite V1R1
- [ ] #3 Malformed truncated XML and non-XCCDF inputs are visibly rejected without partial rows
- [ ] #4 An approved requirement mapping created through UI persists through authoritative database read and reload
- [ ] #5 Coordinator composes named import fragments into evidence-remediation-lifecycle with same-run identity receipts and no standalone import gate, packet derivation or cross-derivation live state
- [ ] #6 Coordinator executes the focused evidence-remediation-lifecycle gate and records exact tested revision, command, outcome and evidence; uncertain authority/design states remain explicitly blocked
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Preserve byte-faithful existing V1R1/V1R2 inputs and derive independent namespace-aware rule/version/identity facts from XML, plus authored malformed inputs.
2. Implement only named stig-import.py/.cjs and owner-local fixtures: actual preview with no policy/framework writes, real import/version preservation, visible malformed-input rejection and real UI requirement mapping with SQL/reload where approved.
3. Consume same-scenario controller/Admin helpers; expose typed facts/receipts and browser-phase interfaces to the coordinator. No new scenario/check, seeded outcome, mocked CF success, product fix, shared helper or registration change.
4. Verify syntax/input facts in the dedicated worker; coordinator composes into the existing Assurance lifecycle and executes its normal owner. Preserve any functional failure and escalate new defects rather than weakening accepted criteria.

Owner-authorized narrow production repair (2026-10-23): preserve the existing benchmark-version regression; populate ParsedRule.version only from direct namespace-qualified Rule/version; protect Benchmark/version from unrelated nested/foreign content. Add focused parser/adapter tests using existing test homes and retain independent real XML facts. Inspect immutable reimport consequences and document the bounded historical-import limitation. Verify focused Rust tests through Nix and relevant reimport tests only through the repository's verified isolated DB launcher. No VM or competing service stack. Commit this repair separately from b9bef89f.

Coordinator-authorized test-prerequisite correction: change only the cfg(test) shared_native_plan expression from legacy cfg.config.* to the documented canonical config.* binding. Keep production validators and all idempotency/immutability/negative-test assertions unchanged. Run the exact foreign shared-reimport test plus the prior three reimport/conflict selections in a fresh invocation-owned cve-test PostgreSQL cluster on 35432; do not touch occupied 35433. Commit separately from 81ed0959.

Owner explicitly approves the narrow non-XCCDF preview production correction: return422 for UnsupportedPackage at the existing preview API boundary, consistent with committed import; preserve parser classification, valid supported namespaces, zero-rule Benchmarks and other error classifications. Confirm actual parse/package/error flow; add focused real API regressions for unsupported/supported/zero-rule/other failures and preview/import consistency without weakening the existing browser422assertion. Reuse current isolated DB tools and existing test homes; no schema/check/wrapper/scenario/framework change. Coordinator uses existing locked TASK4901 worktree/MR335 and task-owned preview. Run focused tests, restart only task backend when rebuilt, then normal Assurance into successful V1R1/V1R2 import and mapping; commit/push separately and inspect exact resulting hosted inputs. Running pipeline2983606845 must not be duplicated. Correction is not blanket Assurance completion or merge/deploy authorization.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: controller prerequisite bba718be passed. Await selection, fixture-derived facts, accepted import/mapping states and coordinator Assurance composition. No import fragment run is claimed.

### Historical
2026-10-17 review recorded foundation/package blockers and no VM execution at that time. Legacy canonical STIG/20ad/20aa proofs were buried in the browser monolith and reconciliation used route mocks. A standalone import scenario was a former proposal; current import cases compose in the single Assurance lifecycle and retain owner-local input provenance.

Owner's wrap-up explicitly selects dependency-ready TASK-490 functional work; this controller-only contribution is selected, not blocked by D2 or target-mode coverage. Existing approved preview/import atomicity, input-derived identity and mapping criteria remain unchanged. Use the existing Assurance lifecycle; coordinator retains composition/normal-owner execution and all production files remain read-only for this test contribution.

Preflight: clean dedicated worktree/branch TASK-490.19-stig-import at0666c65e7e4bd424568f0800cdb5781c4d665556, based on current MR integration. Task-owned lock created. Coordinator retains main worktree, preview, composition and all VM ownership. No main integration worktree registered; existing dev untracked operational files and all other ownership remain preserved. Worker may not start a competing stack or VM.

The owner explicitly expands this worker's write scope for the observed 104 null rule.version values: existing xccdf/parser.rs and existing adapter/tests, necessary independent fixture facts, and directly related import knowledge only. Official XML, identity/schema/auth/policy/UI behavior, shared integration composition, coverage-map/contract/verification files remain unchanged. Coordinator owns the normal Assurance run.

Narrow parser repair implemented: direct namespace-qualified benchmark/rule version contexts and bounded direct-text capture; missing/empty semantics and source bytes/digest retained. Added default/prefixed namespace, nested/foreign version, multiple/missing/empty revision and text-limit tests; retained benchmark_version_not_clobbered_by_rule_versions unchanged. Added ordinary V1R2 expected map under the existing crate testdata directory, independently extracted/verified with stdlib ElementTree. Existing Assurance facts still require all 104/103 non-null source revisions.

Focused verification: parser 44 passed/1 ignored; adapter 27 passed; complete compliance::xccdf:: selection 297 passed/1 ignored. Knowledge validation passed (192 concepts; 215 Mermaid blocks). Isolated launcher (verified fresh data directory and actual DB role) passed framework_version_release_conflict, framework_preview_matches_commit_release_digest_semantics, and cf_native_reimport_reuses_exact_identities, with expected=emitted=1 per selection and test_exit=compose_exit=0.

Retained failure: existing phase_22_shared_creation_exact_reimport_is_idempotent failed before import at compliance_interchange.rs:4212, IMPORT_NATIVE_CHECK_INVALID (assertion expression must use canonical config binding). Expected=emitted=1; launcher test_exit=compose_exit=101. No unrelated fixture/product repair made. Exact-artifact foreign reimport statically returns original rows before normalization, so old null revisions remain; differing artifact bytes for the same release can conflict on corrected canonical metadata. No historical backfill/migration is introduced. Coordinator retains normal Assurance execution.

Committed narrow repair separately as 81ed0959a5752ced983058dcab2abe84f36a48b3 (parent b9bef89fa20ba7611ddec45f841ffd5769754ef9); four files only: parser.rs, existing disa_stig_adapter.rs tests, testdata/xccdf/anduril-v1r2-rule-versions.json, directly related interchange operator guide. Official XML and Assurance assertions unchanged. Worktree clean after commit. Public parse_xccdf contract/example updated; targeted doctest 1 passed and rustfmt checks passed. Knowledge validation rerun after final prose change.

Exact Rust test environment: nix develop --command bash -c with SQLX_OFFLINE=true CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/opencode/task49019-target CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0; cargo test --locked --offline --manifest-path packages/default/Cargo.toml -p cf-server --lib compliance::xccdf:: -- --test-threads=2 => 297 passed/1 ignored, exit 0. Targeted parser and adapter selections also passed; cargo test with --doc compliance::xccdf::parser::parse_xccdf passed. Initial 120-second cold compilation timed out; subsequent completed checks used the same worker-owned compilation directory, not a global compiled-artifact fallback.

Database verification used existing devScripts.state-machine-test launcher with a temporary process-compose command selection override, existing database-environment.sh safety/membership guard, repository migrations, no substitutes/fallback, and the same worker compilation directory. Logs: /tmp/opencode/task49019-db-tests.log (retained foreign shared-reimport fixture failure, exit 101) and task49019-db-conflict-tests.log (three remaining exact selections passed, exit 0). Independent XML map/source-byte proof: task49019-source-facts.log. Other logs: task49019-parser-tests.log, task49019-adapter-tests.log, task49019-xccdf-tests.log, task49019-rustdoc.log, task49019-knowledge-final.log. No VM or normal Assurance run was started; coordinator owns that evidence.

Confirmed contract before fixture edit: deployment-policy-checks.md lines 87-94 names config.* canonical; xccdf/importer.rs lines 523-539 explicitly rejects missing config. or cfg.config. in native custom import-plan assertions. Existing native_custom_check_requires_config_binding_and_preserves_assertions and native_custom_check_rejects_non_cfg_expression tests retain the accepted/rejected cases. No contract conflict prevents authoring canonical input.

Committed authorized cfg(test)-only prerequisite correction as d25f6f097cee84bca4d6ed0d411a5d5cbde77442, parent 81ed0959a5752ced983058dcab2abe84f36a48b3. Only shared_native_plan expression in queries/compliance_interchange.rs:4184 changes cfg.config.services.openssh.settings.PermitRootLogin to config.services.openssh.settings.PermitRootLogin. All original idempotency/no-duplicate/immutability assertions and production validators/negative tests are unchanged. Worktree clean.

Fresh isolated cve-test launcher invocation used port 35432, verified owned data directory /tmp/cf-state-machine.8hgVgy and socket directory /tmp/cf-sm-sockets.jcdO7Q. Both were removed by launcher cleanup; occupied 35433 was untouched. Outer and inner Nix commands were offline with substituters/substitute/builders/fallback disabled; Cargo locked/offline; existing database-environment.sh verified DB ownership and exact-test membership.

All four exact DB selections passed with expected=emitted=1 and exit=0 each: queries::compliance_interchange::tests::phase_22_shared_creation_exact_reimport_is_idempotent; queries::framework_requirements::tests::framework_version_release_conflict; queries::framework_requirements::tests::framework_preview_matches_commit_release_digest_semantics; queries::compliance_interchange::tests::cf_native_reimport_reuses_exact_identities. Launcher final test_exit=0 compose_exit=0. Command per selection: cargo test --locked --offline --manifest-path Cargo.toml -p cf-server --lib <exact-name> -- --ignored --exact --test-threads=1. Log /tmp/opencode/task49019-fixture-db-tests.log; temporary selection /tmp/opencode/task49019-fixture-db-selection.json.

Additional targeted verification passed: rustfmt --edition 2024 --check on changed file; importer::tests::native_custom_check_ selection (2 passed including the invalid-expression rejection); git diff checks. Log /tmp/opencode/task49019-fixture-negative-tests.log. Remaining failures in the requested four-DB-test selection: zero. Earlier exit101 is retained as historical evidence of the stale prerequisite and is superseded by this successful execution. Normal Assurance with actual XML inputs remains coordinator-owned and has not been run by this worker.

Hosted Assurance exact642db0ec pipeline2983100226 job17430452260 is terminal failed:50passed/1failed case records45distinctnames. Original47pass/41names retained; STIG source prerequisites,104rule-version preview/cancel no writes, and truncated422/no-partialrows passed; nonXCCDF preview returned200 expected422. Successful V1R1/V1R2 import and framework mapping were not reached. Downloaded structured result is authoritative because top-leveltrace hit4MBlimit. Componentjob17429864150 independently passed44parser tests plus benchmark-not-clobbered, direct-rule-version metadata/stable-key and independent103-rule-map tests.
Read-only source diagnosis: existing parser classifies absentBenchmark as UnsupportedPackage without a blocking diagnostic; preview checks blocking errors but not that class. Actual import calls check_document_class and rejects UnsupportedPackage422 before commit; import denial here is source-confirmed, not reached by failed browsercase. This predates d58a3125 and is not a Rule/version regression. Keep parserclassification/zero-ruleBenchmark/prefixnamespace contracts; proposed narrowfix is APIpreview UnsupportedPackage rejection, not parserrootrewrite/zero-rule rejection. Current task permits only earlier directversion productrepair; new preview-boundary repair awaits owner scope approval. Preserve422assertion and immutableOfficialXML.
Commit32d1b88d enrolls four exact existing framework reimport/conflict/digest regressions in normal state-machine CI. Fresh local isolatedDB invocation /tmp/opencode/task490-framework-enrolled.{log,status} passed4expected=emitted1 selections, test_exit=compose_exit0; data/socket paths removed. Shellsyntax andShellCheck errorspassed; new enrollment not yet hosted. No newcheck/scenario/DBframework.
<!-- SECTION:NOTES:END -->
