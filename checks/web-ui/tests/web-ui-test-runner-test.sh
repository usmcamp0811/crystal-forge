#!/usr/bin/env bash

# Regression coverage for the host-side web-ui-test runner.
#
# The browser harness itself is not exercised here. These cases prove workflow
# selection, development-stack readiness reporting, artifact creation, and
# exit-status propagation, which is the contract the runner adds around the
# existing integration test.

set -euo pipefail

runner="${1:?runner path is required}"
source_manifest="${2:?coverage manifest path is required}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

project="$tmp_dir/project"
mock_bin="$tmp_dir/bin"
mkdir -p "$project/checks/web-ui/tests" "$mock_bin"
cp "$source_manifest" "$project/checks/web-ui/coverage-manifest.json"

cat >"$project/checks/web-ui/tests/fake-integration-test.js" <<'NODE'
const fs = require("fs");

fs.writeFileSync(
  process.env.TEST_LOG,
  JSON.stringify({
    args: process.argv.slice(2),
    outputExists: fs.existsSync(process.argv[3]),
    steps: process.env.CF_UI_TEST_STEPS,
  }),
);
process.exit(Number(process.env.MOCK_RUNNER_STATUS || 0));
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

export PATH="$mock_bin:$PATH"
export PROJECT_ROOT="$project"
export CF_WEB_UI_INTEGRATION_RUNNER="$project/checks/web-ui/tests/fake-integration-test.js"
export CF_UI_DEV_READY_TIMEOUT=0

run_success() {
  local case_name="$1"
  shift
  TEST_LOG="$tmp_dir/$case_name.json" \
    CF_UI_TEST_OUTPUT_DIR="$tmp_dir/$case_name-output" \
    bash "$runner" "$@" >"$tmp_dir/$case_name.stdout" 2>"$tmp_dir/$case_name.stderr"
}

run_failure() {
  local case_name="$1"
  shift
  if TEST_LOG="$tmp_dir/$case_name.json" \
    CF_UI_TEST_OUTPUT_DIR="$tmp_dir/$case_name-output" \
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

run_success positional 12-systems
assert_contains "$tmp_dir/positional.json" '"steps":"12-systems"'
assert_contains "$tmp_dir/positional.json" '"outputExists":true'
assert_contains "$tmp_dir/positional.stdout" 'Selected workflows: 12-systems'

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

MOCK_FRONTEND_STATUS=1 run_failure missing-frontend 12-systems
assert_contains "$tmp_dir/missing-frontend.stderr" 'Web UI is not reachable'

MOCK_SERVER_STATUS=1 run_failure missing-server 12-systems
assert_contains "$tmp_dir/missing-server.stderr" 'server is not reachable'

MOCK_BUNDLE_STATUS=1 run_failure missing-bundle 12-systems
assert_contains "$tmp_dir/missing-bundle.stderr" 'did not serve a built application bundle'

run_success default-selection
assert_contains "$tmp_dir/default-selection.json" '"steps":"12-systems,12a-systems-empty-state"'

if TEST_LOG="$tmp_dir/failing-runner.json" \
  CF_UI_TEST_OUTPUT_DIR="$tmp_dir/failing-runner-output" \
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
