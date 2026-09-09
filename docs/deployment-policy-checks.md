# Deployment Policy Checks

This document describes the deployment **check policy** types supported by Crystal Forge.

> Note: These checks are distinct from environment rollout modes such as `manual`, `auto_latest`, and `pinned`.

For the authority boundary between Crystal Forge's packaged option catalog and a monitored flake's actual module graph, see [NixOS Option Metadata Authority](./nixos-option-metadata.md). Packaged metadata is policy-authoring guidance; target evaluation remains authoritative.

## Supported policy types

- `require_cf_agent`
- `require_packages`
- `custom_check`
- `require_cve_check`
- `composite` (schema version 1, `all` mode)

## Composite enforcement

Composite policies expose eight typed rule kinds: `nixos_option`,
`packages_installed`, `packages_absent`, `custom_eval`, `eval_passed`,
`pin_required`, `cve_block`, and `time_window`. These are the exact eight kinds
exposed by the policy editor. `approval_required` remains hidden because the
existing approval records are not bound to the exact deployment target and
policy version with an authoritative delivery-time authorization and immutable
audit trail. `rollout_percent` remains hidden because canary state has no
production path that advances rollout phases. Exposing either would present a
control that can be saved without safely governing deployment. A rule UUID and
its array order are stable policy data; imports and exports must preserve both.

Rules execute at an authoritative phase:

- Evaluation: NixOS options, installed/absent packages, custom expressions,
  successful configuration evaluation, and immutable source pinning.
- Scan: CVE thresholds use the newest scan attempt for the exact derivation.
- Deployment: time windows use the configured IANA timezone at authorization time.

Every rule result records `rule_id`, kind, phase, `pass`/`fail`/`error`/
`not_checked`, blocking state, detail, and structured evidence. Aggregation is
deterministic `all` semantics: `error` takes precedence over `fail`, then
`not_checked`; the final result is `pass` only when every rule passes. A due
deployment phase that is failed, errored, or not checked blocks deployment.

Evaluation expressions use stable policy-version/rule keys. NixOS option lookup
is performed against the target configuration's actual module graph; packaged
option metadata is authoring guidance only. String and lines values are emitted
as escaped semantic Nix literals, including quotes, backslashes, literal
`${...}`, and newlines. Package presence and absence use the legacy package
identity contract: each `environment.systemPackages` entry is matched by `pname`
only; `name` is not a fallback. `custom_eval` uses canonical `config.*`
expressions. Both evaluator forms bind `config` to the target's `cfg.config`, and
legacy `cfg.config.*` expressions are normalized to `config.*`. Evaluation is
contained with `builtins.tryEval`; exceptions and non-boolean values become
`error` rather than crashing the evaluator. `pin_required` compares the expected
full immutable revision extracted from the exact requested flake reference with
Nix's resolved `flake.sourceInfo.rev`; it does not compare display labels or an
unresolved commit string.

Composite assessment rows are normalized and scoped by system, exact derivation
and store path, policy lineage and immutable version, and effective-set digest.
Phase merges are transactional and reject mismatched rule, phase, version,
lineage, or target context. CVE evidence includes the exact scan ID, status,
severity count, threshold, and completion time. Time-window evidence includes
the evaluation timestamp and configured timezone/window. Existing derivation
`policy_results` remains the compatibility evidence envelope; readers prefer an
exact policy-version key and retain lineage-key fallback for legacy rows.

Final composite authorization runs before automatic and manual deployment
target updates, commit and generation rollbacks, and agent target delivery.
Heartbeat delivery re-resolves the exact system target and effective policy set,
so a stale or newly disallowed queued target is withheld. Resolution conflicts,
missing exact-target evidence, stale policy context, and authorization errors all
fail closed.

Legacy standalone policy types and their representations retain their existing
behavior. Composite execution does not replace or weaken the unconditional
Crystal Forge agent check or legacy CVE gates.

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
