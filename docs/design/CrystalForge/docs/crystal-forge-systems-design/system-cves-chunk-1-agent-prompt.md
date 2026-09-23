Implement SC1 only: System Detail → CVEs target and scan selection.

BASE
Repository: crystal-forge/crystal-forge
Base branch: TASK-326.2-scanning-cve-triage-parity
Inspected head: 327d03b6d58055eb688fe657f12e223b8419f446
Design directory: docs/design/CrystalForge/docs/crystal-forge-systems-design/

Use the current branch state, not an assumed old head. Preserve unrelated work.
Read AGENTS.md and the applicable task/worktree, database-safety, live-preview,
and verification instructions. This handoff authorizes one bounded SC1 task.
If no SC1 task exists, create it through the repository workflow and place only
that task in To Do. Use a dedicated SC1 branch/worktree based on the current
named MR branch, with its own lock. Do not reset or seize TASK-326.2 or another
agent's worktree. Do not push directly to the base branch, merge, or force-push.

Read system-cves-chunk-1.md in full. Read the parent Systems document's Sections
10, 11.4, and 22. The parent file retains the name systems-view-design-v0.1.md,
but its internal version must be 0.2 or newer with these decisions. If missing,
report the missing handoff before edits; do not implement the old head fallback.
Read the relevant SystemDetail.jsx revision/CVE design, then use rg and targeted
source ranges. No subagents, whole-repository review, or broad refactor.

REQUIRED BEHAVIOR
1. Default the CVE tab to the actual reported running configuration. A local
   nixos-rebuild that maps to a known registered target is not disqualified
   because CF did not initiate it. Full proof retains normal Current behavior.
2. A unique server-proved mapping to the running derivation can display that
   derivation's completed schema-1 scan even when retained proof is missing.
   Mark it read-only and show the specific proof reason. Do not emit the existing
   fully authoritative Current semantics or hydrate mutation context for it.
   Existing direct triage/POA&M/verification/closure calls must still reject it.
3. UNMAPPED MEANS UNMAPPED. No automatic flake-head fallback, no scan substitution,
   no local agent scan, and no fabricated generation/deployment record. The
   operator can inspect head or another known target explicitly, read-only.
4. Mapped without scan, no report, ambiguous mapping, loading, request failure,
   unsupported source, and genuinely empty completed evidence are different
   states. Their banner and empty panel must agree. A scan cannot repair lineage.
5. Resolve latest report before validity checks. Scope mapping to the registered
   flake/effective configuration and prove uniqueness independently of menu caps.
   Use only the chosen derivation's latest completed schema-1 scan, ordered by
   completed_at then scan ID. Never use the first/newest-scanned candidate.
6. Refresh re-resolves Current and available evaluated choices. A new evaluated
   commit does not become Current until observed running. Explicit generation/
   commit targets survive refresh, reload, tab changes and browser history.
   Keep target intent separate from Generations/Commits presentation mode.
7. Validate returned system/target/source context as well as request epoch.
   Restart incompatible pagination on target/source/read-tier/revision changes.
   Preserve explicit selection, loaded pages on continuation failure, and late-
   response guards. A failed newer attempt does not erase same-target evidence.
8. Do not silently discard or retarget a mounted draft during in-app refresh.
   Preserve its start context; defer action-bound replacement or require explicit
   refresh confirmation. Keep existing stale-write rejection. Captured-start
   cross-generation save and continuity are later work, not a client-side bypass.

STARTING POINTS
- packages/web-ui/src/views/system_detail.rs:
  revision_scope_default, RevisionScopeSelectionState, RevisionScopeBar,
  CVE inventory resources, navigation and refresh callbacks.
- packages/web-ui/src/components/cve/mod.rs:
  CvesTab, CveInventoryPaginationState, authority/action and empty-state helpers.
- packages/default/crates/cf-server/src/queries/cves.rs:
  fetch_system_cve_inventory_tx, fetch_historical_system_cve_inventory_tx,
  fetch_inventory_page_for_source, inventory candidates and related tests.
- packages/default/crates/cf-server/src/queries/systems.rs:
  existing observational mapping. A commit-only result is insufficient for scans.
- packages/default/crates/cf-server/src/handlers/api/systems.rs:
  inventory response and read_only-dependent remediation hydration.
- Server/Web UI API models and packages/web-ui/src/api/client.rs.

CAUTION: Current candidate derivation and flake-scoped commit use separate LEFT
JOINs. A failed commit-scope join can leave derivation_id non-null. Candidate
existence/is_current/scan_available is not proof of a unique authorized mapping.

Use the smallest server-owned read-state extension that expresses the required
facts. Do not encode “unmapped” as a fake historical selector. Do not label a
mapped-running result historical because it is read-only. Preserve supported
client compatibility; old clients must fail conservatively, not gain authority.
Keep read-only APIs free of scan enqueueing, Nix execution and persistence writes.

SCOPE LIMITS
Preserve the package-first visual design and existing columns/dialog. No new
fleet filter bar. No new scanner/protocol/toggles, proof repair, continuity,
automatic closure, broad header/count consolidation, Scanning rewrite, new event
bus, or unrelated UI/CI fixes. Shared helper edits require affected Hardening
regression tests but not a redesign of its evidence or mutation policy.
The header/Scanning count-source defects remain SC2 work; report that boundary.

PREVIEW AND PROOF
Start/resume a verified task-owned preview before UI edits and give the owner
its URL as soon as usable. Keep backend and UI builds current. Use an isolated
fixture database; never seed/migrate/reset the persistent user database or stop
another worktree's processes. An old preview/check waiver does not apply here.

Implement and prove SC1-01 through SC1-14 in system-cves-chunk-1.md. In particular,
use real database/API fixtures for missing-proof read-only and unmapped cases,
assert exact IDs, and directly test that mutations remain rejected. Use stubs
only where needed to control races/errors. Preserve explicit historical checks.

Starting verification commands, through Nix:
  nix develop --command cargo test --manifest-path packages/web-ui/Cargo.toml revision_scope
  nix develop --command cargo test --manifest-path packages/web-ui/Cargo.toml components::cve::tests
  nix develop --command cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
  SQLX_OFFLINE=true nix develop --command cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml --tests
  git diff --check

Add/run focused server database regressions through the repository's isolated
harness. Update SQLx metadata only when required and only against that database.
Then run the affected authoritative browser workflows:
  CF_UI_TEST_STEPS='12ha-system-detail-cve-inventory-fallbacks,12h-system-detail-cves-grouped-justification' nix build --impure 'path:.#checks.x86_64-linux.web-ui' --no-link -L
Include 28-system-hardening-tab when a shared selector change affects it.
Verify tests actually executed; zero selected tests and mocked-only success are
not proof. If a shared design prerequisite blocks execution, report the exact
blocker and mark workflows NOT EXECUTED. Do not weaken assertions or update visual
baselines to conceal differences. Do not fix unrelated Config Explorer work.

Update changed source docs, the relevant live API contract, and this slice's
implementation/evidence notes. Keep historical AS-BUILT claims labelled with their
original SHA. Do not mark unrelated audit gaps complete. Supply complete contents
for every design/spec file returned to the owner, not patches or partial sections.

HANDOFF AND STOP
Provide one scoped review commit, preview URL/data mode and freshness, task/branch/
worktree, changed files, SC1 acceptance results with exact commands and exit/test
counts, wide/narrow and light/dark evidence, concrete manual test paths, and known
remaining defects. List structural differences from the design in all six review
categories. Do not claim merge readiness with required checks blocked. Stop for
owner validation before SC2. No merge, force-push or direct base-branch push.
