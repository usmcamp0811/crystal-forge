---
type: Operator Guide
title: Niks3 Cache Operator Guide
description: Describes Niks3 write and read authentication, deployment gates, credential testing, and operator verification.
tags:
  - crystal-forge
  - caches
  - niks3
implementation_status: implemented
---

# Niks3 Cache Operator Guide

This guide describes TASK-470's implemented Niks3 support in
[MR !331](https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/331).
The UI reference is the
[Caches view design](../design/CrystalForge/components/CachesView.jsx).
The upstream protocol reference is [Niks3 v1.6.0](https://github.com/Mic92/niks3/tree/v1.6.0).

## Supported configuration

Create or edit a destination as an administrator in **Caches**, or through
`POST /api/caches` and `PUT /api/caches/:id`. Select `cache_type = "Niks3"`.

| Field | Meaning and requirement |
| --- | --- |
| `niks3_server_url` | HTTPS write API base URL used by `niks3 push`. |
| `push_to` | HTTPS Nix read/substituter URL. Despite its legacy name, this is not the Niks3 write URL. |
| `niks3_public_keys` | Nonempty list of trusted Nix signing public keys; supports multiple keys. |
| `niks3_write_auth_mode` | `token` or `mtls`, selected explicitly. |
| `niks3_auth_token` | Required for static-token writes. No write certificate, key, or custom CA is allowed in this mode. |
| `niks3_write_client_cert`, `niks3_write_client_key` | PEM contents required for mTLS writes. No write token is allowed in this mode. |
| `niks3_write_ca_cert` | Optional certificate-only PEM trust bundle for mTLS writes. |
| `niks3_read_auth_mode` | `none` for public reads or `mtls` for private reads, selected explicitly. |
| `niks3_read_client_cert`, `niks3_read_client_key` | PEM contents required for mTLS reads. Public mode rejects read credentials. |
| `niks3_read_ca_cert` | Optional certificate-only PEM trust bundle for mTLS reads. |
| `parallel_uploads` | Per-job upload cap, passed once as `--max-concurrent-uploads`; minimum one. Not `attic_jobs`. |
| `environment_ids` | Destination assignments. Empty assignments mean global availability. |

Write and read URLs can use different hosts or proxies. Configure each URL for
its own plane. URLs must not embed credentials, fragments, or caller-supplied
TLS/authentication override parameters. Credential fields contain values, not
filesystem paths. Unknown cache types and incomplete or mixed auth modes fail
closed.

Manual create, update, and discovery use the shared authoritative
`cf_protocol::cache::validate_nix_public_key` validator. Each key must have the
form `name:encoded-key`, with a nonempty name, no whitespace, and a standard
padded Base64 payload that decodes to exactly 32 bytes for an Ed25519 public key.
Format validation does not prove ownership of the corresponding private key.

Configuration, credentials, and `environment_ids` are saved in the same request
and database transaction. A failed assignment write rolls back creation or the
entire update; it does not leave a new global destination or partially changed
credentials. On update, omitted `environment_ids` preserves scope; an explicit
empty list makes the destination global.

**Current CA limitation:** custom CA fields are supported only in mTLS modes.
Static-token writes and public reads use the runtime's normal TLS trust store;
there is no CA-only destination mode.

Certificate and CA fields remain plaintext and can appear in API responses.
Every supplied certificate field must contain only valid X.509 `CERTIFICATE`
PEM blocks and ASCII whitespace. Combined certificate/private-key PEM, other
PEM labels, comments, and arbitrary text are rejected. This checks certificate
structure, not remote authorization or certificate/key pairing.

Niks3 owns its backend storage credentials and signing configuration. Builders
receive write configuration; agents receive read configuration only. Niks3
clients receive no S3 access keys, Garage secrets, or write signing private keys.
External credential providers and Bearer/OIDC **reads** are deferred. The supported
static write token is a Niks3 API bearer token; it does not add OIDC support.

### Caches form workflow

Selecting **Niks3** in the common Add form retains the entered name, read URL,
and selected environments. The Niks3 form has a header showing the destination
name, type, and draft status, plus a **Destination**, **Credentials**, and
**Environments** navigation rail. Destination fields show the read/substituter
URL before the write/API URL; credential settings show read authentication
before write authentication. Rail badges describe draft validation and scope,
not verified connectivity.

Discovery populates URLs and signing keys for review before saving. Editing
loads existing environment assignments before enabling Save. If assignments
cannot be loaded, close and reopen the form. Discovery, testing, and saving
freeze the submitted draft while the request runs. A failed save retains the
draft. Save sends configuration and scope together.

## Upgrade and confidential transport

1. Apply the normal server migrations, including `0299` (destination fields),
   `0300` (builder dispatch identity), and `0301` (local queue provenance), after
   the rebased `dev` migrations through `0298` and before the updated server uses
   these fields.
2. Upgrade remote builders before dispatching Niks3 jobs. The updated builder
   advertises `capabilities.niks3_cache = true` in its signed next-job poll.
   A builder without this capability receives HTTP 409 `unsupported_cache_type`
   before a Niks3 candidate is claimed. Legacy cache dispatch remains supported.
   The capability is an authenticated advertisement, not executable attestation
   or proof that write authentication works.
3. Upgrade agents before using Niks3 for deployment, including public reads.
   Updated agents advertise `capabilities.supports_niks3 = true` in the signed
   `/current-system` JSON body. The server reads capabilities only from
   `VerifiedAgentRequest.body` after authentication. An absent capabilities
   object or flag defaults to false; version metadata and headers cannot grant
   support. If the selected cache is Niks3, an incapable agent receives neither
   that cache nor `desired_target`. Heartbeat ingestion continues, and the
   pending deployment remains unclaimed and retryable.
4. Verify the HTTPS proxy boundary for both builder polling and agent heartbeats.

The server's current confidentiality gate requires all of the following:

- `server.trust_forwarded_builder_https = true`.
- `server.trusted_proxy_cidrs` includes the actual direct socket peer's address.
- Exactly one `X-Forwarded-Proto` header with the exact value `https`.

The HTTPS-terminating proxy must overwrite the header and protect its backend
connection. Restrict the allowlist to that proxy and prevent clients from reaching
the trusted backend path directly. The server checks the direct peer, not
`X-Forwarded-For`. The opt-in alone is insufficient. Missing peer information,
untrusted peers, duplicate headers, and protocol chains fail closed. Signed
requests establish identity, not confidentiality.

Builder secrets are withheld without verified transport. If the selected agent
cache requires private reads, unverified transport suppresses both its cache
settings and `desired_target` before the pending deployment is claimed. Omission
does not convert an mTLS cache to public reads. Unreadable selected configuration,
database or decryption failure, and missing Niks3 capability also suppress cache
and target delivery, preserving the pending deployment for retry. The updated
agent replaces its runtime cache list on each heartbeat, rejects unknown types,
and rejects static Niks3 deployment when server-provided read settings are absent.
Agents receive only enabled read URLs, signing keys, and read authentication.

Builder publication, agent reads, local publication, and CVE materialization
share canonical environment eligibility: use enabled destinations assigned to
the environment first; use enabled global destinations only when no enabled
assigned destination applies. Disabled assignments do not block global fallback.
The eligible set is ordered by name, then ID. Agent heartbeats deliver only its
first destination. A capability, transport, or read-configuration failure after
selection cannot substitute another destination. Local jobs and completed CVE
publication references retain their recorded destination identity and must still
satisfy this eligibility policy.

## Secret storage and rotation

The server encrypts Niks3 tokens and read/write private keys with AES-256-GCM
using `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY`, or `CRYSTAL_FORGE_SECRET_KEY` as the
fallback. New ciphertext uses the `enc:v1:` envelope and a random nonce. Legacy
plaintext remains readable for compatibility. Destination API responses omit
tokens and private keys and expose `niks3_write_token_configured`,
`niks3_write_mtls_configured`, and `niks3_read_mtls_configured` instead.

Updates merge and validate under a row lock. Omitted fields preserve existing
values. Changing auth mode clears the previous credential set before applying
replacement fields. Changing read mode to `none` clears read mTLS material.
Explicit `clear_niks3_auth_token`, `clear_niks3_write_client_key`,
`clear_niks3_read_client_key`, `clear_niks3_write_ca_cert`, and
`clear_niks3_read_ca_cert` flags support clearing. Replacing and clearing the same
field in one request is rejected. Required credentials cannot be cleared without
a valid replacement mode. A nonempty `niks3_public_keys` update replaces the list;
an omitted or empty list preserves it.

For signing-key rotation, distribute old and new public keys before changing
Niks3 signing keys. Retain trust in the old key while retained outputs still need
its signature, or republish those outputs with the new signature before removal.
For client credential rotation, authorize the replacement at the endpoint, update
the complete destination credential pair/token atomically, verify a real
publication and agent read, then revoke the old identity after in-flight work
has drained. Queued local attempts reload current credentials; already-dispatched
builders can still hold the previous credentials.

Keep the encryption key with protected backups. Changing the environment key
does not re-encrypt stored envelopes and makes old ciphertext unreadable. There
is no automatic multi-key rotation facility. Preserve the old key until all
encrypted credentials have been re-encrypted through a controlled procedure.

Runtime credentials use owner-only temporary directories (`0700`) and files
(`0600`). Arguments carry file paths, never token/private-key contents. Child
environments exclude ambient AWS, Attic, and Niks3 write credentials. Niks3 output
is suppressed because it can contain presigned upload URLs. Process owners retain
files through exit, cancellation, timeout, kill, and reap. Cleanup unlinks files;
it does not promise secure erasure. Avoid credential-bearing request-body logs.
Upstream Niks3 1.6.0 WARN logs can expose a prefix/suffix of a rejected token;
backend access logs can expose presigned URLs. Crystal Forge's output suppression
does not redact those independently operated services.

## Publication, failure, and recovery

Remote completion is signed and bound to the current builder/session, the
server-authoritative output, and the destination ID persisted at dispatch.
Niks3 requires `cache_destination_id`; a name or URL is not sufficient. Deleted,
disabled, ineligible, or mismatched destinations cannot be replaced implicitly.
Reference-only completion remains compatible for eligible legacy non-Niks3 caches.

Before recording reported Niks3 publication, the server probes the independent
read endpoint and imports the complete closure into a fresh temporary local
store. The import requires signatures, uses only configured trusted keys,
refreshes remote metadata, verifies NAR integrity, and checks that the requested
root materialized. `path-info` alone does not prove valid signatures, and an
already-present host-store path cannot satisfy the fresh-store check.

The total verification process budget is **300 seconds**. The builder completion
HTTP timeout is **360 seconds**, allowing cleanup and commit beyond verification.
This is separate from `push_timeout_seconds` (default **3600 seconds** per push
attempt). A push runs one Niks3 CLI process per job, which traverses the closure
and owns upload concurrency. Server-local cache-queue attempts share one execution
slot; multiple queue workers do not multiply that cap. Requisite publication from
the API is not covered by this queue execution slot.

Verification precedes the success transaction. The transaction rechecks output,
identity, eligibility, and publication configuration, including credentials,
signing keys, URLs, and environment assignments. A concurrent configuration
change can reject completion even after a successful probe. Verified publication,
build/derivation success, and CVE enqueue commit atomically. Probe failure,
timeout, cancellation, or a failed recheck does not record successful publication
and retains the recovery GC root. Repair the endpoint, trust, credentials, or
eligibility and retry through the current claim/recovery workflow. Matching
successful completion retries are idempotent; GC-root release after commit is
best-effort.

Local `cache_push_jobs` retain `cache_destination_id` and
`cache_destination_source` (`database`, `static`, or `legacy`). Database IDs
survive deletion without a foreign key. Each attempt rechecks environment,
enabled state, and current decrypted configuration, including after waiting for
the Niks3 slot. Missing IDs never fall back to a name, URL, or static credentials.
Existing queue records are not retargeted by new producers.

Historical rows remain `legacy`: a unique eligible database name/URL match is
pinned to an ID before publication. Ambiguous, missing, or NULL references fail
closed. Only explicitly `static` rows may use static configuration. An uncertain
historical static job needs administrator-established provenance before retry;
the migration cannot infer that intent. A recorded no-cache dispatch must not
be reinterpreted as permission to use a newly added database cache. Queue failures
retain attempt/backoff state and credential-safe diagnostics. Restore the selected
destination or resolve provenance explicitly rather than substituting another
destination.

Agent deployment pulls and CVE materialization use the same independent read
credentials and multiple trusted keys, with signatures enabled and child-local
CA configuration. Materializing an output does not prove a successful deployment
switch or vulnerability analysis.

## Discovery and connection testing

Admin `POST /api/caches/niks3/discover` (also under `/api/v1`) accepts
`{"server_url":"https://writes.example.org"}` and reads Niks3
`GET /api/cache-config`. Discovery is read-only and sends no write token.
Connection testing validates discovery and the configured read endpoint's
`nix-cache-info`, using separate write/read mTLS identities where selected.
The discovered read URL must match the configured URL before read credentials
are sent.

Structured results contain `server_reachable`, `discovery_valid`,
`read_endpoint_reachable`, `signing_keys_found`, and `write_auth_valid`.
**`write_auth_valid` is always null (untested).** An `ok` result proves the tested
discovery/read stages, not write authorization or signed closure publication.
Confirm writes with a real job.

Probes enforce the existing SSRF policy, pin validated DNS addresses, disable
ambient proxies and redirects, and retain TLS hostname verification. Private
targets require the explicit `server.allow_private_cache_test_targets` opt-in.
Do not interpret a blocked probe as a write-authentication failure.

## Local verification

Run from the repository root. These commands use the pinned flake and disposable
VM databases; they do not require starting host application services. Use
`path:.#` while required source files are untracked; Git-backed `.#` omits them.

```sh
# Focused Rust contracts; SQLx uses checked-in offline metadata.
nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml -p cf-protocol cache
nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml -p cf-config cache_credentials
nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml -p cf-builder niks3
nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml -p cf-agent niks3
nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib niks3

# Server and builder are PACKAGES, not checks.
nix build --no-link -L path:.#packages.x86_64-linux.server path:.#packages.x86_64-linux.builder

# Packaging contract, smaller fixture, then full remote-builder/agent gate.
nix build --no-link -L path:.#checks.x86_64-linux.builder-evaluator-packaging
nix build --no-link -L path:.#checks.x86_64-linux.niks3-cache.fixture
nix build --no-link -L path:.#checks.x86_64-linux.niks3-cache
```

Ignored SQLx integration tests are not run by the Rust commands above. Run those
only against a separately verified isolated test database. VM checks require KVM
access for the Nix builder. Packaging retains pinned **Niks3 1.6.0** and binds its
child Nix, the server, and the builder to evaluator **Nix 2.34.8** through
`pkgs.nix-eval-jobs.nix`; do not replace this with an unrelated Nix on `PATH`.

The [packaging gate](../../checks/builder-evaluator-packaging/README.md) probes
wrappers and service PATH. The [Niks3 VM gate](../../checks/niks3-cache/README.md)
exercises token/public, mTLS/public, token/private, mTLS/private, and token/public
read-proxy variants with real remote builds, signed completion, server verification,
agent pulls, environment denial, local CVE materialization, and secret/cleanup
audits. A fixture pass alone does not satisfy the full gate.

The VM gate seeds evaluation identities and queues; it does not test flake
evaluation or verified-source re-evaluation. Its no-op activation tests agent
pulls, **not a full NixOS generation switch or reboot**. Server-local CVE output
restoration is covered, **not successful NVD-backed vulnerability analysis** or
remote-scanner credential delivery. Expired/untrusted-CA client certificate cases,
an exhaustive captured-heartbeat secret audit, and corrupted-signature rejection
at the Crystal Forge completion endpoint are not part of this VM matrix. The
fixture separately tests untrusted signing-key rejection with fresh local stores.
