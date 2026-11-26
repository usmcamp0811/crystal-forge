#!/bin/sh
# Writes the integration child pipeline (integration-pipeline.yml) and a
# scenario inventory (integration-scenarios.txt) for the parent job
# `integration-pipeline`.
#
# Usage: sh packages/ci/integration-pipeline.sh [<output-directory>]
#
# The pipeline is computed by packages/ci/integration-pipeline.nix from the
# registered scenarios of the integration owner checks; this script keeps no
# list of owners or scenarios. Evaluation builds nothing.
#
# Fails closed: any evaluation error, an empty result, or a job count that
# differs from the registered scenario count exits nonzero, and the parent
# pipeline's trigger job (strategy: depend) then cannot pass.
set -eu

out=${1:-.}
mkdir -p "$out"
pipeline="$out/integration-pipeline.yml"
inventory="$out/integration-scenarios.txt"
rm -f "$pipeline" "$inventory"

nix eval --json .#checks.x86_64-linux \
  --apply "$(cat packages/ci/integration-pipeline.nix)" >"$pipeline.tmp"
# shellcheck disable=SC2016
nix eval --raw .#checks.x86_64-linux --apply '
  checks: builtins.concatStringsSep "\n" (builtins.concatMap
    (owner: map (scenario: "${owner}/${scenario}")
      (builtins.attrNames checks.${owner}.scenarios))
    (builtins.filter (name: builtins.match "integration-.*" name != null
      && checks.${name} ? scenarios) (builtins.attrNames checks)))
' >"$inventory"
printf '\n' >>"$inventory"

scenarios=$(grep -c . "$inventory" || true)
jobs=$(grep -o '"script":\["sh packages/ci/integration-owner-build.sh ' "$pipeline.tmp" | grep -c . || true)
if [ "$scenarios" = 0 ] || [ "$jobs" != "$scenarios" ]; then
  printf 'integration pipeline: %s jobs for %s registered scenarios\n' "$jobs" "$scenarios" >&2
  exit 1
fi
mv "$pipeline.tmp" "$pipeline"
printf 'Generated %s scenario job(s):\n' "$jobs"
cat "$inventory"
