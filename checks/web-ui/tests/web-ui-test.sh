#!/usr/bin/env bash

# Fast host-side runner for the Web UI browser harness.
#
# This runner executes the same `integration-test.js` implementation that the
# authoritative NixOS `web-ui` check runs, but against the persistent
# development stack started by `run-ui-dev`. It never starts, stops, or
# restarts PostgreSQL, the Crystal Forge server, or the Dioxus development
# server, and it never builds a NixOS VM or test driver.
#
# The authoritative NixOS check remains the reproducible verification
# boundary. This runner exists for implementation feedback only.

set -euo pipefail

# Workflow selection accepts either positional names or the harness variable
# `CF_UI_TEST_STEPS`. Supplying both is rejected because the two inputs would
# otherwise silently disagree about which workflows run.
if [[ $# -gt 0 && -n "${CF_UI_TEST_STEPS+x}" ]]; then
  printf '%s\n' \
    "web-ui-test: positional workflows and CF_UI_TEST_STEPS cannot be used together." >&2
  exit 2
fi

if [[ $# -gt 0 ]]; then
  for workflow in "$@"; do
    if [[ -z "$workflow" || "$workflow" == *,* ]]; then
      printf '%s\n' \
        "web-ui-test: pass each non-empty workflow as a separate argument." >&2
      exit 2
    fi
  done
  requested_workflows="$(IFS=,; printf '%s' "$*")"
elif [[ -n "${CF_UI_TEST_STEPS+x}" ]]; then
  if [[ -z "$CF_UI_TEST_STEPS" ]]; then
    printf '%s\n' "web-ui-test: CF_UI_TEST_STEPS must not be empty." >&2
    exit 2
  fi
  requested_workflows="$CF_UI_TEST_STEPS"
else
  requested_workflows=""
fi

PROJECT_ROOT="${PROJECT_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || true)}"
if [[ -z "$PROJECT_ROOT" ]]; then
  printf '%s\n' \
    "web-ui-test: run this command from a Crystal Forge worktree." >&2
  exit 2
fi

manifest="$PROJECT_ROOT/checks/web-ui/coverage-manifest.json"
integration_runner="${CF_WEB_UI_INTEGRATION_RUNNER:-$PROJECT_ROOT/checks/web-ui/tests/integration-test.js}"

if [[ ! -f "$manifest" || ! -f "$integration_runner" ]]; then
  printf '%s\n' \
    "web-ui-test: the Web UI test harness was not found under $PROJECT_ROOT." >&2
  exit 2
fi

# `settings.devStackWorkflows` in the coverage manifest lists the workflows
# that are known to run correctly and repeatably against the development
# stack. Every other workflow needs VM-only infrastructure, so it is rejected
# instead of skipped.
selected_workflows="$({ node - "$manifest" "$requested_workflows" <<'NODE'
const fs = require("fs");

const [manifestPath, requested] = process.argv.slice(2);
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const known = new Set(manifest.steps.map((workflow) => workflow.name));
const devStack = manifest.settings.devStackWorkflows || [];
const selected = requested === ""
  ? devStack
  : requested.split(",").map((name) => name.trim()).filter(Boolean);

if (selected.length === 0) {
  console.error("web-ui-test: no host-compatible workflows are configured.");
  process.exit(2);
}

const unknown = selected.filter((name) => !known.has(name));
if (unknown.length > 0) {
  console.error(`web-ui-test: unknown workflow: ${unknown.join(", ")}`);
  process.exit(2);
}

const unsupported = selected.filter((name) => !devStack.includes(name));
if (unsupported.length > 0) {
  console.error(
    `Workflow ${unsupported.join(", ")} requires the authoritative NixOS VM harness.`,
  );
  console.error("");
  console.error("Run:");
  console.error("");
  console.error(`    CF_UI_TEST_STEPS="${unsupported.join(",")}" \\`);
  console.error("      nix build --impure .#checks.x86_64-linux.web-ui --no-link -L");
  process.exit(2);
}

process.stdout.write([...new Set(selected)].join(","));
NODE
  })" || exit $?

base_url="${CF_UI_DEV_BASE_URL:-http://127.0.0.1:8080}"
api_url="${CF_UI_DEV_API_BASE_URL:-http://127.0.0.1:3445}"

fetch() {
  curl --fail --silent --show-error --max-time 10 "$@"
}

if ! fetch --max-time 2 "$base_url/" >/dev/null 2>&1; then
  cat >&2 <<EOF
Crystal Forge Web UI is not reachable at $base_url.

Start the development stack first:

    run-ui-dev
EOF
  exit 1
fi

if ! fetch --max-time 2 "$api_url/status" >/dev/null 2>&1; then
  cat >&2 <<EOF
Crystal Forge server is not reachable at $api_url/status.

Start the development stack first:

    run-ui-dev
EOF
  exit 1
fi

# The Dioxus development server answers HTTP before its first WebAssembly
# build finishes, and it then serves an application shell that never becomes
# interactive. Browser steps would fail with an unrelated navigation timeout.
# Wait for the served application bundle itself, which is the real readiness
# signal, instead of pausing for a fixed interval.
bundle_ready() {
  local html js_path js_url js wasm_name wasm_url magic

  html="$(fetch "$base_url/" 2>/dev/null || true)"
  js_path="$(printf '%s' "$html" | grep -o 'src="[^"]*\.js"' | head -n 1 |
    sed -e 's/^src="//' -e 's/"$//')"
  [[ -n "$js_path" ]] || return 1

  case "$js_path" in
    http*) js_url="$js_path" ;;
    /*) js_url="$base_url$js_path" ;;
    *) js_url="$base_url/$js_path" ;;
  esac

  js="$(fetch "$js_url" 2>/dev/null || true)"
  [[ -n "$js" ]] || return 1

  wasm_name="$(printf '%s' "$js" | grep -o '[A-Za-z0-9._-]*\.wasm' | head -n 1)"
  [[ -n "$wasm_name" ]] || return 1
  wasm_url="$(dirname "$js_url")/$wasm_name"

  # A WebAssembly module always starts with the four-byte `\0asm` magic
  # header. Remove the leading NUL byte before the shell reads the response,
  # then compare the remaining three bytes.
  magic="$(fetch --range 0-3 "$wasm_url" 2>/dev/null | tr -d '\0' || true)"
  [[ "${magic:0:3}" == "asm" ]]
}

ready_timeout="${CF_UI_DEV_READY_TIMEOUT:-300}"
ready_deadline=$(( $(date +%s) + ready_timeout ))
announced_wait=0
while ! bundle_ready; do
  if [[ "$(date +%s)" -ge "$ready_deadline" ]]; then
    cat >&2 <<EOF
Crystal Forge Web UI at $base_url did not serve a built application bundle
within ${ready_timeout}s.

The Dioxus development server answers HTTP before its first build completes.
Wait for run-ui-dev to report a finished build, then run this command again.
EOF
    exit 1
  fi
  if [[ "$announced_wait" -eq 0 ]]; then
    printf 'Waiting for the Dioxus development build at %s...\n' "$base_url"
    announced_wait=1
  fi
  sleep 2
done

output_root="${CF_UI_TEST_OUTPUT_ROOT:-$PROJECT_ROOT/.tmp/web-ui-test}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
output_dir="${CF_UI_TEST_OUTPUT_DIR:-$output_root/$run_id}"
mkdir -p "$output_dir"

# Browser pages load from the Dioxus development server, while harness-side
# requests and the bootstrapped local account belong to the Crystal Forge
# server that run-ui-dev starts.
export CF_UI_TEST_STEPS="$selected_workflows"
export CF_UI_API_BASE_URL="${CF_UI_API_BASE_URL:-$api_url}"
export CF_UI_TEST_USERNAME="${CF_UI_TEST_USERNAME:-admin}"
export CF_UI_TEST_PASSWORD="${CF_UI_TEST_PASSWORD:-password}"
export CF_UI_TEST_EMAIL="${CF_UI_TEST_EMAIL:-admin@crystal-forge.local}"
# The full profile keeps focused selection independent of CI profile
# membership. Design-parity capture is a reporting pass, not an assertion, so
# it stays disabled for the development loop.
export CF_UI_TEST_PROFILE="${CF_UI_TEST_PROFILE:-full}"
export CF_UI_SKIP_DESIGN_PARITY="${CF_UI_SKIP_DESIGN_PARITY:-1}"

printf 'Web UI base URL: %s\n' "$base_url"
printf 'Selected workflows: %s\n' "$CF_UI_TEST_STEPS"
printf 'Output directory: %s\n' "$output_dir"

exec node "$integration_runner" "$base_url" "$output_dir"
