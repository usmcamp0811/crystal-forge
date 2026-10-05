---
type: Design Specification
title: "Remote builder execution strategies"
description: "Explains the remote build execution strategies (source_re_evaluate_verified, server_derivation), the recommended default, source delivery modes, delta derivation materialization, and the forwarded-HTTPS rule for credential-bearing cache push."
tags:
  - crystal-forge
  - builder
  - execution-strategy
  - cache
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
verified:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T08:29:24-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/builder-security-architecture.md at commit 3b23d36f"
    title: "Crystal Forge Builder Security Architecture"
  - id: origin-mba
    resource: "Crystal Forge repository file docs/multi-builder-api.md at commit 3b23d36f"
    title: "Multi-Builder API Documentation"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-protocol/src/builder.rs at commit 3b23d36f"
    title: "Strategy and source-delivery enums, next-job conflict reasons"
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-config/src/config/server.rs at commit 3b23d36f"
    title: "Server config defaults for strategy, delivery mode, forwarded-HTTPS trust"
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-config/src/config/builder.rs at commit 3b23d36f"
    title: "Builder config defaults for supported strategies and IFD"
  - id: code-4
    resource: "Crystal Forge repository file modules/nixos/crystal-forge/default.nix at commit 3b23d36f"
    title: "NixOS module options and assertions for strategies"
  - id: code-5
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/builders.rs at commit 3b23d36f"
    title: "next-job, derivation manifest/archive, source-archive handlers"
  - id: code-6
    resource: "Crystal Forge repository file packages/default/crates/cf-builder/src/bin/builder.rs at commit 3b23d36f"
    title: "Builder delta materialization and verified-source evaluation"
  - id: code-7
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/flake/verified_source.rs at commit 3b23d36f"
    title: "Canonical source publication layout and object-format rule"
---

# Remote builder execution strategies

> **Status:** Which strategy is the default depends on the layer. Compiled-in Rust defaults select `server_derivation`: `remote_build_execution_strategy` defaults to `server_derivation` and `supported_execution_strategies` defaults to `["server_derivation"]` (`packages/default/crates/cf-config/src/config/server.rs`, `packages/default/crates/cf-config/src/config/builder.rs`, `cf-protocol` `RemoteBuildExecutionStrategy::default`). The NixOS module defaults select `source_re_evaluate_verified` for both `services.crystal-forge.build.remote_execution_strategy` and `services.crystal-forge.build.supported_execution_strategies`, with `source_delivery_mode = "server_bundled_archive"` (`modules/nixos/crystal-forge/default.nix`). The compiled-in `source_delivery_mode` default is also `server_bundled_archive`. The recommendation below therefore matches the NixOS module and is not the bare-binary default. See the migration verification notes.

## 0. Recommended Default Strategy

**Use `source_re_evaluate_verified` + `server_bundled_archive` for any new remote builder deployment.**

```toml
[server]
remote_build_execution_strategy = "source_re_evaluate_verified"
source_delivery_mode             = "server_bundled_archive"

[builder]
supported_execution_strategies = ["source_re_evaluate_verified"]
```

This is the most reliable path because:
- The builder evaluates the flake locally from a server-provided archive, so there is no dependency on an Attic/S3 binary cache being configured before the build can start.
- The builder compares its locally evaluated `.drvPath` against the server's expected value before building — this gives a cryptographic build-plan integrity check (`derivation_mismatch` hard failure).
- The builder never needs Git credentials or direct Git remote access.
- `.drv` materialization is zero-delay: the builder evaluates from local source, so it produces the `.drv` itself rather than waiting for the server to push it somewhere.

**`server_derivation`** is appropriate when you trust the server's evaluation completely and do not need the builder-side re-evaluation check, or when the builder cannot run `nix eval`. When the `.drv` is not already in the builder's Nix store, the builder streams the `.drv` closure directly from the CF server (no Attic dependency); the server pushes to cache in the background. Build inputs are pulled from configured Nix substituters.

**Summary table:**

| Strategy | Builder needs Git? | Builder needs Attic before build? | Build-plan integrity check | Best for |
|---|---|---|---|---|
| `source_re_evaluate_verified` + `server_bundled_archive` | No | No | ✅ `derivation_mismatch` | Recommended default |
| `source_re_evaluate_verified` + `local_git_worktree` | Not supported by contract v1 | No | N/A | Reserved |
| `server_derivation` | No | No | ❌ Server-trusted only | Simplest path |

## Remote Build Execution Strategies

Crystal Forge remote builders use explicit execution strategies. The scheduler must not silently fall back between strategies; a builder only receives jobs for strategies it is configured to support.

### Recommended Default

**Use `source_re_evaluate_verified` + `server_bundled_archive` for all new deployments:**

```toml
# /etc/crystal-forge/server.toml
[server]
remote_build_execution_strategy = "source_re_evaluate_verified"
source_delivery_mode             = "server_bundled_archive"
source_archive_root              = "/var/lib/crystal-forge/source-archives"

# /etc/crystal-forge/builder.toml
[builder]
supported_execution_strategies = ["source_re_evaluate_verified"]
source_worktree_root            = "/var/lib/crystal-forge/flake-worktrees"
allow_import_from_derivation    = true
```

This is the most reliable and fastest startup path because:
- The builder evaluates the flake locally, so there is no dependency on Attic or any binary cache before the build starts.
- The builder checks the evaluated `.drvPath` against the server's expected value before building — giving a build-plan integrity check (`derivation_mismatch` hard failure).
- No Git credentials or direct Git remote access needed on the builder.

Cache publication note: remote builders may perform the post-build cache push themselves. If the selected cache destination requires credentials, the server sends credential-bearing cache push config in the signed next-job response only when `server.trust_forwarded_builder_https` is enabled and the request is verified as HTTPS by the trusted reverse proxy.

For NixOS deployments behind an HTTPS-terminating reverse proxy, enable the
forwarded-HTTPS trust option on the server:

```nix
services.crystal-forge.server.trust_forwarded_builder_https = true;
```

Only enable this when the proxy is under your control and strips/re-sets
forwarding headers before proxying to Crystal Forge. The backend service should
not be directly reachable by builders or untrusted clients over plaintext HTTP.
The proxy must forward one of the HTTPS indicators recognized by the server,
such as `X-Forwarded-Proto: https`, `Forwarded: proto=https`, or
`X-Forwarded-SSL: on`. If this option is left at its secure default of `false`,
credential-bearing builder cache-push jobs are **not dispatched**. The server
sends no credentials. Instead, `get_next_job` fails the just-claimed job as a
transient `[dispatch:cache_config]` failure through `mark_job_failed_with_retry`.
The automatic retry policy then retries the job with backoff. The server answers
the builder with HTTP `404 Not Found`, which the builder treats as "no work this
cycle" (`fail_claimed_job_at_dispatch` in
`packages/default/crates/cf-server/src/handlers/api/builders.rs`). The server
never sends credentials over a connection that it has not verified as HTTPS.
The forwarded-HTTPS indicators that the server recognizes also include
`X-URL-Scheme: https` (`forwarded_header_asserts_https`).

Verified-source evaluator contract version 1 enables Nix
import-from-derivation (IFD) to preserve the authoritative evaluation behavior.
The NixOS module defaults the builder option to true. A builder that disables
IFD must not advertise `source_re_evaluate_verified` contract version 1:

```nix
services.crystal-forge.build.allow_import_from_derivation = true;
```

### `server_derivation` — when to use it

`server_derivation` is simpler and appropriate when you trust the server's evaluation completely and do not need the builder-side re-evaluation check. It is also the only option if the builder cannot run `nix eval` for some reason.

**Materialization:** When the `.drv` is not already in the builder's local Nix store, the builder streams the `.drv` closure archive directly from the CF server into `nix-store --import` — no Attic or binary cache required. In the background, the server pushes the closure to the configured cache so future builds can pull via normal Nix substituters.

**Source delivery for `source_re_evaluate_verified`** is configured server-side via `source_delivery_mode`:

- **`none`**: Not valid for verified-source jobs. The server rejects this mode before claim.

- **`local_git_worktree`**: Reserved for a future evaluator contract. The server rejects this mode before a version-1 job is claimed.

- **`server_bundled_archive`**: Required by contract version 1. During authoritative evaluation, the server creates one uncompressed tracked-tree tar artifact for the exact commit. The server and builder consume the same bytes. Before claim, the server checks the published size and SHA-256 digest. The builder streams those bytes to a unique temporary file, enforces the authorized size, verifies SHA-256, applies bounded safe extraction, and evaluates the resulting Nix store source without Git credentials.

  Materialization schema version 1 accepts full 40-character SHA-1 Git object
  IDs. It rejects 64-character SHA-256 object IDs with a typed unsupported
  object-format error before mirror initialization. A later schema must define
  SHA-256 mirror initialization and compatibility before enabling those IDs.

- **`builder_fetch_public_inputs`**: Reserved for a future evaluator contract. The server rejects this mode before a version-1 job is claimed.

Contract version 1 has no source-delivery fallback. Only
`server_bundled_archive` can claim a `source_re_evaluate_verified` job.

Server publication layout for `server_bundled_archive`:

```
<source_archive_root>/mirrors/<mirror_id>.git
<source_archive_root>/artifacts/<mirror_id>/<commit_hash>.tar
<source_archive_root>/identities/<mirror_id>/<commit_hash>.json
```

### `server_derivation`

The server evaluates the flake, records the authoritative `.drv` path, and sends that derivation identity to the builder.

**Materialization** uses a **delta-aware protocol** by default:

1. Builder checks whether the `.drv` recursive closure is already valid locally via `nix-store --check-validity`.
2. If not, it requests the derivation **manifest** — the server computes `nix-store --query --requisites` from the job's *persisted* drv_path (never a builder-supplied path) and returns the sorted, deduplicated list of store paths.
3. The builder checks which manifest paths are missing locally. It runs `nix-store --check-validity --print-invalid` in batches of 1024 paths (`VALIDITY_CHECK_BATCH`), with no per-path fallback. It then requests **only the missing subset** via `POST /derivation-archive { "paths": [...] }`. It splits that subset into delta requests whose JSON body is at most 512 KiB (`DERIVATION_DELTA_ARCHIVE_REQUEST_MAX_BYTES`) (`packages/default/crates/cf-builder/src/bin/builder.rs`).
4. Server validates every requested path against the authorized manifest and streams `nix-store --export` for exactly that subset. The builder pipes the response into `nix-store --import`.
5. If the server does not support delta endpoints (404/405), the builder transparently falls back to the full closure archive GET.

All streaming is piped stdout with bounded stderr drain — no full closure is buffered in RAM on either side.

Build inputs (nixpkgs, dependencies) are pulled from configured Nix substituters during `nix-store --realise`. Materialization failures are reported as `path_materialization` failures. Builders do not access Postgres directly.

## Related concepts

* [Verified-source evaluator contract (source_re_evaluate_verified)](verified-source-evaluator-contract.md) - Specifies the verified-source flow where the builder re-evaluates a canonical source archive and compares its .drvPath to the server value, including the evaluator fingerprint, next-job 409 reasons, and rolling-upgrade behavior.
* [Builder network flows by execution strategy](builder-network-flows-by-strategy.md) - Shows the network sequence diagrams for the builder job lifecycle and for ServerDerivation, SourceReEvaluateVerified with ServerBundledArchive, and LocalGitWorktree, including the delta derivation protocol security properties.
* [Builder failure phases and retry strategy](builder-failure-phases-and-retry.md) - Lists the pre-build failure phases a builder reports (source_fetch through build), which of them retry or fail permanently, and the priority-weighting retry and max-retries rules for build jobs.
* [Builder trust boundaries and component definitions](builder-trust-boundaries-and-components.md) - Defines the purpose, trust levels, and component definitions (server, builder, agent) that bound what a Crystal Forge remote builder can reach, hold, and compromise; open it to approve or review builder network and credential exposure.
* [Builder filesystem layout, firewall rules, and network-constrained configuration](../operations/builder-network-and-filesystem-requirements.md) - Gives the builder host filesystem layout and cleanup guarantees, the firewall rules required per execution strategy, the server inbound rules, and example configurations for maximum isolation and for colocated deployments.

## Migration verification notes

Scope: every behavioral claim in this concept was compared with the code at commit `3b23d36f`.

- Claim: `source_re_evaluate_verified` with `server_bundled_archive` is the default for new deployments.
  Finding: True only for the NixOS module. The module defaults `remote_execution_strategy` and `supported_execution_strategies` to `source_re_evaluate_verified` and `source_delivery_mode` to `server_bundled_archive`. The Rust config structs default to `server_derivation` for both the server strategy and the builder's supported list. The module also asserts that a colocated builder supports the server's strategy and that `source_re_evaluate_verified` requires `server_bundled_archive`.
  Evidence: `modules/nixos/crystal-forge/default.nix` (`remote_execution_strategy`, `supported_execution_strategies`, `source_delivery_mode`, assertions near the end of the file); `packages/default/crates/cf-config/src/config/server.rs` (`default_remote_build_execution_strategy`, `default_source_delivery_mode`); `packages/default/crates/cf-config/src/config/builder.rs` (`BuilderConfig::default`).
  Case: documentation stale (default described as one global value; now qualified by layer).
- Claim: `none`, `local_git_worktree`, and `builder_fetch_public_inputs` are not accepted for contract version 1; only `server_bundled_archive` claims a verified-source job.
  Finding: Confirmed. `source_delivery_conflict` returns the `incompatible_source_delivery` 409 before the queue lookup. The NixOS module option accepts only `local_git_worktree` and `server_bundled_archive`.
  Evidence: `source_delivery_conflict`, `source_archive_contract_is_authorized` in `builders.rs`; `SourceInputDeliveryMode` in `cf-protocol/src/builder.rs`.
  Case: none (documentation correct).
- Claim: publication layout under `source_archive_root`, SHA-1-only object IDs, IFD default true in the NixOS module and required for contract version 1.
  Finding: Confirmed. 64-character IDs raise `UnsupportedObjectFormat`. The Rust `BuilderConfig::default` also has `allow_import_from_derivation: true` although its field comment says "Defaults to false".
  Evidence: `packages/default/crates/cf-server/src/flake/verified_source.rs`; `cf-config/src/config/builder.rs`; NixOS assertion `allow_import_from_derivation`.
  Case: none for this concept (the stale field comment in Rust is outside the documentation scope).
- Claim: `server_derivation` delta protocol (manifest from persisted drv_path, 403/400/204 rules, full-archive fallback on 404/405, background cache publish).
  Finding: Confirmed. The delta response content type is `application/x-nix-archive`. The builder triggers `publish-derivation-closure` only after it imported archive data; the server pushes the closure to the first assigned or global cache destination.
  Evidence: `get_job_derivation_manifest`, `download_job_derivation_archive_delta`, `publish_job_derivation_closure` in `builders.rs`; `ensure_derivation_available` in `cf-builder/src/bin/builder.rs`.
  Case: none.
