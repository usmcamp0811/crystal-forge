"""Exercises database CI failure paths through the declared offline nix-run entry.

Preparation is explicit and separate. Run with the repository's Nix Python and
SQLx toolchains; root execution requires a disposable root runner or user
namespace. Overrides modify only process-compose fault inputs in temporary
files. They never modify Rust tests, migrations, or a persistent database.
"""

import copy
import json
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import sys
import tempfile
import time


root = Path.cwd()
evidence = Path(sys.argv[1]).resolve()
evidence.mkdir(parents=True, exist_ok=True)
ci = (root / ".gitlab-ci.yml").read_text()
execution = re.search(
    r"      export CARGO_NET_OFFLINE=true SQLX_OFFLINE=true\n(.*?)(?=\n  \S)",
    ci,
    re.S,
).group(1)
execution = "export CARGO_NET_OFFLINE=true SQLX_OFFLINE=true\n" + "\n".join(
    line.removeprefix("      ") for line in execution.splitlines()
)
assert execution.count("-- -t=false up") == 1
# Deliver interruption to the launcher PID, not an intermediate CI shell that
# can leave its foreground child running after that shell terminates.
execution = execution.replace("\nnix run ", "\nexec nix run ", 1)
offline = [
    "--offline", "--option", "substituters", "", "--option", "substitute", "false",
    "--option", "builders", "", "--option", "fallback", "false",
    "--option", "accept-flake-config", "false", "--max-jobs", "0",
    "--no-update-lock-file",
]
results = []


def process_identity(pid):
    """Reads the reported task postmaster's state and PID-reuse identity."""
    if not Path("/proc").is_dir():
        return {"unavailable": True}
    try:
        # comm can contain spaces/parentheses. Fields after its final ')' start
        # with field 3; field 22 is the kernel starttime, not wall-clock time.
        stat = (Path("/proc") / str(pid) / "stat").read_text()
        fields = stat[stat.rfind(")") + 2:].split()
        return {"state": fields[0], "starttime": int(fields[19])}
    except (FileNotFoundError, ProcessLookupError):
        return None
    except PermissionError:
        return {"unavailable": True}


def main_postmasters(text):
    """Selects main-service startup PIDs, excluding db-init's temporary server."""
    return set(map(int, re.findall(
        r"^\[db\s*\].*?\[(\d+)\] LOG:  starting PostgreSQL", text, re.M
    )))


def verify_cleanup(text, observed, initialization=False):
    """Requires actual invocation paths and main-server shutdown/death evidence."""
    reports = re.findall(
        r"^Database invocation: runner_uid=\d+ data_root=(\S+) socket_root=(\S+)$",
        text, re.M,
    )
    assert len(reports) == 1, "Expected one nonempty invocation path report"
    data_roots = {report[0] for report in reports}
    socket_roots = {report[1] for report in reports}
    assert data_roots and socket_roots and data_roots.isdisjoint(socket_roots)
    for paths, prefix in ((data_roots, "cf-state-machine."), (socket_roots, "cf-sm-sockets.")):
        for path in paths:
            assert Path(path).is_absolute() and Path(path).name.startswith(prefix)
            assert not Path(path).exists(), f"Leaked invocation directory: {path}"
    pids = main_postmasters(text)
    if initialization:
        assert not pids, "Initialization fault unexpectedly started the main server"
    else:
        assert len(pids) == 1, "Expected one actual main PostgreSQL startup identity"
    servers = []
    for pid in sorted(pids):
        prefix = rf"^\[db\s*\].*?\[{pid}\] LOG:  "
        assert re.search(prefix + r"database system is shut down\s*$", text, re.M), (
            f"Missing main PostgreSQL shutdown for PID {pid}"
        )
        socket_root = next(iter(socket_roots))
        assert re.search(
            prefix + r'listening on Unix socket "' + re.escape(socket_root)
            + r'/\.s\.PGSQL\.\d+"', text, re.M,
        ), "Main postmaster does not match the reported invocation socket"
        before = observed.get(pid)
        after = process_identity(pid)
        if after is None:
            # hidepid can make an unobservable PID look absent. Claim /proc
            # death corroboration only after observing its live starttime.
            death = "pid-absent" if before and "starttime" in before else "proc-unobserved"
        elif after.get("unavailable"):
            death = "proc-unavailable"
        elif before and not before.get("unavailable") and before["starttime"] != after["starttime"]:
            death = "pid-reused"
        else:
            assert after["state"] in ("Z", "X"), f"Main PostgreSQL PID {pid} is still alive"
            death = "dead-process"
        servers.append(dict(pid=pid, observed=before, after=after, death=death))
    assert "PANIC:" not in text, "Storage was removed before PostgreSQL stopped"
    return dict(data_roots=sorted(data_roots), socket_roots=sorted(socket_roots), main_postmasters=servers)


with tempfile.TemporaryDirectory(prefix="cf-database-ci-") as temporary:
    temporary = Path(temporary)
    for app in (sys.argv[2:] or ("cve-test", "dashboard-visibility-test")):
        assert app in ("cve-test", "dashboard-visibility-test")
        launcher = subprocess.check_output(
            ["nix", "eval", *offline, "--raw", f".#devScripts.{app}.outPath"],
            text=True,
        ).strip()
        launcher_text = (Path(launcher) / "bin/process-compose").read_text()
        compose = re.search(
            r"(/nix/store/[^\s]+/bin/process-compose) \"\$@\"", launcher_text
        ).group(1)
        config = re.search(r"PC_CONFIG_FILES=(\S+)", Path(compose).read_text()).group(1)
        original = json.loads(Path(config).read_text())
        for fault, expected in (
            ("initialization", 1),
            ("cargo-failure", 73),
            ("empty-selection", 1),
            ("unrelated-positive", 1),
            ("missing-completion", 1),
            ("malformed-completion", 1),
            ("receipt-over-compose", 73),
            ("interruption", 143),
        ):
            config_data = copy.deepcopy(original)
            processes = config_data["processes"]
            tests = processes["state-machine-tests"]
            if fault == "initialization":
                # Exercise real initdb failure, not a fabricated status receipt.
                processes["db-init"]["command"] = (
                    'initdb --invalid-task501-option -D "$CF_STATE_MACHINE_DATA_DIR"'
                )
            elif fault in ("cargo-failure", "empty-selection", "unrelated-positive"):
                runner = temporary / f"{fault}.sh"
                body = (
                    "exit 73\n"
                    if fault == "cargo-failure"
                    else 'exec "$@" --skip ""\n'
                )
                if fault == "unrelated-positive":
                    # Keep the real selected count (including empty workspace
                    # targets), but replace every executed name. A positive or
                    # even equal count must not prove intended-suite execution.
                    body = '''listing=$("$@" --list) || exit $?
count=0
while IFS= read -r line; do
  if [[ "$line" == *": test" ]]; then count=$((count + 1)); fi
done <<< "$listing"
printf 'running %s tests\\n' "$count"
for ((index=0; index<count; index++)); do
  printf 'test task501_unrelated::case_%s ... ok\\n' "$index"
done
printf 'test result: ok. %s passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\\n' "$count"
'''
                # Listings always reach the real native libtest binary. Only
                # execution is controlled; no Rust test bodies run in this fault.
                runner.write_text('''#!/usr/bin/env bash
set -euo pipefail
for argument in "$@"; do
  if [[ "$argument" == --list ]]; then exec "$@"; fi
done
''' + body)
                runner.chmod(0o755)
                tests["environment"].append(
                    f"CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER={runner}"
                )
            elif fault == "missing-completion":
                tests["command"] = "exit 0"
            elif fault == "malformed-completion":
                tests["command"] = 'printf "invalid\\n" > "$CF_STATE_MACHINE_STATUS_FILE"'
            elif fault == "receipt-over-compose":
                tests["command"] = 'printf "73\\n" > "$CF_STATE_MACHINE_STATUS_FILE"; exit 0'
            elif fault == "interruption":
                marker = temporary / "interrupt-ready"
                tests["command"] = f"touch {shlex.quote(str(marker))}; sleep 60"
            fault_config = temporary / f"{app}-{fault}.json"
            fault_config.write_text(json.dumps(config_data))
            command = execution.replace(
                "-- -t=false up", f"-- -f {shlex.quote(str(fault_config))} -t=false up"
            )
            log = evidence / f"{app}-{fault}.log"
            environment = dict(os.environ, DATABASE_TEST_APP=app)
            observed = {}
            with log.open("w") as output:
                process = subprocess.Popen(
                    ["bash", "-euo", "pipefail", "-c", command],
                    env=environment, stdout=output, stderr=subprocess.STDOUT,
                )
                try:
                    # A cold CI target must compile the unchanged workspace
                    # before Cargo invokes the controlled runner. Bound that
                    # compilation separately from setup/receipt-only faults.
                    timeout = 1200 if fault in ("cargo-failure", "empty-selection", "unrelated-positive") else 180
                    deadline = time.monotonic() + timeout
                    interrupted = False
                    while process.poll() is None:
                        current = log.read_text()
                        for pid in main_postmasters(current):
                            if not observed.get(pid):
                                observed[pid] = process_identity(pid)
                        if fault == "interruption" and not interrupted and marker.exists() and main_postmasters(current):
                            process.send_signal(signal.SIGTERM)
                            marker.unlink()
                            interrupted = True
                        if time.monotonic() >= deadline:
                            raise subprocess.TimeoutExpired(command, timeout)
                        time.sleep(0.1)
                    status = process.wait()
                    if fault == "interruption":
                        assert interrupted, "Interruption never reached ready child"
                except BaseException:
                    process.terminate()
                    process.wait(timeout=30)
                    raise
            text = log.read_text()
            assert status == expected, (app, fault, status, expected, str(log))
            if fault == "initialization":
                assert "invalid-task501-option" in text
                assert "did not produce an exit-status receipt" in text
            elif fault == "empty-selection":
                assert "Database test selection executed no successful tests" in text
            elif fault == "cargo-failure":
                assert "exit status: 73" in text
            elif fault == "unrelated-positive":
                assert "Database test selection membership mismatch" in text
                assert "task501_unrelated::case_" in text
                counts = re.findall(r"DATABASE_MEMBERSHIP expected=(\d+) emitted=(\d+) exit=1", text)
                assert counts and all(int(a) == int(b) > 0 for a, b in counts)
            elif fault == "missing-completion":
                assert "did not produce an exit-status receipt" in text
            elif fault == "malformed-completion":
                assert "Invalid state-machine test exit-status receipt" in text
            elif fault == "receipt-over-compose":
                assert "Database completion: test_exit=73 compose_exit=0" in text
            cleanup = verify_cleanup(text, observed, initialization=fault == "initialization")
            rejected_mutations = []
            if fault == "interruption":
                # db-init's earlier shutdown remains in this fixture. The
                # verifier must reject absence of the later actual DB shutdown.
                mutations = {
                    "missing-main-shutdown": re.sub(
                        r"^\[db\s*\].*database system is shut down.*$", "", text, flags=re.M
                    ),
                    "missing-path-report": re.sub(r"^Database invocation:.*$", "", text, flags=re.M),
                }
                for mutation, mutated in mutations.items():
                    (evidence / f"{app}-{mutation}.fixture.log").write_text(mutated)
                    try:
                        verify_cleanup(mutated, observed)
                    except AssertionError as error:
                        required = "main PostgreSQL shutdown" if mutation == "missing-main-shutdown" else "invocation path report"
                        assert required in str(error), str(error)
                        rejected_mutations.append(mutation)
                    else:
                        raise AssertionError(f"Cleanup verifier accepted {mutation}")
            result = dict(app=app, fault=fault, exit=status, invoking_uid=os.geteuid(), cleanup=cleanup, rejected_mutations=rejected_mutations)
            results.append(result)
            print(json.dumps(result), flush=True)
            (evidence / "results.json").write_text(json.dumps(results, indent=2) + "\n")
