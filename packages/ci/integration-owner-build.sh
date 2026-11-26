#!/bin/sh
# Builds one registered integration scenario gate for GitLab and exports its
# evidence. The generated child pipeline (packages/ci/integration-pipeline.nix)
# runs one job per scenario.
#
# Usage: sh packages/ci/integration-owner-build.sh <owner> <scenario>
#   e.g. sh packages/ci/integration-owner-build.sh integration-access lifecycle-local-bootstrap
#
# CONTRACT: The job result is the result of building
# `.#checks.x86_64-linux.<owner>.scenarios.<scenario>`, the gate that the owner
# aggregate links. A gate fails unless its recorded scenario status is
# "passed". This script never builds a `.run` output as a gate.
#
# Evidence in the CI workspace:
#   integration-evidence/<owner>/<scenario>.nix-build.log
#   integration-evidence/<owner>/<scenario>/  (result.json, meta.json,
#                                              browser/, journals/)
#
# INVARIANT: After a failed build this script only copies the `.run` output if
# it already exists. It MUST NOT build it: a rebuild after a crash or timeout
# could run the VM again and replace the evidence of the original attempt.
#
# The exit status is the build status. A successful build whose evidence
# cannot be collected exits 1.
set -eu

if [ "$#" != 2 ]; then
  printf 'Usage: %s <integration-owner> <scenario>\n' "$0" >&2
  exit 2
fi
owner=$1
scenario=$2
attribute=".#checks.x86_64-linux.${owner}.scenarios.${scenario}"
evidence="integration-evidence/${owner}"
log="${evidence}/${scenario}.nix-build.log"
link="result-${owner}-${scenario}"

# A stale link or copy from an earlier run must not satisfy success below.
rm -rf "${evidence:?}/${scenario}" "$log" "$link"
mkdir -p "$evidence"

# Keep the CI log live through `tee`; the build status goes through a file.
status_file=$(mktemp)
{
  if nix build "$attribute" -L --show-trace --out-link "$link"; then
    printf '0\n' >"$status_file"
  else
    printf '%s\n' "$?" >"$status_file"
  fi
} 2>&1 | tee "$log"
build_status=$(cat "$status_file")
rm -f "$status_file"

# Store files are read-only; a writable copy lets the runner clean up.
copy_evidence() {
  cp -rL "$1" "${evidence}/${scenario}" && chmod -R u+w "${evidence}/${scenario}"
}

if [ "$build_status" = 0 ]; then
  if ! copy_evidence "${link}/evidence"; then
    printf 'Scenario %s passed but its evidence could not be collected.\n' "$scenario" >&2
    exit 1
  fi
  exit 0
fi

# Failure: resolve the run output path by evaluation, which builds nothing,
# and copy it only if it exists. The build status remains the job result.
printf 'Build failed with exit status %s; collecting existing evidence only.\n' \
  "$build_status" >&2
if run_path=$(nix eval --raw "${attribute}.run.outPath" 2>>"$log") && [ -d "$run_path" ]; then
  copy_evidence "$run_path" || printf 'Could not copy evidence for %s.\n' "$scenario" >&2
else
  printf 'No run output exists for %s; build log retained.\n' "$scenario" >&2
fi
exit "$build_status"
