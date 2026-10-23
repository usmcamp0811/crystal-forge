"""Checks installed coverage execution with real invocation-owned PostgreSQL.

argv[1] is the installed coverage-report. Only cargo-tarpaulin is replaced;
database startup, ownership checks, migrations, cleanup, report validation and
JUnit run unchanged. Controlled measurements do not prove product coverage.
"""

import copy
import concurrent.futures
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import re
import signal
import time
import xml.etree.ElementTree as ET


launcher = str(Path(sys.argv[1]).resolve())
repository = Path(__file__).resolve().parents[2]
# Component CI builds the installed launcher first. Prepare the SQLx shell and
# locked Cargo metadata separately, before any actual-entrypoint offline case.
# This is explicit preparation, never a dependency fallback from an analysis.
subprocess.run([
    'nix', 'develop', str(repository) + '#sqlx',
    '--option', 'substitute', 'true', '--option', 'substituters', 'https://cache.nixos.org',
    '--option', 'extra-substituters', '', '--option', 'accept-flake-config', 'false',
    '--builders', '', '--max-jobs', '1', '--cores', '2',
    '-c', 'env', 'CARGO_NET_OFFLINE=false', 'cargo', 'fetch', '--locked',
    '--manifest-path', str(repository / 'packages/default/Cargo.toml'),
], cwd=repository, check=True)


def verify_cleanup(text, expect_main=True):
    paths = re.findall(r'Database invocation: runner_uid=\d+ data_root=(\S+) socket_root=(\S+)', text)
    assert len(paths) == 1
    assert all(not Path(path).exists() for path in paths[0])
    pids = set(re.findall(r'^\[db\s*\].*?\[(\d+)\] LOG:  starting PostgreSQL', text, re.M))
    assert len(pids) == int(expect_main), pids
    for pid in pids:
        assert re.search(r'^\[db\s*\].*?\[' + pid + r'\] LOG:  database system is shut down\s*$', text, re.M)


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
    [[ "$DATABASE_URL" == postgresql://crystal_forge@127.0.0.1:35433/crystal_forge ]] || return 96
    [[ "$(psql -XAt -c 'SHOW data_directory')" == "$CF_STATE_MACHINE_DATA_DIR" ]] || return 97
    [[ "$(psql -XAt -c 'SHOW shared_preload_libraries')" == pg_stat_statements ]] || return 98
    if [[ "$FIXTURE_MODE" == interruption ]]; then
        touch "$FIXTURE_READY"
        sleep 120
    fi
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
        ("failed-analysis", 7, 10, 101, 101),
        ("missing-tool", 7, 10, 127, 127),
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
        environment = dict(os.environ, PROJECT_ROOT=str(project if name == "missing-manifest" else repository), OUTPUT_DIR=str(output),
                           CARGO_NET_OFFLINE="true", FIXTURE_MODE=name,
                           FIXTURE_JSON=str(fixture), FIXTURE_EXIT=str(cargo_exit),
                           FIXTURE_LAUNCHER=launcher)
        result = subprocess.run(["bash", str(wrapper)], env=environment,
                                capture_output=True, text=True)
        assert result.returncode == expected, (name, result.returncode, result.stdout, result.stderr)
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

        if name != "missing-manifest":
            verify_cleanup(result.stdout)
            assert (output / 'database-exit-status').read_text().strip() == str(expected), name
            assert (output / 'packages/default/tarpaulin-exit-status').read_text().strip() == str(cargo_exit), name

    # Actual entrypoint faults reuse the same installed TASK-501 composition.
    # They must stop before any controlled measurement can be accepted.
    original = json.loads((output / 'database-compose.json').read_text())
    for fault, expected in (("initialization", 1), ("wrong-database", 1),
                            ("missing-receipt", 1), ("receipt-over-compose", 73)):
        config = copy.deepcopy(original)
        tests = config['processes']['state-machine-tests']
        if fault == 'initialization':
            config['processes']['db-init']['command'] = 'initdb --invalid-task501-option -D "$CF_STATE_MACHINE_DATA_DIR"'
        elif fault == 'wrong-database':
            tests['command'] = 'export CF_STATE_MACHINE_DATA_DIR=/tmp/not-the-coverage-invocation; ' + tests['command']
        elif fault == 'missing-receipt':
            tests['command'] = 'exit 0'
        else:
            tests['command'] = 'printf "73\\n" > "$CF_STATE_MACHINE_STATUS_FILE"; exit 0'
        config_path = root / (fault + '.json')
        config_path.write_text(json.dumps(config))
        environment['PROJECT_ROOT'] = str(repository)
        result = subprocess.run([launcher, '--', '-f', str(config_path), '-t=false', 'up'],
                                env=environment, capture_output=True, text=True, timeout=180)
        assert result.returncode == expected, (fault, result.returncode, result.stdout, result.stderr)
        assert not (output / 'coverage.json').exists(), fault
        assert not (output / 'packages/default/tarpaulin-report.json').exists(), fault
        assert ET.parse(output / 'junit-report.xml').findall('.//error'), fault
        verify_cleanup(result.stdout, expect_main=fault != 'initialization')
        print(f'PASS {fault}: exit={result.returncode}, fresh database paths cleaned')

    ready = root / 'interrupt-ready'
    environment.update(PROJECT_ROOT=str(repository), FIXTURE_MODE='interruption',
                       FIXTURE_READY=str(ready))
    process = subprocess.Popen(['bash', str(wrapper)], env=environment,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    # Drain through communicate in a thread: migration logs can exceed a pipe.
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
        completed = executor.submit(process.communicate)
        deadline = time.monotonic() + 180
        while not ready.exists() and process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.1)
        reached_instrumenter = ready.exists()
        process.send_signal(signal.SIGTERM)
        stdout, stderr = completed.result(timeout=40)
    assert reached_instrumenter, ('Interruption did not reach instrumenter after verified DB setup', stdout, stderr)
    assert process.returncode == 143, (process.returncode, stdout, stderr)
    verify_cleanup(stdout)
    assert ET.parse(output / 'junit-report.xml').findall('.//error')
    print('PASS interruption: exit=143, PostgreSQL shutdown and path cleanup verified')
