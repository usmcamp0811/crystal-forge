---
type: Design Specification
title: "Crystal Forge roadmap"
description: "Lists the capabilities that exist at revision 3b23d36f and the planned work that remains (policy approvals and attestation, CVE trending and alerting, STIG verification, reporting and attestation packages, Tvix); open it to see intended direction."
tags:
  - crystal-forge
  - overview
  - roadmap
  - planning
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T16:40:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file ROADMAP.md at commit 3b23d36f"
    title: "Crystal Forge Roadmap"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/server/mod.rs at commit 3b23d36f"
    title: Deployment policy type validation and background tasks
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/bin/server.rs at commit 3b23d36f"
    title: Server routes (UI, register export, compliance)
---

# Crystal Forge Roadmap

The roadmap lists what exists at revision `3b23d36f` and what is still planned.
A planned item is a requirement. It stays until it is delivered, even if it is
old.

## Delivered capabilities

- System monitoring and state tracking with Ed25519-signed agent communication.
- Flake and commit tracking, with server-side evaluation of every
  `nixosConfiguration`.
- The Dioxus web UI, served by the server, and the HTTP API it uses.
- Local and OIDC authentication with role-based access control.
- API-only builders with signed, session-checked requests and builder-side cache
  publication.
- Deployment policies. The server accepts the `require_cve_check`,
  `time_window`, `require_approvals`, `canary_rollout`, and `cve_threshold`
  policy types.
- CVE scanning with exact evidence, fleet triage, and POA&M workflows.
- Compliance bundles and a register export, with OSCAL schema packaging.
- STIG NixOS modules built with the `mkStigModule` pattern.

## Planned work

### 1. Stabilization (ongoing)

Make deployment tracking and enforcement production-ready:

- Reliable agent heartbeats and state reporting.
- Accurate deployment status tracking, including failed and detached deployments.
- Better error handling throughout the system.
- Comprehensive test coverage.

### 2. Deployment policy engine: approvals and overrides

Deployment policies and the CVE gate exist. These parts of the original goal
remain open:

- Manual approval for production systems that is bound to the exact target
  being deployed, with signed running-state attestations. This is tracked as
  TASK-415 and is not delivered.
- Emergency override mechanisms with audit trails.

Time windows and canary rollouts are accepted policy types. This roadmap does
not claim more than that about their end-to-end behavior. See
[Deployment policies](../deployment/deployment-policies.md).

### 3. CVE trending and alerting

The Dioxus CVE views, fleet inventory, and triage exist. These parts remain
open:

- CVE severity trending over time.
- Remediation tracking and velocity metrics.
- Alert rules for new critical vulnerabilities.
- Export functionality beyond the existing register export.

### 4. STIG NixOS modules

The `mkStigModule` pattern and a set of modules exist. Planned expansion:

```nix
mkStigModule {
  name = "firewall";
  srgList = [ "SRG-OS-000298-GPOS-00116" ];
  cciList = [ "CCI-002322" ];
  stigConfig = { networking.firewall.enable = true; };
}
```

- Base OS hardening controls.
- Audit logging (auditd configuration).
- Authentication and access control (PAM, SSH).
- Network hardening (sysctl, firewall rules).
- Filesystem security (permissions, mount options).
- Application security templates.
- Compliance verification in the evaluation process, and per-control status in
  Crystal Forge.
- Required justifications for disabled controls.

### 5. Standardized reporting and attestation

Compliance, POA&M, and register-export handlers exist. The goal remains to make
audit documentation trivial to generate:

- **Report templates** for common frameworks (DISA, NIST, ISO).
- **Evidence collection** from system state, CVE scans, and STIG compliance.
- **Attestation generation**: signed documents proving system compliance.
- **Audit packages** bundling required documentation in standard formats.
- **Continuous compliance** with historical evidence.
- **Export formats**: PDF, CSV, JSON.
- **Control mapping** from system state to control requirements.

### 6. Tvix/Rvix integration (proposed)

No Tvix code exists. The proposal is to replace Nix CLI calls with native Rust
evaluation:

- Native Rust flake evaluation without spawning processes.
- Better performance and error handling.
- Tighter integration between Crystal Forge and Nix evaluation.
- Reduced memory usage.

Start with an investigation of Tvix maturity, build a proof of concept, then
migrate incrementally. Keep the Nix CLI as a fallback during the transition.

## Future possibilities

- TUI or CLI for management operations.
- Multi-tenancy for service providers.
- Additional compliance frameworks (NIST 800-53, SOC2, ISO 27001).
- Remote management capabilities (pull-based deployments, fleet orchestration).

## Related concepts

- [System context](system-context-and-current-state.md) - how the platform is composed
- [Constraints and policy](constraints-and-policy.md) - constraints every roadmap item must respect
- [Problem statement](problem-statement.md) - the problem the roadmap addresses
- [Release roadmap and milestones](release-roadmap-and-milestones.md) - the dated v0.3.0 release plan
