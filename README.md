<p align="center">
  <img src="cf-bg.png" alt="Crystal Forge" width="300"/>
</p>

<p align="center">
  <img src="cf-bg.png" alt="Crystal Forge" width="300"/>
</p>

<p align="center">
  <strong>Compliance and build coordination for NixOS in regulated environments</strong>
</p>

---

## What is Crystal Forge?

Crystal Forge is a self-hosted monitoring, compliance, and deployment system purpose-built for NixOS fleets. It provides cryptographically-verified system state tracking, automated build coordination, CVE scanning, and policy-based deployment management—designed for organizations that need auditability and control.

**Current Status**: Active development. Core monitoring, build coordination, and deployment enforcement are functional. Advanced features are in progress.

## Key Features

### System Monitoring & Compliance

- **Cryptographic verification**: Ed25519 signatures on all agent communications
- **System fingerprinting**: Hardware, software, network, and security status tracking
- **Configuration drift detection**: Compare running systems against evaluated configurations
- **Intelligent heartbeats**: Distinguish between liveness signals and actual state changes

### Build Coordination

- **Automatic NixOS evaluation**: Track derivations from Git commits
- **Concurrent build processing**: Parallel derivation evaluation and building
- **Binary cache integration**: Push to S3, Attic, or standard Nix caches
- **CVE scanning**: Automated vulnerability assessment with vulnix
- **Resource isolation**: SystemD-scoped builds with memory and CPU limits

### Deployment Management

- **Deployment policies**: Manual, auto_latest, or pinned deployment strategies
- **Fleet tracking**: Monitor which systems are running which configurations
- **Flake integration**: Native support for NixOS flakes and Git repositories
- **Deployment enforcement**: Server-directed system updates with cryptographic verification

### Infrastructure

- **PostgreSQL backend**: Optimized schema with migration support
- **Horizontal scaling**: Multiple servers/builders can share database
- **Web dashboards**: Grafana integration for compliance monitoring (in progress)
- **Git webhook integration**: Automatic evaluation on configuration updates

## Architecture

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│   Agent     │────────>│   Server    │<────────│   Builder   │
│ (NixOS)     │  HTTPS  │  (API/DB)   │   DB    │ (Eval/Build)│
└─────────────┘         └─────────────┘         └─────────────┘
                              │
                              │
                        ┌─────▼─────┐
                        │ PostgreSQL│
                        └───────────┘
```

- **Agent**: Runs on each NixOS system, monitors configuration, sends signed reports
- **Server**: Receives agent reports, coordinates work, provides API
- **Builder**: Evaluates flakes, builds derivations, runs CVE scans
- **Database**: Shared state for coordination and compliance tracking

## Quick Start

### NixOS Module Configuration

```nix
{
  services.crystal-forge = {
    enable = true;

    # Database configuration
    database = {
      host = "localhost";
      user = "crystal_forge";
      name = "crystal_forge";
      passwordFile = "/run/secrets/db_password";
    };

    # Server component (coordination & API)
    server = {
      enable = true;
      host = "0.0.0.0";
      port = 3000;
    };

    # Builder component (evaluation & builds)
    build = {
      enable = true;
      cores = 12;
      max_jobs = 6;
      max_concurrent_derivations = 8;
      systemd_memory_max = "32G";
      systemd_cpu_quota = 800;  # 8 cores
    };

    # Agent component (system monitoring)
    client = {
      enable = true;
      server_host = "crystal-forge.example.com";
      server_port = 3000;
      private_key = "/var/lib/crystal-forge/host.key";
    };

    # Flakes to monitor
    flakes.watched = [
      {
        name = "infrastructure";
        repo_url = "git+ssh://git@gitlab.com/company/nixos-configs";
        auto_poll = true;
        initial_commit_depth = 10;
      }
    ];

    # Systems to track
    systems = [
      {
        hostname = "server1";
        public_key = "base64-encoded-ed25519-pubkey";
        environment = "production";
        flake_name = "infrastructure";
        deployment_policy = "manual";  # or "auto_latest" or "pinned"
      }
    ];

    # Binary cache configuration
    cache = {
      cache_type = "S3";  # or "Attic" or "Nix"
      push_after_build = true;
      push_to = "s3://my-bucket?region=us-east-1";
      parallel_uploads = 4;
    };
  };
}
```

### Environment-Based Configuration

Crystal Forge can also be configured via environment variables:

```bash
# Server
CRYSTAL_FORGE__SERVER__HOST=0.0.0.0
CRYSTAL_FORGE__SERVER__PORT=3000
CRYSTAL_FORGE__DATABASE__HOST=localhost
CRYSTAL_FORGE__DATABASE__NAME=crystal_forge

# Agent
CRYSTAL_FORGE__CLIENT__SERVER_HOST=crystal-forge.example.com
CRYSTAL_FORGE__CLIENT__PRIVATE_KEY=/var/lib/crystal-forge/host.key
```

## Development

### Dev Environment

```bash
nix develop
```

This provides a complete development environment with:

- PostgreSQL database
- Ed25519 keypair generation
- Crystal Forge binaries
- Testing infrastructure

### Running Components

```bash
# Start database and server
process-compose up

# In another terminal, run agent
run-agent

# Run in development mode (live code changes)
run-server --dev
run-agent --dev
```

### Testing

Crystal Forge includes comprehensive NixOS VM-based integration tests:

```bash
# Run all tests
nix flake check

# Specific test suites
nix build .#checks.x86_64-linux.database    # Database tests
nix build .#checks.x86_64-linux.server      # Server tests
nix build .#checks.x86_64-linux.builder     # Builder tests
nix build .#checks.x86_64-linux.s3-cache    # S3 cache tests
nix build .#checks.x86_64-linux.attic-cache # Attic cache tests
```

### Utilities

- `sqlx-refresh` - Reset database and prepare SQLx
- `sqlx-prepare` - Run `cargo sqlx prepare` without reset
- `simulate-push` - Test webhook functionality

## System Requirements

- **Server/Builder**: Linux with Nix, PostgreSQL 12+
- **Agent**: NixOS systems only
- **Network**: HTTPS recommended for production

## Roadmap

See [ROADMAP.md](ROADMAP.md) for development plans:

1. **Stabilization** - Fix deployment tracking bugs, improve testing
2. **Deployment Policies** - Advanced conditional deployment rules
3. **CVE Dashboard** - Grafana dashboards for vulnerability tracking
4. **STIG Modules** - Automated DISA STIG compliance for NixOS
5. **Tvix Integration** - Native Rust Nix evaluation

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Crystal Forge is open source and will remain free for personal and homelab use. Commercial support and features for organizations will be offered in the future to sustain long-term development.

## Security

- Ed25519 cryptographic signatures for agent authentication
- Hardware-based system fingerprinting
- No sensitive data transmitted without verification
- SystemD resource isolation for build operations

## License

See [LICENSE](LICENSE) for details.

## Project Links

- **Repository**: [GitLab](https://gitlab.com/crystal-forge/crystal-forge)
- **Issues**: [Issue Tracker](https://gitlab.com/crystal-forge/crystal-forge/-/issues)
- **Documentation**: [docs/](docs/)
