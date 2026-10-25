# Reads explicit same-pipeline producer bundles. No Cargo, tests, or database.
# LLVM's native text report is the only coverage presentation. No percentage
# threshold is applied. An absent or invalid input is a tooling failure.
if (( $# < 2 )); then
  echo 'Usage: coverage-report OUTPUT_DIRECTORY PRODUCER_BUNDLE...' >&2
  exit 65
fi
output=$1
shift
mkdir -p "$output"
rm -f "$output/report.txt" "$output/coverage.profdata"
declare -a profiles=() objects=()
for bundle in "$@"; do
  [[ -s "$bundle/profile.profdata" && -d "$bundle/objects" ]] || {
    echo "COVERAGE_TOOL_ERROR: incomplete producer bundle: $bundle" >&2
    exit 65
  }
  profiles+=("$bundle/profile.profdata")
  count=0
  while IFS= read -r -d '' object; do
    if (( ${#objects[@]} == 0 )); then
      objects+=("$object")
    else
      objects+=(-object "$object")
    fi
    count=$((count + 1))
  done < <(find "$bundle/objects" -type f -print0)
  (( count > 0 )) || { echo "COVERAGE_TOOL_ERROR: empty objects: $bundle" >&2; exit 65; }
done
llvm-profdata merge -sparse "${profiles[@]}" -o "$output/coverage.profdata" || exit 65
# Restrict the denominator to authored backend workspace sources. Remapped
# filenames are identical across component sandboxes and host-side DB runs;
# dependency crates and generated files are not product coverage.
declare -a sources=()
while IFS= read -r -d '' source; do
  sources+=("$PWD/$source")
done < <(find packages/default/crates -name '*.rs' -type f -print0)
(( ${#sources[@]} > 0 )) || { echo 'COVERAGE_TOOL_ERROR: run from repository root' >&2; exit 65; }
llvm-cov report "${objects[@]}" -instr-profile="$output/coverage.profdata" \
  -path-equivalence="/crystal-forge,$PWD/packages/default" \
  "${sources[@]}" > "$output/report.txt.tmp" || exit 65
# LLVM rejects malformed data. Also reject a vacuous scope without a TOTAL row
# or with a zero line denominator. Keep the native output instead of inventing
# another report model. A zero numerator is a valid measurement.
awk '$1 == "TOTAL" && $8 + 0 > 0 { valid=1 } END { exit !valid }' "$output/report.txt.tmp" || {
  echo 'COVERAGE_TOOL_ERROR: no measured source lines' >&2
  exit 65
}
mv "$output/report.txt.tmp" "$output/report.txt"
cat "$output/report.txt"
