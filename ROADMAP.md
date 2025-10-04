# Crystal Forge Roadmap

## Where We Are

Crystal Forge currently provides:

- System monitoring and state tracking with Ed25519 signed communication
- Flake/commit tracking and evaluation
- Basic deployment enforcement (with some bugs to work out)
- Database views for fleet status
- PostgreSQL coordination between server, builder, and agent components

## Where We're Going

### 1. Stabilization

Fix the existing deployment tracking and enforcement to be production-ready. This means:

- Reliable agent heartbeats and state reporting
- Accurate deployment status tracking
- Better error handling throughout the system
- Comprehensive test coverage

### 2. Deployment Policy Engine

Build a system that lets you define rules for when systems should receive updates. Policies might include:

- Only deploy if CVE count is below a threshold
- Require manual approval for production systems
- Block deployments with critical security issues
- Only deploy during maintenance windows
- Gradual rollout strategies (canary deployments)

The engine evaluates policies against systems/flakes and enforces them during deployment decisions. Include override mechanisms for emergencies with proper audit trails.

### 3. CVE Dashboard & Visualization

Comprehensive CVE tracking across the fleet:

- Fleet-wide CVE summary dashboards (Grafana)
- Per-system and per-package vulnerability drill-down
- CVE severity trending over time
- Remediation tracking and velocity metrics
- Alert rules for new critical vulnerabilities
- Integration with deployment policies (block deploys with high CVEs)
- Export functionality for compliance reporting

### 4. STIG NixOS Modules

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

### 5. Tvix/Rvix Integration

Replace system calls to Nix CLI with native Rust evaluation using Tvix. This means:

- Native Rust flake evaluation without spawning processes
- Better performance and error handling
- Tighter integration between Crystal Forge and Nix evaluation
- Reduced memory usage

Start with investigation of Tvix maturity, build a proof of concept, then migrate incrementally. Keep Nix CLI as fallback during transition.

## Future Possibilities

Beyond the core roadmap, potential directions include:

- Custom web frontend to replace Grafana
- Multi-tenancy for service providers
- Additional compliance frameworks (NIST 800-53, SOC2, ISO 27001)
- Remote management capabilities (pull-based deployments, fleet orchestration)
