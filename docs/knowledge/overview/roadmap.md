---
type: Design Specification
title: "Crystal Forge roadmap"
description: "Lists planned Crystal Forge work (stabilization, deployment policy engine, CVE dashboard, STIG modules, reporting and attestation, Tvix) with a per-item implementation status note; open it to see intended direction."
tags:
  - crystal-forge
  - overview
  - roadmap
  - planning
implementation_status: partial
sources:
  - id: origin
    resource: "Crystal Forge repository file ROADMAP.md at commit 3b23d36f"
    title: "Crystal Forge Roadmap"
---

# Crystal Forge Roadmap

> **Status:** partial. Each roadmap item below carries a short status note based on cheap code checks. Items are not rewritten to match the implementation. All notes are verification candidates.

## Where We Are

> **Status:** The "Where We Are" list has not been rechecked. The repository now also contains a Dioxus web UI (`packages/web-ui`), deployment policies (`packages/default/crates/cf-server/src/models/deployment_policies.rs`), and POA&M and compliance handlers (`packages/default/crates/cf-server/src/handlers/api/poam.rs`).

Crystal Forge currently provides:

- System monitoring and state tracking with Ed25519 signed communication
- Flake/commit tracking and evaluation
- Basic deployment enforcement (with some bugs to work out)
- Database views for fleet status
- PostgreSQL coordination between server, builder, and agent components

## Where We're Going

### 1. Stabilization

> **Status:** ongoing. This item is a quality goal and has no single code state.

Fix the existing deployment tracking and enforcement to be production-ready. This means:

- Reliable agent heartbeats and state reporting
- Accurate deployment status tracking
- Better error handling throughout the system
- Comprehensive test coverage

### 2. Deployment Policy Engine

> **Status:** partial. Deployment policies and a CVE gate exist (`packages/default/crates/cf-server/src/deployment/mod.rs`, `packages/default/crates/cf-server/src/handlers/api/deployment_policies.rs`). Manual approval, maintenance windows, and canary rollouts were not checked.

Build a system that lets you define rules for when systems should receive updates. Policies might include:

- Only deploy if CVE count is below a threshold
- Require manual approval for production systems
- Block deployments with critical security issues
- Only deploy during maintenance windows
- Gradual rollout strategies (canary deployments)

The engine evaluates policies against systems/flakes and enforces them during deployment decisions. Include override mechanisms for emergencies with proper audit trails.

### 3. CVE Dashboard & Visualization

> **Status:** partial. CVE scanning and CVE handlers exist (`packages/default/crates/cf-server/src/handlers/api/cves.rs`), and a Grafana dashboard definition exists (`packages/dashboards/crystal-forge-dashboard.json`). Trending, remediation metrics, alert rules, and export were not checked.

Comprehensive CVE tracking across the fleet:

- Fleet-wide CVE summary dashboards (Grafana)
- Per-system and per-package vulnerability drill-down
- CVE severity trending over time
- Remediation tracking and velocity metrics
- Alert rules for new critical vulnerabilities
- Integration with deployment policies (block deploys with high CVEs)
- Export functionality for compliance reporting

### 4. STIG NixOS Modules

> **Status:** partial. The `mkStigModule` pattern and STIG modules exist (`lib/stig/default.nix`, `modules/nixos/stig/`, `modules/nixos/stig-modules/`). Evaluation-time verification, dashboards, and the justification requirement were not checked.

Build NixOS modules that implement DISA STIGs for automated compliance. Starting point is the `mkStigModule` pattern from dotfiles:

```nix
mkStigModule {
  name = "firewall";
  srgList = [ "SRG-OS-000298-GPOS-00116" ];
  cciList = [ "CCI-002322" ];
  stigConfig = { networking.firewall.enable = true; };
}
```

Expand this to cover:

- Base OS hardening controls
- Audit logging (auditd configuration)
- Authentication and access control (PAM, SSH)
- Network hardening (sysctl, firewall rules)
- Filesystem security (permissions, mount options)
- Application security templates

Build compliance verification into the evaluation process and track STIG status in Crystal Forge. Create dashboards showing which systems meet which controls. Require justifications for disabled controls.

### 5. Standardized Reporting & Attestation

> **Status:** partial. Compliance, POA&M, and register-export handlers exist (`packages/default/crates/cf-server/src/handlers/api/compliance.rs`, `poam.rs`, `register_export.rs`), and OSCAL schema packages exist (`packages/oscal-1-1-2-schemas`). Signed attestation and the single-command audit package were not checked.

Build automated compliance reporting and attestation generation to streamline security audits and accreditation processes:

- **Report templates**: Pre-built templates for common frameworks (DISA, NIST, ISO)
- **Evidence collection**: Automatic gathering of system state, CVE scans, STIG compliance data
- **Attestation generation**: Signed attestation documents proving system compliance
- **Audit packages**: Bundle all required documentation for auditors in standard formats
- **Continuous compliance**: Real-time compliance posture tracking with historical evidence
- **Export formats**: PDF, CSV, JSON for different audit requirements
- **Control mapping**: Automatic mapping of system state to security control requirements

The goal: make it trivial to generate all required documentation for passing security audits or accreditations with a single command.

### 6. Tvix/Rvix Integration

> **Status:** proposed. No Tvix reference exists in the code outside documentation.

Replace system calls to Nix CLI with native Rust evaluation using Tvix. This means:

- Native Rust flake evaluation without spawning processes
- Better performance and error handling
- Tighter integration between Crystal Forge and Nix evaluation
- Reduced memory usage

Start with investigation of Tvix maturity, build a proof of concept, then migrate incrementally. Keep Nix CLI as fallback during transition.

## Future Possibilities

> **Status:** partial. A custom web frontend exists (`packages/web-ui`). The remaining items were not checked.

Beyond the core roadmap, potential directions include:

- Custom web frontend to replace Grafana
- TUI and/or CLI for management operations
- Multi-tenancy for service providers
- Additional compliance frameworks (NIST 800-53, SOC2, ISO 27001)
- Remote management capabilities (pull-based deployments, fleet orchestration)

## Related concepts

- [System context and current state](system-context-and-current-state.md) - current state of the product
- [Constraints and policy](constraints-and-policy.md) - constraints every roadmap item must respect
- [Problem statement](problem-statement.md) - the problem the roadmap addresses
