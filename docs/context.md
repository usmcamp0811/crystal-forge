# Crystal Forge Context & Current State

## System Context

Crystal Forge operates as a distributed compliance and build system for NixOS environments, providing monitoring, verification, and build capabilities across regulated deployments.

### Upstream Dependencies

- **Nix ecosystem**: Leverages Nix evaluation engine and flake system
- **NixOS systems**: Monitors and manages NixOS machine configurations
- **PostgreSQL**: Central database for state, compliance data, and build coordination

### System Components

- **Server**: HTTP API for agent communication, compliance reporting, coordination
- **Builder**: Evaluates NixOS configurations, tracks derivations, performs builds
- **Agent**: Runs on monitored NixOS systems, reports state, receives deployment commands

### Current State

- **v0.3.0** — functional web UI, suitable for homelab and tinkering; not yet ready for regulated or production environments
- **2600+ commits** with active development
- **Working features**: system fingerprinting, Ed25519 auth, change detection, web dashboard (Dioxus), OIDC/local auth, RBAC, build coordination, CVE scanning with deduplication, deployment policy enforcement, evaluation cancellation and history
- **Still rough**: UI polish, several views incomplete, many planned features not yet implemented
- **In development**: UI polish, advanced compliance reporting, multi-tenant support

### Communication Patterns

- Agents → Server: HTTP POST with Ed25519 signed payloads
- Shared database: Enables horizontal scaling of servers and builders
- Server → Agent: Deployment triggers for configuration updates (operational)

### Scaling Model

Multiple servers and builders can share the same PostgreSQL instance, enabling distributed processing while maintaining centralized compliance state and coordination.

---

