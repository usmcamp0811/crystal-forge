#!/usr/bin/env bash
# Unit test for prebuilt-cargo-test.sh (TASK-542.1). Uses a synthetic
# manifest and shell-script executables; no Rust, Nix store or database.
# Usage: bash prebuilt-cargo-test-test.sh <path-to-prebuilt-cargo-test.sh>
set -euo pipefail
subject=$(realpath "$1")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export PROJECT_ROOT="$work/project"
root="$PROJECT_ROOT/packages/default"
prebuilt="$work/prebuilt"
log="$work/calls"
mkdir -p "$root/crates/cf-server" "$root/crates/cf-agent" "$root/crates/cf-keygen" "$prebuilt/bin"
touch "$root/Cargo.toml"
fake() {
  cat > "$prebuilt/bin/$1" <<SCRIPT
#!/usr/bin/env bash
printf '%s|%s|%s|%s\n' "$1" "\$PWD" "\$CARGO_MANIFEST_DIR" "\$*" >> "$log"
echo "test example ... ok"
exit ${2:-0}
SCRIPT
  chmod +x "$prebuilt/bin/$1"
}
fake cf-server--lib--crystal_forge
fake cf-agent--lib--cf_agent
fake cf-server--test--mcp_tools
fake cf-server--test--broken 101
printf '%s\t%s\t%s\t%s\t%s\n' \
  cf-agent lib cf_agent crates/cf-agent bin/cf-agent--lib--cf_agent \
  cf-keygen bin-only - crates/cf-keygen - \
  cf-server lib crystal_forge crates/cf-server bin/cf-server--lib--crystal_forge \
  cf-server test broken crates/cf-server bin/cf-server--test--broken \
  cf-server test mcp_tools crates/cf-server bin/cf-server--test--mcp_tools \
  > "$prebuilt/manifest.tsv"
# shellcheck source=/dev/null
source "$subject"
export CF_PREBUILT_TEST_EXECUTABLES="$prebuilt"
cd "$root"
failures=0
expect() {
  local description=$1 expected_status=$2 expected_calls=$3 status=0
  shift 3
  : > "$log"
  cf_cargo "$@" >/dev/null 2>"$work/stderr" || status=$?
  local calls
  calls=$(cat "$log")
  if [[ "$status" != "$expected_status" || "$calls" != "$expected_calls" ]]; then
    echo "FAIL $description: status=$status calls=[$calls]" >&2
    cat "$work/stderr" >&2
    failures=$((failures + 1))
  else
    echo "PASS $description"
  fi
}
server="$root/crates/cf-server"
agent="$root/crates/cf-agent"
expect "package lib selection with filter and libtest arguments" 0 \
  "cf-server--lib--crystal_forge|$server|$server|queries::x --ignored --test-threads=1" \
  test --manifest-path Cargo.toml -p cf-server --lib queries::x -- --ignored --test-threads=1
expect "workspace lib selection runs every library" 0 \
  "cf-agent--lib--cf_agent|$agent|$agent|scan
cf-server--lib--crystal_forge|$server|$server|scan" \
  test --manifest-path Cargo.toml --lib scan
expect "integration target" 0 \
  "cf-server--test--mcp_tools|$server|$server|--test-threads=1" \
  test --offline --package cf-server --test mcp_tools -- --test-threads=1
expect "listing passes --list through" 0 \
  "cf-server--test--mcp_tools|$server|$server|--ignored --list" \
  test -p cf-server --test mcp_tools -- --ignored --list
expect "no-run executes nothing" 0 "" test -p cf-server --test mcp_tools --no-run
expect "failing target stops later targets" 101 \
  "cf-server--test--broken|$server|$server|" \
  test -p cf-server --test broken --test mcp_tools
expect "unknown test target" 2 "" test -p cf-server --test missing
expect "unknown option" 2 "" test -p cf-server --lib --release
expect "no target kind" 2 "" test -p cf-server
expect "foreign manifest" 2 "" test --manifest-path /elsewhere/Cargo.toml --lib
expect "explicit package without library" 2 "" test -p cf-keygen --lib
expect "second positional filter" 2 "" test -p cf-server --lib one two
cat > "$work/runner" <<SCRIPT
#!/usr/bin/env bash
printf 'runner|%s\n' "\$*" >> "$log"
SCRIPT
chmod +x "$work/runner"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER="$work/runner --flag"
expect "target runner wraps the executable" 0 \
  "runner|--flag $prebuilt/bin/cf-server--test--mcp_tools" \
  test -p cf-server --test mcp_tools
unset CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER
rm "$prebuilt/bin/cf-server--test--mcp_tools"
expect "missing executable fails closed" 2 "" test -p cf-server --test mcp_tools --no-run
mv "$prebuilt/manifest.tsv" "$work/manifest.tsv"
expect "missing manifest fails closed" 2 "" test -p cf-server --lib
mkdir -p "$work/path"
cat > "$work/path/cargo" <<SCRIPT
#!/usr/bin/env bash
printf 'cargo|%s\n' "\$*" >> "$log"
SCRIPT
chmod +x "$work/path/cargo"
PATH="$work/path:$PATH"
expect "non-test subcommands use Cargo" 0 "cargo|sqlx migrate run" sqlx migrate run
unset CF_PREBUILT_TEST_EXECUTABLES
expect "unset variable keeps the Cargo path" 0 "cargo|test -p cf-server --lib" \
  test -p cf-server --lib
if (( failures )); then
  echo "$failures prebuilt cargo test case(s) failed" >&2
  exit 1
fi
echo "PASS prebuilt-cargo-test"
