---
id: TASK-137
title: 'Hotfix: eliminate NixOS generation verification race in crystal-forge agents'
status: In Progress
assignee: []
created_date: '2026-02-27 14:58'
updated_date: '2026-02-27 15:05'
labels:
  - hotfix
  - deployment
  - agent
  - nixos
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
**Goal**
Stop agent deployments from failing with `Generation verification failed: profile points to system-XXX` when the target configuration is actually applied. Make verification race-proof when the system briefly points at an `*-activatable-nixos-system-*` generation.

**Observed behavior**
On deploy, the agent creates an intermediate generation that may point to a store path like `...-activatable-nixos-system-...` (e.g., `system-932-link`) and then shortly after creates the final `...-nixos-system-...` generation (e.g., `system-933-link`). Verification sometimes runs during that window and fails even though the machine reaches the desired target seconds later.

**Desired behavior**
Deploy should only be reported as failed if the system does not converge to the desired store path after a short bounded wait. Intermediate `activatable` generations must be treated as transient.

**Scope**

- Update the agent “generation verification” logic only (minimal changes).
- Do not change build/copy logic.
- Keep existing logging but add one or two debug lines to show what symlinks resolve to during verification.

**Non-goals**

- Refactor deployment architecture.
- Change cache/attic behavior.
- Add long sleeps; this should be a fast bounded convergence wait with retries.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

- Agent no longer fails deployments due to `profile points to system-XXX` when:
  - `readlink -f /run/current-system` equals the desired `/nix/store/...-nixos-system-...`, and
  - `readlink -f /nix/var/nix/profiles/system` converges to the same desired store path within a bounded retry window.

- Verification explicitly tolerates intermediate targets that include `-activatable-nixos-system-`.
- If convergence does not happen within the retry window, deployment fails with a clear message that includes:
  - desired store path
  - resolved `/run/current-system`
  - resolved `/nix/var/nix/profiles/system`

- Add a regression test plan (manual ok) that reproduces the race (or demonstrates the prior failure mode) and shows it no longer occurs.

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. **Locate verification code path**
   - Find the function/method that emits:
     - `Generation verification failed: profile points to system-...`

   - Confirm what it is checking (generation number vs resolved store path).

2. **Make verification compare resolved store paths**
   - Use `readlink -f` (or equivalent Rust `std::fs::canonicalize`) for:
     - `/run/current-system`
     - `/nix/var/nix/profiles/system`

   - Compare both to the desired target store path.

3. **Add bounded convergence retry**
   - Retry for up to ~10–20 seconds total (e.g., 20 attempts × 500ms, or 10 × 1s).
   - During retries:
     - If resolved paths contain `-activatable-nixos-system-`, keep retrying.
     - If they equal the desired target, succeed immediately.

   - On timeout, fail with a message that includes desired + observed resolved paths.

4. **If using systemd-run, wait for completion**
   - If activation is launched via `systemd-run`, ensure the agent waits for the unit/job completion before beginning verification.
   - Minimal acceptable approach: poll `systemctl is-active <unit>` until inactive/failed, then proceed to the convergence retry above.

5. **Add logs**
   - One line per verification attempt at debug level, or only on failure:
     - `desired=... current_system=... system_profile=...`

## Verification

Run on a target host that previously showed the issue.

1. Trigger a deploy that previously produced:
   - `system-XXX-link -> ...-activatable-nixos-system-...`
   - followed by `system-(XXX+1)-link -> ...-nixos-system-...`

2. Confirm agent reports success and does not emit:
   - `Deployment failed: Generation verification failed: profile points to system-...`

3. Confirm end state matches:

```bash
readlink -f /run/current-system
readlink -f /nix/var/nix/profiles/system
sudo nix-env -p /nix/var/nix/profiles/system --list-generations | tail -n 5
```
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: agent-claude on gray in ~/code/crystal-forge/TASK-137-agent-generation-verification-fix
<!-- SECTION:NOTES:END -->

## Definition of Done

- Hotfix merged to `main`.
- Verified on at least one host that previously hit the race (e.g., `lucas`).
- Deployment logs show either:
  - successful convergence within retry window, or
  - a failure with concrete observed symlink targets (if genuinely broken).
