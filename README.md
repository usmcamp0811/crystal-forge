# Crystal Forge

Crystal Forge is a lightweight monitoring and compliance system for NixOS machines—designed to be a self-hosted alternative to tools like Microsoft Intune, but purpose-built for reproducibility-focused environments.

> ⚠️ **Early days:** This project is under active development and not yet production-ready. I’m working to get the minimal set of features running first.

---

## ✨ Goals

- Declarative and reproducible system state tracking
- Strong cryptographic identity and verification of agents
- Simple, reliable communication between clients and server
- Store system metadata and fingerprints for compliance/auditability

---

## 🧱 Architecture

- `agent`: runs on NixOS systems and sends signed system fingerprints
- `server`: receives and verifies agent reports, stores in PostgreSQL
- `cf-keygen`: generates Ed25519 keys for agents

---

## 📦 Features (WIP)

- [x] Agent fingerprint collection
- [x] Ed25519 signature-based authentication
- [x] Server ingestion endpoint with verification
- [x] PostgreSQL backend
- [x] Auto-initializes DB table (`system_state`)
- [ ] Dashboard for tracking system state
- [ ] System drift detection
- [ ] Rule-based compliance checks
- [ ] Policy push / remote execution

---

## 🛠️ Running

Crystal Forge supports configuration via a `config.toml` file **or** via structured environment variables. A first-party NixOS module is provided to make setup seamless and reproducible.

---

### 🔧 Option 1: `config.toml`

You can configure Crystal Forge using a simple TOML file:

```toml
[database]
host = "localhost"
user = "crystal_forge"
password = "password"
dbname = "crystal_forge"

[server]
host = "0.0.0.0"
port = 3000

[server.authorized_keys]
host1 = "<base64-pubkey>"
host2 = "<base64-pubkey>"

[client]
server_host = "localhost"
server_port = 3000
private_key = "/var/lib/crystal_forge/host.key"
```

Optionally set `CONFIG_PATH=/path/to/config.toml` to override the default location.

---

### 🌱 Option 2: Environment Variables

Crystal Forge can be configured entirely via environment variables, using a nested key format with double underscores:

#### Server

```bash
CRYSTAL_FORGE__SERVER__HOST=0.0.0.0
CRYSTAL_FORGE__SERVER__PORT=3000
CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__host1=<base64-pubkey>
CRYSTAL_FORGE__DATABASE__HOST=localhost
CRYSTAL_FORGE__DATABASE__USER=crystal_forge
CRYSTAL_FORGE__DATABASE__DBNAME=crystal_forge
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
      dbname = "crystal_forge";
      passwordFile = "/run/secrets/crystal_forge_db_password";
    };

    server = {
      enable = true;
      host = "0.0.0.0";
      port = 3000;
      authorized_keys = {
        host1 = "<base64-pubkey>";
        host2 = "<base64-pubkey>";
      };
    };

    client = {
      enable = true;
      server_host = "localhost";
      server_port = 3000;
      private_key = "/var/lib/crystal_forge/host.key";
    };
  };
}
```

The module will automatically generate the correct environment variables, systemd services, and config paths based on your input.
