#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
approval_script="$script_dir/../approve-baselines.sh"
fixture_root=$(mktemp -d)
trap 'rm -rf "$fixture_root"' EXIT

mkdir -p "$fixture_root/checks/web-ui/baselines" "$fixture_root/screenshots"
cp -- "$approval_script" "$fixture_root/checks/web-ui/approve-baselines.sh"

write_report() {
  local step_ok=$1
  local visuals=$2
  local semantic_assertions=${3:-true}
  printf '{"steps":[{"name":"semantic-step","semanticAssertions":%s,"ok":%s,"visuals":%s}]}\n' \
    "$semantic_assertions" "$step_ok" "$visuals" > "$fixture_root/screenshots/visual-report.json"
}

assert_rejected() {
  if "$fixture_root/checks/web-ui/approve-baselines.sh" "$fixture_root/screenshots" >/dev/null 2>&1; then
    printf 'expected approval fixture to be rejected: %s\n' "$1" >&2
    exit 1
  fi
}

printf 'png' > "$fixture_root/screenshots/strict-capture.png"
write_report true '[{"name":"strict-capture","policy":"strict","diagnostic":false}]'
"$fixture_root/checks/web-ui/approve-baselines.sh" "$fixture_root/screenshots" >/dev/null
test -f "$fixture_root/checks/web-ui/baselines/strict-capture.png"

# Captures copied from a Nix result are read-only. A later approval must replace
# the existing file and leave the repository copy writable.
chmod 0444 "$fixture_root/checks/web-ui/baselines/strict-capture.png"
printf 'updated-png' > "$fixture_root/screenshots/strict-capture.png"
"$fixture_root/checks/web-ui/approve-baselines.sh" "$fixture_root/screenshots" >/dev/null
test "$(<"$fixture_root/checks/web-ui/baselines/strict-capture.png")" = 'updated-png'
test -w "$fixture_root/checks/web-ui/baselines/strict-capture.png"

write_report true '[]'
assert_rejected "no strict captures"
write_report false '[{"name":"strict-capture","policy":"strict"}]'
assert_rejected "failed owning semantic step"
write_report true '[{"name":"strict-capture","policy":"strict"}]' false
assert_rejected "non-semantic owning step"
write_report true '[{"name":"../escape","policy":"strict"}]'
assert_rejected "path traversal"
write_report true '[{"name":"strict\u000acapture","policy":"strict"}]'
assert_rejected "control character"
write_report true '[{"name":"strict-capture","policy":"strict"},{"name":"strict-capture","policy":"strict"}]'
assert_rejected "duplicate capture"

write_report true '[{"name":"strict-capture","policy":"strict"}]'
rm -f "$fixture_root/screenshots/strict-capture.png"
printf 'outside' > "$fixture_root/outside.png"
ln -s "$fixture_root/outside.png" "$fixture_root/screenshots/strict-capture.png"
assert_rejected "source symlink escape"

rm -f "$fixture_root/screenshots/strict-capture.png" "$fixture_root/screenshots/visual-report.json"
printf 'png' > "$fixture_root/screenshots/strict-capture.png"
write_report true '[{"name":"strict-capture","policy":"strict"}]'
mv "$fixture_root/screenshots/visual-report.json" "$fixture_root/outside-report.json"
ln -s "$fixture_root/outside-report.json" "$fixture_root/screenshots/visual-report.json"
assert_rejected "report symlink escape"

rm -f "$fixture_root/screenshots/visual-report.json" "$fixture_root/checks/web-ui/baselines/strict-capture.png"
cp -- "$fixture_root/outside-report.json" "$fixture_root/screenshots/visual-report.json"
ln -s "$fixture_root/outside.png" "$fixture_root/checks/web-ui/baselines/strict-capture.png"
assert_rejected "destination symlink escape"

printf 'approve-baselines fixtures passed\n'
