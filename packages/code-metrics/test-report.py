"""Checks the installed metrics launcher without compiling application code.

Run through the repository Nix shell with the built launcher's path as argv[1].
An optional argv[2] validates reports from a real application analysis.
Only Cargo is replaced. Real tokei, jq, shell control flow, and report generation
exercise diagnostic parsing, exit propagation, XML escaping, and JUnit counts.
"""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET


def diagnostic(code, message):
    return {
        "reason": "compiler-message",
        "message": {
            "code": {"code": code},
            "level": "warning",
            "message": message,
            "rendered": message,
            "spans": [{"file_name": "source.rs", "line_start": 1}],
        },
    }


launcher = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="metrics-fixture-") as temporary:
    root = Path(temporary)
    project = root / "project"
    for package in ("default", "web-ui"):
        directory = project / "packages" / package
        directory.mkdir(parents=True)
        (directory / "Cargo.toml").touch()
    source = project / 'source&".rs'
    source.write_text("fn fixture() {}\n")
    fixture = root / "diagnostics.jsonl"
    wrapper = root / "run.sh"
    wrapper.write_text(
        """set -euo pipefail
cargo() {
    cat "$FIXTURE_MESSAGES"
    printf 'fixture cargo stderr\n' >&2
    return "$FIXTURE_EXIT"
}
export -f cargo
exec "$FIXTURE_LAUNCHER"
"""
    )
    for name, messages, cargo_exit, lines, expected_exit, missing in (
        ("pass", [], 0, 1, 0, False),
        ("lint", [diagnostic("clippy::too_many_lines", "too many lines")]*2
         + [diagnostic("unused_imports", "clippy::vec_box is only prose")],
         0, 1, 1, False),
        ("file", [], 0, 501, 1, False),
        ("compiler", [], 101, 1, 1, False),
        ("missing-package", [], 0, 1, 1, True),
    ):
        output = root / name
        source.write_text("// fixture\n" * lines)
        fixture.write_text("".join(json.dumps(item) + "\n" for item in messages))
        if missing:
            (project / "packages" / "web-ui" / "Cargo.toml").unlink()
            (project / "packages" / "web-ui").rmdir()
        environment = dict(os.environ, PROJECT_ROOT=str(project),
                           OUTPUT_DIR=str(output), FIXTURE_MESSAGES=str(fixture),
                           FIXTURE_EXIT=str(cargo_exit), FIXTURE_LAUNCHER=launcher)
        result = subprocess.run(["bash", str(wrapper)], env=environment,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True)
        assert result.returncode == expected_exit, (name, result.stdout, result.stderr)
        statuses = json.loads((output / "clippy-status.json").read_text())
        assert len(statuses) == 2, name
        assert statuses[0]["exit_status"] == cargo_exit, name
        assert statuses[1]["exit_status"] == (1 if missing else cargo_exit), name
        violations = json.loads((output / "complexity-violations.json").read_text())
        assert len(violations) == (1 if name == "lint" else 0), name
        report = ET.parse(output / "junit-report.xml").getroot()
        suite = report.find("testsuite")
        cases = suite.findall("testcase")
        for element in (report, suite):
            assert int(element.attrib["tests"]) == len(cases), name
            assert int(element.attrib["failures"]) == len(suite.findall("testcase/failure")), name
            assert int(element.attrib["errors"]) == len(suite.findall("testcase/error")), name
        assert any(case.attrib["name"] == source.name for case in cases), name
        expected_status = "INCOMPLETE" if cargo_exit or missing else (
            "FAILED" if expected_exit else "PASSED")
        assert f"Status: {expected_status}" in (output / "summary.md").read_text(), name
        assert f"Analysis status: {expected_status}" in (output / "index.html").read_text(), name
        print(f"PASS {name}: exit={result.returncode}, tests={len(cases)}, "
              f"failures={suite.attrib['failures']}, errors={suite.attrib['errors']}")

if len(sys.argv) > 2:
    artifacts = Path(sys.argv[2])
    report = ET.parse(artifacts / "junit-report.xml").getroot()
    suite = report.find("testsuite")
    cases = suite.findall("testcase")
    for element in (report, suite):
        assert int(element.attrib["tests"]) == len(cases)
        assert int(element.attrib["failures"]) == len(suite.findall("testcase/failure"))
        assert int(element.attrib["errors"]) == len(suite.findall("testcase/error"))
    assert not suite.findall("testcase/skipped")
    statuses = json.loads((artifacts / "clippy-status.json").read_text())
    for status in statuses:
        case = next(case for case in cases if case.attrib["name"] == status["package"])
        assert (case.find("error") is not None) == (status["exit_status"] != 0)
        raw = artifacts / (status["package"].replace("/", "-") + ".jsonl")
        assert raw.stat().st_mode & 0o777 == 0o600
    print(f"PASS real artifacts: tests={len(cases)}, "
          f"failures={suite.attrib['failures']}, errors={suite.attrib['errors']}, skipped=0")
