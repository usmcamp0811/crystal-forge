---
type: Operator Guide
title: "Server configuration reference"
description: "Shows the config.toml structure (database, server, auth, cache, systems, builder, flake watch list) as read by cf-config, the environment variables that override it, and the defaults that differ in the NixOS module; open it when configuring a Crystal Forge server."
tags:
  - crystal-forge
  - operations
  - configuration
  - toml
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/00-system-overview.md at commit 3b23d36f"
    title: "Crystal Forge - System Overview"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-config/src/config/mod.rs at commit 3b23d36f"
    title: Root configuration struct and loader
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-config/src/config/server.rs at commit 3b23d36f"
    title: Server configuration struct
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-config/src/config/flakes.rs at commit 3b23d36f"
    title: Flake configuration struct
  - id: code-4
    resource: "Crystal Forge repository file packages/default/crates/cf-config/src/config/database.rs at commit 3b23d36f"
    title: Database configuration struct
  - id: code-5
    resource: "Crystal Forge repository file packages/default/crates/cf-config/src/config/auth.rs at commit 3b23d36f"
    title: Git authentication configuration struct
  - id: code-6
    resource: "Crystal Forge repository file packages/default/crates/cf-config/src/config/system.rs at commit 3b23d36f"
    title: Static system configuration struct
  - id: code-7
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/security/cache_secrets.rs at commit 3b23d36f"
    title: Cache secret encryption key variables
  - id: code-8
    resource: "Crystal Forge repository file modules/nixos/crystal-forge/default.nix at commit 3b23d36f"
    title: NixOS module option defaults
verified:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T09:10:00-05:00
---

# Server Configuration Reference

> **Status:** implemented. The configuration structs live in `packages/default/crates/cf-config/src/config/`. The server loads them with `CrystalForgeConfig::load()`. The example below was corrected against those structs. The original example used keys that the structs do not read (see Migration verification notes).

## Configuration

### TOML Config (`config.toml`)

```toml
[database]
host = "localhost"
port = 5432
user = "crystal_forge"
password = "..."
name = "crystal_forge"

[server]
host = "0.0.0.0"
port = 8080
auth_mode = "oidc"  # or "dev" (debug builds only) or "local"

[cache]
push_to = "s3://my-cache"
cache_type = "S3"   # S3 | Attic | Http | Nix (default)

[[systems]]
hostname = "prod-web-01.example.com"
public_key = "<base64 Ed25519 public key>"
environment = "production"

[builder]
# Read only by the builder process.
server_url = "https://crystal-forge.example.com"
private_key_path = "/var/lib/crystal-forge/builder-api.key"

[flakes]
flake_polling_interval = "10m"
commit_evaluation_interval = "1m"

[[flakes.watched]]
name = "nixpkgs"
repo_url = "https://github.com/NixOS/nixpkgs"
branch = "nixos-unstable"
auto_poll = true
```

Builders are not declared in `config.toml` on the server. An operator registers each builder through `POST /api/v1/builders` and the builder process authenticates with its own key (see [Builder architecture](../builders/builder-architecture-and-job-scheduling.md)).

The `[auth]` table configures Git credentials (`ssh_key_path`, `ssh_known_hosts_path`, `netrc_path`, `ssh_disable_strict_host_checking`). It does not configure the user authentication mode. The mode is `server.auth_mode`.

### Environment Variables

- `CRYSTAL_FORGE_CONFIG` - Path to config file. Default `/var/lib/crystal_forge/config.toml`. The file is optional.
- `CRYSTAL_FORGE__<SECTION>__<KEY>` - Overrides any key of the file, for example `CRYSTAL_FORGE__DATABASE__HOST`. The separator is a double underscore.
- `AUTH_MODE=dev|oidc|local` - Supplies the default of `server.auth_mode` when the key is absent. The default is `oidc`.
- `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY` - Key material for encrypting stored cache secrets (AES-256-GCM). `CRYSTAL_FORGE_SECRET_KEY` is the fallback variable.

The server does not read `DATABASE_URL`. The database connection is built from the `[database]` fields. `DATABASE_URL` is used by development tooling and the `crystal-forge-migrate` helper (see [Local development workflow](local-development-workflow.md)).

### Defaults that differ between the Rust structs and the NixOS module

The NixOS module (`modules/nixos/crystal-forge`) writes its own defaults into the generated `config.toml`. These differ from the struct defaults used when a key is absent:

| Key | Rust struct default | NixOS module default |
| --- | --- | --- |
| `deployment.deployment_poll_interval` | 60 s | `15m` |
| `build.poll_interval` | 300 s | `5m` |
| `cache.poll_interval` | 30 s | 5 s |
| `server.host` / `server.port` | `127.0.0.1` / `3000` | not checked |
| `flakes.flake_polling_interval` | 600 s | `10m` |
| `flakes.commit_evaluation_interval` | 60 s | `1m` |
| `vulnix.poll_interval` | 60 s | `1m` |

The server listens on `0.0.0.0:<server.port>` regardless of `server.host` (`TcpListener::bind(("0.0.0.0", port))` in `cf-server/src/bin/server.rs`).

## Related concepts

- [Local development workflow](local-development-workflow.md) - running the server with this configuration
- [Authentication, authorization, and system registration](../security/authentication-and-authorization-overview.md) - auth modes

## Migration verification notes

Scope: every key in the example, every environment variable, and the defaults table were compared with `cf-config`, `cf-server`, and the NixOS module. The NixOS server host and port defaults were not checked.

- Claim: `[database]` has a `url` key.
  Finding: `DatabaseConfig` has `host`, `port` (default 5432), `user`, `password`, `name`; the URL is derived by `to_url()`.
  Evidence: `cf-config/src/config/database.rs`; `cf-server/src/config/mod.rs` (`db_pool_from_config`, `validate_db_connection`).
  Case: documentation stale (corrected in place).
- Claim: `DATABASE_URL` overrides the database connection.
  Finding: The server never reads `DATABASE_URL`. Overrides use `CRYSTAL_FORGE__DATABASE__*`.
  Evidence: `cf-config/src/config/mod.rs` (`load`); `rg DATABASE_URL` finds only test and tooling uses.
  Case: documentation stale (corrected in place).
- Claim: `[auth] mode = "oidc"` selects the auth mode.
  Finding: `AuthConfig` holds Git credential paths. The mode is `server.auth_mode`, defaulting from `AUTH_MODE`, then `oidc`. Values used by the server binary include `dev`, `oidc`, `local`.
  Evidence: `cf-config/src/config/auth.rs`, `server.rs` (`default_auth_mode`); `cf-server/src/bin/server.rs`.
  Case: documentation stale (corrected in place).
- Claim: `[[builders]]` with `name` and `public_key` declares builders.
  Finding: `CrystalForgeConfig` has no `builders` field. It has a `[builder]` table for the builder process (`BuilderConfig`). Builders are registered via the builder API.
  Evidence: `cf-config/src/config/mod.rs`, `builder.rs`; `cf-server/src/bin/server.rs` (`/api/v1/builders`).
  Case: documentation stale (corrected in place).
- Claim: `[[flakes.registry]]` declares watched flakes.
  Finding: The key is `[[flakes.watched]]` with `name`, `repo_url`, optional `branch`, `auto_poll`, optional `initial_commit_depth` (default 5).
  Evidence: `cf-config/src/config/flakes.rs`.
  Case: documentation stale (corrected in place).
- Claim: `[[systems]]` has `name`, `hostname`, `environment`.
  Finding: `SystemConfig` requires `hostname`, `public_key`, `environment`; optional `flake_name`, `deployment_policy` (default `manual`), `desired_target`. There is no `name`.
  Evidence: `cf-config/src/config/system.rs`.
  Case: documentation stale (corrected in place).
- Claim: `CRYSTAL_FORGE_SECRET_KEY` is the session encryption key.
  Finding: It is the fallback key material for encrypting stored cache secrets. `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY` takes precedence. No session-signing use was found in `cf-server/src/security`.
  Evidence: `cf-server/src/security/cache_secrets.rs`.
  Case: documentation stale (corrected in place).
- Claim: `CRYSTAL_FORGE_CONFIG`, `AUTH_MODE`, `cache.push_to`, `cache.cache_type` are read.
  Finding: Confirmed.
  Evidence: `cf-config/src/config/mod.rs`, `cache.rs`, `server.rs`.
  Case: implemented.
- Claim (added): `server.host` controls the bind address.
  Finding: It does not. The listener binds `0.0.0.0`. `ServerConfig::bind_address()` exists but the server binary does not call it.
  Evidence: `cf-server/src/bin/server.rs` (`TcpListener::bind(("0.0.0.0", ...))`).
  Case: documentation stale (stated in the defaults section).
