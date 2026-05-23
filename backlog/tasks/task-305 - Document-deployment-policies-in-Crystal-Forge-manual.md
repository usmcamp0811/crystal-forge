---
id: TASK-305
title: Implement deployment policy system for fleet management
status: In Progress
assignee: []
created_date: '2026-05-23 14:12'
updated_date: '2026-05-23 14:56'
labels:
  - feature
  - deployment
  - policies
  - fleet-management
  - architecture
dependencies: []
priority: medium
ordinal: 252000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend Crystal Forge's existing policy engine with new policy types for time-based restrictions, approval workflows, and fleet orchestration. Policies are already represented as JSON/TOML structures with different `policy_type` values. This task adds new policy types beyond the existing `custom_check`, `cve_gate`, and `require_packages` types.

## Current State

Crystal Forge has a policy-as-data system. Policies are JSON/TOML structures with:
- `policy_type`: defines the policy behavior
- `config`: type-specific configuration
- `enabled`: whether policy is active

**Existing policy types:**
- `custom_check` - Evaluate Nix expressions against system config (e.g., `config.services.auditd.enable`)
- `cve_gate` - CVE-based blocking with thresholds and no-scan behavior
  - Example: `require_no_critical_cves` (critical ≤ 0, no-scan → block)
  - Example: `require_high_cve_justification` (high requires justification, no-scan → skip)
- `require_packages` - Guarantee package set is installed
- **Core policies** (always-on, like `require_crystal_forge_agent`)

## Goals

Add new policy types that layer orchestration and workflow logic around the existing evaluation engine:

1. **Time-windowed deployment** - Only allow deployments during specific time windows
2. **Multi-approver workflow** - Require N approvals from operators with specific roles
3. **Canary/phased rollout** - Deploy to fleet subsets with observation periods between phases
4. **Enhanced CVE policies** - More sophisticated CVE gating with configurable thresholds

These should remain **declarative** - stored as JSON/TOML, sharable, version-controllable.

## New Policy Types to Implement

### 1. `time_window` - Time-based deployment restrictions

```json
{
  "policy_type": "time_window",
  "enabled": true,
  "config": {
    "description": "Deploy only during business hours",
    "days": ["mon", "tue", "wed", "thu", "fri"],
    "start_time": "09:00",
    "end_time": "17:00",
    "timezone": "America/New_York",
    "action": "block"  // or "warn"
  }
}
```

**Behavior:**
- Evaluate current time against configured window
- Block or warn when outside window
- Support multiple windows (e.g., weekday + weekend maintenance windows)

### 2. `require_approvals` - Multi-operator approval workflow

```json
{
  "policy_type": "require_approvals",
  "enabled": true,
  "config": {
    "description": "Require 2 admin approvals",
    "count": 2,
    "role": "admin",
    "distinct": true,  // must be different operators
    "expires_after_hours": 24  // approval window
  }
}
```

**Behavior:**
- Track approval state per deployment/commit
- Verify approver has required role
- Enforce distinct approvers (can't approve own deployment twice)
- Expire approvals after time window

### 3. `canary_rollout` - Phased fleet deployment

```json
{
  "policy_type": "canary_rollout",
  "enabled": true,
  "config": {
    "description": "Deploy to 25% at a time, observe 30min",
    "percentage": 25,
    "observe_duration_minutes": 30,
    "selection_strategy": "random",  // or "labeled", "hash-based"
    "health_check": {
      "type": "systemd",  // or "custom_check", "none"
      "fail_threshold": 0  // halt rollout if N systems fail
    }
  }
}
```

**Behavior:**
- Select subset of fleet (25% of systems matching policy)
- Deploy to subset, mark as "canary phase 1"
- Wait observation period
- Check health (systemd status, custom checks, etc.)
- If healthy, continue to next 25%; if unhealthy, halt
- Track rollout state (which systems deployed, which phase)

### 4. `cve_threshold` - Enhanced CVE gating

```json
{
  "policy_type": "cve_threshold",
  "enabled": true,
  "config": {
    "description": "Block critical, limit high CVEs",
    "thresholds": {
      "critical": {"max": 0, "action": "block"},
      "high": {"max": 2, "action": "block"},
      "medium": {"max": 10, "action": "warn"}
    },
    "no_scan_behavior": "block",  // or "skip", "warn"
    "allow_justifications": true,
    "require_acknowledgment": false
  }
}
```

**Behavior:**
- More flexible than binary cve_gate
- Configurable per-severity thresholds
- Different actions per severity (block vs warn)
- Optional justification/acknowledgment workflow

## Architecture Considerations

**Policy evaluation engine:**
- Where do policies get evaluated? (orchestrator, per-system agent, separate policy service)
- How to compose multiple policies? (all must pass, priority order, fail-fast vs collect all violations)

**State persistence:**
- Approval records → database table
- Canary rollout state → deployment tracking table
- Time window evaluation → stateless (evaluate on-demand)

**Fleet awareness:**
- Does CF track system inventory with labels/tags?
- How to query "all systems in group X"?
- How to persist "these systems are in canary phase 1"?

**Scheduler/background jobs:**
- Time-based re-evaluation (check if now in allowed window)
- Canary phase progression (wait 30min, then continue)
- Approval expiration cleanup

## Deliverables

- **New policy type implementations** (4 types: time_window, require_approvals, canary_rollout, cve_threshold)
- **Policy evaluation integration** - Extend engine to handle new types
- **State tracking** - Database schema for approvals, rollout state
- **API endpoints** - Submit approvals, query rollout status, configure policies
- **UI components** (if applicable) - Approval button, rollout status view, policy editor
- **Documentation**:
  - JSON/TOML schema for each new policy type
  - Configuration examples for common scenarios
  - Policy composition behavior (how multiple policies interact)
  - Approval workflow for operators
  - Canary rollout behavior and health checks
- **Tests**:
  - Time window evaluation (TZ handling, day-of-week, time ranges)
  - Approval counting, role checking, expiration
  - Canary subset selection, phase progression, health checks
  - CVE threshold evaluation with multiple severities

## Non-Goals

- Changing existing policy types (custom_check, cve_gate, require_packages)
- Policy inheritance or composition DSL (keep flat for now)
- Full policy audit log / history (could be follow-up)
- Automated rollback on canary health check failure (halt only for MVP)
- External policy engine integration (OPA, Cedar, etc.)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 time_window policy type blocks deployments outside configured time windows
- [ ] #2 time_window policy correctly handles multiple time zones
- [ ] #3 require_approvals policy tracks approval state per deployment
- [ ] #4 require_approvals policy enforces distinct approvers when configured
- [ ] #5 require_approvals policy verifies approver roles
- [ ] #6 require_approvals policy expires approvals after configured duration
- [ ] #7 canary_rollout policy selects correct percentage of fleet
- [ ] #8 canary_rollout policy waits for observation period between phases
- [ ] #9 canary_rollout policy tracks rollout state across phases
- [ ] #10 canary_rollout policy can halt on health check failures
- [ ] #11 cve_threshold policy blocks deployments exceeding configured severity limits
- [ ] #12 cve_threshold policy supports different actions per severity (block/warn)
- [ ] #13 All 4 new policy types can be represented as JSON/TOML
- [ ] #14 Policy evaluation engine integrates new policy types
- [ ] #15 Database schema supports approval and rollout state persistence
- [ ] #16 API endpoints exist for submitting approvals and querying rollout status
- [ ] #17 Documentation includes JSON/TOML schema for each new policy type
- [ ] #18 Documentation includes configuration examples for common scenarios
- [ ] #19 Tests verify time window evaluation logic
- [ ] #20 Tests verify approval workflow and expiration
- [ ] #21 Tests verify canary phase progression
- [ ] #22 Tests verify CVE threshold evaluation
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: agent on gray in ~/code/crystal-forge/TASK-305-deployment-policies

Phase 1-3 completed: Database migrations, type definitions, time window and approval services implemented. Documentation added. Remaining: canary rollout service, CVE threshold service, deployment integration, API endpoints, tests.

## Implementation Strategy Recommendation: This task is large and should be split into focused sub-tasks. Current commit provides foundation (types, schemas, services). Suggest creating follow-up tasks for: 1) Canary rollout orchestration, 2) Deployment integration, 3) API endpoints, 4) Comprehensive testing.
<!-- SECTION:NOTES:END -->
