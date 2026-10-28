# Optional Attic inventory adapter

`cf-attic-metrics` is a provider-side, opt-in service. Crystal Forge accesses only
its authenticated HTTPS aggregates, not the Attic database. No deployment or
database grants occur during installation.

## API and meaning

Send one `Authorization: Bearer <operator-token>` header with strict GET
`/v1/caches/<percentencoded-cache-name>/inventory`. A successful response contains
only these fields:

```json
{"schema_version":1,"cache":"team","cached_path_count":2,"logical_nar_bytes":20}
```

Both totals are unsigned 64-bit integers. The count includes objects referencing
NARs in state `V` in an active cache. Each object contributes its NAR size: two
paths sharing a 10-byte NAR count as two paths and 20 logical bytes. Empty active
caches return zero. Missing or deleted caches return 404 after authentication.
An unknown token scope returns 401 without querying PostgreSQL.

These are logical inventory totals, not physical storage, compressed size,
deduplicated usage, or a download guarantee. Attic's `V` state does not prove
that every chunk remains available. No paths, hashes, signing keys, chunks, or
inventory rows leave the adapter. Negative/NULL sizes and invalid or overflowing
aggregates fail with 503. Errors contain only static `bad_request`, `unauthorized`,
`not_found`, or `unavailable` JSON. The adapter suppresses all request/exception
logs, including URLs and headers.

Schema verified against Attic revision
[`12cbeca141f46e1ade76728bce8adc447f2166c6`](https://github.com/zhaofengli/attic/tree/12cbeca141f46e1ade76728bce8adc447f2166c6/server/src/database/entity):
`cache(id,name,deleted_at)`, `object(cache_id,nar_id)`, and
`nar(id,nar_size,state)`. `NarState::Valid` serializes as the one-character
database value `V`. The parameterized aggregate uses two LEFT JOINs and one
statement snapshot; there is no separate cache-existence query or chunk join.
Recheck schema compatibility before upgrading Attic.

## Operator setup

Use a dedicated login role in the **Attic** database, with no owner membership,
superuser, schema creation, signing-key access, path-column access, or CF database
access. Operators must review inherited and PUBLIC privileges first. This manual
example assumes a dedicated database with PUBLIC privileges already restricted;
the adapter never runs these statements against an owner database:

```sql
CREATE ROLE cf_attic_metrics LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE
  NOINHERIT NOREPLICATION NOBYPASSRLS;
GRANT CONNECT ON DATABASE atticd TO cf_attic_metrics;
GRANT USAGE ON SCHEMA public TO cf_attic_metrics;
GRANT SELECT (id, name, deleted_at) ON public.cache TO cf_attic_metrics;
GRANT SELECT (cache_id, nar_id) ON public.object TO cf_attic_metrics;
GRANT SELECT (id, nar_size, state) ON public.nar TO cf_attic_metrics;
ALTER ROLE cf_attic_metrics SET default_transaction_read_only = on;
```

Set the role password through a private operator session, not shell history or
Nix expressions. Verify the role cannot read `cache.keypair`, object paths,
unrelated tables/databases or mutate data. The isolated `attic-metrics` check
verifies native restricted-role privileges; unit tests alone do not prove them.

Create two root-owned mode-0600 runtime files, outside the Nix store:

- `/run/secrets/attic-metrics-db`: PostgreSQL URI for that restricted role using
  explicit TCP host `127.0.0.1`, port and database, with percent-encoded password.
  No trailing newline. Limit: 8192 bytes. Do not use owner credentials or rely on
  Unix peer authentication: `DynamicUser` cannot peer-authenticate as Attic.
- `/run/secrets/attic-metrics-tokens`: JSON object mapping exact cache names to
  separately minted operator tokens. Limit: 64 KiB. Example shape:
  `{"team":"<minted-token>"}` (the placeholder is not a valid token).

Mint a unique random token per cache with 32 through 1024 ASCII bytes. The accepted
token grammar is `[A-Za-z0-9_-]{32,1024}`. Dots, tildes, plus signs, slashes and
padding are rejected. Tokens are operator-minted adapter credentials, not Attic
JWTs. Do not reuse Attic signing credentials or a token between scopes.
Empty, short, oversized, duplicate, control-bearing and newline-bearing tokens
are rejected. The cache grammar matches CF's native Attic resolver:
`[A-Za-z0-9][A-Za-z0-9_+\-]{0,49}`. Names are case-sensitive; the first character
must be alphanumeric. For example, `Team+1` uses the encoded segment `Team%2B1`.
Credential files must be regular files owned by root or the service UID, with no
group/world permissions; symlinks are rejected. Restart after configuration or
credential changes.

```nix
services.crystal-forge.attic-metrics = {
  enable = true; # Disabled by default.
  databaseUriFile = "/run/secrets/attic-metrics-db";
  tokensFile = "/run/secrets/attic-metrics-tokens";
  port = 9121;
};
```

Snowfall discovers `packages/attic-metrics` as
`pkgs.crystal-forge.attic-metrics` and `modules/nixos/attic-metrics` as the optional
module. systemd loads the source files as read-only `LoadCredential` originals.
Before each start, `ExecStartPre` runs as the DynamicUser service owner and copies
both credentials with `install -m0600` into `/run/cf-attic-metrics`. systemd creates
that service-owned runtime directory with mode 0700 and permits its writes under
`ProtectSystem=strict`. The adapter reads only the private runtime copies. This
preserves the adapter's owner-only file checks when systemd credential originals
have additional permission bits.

These copies are local protected temporary files, not persistent configuration or
Nix store content. systemd removes the runtime directory when the service stops;
each restart loads and copies fresh credentials. Restart after changing either
source file. The read-only credential originals remain unchanged. No secrets are
embedded in Nix, argv or environment variables. The unit uses DynamicUser, 128 MiB memory,
16 tasks, restricted address families and loopback-only network access. It opens
no firewall ports. Its PostgreSQL connection must therefore use local TCP (or an
explicitly permitted local socket arrangement).

Standalone invocation:

```sh
cf-attic-metrics --database-uri-file /run/secrets/attic-metrics-db \
  --tokens-file /run/secrets/attic-metrics-tokens --listen 127.0.0.1 --port 9121
```

Only literal loopback IPs are accepted; default listen/port are `127.0.0.1:9121`.
An operator-owned private TLS reverse proxy must restrict CF access, preserve
exactly one Authorization header, and disable sensitive request/header/error
logging. Bearer authentication is still required behind that proxy. CF's client
must use an 8-second timeout. The adapter enforces one absolute 8-second deadline
for reading the request line and all headers. Each bounded `read1()` performs at
most one underlying raw read; the adapter checks the remaining wall-clock budget
before each read and before returning buffered records. Pending bytes count once
toward the shared 64-KiB request-line/header limit. The URI limit is 1024 bytes;
the Authorization value limit is 1031 bytes, including the `Bearer ` prefix.
At most four workers are reserved before thread creation; overload returns
static 503. After header parsing, response writes use an 8-second socket timeout.
The header deadline is not an 8-second whole-request processing deadline.
Each request opens one connection (2-second connect timeout), then one read-only
transaction (3-second statement timeout, 1-second lock timeout). There is no
background polling or cached snapshot. Socket timeout is not a total PostgreSQL
wall-clock deadline; local DNS/connection behavior must also remain bounded.

## Verification

The package check runs `python -m unittest -v test_attic_metrics` with packaged
psycopg. Tests use fake connections and ephemeral loopback listeners only. They
cover the exact SQL/transaction contract, empty/shared-NAR totals, unsafe numbers,
credential bounds/modes, cache token isolation, malformed URI/header/method
handling, static failures and pre-thread concurrency admission. Real buffered
socket-pair tests prove unfinished request lines and headers cannot extend the
absolute read budget by trickling bytes. Grammar tests cover encoded `Team+1`,
invalid leading characters, URL-safe token boundaries and rejected alphabets.

Build the package with `nix build --no-link .#attic-metrics`. Unit tests do not
establish production deployment or production-version compatibility. The
[native integration check](../../checks/attic-metrics/README.md) verifies pinned
Attic migrations and uploads, restricted-role queries, authenticated TLS, and CF
API totals in separate disposable database hosts. The authoritative Web UI
step 25 verifies populated metrics, shared Cards/Table values and tooltips,
explicit refresh, and bounded request counts.

## Crystal Forge configuration

Configure the adapter explicitly for one CF destination ID. Do not use an Attic
upload token. The token file on the CF host contains exactly the corresponding
operator token, with no trailing newline. CF requires an owner-only regular
runtime file owned by the CF service user. The adapter token mapping remains on
the provider host; its database URI never reaches CF.

For example, the CF NixOS server option is:

```nix
services.crystal-forge.server.attic_metrics_endpoints = [
  {
    destination_id = 42;
    url = "https://private-metrics.example/attic-metrics/";
    token_file = "/run/cf-private-credentials/campground-metrics";
  }
];
```

The equivalent generated TOML uses:

```toml
[[server.attic_metrics_endpoints]]
destination_id = 42
url = "https://private-metrics.example/attic-metrics/"
token_file = "/run/cf-private-credentials/campground-metrics"
```

CF appends `v1/caches/<encoded-native-cache-name>/inventory` to the base URL.
The current destination's `attic_cache_name`, not its display name, selects the
provider cache. HTTPS and hostname verification remain required. Private
addresses also require the existing `server.allow_private_cache_test_targets`
opt-in. CF pins validated addresses, disables redirects/proxies, and applies an
eight-second total observation deadline and a 64-KiB response bound. It sends
only the adapter token, never Attic upload credentials or browser headers.

Unconfigured Attic destinations remain unsupported without a network request.
Disabled caches do not probe. Only Admin can collect configured observations;
other viewers receive `requires_admin`. Missing, invalid, denied, or failed
observations have null totals, not zero. A verified empty cache reports zero.
Cards and Table label sizes **Logical NAR** and counts **cached Nix paths**.

Production enablement requires operator verification of the deployed Attic
revision/schema, restricted-role grants, protected files on both hosts, private
HTTPS routing and trust, and the actual CF destination ID. Restart both configured
services, then use **Refresh metrics**. This repository change does not install
or restart a service in production.
