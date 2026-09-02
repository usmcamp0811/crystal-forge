#!/usr/bin/env bash

# Regression coverage for the host-side web-ui-test runner.
#
# The browser harness itself is not exercised here. These cases prove workflow
# selection, development-stack readiness reporting, fixture-precondition
# reset behavior, artifact creation, and exit-status propagation, which is
# the contract the runner adds around the existing integration test.

set -euo pipefail

runner="${1:?runner path is required}"
source_manifest="${2:?coverage manifest path is required}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

project="$tmp_dir/project"
mock_bin="$tmp_dir/bin"
mkdir -p "$project/checks/web-ui/tests" "$mock_bin"

# Build an isolated test manifest: the real coverage-manifest.json (so
# unknown-workflow checks exercise the real 151-workflow step list), with a
# synthetic `fixture`-category entry added under a name ("13-flakes") that is
# not part of the real devStackWorkflows set. This keeps the fixture-category
# test path independent of which workflows the repository currently
# classifies as host-compatible.
node - "$source_manifest" "$project/checks/web-ui/coverage-manifest.json" <<'NODE'
const fs = require("fs");
const [srcPath, destPath] = process.argv.slice(2);
const manifest = JSON.parse(fs.readFileSync(srcPath, "utf8"));
manifest.settings.devStackWorkflows["13-flakes"] = { category: "fixture" };
fs.writeFileSync(destPath, JSON.stringify(manifest));
NODE

cat >"$project/checks/web-ui/tests/fake-integration-test.js" <<'NODE'
const fs = require("fs");
const path = require("path");

const outputDir = process.argv[3];
const steps = (process.env.CF_UI_TEST_STEPS || "")
  .split(",")
  .map((name) => name.trim())
  .filter(Boolean);

fs.writeFileSync(
  process.env.TEST_LOG,
  JSON.stringify({
    args: process.argv.slice(2),
    outputExists: fs.existsSync(outputDir),
    steps: process.env.CF_UI_TEST_STEPS,
  }),
);

const status = Number(process.env.MOCK_RUNNER_STATUS || 0);
// Mirror the real harness: a fatal setup/runtime error (nonzero exit) never
// produces results.json (see integration-test.js's top-level .catch), while
// a normal completion always writes one, whether or not individual steps
// passed. MOCK_SKIP_RESULTS and MOCK_RESULTS_MALFORMED simulate the two
// distinct "results.json missing or unusable despite a clean exit" cases the
// runner must also reject.
if (status === 0 && process.env.MOCK_SKIP_RESULTS !== "1") {
  fs.mkdirSync(outputDir, { recursive: true });
  const resultsPath = path.join(outputDir, "results.json");
  if (process.env.MOCK_RESULTS_MALFORMED === "1") {
    fs.writeFileSync(resultsPath, "{ not valid json");
  } else {
    const ok = process.env.MOCK_STEP_OK !== "0";
    const results = steps.map((name) => ({
      name,
      description: `mock ${name}`,
      ok,
      error: ok ? null : "expected systems page assertion failure",
      visuals: [],
    }));
    fs.writeFileSync(resultsPath, JSON.stringify(results, null, 2));
  }
}
process.exit(status);
NODE

# Stands in for the development stack. The application-bundle responses match
# the shape the Dioxus development server serves once its build completes.
# The interpreter is resolved from PATH because `/usr/bin/env` does not exist
# inside the Nix build sandbox that runs this script as a check.
printf '#!%s\n' "$(command -v bash)" >"$mock_bin/curl"
cat >>"$mock_bin/curl" <<'SH'

case "$*" in
  *crystal-forge-ui.js*)
    [[ "${MOCK_BUNDLE_STATUS:-0}" -eq 0 ]] || exit 1
    printf 'import init from "./crystal-forge-ui_bg.wasm";\n'
    ;;
  *crystal-forge-ui_bg.wasm*)
    [[ "${MOCK_BUNDLE_STATUS:-0}" -eq 0 ]] || exit 1
    printf '\0asm'
    ;;
  *:3445/status*)
    exit "${MOCK_SERVER_STATUS:-0}"
    ;;
  *:8080/*)
    [[ "${MOCK_FRONTEND_STATUS:-0}" -eq 0 ]] || exit 1
    printf '<html><body><script src="/./wasm/crystal-forge-ui.js"></script></body></html>\n'
    ;;
  *)
    exit 1
    ;;
esac
SH
chmod +x "$mock_bin/curl"

# Stands in for the fixture-precondition check `psql` invocation. Records the
# exact connection arguments it was called with so tests can assert the
# runner reused the devshell's DB_HOST/DB_PORT/DB_USER/DB_NAME convention
# instead of hard-coding a second configuration.
printf '#!%s\n' "$(command -v bash)" >"$mock_bin/psql"
cat >>"$mock_bin/psql" <<'SH'

printf '%s\n' "$*" >>"$PSQL_CALL_LOG"
if [[ "${MOCK_PSQL_STATUS:-0}" -ne 0 ]]; then
  echo "mock psql connection failure" >&2
  exit "${MOCK_PSQL_STATUS}"
fi
printf '%s' "${MOCK_PSQL_RESULT:-t}"
SH
chmod +x "$mock_bin/psql"

export PATH="$mock_bin:$PATH"
export PROJECT_ROOT="$project"
export CF_WEB_UI_INTEGRATION_RUNNER="$project/checks/web-ui/tests/fake-integration-test.js"
export CF_UI_DEV_READY_TIMEOUT=0

run_success() {
  local case_name="$1"
  shift
  TEST_LOG="$tmp_dir/$case_name.json" \
    CF_UI_TEST_OUTPUT_DIR="$tmp_dir/$case_name-output" \
    PSQL_CALL_LOG="$tmp_dir/$case_name.psql-calls" \
    bash "$runner" "$@" >"$tmp_dir/$case_name.stdout" 2>"$tmp_dir/$case_name.stderr"
}

run_failure() {
  local case_name="$1"
  shift
  if TEST_LOG="$tmp_dir/$case_name.json" \
    CF_UI_TEST_OUTPUT_DIR="$tmp_dir/$case_name-output" \
    PSQL_CALL_LOG="$tmp_dir/$case_name.psql-calls" \
    bash "$runner" "$@" >"$tmp_dir/$case_name.stdout" 2>"$tmp_dir/$case_name.stderr"; then
    printf 'Expected %s to fail.\n' "$case_name" >&2
    exit 1
  fi
}

assert_contains() {
  local file="$1"
  local expected="$2"
  if [[ "$(<"$file")" != *"$expected"* ]]; then
    printf 'Expected %s to contain: %s\n' "$file" "$expected" >&2
    exit 1
  fi
}

assert_absent() {
  local file="$1"
  if [[ -e "$file" ]]; then
    printf 'Expected %s not to exist.\n' "$file" >&2
    exit 1
  fi
}

# ── Basic selection ─────────────────────────────────────────────────────────

run_success positional 12-systems
assert_contains "$tmp_dir/positional.json" '"steps":"12-systems"'
assert_contains "$tmp_dir/positional.json" '"outputExists":true'
assert_contains "$tmp_dir/positional.stdout" 'Selected workflows: 12-systems'
assert_absent "$tmp_dir/positional.psql-calls"

run_success multiple 12-systems 12a-systems-empty-state
assert_contains "$tmp_dir/multiple.json" '"steps":"12-systems,12a-systems-empty-state"'

CF_UI_TEST_STEPS=12-systems run_success environment
assert_contains "$tmp_dir/environment.json" '"steps":"12-systems"'

CF_UI_TEST_STEPS=12-systems run_failure conflict 12-systems
assert_contains "$tmp_dir/conflict.stderr" 'cannot be used together'

CF_UI_TEST_STEPS='' run_failure empty-environment
assert_contains "$tmp_dir/empty-environment.stderr" 'must not be empty'

run_failure invalid does-not-exist
assert_contains "$tmp_dir/invalid.stderr" 'unknown workflow: does-not-exist'

run_failure vm-only 01-login-page
assert_contains "$tmp_dir/vm-only.stderr" 'requires the authoritative NixOS VM harness'
assert_contains "$tmp_dir/vm-only.stderr" 'nix build --impure .#checks.x86_64-linux.web-ui'

# The isolated test manifest adds a third (fixture-category) workflow, so a
# no-argument selection here covers all three, including the precondition
# check for the fixture-category member.
run_success default-selection
assert_contains "$tmp_dir/default-selection.json" '"steps":"12-systems,12a-systems-empty-state,13-flakes"'
assert_contains "$tmp_dir/default-selection.psql-calls" "-h 127.0.0.1 -p 3042 -U crystal_forge -d crystal_forge"

# ── --list / --help ──────────────────────────────────────────────────────────

bash "$runner" --list >"$tmp_dir/list.stdout" 2>"$tmp_dir/list.stderr"
assert_contains "$tmp_dir/list.stdout" '12-systems	mock'
assert_contains "$tmp_dir/list.stdout" '13-flakes	fixture'

bash "$runner" --help >"$tmp_dir/help.stdout" 2>"$tmp_dir/help.stderr"
assert_contains "$tmp_dir/help.stdout" 'Usage:'

# ── Readiness failures ───────────────────────────────────────────────────────

MOCK_FRONTEND_STATUS=1 run_failure missing-frontend 12-systems
assert_contains "$tmp_dir/missing-frontend.stderr" 'Web UI is not reachable'

MOCK_SERVER_STATUS=1 run_failure missing-server 12-systems
assert_contains "$tmp_dir/missing-server.stderr" 'server is not reachable'

MOCK_BUNDLE_STATUS=1 run_failure missing-bundle 12-systems
assert_contains "$tmp_dir/missing-bundle.stderr" 'did not serve a built application bundle'

# ── Fixture-category precondition (reset) behavior ──────────────────────────
#
# "13-flakes" is marked `fixture` only in this test's isolated manifest copy
# (see above); its real manifest classification is unrelated.

run_success fixture-precondition-ok 13-flakes
assert_contains "$tmp_dir/fixture-precondition-ok.psql-calls" "-h 127.0.0.1 -p 3042 -U crystal_forge -d crystal_forge"
assert_contains "$tmp_dir/fixture-precondition-ok.json" '"steps":"13-flakes"'

DB_HOST=10.1.2.3 DB_PORT=6543 DB_USER=custom_user DB_NAME=custom_db \
  run_success fixture-precondition-custom-db 13-flakes
assert_contains "$tmp_dir/fixture-precondition-custom-db.psql-calls" \
  "-h 10.1.2.3 -p 6543 -U custom_user -d custom_db"

MOCK_PSQL_RESULT=f run_failure fixture-precondition-empty 13-flakes
assert_contains "$tmp_dir/fixture-precondition-empty.stderr" 'fixture precondition not met'
assert_contains "$tmp_dir/fixture-precondition-empty.stderr" 'run-ui-dev'

MOCK_PSQL_STATUS=2 run_failure fixture-precondition-unreachable 13-flakes
assert_contains "$tmp_dir/fixture-precondition-unreachable.stderr" \
  'could not verify fixture preconditions'

# A `mock`-category selection must not touch the database at all.
run_success mock-skips-precondition 12-systems
assert_absent "$tmp_dir/mock-skips-precondition.psql-calls"

# ── Repeated execution and failure recovery ──────────────────────────────────
#
# Each invocation is an independent process (fresh browser, fresh precondition
# check), so a prior failure must not affect a later, unrelated invocation.

MOCK_SERVER_STATUS=1 run_failure repeat-then-recover-1 12-systems
run_success repeat-then-recover-2 12-systems
assert_contains "$tmp_dir/repeat-then-recover-2.json" '"steps":"12-systems"'

run_success repeat-a 13-flakes
run_success repeat-b 13-flakes
assert_contains "$tmp_dir/repeat-a.json" '"steps":"13-flakes"'
assert_contains "$tmp_dir/repeat-b.json" '"steps":"13-flakes"'

# ── results.json is authoritative, not the Node exit code ──────────────────
#
# integration-test.js intentionally exits 0 when a step's assertions fail;
# only fatal setup/runtime errors make it exit nonzero. web-ui-test must
# still fail when a workflow it explicitly ran did not pass.

MOCK_STEP_OK=0 run_failure failing-step-result 12-systems
assert_contains "$tmp_dir/failing-step-result.stderr" \
  'workflow failed: 12-systems - expected systems page assertion failure'

# A genuinely successful workflow (the common case exercised throughout this
# file already) must still exit 0 now that results.json is inspected.
run_success genuinely-successful 12-systems
assert_contains "$tmp_dir/genuinely-successful.json" '"steps":"12-systems"'

MOCK_SKIP_RESULTS=1 run_failure missing-results 12-systems
assert_contains "$tmp_dir/missing-results.stderr" 'was not produced'

MOCK_RESULTS_MALFORMED=1 run_failure malformed-results 12-systems
assert_contains "$tmp_dir/malformed-results.stderr" 'could not parse'

# ── Browser-runner exit-status propagation ───────────────────────────────────

if TEST_LOG="$tmp_dir/failing-runner.json" \
  CF_UI_TEST_OUTPUT_DIR="$tmp_dir/failing-runner-output" \
  PSQL_CALL_LOG="$tmp_dir/failing-runner.psql-calls" \
  MOCK_RUNNER_STATUS=23 bash "$runner" 12-systems >/dev/null 2>&1; then
  printf 'Expected the integration runner failure to propagate.\n' >&2
  exit 1
else
  status=$?
  if [[ $status -ne 23 ]]; then
    printf 'Expected exit status 23, got %s.\n' "$status" >&2
    exit 1
  fi
fi

printf 'web-ui-test runner tests passed.\n'
