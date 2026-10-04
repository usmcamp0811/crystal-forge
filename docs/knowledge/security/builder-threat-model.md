---
type: Security Model
title: "Builder threat model"
description: "Analyzes what an attacker obtains from a compromised builder, a malicious job claim, source archive tampering, and request replay, and which defenses (digest check, derivation_mismatch, timestamp window) apply."
tags:
  - crystal-forge
  - builder
  - threat-model
  - security
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/builder-security-architecture.md at commit 3b23d36f"
    title: "Crystal Forge Builder Security Architecture"
---

# Builder threat model

## 9. Threat Model — What Builders Can and Cannot Do

### 9.1 Compromised Builder

If an attacker compromises a builder host and exfiltrates everything on it, they obtain:

| Obtained | Impact |
|---|---|
| `builder-api.key` | Can impersonate the builder: claim jobs, complete/fail them. **Cannot** access other builders' jobs, the database, or deployment credentials. |
| Per-job cache push credentials (conditional) | Can push to the configured cache destination within the permissions granted by the cache token/key. Only present when builder-side cache push is enabled and trusted HTTPS forwarding is configured. |
| Nix build artifacts in `/nix/store` | Build outputs that were already pushed to the cache. |
| Temporary source artifact (during extraction) | The canonical tracked tree for one exact commit. It contains no `.git` directory or repository remote configuration. |
| Build log content | Text output from `nix build` of potentially sensitive derivations. |
| Nix store paths from job manifest | The `.drv` path and output path for the current job. |

**Not obtainable from a builder host:**
- PostgreSQL credentials or DB network access
- Git SSH keys for any repository
- OIDC client secrets
- Deployment credentials or authorized SSH keys for managed hosts
- Other builders' private keys
- The CF server's internal evaluation state

**Conditional cache credential boundary:** Builders do not receive database,
deploy-target, OIDC, or Git credentials. Remote builder-side cache push may use
narrowly scoped Attic tokens, S3 access/session keys, or Niks3 write tokens and
mTLS private keys from the signed next-job response. Sending private material
requires `server.trust_forwarded_builder_https = true`, an actual direct socket
peer matching `server.trusted_proxy_cidrs`, and exactly one `X-Forwarded-Proto`
header whose value bytes are `https`. Header names are case-insensitive; the
value is case-sensitive. Missing peers, empty or unmatched CIDRs, duplicate
headers, whitespace, and comma chains fail closed. `Forwarded: proto=https` and
`X-Forwarded-SSL: on` do not satisfy this gate.

The controlled HTTPS proxy must strip client forwarding assertions and overwrite
`X-Forwarded-Proto`. Protect the proxy-to-backend path and block untrusted direct
access. Allow only the observed backend-facing proxy IP, normally a `/32` or
`/128`; the public builder/client IP and `X-Forwarded-For` are not the peer used
by this check. Loopback CIDRs apply only to same-host loopback connections.
The server has an HTTP listener; an HTTPS client URL or signature alone does
not establish confidentiality.

The same transport gate protects agent private mTLS read credentials. With the
flag disabled or another check unmet, private builder material is withheld and
private agent cache/target delivery remains unclaimed. Public cache config
without credentials does not require this gate. Builder transport rejection
occurs after claim, records a transient cache-config dispatch failure, and
returns HTTP 404 without private material. See the
[operator upgrade procedure](../caches/niks3-cache.md#proxy-upgrade-repair-and-loaded-configuration)
for loaded-config verification and failed-job recovery.

### 9.2 Malicious Job Claim

If an attacker injects a malicious job into the queue (requires compromising the CF server or admin credentials), the builder will:

1. Accept the job manifest.
2. Download the source archive URL listed in the manifest.
3. Verify the commit, lock digest, canonical source NAR, and evaluator contract.
   The contract includes the probed Nix version, `builtins.currentSystem`,
   purity, lock mutation, IFD, and source materialization schema. The server
   rejects a mismatched polling capability before queue lookup or claim.
4. Evaluate the verified store source in pure mode.
5. Compare the evaluated `.drvPath` against the server-provided `expected_drv_path`.

**Steps 3 and 5 are the critical defenses for
`SourceReEvaluateVerified`.** Source or evaluator incompatibility fails before
evaluation or build. A different build plan causes `derivation_mismatch` before
any build starts.

For `ServerDerivation`, the `.drv` itself arrives from the server. A malicious `.drv` injected at queue time would build and report whatever the injected derivation produces.

### 9.3 Source Archive Tampering

The builder verifies the server-provided `archive_sha256` against the downloaded archive before extraction. A man-in-the-middle or storage corruption that alters the archive will cause a `SourceFetch` failure. The SHA-256 is computed on the server at archive generation time and included in the job manifest, which is itself Ed25519-signed.

### 9.4 Replay Attacks

Each request is independently signed with a timestamp. Replaying a captured request after ±5 minutes is rejected. The server session ID further scopes job operations to the current builder process lifetime.

## Related concepts

* [Builder trust boundaries and component definitions](../builders/builder-trust-boundaries-and-components.md) - Defines the purpose, trust levels, and component definitions (server, builder, agent) that bound what a Crystal Forge remote builder can reach, hold, and compromise; open it to approve or review builder network and credential exposure.
* [Verified-source evaluator contract (source_re_evaluate_verified)](../builders/verified-source-evaluator-contract.md) - Specifies the verified-source flow where the builder re-evaluates a canonical source archive and compares its .drvPath to the server value, including the evaluator fingerprint, next-job 409 reasons, and rolling-upgrade behavior.
* [Builder request authentication and data in transit](builder-request-authentication-and-data-in-transit.md) - Specifies the per-request Ed25519 signing protocol, replay protection, session scoping, the exact permissions of the builder private key, and the classification of every data flow between builder, server, Git, and cache.
* [Builder failure phases and retry strategy](../builders/builder-failure-phases-and-retry.md) - Lists the pre-build failure phases a builder reports (source_fetch through build), which of them retry or fail permanently, and the priority-weighting retry and max-retries rules for build jobs.
