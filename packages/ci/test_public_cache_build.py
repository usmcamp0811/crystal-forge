"""Checks scoped CI routing and real Nix configuration without building gates.

Run with `nix develop -c python3 packages/ci/test_public_cache_build.py`.
The proxy delegates configuration queries to real Nix and records build argv.
Synthetic unavailable caches must be removed even when inherited through both
base and extra settings. No runner configuration values are printed.
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

old = yaml.safe_load(subprocess.check_output(
    ["git", "show", "HEAD:.gitlab-ci.yml"], cwd=ROOT, text=True))
new = yaml.safe_load((ROOT / ".gitlab-ci.yml").read_text())
changed = {"flake-check", "web-ui-check", "web-ui-baseline-candidates"}
assert old.keys() == new.keys()
for job in old:
    if job not in changed:
        assert old[job] == new[job], job
    else:
        assert {k: v for k, v in old[job].items() if k != "script"} == {
            k: v for k, v in new[job].items() if k != "script"}, job
        assert old[job]["script"][1:] == new[job]["script"][1:], job

with tempfile.TemporaryDirectory(prefix="cf-ci-policy-") as temporary:
    directory = Path(temporary)
    real_nix = shutil.which("nix")
    assert real_nix
    for name in ("builders", "builders-use-substitutes", "max-jobs"):
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
if sys.argv[1] == 'build':
    with open(os.environ['ARGV_RECORD'], 'w') as record:
        json.dump(sys.argv[1:], record)
    sys.exit(int(os.environ.get('BUILD_EXIT', '0')))
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
builders =
builders-use-substitutes = true
max-jobs = 3
"""
    for name in ("builders", "builders-use-substitutes", "max-jobs"):
        before = subprocess.check_output([real_nix, "config", "show", name], env=env)
        after = subprocess.check_output([real_nix, *POLICY, "config", "show", name], env=env)
        assert before == after, name

    def run(arguments, **extra):
        result = subprocess.run(["bash", str(HELPER), *arguments],
                                env=dict(env, **extra), capture_output=True, text=True)
        assert "private-marker" not in result.stdout + result.stderr
        assert "127.0.0.1" not in result.stdout + result.stderr
        return result

    route = new["flake-check"]["script"][0]
    for name in old["flake-check"]["parallel"]["matrix"][0]["CHECK_NAME"]:
        result = subprocess.run(["bash", "-euc", route], cwd=ROOT,
                                env=dict(env, CHECK_NAME=name), capture_output=True, text=True)
        assert result.returncode == 0, name
        recorded = json.loads((directory / "argv").read_text())
        arguments = [f".#checks.x86_64-linux.{name}", "-L", "--show-trace"]
        scoped = name in {"server-regressions", "niks3-cache", "builder-evaluator-packaging"}
        assert recorded == ["build", *(POLICY if scoped else []), *arguments], name
        assert result.stdout.count("InheritedInternalCache=") == int(scoped), name
    for job in ("web-ui-check", "web-ui-baseline-candidates"):
        result = subprocess.run(["bash", "-euc", new[job]["script"][0]], cwd=ROOT,
                                env=env, capture_output=True, text=True)
        assert result.returncode == 0, job
        impure = ["--impure"] if job.endswith("candidates") else []
        assert json.loads((directory / "argv").read_text()) == [
            "build", *POLICY, *impure, ".#checks.x86_64-linux.web-ui", "-L", "--show-trace"]
        assert result.stdout.count("InheritedInternalCache=true") == 1
    assert run(["--check-config"]).returncode == 0
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

print("PASS: modern/legacy Nix policy, builder controls, scoped argv, secret redaction, "
      "missing/empty/duplicate/conflicting/unsupported settings, exit status")
