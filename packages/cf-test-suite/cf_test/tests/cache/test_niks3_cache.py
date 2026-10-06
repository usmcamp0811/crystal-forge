"""Fail-closed Niks3 VM integration; invoked by checks/niks3-cache.

Queue setup and evaluated identities are seeded. The packaged remote builder
claims a real job, imports its derivation, builds it, pushes it, and signs its
completion. The server must independently read the selected cache before
recording publication.
The deployment target has a no-op activation script: this check proves agent
pulls, not a complete NixOS generation switch.
"""

import base64
import hashlib
import inspect
import json
import shlex
import time
import tempfile
import uuid
from datetime import datetime, timezone
from pathlib import Path

import psycopg2
from nacl.signing import SigningKey
from cryptography.hazmat.primitives.ciphers.aead import AESGCM


TOKEN = "niks3-nonproduction-static-token-470-00000000"
ENCRYPTION_KEY = "niks3-vm-only-cache-encryption-key-470"


def cache_path_referenced(data, path):
    """Matches an exact directory or descendant, never a shared name prefix.

    Argument, map and symlink bytes remain private inputs. Only the boolean
    result can cross the guest diagnostic boundary.
    """
    import re

    return bool(re.search(rb"(?<![A-Za-z0-9_./-])" + re.escape(path.encode())
                          + rb"(?=/|[\x00\s?'\"&]|$)", data))


def cache_process_snapshot(paths, proc_root="/proc"):
    """Returns allowlisted process identities and safe path correlations.

    A reference is correlation, not proof of TempDir ownership. Rust owners
    need not retain an FD. PID/start-time rechecks discard raced or reused PIDs.
    No argument bytes, environment, raw links or map lines leave this function.
    Permission failures are counted and leave ownership unknown.
    """
    import os
    import re
    import time
    from pathlib import Path

    relevant = {"nix", "nix-store", "niks3", "cf-server", "cf-server-core",
                "crystal-forge-server", "server", "cf-builder", "crystal-forge-builder"}
    reference_only = {"sh", "bash", "curl", "python3", "vulnix", "nix-eval-jobs"}
    result = {"processes": [], "unreadable": 0, "raced": 0,
              "correlation": "unknown", "truncated": False}
    deadline = time.monotonic() + 6

    def identity(base):
        # stat's parenthesized comm can contain spaces and closing parentheses.
        fields = (base / "stat").read_text().rsplit(")", 1)[1].split()
        return fields[19], fields[0]  # Field 22 start time, field 3 state.

    for base in sorted(Path(proc_root).iterdir()):
        if not base.name.isdigit():
            continue
        if time.monotonic() >= deadline:
            result["truncated"] = True
            break
        try:
            before, state = identity(base)
            exe = Path(os.readlink(base / "exe")).name
            canonical = exe.removeprefix(".").removesuffix("-wrapped")
            # Ignore arbitrary executable names rather than logging a basename
            # which could itself contain private material.
            if canonical not in relevant | reference_only:
                continue
            comm = (base / "comm").read_text().strip()
            comm = comm if comm in relevant or comm in (exe, exe[:15], canonical[:15], "nix-daemon") else "unknown"
            matches = set()
            if paths:
                aliases = {path: [path] for path in paths}
                for path in paths:
                    if re.fullmatch(r"/tmp/systemd-private-[A-Za-z0-9_.-]+/tmp/cf-cache-[A-Za-z0-9_-]+", path):
                        visible = "/tmp/" + Path(path).name
                        try:
                            outside = os.stat(path)
                            inside = os.stat(base / "root" / visible.lstrip("/"))
                            # CONCURRENCY: PrivateTmp names alone do not identify
                            # the same object. Correlate only equal device/inode.
                            if (outside.st_dev, outside.st_ino) == (inside.st_dev, inside.st_ino):
                                aliases[path].append(visible)
                        except PermissionError:
                            result["unreadable"] += 1
                        except FileNotFoundError:
                            pass
                def referenced(data):
                    return {path for path, names in aliases.items()
                            if any(cache_path_referenced(data, name) for name in names)}
                for entry in (base / "cmdline", base / "maps"):
                    try:
                        data = entry.read_bytes()
                        matches.update(referenced(data))
                    except PermissionError:
                        result["unreadable"] += 1
                # Read links and mappings privately. Never serialize their bytes.
                try:
                    links = list((base / "fd").iterdir())
                except PermissionError:
                    result["unreadable"] += 1
                    links = []
                for entry in [base / "cwd", base / "root", *links]:
                    try:
                        link = os.fsencode(os.readlink(entry))
                        matches.update(referenced(link))
                    except PermissionError:
                        result["unreadable"] += 1
                    except FileNotFoundError:
                        pass
            after, _ = identity(base)
            if before != after:
                result["raced"] += 1
                continue
            if canonical not in relevant and not matches:
                continue
            result["processes"].append({"pid": int(base.name), "exe": exe,
                                        "comm": comm, "state": state,
                                        "start_time": before,
                                        "paths": sorted(matches)})
        except PermissionError:
            result["unreadable"] += 1
        except (FileNotFoundError, ProcessLookupError):
            result["raced"] += 1
        except (OSError, ValueError, IndexError):
            result["unreadable"] += 1
    if any(process["paths"] for process in result["processes"]):
        result["correlation"] = "reference_observed_owner_unknown"
    return result


def cache_directory_snapshot():
    """Inspects the exact cleanup assertion roots, depth and directory prefix.

    Emits directory identities only. It does not enumerate directory contents,
    remove directories, wait for cleanup or change the assertion's target.
    """
    import re
    import subprocess
    from pathlib import Path

    roots = ["/tmp", "/var/lib/crystal-forge", "/var/lib/crystal-forge-agent"]
    scan = subprocess.run(["find", *roots, "-maxdepth", "3", "-type", "d",
                           "-name", "cf-cache-*", "-print"],
                          stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                          timeout=1, check=False)
    paths = []
    for path in scan.stdout.decode().splitlines():
        # Only known, nonsecret random TempDir names are exportable. Refuse any
        # unexpected path instead of exposing arbitrary filenames in diagnostics.
        if (any(path.startswith(root + "/") for root in roots)
                and re.fullmatch(r"cf-cache-[A-Za-z0-9_-]+", Path(path).name)
                and all(re.fullmatch(r"[A-Za-z0-9_.-]+", part)
                        for part in Path(path).parts[1:])):
            paths.append(path)
    return {"directories": [{"path": path, "basename": Path(path).name,
                              "type": "verification_store" if Path(path).name.startswith("cf-cache-verify-") else "credential",
                              "filesystem_type": "directory"} for path in sorted(paths)],
            "find_status": scan.returncode, "process": cache_process_snapshot(paths)}


def private_journal_message(record):
    """Decodes journal message bytes privately without exporting their contents.

    journalctl JSON represents non-printable data, including ANSI escapes, as
    byte arrays. Callers must project allowlisted fields, never emit or persist
    this result. Missing, oversized-null and invalid UTF-8 fields remain unknown.
    """
    message = record.get("MESSAGE")
    if isinstance(message, str):
        return message
    if isinstance(message, list) and all(type(value) is int and 0 <= value <= 255 for value in message):
        try:
            return bytes(message).decode("utf-8")
        except UnicodeDecodeError:
            pass
    return ""


def input_owner_projection(records):
    """Projects validated lifecycle fields from structured journal records.

    Raw messages remain private. Malformed, duplicate or out-of-order lifecycle
    records fail closed. Completion proves cleanup attempted after child reap;
    it does not assert that TempDir removed every file successfully.
    """
    import re

    projected = []
    operations = {}
    for record in records:
        message = re.sub(r"\x1b\[[0-9;]*m", "", private_journal_message(record))
        if "Niks3 input owner lifecycle" not in message:
            continue
        if not re.match(r"^[0-9T:.Z+-]+\s+INFO\s+crystal_forge::niks3_input_owner:\s+Niks3 input owner lifecycle\s", message):
            raise ValueError("invalid input owner lifecycle source")
        fields = re.search(
            r'operation=([0-9a-f-]{36})\s+phase="(started|completed)"\s+'
            r'child_pid=(\d+)\s+child_reaped=(true|false)\s+'
            r'cleanup_attempted=(true|false)\s+outcome="([a-z_]+)"', message)
        if fields is None:
            raise ValueError("invalid input owner lifecycle fields")
        operation, phase, pid, reaped, cleaned, outcome = fields.groups()
        if str(uuid.UUID(operation)) != operation or outcome not in {
                "running", "success", "exit_failure", "spawn_failure",
                "configuration_failure", "wait_failure", "timeout"}:
            raise ValueError("invalid input owner lifecycle identity")
        event = {"operation": operation, "phase": phase, "child_pid": int(pid),
                 "child_reaped": reaped == "true", "cleanup_attempted": cleaned == "true",
                 "outcome": outcome}
        previous = operations.get(operation)
        if phase == "started":
            if previous or event["cleanup_attempted"] or event["child_reaped"] or outcome != "running":
                raise ValueError("invalid input owner start")
        elif (not previous or previous["phase"] != "started"
              or previous["child_pid"] != event["child_pid"]
              or not event["cleanup_attempted"] or outcome == "running"):
            raise ValueError("invalid input owner completion")
        operations[operation] = event
        projected.append(event)
    pending = [operation for operation, event in operations.items()
               if event["phase"] != "completed" or (event["child_pid"] and not event["child_reaped"])]
    return {"events": projected, "started_count": len(operations),
            "completed_count": sum(event["phase"] == "completed" for event in operations.values()),
            "pending_operations": sorted(pending)}


def credential_consumers(process_snapshot):
    """Returns live Nix/Niks3 clients, excluding the identified Nix daemon."""
    return [process for process in process_snapshot["processes"]
            if process["exe"].removeprefix(".").removesuffix("-wrapped") in {"nix", "nix-store", "niks3"}
            and process["comm"] != "nix-daemon"]


def input_owner_journal_state():
    """Reads current-boot server journals privately and returns safe events."""
    import subprocess

    result = subprocess.run(["journalctl", "-b", "-u", "crystal-forge-server",
                             "--no-pager", "-o", "json"],
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                            timeout=1, check=False)
    if result.returncode:
        raise ValueError("input owner journal unavailable")
    return input_owner_projection([json.loads(line) for line in result.stdout.splitlines()])


def agent_read_projection(records):
    """Extracts fresh read-owner completion and allowlisted activation units.

    Journal bytes stay private while their UTF-8 markers are parsed. Absence of
    a valid completion marker remains a failed boundary.
    """
    import re

    units = set()
    joined = False
    for record in records:
        message = private_journal_message(record)
        message = re.sub(r"\x1b\[[0-9;]*m", "", message)
        match = re.search(r"Deployment detached to systemd unit: (crystal-forge-deploy-[0-9]+)", message)
        if match:
            units.add(match.group(1))
        joined |= bool(re.search(r"Deployment completed successfully\s*$", message))
    return {"read_owner_joined": joined, "activation_units": sorted(units)}


def test_agent_read_projection_handles_private_journal_field_types():
    """Keeps null/binary journal fields private and requires actual markers."""
    records = [{"MESSAGE": None}, {"MESSAGE": [80, 82, 73, 86, 65, 84, 69]},
               {"MESSAGE": "PRIVATE irrelevant message"},
               {"MESSAGE": "INFO Deployment detached to systemd unit: crystal-forge-deploy-470"},
               {"MESSAGE": list(b'\x1b[32mINFO\x1b[0m Deployment completed successfully')}]
    result = agent_read_projection(records)
    assert result == {"read_owner_joined": True, "activation_units": ["crystal-forge-deploy-470"]}
    assert "PRIVATE" not in json.dumps(result)
    assert not agent_read_projection(records[:3])["read_owner_joined"]
    assert private_journal_message({"MESSAGE": [255]}) == ""


def test_input_owner_projection_is_safe_and_ordered():
    """Rejects malformed acknowledgments without returning raw journal bytes."""
    operation = "00000000-0000-4000-8000-000000000470"
    prefix = "2026-10-06T17:00:00Z INFO crystal_forge::niks3_input_owner: Niks3 input owner lifecycle "
    start = {"MESSAGE": prefix + f'operation={operation} phase="started" child_pid=123 child_reaped=false cleanup_attempted=false outcome="running" PRIVATE_DO_NOT_EMIT'}
    complete = {"MESSAGE": prefix + f'operation={operation} phase="completed" child_pid=123 child_reaped=true cleanup_attempted=true outcome="success"'}
    projected = input_owner_projection([start, complete])
    assert projected["pending_operations"] == [] and projected["completed_count"] == 1
    assert "PRIVATE" not in json.dumps(projected)
    binary_start = {"MESSAGE": list(("\x1b[32m" + start["MESSAGE"] + "\x1b[0m").encode())}
    assert input_owner_projection([binary_start, complete]) == projected
    assert input_owner_projection([start])["pending_operations"] == [operation]
    unreaped = {"MESSAGE": complete["MESSAGE"].replace("child_reaped=true", "child_reaped=false")}
    assert input_owner_projection([start, unreaped])["pending_operations"] == [operation]
    for records in ([complete], [start, start], [start, complete, complete],
                    [start, {"MESSAGE": complete["MESSAGE"].replace("cleanup_attempted=true", "cleanup_attempted=false")}],
                    [start, {"MESSAGE": complete["MESSAGE"].replace("child_pid=123", "child_pid=124")}],
                    [{"MESSAGE": start["MESSAGE"].replace("crystal_forge::niks3_input_owner", "untrusted_provider")}],
                    [{"MESSAGE": "Niks3 input owner lifecycle SECRET"}]):
        try:
            input_owner_projection(records)
        except ValueError as error:
            assert "SECRET" not in str(error)
        else:
            raise AssertionError("unsafe lifecycle record accepted")


def test_cleanup_diagnostic_path_boundaries():
    """Rejects shared prefixes while accepting exact paths and store-root args."""
    path = "/tmp/cf-cache-verify-Ab123"
    for data in (path.encode(), (path + "/key.pem").encode(),
                 ("--to\0local?root=" + path + "\0SECRET").encode()):
        assert cache_path_referenced(data, path)
    for data in ((path + "stale").encode(), ("/other" + path).encode()):
        assert not cache_path_referenced(data, path)


def test_cleanup_diagnostic_process_safety(tmp_path, monkeypatch):
    """Proves safe serialization, PID rechecks and unknown permission results."""
    import os

    proc = tmp_path / "proc"
    proc.mkdir()
    base = proc / "123"
    base.mkdir()
    def stat(start):
        return "123 (niks3) " + " ".join(["S"] + ["0"] * 18 + [str(start)])
    (base / "stat").write_text(stat(42))
    (base / "exe").symlink_to("/nix/store/nonsecret/bin/niks3")
    (base / "comm").write_text("niks3\n")
    path = "/tmp/cf-cache-verify-Ab123"
    (base / "cmdline").write_bytes(("niks3\0--token\0NEVER_EMIT_SECRET\0local?root=" + path).encode())
    (base / "maps").write_text("SECRET_MAP " + path + "/private.key")
    (base / "fd").mkdir()
    (base / "fd" / "0").symlink_to(path + "/SECRET_FD")
    (base / "cwd").symlink_to("/tmp")
    (base / "root").symlink_to("/")
    result = cache_process_snapshot([path], str(proc))
    assert result["processes"] == [{"pid": 123, "exe": "niks3", "comm": "niks3",
                                    "state": "S", "start_time": "42", "paths": [path]}]
    assert "SECRET" not in json.dumps(result)
    original = Path.read_text
    calls = 0
    def raced_read(entry, *args, **kwargs):
        nonlocal calls
        if entry == base / "stat":
            calls += 1
            return stat(42 if calls == 1 else 43)
        return original(entry, *args, **kwargs)
    monkeypatch.setattr(Path, "read_text", raced_read)
    result = cache_process_snapshot([path], str(proc))
    assert not result["processes"] and result["raced"] == 1
    monkeypatch.setattr(Path, "read_text", original)
    original_link = os.readlink
    def denied_link(entry, *args, **kwargs):
        if entry == base / "exe":
            raise PermissionError
        return original_link(entry, *args, **kwargs)
    monkeypatch.setattr(os, "readlink", denied_link)
    result = cache_process_snapshot([path], str(proc))
    assert not result["processes"] and result["unreadable"] == 1
    assert result["correlation"] == "unknown"


def test_cleanup_diagnostic_exact_roots(monkeypatch):
    """Keeps the assertion's exact roots/depth/prefix and rejects unsafe names."""
    import subprocess
    from types import SimpleNamespace

    def scan(command, **kwargs):
        assert command == ["find", "/tmp", "/var/lib/crystal-forge",
                           "/var/lib/crystal-forge-agent", "-maxdepth", "3",
                           "-type", "d", "-name", "cf-cache-*", "-print"]
        assert kwargs["stderr"] == subprocess.DEVNULL
        assert kwargs["timeout"] == 1
        return SimpleNamespace(returncode=1, stdout=(
            b"/tmp/cf-cache-Ab123\n/var/lib/crystal-forge/cf-cache-verify-A\n"
            b"/other/cf-cache-not-in-root\n/tmp/cf-cache-PRIVATE@TOKEN\n"))
    monkeypatch.setattr(subprocess, "run", scan)
    monkeypatch.setitem(cache_directory_snapshot.__globals__, "cache_process_snapshot",
                        lambda paths: {"paths": paths, "correlation": "unknown"})
    result = cache_directory_snapshot()
    assert result["find_status"] == 1
    assert result["process"]["paths"] == ["/tmp/cf-cache-Ab123",
                                           "/var/lib/crystal-forge/cf-cache-verify-A"]
    assert "TOKEN" not in json.dumps(result)


def test_cleanup_diagnostic_private_tmp_identity(tmp_path, monkeypatch):
    """Maps PrivateTmp references only when process-root device/inode agree."""
    import os
    from types import SimpleNamespace

    proc = tmp_path / "proc"
    base = proc / "321"
    base.mkdir(parents=True)
    (base / "stat").write_text("321 (.niks3-wrapped) " + " ".join(["S"] + ["0"] * 18 + ["42"]))
    (base / "exe").symlink_to("/nix/store/nonsecret/bin/.niks3-wrapped")
    (base / "comm").write_text(".niks3-wrapped")
    path = "/tmp/systemd-private-Ab-server.service-Cd/tmp/cf-cache-Ab123"
    (base / "cmdline").write_bytes(b"niks3\0--auth-token-path\0/tmp/cf-cache-Ab123/token\0SECRET")
    (base / "maps").write_bytes(b"")
    (base / "fd").mkdir()
    (base / "root").symlink_to("/")
    (base / "cwd").symlink_to("/")
    original = os.stat
    matched = True
    def stat(entry, *args, **kwargs):
        if str(entry) == path:
            return SimpleNamespace(st_dev=1, st_ino=99)
        if entry == base / "root/tmp/cf-cache-Ab123":
            return SimpleNamespace(st_dev=1, st_ino=99 if matched else 100)
        return original(entry, *args, **kwargs)
    monkeypatch.setattr(os, "stat", stat)
    result = cache_process_snapshot([path], str(proc))
    assert result["processes"][0]["paths"] == [path]
    assert result["processes"][0]["exe"] == ".niks3-wrapped"
    assert "SECRET" not in json.dumps(result)
    matched = False
    result = cache_process_snapshot([path], str(proc))
    assert result["processes"][0]["paths"] == []
    assert result["correlation"] == "unknown"


def run_proxy_claims(server, builder, sql, encrypt, target, public_key, signing_keys):
    """Exercises signed dispatch through the real loader, socket and TLS proxy.

    Scratch jobs stop at authenticated /start. No legacy cache upload is
    fabricated. Raw invalid TOML cases bypass only the Nix evaluation assertion,
    through a reversible VM-only bind mount at the module's real config path.
    """
    def guest_python(source):
        return server.succeed("python3 -c " + shlex.quote(source))

    def config_evidence():
        return json.loads(guest_python("""
import json, pathlib, re, shutil, subprocess, tomllib
pid = subprocess.check_output(['systemctl','show','-p','MainPID','--value','crystal-forge-server']).decode().strip()
env = dict(item.split('=',1) for item in pathlib.Path('/proc/'+pid+'/environ').read_bytes().decode().split('\\0') if '=' in item)
assert not any(k.startswith('CRYSTAL_FORGE__SERVER__') for k in env), 'server environment masks TOML'
assert not any('TRUST_FORWARDED_BUILDER_HTTPS' in k or 'TRUSTED_PROXY_CIDRS' in k for k in env), 'trust environment masks TOML'
path = env['CRYSTAL_FORGE_CONFIG']
cfg = tomllib.loads(pathlib.Path(path).read_text())['server']
unit = subprocess.check_output(['systemctl','show','-p','ExecStart','--value','crystal-forge-server']).decode()
script = re.search(r'path=([^ ;]+)',unit).group(1)
assert 'export CRYSTAL_FORGE_CONFIG="'+path+'"' in pathlib.Path(script).read_text()
nix = shutil.which('nix',path=env['PATH'])
version = subprocess.check_output([nix,'--version']).decode().strip()
print(json.dumps({'path':path,'pid':int(pid),'exec_start':script,'nix':nix,'nix_version':version,'flag':cfg['trust_forwarded_builder_https'],'cidrs':cfg['trusted_proxy_cidrs']}))
"""))

    original = config_evidence()
    assert original["flag"] is True and original["cidrs"] == ["127.0.0.1/32"]
    print("Proxy claim loaded module config: " + json.dumps(original, sort_keys=True))
    config_path = original["path"]
    server.succeed(f"cp {shlex.quote(config_path)} /run/proxy-claim-original.toml")

    def runtime_config(flag, cidrs):
        # Keep the module ExecStart and CRYSTAL_FORGE_CONFIG unchanged. A mount
        # changes only this disposable guest's view of its generated config.
        guest_python(f"""
import pathlib, re, shutil
text = pathlib.Path('/run/proxy-claim-original.toml').read_text()
text, n = re.subn(r'(?m)^trust_forwarded_builder_https\\s*=.*$', 'trust_forwarded_builder_https = {str(flag).lower()}', text)
assert n == 1
text, n = re.subn(r'(?m)^trusted_proxy_cidrs\\s*=.*$', 'trusted_proxy_cidrs = {json.dumps(cidrs)}', text)
assert n == 1
runtime = pathlib.Path('/run/proxy-claim-runtime.toml')
runtime.write_text(text)
shutil.chown(runtime, user='crystal-forge', group='crystal-forge')
runtime.chmod(0o600)
# The module regenerates TOML in ExecStartPre. Disable that one guest-local
# step while the explicit raw fixture is mounted; ExecStart remains intact.
dropin = pathlib.Path('/run/systemd/system/crystal-forge-server.service.d/proxy-claim.conf')
dropin.parent.mkdir(parents=True, exist_ok=True)
dropin.write_text('[Service]\\nExecStartPre=\\n')
""")
        server.succeed(f"mount --bind /run/proxy-claim-runtime.toml {shlex.quote(config_path)}")
        server.succeed("systemctl daemon-reload")
        server.succeed("systemctl restart crystal-forge-server")
        server.wait_for_unit("crystal-forge-server.service")
        server.wait_for_open_port(8000)
        evidence = config_evidence()
        assert evidence["path"] == config_path and evidence["flag"] == flag and evidence["cidrs"] == cidrs
        print("Proxy claim isolated raw-TOML runtime: " + json.dumps(evidence, sort_keys=True))

    env_id = sql("INSERT INTO environments(name) VALUES ('proxy-claim-scratch') RETURNING id")[0][0]
    builder_id = sql("INSERT INTO builders(name,public_key,status,arch) VALUES ('proxy-claim-scratch',%s,'active','x86_64-linux') RETURNING id", (public_key,))[0][0]
    sql("INSERT INTO builder_environment_assignments(builder_id,environment_id) VALUES (%s,%s)", (builder_id, env_id))
    flake_id = sql("INSERT INTO flakes(name,repo_url) VALUES ('proxy-claim-scratch','https://example.invalid/proxy-claim') RETURNING id")[0][0]
    sql("INSERT INTO systems(hostname,environment_id,public_key,flake_id,derivation) VALUES ('proxy-claim-scratch',%s,%s,%s,'proxy-claim-scratch')", (env_id, public_key, flake_id))
    session_id = str(uuid.uuid4())
    key = SigningKey(base64.b64decode("+/GIbrjuyb3Hf2es5w+vWSlDUhEsAIojiyyfgskC7QA="))

    def signed_request(machine, endpoint, suffix, payload, headers=()):
        path = f"/api/v1/builders/{builder_id}/{suffix}"
        body = json.dumps(payload, separators=(",", ":"))
        timestamp = datetime.now(timezone.utc).isoformat()
        signature = base64.b64encode(key.sign(f"POST\n{path}\n{timestamp}\n{body}".encode()).signature).decode()
        auth = ["Content-Type: application/json", f"X-Builder-ID: {builder_id}",
                f"X-Builder-Session-ID: {session_id}", f"X-Timestamp: {timestamp}", f"X-Signature: {signature}"]
        machine.succeed(f"umask 077; printf %s {shlex.quote(body)} > /tmp/proxy-claim-body")
        status = machine.succeed(f"curl --silent --show-error -X POST {shlex.quote(endpoint + path)} "
                                 + " ".join("-H " + shlex.quote(h) for h in auth + list(headers))
                                 + " --data-binary @/tmp/proxy-claim-body -o /tmp/proxy-claim-response -w '%{http_code}'").strip()
        # Never print a credential-bearing response through the VM driver.
        response = json.loads(machine.succeed("python3 -c " + shlex.quote("""
import json, pathlib
text = pathlib.Path('/tmp/proxy-claim-response').read_text()
try:
    data = json.loads(text)
except ValueError:
    data = {}
cache = data.get('derivation',{}).get('cache_push',{})
print(json.dumps({'job':data.get('job',{}).get('id'),'type':cache.get('cache_type'),
    'destination':cache.get('cache_destination_id'), 'push':cache.get('push_after_build'),
    'attic':cache.get('attic_token') == 'proxy-claim-attic-secret',
    's3':cache.get('s3_secret_access_key') == 'proxy-claim-s3-secret',
    'niks3':cache.get('niks3_write_auth') == {'kind':'token','token':'proxy-claim-niks3-secret'},
    'no_secrets':not any(cache.get(k) for k in ('attic_token','s3_access_key_id','s3_secret_access_key','s3_session_token','signing_key','niks3_write_auth'))}))
""")))
        machine.succeed("rm /tmp/proxy-claim-body /tmp/proxy-claim-response")
        return int(status), response

    # Both request origins need the bounded response inspector.
    status, _ = signed_request(server, "https://server", "session", {"session_id": session_id, "capabilities": {"niks3_cache": True}})
    assert status == 200, "signed scratch session establishment failed"
    assert str(sql("SELECT current_session_id FROM builders WHERE id=%s", (builder_id,))[0][0]) == session_id
    commit_id = sql("INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp,evaluation_status) VALUES (%s,%s,NOW(),'complete') RETURNING id", (flake_id, hashlib.sha1(b"proxy-claim-scratch").hexdigest()))[0][0]
    sql("INSERT INTO commit_artifacts_cache(commit_id,nixos_configurations) VALUES (%s,ARRAY['proxy-claim-scratch'])", (commit_id,))
    derivation_id = sql("""INSERT INTO derivations
        (commit_id,derivation_type,derivation_name,derivation_target,derivation_path,store_path,status_id,
         cf_agent_enabled,policy_requirements_met,scheduled_at)
        VALUES (%s,'nixos','proxy-claim-scratch','proxy-claim-scratch',%s,%s,5,true,true,NOW()) RETURNING id""", (commit_id, target["drv"], target["out"]))[0][0]
    caches = {}
    for cache_type in ("Attic", "S3", "Niks3", "Http", "Nix"):
        cache_id = sql("""INSERT INTO cache_destinations
            (name,cache_type,enabled,push_to,attic_cache_name,attic_token,s3_region,s3_access_key_id,s3_secret_access_key,
             niks3_server_url,niks3_public_keys,niks3_write_auth_mode,niks3_auth_token,niks3_read_auth_mode)
            VALUES (%s,%s,false,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s) RETURNING id""",
            (f"proxy-claim-{cache_type}", cache_type, "s3://proxy-claim" if cache_type == "S3" else "https://cache:5753",
             "proxy-claim" if cache_type == "Attic" else None, encrypt("proxy-claim-attic-secret") if cache_type == "Attic" else None,
             "us-east-1" if cache_type == "S3" else None, "proxy-claim-access-id" if cache_type == "S3" else None,
             encrypt("proxy-claim-s3-secret") if cache_type == "S3" else None,
             "https://cache:5751" if cache_type == "Niks3" else None, signing_keys if cache_type == "Niks3" else [],
             "token" if cache_type == "Niks3" else None, encrypt("proxy-claim-niks3-secret") if cache_type == "Niks3" else None,
             "none" if cache_type == "Niks3" else None))[0][0]
        sql("INSERT INTO cache_destination_environments(cache_destination_id,environment_id) VALUES (%s,%s)", (cache_id, env_id))
        caches[cache_type] = cache_id

    def claim(cache_type, label, machine=server, endpoint="http://127.0.0.1:8000", headers=(), allowed=False):
        sql("UPDATE cache_destinations SET enabled=(id=%s) WHERE id=ANY(%s)", (caches[cache_type], list(caches.values())))
        sql("UPDATE derivations SET status_id=5 WHERE id=%s", (derivation_id,))
        # Exhaust the scratch row's retry budget without changing global policy.
        job_id = sql("INSERT INTO build_jobs(derivation_id,environment_id,status,queue_position,max_retries,retry_count) VALUES (%s,%s,'queued',1000,0,100000) RETURNING id", (derivation_id, env_id))[0][0]
        cursor = server.succeed("journalctl -u crystal-forge-server -n 0 --show-cursor --no-pager").strip().split("-- cursor: ")[-1]
        status, response = signed_request(machine, endpoint, "next-job", {"capabilities": {"niks3_cache": True}, "supported_execution_strategies": ["server_derivation"]}, headers)
        print(f"Proxy claim safe response {label}/{cache_type}: status={status}; {json.dumps(response, sort_keys=True)}")
        row = sql("SELECT status,builder_id,builder_session_id,dispatched_cache_destination_id,cache_dispatch_recorded_at,logs FROM build_jobs WHERE id=%s", (job_id,))[0]
        if allowed:
            assert status == 200 and response["job"] == str(job_id), f"{label}/{cache_type}: claim failed ({status})"
            assert response["type"] == cache_type and response["destination"] == caches[cache_type] and response["push"] is True
            assert response[{"Attic": "attic", "S3": "s3", "Niks3": "niks3", "Http": "no_secrets", "Nix": "no_secrets"}[cache_type]], f"{label}/{cache_type}: payload mismatch"
            assert row[:2] == ("building", builder_id) and str(row[2]) == session_id
            assert row[3] == caches[cache_type] and row[4] is not None
            assert row[5] is None or "[dispatch:cache_config]" not in row[5]
            start_status, _ = signed_request(machine, endpoint, f"jobs/{job_id}/start", {}, headers)
            assert start_status == 202
            assert sql("SELECT status FROM build_jobs WHERE id=%s", (job_id,))[0][0] == "building"
        else:
            assert status == 404, f"{label}/{cache_type}: expected confidentiality refusal, got {status}"
            assert row[0] in ("queued", "failed") and row[3:5] == (None, None)
            assert "[dispatch:cache_config]" in (row[5] or "") and "not verified HTTPS" in row[5]
            logs = server.succeed(f"journalctl -u crystal-forge-server --after-cursor={shlex.quote(cursor)} --no-pager")
            assert "refusing to send cache push credentials" in logs
        assert not sql("SELECT id FROM cache_push_jobs WHERE derivation_id=%s", (derivation_id,)), "claim fabricated publication"
        sql("DELETE FROM build_jobs WHERE id=%s", (job_id,))
        print(f"Proxy real signed claim {label}/{cache_type}: status={status}; {'dispatch bound and /start accepted' if allowed else 'cache_config refusal before dispatch'}")

    try:
        for cache_type in ("Attic", "S3", "Niks3"):
            claim(cache_type, "module-HTTPS", endpoint="https://server", allowed=True)
            claim(cache_type, "proxy-overwrites-spoof", endpoint="https://server", headers=("X-Forwarded-Proto: http", "X-Forwarded-Proto: attacker"), allowed=True)
            claim(cache_type, "missing-header")
            claim(cache_type, "duplicate-header", headers=("X-Forwarded-Proto: https", "X-Forwarded-Proto: https"))
            claim(cache_type, "http-header", headers=("X-Forwarded-Proto: http",))
            claim(cache_type, "wrong-peer", machine=builder, endpoint="http://server:8000", headers=("X-Forwarded-Proto: https",))
        for label, flag, cidrs in (("flag-false", False, ["127.0.0.1/32"]), ("empty-CIDR", True, []), ("untrusted-CIDR", True, ["192.0.2.1/32"])):
            runtime_config(flag, cidrs)
            for cache_type in ("Attic", "S3", "Niks3"):
                claim(cache_type, label, endpoint="https://server")
            if not flag:
                claim("Http", "nonsecret-direct-HTTP-flag-false", allowed=True)
                claim("Nix", "nonsecret-direct-HTTP-flag-false", allowed=True)
            server.succeed(f"umount {shlex.quote(config_path)}")
    finally:
        server.execute(f"mountpoint -q {shlex.quote(config_path)} && umount {shlex.quote(config_path)}")
        server.succeed("rm -f /run/systemd/system/crystal-forge-server.service.d/proxy-claim.conf; systemctl daemon-reload")
        server.succeed("systemctl restart crystal-forge-server")
        server.wait_for_unit("crystal-forge-server.service")
        server.wait_for_open_port(8000)
        restored = config_evidence()
        assert restored["flag"] is True and restored["cidrs"] == ["127.0.0.1/32"]
        print("Proxy claim restored module config: " + json.dumps(restored, sort_keys=True))
        sql("DELETE FROM build_jobs WHERE derivation_id=%s", (derivation_id,))
        sql("DELETE FROM derivations WHERE id=%s", (derivation_id,))
        sql("DELETE FROM systems WHERE hostname='proxy-claim-scratch'")
        sql("DELETE FROM commits WHERE id=%s", (commit_id,))
        sql("DELETE FROM flakes WHERE id=%s", (flake_id,))
        sql("DELETE FROM cache_destinations WHERE id=ANY(%s)", (list(caches.values()),))
        sql("DELETE FROM builders WHERE id=%s", (builder_id,))
        sql("DELETE FROM environments WHERE id=%s", (env_id,))
        server.succeed("rm -f /run/proxy-claim-original.toml /run/proxy-claim-runtime.toml")


def run_matrix(machines, targets, builder_public_key, credentials):
    """Runs all five auth/read-endpoint variants without skip-on-error paths."""
    server, builder, agent, cache = (machines[k] for k in ("server", "builder", "agent", "cache"))
    credentials = Path(credentials)
    server.forward_port(5439, 5432)
    db = psycopg2.connect(host="127.0.0.1", port=5439, user="postgres", dbname="crystal_forge")
    db.autocommit = True

    def sql(query, params=()):
        with db.cursor() as cursor:
            cursor.execute(query, params)
            return cursor.fetchall() if cursor.description else []

    fixture_ids = {"job": None, "scan": None, "selected_caches": []}
    fixture_jobs, fixture_scans, fixture_derivations = [], [], []
    agent_completions = []
    diagnostic_source = "\n\n".join(inspect.getsource(function) for function in
                                    (cache_path_referenced, cache_process_snapshot,
                                     cache_directory_snapshot))

    def snapshot(stage, selected_machines=None):
        # Read only state and IDs from the VM database. Never select payloads,
        # logs, errors, metadata or URL/credential columns for this diagnostic.
        state = {"fixture": dict(fixture_ids),
                 "job": sql("SELECT id::text,status FROM build_jobs WHERE id=%s", (fixture_ids["job"],)),
                 "scan": sql("SELECT id::text,status FROM cve_scans WHERE id=%s", (fixture_ids["scan"],)),
                 "active_cache_push_jobs": sql("SELECT id::text,derivation_id::text,cache_destination_id,status FROM cache_push_jobs WHERE status IN ('pending','in_progress') OR (status='failed' AND retry_after IS NOT NULL) ORDER BY id")}
        print("Niks3 cleanup state: " + json.dumps({"stage": stage, **state}, sort_keys=True))
        captured = {}
        for name, machine in (selected_machines or machines).items():
            source = diagnostic_source + "\nimport json\nprint(json.dumps(cache_directory_snapshot(), sort_keys=True))"
            # The guest script contains code only, never credentials. The VM
            # driver's command logging therefore cannot expose argument inputs.
            status, output = machine.execute("python3 -c " + shlex.quote(source), timeout=8)
            if status == 0:
                captured[name] = json.loads(output)
                print("Niks3 cleanup snapshot: " + json.dumps(
                    {"stage": stage, "machine": name, **captured[name]}, sort_keys=True))
            else:
                # Tracebacks can include private data. Suppress all failed output.
                print("Niks3 cleanup snapshot: " + json.dumps(
                    {"stage": stage, "machine": name, "status": status,
                     "correlation": "unknown", "diagnostic_failed": True}, sort_keys=True))
        return captured

    snapshot("initial")

    def wait_row(query, params, predicate, description, timeout=180):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            rows = sql(query, params)
            if predicate(rows):
                return rows
            time.sleep(2)
        # Never dump whole rows: cache configuration and job payloads contain
        # credentials. A timeout is a test failure, not a skipped scenario.
        raise AssertionError(f"Timed out: {description}")

    lifecycle_source = "import json, uuid\n" + "\n\n".join(inspect.getsource(function) for function in
        (private_journal_message, input_owner_projection, input_owner_journal_state, cache_path_referenced,
         cache_process_snapshot, credential_consumers))
    safe_lifecycle_read = lifecycle_source + """
try:
    print(json.dumps(input_owner_journal_state(), sort_keys=True))
except Exception:
    print(json.dumps({'journal_unavailable':True}))
    raise SystemExit(1)
"""
    owner_barrier_source = lifecycle_source + """
try:
    state = input_owner_journal_state()
    process = cache_process_snapshot([])
    consumers = credential_consumers(process)
    unknown = process['unreadable'] or process['truncated']
    ready = not state['pending_operations'] and not consumers and not unknown
    print(json.dumps({'lifecycle':state, 'consumers':consumers, 'unknown':bool(unknown), 'ready':ready}, sort_keys=True))
    raise SystemExit(0 if ready else 1)
except Exception:
    print(json.dumps({'barrier_unavailable':True}))
    raise SystemExit(1)
"""
    consumer_barrier_source = lifecycle_source + """
try:
    process = cache_process_snapshot([])
    consumers = credential_consumers(process)
    ready = not consumers and not process['unreadable'] and not process['truncated']
    print(json.dumps({'consumers':consumers, 'ready':ready}, sort_keys=True))
    raise SystemExit(0 if ready else 1)
except Exception:
    print(json.dumps({'barrier_unavailable':True}))
    raise SystemExit(1)
"""

    def wait_projection(machine, source, description, timeout=120):
        command = "timeout 8 python3 -c " + shlex.quote(source)
        try:
            return json.loads(machine.wait_until_succeeds(command, timeout=timeout))
        except Exception:
            # Report the last SAFE projection on failure, not raw journal bytes
            # or a traceback. Every caller passes a projection-only guest script.
            status, output = machine.execute(command, timeout=8)
            try:
                projection = json.loads(output)
                allowed = {"lifecycle", "consumers", "unknown", "ready", "barrier_unavailable",
                           "read_owner_joined", "activation_terminal", "agent_boundary_unavailable", "reason"}
                if not isinstance(projection, dict) or not set(projection) <= allowed:
                    projection = {"projection_unavailable": True}
            except ValueError:
                projection = {"projection_unavailable": True}
            print("Niks3 barrier failure: " + json.dumps({"stage": description,
                  "status": status, "projection": projection}, sort_keys=True))
            raise

    def authoritative_barrier(stage, stop_builder=False):
        # Fixture producers create no new jobs during this barrier. Automatic
        # scan scheduling and cache pushes were disabled in the original setup.
        # Wait for every fixture identity, not just the last variant's rows.
        jobs = wait_row("SELECT id::text,status FROM build_jobs WHERE id=ANY(%s::uuid[]) ORDER BY id",
                        (fixture_jobs,), lambda rows: len(rows) == len(fixture_jobs)
                        and all(row[1] in ("success", "failed", "cancelled") for row in rows),
                        "all fixture build jobs terminal")
        scans = wait_row("SELECT id::text,status FROM cve_scans WHERE id=ANY(%s::uuid[]) ORDER BY id",
                         (fixture_scans,), lambda rows: len(rows) == len(fixture_scans)
                         and all(row[1] in ("completed", "failed", "cancelled") for row in rows),
                         "all fixture scans terminal")
        pushes = wait_row("SELECT id,status,(retry_after IS NOT NULL) FROM cache_push_jobs WHERE derivation_id=ANY(%s) ORDER BY id",
                          (fixture_derivations,), lambda rows: all(
                              row[1] in ("completed", "permanently_failed", "cancelled")
                              or (row[1] == "failed" and not row[2]) for row in rows),
                          "all fixture cache-push jobs terminal")
        owner_state = wait_projection(server, owner_barrier_source, stage + ": input owners", timeout=180)
        assert owner_state["lifecycle"]["started_count"] >= len(fixture_jobs), "input owner start evidence missing"
        # A stop is permitted only AFTER terminal jobs and resource-owner acks.
        # It cannot kill an outstanding owner to manufacture clean directories.
        if stop_builder:
            builder.succeed("systemctl stop crystal-forge-builder.service")
        for name, machine in (("builder", builder), ("agent", agent)):
            wait_projection(machine, consumer_barrier_source, stage + ": " + name + " consumers")
        producers = {}
        for name, machine, unit in (("builder", builder, "crystal-forge-builder"), ("agent", agent, "crystal-forge-agent")):
            status, output = machine.execute("systemctl is-active " + unit, timeout=8)
            assert status == 3 and output.strip() == "inactive", "fixture producer not quiescent"
            producers[name] = "stopped_after_completion"
        assert len(agent_completions) == len(targets), "agent read-owner completion evidence missing"
        print("Niks3 authoritative barrier: " + json.dumps({
            "stage": stage, "build_jobs": jobs, "scans": scans,
            "cache_push_jobs": [[row[0], row[1]] for row in pushes],
            "input_owner_started": owner_state["lifecycle"]["started_count"],
            "input_owner_completed": owner_state["lifecycle"]["completed_count"],
            "input_owner_pending": 0, "credential_consumers": 0,
            "agent_read_activation_completions": len(agent_completions),
            "producers": producers}, sort_keys=True))

    def request_target(target, derivation_id, commit_id):
        # A fresh desired target and pending request are one queue operation.
        # The heartbeat handler still performs the real authorization/claim.
        sql("""UPDATE pending_system_deployments SET status='superseded',completed_at=NOW()
            WHERE system_id=(SELECT id FROM systems WHERE hostname='agent') AND status='pending'""")
        return sql("""WITH requested AS (
            UPDATE systems SET desired_target=%s,desired_target_set_at=NOW()
            WHERE hostname='agent' RETURNING id
        ) INSERT INTO pending_system_deployments
          (system_id,target_store_path,source,request_action,requested_derivation_id,requested_commit_id)
          SELECT id,%s,'manual_deploy','deploy',%s,%s FROM requested RETURNING id""",
          (target, target, derivation_id, commit_id))[0][0]

    def heartbeat(capable=False, confidential=True):
        # Sign the exact legacy flat body, not a current packaged agent request.
        # Spoofed capability headers must not override the signed absent flag.
        state = {"hostname": "agent", "change_reason": "startup",
                 "store_path": agent.succeed("readlink -f /run/current-system").strip()}
        if capable:
            state["capabilities"] = {"supports_niks3": True}
        body = json.dumps(state, separators=(",", ":")).encode()
        signature = base64.b64encode(SigningKey(base64.b64decode(
            "+/GIbrjuyb3Hf2es5w+vWSlDUhEsAIojiyyfgskC7QA="
        )).sign(body).signature).decode()
        agent.succeed(f"umask 077; printf %s {shlex.quote(body.decode())} > /tmp/heartbeat.json")
        endpoint = "https://server" if confidential else "http://server:8000"
        agent.succeed(f"curl --fail --silent --show-error {endpoint}/agent/heartbeat "
                      "-H 'Content-Type: application/json' -H 'X-Key-ID: agent' "
                      "-H 'X-Agent-Supports-Niks3: true' "
                      "-H 'X-Agent-Capabilities: {\"supports_niks3\":true}' "
                      f"-H {shlex.quote('X-Signature: ' + signature)} "
                      "--data-binary @/tmp/heartbeat.json -o /tmp/heartbeat-response.json")
        # Print only public selection fields; private read credentials stay in
        # the protected VM-local response file until it is removed below.
        response = json.loads(agent.succeed("jq '{desired_target, runtime_caches: [.runtime_caches[] | {cache_url, cache_type}]}' /tmp/heartbeat-response.json"))
        agent.succeed("rm /tmp/heartbeat.json /tmp/heartbeat-response.json")
        return response

    def assert_withheld(pending_id, description, capable=True, confidential=True):
        response = heartbeat(capable=capable, confidential=confidential)
        assert response == {"desired_target": None, "runtime_caches": []}, description
        assert sql("SELECT status,delivered_at,completed_at,request_action FROM pending_system_deployments WHERE id=%s",
                   (pending_id,))[0] == ("pending", None, None, "deploy"), f"{description}: request consumed"

    def assert_delivered(target, read_url, description):
        response = heartbeat(capable=True)
        assert response == {"desired_target": target, "runtime_caches": [
            {"cache_url": read_url, "cache_type": "Niks3"}]}, description

    counter = 0

    def encrypt(value):
        nonlocal counter
        counter += 1
        nonce = counter.to_bytes(12, "big")
        ciphertext = AESGCM(hashlib.sha256(ENCRYPTION_KEY.encode()).digest()).encrypt(nonce, value.encode(), None)
        return "enc:v1:" + base64.b64encode(nonce).decode() + "." + base64.b64encode(ciphertext).decode()

    pem = lambda name: (credentials / name).read_text()
    signing_keys = [pem(f"signing-{index}.pub").strip() for index in range(2)]
    run_proxy_claims(server, builder, sql, encrypt, next(iter(targets.values())), builder_public_key, signing_keys)
    builder_id = sql("INSERT INTO builders(name, public_key, status, arch) VALUES ('niks3-remote', %s, 'active', 'x86_64-linux') RETURNING id", (builder_public_key,))[0][0]
    environment_id = sql("SELECT id FROM environments WHERE name='niks3'")[0][0]
    other_environment_id = sql("SELECT id FROM environments WHERE name='unrelated'")[0][0]
    sql("INSERT INTO builder_environment_assignments(builder_id, environment_id) VALUES (%s,%s)", (builder_id, environment_id))
    flake_id = sql("INSERT INTO flakes(name,repo_url) VALUES ('niks3-fixture','https://example.invalid/niks3') RETURNING id")[0][0]
    sql("UPDATE systems SET flake_id=%s,deployment_policy='manual' WHERE hostname='agent'", (flake_id,))
    sql("UPDATE scan_schedule_policy SET on_build=false WHERE id=1")

    # This usable public fallback sorts before every assigned destination. It
    # must never replace an assigned Niks3 cache, even for an incapable agent.
    global_id = sql("""INSERT INTO cache_destinations
        (name,cache_type,enabled,push_to,attic_public_key,require_sigs)
        VALUES ('a-global-public','Http',true,'https://cache:5753',%s,true)
        RETURNING id""", (signing_keys[0],))[0][0]

    # An enabled private destination in another environment must never be
    # selected for these jobs or disclosed to this agent.
    unrelated_id = sql("""INSERT INTO cache_destinations
        (name,cache_type,enabled,push_to,niks3_server_url,niks3_public_keys,
         niks3_write_auth_mode,niks3_auth_token,niks3_read_auth_mode,
         niks3_read_client_cert,niks3_read_client_key,niks3_read_ca_cert)
        VALUES ('unrelated-private','Niks3',true,'https://cache:5752','https://cache:5751',%s,
                'token',%s,'mtls',%s,%s,%s) RETURNING id""",
        (signing_keys, encrypt("unrelated-environment-token-470-00000000"), pem("wrong.crt"), encrypt(pem("wrong.key")), pem("ca.crt")))[0][0]
    sql("INSERT INTO cache_destination_environments(cache_destination_id,environment_id) VALUES (%s,%s)", (unrelated_id, other_environment_id))
    builder.succeed("systemctl start crystal-forge-builder.service")
    wait_row("SELECT current_session_id FROM builders WHERE id=%s", (builder_id,), lambda rows: rows and rows[0][0] is not None, "remote builder session")

    for variant, target in targets.items():
        published_name = f"z-published-{variant}"
        commit_id = sql("INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp,evaluation_status) VALUES (%s,%s,NOW(),'complete') RETURNING id", (flake_id, hashlib.sha1(variant.encode()).hexdigest()))[0][0]
        # The evaluated identity is seeded; background metadata hydration must
        # not try to fetch the intentionally nonexistent fixture repository.
        sql("INSERT INTO commit_artifacts_cache(commit_id,nixos_configurations) VALUES (%s,ARRAY['agent'])", (commit_id,))
        private = "private" in variant
        mtls_write = variant.startswith("mtls")
        read_port = 5752 if private else (5753 if variant.endswith("proxy") else 5751)
        read_url = f"https://cache:{read_port}"
        cache_id = sql("""INSERT INTO cache_destinations
            (name,cache_type,enabled,push_to,niks3_server_url,niks3_public_keys,
             niks3_write_auth_mode,niks3_auth_token,niks3_write_client_cert,
             niks3_write_client_key,niks3_write_ca_cert,niks3_read_auth_mode,
             niks3_read_client_cert,niks3_read_client_key,niks3_read_ca_cert,
             require_sigs,parallel_uploads,max_retries)
            VALUES (%s,'Niks3',true,%s,'https://cache:5751',%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,true,2,0)
            RETURNING id""", (published_name, read_url, signing_keys,
                "mtls" if mtls_write else "token", None if mtls_write else encrypt(TOKEN),
                pem("write.crt") if mtls_write else None, encrypt(pem("write.key")) if mtls_write else None,
                pem("ca.crt") if mtls_write else None, "mtls" if private else "none", pem("read.crt") if private else None,
                encrypt(pem("read.key")) if private else None, pem("ca.crt") if private else None))[0][0]
        sql("INSERT INTO cache_destination_environments(cache_destination_id,environment_id) VALUES (%s,%s)", (cache_id, environment_id))
        derivation_id = sql("""INSERT INTO derivations
            (commit_id,derivation_type,derivation_name,derivation_target,derivation_path,
             store_path,status_id,cf_agent_enabled,policy_requirements_met,scheduled_at)
            VALUES (%s,'nixos','agent','agent',%s,%s,5,true,true,NOW()) RETURNING id""",
            (commit_id, target["drv"], target["out"]))[0][0]
        # The driver knows the output identity, but the worker must compute it.
        # Remove any output preloaded through the server's derivation closure
        # so the server cannot export a prebuilt result with the build inputs.
        server.succeed(f"if test -e {shlex.quote(target['out'])}; then nix-store --delete {shlex.quote(target['out'])}; fi")
        server.fail(f"test -e {shlex.quote(target['out'])}")
        builder.fail(f"test -e {shlex.quote(target['out'])}")
        job_id = sql("INSERT INTO build_jobs(derivation_id,environment_id,status,queue_position,max_retries) VALUES (%s,%s,'queued',1000,0) RETURNING id", (derivation_id, environment_id))[0][0]
        fixture_jobs.append(str(job_id))
        fixture_derivations.append(derivation_id)
        fixture_ids.update(job=str(job_id), scan=None, selected_caches=[cache_id])
        snapshot(f"{variant}: queued", {"server": server})
        wait_row("SELECT status,builder_id FROM build_jobs WHERE id=%s", (job_id,),
                 lambda rows: rows and rows[0][0] in ("success", "failed"), f"{variant} remote completion")
        assert sql("SELECT status,builder_id FROM build_jobs WHERE id=%s", (job_id,))[0] == ("success", builder_id), f"{variant}: remote build failed"
        dispatch = sql("SELECT dispatched_cache_destination_id,cache_dispatch_recorded_at FROM build_jobs WHERE id=%s", (job_id,))[0]
        assert dispatch[0] == cache_id and dispatch[1] is not None, f"{variant}: dispatch identity was not bound"
        publication = sql("SELECT status,cache_destination,cache_destination_id,cache_destination_source FROM cache_push_jobs WHERE derivation_id=%s", (derivation_id,))
        assert publication and all(row == ("completed", published_name, cache_id, "database") for row in publication), f"{variant}: server did not verify selected destination"
        assert dispatch[0] != global_id
        print(f"Niks3 {variant}: remote completion, dispatch binding, and selected publication verified")
        snapshot(f"{variant}: publication completed", {"server": server})
        builder.succeed(f"test -e {shlex.quote(target['out'])}")
        agent.fail(f"test -e {shlex.quote(target['out'])}")

        # Verify both configured signatures on the actual published narinfo.
        nar_hash = Path(target["out"]).name.split("-", 1)[0]
        cert_args = "--cert /etc/niks3-fixtures/read.crt --key /etc/niks3-fixtures/read.key" if private else ""
        narinfo = agent.succeed(f"curl --fail --silent {cert_args} {read_url}/{nar_hash}.narinfo")
        for public_key in signing_keys:
            assert f"Sig: {public_key.split(':')[0]}:" in narinfo
        if private:
            agent.fail(f"curl --fail --silent {read_url}/nix-cache-info")
            agent.fail(f"curl --fail --silent --cert /etc/niks3-fixtures/wrong.crt --key /etc/niks3-fixtures/wrong.key {read_url}/nix-cache-info")
            agent.fail("curl --fail --silent --cert /etc/niks3-fixtures/read.crt --key /etc/niks3-fixtures/read.key https://cache:5752/api/cache-config")

        # A server-local scan must restore a missing output from the completed
        # publication using read credentials. Scanner success is independent:
        # the VM has no external vulnerability database and a synthetic target.
        server.succeed(f"if test -e {shlex.quote(target['out'])}; then nix-store --delete {shlex.quote(target['out'])}; fi")
        server.fail(f"test -e {shlex.quote(target['out'])}")
        # Rebased completion creates a durable post-build scan intent even when
        # automatic scanning is disabled. Reuse that active identity for this
        # explicit manual request instead of violating active-scan uniqueness.
        scan_id = sql("""INSERT INTO cve_scans(derivation_id,scanner_name,status,attempts,source_trigger)
            VALUES (%s,'vulnix','pending',0,'manual')
            ON CONFLICT (derivation_id) WHERE status IN
                ('awaiting_build','awaiting_closure','pending','in_progress')
            DO UPDATE SET source_trigger='manual' RETURNING id""", (derivation_id,))[0][0]
        fixture_ids["scan"] = str(scan_id)
        fixture_scans.append(str(scan_id))
        snapshot(f"{variant}: scan requested", {"server": server})
        server.wait_until_succeeds(f"test -e {shlex.quote(target['out'])}", timeout=120)
        wait_row("SELECT status FROM cve_scans WHERE id=%s", (scan_id,),
                 lambda rows: rows and rows[0][0] in ("completed", "failed"), f"{variant} CVE materialization/process cleanup", timeout=120)
        assert sql("SELECT lease_builder_id FROM cve_scans WHERE id=%s", (scan_id,))[0][0] is None
        scan_status = sql("SELECT status FROM cve_scans WHERE id=%s", (scan_id,))[0][0]
        print(f"Niks3 {variant}: CVE output materialized; scanner terminal status={scan_status}")
        snapshot(f"{variant}: scan terminal", {"server": server})

        # Add a new assigned identity only AFTER the real build publishes. Its
        # distinct usable read URL sorts first but has no publication evidence.
        # Endpoint object presence alone must not invent publication for an ID.
        earlier_url = "https://cache:5751" if read_port == 5753 else "https://cache:5753"
        earlier_id = sql("""INSERT INTO cache_destinations
            (name,cache_type,enabled,push_to,attic_public_key,require_sigs)
            VALUES (%s,'Http',true,%s,%s,true) RETURNING id""",
            (f"b-unpublished-{variant}", earlier_url, signing_keys[0]))[0][0]
        sql("INSERT INTO cache_destination_environments(cache_destination_id,environment_id) VALUES (%s,%s)",
            (earlier_id, environment_id))
        assert not sql("SELECT id FROM cache_push_jobs WHERE cache_destination_id=%s AND status='completed'", (earlier_id,))

        if variant == "mtls-private":
            # Moving the agent cannot authorize an unpublished unrelated cache
            # or consume the request. No pull should be instructed at all.
            sql("UPDATE systems SET environment_id=%s WHERE hostname='agent'", (other_environment_id,))
            isolated_pending = request_target(target["out"], derivation_id, commit_id)
            assert_withheld(isolated_pending, "unrelated environment received publication")
            agent.fail(f"test -e {shlex.quote(target['out'])}")
            print("Niks3 mtls-private: unrelated-environment delivery withheld without claim")
            sql("UPDATE systems SET environment_id=%s WHERE hostname='agent'", (environment_id,))

        pending_id = request_target(target["out"], derivation_id, commit_id)
        assert_withheld(pending_id, f"{variant}: legacy body received target/cache", capable=False)
        if private:
            assert_withheld(pending_id, f"{variant}: private reads crossed unverified transport", confidential=False)
        assert sql("SELECT desired_target FROM systems WHERE hostname='agent'")[0][0] == target["out"]
        agent.fail(f"test -e {shlex.quote(target['out'])}")
        assert_delivered(target["out"], read_url, f"{variant}: earlier assigned cache replaced published ID")
        assert sql("SELECT delivered_at IS NOT NULL FROM pending_system_deployments WHERE id=%s", (pending_id,))[0][0]
        sql("UPDATE cache_destinations SET name=%s WHERE id=%s", (f"zz-renamed-{variant}", cache_id))
        pending_id = request_target(target["out"], derivation_id, commit_id)
        assert_delivered(target["out"], read_url, f"{variant}: rename broke durable publication")
        assert sql("SELECT cache_destination,cache_destination_id FROM cache_push_jobs WHERE derivation_id=%s", (derivation_id,))[0] == (published_name, cache_id)

        pending_id = request_target(target["out"], derivation_id, commit_id)
        sql("UPDATE cache_destinations SET enabled=false WHERE id=%s", (cache_id,))
        assert_withheld(pending_id, f"{variant}: disabled source delivered unpublished fallback")
        sql("UPDATE cache_destinations SET enabled=true WHERE id=%s", (cache_id,))
        sql("UPDATE cache_destination_environments SET environment_id=%s WHERE cache_destination_id=%s",
            (other_environment_id, cache_id))
        assert_withheld(pending_id, f"{variant}: reassigned source delivered unpublished fallback")
        sql("UPDATE cache_destination_environments SET environment_id=%s WHERE cache_destination_id=%s",
            (environment_id, cache_id))

        if variant == "token-public":
            # The secondary is backed by the same real Niks3 object store, not
            # an invented completed row. Publish again with the packaged CLI,
            # then independently import the exact closure on the SERVER into a
            # fresh signature-required store before recording fixture evidence.
            # Workers were disabled at startup (no destination then existed),
            # so this fixture records completion only after these real probes.
            secondary_url = "https://cache:5753"
            secondary_id = sql("""INSERT INTO cache_destinations
                (name,cache_type,enabled,push_to,niks3_server_url,niks3_public_keys,
                 niks3_write_auth_mode,niks3_auth_token,niks3_read_auth_mode,require_sigs)
                VALUES ('y-secondary-published','Niks3',true,%s,'https://cache:5751',%s,
                        'token',%s,'none',true) RETURNING id""",
                (secondary_url, signing_keys, encrypt(TOKEN)))[0][0]
            sql("INSERT INTO cache_destination_environments(cache_destination_id,environment_id) VALUES (%s,%s)",
                (secondary_id, environment_id))
            builder.succeed(f"umask 077; printf %s {shlex.quote(TOKEN)} > /tmp/niks3-secondary-token")
            builder.succeed(f"niks3 push --server-url https://cache:5751 --auth-token-path /tmp/niks3-secondary-token {shlex.quote(target['out'])} >/dev/null 2>&1")
            builder.succeed("rm /tmp/niks3-secondary-token")
            probe_root = "/tmp/niks3-secondary-read"
            server.succeed(f"nix copy --refresh --from {secondary_url} --to 'local?root={probe_root}' "
                           f"--option trusted-public-keys {shlex.quote(' '.join(signing_keys))} "
                           f"--option require-sigs true {shlex.quote(target['out'])}")
            server.succeed(f"test -e {probe_root}{shlex.quote(target['out'])}")
            server.succeed(f"chmod -R u+w {probe_root}; rm -rf {probe_root}")
            sql("""INSERT INTO cache_push_jobs
                (derivation_id,store_path,cache_destination,cache_destination_id,cache_destination_source,status,completed_at)
                VALUES (%s,%s,'y-secondary-published',%s,'database','completed',NOW())""",
                (derivation_id, target["out"], secondary_id))
            sql("UPDATE cache_destinations SET enabled=false WHERE id=%s", (cache_id,))
            assert_delivered(target["out"], secondary_url, "verified secondary publication did not permit failover")
            assert sql("SELECT delivered_at IS NOT NULL FROM pending_system_deployments WHERE id=%s", (pending_id,))[0][0]
            sql("UPDATE cache_destinations SET enabled=false WHERE id=%s", (secondary_id,))
            sql("UPDATE cache_destinations SET enabled=true WHERE id=%s", (cache_id,))
            pending_id = request_target(target["out"], derivation_id, commit_id)
        print(f"Niks3 {variant}: published identity survives earlier assigned cache/rename; unavailable source preserves request")

        # The packaged capable agent still performs the actual pull and no-op
        # switch through the primary's original public/private read endpoint.
        # The agent has never started before the first variant, so its filtered
        # journal can have no cursor. A global cursor still precedes this pull;
        # the reader below filters only fresh agent records after that boundary.
        agent_cursor = agent.succeed("journalctl -n 0 --show-cursor --no-pager").strip().split("-- cursor: ")[-1]
        assert agent_cursor.startswith("s="), "fresh agent journal cursor unavailable"
        agent.succeed("systemctl restart crystal-forge-agent.service")
        agent.wait_until_succeeds(f"test -e {shlex.quote(target['out'])}", timeout=120)
        assert sql("SELECT delivered_at IS NOT NULL FROM pending_system_deployments WHERE id=%s", (pending_id,))[0][0], f"{variant}: packaged agent did not claim deployment"
        print(f"Niks3 {variant}: real agent pulled previously absent output")
        # This existing marker follows the awaited authenticated-copy owner,
        # including explicit read credential drop. Output presence alone does
        # not prove that boundary. Also require the detached activation terminal.
        agent_boundary_source = inspect.getsource(private_journal_message) + "\n\n" + inspect.getsource(agent_read_projection) + f"""
import json, re, subprocess
try:
    journal = subprocess.run(['journalctl','-b','-u','crystal-forge-agent','--after-cursor',{agent_cursor!r},'--no-pager','-o','json'], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=1, check=True)
    projection = agent_read_projection([json.loads(line) for line in journal.stdout.splitlines()])
    units = projection['activation_units']
    joined = projection['read_owner_joined']
    terminal = False
    if len(units) == 1:
        unit = units[0]
        # Explicit UNIT matching survives --collect removing the transient unit
        # from systemd's current unit lookup. The record must come from PID 1.
        activation = subprocess.run(['journalctl','-b','UNIT=' + unit + '.service','--no-pager','-o','json'], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=1, check=True)
        records = [json.loads(line) for line in activation.stdout.splitlines()]
        terminal = any(record.get('_PID') == '1' and private_journal_message(record) == unit + '.service: Deactivated successfully.' for record in records)
    ready = joined and terminal
    print(json.dumps({{'read_owner_joined':joined,'activation_terminal':terminal,'ready':ready}}))
    raise SystemExit(0 if ready else 1)
except Exception as error:
    reason = 'journal_read_failed' if isinstance(error, subprocess.CalledProcessError) else 'journal_timeout' if isinstance(error, subprocess.TimeoutExpired) else 'journal_projection_failed'
    print(json.dumps({{'agent_boundary_unavailable':True,'reason':reason}}))
    raise SystemExit(1)
"""
        boundary = wait_projection(agent, agent_boundary_source, variant + ": agent read/activation completion")
        agent_completions.append(boundary)
        wait_projection(agent, consumer_barrier_source, variant + ": agent consumers")
        print(f"Niks3 {variant}: agent read-owner joined, credentials dropped, activation terminal, consumers=0")
        agent.succeed("systemctl stop crystal-forge-agent.service")
        pending_id = request_target(target["out"], derivation_id, commit_id)
        sql("DELETE FROM cache_destinations WHERE id=%s", (cache_id,))
        assert_withheld(pending_id, f"{variant}: deleted ID delivered unpublished fallback")
        assert sql("SELECT cache_destination_id FROM cache_push_jobs WHERE derivation_id=%s AND cache_destination_id=%s",
                   (derivation_id, cache_id)), f"{variant}: deletion lost historical publication ID"
        print(f"Niks3 {variant}: deleted primary withholds target and leaves pending request unclaimed")
        sql("UPDATE systems SET desired_target=NULL,desired_target_set_at=NULL WHERE hostname='agent'")
        sql("UPDATE pending_system_deployments SET status='superseded',completed_at=NOW() WHERE id=%s", (pending_id,))
        sql("UPDATE cache_destinations SET enabled=false WHERE id=%s", (earlier_id,))
        snapshot(f"{variant}: delivery audit complete", {"server": server})

    # Invalid write identity must fail against the real native-mTLS server.
    # Keep tokens in protected files and suppress output, including URLs.
    builder.succeed("command -v niks3; niks3 --help >/dev/null 2>&1")
    builder.succeed("umask 077; printf '%s' invalid-niks3-token-470-00000000000000000 > /tmp/niks3-invalid-token")
    some_target = next(iter(targets.values()))["out"]
    builder.fail(f"niks3 push --server-url https://cache:5751 --auth-token-path /tmp/niks3-invalid-token {shlex.quote(some_target)} >/dev/null 2>&1")
    builder.fail(f"niks3 push --server-url https://cache:5751 --client-cert /etc/niks3-fixtures/wrong.crt --client-key /etc/niks3-fixtures/wrong.key {shlex.quote(some_target)} >/dev/null 2>&1")
    builder.succeed("rm /tmp/niks3-invalid-token")

    # Reproduce a concurrent audit with an actual request-owned input upload.
    # The native TLS forwarding gate acknowledges the first request, then blocks
    # on an explicit release event. No sleep or folder-disappearance polling
    # creates the boundary. The original five variants have already completed.
    authoritative_barrier("before handshaked audit", stop_builder=True)
    exact_cleanup = "test -z \"$(find /tmp /var/lib/crystal-forge /var/lib/crystal-forge-agent -maxdepth 3 -type d -name 'cf-cache-*' -print 2>/dev/null)\""
    server.succeed(exact_cleanup)
    gate_source = f"""
import http.client, http.server, os, pathlib, ssl, threading
os.umask(0o077)
release = '/run/niks3-diagnostic-release'
os.mkfifo(release, 0o600)
fifo = os.open(release, os.O_RDWR)
event = threading.Event()
credentials = pathlib.Path({str(credentials)!r})
upstream = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
upstream.load_verify_locations(str(credentials / 'ca.crt'))
class Gate(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def forward(self):
        pathlib.Path('/run/niks3-diagnostic-entered').touch(mode=0o600)
        event.wait()
        # Headers, body, native response and private TLS files stay in memory.
        # Never log them, even on disconnect or native provider failure.
        try:
            body = self.rfile.read(int(self.headers.get('Content-Length', '0')))
            headers = dict(self.headers)
            headers.pop('Host', None)
            connection = http.client.HTTPSConnection('cache', 5751, context=upstream, timeout=8)
            connection.request(self.command, self.path, body=body, headers=headers)
            response = connection.getresponse()
            data = response.read()
            self.send_response(response.status)
            for key, value in response.getheaders():
                if key.lower() not in ('transfer-encoding', 'connection', 'content-length'):
                    self.send_header(key, value)
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            connection.close()
        except Exception:
            self.close_connection = True
    do_GET = do_POST = do_PUT = do_DELETE = do_HEAD = forward
listener = http.server.ThreadingHTTPServer(('0.0.0.0', 5754), Gate)
tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
tls.load_cert_chain(str(credentials / 'server.crt'), str(credentials / 'server.key'))
listener.socket = tls.wrap_socket(listener.socket, server_side=True)
threading.Thread(target=listener.serve_forever, daemon=True).start()
pathlib.Path('/run/niks3-diagnostic-ready').touch(mode=0o600)
os.read(fifo, 1)
event.set()
threading.Event().wait()
"""
    cache.succeed("systemd-run --unit=niks3-diagnostic-gate python3 -c " + shlex.quote(gate_source))
    cache.wait_until_succeeds("test -e /run/niks3-diagnostic-ready", timeout=30)
    race_cache = sql("""INSERT INTO cache_destinations
        (name,cache_type,enabled,push_to,niks3_server_url,niks3_public_keys,
         niks3_write_auth_mode,niks3_auth_token,niks3_read_auth_mode,require_sigs,parallel_uploads,max_retries)
        VALUES ('niks3-diagnostic-input','Niks3',true,'https://cache:5751','https://cache:5754',%s,
                'token',%s,'none',true,2,0) RETURNING id""", (signing_keys, encrypt(TOKEN)))[0][0]
    sql("INSERT INTO cache_destination_environments(cache_destination_id,environment_id) VALUES (%s,%s)",
        (race_cache, environment_id))
    race_commit = sql("INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp,evaluation_status) VALUES (%s,%s,NOW(),'complete') RETURNING id",
                      (flake_id, hashlib.sha1(b"niks3-diagnostic-input").hexdigest()))[0][0]
    sql("INSERT INTO commit_artifacts_cache(commit_id,nixos_configurations) VALUES (%s,ARRAY['agent'])", (race_commit,))
    # Derivation paths have a global uniqueness constraint. Instantiate a real
    # distinct input root, using the already imported fixture shell closure.
    shell = server.succeed("nix-store --query --binding builder " + shlex.quote(target["drv"])).strip()
    expression = 'builtins.derivation { name="niks3-diagnostic-input"; system="x86_64-linux"; builder=' + json.dumps(shell) + '; args=["-c" "exit 0"]; }'
    race_drv = server.succeed("nix-instantiate --expr " + shlex.quote(expression)).strip()
    race_out = server.succeed("nix-store --query --outputs " + shlex.quote(race_drv)).strip()
    race_derivation = sql("""INSERT INTO derivations
        (commit_id,derivation_type,derivation_name,derivation_target,derivation_path,store_path,status_id,
         cf_agent_enabled,policy_requirements_met,scheduled_at)
        VALUES (%s,'nixos','agent','agent',%s,%s,5,true,true,NOW()) RETURNING id""",
        (race_commit, race_drv, race_out))[0][0]
    race_job = sql("INSERT INTO build_jobs(derivation_id,environment_id,status,queue_position,max_retries) VALUES (%s,%s,'queued',1000,0) RETURNING id",
                   (race_derivation, environment_id))[0][0]
    session = str(sql("SELECT current_session_id FROM builders WHERE id=%s", (builder_id,))[0][0])

    def protected_request(path, payload):
        body = json.dumps(payload, separators=(",", ":"))
        timestamp = datetime.now(timezone.utc).isoformat()
        key = SigningKey(base64.b64decode("+/GIbrjuyb3Hf2es5w+vWSlDUhEsAIojiyyfgskC7QA="))
        signature = base64.b64encode(key.sign(f"POST\n{path}\n{timestamp}\n{body}".encode()).signature).decode()
        request = {"path": path, "body": body, "headers": {
            "Content-Type": "application/json", "X-Builder-ID": str(builder_id),
            "X-Builder-Session-ID": session, "X-Timestamp": timestamp, "X-Signature": signature}}
        # The copy operation logs filenames only. Do not place authorization in
        # a guest command or the diagnostic log. Remove the private host file.
        with tempfile.TemporaryDirectory(prefix="niks3-diagnostic-request-") as directory:
            request_file = Path(directory) / "request.json"
            request_file.write_text(json.dumps(request))
            request_file.chmod(0o600)
            server.copy_from_host(str(request_file), "/run/niks3-diagnostic-request.json")
        server.succeed("chmod 600 /run/niks3-diagnostic-request.json")

    client_source = """
import http.client, json, pathlib, ssl
request = json.loads(pathlib.Path('/run/niks3-diagnostic-request.json').read_text())
tls = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
tls.load_verify_locations('/etc/niks3-fixtures/ca.crt')
connection = http.client.HTTPSConnection('server', context=tls, timeout=60)
try:
    connection.request('POST', request['path'], body=request['body'], headers=request['headers'])
    response = connection.getresponse()
    body = response.read()
    data = json.loads(body) if response.status == 200 else {}
    print(json.dumps({'status':response.status, 'job':data.get('job',{}).get('id'),
        'destination':data.get('derivation',{}).get('cache_push',{}).get('cache_destination_id')}))
except Exception:
    print(json.dumps({'request_failed':True}))
finally:
    connection.close()
"""
    protected_request(f"/api/v1/builders/{builder_id}/next-job",
                      {"capabilities": {"niks3_cache": True}, "supported_execution_strategies": ["server_derivation"]})
    claim = json.loads(server.succeed("python3 -c " + shlex.quote(client_source)))
    assert claim == {"status": 200, "job": str(race_job), "destination": race_cache}, "diagnostic input claim failed"
    protected_request(f"/api/v1/builders/{builder_id}/jobs/{race_job}/publish-derivation-closure", {})
    server.succeed("systemd-run --unit=niks3-diagnostic-client python3 -c " + shlex.quote(client_source))
    cache.wait_until_succeeds("test -e /run/niks3-diagnostic-entered", timeout=60)
    fixture_ids.update(job=str(race_job), scan=None, selected_caches=[race_cache])
    snapshot("handshake: native input upload blocked", {"server": server})
    server.succeed("systemctl stop niks3-diagnostic-client.service")
    held = snapshot("handshake: request client dropped; upload not released", {"server": server})
    owners = [process for process in held["server"]["process"]["processes"]
              if process["paths"] and process["exe"].removeprefix(".").removesuffix("-wrapped") == "niks3"]
    assert owners, "handshaked input upload owner remains unknown"
    lifecycle = json.loads(server.succeed("timeout 8 python3 -c " + shlex.quote(safe_lifecycle_read)))
    started = [event for event in lifecycle["events"] if event["phase"] == "started"
               and event["child_pid"] in {owner["pid"] for owner in owners}]
    assert len(started) == 1 and lifecycle["pending_operations"] == [started[0]["operation"]], "handshaked owner lifecycle not uniquely bound"
    print("Niks3 handshaked running owner: " + json.dumps(started[0], sort_keys=True))
    # This is the unchanged assertion, deliberately evaluated while its real
    # consuming child is held at a confirmed TLS request boundary.
    server.fail(exact_cleanup)
    print("Niks3 diagnostic race: exact cleanup assertion fails with handshaked request-owned upload alive")
    cache.succeed("python3 -c " + shlex.quote("import os; fd=os.open('/run/niks3-diagnostic-release', os.O_WRONLY); os.write(fd,b'1'); os.close(fd)"))
    # A pidfd exit alone cannot prove reap or TempDir destruction. Match the
    # owner's explicit post-reap, post-drop acknowledgment instead, and require
    # no live Nix/Niks3 clients. This barrier never scans credential folders.
    finished = wait_projection(server, owner_barrier_source, "handshake: reap/drop acknowledgment")
    completed = [event for event in finished["lifecycle"]["events"]
                 if event["phase"] == "completed" and event["operation"] == started[0]["operation"]]
    assert len(completed) == 1 and completed[0]["child_pid"] == started[0]["child_pid"]
    assert completed[0]["child_reaped"] and completed[0]["cleanup_attempted"] and completed[0]["outcome"] == "success"
    print("Niks3 handshaked completed owner: " + json.dumps(completed[0], sort_keys=True))
    snapshot("handshake: input owner acknowledged reap and credential drop", {"server": server})
    server.succeed(exact_cleanup)
    assert not sql("SELECT id FROM cache_push_jobs WHERE derivation_id=%s", (race_derivation,)), "input-only diagnostic fabricated output publication"
    cache.succeed("systemctl stop niks3-diagnostic-gate.service")
    server.succeed("rm /run/niks3-diagnostic-request.json")
    # Delete only this scratch job, AFTER its exact input operation completes.
    # It never claims an output publication or successful build. Original jobs
    # and publication records remain intact for the final all-fixture barrier.
    sql("DELETE FROM build_jobs WHERE id=%s", (race_job,))
    sql("DELETE FROM derivations WHERE id=%s", (race_derivation,))
    sql("DELETE FROM commit_artifacts_cache WHERE commit_id=%s", (race_commit,))
    sql("DELETE FROM commits WHERE id=%s", (race_commit,))
    sql("DELETE FROM cache_destinations WHERE id=%s", (race_cache,))

    authoritative_barrier("before final audit")
    snapshot("before final audit")
    # Audit service logs in memory; do not print a failing secret or full log.
    secrets = [TOKEN, "unrelated-environment-token-470-00000000",
               "proxy-claim-attic-secret", "proxy-claim-s3-secret", "proxy-claim-niks3-secret"]
    secrets += [pem(f"{name}.key").splitlines()[1] for name in ("write", "read", "wrong")]
    for name, machine in machines.items():
        logs = machine.succeed("journalctl --no-pager -u crystal-forge-server -u crystal-forge-builder -u crystal-forge-agent -u niks3 -u nginx -u garage")
        assert not any(secret in logs for secret in secrets), f"credential leaked in {name} service log"
        assert "X-Amz-Signature=" not in logs, f"presigned upload URL leaked in {name} service log"
        snapshot("immediately before exact cleanup assertion", {name: machine})
        machine.succeed("test -z \"$(find /tmp /var/lib/crystal-forge /var/lib/crystal-forge-agent -maxdepth 3 -type d -name 'cf-cache-*' -print 2>/dev/null)\"")
    with db.cursor() as cursor:
        cursor.execute("SELECT COALESCE(scan_metadata::text,'') FROM cve_scans UNION ALL SELECT message FROM cve_scan_diagnostic_events")
        diagnostics = cursor.fetchall()
        assert len(diagnostics) >= len(targets), "scan audit had no persisted evidence"
        assert not any(secret in row[0] for row in diagnostics for secret in secrets), "credential leaked in persisted scan diagnostics"
    db.close()
    print("Niks3: all five remote-builder/agent variants and credential audits passed")
