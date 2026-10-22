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
            elif fault in ("cargo-failure", "empty-selection"):
                runner = temporary / f"{fault}.sh"
                runner.write_text(
                    "#!/usr/bin/env bash\nexit 73\n"
                    if fault == "cargo-failure"
                    else '#!/usr/bin/env bash\nexec "$@" --skip ""\n'
                )
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
            with log.open("w") as output:
                process = subprocess.Popen(
                    ["bash", "-euo", "pipefail", "-c", command],
                    env=environment, stdout=output, stderr=subprocess.STDOUT,
                )
                try:
                    if fault == "interruption":
                        for _ in range(600):
                            if marker.exists() or process.poll() is not None:
                                break
                            time.sleep(0.1)
                        assert marker.exists(), "interruption never reached ready child"
                        process.send_signal(signal.SIGTERM)
                        marker.unlink()
                    # A cold CI target must compile the unchanged workspace
                    # before Cargo invokes the controlled runner. Bound that
                    # compilation separately from setup/receipt-only faults.
                    timeout = 1200 if fault in ("cargo-failure", "empty-selection") else 180
                    status = process.wait(timeout=timeout)
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
            elif fault == "missing-completion":
                assert "did not produce an exit-status receipt" in text
            elif fault == "malformed-completion":
                assert "Invalid state-machine test exit-status receipt" in text
            elif fault == "receipt-over-compose":
                assert "Database completion: test_exit=73 compose_exit=0" in text
            if fault != "initialization":
                assert "database system is shut down" in text
                assert "PANIC:" not in text, "Storage was removed before PostgreSQL stopped"
            # Every fixture uses a new launcher-owned data/socket pair. Verify
            # shutdown removed the actual paths reported by the real children.
            for path in re.findall(r"/tmp/cf-sm-sockets\.[A-Za-z0-9]+", text):
                assert not Path(path).exists(), f"Leaked socket directory: {path}"
            for path in re.findall(r"data_root=(\S+)", text):
                assert not Path(path).exists(), f"Leaked data directory: {path}"
            result = dict(app=app, fault=fault, exit=status, invoking_uid=os.geteuid())
            results.append(result)
            print(json.dumps(result), flush=True)
            (evidence / "results.json").write_text(json.dumps(results, indent=2) + "\n")
