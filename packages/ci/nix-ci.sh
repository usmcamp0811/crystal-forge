#!/bin/sh
# Runs one Nix command for GitLab CI with the Crystal Forge cache and resource
# policy, and records build metrics.
#
# Usage:
#   sh packages/ci/nix-ci.sh <nix-subcommand> [arguments...]
#   sh packages/ci/nix-ci.sh --check-config
#
# CACHE POLICY: Substitutes from the trusted Campground Niks3 cache and from
# https://cache.nixos.org. The URL and the public key are the values that the
# Campground infrastructure already trusts (dotfiles module
# `modules/nixos/cache/niks3`, options `url` and `public-key`; and the former
# Attic key, see LEGACY_KEY). Signatures are
# required; a path signed by neither key is never accepted. Flake-supplied
# cache settings are refused. Connection attempts, stalled downloads and
# retries are bounded, and `--fallback` builds from source when a substituter
# fails.
#
# HISTORY: TASK-470 restricted CI to cache.nixos.org and `--builders ''
# --max-jobs 1 --cores 2` because the former Attic cache stalled and failed
# TLS, and the former remote builders were unhealthy. Niks3 replaced Attic and
# nix-grpc-store replaced the builders. This script therefore restores the
# private cache and inherits the runner host's builder, max-jobs and cores
# settings, which the infrastructure owns.
#
# RESOURCE POLICY: The runner host's Nix daemon settings apply unless the job
# sets CF_CI_NIX_MAX_JOBS (positive integer or "auto"), CF_CI_NIX_CORES
# (non-negative integer) or CF_CI_NIX_LOCAL_ONLY=1 (no remote builders). The
# variables are validated; any other value fails before Nix runs.
#
# HEALTH: Before a build the script probes Niks3 with the same bounded
# timeouts and prints `CacheHealth niks3=available` or
# `CacheHealth niks3=unavailable`. An unavailable cache is reported in the log
# and in the metrics record. It does not fail the job, because the policy
# falls back to cache.nixos.org and source builds. Set
# CF_CI_REQUIRE_PRIVATE_CACHE=1 to fail instead.
#
# METRICS: For `build`, one JSON line is appended to
# ${CF_CI_METRICS_DIR:-ci-metrics}/nix.jsonl with the job name, exit status,
# duration, cache health, built derivation count and substituted path counts
# per known cache. Unknown sources are counted as `other`; their URLs are not
# written.
#
# SECURITY: The script never prints or records inherited Nix settings, because
# runner cache URLs and client certificate paths can be sensitive. It prints
# only the values that it sets itself. `set -x` is disabled.
set -eu
set +x

NIKS3_URL=https://niks3.lan.aicampground.com
NIKS3_KEY=niks3-campground-1:GyAEyqlwuxporKjJr9RhvtKCMgVBa+1q0sbah1y4KYE=
# Niks3 also serves paths signed with the former Attic key, which every
# Campground host still trusts (dotfiles `modules/nixos/cache/campground`,
# enabled by the common suite). Trusting it here matches the hosts and adds no
# key that they do not already trust.
LEGACY_KEY=campground:XZ6LmOgWmChUUb5ZWWn/XnTreAYaNcPTQHxUR3T3dc8=
NIXOS_URL=https://cache.nixos.org
NIXOS_KEY=cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=

# Seconds and attempts. A healthy Niks3 answers in well under a second on the
# LAN, so 10 seconds to connect and 30 seconds without progress detect a stall
# quickly; two attempts tolerate one transient failure.
CONNECT_TIMEOUT=10
STALLED_TIMEOUT=30
DOWNLOAD_ATTEMPTS=2

fail() {
  printf 'nix-ci: %s\n' "$1" >&2
  exit 2
}

# Validates the resource variables before Nix runs.
max_jobs=${CF_CI_NIX_MAX_JOBS:-}
cores=${CF_CI_NIX_CORES:-}
local_only=${CF_CI_NIX_LOCAL_ONLY:-0}
case "$max_jobs" in
  ''|auto) ;;
  *[!0-9]*|0) fail 'CF_CI_NIX_MAX_JOBS must be a positive integer or auto' ;;
esac
case "$cores" in
  '') ;;
  *[!0-9]*) fail 'CF_CI_NIX_CORES must be a non-negative integer' ;;
esac
case "$local_only" in
  0|1) ;;
  *) fail 'CF_CI_NIX_LOCAL_ONLY must be 0 or 1' ;;
esac

# Runs Nix with the policy. The first argument is the Nix subcommand. Resource
# options come after the policy so that they are visible in the log line.
policy_nix() {
  policy_command=$1
  shift
  set -- nix "$policy_command" \
    --option substituters "$NIKS3_URL $NIXOS_URL" \
    --option trusted-public-keys "$NIKS3_KEY $LEGACY_KEY $NIXOS_KEY" \
    --option extra-substituters '' \
    --option extra-trusted-public-keys '' \
    --option accept-flake-config false \
    --option require-sigs true \
    --option connect-timeout "$CONNECT_TIMEOUT" \
    --option stalled-download-timeout "$STALLED_TIMEOUT" \
    --option download-attempts "$DOWNLOAD_ATTEMPTS" \
    --fallback "$@"
  if [ -n "$max_jobs" ]; then set -- "$@" --max-jobs "$max_jobs"; fi
  if [ -n "$cores" ]; then set -- "$@" --cores "$cores"; fi
  if [ "$local_only" = 1 ]; then set -- "$@" --builders ''; fi
  "$@"
}

# Validates the effective policy with exactly the argv used for the command.
# Only values that this script sets are printed. stderr is suppressed because
# Nix diagnostics can contain inherited cache URLs.
validate_policy() {
  for expected in \
    "substituters=$NIKS3_URL $NIXOS_URL" \
    "trusted-public-keys=$NIKS3_KEY $LEGACY_KEY $NIXOS_KEY" \
    'accept-flake-config=false' \
    'require-sigs=true' \
    "connect-timeout=$CONNECT_TIMEOUT" \
    "stalled-download-timeout=$STALLED_TIMEOUT" \
    "download-attempts=$DOWNLOAD_ATTEMPTS" \
    'fallback=true'; do
    name=${expected%%=*}
    if ! effective=$(policy_nix config show "$name" 2>/dev/null) ||
      [ "$effective" != "${expected#*=}" ]; then
      printf 'nix-ci: cache policy validation failed for %s (value redacted)\n' \
        "$name" >&2
      exit 1
    fi
  done
  printf 'NixCiPolicy substituters=%s,%s require-sigs=true fallback=true\n' \
    "$NIKS3_URL" "$NIXOS_URL"
  # The daemon applies restricted client settings (substituters, keys,
  # builders) only for a trusted user; otherwise its own configuration wins.
  # CI jobs run as root, which the daemon trusts. Report the actual state so
  # an ignored policy is visible in the log rather than assumed.
  trust=unknown
  if info=$(nix store info --json 2>/dev/null); then
    case "$info" in
      *'"trusted":1'*|*'"trusted": 1'*|*'"trusted":true'*) trust=trusted ;;
      *'"trusted":0'*|*'"trusted": 0'*|*'"trusted":false'*) trust=untrusted ;;
    esac
  fi
  printf 'NixCiTrust daemon=%s\n' "$trust"
  if [ "$trust" = untrusted ]; then
    printf 'nix-ci: WARNING the Nix daemon does not trust this user; it ignores the cache and resource settings above.\n' >&2
  fi
  printf 'NixCiResources max-jobs=%s cores=%s builders=%s\n' \
    "${max_jobs:-inherited}" "${cores:-inherited}" \
    "$( [ "$local_only" = 1 ] && echo none || echo inherited )"
}

# Probes Niks3 within the bounded timeouts. Prints the result; never fails
# unless CF_CI_REQUIRE_PRIVATE_CACHE=1.
cache_health=unknown
probe_cache() {
  if nix store info --store "$NIKS3_URL" \
    --option connect-timeout "$CONNECT_TIMEOUT" \
    --option download-attempts 1 >/dev/null 2>&1; then
    cache_health=available
  else
    cache_health=unavailable
  fi
  printf 'CacheHealth niks3=%s\n' "$cache_health"
  if [ "$cache_health" = unavailable ]; then
    printf 'nix-ci: WARNING Niks3 is unavailable; substituting from %s and building from source.\n' \
      "$NIXOS_URL" >&2
    if [ "${CF_CI_REQUIRE_PRIVATE_CACHE:-0}" = 1 ]; then
      printf 'nix-ci: CF_CI_REQUIRE_PRIVATE_CACHE=1 and Niks3 is unavailable\n' >&2
      exit 1
    fi
  fi
}

# Escapes a string for a JSON string literal (backslash and double quote;
# installables never contain control characters).
json_string() {
  printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g')"
}

if [ "$#" = 0 ]; then
  fail 'usage: nix-ci.sh <nix-subcommand> [arguments...] | --check-config'
fi
nix --version
validate_policy
if [ "$1" = --check-config ]; then
  [ "$#" = 1 ] || fail '--check-config takes no arguments'
  exit 0
fi

command_name=$1
if [ "$command_name" != build ]; then
  # `set -e` exits with the Nix status when the command fails.
  policy_nix "$@"
  exit 0
fi

probe_cache
metrics_dir=${CF_CI_METRICS_DIR:-ci-metrics}
mkdir -p "$metrics_dir"
log=$(mktemp)
status_file=$(mktemp)
started=$(date +%s)
shift
{
  if policy_nix build "$@"; then
    printf '0\n' >"$status_file"
  else
    printf '%s\n' "$?" >"$status_file"
  fi
} 2>&1 | tee "$log"
status=$(cat "$status_file")
finished=$(date +%s)

# Counts come from Nix's own progress lines:
#   copying path '/nix/store/...' from 'https://...'...
#   building '/nix/store/...drv'...
built=$(grep -c "^building '/nix/store/" "$log" || true)
from_niks3=$(grep -c "^copying path '.*' from '$NIKS3_URL" "$log" || true)
from_nixos=$(grep -c "^copying path '.*' from '$NIXOS_URL" "$log" || true)
copied_total=$(grep -c "^copying path '.*' from '" "$log" || true)
from_other=$((copied_total - from_niks3 - from_nixos))
rm -f "$log" "$status_file"

installables=''
for argument in "$@"; do
  case "$argument" in
    *#*|/nix/store/*)
      installables="${installables:+$installables,}$(json_string "$argument")"
      ;;
  esac
done
printf '{"job":%s,"command":"build","installables":[%s],"exit":%s,"seconds":%s,"cache_health":"%s","built":%s,"substituted":{"niks3":%s,"cache.nixos.org":%s,"other":%s}}\n' \
  "$(json_string "${CI_JOB_NAME:-local}")" "$installables" "$status" \
  "$((finished - started))" "$cache_health" "$built" "$from_niks3" \
  "$from_nixos" "$from_other" >>"$metrics_dir/nix.jsonl"
printf 'NixCiMetrics exit=%s seconds=%s built=%s substituted_niks3=%s substituted_nixos=%s substituted_other=%s cache=%s\n' \
  "$status" "$((finished - started))" "$built" "$from_niks3" "$from_nixos" \
  "$from_other" "$cache_health"
exit "$status"
