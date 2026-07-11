# Crystal Forge Builder Security Architecture

**Audience:** Network engineers, security architects, and cyber analysts (NSA/DoD context)  
**Classification:** Unclassified // For Official Use  
**Last updated:** 2026-07

---

## 1. Purpose and Scope

This document defines every network boundary, data flow, credential exposure, and trust boundary that exists between the Crystal Forge server, its remote builders, and external systems. It answers the questions a network or security engineer needs to approve or deny network access rules for builder hosts.

A Crystal Forge **builder** is a host that performs Nix builds. It never talks to a database. It never receives credentials for deployment targets. It is intentionally limited to:

1. Polling the Crystal Forge server for work.
2. Pulling build inputs from authorized Nix binary caches.
3. Reporting build results back to the Crystal Forge server.

Everything else — evaluation, policy enforcement, secret management, deployment authorization — stays on the server or on the agent running on the managed NixOS host.

---

## 2. System Components and Trust Levels

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                            TRUST BOUNDARY OVERVIEW                              │
│                                                                                 │
│  ┌──────────────────────────────────┐                                           │
│  │   HIGH TRUST (Server Enclave)    │                                           │
│  │                                  │                                           │
│  │  ┌──────────────┐  ┌──────────┐  │                                           │
│  │  │   CF Server  │  │ Postgres │  │  • Holds all secrets                     │
│  │  │   (Rust)     │──│   DB     │  │  • Authoritative evaluator               │
│  │  │              │  │          │  │  • Controls job queue                    │
│  │  └──────────────┘  └──────────┘  │  • Stores flake credentials              │
│  │         │                        │  • Issues no-reuse session tokens        │
│  └─────────│──────────────────────-─┘                                           │
│            │  HTTPS / Ed25519-signed API                                        │
│            │  (one-way: builders poll server)                                   │
│  ┌─────────┴──────────────────────────────┐                                     │
│  │  REDUCED TRUST (Builder Host)          │                                     │
│  │                                        │  • No DB credentials               │
│  │  ┌────────────────────────────────┐    │  • No Git credentials              │
│  │  │  CF Builder Binary (Rust)      │    │  • No deployment secrets           │
│  │  │                                │    │  • Nix build sandbox enforced      │
│  │  │  • Polls CF server for jobs    │    │  • Holds only its own private key  │
│  │  │  • Downloads source archives   │    │                                    │
│  │  │  • Runs nix-store --realise    │    │                                    │
│  │  │  • Pushes built outputs        │    │                                    │
│  │  └────────────────────────────────┘    │                                    │
│  └────────────────────────────────────────┘                                     │
│                                                                                 │
│  ┌──────────────────────────────────────────────────────────┐                  │
│  │  MONITORED ENDPOINTS (Managed NixOS Hosts)               │                  │
│  │                                                          │                  │
│  │  CF Agent: reports state, receives deployment targets    │                  │
│  │  (agents never talk to builders)                         │                  │
│  └──────────────────────────────────────────────────────────┘                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Component Definitions

### 3.1 Crystal Forge Server

**Role:** Central authority. Only component with database access and credential storage.

| Property | Value |
|---|---|
| Language | Rust (Axum web framework) |
| Database | PostgreSQL (private network, no external exposure) |
| Auth outbound | Ed25519 verification of builder requests; OIDC for human users |
| Network exposure | HTTPS API (configurable port, typically 443/8443) |
| Secrets held | Flake Git credentials (SSH keys / netrc); OIDC client secrets; DB password |
| Evaluator | `nix-eval-jobs` runs on the server with impure mode for remote flake refs |
| Source mirroring | Server maintains bare Git mirrors at `source_archive_root/mirrors/` |
| Source archives | Per-job tar.gz archives at `source_archive_root/archives/jobs/<job_id>.tar.gz` |

**The server is the only component that touches Git remotes or repository credentials.** Builders receive pre-packaged artifacts.

### 3.2 Crystal Forge Builder

**Role:** Isolated build executor. Talks only to the CF server and Nix binary caches.

| Property | Value |
|---|---|
| Language | Rust |
| Database access | **None.** Zero DB credentials. Zero DB network access required. |
| Git access | **None** when `ServerBundledArchive` is configured (recommended for GovCloud) |
| Network outbound | CF server HTTPS; Nix binary cache HTTPS (configurable substituters) |
| Secrets held | **Only** its own Ed25519 private key (`/var/lib/crystal-forge/builder-api.key`) |
| Authentication | Per-request Ed25519 signature on all API calls to the CF server |
| Session scope | Builder session ID scoped to process lifetime; server validates ownership per job |
| Build isolation | Nix sandbox enabled; `--restrict-eval`, no impure by default |

**A builder that is compromised gives an attacker:**
- The builder's Ed25519 private key (allows claiming build jobs only)
- Access to build job outputs before they reach the cache
- No DB access, no deployment credentials, no Git credentials, no other builders' keys

### 3.3 Crystal Forge Agent

**Role:** NixOS host monitor. Reports system state to the server. Receives deployment targets. Never communicates with builders.

| Property | Value |
|---|---|
| Network outbound | CF server HTTPS (heartbeat and state reports only) |
| Auth | Ed25519-signed state reports |
| Deployment | Pull-based: reads `desired_target` store path from server heartbeat response, calls `nixos-rebuild switch --flake <cache-path>` |
| Credentials | Its own private key; no Git credentials; no builder credentials |

---

## 4. Network Flow Diagrams

### 4.1 Builder Job Lifecycle — Complete Network Picture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                    BUILDER JOB LIFECYCLE — NETWORK FLOWS                        │
└─────────────────────────────────────────────────────────────────────────────────┘

Builder Host                           CF Server                     Git Remote / Cache
─────────────                          ─────────                     ─────────────────
    │                                      │
    │  1. POST /builders/:id/heartbeat     │
    │  Ed25519-signed, CPU/RAM metrics     │
    │─────────────────────────────────────>│
    │  200 OK (heartbeat_interval_secs)    │
    │<─────────────────────────────────────│
    │                                      │
    │  2. POST /builders/:id/next-job      │
    │  Ed25519-signed, strategy list       │
    │─────────────────────────────────────>│
    │                                      │ (DB: atomic job claim, FOR UPDATE SKIP LOCKED)
    │                                      │
    │  [IF ServerBundledArchive mode]      │
    │                                      │──────────────────────────────────────>│
    │                                      │  git clone --bare / git fetch         │
    │                                      │  (server uses stored SSH key or netrc)│
    │                                      │<──────────────────────────────────────│
    │                                      │
    │                                      │ (server: tar czf archive, sha256sum)
    │                                      │ archive saved to:
    │                                      │ source_archive_root/archives/jobs/<job_id>.tar.gz
    │                                      │
    │  200 OK — Job Manifest               │
    │  {job_id, drv_path, source_identity, │
    │   archive_url, archive_sha256,        │
    │   expected_drv_path}                 │
    │<─────────────────────────────────────│
    │                                      │
    │  3. GET /builders/:id/jobs/:jid/     │
    │     source-archive                   │
    │  Ed25519-signed                      │
    │─────────────────────────────────────>│
    │  200 OK — streaming tar.gz           │ (server streams from file, ReaderStream)
    │  (no full archive in server RAM)     │
    │<─────────────────────────────────────│
    │                                      │
    │  [Builder: verify SHA-256,           │
    │   extract to job-scoped mirror,      │
    │   git worktree add]                  │
    │                                      │
    │  4. POST /builders/:id/jobs/:jid/    │
    │     publish-derivation-closure       │
    │  Ed25519-signed                      │
    │─────────────────────────────────────>│
    │                                      │──────────────────────────────────────>│
    │                                      │  nix copy --to <cache>               │
    │                                      │  (server pushes .drv closure to cache)│
    │                                      │<──────────────────────────────────────│
    │  200 OK                              │
    │<─────────────────────────────────────│
    │                                      │
    │  [Builder: nix-store --realise]     ─ ─ ─ ─ ─ ─ ─ ─ ─>│ (pull from subst.)
    │  (pulls .drv closure from cache      │                   │
    │   OR server streams via             │                   │
    │   derivation-archive endpoint)       │                   │
    │<─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│
    │                                      │
    │  [nix build runs inside Nix sandbox] │
    │                                      │
    │  5. WS /api/v1/build-jobs/:jid/logs/ │
    │     stream (real-time log + metrics) │
    │─────────────────────────────────────>│ (WebSocket, falls back to HTTP POST)
    │  streaming build output              │
    │                                      │
    │  6. POST /builders/:id/jobs/:jid/    │
    │     complete                         │
    │  {output_path, cache_pushed: true,   │
    │   cache_reference}                   │
    │─────────────────────────────────────>│
    │                                      │ (DB: verify cache_reference against
    │                                      │  known destinations, create cache_push row)
    │  200 OK                              │
    │<─────────────────────────────────────│
```

### 4.2 ServerDerivation Strategy (Default — No Source Access on Builder)

```
┌──────────────────────────────────────────────────────────────┐
│         ServerDerivation — Builder Has No Source Access      │
└──────────────────────────────────────────────────────────────┘

CF Server                                       Builder Host
─────────                                       ────────────
│                                                     │
│ 1. Evaluate flake (server-side):                    │
│    nix eval --impure \                              │
│      "git+ssh://...?rev=<hash>" \                   │
│      #nixosConfigs.host.system.build.toplevel.drvPath
│                                                     │
│ 2. Job manifest sent to builder:                    │
│    { drv_path: "/nix/store/xxx.drv",               │
│      execution_strategy: server_derivation,         │
│      source_input_delivery: none }                  │─────>│
│                                                     │
│ 3. Builder materializes the .drv:                   │
│    Option A: server publishes closure to cache       │
│              builder pulls via substituter          │
│    Option B: server streams nix-store --export      │
│              builder pipes to nix-store --import    │
│    (no Git access, no source code on builder)       │
│                                                     │
│ 4. Builder runs: nix-store --realise /nix/store/xxx.drv
│                                                     │
│ 5. Builder reports output_path + cache_pushed       │
│                                                ─────>│
└─────────────────────────────────────────────────────────────┘

WHAT THE BUILDER CAN ACCESS IN THIS MODE:
  ✅ CF server API (HTTPS)
  ✅ Nix binary caches (HTTPS, configured substituters)
  ❌ Git remotes
  ❌ Database
  ❌ Deployment credentials
  ❌ Other builders
```

### 4.3 SourceReEvaluateVerified + ServerBundledArchive Strategy

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│  SourceReEvaluateVerified + ServerBundledArchive                                 │
│  Use when: air-gapped builders, GovCloud, builders with no Git remote access     │
└──────────────────────────────────────────────────────────────────────────────────┘

Git Remote ──> CF Server ──────────────────────────────────────> Builder Host
               (server only)                                      (no Git access)
                   │                                                    │
                   │ clone/fetch top-level repo                         │
                   │ (using stored SSH key from DB)                     │
                   │                                                    │
                   │ generate tar.gz of bare mirror                     │
                   │ sha256sum                                           │
                   │ save to archives/jobs/<job_id>.tar.gz              │
                   │                                                    │
                   │ ─── job manifest (job_id, archive_url, sha256) ──>│
                   │                                                    │
                   │ <── GET /source-archive (auth'd, streaming) ───── │
                   │ ─── streaming tar.gz ─────────────────────────── >│
                   │                                                    │
                   │                                         verify sha256
                   │                                         extract to:
                   │                                  mirrors/server-bundled/
                   │                                    <job_id>/<mirror_id>.git
                   │                                         git worktree add
                   │                                         nix eval (local)
                   │                                         compare .drvPath
                   │                                         nix-store --realise
                   │                                         report complete
                   │ <── POST /complete (output_path, cache_pushed) ── │
                   │                                                    │

WHAT IS BUNDLED:     Top-level flake repository only
WHAT IS NOT BUNDLED: Locked flake inputs (nixpkgs, etc.)
IMPLICATION:         Builder needs substituter access for flake inputs
                     OR inputs must already be in builder's /nix/store
                     Private flake inputs (not nixpkgs) must be:
                       - publicly accessible, OR
                       - pre-seeded in the Nix store/cache, OR
                       - handled by a future full-input-closure mode
```

### 4.4 SourceReEvaluateVerified + LocalGitWorktree Strategy

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│  SourceReEvaluateVerified + LocalGitWorktree                                     │
│  Use when: builder is colocated with or has direct access to the Git remote      │
│  NOTE: Builder needs read access to the repository URL                           │
└──────────────────────────────────────────────────────────────────────────────────┘

Git Remote ──────────────────────────────────────────> Builder Host
(builder clones directly — needs network path to repo)      │
                                                            │
CF Server ──> job manifest (repo_url, commit_hash, ──────> │
              expected_drv_path)                            │
                                                            │
                                               git clone --bare <repo_url>
                                               (only if mirror doesn't exist)
                                               git fetch +refs/*:refs/*
                                               git worktree add --detach <commit>
                                               nix eval (local worktree)
                                               compare .drvPath to expected
                                               nix-store --realise
                                               report complete

WHAT THE BUILDER CAN ACCESS IN THIS MODE:
  ✅ CF server API (HTTPS)
  ✅ Git remote (builder needs network + credentials for the repo URL)
  ✅ Nix binary caches
  ❌ Database
  ❌ Deployment credentials
  ❌ Repositories NOT listed in the job manifest

NETWORK RULE IMPLICATION:
  Builders in this mode need outbound TCP/22 or TCP/443 to the Git remote.
  For GovCloud or classified networks, prefer ServerBundledArchive instead.
```

---

## 5. Authentication and Signing Protocol

### 5.1 Per-Request Ed25519 Signature

Every API request from builder to server is independently signed. There are no bearer tokens, no session cookies, no long-lived secrets exchanged at runtime.

```
Canonical payload = METHOD + "\n" + PATH + "\n" + TIMESTAMP + "\n" + BODY_BYTES

Example:
  "POST\n/api/v1/builders/550e8400.../next-job\n2026-07-11T14:30:00Z\n{...body...}"

Signature = Ed25519.sign(canonical_payload, builder_private_key)

Headers sent:
  X-Builder-ID:         550e8400-e29b-41d4-a716-446655440000
  X-Builder-Session-ID: <process-lifetime session UUID>
  X-Signature:          base64(signature)
  X-Timestamp:          2026-07-11T14:30:00Z
```

**Replay protection:** Timestamp must be within ±5 minutes of server time. Requests outside this window are rejected with 401.

**Session scoping:** `X-Builder-Session-ID` is assigned by the server at startup. Job ownership checks verify both builder ID and session ID, so a job claimed in session A cannot be completed in session B (prevents build-job hijacking if a builder restarts mid-job).

### 5.2 What the Builder Private Key Controls

The builder private key authorizes exactly:

| Permitted | Not Permitted |
|---|---|
| Poll for next job | Access the database |
| Download source archive for claimed job | Download source archive for another builder's job |
| Stream logs for claimed job | Access logs for other builders |
| Complete / fail owned job | Complete / fail jobs owned by other builders |
| Send heartbeat metrics | Read/write deployment policy |
| Download .drv archive for claimed job | Evaluate flakes |
| Publish closure to cache (server-side push) | Manage builders (admin only) |

**Job ownership is double-checked on every API call:** The server verifies `builder_id` + `builder_session_id` + `job.status == "building"` before serving source archives, drv archives, or accepting completion reports.

---

## 6. Data in Transit — What Crosses the Wire

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│              DATA CLASSIFICATION BY FLOW                                         │
├─────────────────────┬───────────────────────────────────────────────────────────┤
│  Flow               │  Data / Classification                                    │
├─────────────────────┼───────────────────────────────────────────────────────────┤
│  Builder → Server   │  Ed25519 signature (public key material, not secret)      │
│  (all requests)     │  Builder ID (UUID, not secret)                            │
│                     │  Session ID (process-lifetime, not a credential)          │
│                     │  Timestamp                                                │
│                     │  Request body (job poll: strategy list;                   │
│                     │    complete: store path, cache reference)                  │
├─────────────────────┼───────────────────────────────────────────────────────────┤
│  Builder → Server   │  Build log text (stdout/stderr of nix build)             │
│  (log streaming)    │  CPU/RAM metrics                                          │
│                     │  WebSocket or HTTP POST                                   │
├─────────────────────┼───────────────────────────────────────────────────────────┤
│  Server → Builder   │  Job manifest: job_id, derivation name, drv_path,        │
│  (next-job resp)    │    execution strategy, source identity (repo URL,         │
│                     │    commit hash, mirror_id), archive_url (relative path),  │
│                     │    archive_sha256, expected_drv_path                      │
│                     │  NOTE: No repository credentials. No DB passwords.        │
│                     │  NOTE: archive_url is a CF server path, not a Git URL.   │
├─────────────────────┼───────────────────────────────────────────────────────────┤
│  Server → Builder   │  Binary tar.gz of bare Git mirror (top-level repo only)  │
│  (source-archive)   │  Streamed per-job; per-chunk RAM on server is bounded    │
│                     │  Builder verifies sha256 before unpacking                 │
├─────────────────────┼───────────────────────────────────────────────────────────┤
│  Server → Builder   │  nix-store export binary format (.drv + input closure)   │
│  (drv archive)      │  Streamed per argv chunk; no full-closure server buffer   │
│                     │  Fallback path when cache substituter unavailable         │
├─────────────────────┼───────────────────────────────────────────────────────────┤
│  Server → Git       │  SSH private key (from DB, used by server only)          │
│  (mirror clone)     │  Applied via GIT_SSH_COMMAND env var to git process      │
│                     │  Never leaves server; never sent to builder               │
├─────────────────────┼───────────────────────────────────────────────────────────┤
│  Server → Cache     │  nix copy --to <attic/S3 endpoint>                       │
│  (push)             │  Attic token / AWS credentials (server-held)             │
│                     │  Called when builder reports publish-derivation-closure   │
│                     │  OR during server-side cache push worker                  │
├─────────────────────┼───────────────────────────────────────────────────────────┤
│  Builder → Cache    │  Standard Nix substituter pulls (narinfo / .nar)         │
│  (substituter pull) │  Cache URL + optional auth token (configured on builder) │
│                     │  Builder only pulls paths listed in job manifest          │
└─────────────────────┴───────────────────────────────────────────────────────────┘
```

---

## 7. Filesystem Layout on the Builder Host

```
/var/lib/crystal-forge/
├── builder-api.key              # Ed25519 private key (mode 600, owned by cf user)
├── builder-api.pub              # Corresponding public key (registered with server)
│
├── flake-mirrors/               # Bare Git mirrors (LocalGitWorktree mode only)
│   └── <mirror_id>.git/         # Shared across jobs for same repo
│       └── (bare git repo)
│
├── flake-mirrors/
│   └── server-bundled/          # Job-scoped mirrors (ServerBundledArchive mode)
│       └── <job_id>/            # Created per job, deleted on job cleanup
│           └── <mirror_id>.git/ # Extracted from server-provided tar.gz
│               └── (bare git repo)
│
├── flake-worktrees/             # Per-job git worktrees (both modes)
│   └── <mirror_id>/
│       └── <commit_hash>/
│           └── <job_id>/        # Detached worktree, deleted on job cleanup
│               └── (nix flake source tree)
│
└── source-archives/             # Temp location during ServerBundledArchive download
    └── source-archive-<job_id>.tar.gz.tmp   # Written then extracted, deleted
```

**Cleanup guarantees:**
- Per-job worktrees are removed via `git worktree remove --force` after the build completes or fails.
- Per-job server-bundled mirror dirs (`server-bundled/<job_id>/`) are deleted after worktree removal.
- Temp archive files are deleted immediately after extraction regardless of success or failure.
- Server-side job-scoped archives (`archives/jobs/<job_id>.tar.gz`) are deleted on job `complete` or `fail` via `cleanup_source_archive()`.

---

## 8. Firewall Rules Required per Strategy

### 8.1 ServerDerivation (Recommended Default)

```
# Builder host outbound rules
ALLOW TCP  <builder>  →  <cf_server>:443     # API polling, drv archive download
ALLOW TCP  <builder>  →  <cache_host>:443    # Nix substituter pulls (narinfo, NAR)
DENY  ALL  <builder>  →  <database>          # Builder has no DB access
DENY  ALL  <builder>  →  <git_remote>        # No Git access in this mode
DENY  ALL  <builder>  →  <other_builders>    # Builders don't talk to each other
DENY  ALL  <builder>  →  <managed_hosts>     # Builder never touches managed NixOS hosts
```

### 8.2 SourceReEvaluateVerified + ServerBundledArchive

Same as 8.1. The builder never contacts Git remotes in this mode.

```
ALLOW TCP  <builder>  →  <cf_server>:443     # API + source archive download
ALLOW TCP  <builder>  →  <cache_host>:443    # Nix substituter pulls
DENY  ALL  <builder>  →  <git_remote>        # Source arrives from CF server
DENY  ALL  <builder>  →  <database>
DENY  ALL  <builder>  →  <other_builders>
DENY  ALL  <builder>  →  <managed_hosts>
```

### 8.3 SourceReEvaluateVerified + LocalGitWorktree

```
ALLOW TCP  <builder>  →  <cf_server>:443     # API calls
ALLOW TCP  <builder>  →  <git_remote>:22     # SSH git clone/fetch  ← ADDITIONAL
# OR
ALLOW TCP  <builder>  →  <git_remote>:443    # HTTPS git clone/fetch
ALLOW TCP  <builder>  →  <cache_host>:443    # Nix substituter pulls
DENY  ALL  <builder>  →  <database>
DENY  ALL  <builder>  →  <other_builders>
DENY  ALL  <builder>  →  <managed_hosts>
```

**Note for security reviewers:** LocalGitWorktree requires the builder to hold or discover Git credentials for the repository URL embedded in the job manifest. For private repositories this means SSH keys or netrc on the builder. ServerBundledArchive eliminates this requirement.

### 8.4 CF Server Inbound Rules

```
# Server host inbound rules
ALLOW TCP  <builders>          →  <cf_server>:443   # Builder API
ALLOW TCP  <agents>            →  <cf_server>:443   # Agent heartbeat/state
ALLOW TCP  <admin_workstations> →  <cf_server>:443  # Web UI / admin API
ALLOW TCP  <git_webhooks>      →  <cf_server>:443   # Git push webhooks
DENY  ALL  EXTERNAL            →  <cf_server>:5432  # DB never exposed externally
```

---

## 9. Threat Model — What Builders Can and Cannot Do

### 9.1 Compromised Builder

If an attacker compromises a builder host and exfiltrates everything on it, they obtain:

| Obtained | Impact |
|---|---|
| `builder-api.key` | Can impersonate the builder: claim jobs, complete/fail them. **Cannot** access other builders' jobs, the database, or deployment credentials. |
| Nix build artifacts in `/nix/store` | Build outputs that were already pushed to the cache. |
| Job-scoped source archive (if present during extraction) | A tar.gz of the top-level flake repo at one specific commit, already public via Git. |
| Build log content | Text output from `nix build` of potentially sensitive derivations. |
| Nix store paths from job manifest | The `.drv` path and output path for the current job. |

**Not obtainable from a builder host:**
- PostgreSQL credentials or DB network access
- Git SSH keys for any repository
- OIDC client secrets
- Deployment credentials or authorized SSH keys for managed hosts
- Nix binary cache push credentials (server holds these)
- Attic / S3 credentials (server pushes; builder only pulls via configured substituters)
- Other builders' private keys
- The CF server's internal evaluation state

### 9.2 Malicious Job Claim

If an attacker injects a malicious job into the queue (requires compromising the CF server or admin credentials), the builder will:

1. Accept the job manifest.
2. Download the source archive URL listed in the manifest.
3. Evaluate `nix eval` against the source archive.
4. Compare the evaluated `.drvPath` against the server-provided `expected_drv_path`.

**Step 4 is the critical defense for `SourceReEvaluateVerified`.** A manipulated source that evaluates to a different `.drvPath` than the server recorded will cause a `derivation_mismatch` failure before any build starts. The build plan cannot be altered without invalidating the derivation identity check.

For `ServerDerivation`, the `.drv` itself arrives from the server. A malicious `.drv` injected at queue time would build and report whatever the injected derivation produces.

### 9.3 Source Archive Tampering

The builder verifies the server-provided `archive_sha256` against the downloaded archive before extraction. A man-in-the-middle or storage corruption that alters the archive will cause a `SourceFetch` failure. The SHA-256 is computed on the server at archive generation time and included in the job manifest, which is itself Ed25519-signed.

### 9.4 Replay Attacks

Each request is independently signed with a timestamp. Replaying a captured request after ±5 minutes is rejected. The server session ID further scopes job operations to the current builder process lifetime.

---

## 10. Pre-Build Failure Phases

When a build cannot proceed safely, the builder reports a specific failure phase rather than leaving the job in `building` state indefinitely.

| Phase | Trigger | Job State |
|---|---|---|
| `source_fetch` | Archive download failed, SHA-256 mismatch, tar extraction error, `git clone`/`fetch` failed | `failed` or retry |
| `source_input_availability` | Required source inputs not available | `failed` or retry |
| `evaluation` | `nix eval .drvPath` failed on builder (timeout, eval error) | `failed` or retry |
| `derivation_mismatch` | Builder-evaluated `.drvPath` ≠ server-expected `.drvPath` | `failed` (no retry — policy violation) |
| `path_materialization` | `.drv` not available locally after substituter pull and archive download | `failed` or retry |
| `build` | `nix-store --realise` failed | `failed` or retry |

`derivation_mismatch` is treated as a hard failure with no automatic retry because it indicates the build plan has diverged from the server-evaluated state, which is a security-relevant event that warrants human review.

---

## 11. Configuration Reference for Network-Constrained Environments

### 11.1 Maximum Isolation (GovCloud / Air-Gap Adjacent)

```toml
# /etc/crystal-forge/server.toml
[server]
remote_build_execution_strategy = "source_re_evaluate_verified"
source_delivery_mode             = "server_bundled_archive"
source_archive_root              = "/var/lib/crystal-forge/source-archives"

# /etc/crystal-forge/builder.toml
[builder]
supported_execution_strategies = ["server_derivation", "source_re_evaluate_verified"]
source_mirror_root              = "/var/lib/crystal-forge/flake-mirrors"
source_worktree_root            = "/var/lib/crystal-forge/flake-worktrees"
cleanup_source_worktrees        = true

# No git credentials on the builder. Server holds all repo credentials.
```

**Remaining builder network requirements:**
- Outbound HTTPS to CF server
- Outbound HTTPS to Nix binary caches (or air-gapped store path pre-seeded)

**Builder network requirements that are eliminated:**
- Any access to Git remotes
- Any database access
- Any access to OIDC providers

### 11.2 Colocated / Internal Deployment (Relaxed)

```toml
# /etc/crystal-forge/server.toml
[server]
remote_build_execution_strategy = "server_derivation"
# source_delivery_mode defaults to local_git_worktree (ignored for server_derivation)

# /etc/crystal-forge/builder.toml
[builder]
supported_execution_strategies = ["server_derivation"]
# No source mirror or worktree needed for server_derivation
```

---

## 12. Key Management Lifecycle

```
                     BUILDER KEY LIFECYCLE
                     
Admin                    CF Server                Builder Host
─────                    ─────────                ────────────
│                            │                         │
│  1. POST /api/v1/builders  │                         │
│  (create builder record)   │                         │
│ ──────────────────────────>│                         │
│  201 + {builder_id}        │                         │
│<───────────────────────────│                         │
│                            │                         │
│  (out of band: provision   │                         │
│   builder host)            │                         │
│                            │                         │
│                            │  2. cf-keygen runs on first start
│                            │     generates Ed25519 keypair
│                            │ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─>│
│                            │                         │  /var/lib/crystal-forge/
│                            │                         │  builder-api.key (mode 600)
│                            │                         │  builder-api.pub
│                            │                         │
│                            │  3. Builder reports pub key at stdout
│                            │ <─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│
│                            │                         │
│  4. Admin copies pub key   │                         │
│  PUT /api/v1/builders/:id/ │                         │
│     public-key             │                         │
│ ──────────────────────────>│                         │
│                            │                         │
│                            │  5. POST /resolve-id    │
│                            │<────────────────────────│ (bootstrap: derive UUID)
│                            │  POST /session          │
│                            │<────────────────────────│
│                            │  {builder_session_id}   │
│                            │─────────────────────────>│
│                            │                         │
│                            │    [normal operation]   │

KEY ROTATION:
  PUT /api/v1/builders/:id/public-key with new public key
  Old sessions automatically invalidated on next heartbeat verification
  Builder private key replaced on host (service restart required)
```

---

## 13. Logging and Auditability

| Event | Logged Where | Log Content |
|---|---|---|
| Builder registration | Server / DB | builder_id, public_key hash, admin user |
| Job claimed | Server / DB | job_id, builder_id, session_id, timestamp |
| Source archive generated | Server log | job_id, mirror_id, commit_hash, sha256 |
| Source archive downloaded | Server log | job_id, builder_id, HTTP response status |
| `.drvPath` mismatch | Server log | job_id, expected, actual |
| Build complete | Server / DB | job_id, output_path, cache_reference |
| Build failure | Server / DB | job_id, failure_phase, error message |
| Session ID mismatch | Server log (warn) | builder_id, presented session vs. stored |
| Timestamp replay | Server log | builder_id, timestamp diff |
| Source archive cleanup | Server log (debug) | job_id, archive_path |

---

## 14. Relationship to Other Crystal Forge Documentation

| Document | Covers |
|---|---|
| `multi-builder-api.md` | API reference: endpoints, request/response schemas, retry logic |
| `eval-build-deploy-flow.md` | End-to-end commit → eval → build → deploy flowchart |
| `architecture.md` | High-level component overview, queue notification system |
| `deployment-policies.md` | Policy evaluation (server-side), not builder-specific |
| `store-path-flow.md` | Nix store path lifecycle and cache push flow |
| `auth-session-security.md` | Human user session security (separate from builder key auth) |
| **`builder-security-architecture.md`** | **This document** — builder boundaries, trust model, firewall rules |
