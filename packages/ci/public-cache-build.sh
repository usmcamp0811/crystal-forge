#!/bin/sh
# TASK-470: These gates encountered inherited Attic narinfo stalls (300 seconds,
# five attempts) and TLS hostname failures. Replace only their CLI cache policy.
# Fallback permits source builds after substitution failure; it does not bypass
# authentication, TLS verification, or NAR signatures. Builder settings remain
# inherited. This bounds individual downloads, not total build duration.
set -eu
# Never trace captured runner settings: cache URLs can contain credentials.
set +x

# Use one quoted argv definition for config queries and the final exec. The
# runner's Alpine /bin/sh needs no Bash or additional dependency bootstrap.
policy_nix() {
  policy_mode=$1
  shift
  policy_command=$1
  shift
  set -- nix "$policy_command" \
    --option substituters https://cache.nixos.org \
    --option extra-substituters '' \
    --option accept-flake-config false \
    --option connect-timeout 10 \
    --option stalled-download-timeout 30 \
    --option download-attempts 2 \
    --option require-sigs true \
    --fallback "$@"
  if [ "$policy_mode" = exec ]; then
    exec "$@"
  fi
  "$@"
}

# Prefer the single-setting query. Older runner Nix can use show-config instead.
# Its complete stdout stays in memory; only one exact requested key is returned
# to the caller's capture. Missing, empty, or duplicate keys fail closed. Suppress
# stderr from both commands because diagnostics can contain private cache URLs.
setting() {
  count=0
  if value=$("$@" config show "$setting_name" 2>/dev/null) && [ -n "$value" ]; then
    printf '%s\n' "$value"
    return 0
  fi
  if ! configuration=$("$@" show-config 2>/dev/null); then
    return 1
  fi
  value=''
  newline='
'
  # Split captured lines with POSIX parameter expansion. Keep the complete
  # configuration out of stdout, pipelines, and temporary here-document files.
  while [ -n "$configuration" ]; do
    case "$configuration" in
      *"$newline"*)
        line=${configuration%%"$newline"*}
        configuration=${configuration#*"$newline"}
        ;;
      *) line=$configuration; configuration='' ;;
    esac
    case "$line" in
      "$setting_name = "*)
        count=$((count + 1))
        value=${line#"$setting_name = "}
        ;;
    esac
  done
  unset configuration line
  if [ "$count" != 1 ] || [ -z "$value" ]; then
    return 1
  fi
  printf '%s\n' "$value"
}

nix --version
setting_name=substituters
if ! inherited=$(setting nix); then
  printf '%s\n' 'Cannot query inherited substituters safely.' >&2
  exit 1
fi
inherited_internal=false
case "$inherited" in
  *attic.aicampground.com*) inherited_internal=true ;;
esac
unset inherited
printf 'InheritedInternalCache=%s\n' "$inherited_internal"

# Validate with exactly the argv passed to the build. Print only known values,
# never unexpected URLs. The root flake currently defines no nixConfig/cache
# policy. accept-flake-config=false avoids blanket acceptance; a previously
# trusted flake can still apply configuration after evaluation. Future root
# nixConfig changes therefore need review; this pre-evaluation check does not
# prove post-evaluation cache policy. CI build logs remain part of that audit.
for expected in \
  'substituters=https://cache.nixos.org' \
  'accept-flake-config=false' \
  'connect-timeout=10' \
  'stalled-download-timeout=30' \
  'download-attempts=2' \
  'require-sigs=true' \
  'fallback=true'; do
  setting_name=${expected%%=*}
  if ! effective=$(setting policy_nix run) || [ "$effective" != "${expected#*=}" ]; then
    printf 'Cache policy validation failed for %s (value redacted).\n' "$setting_name" >&2
    exit 1
  fi
  printf 'Effective %s\n' "$expected"
done
unset effective

# Configuration-only verification needs no evaluation, network, or build.
if [ "${1:-}" = --check-config ] && [ "$#" = 1 ]; then
  exit 0
fi
policy_nix exec build "$@"
