# Access integration check (work in progress)

Snowfall discovers `default.nix` as
`checks.x86_64-linux.integration-access`. The normal check builds **every**
gate in the explicit `scenarios` attribute set. A failed scenario fails the
normal check. Registration is coordinator-owned; adding a script without
registering it is incomplete work.

Currently registered: `lifecycle-local-bootstrap`. It starts a fresh
controller VM with the production embedded-UI server and PostgreSQL, checks
startup migrations and local setup, registers the first administrator in
the browser, signs in, reloads, and reads persisted identity and session
state. Wrong-password rejection must not create a session. Five appended
owner-selected session cases prove missing/mismatched CSRF denial without
session lifecycle writes, valid 204 revocation of only the selected session,
its next GET returning 401 while the retained session returns 200, and an
independent browser signing out through the profile. Browser sign-out requires
logout 204, exact-row invalidation, `/login` after reload, protected GET 401,
and protected navigation returning to `/login`.

The historical scenario at `fc930335878279ac426510359c039dfd8961dc13`
failed at server startup. TASK-490.25 fixed shared URI serialization while
preserving the Unix socket; TASK-490.26 passes the selected local/OIDC mode
from the normal module to the production unit. Targeted checks and the normal
server package build pass. After rebase on `origin/dev` `5ae7feab`, the
latest focused Access gate passed ten cases at
`81d65be745cc9f9777d368900d01fe0f207cb052`; the normal owner aggregate
passed by reusing that run. Evidence:
`/nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`.
Actual receipts record 292 migrations and 22 Unix-socket clients. The socket
assertion filters `pg_stat_activity` by configured role/database and
`backend_type = 'client backend'`, requires count > 0, and checks every
`client_addr` is NULL and `client_port` is -1. The
[execution record](../../docs/knowledge/testing/integration-coverage-map.md#current-bootstrap-and-selected-session-proof)
lists exact admin/session receipts and uploaded scenario/browser results and
the signed-out screenshot. The browser result passed at `/login`.
Keep startup, migration, setup-status, browser, persistence, and rejected-login
assertions. Do not add an `AUTH_MODE` test override. The journal also records
TASK-492, a separate Postgres Jobs startup-order defect; it does not fail the
Access gate. TASK-491 resolves the Rust test race; see MR !336.

Failure evidence from tested commit
`fc930335878279ac426510359c039dfd8961dc13`:
`/nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`.
`result.json` records failure in `fresh-startup-applies-migrations`;
`journals/controller/generated-config-summary.txt` records the generated
database host and missing auth mode; `journals/controller/crystal-forge.journal`
records `both host and hostaddr are missing`.

## Commands

```bash
# Normal check: every registered scenario.
nix build .#checks.x86_64-linux.integration-access -L

# Focused development: same scenario gate as the normal check.
nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L

# Evidence only: inspect a failed scenario's artifacts. NOT a gate.
nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap.run -L

# Inspect explicit registration.
nix eval .#checks.x86_64-linux.integration-access.scenarios --apply builtins.attrNames
```

The scenario gate's `evidence` link points to its NixOS run output:
`result.json`, `meta.json`, browser artifacts, and journals. The `.run`
target preserves artifacts of an assertion failure, but a package failure
or driver timeout can fail before those files exist. Never use `.run` as a
replacement CI gate. Normal verification does not update visual baselines.

## Missing obligations

Access is not complete. The selected session/CSRF/sign-out proof is covered
under the owner's explicit selection. TASK-490.2 remains `In Progress`:
Operator/Viewer admin denials and unavailable UI states, environment-scope
restrictions, and closed-registration cases are unimplemented, with exact
expectations blocked under D1. OIDC, forms, preferences, notifications,
and shell behavior also remain in the
[coverage map](../../docs/knowledge/testing/integration-coverage-map.md).
In Operator/Viewer terms, the denial cases are unimplemented; only the exact
environment-scope and closed-registration expectations wait for decisions.
GitLab runs this owner automatically and requires it (see below). The legacy
integration checks are optional manual jobs, and that does not mark their
obligations covered.

The coordinator composes `scenarios/fragments/task-490.2-sessions.py` after
the five bootstrap cases using `pkgs.writeText` and `builtins.readFile` in
`scenarios/lifecycle-local-bootstrap.nix`; the matching `.cjs` is registered as
`session-sign-out`. This uses the same VM/database and real admin receipt,
with an independent browser context; it adds no DSL, gate, or role.

The first session attempt at `537186835956d7af48a0868289e16364625dd85c`
failed the last UI case due to the coach overlay and unhandled-promise test
mechanics. The passing correction closes the real coach control and uses
`Promise.all` for click/response coordination; assertions remain intact.
Keep runtime-token logs private; the corrected header stays in a VM file.
TASK-490.1 foundation is in `Review`; its documentation validation passed.
Documentation follow-ups do not require another VM run.

## GitLab CI

The `integration-access` job in `.gitlab-ci.yml` runs automatically on merge
requests and `main` and is required. It runs:

```bash
sh packages/ci/integration-owner-build.sh integration-access
```

The script builds the normal owner aggregate with
`sh packages/ci/public-cache-build.sh ".#checks.x86_64-linux.integration-access" -L --show-trace --builders '' --max-jobs 1 --cores 2`.
It never builds a `.run` output as the gate. A `.run` output can build
successfully while its `result.json` records a failed scenario.

Evidence goes to `integration-evidence/integration-access/`, uploaded with
`artifacts: when: always`:

- `nix-build.log` — the complete build output.
- `<scenario>/` — real files, not Nix store symlinks: `result.json`,
  `meta.json`, `browser/` (results, screenshots, console logs), and
  `journals/`.

After a successful build, the script copies `result/<scenario>/evidence/`. If
the build succeeded but evidence is missing, the job fails. After a failed
build, the `result` link does not exist. The script then resolves each
registered scenario's `.run.outPath` through evaluation and copies only outputs
that already exist. It never builds a `.run` derivation, because a crash or
timeout could run the VM again and replace the original attempt's evidence. It
always exits with the original build status. The scenario names come from the
owner's own `scenarios` attribute set; the script keeps no second list.

Run the same script locally from the repository root. It removes a stale
`result` link and writes `integration-evidence/`, which Git ignores.

Shared mechanics live under `lib/integration-test/`, exposed through
`lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }`.
Read the [maintenance contract](../../docs/knowledge/testing/integration-test-contract.md)
before changing expectations. Application and design sources are read-only
for this overhaul.
