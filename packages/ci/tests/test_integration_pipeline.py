"""Regression tests for the generated integration child pipeline.

1. packages/ci/integration-pipeline.nix is evaluated with real Nix against
   synthetic owner checks: one job per scenario, and evaluation fails closed
   for an empty owner, an aggregate that differs from its scenarios, an unsafe
   name, or no owners.
2. packages/ci/integration-owner-build.sh runs against a fake `nix` in scenario
   mode: success collects the gate's evidence; failure copies an existing run
   output without building it; a missing run output keeps the build status.
3. .gitlab-ci.yml wires the generated pipeline with `strategy: depend`.

Run: python3 packages/ci/tests/test_integration_pipeline.py  (needs PyYAML, nix)
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

import yaml

ROOT = Path(__file__).resolve().parents[3]
GENERATOR = ROOT / "packages/ci/integration-pipeline.nix"
BUILD = ROOT / "packages/ci/integration-owner-build.sh"


def generate(checks_expression):
    """Evaluates the generator with synthetic checks; returns (ok, value)."""
    expression = f"(import {GENERATOR}) ({checks_expression})"
    result = subprocess.run(
        ["nix-instantiate", "--eval", "--strict", "--json", "--expr", expression],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        return False, result.stderr
    return True, json.loads(result.stdout)


def synthetic(owner_scenarios, entries_override=None, extra=""):
    """Builds a checks attrset whose gates are plain attrsets with outPath."""
    owners = []
    for owner, scenarios in owner_scenarios.items():
        gates = " ".join(
            f'"{name}" = {{ outPath = "/nix/store/{owner}-{name}"; }};' for name in scenarios
        )
        entries = entries_override.get(owner) if entries_override else None
        if entries is None:
            entries = " ".join(f'"{name}" = "/nix/store/{owner}-{name}";' for name in scenarios)
        owners.append(f'"{owner}" = {{ scenarios = {{ {gates} }}; entries = {{ {entries} }}; }};')
    return "{ " + " ".join(owners) + f' "unrelated" = {{ }}; {extra} }}'


def check_generator():
    ok, pipeline = generate(synthetic({
        "integration-access": ["one"],
        "integration-delivery": ["two", "three"],
    }))
    assert ok, pipeline
    assert pipeline["stages"] == ["integration-access", "integration-delivery"]
    jobs = {name: job for name, job in pipeline.items() if name != "stages"}
    assert sorted(jobs) == ["integration-access/one", "integration-delivery/three",
                            "integration-delivery/two"], jobs
    for name, job in jobs.items():
        owner, scenario = name.split("/")
        assert job["needs"] == [] and job["stage"] == owner
        assert job["script"] == [f"sh packages/ci/integration-owner-build.sh {owner} {scenario}"]
        assert job["artifacts"]["when"] == "always"
        assert job["retry"]["when"] == ["runner_system_failure"]
        assert "allow_failure" not in job
    failures = {
        "empty owner": synthetic({"integration-access": []}),
        "aggregate differs": synthetic(
            {"integration-access": ["one", "two"]},
            entries_override={"integration-access": '"one" = "/nix/store/integration-access-one";'}),
        "different gate output": synthetic(
            {"integration-access": ["one"]},
            entries_override={"integration-access": '"one" = "/nix/store/stale";'}),
        "unsafe name": synthetic({"integration-access": ["bad name"]}),
        "no owners": '{ "unrelated" = { }; }',
    }
    for description, expression in failures.items():
        ok, error = generate(expression)
        assert not ok, description
        assert "integration pipeline" in error, (description, error)
    print("PASS generator")


FAKE_NIX = r'''#!PYTHON
import json, os, sys
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as handle:
    handle.write(json.dumps(args) + "\n")
if args[:1] == ["--version"]:
    print("nix (Nix) 2.34.8"); sys.exit(0)
options, i = {}, 0
while i < len(args):
    if args[i] == "--option":
        options[args[i + 1]] = args[i + 2]; i += 3; continue
    if args[i] == "--fallback":
        options["fallback"] = "true"
    i += 1
if args[:1] == ["config"] and "show" in args:
    print(options.get(args[args.index("show") + 1], "")); sys.exit(0)
if args[:2] == ["store", "info"]:
    sys.exit(0)
if args[:1] == ["build"]:
    if os.environ.get("BUILD_EXIT", "0") != "0":
        sys.exit(int(os.environ["BUILD_EXIT"]))
    link = args[args.index("--out-link") + 1]
    os.makedirs(os.path.join(link, "evidence"), exist_ok=True)
    with open(os.path.join(link, "evidence", "result.json"), "w") as handle:
        handle.write('{"status": "passed"}')
    if os.environ.get("NO_EVIDENCE"):
        import shutil; shutil.rmtree(os.path.join(link, "evidence"))
    sys.exit(0)
if args[:1] == ["eval"]:
    print("scenario-a " + os.environ["RUN_PATH"], end=""); sys.exit(0)
sys.exit(0)
'''


def run_build(directory, **extra):
    fake = directory / "bin/nix"
    fake.parent.mkdir(exist_ok=True)
    fake.write_text(FAKE_NIX.replace("#!PYTHON", "#!" + sys.executable))
    fake.chmod(0o755)
    work = directory / "work"
    shutil.rmtree(work, ignore_errors=True)
    (work / "packages/ci").mkdir(parents=True)
    for name in ("nix-ci.sh", "integration-owner-build.sh"):
        shutil.copy(ROOT / "packages/ci" / name, work / "packages/ci" / name)
    log = directory / "argv.jsonl"
    log.unlink(missing_ok=True)
    environment = {"PATH": f"{fake.parent}:{os.environ['PATH']}", "FAKE_LOG": str(log),
                   "RUN_PATH": str(directory / "missing-run"), **extra}
    result = subprocess.run(
        ["sh", "packages/ci/integration-owner-build.sh", "integration-access", "scenario-a"],
        capture_output=True, text=True, env=environment, cwd=work)
    calls = [json.loads(line) for line in log.read_text().splitlines()]
    return result, calls, work / "integration-evidence/integration-access"


def check_scenario_build():
    with tempfile.TemporaryDirectory(prefix="cf-integration-") as temporary:
        directory = Path(temporary)
        result, calls, evidence = run_build(directory)
        assert result.returncode == 0, result.stderr
        [build] = [call for call in calls if call[:1] == ["build"]]
        assert ".#checks.x86_64-linux.integration-access.scenarios.scenario-a" in build
        assert (evidence / "scenario-a/result.json").exists()
        assert (evidence / "scenario-a.nix-build.log").exists()

        # A successful gate without evidence is not a complete result.
        result, _, _ = run_build(directory, NO_EVIDENCE="1")
        assert result.returncode == 1, result.stderr

        # Failure: an existing run output is copied, never built; the build
        # status is the job status.
        run = directory / "run-output"
        run.mkdir()
        (run / "result.json").write_text('{"status": "failed"}')
        result, calls, evidence = run_build(directory, BUILD_EXIT="17", RUN_PATH=str(run))
        assert result.returncode == 17, result.stderr
        assert (evidence / "scenario-a/result.json").read_text() == '{"status": "failed"}'
        builds = [call for call in calls if call[:1] == ["build"]]
        assert len(builds) == 1 and not any(".run" in arg for arg in builds[0])
        [evaluation] = [call for call in calls if call[:1] == ["eval"]]
        assert "scenario.run.outPath" in " ".join(evaluation)

        # Failure without a run output keeps the build status.
        result, _, evidence = run_build(directory, BUILD_EXIT="23")
        assert result.returncode == 23 and not (evidence / "scenario-a").exists()

        bad = subprocess.run(["sh", str(BUILD), "integration-access", "a b"],
                             capture_output=True, text=True)
        assert bad.returncode == 2
    print("PASS scenario build")


def check_parent_pipeline():
    pipeline = yaml.safe_load((ROOT / ".gitlab-ci.yml").read_text())
    for owner in ("integration-access", "integration-delivery",
                  "integration-assurance", "integration-upgrade"):
        assert owner not in pipeline, f"{owner} must run through the child pipeline"
    generator = pipeline["integration-pipeline"]
    assert generator["needs"] == []
    assert generator["script"] == ["sh packages/ci/integration-pipeline.sh"]
    assert "integration-pipeline.yml" in generator["artifacts"]["paths"]
    trigger = pipeline["integration-scenarios"]
    assert trigger["needs"] == ["integration-pipeline"]
    assert trigger["trigger"]["strategy"] == "depend"
    assert trigger["trigger"]["include"] == [
        {"artifact": "integration-pipeline.yml", "job": "integration-pipeline"}]
    assert "allow_failure" not in trigger
    print("PASS parent pipeline")


if __name__ == "__main__":
    check_generator()
    check_scenario_build()
    check_parent_pipeline()
    print("PASS test_integration_pipeline")
