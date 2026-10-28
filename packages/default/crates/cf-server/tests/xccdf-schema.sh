#!/usr/bin/env bash
# Validate the installed writer against external XCCDF and CF schema contracts.
# Usage: bash xccdf-schema.sh CORE_PACKAGE XCCDF_SCHEMA_DIRECTORY CF_SCHEMA_FILE
# Requires xmllint (libxml2), oscap (OpenSCAP), and standard shell utilities.
#
# Rust writer/parser tests own element ordering, profile selection, custom-check
# variants (including empty-all), and native round-trip semantics. Handwritten
# documents that merely resemble writer output do not prove exporter validity.
# This command validates actual exporter bytes and all extracted CF payloads.
set -euo pipefail
if [ "$#" -ne 3 ]; then
  printf 'Usage: %s CORE_PACKAGE XCCDF_SCHEMA_DIRECTORY CF_SCHEMA_FILE\n' "$0" >&2
  exit 2
fi
core=$(realpath -e "$1")
xccdf_schemas=$(realpath -e "$2")
cf_schema=$(realpath -e "$3")
test -x "$core/bin/xccdf-export-fixture"
test -f "$xccdf_schemas/xccdf/1.2/xccdf_1.2.xsd"
test -f "$cf_schema"
workspace=$(mktemp -d)
trap 'rm -rf "$workspace"' EXIT
document="$workspace/generated.xml"
"$core/bin/xccdf-export-fixture" > "$document"
xmllint --nonet --noout --schema \
  "$xccdf_schemas/xccdf/1.2/xccdf_1.2.xsd" "$document"

# XCCDF extension points do not validate CF payloads. Require every payload
# type emitted by this fixture, then validate each instance independently.
for element in bundle policy-identity policy source-mappings opaque-xml; do
  selector="//*[local-name()='$element' and namespace-uri()='urn:crystal-forge:xccdf:1']"
  count=$(xmllint --nonet --xpath "count($selector)" "$document")
  if ! [[ "$count" =~ ^[1-9][0-9]*$ ]]; then
    printf 'Writer fixture is missing CF extension %s\n' "$element" >&2
    exit 1
  fi
  for ((index = 1; index <= count; index++)); do
    fragment="$workspace/$element-$index.xml"
    # The writer uses the cf prefix declared on Benchmark. XPath extraction
    # omits that inherited declaration; restore it on the extracted root.
    xmllint --nonet --xpath "($selector)[$index]" "$document" \
      | sed "s#^<cf:$element#<cf:$element xmlns:cf=\"urn:crystal-forge:xccdf:1\"#" \
      > "$fragment"
    xmllint --nonet --noout --schema "$cf_schema" "$fragment"
  done
done

# Keep independent OpenSCAP acceptance of the same generated document. An
# informational metadata dump adds no assertion beyond successful validation.
oscap xccdf validate "$document"
printf 'Installed XCCDF writer and CF extension schema validation passed.\n'
