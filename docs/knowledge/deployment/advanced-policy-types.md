---
type: Concept
title: "Advanced Policy Types"
description: "Defines the time_window, require_approvals, canary_rollout, and cve_threshold deployment-time policy types with configuration fields, approval workflow and API endpoints, canary phases and state tracking, and the difference from require_cve_check."
tags:
  - crystal-forge
  - deployment-policy
  - time_window
  - require_approvals
  - canary_rollout
  - cve_threshold
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/deployment-policies.md at commit 3b23d36f"
    title: "Deployment Policies"
---
# Advanced Policy Types

## time_window
Restrict deployments to specific time windows.

**Config:**
```json
{
  "policy_type": "time_window",
  "config": {
    "description": "Deploy only during business hours",
    "days": ["mon", "tue", "wed", "thu", "fri"],
    "start_time": "09:00",
    "end_time": "17:00",
    "timezone": "America/New_York",
    "action": "block"
  }
}
```

**Fields:**
- `days`: Array of allowed days (`mon`, `tue`, `wed`, `thu`, `fri`, `sat`, `sun`)
- `start_time`/`end_time`: 24-hour format (HH:MM)
- `timezone`: IANA timezone (e.g., `America/New_York`, `Europe/London`, `UTC`)
- `action`: `block` (prevent deployment) or `warn` (log warning but allow)

**Behavior:**
- Evaluated at deployment time
- Blocks deployment if current time (in configured timezone) falls outside window
- Supports wrap-around windows (e.g., `22:00` - `02:00` crosses midnight)

---

## require_approvals
Require N approvals from operators with specific roles.

**Config:**
```json
{
  "policy_type": "require_approvals",
  "config": {
    "description": "Require 2 admin approvals",
    "count": 2,
    "role": "admin",
    "distinct": true,
    "expires_after_hours": 24
  }
}
```

**Fields:**
- `count`: Number of approvals required
- `role`: Required role for approvers (`admin`, `operator`, etc.)
- `distinct`: If `true`, approvers must be different users
- `expires_after_hours`: Approval validity window (null = never expires)

**Workflow:**
1. Deployment is requested
2. Policy check fails with "awaiting approval" status
3. Operators with required role submit approvals via API
4. Once `count` approvals are collected, deployment proceeds

**Role Enforcement:**
- Approver role is verified at submission time (handler checks user has required role)
- Stored approvals are trusted; role changes do not retroactively invalidate approvals
- For stricter enforcement, re-check roles during policy evaluation (future enhancement)

> **Status:** [Composite policy enforcement](composite-policy-enforcement.md) states that the composite rule `approval_required` is hidden because existing approval records are not bound to the exact deployment target and policy version. The approval code is in `packages/default/crates/cf-server/src/services/approval_policy.rs` and `packages/default/crates/cf-server/src/handlers/api/deployments.rs`. This migration did not reconcile the two statements.

**Current API Endpoints:**
```bash
# Submit approval (requires authentication + role verification)
POST /api/v1/deployments/commit/:commit_id/approve
{
  "policy_id": "uuid",
  "comment": "Approved for production rollout"
}

# Check approval status (requires authentication)
GET /api/v1/deployments/commit/:commit_id/approvals/:policy_id

# Get rollout status (requires authentication)
GET /api/v1/deployments/commit/:commit_id/rollout/:policy_id
```

---

## canary_rollout
Deploy to fleet subsets with observation periods between phases.

**Config:**
```json
{
  "policy_type": "canary_rollout",
  "config": {
    "description": "Deploy to 25% at a time, observe 30min",
    "percentage": 25,
    "observe_duration_minutes": 30,
    "selection_strategy": "random",
    "health_check": {
      "type": "systemd",
      "fail_threshold": 0
    }
  }
}
```

**Fields:**
- `percentage`: Percentage of fleet per phase (1-100)
- `observe_duration_minutes`: Wait time between phases
- `selection_strategy`: How to select systems (`random`, `labeled`, `hash-based`)
- `health_check.type`: Health check method (`systemd`, `custom_check`, `none`)
- `health_check.fail_threshold`: Max failures before halting rollout

**Phases:**
1. Select first `percentage%` of fleet
2. Deploy to selected systems
3. Wait `observe_duration_minutes`
4. Run health checks
5. If healthy, proceed to next phase; if unhealthy, halt
6. Repeat until all systems deployed

**State Tracking:**
- Rollout state persisted in `canary_rollout_state` table
- Tracks current phase, systems in phase, completion/failure status

> **Status:** [Composite policy enforcement](composite-policy-enforcement.md) states that `rollout_percent` is hidden because canary state has no production path that advances rollout phases. `packages/default/crates/cf-server/src/services/canary_rollout.rs` contains `advance_to_next_phase`, and state is stored by `packages/default/crates/cf-server/migrations/0123_add_canary_rollout_state.sql`. This migration did not reconcile the two statements.

---

## cve_threshold
Enhanced CVE gating with per-severity thresholds and actions.

**Config:**
```json
{
  "policy_type": "cve_threshold",
  "config": {
    "description": "Block critical, limit high CVEs",
    "thresholds": {
      "critical": {"max": 0, "action": "block"},
      "high": {"max": 2, "action": "block"},
      "medium": {"max": 10, "action": "warn"}
    },
    "no_scan_behavior": "block",
    "allow_justifications": true,
    "require_acknowledgment": false
  }
}
```

**Fields:**
- `thresholds`: Map of severity → `{max, action}`
  - Severities: `critical`, `high`, `medium`, `low`
  - Actions: `block` or `warn`
- `no_scan_behavior`: What to do when no scan exists (`block`, `skip`, `warn`)
- `allow_justifications`: If `true`, allow CVEs with operator-provided justifications
- `require_acknowledgment`: If `true`, require acknowledgment even for warnings

**Difference from `require_cve_check`:**
- `require_cve_check`: Binary thresholds (max critical, max high), single action (block/warn)
- `cve_threshold`: Per-severity thresholds with independent actions (block critical, warn medium)

> **Status:** The `require_cve_check` policy type is defined in [Deployment Policy Checks](deployment-policy-checks.md#require_cve_check), including its applicability rules and the seeded canonical CVE policies.

---


## Related concepts

- [Deployment Policies](deployment-policies.md)
- [Built-in Policy Types](built-in-policy-types.md)
- [Deployment Policy Checks](deployment-policy-checks.md)
- [Composite policy enforcement](composite-policy-enforcement.md)
- [Deployment policy use cases, best practices, and future enhancements](policy-use-cases-and-best-practices.md)
