# Crystal Forge

Crystal Forge is a lightweight monitoring and compliance system for NixOS machines—designed to be a self-hosted alternative to tools like Microsoft Intune, but purpose-built for reproducibility-focused environments.

Crystal Forge builds secure, verifiable software ecosystems by embedding compliance, integrity, and trust into the entire lifecycle—from development to deployment. We empower organizations to evolve DevSecOps into a system of continuous assurance, where every component is provable, auditable, aligned with policy, and mapped directly to required security frameworks.

> 📋 **Beta Release**: This is the first official release (v0.1.0) with core monitoring functionality. While stable for basic use cases, advanced features are still in development.

---

## ✨ Goals

- Declarative and reproducible system state tracking
- Strong cryptographic identity and verification of agents
- Simple, reliable communication between clients and server
- Store system metadata and fingerprints for compliance/auditability
- Efficient monitoring with intelligent change detection

---

## 🧱 Architecture

- **`agent`**: runs on NixOS systems, monitors configuration changes, sends signed system fingerprints
- **`server`**: receives and verifies agent reports, evaluates NixOS configurations, stores in PostgreSQL
- **`evaluation engine`**: builds and tracks NixOS system derivations from Git commits
- **`heartbeat system`**: efficient monitoring distinguishing between liveness signals and actual changes

---

## 📦 Features (v0.1.0)

### Core Monitoring

- [x] Comprehensive system fingerprint collection (hardware, software, network, security)
- [x] Ed25519 signature-based agent authentication
- [x] Intelligent heartbeat vs. state change detection
- [x] Real-time system configuration monitoring via inotify

### Infrastructure

- [x] PostgreSQL backend with optimized schema
- [x] Database migrations and auto-initialization
- [x] Dual endpoint architecture (`/agent/heartbeat`, `/agent/state`)
- [x] Git webhook integration for configuration updates

### NixOS Integration

- [x] Automatic NixOS configuration evaluation
- [x] Derivation path tracking and comparison
- [x] Background processing with concurrent evaluation
- [x] System drift detection (current vs. latest configurations)

### Coming Soon

- [ ] Web dashboard for system monitoring
- [ ] Advanced compliance reporting
- [ ] Rule-based policy enforcement
- [ ] Remote system management capabilities

---

## 🛠️ Running

Crystal Forge supports configuration via a `config.toml` file **or** via structured environment variables.
A first-party NixOS module is provided to make setup seamless and reproducible.

---

### 🔧 Option 1: `config.toml`

You can configure Crystal Forge using a simple TOML file:

```toml
[database]
host = "localhost"
user = "crystal_forge"
password = "password"
name = "crystal_forge"

[server]
host = "0.0.0.0"
port = 3000

[client]
server_host = "localhost"
server_port = 3000
private_key = "/var/lib/crystal_forge/host.key"

# System configurations to track
[[systems]]
hostname = "server1"
flake_url = "git+https://github.com/yourorg/nixos-configs"
derivation = "/nix/store/...-nixos-system-server1"
```

Optionally set `CONFIG_PATH=/path/to/config.toml` to override the default location.

---

### 🌱 Option 2: Environment Variables

Crystal Forge can be configured entirely via environment variables, using a nested key format with double underscores:

#### Server

```bash
CRYSTAL_FORGE__SERVER__HOST=0.0.0.0
CRYSTAL_FORGE__SERVER__PORT=3000
CRYSTAL_FORGE__DATABASE__HOST=localhost
CRYSTAL_FORGE__DATABASE__USER=crystal_forge
CRYSTAL_FORGE__DATABASE__NAME=crystal_forge
CRYSTAL_FORGE__DATABASE__PASSWORD=password
```

#### Agent

```bash
CRYSTAL_FORGE__CLIENT__SERVER_HOST=localhost
CRYSTAL_FORGE__CLIENT__SERVER_PORT=3000
CRYSTAL_FORGE__CLIENT__PRIVATE_KEY=/var/lib/crystal_forge/host.key
```

---

### 🧊 Option 3: NixOS Module

```nix
{
  services.crystal-forge = {
    enable = true;

    database = {
      host = "localhost";
      user = "crystal_forge";
      name = "crystal_forge";
      passwordFile = "/run/secrets/crystal_forge_db_password";
    };

    server = {
      enable = true;
      host = "0.0.0.0";
      port = 3000;
    };

    client = {
      enable = true;
      server_host = "localhost";
      server_port = 3000;
      private_key = "/var/lib/crystal_forge/host.key";
    };

    # Systems to monitor
    systems = [
      {
        hostname = "server1";
        flake_url = "git+https://github.com/yourorg/nixos-configs";
        derivation = "/nix/store/...-nixos-system-server1";
      }
    ];
  };
}
```

The module will automatically generate the correct environment variables, systemd services, and config paths based on your input.

## 🔍 Monitoring Features

### System Fingerprinting

Crystal Forge collects comprehensive system information including:

- Hardware identifiers (serial numbers, UUIDs, MAC addresses)
- System specifications (CPU, memory, network interfaces)
- Security status (TPM, Secure Boot, SELinux, FIPS mode)
- Software versions (NixOS, kernel, agent)

### Change Detection

- **Heartbeats**: Periodic liveness signals without state changes
- **State Changes**: Full system reports when configuration actually changes
- **Drift Detection**: Compare running systems against evaluated configurations

### Performance

- Optimized database storage reducing redundant data
- Concurrent configuration evaluation
- Background processing for minimal system impact

## 🧑‍💻 Development

Crystal Forge includes a full development environment using Nix. To get started:

### 🚀 Quickstart

```bash
nix develop
```

Then, in one terminal:

```bash
process-compose up
```

This will start the PostgreSQL database and the Crystal Forge server with environment variables preloaded.

In another terminal:

```bash
run-agent
```

This launches the agent and sends reports to the local server.

### 🔁 Live Development

To run the server or agent from source instead of the latest build:

```bash
run-server --dev
run-agent --dev
```

This ensures you're running against your latest code changes.

### 🛠 Utilities

From inside the dev shell:

- `sqlx-refresh` — Resets the database and prepares SQLx.
- `sqlx-prepare` — Runs `cargo sqlx prepare` without resetting.
- `simulate-push` — Test webhook functionality

The dev shell auto-generates an Ed25519 keypair if missing and sets all required `CRYSTAL_FORGE__*` env vars.

---

## 📊 System Requirements

- **Server**: PostgreSQL 12+, Linux/macOS
- **Agent**: NixOS systems only
- **Network**: HTTPS recommended for production deployments

## 🔐 Security

- Ed25519 cryptographic signatures for all agent communications
- No sensitive data transmitted without verification
- Hardware-based system fingerprinting for identity assurance
- Configurable authentication keys per system
