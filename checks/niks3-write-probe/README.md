# Native Niks3 write-route proof

`niks3-write-probe` is an authoritative NixOS VM check for the non-mutating
Crystal Forge Test contract. The parent task owns blocking CI registration.
The production package remains pinned to Niks3 1.6.0. The second native server
uses the exact 1.8.0 `buildGoModule` source and vendor hashes already used by
`checks/web-ui/default.nix`; this check does not update a production pin.

## Native evidence

Both versions run real Go handlers, PostgreSQL and Garage S3. Native TLS uses
runtime-generated server CA A, S3 server CA B, client CA C and an unrelated issuer.
The distinct read listener also uses server CA B. Client trust includes A+B;
native client trust follows the deployed union A+B+C shape. Tokens, JWT-shaped
invalid tokens, signing keys, certificate private keys,
admin password and the CF encryption key are created inside disposable VMs.
Private files have mode `0600` and their directory has mode `0700`.
Systemd supplies private copies of only the S3 certificate and key to Nginx.
The S3 proxy binds the signed Host authority to `cache:3904`. The cache VM
explicitly overrides the imported 2048 MiB allocation with 3072 MiB because
both native Go servers share Garage and PostgreSQL.

The direct native preflight requires these exact status/media/body contracts:

1. Token-free `GET /api/cache-config` returns valid native public keys.
2. Authorized `GET /api/gc/status` returns the native no-GC `404` fingerprint.
3. Omitted credentials return exact native `401 Unauthorized`.
4. Authorized `POST /api/pending_closures` with exactly `{}` returns exact native
   `400 missing closure key`.
5. Omitted credentials return exact native `401 Unauthorized` on that POST.

No GC task is started. The optional completed-GC `200` branch is not exercised
by this check. In-memory HTTP metrics on 1.8 may change; domain tables may not.

The CF matrix uses the actual API, first-admin registration, session cookies,
CSRF and verified Admin role. Real API Create encrypts retained credentials.
Unsaved create-shaped and retained-ID update-shaped Tests exercise `write`,
`read` and `all`. Both write scopes require exactly one token-free metadata GET
before the four protected requests: authorized GC GET, omission GC GET,
authorized invalid POST and omission POST. Certificate-required omission controls
fail before HTTP and are absent from the HTTP observer. A minimal default-token
Write draft needs no configured read URL or signing keys. Successful writes require true boolean
`write_api_reachable`, `write_authn_valid`, `write_authorization_valid` and the
legacy `write_auth_valid` alias. API reachability comes only from validated native
HTTP-200 JSON metadata and its typed signing-key/read-URL fields. Native GC `401`
cannot establish that field. Authentication and authorization describe protected
handler access only. They do not prove S3 permissions, signing or publication.
The fixtures test pinned 1.6 and 1.8 contracts; responses do not identify a
version, and the check makes no guarantee about unknown versions.

The TLS observer forwards to the actual native TLS listener. It re-originates
only the independently verified fixture client identity using the corresponding
private fixture certificate. Native Go checks the TLS peer subject; no forwarded
subject/header can authorize the native listener. The observer records only route,
method, credential-presence booleans, allowlisted identity, exact-empty-object
boolean and status. A second listener requires a client certificate at TLS
handshake time. Its omission control fails before HTTP; CF must report
`write_route_verified_certificate_gate`. That result describes ingress session
acceptance, not native origin subject enforcement. Direct native wrong-subject
`401` evidence establishes the separate origin behavior.

## Failure and isolation assertions

- Wrong tokens preserve successful public discovery but fail protected auth and
  prohibit POST. Missing draft tokens in full `all` scope fail CF validation before
  provider contact. Independent token-free Discover still succeeds. Write-only
  missing-token Tests must report false authentication/authorization booleans and
  null API reachability without metadata, DNS or protected requests. Invalid
  Bearer headers fail the same local preflight in both write scopes, even with an
  unresolved target. The complete local native gate exercises these cases.
- Trusted wrong subjects reach native `401`; wrong issuers fail TLS. Invalid key
  material fails local validation. Explicit B-only write trust cannot borrow
  ambient A. After selected-identity TLS failure, the anonymous GC control may
  still reach exact native `401`; no positive HTTP identity or POST is permitted.
  Token writes send no client identity; mTLS writes send no Bearer. Explicit
  A-only read CA rejects B's read certificate without borrowing ambient B.
- Public, Basic and mTLS read drafts use distinct read URLs and identities.
  Read-only scope cannot report write evidence. Write-only scope cannot borrow
  read Basic or mTLS credentials. Unsaved mode changes use the effective update;
  changing modes without the required replacement cannot borrow inactive secrets.
  Unknown cache types fail validation before network work. In `all` scope, a
  separate read-trust failure cannot overwrite successful per-write evidence or
  its stable `write_probe_diagnostic_code`.
- Adversarial fixtures supply generic `400`, unknown `404`, `405`, proxy `403`,
  `302`, oversized bodies, delays, generic POST errors and unexpected POST success.
  Unknown GET fingerprints prohibit POST. Redirect targets receive no requests.
  These synthetic cases supplement real native positive evidence.
  Invalid metadata status or signing-key JSON remains independent of protected
  success. Shadow-proxy `400` cannot prove write authorization or publication.
- Before and after every good/bad Test and draft/retained Discover, full raw CF
  destinations, environment assignments and job queues compare byte-identically.
  Ciphertext, all configuration fields and all timestamps remain protected.
- Before and after the complete probe matrix, every native public PostgreSQL
  table and sequence is hashed. The complete S3 object inventory includes body
  hashes; multipart inventory must remain empty. Startup metadata objects are
  included in the baseline. No closure, NAR, object or multipart upload is created
  by an operator Test. The initial closure/object/upload tables must have zero
  rows; startup migration records and metadata objects remain in the comparison.
  Native 1.8 seeds one `object_stats` row. Its exact zero-object/zero-byte state
  is asserted at baseline and remains included in the full before/after hash.

Only after those snapshots match, a separate fixture phase uses the real packaged
Niks3 CLI to publish fresh paths through token and mTLS authentication to both
native versions. A signature-required Nix copy into a fresh rooted local store
unpacks the NAR and compares its contents. The outputs are input-addressed real
derivations, so content-addressed trust cannot bypass signature verification. A
fresh-root copy with an unrelated signing key must fail. Bad-token and read-subject
CLI pushes use fresh uncached paths and must fail with the paths still absent. This does not
replace the existing builder/job/publication gates.

The runtime-resolved BusyBox builder uses `builtins.storePath` to preserve its
Nix dependency context and sandbox executable closure. Native 1.8 publication
paths contain 11 MiB of zero padding. The uncompressed NAR exceeds the upstream
10 MiB simple-upload threshold, selecting the multipart protocol supported by
the pinned 1.6 CLI. Padding compresses to a small transfer and is compared after
signature-required unpack. This fixture does not claim that the 1.6 CLI can
consume 1.8 simple-upload responses for smaller paths.

## Credential-safe output and upstream limit

Sensitive API bodies, raw database records, cookies, keys and CLI output stay in
private guest memory/files. Entry-point failures report only phase and exception
type. Fixture assertions also print their static, credential-free description.
CF responses must omit private keys and secrets; public certificate fields from
Create remain valid API output but are never printed. CF journals must omit all
PEM, credentials, cookie values, Bearer material, provider prose markers and
upstream invalid-token preview fragments. Provider
journals are never exported. Native Niks3 AuthMiddleware logs invalid-token
previews upstream; this check preserves native authentication and does not claim
provider-wide secret-log cleanliness or suppress that behavior.
The cache guest disables journal forwarding to the test driver's serial console;
native journals remain in the guest. Child-command failures report only static
stage/category enums and exit codes. Child stdout and stderr remain private.

## Verification status

On 2026-10-13, the repaired working tree based on `269b58ad` passed derivation
evaluation and the complete native gate. Evaluation returned exit 0:

```sh
nix eval --raw .#checks.x86_64-linux.niks3-write-probe.drvPath
```

The successful runtime derivation was
`jzlh3a4wbyqbkaxd5kvizz1hk70zflqr-vm-test-run-niks3-write-probe.drv`.
The foreground build returned exit 0 in 74 seconds with one build job and two
cores. The existing wrapper validated public-cache substitution, source fallback
and required signatures for the exact `checks.x86_64-linux.niks3-write-probe`
attribute:

```sh
sh packages/ci/public-cache-build.sh \
  --out-link /tmp/opencode/task470-niks3-write-probe-repaired -L \
  .#checks.x86_64-linux.niks3-write-probe --max-jobs 1 --cores 2
```

The successful run observed 146 CF Tests, 16 Discover operations and 325 exact
raw-state comparisons. The separate publication phase passed four authorized
pushes, four signed copies, four wrong-signature denials and eight denied pushes.
Both test QEMU processes exited through normal driver cleanup. The private local
log is `/tmp/opencode/task470-niks3-write-probe-repaired-attempt14.log`.
This local working-tree result does not establish exact-head remote CI success.

The unwrapped authoritative command is:

```sh
nix build --no-link -L .#checks.x86_64-linux.niks3-write-probe
```

Retain the real exit status. Do not publish raw native journals or private guest
files as CI artifacts. This check does not change cache-node defaults, production
policy, backend/UI code, publishing logic or CI YAML.
