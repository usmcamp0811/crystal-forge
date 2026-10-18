# Evaluation drawer comparison

`capture-eval-drawer-shell.cjs` captures the unchanged React reference drawer
and the Dioxus preview drawer. Run the script only against a task-owned fixture
preview. The script logs in with the local fixture account. Queue and log
responses are intercepted in a separate browser context; evaluation rows are
not inserted or changed.

## Inputs

Run Node with the repository-pinned Playwright module and Chromium environment
(`NODE_PATH` and `PLAYWRIGHT_BROWSERS_PATH`). These are the same inputs as the
`generate-design-targets` app. The reference directory must be the offline design
produced by that app, including its vendored React, ReactDOM, and Babel files.

```sh
CF_UI_DEV_BASE_URL=http://127.0.0.1:8080 \
CF_UI_DEV_API_BASE_URL=http://127.0.0.1:3445 \
node packages/devScripts/web-ui/design-parity/capture-eval-drawer-shell.cjs \
  "$OFFLINE_DESIGN_DIR" /tmp/opencode/task488/drawer-shell
```

`CF_UI_TEST_USERNAME` and `CF_UI_TEST_PASSWORD` override the local fixture
account. Do not use production credentials or a shared backend.

## Current contract

The script compares desktop dark, desktop light, and 720px-wide dark states.
It uses the first reference active evaluation for identity and policy counts.
Both renderers receive commit ID 488 and an empty log response. The original
React component is mounted with those props; its source and styles are unchanged.
Finite opening animations and fonts settle before capture.

Each state produces two drawer screenshots and a JSON record of bounding boxes,
font metrics, colors, padding, and gaps. Assertions compare header and tab heights,
metadata color, and the typography of metadata, time, tabs, and logs.

**A passing shell comparison does not establish pixel parity.** Status labels,
timestamp rendering, action availability, development chrome offsets, and the
design's empty-log behavior still differ. Screenshots retain those differences
rather than masking them. TASK-488 now includes bundle matrix data and rendering;
the user expanded its original presentation-only scope.

## Structured matrix workflow

`26e-evaluation-policy-bundle-controls` is a retained host browser workflow.
It uses browser-local test responses with 60
systems, 128 exact control identities, six real bundles, 32 shared controls, and
separate warning, failure, unknown, and nonapplicable cases. Example counts and
names exist only in the test fixture; the production component derives them
from the API response.

```sh
CF_UI_DEV_BASE_URL=http://localhost:8080 \
CF_UI_DEV_API_BASE_URL=http://localhost:3445 \
nix develop --command web-ui-test 26e-evaluation-policy-bundle-controls
```

The workflow covers rollups, denominator consistency, state-preserving drilldown,
actual assertion detail text, horizontal overflow, sticky identification,
unavailable/N/A rows, API failure, loading, empty capture, and legacy fallback.
It does not prove resolver or persistence correctness: migration-backed backend
tests separately exercise the actual resolver/loader and historical capture.

Full visual acceptance still requires matched-viewport comparison against the
requested reference, including any supplied screenshots. Browser assertion
success and screenshot generation alone do not establish that acceptance.
