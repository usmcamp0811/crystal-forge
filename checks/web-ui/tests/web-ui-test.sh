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

print_help() {
  cat <<'EOF'
Usage:
  web-ui-test [WORKFLOW ...]
  web-ui-test --list
  web-ui-test --help

Run one or more host-compatible Web UI browser workflows against the
persistent development stack started by `run-ui-dev`. With no arguments,
every host-compatible workflow runs.

CF_UI_TEST_STEPS is also accepted for callers that already set it; supplying
both positional workflows and CF_UI_TEST_STEPS is an error.

A workflow outside the host-compatible set fails with the authoritative
NixOS command instead of being silently skipped:

    CF_UI_TEST_STEPS="<name>" \
      nix build --impure .#checks.x86_64-linux.web-ui --no-link -L

Environment overrides:
  CF_UI_DEV_BASE_URL          Dioxus dev server URL (default http://127.0.0.1:8080)
  CF_UI_DEV_API_BASE_URL      Crystal Forge server URL (default http://127.0.0.1:3445)
  CF_UI_DEV_READY_TIMEOUT     Seconds to wait for the Dioxus bundle (default 300)
  CF_UI_TEST_OUTPUT_DIR       Exact artifact directory (default a new run under .tmp/web-ui-test)
EOF
}

case "${1:-}" in
  --help|-h)
    print_help
    exit 0
    ;;
  --list)
    exec node - "$manifest" <<'NODE'
const fs = require("fs");
const manifest = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const devStack = manifest.settings.devStackWorkflows || {};
for (const [name, info] of Object.entries(devStack)) {
  if (name.startsWith("$")) continue;
  console.log(`${name}\t${info.category}`);
}
NODE
    ;;
esac

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

# `settings.devStackWorkflows` in the coverage manifest maps each
# host-compatible workflow to its category (see the manifest's own $note).
# Every other workflow needs VM-only infrastructure, so it is rejected
# instead of skipped. The selection script prints two lines: the resolved,
# de-duplicated step list, then "1" if any selected workflow is in the
# `fixture` category (needs the non-destructive DB precondition check below)
# or "0" otherwise.
selection_output="$(node - "$manifest" "$requested_workflows" <<'NODE'
const fs = require("fs");

const [manifestPath, requested] = process.argv.slice(2);
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const known = new Set(manifest.steps.map((workflow) => workflow.name));
const devStack = manifest.settings.devStackWorkflows || {};
const devStackNames = Object.keys(devStack).filter((name) => !name.startsWith("$"));
const selected = requested === ""
  ? devStackNames
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

const unsupported = selected.filter((name) => !devStackNames.includes(name));
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

const uniqueSelected = [...new Set(selected)];
const needsFixtureReset = uniqueSelected.some((name) => devStack[name].category === "fixture");
process.stdout.write(`${uniqueSelected.join(",")}\n${needsFixtureReset ? "1" : "0"}\n`);
NODE
)" || exit $?
selected_workflows="$(sed -n '1p' <<<"$selection_output")"
needs_fixture_reset="$(sed -n '2p' <<<"$selection_output")"

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

# `fixture` category workflows create rows through runFixtureSql using fresh
# randomUUID identity on every run, so repeated execution never collides and
# no destructive reset is required (see the manifest's devStackWorkflows
# note). They do assume the base run-ui-dev JSON fixture seeded at least one
# environment and one commit, because their SQL selects an arbitrary existing
# row of each to attach new fixture objects to. Failing that assumption here,
# as a clear setup error, is far more useful than the confusing downstream
# symptom: a NULL foreign key surfacing as an unrelated browser assertion
# failure deep into the run.
if [[ "$needs_fixture_reset" == "1" ]]; then
  reset_db_host="${DB_HOST:-127.0.0.1}"
  reset_db_port="${DB_PORT:-3042}"
  reset_db_user="${DB_USER:-crystal_forge}"
  reset_db_password="${DB_PASSWORD:-password}"
  reset_db_name="${DB_NAME:-crystal_forge}"
  precondition_query='SELECT (EXISTS (SELECT 1 FROM environments)) AND (EXISTS (SELECT 1 FROM commits));'
  if ! precondition_result="$(PGPASSWORD="$reset_db_password" psql \
    -h "$reset_db_host" -p "$reset_db_port" -U "$reset_db_user" -d "$reset_db_name" \
    -v ON_ERROR_STOP=1 -A -t -c "$precondition_query" 2>&1)"; then
    cat >&2 <<EOF
web-ui-test: could not verify fixture preconditions against the development
database at $reset_db_host:$reset_db_port/$reset_db_name.

$precondition_result

Start the development stack first:

    run-ui-dev
EOF
    exit 1
  fi
  if [[ "$precondition_result" != "t" ]]; then
    cat >&2 <<EOF
web-ui-test: fixture precondition not met at $reset_db_host:$reset_db_port/$reset_db_name.

The selected workflow requires at least one seeded environment and one
seeded commit. Restart the development stack to reseed the fixture data:

    run-ui-dev
EOF
    exit 1
  fi
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

# CF_UI_TEST_OUTPUT_DIR lets a caller reuse a fixed directory across
# invocations (the default run-id-suffixed path never collides, but a caller
# is free to override it). A previous invocation's results.json in that same
# directory must not be mistaken for this invocation's outcome: if the
# browser harness this time exits 0 without producing a fresh results.json
# (for example, a bug that skips the write on some early-return path), the
# stale file would otherwise still read back as a passing result and this
# command would report success for a run that never actually produced one.
# Removing it up front, before the harness runs, makes that impossible
# regardless of why the harness fails to write a new one. Only results.json
# is removed; screenshots, logs, and other prior artifacts in the directory
# are left alone since nothing else here is read to decide pass/fail.
rm -f "$output_dir/results.json"

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

# `integration-test.js` intentionally exits 0 when an individual workflow
# step fails; only a fatal setup/runtime error makes it exit nonzero. Which
# step failures are blocking is normally decided afterward by the
# authoritative NixOS driver's critical-workflow list (see
# checks/web-ui/default.nix), which tolerates some noncritical failures.
# This fast host runner has no such notion of "noncritical": every workflow
# named on the command line, or implied by the default selection, was
# explicitly requested, so any selected workflow that did not pass must fail
# this command even though Node itself exited 0. `exec` would hand our exit
# status straight to Node and skip that check, so run it as a normal command
# instead and inspect its own exit code plus the results it produced.
integration_exit_code=0
node "$integration_runner" "$base_url" "$output_dir" || integration_exit_code=$?

results_path="$output_dir/results.json"
if [[ ! -f "$results_path" ]]; then
  printf 'web-ui-test: %s was not produced; the browser harness did not complete.\n' \
    "$results_path" >&2
  # A nonzero Node exit already explains why: propagate it. A clean exit
  # with no results is itself a failure the runner must not treat as
  # success, so it still needs a nonzero status here.
  if [[ "$integration_exit_code" -ne 0 ]]; then
    exit "$integration_exit_code"
  fi
  exit 1
fi

results_ok=1
if ! node - "$results_path" "$selected_workflows" <<'NODE'
const fs = require("fs");

const [resultsPath, selectedCsv] = process.argv.slice(2);
const selected = selectedCsv.split(",").map((name) => name.trim()).filter(Boolean);

let results;
try {
  results = JSON.parse(fs.readFileSync(resultsPath, "utf8"));
} catch (err) {
  console.error(`web-ui-test: could not parse ${resultsPath}: ${err.message}`);
  process.exit(1);
}
if (!Array.isArray(results)) {
  console.error(`web-ui-test: ${resultsPath} did not contain a JSON array of step results.`);
  process.exit(1);
}

const byName = new Map(results.filter((r) => r && typeof r.name === "string").map((r) => [r.name, r]));
let failed = false;
for (const name of selected) {
  const result = byName.get(name);
  if (!result) {
    console.error(`web-ui-test: no result recorded for selected workflow: ${name}`);
    failed = true;
    continue;
  }
  if (result.ok !== true) {
    console.error(`web-ui-test: workflow failed: ${name} - ${result.error || "no error message recorded"}`);
    failed = true;
  }
}
process.exit(failed ? 1 : 0);
NODE
then
  results_ok=0
fi

if [[ "$integration_exit_code" -ne 0 ]]; then
  exit "$integration_exit_code"
fi
if [[ "$results_ok" -ne 1 ]]; then
  exit 1
fi
exit 0
