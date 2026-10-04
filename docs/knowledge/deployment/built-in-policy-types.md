---
type: Concept
title: "Built-in Policy Types"
description: "Defines the require_cf_agent, require_packages, and custom_check policy types with their JSON configuration, the legacy single-expression and multi-rule custom_check shapes, and the all and any modes."
tags:
  - crystal-forge
  - deployment-policy
  - require_cf_agent
  - require_packages
  - custom_check
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/deployment-policies.md at commit 3b23d36f"
    title: "Deployment Policies"
---
# Built-in Policy Types

## require_cf_agent (Core)
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

## require_packages
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

## custom_check
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

> **Status:** The `cfg.config.*` expression prefix in these examples is the legacy form. [Deployment Policy Checks](deployment-policy-checks.md#custom_check) states that `config.*` is canonical and that `cfg.config.*` is normalized to `config.*` during validation. The `custom_check` handling is in `packages/default/crates/cf-server/src/models/deployment_policies.rs`. This migration did not compare the two descriptions with the code.

---


## Related concepts

- [Deployment Policies](deployment-policies.md)
- [Deployment Policy Checks (custom_check validation and semantics)](deployment-policy-checks.md)
- [Advanced Policy Types](advanced-policy-types.md)
