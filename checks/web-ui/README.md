# Web UI Check

This is the authoritative pre-merge gate for the web UI. It boots a NixOS
VM running the **production embedded-UI server build**
(`cf-server-drv`, not the core build used by `integration` and `oidc-auth`),
the agent, the builder, a Git server, and native cache fixture VMs when
the cache workflow is selected. It drives the real web UI
through Playwright, against the manifest of workflows declared in
`coverage-manifest.json`.

Do not change the production server package binding
(`cfServer = pkgs.crystal-forge.default.cf-server-drv;`) to the core build.
This is the one check that proves the shipped server binary serves the
shipped production WASM bundle through a real browser; using the core build
here would silently remove that guarantee.

## What it verifies

- **Build verification** — served `index.html` references a JS loader, the
  loader is served, and the referenced packaged `.wasm` has a valid
  WebAssembly magic header, checked directly against the production build's
  output (`verifyWebUiAssets`).
- **Semantic assertions and screenshots per manifest step** — every step
  named in `tests/integration-test.js` must exist in
  `coverage-manifest.json`, and vice versa; the check fails outright
  (`fatal.json`) on manifest/test drift before any workflow runs.
- **A fixed list of critical workflows** (see `critical_tests` in
  `default.nix`) must be present in the results and must pass. This
  includes, among many others, login/registration, system management and
  TASK-435 agent key-rotation authorization/persistence workflows, flakes,
  builds, CVE triage, evaluations, POA&M lifecycle and bulk workflows, admin
  automatic-retry settings, evidence lifecycle, and the full policy/STIG
  authoring and mapping-round-trip family. Non-critical workflows in the
  manifest may fail without failing the check, but critical ones cannot.
- **Strict visual baselines** — manifest steps marked `strict` must match
  their approved baseline in `baselines/` within threshold, or the check
  fails; `advisory` steps are reported with diff images but never block.
- **OSCAL export validation (Phase 5)** — routes real compliance API data,
  opens the export modal in the real web UI, captures the browser-triggered
  download, and validates it against the vendored NIST OSCAL 1.1.2 AR/AP/SSP
  schemas. This exercises the actual production `build_oscal()` WASM code
  path, the file a user would actually download — a stronger guarantee than
  the fixture-only `oscal-export` check provides.
- **SARIF export validation (Phase 6)** — the same end-to-end pattern for
  SARIF 2.1.0, validated against the vendored OASIS Errata 01 schema with
  format checking and semantic checks (rule-ID resolution, host locations,
  waiver suppressions).
- **Design-parity visual comparison** — renders the tracked design example
  (`docs/design/CrystalForge`, vendored offline) and compares it against the
  real Dioxus captures. This is reported as a drift gauge and summary matrix
  and is explicitly non-blocking; it never fails the check on its own.

## Run it

```sh
nix build .#checks.x86_64-linux.web-ui --print-build-logs
```

To run a subset of workflows (much faster iteration):

```sh
CF_UI_TEST_STEPS="16-cves,16b-cves-severity-filter" \
  nix build --impure .#checks.x86_64-linux.web-ui --no-link -L
```

`--impure` is required whenever `CF_UI_TEST_STEPS` (or other environment
overrides consumed via `builtins.getEnv`) should actually take effect,
because `testSteps` defaults to reading `CF_UI_TEST_STEPS` from the
environment. Global timeout is 2400 seconds (40 minutes) for the full
manifest; `playwrightResultTimeout` (default 1800s) additionally bounds how
long the check waits for the Playwright process's own exit marker.

Useful environment variables (all require `--impure` to take effect):

- `CF_UI_TEST_STEPS` — comma-separated workflow names to run instead of the
  full manifest.
- `CF_UI_TEST_PROFILE` — defaults to `ci_fast`.
- `CF_UI_UPDATE_BASELINES=1` — export strict-baseline candidates instead of
  failing on visual mismatch (used by the manual
  `web-ui-baseline-candidates` CI job; review and approve candidates with
  `approve-baselines.sh` before committing them).
- `CF_WEB_UI_RUN_MEGA_PHASES=1` — also boots the Attic and S3 cache VMs and
  runs the legacy cache/builder pytest phases. Interactive/manual use only;
  this variable cannot cross the Nix build sandbox in a normal CI run.

## Native retained-credential cache workflow

`25-caches-modal-attic` owns the focused TASK-470 cache workflow. Selecting
that step, or running the full manifest, boots Attic and a dedicated
Garage/Niks3 VM. This selection is evaluated by Nix before sandbox startup;
it does not depend on the interactive mega-phase flag. Other focused steps
do not boot these VMs. The legacy mega-phase fixtures remain separate.

```sh
CF_UI_TEST_STEPS=25-caches-modal-attic \
  nix build --impure --no-link -L .#checks.x86_64-linux.web-ui
```

Infrastructure-only startup/authentication proof is also available as
`.#checks.x86_64-linux.web-ui.nativeCacheFixture`. It shares the same native
nodes and runtime driver. It does not satisfy the authoritative browser gate.

### Certificate-required Niks3 discovery

The additional production-shaped write-discovery fixture requires three
independent CA roles. Root **A** signs the API server. Root **B** signs the
separate presigned S3 server in the topology owner's full-push VM. Root **C**
issues write client identities. The **Server CA bundle** contains A+B; C is
not remote server trust. Discovery never contacts S3 and never proves that an
upload or write authorization succeeds.

The shared PKI producer is an integration dependency supplied as
`productionWritePki`. By default the check consumes
`makeNiks3TestCredentials { productionPki = true; }.productionWritePki`.
Its fixture file contract is:

```text
api-ca.crt, s3-ca.crt, client-ca.crt
api-server.crt, api-server.key
write-client.crt, write-client.key
replacement-write-client.crt, replacement-write-client.key
server-ca-bundle.pem                 # Exactly roots A+B
```

Both write identities use subject `CN=write`, separate keys, and client issuer
C. The API certificate supports `cache`, `localhost` and loopback TLS names.
The legacy single-CA fixture cannot substitute for this producer. API root A
must not already be trusted through the legacy CA, so the custom-server-trust
failure case remains discriminating.

Endpoints:

- `https://cache:5754/api/cache-config`: mandatory client mTLS at the ingress;
  native metadata comes from the pinned Niks3 1.6.0 server on loopback 5755.
  The proxy verifies the native TLS server and presents a valid C-issued client.
- `https://cache:5753`: public signed read metadata for the added stored-mTLS row.
- `https://cache:5756/api/cache-config`: observed anonymous token-mode discovery
  into the existing native server on 5751. The original token/private-read
  fixture keeps its 5751/5752 URLs.

The ingress allows GET only. Missing and wrong client certificates must fail.
The default, public-discovery and Basic-discovery native processes each have
their own PostgreSQL database. Each process owns its Goose migrations; parallel
startup cannot race initial sequence creation in another process's database.
Correct write mTLS with roots A+B must succeed. Root A alone can also complete
this API GET; that observation does not establish S3 trust. The native pins API
is absent. The check does not add a pins response or a GC authorization shortcut.
The UI must continue to report **Write authorization: Untested**.

The added disabled environment-scoped row is seeded only after the existing
late step-25 request. It is separate from the original seven and from the three
direct-SQL legacy Attic rows. Its read mode is public and has no read credentials
or token. Runtime synthetic write keys enter the browser only through the
mode-0600 fixture file and the nested Write credential modal. Filled private
keys are never captured. The production-write object and Discovery payloads
contain no Garage/S3 credentials; the separate existing S3 workflow is preserved.

Browser coverage uses real authenticated CF requests:

- Unsaved Add: `POST /api/v1/caches/niks3/discover` with server URL, write mode,
  write certificate/key and server CA bundle. Metadata populates the read URL
  and signing keys without creating a cache.
- Existing Edit: `POST /api/v1/caches/:id/niks3/discover` with the unwrapped
  Save-shaped update patch. A retained selection omits the write identity.
  False/absent configured GET hints must not prevent the server-authoritative
  request. Replacement drafts are tested without Save, then canceled.
- Negative calls reject missing identity, wrong client identity and wrong server
  trust. Token-mode discovery omits both Bearer and the stored read identity.

Every Discovery and Cancel has before/after private checkpoints covering all
seven original rows, all three legacy rows and the added mTLS row. Complete raw
ciphertext, configuration, timestamps and assignments must remain unchanged.
Shared observer logs record only method/path/status and client/Authorization
presence. Successful Discovery makes one native cache-config GET; Cancel makes
none. TLS may reject before an HTTP record exists. No Discovery may upload,
contact pins, forward Authorization or borrow a read client identity. The final
observer must account for every request through these checkpoints.

### Five-rail Niks3 Basic Add/Edit

The dedicated production UI has **Destination**, **Write / API**, **Read / Pull**,
**Trust**, and **Advanced** rails. The native workflow exercises every rail at
desktop width and 390 px. The original capture states and provider guards remain.

A separate native instance advertises `https://read-cache.test:5757`. Its write
ingress at `https://cache:5758` requires the C-issued client identity and API root
A. The read ingress uses a B-issued `read-cache.test` certificate and a runtime
Basic ACL. Root B is in the VM system trust bundle. Read requests stream native
Nix metadata through the proxy without redirects; the proxy does not forward
the Basic header to native storage. API and presigned-server trust remains the
explicit A+B **Server CA bundle**, independent from client issuer C.

`production_basic` in the mode-0600 runtime fixture contains write identities,
server trust, two real 32-byte signing keys, and independent Basic original and
replacement pairs. The username includes a space and passwords have legal
leading/trailing spaces. An untrimmed pair succeeds against the native ACL;
trimmed or wrong passwords fail. These values never enter a URL or log.

The Add workflow performs real unsaved mTLS Discovery, then checks that a second
Discovery cannot silently replace nonempty read URL or signing-key rows. Only
**Apply discovered metadata** changes conflicting public draft values. Basic
credentials are entered in the nested Read credential dialog; its password is
masked, the parent is inert, and closing restores focus. No filled private-key
dialog is captured. Outside the nested dialog, the UI shows lifecycle state only.

Trust retains multiple real native signing keys. Advanced defaults are 1 parallel
upload, 3 retries, a 3600-second timeout and required signatures. Scoped **Test
write API** sends no read identity and contacts only cache-config. **Test read
endpoint** sends no write transport and contacts only native `nix-cache-info`.
Both use real Admin API POSTs. The Add Save includes environment IDs atomically
and persists encrypted write private key plus both Basic fields. The added cache
is deliberately disabled during raw-state verification.

Edit refreshes the saved ID. GET exposes only the Basic configured flag, never
the username/password or private key. Retained Tests and Discovery omit secrets.
The workflow checks nested Cancel, valid/invalid replacement read Tests, Discard,
Use current credential, and whole-parent Cancel without Save. A real 302 read
response must fail after exactly one authenticated source GET; the redirect
target must receive no request. Strict checkpoints compare every raw row,
ciphertext, timestamp and assignment. A wrong-password native 401 is read-plane
failure, not a write result. No probe performs an upload or proves write permission.

### Real remote inventory observations

Production packaging remains Niks3 **1.6.0**. The web-ui fixture separately builds
remote **1.8.0** for `GET /api/cache-stats`, using the source/vendor hashes from
Nixpkgs commit `c27cdad491a991b11ed731760aa2ef8db0cb0410`. Its database is separate
from the 1.6 native fixture. The enabled metrics-only destinations are scoped to
a new empty environment so they cannot become destinations for seeded builds.
Its server-only API token is generated at runtime as 64 hexadecimal characters,
stored privately, and never supplied to Discovery, Test, metrics or the browser.

The 1.6 native endpoint returns 404. CF must return unavailable totals rather
than fabricated zero values or a claimed remote version. The real empty 1.8
database returns zero live tracked objects and reported logical bytes. CF labels
those bases explicitly; neither number is a Nix path count or physical disk usage.
Cards and details must show the same `0.00 GiB` reported logical size and
`0 objects` native count. The unavailable 1.6 detail must show neither zero.
Observations use selected write TLS, no Bearer/Basic/read credential, no upload,
no GC and no inventory enumeration. Provider logs permit only the stats GET.
Whole-row snapshots prove that observations do not modify credentials, usage,
scope or timestamps. All replies are `Cache-Control: no-store`.

The Crystal Forge VM trusts the public fixture CA through its system trust
store. The disposable server explicitly enables
`allow_private_cache_test_targets` for this workflow. Host verification,
certificate verification, pinned target checks and redirect rejection remain
enabled. TLS terminators live on the native backend VMs; the Garage TLS
terminator preserves the signed Host header. It does not simulate S3 replies.

`native-cache-fixture.py` first proves native authentication:

- A private Attic cache rejects anonymous metadata reads. Two distinct
  runtime-minted pull JWTs successfully read its native API v1 cache config.
  Authenticated server-root requests return 404 while the private cache-config
  API returns 200. This discriminator rejects the old generic-root Test.
  A separate runtime pull/push identity runs real CLI login, cache-info and push
  during fixture setup. The published tiny Nix output has readable narinfo
  before Test. The fixture advertises the HTTPS API endpoint so CLI delegation
  does not attempt HTTP on the TLS-only port. No setup JWT enters argv or logs.
- Two imported Garage key pairs successfully perform native SigV4
  `ListObjectsV2`. Anonymous requests and incorrect signatures fail. Bucket
  listing proves read permission, not write authorization or native uploads.
- Native Niks3 discovery advertises the exact private read endpoint. The
  read certificate succeeds; anonymous and write-subject reads fail.
- Public Nix HTTPS `nix-cache-info` succeeds without authentication.
- Native TLS Basic-auth metadata rejects anonymous and incorrect credentials
  and accepts a runtime-generated password against an nginx password ACL.
  Nix and Http destinations retain userinfo only in server-side stored URLs.

Playwright receives only a filepath in `CF_CACHE_CREDENTIAL_FIXTURE`:
`/run/cf-cache-credential-fixture.json`. The file has mode `0600`, lives in
the disposable browser VM, and is never copied to screenshots or derivation
outputs. Runtime JWTs are transferred through a private temporary driver
directory that is removed before the browser starts. Consumers MUST NOT
log the JSON or use credential values in assertion messages or screenshots.

Provider authentication warmup may occur before the browser starts. The seed
process does not create cache rows when registration or login completes.
At entry to the retained workflow in `25-caches-modal-attic`, after that step's
original shared Add and Niks3 security workflows, Playwright writes a mode-0600
nonsecret seed-request marker. Only then does the worker authenticate the
existing browser-owned admin and create seven disabled environment-scoped
destinations through the real API. Earlier workflows such as `21-caches` see
their original cache data; no cleanup or assertion exception hides native rows.
It atomically publishes their IDs with `seed_complete: true`. The browser
consumer must wait for that flag before it reads IDs. Registration remains
covered by the existing registration workflows.

Version 1 JSON contract (all values below are field descriptions):

```text
version: 1
seed_complete: true
seed_request_path
checkpoint:
  request_path, ack_path
attic:
  id, server_url, cache_name, read_url, public_key, token, replacement_token
  denied_token, missing_cache_name, missing_cache_token, setup_proof,
  browser_observer_baseline
s3:
  id, endpoint, region, bucket, access_key_id, secret_access_key,
  replacement_access_key_id, replacement_secret_access_key
nix:
  id, url
niks3:
  id, server_url, substituter_url, public_keys[], token, ca_cert,
  write_client_cert, write_client_key, read_client_cert, read_client_key
production_write_mtls:
  id, server_url, substituter_url, public_keys[], server_ca_bundle,
  write_client_cert, write_client_key,
  replacement_write_client_cert, replacement_write_client_key,
  wrong_client_cert, wrong_client_key, token_discovery_server_url,
  observer_baselines
production_basic:
  id_after_ui_save, server_url, substituter_url, public_keys[], server_ca_bundle,
  write_client_cert, write_client_key, replacement_write_client_cert,
  replacement_write_client_key, username, password, replacement_username,
  replacement_password, observer_baselines
metrics:
  environment_id
  v16, v18:
    id, server_url, native_status
nix_basic:
  id, url, authority_change_url
http_basic:
  id, url, authority_change_url
legacy_query:
  id, sanitized_url, query_parameter, query_value_local_only
legacy_attic:
  legacy_plain, legacy_encrypted, legacy_missing:
    id, name, storage, token_expected
```

Native endpoints are `https://atticCache:9443` (private cache
`web-ui-private`), `https://cache:9443` (Garage), `https://cache:5751`
(Niks3 write API), `https://cache:5752` (private Niks3 read), and
`https://cache:5753/nix-cache-info` (public Nix metadata). Attic destinations
persist `push_to: server_url` and a separate `attic_cache_name: cache_name`.
This applies to primary rows, direct-SQL legacy rows and replacement clones.
Test must request `/_api/v1/cache-config/<cache_name>` on that server. The
`read_url` field is infrastructure metadata, not a persisted Test target.
Niks3 uses the token
or write certificate independently from its read certificate.

Every browser Attic Test has a private before/after checkpoint. The native VM
shares an unbuffered access log with the verifier. The log records only method,
path, HTTP status and Authorization presence. Destination IDs correlate the
checkpoint with row identity; URLs carry no fixture query marker. Successful
Tests make exactly one authenticated cache-config GET. NULL credentials make
zero requests. Every Test delta must contain zero uploads, independently of
the earlier CLI setup publication. Complete raw database snapshots also remain
unchanged at these checkpoints. The total browser-phase request count must equal
the sum of its Test checkpoints. Cancel and Save cannot hide provider requests
between checkpoints.

A correctly signed JWT without private-cache permission and an invalid token
must yield structured `authentication` failures. A token with pull permission
for an explicitly nonexistent name must yield native `NoSuchCache` HTTP 404
and the structured `cache_not_found` stage. A hidden-cache 401 is not a
not-found assertion. Negative replacement and cache-name overrides must not
persist; a subsequent retained Test must still succeed. Success reports
`probe_kind: attic_cache_config`, `stage: complete`, `cache_access_valid: true`
and private `token_auth_valid: true`. `write_auth_valid` remains null and the
UI reports `Write authorization: Untested`.

After a successful browser run, an independent real Crystal Forge API
verification first requires a VM-private checkpoint taken before any Save.
All seven retained Tests and canceled drafts must leave their complete raw
destination rows and assignments byte-identical, including ciphertext and
timestamps. The browser runs the entire Test/Cancel phase first, then writes
only a phase name and destination IDs to a mode-0600 runtime request file.
An independent Python process compares private database hashes and atomically
acknowledges success. No row contents, hashes or URI credentials enter the
browser or the acknowledgment. Save cannot proceed before that acknowledgment.

The subsequent Basic/query Save phase may change only the explicitly requested
destination name and `updated_at`. Intentional Save may also renew assignment
`created_at`: the existing backend recreates current assignment rows when Save
resubmits environment IDs. The option-1 audit found no production ordering,
history or reconciliation dependency on that current-row creation timestamp.
The fixture keeps this existing persistence behavior.

Assignment membership must match the exact `(cache_destination_id,
environment_id)` pairs. Every other assignment column, including any future
column, remains part of the comparison. Every other raw destination field,
ciphertext and credential-bearing URI must match the seed. These exceptions
apply only to purposeful Save comparisons, including the secondary sanitized
URL round trips. Failed Save, Test/Cancel and retained Test after Save use the
complete raw snapshot with all timestamps. The latter compares against the new
saved baseline. A real invalid-environment Save must fail without changing that
complete baseline.

Ephemeral Attic/S3 replacement clones
have separate raw-row baseline, pre-Save and post-Save retained-Test checkpoints.
Their draft Tests cannot mutate any stored field before Save; their retained
Tests cannot mutate the saved replacement afterward.
The verifier then creates disabled, environment-scoped fixtures for Attic, S3,
Nix and both Niks3 write modes with private reads. Stored-ID Tests and
replacement draft Tests must return actual successful read-probe results.
Private PostgreSQL SHA-256 comparisons cover the complete raw destination
row (including encrypted credentials and all timestamps) and assignments.
The verifier compares snapshots after each Test. Replacement Test leaves
storage unchanged; Save rotates credentials; the next retained Test uses
the saved credentials and leaves storage unchanged. GET responses must
redact credentials. Only nonsecret result labels are retained in
`screenshots/native-cache-proof.json`; proof records are deleted through
the real API.

### Existing legacy Attic rows

Three separate disabled scoped rows are inserted directly through parameterized
SQL after the retained-workflow seed request. The current create API is not used
to manufacture these compatibility cases. Canonical Attic cache metadata comes
from the native private cache. Tokens are respectively legacy plaintext, an
independently constructed historical `enc:v1` envelope, and SQL NULL.

Historical encryption uses Python cryptography AES-256-GCM, SHA-256 of the
runtime server encryption key, a random 12-byte nonce, empty AAD, and standard
base64 `nonce.ciphertext-plus-tag`. The key is read privately from the fixture
server process. A distinct legacy pull JWT remains in a mode-0600 server-only
file and is never included in the browser fixture JSON or GET responses.

The browser projects only safe collection GET flags to false/absent to model
stale list snapshots. Edit must refresh the real ID metadata and show the
current configured credential. Defensive cases project one ID metadata GET
flag to false/absent: the UI must show unconfirmed state, permit the real stored-ID
Test, and omit `attic_token`. Neither case intercepts or fulfills a Test POST.
The SQL NULL case must return a safe real HTTP 400 without contacting Attic.

All three raw rows and assignments must match their pre-Save checkpoint exactly
after Test/Cancel. Unrelated name Save for the two valid legacy rows must retain
the exact plaintext or historical ciphertext, raw URI and scope; only the
approved metadata timestamp changes are allowed. A new complete raw baseline
then proves retained Test non-mutation. Native Attic observer logs record only
method/path/status/Authorization presence. Private ID-scoped checkpoints, not
query strings or header values, identify each legacy row. Both valid legacy
rows require authenticated HTTP 200; the missing-token case requires zero
provider requests. These proofs supplement the existing seven current-API rows
and replacement-clone guards without replacing them.

### Legacy URL decision: retained Basic and query refusal

The `nix_basic` and `http_basic` fixtures use
`https://cache:9444/basic/nix-cache-info`. A protected runtime file holds a
synthetic username/password and query token. Python constructs raw userinfo
and query URLs in memory before real API creation. The browser receives only
IDs and safe URLs for these fixtures. Password hashing uses openssl stdin;
neither the password nor the raw URL enters a driver command.

Stored-ID Basic Tests must authenticate successfully. GET and Test responses
must not expose URI credentials. GET returns the sanitized URL and
`http_basic_auth_configured` flag. Stored-ID Test omits `tested_url`; if that
optional field is present it must equal the exact sanitized fixture URL.
A Save round trip of the same sanitized URL must retain the raw server-side
userinfo, and the next stored-ID Test must still authenticate successfully.
A Test override changes the DNS authority to
`https://cache-alt:9444/authority/nix-cache-info`. The isolated VM resolves
`cache-alt` to the native cache node and trusts the fixture certificate SAN.
The native metadata route succeeds anonymously. Its observer must prove that
the old Basic Authorization header did not reach the changed authority.

`legacy_query` stores a runtime token under a recognized query parameter.
Its safe URL is
`https://cache:9444/legacy-query/nix-cache-info?fixture=legacy`. GET must
return that URL with `legacy_query_credentials_configured: true`. Stored-ID
Test must return HTTP 400 with a static explanation before any connection.
Opening, canceling and saving unrelated fields must preserve the raw stored
URI. Before any Save, whole-row comparison includes `name` and `updated_at`
for this fixture too. Only the later intentional Basic/query Save phase excludes
those two destination fields and assignment `created_at`; exact assignment
membership and every other raw field remain protected by the comparison.
A separate raw-URI hash proves credential retention.

Run the isolated policy regressions without a database or service:

```sh
nix develop -c python checks/web-ui/native-cache-fixture.py snapshot-policy-regressions
```

The regressions reject scope additions, removals, changed cache/environment
identities, changed or removed future assignment columns, ciphertext/URI changes
and Test/Cancel timestamp changes. Only intentional Save accepts the named
timestamp exceptions.

The native observer records only URI paths and `auth=present|absent`. It
does not record query strings or Authorization values. After the browser
workflow and before the independent API verifier, the driver requires zero
`/legacy-query/` requests and absent Authorization on every `/authority/`
request. This artifact covers the browser phase. Nonsecret evidence is retained in
`screenshots/native-http-observer-proof.json`.

This fixture preserves existing server-side URL Basic authentication. A
separate Basic replacement UI and a complete HTTP credential subsystem are
outside the accepted option-1 scope. No migration is required by the fixture.

This coverage does not replace the five-variant `niks3-cache` publication,
agent, CVE and proxy-claim gate. It does not claim the full native legacy
upload suite passed. Existing review exceptions remain unchanged.

## Out of scope

- OIDC authentication (`oidc-auth` check).
- Rust-only PostgreSQL regressions not reachable through the browser
  (`server-regressions` check).
- The Attic/S3/builder pytest phases are opt-in and skipped by default; they
  are legacy coverage retained for interactive use, not part of the default
  gate.
- Design-parity comparison is diagnostic only; it does not enforce visual
  match against the design example the way strict baselines do.

## CI

Part of the `.gitlab-ci.yml` `flake-check` matrix (`CHECK_NAME: web-ui`), so
it runs on every merge request and on `main`. Its screenshots are copied
into CI artifacts and posted as an MR comment by the separate
`web-ui-screenshots-mr-comment` job. `web-ui-baseline-candidates` is a
manual, allow-failure job that runs this same check with
`CF_UI_UPDATE_BASELINES=1` to produce reviewable baseline candidates.

## Related files

- `coverage-manifest.json` — the authoritative list of workflow steps; drift
  between this file and `tests/integration-test.js` fails the check before
  any workflow runs.
- `tests/integration-test.js` — the Playwright workflow implementations.
- `tests/oscal-export-test.js`, `tests/sarif-export-test.js` — the Phase 5
  and Phase 6 export validation drivers.
- `baselines/` — committed strict-baseline PNGs; see
  `baselines/README.md` and `approve-baselines.sh` for the approval
  workflow. Do not hand-edit this directory outside that workflow.
- `design-parity/` — the offline design-parity rendering and comparison
  harness.
- `design-fixtures.json` — shared fixture data referenced by the harness.
