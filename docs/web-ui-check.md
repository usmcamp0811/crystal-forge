# Web UI Check Runbook

The Web UI checks verify the production Web UI against real Chromium and an
embedded-UI Crystal Forge server in isolated NixOS VMs. The stable checks split
production packaging, required semantics, browser exports, and advisory design
parity into independently reproducible responsibilities.

The `web-ui` attribute is a compatibility check. It proves that
`cf-server-drv` serves the production `pkgs.crystal-forge.web-ui` assets and
that the authentication and application shell work in real Chromium. It is not
the complete merge gate. The complete blocking gate is `web-ui`, the required
steps owned by the three semantic groups, and `web-ui-exports`. Advisory steps
in those same semantic groups still run and publish evidence.

## Layout

| Path | Purpose |
| --- | --- |
| `checks/web-ui/default.nix` | NixOS VM test definition (server, gitserver, phases, gates) |
| `checks/web-ui/coverage-manifest.json` | Single source of truth: steps, profiles, routes, design refs, baseline policies, exclusions |
| `checks/web-ui/check-groups.json` | Explicit ordered ownership for required, compatibility, and advisory named profiles |
| `checks/web-ui/design-fixtures.json` | Canonical non-secret fixture data for aligning design examples with representative checked UI states |
| `checks/web-ui/tests/integration-test.js` | Playwright steps (must stay in sync with the manifest) |
| `checks/web-ui/tests/validate-check-groups.js` | Static ownership validator for omissions, duplicates, unknown steps, and invalid group order |
| `checks/web-ui/baselines/` | Approved golden screenshots for strict workflows |
| `checks/web-ui/approve-baselines.sh` | Copies reviewed strict captures into the baseline directory |
| `docs/design/CrystalForge/` | Mocked JSX design reference (gold standard); steps map to it via `designRef` |

## Running locally

| Responsibility | Stable attribute | Policy | Required / advisory `ci_fast` steps |
| --- | --- | --- | ---: |
| Embedded production server and shell compatibility | `web-ui` | Blocking compatibility | 6 |
| Fleet semantics and evidence | `web-ui-fleet` | Mixed | 3 / 32 |
| Pipeline semantics and evidence | `web-ui-pipeline` | Mixed | 5 / 28 |
| Governance semantics and evidence | `web-ui-governance` | Mixed | 36 / 12 |
| Browser OSCAL and SARIF exports | `web-ui-exports` | Blocking | Independent export flows |
| Rendered design parity | `web-ui-design-parity` | Advisory | Independent capture selection |

Run any responsibility with its exact command:

```bash
nix build .#checks.x86_64-linux.web-ui -L
nix build .#checks.x86_64-linux.web-ui-fleet -L
nix build .#checks.x86_64-linux.web-ui-pipeline -L
nix build .#checks.x86_64-linux.web-ui-governance -L
nix build .#checks.x86_64-linux.web-ui-exports -L
nix build .#checks.x86_64-linux.web-ui-design-parity -L
```

Each VM needs KVM and approximately 20 GB RAM. Required groups create their
own database and browser context. Groups can run in any order or concurrently.
The group step arrays preserve manifest order so stateful chains keep their
historical transition order. Named groups without the registration and login
chain run an authentication preflight against their isolated server.

The `ci_fast` profile has 116 steps. `check-groups.json` assigns each step to
exactly one of `fleet`, `pipeline`, or `governance` and classifies that ownership
as required or advisory. The 44 required steps preserve the historical gate,
add the complete TASK-433 critical workflow set, and require
`03-registration-submit` and `04-post-register-login` so a broken authentication
transition cannot be masked. The other 72 steps remain advisory, but each still
executes once and retains screenshots and result evidence. The three Setup Coach
workflows in `ci_fast` belong to the fleet group and remain advisory. The exact
ordered classifications are in `check-groups.json`. The compatibility smoke
separately requires all six selected steps. Run the static invariant check
without a VM:

```bash
node checks/web-ui/tests/validate-check-groups.js
```

The Nix check copies `checks/web-ui/baselines/` into the VM and sets
`CF_UI_BASELINES_DIR` automatically. A normal pure build always enforces every
`strict` manifest entry.

Legacy cache/builder "mega" phases only run interactively with
`CF_WEB_UI_RUN_MEGA_PHASES=1` (the env var cannot cross the Nix build
sandbox); their VMs are not booted otherwise.

## Phases and gates

1. **Warmup** — boots `machine` + `gitserver` only (cache VMs are skipped
   unless mega phases are enabled).
2. **Build verification** — `web-ui` verifies that index.html is served, its JS
   loader is served, and the packaged WASM has a valid `\0asm` magic header.
   Other partitions still verify server readiness.
3. **Playwright steps** — coverage and ownership gates run first. Each selected
   step runs its semantic assertions and captures dark and light screenshots.
4. **Semantic gate** — every selected required step must produce a passing
   result. A missing required result fails the gate. Failed or missing advisory
   results are reported but do not fail the group.
5. **Visual gate** — steps with baseline policy `strict` must have every expected
   baseline and match within threshold. `advisory` steps only report drift.
6. **OSCAL / SARIF export validation** — the independent export check validates
   browser downloads against vendored schemas.

## Evidence and verdicts

Each stable attribute is a small logical gate derivation. Its `.evidence`
passthru is the NixOS VM derivation. The VM records logical browser failures and
still succeeds after it copies available evidence. The outer gate reads
`screenshots/check-verdict.json` and fails when a blocking logical check failed.
This separation lets CI retrieve failed-step evidence without running the VM a
second time:

```bash
nix build .#checks.x86_64-linux.web-ui-fleet.evidence -L
```

Evidence includes `results.json`, `verdict.json`, `check-verdict.json`, visual
reports, successful themed screenshots, and a best-effort `<step>.png` for each
failed step. Export evidence also includes OSCAL and SARIF result JSON and final
screenshots. `phase-timings.json` records VM and fixture setup, semantic browser
execution, design parity, exports, evidence finalization, and the total VM
evidence duration. Disabled optional phases have an explicit `skipped` timing.
Server startup, missing result files, manifest drift, and other
infrastructure failures still fail the evidence derivation because no complete
verdict can be trusted. Browser verdicts separate `failedRequiredSteps` from
`failedAdvisorySteps`; `failedSteps` is a compatibility alias for required
failures only. `web-ui-design-parity` is advisory, so design or browser
logical drift does not fail its outer derivation. Its `check-verdict.json` still
records command statuses, missing outputs, and a false verdict, and CI labels
that producer `advisory-failed`. Infrastructure failures still fail it.

The browser process has a 900-second timeout. This reserves five minutes of the
20-minute job target for VM startup, Nix realization, and evidence publication.
Local complete-group runs recorded during this change remained below this
browser limit. The wrapper sends
`TERM`, waits 30 seconds, and then sends `KILL`. It always publishes
`integration.exit` atomically. Before each browser action, the harness publishes
`current-step.json` atomically. A timeout prints `integration.log`, the current
step, and the server journal, then fails as infrastructure without creating or
accepting a logical verdict. For an impure local diagnosis only, set
`CF_UI_PROCESS_TIMEOUT` to a shorter number of seconds and build with `--impure`.

## Visual baselines

- `none` — no comparison.
- `advisory` (default) — compared when a baseline exists; differences are
  reported in `visual-report.json` and the MR comment, with a diff image in
  `screenshots/diffs/`, but never fail the check. A missing baseline reports
  `new`.
- `strict` — baseline must exist and match within threshold; otherwise the
  check fails.

Threshold: ImageMagick `compare -metric AE -fuzz <fuzzPercent>%`; a step
differs when `diffPixels / totalPixels > maxDiffPixelRatio`. Defaults live in
`settings.visualDiff` in the manifest; override per step with
`maxDiffPixelRatio`.

Set `CF_UI_BASELINES_DIR` when invoking `integration-test.js` directly. The Nix
check discovers the repository baselines without extra configuration. TASK-433
canonical workflows and the critical catalog-deletion workflow are strict.
Their final, intermediate, narrow, and mobile captures must all have committed
baselines. The rendered design example remains a separate non-blocking visual
reference.

Every covered step captures one screenshot per configured visual theme in the
check evidence directory. The default themes are listed in
`settings.visualThemes` and currently require both `dark` and `light`.
Design-parity command and output failures produce a false advisory verdict, but
visual similarity itself does not block merging.

## Design parity gauge

`docs/design/CrystalForge/` remains the mocked JSX design reference. To make
comparisons more objective, `checks/web-ui/design-fixtures.json` defines the
canonical representative data that design examples should render when they are
refactored or regenerated. Pass this JSON to Claude/design tooling alongside the
target design component so the design example and Playwright state use the same
base fleet/build/security/compliance data.

The web-ui check reports a **non-blocking design parity gauge** in
`visual-report.json` and `visual-summary.md`:

- which checked steps have a `designRef` mapping;
- which fixture file should be used to align the design example;
- the current policy (`non-blocking-gauge`).

### Rendered design-parity harness (non-blocking)

The check also renders the tracked design example itself and compares it,
per view and theme, against the real Dioxus UI. Both sides are backed by the
shared golden fixture (`docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json`),
so a difference indicates real UI/design drift rather than data differences.

| Path | Purpose |
| --- | --- |
| `checks/web-ui/design-parity/manifest.json` | Maps each named surface to its Dioxus route, design and Dioxus interaction paths, identity markers, themes, and compare method |
| `checks/web-ui/design-parity/generate-design-targets.js` | Playwright uses the real design navigation and identity marker per surface and theme → `<view>--<theme>.design.png` |
| `checks/web-ui/design-parity/compare-design-parity.js` | Normalizes both sides and scores drift (ImageMagick RMSE) → report, summary, montages |

Flow inside `web-ui-design-parity`:

1. `integration-test.js` loads each real Dioxus route, performs optional
   `dioxusActions`, validates `dioxusMarker`, and captures
   `<view>--<theme>.dioxus.png`. The app applies each seeded theme through the
   real CF theme path.
2. The design example is vendored offline (React/ReactDOM/Babel are pinned via
   `fetchurl` and its CDN `<script>` tags rewritten to local files). Playwright
   starts from the design app shell, selects the theme through the real control,
   performs optional `designActions`, and saves a target only after
   `designMarker` identifies the expected surface.
3. Each pair is normalized (resized to a common width, flattened) and scored with
   `compare -metric RMSE`. Lower drift = closer to the design.

Outputs (in `screenshots/`, exposed as MR artifacts):

- `design-drift-report.json` — per view/theme drift + similarity, averages, worst offenders.
- `design-drift-summary.md` — table + overall similarity, prepended to the MR comment.
- `montages/<view>--<theme>.montage.png` — side-by-side (design target | real Dioxus).
- `design-targets/` and `design-parity/` — raw target and Dioxus captures.

This is **non-blocking** at the outer Nix and pipeline policy layers. Generation,
comparison, or missing-output failures produce a false advisory verdict and an
`advisory-failed` producer status. React and Dioxus will not be pixel-identical,
so treat the similarity as a directional gauge and inspect the montages.

To add a view to the parity harness, add an entry to
`checks/web-ui/design-parity/manifest.json`. Set the design `route` and, when
the production route differs, set `dioxusRoute`. Add required `designMarker`
and `dioxusMarker` selectors and the `designActions` or `dioxusActions` needed
to reach nested surfaces. A design action can set `force: true` only when a
known design overlay obscures the intended control; normal actions retain
Playwright actionability checks.

### Approving baselines

1. Generate fresh captures without disabling semantic, critical-workflow, or
   process-exit gates. Baseline update mode bypasses only strict pixel rejection:

   ```bash
   CF_UI_UPDATE_BASELINES=1 nix build --impure path:.#checks.x86_64-linux.web-ui-governance -L
   ```

2. Inspect `result/screenshots/visual-report.json`, every TASK-433 PNG, and any
   files under `result/screenshots/diffs/`. Do not approve a failed semantic
   run or an unexpected layout.
3. Approve all strict captures listed by the generated visual report:

   ```bash
   ./checks/web-ui/approve-baselines.sh result/screenshots
   ```

4. Review the PNG diff and commit all approved strict baselines:

   ```bash
   git diff --stat -- checks/web-ui/baselines
   git status --short -- checks/web-ui/baselines
   ```

5. Run the normal strict gate. Do not use `--impure` or
   `CF_UI_UPDATE_BASELINES` for final verification:

   ```bash
   nix build path:.#checks.x86_64-linux.web-ui -L
   ```

   The `path:.` form includes newly generated baseline PNGs before they are
   committed. After the baseline commit, the normal `.#checks...` form is
   equivalent.

If a strict failure occurs only in GitLab CI and the failed Nix derivation has
no review artifact, start the manual MR job `web-ui-baseline-candidates`. The
job runs the same check with baseline update mode enabled and publishes the
`web-ui-baseline-candidates/` artifact. Download the artifact, inspect its
`visual-report.json`, strict PNGs, and diffs, and pass the artifact directory to
`approve-baselines.sh`. The manual job is not final verification. After an
approved baseline commit, the normal `web-ui-check: [web-ui-governance]` job
must pass at the exact MR head.

The approval utility copies only captures whose generated visual record has
policy `strict`. It skips failed-step diagnostics, reports, diffs, and export
screenshots. It rejects empty strict sets, failed owning semantic steps,
duplicate or unsafe capture names, and source or destination paths that resolve
outside their canonical roots.

### Strict baseline status

Set `"baseline": "strict"` only when the workflow controls rendered values and
ordering. A strict workflow must avoid visible random identifiers and live
timestamps, or normalize them before capture. After promotion, generate,
review, approve, and commit every final and intermediate themed capture before
the normal gate can pass.

## Adding a route/state to coverage

1. Add a step to `tests/integration-test.js`: navigate, run semantic
   assertions (`assertVisible` etc.) for the critical content, interact if
   applicable.
2. Add a matching entry to `coverage-manifest.json` (`name` must match):
   route, `designRef` (path under `docs/design/CrystalForge/` if an equivalent
   mocked design exists), `profiles`, `baseline` policy.
3. If the route/state introduces new representative data, update
   `checks/web-ui/design-fixtures.json` so design examples and Playwright mocks
   can share the same objective data contract.
4. The check fails on any drift between the steps and the manifest, so CI
   will catch a missed update.
5. Confirm that both themed screenshots appear in producer evidence. If the
   workflow is strict, approve and commit all required captures.

Destructive or unsafe flows must use mocked routes/disposable data or be
documented in the manifest's `exclusions`.

## Debugging failures

- **Coverage gate ("coverage manifest drift")** — the steps in
  `integration-test.js` and the manifest entries no longer agree; the error
  lists both directions.
- **Step failure** — see the step's error in the job log and its screenshot
  in the artifacts; failed steps still attempt a screenshot for debugging.
- **Build verification failure** — the served index/loader/wasm chain is
  broken; check the server unit log in the VM output.
- **Everything failing/timeout** — check `integration.log` output in the job
  log; a `fatal.json` marker means the run aborted before steps executed.
- **Failed derivation diagnostics** — when the VM remains reachable, the test
  driver exports `browser-failure-artifacts/` with `integration.log`,
  `server-journal.log`, `integration.exit`, and all available browser reports
  and screenshots before it rejects the derivation. The driver also prints
  `integration.log` and the server journal before it rethrows a browser timeout.
  A failed Nix derivation has no `result` output, so exported files are not a
  durable artifact unless the caller or CI captures the test-driver workdir.
  `--keep-failed` preserves the failed sandbox when the local Nix builder
  supports it, but it does not keep a running VM and cannot recover files when
  the VM or test driver became unreachable. Treat the printed logs as the
  reliable fallback.

## CI integration

The `web-ui-check` CI matrix runs `web-ui`, `web-ui-fleet`,
`web-ui-pipeline`, `web-ui-governance`, and `web-ui-exports` as five required
jobs. The three semantic jobs fail only for required step or process failures;
their advisory step failures remain visible in `verdict.json`,
`check-verdict.json`, and the aggregate report. Available runners can execute
the jobs concurrently. Limited runner capacity only queues jobs; it does not
remove checks or advisory execution from the gate.

The separate `web-ui-design-parity` job is advisory. All six producers are
interruptible and publish evidence under `web-ui-evidence/<check>/`. Each
directory contains `producer.json` with the outer Nix realization/gate,
evidence lookup, and artifact-copy statuses and durations, plus the VM's
`screenshots/` evidence when transfer succeeded. It also contains
`nix-realization.log`. `producer.json` classifies the realization as a local
hit, substitution, build, mixed realization, or unknown realization and records
the runner queue duration reported by GitLab's authenticated Jobs API. Local
runs and API failures record the queue duration as unavailable.

The advisory `web-ui-evidence-report` job uses GitLab `needs` to download every
producer artifact after success or failure. Its `web-ui-report/report.md`
artifact reports passed, failed, and missing producers, failed browser steps,
export results, visual and design summaries, screenshots, and producer job
links. On merge-request pipelines, this job also posts one pipeline-specific MR
comment and uploads a bounded subset of screenshots inline. The report lists all
downloaded artifacts, and the aggregate job preserves both `web-ui-report/` and
`web-ui-evidence/`. `web-ui-report/aggregation.json` records aggregation and
publication timing. Upload and note writes require a masked `GITLAB_TOKEN` and
use `PRIVATE-TOKEN`; when the token is absent, CI warns and retains artifacts
without claiming publication. Comment or upload failures do not fail the
advisory report job. Blocking producer failures still fail the pipeline. Main
pipelines retain the report artifact but do not post an MR comment.
Both producer and aggregate scripts run through flake-pinned Nix packages.

The opt-in `web-ui-baseline-candidates` job runs the governance shard in
baseline-update mode and publishes `web-ui-baseline-candidates/` after its
required semantic workflows pass. The job does not replace or weaken the normal
`web-ui-check` matrix.

### Timing measurement and regression detection

`web-ui-report/aggregation.json` contains each producer's job duration, runner
queue duration, cache state, Nix realization duration, and artifact-copy
duration. It also records the blocking-job median, blocking-job maximum, and
the blocking critical path. The critical path is the maximum duration of the
five blocking producers. Queue duration is reported separately and does not
count toward the 20-minute execution target. Critical-path, median, and maximum
values are unavailable when any blocking producer lacks valid timing metadata;
an incomplete pipeline cannot report a passing timing envelope.

Use three merge-request pipelines after a cache-affecting change. For each
pipeline, record these values from `aggregation.json`:

1. Each producer duration, queue duration, and cache state.
2. The blocking critical path.
3. The blocking-job median and maximum.
4. The VM phase timings from each producer's
   `screenshots/phase-timings.json`.

Every blocking producer and the blocking critical path must remain below 20
minutes. Compare a slower run first by cache state, then by Nix realization,
VM fixture setup, browser semantics, exports or design processing, evidence
copy, and aggregation. Treat a blocking critical path of 20 minutes or more as
a latency regression. Do not attribute runner queue time to check execution.
The representative three-pipeline baseline for this change is recorded after
the merge-request pipelines run.

## Known issues

- `27-hardening-fleet` targets `/hardening`, which has no registered route
  (orphaned view) — tracked as TASK-377.
- Required failures block their responsible semantic group. Advisory failures
  remain in the same group's evidence and do not fail that group.
