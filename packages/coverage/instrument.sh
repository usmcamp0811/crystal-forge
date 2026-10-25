#!/usr/bin/env bash
# Optional hooks for the existing component and database test owners. These
# functions never execute tests or own a database. Ordinary builds do not source
# this file. All producers use one logical source root to merge shared crates.
coverage_start() {
  : "${CF_COVERAGE_DIR:?Missing coverage artifact directory}"
  : "${CF_LLVM_BIN:?Use the pinned coverage.llvm tools}"
  mkdir -p "$CF_COVERAGE_DIR/raw" || return 65
  export LLVM_PROFILE_FILE="$CF_COVERAGE_DIR/raw/%m-%p.profraw"
  export RUSTFLAGS="${RUSTFLAGS:-} -C instrument-coverage --remap-path-prefix=$1=/crystal-forge"
}

# Retain the exact coverage-bearing objects before Nix strips binaries or the
# database launcher removes temporary storage. A missing measurement fails with
# a tooling diagnostic; successful libtest status alone is not coverage proof.
coverage_export() {
  local target=$1 destination=$2 object name
  local -a profiles objects
  shopt -s nullglob
  profiles=("$destination"/raw/*.profraw)
  if (( ${#profiles[@]} == 0 )); then
    echo 'COVERAGE_TOOL_ERROR: no LLVM profiles were emitted' >&2
    return 65
  fi
  mkdir -p "$destination/objects" || return 65
  "$CF_LLVM_BIN/llvm-profdata" merge -sparse "${profiles[@]}" -o "$destination/profile.profdata" || return 65
  find "$target" -type f -executable -print0 > "$destination/objects.list" || return 65
  while IFS= read -r -d '' object; do
    if "$CF_LLVM_BIN/llvm-readobj" --sections "$object" 2>/dev/null | grep '__llvm_covmap' >/dev/null; then
      name=$(sha256sum "$object") || return 65
      cp "$object" "$destination/objects/${name%% *}" || return 65
    fi
  done < "$destination/objects.list"
  objects=("$destination"/objects/*)
  if (( ${#objects[@]} == 0 )); then
    echo 'COVERAGE_TOOL_ERROR: no coverage-bearing executable objects' >&2
    return 65
  fi
  rm -r "$destination/raw" "$destination/objects.list" || return 65
}
