---
type: Concept
title: "Deployment policy use cases, best practices, and future enhancements"
description: "Gives example policy configurations (approval gate, change window, gradual rollout, zero-tolerance CVE), configuration best practices, and the proposed future enhancements for deployment policies."
tags:
  - crystal-forge
  - deployment-policy
  - examples
  - best-practices
  - future
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/deployment-policies.md at commit 3b23d36f"
    title: "Deployment Policies"
---
# Deployment policy use cases, best practices, and future enhancements

These examples use the policy types defined in [Advanced Policy Types](advanced-policy-types.md). The architecture and assignment model is in [Deployment Policies](deployment-policies.md).

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

> **Status:** proposed. The source lists the items below as future enhancements. This migration did not find an implementation of any of them and did not search for one.

- Policy composition/inheritance
- Audit trail for policy evaluations
- Automated rollback on canary health check failures
- Policy templates for common scenarios
- External policy engine integration (OPA, Cedar)

## Related concepts

- [Deployment Policies](deployment-policies.md)
- [Advanced Policy Types](advanced-policy-types.md)
- [Built-in Policy Types](built-in-policy-types.md)
