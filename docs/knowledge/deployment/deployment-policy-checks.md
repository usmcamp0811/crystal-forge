---
type: Concept
title: "Deployment Policy Checks"
description: "Defines the deployment check policy types (require_cf_agent, require_packages, custom_check, require_cve_check, composite), custom_check validation and semantics, require_cve_check applicability and config, and the seeded canonical CVE policies."
tags:
  - crystal-forge
  - deployment-policy
  - custom_check
  - require_cve_check
  - auto_latest
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/deployment-policy-checks.md at commit 3b23d36f"
    title: "Deployment Policy Checks"
---
# Deployment Policy Checks

This document describes the deployment **check policy** types supported by Crystal Forge.

> Note: These checks are distinct from environment rollout modes such as `manual`, `auto_latest`, and `pinned`.

For the authority boundary between Crystal Forge's packaged option catalog and a monitored flake's actual module graph, see [NixOS Option Metadata Authority](../evaluation/nixos-option-metadata-authority.md). Packaged metadata is policy-authoring guidance; target evaluation remains authoritative.

## Supported policy types

- `require_cf_agent`
- `require_packages`
- `custom_check`
- `require_cve_check`
- `composite` (schema version 1, `all` mode)

> **Status:** This list defines the check policy types of this document. [Deployment Policies](deployment-policies.md) also defines `time_window`, `require_approvals`, `canary_rollout`, and `cve_threshold` (see [Advanced Policy Types](advanced-policy-types.md)), which this list omits. The policy dispatch in `packages/default/crates/cf-server/src/server/mod.rs` accepts all eight type names. Neither document lists the full set; they do not contradict each other on shared types.

Composite enforcement is described in [Composite policy enforcement](composite-policy-enforcement.md).

Historical artifact deployability does not establish current cache readability.
Agent delivery resolves completed publication provenance for the exact authorized
derivation and output path inside the same SERIALIZABLE transaction that claims
pending delivery. Durable database destination IDs survive rename; deleted IDs
never resolve through replacement names or URLs. Only currently enabled,
environment-scoped, publication-backed read sources participate in selection.
Read authentication, confidential transport, and signed agent capabilities are
checked before any delivery mutation. If no source is usable, target and cache
settings are withheld and pending work remains retryable. Source archival alone
does not invalidate retained lineage; policy authorization and readable source
selection are separate delivery-time requirements. See the
[Niks3 operator contract](../caches/niks3-cache.md) for selection and rotation rules.

## `custom_check`

`custom_check` supports two config shapes.

### 1) Legacy single-expression shape (backward compatible)

```json
{
  "strict": true,
  "expression": "config.services.openssh.enable"
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
      "expression": "config.networking.firewall.enable",
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
- `config.*` is canonical; legacy `cfg.config.*` references are normalized to
  `config.*` during validation.

Semantics:

- `mode=all`: all rules must pass.
- `mode=any`: at least one rule must pass.
- `strict=false` records warnings and does not block deployment.

## `require_cve_check`

`require_cve_check` enforces vulnerability posture using the latest completed scan for the built derivation.

### Applicability

`deployment_policies.enabled = true` names a policy lineage as usable. It does
not, by itself, make the policy apply to any system. Applicability comes only
from the system's resolved effective policy set: a compliance bundle
assignment (environment- or system-scope), a legacy direct
environment/system policy addition, or a system-scope assignment that
overrides an environment default for the same policy lineage. A `require_cve_check`
policy that exists but is not part of any system's effective set never affects
that system's `auto_latest` deployment.

`auto_latest` evaluates `require_cve_check` for a given system using that
system's effective config for the policy (assignment-level overrides applied),
and only when the effective assignment mode is `enforce`:

- **`enforce`**: a failing check blocks `desired_target` from advancing.
- **`report_only`**: the same evaluation still runs elsewhere in the
  compliance pipeline and can produce a `FAIL` finding, POA&M creation, and
  waiver workflow, but it never blocks `auto_latest` delivery.
- **Unassigned**: the policy has no effect on this system at all.

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

- `max_critical` (optional non-negative integer, defaults to `0`): maximum allowed critical CVEs.
- `max_high` (optional non-negative integer, defaults to no limit): maximum allowed high CVEs.
- `require_high_justification` (optional bool, defaults to `false`): if true, high CVEs must have `whitelist_reason`.
- `strict` (optional bool, defaults to true): blocking vs warning-only behavior.
- `when_no_scan` (optional `block` or `skip`, defaults to `block`): behavior when no completed scan exists.

Deployment flow position:

- CVE checks run **after build completes** and **before** `desired_target` is updated for rollout.
- `when_no_scan=block` treats missing scan as a violation.
- `when_no_scan=skip` allows rollout without silent pass/fail ambiguity.

## Seeded canonical CVE policies

Migration seeds two disabled-by-default policies:

1. `require_no_critical_cves` (`max_critical=0`, `strict=true`)
2. `require_high_cve_justification` (`require_high_justification=true`, `strict=true`)

## Related concepts

- [Deployment Policies](deployment-policies.md)
- [Built-in Policy Types](built-in-policy-types.md)
- [Advanced Policy Types](advanced-policy-types.md)
- [Composite policy enforcement](composite-policy-enforcement.md)
