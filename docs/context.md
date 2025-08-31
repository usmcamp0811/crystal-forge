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

- **Beta release (v0.1.0)** with core monitoring functionality
- **247 commits** with active development (auto-versioning at 0.1.26)
- **Working features**: system fingerprinting, Ed25519 auth, change detection
- **In development**: web dashboard, advanced compliance reporting, remote management

### Communication Patterns

- Agents → Server: HTTP POST with Ed25519 signed payloads
- Shared database: Enables horizontal scaling of servers and builders
- Future: Server → Agent deployment triggers for configuration updates

### Scaling Model

Multiple servers and builders can share the same PostgreSQL instance, enabling distributed processing while maintaining centralized compliance state and coordination.

---

