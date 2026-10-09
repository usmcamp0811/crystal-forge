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
[Caches view design](../../design/CrystalForge/components/CachesView.jsx).
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
| `niks3_read_auth_mode` | `none` (Public), `basic`, or `mtls`, selected independently of writes. |
| `niks3_read_basic_username`, `niks3_read_basic_password` | Complete Basic pair; management responses omit both. Passwords are encrypted at rest. |
| `niks3_read_client_cert`, `niks3_read_client_key` | PEM contents required for mTLS reads. Public mode rejects read credentials. |
| `niks3_read_ca_cert` | Optional certificate-only PEM trust bundle for mTLS reads. |
| `parallel_uploads` | Per-job upload cap, passed once as `--max-concurrent-uploads`; minimum one. Not `attic_jobs`. |
| `environment_ids` | Destination assignments. Empty assignments mean global availability. |
| `max_retries`, `push_timeout_seconds` | Defaults are 3 retries and 3600 seconds; retries may be zero. Parallel uploads default to 1. |

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

**Add cache** opens one shared form for S3-compatible, Attic, Nix HTTPS, and
Niks3. Selecting a type does not replace the dialog. Name and selected environments
remain common, while each type retains its URL, credential, signing, and compression
draft during switches. Nothing is saved by changing type.

Niks3 uses a dedicated five-section rail: **Destination**, **Write / API**,
**Read / Pull**, **Trust**, and **Advanced**, with scope selection in Destination.
Cards and details distinguish the two planes. Nested credential dialogs contain
local replacement drafts or Current configured choices, not a reusable credential
inventory. Stored secrets never seed inputs. The parent dialog is inert while a
nested dialog owns focus; closing restores focus. Rail badges describe draft
state, not connectivity or write authorization. Save commits configuration and
scope together; Discovery and plane Tests do not save drafts.

Discovery populates URLs and signing keys for review before saving. Editing
fetches the destination by ID before mounting the form, then loads its existing
environment assignments. Save waits for both destination and scope readiness.
If the destination fetch fails, use **Retry loading destination**; there is no
list-data fallback. If assignments cannot be loaded, close and reopen the form.
Discovery, testing, and saving
freeze the submitted draft while the request runs. A failed save retains the
draft. Save sends configuration and scope together.

## Upgrade and confidential transport

1. Apply the normal server migrations, including `0299` (destination fields),
   `0300` (builder dispatch identity), `0301` (local queue provenance), and additive
   `0302` (Basic read fields and auth-mode constraints), after
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

Basic delivery additionally requires signed
`capabilities.supports_niks3_basic_read = true`, based on a feature probe of the
actual agent Nix executable. Old or unsupported runtimes advertise false. Without
that capability, or without private-transport verification, cache and target are
withheld before claim. Public and mTLS preparation retain their existing paths.

The server's current confidentiality gate requires all of the following:

- `server.trust_forwarded_builder_https = true`.
- `server.trusted_proxy_cidrs` includes the actual direct socket peer's address.
- Exactly one `X-Forwarded-Proto` header with the exact value bytes `https`.

The controlled HTTPS-terminating proxy must strip client forwarding assertions,
overwrite the header, and protect its backend connection. Restrict the allowlist
to that proxy and prevent untrusted direct access to the backend. The server checks
the direct socket peer, not `X-Forwarded-For`. The opt-in alone is insufficient.
Missing peer information, empty or unmatched CIDRs, duplicate headers, and protocol
chains fail closed. Header names are case-insensitive. The value is case-sensitive:
`HTTPS`, `http`, whitespace, and `https,http` are rejected. `Forwarded: proto=https`
and `X-Forwarded-SSL: on` are not alternate assertions for this cache credential
gate. Signed requests establish identity, not confidentiality.

The gate covers existing Attic tokens and S3 access/session keys as well as Niks3
write tokens/private keys and agent private Basic or mTLS reads. Public cache config without
credentials does not require this gate. The default flag value `false` withholds
private material. The server listener uses HTTP and does not terminate native TLS.
An HTTPS client URL alone is insufficient; use the controlled TLS proxy boundary.

Builder secrets are withheld without verified transport. If the selected agent
cache requires private reads, unverified transport suppresses both its cache
settings and `desired_target` before the pending deployment is claimed. Omission
does not convert an mTLS cache to public reads. Unreadable selected configuration,
database or decryption failure, and missing Niks3 capability also suppress cache
and target delivery, preserving the pending deployment for retry. The updated
agent replaces its runtime cache list on each heartbeat, rejects unknown types,
and rejects static Niks3 deployment when server-provided read settings are absent.
Agents receive only enabled read URLs, signing keys, and read authentication.

### Proxy upgrade repair and loaded configuration

An upgrade can expose a previous flag-only configuration. For same-host Traefik
that connects to the Crystal Forge backend through loopback, configure both fields:

```nix
services.crystal-forge.server = {
  trust_forwarded_builder_https = true;
  trustedProxyCidrs = [ "127.0.0.1/32" "::1/128" ];
};
```

Use these CIDRs only when the observed direct backend peer is loopback. For a
remote or container proxy, use the actual observed backend-facing proxy IP as
`/32` for IPv4 or `/128` for IPv6. IPv4 CIDRs do not match IPv6 peers, including
IPv4-mapped IPv6 addresses. Do not trust the public client/builder address, the
proxy's public endpoint, or an entire container network. Do not use
`0.0.0.0/0`, `::/0`, or another broad CIDR to bypass the check. An allowlisted
peer must not relay an untrusted plaintext request with an HTTPS assertion.

On the HTTPS route, Traefik must overwrite `X-Forwarded-Proto` with one `https`
value, even when the client supplies spoofed forwarding headers. Configure the
header at the trusted TLS terminator; appending to a client value is insufficient.
Protect the backend path as well as the external HTTPS route.

For a manually managed TOML file, the equivalent same-host loopback settings are:

```toml
[server]
trust_forwarded_builder_https = true
trusted_proxy_cidrs = ["127.0.0.1/32", "::1/128"]
```

The Nix field `trustedProxyCidrs` maps to TOML `trusted_proxy_cidrs`.
`trust_forwarded_builder_https` retains its spelling. Verify the file used by the
running process, not only the Nix source or a generated store artifact. The module
normally generates `/var/lib/crystal-forge/config.toml` and exports its path as
`CRYSTAL_FORGE_CONFIG` in the service wrapper. Outside the module, the loader's
default is `/var/lib/crystal_forge/config.toml` (underscore). Environment settings
with the `CRYSTAL_FORGE__SERVER__` prefix can override TOML values. The loader does
not enable environment-list parsing; configure the CIDR array in TOML rather
than treating an environment string as a supported array override.

For an externally managed service, `CRYSTAL_FORGE_CONFIG` selects the manual
configuration file instead of the generated file. The module wrapper explicitly
exports its generated path; a service environment setting alone cannot override
that export. The current module exposes read-only `configPath`, not a writable
`configFile` option. If a deployment wrapper supplies a manual `configFile`, verify
that wrapper's selected path in the running process. Editing generated TOML is
not a durable module repair because the next generation can replace it.

Inspect only the service properties and process fields required for this check:

```sh
systemctl show crystal-forge-server.service --property=MainPID --property=ExecStart --property=FragmentPath
pid=$(systemctl show crystal-forge-server.service --property=MainPID --value)
sudo grep -z -E '^(CRYSTAL_FORGE_CONFIG|CRYSTAL_FORGE__SERVER__(TRUST_FORWARDED_BUILDER_HTTPS|TRUSTED_PROXY_CIDRS))=' "/proc/$pid/environ" | tr '\0' '\n'
```

Use a live, nonzero PID. From the reported `CRYSTAL_FORGE_CONFIG` path, inspect
only the two safe TOML keys and the section heading:

```sh
config_path=/var/lib/crystal-forge/config.toml
sudo grep -nE '^[[:space:]]*(\[server\]|trust_forwarded_builder_https[[:space:]]*=|trusted_proxy_cidrs[[:space:]]*=)' "$config_path"
```

Replace `config_path` with the observed path. This command assumes single-line
values as in the example; a multiline CIDR array needs a restricted TOML inspector
that outputs only these two `[server]` fields. Do not print the whole configuration,
service environment, environment file, or request headers. A file edit does not
prove that the running server loaded it. After applying the configuration through
the normal operator rollout, verify a new server PID and a real credential-bearing
builder claim through the HTTPS proxy.

The updated NixOS module fails evaluation when the Crystal Forge service and
server are enabled with `trust_forwarded_builder_https = true` and empty
`trustedProxyCidrs`. The assertion does not add CIDRs automatically or prove that
a nonempty list matches the proxy. Raw/non-Nix TOML has no Nix assertion. An empty
or unmatched list still fails the runtime credential gate and dispatch.

The credential-safe denial warning includes `direct_peer_ip`,
`trust_forwarded_builder_https`, `peer_cidr_match`, `x_forwarded_proto_count`, and
`exact_https`, alongside job, derivation, and builder IDs. `direct_peer_ip` is
absent when connection metadata is unavailable. A valid private dispatch requires
the controlled proxy IP, `true`, `true`, `1`, and `true`, respectively. The count
is the number of header values, not the number of comma-separated tokens.
These are decision facts, not raw header contents. Do not log tokens, private
keys, signed URLs, raw headers, or request bodies to diagnose this boundary.

Older documentation promised HTTP `426 Upgrade Required`. The current builder
gate runs after claim and records a transient `[dispatch:cache_config]` failure
before returning HTTP 404 (no work this poll). No credentials reach the builder.
A build can fail at zero seconds without executing Nix. Automatic retries use
the configured backoff and budget; exhausted or disabled retries need operator
requeue. Repair and verify the loaded proxy configuration first, then requeue
affected failed work. Agent private-read rejection instead withholds cache and
target before pending deployment claim, preserving retryable work.

Builder and local publication use canonical environment eligibility: enabled
assigned destinations precede global destinations, with stable name/ID ordering.
Local jobs and CVE materialization retain recorded publication identity rather
than reinterpret a deleted ID by its former name or URL.

Deployment reads start from completed publication evidence for the exact
authorized derivation and exact output path, not today's first configured cache.
The server resolves database provenance by durable destination ID. A rename
preserves identity; deletion never redirects evidence to a replacement cache.
Unambiguous legacy evidence is compatible, but static or unpublished settings
cannot supply deployment credentials.

Assigned-before-global precedence applies among enabled, currently scoped,
publication-backed candidates. Adding an unpublished assigned cache cannot
displace an existing proven source. Within that scope group, database evidence
precedes legacy evidence, then publication ID orders candidates. If a recorded
source is disabled, deleted, reassigned, unreadable, or fails its capability or
transport gate, another completed publication in the eligible scope group may
serve the target. Failed assigned gates cannot downgrade to global sources.

The server sends only the selected read source together with the desired target.
Without a usable publication, both fields are withheld and pending work remains
unclaimed and retryable. A cache name, URL, or completed row for a different
derivation or output path cannot satisfy this requirement.

Artifact deployability remains historical. Auto-latest and retained deployable
commits can remain selectable after source archival or cache changes. Actual
delivery rechecks current publication readability and policy authorization for
auto-latest, manual, pinned, rollback, and retained-generation targets. Historical
authorization without an exact resolvable publication does not permit delivery.

Read resolution and the pending-delivery claim share one SERIALIZABLE transaction
using the locked system's current environment. Shared publication, destination,
and assignment locks retain the selected configuration through commit; conflicting
writers and serialization failures cannot yield a claimed target with stale read
settings. No network probe occurs while those locks are held. Changes after
commit cannot revoke credentials already sent in an HTTP response; credential
rotation must overlap identities while in-flight operations drain.

## Secret storage and rotation

The server encrypts Niks3 tokens and read/write private keys with AES-256-GCM
using `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY`, or `CRYSTAL_FORGE_SECRET_KEY` as the
fallback. New ciphertext uses the `enc:v1:` envelope and a random nonce. Legacy
plaintext remains readable for compatibility. Destination API responses omit
tokens, private keys, and Basic usernames/passwords. Responses expose
`niks3_write_token_configured`, `niks3_write_mtls_configured`,
`niks3_read_mtls_configured`, and `niks3_read_basic_configured` instead.

Updates merge and validate under a row lock. Omitted fields preserve existing
values. Changing auth mode clears the previous credential set before applying
replacement fields. Changing read mode to `none` clears read mTLS material.
Read-mode changes clear inactive Basic/mTLS credentials. Basic updates require
a complete explicit pair; omitted pairs retain credentials only at the same
HTTPS host and effective port. An authority change requires a replacement pair
or a non-Basic mode, rather than forwarding retained credentials.
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

Server-owned Niks3 derivation-input uploads emit fixed-field `started` and
`completed` lifecycle events with an opaque operation UUID, child PID, reap
status, cleanup-attempt status, and a static outcome. Events contain no request
headers, protected filenames, credential-directory paths, or secret arguments.
Start precedes the first await and resource handoff. Dropping the HTTP request
caller detaches the owner; the upload continues under its existing deadline,
measured from child startup. Controlled completion follows child wait/reap and
prepared-credential drop. Failed reap cannot establish a terminal boundary.
`cleanup_attempted = true` records resource destruction, not guaranteed file
deletion: temporary-directory cleanup can fail, so the final filesystem audit
remains required. These observations do not change generic CVE cancellation or
guarantee that all detached tasks globally kill their children on cancellation.

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

Admin `POST /api/caches/niks3/discover` (also under `/api/v1`) accepts a URL-only
request or an unsaved write-mTLS certificate/key/optional-CA draft and reads
`GET /api/cache-config`. Token/public discovery sends no Bearer token. Edit uses
`POST /api/v1/caches/:id/niks3/discover` with the unwrapped Update shape; the
server decrypts and merges one unlocked snapshot without writes. Both forms
return public discovery metadata only, never read credentials, keys, or PEM.
Connection testing validates discovery and the configured read endpoint's
`nix-cache-info`, using separate write/read mTLS identities where selected.
The discovered read URL must match the configured URL before read credentials
are sent.

Structured results contain `server_reachable`, `discovery_valid`,
`read_endpoint_reachable`, `signing_keys_found`, and `write_auth_valid`.
**`write_auth_valid` is always null (untested).** An `ok` result proves the tested
discovery/read stages, not write authorization or signed closure publication.
Confirm writes with a real job.

Test accepts `probe_scope: "write" | "read" | "all"`, default `all`.
Write scope observes metadata/API transport only. Read scope checks the configured
read endpoint with its selected Public, Basic, or mTLS identity; all scope also
checks discovery consistency. Optional stage fields distinguish untested work.
Neither TLS acceptance nor metadata success proves write authorization on pinned
Niks3 1.6.0. Errors never echo provider bodies, credentials, or PEM.

### Basic read transport and CA trust

Basic reads use the opt-in `cf-netrc-authority` extension in packaged evaluator
Nix 2.34.8. Before transport, the native guard binds netrc credentials to the exact
HTTPS origin (host and effective port) and disables all redirects. Unset behavior
retains normal native transport. A probe of the actual selected runtime must
establish support before Basic preparation. Credentials live in owned `0700`
directories and `0600` netrc files; child-local Nix settings select the file and
authority. Passwords never enter URLs or argv. Artifact copies and fresh-store
publication verification retain signatures and independent signing-key trust.
Redirect-based read proxies cannot be used with this Basic guard; use the native
streaming endpoint. This does not alter existing mTLS preparation or combine
token and mTLS write modes.

Write CA bundles establish server trust for both the Niks3 API and presigned
HTTPS storage targets, not trust in the issuer of the client certificate. A
two-root deployment must include API root A and storage root B even when client
identity comes from root C. Niks3's Go custom root pool replaces system roots;
include public roots when required. The server discovery client instead adds
custom roots to system trust. Certificate-only PEM validation remains strict.
Garage credentials remain server-side; read access never establishes signature
validity or write permission.

### Response-only storage observations

`GET /api/v1/caches/:id/metrics` returns a scoped, volatile observation with
`Cache-Control: no-store`. Only Admin can decrypt an enabled Niks3 snapshot and
probe its write API using selected write TLS; no Bearer or read Basic identity is
sent. Disabled destinations and unsupported Attic/S3/Http/Nix totals do no provider
network work or bucket enumeration. Niks3 1.8's public `GET /api/cache-stats`
reports `objects` and `logical_bytes` from its singleton database aggregate;
packaged 1.6 lacks this endpoint. Absence does not identify a remote version.

Logical bytes sum known client-reported uncompressed sizes and omit unknown
legacy sizes. They are not physical usage, capacity, or Nix-path totals. Counts
are live tracked objects, including metadata, not paths or untracked bucket
objects. `path_count` is null. Missing/error values remain null and display
**Unavailable**, never fabricated zero. Tooltips explain the basis; `measured_at`
is local observation time, not an upstream timestamp. DNS, TLS and body reading
share an eight-second total deadline and a 64 KiB body bound, with pinned DNS,
no proxies, and no redirects. No observation changes database usage or credentials.
The UI loads at most three requests concurrently once per list load or explicit
refresh; cards, table and details share one snapshot without timer polling or
duplicate global totals. The native 1.8 stats fixture uses a separate database
and bucket; production packaging remains 1.6.

Probes enforce the existing SSRF policy, pin validated DNS addresses, disable
ambient proxies and redirects, and retain TLS hostname verification. Private
targets require the explicit `server.allow_private_cache_test_targets` opt-in.
Do not interpret a blocked probe as a write-authentication failure.

### Testing retained credentials in Edit

In **Caches**, Edit can use **Current configured credential** without retrieving
the stored secret. Attic and S3 replacements are local credential-dialog drafts;
Niks3 retains independent read and write credential choices. Test uses the draft,
but only Save persists it. Cancel discards the draft.

Existing Edit fetches fresh `GET /api/v1/caches/:id` metadata before mounting the
form. Loading exposes no editing or testing actions. Failure exposes **Retry
loading destination** and Cancel; Edit never falls back to the collection row.
Test and Save also wait for environment-scope readiness and any active operation
to finish. Fresh metadata is still a snapshot, not a lock on the destination.

For Existing Edit Test, configured flags control presentation only. A true flag
for the active type or Niks3 plane offers **Current configured credential**; the
local `__current__` selection marker is not a secret or an API credential.
False or absent metadata shows an unconfirmed stored-credential state but does
not block a stored-ID Test. The server resolves retained credentials by ID.
Save keeps its separate credential and destination validation; permitting Test
does not permit Save or establish that stored credentials are usable. Explicit
replacement drafts must be complete. The client rejects a selected blank token,
an incomplete S3 identity, or a partial mTLS certificate/key pair rather than
silently treating the replacement as retained material.

Admin `POST /api/v1/caches/:id/test-credentials` accepts the same unwrapped update
JSON as Save. An empty object tests the stored configuration. The server loads
and decrypts the destination by ID, then uses Save's shared merge and validation
in memory. Same-type omitted credential fields retain stored values; changing
type cannot borrow inactive credentials. Replacement fields, authentication-mode
changes, and explicit clears follow Save's rules. The probe does not write
configuration, credentials, assignments, timestamps, usage, or jobs. Its unlocked
snapshot can become stale; a successful Test does not guarantee a later Save.

Destination responses omit Attic tokens, S3 access IDs, secret access keys, and
session tokens, as well as Niks3 tokens and private keys. These response-only
flags describe configured material, not verified connectivity or authorization.
They do not grant permission to probe or replace server validation:

| Flag | Meaning |
| --- | --- |
| `attic_token_configured` | A nonempty token for the active Attic type. |
| `s3_credentials_configured` | A complete access ID and secret for the active S3 type. |
| `s3_session_token_configured` | A nonempty session token for the active S3 type. |
| `http_basic_auth_configured` | Stored URL userinfo for the active Http or Nix type. |
| `legacy_query_credentials_configured` | Recognized credential query parameters in a legacy URL. |

S3 Test signs a bounded, read-only `ListObjectsV2` request with the effective
explicit keys and optional session token, requesting at most one key. It does not
use ambient profiles or credentials. Success proves only the checked bucket-list
read access, not object reads, uploads, or write authorization. Attic bearer
authentication is sent only for Attic tests. All probes retain the target, TLS,
DNS-pinning, proxy, and redirect protections described above.

### Attic URL interpretation and named-cache Test

Attic stores `push_to` as its server URL and `attic_cache_name` separately. The
pure `cf_config::resolve_attic_urls` helper derives four endpoints in memory:

| Field | Purpose and path |
| --- | --- |
| `server_url` | Login server directory, ending in `/`, including an HTTP(S) proxy prefix. |
| `cache_url` | Nix read root: `<server prefix>/<cache>`. |
| `metadata_url` | Nix metadata: `<server prefix>/<cache>/nix-cache-info`. |
| `cache_config_url` | Read-only Attic API: `<server prefix>/_api/v1/cache-config/<cache>`. |

HTTP(S) server bases, matching cache roots with an optional trailing slash, and
matching full metadata URLs resolve to the same endpoints. Only the matching
final cache segment or cache/metadata suffix is removed; unrelated paths remain
proxy prefixes. Percent-encoded matching segments are compared as decoded ASCII.
HTTP remains HTTP for CLI and read consumers; Test still requires HTTPS under
its target policy. Legacy `attic://` selects HTTPS at the configured authority
and discards the entire path, including any apparent proxy prefix.

The configured name is authoritative. A legacy `remote:cache` reference needs
one colon and a nonempty remote; endpoints use only the actual cache component.
Native cache names contain 1–50 ASCII characters, start with an alphanumeric
character, and otherwise allow alphanumerics, `_`, `+`, and `-`. The helper
preserves queries, including empty queries, on all four URLs. It rejects URI
userinfo and fragments, including empty forms. Test's sensitive-query refusal
still runs before DNS; normalization does not grant permission to contact a
target or send credentials. Test never rewrites the persisted URL or name.

The same resolver supplies the server model's `read_config` for agent delivery,
the CVE read root, builder/server login endpoints, publication read helpers, and
the requisite-publication `ATTIC_SERVER_URL` export. Native CLI remote profiles
still require initialization outside that export. Existing ambient endpoint
precedence, remote-only login memoization, and already-configured acceptance
remain separate compatibility behavior; URL normalization does not establish
that a cached CLI profile holds the current request's credentials.

Attic Test sends one Bearer-authenticated GET to the named cache's
`cache_config_url`, not the generic server root. Bearer authentication is used
only for Attic. Success requires HTTP 200 with typed JSON containing a valid Nix
`public_key`, boolean `is_public`, `store_dir = "/nix/store"`, and i32 `priority`.
HTTP 204, HTML, arbitrary JSON, missing fields, invalid keys, and another store
directory cannot establish access. Metadata is bounded to 64 KiB. The existing
eight-second transport timeout, pinned DNS, verified TLS, no-proxy, and
no-redirect policy remain enforced; advertised URLs are not followed.

Results expose `probe_kind: "attic_cache_config"`, `stage`,
`cache_access_valid`, `token_auth_valid`, and `write_auth_valid`. Stages are
`target_policy`, `dns`, `transport`, `authentication`, `cache_not_found`,
`response`, and `complete`. Cache access is null before HTTP observation, false
after an unsuccessful response, and true only for validated metadata. A private
cache's successful response sets `token_auth_valid = true`; public-cache success
leaves token authentication untested (null). Write authorization is always
untested (null). HTTP 401/403 does not establish cache existence. Only HTTP 404
with typed `code: 404` and `error: "NoSuchCache"` reports cache absence; generic
404 reports `endpoint_unavailable` at the response stage. Policy refusal returns
safe HTTP 400 fields with null `details`; other probe outcomes use HTTP 200
result objects. Upstream bodies, URLs, tokens, and error text are never echoed.

A healthy Attic server can return 404 at `/` while authenticated named-cache
metadata returns 200. Conversely, root HTTP 200 does not prove cache access.
The old generic-root Test could therefore fail while a real CLI push worked:
publication and cache-specific read authorization use different endpoints.
The corrected Test proves read access only and performs no upload or persistent
mutation. Configured flags remain presentation metadata for Existing Edit Test;
retained credential decryption and merge remain server-authoritative.

Compatibility evidence is limited to the inspected pinned Attic `12cbeca…`
revision shared by the 25.11 and 26.05 native packages. Inspection of older
`ff8…` source is source evidence only, not a runtime pass or a guarantee for all
nightly versions. The current native fixture persists server-base URLs for
primary and direct-SQL legacy rows, separates real CLI setup publication from
Test, and requires root-404/API-200 discrimination with zero Test uploads and
unchanged raw rows. Final corrected runtime proof remains pending owner
verification; this contract does not establish a deployed cause or current
browser, backend, or CI pass.

### Legacy Attic verification scope

The selected PostgreSQL regressions insert legacy Attic columns directly,
bypassing the current create API. Plaintext and independently constructed
historical `enc:v1` rows must retain their exact raw credentials and URLs through
stored-ID Test and an unrelated same-type Save. Test/Cancel preserves complete
row and assignment snapshots; Save permits only the requested name and specified
metadata timestamp changes with unchanged assignment membership. Explicit blank
same-type Save fails without mutation. SQL NULL and empty tokens fail before
the probe callback. Synthetic Rust tokens verify retention, not native Attic
authentication.

The separate native browser fixture inserts plaintext, independently encrypted
historical `enc:v1`, and SQL NULL rows without the create API. Both valid rows
must authenticate against the real private Attic cache-config API. The NULL row
must return a safe HTTP 400 with zero provider requests. Stale collection flags must trigger
fresh ID loading; false or absent ID flags must permit stored-ID Test without
including `attic_token`. Private database checkpoints require exact Test/Cancel
non-mutation and credential retention across unrelated Save. See the
[native legacy-row workflow](../../../checks/web-ui/README.md#existing-legacy-attic-rows)
for fixture isolation and evidence limits. These fixture contracts do not
establish a cause or recovery for any deployed destination. Use safe destination
GET metadata and credential-free diagnostics to investigate an actual failure;
do not log stored tokens or ciphertext. This description does not establish a
browser or backend test pass for the current revision.

### Legacy URL credentials and migration

Legacy Http/Nix Basic credentials remain server-only. API URL fields are
sanitized; Http/Nix Test sends stored userinfo through a sensitive Basic
Authorization header. On a same-type unrelated Save, omitting the URL or sending
the same sanitized URL preserves the stored credential-bearing URI. An explicitly
different URL cannot inherit its Basic or query credentials. Changing type strips
URI credentials from inherited URLs. Basic credential replacement is unavailable
in the Caches form.

Recognized credential query parameters, including signed AWS query parameters,
are hidden in responses but retained in storage on same-type unrelated saves.
Test rejects these configurations with HTTP 400
`legacy_query_credentials_unsupported` before DNS resolution or any network
request. The operator must migrate to a credential-free URL and supported access
configuration before testing. Crystal Forge does not automatically migrate or
replay these query credentials; removing them from the displayed URL is not a
storage migration.

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

The [packaging gate](../../../checks/builder-evaluator-packaging/README.md) probes
wrappers and service PATH. The [Niks3 VM gate](../../../checks/niks3-cache/README.md)
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

The full Niks3 gate waits for all recorded fixture builds and scans to become
terminal, relevant cache-push jobs to finish without pending retries, and every
input-owner start to receive a post-reap, post-drop completion. Each agent copy
must reach its awaited read-owner completion and its activation unit's terminal
boundary. Independent inspection must then find no live Nix/Niks3 consuming
client; daemon identity, unknown inspection, and directory disappearance cannot
substitute for owner completion. Producers stop only after these boundaries.
The final assertion still checks directories named `cf-cache-*` under exactly
`/tmp`, `/var/lib/crystal-forge`, and `/var/lib/crystal-forge-agent`, at depth 3,
without exclusions. The controlled detached-upload race explains a possible
concurrent audit failure; it does not identify the leftover owner in an earlier
CI run without that run's evidence.
