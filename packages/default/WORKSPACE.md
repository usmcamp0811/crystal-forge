# Crystal Forge Backend — Workspace Architecture

This document describes the Cargo workspace layout after the crate split
introduced in TASK-395. Each production component is independently selectable
by Cargo and independently buildable by Nix.

## Workspace layout

```
packages/default/
├── Cargo.toml          # Virtual workspace manifest
├── Cargo.lock          # Shared workspace lock file
└── crates/
    ├── cf-protocol/    # Wire protocol types (no server deps)
    ├── cf-config/      # Configuration loading (no DB)
    ├── cf-agent/       # Deployment agent
    ├── cf-builder/     # Remote build worker
    ├── cf-keygen/      # Key generation utility
    └── cf-server/      # HTTP server, queries, migrations, tasks
```

## Crate boundaries

| Crate | Binary | Purpose | Key deps |
|---|---|---|---|---|
| `cf-protocol` | — | Wire types (builder↔server, agent↔server) | serde, chrono |
| `cf-config` | — | TOML/env config loading | cf-protocol, config, serde |
| `cf-agent` | `agent` | NixOS deployment agent | cf-protocol, cf-config, nix, reqwest, sysinfo |
| `cf-builder` | `builder` | Remote Nix build worker | cf-protocol, cf-config, reqwest |
| `cf-keygen` | `cf-keygen` | Ed25519 keypair generator | ed25519-dalek, rand |
| `cf-server` | `server`, `test-agent` | API server, DB, jobs | sqlx, axum, cf-protocol, cf-config |

### Dependency direction

```
cf-keygen     (no local deps)
cf-protocol   (no local deps)
cf-config  ── cf-protocol
cf-agent   ── cf-config, cf-protocol
cf-builder ── cf-config, cf-protocol
cf-server  ── cf-config, cf-protocol (cf-agent and cf-builder are separate)
```

`cf-server` does NOT depend on `cf-agent` or `cf-builder`. The server includes
the agent and builder binaries alongside it in the Nix `server` output, but
they are built from their own separate derivations.

## Targeted Cargo checks

```bash
# Check only the agent (no SQLx, Axum, or server deps compiled):
SQLX_OFFLINE=true cargo check -p cf-agent --all-targets --manifest-path packages/default/Cargo.toml

# Check only the builder:
SQLX_OFFLINE=true cargo check -p cf-builder --all-targets --manifest-path packages/default/Cargo.toml

# Check only the server:
SQLX_OFFLINE=true cargo check -p cf-server --all-targets --manifest-path packages/default/Cargo.toml

# Check the key generation utility:
cargo check -p cf-keygen --manifest-path packages/default/Cargo.toml

# Check all workspace members:
SQLX_OFFLINE=true cargo check --workspace --all-targets --manifest-path packages/default/Cargo.toml

# Run targeted tests:
SQLX_OFFLINE=true cargo test -p cf-agent --manifest-path packages/default/Cargo.toml
SQLX_OFFLINE=true cargo test -p cf-builder --manifest-path packages/default/Cargo.toml
SQLX_OFFLINE=true cargo test -p cf-protocol --manifest-path packages/default/Cargo.toml
SQLX_OFFLINE=true cargo test -p cf-config --manifest-path packages/default/Cargo.toml
```

## Targeted Nix builds

```bash
# Build the deployment agent (does NOT build server/builder packages):
nix build .#agent --no-link

# Build the server (includes server + cf-keygen + builder):
nix build .#server --no-link

# Build the remote builder:
nix build .#builder --no-link

# Build the key generation utility:
nix build .#cf-keygen --no-link

# Legacy full workspace build (all components):
nix build . --no-link
```

## Forbidden dependency boundaries

The acceptance criteria require these dependency exclusions:

### `cf-agent` must NOT depend on:
- `sqlx`, `postgres`, PostgreSQL clients
- `axum`
- `openidconnect`, `jsonwebtoken`, `argon2`
- Server query/task/handler modules

Verify: `cargo tree -p cf-agent --bin agent | grep -iE 'sqlx|axum|argon|oidc|jwt'`
→ expected: no output

### `cf-builder` must NOT depend on:
- `sqlx`, `postgres`, PostgreSQL clients
- Server-only authentication, query, handler, or background-task code

Verify: `cargo tree -p cf-builder --bin builder | grep -iE 'sqlx|axum|argon|oidc|jwt'`
→ expected: no output

## Timing evidence

Baseline (before split, full monolithic crate, SQLX_OFFLINE=true, warm cache):
- `cargo check --bin agent`: 35.23s
- `cargo check --bin builder`: 31.98s
- `cargo check --bin server`: 29.55s

After split (clean build, SQLX_OFFLINE=true):
- `cargo check -p cf-agent --all-targets`: 13.93s (58 crates)
- `cargo check -p cf-builder --all-targets`: 11.24s
- `cargo check -p cf-server --all-targets`: 68s (full server dep graph)

After split (incremental, no changes):
- `cargo check -p cf-agent --all-targets`: 0.25s

The targeted agent and builder checks no longer compile the server crate
dependency set (sqlx, axum, openidconnect, argon2, etc.).

## Known follow-ups (outside this MR)

- **Deployment-policy schema deduplication**: The `DeploymentPolicy` struct and
  `DeploymentPolicyKind` enum are currently duplicated in both `cf-protocol`
  and `cf-server`. After the `cf-server` row type is separated from the
  serializable DTO, the protocol copy can become the single canonical
  definition. (P2, not blocking merge.)

- **SystemState unification**: `cf-server/src/models/system_states.rs` contains
  a second `SystemState` definition (with `sqlx::FromRow`). Once server row
  types are cleanly split from protocol DTOs, the server copy should delegate
  to `cf_protocol::agent::SystemState`. (P1, deferred to keep this MR focused.)

## SQLx offline metadata

Server SQLx query metadata lives at `crates/cf-server/.sqlx/`. When modifying
server queries, regenerate with:

```bash
nix develop -c bash -c 'cd packages/default && cargo sqlx prepare --workspace'
```
