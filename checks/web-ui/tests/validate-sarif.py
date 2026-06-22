#!/usr/bin/env python3
"""
Validate a SARIF 2.1.0 document against the official OASIS schema (Errata 01)
and perform semantic checks that JSON Schema alone cannot enforce.

Usage:
    python3 validate-sarif.py --sarif <path> --schema <path>

The --schema argument should point to the vendored sarif-schema-2.1.0.json.

Requires: jsonschema (pip install jsonschema)
"""

import argparse
import json
import sys
from pathlib import Path

from jsonschema import Draft4Validator, FormatChecker


def semantic_checks(document: dict) -> list[str]:
    """
    Return a list of error strings for violations that the JSON Schema cannot
    enforce on its own.  Returns an empty list when the document is clean.
    """
    errors = []

    runs = document.get("runs", [])
    if not runs:
        errors.append("SARIF document contains no runs")
        return errors

    for run_idx, run in enumerate(runs):
        prefix = f"runs[{run_idx}]"

        driver = run.get("tool", {}).get("driver", {})
        rules = driver.get("rules", [])
        rule_ids = {rule["id"] for rule in rules if "id" in rule}

        results = run.get("results", [])

        for res_idx, result in enumerate(results):
            rp = f"{prefix}.results[{res_idx}]"

            # Every result must reference a declared rule.
            rule_id = result.get("ruleId", "")
            if rule_id and rule_id not in rule_ids:
                errors.append(
                    f"{rp}: ruleId {rule_id!r} is not declared in "
                    f"tool.driver.rules"
                )

            # Every compliance result must carry a host logical location.
            logical_locations = [
                loc
                for location in result.get("locations", [])
                for loc in location.get("logicalLocations", [])
            ]
            if not logical_locations:
                errors.append(f"{rp}: no logicalLocations — host is unidentified")
            elif not any(loc.get("name") for loc in logical_locations):
                errors.append(
                    f"{rp}: logicalLocations present but none have a 'name' field"
                )

            # Waiver results must carry suppressions.
            props = result.get("properties", {})
            if props.get("disposition") == "waived":
                suppressions = result.get("suppressions", [])
                if not suppressions:
                    errors.append(
                        f"{rp}: disposition=waived but no suppressions array"
                    )
                elif not any(
                    s.get("status") == "accepted" for s in suppressions
                ):
                    errors.append(
                        f"{rp}: waiver suppression present but status != accepted"
                    )

            # kind must be one of the SARIF-defined values.
            kind = result.get("kind", "")
            valid_kinds = {"pass", "fail", "open", "review", "informational", "notApplicable"}
            if kind and kind not in valid_kinds:
                errors.append(f"{rp}: kind={kind!r} is not a valid SARIF kind")

        # Warn if rules exist with no corresponding results (not fatal, but
        # indicates the include_waivers filter may be mismatched).
        result_rule_ids = {r.get("ruleId") for r in results}
        for rule in rules:
            rid = rule.get("id")
            if rid and rid not in result_rule_ids:
                # Emit as a warning (prefixed) rather than an error.
                print(
                    f"  WARNING: {prefix} rule {rid!r} has no corresponding result",
                    file=sys.stderr,
                )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate a SARIF 2.1.0 document against the OASIS schema"
    )
    parser.add_argument("--sarif", required=True, type=Path, help="Path to SARIF file")
    parser.add_argument(
        "--schema", required=True, type=Path, help="Path to sarif-schema-2.1.0.json"
    )
    args = parser.parse_args()

    with args.sarif.open(encoding="utf-8") as fh:
        document = json.load(fh)

    with args.schema.open(encoding="utf-8") as fh:
        schema = json.load(fh)

    # ── JSON Schema validation (Draft 4 with URI format checking) ─────────────
    # FormatChecker() is required — without it Draft4Validator silently accepts
    # empty strings for "format": "uri" fields, defeating the URI validation
    # that catches leftover empty helpUri / informationUri fields.
    validator = Draft4Validator(schema, format_checker=FormatChecker())

    schema_errors = sorted(
        validator.iter_errors(document),
        key=lambda e: [str(p) for p in e.absolute_path],
    )

    if schema_errors:
        print("SARIF 2.1.0 schema validation FAILED:", file=sys.stderr)
        for err in schema_errors:
            path = ".".join(str(p) for p in err.absolute_path)
            print(f"  {path or '<root>'}: {err.message}", file=sys.stderr)
        return 1

    # ── Semantic checks ───────────────────────────────────────────────────────
    sem_errors = semantic_checks(document)
    if sem_errors:
        print("SARIF 2.1.0 semantic validation FAILED:", file=sys.stderr)
        for err in sem_errors:
            print(f"  {err}", file=sys.stderr)
        return 1

    print("SARIF 2.1.0: valid (schema + semantic checks passed)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
