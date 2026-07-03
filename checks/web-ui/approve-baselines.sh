#!/usr/bin/env bash
# Approve web-ui check screenshots as golden baselines.
#
# Usage:
#   ./approve-baselines.sh <screenshots-dir> [step-name ...]
#
#   <screenshots-dir>  Directory containing freshly captured <step>.png files,
#                      e.g. ./result/screenshots from
#                      `nix build .#checks.x86_64-linux.web-ui` or an unpacked
#                      `web-ui-screenshots` CI artifact.
#   [step-name ...]    Optional list of step names to approve. When omitted,
#                      every PNG that matches a step in coverage-manifest.json
#                      is approved.
#
# The script only copies screenshots whose names exist as steps in
# coverage-manifest.json (reports, diffs, and export screenshots are skipped).
# Review `git diff --stat checks/web-ui/baselines` before committing.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
manifest="$script_dir/coverage-manifest.json"
baselines="$script_dir/baselines"

src="${1:-}"
if [ -z "$src" ] || [ ! -d "$src" ]; then
  echo "usage: $0 <screenshots-dir> [step-name ...]" >&2
  exit 1
fi
shift

mapfile -t manifest_steps < <(
  node -e '
    const m = require(process.argv[1]);
    for (const s of m.steps) console.log(s.name);
  ' "$manifest"
)

is_step() {
  local name="$1"
  for s in "${manifest_steps[@]}"; do
    [ "$s" = "$name" ] && return 0
  done
  return 1
}

approved=0
skipped=0

if [ "$#" -gt 0 ]; then
  names=("$@")
else
  names=()
  for f in "$src"/*.png; do
    [ -e "$f" ] || continue
    names+=("$(basename "$f" .png)")
  done
fi

mkdir -p "$baselines"
for name in "${names[@]}"; do
  if ! is_step "$name"; then
    echo "skip (not a manifest step): $name"
    skipped=$((skipped + 1))
    continue
  fi
  if [ ! -f "$src/$name.png" ]; then
    echo "skip (no screenshot in $src): $name" >&2
    skipped=$((skipped + 1))
    continue
  fi
  cp "$src/$name.png" "$baselines/$name.png"
  echo "approved: $name"
  approved=$((approved + 1))
done

echo ""
echo "Approved $approved baseline(s), skipped $skipped."
echo "Review with: git diff --stat -- checks/web-ui/baselines"
