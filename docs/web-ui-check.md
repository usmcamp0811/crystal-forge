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
2. **Build verification** — index.html served, JS loader referenced, WASM
   asset served with a valid `\0asm` magic header. Hard gate.
3. **Playwright steps** — coverage gate first (steps ⇄ manifest must agree
   exactly), then each step runs its semantic assertions, captures dark and
   light screenshots, and compares each themed screenshot to its baseline.
4. **Critical gate** — the `critical_tests` list in `default.nix` must pass.
5. **Visual gate** — steps with baseline policy `strict` must match within
   threshold. `advisory` steps only report.
6. **OSCAL / SARIF export validation** — downloads validated against vendored
   schemas.

## Visual baselines

Policies (per step, in the manifest):

- `none` — no comparison.
- `advisory` (default) — compared when a baseline exists; differences are
  reported in `visual-report.json` and the MR comment, with a diff image in
  `screenshots/diffs/`, but never fail the check.
- `strict` — baseline must exist and match within threshold; otherwise the
  check fails.

Threshold: ImageMagick `compare -metric AE -fuzz <fuzzPercent>%`; a step
differs when `diffPixels / totalPixels > maxDiffPixelRatio`. Defaults live in
`settings.visualDiff` in the manifest; override per step with
`maxDiffPixelRatio`.

Every covered step captures one baseline per configured visual theme. The
default themes are listed in `settings.visualThemes` and currently require both
`dark` and `light`, producing `checks/web-ui/baselines/<step>--dark.png` and
`checks/web-ui/baselines/<step>--light.png`. Review and approve both files so
theme-specific regressions are visible in CI/MR artifacts.

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

TASK-139 does not make rendered-design pixel comparison a hard gate. A future
task can add a rendered design-example artifact and compare it against the
Playwright screenshots once the design examples consistently consume
`design-fixtures.json`.

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
`visual-summary.md`, `diffs/`) as artifacts. The
`web-ui-screenshots-mr-comment` job posts/updates an MR comment with the
coverage + visual/design-parity summary, all themed step screenshots, and up to
20 diff images.

## Known issues

- `27-hardening-fleet` targets `/hardening`, which has no registered route
  (orphaned view) — tracked as TASK-377.
- A number of ci_fast steps fail routinely without blocking (only the
  critical list gates) — triage tracked as TASK-378.
