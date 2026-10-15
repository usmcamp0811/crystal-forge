# Native Attic publication regression

Run from the task worktree:

```sh
sh packages/ci/public-cache-build.sh \
  .#checks.x86_64-linux.attic-publication -L --max-jobs 1 --cores 2 \
  --out-link /tmp/opencode/task470-attic-publication-result
```

This check uses one disposable NixOS VM. PostgreSQL stores the exact JWT-bearing
publication configuration. The native Attic client and server are pinned to
`12cbeca141f46e1ade76728bce8adc447f2166c6`. No host database, service, or home
directory participates.

The examples exercise the real builder wire-to-config mapping and builder/server
derivation publication functions. The fixture table stores private JSON
configuration; it does not reproduce the Crystal Forge schema, encryption,
destination eligibility resolver, queue worker, or authenticated HTTP dispatch.
The production callers select database authority from the dispatched destination
ID or resolved job's persisted database source. Token presence is not provenance.

Each attempt adds a distinct random store path and proves native inventory
absence before publication. Successful publication must expose native narinfo
and downloadable NAR bytes. Rejected publication must leave native inventory
absent. The matrix covers public/private write-scoped JWTs, read-only/invalid
JWTs, conflicting ambient endpoint/token, stale CLI config, qualified cache
names, concurrent caches with different write scopes, retries, and static legacy
ambient login compatibility. Anonymous and invalid-token public cache-config
success is explicitly not proof of write authorization.

Timeout/cancellation tests pause the real Attic server after the absence probe.
They inspect the actual native child's private XDG directory, file modes,
environment, and argv inside the VM. They require child termination/reaping and
protected config removal before resuming Attic and checking inventory. The
examples keep the Tokio runtime alive until cancelled-owner cleanup completes.
Normal success/nonzero attempts also require no remaining credential directory.
Stored rows and selected-operation stale CLI config must remain unchanged.

The native proxy observer records only Authorization presence, method, and
status. Tokens, config bodies, response bodies, and child output never enter
driver logs. HTTP loopback is the fixture's native transport; this check does
not establish remote HTTPS dispatch or TLS policy coverage.

## Test-first evidence

On production baseline `c3fd244b`, the first genuine regression run showed:

```text
stored_id=4701 driver_exit=0 native_inventory_present=True
stored_id=4702 driver_exit=0 native_inventory_present=True
AssertionError: selected read-only JWT incorrectly authorized a fresh native upload
RED_GATE_EXIT=1
```

The second path was uncached before the attempt. Ambient/stale credentials had
write permission; the selected PostgreSQL-stored JWT had only pull permission.
Production code remained unchanged until this failure was retained in
`/tmp/opencode/task470-attic-publication-red-c3fd244b-r5.log`.

## Upstream contract

- [`client/src/config.rs`](https://github.com/zhaofengli/attic/blob/12cbeca141f46e1ade76728bce8adc447f2166c6/client/src/config.rs)
  loads `$XDG_CONFIG_HOME/attic/config.toml`, accepts `token-file`, trims its
  contents, and resolves server-qualified cache references.
- [`client/src/command/push.rs`](https://github.com/zhaofengli/attic/blob/12cbeca141f46e1ade76728bce8adc447f2166c6/client/src/command/push.rs)
  resolves the configured server/cache, obtains native cache-config, plans
  missing paths, and uploads through the native API client.
- [`client/src/api/mod.rs`](https://github.com/zhaofengli/attic/blob/12cbeca141f46e1ade76728bce8adc447f2166c6/client/src/api/mod.rs)
  builds Bearer authentication from that server token and implements native
  `_api/v1/cache-config`, `get-missing-paths`, and `upload-path` operations.
- [`attic/src/api/v1/cache_config.rs`](https://github.com/zhaofengli/attic/blob/12cbeca141f46e1ade76728bce8adc447f2166c6/attic/src/api/v1/cache_config.rs)
  defines the native fixture cache-creation payload.

Selected production publication writes a private per-operation config with
`[servers.cf-selected]`, the existing URL resolver's canonical server URL, and
the stored token's protected file path. The push target is
`cf-selected:<actual-native-cache>`. No `attic login` or token argv is used.
Only the selected child loses ambient Attic variables; proxy, trust, signing,
and other inherited environment remains available. The detached process owner
handles native exit, nonzero status, deadline, and caller cancellation. Raw
Attic output is discarded because server error bodies can contain credentials.
Static publication retains its legacy ambient login path. Niks3 behavior,
publication identity/fingerprints, signing, completion, metrics, and UI are
outside this check's changes.

## CI and current verification status

`flake-check: [attic-publication]` is a blocking matrix job. It uses the existing
command-local public-cache wrapper with bounded downloads, source fallback, and
required signatures. This policy does not change native fixture authentication.

The retained test-first evidence above describes its recorded baseline run.
Final-source verification is pending the parent's new-head CI pipeline. The
expanded encrypted-row lifecycle and database-selected model driver belong to
the separate [Attic metrics check](../attic-metrics/README.md); their final-source
execution is also pending CI. Browser rendering and screenshots remain separate
authoritative Web UI evidence.

## Requisite Attic credential correction: pending CI

The database-selected Attic branch of
`push_derivation_requisites_to_cache_destination` now prepares each existing
chunk with `PreparedAtticPush::for_paths` and the same selected child owner used
by server-local publication. Signing still occurs before each chunk. Every
explicit path, including `.drv` roots, remains in its original order. The
upstream-filter default remains true and jobs remain clamped to at least one.
The branch does not add `--no-closure` or reduce the chunk to its first path.
Selection, chunk size, source protocols, evaluator policy, signing, and the
Niks3 branch are unchanged.

The old Attic branch waited for unbounded command output. The selected owner
uses the existing destination `push_timeout_seconds` setting, defaulting to
3600 seconds and clamped to at least one second. Private config is applied last
by the owner; generic environment setup cannot replace XDG or authentication
after preparation. Child output is discarded and errors contain only static
text or an exit status.

Added source assertions cover complete chunk arguments, `.drv` root retention,
duplicates, both upstream-filter settings, jobs, private config/token ownership,
invalid paths, and missing selected credentials without ambient fallback. The
native check now includes a separate bulk example that verifies its exact CLI
arguments before real public/private uploads of two fresh store paths and one
fresh native `.drv` root. All three paths must be absent before publication and
present only for a selected writer. This proves shared preparation and native
CLI behavior when executed; it does not invoke the private HTTP requisite
dispatch function or establish full signed job-flow coverage.

These bulk changes have only source-format and diff inspection evidence. No
compiler, unit test, Nix build, or VM gate was run after this correction under
the user's CI-only verification instruction. Earlier RED/GREEN results apply
to the preceding single-root implementation, not this final revision.
