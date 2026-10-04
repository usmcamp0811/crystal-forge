---
type: Concept
title: "Deployment Policies"
description: "Explains deployment policy architecture, how the deployment manager applies allow, warn, block, and pending decisions to auto_latest systems, policy assignment to environments and systems, and the build-time and deployment-time evaluation flow."
tags:
  - crystal-forge
  - deployment-policy
  - auto_latest
  - manual
  - pinned
  - rollout-modes
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/deployment-policies.md at commit 3b23d36f"
    title: "Deployment Policies"
---
# Deployment Policies

Crystal Forge supports declarative deployment policies that control how and when systems are deployed. Policies are defined as JSON/TOML structures and can be assigned to environments (mandatory for all systems) or individual systems (optional additions).

## Policy Architecture

- **Nix-Evaluated Policies**: Evaluated during `nix-eval-jobs` at build time (e.g., `require_cf_agent`, `require_packages`, `custom_check`)
- **Deployment-Time Policies**: Evaluated when deployment is requested (e.g., `time_window`, `require_approvals`, `canary_rollout`, `cve_threshold`)

> **Status:** This list names the policy types that this document defines. [Deployment Policy Checks](deployment-policy-checks.md) also defines `require_cve_check` and `composite` (see [Composite policy enforcement](composite-policy-enforcement.md)), which this list omits. The policy dispatch in `packages/default/crates/cf-server/src/server/mod.rs` accepts `require_packages`, `custom_check`, `require_cve_check`, `time_window`, `require_approvals`, `canary_rollout`, `cve_threshold`, and `composite`. Neither source document lists the full set.

Per-system rollout modes (`manual`, `auto_latest`, `pinned`) are a separate concept from the policy types above. The `deployment_policy` column that holds the mode is modeled by `DeploymentPolicy` in `packages/default/crates/cf-server/src/models/systems.rs`. The original proposal for these modes is in [Crystal Forge Agent Deployment Design Document](../historical/agent-deployment-design-proposal.md). The enforcement semantics below apply to systems with `deployment_policy = auto_latest`.

## Deployment-Manager Enforcement Semantics

For systems with `deployment_policy = auto_latest`, Crystal Forge evaluates enabled effective advanced policies before updating `desired_target`.

- `allow` → deployment manager proceeds
- `warn` → deployment manager proceeds and logs a warning
- `block` → deployment manager skips update and logs the blocking reason
- `pending` → deployment manager skips update until condition is satisfied (e.g., approvals pending or canary observation window)

### Advanced Policy Behavior in Deployment Manager

- `time_window`:
  - Evaluated against policy timezone/day/time window.
  - Outside window with `action = block` blocks update.
  - Outside window with `action = warn` allows update with warning log.

- `require_approvals`:
  - Evaluated in commit context (`commit_hash`) for the candidate deployment.
  - Insufficient approvals returns a pending decision; update is deferred.

- `canary_rollout`:
  - Evaluated in commit context (`commit_hash`) across all flake systems governed by the same canary policy.
  - Only systems selected for the current canary phase are allowed to advance.
  - Non-selected systems remain pending for later phases.

- `cve_threshold`:
  - Evaluated against the target derivation's latest CVE scan summary.
  - Threshold block actions prevent update; warn actions log and continue.

## Policy Assignment

### Environment Baseline (Mandatory)
All systems in an environment inherit environment policies (cannot be removed).

**API:**
```bash
PATCH /api/v1/environments/{id}/policies
{
  "policy_ids": ["uuid1", "uuid2", "uuid3"]
}
```

### System-Specific (Optional)
Individual systems can have additional policies on top of environment baseline.

**API:**
```bash
# Add policy to system
POST /api/v1/systems/{id}/policies
{
  "policy_id": "uuid"
}

# Remove system-specific policy (cannot remove environment policies)
DELETE /api/v1/systems/{id}/policies/{policy_id}
```

> **Status:** This section describes assignment to environments and individual systems. The "Applicability" section of [Deployment Policy Checks](deployment-policy-checks.md#applicability) also names compliance bundle assignments and an `enforce` or `report_only` assignment mode as sources of a system's effective policy set. The assignment resolver is in `packages/default/crates/cf-server/src/compliance/resolver.rs`. This migration did not compare the two descriptions.

---

## Policy Evaluation Flow

### Build-Time (Nix-Evaluated)
1. Load enabled policies from database
2. Filter to Nix-evaluated policies (`require_cf_agent`, `require_packages`, `custom_check`)
3. Build Nix expression embedding policy checks
4. Execute `nix-eval-jobs --meta --apply 'derivation: derivation.meta.policies'`
5. Normalize `extraValue` internally and parse the policy payload from `meta.policies`
6. Block build queueing for systems failing strict policies

### Deployment-Time
1. Load deployment-time policies (`time_window`, `require_approvals`, `canary_rollout`, `cve_threshold`)
2. Evaluate each policy:
   - `time_window`: Check current time against window
   - `require_approvals`: Query approval records, check count/expiration
   - `canary_rollout`: Check rollout state, select next phase systems
   - `cve_threshold`: Query CVE scan results, evaluate thresholds
3. Block deployment if any policy fails

> **Status:** The two lists above name the Nix-evaluated and deployment-time policy types of this document only. [Composite policy enforcement](composite-policy-enforcement.md) describes composite rules that run at evaluation, scan, and deployment phases, and final composite authorization before target updates. That authorization is implemented in `packages/default/crates/cf-server/src/services/composite_enforcement.rs` and called from `packages/default/crates/cf-server/src/deployment/mod.rs`.

---


## Related concepts

- [Built-in Policy Types](built-in-policy-types.md)
- [Advanced Policy Types](advanced-policy-types.md)
- [Deployment Policy Checks](deployment-policy-checks.md)
- [Composite policy enforcement](composite-policy-enforcement.md)
- [Deployment policy use cases, best practices, and future enhancements](policy-use-cases-and-best-practices.md)
- [Crystal Forge Agent Deployment Design Document (historical rollout mode proposal)](../historical/agent-deployment-design-proposal.md)
