# Authoritative Attic inventory check

Run from the task worktree:

```sh
sh packages/ci/public-cache-build.sh --no-link -L --max-jobs 1 --cores 2 .#checks.x86_64-linux.attic-metrics
```

This focused NixOS check boots two disposable VMs. The Attic VM owns native
PostgreSQL, local chunk storage, native `atticd` migrations, and the packaged
`pkgs.crystal-forge.attic-metrics` adapter. The Crystal Forge VM owns a separate
PostgreSQL cluster and runs the packaged core server with local authentication.
No host database or long-running preview participates.

## Native fixture and accounting

Both Attic packages use the locked nixpkgs source revision
`12cbeca141f46e1ade76728bce8adc447f2166c6`. An evaluation assertion requires
schema review if that source changes. PostgreSQL comes from the same lock.
The test creates private empty, populated, secondary, shared, deleted, `Team+1`,
and minimum-token caches through the native API. Native CLI uploads publish two different equal-size NARs into
the populated cache and one into the secondary cache. A protected token file
supplies the CLI credential; JWTs and CLI output do not enter driver logs.

One VM-owned SQL insertion clones native object metadata with a unique path hash
and the original uploaded NAR foreign key. This models a second path sharing
one NAR without assuming the CLI supports cross-path DAG registration. The test
requires two distinct equal-size native NARs before cloning. Three populated
objects must contribute three sizes, while the secondary cache contributes one.
A separate shared cache receives two metadata clones pointing to the same
genuine uploaded NAR. Its two paths must contribute twice the native size
(120 + 120 = 240 bytes on the locked fixture), rather than one deduplicated size.
This discriminates per-object logical accounting from NAR deduplication and
`SUM(DISTINCT nar_size)`. Independent bound SQL row reads and a Python sum provide
expected totals; the expected-value calculation does not reuse adapter SQL.

The adapter returns exactly `schema_version`, `cache`, `cached_path_count`, and
`logical_nar_bytes`. Active empty caches return real zeros. Missing and deleted
caches return authenticated static 404 errors. A temporary negative native NAR
size must produce static 503, then the test restores the exact original size.
The provider unit suite asserts that the SQL join filters on `n.state = 'V'`.
That SQL-shape assertion does not execute pending/nonvalid NAR rows against
native PostgreSQL; this native fixture does not exercise those states.

## Security and API evidence

The check enables the actual optional Attic metrics NixOS module with the
packaged adapter. The module uses `DynamicUser`, root-owned mode-0600
`LoadCredential` source files, and `ExecStartPre` owner-only copies into its
mode-0700 `RuntimeDirectory`. The listener is `127.0.0.1:9121`.
Native Attic and PostgreSQL also listen only on loopback. The private TLS proxy
uses a verified fixture CA
and DNS SAN. Its firewall and nginx ACL admit only the CF VM and local fixture
requests. CF connection attempts to native HTTP, adapter and PostgreSQL ports
must fail. No plain HTTP adapter or database port is exposed between VMs.

The adapter database role receives only these column grants:

```sql
GRANT SELECT (id,name,deleted_at) ON cache TO cf_attic_metrics;
GRANT SELECT (cache_id,nar_id) ON object TO cf_attic_metrics;
GRANT SELECT (id,nar_size,state) ON nar TO cf_attic_metrics;
```

The test verifies every public table/column privilege, role flags, membership,
schema creation denial, unrelated-database connection denial, and read-only
session default. Real restricted connections must reject private signing-key
and path reads plus insert/update/delete. A read-only `EXPLAIN` under the role
proves permission resolution without exporting the plan or private path names.

Distinct per-cache adapter tokens are public synthetic fixture strings, written
only into protected runtime files. Missing/wrong credentials, wrong-cache tokens,
and forwarded-authorization fallback are rejected. CF upload ciphertext is
deliberately invalid to prove metrics do not decrypt or reuse upload credentials.
Temporarily revoking a required database grant makes valid adapter auth fail
with 503 while missing/wrong auth still returns 401 before a database query.
CF token files belong to its service user, mode 0600; a root-owned file must fail.

Native `Team+1` creation and provider requests use an encoded `Team%2B1` path
segment. Native CLI publication supplies its real object/NAR row. Its dedicated
1024-byte token exercises the complete 1031-byte Bearer value through the actual
module credential map, HTTP provider, and CF protected token file/API. A separate
published native cache uses a 32-byte token. Both tokens include `-` and `_`.
Independent SQL totals must match both genuine flows, including after module
restart. Missing/wrong case scope, 31/1025-byte values, and `.`, `+`, `~`, `=`, `/`
token characters must return static 401 while the required DB grant is revoked.
CF token files contain exact token bytes with no trailing newline. In-place
newline, alphabet, and length corruption must yield `invalid_configuration`
without a provider request, followed by exact byte restoration.

Real registered Admin and Viewer sessions exercise
`GET /api/v1/caches/:id/metrics`. Tests require exact totals, field/basis semantics,
successful timestamps, and `Cache-Control: no-store`. Viewer, disabled, unmapped,
unauthenticated, and below-Viewer cases must leave provider request counts
unchanged. Temporarily removing the selected credential proves Viewer denial
precedes file access. Explicit TLS fault routes exercise missing fields, wrong
cache identity, unsigned overflow, and access denial with null unknown values
and static reasons. Fault routes supplement the genuine provider path.

Complete raw destination/scope/job and native cache/object/NAR/chunk snapshots
remain byte-equivalent after observations and reversible corruption. The safe
proxy observer records only token/cookie/CSRF booleans and status. API field-set
assertions reject extra inventory fields. Runtime credentials, DSNs, native keys,
raw provider errors, query text, and private paths are never printed by the test.

This check establishes API and pinned provider behavior. The parent authoritative
browser workflow owns UI consistency and screenshots. Existing Niks3 statistics
checks and global CI membership are separate from this new gate.

## Module credential lifecycle

The test inspects the effective systemd credential/runtime configuration. It
reads only UID evidence from the main process status and requires a nonroot
effective UID. Both runtime credential files must be regular files, mode 0600,
owned by that effective UID. Their directory must be mode 0700 with the same
owner. The original files must remain root-owned and mode 0600 in a root-owned
mode-0700 directory. Byte comparisons remain private inside the guest.

Changing the root token source must not change the running adapter's
restart-scoped snapshot. A real module restart must load the replacement token,
deny the old token and removed scope, and authenticate a newly configured scope.
The new scope deliberately names a nonexistent native cache: authenticated 404
distinguishes successful scope loading from unknown-scope 401. The test restores
the original token bytes, restarts, and rechecks every native total and complete
native persistence snapshot. A final stop must remove the module runtime
directory and both copied credential files, while preserving the original source.
All readiness waits are bounded. No credential bytes or DSNs enter test output.
