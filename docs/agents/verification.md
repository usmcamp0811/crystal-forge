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
nix build .#checks.x86_64-linux.integration-access --no-link
nix build .#ui-screenshots --no-link
nix build .#okf-knowledge --no-link
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
requires retired VM-only infrastructure, the command rejects it and reports
that the legacy harness is retired.

`web-ui-test` is the retained development feedback loop, implemented under
`packages/devScripts/web-ui/`. Its supported host workflows do not replace
owning integration evidence. The legacy `web-ui` NixOS check is retired.

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

Access, Delivery and Assurance are currently discoverable. Each coordinator-owned
`checks/integration-<owner>/default.nix` explicitly registers a `scenarios`
attribute set and uses normal `pkgs.linkFarm` aggregation to build all registered
scenario gates. Meaningful WIP checks may be discoverable; there is no
completeness flag. A passing registered subset does not prove missing
obligations.

Implemented integration owners run automatically in GitLab. The three owner
jobs are required on merge requests and `main`. For example, Access runs
`sh packages/ci/integration-owner-build.sh integration-access`, which builds the
normal owner aggregate (never a `.run` output) and exports evidence files to
`integration-evidence/integration-access/`, including after a failed build.
TASK-490.24 authorizes retirement of the nine legacy suite directories before
replacement coverage is complete. `legacy-flake-check`, `web-ui-check`, and
`web-ui-baseline-candidates` are removed. One explicit `component-tests` job
replaces the six-entry package matrix. All fifteen public `verify-*` wrappers
are removed. Three process-compose database Rust jobs remain and share the
invocation-owned launcher. The state-machine workflow owns the consolidated
server regressions. The
[coverage map](../knowledge/testing/integration-coverage-map.md#current-gitlab-selection)
lists the obligations that have no automatic integration coverage.

Shared helpers live in `lib/integration-test/`; native Snowfall checks call
`lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }`. An
implementation packet does not automatically receive a NixOS derivation.
Compatible case fragments use the existing Nix/Python/browser-script structure;
the coordinator composes and registers the scenario. The local scenario uses
`pkgs.writeText` plus `builtins.readFile` to append
`scenarios/fragments/sessions.py` after bootstrap and registers the
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
TASK-490.2 remains `In Progress`: the selected Operator/Viewer admin-denial cases
are implemented, and only the exact environment-scope and closed-registration
cases wait for accepted decisions. The Access scenario previously passed all 16 cases
(ten earlier, six authorization) through the CI job script at
`8a857bc872cf331a19036b97a93f2b7741e9728b`, with the VM executed newly; see the
[authorization record](../knowledge/testing/integration-coverage-map.md#access-authorization-cases-task-4902).
Validate this
documentation follow-up with `nix build .#okf-knowledge -L`;
do not rerun a VM merely because documentation follows the tested source.

## UI evidence

For a user-visible UI change:

1. Update or add an assertion in the owning integration scenario.
2. Ensure the scenario navigates to and renders the changed state.
3. Capture the screenshot and any design comparison from that scenario.
4. Include it in the MR description or discussion with the tested state identified.

The legacy `web-ui` suite and baseline-candidate job are retired. Its historical
workflow list is not a mandatory replacement catalog. Accepted requirements
in the coverage map govern new owner scenarios. `ui-screenshots` remains a
package for design iteration, without semantic integration proof.

## Final inventory verification

Current inventory: `integration-access` and `integration-delivery`,
versus 27 before retirement. Access's original 16 cases remain unchanged; seven
Admin-form cases now follow them and passed at `c254b1c5` through the normal
owner CI script, with a new VM and independent SQL/reload evidence.
Assurance and Upgrade are absent; retained XML fixtures create no check.
Delivery's `delivery-lifecycle` registers real Git/native evaluation, API-only
builders and target-agent proof. Run its normal aggregate with
`sh packages/ci/integration-owner-build.sh integration-delivery`; exact tested
revisions and remaining obligations are in the coverage map.
Final completion requires exactly four TOTAL attributes, plus complete accepted
coverage and execution evidence. Evaluate the full set (not a filtered subset):

```bash
nix eval --json .#checks.x86_64-linux --apply builtins.attrNames
nix eval --json .#checks.x86_64-linux --apply 'c: builtins.attrNames c == [ "integration-access" "integration-assurance" "integration-delivery" "integration-upgrade" ]'
```

The final predicate must return `true`; a successful evaluation returning
`false` does not pass final acceptance. Consolidated lower-level verification
runs outside `checks`, as described below. Do not add a fifth check or replace
the removed wrappers with another registry or aggregation framework.

## Consolidated component verification

The explicit `component-tests` job in `.gitlab-ci.yml` runs these commands on
merge requests and `main`. Ordinary core tests own nonignored server library/bin
tests and all thirteen unchanged `cf-config` CA library tests. Core `postCheck`
owns real Nix inspection, observation, snapshot isolation, source parity and
Go/Rust CA dataset comparison. These checks do not repeat in the embedded build.

```bash
sh packages/ci/public-cache-build.sh .#default.cf-server-core-drv -L --show-trace --builders '' --max-jobs 1 --cores 2
nix build .#nixos-options-metadata .#oscal-fixture .#okf-knowledge .#xccdf-1-2-schemas -L --show-trace --builders '' --max-jobs 1 --cores 2

for suite in builder-cve stig packaging; do
  nix eval --impure --json --expr "
    let
      f = builtins.getFlake (\"git+file://\" + toString ./.);
      p = import f.inputs.nixpkgs {
        system = builtins.currentSystem;
        overlays = [ f.overlays.default ];
      };
    in import ./modules/nixos/crystal-forge/tests/$suite.nix {
      inputs = f.inputs // { self = f; };
      pkgs = p;
      lib = p.lib // { crystal-forge = f.lib; };
    }"
done

nix develop -c bash modules/nixos/crystal-forge/tests/discovery.sh
nix shell --impure --expr '
  let
    f = builtins.getFlake ("git+file://" + toString ./.);
    p = import f.inputs.nixpkgs { system = builtins.currentSystem; };
  in [ p.remarshal p.jq p.libxml2 p.openscap ]
' --command bash -euo pipefail -c '
  bash modules/nixos/crystal-forge/tests/packaging.sh
  core=$(nix eval --raw .#default.cf-server-core-drv.outPath)
  xccdf_schemas=$(nix eval --raw .#xccdf-1-2-schemas.outPath)
  bash packages/default/crates/cf-server/tests/xccdf-schema.sh \
    "$core" "$xccdf_schemas" "$PWD/schemas/cf-xccdf-1/cf-xccdf-1.xsd"
'

nix develop -c env -u DB_HOST -u DB_PORT -u DB_USER -u DB_NAME bash -euo pipefail -c '
  bash packages/devScripts/db-usability-check-test.sh packages/devScripts/db-usability-check.sh
  bash packages/devScripts/db-only-start-test.sh packages/devScripts/db-only-start.sh
  bash packages/devScripts/web-ui/tests/web-ui-test-runner-test.sh \
    packages/devScripts/web-ui/tests/web-ui-test.sh packages/devScripts/web-ui/coverage-manifest.json
'
```

Each pure module evaluation must return `true`; none creates a verification
derivation. Discovery uses independent temporary Nix stores. Packaging probes
the installed wrappers and generated service contract. Schema validation uses
actual installed exporter output, five CF extension types and OpenSCAP; it does
not claim XSD coverage for every custom-check variant. Build core before running
the installed-fixture command. Evaluation alone does not realize its inputs.

Prepare the launcher, SQLx environment and locked Cargo sources separately.
Only this preparation phase permits network acquisition:

```bash
nix build .#devScripts.state-machine-test .#devScripts.cve-test \
  .#devScripts.dashboard-visibility-test --no-link --builders '' \
  --option substituters https://cache.nixos.org --option extra-substituters '' \
  --option accept-flake-config false --max-jobs 1 --cores 2
nix develop .#sqlx --builders '' \
  --option substituters https://cache.nixos.org --option extra-substituters '' \
  --option accept-flake-config false --max-jobs 1 --cores 2 \
  -c cargo fetch --locked --manifest-path packages/default/Cargo.toml
```

Run one prepared database workflow offline. Select `state-machine-test`,
`cve-test`, or `dashboard-visibility-test` explicitly:

```bash
DATABASE_TEST_APP=state-machine-test
CARGO_NET_OFFLINE=true SQLX_OFFLINE=true \
nix run --offline --option substituters '' --option substitute false \
  --option builders '' --option fallback false \
  --option accept-flake-config false --max-jobs 0 --no-update-lock-file \
  ".#devScripts.$DATABASE_TEST_APP" -- -t=false up
```

The nested SQLx shell uses the same Nix settings. Missing shell or Cargo inputs
fail before cluster creation with preparation instructions. `SQLX_OFFLINE`
does not control Nix networking. Descendant Nix fixture builds can execute
locally, but must not substitute, use remote builders or fall back to fetching.
The launch chain declares Bash and uses absolute Nix executables. On root CI
runners, only PostgreSQL children drop to the resolved `nobody` UID/GID; Cargo
and evidence retain the invoking user's permissions. Cluster and short socket
directories are invocation-owned. The launcher waits for PostgreSQL shutdown
before removing them. Cleanup preserves an earlier failure or signal status.
If PostgreSQL cannot stop, the launcher retains its storage and fails.

Each launcher owns a fresh temporary cluster: CVE uses port 35432, state-machine
uses 35433, and dashboard uses 35434. An occupied port fails; an existing server
is never reused. The SQLx shell verifies the actual data
directory, role privileges (`LOGIN SUPERUSER CREATEDB`) and `pg_stat_statements`
preload, and exports both database URLs inside the SQLx shell. All selections
share `target/database-tests`. The state-machine selection preserves both
populated migration rehearsals, its ignored library regressions, and fourteen
integration targets. Its current execution count requires a fresh normal run.
The library selections include two TASK-496 heartbeat/state-decoding regressions.
Six repeated ignored selections and 94 nonignored library selections were
removed; normal package tests own the latter. Two repeated `composite_policy`
executions were removed from the CVE job because the full target owns them.
The CVE and dashboard jobs remain separate. CVE retains 27 workspace `--lib`
filters: 24 nonignored filters select 27 tests, and three `--ignored` filters
select one test each. Dashboard retains two `cf-server --lib --ignored`
visibility prefixes, each selecting two tests. Both dashboard groups execute
even when one fails; any failure makes the job fail. These selections are not
fully subsumed by the consolidated state-machine runner. The dashboard query
prefix owns `visibility_scope_handles_ambiguous_systems_cache_pushes_and_eval_attempts`;
TASK-501 removes that one duplicate from the state-machine job. The other
dedicated selections do not overlap the consolidated database selections.
Ordinary nonignored package
tests do not replace live-database execution: some CVE tests return early when
`CRYSTAL_FORGE_TEST_DATABASE_URL` is absent.

Every executed Cargo selection reports its command, passed/failed/ignored
counts, and exit status. A successful Cargo exit with zero passed tests fails
the runner. Workspace libraries with zero matches do not establish success;
the selection must execute at least one test across the selected libraries.
Compilation-only commands do not count as executed tests. The outer launcher
also requires a valid invocation-owned completion receipt. Process-compose
success without that receipt fails. Never interpret selection inventory,
compilation, or a reused build as passing test execution.

See the [component ownership table](../knowledge/testing/flake-checks.md#current-inventory-and-commands)
for the exact source homes and the task notes for the assertion audit.

### Database CI failure-path regression

The dedicated CVE/dashboard jobs run
`packages/devScripts/tests/database-ci-test.py` before their normal entry.
The script reads the actual offline execution block from `.gitlab-ci.yml` and
uses temporary process-compose fault configurations. It proves initialization
failure, Cargo-child failure, empty selection, missing/malformed completion,
failed receipt with successful compose exit, and interruption. It checks actual
statuses and removal of the invocation's data/socket directories. Its controlled
runner inputs are launcher regression evidence, not product integration tests.
CI retains logs and `results.json` under `database-ci-evidence/`, including after
a failed job.

Prepare the pinned Nix Python separately, then run the script offline. This
uses the same Python expression as CI and adds no flake check or verification
package:

```bash
python_expression='let
  f = builtins.getFlake ("git+file://" + toString ./.);
  p = import f.inputs.nixpkgs { system = builtins.currentSystem; };
in p.python3'
nix shell --impure --expr "$python_expression" --builders '' \
  --option substituters https://cache.nixos.org --option extra-substituters '' \
  --option accept-flake-config false --max-jobs 1 --cores 2 --command true
nix shell --offline --option substituters '' --option substitute false \
  --option builders '' --option fallback false \
  --option accept-flake-config false --max-jobs 0 --no-update-lock-file \
  --impure --expr "$python_expression" --command python3 \
  packages/devScripts/tests/database-ci-test.py /tmp/task501-database-evidence
```

An optional app argument limits the script to `cve-test` or
`dashboard-visibility-test`. Run on a disposable root CI runner to exercise
privilege separation. On Linux hosts with subordinate-ID user namespaces,
prefix the offline command with
`unshare --user --map-auto --map-root-user`. This exercises UID 0 in that
namespace, not a hosted runner. Nonroot execution checks the ordinary path.

TASK-501 local verification exposed assertions that the former false-green
jobs did not execute. At base `dbf50b47` plus the runner repair, the full CVE
entry executed one passing test, then failed
`recorded_derivation_path_restores_when_output_deriver_is_unknown` at
`cve_worker.rs:2705` (scanner calls 0, expected 1), returning 101. The normal
dashboard entry executed all four declared tests on root/nonroot paths:
three passed and `visibility_deployment_status_preserves_ahead_as_up_to_date`
failed at `queries/dashboard.rs:2723` (0, expected 2), returning 101.
These are failing product-test gates, not passing TASK-501 integration evidence.
The recorded-derivation failure also reproduced without root. Its fixture writes
a legacy completed cache publication for `file:///unused-test-cache` but creates
no matching eligible cache destination. Disposable-database inspection found
zero matching enabled destinations and a failed scan with unavailable inputs.
Migration `0301` and the materialization resolver require that match. This is a
fixture precondition mismatch, not evidence of a root or offline-toolchain fault.

Independent CVE selector diagnostics executed the remaining declared tests,
including each of the three ignored tests once with one passing result. Across
the root first test and those separate diagnostics, all 30 distinct selected
names executed: 27 passed and three failed. Besides the recorded-derivation
assertion, `policy_scan_heartbeat_prevents_recovery_and_ownership_loss_cancels_execution`
timed out waiting for its first scanner invocation, and
`remote_lease_claim_heartbeat_validation_and_clean_completion` observed one
diagnostic where its assertion expected zero. These independent results do not
constitute a successful full CVE job. Dashboard's failing fixture also lacks
the deployable same-flake target required by the current deployment view;
disposable-database inspection classified its two hosts as `unknown`.

The controlled fault regression passed all seven cases for both apps on both
root-namespace and nonroot paths: 28 verified case results. An interruption
exercise exposed a cleanup race; the launcher now waits for the invocation's
PostgreSQL shutdown before removing files. The final exercises verified clean
shutdown, removed data/socket directories, and preserved exit 143. A separate
intermediate execution correctly failed before cluster creation when a changed
launcher closure had not yet been prepared; that attempt is not a passing fault
case. After explicit preparation, the nonroot fault regression passed.

Production Rust and assertions remain unchanged. The canonical task notes
retain raw logs, selection inventory, independent diagnostic results, and
remaining verification. Full-flake/VM and exact-head hosted verification remain
coordinator-owned; no local fault result substitutes for those checks.

## Output handling

Large-output summarizers may help locate failures, but verification depends on the original command's exit status. If piping output, enable reliable pipeline status handling or capture the command status separately. Inspect raw output whenever a summary is ambiguous.

When a command fails, report:

- Exact command
- Exit status
- First actionable failure
- Whether it appears caused by the change, the environment, or an unrelated existing failure
- What remains unverified
