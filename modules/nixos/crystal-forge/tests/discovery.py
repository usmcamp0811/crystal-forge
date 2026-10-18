"""Checks real cold/warm discovery without external inputs or shared caches.

Run through discovery.sh, which owns the disposable store and working directory.
This exercises Nix fetch/evaluation behavior, not server orchestration or static
source contracts. The 65-second delay distinguishes the historical 60-second
deadline from the supported 300-second discovery deadline.
"""

import base64
import hashlib
import http.server
import io
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import subprocess
import sys
import tarfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request


DELAY_SECONDS = 65
DISCOVERY_DEADLINE = 300
output = Path(sys.argv[2])
workspace = Path.cwd()
username = secrets.token_hex(12)
password = secrets.token_hex(24)
authorization = "Basic " + base64.b64encode(
    f"{username}:{password}".encode()
).decode()
requests = {"authorized": 0, "rejected": 0, "completed": 0, "unexpected": 0}
request_lock = threading.Lock()
archive = b""


class PrivateInput(http.server.BaseHTTPRequestHandler):
    """Serves one locked archive after an intentional cold-fetch delay."""

    def log_message(self, *_args):
        # HTTP logs must never include authorization or generated credentials.
        pass

    def do_GET(self):
        if self.path != "/private.tar.gz":
            with request_lock:
                requests["unexpected"] += 1
            self.send_error(404)
            return
        if self.headers.get("Authorization") != authorization:
            with request_lock:
                requests["rejected"] += 1
            self.send_response(401)
            self.send_header("WWW-Authenticate", 'Basic realm="fixture"')
            self.end_headers()
            return
        with request_lock:
            requests["authorized"] += 1
        time.sleep(DELAY_SECONDS)
        try:
            self.send_response(200)
            self.send_header("Content-Type", "application/gzip")
            self.send_header("Content-Length", str(len(archive)))
            self.end_headers()
            self.wfile.write(archive)
        except (BrokenPipeError, ConnectionResetError):
            # The historical deadline intentionally disconnects the first fetch.
            pass
        finally:
            with request_lock:
                requests["completed"] += 1


def run(label, command, deadline=DISCOVERY_DEADLINE, env=None, timeout=False):
    """Records redaction-checked diagnostics and reaps the isolated process group."""
    start = time.monotonic()
    child = subprocess.Popen(
        command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, start_new_session=True, env=env,
    )
    timed_out = False
    try:
        stdout, stderr = child.communicate(timeout=deadline)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(child.pid, signal.SIGKILL)
        stdout, stderr = child.communicate(timeout=10)
    elapsed = time.monotonic() - start
    for secret in (username, password, authorization):
        assert secret not in stdout + stderr, "credential appeared in diagnostics"
    (output / f"{label}.stdout").write_text(stdout)
    (output / f"{label}.stderr").write_text(stderr)
    assert timed_out == timeout, f"{label}: unexpected deadline outcome"
    print(f"{label}: elapsed={elapsed:.3f}s exit={child.returncode} timeout={timed_out}", flush=True)
    return child.returncode, stdout, stderr, elapsed


def nar_hash(path, env=None):
    code, stdout, _, _ = run(
        f"hash-{path.name}", ["nix", "hash", "path", "--type", "sha256", "--sri", str(path)],
        env=env,
    )
    assert code == 0
    return stdout.strip()


def qualified(path, digest):
    return f"path:{path}?narHash={urllib.parse.quote(digest, safe='')}"


def ingest(label, source, digest, lock_bytes, env=None):
    """Adds and verifies a source in the selected disposable local store."""
    code, stdout, _, _ = run(label, [
        "nix", "store", "add-path", "--name", "source", str(source),
    ], env=env)
    assert code == 0
    logical_path = Path(stdout.strip())
    store_root = Path((env or os.environ)["NIX_REMOTE"])
    # A rooted local store prints logical /nix/store paths. Access its physical
    # immutable tree directly; never fall back to the host daemon's store.
    physical_path = store_root / "nix/store" / logical_path.name
    assert physical_path.is_dir(), f"missing ingested source: {physical_path}"
    assert nar_hash(physical_path, env=env) == digest
    assert (physical_path / "flake.lock").read_bytes() == lock_bytes
    assert physical_path.stat().st_mode & 0o222 == 0
    return physical_path


def discovery(reference):
    return [
        "nix", "eval", "--json", "--no-write-lock-file",
        "--option", "pure-eval", "true",
        "--option", "allow-import-from-derivation", "true",
        "--expr", f"builtins.attrNames (builtins.getFlake {json.dumps(reference)}).nixosConfigurations",
    ]


def main():
    global archive
    private = workspace / "private-input"
    private.mkdir()
    (private / "names.nix").write_text('[ "alpha" "beta" ]\n')
    input_hash = nar_hash(private)
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as tar:
        tar.add(private, arcname="private", recursive=True)
    archive = buffer.getvalue()

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), PrivateInput)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    url = f"http://127.0.0.1:{server.server_port}/private.tar.gz"
    try:
        try:
            urllib.request.urlopen(url, timeout=5)
            raise AssertionError("private endpoint accepted unauthenticated request")
        except urllib.error.HTTPError as error:
            assert error.code == 401

        root = workspace / "exact-root"
        root.mkdir()
        (root / "flake.nix").write_text('''{
  inputs.private = { url = "__URL__"; flake = false; };
  outputs = { self, private }: {
    nixosConfigurations = builtins.listToAttrs (map (name: {
      inherit name;
      value.config = {
        system.build.toplevel = builtins.derivation {
          name = "discovery-fixture-${name}";
          system = "x86_64-linux";
          builder = "/bin/sh";
        };
        crystalForgePolicyMarker = name;
        systemd.services.crystal-forge-agent.enable = true;
      };
    }) (import (private + "/names.nix")));
  };
}
'''.replace("__URL__", url))
        lock = {
            "nodes": {
                "root": {"inputs": {"private": "private"}},
                "private": {
                    "flake": False,
                    "locked": {"type": "tarball", "url": url, "narHash": input_hash},
                    "original": {"type": "tarball", "url": url},
                },
            },
            "root": "root", "version": 7,
        }
        lock_path = root / "flake.lock"
        lock_path.write_text(json.dumps(lock, sort_keys=True))
        original_lock = lock_path.read_bytes()
        source_hash = nar_hash(root)

        unauth_env = os.environ.copy()
        unauth_env["NIX_REMOTE"] = str(workspace / "unauthenticated-store")
        unauth_env["XDG_CACHE_HOME"] = str(workspace / "unauthenticated-cache")
        unauth_root = ingest("ingest-unauthenticated", root, source_hash, original_lock, unauth_env)
        rejected_before = requests["rejected"]
        code, _, stderr, unauth_time = run(
            "unauthenticated-discovery",
            discovery(qualified(unauth_root, source_hash)),
            deadline=15, env=unauth_env,
        )
        assert code != 0 and ("401" in stderr or "unauthorized" in stderr.lower())
        assert requests["rejected"] > rejected_before
        assert requests["authorized"] == 0 and requests["completed"] == 0
        assert (unauth_root / "flake.lock").read_bytes() == original_lock

        netrc = workspace / "fixture.netrc"
        netrc.write_text(f"machine 127.0.0.1 login {username} password {password}\n")
        netrc.chmod(0o600)
        os.environ["NIX_CONFIG"] += f"\nnetrc-file = {netrc}\n"

        old_env = os.environ.copy()
        old_env["NIX_REMOTE"] = str(workspace / "historical-store")
        old_env["XDG_CACHE_HOME"] = str(workspace / "historical-cache")
        old_root = ingest("ingest-historical", root, source_hash, original_lock, old_env)
        stored_root = ingest("ingest-authoritative", root, source_hash, original_lock)
        assert old_root.name == stored_root.name == unauth_root.name
        reference = qualified(stored_root, source_hash)
        # Removing the staging tree proves evaluation uses only the ingested
        # exact source. The server exposes no root route or root repository.
        shutil.rmtree(root)
        lock_path = stored_root / "flake.lock"
        run("historical-60s", discovery(qualified(old_root, source_hash)),
            deadline=60, env=old_env, timeout=True)
        assert requests["authorized"] == 1, requests
        # Wait for the disconnected response before measuring the new cold run.
        end = time.monotonic() + 15
        while requests["completed"] != 1 and time.monotonic() < end:
            time.sleep(0.05)
        assert requests["completed"] == 1

        code, stdout, _, cold = run("cold-300s", discovery(reference))
        assert code == 0, "cold discovery failed; inspect cold-300s.stderr"
        names = json.loads(stdout)
        assert names == ["alpha", "beta"]
        assert 60 < cold < DISCOVERY_DEADLINE
        assert requests["authorized"] == 2, requests
        fetch_count = requests.copy()
        code, stdout, _, warm = run("warm-300s", discovery(reference))
        assert code == 0 and json.loads(stdout) == names
        assert warm < 10, f"warm discovery took {warm:.3f}s"
        assert requests == fetch_count, "warm discovery refetched private input"

        wrong_hash = "sha256-" + base64.b64encode(bytes(32)).decode()
        code, _, stderr, _ = run("mismatched-root", discovery(qualified(stored_root, wrong_hash)))
        assert code != 0 and ("mismatch" in stderr.lower() or "hash" in stderr.lower())
        assert lock_path.read_bytes() == original_lock

        primary = Path(sys.argv[1]).read_text()
        selected_names = "[ " + " ".join(json.dumps(name) for name in names) + " ]"
        expression = f'''({primary}) {{
          flakeRef = {json.dumps(reference)};
          requestedRevision = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
          configurationNames = {selected_names};
          policyCheckers = {{
            alpha = config: {{ marker = config.crystalForgePolicyMarker; }};
            beta = config: {{ marker = config.crystalForgePolicyMarker; }};
          }};
        }}'''
        code, stdout, _, policy_time = run("policy-evaluation", [
            "nix-eval-jobs", "--expr", expression,
            "--meta", "--apply", "derivation: derivation.meta.policies",
            "--option", "pure-eval", "true",
            "--option", "allow-import-from-derivation", "true",
            "--workers", "1", "--max-memory-size", "0",
        ])
        assert code == 0
        results = [json.loads(line) for line in stdout.splitlines()]
        assert sorted(item["attr"] for item in results) == names
        for item in results:
            assert item.get("error") is None and item["drvPath"].endswith(".drv")
            assert item["extraValue"]["marker"] == item["attr"]
            assert item["extraValue"]["cfAgentEnabled"] is True
        assert requests == fetch_count, "policy evaluation refetched locked input"
        assert lock_path.read_bytes() == original_lock
        assert nar_hash(stored_root) == source_hash
        assert (old_root / "flake.lock").read_bytes() == original_lock
        assert requests["unexpected"] == 0, "unexpected endpoint/root fetch"

        broken = workspace / "error-root"
        broken.mkdir()
        (broken / "flake.nix").write_text('''{
          outputs = { self }: {
            nixosConfigurations = throw "fixture-genuine-discovery-error";
          };
        }''')
        (broken / "flake.lock").write_text(
            '{"nodes":{"root":{"inputs":{}}},"root":"root","version":7}'
        )
        broken_lock = (broken / "flake.lock").read_bytes()
        broken_hash = nar_hash(broken)
        stored_broken = ingest("ingest-error", broken, broken_hash, broken_lock)
        shutil.rmtree(broken)
        code, _, stderr, error_time = run(
            "genuine-discovery-error", discovery(qualified(stored_broken, broken_hash)), deadline=15,
        )
        assert code != 0 and "fixture-genuine-discovery-error" in stderr
        assert (stored_broken / "flake.lock").read_bytes() == broken_lock
        assert requests == fetch_count, "error fixture fetched an input"

        incomplete = workspace / "incomplete-root"
        incomplete.mkdir()
        (incomplete / "flake.nix").write_bytes((stored_root / "flake.nix").read_bytes())
        (incomplete / "flake.lock").write_bytes(broken_lock)
        incomplete_hash = nar_hash(incomplete)
        stored_incomplete = ingest("ingest-incomplete", incomplete, incomplete_hash, broken_lock)
        shutil.rmtree(incomplete)
        incomplete_ref = qualified(stored_incomplete, incomplete_hash)
        code, _, stderr, _ = run(
            "incomplete-lock-discovery", discovery(incomplete_ref), deadline=15,
        )
        assert code != 0 and "lock" in stderr.lower()
        # nix-eval-jobs can encode evaluation errors in JSON while exiting zero.
        # Require parity with discovery's rejection, not a particular exit code.
        incomplete_expression = expression.replace(json.dumps(reference), json.dumps(incomplete_ref))
        code, stdout, stderr, _ = run("incomplete-lock-policy", [
            "nix-eval-jobs", "--expr", incomplete_expression,
            "--option", "pure-eval", "true",
            "--option", "allow-import-from-derivation", "true",
            "--workers", "1", "--max-memory-size", "0",
        ], deadline=15)
        if code == 0:
            failures = [json.loads(line) for line in stdout.splitlines()]
            assert failures and all(item.get("error") for item in failures)
            assert "lock" in stdout.lower()
        else:
            assert "lock" in stderr.lower()
        assert (stored_incomplete / "flake.lock").read_bytes() == broken_lock
        assert requests == fetch_count, "incomplete lock attempted an input fetch"
        report = {
            "delay_seconds": DELAY_SECONDS, "deadline_seconds": DISCOVERY_DEADLINE,
            "cold_seconds": cold, "warm_seconds": warm, "policy_seconds": policy_time,
            "configuration_names": names, "requests": requests,
            "source_nar_hash": source_hash,
            "ingested_source": str(stored_root),
            "unauthenticated_seconds": unauth_time, "genuine_error_seconds": error_time,
            "incomplete_lock_discovery_and_policy": "rejected-without-fetch",
            "lock_sha256": hashlib.sha256(original_lock).hexdigest(),
        }
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, sort_keys=True), flush=True)
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


if __name__ == "__main__":
    main()
