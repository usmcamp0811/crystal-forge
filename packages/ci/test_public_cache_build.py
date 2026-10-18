"""Checks scoped CI routing, real Nix policy, and fixture source realization.

Run with `nix develop -c python3 packages/ci/test_public_cache_build.py`.
The proxy delegates configuration queries to real Nix and records build argv.
Synthetic unavailable caches must be removed even when inherited through both
base and extra settings. A fresh instance of the integration test-flake builder
proves source realization without running a VM gate. No runner settings are printed.
"""

import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

import yaml


ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "packages/ci/public-cache-build.sh"
POLICY = [
    "--option", "substituters", "https://cache.nixos.org",
    "--option", "extra-substituters", "",
    "--option", "accept-flake-config", "false",
    "--option", "connect-timeout", "10",
    "--option", "stalled-download-timeout", "30",
    "--option", "download-attempts", "2",
    "--option", "require-sigs", "true",
    "--fallback",
]
LOCAL_EXECUTION = ["--builders", "", "--max-jobs", "1", "--cores", "2"]

old = yaml.safe_load(subprocess.check_output(
    ["git", "show", "HEAD:.gitlab-ci.yml"], cwd=ROOT, text=True))
new = yaml.safe_load((ROOT / ".gitlab-ci.yml").read_text())
# The manual `legacy-flake-check` job shares the `flake-check` definition, so
# its routing is still checked here. Only its matrix and scheduling differ.
MATRIX_NAMES = (new["flake-check"]["parallel"]["matrix"][0]["CHECK_NAME"]
                + new["legacy-flake-check"]["parallel"]["matrix"][0]["CHECK_NAME"])
LOCAL_CHECKS = set(MATRIX_NAMES)
changed = {"flake-check", "web-ui-check", "web-ui-baseline-candidates",
           "complexity-check", "coverage-check"}
assert old.keys() == new.keys()
for job in old:
    if job not in changed:
        assert old[job] == new[job], job
    else:
        expected = copy.deepcopy({k: v for k, v in old[job].items() if k != "script"})
        assert expected == {
            k: v for k, v in new[job].items() if k != "script"}, job
        expected_scripts = copy.deepcopy(old[job]["script"][1:])
        if job == "flake-check":
            # Only the new shipped-UI alias also exports the existing artifacts.
            # The replacement is idempotent after the CI source is committed.
            expected_scripts[1] = expected_scripts[1].replace(
                'if [ "$CHECK_NAME" = "web-ui" ]; then',
                'if [ "$CHECK_NAME" = "web-ui" ] || [ "$CHECK_NAME" = "niks3-ui" ]; then')
        assert expected_scripts == new[job]["script"][1:], job

with tempfile.TemporaryDirectory(prefix="cf-ci-policy-") as temporary:
    directory = Path(temporary)
    real_nix = shutil.which("nix")
    assert real_nix
    for name in ("builders", "builders-use-substitutes", "max-jobs", "cores"):
        before = subprocess.check_output([real_nix, "config", "show", name])
        after = subprocess.check_output([real_nix, *POLICY, "config", "show", name])
        assert before == after, name
    proxy = directory / "nix"
    proxy.write_text(f"#!{sys.executable}\n" + """
import json, os, subprocess, sys
if os.environ.get('OLD_CLI'):
    if 'config' in sys.argv:
        print('unsupported command private-marker', file=sys.stderr)
        sys.exit(1)
    if 'show-config' in sys.argv:
        result = subprocess.run([os.environ['REAL_NIX'], *sys.argv[1:]],
                                capture_output=True, text=True)
        if result.returncode:
            sys.exit(result.returncode)
        lines = result.stdout.splitlines()
        mode = os.environ.get('LEGACY_MODE', '')
        if mode in ('missing', 'empty', 'duplicate', 'conflict', 'malformed'):
            lines = [line for line in lines if not line.startswith('require-sigs = ')]
            if mode == 'empty':
                lines.append('require-sigs = ')
            elif mode in ('duplicate', 'conflict'):
                lines.extend(['require-sigs = true', 'require-sigs = ' +
                              ('true' if mode == 'duplicate' else 'false')])
            elif mode == 'malformed':
                lines.append('require-sigs=true')
        elif mode == 'unsupported':
            print('private-marker unsupported settings')
            print('private-marker unsupported settings', file=sys.stderr)
            sys.exit(1)
        # These full-body markers must never reach helper stdout or stderr.
        lines.extend(['unrelated-secret = private-marker',
                      'substituters-extra = https://unknown.invalid/private-marker'])
        print('\\n'.join(lines))
        sys.exit(0)
if sys.argv[1] in ('build', 'run'):
    with open(os.environ['ARGV_RECORD'], 'w') as record:
        json.dump(sys.argv[1:], record)
    variable = 'BUILD_EXIT' if sys.argv[1] == 'build' else 'RUN_EXIT'
    sys.exit(int(os.environ.get(variable, '0')))
if os.environ.get('BAD_EFFECTIVE') and '--option' in sys.argv:
    print('https://unknown.invalid/private-marker')
    sys.exit(0)
sys.exit(subprocess.call([os.environ['REAL_NIX'], *sys.argv[1:]]))
""")
    proxy.chmod(0o700)
    env = dict(os.environ, REAL_NIX=real_nix, ARGV_RECORD=str(directory / "argv"))
    env["PATH"] = str(directory) + os.pathsep + env["PATH"]
    # Closed loopback port models an unavailable inherited provider. The extra
    # cache has a synthetic credential marker to detect accidental disclosure.
    env["NIX_CONFIG"] = """substituters = http://127.0.0.1:1
extra-substituters = https://private-marker@attic.aicampground.com/test
accept-flake-config = true
connect-timeout = 300
stalled-download-timeout = 300
download-attempts = 5
require-sigs = true
builders = ssh://private-marker@builder.invalid x86_64-linux - 1 1
builders-use-substitutes = true
max-jobs = 3
cores = 4
"""
    inherited_controls = {}
    for name in ("builders", "builders-use-substitutes", "max-jobs", "cores"):
        before = subprocess.check_output([real_nix, "config", "show", name], env=env)
        after = subprocess.check_output([real_nix, *POLICY, "config", "show", name], env=env)
        assert before == after, name
        inherited_controls[name] = before

    def run(arguments, **extra):
        result = subprocess.run(["sh", str(HELPER), *arguments],
                                env=dict(env, **extra), capture_output=True, text=True)
        assert "private-marker" not in result.stdout + result.stderr
        assert "127.0.0.1" not in result.stdout + result.stderr
        return result

    route = new["flake-check"]["script"][0]
    assert new["legacy-flake-check"]["script"][0] == route
    for name in MATRIX_NAMES:
        result = subprocess.run(["sh", "-euc", route], cwd=ROOT,
                                env=dict(env, CHECK_NAME=name), capture_output=True, text=True)
        assert result.returncode == 0, name
        recorded = json.loads((directory / "argv").read_text())
        arguments = [f".#checks.x86_64-linux.{name}", "-L", "--show-trace"]
        local = name in LOCAL_CHECKS
        if local:
            arguments.extend(LOCAL_EXECUTION)
        scoped = name in {"integration", "oidc-auth", "server-regressions", "niks3-cache", "niks3-write-probe", "niks3-ui",
                          "nixos-discovery-reliability", "attic-metrics", "attic-publication",
                          "builder-evaluator-packaging"}
        assert recorded == ["build", *(POLICY if scoped else []), *arguments], name
        assert result.stdout.count("InheritedInternalCache=") == int(scoped), name
        # Query real Nix with the recorded build's exact configuration flags.
        # Compare private inherited values in memory; never print those values.
        controls = recorded[1:]
        controls = [arg for arg in controls if arg not in
                    {arguments[0], "-L", "--show-trace"}]
        for setting in inherited_controls:
            effective = subprocess.check_output(
                [real_nix, *controls, "config", "show", setting], env=env)
            expected = ({"builders": b"\n", "max-jobs": b"1\n", "cores": b"2\n"}
                        .get(setting, inherited_controls[setting]) if local
                        else inherited_controls[setting])
            assert effective == expected, (name, setting)
    for job in ("web-ui-check", "web-ui-baseline-candidates"):
        result = subprocess.run(["sh", "-euc", new[job]["script"][0]], cwd=ROOT,
                                env=env, capture_output=True, text=True)
        assert result.returncode == 0, job
        impure = ["--impure"] if job.endswith("candidates") else []
        local = LOCAL_EXECUTION if job == "web-ui-check" else []
        assert json.loads((directory / "argv").read_text()) == [
            "build", *POLICY, *impure, ".#checks.x86_64-linux.web-ui", "-L", "--show-trace", *local]
        assert result.stdout.count("InheritedInternalCache=true") == 1
        for setting, inherited in inherited_controls.items():
            effective = subprocess.check_output(
                [real_nix, *POLICY, *local, "config", "show", setting], env=env)
            expected = ({"builders": b"\n", "max-jobs": b"1\n", "cores": b"2\n"}
                        .get(setting, inherited) if local else inherited)
            assert effective == expected, (job, setting)
    for job, installable in (("complexity-check", ".#code-metrics.complexity-report"),
                             ("coverage-check", ".#coverage.coverage-report")):
        script = new[job]["script"]
        assert len(script) == 1, job
        result = subprocess.run(["sh", "-euc", script[0]], cwd=ROOT,
                                env=env, capture_output=True, text=True)
        assert result.returncode == 0, job
        recorded = json.loads((directory / "argv").read_text())
        assert recorded == ["run", installable, *LOCAL_EXECUTION], job
        assert "private-marker" not in result.stdout + result.stderr
        for setting in inherited_controls:
            effective = subprocess.check_output(
                [real_nix, *LOCAL_EXECUTION, "config", "show", setting], env=env)
            expected = {"builders": b"\n", "max-jobs": b"1\n", "cores": b"2\n"}.get(
                setting, inherited_controls[setting])
            assert effective == expected, (job, setting)
        failed = subprocess.run(["sh", "-euc", script[0]], cwd=ROOT,
                                env=dict(env, RUN_EXIT="17"), capture_output=True, text=True)
        assert failed.returncode == 17, job
    configured = run(["--check-config"])
    assert configured.returncode == 0
    for setting in ("require-sigs=true", "fallback=true"):
        assert f"Effective {setting}" in configured.stdout
    (directory / "argv").unlink()
    assert run([".#checks.x86_64-linux.niks3-cache"], BAD_EFFECTIVE="1").returncode == 1
    assert not (directory / "argv").exists()
    assert run([".#checks.x86_64-linux.niks3-cache"], BUILD_EXIT="17").returncode == 17
    # An old CLI rejects config show but supports legacy show-config. Exercise
    # real legacy configuration parsing, including the exact same build argv.
    result = run([".#checks.x86_64-linux.builder-evaluator-packaging", "-L"], OLD_CLI="1")
    assert result.returncode == 0
    assert result.stdout.count("InheritedInternalCache=true") == 1
    assert json.loads((directory / "argv").read_text()) == [
        "build", *POLICY, ".#checks.x86_64-linux.builder-evaluator-packaging", "-L"]
    assert run(["--check-config"], OLD_CLI="1").returncode == 0
    for mode in ("missing", "empty", "duplicate", "conflict", "malformed", "unsupported"):
        (directory / "argv").unlink(missing_ok=True)
        assert run([".#checks.x86_64-linux.niks3-cache"],
                   OLD_CLI="1", LEGACY_MODE=mode).returncode == 1, mode
        assert not (directory / "argv").exists(), mode

    # Exercise the real integration fixture builder without launching its VMs.
    # A unique derivation name prevents an existing local/cached fixture from
    # satisfying this assertion. Only the name changes; the runCommand body and
    # locked inputs remain the repository's actual test-flake implementation.
    fixture_name = "crystal-forge-test-flake-ci-fallback-" + directory.name
    expression = f"""
      let
        flake = builtins.getFlake {json.dumps('git+file://' + str(ROOT))};
        pkgs = flake.inputs.nixpkgs.legacyPackages.x86_64-linux;
        fixture = import {ROOT}/lib/test-flake/default.nix {{
          lib = pkgs.lib; inputs = flake.inputs; system = "x86_64-linux";
        }};
      in fixture.testFlake.overrideAttrs (_: {{ name = {json.dumps(fixture_name)}; }})
    """
    real_env = dict(env, PATH=os.pathsep.join(
        part for part in env["PATH"].split(os.pathsep) if part != str(directory)))
    # The fresh source-build proof uses the original local-builder environment.
    real_env["NIX_CONFIG"] = env["NIX_CONFIG"].replace(
        "builders = ssh://private-marker@builder.invalid x86_64-linux - 1 1",
        "builders =")
    built = subprocess.run(
        ["sh", str(HELPER), "--impure", "--no-link", "--print-out-paths",
         "--expr", expression], cwd=ROOT, env=real_env,
        capture_output=True, text=True, timeout=180)
    assert built.returncode == 0, "integration fixture source build failed"
    assert fixture_name + ".drv'" in built.stderr, "fixture builder did not execute"
    output = Path(built.stdout.splitlines()[-1])
    assert output.is_dir() and output.name.endswith(fixture_name)
    for branch, count in (("MAIN", 5), ("DEVELOPMENT", 7), ("FEATURE", 3)):
        assert (output / f"{branch}_COMMIT_COUNT").read_text().strip() == str(count)
        head = (output / f"{branch}_HEAD").read_text().strip()
        commits = (output / f"{branch}_COMMITS").read_text().splitlines()
        assert len(commits) == count and commits[-1] == head
    assert "private-marker" not in built.stdout + built.stderr
    assert "Effective require-sigs=true" in built.stdout
    assert "Effective fallback=true" in built.stdout

print("PASS: modern/legacy Nix policy, inherited builder controls equal=true, "
      "all matrix and web-ui-check local execution cases equal=true, "
      "manual baseline inherited controls equal=true, scoped argv, secret redaction, "
      "missing/empty/duplicate/conflicting/unsupported settings, exit status, "
      "real integration fixture source build with signatures and fallback enabled")
