#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  printf 'usage: %s <web-ui screenshots directory>\n' "$0" >&2
  exit 2
fi

source_dir=${1%/}
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)

if [[ ! -d "$source_dir" ]]; then
  printf 'screenshots directory not found: %s\n' "$source_dir" >&2
  exit 1
fi

source_root=$(cd -- "$source_dir" && pwd -P)
report="$source_root/visual-report.json"
destination="$script_dir/baselines"

if [[ ! -f "$report" ]]; then
  printf 'visual report not found: %s\n' "$report" >&2
  exit 1
fi
resolved_report=$(realpath -e -- "$report")
case "$resolved_report" in
  "$source_root"/*) ;;
  *) printf 'visual report resolves outside screenshots root: %s\n' "$report" >&2; exit 1 ;;
esac

mkdir -p "$destination"
destination_root=$(cd -- "$destination" && pwd -P)
captures=$(node - "$resolved_report" <<'NODE'
const fs = require("fs");
const report = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
if (!Array.isArray(report.steps)) throw new Error("visual report steps must be an array");
const names = new Set();
for (const [stepIndex, step] of report.steps.entries()) {
  if (!step || typeof step !== "object" || !Array.isArray(step.visuals)) {
    throw new Error(`visual report step ${stepIndex} is invalid`);
  }
  for (const visual of step.visuals) {
    if (visual?.policy !== "strict" || visual.diagnostic) continue;
    if (step.semanticAssertions !== true || step.ok !== true) {
      throw new Error(`strict capture belongs to failed semantic step: ${step.name || stepIndex}`);
    }
    if (typeof visual.name !== "string" || !/^[A-Za-z0-9][A-Za-z0-9_-]*$/.test(visual.name)) {
      throw new Error(`unsafe strict capture name: ${JSON.stringify(visual?.name)}`);
    }
    if (/[\x00-\x1f\x7f]/.test(visual.name)) {
      throw new Error(`strict capture name contains a control character: ${JSON.stringify(visual.name)}`);
    }
    if (names.has(visual.name)) throw new Error(`duplicate strict capture name: ${visual.name}`);
    names.add(visual.name);
  }
}
if (names.size === 0) throw new Error("visual report contains no strict captures");
process.stdout.write([...names].join("\n"));
NODE
)

while IFS= read -r name; do
  source_path="$source_root/$name.png"
  if [[ ! -f "$source_path" ]]; then
    printf 'strict capture not found: %s\n' "$source_path" >&2
    exit 1
  fi
  resolved_source=$(realpath -e -- "$source_path")
  case "$resolved_source" in
    "$source_root"/*) ;;
    *) printf 'strict capture resolves outside screenshots root: %s\n' "$source_path" >&2; exit 1 ;;
  esac

  destination_path="$destination_root/$name.png"
  resolved_destination=$(realpath -m -- "$destination_path")
  case "$resolved_destination" in
    "$destination_root"/*) ;;
    *) printf 'baseline destination resolves outside baseline root: %s\n' "$destination_path" >&2; exit 1 ;;
  esac
  if [[ -L "$destination_path" ]]; then
    printf 'baseline destination must not be a symbolic link: %s\n' "$destination_path" >&2
    exit 1
  fi

  # Nix store captures are read-only. Replace rather than overwrite an existing
  # baseline, then make the repository copy writable for later approvals.
  cp --remove-destination -- "$resolved_source" "$destination_path"
  chmod u+w -- "$destination_path"
  printf 'approved %s\n' "$name"
done <<< "$captures"
