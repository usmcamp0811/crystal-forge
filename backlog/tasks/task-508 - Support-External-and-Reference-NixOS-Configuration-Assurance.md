---
id: TASK-508
title: Support External and Reference NixOS Configuration Assurance
status: In Progress
assignee:
  - '@openai-agent'
created_date: '2026-10-31T15:50:00Z'
updated_date: '2026-10-31T19:40:00Z'
labels:
  - management-mode
  - assurance
  - security
  - web-ui
  - server
dependencies: []
references:
  - >-
    git:18b47eed40620508e8b5725f1e7845c02146a122 (approved management-mode UI
    design on dev)
documentation:
  - docs/design/CrystalForge/components/MgmtParts.jsx
  - docs/design/CrystalForge/data-external.js
  - docs/design/CrystalForge/data-external-scan.js
  - docs/design/CrystalForge/components/AddSystemModal.jsx
  - docs/design/CrystalForge/components/EditSystemModal.jsx
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/design/CrystalForge/components/Systems.jsx
  - docs/design/CrystalForge/components/FlakeExplorer.jsx
  - docs/design/CrystalForge/components/ConfigExplorer.jsx
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/components/DashboardView.jsx
  - docs/design/CrystalForge/styles.css
  - docs/knowledge/index.md
  - docs/knowledge/testing/integration-test-contract.md
priority: high
type: feature
ordinal: 559000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Crystal Forge only models systems that it deploys through an enrolled agent. Operators also run NixOS systems deployed by other teams or tools (including MicroVM guests on CF-managed hosts), and they maintain reference/golden NixOS configurations (workstations, VM baselines, ISO-oriented configurations). Those configurations need the same evaluation, build, CVE-scan, hardening and compliance evidence, but registering them today requires an agent key and grants CF deployment authority it must not have.

## Goal
One complete feature that lets Crystal Forge evaluate, build, scan and retain evidence for a NixOS configuration without being authorized to deploy it. Three management modes exist:
- `managed`: CF-managed system; existing deployment policies apply.
- `external`: deployed independently of CF; CF has no deployment authority.
- `reference`: configuration tracked for assurance; deployment is not applicable; it is not a machine.

Backend field name: `management_mode`. Existing systems default to `managed`. `deployment_policy` stays separate and applies only with deployment authority. `is_active` stays separate from runtime health and mode.

Fundamental invariant: configuration evidence is independent of runtime observation. A successful scan does not establish that a guest runs the scanned configuration. A reference configuration is never an active or offline machine.

## Design authority
The UI design in commit 18b47eed (docs/design/CrystalForge, notably MgmtParts.jsx, data-external*.js, AddSystemModal, EditSystemModal, SystemDetail, Systems, FlakeExplorer, ConfigExplorer, ScanningView, DashboardView, styles.css, app.jsx, Shell.jsx, EnvironmentsView, DashboardWidgetsOps) is the authoritative presentation and interaction specification. Its fixture data, simulated Refresh and mock observations are NOT a backend data model and must not be copied into production.

## Non-goals
- Generic asset management framework, separate scanning pipeline, second deployment system, VM orchestrator.
- Automatic discovery of all guests, guest-agent enrollment, hypervisor orchestration, remote command execution, VM deployment.
- Synchronous request/response refresh channel to agents (transport stays pull-based).
- Full guest attestation or advanced MicroVM monitoring.
- Image distribution service; claiming that a NixOS closure scan equals scanning a generated ISO/VM image.

## Architectural constraints
- Server owns persistence, authorization, and deployment policy; enforcement is at backend boundaries, not UI.
- Schema change via new migration; SQLx offline metadata updated.
- Compatibility with deployed agents (older agents without guest reporting keep working).
- Reuse existing evaluation, build, builder API, cache verification, and derivation-based scan infrastructure.
- Exact-CVE / POA&M authority rules preserved; configuration-only evidence never authorizes runtime claims, remediation verification, POA&M closure or deployment attestations.
- Preserve RBAC, environment membership, secret handling; audit registration, mode transitions, parent-host changes.

## Impact areas
cf-server (models, migrations, systems API, deployment policy / desired target / agent responses / rollback, evaluation targets, build eligibility, scan selection & scheduling, dashboard/fleet metrics, audit), cf-agent (optional parent-host guest observation report), web-ui (Systems list/panel/detail all tabs, Add/Edit System, Flake Explorer, Config Explorer, Scanning, Dashboard, Environments, Shell), integration checks, docs/knowledge.

## Risk
High: security-sensitive deployment authority, persistence migration, evidence integrity.

## Dependencies
Approved design commit 18b47eed on dev (present on origin/dev). No blocking backlog tasks.

## Verification plan
- cargo tests for domain/API/deployment gating/scan selection/metrics in the Nix dev shell; SQLx prepare check.
- Migration applied to an isolated database containing pre-existing systems.
- Live task-owned preview with isolated DB; browser comparison against design mock (dark/light, responsive).
- Owning integration scenarios (integration-access / integration-delivery / integration-assurance) extended per docs/knowledge/testing/integration-coverage-map.md.
- nix build .#okf-knowledge -L for docs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems persist a typed management_mode (managed|external|reference); a migration defaults every existing system to managed and invalid modes are rejected by the server
- [ ] #2 external and reference systems register without agent credentials and no fake key is generated; managed registration keeps existing agent-key requirements
- [ ] #3 Reference configurations use a descriptive identity without requiring a runtime hostname; multiple registered systems may share one NixOS configuration with unique resource identity
- [ ] #4 Create/edit/list/detail/filter APIs expose management_mode and parent-host association while preserving environment membership and RBAC
- [ ] #5 For external and reference systems the server never initiates, authorizes, queues or returns a deployment instruction across automatic policy, manual deploy, desired-target update, pending requests, rollback, rollout and agent responses
- [ ] #6 managed->external/reference transition atomically revokes deployment authority, invalidates outstanding deployment requests, disables the previous agent identity and preserves history; concurrent processes cannot issue a forbidden deployment
- [ ] #7 external/reference->managed requires explicit authorization, valid agent enrollment credentials and a valid deployment policy and never deploys on conversion; external<->reference transitions keep configuration evidence
- [ ] #8 External and reference configurations are evaluated, built (without an agent-enabled configuration) and CVE-scanned through the existing pipelines using exact configuration and derivation identity, including scheduled and manual scans, with no fabricated running store path and no duplicate builds for identical derivations
- [ ] #9 Removed or renamed configurations, failed evaluations, failed builds and failed or unavailable scans are reported explicitly while historical evidence is retained
- [ ] #10 Configuration-only scan evidence never authorizes current-runtime vulnerability claims, remediation verification, POA&M closure or deployment attestations; existing exact-CVE authority rules still pass
- [ ] #11 A CF-managed parent host can report authenticated guest observations (guest identity, unit, unit state, optional runner store path, timestamp); the server rejects unauthorized or cross-environment reports; older agents keep working; observations are stored and shown separately from scan evidence and never claimed as a configuration match without evidence
- [ ] #12 Dashboard, fleet health, deployment, environment, scan-coverage, hardening and compliance metrics count reference configurations and unobserved external systems according to documented definitions (no reference offline, no reference in managed deployment denominators, external included in scan coverage)
- [ ] #13 Registration, management-mode transitions and parent-host changes are audited and require existing authorization
- [ ] #14 Web UI implements the approved design for Add System, Edit System (mode change confirmation), Systems list filters/labels/statuses and side panel, every System Detail tab, Flake Explorer registration with preselection, Config Explorer, Scanning, Dashboard and supporting views, with real data, loading/empty/error/unavailable states and dark/light parity
- [ ] #15 Automated tests cover domain/API, deployment safety (including concurrency and bypass attempts), evaluation/build/scan eligibility, CVE/POA&M authority, host observations, fleet metrics, and owning browser integration scenarios
- [ ] #16 OKF knowledge pages document the management-mode model, deployment authority and transitions, external/reference registration, configuration vs runtime assurance, CVE evidence authority, MicroVM observation limits, scan scheduling, metric semantics, API and migration changes, and troubleshooting; nix build .#okf-knowledge passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation sequence (single branch and MR)
1. Preserve approved design at 18b47eed; inspect actual files and interact with mockup. Complete design-to-code inventory below. Start task-owned persistent preview through supported devenv isolated stack; verify process/database/API/browser ownership before any UI edits. No default-port stack reuse.
2. Add typed management modes and a new migration: nullable agent key for nonmanaged resources, valid mode/key constraints, optional parent association and separately stored authenticated guest observations. Preserve unique resource IDs and effective configuration selection. Expose mode in APIs and defaults compatible with existing clients. Registration and transitions use existing RBAC/environment checks and transactional audit.
3. Enforce deployment authority under existing system-row locks in composite authorization/claim, manual queue, automatic selection and agent authentication/delivery. Conversion clears desired targets, supersedes pending requests and removes legacy delivery bridge/reservations atomically; conversion into managed requires explicit credentials/policy and suppresses automatic deployment until explicit action. Preserve history, do not relax deployment agent requirements.
4. Separate artifact eligibility from deployment eligibility in evaluation/build policy gates and queue selectors. Preserve actual cf_agent_enabled fact and configuration identity; shared configurations remain one evaluation/build artifact. Extend configuration-target scan selection and scheduled fleet scans without inventing runtime paths. Keep Current CVE/POA&M authority restricted to verified managed runtime evidence, including retained observations after conversion.
5. Implement optional guest report fields through existing signed pull transport with backward-compatible defaults. Validate associated parent, environment, mode and unit; store observation separate from scan evidence. UI reloads latest observation and never claims synchronous guest refresh or guest attestation.
6. Extend real evidence APIs and mode-aware fleet metrics. Implement approved Dioxus UI, all affected tabs and entry points, real completion/request outcomes. Update knowledge contracts and existing owning integration scenarios together.
7. Run targeted Rust/SQLx/migration/browser verification in isolated environments, owning integration checks, knowledge validation and applicable flake checks. Perform requirements/backend/integrity/query/UI/empty-state/e2e/test review passes. Open exactly one complete MR to dev only after required verification.

## Design-to-code acceptance checklist (source inventory; all unverified)
- [ ] Identity/status: DTOs + systems/adapter.rs expose managed/external/reference; mode chips, distinct deployment labels, evidence issue precedence, absent evidence != zero CVEs; no reference/runtime unknown as offline.
- [ ] Systems: views/systems_list.rs, systems_list_helpers.rs, systems_stat_strip.rs add global mode counts/filter, managed operational denominators, evidence/attention filters, config/parent search, reset empty state, preserve card/table controls.
- [ ] Cards/table/preview: system_card_v2.rs, systems_table.rs and SystemPreviewPanel retain hierarchy, mode chips, selected config/revision/scan/runtime metadata, real evidence CVEs, Evidence actions, separate nonmanaged panel sections.
- [ ] Detail Overview: views/system_detail.rs adds four-stage revision/eval/build/scan pipeline with exact identities/navigation, purpose/control card, external observations or reference recent evidence, missing-config diagnostics.
- [ ] Deploy: explicit unavailable explanation and history/change-mode actions, no deploy/rollback for nonmanaged; server denial independently verified.
- [ ] History: evidence timeline with All/Eval/Build/Scan filters and empty states; retain managed deployment history and historical evidence after conversion.
- [ ] Logs: nonmanaged runtime-log unavailable state and real build-log navigation.
- [ ] Config: components/system/config_explorer.rs offers commit/config evidence without fake host generations; deep links preserve revision.
- [ ] CVEs/Hardening/Compliance/POA&M: separate configuration evidence banners/gates, waiting/failed/unavailable/stale/last-good/mismatch states, read-only artifact findings, no Current-authority mutation enablement; preserve existing sections/drawers.
- [ ] Runtime: reporting parent/time/guest/unit/state/runner, verified guest unavailable, comparison not-comparable unless proved; recent observation reload only, preserve last observation on failure; reference has no runtime card.
- [ ] Add: components/forms/add_system_form.rs + list modal adopt management-first rail and conditional identities/key/config/parent/policy/tags, exact declared config validation and preselection, real enrollment/completion outcomes.
- [ ] Edit: components/system/edit_system_modal.rs retains General, conditional Deployment/Security or Configuration, Management warnings/credential requirements, Danger confirmation, real transactional save/error/cancel behavior.
- [ ] Flakes: views/flakes_list.rs Add to Forge mode menu prefills config/mode/flake/branch, mode labels on registered configs, exact revision Open config, reconciliation and shared registration identities retained.
- [ ] Scanning: views/scanning.rs mode chips + exact config evidence revisions/status/diagnostics, real rescan queue results, preserved filters/schedule/log drawers.
- [ ] Dashboard/environments/sidebar: managed operational counts only; all applicable configurations remain in scan coverage; alerts do not treat missing observations as outage.
- [ ] CSS: assets/app.css ports approved management components/tokens, spacing, hierarchy, responsive grids/table scrolling; inspect dark/light and long paths/narrow forms.
- [ ] Browser compare and screenshots for every affected section; missing/loading/empty/error/restricted/unavailable states and full registration/edit/flake/evidence workflows exercised.

## Initial code findings
systems.public_key is NOT NULL and System decodes PublicKey eagerly; health views mark no heartbeat offline. Composite authorization locks system rows, manual queue has a separate lock path; auto-latest reads a snapshot and must recheck under lock. Evaluation registered-config selection already includes agentless registrations, but global agent policy and build queue SQL block agentless artifacts. Manual CVE artifact resolver exists; fleet scan selector is runtime-based. view_current_cve_authority depends on latest observed generation/path and must explicitly exclude modes after conversion. No UI simplification or mock backend data authorized.

Bounded delegated backend increment in existing TASK-508-management-modes worktree: add migration 0311 mode/key/desired-target/parent constraints; preserve owner ManagementMode and tests; adapt System storage, config upsert with fail-closed mode conflict, nullable hostname lookup, and agent authentication regressions. Make only required mechanical System literal corrections. Verify changed Rust with nix develop rustfmt and SQLX_OFFLINE=true targeted management_mode_tests / agent_request tests. No database migration, services, preview, UI, commits or pushes. Parent authorization/observations, API transitions and transactional deployment gates remain owner follow-on work. Heartbeat nullable-key consumer adaptation requires scope clarification.

Owner-approved listener correction (single existing TASK-508 branch/worktree): honor loaded server.host and log that host; change cf-config ServerConfig::default host to 0.0.0.0 to preserve the previous effective wildcard bind with no override, with rationale adjacent to host. Scope is listener and directly related configuration/tests. No commit/push or preview process changes. No existing server-launch test infrastructure was found; main performs migrations and starts background jobs before binding. Use one tiny private bind function shared by main and focused real-socket tests to verify configured 127.0.0.1, omitted-server default, and explicit wildcard without database access. Run targeted tests/formatting through nix develop with SQLX_OFFLINE=true and database URLs unset; preserve existing owner edits and devenv.lock contents.

Authorized listener startup correction is directly related to safe task preview. Keep default bind backward compatible, test exact production bind with real ephemeral sockets, then resume backend using task-owned generated config/PG and verify loopback. No additional networking feature or public config field.

Guest-observation transport slice (same owner worktree, exclusive file scope): add defaulted signed reports and heartbeat targets binding external guest UUID/identity to validated microvm@identity.service; collect only fixed systemctl show ActiveState (runner None unless authoritative local runner proven); validate active managed parent/key, exact same-environment external association and identity under existing snapshot/system transaction locks; append separate evidence migration 0315; expose scoped latest observation without refresh or runtime/scan authority. Add compatibility, collector/validation and ignored isolated SQLx tests. Run targeted Nix-shell checks without services or DB access; owner handles broader docs/UI/integration.

Bounded scanning backend continuation in existing TASK-508-management-modes worktree: exclusive edits to queries/scanning.rs, handlers/api/scanning.rs and scanning DTOs in api/models.rs. Add nonmanaged exact configuration artifacts to operational coverage independently of heartbeat/recovery eligibility; gate every scanning runtime/current selector to managed; expose distinct registration management modes on shared artifact rows and preserve per-resource system identity/environment. Preserve existing selectors, schedule, queue, logs, archive/history and filter contracts. Add pure response tests and ignored isolated-harness database regression tests in these files; run targeted pure scanning filters through Nix with SQLX_OFFLINE=true. No commits, services or database access.

Detail/Config Explorer delegated slice: use the existing TASK-508 worktree and owner preview UI8081/API3447. Implement approved nonmanaged detail presentation in a real management-detail submodule, preserve all managed tabs and edit wiring, use configuration-assurance exact identities and separate latest-observation GET. Use real scanning schedule for scan freshness; unavailable observation policy remains unknown. Gate artifact findings read-only; preserve POA&M drawer navigation. Verify meaningful state derivation with targeted native tests and WASM check. Owner handles backend refresh, browser design comparison/screenshots, knowledge and owning integration evidence. Exclusive edits: system_detail.rs, config_explorer.rs, cve/mod.rs only if required, and meaningful detail presentation submodule.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-opus on sledge in /home/mcamp/code/crystal-forge/TASK-508-management-modes

Preflight: branch TASK-508-management-modes created from origin/dev @ 18b47eed (design commit). Local dev worktree is 4 backlog commits ahead/3 behind origin/dev and has an uncommitted change to packages/default/crates/cf-agent/src/deployment/agent.rs (other work, untouched). No main worktree exists. Upstream tracking unset to prevent accidental push to dev.

Lock correction for this continuation: the earlier agent self-label 'claude-opus' was incorrect. LOCK: openai-agent (gpt-6.1-sol) on sledge in /home/mcamp/code/crystal-forge/TASK-508-management-modes. Same requested task/worktree; clean HEAD 18b47eed verified. No application edits yet.

Preview startup: default 8080/3445 occupied by unrelated processes; untouched. Existing supported per-worktree devenv workflow found (docs/agents/devenv-workflow.md). Invoked nix develop .#devenv --command devenv up -d twice; first exceeded 120s, second exceeded 600s while building baseline core backend and running package tests. Neither invocation establishes a usable preview. Logs /home/mcamp/.local/share/opencode/tool-output/tool_192cc40cc001SHjg66ialoJkkz and tool_192ce5c870010Q4CbPRJ0H270k. Do not claim test suite passed from partial build output.

Bounded storage/auth increment implemented in TASK-508-management-modes via apply_patch. New 0311 follows latest existing 0310; adds exact modes, nullable mode-bound key, nonmanaged desired-target exclusions, external-only non-self parent FK and semantic comments. Primary System now has typed default mode, Option<PublicKey>, parent_host_id; legacy constructor remains managed with validated Some(key). Config upsert explicitly binds mode/key/parent and refuses every conflict unless both existing and incoming modes are managed; rejected conflicts return an explicit error, preserving nonmanaged rows and preventing silent promotion. Hostname lookup retains runtime FromRow decoding, now nullable through Option<PublicKey>. Authentication checks managed + active + present key before signature verification; signed denial regression covers external/reference with absent or retained keys, inactive managed, and keyless managed. Existing valid-signature and key-rotation regressions retained. Other edits are mechanical System test literal corrections only.

Formatting via nix develop -c rustfmt --edition 2024 --config skip_children=true on the ten changed Rust files succeeded; git diff --check succeeded. Both requested SQLX_OFFLINE=true cf-server --lib cargo test filters (management_mode_tests and handlers::agent_request::tests) were attempted via nix develop with 600s tool timeout, but compilation failed with E0599 at handlers/agent/heartbeat.rs:139: Option<PublicKey> has no to_base64 method. No tests executed. The heartbeat consumer is outside the exclusive literal-only secondary-edit scope; requested approval for a minimal adaptation and left it untouched. No database migration/services/previews/commits/pushes; pre-existing devenv.lock and owner enum/tests preserved. Isolated migration application, SQLx metadata validation, upsert runtime verification, parent authorization/observations, API changes and transactional deployment gates remain owner follow-on work.

Listener-only owner-approved correction implemented in packages/default/crates/cf-config/src/config/server.rs and cf-server/src/bin/server.rs. Loaded default host now 0.0.0.0 with adjacent compatibility rationale; startup Host log uses configured host; main calls a tiny private bind_listener using TcpListener::bind((server_cfg.host.as_str(), server_cfg.port)). Three real-socket tests share this production function and deserialize root configuration: configured loopback, omitted-server wildcard default, explicit wildcard. Verified: nix develop --no-update-lock-file -c env -u DATABASE_URL -u CRYSTAL_FORGE_TEST_DATABASE_URL SQLX_OFFLINE=true cargo test --locked --manifest-path packages/default/Cargo.toml -p cf-server --bin server listener_tests:: (3 passed, exit 0); equivalent cf-config --lib config::server::tests:: (10 passed, exit 0). Scoped git diff --check passed. rustfmt --edition 2024 --check on both files reported only the pre-existing server.rs:1196 caches/metrics route formatting difference (exit 1); no changed-line formatting differences. No database-dependent test required, no database access, no commit/push, no preview processes started/restarted. Owner migration 0311 and management-mode edits preserved. Existing devenv.lock was already dirty before this work; no edits applied to it. Owner can rerun the exact focused listener command above; no isolated SQLx execution is needed for these socket tests.

Owner explicitly approved narrow listener correction: honor configured host, preserve effective wildcard default when omitted, no new options/env vars/networking framework; focused socket regressions required. Implemented server listener uses existing ServerConfig.host; loaded default changed127.0.0.1->0.0.0.0 to preserve previous actual default, truthful startup log. Three real socket regressions pass (loopback, omitted-server default, explicit wildcard), plus10 existing config tests. No other listener scope authorized.

Backend foundation implemented, not complete feature: migration0311 adds mode/key/target/parent invariants; System key optional, typed mode and parent; managed constructor signature unchanged; config upsert cannot promote nonmanaged registration; agent authentication rejects nonmanaged/inactive/keyless even with valid signature. Three mode and18 agent_request tests pass through Nix offline. Migration not yet applied/verified, deployment and evidence boundaries still incomplete.

Preview baseline reached processes ready: task PG PID16730 verified cwd TASK-508-management-modes/.devenv/state/postgres, frontend17060 cwd packages/web-ui port8081, baseline API16871 cwd task root port3447. API unexpectedly listened0.0.0.0; stopped only task api via devenv processes stop api, verified3447 absent. Need rebuild/start corrected task backend before calling preview ready. Design preview loopback18088 verified200; Chromium exercised management filtering and reference Add sections; screenshots /tmp/opencode/task508-design-{dashboard,external-list,reference-list,reference-form,config-form}.png. Mockup interaction is not production parity proof.

Bounded scanning backend implemented in existing worktree; only application files touched by this continuation are packages/default/crates/cf-server/src/queries/scanning.rs, src/handlers/api/scanning.rs and scanning DTO section of src/api/models.rs. Operational coverage now unions exact built external/reference artifacts at the ready registered flake head, independently of heartbeat/on_build/recovery expiry, with existing operational populations. Every current/runtime classification is managed-only, including system-scoped records when a managed registration shares the artifact. Artifact queue/deployed/record responses add sorted distinct management_modes; system-scoped history reports only selected registration mode; systems rows add singular management_mode and retain actual registered UUID/name/environment. Artifact names remain config identities, not resource navigation IDs. Existing CVE selectors, schedule policy, logs, archive, queue/filter/history mechanics preserved.

Verification: nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib scanning:: exited 0 on final code: 14 passed, 0 failed, 1 ignored. New pure tests: configuration_history_cursor_is_bound_to_registration_and_filters; scan_queue_response_preserves_shared_modes_and_registration_identity. New SQLx ignored test configuration_coverage_and_shared_modes_do_not_confer_runtime compiled but NOT EXECUTED (no authorized safe DB); covers no-heartbeat/old-build coverage, shared-mode deduplication, new unbuilt head excluding old evidence, retained-report runtime denial, per-resource environment/name/UUID, mixed managed sharing and renamed-registration history retention. Targeted Nix rustfmt --check for all three files and scoped git diff --check passed. Initial shorter test invocation timed out during compilation; later final selection succeeded. No commits, services or database access performed. Runtime SQL behavior remains unverified until coordinator runs the ignored regression in the isolated existing SQLx harness.
<!-- SECTION:NOTES:END -->
