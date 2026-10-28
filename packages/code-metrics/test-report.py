"""Exercises the installed launcher with controlled Cargo JSON, without Rust builds.

Run with Nix-provided Python and Bash; argv[1] is the installed launcher.
Only Cargo is replaced. Real tokei, jq, and report generation remain active.
"""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET


def diagnostic(code, message, level="warning"):
    return {"reason": "compiler-message", "message": {
        "code": {"code": code}, "level": level, "message": message,
        "rendered": message,
        "spans": [{"file_name": "source.rs", "line_start": 1}],
    }}


complete = [{"reason": "compiler-artifact"},
            {"reason": "build-finished", "success": True}]
lint = diagnostic("clippy::too_many_lines", "too many lines (101/100)")
launcher = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="metrics-launcher-") as temporary:
    root = Path(temporary)
    project = root / "project"
    for package in ("default", "web-ui"):
        directory = project / "packages" / package
        directory.mkdir(parents=True)
        (directory / "Cargo.toml").touch()
    source = project / 'source&".rs'
    fixture = root / "messages.jsonl"
    wrapper = root / "run.sh"
    wrapper.write_text('''set -euo pipefail
cargo() {
    [[ "$*" == 'clippy --all-targets --all-features --message-format=json' ]] || return 90
    [[ "$SQLX_OFFLINE" == true && "$CARGO_NET_OFFLINE" == true && "$CARGO_BUILD_JOBS" == 2 ]] || return 91
    [[ "$RUSTC" == /nix/store/*/bin/rustc && -x "$RUSTC" && -x "$RUSTDOC" ]] || return 92
    [[ -f "$OPENSSL_INCLUDE_DIR/openssl/ssl.h" && -d "$OPENSSL_LIB_DIR" ]] || return 93
    pkg-config --exists openssl || return 94
    cat "$FIXTURE_MESSAGES"
    printf 'controlled Cargo stderr\n' >&2
    return "$FIXTURE_EXIT"
}
export -f cargo
exec "$FIXTURE_LAUNCHER"
''')
    for name, messages, cargo_exit, lines, expected, state in (
        ("pass", complete, 0, 1, 0, "COMPLETE"),
        ("prose-only", [diagnostic("unused_imports", "clippy::vec_box is prose")] + complete,
         0, 1, 0, "COMPLETE"),
        ("lint", [lint, lint] + complete, 0, 1, 0, "COMPLETE_WITH_DEBT"),
        ("file-debt", complete, 0, 501, 0, "COMPLETE_WITH_DEBT"),
        ("combined-debt", [lint] + complete, 0, 501, 0, "COMPLETE_WITH_DEBT"),
        ("stale-output", [], 0, 1, 1, "INCOMPLETE"),
        ("compiler", complete, 101, 1, 1, "INCOMPLETE"),
        ("threshold-denied", [diagnostic("clippy::too_many_lines", "too many lines (101/100)", "error")]
         + complete, 101, 1, 1, "INCOMPLETE"),
        ("missing-cargo", [], 127, 1, 1, "INCOMPLETE"),
        ("default-denied", [diagnostic("clippy::never_loop", "never loops", "error")]
         + complete, 0, 1, 1, "INCOMPLETE"),
        ("unknown-lint", [diagnostic("unknown_lints", "unknown lint: clippy::vec_box")]
         + complete, 0, 1, 1, "INCOMPLETE"),
        ("empty", [], 0, 1, 1, "INCOMPLETE"),
        ("partial", complete[:-1], 0, 1, 1, "INCOMPLETE"),
        ("malformed", None, 0, 1, 1, "INCOMPLETE"),
        ("missing-manifest", complete, 0, 1, 1, "INCOMPLETE"),
    ):
        source.write_text("fn fixture() {}\n" + "// fixture\n" * (lines - 1))
        fixture.write_text("not JSON\n" if messages is None else
                           "".join(json.dumps(item) + "\n" for item in messages))
        if name == "missing-manifest":
            (project / "packages/web-ui/Cargo.toml").unlink()
        output = root / "output"
        # Reuse one output directory: passing reports must not survive failure.
        environment = dict(os.environ, PROJECT_ROOT=str(project), OUTPUT_DIR=str(output),
                           CARGO_NET_OFFLINE="true", FIXTURE_MESSAGES=str(fixture),
                           FIXTURE_EXIT=str(cargo_exit), FIXTURE_LAUNCHER=launcher)
        result = subprocess.run(["bash", str(wrapper)], env=environment,
                                capture_output=True, text=True)
        assert result.returncode == expected, (name, result.stdout, result.stderr)
        statuses = json.loads((output / "clippy-status.json").read_text())
        assert len(statuses) == 2, name
        if state == "INCOMPLETE":
            assert any(item["exit_status"] != 0 for item in statuses), name
            if cargo_exit:
                assert all(item["exit_status"] == cargo_exit for item in statuses), name
        else:
            assert all(item["exit_status"] == 0 for item in statuses), name
        violations = json.loads((output / "complexity-violations.json").read_text())
        assert len(violations) == (1 if name in ("lint", "combined-debt", "threshold-denied") else 0), name
        report = ET.parse(output / "junit-report.xml").getroot()
        suite = report.find("testsuite")
        cases = suite.findall("testcase")
        for element in (report, suite):
            assert int(element.attrib["tests"]) == len(cases), name
            assert int(element.attrib["failures"]) == len(suite.findall("testcase/failure")), name
            assert int(element.attrib["errors"]) == len(suite.findall("testcase/error")), name
        assert not suite.findall("testcase/failure"), name
        assert bool(suite.findall("testcase/error")) == (state == "INCOMPLETE"), name
        measurements = json.loads((output / "analysis-summary.json").read_text())
        assert measurements["analysis_status"] == state, name
        assert measurements["thresholds_blocking"] is False, name
        assert measurements["files"] == 1 and measurements["functions"] == 1, name
        assert measurements["files_over_500_lines"] == (1 if lines > 500 else 0), name
        assert measurements["selected_lint_findings"] == len(violations), name
        assert measurements["thresholds"]["file_lines"] == 500, name
        if name in ("lint", "combined-debt"):
            assert "101/100" in violations[0]["message"], name
            assert "101/100" in "".join(suite.itertext()), name
        if not expected:
            assert measurements["debt_status"] == (
                "MEASURED_DEBT" if state == "COMPLETE_WITH_DEBT" else "NO_SELECTED_DEBT"), name
        assert f"Status: {state}" in (output / "summary.md").read_text(), name
        assert f"Analysis status: {state}" in (output / "index.html").read_text(), name
        print(f"PASS {name}: exit={result.returncode}, status={state}")

    for name, source_text in (("zero-code", "// no measurable code\n"),
                              ("zero-sources", None)):
        if source_text is None:
            source.unlink()
        else:
            source.write_text(source_text)
        result = subprocess.run(["bash", str(wrapper)], env=environment,
                                capture_output=True, text=True)
        assert result.returncode != 0, name
        assert not (output / "summary.md").exists(), name
        assert not (output / "index.html").exists(), name
        assert not (output / "clippy-status.json").exists(), name
        assert not (output / "analysis-summary.json").exists(), name
        assert ET.parse(output / "junit-report.xml").findall(".//error"), name
        print(f"PASS {name}: exit={result.returncode}")
