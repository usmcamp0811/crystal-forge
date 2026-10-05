---
type: Security Model
title: "Builder request authentication and data in transit"
description: "Specifies the per-request Ed25519 signing protocol, replay protection, session scoping, the exact permissions of the builder private key, and the classification of every data flow between builder, server, Git, and cache."
tags:
  - crystal-forge
  - builder
  - authentication
  - ed25519
  - data-in-transit
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/builder-security-architecture.md at commit 3b23d36f"
    title: "Crystal Forge Builder Security Architecture"
---

# Builder request authentication and data in transit

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
| Download .drv manifest for claimed job (authorized path list) | Evaluate flakes |
| Download .drv archive (full or delta subset) for claimed job | Manage builders (admin only) |
| Publish closure to cache (server-side push) | Manage builders (admin only) |

**Job ownership is double-checked on every API call:** The server verifies `builder_id` + `builder_session_id` + `job.status == "building"` before serving source archives, drv archives, or accepting completion reports.

---

## 6. Data in Transit — What Crosses the Wire

| Flow | Data / Classification |
|---|---|
| **Builder → Server** *(all requests)* | Ed25519 signature (public key material, not secret), Builder ID (UUID, not secret), Session ID (process-lifetime, not a credential), Timestamp, Request body (job poll: strategy list; complete: store path, cache reference) |
| **Builder → Server** *(log streaming)* | Build log text (stdout/stderr of nix build), CPU/RAM metrics, WebSocket or HTTP POST |
| **Server → Builder** *(next-job response)* | Job manifest: job_id, derivation name, drv_path, execution strategy, source identity (repo URL, commit hash, mirror_id), archive_url (relative path), archive_sha256, expected_drv_path. **NOTE:** No repository credentials. No DB passwords. archive_url is a CF server path, not a Git URL. When remote builder-side cache push is enabled, this response may include narrowly scoped cache push config/credentials for the selected cache destination. |
| **Server → Builder** *(source-archive)* | Canonical uncompressed tracked-tree tar used by authoritative evaluation. The builder enforces the manifest size and SHA-256 before bounded safe extraction. |
| **Server → Builder** *(drv manifest)* | `GET /derivation-manifest` — JSON list of store paths (sorted, deduplicated requisite closure of the job's persisted drv_path). Server-computed, not builder-supplied. Used as authorization baseline for delta. |
| **Server → Builder** *(drv archive)* | nix-store export binary format (full OR delta subset). **PREFERRED:** `POST /derivation-archive` with JSON `{"paths": [missing...]}` — server validates each requested path against the authorized manifest; 403 if any path is NOT in the manifest. Streams nix-store --export for exactly the validated subset. **FALLBACK:** `GET /derivation-archive` — streams full recursive closure (for servers that do not support the delta protocol). Both paths are streamed per argv chunk; no full-closure server buffer. |
| **Server → Git** *(explicit mirror fetch)* | SSH private key (from DB, used by server only). Applied via `GIT_SSH_COMMAND` env var to the fetch process. The mirror stores no remote URL. The key never leaves the server and is never sent to a builder. |
| **Server → Cache** *(push)* | `nix copy --to <attic/S3 endpoint>`. Attic token / AWS credentials remain server-held for server-side cache push worker flows. Builders may separately receive short-scoped cache push credentials only for builder-side cache push jobs when trusted HTTPS forwarding is verified. |
| **Builder → Cache** *(optional push)* | `attic push` or equivalent cache push command using credential-bearing cache config from the signed next-job response. Only used when builder-side cache push is enabled and credential transport is explicitly allowed. |
| **Builder → Cache** *(substituter pull)* | Standard Nix substituter pulls (narinfo / .nar). Cache URL + optional auth token (configured on builder). Builder only pulls paths listed in job manifest. |

## Related concepts

* [Builder API: authentication and admin endpoints](../api/builder-api-authentication-and-admin-endpoints.md) - Documents the builder API signature authentication headers and replay window, and the admin endpoints that create, list, update, deactivate, re-key, assign environments to, and read metrics for builders.
* [Builder threat model](builder-threat-model.md) - Analyzes what an attacker obtains from a compromised builder, a malicious job claim, source archive tampering, and request replay, and which defenses (digest check, derivation_mismatch, timestamp window) apply.
* [Builder credential boundary, key management, and audit logging](builder-credential-boundary-key-management-and-audit-logging.md) - Summarizes builder authentication, authorization, network boundary, and credential boundary rules, the builder key lifecycle (cf-keygen, registration, rotation), and the audit log events that the server and builder record.
* [Builder trust boundaries and component definitions](../builders/builder-trust-boundaries-and-components.md) - Defines the purpose, trust levels, and component definitions (server, builder, agent) that bound what a Crystal Forge remote builder can reach, hold, and compromise; open it to approve or review builder network and credential exposure.
