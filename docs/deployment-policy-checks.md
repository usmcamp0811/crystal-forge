# Deployment Policy Checks

This document describes the deployment **check policy** types supported by Crystal Forge.

> Note: These checks are distinct from environment rollout modes such as `manual`, `auto_latest`, and `pinned`.

## Supported policy types

- `require_cf_agent`
- `require_packages`
- `custom_check`
- `require_cve_check`

## `custom_check`

`custom_check` supports two config shapes.

### 1) Legacy single-expression shape (backward compatible)

```json
{
  "strict": true,
  "expression": "cfg.config.services.openssh.enable"
}
```

### 2) Multi-rule shape (`rules[]` + `mode`)

```json
{
  "strict": true,
  "mode": "all",
  "rules": [
    {
      "field_name": "sshEnabled",
      "expression": "cfg.config.services.openssh.enable",
      "strict": true
    },
    {
      "field_name": "firewallEnabled",
      "expression": "cfg.config.networking.firewall.enable",
      "strict": true
    }
  ]
}
```

Validation and behavior:

- `config.expression` or `config.rules[]` is required.
- `config.mode` must be `all` or `any` when provided.
- Every `rules[i]` entry must include non-empty `field_name` and `expression`.
- `rules[].field_name` values must be unique.
- Expressions are normalized from `config.*` to `cfg.config.*` during validation.

Semantics:

- `mode=all`: all rules must pass.
- `mode=any`: at least one rule must pass.
- `strict=false` records warnings and does not block deployment.

## `require_cve_check`

`require_cve_check` enforces vulnerability posture using the latest completed scan for the built derivation.

Example config:

```json
{
  "max_critical": 0,
  "max_high": 5,
  "require_high_justification": true,
  "strict": true,
  "when_no_scan": "block"
}
```

Config fields:

- `max_critical` (required): maximum allowed critical CVEs.
- `max_high` (optional): maximum allowed high CVEs.
- `require_high_justification` (optional bool): if true, high CVEs must have `whitelist_reason`.
- `strict` (optional bool, defaults to true): blocking vs warning-only behavior.
- `when_no_scan` (`block` or `skip`): explicit behavior when no completed scan exists.

Deployment flow position:

- CVE checks run **after build completes** and **before** `desired_target` is updated for rollout.
- `when_no_scan=block` treats missing scan as a violation.
- `when_no_scan=skip` allows rollout without silent pass/fail ambiguity.

## Seeded canonical CVE policies

Migration seeds two disabled-by-default policies:

1. `require_no_critical_cves` (`max_critical=0`, `strict=true`)
2. `require_high_cve_justification` (`require_high_justification=true`, `strict=true`)
