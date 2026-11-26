"""Regression tests for packages/ci/nix-ci.sh and its use in .gitlab-ci.yml.

A fake `nix` on PATH records every invocation, answers `config show` from the
`--option` arguments it receives, simulates the Niks3 health probe, and prints
Nix-style progress lines for `build`. No network, store or daemon is used.

Run: python3 packages/ci/tests/test_nix_ci.py  (needs PyYAML)
"""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import yaml

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "packages/ci/nix-ci.sh"
NIKS3 = "https://niks3.lan.aicampground.com"
NIXOS = "https://cache.nixos.org"
PRIVATE_MARKER = "private-runner-value-must-not-leak"

FAKE_NIX = r'''#!PYTHON
import json, os, sys
args = sys.argv[1:]
log = os.environ["FAKE_LOG"]
with open(log, "a") as handle:
    handle.write(json.dumps(args) + "\n")
if args[:1] == ["--version"]:
    print("nix (Nix) 2.34.8")
    sys.exit(0)
options = {}
i = 0
while i < len(args):
    if args[i] == "--option":
        options[args[i + 1]] = args[i + 2]
        i += 3
        continue
    if args[i] == "--fallback":
        options["fallback"] = "true"
    i += 1
if args[:1] == ["config"] and "show" in args:
    name = args[args.index("show") + 1]
    print("private-diagnostic " + os.environ.get("MARKER", ""), file=sys.stderr)
    if os.environ.get("BAD_POLICY") == name:
        print("wrong")
    else:
        print(options.get(name, os.environ.get("MARKER", "")))
    sys.exit(0)
if args[:2] == ["store", "info"]:
    sys.exit(1 if os.environ.get("CACHE_DOWN") else 0)
if args[:1] == ["build"]:
    print("building '/nix/store/aaaa-one.drv'...", file=sys.stderr)
    print("building '/nix/store/bbbb-two.drv'...", file=sys.stderr)
    print("copying path '/nix/store/cccc' from 'NIKS3'...", file=sys.stderr)
    print("copying path '/nix/store/dddd' from 'NIKS3'...", file=sys.stderr)
    print("copying path '/nix/store/eeee' from 'NIXOS'...", file=sys.stderr)
    print("copying path '/nix/store/ffff' from 'https://user:secret@other.invalid'...",
          file=sys.stderr)
    sys.exit(int(os.environ.get("BUILD_EXIT", "0")))
sys.exit(0)
'''


def run(arguments, directory, **extra):
    fake = directory / "nix"
    fake.write_text(
        FAKE_NIX.replace("#!PYTHON", "#!" + sys.executable)
        .replace("NIKS3", NIKS3).replace("NIXOS", NIXOS)
    )
    fake.chmod(0o755)
    log = directory / "argv.jsonl"
    log.unlink(missing_ok=True)
    metrics = directory / "metrics"
    if metrics.exists():
        for item in metrics.iterdir():
            item.unlink()
    environment = {
        "PATH": f"{directory}:{os.environ['PATH']}",
        "FAKE_LOG": str(log),
        "CF_CI_METRICS_DIR": str(metrics),
        "CI_JOB_NAME": "unit",
        "MARKER": PRIVATE_MARKER,
        **extra,
    }
    result = subprocess.run(["sh", str(SCRIPT), *arguments], capture_output=True,
                            text=True, env=environment, cwd=directory)
    calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
    records = []
    if (metrics / "nix.jsonl").exists():
        records = [json.loads(line) for line in (metrics / "nix.jsonl").read_text().splitlines()]
    assert PRIVATE_MARKER not in result.stdout + result.stderr, "inherited setting leaked"
    own = [line for line in (result.stdout + result.stderr).splitlines()
           if line.startswith(("NixCi", "CacheHealth", "nix-ci:"))]
    assert "secret" not in "\n".join(own) + json.dumps(records), "unknown cache URL recorded"
    return result, calls, records


def builds(calls):
    return [call for call in calls if call[:1] == ["build"]]


def main():
    with tempfile.TemporaryDirectory(prefix="cf-nix-ci-") as temporary:
        directory = Path(temporary)

        # Default: full cache policy, host resource settings inherited.
        result, calls, records = run(["build", ".#default.cf-server-core-drv", "-L"], directory)
        assert result.returncode == 0, result.stderr
        [build] = builds(calls)
        assert build[build.index("substituters") + 1] == f"{NIKS3} {NIXOS}"
        assert "niks3-campground-1:" in build[build.index("trusted-public-keys") + 1]
        assert build[build.index("require-sigs") + 1] == "true"
        assert build[build.index("accept-flake-config") + 1] == "false"
        assert "--fallback" in build
        for flag in ("--builders", "--max-jobs", "--cores"):
            assert flag not in build, flag
        assert "CacheHealth niks3=available" in result.stdout
        [record] = records
        assert record == {
            "job": "unit", "command": "build",
            "installables": [".#default.cf-server-core-drv"], "exit": 0,
            "seconds": record["seconds"], "cache_health": "available", "built": 2,
            "substituted": {"niks3": 2, "cache.nixos.org": 1, "other": 1},
        }, record

        # Validated resource overrides.
        result, calls, _ = run(["build", ".#x"], directory, CF_CI_NIX_MAX_JOBS="4",
                               CF_CI_NIX_CORES="8", CF_CI_NIX_LOCAL_ONLY="1")
        assert result.returncode == 0
        [build] = builds(calls)
        assert build[-6:] == ["--max-jobs", "4", "--cores", "8", "--builders", ""], build
        for variable, value in (("CF_CI_NIX_MAX_JOBS", "0"), ("CF_CI_NIX_MAX_JOBS", "x"),
                                ("CF_CI_NIX_CORES", "-1"), ("CF_CI_NIX_LOCAL_ONLY", "yes")):
            result, calls, _ = run(["build", ".#x"], directory, **{variable: value})
            assert result.returncode == 2 and not builds(calls), (variable, value)

        # An unavailable cache is reported, recorded, and falls back.
        result, calls, records = run(["build", ".#x"], directory, CACHE_DOWN="1")
        assert result.returncode == 0 and builds(calls)
        assert "CacheHealth niks3=unavailable" in result.stdout
        assert "WARNING Niks3 is unavailable" in result.stderr
        assert records[0]["cache_health"] == "unavailable"
        result, calls, _ = run(["build", ".#x"], directory, CACHE_DOWN="1",
                               CF_CI_REQUIRE_PRIVATE_CACHE="1")
        assert result.returncode == 1 and not builds(calls)

        # The build status is the script status, and is recorded.
        result, _, records = run(["build", ".#x"], directory, BUILD_EXIT="17")
        assert result.returncode == 17 and records[0]["exit"] == 17

        # A policy that Nix does not apply fails before any build.
        for setting in ("substituters", "trusted-public-keys", "require-sigs",
                        "accept-flake-config", "fallback"):
            result, calls, _ = run(["build", ".#x"], directory, BAD_POLICY=setting)
            assert result.returncode == 1 and not builds(calls), setting

        # Non-build subcommands get the policy but no probe or metrics.
        result, calls, records = run(["develop", ".#sqlx", "-c", "true"], directory)
        assert result.returncode == 0 and not records
        assert calls[-1][0] == "develop" and "--fallback" in calls[-1]
        assert not any(call[:2] == ["store", "info"] for call in calls)

        result, calls, _ = run(["--check-config"], directory)
        assert result.returncode == 0 and not builds(calls)
        assert run([], directory)[0].returncode == 2

    check_pipeline_uses_policy()
    check_pipeline_schedule()
    print("PASS test_nix_ci")


def script_lines(value):
    if isinstance(value, str):
        return value.splitlines()
    return [line for item in value for line in script_lines(item)]


def check_pipeline_uses_policy():
    """Every acquiring Nix command in CI uses nix-ci.sh; offline execution
    blocks keep their deliberate no-substitute, no-builder restrictions."""
    pipeline = yaml.safe_load((ROOT / ".gitlab-ci.yml").read_text())
    assert not (ROOT / "packages/ci/public-cache-build.sh").exists()
    for name, job in pipeline.items():
        if not isinstance(job, dict):
            continue
        for key in ("before_script", "script"):
            text = "\n".join(script_lines(job.get(key, [])))
            assert "--max-jobs 1 --cores 2" not in text.replace("\\\n", " ") or \
                "--offline" in text, (name, "blanket resource restriction")
            for line in text.splitlines():
                stripped = line.strip()
                for command in ("nix build", "nix develop", "nix shell"):
                    if stripped.startswith(command):
                        assert "--offline" in stripped, (name, stripped)
                assert "public-cache-build" not in stripped, (name, stripped)
                if "--builders ''" in stripped or "builders ''" in stripped:
                    assert "--offline" in text, (name, stripped)



def check_pipeline_schedule():
    """The DAG encodes only real dependencies, and cancellation never stops
    deployment, publication or version bumps."""
    pipeline = yaml.safe_load((ROOT / ".gitlab-ci.yml").read_text())
    assert pipeline["workflow"]["auto_cancel"]["on_new_commit"] == "interruptible"
    assert pipeline["default"]["interruptible"] is True
    for name in ("docs-pages", "auto-bump-patch", "manual-version-bump"):
        assert pipeline[name]["interruptible"] is False, name
    assert pipeline["complexity-check"]["needs"] == [], "complexity-check waits"
    assert pipeline["docs-pages"]["needs"] == ["docs-site-check"]
    coverage_inputs = set(pipeline["coverage-check"]["needs"])
    for name, job in pipeline.items():
        if not isinstance(job, dict) or name == "coverage-check":
            continue
        text = "\n".join(script_lines(job.get("script", [])))
        if "coverage-artifacts" in text or job.get("extends") == "state-machine-tests":
            assert name in coverage_inputs, (name, "coverage producer not awaited")
    for name in coverage_inputs:
        assert name in pipeline, name
    for name, job in pipeline.items():
        if isinstance(job, dict):
            assert "allow_failure" not in job, (name, "required gate made optional")
    builds = [line for line in script_lines(pipeline["component-tests"]["script"])
              if "nix-ci.sh build" in line]
    assert len(builds) == 1, "component builds must share one Nix invocation"


if __name__ == "__main__":
    main()
