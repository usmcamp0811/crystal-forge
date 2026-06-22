#!/usr/bin/env python3
"""Validate a Crystal Forge OSCAL 1.1.2 Assessment Results export.

Decodes and validates:
  1. The outer Assessment Results document.
  2. The embedded Assessment Plan (base64 in back-matter).
  3. The AP's embedded System Security Plan (base64 in back-matter).
  4. The import-ap and import-ssp fragment references resolve.
"""

import argparse
import base64
import json
import sys
from pathlib import Path
from typing import Any

from jsonschema import Draft7Validator


def load_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as f:
        return json.load(f)


def validate_document(
    document: dict[str, Any],
    schema_path: Path,
    label: str,
) -> None:
    schema = load_json(schema_path)
    validator = Draft7Validator(schema)
    errors = sorted(
        validator.iter_errors(document),
        key=lambda e: list(e.absolute_path),
    )

    if not errors:
        print(f"  ✓ {label}: valid")
        return

    print(f"  ✗ {label}: invalid", file=sys.stderr)
    for error in errors:
        path = ".".join(str(p) for p in error.absolute_path) or "<root>"
        print(f"    {path}: {error.message}", file=sys.stderr)
    raise SystemExit(1)


def resource_by_fragment(
    back_matter: dict[str, Any],
    href: str,
) -> dict[str, Any]:
    if not href.startswith("#"):
        raise ValueError(f"Expected internal fragment reference, got {href!r}")
    resource_uuid = href[1:]
    for res in back_matter.get("resources", []):
        if res.get("uuid") == resource_uuid:
            return res
    raise ValueError(f"Back-matter resource {resource_uuid!r} not found")


def decode_base64_resource(resource: dict[str, Any]) -> dict[str, Any]:
    encoded = resource.get("base64", {}).get("value")
    if not encoded:
        raise ValueError(f"Resource {resource.get('uuid','<unknown>')} has no base64 value")
    raw = base64.b64decode(encoded)
    return json.loads(raw)


def check_assert(condition: bool, msg: str) -> None:
    if not condition:
        print(f"  ✗ {msg}", file=sys.stderr)
        raise SystemExit(1)
    print(f"  ✓ {msg}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Validate Crystal Forge OSCAL 1.1.2 export"
    )
    parser.add_argument("--assessment-results", required=True, type=Path)
    parser.add_argument("--schema-dir", required=True, type=Path)
    args = parser.parse_args()

    schema_ar = args.schema_dir / "oscal_assessment-results_schema.json"
    schema_ap = args.schema_dir / "oscal_assessment-plan_schema.json"
    schema_ssp = args.schema_dir / "oscal_ssp_schema.json"

    for p in [schema_ar, schema_ap, schema_ssp]:
        if not p.exists():
            print(f"Schema not found: {p}", file=sys.stderr)
            raise SystemExit(1)

    # 1. Load and validate Assessment Results
    doc = load_json(args.assessment_results)
    if "assessment-results" not in doc:
        print("Root key 'assessment-results' not found", file=sys.stderr)
        raise SystemExit(1)

    print("Validating OSCAL 1.1.2 document chain...")
    validate_document(doc, schema_ar, "Assessment Results")

    ar_root = doc["assessment-results"]
    back_matter = ar_root.get("back-matter", {})

    # 2. Decode and validate Assessment Plan from back-matter
    ap_href = ar_root.get("import-ap", {}).get("href", "")
    check_assert(ap_href.startswith("#"), "import-ap.href is a fragment reference")

    ap_resource = resource_by_fragment(back_matter, ap_href)
    ap_doc = decode_base64_resource(ap_resource)

    check_assert(
        ap_resource["base64"]["media-type"] == "application/oscal+json",
        "AP base64 media-type is application/oscal+json",
    )
    check_assert(
        "assessment-plan" in ap_doc,
        "Decoded AP has 'assessment-plan' root",
    )

    validate_document(ap_doc, schema_ap, "Assessment Plan")

    ap_root = ap_doc["assessment-plan"]

    # 3. Decode and validate SSP from back-matter via AP's import-ssp
    ssp_href = ap_root.get("import-ssp", {}).get("href", "")
    check_assert(ssp_href.startswith("#"), "import-ssp.href is a fragment reference")

    ssp_resource = resource_by_fragment(back_matter, ssp_href)
    ssp_doc = decode_base64_resource(ssp_resource)

    check_assert(
        ssp_resource["base64"]["media-type"] == "application/oscal+json",
        "SSP base64 media-type is application/oscal+json",
    )
    check_assert(
        "system-security-plan" in ssp_doc,
        "Decoded SSP has 'system-security-plan' root",
    )

    validate_document(ssp_doc, schema_ssp, "System Security Plan")

    # 4. Reference chain integrity
    check_assert(
        ar_root["import-ap"]["href"] == f"#{ap_resource['uuid']}",
        "AR import-ap points to AP resource UUID",
    )
    check_assert(
        ap_root["import-ssp"]["href"] == f"#{ssp_resource['uuid']}",
        "AP import-ssp points to SSP resource UUID",
    )

    print("\nOSCAL export validation: ALL CHECKS PASSED")


if __name__ == "__main__":
    main()
