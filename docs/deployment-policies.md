# Deployment Policies

Crystal Forge supports declarative deployment policies that control how and when systems are deployed. Policies are defined as JSON/TOML structures and can be assigned to environments (mandatory for all systems) or individual systems (optional additions).

## Policy Architecture

- **Nix-Evaluated Policies**: Evaluated during `nix-eval-jobs` at build time (e.g., `require_cf_agent`, `require_packages`, `custom_check`)
- **Deployment-Time Policies**: Evaluated when deployment is requested (e.g., `time_window`, `require_approvals`, `canary_rollout`, `cve_threshold`)

## Built-in Policy Types

### require_cf_agent (Core)
Ensures Crystal Forge agent is enabled on the system.

**Config:**
```json
{
  "policy_type": "require_cf_agent",
  "config": {
    "strict": true
  }
}
```

**Always enforced; cannot be disabled.**

---

### require_packages
Guarantees specific packages are installed.

**Config:**
```json
{
  "policy_type": "require_packages",
  "config": {
    "packages": ["vim", "git", "htop"],
    "strict": true
  }
}
```

---

### custom_check
Evaluate custom Nix expressions against system configuration.

**Single Expression (Legacy):**
```json
{
  "policy_type": "custom_check",
  "config": {
    "expression": "cfg.config.networking.firewall.enable",
    "description": "Firewall must be enabled",
    "field_name": "firewallEnabled",
    "strict": true
  }
}
```

**Multi-Rule (Modern):**
```json
{
  "policy_type": "custom_check",
  "config": {
    "description": "Security hardening baseline",
    "strict": true,
    "mode": "all",
    "rules": [
      {
        "expression": "cfg.config.networking.firewall.enable",
        "description": "Firewall enabled",
        "field_name": "firewallEnabled",
        "strict": true
      },
      {
        "expression": "!cfg.config.services.openssh.settings.PasswordAuthentication",
        "description": "SSH password auth disabled",
        "field_name": "sshKeyOnly",
        "strict": true
      }
    ]
  }
}
```

**Modes:**
- `all`: All rules must pass
- `any`: At least one rule must pass

---

## Advanced Policy Types

### time_window
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

### require_approvals
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

### canary_rollout
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

---

### cve_threshold
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

---

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

---

## Policy Evaluation Flow

### Build-Time (Nix-Evaluated)
1. Load enabled policies from database
2. Filter to Nix-evaluated policies (`require_cf_agent`, `require_packages`, `custom_check`)
3. Build Nix expression embedding policy checks
4. Execute `nix-eval-jobs --meta`
5. Parse results from `meta.policies` JSON
6. Block build queueing for systems failing strict policies

### Deployment-Time
1. Load deployment-time policies (`time_window`, `require_approvals`, `canary_rollout`, `cve_threshold`)
2. Evaluate each policy:
   - `time_window`: Check current time against window
   - `require_approvals`: Query approval records, check count/expiration
   - `canary_rollout`: Check rollout state, select next phase systems
   - `cve_threshold`: Query CVE scan results, evaluate thresholds
3. Block deployment if any policy fails

---

## Example Use Cases

### Production Safety Gate
```json
{
  "policy_type": "require_approvals",
  "config": {
    "description": "Production deployments require 2 admin approvals",
    "count": 2,
    "role": "admin",
    "distinct": true,
    "expires_after_hours": 4
  }
}
```

### Change Window Enforcement
```json
{
  "policy_type": "time_window",
  "config": {
    "description": "Deployments only during maintenance windows",
    "days": ["sat", "sun"],
    "start_time": "02:00",
    "end_time": "06:00",
    "timezone": "UTC",
    "action": "block"
  }
}
```

### Gradual Rollout with Safety Checks
```json
{
  "policy_type": "canary_rollout",
  "config": {
    "description": "Roll out to 10% at a time, observe 1 hour",
    "percentage": 10,
    "observe_duration_minutes": 60,
    "selection_strategy": "hash-based",
    "health_check": {
      "type": "systemd",
      "fail_threshold": 1
    }
  }
}
```

### Zero-Tolerance CVE Policy
```json
{
  "policy_type": "cve_threshold",
  "config": {
    "description": "Block any critical/high CVEs, warn for medium",
    "thresholds": {
      "critical": {"max": 0, "action": "block"},
      "high": {"max": 0, "action": "block"},
      "medium": {"max": 20, "action": "warn"}
    },
    "no_scan_behavior": "block",
    "allow_justifications": false
  }
}
```

---

## Configuration Best Practices

1. **Start with warn actions** when introducing new policies to understand impact
2. **Use distinct approvers** to prevent self-approval
3. **Set reasonable expiration windows** for approvals (4-24 hours)
4. **Test time windows** carefully across timezones
5. **Start with higher canary percentages** (25-50%) and reduce as confidence grows
6. **Allow justifications for CVEs** during migration periods

---

## Future Enhancements

- Policy composition/inheritance
- Audit trail for policy evaluations
- Automated rollback on canary health check failures
- Policy templates for common scenarios
- External policy engine integration (OPA, Cedar)
