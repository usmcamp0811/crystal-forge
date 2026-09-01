# Web UI Check — Runbook

The `web-ui` Nix check verifies the web UI end-to-end against a real Crystal
Forge server inside a NixOS VM: build verification, manifest-driven Playwright
steps with semantic assertions, screenshots, and visual regression against
approved baselines.

## Layout

| Path | Purpose |
| --- | --- |
| `checks/web-ui/default.nix` | NixOS VM test definition (server, gitserver, phases, gates) |
| `checks/web-ui/coverage-manifest.json` | Single source of truth: steps, profiles, routes, design refs, baseline policies, exclusions |
| `checks/web-ui/design-fixtures.json` | Canonical non-secret fixture data for aligning design examples with representative checked UI states |
| `checks/web-ui/tests/integration-test.js` | Playwright steps (must stay in sync with the manifest) |
| `checks/web-ui/baselines/` | Approved golden screenshots (`<step>--dark.png` and `<step>--light.png`) |
| `checks/web-ui/approve-baselines.sh` | Copies captured screenshots into `baselines/` |
| `docs/design/CrystalForge/` | Mocked JSX design reference (gold standard); steps map to it via `designRef` |

## Running locally

### Fast development loop

Enter the development shell and start the persistent development stack once:

```bash
nix develop
run-ui-dev
```

In another terminal in the same development shell, run one or more supported
workflows:

```bash
web-ui-test 12-systems
web-ui-test 12-systems 12a-systems-empty-state
```

`CF_UI_TEST_STEPS` remains available when a calling tool already supplies the
selection:

```bash
CF_UI_TEST_STEPS="12-systems" web-ui-test
```

The command prints the UI URL, selected workflows, and artifact directory.
Artifacts are written below `.tmp/web-ui-test/` by default. The command uses
the Nix-provided Playwright and Chromium versions from the development shell.
It does not download a browser or restart PostgreSQL, the Crystal Forge server,
or Dioxus.

The host loop asserts semantics and captures screenshots, but it does not
compare against approved baselines. Baseline comparison belongs to the
authoritative check, which renders the packaged UI in a fixed environment. Set
`CF_UI_BASELINES_DIR` to opt in locally, and expect environment-related
differences.

The `settings.devStackWorkflows` list in `coverage-manifest.json` contains the
workflows that are repeat-safe against the persistent development stack. The
initial list contains `12-systems` and `12a-systems-empty-state`. These
workflows use a new browser context on each invocation and do not mutate
persistent server state. A workflow that is not in this list fails with the
command for the authoritative VM harness.
Add a workflow only after confirming that it does not depend on VM services,
ordered predecessor workflows, or state left by an earlier run.

> `web-ui-test` is the fast development feedback loop. The NixOS Web UI check
> remains the reproducible authoritative verification boundary.

The runner's own contract (selection, rejection of VM-only workflows,
readiness reporting, artifact creation, exit-status propagation) is covered by
a separate lightweight check that starts no services:

```bash
nix build .#checks.x86_64-linux.web-ui-test-runner --no-link -L
```

### Authoritative verification

Before claiming a Web UI implementation complete, run each relevant workflow
through the authoritative NixOS check:

```bash
CF_UI_TEST_STEPS="12-systems" \
  nix build --impure .#checks.x86_64-linux.web-ui --no-link -L
```

Run broader checks such as `nix flake check --keep-going -L` when the active
task or repository verification policy requires them.

### NixOS Web UI check

```bash
nix build .#checks.x86_64-linux.web-ui -L
ls result/screenshots/           # per-step PNGs + results.json + visual-report.json
ls result/screenshots/diffs/     # visual diff images for changed steps (if any)
```

The VM needs KVM and ~20GB RAM. The check runs the `ci_fast` profile (steps
whose manifest `profiles` include `ci_fast`).

The Nix check copies `checks/web-ui/baselines/` into the VM and sets
`CF_UI_BASELINES_DIR` automatically. A normal pure build always enforces every
`strict` manifest entry.

Legacy cache/builder "mega" phases only run interactively with
`CF_WEB_UI_RUN_MEGA_PHASES=1` (the env var cannot cross the Nix build
sandbox); their VMs are not booted otherwise.

## Phases and gates

1. **Warmup** — boots `machine` + `gitserver` only (cache VMs are skipped
   unless mega phases are enabled).
2. **Build verification** — index.html served, JS loader referenced and served,
   packaged WASM output present with a valid `\0asm` magic header. Hard gate.
3. **Playwright steps** — coverage gate first (steps ⇄ manifest must agree
   exactly), then each step runs its semantic assertions and captures dark and
   light screenshots. Any failed step gives the integration process a nonzero
   exit after it writes diagnostic artifacts.
4. **Critical gate** — the `critical_tests` list in `default.nix` additionally
   prevents focused profiles from omitting required workflows.
5. **Visual gate** — steps with baseline policy `strict` must match within
   threshold. `advisory` steps only report.
6. **OSCAL / SARIF export validation** — downloads validated against vendored
   schemas.

## Visual baselines

Policies (per step, in the manifest):

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

## Design parity evidence

`docs/design/CrystalForge/` remains the mocked JSX design reference. To make
comparisons more objective, `checks/web-ui/design-fixtures.json` defines the
canonical representative data that design examples should render when they are
refactored or regenerated. Pass this JSON to Claude/design tooling alongside the
target design component so the design example and Playwright state use the same
base fleet/build/security/compliance data.

The web-ui check records design-reference coverage in `visual-report.json` and
`visual-summary.md`:

- which checked steps have a `designRef` mapping;
- which fixture file should be used to align the design example;
- the current design-reference policy.

### Rendered design-parity harness

The check also renders the tracked design example itself and compares it,
per view and theme, against the real Dioxus UI. TASK-440 has eight canonical
targets in two themes, so each focused run produces 16 actual React-vs-Dioxus
pixel comparisons. `checks/web-ui/design-fixtures.json` records the shared
representative identities and counts that keep these targets comparable.

| Path | Purpose |
| --- | --- |
| `checks/web-ui/design-parity/manifest.json` | Maps primary surfaces and nested TASK-440 targets to design and Dioxus interactions, identity markers, viewports, canonical workflow steps, themes, and the compare method |
| `checks/web-ui/design-parity/generate-design-targets.js` | Playwright drives the offline design through real controls, validates each observable state, and writes `<target>--<theme>.design.png` |
| `checks/web-ui/design-parity/compare-design-parity.js` | Normalizes both sides and scores drift (ImageMagick RMSE) → report, summary, montages |

Flow inside the check (Phase 4c):

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
4. Canonical TASK-440 workflows also copy their themed captures to the mapped
   parity target names. The generator drives `cf-open-system` and Flake controls,
   validates the active state and exact fixture text, and records semantic
   contracts for both React and Dioxus.
5. TASK-440 source images must match the target viewport. The comparator crops
   the mapped content surface, creates a montage and absolute-difference image,
   and requires all selected pairs to compare without semantic or report errors.
   A focused run requires two themed pairs per selected workflow. A complete run
   requires all 16 pairs.

Outputs (in `screenshots/`, exposed as MR artifacts):

- `design-drift-report.json` — per view/theme drift + similarity, averages, worst offenders.
- `design-drift-summary.md` — table + overall similarity, prepended to the MR comment.
- `montages/<view>--<theme>.montage.png` — side-by-side (design target | real Dioxus).
- `diffs/<view>--<theme>.difference.png` — absolute content-surface difference.
- `design-targets/` and `design-parity/` — raw target and Dioxus captures.

For TASK-440, missing, mislabeled, failed, wrong-viewport, and semantically
invalid targets fail the check. Failure to compare an expected pair also fails
the check. RMSE is advisory: a low similarity score does not fail the check and
is not a visual-parity verdict. Inspect every TASK-440 montage and
absolute-difference image before accepting the implementation.

The design data uses abbreviated commit revisions for presentation. Browser
API fixtures use synthetic 40-character revisions with the same prefixes
because production APIs, navigation, snapshots, and comparisons require full
immutable SHA identity. Assertions continue to enforce the displayed design
prefix separately.

The Config comparison has one intentional security-driven difference. The
production DTO exposes provenance paths and input/revision identity, but it does
not expose Nix source text. Dioxus therefore reports that source text is
unavailable while the mocked design example can display source code. Do not add
fixture-only DTO fields to erase this difference.

To add a view to the parity harness, add an entry to
`checks/web-ui/design-parity/manifest.json`. Set the design `route` and, when
the production route differs, set `dioxusRoute`. Add required `designMarker`
and `dioxusMarker` selectors and the `designActions` or `dioxusActions` needed
to reach nested surfaces. A design action can set `force: true` only when a
known design overlay obscures the intended control; normal actions retain
Playwright actionability checks.

For a nested TASK-440 target, also define its viewport, observable
`designState`, expected text, content selector, and canonical `dioxusStep`
mapping.

### Approving baselines

1. Generate fresh captures without disabling semantic, critical-workflow, or
   process-exit gates. Baseline update mode bypasses only strict pixel rejection:

   ```bash
   CF_UI_UPDATE_BASELINES=1 nix build --impure path:.#checks.x86_64-linux.web-ui -L
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
approved baseline commit, the normal `flake-check: [web-ui]` job must pass at
the exact MR head.

The approval utility copies only captures whose generated visual record has
policy `strict`. It skips failed-step diagnostics, reports, diffs, and export
screenshots. It rejects empty strict sets, failed owning semantic steps,
duplicate or unsafe capture names, and source or destination paths that resolve
outside their canonical roots.

### Promoting a step to strict

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
5. After the first green run, approve both themed baselines for the new step.

Destructive or unsafe flows must use mocked routes/disposable data or be
documented in the manifest's `exclusions`.

## Debugging failures

- **Coverage gate ("coverage manifest drift")** — the steps in
  `integration-test.js` and the manifest entries no longer agree; the error
  lists both directions.
- **Step failure** — see the step's error in the job log and its screenshot
  in the artifacts; failed steps still attempt a screenshot for debugging.
- **Strict visual failure** — inspect `screenshots/diffs/<step>--<theme>.diff.png`
  (red = changed pixels). If the change is intended, re-approve the baseline.
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

`flake-check: [web-ui]` builds the check and exposes
`web-ui-screenshots/` (screenshots, `results.json`, `visual-report.json`,
`visual-summary.md`, `diffs/`, plus `design-drift-report.json`,
`design-drift-summary.md`, `montages/`, `design-targets/`, `design-parity/`) as
artifacts. The `web-ui-screenshots-mr-comment` job posts/updates an MR comment
with the coverage + visual + design-parity summary, all themed step
screenshots, up to 20 diff images, and up to 26 design-parity montages.
The opt-in `web-ui-baseline-candidates` job publishes equivalent candidate
artifacts after semantic and critical-workflow gates pass in baseline update
mode. It does not replace or weaken `flake-check: [web-ui]`.

## Known issues

- `27-hardening-fleet` targets `/hardening`, which has no registered route
  (orphaned view) — tracked as TASK-377.
- A number of ci_fast steps fail routinely without blocking (only the
  critical list gates) — triage tracked as TASK-378.
