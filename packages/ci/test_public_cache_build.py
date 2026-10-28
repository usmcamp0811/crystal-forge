"""Checks scoped CI routing, real Nix policy, and fixture source realization.

Run with the pinned nixpkgs Python environment including PyYAML.
The proxy delegates configuration queries to real Nix and records build argv.
Synthetic unavailable caches must be removed even when inherited through both
base and extra settings. A fresh instance of the integration test-flake builder
proves source realization without running a VM gate. No runner settings are printed.
"""

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

new = yaml.safe_load((ROOT / ".gitlab-ci.yml").read_text())
# Source retirement removes the legacy jobs, not their accepted replacement
# obligations. Only integration owners belong in the flake check namespace.
assert not {"flake-check", "legacy-flake-check", "web-ui-check",
            "web-ui-baseline-candidates", "package-verification"}.intersection(new)

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
if os.environ.get('METRICS_RECORD') and sys.argv[1] in ('build', 'develop', 'run'):
    # Record only allowlisted effective settings. The real CLI receives the
    # invocation's actual policy flags and inherited offline-phase environment.
    controls = []
    arguments = sys.argv[2:]
    index = 0
    while index < len(arguments):
        argument = arguments[index]
        if argument == '--option':
            controls.extend(arguments[index:index + 3])
            index += 3
        elif argument in ('--builders', '--max-jobs', '--cores'):
            controls.extend(arguments[index:index + 2])
            index += 2
        elif argument == '--offline':
            controls.append(argument)
            index += 1
        elif argument == '-c':
            break
        else:
            index += 1
    effective = json.loads(subprocess.check_output(
        [os.environ['REAL_NIX'], *controls, 'config', 'show', '--json'], text=True))
    settings = {key: effective[key]['value'] for key in (
        'substituters', 'substitute', 'builders', 'builders-use-substitutes',
        'fallback', 'accept-flake-config', 'max-jobs', 'cores')}
    nested_settings = None
    if sys.argv[1] == 'run':
        nested = json.loads(subprocess.check_output(
            [os.environ['REAL_NIX'], 'config', 'show', '--json'], text=True))
        nested_settings = {key: nested[key]['value'] for key in settings}
    record = os.environ['METRICS_RECORD']
    try:
        with open(record) as previous:
            invocation = sum(1 for _ in previous)
    except FileNotFoundError:
        invocation = 0
    with open(record, 'a') as output:
        output.write(json.dumps(dict(argv=sys.argv[1:], settings=settings,
                                    nested_settings=nested_settings,
                                    cargo_offline=os.environ.get('CARGO_NET_OFFLINE'),
                                    sqlx_offline=os.environ.get('SQLX_OFFLINE'))) + '\\n')
    if sys.argv[1] == 'run':
        sys.exit(int(os.environ.get('RUN_EXIT', '0')))
    sys.exit(23 if invocation == int(os.environ.get('METRICS_FAIL_AT', '-1')) else 0)
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

    components = ["cf-server-core-drv", "cf-server-drv", "cf-agent-drv",
                  "cf-builder-drv", "cf-keygen-drv"]
    # Execute the real default CI loop. Append records so every component's
    # policy and failure propagation are checked, including early loop exits.
    record = directory / "components.jsonl"
    for failed_component in [-1, *range(len(components))]:
        record.unlink(missing_ok=True)
        result = subprocess.run(
            ["sh", "-euc", new["component-tests"]["script"][0]], cwd=ROOT,
            env=dict(env, CF_COVERAGE="0", METRICS_RECORD=str(record),
                     METRICS_FAIL_AT=str(failed_component)),
            capture_output=True, text=True,
        )
        assert result.returncode == (0 if failed_component < 0 else 23), failed_component
        assert "private-marker" not in result.stdout + result.stderr
        assert "127.0.0.1" not in result.stdout + result.stderr
        recorded = [json.loads(line) for line in record.read_text().splitlines()]
        expected = (components if failed_component < 0
                    else components[:failed_component + 1])
        assert [entry["argv"] for entry in recorded] == [
            ["build", *POLICY, f".#default.{component}", "-L", "--show-trace",
             *LOCAL_EXECUTION] for component in expected
        ]
        assert result.stdout.count("InheritedInternalCache=true") == len(expected)
        for entry in recorded:
            settings = entry["settings"]
            assert settings["substituters"] == ["https://cache.nixos.org"]
            assert settings["accept-flake-config"] is False
            assert settings["fallback"] is True
            assert not settings["builders"]
            assert settings["builders-use-substitutes"] is True
            assert str(settings["max-jobs"]) == "1" and settings["cores"] == 2
        print(f"PASS component loop: fault={failed_component}, "
              f"propagated exit={result.returncode}, builds={len(recorded)}")

    # Optional variants use the same wrapper. Probe their argv without copying
    # profile artifacts or executing component test bodies in this routing test.
    component_builds = [
        ([f".#default.{component}.coverage"], True) for component in components
    ] + [
        ([".#nixos-options-metadata", ".#oscal-fixture", ".#okf-knowledge",
          ".#xccdf-1-2-schemas"], False),
    ]
    for installables, scoped in component_builds:
        arguments = [*installables, "-L", "--show-trace", *LOCAL_EXECUTION]
        if scoped:
            result = run(arguments)
            assert run(arguments, BUILD_EXIT="17").returncode == 17, installables
        else:
            result = subprocess.run(
                ["sh", "-euc", new["component-tests"]["script"][1]], cwd=ROOT,
                env=env, capture_output=True, text=True)
        assert result.returncode == 0, installables
        recorded = json.loads((directory / "argv").read_text())
        assert recorded == ["build", *(POLICY if scoped else []), *arguments], installables
        assert result.stdout.count("InheritedInternalCache=") == int(scoped), installables
        # Query real Nix with the recorded build's exact configuration flags.
        # Compare private inherited values in memory; never print those values.
        controls = recorded[1:]
        controls = [arg for arg in controls if arg not in
                    {*installables, "-L", "--show-trace"}]
        for setting in inherited_controls:
            effective = subprocess.check_output(
                [real_nix, *controls, "config", "show", setting], env=env)
            expected = {"builders": b"\n", "max-jobs": b"1\n", "cores": b"2\n"}.get(
                setting, inherited_controls[setting])
            assert effective == expected, (installables, setting)
    # Complexity preparation may acquire dependencies. Only the final analysis
    # phase is offline. Execute the actual YAML script through the same probe;
    # this checks orchestration and real CLI policy, not application analysis.
    online = [
        "--option", "substitute", "true",
        "--option", "substituters", "https://cache.nixos.org",
        "--option", "extra-substituters", "",
        "--option", "accept-flake-config", "false", *LOCAL_EXECUTION,
    ]
    offline = [
        "--offline", "--option", "substituters", "",
        "--option", "substitute", "false", "--builders", "",
        "--option", "fallback", "false",
        "--option", "accept-flake-config", "false",
        "--max-jobs", "1", "--cores", "2",
    ]
    for job, installable, manifests in (
        ("complexity-check", ".#code-metrics.complexity-report",
         ["packages/default/Cargo.toml", "packages/web-ui/Cargo.toml"]),
    ):
        script = new[job]["script"]
        assert len(script) == len(manifests) + 2, job
        expected_preparation = [["build", installable, "--no-link", *online]] + [
            ["develop", ".#sqlx", *online, "-c", "env", "CARGO_NET_OFFLINE=false",
             "cargo", "fetch", "--locked", "--manifest-path", manifest]
            for manifest in manifests
        ]
        expected_run = ["run", *offline, installable]
        record = directory / "metrics.jsonl"
        cases = [(0, -1), (17, -1)] + [
            (23, index) for index in range(len(expected_preparation))
        ]
        for expected_status, failed_preparation in cases:
            record.unlink(missing_ok=True)
            result = subprocess.run(
                ["sh", "-euc", "\n".join(script)], cwd=ROOT,
                env=dict(env, METRICS_RECORD=str(record),
                         METRICS_FAIL_AT=str(failed_preparation),
                         RUN_EXIT="17" if expected_status == 17 else "0"),
                capture_output=True, text=True,
            )
            assert result.returncode == expected_status, (job, expected_status)
            assert "private-marker" not in result.stdout + result.stderr
            assert "127.0.0.1" not in result.stdout + result.stderr
            recorded = [json.loads(line) for line in record.read_text().splitlines()]
            expected = (expected_preparation[:failed_preparation + 1]
                        if failed_preparation >= 0 else [*expected_preparation, expected_run])
            assert [entry["argv"] for entry in recorded] == expected, job
            for entry in recorded:
                settings = entry["settings"]
                assert not settings["builders"], job
                assert str(settings["max-jobs"]) == "1" and settings["cores"] == 2, job
                assert settings["accept-flake-config"] is False, job
                if entry["argv"][0] == "run":
                    assert entry["nested_settings"] == settings, job
                    assert entry["cargo_offline"] == "true", job
                    assert entry["sqlx_offline"] == "true", job
                    assert settings["substituters"] == [], job
                    for key in ("substitute", "builders-use-substitutes", "fallback"):
                        assert settings[key] is False, (job, key)
                else:
                    assert settings["substituters"] == ["https://cache.nixos.org"], job
                    assert settings["substitute"] is True, job
            print(f"PASS {job}: preparation fault={failed_preparation}, "
                  f"propagated exit={result.returncode}, "
                  f"analysis started={failed_preparation < 0}")
    configured = run(["--check-config"])
    assert configured.returncode == 0
    for setting in ("require-sigs=true", "fallback=true"):
        assert f"Effective {setting}" in configured.stdout
    (directory / "argv").unlink()
    assert run([".#checks.x86_64-linux.integration-access"], BAD_EFFECTIVE="1").returncode == 1
    assert not (directory / "argv").exists()
    assert run([".#checks.x86_64-linux.integration-access"], BUILD_EXIT="17").returncode == 17
    # An old CLI rejects config show but supports legacy show-config. Exercise
    # real legacy configuration parsing, including the exact same build argv.
    result = run([".#default.cf-server-core-drv", "-L"], OLD_CLI="1")
    assert result.returncode == 0
    assert result.stdout.count("InheritedInternalCache=true") == 1
    assert json.loads((directory / "argv").read_text()) == [
        "build", *POLICY, ".#default.cf-server-core-drv", "-L"]
    assert run(["--check-config"], OLD_CLI="1").returncode == 0
    for mode in ("missing", "empty", "duplicate", "conflict", "malformed", "unsupported"):
        (directory / "argv").unlink(missing_ok=True)
        assert run([".#checks.x86_64-linux.integration-access"],
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
      "component build local execution cases equal=true, "
      "scoped argv, secret redaction, "
      "missing/empty/duplicate/conflicting/unsupported settings, exit status, "
      "real integration fixture source build with signatures and fallback enabled")
