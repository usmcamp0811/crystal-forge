---
type: Concept
title: Project introduction and key features
description: States what Crystal Forge is, its v0.3.0 status, the key features by area (monitoring, build coordination, deployment, authentication), and short data model and security model summaries; open it for the feature-level pitch before the detailed concepts.
tags:
  - crystal-forge
  - overview
  - features
  - orientation
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file README.md at commit 3b23d36f"
    title: Crystal Forge README
---

# Project introduction and key features

> **Status:** partial. This concept holds the `What is Crystal Forge?`, `Key Features`, `Data Model`, and `Security Model` sections of the repository `README.md`. The feature list was written for v0.3.0. The STIG count and the `Security Model` bullets are corrected below. The deployment policy names and the cache types were not re-checked in this pass.

> **Status:** historical. The original `README.md` header at commit 3b23d36f used the logo path shown below. That path (`../docs/cf-logo-transparent.png`) was stale because the logo is at `docs/cf-logo-transparent.png`. The migrated `README.md` uses `./docs/cf-logo-transparent.png`.
>
> ```html
> <img src="../docs/cf-logo-transparent.png" alt="Crystal Forge" width="300">
> ```

## What is Crystal Forge?

Crystal Forge is a self-hosted monitoring, compliance, and build system purpose-built for NixOS fleets. It provides cryptographically-verified system state tracking, automated build coordination, CVE scanning, and policy-based deployment management—built toward the goal of auditability and control in regulated environments.

**Current Status**: v0.3.0 — The core backend is solid and the web UI covers the main workflows, but the UI still has rough edges and a number of planned features aren't implemented yet. Aimed at homelabbers and NixOS enthusiasts who want to run it and help shape it; the compliance and regulated-environment story is still in progress.

## Key Features

### System Monitoring & Compliance

- **Cryptographic verification**: Ed25519 signatures on all agent communications
- **System fingerprinting**: Hardware, software, network interfaces, and security status tracking
- **Configuration drift detection**: Compare running systems against evaluated configurations
- **Intelligent heartbeats**: Distinguish between liveness signals and actual state changes
- **Agent health monitoring**: Track agent connectivity and state reporting frequency
- **STIG Compliance Modules**: Declarative security controls. The flake exports 25 control modules and four presets (`high`, `medium`, `low`, `off`). See [STIG modules](../compliance/stig-modules.md).

### Build Coordination

- **Automatic NixOS evaluation**: Track derivations from Git commits
- **Parallel build processing**: Concurrent derivation evaluation and building with resource limits
- **Binary cache integration**: Push to S3, Attic, or standard Nix caches
- **CVE scanning**: Automated vulnerability assessment with vulnix integration
- **Resource isolation**: SystemD-scoped builds with configurable memory and CPU limits
- **Build queue management**: Track in-progress and completed builds with status visibility

### Deployment Management

- **Deployment policies**: `manual`, `auto_latest`, or `pinned` deployment strategies
- **Deployment strategies**: `immediate_persist` (default) or `boot_only`
- **Fleet tracking**: Monitor which systems are running which configurations
- **Flake integration**: Native support for NixOS flakes and Git repositories
- **Crystal Forge assertion**: Prevent deployments that would disconnect agents
- **Generation tracking**: NixOS generation creation and verification

### Authentication & Authorization

- **OIDC/OAuth2**: Connect to Keycloak, Authentik, Okta, Azure AD, Google, etc.
- **Local auth**: Username/password for self-hosted deployments
- **Dev mode**: Bypass authentication for local development
- **RBAC**: Admin, Operator, Viewer roles with permission guards
- **Session security**: HttpOnly secure cookies, CSRF protection, JIT provisioning

## Data Model

- **Commits**: Git commits in monitored flakes
- **Derivations**: NixOS configurations evaluated from commits
- **Systems**: Monitored NixOS hosts with configurations and policies
- **System States**: Periodic fingerprints (hardware, software, network)
- **Agent Heartbeats**: Connectivity and health signals
- **CVE Data**: Vulnerabilities from vulnix scans
- **Deployment Status**: Current vs. desired configuration state
- **STIG Controls**: Active/inactive compliance controls
- **Users & Roles**: Identity mappings, RBAC, sessions

## Security Model

- **Ed25519 signatures**: All agent-server communication verified
- **Hardware fingerprints**: Unique system identification
- **Encrypted transport**: Run the server behind HTTPS. The server binds plain HTTP on `0.0.0.0` and does not enforce HTTPS itself, so terminate TLS in a reverse proxy. The session cookies use the `Secure` attribute and the `__Host-` prefix, which browsers accept only from a secure context. An agent connects with `https` only when its `server_port` is 443. Otherwise it uses `http`.
- **Secure by default per imported STIG control**: A STIG control module that a system imports is enabled unless the configuration disables it with a justification. The `crystal-forge` NixOS module does not import the controls, so a system is hardened only when it imports them.
- **Authentication**: OIDC or local with secure sessions
- **Authorization**: Viewer, Operator, and Admin roles, enforced by role checks in the API handlers
- **Session security**: HttpOnly session cookies, a double-submit CSRF token (checked by the handlers that call `require_csrf`), JIT provisioning

## Related concepts

- [Crystal Forge system overview](system-overview.md)
- [Ecosystem architecture summary](../architecture/ecosystem-architecture-summary.md)
- [Crystal Forge v0.3.0 release notes](release-notes-v0-3-0.md)
- [Release roadmap and milestones](release-roadmap-and-milestones.md)
- [Core entities and relationships](../data-model/core-entities-and-relationships.md)
- [Authentication and authorization overview](../security/authentication-and-authorization-overview.md)
- [STIG modules](../compliance/stig-modules.md)
- [NixOS module configuration quick start](../operations/nixos-module-configuration.md)
