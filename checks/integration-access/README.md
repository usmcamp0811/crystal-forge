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
state. Wrong-password rejection must not create a session.

The scenario executed but **failed** at server startup. It records the
redacted module-generated configuration and server/PostgreSQL journals in
the evidence output. The default PostgreSQL socket path is formatted as a
URI with no host, so the server exits before migrations or auth routes.
The generated configuration also omits `auth_mode` and the unit has no
`AUTH_MODE`; the local-auth request is not reached. Keep the startup and
local-mode assertions. Do not add an `AUTH_MODE` test override. Product
defects are tracked by TASK-490.25 and TASK-490.26. TASK-491 resolves only
the Rust package-check failure; see MR !336.

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

Access is not complete. Sessions/CSRF, role/environment restrictions,
OIDC, forms, preferences, notifications, and shell behavior remain in the
[coverage map](../../docs/knowledge/testing/integration-coverage-map.md).
Legacy checks and required CI selection remain unchanged.

Shared mechanics live under `lib/integration-test/`, exposed through
`lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }`.
Read the [maintenance contract](../../docs/knowledge/testing/integration-test-contract.md)
before changing expectations. Application and design sources are read-only
for this overhaul.
