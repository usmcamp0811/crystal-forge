"""Checks installed coverage execution and failure propagation without Rust builds.

argv[1] is the installed coverage-report. Only cargo-tarpaulin is replaced;
the installed shell, jq validation, aggregation, and JUnit run unchanged.
"""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET


launcher = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="coverage-launcher-") as temporary:
    root = Path(temporary)
    project = root / "project"
    package = project / "packages/default"
    package.mkdir(parents=True)
    (package / "Cargo.toml").touch()
    fixture = root / "tarpaulin.json"
    wrapper = root / "run.sh"
    wrapper.write_text('''set -euo pipefail
cargo-tarpaulin() {
    [[ "$*" == 'tarpaulin --out Html --out Json --output-dir '* ]] || return 90
    [[ "$*" == *' --all-features --workspace --timeout 300' ]] || return 91
    [[ "$SQLX_OFFLINE" == true && "$CARGO_NET_OFFLINE" == true && "$CARGO_BUILD_JOBS" == 2 ]] || return 92
    [[ "$RUSTC" == /nix/store/*/bin/rustc && -x "$RUSTC" && -x "$RUSTDOC" ]] || return 93
    [[ -f "$OPENSSL_INCLUDE_DIR/openssl/ssl.h" && -d "$OPENSSL_LIB_DIR" ]] || return 94
    pkg-config --exists openssl || return 95
    while [[ $# -gt 0 ]]; do
        if [[ "$1" == --output-dir ]]; then destination="$2"; break; fi
        shift
    done
    if [[ "$FIXTURE_MODE" != missing ]]; then
        cp "$FIXTURE_JSON" "$destination/tarpaulin-report.json"
    fi
    if [[ "$FIXTURE_MODE" != missing-html ]]; then
        printf '<html>controlled report</html>\n' > "$destination/tarpaulin-report.html"
    fi
    return "$FIXTURE_EXIT"
}
export -f cargo-tarpaulin
exec "$FIXTURE_LAUNCHER"
''')
    output = root / "output"
    for name, covered, coverable, cargo_exit, expected in (
        ("pass", 7, 10, 0, 0),
        ("zero-covered", 0, 10, 0, 0),
        ("failed-analysis", 7, 10, 101, 1),
        ("missing-tool", 7, 10, 127, 1),
        ("missing", 7, 10, 0, 1),
        ("missing-html", 7, 10, 0, 1),
        ("malformed", 7, 10, 0, 1),
        ("wrong-schema", 7, 10, 0, 1),
        ("zero-denominator", 0, 0, 0, 1),
        ("invalid-count", 11, 10, 0, 1),
        ("missing-manifest", 7, 10, 0, 1),
    ):
        report = {"files": [{"covered": covered, "coverable": coverable}],
                  "covered": covered, "coverable": coverable}
        if name == "wrong-schema":
            report = {"files": [{"covered": 7, "line_count": 10}]}
        fixture.write_text("not JSON" if name == "malformed" else json.dumps(report))
        if name == "missing-manifest":
            (package / "Cargo.toml").unlink()
        environment = dict(os.environ, PROJECT_ROOT=str(project), OUTPUT_DIR=str(output),
                           CARGO_NET_OFFLINE="true", FIXTURE_MODE=name,
                           FIXTURE_JSON=str(fixture), FIXTURE_EXIT=str(cargo_exit),
                           FIXTURE_LAUNCHER=launcher)
        result = subprocess.run(["bash", str(wrapper)], env=environment,
                                capture_output=True, text=True)
        assert result.returncode == expected, (name, result.stdout, result.stderr)
        junit = ET.parse(output / "junit-report.xml").getroot()
        if expected:
            assert junit.findall(".//error"), name
            assert not (output / "coverage.json").exists(), name
            assert not (output / "summary.md").exists(), name
            assert not (output / "index.html").exists(), name
        else:
            data = json.loads((output / "coverage.json").read_text())
            assert data["summary"]["total_lines"] == coverable, name
            assert data["summary"]["total_covered"] == covered, name
            assert len(junit.findall(".//testcase")) == 1, name
            assert not junit.findall(".//failure") and not junit.findall(".//error"), name
            assert (output / "packages-default-coverage.html").exists(), name
        print(f"PASS {name}: exit={result.returncode}")
