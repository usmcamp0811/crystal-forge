#!/usr/bin/env bash
# Sourced by database-environment.sh and by prebuilt-cargo-test-test.sh.
# Defines cf_prebuilt_cargo_test and cf_cargo; it runs nothing itself.

# PERFORMANCE (TASK-542.1): CI sets CF_PREBUILT_TEST_EXECUTABLES to the
# output of .#default.cf-database-test-executables, which compiles this
# workspace's test configuration once as an immutable Nix output. Every
# `cargo test` selection then runs those executables instead of compiling in
# this invocation's target directory. The selection guard in
# database-environment.sh is unchanged:
# listing, membership, counts and exit status are checked the same way.
#
# INVARIANT: Prebuilt mode fails closed. An unset variable keeps the original
# Cargo path. A set variable whose manifest or executable is missing, an
# unsupported Cargo option, or an unknown package or test target is an error;
# it never falls back to compiling or fetching.
cf_prebuilt_cargo_test() {
  local root="$PROJECT_ROOT/packages/default"
  local manifest="${CF_PREBUILT_TEST_EXECUTABLES}/manifest.tsv"
  local -a packages=() tests=() harness=() runner=() selected=() filter_args=()
  local lib=false no_run=false filter_set=false
  local package kind target crate_dir executable entry wanted found status=0
  if [[ ! -s "$manifest" ]]; then
    echo "Prebuilt test manifest is missing: $manifest" >&2
    return 2
  fi
  while (( $# )); do
    case "$1" in
      --) shift; harness=("$@"); break ;;
      --manifest-path)
        if [[ "$(realpath -m -- "$2")" != "$root/Cargo.toml" ]]; then
          echo "Prebuilt tests cover only $root/Cargo.toml" >&2
          return 2
        fi
        shift 2 ;;
      -p|--package) packages+=("$2"); shift 2 ;;
      --lib) lib=true; shift ;;
      --test) tests+=("$2"); shift 2 ;;
      --offline|--locked|--frozen) shift ;;
      --no-run) no_run=true; shift ;;
      -*) echo "Unsupported cargo test option in prebuilt mode: $1" >&2; return 2 ;;
      *)
        if [[ "$filter_set" == true ]]; then
          echo "Unsupported second test filter in prebuilt mode: $1" >&2
          return 2
        fi
        filter_set=true; filter_args=("$1"); shift ;;
    esac
  done
  if [[ "$lib" == false && ${#tests[@]} == 0 ]]; then
    echo "Prebuilt mode needs --lib or --test; other target kinds are not built" >&2
    return 2
  fi
  in_scope() {
    local name
    (( ${#packages[@]} == 0 )) && return 0
    for name in "${packages[@]}"; do [[ "$name" == "$1" ]] && return 0; done
    return 1
  }
  # Explicit packages must exist; a workspace --lib skips packages without a
  # library, as Cargo does, but an explicit `-p X --lib` requires one.
  for wanted in "${packages[@]}"; do
    found=false
    while IFS=$'\t' read -r package kind target crate_dir executable; do
      [[ "$package" == "$wanted" && ( "$lib" == false || "$kind" == lib ) ]] && found=true
    done < "$manifest"
    if [[ "$found" == false ]]; then
      echo "No prebuilt target for package $wanted" >&2
      return 2
    fi
  done
  while IFS=$'\t' read -r package kind target crate_dir executable; do
    in_scope "$package" || continue
    if [[ "$lib" == true && "$kind" == lib ]]; then
      selected+=("$package"$'\t'"$kind"$'\t'"$target"$'\t'"$crate_dir"$'\t'"$executable")
    fi
  done < "$manifest"
  for wanted in "${tests[@]}"; do
    found=false
    while IFS=$'\t' read -r package kind target crate_dir executable; do
      if in_scope "$package" && [[ "$kind" == test && "$target" == "$wanted" ]]; then
        selected+=("$package"$'\t'"$kind"$'\t'"$target"$'\t'"$crate_dir"$'\t'"$executable")
        found=true
      fi
    done < "$manifest"
    if [[ "$found" == false ]]; then
      echo "No prebuilt test target named $wanted" >&2
      return 2
    fi
  done
  # Cargo honours its target runner variable for test executables; keep that.
  if [[ -n "${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER:-}" ]]; then
    read -r -a runner <<< "$CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER"
  fi
  for entry in "${selected[@]}"; do
    IFS=$'\t' read -r package kind target crate_dir executable <<< "$entry"
    executable="$CF_PREBUILT_TEST_EXECUTABLES/$executable"
    if [[ ! -x "$executable" ]]; then
      echo "Prebuilt test executable is missing: $executable" >&2
      return 2
    fi
    [[ "$no_run" == true ]] && continue
    printf '     Running %s %s (prebuilt %s)\n' "$kind" "$target" "$executable" >&2
    # Cargo runs each executable from its package directory with
    # CARGO_MANIFEST_DIR set; tests read checkout files through both.
    if (
      cd "$root/$crate_dir" &&
        CARGO_MANIFEST_DIR="$root/$crate_dir" CARGO_PKG_NAME="$package" \
          exec "${runner[@]}" "$executable" "${filter_args[@]}" "${harness[@]}"
    ); then
      :
    else
      # Like Cargo without --no-fail-fast: stop at the first failing target.
      status=$?
      break
    fi
  done
  return "$status"
}

cf_cargo() {
  if [[ "${1:-}" == test && -n "${CF_PREBUILT_TEST_EXECUTABLES:-}" ]]; then
    shift
    cf_prebuilt_cargo_test "$@"
  else
    command cargo "$@"
  fi
}

