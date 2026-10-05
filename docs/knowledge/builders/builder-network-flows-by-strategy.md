---
type: Architecture
title: "Builder network flows by execution strategy"
description: "Shows the network sequence diagrams for the builder job lifecycle and for ServerDerivation, SourceReEvaluateVerified with ServerBundledArchive, and LocalGitWorktree, including the delta derivation protocol security properties."
tags:
  - crystal-forge
  - builder
  - network
  - sequence-diagram
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/builder-security-architecture.md at commit 3b23d36f"
    title: "Crystal Forge Builder Security Architecture"
---

# Builder network flows by execution strategy

## 4. Network Flow Diagrams

### 4.1 Builder Job Lifecycle — Complete Network Picture

```mermaid
sequenceDiagram
    participant Builder as Builder Host
    participant Server as CF Server
    participant GitCache as Git Remote / Cache

    Builder->>Server: 1. POST /builders/:id/heartbeat<br/>Ed25519-signed, CPU/RAM metrics
    Server-->>Builder: 200 OK (heartbeat_interval_secs)

    Builder->>Server: 2. POST /builders/:id/next-job<br/>Ed25519-signed, strategy list
    Note over Server: DB: atomic job claim<br/>FOR UPDATE SKIP LOCKED

    alt ServerBundledArchive mode
        Server->>GitCache: git clone --bare / git fetch<br/>(server uses stored SSH key or netrc)
        Note over Server: publish canonical tracked-tree tar and identity<br/>before the build job becomes claimable
    end

    Server-->>Builder: 200 OK — Job Manifest<br/>{job_id, drv_path, source_identity,<br/>archive_url, archive_sha256, expected_drv_path}

    Builder->>Server: 3. GET /builders/:id/jobs/:jid/source-archive<br/>Ed25519-signed
    Server-->>Builder: 200 OK — streaming canonical tar<br/>(ReaderStream, no full artifact in server RAM)

    Note over Builder: enforce authorized size and SHA-256<br/>bounded extraction<br/>Nix store ingestion

    Builder->>Server: 4. POST /builders/:id/jobs/:jid/publish-derivation-closure<br/>Ed25519-signed
    Server->>GitCache: nix copy to the configured cache (server pushes .drv closure)
    Server-->>Builder: 200 OK

    Note over Builder,GitCache: Builder: nix-store --realise<br/>(pulls .drv closure from cache or server<br/>via derivation-archive endpoint)

    Builder->>Server: 5. WS /api/v1/build-jobs/:jid/logs/stream<br/>(real-time log + metrics)
    Note over Builder,Server: WebSocket, falls back to HTTP POST
    Server-->>Builder: streaming build output

    Builder->>Server: 6. POST /builders/:id/jobs/:jid/complete<br/>{output_path, cache_pushed, cache_reference}
    Note over Server: DB: verify cache_reference<br/>against known destinations<br/>create cache_push row
    Server-->>Builder: 200 OK
```

### 4.2 ServerDerivation Strategy (No Source Access on Builder)

```mermaid
sequenceDiagram
    participant S as CF Server
    participant B as Builder Host

    Note over S: 1. Materialize verified commit tree in Nix store.<br/>Evaluate pure with explicit IFD and no lock mutation.<br/>#nixosConfigurations.host.system.build.toplevel.drvPath

    S->>B: 2. Job manifest<br/>{drv_path, execution_strategy, source_input_delivery}

    Note over B: 3. Check: is /nix/store/xxx fully valid?<br/>(nix-store --check-validity)

    alt Already valid
        Note over B: Skip to step 6
    else Delta materialization (preferred)
        B->>S: 3a. GET /derivation-manifest
        Note over S: computes nix-store --query --requisites<br/>from persisted drv_path<br/>returns sorted, deduped path list
        S-->>B: {job_id, drv_path, paths: [...]}

        Note over B: 3b. Check local validity of each manifest path<br/>(chunked 256/batch, per-path fallback)

        B->>S: 3c. POST /derivation-archive {paths: [missing...]}
        Note over S: validates each path ∈ authorized manifest<br/>403 if any outside,<br/>400 if malformed,<br/>204 if empty
        S-->>B: streaming nix-store --export<br/>for exactly the validated subset
        Note over B: pipe → nix-store --import
    else Fallback (delta unsupported)
        Note over S: Server too old for delta (404/405)
        B->>S: GET /derivation-archive (full closure)
        S-->>B: streaming nix-store --export<br/>of full recursive closure
        Note over B: pipe → nix-store --import
    end

    Note over B: 4. Verify full recursive closure<br/>nix-store --check-validity<br/>If incomplete → path_materialization

    Note over S,B: 5. Background (fire-and-forget, non-blocking):<br/>POST /publish-derivation-closure<br/>server runs attic push to cache<br/>(next builder skips step 3)

    Note over B: 6. nix-store --realise /nix/store/xxx.drv<br/>(pulls build INPUTS from substituters/cache)

    B->>S: 7. POST .../complete<br/>{output_path, cache_pushed}
```

**Security property of the delta protocol:**
- The server NEVER exports a path just because the builder asked. The server computes the authorized manifest from its own persisted drv_path and enforces requested ⊆ manifest.
- The builder NEVER sends its store inventory. It only names paths from the manifest the server just gave it.
- A builder requesting a path outside the manifest is a **403 FORBIDDEN** (logged with builder and job IDs; path list is NOT logged).
- A builder requesting a non-store path is a **400 BAD REQUEST**.

**Fallback policy:**
- **404/405** from the delta endpoint = `Unsupported` → transparent fallback to the full closure archive GET. The builder never gets stuck waiting for a server that doesn't speak delta.
- **403**, drv path mismatch, malformed response, or import failure = `Fatal` → hard error. Never silently retried as full archive.
- The fallback distinction is encoded in the `DeltaError` enum at the client level, not an ad-hoc string check.

**What the builder can access in this mode:**
- ✅ CF server API (HTTPS) — for drv manifest, drv archive, and job lifecycle
- ✅ Nix binary caches (HTTPS) — for build INPUTS during `nix-store --realise`
- ❌ Git remotes
- ❌ Database
- ❌ Deployment credentials
- ❌ Other builders

**Note:** No Attic/S3 cache is required for the builder to START a build in this mode. The .drv closure (or the delta subset) arrives directly from the CF server via streaming export. Attic is used in the background to warm the cache for subsequent builds. The build INPUTS (nixpkgs, dependencies) still come from Nix substituters.

### 4.3 SourceReEvaluateVerified + ServerBundledArchive Strategy

```mermaid
sequenceDiagram
    participant Git as Git Remote
    participant S as CF Server
    participant B as Builder Host

    Git-->>S: fetch top-level repo into bare mirror<br/>(server uses stored SSH key from DB)
    Note over S: load and digest-check the artifact<br/>published during authoritative evaluation
    S->>B: job manifest (job_id, archive_url, sha256)
    B->>S: GET /source-archive (authenticated, streaming)
    S-->>B: streaming canonical tracked-tree tar

    Note over B: enforce authorized size and sha256<br/>extract tracked tree safely<br/>verify lock and NAR hashes<br/>nix eval (pure)<br/>compare .drvPath<br/>nix-store --realise
    B-->>S: POST /complete<br/>{output_path, cache_pushed}
```

**What is bundled:** Top-level flake repository only
**What is NOT bundled:** Locked flake inputs (nixpkgs, etc.)
**Implication:** Builder needs substituter access for flake inputs OR inputs must already be in builder's `/nix/store`. Private flake inputs (not nixpkgs) must be: publicly accessible, pre-seeded in the Nix store/cache, or handled by a future full-input-closure mode.

### 4.4 SourceReEvaluateVerified + LocalGitWorktree Strategy

```mermaid
sequenceDiagram
    participant Git as Git Remote
    participant S as CF Server
    participant B as Builder Host

    S->>B: job manifest (repo_url, commit_hash, expected_drv_path)
    Note over S: contract version 1 rejects local_git_worktree<br/>before claiming a job
    B-->>S: report complete
```

**What the builder can access in this mode:**
- ✅ CF server API (HTTPS)
- ✅ Git remote (builder needs network + credentials for the repo URL)
- ✅ Nix binary caches
- ❌ Database
- ❌ Deployment credentials
- ❌ Repositories NOT listed in the job manifest

**Network rule implication:** Builders in this mode need outbound TCP/22 or TCP/443 to the Git remote. For GovCloud or classified networks, prefer ServerBundledArchive instead.

## Related concepts

* [Remote builder execution strategies](remote-build-execution-strategies.md) - Explains the remote build execution strategies (source_re_evaluate_verified, server_derivation), the recommended default, source delivery modes, delta derivation materialization, and the forwarded-HTTPS rule for credential-bearing cache push.
* [Builder trust boundaries and component definitions](builder-trust-boundaries-and-components.md) - Defines the purpose, trust levels, and component definitions (server, builder, agent) that bound what a Crystal Forge remote builder can reach, hold, and compromise; open it to approve or review builder network and credential exposure.
* [Builder filesystem layout, firewall rules, and network-constrained configuration](../operations/builder-network-and-filesystem-requirements.md) - Gives the builder host filesystem layout and cleanup guarantees, the firewall rules required per execution strategy, the server inbound rules, and example configurations for maximum isolation and for colocated deployments.
* [Builder API: job lifecycle endpoints](../api/builder-job-lifecycle-api.md) - Documents the builder-signed endpoints for heartbeat, next-job polling (including 409 evaluator conflicts), derivation manifest and delta/full derivation archives, job completion, failure, and log append.
