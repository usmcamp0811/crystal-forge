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

```bash
nix build .#checks.x86_64-linux.web-ui -L
ls result/screenshots/           # per-step PNGs + results.json + visual-report.json
ls result/screenshots/diffs/     # visual diff images for changed steps (if any)
```

The VM needs KVM and ~20GB RAM. The check runs the `ci_fast` profile (steps
whose manifest `profiles` include `ci_fast`).

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

Set `CF_UI_BASELINES_DIR` when invoking `integration-test.js` directly to enable
stored-baseline comparison. The repository does not currently ship stored
Dioxus baselines, and all manifest entries are advisory. Without a configured
directory, advisory captures report `new`; a future strict entry fails until its
dark and light baselines are supplied. The rendered design example remains the
default non-blocking visual reference in the Nix check.

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
| `checks/web-ui/design-parity/manifest.json` | Maps each primary view to the design example's `view` name and the real Dioxus route; lists themes and the compare method |
| `checks/web-ui/design-parity/generate-design-targets.js` | Playwright renders the offline design example per `view`+`theme` → `<view>--<theme>.design.png` |
| `checks/web-ui/design-parity/compare-design-parity.js` | Normalizes both sides and scores drift (ImageMagick RMSE) → report, summary, montages |

Flow inside the check (Phase 4c):

1. `integration-test.js` captures the real Dioxus views (`<view>--<theme>.dioxus.png`)
   by seeding `cf.ui.theme` and loading each route so the app applies its own
   theme through the real CF theme path.
2. The design example is vendored offline (React/ReactDOM/Babel are pinned via
   `fetchurl` and its CDN `<script>` tags rewritten to local files), then rendered
   headlessly for each `view`+`theme` using the `?view=&theme=` hook in `app.jsx`.
3. Each pair is normalized (resized to a common width, flattened) and scored with
   `compare -metric RMSE`. Lower drift = closer to the design.

Outputs (in `screenshots/`, exposed as MR artifacts):

- `design-drift-report.json` — per view/theme drift + similarity, averages, worst offenders.
- `design-drift-summary.md` — table + overall similarity, prepended to the MR comment.
- `montages/<view>--<theme>.montage.png` — side-by-side (design target | real Dioxus).
- `design-targets/` and `design-parity/` — raw target and Dioxus captures.

This is **non-blocking**: it never fails the check. It is a directional gauge —
React and Dioxus will never be pixel-identical, so treat the number as "how far
from the design" and inspect the montages for real drift.

To add a view to the parity harness, add an entry to
`checks/web-ui/design-parity/manifest.json` (`designView` must be a valid design
example view name; `route` must be a real Dioxus route).

### Approving baselines

1. Get fresh screenshots: `result/screenshots/` from a local build, or
   download the `web-ui-screenshots` artifact from the `flake-check: [web-ui]`
   CI job.
2. Approve (all, or specific steps):

   ```bash
   ./checks/web-ui/approve-baselines.sh result/screenshots
   ./checks/web-ui/approve-baselines.sh result/screenshots 06-dashboard--dark 06-dashboard--light
   ```

3. Review `git diff --stat -- checks/web-ui/baselines`, commit, and note the
   approval in the MR.

Only manifest step/theme captures are approved; reports, diffs, and export
screenshots are skipped.

### Promoting a step to strict

Once a step's screenshot is stable across runs (deterministic mocked data, no
live timestamps), set `"baseline": "strict"` for it in the manifest. Steps
rendering real backend data (auth flow, roundtrip steps) should stay
`advisory` until their rendering is made time-independent.

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

## CI integration

`flake-check: [web-ui]` builds the check and exposes
`web-ui-screenshots/` (screenshots, `results.json`, `visual-report.json`,
`visual-summary.md`, `diffs/`, plus `design-drift-report.json`,
`design-drift-summary.md`, `montages/`, `design-targets/`, `design-parity/`) as
artifacts. The `web-ui-screenshots-mr-comment` job posts/updates an MR comment
with the coverage + visual + non-blocking design-parity summary, all themed step
screenshots, up to 20 diff images, and up to 26 design-parity montages.

## Known issues

- `27-hardening-fleet` targets `/hardening`, which has no registered route
  (orphaned view) — tracked as TASK-377.
- A number of ci_fast steps fail routinely without blocking (only the
  critical list gates) — triage tracked as TASK-378.
