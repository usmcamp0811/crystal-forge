# Verification Guide

Verification must prove the active task's acceptance criteria while remaining proportional to risk. Run project commands through `nix develop`.

The examples below reflect common Crystal Forge targets. Confirm current flake attributes and package manifests before using them; repository definitions are authoritative.

## Baseline matrix

| Change | Expected verification |
| --- | --- |
| Rust formatting | `nix develop -c cargo fmt --manifest-path <manifest> -- --check` |
| Server/agent/builder logic | Targeted tests for the affected package or module, then the relevant Nix package build |
| Dioxus state or component logic | Web UI formatting and targeted tests, then `nix build .#web-ui` |
| User-visible UI | Relevant UI tests, the owning integration scenario, and screenshot (see "Integration owners" below) |
| API contract shared across components | Tests/builds for every affected producer and consumer |
| SQLx checked query | SQLx preparation plus affected server tests/build |
| Migration/schema | Migration against isolated dev DB, SQLx preparation, and affected integration tests |
| Nix package or module | Targeted Nix build/check; full flake check if impact is cross-cutting |
| CI/release behavior | Relevant local equivalent and `nix flake check --keep-going` |

## Common Crystal Forge commands

Use the subset applicable to the change:

```bash
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/default/Cargo.toml
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build .#packages.x86_64-linux.server --no-link
nix build .#packages.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.ui-screenshots --no-link
nix flake check --keep-going
```

Do not blindly run every command. Confirm the current attribute names with `nix flake show` or repository definitions when needed.

## Verification levels

### Targeted confidence

Use during implementation for isolated logic or a small component. Run formatting and the smallest relevant test selection.

For a supported browser workflow, start `run-ui-dev` once and use the existing
browser harness directly:

```bash
web-ui-test 12-systems
```

Pass more workflow names as separate arguments. The command keeps PostgreSQL,
the Crystal Forge server, and Dioxus running between invocations. If a workflow
requires VM-only infrastructure, the command rejects it and prints the
authoritative NixOS invocation.

`web-ui-test` is the fast development feedback loop. The NixOS Web UI check
remains the reproducible authoritative verification boundary. Host-side
success does not replace the relevant focused NixOS check before completion.

### Feature integration

Use when acceptance criteria cross server/database, UI/API, builder/server, or agent/server boundaries. Start repository services through the devshell's process-compose workflow and exercise the actual path.

### Full Nix integration

Run `nix flake check --keep-going` when:

- The task explicitly requires it.
- Flakes, NixOS modules, devshells, packaging, CI, or release behavior changed.
- Multiple component interfaces changed.
- Targeted verification cannot establish compatibility.
- The task is high risk and preparing for review.

If it is not run, state what was run instead and why that is sufficient. Never imply that the full check passed.

## Integration owners

The integration layer is defined by the
[integration test maintenance contract](../knowledge/testing/integration-test-contract.md).
Owners, scenarios, status, and legacy retirement are in the
[integration coverage map](../knowledge/testing/integration-coverage-map.md).

```bash
# One scenario (gate): fails unless every case passes.
nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L
# Evidence of the same run, including after a failure (never a gate).
nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap.run -L
# Every registered Access scenario gate.
nix build .#checks.x86_64-linux.integration-access -L --keep-going
```

Only Access is currently discoverable. Its coordinator-owned
`checks/integration-access/default.nix` explicitly registers a `scenarios`
attribute set. The normal check uses `pkgs.linkFarm` to build all registered
scenario gates. Meaningful WIP checks may be discoverable; there is no
completeness flag. A passing registered subset does not prove missing
obligations.

Implemented integration owners run automatically in GitLab. The
`integration-access` job is required on merge requests and `main`. It runs
`sh packages/ci/integration-owner-build.sh integration-access`, which builds the
normal owner aggregate (never a `.run` output) and exports evidence files to
`integration-evidence/integration-access/`, including after a failed build.
Retiring legacy integration checks remain available for optional manual
execution while replacements are constructed: `legacy-flake-check` for
`integration`, `oidc-auth`, `niks3-cache`, `niks3-write-probe`, `niks3-ui`,
`attic-metrics`, and `attic-publication`, and `web-ui-check`. Removing a legacy
check from automatic CI does not mark its obligations covered. Delete its
definition and fixtures after passing replacement coverage is recorded, or after
an explicit decision removes the underlying requirement. The
[coverage map](../knowledge/testing/integration-coverage-map.md#current-gitlab-selection)
lists the obligations that have no automatic integration coverage.

Shared helpers live in `lib/integration-test/`; native Snowfall checks call
`lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }`. An
implementation packet does not automatically receive a NixOS derivation.
Compatible case fragments use the existing Nix/Python/browser-script structure;
the coordinator composes and registers the scenario. The local scenario uses
`pkgs.writeText` plus `builtins.readFile` to append
`scenarios/fragments/task-490.2-sessions.py` after bootstrap and registers the
matching `.cjs` as `session-sign-out`; no new DSL, gate, or role is needed.
Each separately built
scenario gets a fresh VM/database; cases within one scenario may use real earlier
records through receipts. Future Delivery, Assurance, and Upgrade scenarios,
fixtures, and assets belong beneath `checks/integration-{delivery,assurance,upgrade}/`.
Do not create placeholder `default.nix` files or custom discovery.

The historical Access scenario at `fc930335878279ac426510359c039dfd8961dc13`
failed before migrations/HTTP. Separate TASK-490.25/.26 repairs are integrated.
After rebase on `origin/dev` `5ae7feab`, the latest focused Access scenario
passed ten cases at `81d65be745cc9f9777d368900d01fe0f207cb052`: the original
five bootstrap cases and five selected session/CSRF/sign-out cases. The exact
owner command `nix build .#checks.x86_64-linux.integration-access -L` passed
by reusing the focused run. Receipts record 292 migrations, 22 actual Unix-socket
clients, and exact retained/revoked/browser session identities with persisted
revocation and sign-out. This proves the registered subset; missing Access
obligations and three other owners remain uncovered. The same VM journal
records TASK-492, an independent
Postgres Jobs ordering defect that does not fail the Access gate. The
[coverage map](../knowledge/testing/integration-coverage-map.md#foundation-execution-record)
records the failures, current receipts, and uploaded reviewer artifacts.
TASK-490.1 foundation is in `Review`; its documentation validation passed.
TASK-490.2 remains `In Progress`: the Operator/Viewer cases are unimplemented,
and only the exact environment-scope and closed-registration cases wait for
accepted decisions. The same Access scenario passed ten cases through the CI job
script at `2c494ac4bf8cc7155899723ee69fcf879ac49248`, with the VM executed
newly. Validate this
documentation follow-up with `nix build .#checks.x86_64-linux.okf-knowledge -L`;
do not rerun a VM merely because documentation follows the tested source.

## UI evidence

For a user-visible UI change:

1. Update or add an assertion in the owning integration scenario.
2. Ensure the scenario navigates to and renders the changed state.
3. Capture the screenshot and any design comparison from that scenario.
4. Include it in the MR description or discussion with the tested state identified.

While the legacy `web-ui` check is still registered, also keep it passing for
the behaviors that the coverage map has not yet replaced. GitLab no longer runs
it automatically, so start the manual `web-ui-check` job or run the check
locally when your change touches those behaviors. The lightweight
`ui-screenshots` helper is useful for iteration but is not integration
evidence.

## Output handling

Large-output summarizers may help locate failures, but verification depends on the original command's exit status. If piping output, enable reliable pipeline status handling or capture the command status separately. Inspect raw output whenever a summary is ambiguous.

When a command fails, report:

- Exact command
- Exit status
- First actionable failure
- Whether it appears caused by the change, the environment, or an unrelated existing failure
- What remains unverified
