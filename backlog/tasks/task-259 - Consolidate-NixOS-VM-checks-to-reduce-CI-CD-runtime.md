---
id: TASK-259
title: Consolidate NixOS VM checks to reduce CI/CD runtime
status: In Progress
assignee: []
created_date: '2026-04-10 03:12'
updated_date: '2026-04-10 03:13'
labels:
  - ci-cd
  - nix
  - performance
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The project currently has 8 separate NixOS VM checks, each spinning up independent QEMU VMs. Each VM has significant boot overhead and resource costs. Many checks share nearly identical infrastructure (PostgreSQL, Crystal Forge server, gitserver, agent key generation) but are run completely independently.

Current checks and their topology:
| Check | Nodes | Peak RAM | Purpose |
|-------|-------|----------|---------|
| `database` | 1 | ~1 GB | Runs migrations only |
| `dashboard` | 1 | 2 GB | Grafana provisioning |
| `server` | 2 | 6 GB + gitserver | Flake polling, server lifecycle |
| `web-ui` | 1 | 12 GB | Playwright browser UI tests |
| `oidc-auth` | 2 | 2 GB + Keycloak | OIDC token exchange |
| `builder` | 2 | 4 GB + gitserver | Nix builder (no server) |
| `attic_cache` | 3 | 8 GB + 2 nodes | Attic cache push/pull |
| `s3_cache` | 3 | 8 GB + 2 nodes | MinIO/S3 cache push/pull |

## Goal

Reduce the number of separate VM checks to the minimum that preserves full test coverage, eliminating redundant VM boots and duplicated infrastructure setup.

## Non-Goals

- Do not reduce test coverage or remove any assertion
- Do not merge checks that require fundamentally different topologies or auth modes (e.e.g. oidc-auth must keep Keycloak node)
- Do not change application code — only check definitions and shared Nix helpers
- Do not merge `web-ui` into another check (Playwright requires 12 GB and headless browser; isolation is warranted)
- Do not change what the tests assert, only what VMs they share

## Proposed Consolidation

### Must-Keep Separate (3 checks)
- **`web-ui`** — 12 GB, Playwright browser automation; unique setup
- **`oidc-auth`** — Requires Keycloak node; fundamentally different auth topology
- **`cache`** — attic_cache and s3_cache can potentially be combined into a single multi-backend cache check (test Attic then S3 on the same nodes, or test both backends in one run)

### Candidate for Merging (into 1 unified check, currently 3 checks)
- **`database` + `dashboard` + `server`** → single `integration` check
  - `database` only tests migrations — any check that boots a full server already runs migrations
  - `dashboard` is a single-node check that adds Grafana provisioning — can be enabled on the server node in the `server` check
  - `server` already has PostgreSQL + Crystal Forge server + gitserver — add `dashboards.enable=true` and the Grafana assertions
  - Net result: 3 checks → 1 check

### Candidate for Merging or Absorption
- **`builder`** — runs no server, uses gitserver + cfServer. Could be merged into the unified integration check if resource budgets allow (builder needs 4 GB, server needs 6 GB — total 10 GB may be acceptable). Alternatively keep as its own 2-node check if memory constraints require it.

### Final Target (4–5 checks, down from 8)
1. `integration` — server + database migrations + Grafana dashboard (was 3 checks)
2. `builder` — builder-only, 2 nodes (keep separate if too heavy to merge, else absorb)
3. `cache` — attic and s3 backends (combined, was 2 checks)
4. `oidc-auth` — unchanged
5. `web-ui` — unchanged

## Implementation Approach

1. **Audit shared helpers** — review `lib/crystal-forge/` for `makeGitServerNode`, `makeAtticCacheNode`, `makeS3CacheNode` helpers and note what's available for reuse.
2. **Merge database into server/integration** — confirm server check already triggers migrations, remove `checks/database/` as a standalone check.
3. **Merge dashboard into integration** — add `services.crystal-forge.dashboards.enable = true` to the server node in the consolidated check; migrate Grafana assertions.
4. **Evaluate builder merge** — profile resource usage; merge into integration if within ~10 GB budget, otherwise leave as separate 2-node check.
5. **Combine attic_cache + s3_cache** — create a single `cache` check that tests both backends sequentially (or in parallel on different node sets within the same test).
6. **Delete obsolete check directories** — remove `checks/database/`, `checks/dashboard/`, and the original cache checks once consolidated versions pass.
7. **Update CI/CD configuration** if there are explicit references to check names.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All assertions from database check are covered by the consolidated check (migrations verified)
- [ ] #2 All assertions from dashboard check are covered (Grafana provisioning, datasource, API health)
- [ ] #3 All assertions from server check are preserved (flake polling, eval, branch detection)
- [ ] #4 web-ui check is unchanged
- [ ] #5 oidc-auth check is unchanged
- [ ] #6 builder check coverage is preserved (either merged or kept separate with identical assertions)
- [ ] #7 Attic cache push/pull assertions from attic_cache are preserved in the consolidated cache check
- [ ] #8 S3 cache push/pull assertions from s3_cache are preserved in the consolidated cache check
- [ ] #9 nix flake check passes with the new consolidated check set
- [ ] #10 Total number of separate VM checks is 5 or fewer (down from 8)
- [ ] #11 No new code duplication introduced; shared helpers are used or extended

## Architectural Constraints

- Check definitions live in `checks/<name>/default.nix` following the snowfall-lib convention
- Shared node helpers live in `lib/crystal-forge/`; new helpers MUST go there
- Do not hardcode resource sizes that are already parameterized via helpers
- New combined checks MUST use `skipLint = true; skipTypeCheck = true;` (existing pattern)

## Verification Plan

**Tier 2: Nix Integration Check (required)**

```bash
nix flake check 2>&1 | distill "list each check, pass/fail status, and any failing assertion"
```

Each individual check within the consolidated set must pass. No regressions allowed.

Additionally, validate check count:
```bash
nix flake show --json 2>/dev/null | jq '[.checks."x86_64-linux" | keys[]] | length'
```
Result must be ≤ 5.

## Impact Areas

- `checks/` directory (additions and deletions)
- `lib/crystal-forge/` (shared helpers, potentially extended)
- CI/CD pipeline config (if check names are referenced)
- Potentially `flake.nix` (if checks are referenced explicitly)

## Risk Level

**Medium**

- Risk: Merged checks may hit memory/CPU ceilings and flap under CI resource constraints
- Mitigation: Profile each candidate merge; keep builder separate if combined memory exceeds ~10 GB
- Risk: Assertions may rely on service startup order that changes when co-located
- Mitigation: Preserve all existing `waitForUnit` / `waitForOpenPort` sequencing in merged checks

## Dependencies

None
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-sonnet-4-6 on crystal-forge in ~/code/crystal-forge/TASK-259-consolidate-vm-checks
<!-- SECTION:NOTES:END -->
