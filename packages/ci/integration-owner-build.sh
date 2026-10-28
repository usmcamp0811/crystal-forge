#!/bin/sh
# Builds one integration owner aggregate for GitLab and exports its evidence.
#
# Usage: sh packages/ci/integration-owner-build.sh integration-access
#
# CONTRACT: The job result is the result of the normal owner aggregate build.
# The aggregate builds every registered scenario gate, and a gate fails unless
# its recorded scenario status is "passed". This script never substitutes a
# focused scenario or a `.run` output for that build.
#
# Evidence layout in the CI workspace:
#   integration-evidence/<owner>/nix-build.log
#   integration-evidence/<owner>/<scenario>/   (result.json, meta.json,
#                                               browser/, journals/)
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

if [ "$#" != 1 ]; then
  printf 'Usage: %s <integration-owner>\n' "$0" >&2
  exit 2
fi
owner=$1
attribute=".#checks.x86_64-linux.${owner}"
evidence="integration-evidence/${owner}"
log="${evidence}/nix-build.log"

# A stale link from an earlier run must not satisfy the success path below.
rm -rf "$evidence" result
mkdir -p "$evidence"

# Run the build through `tee` so the CI log stays live. A pipeline reports only
# the status of `tee`, so the build status goes through a temporary file.
status_file=$(mktemp)
{
  if sh packages/ci/public-cache-build.sh "$attribute" -L --show-trace \
    --builders '' --max-jobs 1 --cores 2; then
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
  # Success: the owner link is a linkFarm of scenario gates. Each gate exposes
  # its run output as `evidence`. The job promises that evidence, so any
  # collection problem fails the job. Every scenario is still attempted so the
  # report names all problems at once.
  if [ ! -d result ]; then
    printf 'Owner build succeeded but no result link exists.\n' >&2
    exit 1
  fi
  for gate in result/*/; do
    scenario=$(basename "$gate")
    if [ ! -d "${gate}evidence" ]; then
      printf 'Scenario %s has no evidence directory.\n' "$scenario" >&2
      collection_errors=$((collection_errors + 1))
    elif copy_evidence "$scenario" "${gate}evidence"; then
      collected=$((collected + 1))
    else
      printf 'Could not copy evidence for scenario %s.\n' "$scenario" >&2
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

# Failure: the owner `result` link does not exist. Resolve the registered
# scenarios' run output paths through evaluation, which builds nothing.
#
# INVARIANT: The original build status is the job result. Nothing below may
# replace it, so `set -e` is off for the rest of the script. A collection
# problem is reported and counted, and the script continues with the next
# scenario.
set +e
printf 'Owner build failed with exit status %s; collecting existing evidence only.\n' \
  "$build_status" >&2
listing=$(mktemp)
# The single quotes are deliberate: `${...}` below is Nix string interpolation,
# not shell expansion.
# shellcheck disable=SC2016
if nix eval --raw "${attribute}.scenarios" --apply \
  'scenarios: builtins.concatStringsSep "\n" (builtins.attrValues (builtins.mapAttrs (name: scenario: "${name} ${scenario.run.outPath}") scenarios))' \
  >"$listing" 2>>"$log"; then
  # The Nix expression joins records with newlines and adds none after the
  # last one. `read` returns nonzero for that unterminated last record but
  # still sets its variables, so test them as well. Without the second test
  # the final scenario is silently skipped.
  while IFS=' ' read -r scenario run_path || [ -n "$scenario" ]; do
    if [ -z "$scenario" ]; then
      continue
    elif [ ! -d "$run_path" ]; then
      printf 'No run output exists for scenario %s; build log retained.\n' "$scenario" >&2
    elif copy_evidence "$scenario" "$run_path"; then
      collected=$((collected + 1))
    else
      printf 'Could not copy evidence for scenario %s; continuing.\n' "$scenario" >&2
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
