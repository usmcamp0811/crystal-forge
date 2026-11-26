#!/bin/sh
# Builds one integration owner aggregate, or one registered scenario gate, for
# GitLab and exports its evidence.
#
# Usage:
#   sh packages/ci/integration-owner-build.sh integration-access
#   sh packages/ci/integration-owner-build.sh integration-access lifecycle-local-bootstrap
#
# CONTRACT: The job result is the result of the normal build of the selected
# gate. Owner mode builds the owner aggregate, which builds every registered
# scenario gate. Scenario mode builds
# `.#checks.x86_64-linux.<owner>.scenarios.<scenario>`, the same gate
# derivation that the aggregate links. A gate fails unless its recorded
# scenario status is "passed". This script never substitutes a `.run` output
# for a gate build. CI runs scenario mode from the generated child pipeline
# (packages/ci/integration-pipeline.sh); owner mode is the developer command.
#
# Evidence layout in the CI workspace:
#   owner mode:    integration-evidence/<owner>/nix-build.log
#                  integration-evidence/<owner>/<scenario>/  (one per scenario)
#   scenario mode: integration-evidence/<owner>/<scenario>.nix-build.log
#                  integration-evidence/<owner>/<scenario>/
# Each scenario directory holds result.json, meta.json, browser/ and journals/.
#
# INVARIANT: After a failed build this script only copies `.run` outputs that
# already exist. It MUST NOT build a `.run` derivation. A rebuild after a
# crash or a timeout could run the VM again and replace the evidence of the
# original attempt.
#
# The exit status is the original build status. A successful build whose
# evidence cannot be collected exits 1, so the job never reports a complete
# result without the evidence it promises.
#
# The scenario list comes from the owner's own `scenarios` attribute set.
# This script keeps no second list of scenario names.
set -eu

case "$#" in
  1) owner=$1; scenario='' ;;
  2) owner=$1; scenario=$2 ;;
  *)
    printf 'Usage: %s <integration-owner> [<scenario>]\n' "$0" >&2
    exit 2
    ;;
esac
case "$owner$scenario" in
  *[!A-Za-z0-9_-]*)
    printf 'Owner and scenario names may contain only letters, digits, - and _.\n' >&2
    exit 2
    ;;
esac
evidence="integration-evidence/${owner}"
if [ -n "$scenario" ]; then
  attribute=".#checks.x86_64-linux.${owner}.scenarios.${scenario}"
  log="${evidence}/${scenario}.nix-build.log"
  link="result-${owner}-${scenario}"
  # A stale link or copy from an earlier run must not satisfy success below.
  rm -rf "${evidence:?}/${scenario}" "$log" "$link"
else
  attribute=".#checks.x86_64-linux.${owner}"
  log="${evidence}/nix-build.log"
  link="result-${owner}"
  rm -rf "$evidence" "$link"
fi
mkdir -p "$evidence"

# Run the build through `tee` so the CI log stays live. A pipeline reports only
# the status of `tee`, so the build status goes through a temporary file.
status_file=$(mktemp)
{
  if sh packages/ci/nix-ci.sh build "$attribute" -L --show-trace --out-link "$link"; then
    build_status=0
  else
    build_status=$?
  fi
  printf '%s\n' "$build_status" >"$status_file"
} 2>&1 | tee "$log"
build_status=$(cat "$status_file")
rm -f "$status_file"

# Store files are read-only. A writable copy lets the runner clean its workspace.
#
# Returns nonzero when the copy or the permission change fails. The status is
# explicit because `set -e` does not apply inside a function that the caller
# tests, and a later successful `chmod` must not hide a failed `cp`.
copy_evidence() {
  copy_status=0
  cp -rL "$2" "${evidence}/$1" || copy_status=1
  if [ -e "${evidence}/$1" ]; then
    chmod -R u+w "${evidence}/$1" || copy_status=1
  fi
  return "$copy_status"
}

collected=0
collection_errors=0
if [ "$build_status" = 0 ]; then
  # Success: the job promises the evidence, so any collection problem fails
  # the job. Every scenario is still attempted so the report names all
  # problems at once.
  if [ ! -d "$link" ]; then
    printf 'Build succeeded but no result link exists.\n' >&2
    exit 1
  fi
  if [ -n "$scenario" ]; then
    # A gate output exposes its run output as `evidence`.
    if [ ! -d "${link}/evidence" ]; then
      printf 'Scenario %s has no evidence directory.\n' "$scenario" >&2
      exit 1
    fi
    if ! copy_evidence "$scenario" "${link}/evidence"; then
      printf 'Could not copy evidence for scenario %s.\n' "$scenario" >&2
      exit 1
    fi
    printf 'Collected evidence for scenario %s under %s\n' "$scenario" "$evidence"
    exit 0
  fi
  # Owner mode: the link is a linkFarm of scenario gates.
  for gate in "$link"/*/; do
    name=$(basename "$gate")
    if [ ! -d "${gate}evidence" ]; then
      printf 'Scenario %s has no evidence directory.\n' "$name" >&2
      collection_errors=$((collection_errors + 1))
    elif copy_evidence "$name" "${gate}evidence"; then
      collected=$((collected + 1))
    else
      printf 'Could not copy evidence for scenario %s.\n' "$name" >&2
      collection_errors=$((collection_errors + 1))
    fi
  done
  if [ "$collected" = 0 ] && [ "$collection_errors" = 0 ]; then
    printf 'Owner build succeeded but registered no scenario evidence.\n' >&2
    exit 1
  fi
  if [ "$collection_errors" != 0 ]; then
    printf 'Evidence collection failed for %s scenario(s) after a successful build.\n' \
      "$collection_errors" >&2
    exit 1
  fi
  printf 'Collected evidence for %s scenario(s) under %s\n' "$collected" "$evidence"
  exit 0
fi

# Failure: no result link exists. Resolve the run output paths through
# evaluation, which builds nothing.
#
# INVARIANT: The original build status is the job result. Nothing below may
# replace it, so `set -e` is off for the rest of the script. A collection
# problem is reported and counted, and the script continues with the next
# scenario.
set +e
printf 'Build failed with exit status %s; collecting existing evidence only.\n' \
  "$build_status" >&2
listing=$(mktemp)
# The single quotes are deliberate: `${...}` below is Nix string interpolation,
# not shell expansion.
# shellcheck disable=SC2016
if [ -n "$scenario" ]; then
  expression='scenario: "'"$scenario"' ${scenario.run.outPath}"'
  target="${attribute}"
else
  expression='scenarios: builtins.concatStringsSep "\n" (builtins.attrValues (builtins.mapAttrs (name: scenario: "${name} ${scenario.run.outPath}") scenarios))'
  target="${attribute}.scenarios"
fi
if nix eval --raw "$target" --apply "$expression" >"$listing" 2>>"$log"; then
  # The Nix expression joins records with newlines and adds none after the
  # last one. `read` returns nonzero for that unterminated last record but
  # still sets its variables, so test them as well. Without the second test
  # the final scenario is silently skipped.
  while IFS=' ' read -r name run_path || [ -n "$name" ]; do
    if [ -z "$name" ]; then
      continue
    elif [ ! -d "$run_path" ]; then
      printf 'No run output exists for scenario %s; build log retained.\n' "$name" >&2
    elif copy_evidence "$name" "$run_path"; then
      collected=$((collected + 1))
    else
      printf 'Could not copy evidence for scenario %s; continuing.\n' "$name" >&2
      collection_errors=$((collection_errors + 1))
    fi
  done <"$listing"
else
  printf 'Could not resolve scenario run outputs; build log retained.\n' >&2
fi
rm -f "$listing"
printf 'Collected evidence for %s scenario(s) after the failed build (%s collection error(s)).\n' \
  "$collected" "$collection_errors" >&2
exit "$build_status"
