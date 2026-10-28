"""Fail-closed Niks3 VM integration; invoked by checks/niks3-cache.

Queue setup and evaluated identities are seeded. The packaged remote builder
claims a real job, imports its derivation, builds it, pushes it, and signs its
completion. The server must independently observe refreshed exact-root metadata
at the selected cache before confirming publication. Completion does not prove
NAR integrity. Fresh signature-required copies, CVE imports and agent pulls
exercise materialization separately.
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


def production_observations(machine, plane):
    """Reads only nginx's boolean projection, never URI/query/header values."""
    assert plane in ("api", "s3", "read", "target")
    return json.loads(machine.succeed("python3 -c " + shlex.quote(f"""
import json, pathlib
path = pathlib.Path('/var/log/nginx/production-{plane}-observer.json')
records = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
allowed = {{'authorization','client_certificate','write_subject','put','post','backend','signature_validated','status'}}
assert all(set(record) == allowed for record in records)
assert all(all(type(value) is bool for key,value in record.items() if key != 'status') and type(record['status']) is int for record in records)
print(json.dumps(records))
""")))


def check_production_transport(production, probe, credentials):
    """Proves frontend mTLS, subject bounds, private origin and unsigned denial."""
    from cryptography import x509
    pki = Path(credentials) / "production"
    roots = x509.load_pem_x509_certificates((pki / "server-roots-ab.pem").read_bytes())
    a, b, c = (x509.load_pem_x509_certificate((pki / (name + ".crt")).read_bytes())
               for name in ("server-ca-a", "server-ca-b", "client-ca-c"))
    assert len(roots) == 2 and {root.subject for root in roots} == {a.subject, b.subject}
    assert c.subject not in {root.subject for root in roots}
    assert len({root.public_key().public_bytes_raw() for root in (a, b, c)}) == 3
    for name in ("write-client-1", "write-client-2"):
        cert = x509.load_pem_x509_certificate((pki / (name + ".crt")).read_bytes())
        assert cert.issuer == c.subject and cert.subject.rfc4514_string() == "CN=write"
        c.public_key().verify(cert.signature, cert.tbs_certificate_bytes)
    assert (pki / "write-client-1.key").read_bytes() != (pki / "write-client-2.key").read_bytes()
    base = "curl --silent --show-error --max-time 8 --cacert /etc/niks3-production-pki/server-ca-a.crt "
    endpoint = "https://push-cache.test:5754/api/cache-config"
    probe.fail(base + "--fail " + endpoint + " >/dev/null 2>&1")
    probe.fail(base + "--fail --cert /etc/niks3-production-pki/wrong-issuer.crt --key /etc/niks3-production-pki/wrong-issuer.key " + endpoint + " >/dev/null 2>&1")
    for name in ("write-client-1", "write-client-2", "wrong-subject"):
        probe.succeed(base + f"--fail --cert /etc/niks3-production-pki/{name}.crt --key /etc/niks3-production-pki/{name}.key " + endpoint + " >/dev/null 2>&1")
    # A valid issuer does not imply write authorization. This protected write
    # route rejects the wrong subject before parsing or creating an upload.
    status = probe.succeed(base + "--cert /etc/niks3-production-pki/wrong-subject.crt --key /etc/niks3-production-pki/wrong-subject.key -X POST --data '{}' -o /dev/null -w '%{http_code}' https://push-cache.test:5754/api/pending_closures 2>/dev/null").strip()
    assert status == "401", "native bound subject accepted wrong client"
    # Forged verified headers cannot replace mandatory certificate verification.
    probe.fail(base + "--fail -H 'X-SSL-Client-Verify: SUCCESS' -H 'X-SSL-Client-Dn: CN=write' " + endpoint + " >/dev/null 2>&1")
    probe.fail("curl --silent --max-time 3 --fail http://production:5755/api/cache-config >/dev/null 2>&1")
    production.fail("curl --silent --max-time 3 --fail http://127.0.0.1:5755/api/cache-config >/dev/null 2>&1")
    production.fail("runuser -u nobody -- curl --silent --max-time 3 --fail http://127.0.0.1:5755/api/cache-config >/dev/null 2>&1")
    production.fail("runuser -u nobody -- curl --silent --max-time 3 --fail --unix-socket /run/niks3-production-api/native.sock http://localhost/api/cache-config >/dev/null 2>&1")
    production.succeed("python3 -c " + shlex.quote("""
import os, pathlib, stat
path = pathlib.Path('/run/niks3-production-api/native.sock')
info = path.stat()
assert stat.S_ISSOCK(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o660
assert info.st_uid == 63071
assert stat.S_IMODE(path.parent.stat().st_mode) == 0o750
"""))
    status = probe.succeed("curl --silent --show-error --max-time 8 --cacert /etc/niks3-production-pki/server-ca-b.crt -X PUT --data '' -o /dev/null -w '%{http_code}' https://s3-cache.test:3901/production-niks3/unsigned-denial 2>/dev/null").strip()
    assert status == "403", "private Garage accepted unsigned PUT"
    print("Niks3 split PKI: A/B/C distinct; mandatory client issuer C; native CN=write binding; private socket/origin; unsigned S3 PUT denied")


def run_production_transport_fixture(production, probe, target, credentials):
    """Exercises the opt-in native fixture; cannot replace the CF remote gate."""
    check_production_transport(production, probe, credentials)
    probe.succeed("nix-store --realise " + shlex.quote(target["drv"]) + " >/dev/null 2>&1")
    cli = "niks3 push --server-url https://push-cache.test:5754 --client-cert /etc/niks3-production-pki/write-client-1.crt --client-key /etc/niks3-production-pki/write-client-1.key --ca-cert "
    for root in ("server-ca-a.crt", "server-ca-b.crt"):
        probe.fail(cli + "/etc/niks3-production-pki/" + root + " -- " + shlex.quote(target["out"]) + " >/dev/null 2>&1")
    probe.succeed(cli + "/etc/niks3-production-pki/server-roots-ab.pem -- " + shlex.quote(target["out"]) + " >/dev/null 2>&1")
    records = production_observations(production, "s3")
    puts = [record for record in records if record["put"] and not record["backend"] and 200 <= record["status"] < 300]
    assert puts and all(not record["authorization"] and not record["client_certificate"] and record["signature_validated"] for record in puts)
    print("Niks3 split PKI fixture: A-only and B-only fail; A+B native API/direct signed HTTPS S3 upload succeeds; no client Basic/Bearer header or S3 client certificate")


def production_cli_projection(machine, expected_bundle=None, expected_digest=None):
    """Checks live CF-launched CLI flags, protected files and isolated env privately."""
    digest = expected_digest or hashlib.sha256(Path(expected_bundle).read_bytes()).hexdigest()
    source = f"""
import hashlib,json,os,pathlib,stat,time
deadline = time.monotonic() + 6
while time.monotonic() < deadline:
    for proc in pathlib.Path('/proc').iterdir():
        if not proc.name.isdigit():
            continue
        try:
            executable = pathlib.Path(os.readlink(proc / 'exe')).name
            if executable.removeprefix('.').removesuffix('-wrapped') != 'niks3':
                continue
            before = (proc / 'stat').read_text().rsplit(')',1)[1].split()[19]
            args = (proc / 'cmdline').read_bytes().decode().split('\\0')
            flags = ('--client-cert','--client-key','--ca-cert')
            if not all(flag in args for flag in flags):
                continue
            # Resolve protected paths through the consuming process's mount
            # namespace; systemd PrivateTmp does not expose them at guest /tmp.
            files = [proc / 'root' / pathlib.Path(args[args.index(flag)+1]).relative_to('/') for flag in flags]
            environment = (proc / 'environ').read_bytes().decode()
            env = {{item.split('=',1)[0] for item in environment.split('\\0') if '=' in item}}
            read_secrets = [(pathlib.Path('/etc/niks3-production-pki') / name).read_text() for name in ('basic-username','basic-password')]
            projection = {{'pid':int(proc.name),'mtls_flags':True,
                'write_url_matches':args[args.index('--server-url')+1] == 'https://push-cache.test:5754/',
                'token_flags_absent':not any(arg.startswith('--auth-token') for arg in args),
                'aws_environment_absent':not any(key.startswith('AWS_') for key in env),
                'token_environment_absent':'NIKS3_AUTH_TOKEN_FILE' not in env,
                'basic_read_absent':not any(secret in '\\0'.join(args)+environment for secret in read_secrets) and not any('netrc' in arg for arg in args),
                'bundle_matches':hashlib.sha256(files[2].read_bytes()).hexdigest() == {digest!r},
                'client_pair_matches':all(hashlib.sha256(path.read_bytes()).digest() == hashlib.sha256((pathlib.Path('/etc/niks3-production-pki') / name).read_bytes()).digest()
                    for path,name in zip(files[:2],('write-client-1.crt','write-client-1.key'))),
                'protected_files':all(stat.S_IMODE(path.stat().st_mode) == 0o600 for path in files),
                'protected_directory':all(stat.S_IMODE(path.parent.stat().st_mode) == 0o700 for path in files)}}
            after = (proc / 'stat').read_text().rsplit(')',1)[1].split()[19]
            if before == after and projection['bundle_matches']:
                print(json.dumps(projection))
                assert all(value for key,value in projection.items() if key != 'pid')
                raise SystemExit(0)
        except (FileNotFoundError,ProcessLookupError):
            pass
print(json.dumps({{'command_projection_unavailable':True}}))
raise SystemExit(1)
"""
    # Sampling has a real command/scan deadline and never persists argument or
    # environment bytes. A missing observation is a failed proof, not a skip.
    return json.loads(machine.succeed("timeout 8 python3 -c " + shlex.quote(source), timeout=8))


def fresh_split_target(server, template):
    """Preflights a VM-only random-content derivation, then removes its output.

    A server sandbox build supplies the actual NAR hash, never a publication.
    The exact fresh output is deleted before dispatch to require a remote build.
    Fresh payload bytes defeat Niks3's content-addressed NAR deduplication even
    when an earlier fixture already uploaded the same activation script.
    """
    return json.loads(server.succeed("python3 -c " + shlex.quote(f"""
import json,pathlib,secrets,subprocess,tempfile
def run(args):
    result = subprocess.run(args,capture_output=True,timeout=30)
    assert result.returncode == 0, 'fresh fixture derivation preparation failed'
    return result.stdout.decode().strip()
shown = json.loads(run(['nix','derivation','show',{template['drv']!r}]))
# The pinned evaluator uses version 4 and basename-keyed derivation records.
assert shown['version'] == 4 and set(shown['derivations']) == {{pathlib.Path({template['drv']!r}).name}}
template = shown['derivations'][pathlib.Path({template['drv']!r}).name]
assert template['version'] == 4
busybox = str(pathlib.Path(template['builder']).parent)
package = str(pathlib.Path(busybox).parent)
assert template['builder'].startswith('/nix/store/') and template['builder'].endswith('/bin/sh')
with tempfile.TemporaryDirectory(prefix='niks3-fresh-output-') as directory:
    directory = pathlib.Path(directory)
    payload = directory/'payload'
    payload.write_bytes(secrets.token_bytes(16 * 1024 * 1024))
    payload.chmod(0o600)
    source = run(['nix-store','--add',str(payload)])
    script = (busybox+'/mkdir -p "$out/bin" "$out/sw"\\n'
              +busybox+'/printf '+json.dumps('#!'+template['builder']+'\\nexit 0\\n')+' > "$out/bin/switch-to-configuration"\\n'
              +busybox+'/chmod +x "$out/bin/switch-to-configuration"\\n'
              +busybox+'/cp "$fixturePayload" "$out/split-pki-workload"\\n')
    expression = ('builtins.derivation {{ name = "niks3-mtls-split-fresh"; system = "x86_64-linux"; '
                  # JSON paths have no Nix context. Restore the packaged shell
                  # dependency explicitly so the sandbox mounts its full closure.
                  +'builder = (builtins.storePath '+json.dumps(package)+') + "/bin/sh"; args = [ "-c" '+json.dumps(script)+' ]; '
                  +'fixturePayload = builtins.storePath '+json.dumps(source)+'; }}')
    drv = run(['nix-instantiate','--expr',expression])
    out = run(['nix-store','--query','--outputs',drv])
    assert not pathlib.Path(out).exists(), 'fresh output already realized'
    assert run(['nix-store','--realise',drv,'--option','sandbox','true']) == out
    assert (pathlib.Path(out)/'split-pki-workload').read_bytes() == payload.read_bytes()
    nar_hash = run(['nix','hash','path','--type','sha256','--base32',out])
    assert len(nar_hash) == 52 and all(c in '0123456789abcdfghijklmnpqrsvwxyz' for c in nar_hash)
    run(['nix-store','--delete',out])
    assert not pathlib.Path(out).exists(), 'fresh server preflight output remains'
    print(json.dumps({{'drv':drv,'out':out,'nar_key':'nar/'+nar_hash+'.nar.zst'}}))
""")))


def onboarding_ca_expectation(raw):
    """Selects ordinary DER identities from the known synthetic-plus-NSS fixture.

    This independent fixture oracle does not implement the production annotation
    grammar. The separate Go/Rust gate validates that parser. Trusted PEM blocks
    never become expected roots; ordinary DER order and duplicates are retained.
    """
    import re
    from cryptography import x509

    assert b"PRIVATE KEY" not in raw, "fixture CA input contains key material"
    bodies = re.findall(rb"(?m)^-----BEGIN CERTIFICATE-----\r?\n([A-Za-z0-9+/=\r\n]+)^-----END CERTIFICATE-----\r?$", raw)
    assert len(bodies) == raw.count(b"-----BEGIN CERTIFICATE-----") == 124, "ordinary fixture root count changed"
    assert raw.count(b"-----BEGIN TRUSTED CERTIFICATE-----") == 172, "NSS trusted fixture count changed"
    outside_pem = re.sub(rb"(?m)^-----BEGIN (?:TRUSTED )?CERTIFICATE-----\r?\n[A-Za-z0-9+/=\r\n]+^-----END (?:TRUSTED )?CERTIFICATE-----\r?$", b"", raw)
    assert sum(bool(line) and not line.startswith(b"#") for line in outside_pem.splitlines()) == 605, "NSS annotation fixture count changed"
    roots, canonical = [], []
    for body in bodies:
        der = base64.b64decode(b"".join(body.splitlines()), validate=True)
        x509.load_der_x509_certificate(der)
        encoded = base64.b64encode(der).decode("ascii")
        roots.append(hashlib.sha256(der).hexdigest())
        canonical.append("-----BEGIN CERTIFICATE-----\n" + "\n".join(
            encoded[offset:offset + 64] for offset in range(0, len(encoded), 64))
            + "\n-----END CERTIFICATE-----\n")
    assert len(set(roots)) == 124, "fixture ordinary roots are not distinct"
    return "".join(canonical), roots


def save_split_ca(server, cache_id, expected_ca_digest, expected_raw_digest):
    """Saves raw synthetic A+B+C plus NSS write trust through the real admin API.

    Retained client identity and Basic read credentials are omitted. Response
    bytes and session cookies stay in the guest; only safe evidence is emitted.
    Probe results are projected before assertions. Protected handler access is
    distinct from the later CLI upload, storage and publication proofs.
    """
    source = f"""
import hashlib,http.client,json,pathlib,re,secrets,ssl,subprocess
from http.cookies import SimpleCookie
stage,http_status = 'registration',None
replacement_projection = None
omission_projection = None
probe_results = []
allowed_codes = ('ca_bundle_framing_invalid','ca_certificate_invalid','ca_bundle_content_unsupported',
    'ca_bundle_trust_ambiguous','ca_bundle_empty','ca_bundle_too_large','client_identity_invalid',
    'client_certificate_invalid','client_private_key_invalid','client_identity_mismatch',
    'tls_verification_failed','dns_or_connection_failed','http_response_failed',
    'invalid_niks3_metadata','target_policy_rejected','probe_timeout','probe_failed',
    'proxy_access_denied','method_rejected','mtls_subject_rejected','token_rejected',
    'unexpected_write_response','route_unsupported','auth_control_inconclusive',
    'probe_response_too_large','write_credentials_missing','write_credentials_invalid',
    'write_route_verified','write_route_verified_certificate_gate','read_access_verified')
def probe_projection(result):
    # Export only typed evidence and fixed labels. All other response fields,
    # including provider prose and public metadata, remain private inputs.
    value = result if type(result) is dict else {{}}
    def boolean(field):
        item = value.get(field)
        return item if type(item) is bool else None
    def code(field):
        item = value.get(field)
        return item if type(item) is str and item in allowed_codes else 'unknown'
    scope = value.get('probe_scope')
    status = value.get('status_code')
    return {{'stage':stage,'probe_scope':scope if type(scope) is str and scope in ('read','write','all') else 'unknown',
        'status_code':status if type(status) is int and 100 <= status <= 599 else 'unknown',
        'ok':boolean('ok'),'write_api_reachable':boolean('write_api_reachable'),
        'write_authn_valid':boolean('write_authn_valid'),
        'write_authorization_valid':boolean('write_authorization_valid'),
        'write_auth_valid':boolean('write_auth_valid'),
        'diagnostic_code':code('diagnostic_code'),
        'write_probe_diagnostic_code':code('write_probe_diagnostic_code'),
        'tls_verification_failed':value.get('diagnostic_code') == 'tls_verification_failed'}}
try:
    connection = http.client.HTTPSConnection('server',context=ssl.create_default_context(),timeout=12)
    cookies = {{}}
    def request(method,path,body,expected=(200,201)):
        global http_status
        headers = {{'Content-Type':'application/json','Cookie':'; '.join(k+'='+v for k,v in cookies.items())}}
        csrf = [value for key,value in cookies.items() if 'csrf' in key]
        if csrf:
            headers['X-CSRF-Token'] = csrf[0]
        connection.request(method,path,json.dumps(body) if body is not None else None,headers)
        response = connection.getresponse()
        http_status = response.status
        for name,value in response.getheaders():
            if name.lower() == 'set-cookie':
                parsed = SimpleCookie()
                parsed.load(value)
                cookies.update({{k:v.value for k,v in parsed.items()}})
        data = response.read(1024 * 1024)
        try:
            result = json.loads(data)
        except ValueError:
            result = None
        if path.endswith('/test-credentials'):
            probe_results.append(probe_projection(result))
        assert response.status in expected, 'synthetic admin API request failed'
        assert result is not None, 'synthetic admin API JSON unavailable'
        assert b'PRIVATE KEY' not in data, 'API exposed private key'
        return result
    password = secrets.token_hex(24)
    request('POST','/api/auth/local/register',{{'username':'niks3-fixture-admin','email':'niks3-fixture@example.invalid','password':password}})
    stage = 'login'
    request('POST','/api/auth/local/login',{{'username':'niks3-fixture-admin','password':password}})
    pki = pathlib.Path('/etc/niks3-production-pki')
    bundle = (pki/'write-ca-bundle-onboarding.pem').read_text()
    assert hashlib.sha256(bundle.encode()).hexdigest() == {expected_raw_digest!r}
    assert bundle.count('-----BEGIN CERTIFICATE-----') == 124 and 'PRIVATE KEY' not in bundle
    assert bundle.count('-----BEGIN TRUSTED CERTIFICATE-----') == 172
    path = '/api/v1/caches/{cache_id}'
    stage = 'save'
    result = request('PUT',path,{{'niks3_write_ca_cert':bundle}})
    stage = 'get'
    result = request('GET',path,None)
    assert result['id'] == {cache_id} and result['niks3_server_url'] == 'https://push-cache.test:5754'
    assert result['niks3_write_mtls_configured'] and result['niks3_read_basic_configured']
    assert not result['niks3_write_token_configured']
    assert not any(result.get(field) for field in ('niks3_write_client_key','niks3_read_client_key','niks3_read_basic_password','niks3_auth_token','s3_secret_access_key'))
    canonical = result['niks3_write_ca_cert']
    assert canonical.count('-----BEGIN CERTIFICATE-----') == 124
    assert 'TRUSTED CERTIFICATE' not in canonical and 'PRIVATE KEY' not in canonical
    assert hashlib.sha256(canonical.encode()).hexdigest() == {expected_ca_digest!r}
    def raw_state():
        query = "SELECT jsonb_build_object('destination',to_jsonb(c),'scope',(SELECT jsonb_agg(to_jsonb(e) ORDER BY environment_id) FROM cache_destination_environments e WHERE cache_destination_id=c.id)) FROM cache_destinations c WHERE id={cache_id}"
        result = subprocess.run(['runuser','-u','postgres','--','psql','-XAt','-d','crystal_forge','-c',query],capture_output=True,timeout=8)
        assert result.returncode == 0, 'private probe state unavailable'
        return result.stdout
    baseline = raw_state()
    stage = 'invalid_ca'
    # Only generated VM fixture material enters these deliberately invalid CA
    # requests. Input and response bytes stay private; failures cannot persist.
    fixture_key = (pki/'write-client-1.key').read_text()
    for invalid in (bundle+fixture_key, bundle+'\\n-----BEGIN CERTIFICATE-----\\ninvalid\\n'):
        request('POST',path+'/test-credentials',{{'probe_scope':'write','niks3_write_ca_cert':invalid}},expected=(400,))
        request('PUT',path,{{'niks3_write_ca_cert':invalid}},expected=(400,))
        assert raw_state() == baseline, 'invalid CA request mutated raw destination or scope'
    stage = 'mismatched_identity'
    request('POST',path+'/test-credentials',{{'probe_scope':'write','niks3_write_client_key':(pki/'write-client-2.key').read_text()}},expected=(400,))
    assert raw_state() == baseline, 'mismatched client identity Test mutated raw destination or scope'
    stage = 'ambient_trust'
    # Measure the genuine ingress omission response without an identity or
    # token. Exact body matching stays private; no header value is exported.
    omitted = http.client.HTTPSConnection('push-cache.test',5754,
        context=ssl.create_default_context(cafile=str(pki/'server-ca-a.crt')),timeout=8)
    try:
        omitted.request('GET','/api/gc/status')
        response = omitted.getresponse()
        status = response.status
        media = response.getheader('Content-Type')
        body = response.read(65537)
        nginx_head = (b'<html>\\r\\n<head><title>400 No required SSL certificate was sent</title></head>\\r\\n'
            b'<body>\\r\\n<center><h1>400 Bad Request</h1></center>\\r\\n'
            b'<center>No required SSL certificate was sent</center>\\r\\n')
        nginx_tail = rb'<hr><center>nginx(?:/[0-9]{{1,3}}\\.[0-9]{{1,3}}\\.[0-9]{{1,3}})?</center>\\r\\n</body>\\r\\n</html>\\r\\n'
        fingerprint = 'other'
        if status == 401 and media == 'text/plain; charset=utf-8' and body == b'Unauthorized\\n':
            fingerprint = 'native401Unauthorized'
        elif status == 400 and media == 'text/html' and body.startswith(nginx_head) and re.fullmatch(nginx_tail,body[len(nginx_head):]):
            fingerprint = 'nginx400certrequired'
        omission_projection = {{'status':status,'mime_class':'html' if media == 'text/html' else 'plain' if media == 'text/plain; charset=utf-8' else 'other',
            'fingerprint':fingerprint}}
    finally:
        omitted.close()
    ambient = request('POST',path+'/test-credentials',{{'probe_scope':'write','clear_niks3_write_ca_cert':True}})
    assert ambient['ok'] is True and ambient['write_api_reachable'] is True and ambient['discovery_valid'] is True
    assert ambient['write_auth_valid'] is True and ambient['write_authn_valid'] is True and ambient['write_authorization_valid'] is True
    assert ambient['probe_scope'] == 'write' and ambient['status_code'] == 400
    assert ambient['write_probe_diagnostic_code'] == 'write_route_verified_certificate_gate'
    assert raw_state() == baseline, 'ambient trust Test mutated raw destination or scope'
    stage = 'custom_trust_replacement'
    replacement = request('POST',path+'/test-credentials',{{'probe_scope':'write','niks3_write_ca_cert':(pki/'server-ca-b.crt').read_text()}})
    # Keep finite response fields for the one success or failure JSON record.
    # Provider prose, URLs, PEM, request bodies and exceptions stay private.
    code = replacement.get('diagnostic_code')
    reachable = replacement.get('write_api_reachable')
    replacement_projection = {{'ok':replacement.get('ok') if type(replacement.get('ok')) is bool else False,
        'write_api_reachable':reachable if type(reachable) is bool else None,
        'diagnostic_code':code if type(code) is str and code in allowed_codes else 'unrecognized',
        'returned_http_status':http_status if type(http_status) is int and 100 <= http_status <= 599 else None}}
    assert replacement['ok'] is False and replacement['diagnostic_code'] == 'tls_verification_failed'
    assert replacement['write_auth_valid'] is None and replacement['write_authn_valid'] is None and replacement['write_authorization_valid'] is None
    assert raw_state() == baseline, 'explicit B-only Test mutated raw destination or scope'
    stage = 'write_metadata'
    probe = request('POST',path+'/test-credentials',{{'probe_scope':'write'}})
    assert probe['ok'] is True and probe['write_api_reachable'] is True and probe['discovery_valid'] is True
    assert probe['write_auth_valid'] is True and probe['write_authorization_valid'] is True and probe['write_authn_valid'] is True
    assert probe['probe_scope'] == 'write' and probe['status_code'] == 400
    assert probe['write_probe_diagnostic_code'] == 'write_route_verified_certificate_gate'
    assert raw_state() == baseline, 'protected write Test mutated raw destination or scope'
    stage = 'all_planes'
    combined = request('POST',path+'/test-credentials',{{'probe_scope':'all'}})
    assert combined['ok'] is True and combined['probe_scope'] == 'all'
    assert combined['write_api_reachable'] is True and combined['discovery_valid'] is True
    assert combined['write_authn_valid'] is True and combined['write_authorization_valid'] is True and combined['write_auth_valid'] is True
    assert combined['read_access_valid'] is True and combined['signing_keys_valid'] is True
    assert combined['write_probe_diagnostic_code'] == 'write_route_verified_certificate_gate'
    assert raw_state() == baseline, 'All Test mutated raw destination or scope'
    stage = 'denied_write_identity'
    # The existing C-issued wrong-subject identity passes ingress client trust.
    # Public metadata remains reachable, but native bound-subject auth must fail.
    denied = request('POST',path+'/test-credentials',{{'probe_scope':'write',
        'niks3_write_client_cert':(pki/'wrong-subject.crt').read_text(),
        'niks3_write_client_key':(pki/'wrong-subject.key').read_text()}})
    assert denied['ok'] is False and denied['write_api_reachable'] is True and denied['discovery_valid'] is True
    assert denied['write_authn_valid'] is False and denied['write_authorization_valid'] is False and denied['write_auth_valid'] is False
    assert denied['probe_scope'] == 'write' and denied['status_code'] == 401
    assert denied['write_probe_diagnostic_code'] == 'mtls_subject_rejected'
    assert raw_state() == baseline, 'denied identity Test mutated raw destination or scope'
    print(json.dumps({{'saved_ca_sha256':hashlib.sha256(canonical.encode()).hexdigest(),
                      'raw_ordinary_roots':124,'raw_trusted_blocks':172,'saved_ordinary_roots':124,
                      'raw_annotation_lines':605,'trusted_blocks_ignored':172,'trusted_roots_promoted':0,
                      'ambient_metadata_valid':True,'b_only_replaces_ambient':True,
                      'replacement_diagnostic':'tls_verification_failed','saved_metadata_valid':True,
                      'invalid_ca_rejected':True,'mismatched_identity_rejected':True,
                      'retained_flags':True,'redacted':True,'protected_handler_access_only':True,
                      'all_planes_valid':True,'denied_write_identity':True,'raw_state_unchanged':True,
                      'probe_calls':len(probe_results),
                      'trust_negative':replacement_projection,'probe_results':probe_results,
                      'omission_control':omission_projection}}))
except Exception as error:
    # Never let an HTTP/TLS error or response-body traceback export private data.
    print(json.dumps({{'saved_ca_api_proof_failed':True,'stage':stage,'http_status':http_status,
        'tls_verification_failed':isinstance(error,ssl.SSLCertVerificationError),
        'trust_negative':replacement_projection,'probe_results':probe_results,
        'omission_control':omission_projection}}))
    raise SystemExit(1)
"""
    return json.loads(server.succeed("timeout 60 python3 -c " + shlex.quote(source), timeout=65))


def split_object_boundary(production, target, before):
    """Attests fresh objects and one logical multipart publication privately.

    Signed HEAD uses backend-only fixture credentials and never mutates Garage.
    Private object paths exclude queries. Only hashes, counts and booleans leave
    the guest. The pre-dispatch baseline excludes earlier variants and inputs.
    Successful part PUTs are HTTP requests, not logical upload counts. The
    trusted native provider completion must identify one upload for this root.
    """
    keys = [target['nar_key'], Path(target['out']).name.split('-', 1)[0] + '.narinfo']
    source = inspect.getsource(private_journal_message) + f"""
import hashlib,hmac,http.client,json,pathlib,re,stat,subprocess
from datetime import datetime,timezone
try:
    directory = pathlib.Path('/run/niks3-publication-observer')
    observer = directory/'objects.json'
    state_path = directory/'boundary.json'
    assert stat.S_IMODE(directory.stat().st_mode) == 0o700 and stat.S_IMODE(observer.stat().st_mode) == 0o600
    records = [json.loads(line) for line in observer.read_text().splitlines()]
    keys = {keys!r}
    journal = subprocess.run(['journalctl','-b','-u','niks3-production.service','--no-pager','-o','json'],
        stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,timeout=5,check=True).stdout
    assert len(journal) <= 16 * 1024 * 1024, 'provider journal exceeds private capture bound'
    provider = [json.loads(line) for line in journal.splitlines()]
    assert provider and len({{record['_BOOT_ID'] for record in provider}}) == 1
    boot = provider[0]['_BOOT_ID']
    def head(key):
        secrets = pathlib.Path('/run/niks3-production-secrets')
        access = (secrets/'access-key').read_text().strip()
        secret = (secrets/'secret-key').read_text().strip()
        now = datetime.now(timezone.utc)
        stamp,day = now.strftime('%Y%m%dT%H%M%SZ'),now.strftime('%Y%m%d')
        path = '/production-niks3/'+key
        payload = hashlib.sha256(b'').hexdigest()
        canonical = 'HEAD\\n'+path+'\\n\\nhost:127.0.0.1:3900\\nx-amz-content-sha256:'+payload+'\\nx-amz-date:'+stamp+'\\n\\nhost;x-amz-content-sha256;x-amz-date\\n'+payload
        scope = day+'/garage/s3/aws4_request'
        signing = ('AWS4'+secret).encode()
        for part in (day,'garage','s3','aws4_request'):
            signing = hmac.new(signing,part.encode(),hashlib.sha256).digest()
        signature = hmac.new(signing,('AWS4-HMAC-SHA256\\n'+stamp+'\\n'+scope+'\\n'+hashlib.sha256(canonical.encode()).hexdigest()).encode(),hashlib.sha256).hexdigest()
        connection = http.client.HTTPConnection('127.0.0.1',3900,timeout=8)
        connection.request('HEAD',path,headers={{'X-Amz-Date':stamp,'X-Amz-Content-Sha256':payload,
            'Authorization':'AWS4-HMAC-SHA256 Credential='+access+'/'+scope+', SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature='+signature}})
        response = connection.getresponse()
        status = response.status
        response.read()
        connection.close()
        return status
    if {before!r}:
        assert all(head(key) == 404 for key in keys), 'fresh NAR or narinfo already exists'
        state_path.write_text(json.dumps({{'keys':keys,'baseline':len(records),
            'puts':sum(record['put'] for record in records),
            'provider_baseline':len(provider),'provider_boot':boot,
            'provider_cursor':provider[-1]['__CURSOR']}}))
        state_path.chmod(0o600)
        print(json.dumps({{'absent_nar':True,'absent_narinfo':True,'put_baseline':sum(record['put'] for record in records)}}))
    else:
        state = json.loads(state_path.read_text())
        assert state['keys'] == keys and len(records) >= state['baseline']
        assert state['provider_boot'] == boot and 0 < state['provider_baseline'] <= len(provider)
        assert provider[state['provider_baseline']-1]['__CURSOR'] == state['provider_cursor']
        delta = records[state['baseline']:]
        counts = []
        for key in keys:
            matching = [record for record in delta if record['path'] == '/production-niks3/'+key and record['put'] and not record['backend'] and 200 <= record['status'] < 300]
            assert matching and head(key) == 200, 'fresh object missing matching successful direct PUT'
            counts.append(len(matching))
        uploads = []
        # Niks3 1.6.0 emits this exact Go slog record after native completion.
        # The root content-hash key and opaque upload IDs remain private inputs.
        pattern = (r'[0-9]{{4}}/[0-9]{{2}}/[0-9]{{2}} [0-9]{{2}}:[0-9]{{2}}:[0-9]{{2}} '
            r'INFO Completed multipart upload object_key='+re.escape(keys[0])+
            r' upload_id=([0-9a-f]{{64}}) parts=([1-9][0-9]*)')
        for record in provider[state['provider_baseline']:]:
            message = private_journal_message(record)
            if 'Completed multipart upload' not in message or 'object_key='+keys[0]+' ' not in message:
                continue
            assert record.get('_SYSTEMD_UNIT') == 'niks3-production.service'
            match = re.fullmatch(pattern,message)
            assert match is not None, 'invalid native root multipart completion record'
            uploads.append((match[1],int(match[2])))
        assert len(uploads) == len({{identity for identity,parts in uploads}}) == 1, 'root has zero or repeated logical multipart uploads'
        parts = uploads[0][1]
        # The fresh fixture carries 16 MiB of incompressible payload. Native
        # Niks3 splits this NAR into two parts; each part has one successful PUT.
        assert parts == counts[0] == 2 and counts[1] == 1, 'multipart part or narinfo PUT count mismatch'
        print(json.dumps({{'nar_puts':counts[0],'narinfo_puts':counts[1],
            'logical_nar_uploads':len(uploads),'unique_upload_ids':1,'multipart_parts':parts,
            'new_puts':sum(record['put'] for record in delta),
            'object_path_sha256':[hashlib.sha256(key.encode()).hexdigest() for key in keys]}}))
except Exception:
    print(json.dumps({{'object_boundary_failed':True}}))
    raise SystemExit(1)
"""
    return json.loads(production.succeed("timeout 30 python3 -c " + shlex.quote(source), timeout=35))


def completion_rate_limit(production, reader, server_address, target, enabled):
    """Routes only the server's exact root metadata through a VM-local 429 sink.

    The normal read location still enforces TLS and Basic authentication before
    proxying. The same fixed map include remains present through both reloads.
    NixOS ExecReload validates the actual store-backed config with its configured
    nginx binary before HUP. An authenticated request from the server then proves
    the new workers serve the selected status, rather than an old map.
    No provider object, certificate, credential or guard fixture changes.
    """
    narinfo = "/" + Path(target["out"]).name.split("-", 1)[0] + ".narinfo"
    production.succeed("python3 -c " + shlex.quote(f"""
import ipaddress,json,pathlib,re,subprocess
assert str(ipaddress.ip_address({server_address!r})) == {server_address!r}
assert re.fullmatch(r'/[0123456789abcdfghijklmnpqrsvwxyz]{{32}}\\.narinfo', {narinfo!r})
path = pathlib.Path('/run/niks3-publication-observer/metadata-map.conf')
text = json.dumps({server_address!r}+':'+{narinfo!r})+' "http://127.0.0.1:5758";\\n' if {enabled!r} else ''
path.write_text(text)
path.chmod(0o600)
result = subprocess.run(['systemctl','reload','nginx'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=10)
assert result.returncode == 0, 'completion metadata map reload failed'
print(json.dumps({{'selected_root_rate_limit':{enabled!r},'authenticated_proxy_preserved':True}}))
"""))
    reader.succeed("timeout 12 python3 -c " + shlex.quote(f"""
import base64,http.client,json,pathlib,ssl,time
try:
    pki = pathlib.Path('/etc/niks3-production-pki')
    authorization = 'Basic '+base64.b64encode((pki/'basic-username').read_bytes()+b':'+(pki/'basic-password').read_bytes()).decode()
    context = ssl.create_default_context()
    def status(authenticated):
        connection = http.client.HTTPSConnection('read-cache.test',5753,context=context,timeout=2)
        try:
            connection.request('GET',{narinfo!r},headers={{'Authorization':authorization}} if authenticated else {{}})
            response = connection.getresponse()
            result = response.status
            response.read(65537)
            return result
        finally:
            connection.close()
    if {enabled!r}:
        assert status(False) == 403, 'rate-limit routing bypassed Basic authentication'
    expected = 429 if {enabled!r} else 200
    deadline = time.monotonic()+8
    while status(True) != expected:
        assert time.monotonic() < deadline, 'selected-root metadata map not active'
        time.sleep(0.1)
    print(json.dumps({{'selected_root_status':expected,'basic_unauthenticated_status':403 if {enabled!r} else None}}))
except Exception:
    print(json.dumps({{'selected_root_status_failed':True}}))
    raise SystemExit(1)
"""), timeout=15)


def completion_receipt(server, builder_id, session_id, job_id, cache_id, target, cache_name, pending):
    """Asserts real signed same-claim JSON receipts without exporting auth bytes.

    The disposable guest loads the public fixture signing key, signs a fresh
    timestamp and keeps the complete response private. This test-owned replay
    is distinguished from the packaged builder's automatic callback retries.
    """
    source = f"""
import base64,http.client,json,pathlib,ssl,subprocess,tempfile
from datetime import datetime,timezone
try:
    path = '/api/v1/builders/{builder_id}/jobs/{job_id}/complete'
    body = json.dumps({{'output_path':{target['out']!r},'cache_pushed':True,
        'cache_destination_id':{cache_id!r},'cache_reference':{cache_name!r}}},separators=(',',':'))
    timestamp = datetime.now(timezone.utc).isoformat()
    raw = base64.b64decode(pathlib.Path('/etc/agent.key').read_bytes(),validate=False)
    der = bytes.fromhex('302e020100300506032b657004220420')+raw
    with tempfile.TemporaryDirectory(prefix='niks3-test-receipt-') as directory:
        directory = pathlib.Path(directory)
        key, message = directory/'key.der', directory/'message'
        key.write_bytes(der)
        message.write_bytes(('POST\\n'+path+'\\n'+timestamp+'\\n'+body).encode())
        key.chmod(0o600)
        message.chmod(0o600)
        signed = subprocess.run(['openssl','pkeyutl','-sign','-rawin','-keyform','DER',
            '-inkey',str(key),'-in',str(message)],capture_output=True,timeout=5)
        assert signed.returncode == 0 and len(signed.stdout) == 64
    connection = http.client.HTTPSConnection('server',context=ssl.create_default_context(),timeout=90)
    connection.request('POST',path,body,{{'Content-Type':'application/json',
        'X-Builder-ID':{str(builder_id)!r},'X-Builder-Session-ID':{str(session_id)!r},
        'X-Timestamp':timestamp,'X-Signature':base64.b64encode(signed.stdout).decode(),
        'X-CF-Test-Receipt':'true'}})
    response = connection.getresponse()
    data = json.loads(response.read(65537))
    expected = ({{'build_status':'success','publication_status':'pending',
        'verification_code':'cache_rate_limited','retryable':True,'retry_after_secs':2}}
        if {pending!r} else {{'build_status':'success','publication_status':'confirmed'}})
    assert response.status == (202 if {pending!r} else 200) and data == expected
    connection.close()
    print(json.dumps({{'http_status':response.status,**expected}}))
except Exception:
    print(json.dumps({{'completion_receipt_failed':True}}))
    raise SystemExit(1)
"""
    return json.loads(server.succeed("timeout 100 python3 -c " + shlex.quote(source), timeout=105))


def completion_callbacks(server, builder_id, job_id):
    """Returns safe real callback intervals for one exact job, never other URLs."""
    return json.loads(server.succeed("python3 -c " + shlex.quote(f"""
import json,pathlib,stat
path = pathlib.Path('/run/niks3-completion-observer/callbacks.json')
assert stat.S_IMODE(path.parent.stat().st_mode) == 0o700 and stat.S_IMODE(path.stat().st_mode) == 0o600
records = [json.loads(line) for line in path.read_text().splitlines()]
selected = [record for record in records if record['path'] == '/api/v1/builders/{builder_id}/jobs/{job_id}/complete']
assert all(record['post'] and set(record) == {{'path','post','driver','status','end','duration'}} for record in selected)
print(json.dumps([{{key:record[key] for key in ('driver','status','end','duration')}} for record in selected]))
""")))


def selected_build_start_proof(builder, job_id, derivation_id):
    """Requires one packaged build-engine start for the exact selected job.

    Current-boot journal messages and process context remain private. Only the
    number of trusted builder-module starts leaves the disposable guest.
    """
    source = inspect.getsource(private_journal_message) + f"""
import json,re,subprocess
try:
    journal = subprocess.run(['journalctl','-b','-u','crystal-forge-builder.service',
        '--no-pager','-o','json'],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,timeout=5,check=True).stdout
    assert len(journal) <= 16 * 1024 * 1024
    pattern = (r'[0-9T:.Z+-]+\\s+INFO\\s+builder:\\s+🔨 Starting build for job #'
        +re.escape({str(job_id)!r})+r' \\(derivation: {derivation_id}\\)')
    starts = 0
    for line in journal.splitlines():
        record = json.loads(line)
        message = re.sub(r'\\x1b\\[[0-9;]*m','',private_journal_message(record))
        if re.fullmatch(pattern,message) is not None:
            assert record.get('_SYSTEMD_UNIT') == 'crystal-forge-builder.service'
            starts += 1
    assert starts == 1, 'selected job rebuilt or build-engine start missing'
    print(json.dumps({{'selected_job_build_starts':starts}}))
except Exception:
    print(json.dumps({{'selected_build_start_proof_failed':True}}))
    raise SystemExit(1)
"""
    return json.loads(builder.succeed("timeout 8 python3 -c " + shlex.quote(source), timeout=10))


def completion_metadata_boundary(production, target, callbacks, before=False):
    """Attests root-only server metadata reads through confirmed receipts.

    nginx records path and timing privately. Only counts leave the guest. The
    window excludes subsequent explicit CVE and independent signature-copy
    phases. A private record offset avoids cross-guest clock assumptions. The
    callback observer separately proves the actual POST intervals and statuses.
    This proves native gateway behavior, not the CLI argument shape;
    the server's command-contract tests protect the absence of recursive flags.
    """
    source = f"""
import json,pathlib,stat
stage = 'observer_permissions'
counts = {{'total_read_records':0,'window_read_records':0,'server_read_records':0,
    'server_get_records':0,'server_head_records':0}}
try:
    path = pathlib.Path('/run/niks3-publication-observer/reads.json')
    assert stat.S_IMODE(path.parent.stat().st_mode) == 0o700 and stat.S_IMODE(path.stat().st_mode) == 0o600
    stage = 'observer_decode'
    records = [json.loads(line) for line in path.read_text().splitlines()]
    counts['total_read_records'] = len(records)
    root = '/'+pathlib.Path({target['out']!r}).name.split('-',1)[0]+'.narinfo'
    boundary = path.parent/'read-boundary.json'
    if {before!r}:
        boundary.write_text(json.dumps({{'root':root,'offset':len(records)}}))
        boundary.chmod(0o600)
        print(json.dumps({{'completion_read_baseline':True}}))
        raise SystemExit(0)
    stage = 'baseline_identity'
    state = json.loads(boundary.read_text())
    assert state['root'] == root and 0 <= state['offset'] <= len(records)
    counts['window_read_records'] = len(records)-state['offset']
    reads = [record for record in records[state['offset']:] if record['server']]
    counts['server_read_records'] = len(reads)
    counts['server_get_records'] = sum(record['get'] for record in reads)
    counts['server_head_records'] = sum(record['head'] for record in reads)
    # Native Nix can HEAD metadata before fetching it. Method acceptance does
    # not authorize any NAR or reference path; the exact path boundary follows.
    stage = 'server_metadata_methods'
    assert reads and all(record['get'] or record['head'] for record in reads)
    roots = [record for record in reads if record['path'] == root]
    stage = 'root_status'
    assert roots and any(record['status'] == 429 for record in roots) and any(record['status'] == 200 for record in roots)
    # A refreshed isolated metadata cache must not walk root references. NAR
    # imports are verified later by the unchanged native security fixtures.
    stage = 'root_only'
    assert all(record['path'] in (root,'/nix-cache-info') for record in reads), 'completion downloaded NAR or traversed references'
    print(json.dumps({{**counts,'root_metadata_requests':len(roots),
        'root_metadata_gets':sum(record['get'] for record in roots),
        'root_metadata_heads':sum(record['head'] for record in roots),
        'rate_limited_root_requests':sum(record['status'] == 429 for record in roots),
        'available_root_requests':sum(record['status'] == 200 for record in roots),
        'nar_downloads':0,'reference_metadata_gets':0,'completion_intervals':{len(callbacks)}}}))
except Exception:
    print(json.dumps({{'completion_metadata_boundary_failed':True,'stage':stage,**counts}}))
    raise SystemExit(1)
"""
    return json.loads(production.succeed("python3 -c " + shlex.quote(source)))


def publication_stage_projection(logs, cache_id, expected_name):
    """Selects bounded availability display events without returning raw logs.

    The expected name comes from the configured fixture. Display text does not
    prove destination identity. The cache_id field is caller context only;
    persisted dispatch/publication rows and exact-job callbacks prove identity
    separately. Only terminal events enter the existing evidence shape.
    """
    import re

    assert isinstance(logs, str) and len(logs.encode()) <= 32 * 1024 * 1024
    assert re.fullmatch(r"[A-Za-z0-9_-]+", expected_name), "invalid fixture display name"
    name = re.escape(expected_name)
    start = f"🔎 Checking output availability in {expected_name}..."
    success = rf"✅ Output confirmed in {name} \(([0-9]+) ms\)"
    failure = rf"⚠️ Cache availability check failed for {name} \(Niks3\): rate limited \(([0-9]+) ms\)"
    events = []
    checking = False
    # Split only on the record delimiter so CR/VT and other injected controls
    # cannot disappear through splitlines normalization.
    for line in logs.split("\n"):
        if not any(marker in line for marker in ("Checking output availability",
                   "Output confirmed", "Cache availability check failed", "cache_publication")):
            continue
        if line == start:
            assert not checking, "duplicate completion stage start"
            checking = True
            continue
        match = re.fullmatch(success, line)
        outcome, status = "success", "confirmed"
        if match is None:
            match = re.fullmatch(failure, line)
            outcome, status = "cache_rate_limited", "pending"
        assert match is not None, "invalid completion stage projection"
        assert checking, "completion stage start missing"
        elapsed = match.group(1)
        assert int(elapsed) <= 90000, "completion stage duration exceeded"
        checking = False
        events.append({"cache_id": cache_id, "cache_type": "Niks3", "stage": "availability_path_info",
                       "elapsed_ms": int(elapsed), "outcome": outcome, "publication_status": status})
    assert not checking, "completion stage result missing"
    assert events and len(events) <= 7, "completion stage evidence missing or retry bound exceeded"
    return events


def test_completion_guest_scripts_compile():
    """Compiles generated guest code; this check does not attest VM behavior."""
    class SyntaxGuest:
        def __init__(self):
            self.scripts = []

        def succeed(self, command, **kwargs):
            arguments = shlex.split(command)
            source = arguments[arguments.index("-c") + 1]
            compile(source, "<completion-guest>", "exec")
            self.scripts.append(source)
            return "{}"

    guest = SyntaxGuest()
    target = {"out": "/nix/store/0123456789abcdfghijklmnpqrsvwxyz0-completion", "nar_key": "nar/fixture.nar.zst"}
    identity = "00000000-0000-4000-8000-000000000470"
    for pending in (True, False):
        completion_rate_limit(guest, guest, "192.0.2.10", target, pending)
        completion_receipt(guest, identity, identity, identity, 470, target, "fixture", pending)
    completion_callbacks(guest, identity, identity)
    completion_metadata_boundary(guest, target, [], before=True)
    completion_metadata_boundary(guest, target, [{"end": 42.0, "duration": 1.0, "status": 200, "driver": False}])
    split_object_boundary(guest, target, before=True)
    split_object_boundary(guest, target, before=False)
    selected_build_start_proof(guest, identity, 470)
    assert len(guest.scripts) == 12


def test_publication_stage_projection_rejects_untrusted_fields():
    """Rejects wrong names, unsafe display fields, durations and retry overflow."""
    name = "z-published-mtls-split"
    start = f"🔎 Checking output availability in {name}...\n"
    pending = f"⚠️ Cache availability check failed for {name} (Niks3): rate limited (12 ms)\n"
    confirmed = f"✅ Output confirmed in {name} (0 ms)\n"
    logs = "PRIVATE unrelated log\n" + start + pending + start + confirmed
    events = publication_stage_projection(logs, 470, name)
    assert events == [
        {"cache_id": 470, "cache_type": "Niks3", "stage": "availability_path_info",
         "elapsed_ms": 12, "outcome": "cache_rate_limited", "publication_status": "pending"},
        {"cache_id": 470, "cache_type": "Niks3", "stage": "availability_path_info",
         "elapsed_ms": 0, "outcome": "success", "publication_status": "confirmed"}]
    assert "PRIVATE" not in json.dumps(events)
    assert len(publication_stage_projection((start + pending) * 7, 470, name)) == 7
    assert publication_stage_projection(start + confirmed.replace("0 ms", "90000 ms"), 470, name)[0]["elapsed_ms"] == 90000
    for invalid in (logs.replace(name, "unexpected-cache"),
                    logs.replace(name, "https://user:PRIVATE@cache.invalid"),
                    start + pending.rstrip("\n") + " url=PRIVATE\n",
                    start + confirmed.replace(name, name + " (Niks3)"),
                    start + pending.replace("rate limited", "PRIVATE provider error"),
                    start + pending.replace("(Niks3)", "(Attic)"),
                    start + pending.replace("12 ms", "90001 ms"),
                    start + pending.replace("12 ms", "-1 ms"),
                    start + pending.replace("12 ms", "1.2 ms"),
                    start + pending.replace("12 ms", "١٢ ms"),
                    start + pending.replace("12 ms", "12ms"),
                    start + pending.replace(name, name + "\x1b[0m"),
                    start + pending.replace("\n", "\r\n"),
                    start + pending.replace(name, name + "\x00"),
                    start + pending.replace(name, name + "\v"),
                    pending, start, start + start + pending, start + pending + confirmed,
                    (start + pending) * 8, "PRIVATE",
                    "cache_publication cache_id=470 cache_type=Niks3 PRIVATE"):
        try:
            publication_stage_projection(invalid, 470, name)
        except AssertionError as error:
            assert "PRIVATE" not in str(error)
        else:
            raise AssertionError("untrusted completion stage accepted")


def negative_tls_hint(builder, cursor):
    """Requires the recognized static hint without exporting child diagnostics."""
    source = f"""
import json,subprocess
try:
    result = subprocess.run(['journalctl','-u','crystal-forge-builder','--after-cursor={cursor}','--no-pager','-o','json'],capture_output=True,timeout=8)
    assert result.returncode == 0
    messages = [record.get('MESSAGE') for record in map(json.loads,result.stdout.splitlines())]
    messages = [message if isinstance(message,str) else bytes(message).decode('utf-8',errors='replace') if isinstance(message,list) else '' for message in messages]
    hint = 'Niks3 diagnostic hint: check certificate trust, validity, and hostname coverage for both the API and S3 endpoints.'
    assert any(hint in message for message in messages), 'recognized TLS hint absent'
    assert not any(marker in message for marker in ('X-Amz-Signature','X-Amz-Credential','-----BEGIN PRIVATE KEY-----') for message in messages), 'unsafe child diagnostics escaped'
    print(json.dumps({{'tls_hint':True,'private_diagnostics_suppressed':True}}))
except Exception:
    print(json.dumps({{'tls_hint_proof_failed':True}}))
    raise SystemExit(1)
"""
    return json.loads(builder.succeed("timeout 12 python3 -c " + shlex.quote(source), timeout=15))


def basic_read_narinfo(machine, target):
    """Fetches public signed metadata with file-loaded read-only credentials."""
    return machine.succeed("python3 -c " + shlex.quote(f"""
import base64,http.client,pathlib,ssl
pki = pathlib.Path('/etc/niks3-production-pki')
authorization = base64.b64encode((pki/'basic-username').read_bytes()+b':'+(pki/'basic-password').read_bytes()).decode()
connection = http.client.HTTPSConnection('read-cache.test',5753,context=ssl.create_default_context(),timeout=8)
connection.request('GET','/{Path(target['out']).name.split('-', 1)[0]}.narinfo',headers={{'Authorization':'Basic '+authorization}})
response = connection.getresponse()
assert response.status == 200, 'authenticated read metadata failed'
data = response.read().decode()
assert authorization not in data and (pki/'basic-password').read_text() not in data
# NarInfo's parser requires a field delimiter on every line. Do not append a
# blank line to the provider's already newline-terminated metadata.
print(data,end='')
connection.close()
"""))


def run_basic_read_guard(production, reader, target, signing_keys, narinfo):
    """Exercises native guarded transfers and fresh signature-required closures.

    Eight negative operations use actual packaged Nix, not a Rust callback.
    Separate target logs prove rejection before a transfer, including absolute
    NAR URLs. The ordinary unguarded control still follows a same-origin redirect.
    All child output stays private and every child is reaped before netrc removal.
    """
    nar_hash = Path(target["out"]).name.split("-", 1)[0]
    closure = reader.succeed("nix-store --query --requisites " + shlex.quote(target["out"])).splitlines()
    assert target["out"] in closure and all(path.startswith("/nix/store/") for path in closure)
    for name, authority in (("host", "read-foreign.test:5756"), ("port", "read-cache.test:5756")):
        import re
        altered, replacements = re.subn(r"(?m)^URL: .*", f"URL: https://{authority}/guard-target/payload.nar", narinfo)
        assert replacements == 1, "foreign NAR fixture requires exactly one URL field"
        # INVARIANT: URL is not part of Nix's signature fingerprint. Preserve
        # StorePath, NarHash, NarSize, References and every original signature.
        assert [line for line in altered.splitlines() if not line.startswith("URL:")] == [
            line for line in narinfo.splitlines() if not line.startswith("URL:")]
        production.succeed("python3 -c " + shlex.quote(f"""
import base64,http.client,pathlib,re,ssl,urllib.parse
directory = pathlib.Path('/run/niks3-basic-guard/{name}')
directory.mkdir(parents=True,exist_ok=True)
directory.chmod(0o755)
(directory/'nix-cache-info').write_text('StoreDir: /nix/store\\nWantMassQuery: 1\\nPriority: 40\\n')
pki = pathlib.Path('/etc/niks3-production-pki')
authorization = base64.b64encode((pki/'basic-username').read_bytes()+b':'+(pki/'basic-password').read_bytes()).decode()
connection = http.client.HTTPSConnection('read-cache.test',5753,context=ssl.create_default_context(),timeout=8)
# Populate every reference's genuine signed metadata. A missing dependency
# must not masquerade as a successful foreign-NAR transport rejection.
for store_path in {closure!r}:
    hash_part = pathlib.Path(store_path).name.split('-',1)[0]
    connection.request('GET','/'+hash_part+'.narinfo',headers={{'Authorization':'Basic '+authorization}})
    response = connection.getresponse()
    assert response.status == 200, 'closure metadata unavailable'
    data = response.read().decode()
    assert authorization not in data and (pki/'basic-password').read_text() not in data
    # The prefixed synthetic metadata cache has no NAR files of its own.
    # Resolve reference NARs against the real read origin, not this prefix.
    data,count = re.subn(r'(?m)^URL: (.*)$',lambda match:'URL: '+urllib.parse.urljoin('https://read-cache.test:5753/',match.group(1)),data)
    assert count == 1, 'closure metadata URL missing'
    (directory/(hash_part+'.narinfo')).write_text(data)
connection.close()
(directory/{(nar_hash + '.narinfo')!r}).write_text({altered!r})
"""))
    before = len(production_observations(production, "target"))
    source = f"""
import json,pathlib,shutil,stat,subprocess,tempfile
pki = pathlib.Path('/etc/niks3-production-pki')
nix = shutil.which('nix')
probe = subprocess.run([nix,'config','show','--json'],capture_output=True,timeout=10,check=True)
assert isinstance(json.loads(probe.stdout)['cf-netrc-authority']['value'],str), 'native guard missing'
username,password = (pki/'basic-username').read_text(),(pki/'basic-password').read_text()
def run(args):
    result = subprocess.run([nix,*args],capture_output=True,timeout=45)
    assert all(secret.encode() not in result.stdout+result.stderr for secret in (username,password)), 'native child exposed read credentials'
    return result
def failure_projection(label,result):
    # Raw diagnostics can include URLs or upstream text. Export only static
    # categories; an unknown failure never satisfies the expected guard proof.
    messages = {{
        'guard_mismatch':b'CF netrc transfer authority mismatch',
        'guard_invalid_origin':b'CF netrc authority requires a credential-free HTTPS origin',
        'guard_redirect':b'CF netrc reads reject HTTP 3xx responses',
        'invalid_narinfo':b'is corrupt:',
        'invalid_path':b'is not valid',
        'untrusted_signature':b'lacks a signature by a trusted key',
        'missing_cache_file':b'does not exist in binary cache',
        'http_forbidden':b'HTTP error 403',
        'tls_failure':b'SSL peer certificate',
    }}
    return {{'case':label,'exit_status':result.returncode,
             'reasons':{{reason:message in result.stderr for reason,message in messages.items()}}}}
with tempfile.TemporaryDirectory(prefix='cf-cache-read-guard-') as directory:
    directory = pathlib.Path(directory)
    directory.chmod(0o700)
    netrc = directory/'netrc'
    netrc.write_text('machine read-cache.test login "'+username+'" password "'+password+'"\\n')
    netrc.chmod(0o600)
    assert stat.S_IMODE(netrc.stat().st_mode) == 0o600 and stat.S_IMODE(directory.stat().st_mode) == 0o700
    settings = ['--option','netrc-file',str(netrc),'--option','cf-netrc-authority','https://read-cache.test:5753',
                '--option','require-sigs','true','--option','trusted-public-keys',{' '.join(signing_keys)!r}]
    for index,key in enumerate({signing_keys!r}):
        root = directory/('verified-'+str(index))
        args = ['copy','--refresh','--from','https://read-cache.test:5753','--to','local?root='+str(root),*settings,
                '--option','trusted-public-keys',key,{target['out']!r}]
        assert run(args).returncode == 0, 'fresh signed Basic closure copy failed'
        closure = run(['path-info','--recursive','--store','local?root='+str(root),{target['out']!r}])
        expected = run(['path-info','--recursive',{target['out']!r}])
        assert closure.returncode == expected.returncode == 0 and set(closure.stdout.splitlines()) == set(expected.stdout.splitlines()), 'incomplete signed closure'
        assert (root/{target['out'].lstrip('/')!r}).exists()
    for label in ('same-path','hostname','port','downgrade','permanent','not-modified','nar-host','nar-port'):
        root = directory/('denied-'+label)
        result = run(['copy','--refresh','--from','https://read-cache.test:5753/guard-'+label,
                      '--to','local?root='+str(root),*settings,{target['out']!r}])
        assert result.returncode != 0, 'guard accepted '+label
        assert not (root/{target['out'].lstrip('/')!r}).exists(), 'guard imported rejected output'
        expected = b'CF netrc transfer authority mismatch' if label.startswith('nar-') else b'CF netrc reads reject HTTP 3xx responses'
        print('Niks3 native negative classification: '+json.dumps(failure_projection(label,result),sort_keys=True),flush=True)
        assert expected in result.stderr, 'negative failed outside native guard: '+label
    for label in ('anonymous','bad-password'):
        if label == 'bad-password':
            netrc.write_text('machine read-cache.test login "'+username+'" password "incorrect"\\n')
        args = ['copy','--refresh','--from','https://read-cache.test:5753','--to','local?root='+str(directory/label),*settings,{target['out']!r}]
        if label == 'anonymous':
            args[args.index('netrc-file')+1] = '/dev/null'
        assert run(args).returncode != 0, 'native read accepted '+label
    # No CF option, no netrc, no credentials: ordinary Nix still follows redirects.
    control = run(['store','ping','--store','https://read-cache.test:5753/guard-same-path','--option','netrc-file','/dev/null'])
    assert control.returncode == 0, 'ordinary Nix redirect behavior changed'
print(json.dumps({{'native_guard':True,'negative_cases':8,'signed_complete_closures':2,'protected_netrc':True,'ordinary_redirect':True}}))
"""
    status, output = reader.execute("python3 -c " + shlex.quote(source), timeout=180)
    prefix = "Niks3 native negative classification: "
    for line in output.splitlines():
        if line.startswith(prefix):
            projection = json.loads(line[len(prefix):])
            assert set(projection) == {"case", "exit_status", "reasons"}
            assert projection["case"] in ("same-path", "hostname", "port", "downgrade", "permanent", "not-modified", "nar-host", "nar-port")
            assert type(projection["exit_status"]) is int
            assert set(projection["reasons"]) == {"guard_mismatch", "guard_invalid_origin", "guard_redirect", "invalid_narinfo", "invalid_path", "untrusted_signature", "missing_cache_file", "http_forbidden", "tls_failure"}
            assert all(type(value) is bool for value in projection["reasons"].values())
            print(prefix + json.dumps(projection, sort_keys=True))
    if status:
        targets = production_observations(production, "target")[before:]
        print("Niks3 native failed matrix target projection: " + json.dumps({
            "exit_status": status, "target_requests": len(targets),
            "authorization_present": any(record["authorization"] for record in targets)}, sort_keys=True))
        assert not targets, "failed guarded matrix reached a target"
        raise AssertionError("native guard matrix failed; see allowlisted classification")
    proof = json.loads(output.splitlines()[-1])
    # Only the final ordinary control may reach a target. No Basic header may
    # reach any target; guarded redirects and foreign NARs must make zero calls.
    targets = production_observations(production, "target")[before:]
    assert len(targets) == 1 and not targets[0]["authorization"], "guard forwarded a transfer or credentials"
    assert any(record["authorization"] and record["status"] == 200 for record in production_observations(production, "read"))
    assert sum(record["status"] == 403 for record in production_observations(production, "read")) >= 2, "missing native wrong/absent Basic denials"
    print("Niks3 Basic native guard: " + json.dumps(proof, sort_keys=True) + "; guarded target requests=0; ordinary target requests=1; target authorization=false")


def run_matrix(machines, targets, builder_public_key, credentials, negative_targets=None, onboarding_bundle=None):
    """Runs the original five plus the opt-in split-PKI variant and negatives."""
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

    def wait_row(query, params, predicate, description, timeout=180, interval=2):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            rows = sql(query, params)
            if predicate(rows):
                return rows
            time.sleep(interval)
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

    def heartbeat(capable=False, confidential=True, basic_capable=True):
        # Sign the exact legacy flat body, not a current packaged agent request.
        # Spoofed capability headers must not override the signed absent flag.
        state = {"hostname": "agent", "change_reason": "startup",
                 "store_path": agent.succeed("readlink -f /run/current-system").strip()}
        if capable:
            state["capabilities"] = {"supports_niks3": True, "supports_niks3_basic_read": basic_capable}
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

    def assert_withheld(pending_id, description, capable=True, confidential=True, basic_capable=True):
        response = heartbeat(capable=capable, confidential=confidential, basic_capable=basic_capable)
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
    legacy_signing_keys = list(signing_keys)
    production = machines.get("production")
    pki = credentials / "production"
    if production is not None:
        check_production_transport(production, builder, credentials)
        for name, machine in (("server", server), ("builder", builder)):
            machine.succeed("python3 -c " + shlex.quote("""
import pathlib
system = pathlib.Path('/etc/ssl/certs/ca-certificates.crt').read_bytes()
pki = pathlib.Path('/etc/niks3-production-pki')
assert (pki / 'server-ca-b.crt').read_bytes().strip() in system
assert (pki / 'server-ca-a.crt').read_bytes().strip() not in system
assert (pki / 'client-ca-c.crt').read_bytes().strip() not in system
"""))
            print(f"Niks3 {name}: system trusts B, not private A or client issuer C")
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

    for variant, target in (negative_targets or {}).items():
        ca_name = "server-ca-a.crt" if variant == "trust-a-only" else "server-ca-b.crt"
        negative_keys = [(pki / f"signing-{index}.pub").read_text().strip() for index in range(2)]
        commit_id = sql("INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp,evaluation_status) VALUES (%s,%s,NOW(),'complete') RETURNING id",
                        (flake_id, hashlib.sha1(variant.encode()).hexdigest()))[0][0]
        sql("INSERT INTO commit_artifacts_cache(commit_id,nixos_configurations) VALUES (%s,ARRAY['agent'])", (commit_id,))
        cache_id = sql("""INSERT INTO cache_destinations
            (name,cache_type,enabled,push_to,niks3_server_url,niks3_public_keys,
             niks3_write_auth_mode,niks3_auth_token,niks3_write_client_cert,niks3_write_client_key,
             niks3_write_ca_cert,niks3_read_auth_mode,require_sigs,parallel_uploads,max_retries,push_timeout_seconds)
            VALUES (%s,'Niks3',true,'https://read-cache.test:5753','https://push-cache.test:5754',%s,
                'mtls',NULL,%s,%s,%s,'none',true,2,0,30) RETURNING id""",
            (variant, negative_keys, (pki / "write-client-1.crt").read_text(),
             encrypt((pki / "write-client-1.key").read_text()), (pki / ca_name).read_text()))[0][0]
        sql("INSERT INTO cache_destination_environments(cache_destination_id,environment_id) VALUES (%s,%s)", (cache_id, environment_id))
        derivation_id = sql("""INSERT INTO derivations
            (commit_id,derivation_type,derivation_name,derivation_target,derivation_path,store_path,status_id,
             cf_agent_enabled,policy_requirements_met,scheduled_at)
            VALUES (%s,'nixos','agent','agent',%s,%s,5,true,true,NOW()) RETURNING id""", (commit_id, target["drv"], target["out"]))[0][0]
        cursor = builder.succeed("journalctl -u crystal-forge-builder -n 0 --show-cursor --no-pager").strip().split("-- cursor: ")[-1]
        job_id = sql("INSERT INTO build_jobs(derivation_id,environment_id,status,queue_position,max_retries) VALUES (%s,%s,'queued',1000,0) RETURNING id", (derivation_id, environment_id))[0][0]
        fixture_jobs.append(str(job_id))
        fixture_derivations.append(derivation_id)
        fixture_ids.update(job=str(job_id), scan=None, selected_caches=[cache_id])
        wait_row("SELECT cache_dispatch_recorded_at IS NOT NULL FROM build_jobs WHERE id=%s", (job_id,),
                 lambda rows: rows and rows[0][0], variant + " dispatch before bounded CLI observation")
        projection = production_cli_projection(server, pki / ca_name)
        print(f"Niks3 {variant}: CF input CLI safe flags/env " + json.dumps(projection, sort_keys=True))
        wait_row("SELECT status FROM build_jobs WHERE id=%s", (job_id,), lambda rows: rows and rows[0][0] in ("success", "failed"), variant + " CF job terminal", timeout=180)
        assert sql("SELECT dispatched_cache_destination_id FROM build_jobs WHERE id=%s", (job_id,))[0][0] == cache_id
        assert not sql("SELECT id FROM cache_push_jobs WHERE derivation_id=%s AND status='completed'", (derivation_id,)), variant + ": one-root trust fabricated publication"
        hint = negative_tls_hint(builder, cursor)
        print(f"Niks3 {variant}: safe TLS diagnostic " + json.dumps(hint, sort_keys=True))
        scans = sql("SELECT id::text,status FROM cve_scans WHERE derivation_id=%s", (derivation_id,))
        for scan_id, _ in scans:
            wait_row("SELECT status FROM cve_scans WHERE id=%s", (scan_id,), lambda rows: rows and rows[0][0] in ("completed", "failed", "cancelled"), variant + " scan terminal", timeout=120)
            fixture_scans.append(scan_id)
        sql("UPDATE cache_destinations SET enabled=false WHERE id=%s", (cache_id,))
        print(f"Niks3 {variant}: real CF dispatch bound to cache {cache_id}; no completed publication")

    for variant, target in targets.items():
        split = variant == "mtls-split"
        if split:
            target = fresh_split_target(server, target)
            targets[variant] = target
        signing_keys = [(pki / f"signing-{index}.pub").read_text().strip() for index in range(2)] if split else legacy_signing_keys
        published_name = f"z-published-{variant}"
        commit_id = sql("INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp,evaluation_status) VALUES (%s,%s,NOW(),'complete') RETURNING id", (flake_id, hashlib.sha1(variant.encode()).hexdigest()))[0][0]
        # The evaluated identity is seeded; background metadata hydration must
        # not try to fetch the intentionally nonexistent fixture repository.
        sql("INSERT INTO commit_artifacts_cache(commit_id,nixos_configurations) VALUES (%s,ARRAY['agent'])", (commit_id,))
        private = "private" in variant
        mtls_write = variant.startswith("mtls")
        read_port = 5752 if private else (5753 if variant.endswith("proxy") else 5751)
        read_url = "https://read-cache.test:5753" if split else f"https://cache:{read_port}"
        write_url = "https://push-cache.test:5754" if split else "https://cache:5751"
        write_cert = (pki / "write-client-1.crt").read_text() if split else pem("write.crt")
        write_key = (pki / "write-client-1.key").read_text() if split else pem("write.key")
        # Seed incomplete server trust, then correct the SAME ID via Save before
        # dispatch. This must not reuse an in-process pre-Save configuration.
        write_ca = (pki / "server-ca-a.crt").read_text() if split else pem("ca.crt")
        cache_id = sql("""INSERT INTO cache_destinations
            (name,cache_type,enabled,push_to,niks3_server_url,niks3_public_keys,
             niks3_write_auth_mode,niks3_auth_token,niks3_write_client_cert,
             niks3_write_client_key,niks3_write_ca_cert,niks3_read_auth_mode,
             niks3_read_client_cert,niks3_read_client_key,niks3_read_ca_cert,
              niks3_read_basic_username,niks3_read_basic_password,require_sigs,parallel_uploads,max_retries)
              VALUES (%s,'Niks3',true,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,true,2,0)
             RETURNING id""", (published_name, read_url, write_url, signing_keys,
                "mtls" if mtls_write else "token", None if mtls_write else encrypt(TOKEN),
                write_cert if mtls_write else None, encrypt(write_key) if mtls_write else None,
                 write_ca if mtls_write else None, "basic" if split else ("mtls" if private else "none"), pem("read.crt") if private else None,
                 encrypt(pem("read.key")) if private else None, pem("ca.crt") if private else None,
                 (pki / "basic-username").read_text() if split else None,
                 encrypt((pki / "basic-password").read_text()) if split else None))[0][0]
        sql("INSERT INTO cache_destination_environments(cache_destination_id,environment_id) VALUES (%s,%s)", (cache_id, environment_id))
        if split:
            baseline = sql("SELECT to_jsonb(c) FROM cache_destinations c WHERE id=%s", (cache_id,))[0][0]
            scope_before = sql("SELECT * FROM cache_destination_environments WHERE cache_destination_id=%s ORDER BY environment_id", (cache_id,))
            api_baseline = len(production_observations(production, "api"))
            s3_baseline = len(production_observations(production, "s3"))
            assert onboarding_bundle is not None, "onboarding CA fixture missing"
            raw_onboarding = Path(onboarding_bundle).read_bytes()
            expected_ca, expected_der_hashes = onboarding_ca_expectation(raw_onboarding)
            expected_ca_digest = hashlib.sha256(expected_ca.encode()).hexdigest()
            saved = save_split_ca(server, cache_id, expected_ca_digest, hashlib.sha256(raw_onboarding).hexdigest())
            persisted = sql("SELECT to_jsonb(c) FROM cache_destinations c WHERE id=%s", (cache_id,))[0][0]
            assert persisted["niks3_write_ca_cert"] == expected_ca
            # Exact canonical equality covers every ordinary DER identity and
            # its order. The Go/Rust gate separately proves raw AUX/label policy.
            from cryptography import x509
            from cryptography.hazmat.primitives.serialization import Encoding
            assert [hashlib.sha256(cert.public_bytes(Encoding.DER)).hexdigest() for cert in
                    x509.load_pem_x509_certificates(persisted["niks3_write_ca_cert"].encode())] == expected_der_hashes
            assert hashlib.sha256(expected_ca.encode()).hexdigest() == saved["saved_ca_sha256"]
            assert persisted["niks3_write_client_key"] == baseline["niks3_write_client_key"]
            assert persisted["niks3_write_client_key"].startswith("enc:v1:")
            assert {k: v for k, v in persisted.items() if k not in ("niks3_write_ca_cert", "updated_at")} == {
                k: v for k, v in baseline.items() if k not in ("niks3_write_ca_cert", "updated_at")}
            assert sql("SELECT * FROM cache_destination_environments WHERE cache_destination_id=%s ORDER BY environment_id", (cache_id,)) == scope_before
            # Protected probes add only the native invalid-body validation POSTs.
            # Exact ordering and identity flags distinguish those calls from a
            # valid upload. Local validation and B-only trust make no API call.
            api_delta = production_observations(production, "api")[api_baseline:]
            positive = [(False, True, True, 200), (False, True, True, 404),
                        (False, False, False, 400), (True, True, True, 400),
                        (True, False, False, 400)]
            expected_api = ([(False, False, False, 400)] + positive * 3
                            + [(False, True, False, 200), (False, True, False, 401),
                               (False, False, False, 400)])
            observed_api = [(record["post"], record["client_certificate"],
                             record["write_subject"], record["status"]) for record in api_delta]
            assert observed_api == expected_api, "unexpected protected probe choreography"
            assert not any(record["put"] or record["authorization"] for record in api_delta)
            assert not any(record["put"] or record["post"] for record in
                           production_observations(production, "s3")[s3_baseline:])
            print("Niks3 mtls-split: saved synthetic CA-only correction " + json.dumps(saved, sort_keys=True))
            print("Niks3 mtls-split: protected probe counts " + json.dumps({
                "api_requests": len(api_delta), "invalid_validation_posts": sum(record["post"] for record in api_delta),
                "api_puts": 0, "s3_writes": 0, "raw_cf_state_unchanged": True}, sort_keys=True))
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
        if split:
            object_before = split_object_boundary(production, target, before=True)
            api_baseline = len(production_observations(production, "api"))
            s3_baseline = len(production_observations(production, "s3"))
            print("Niks3 mtls-split: pre-dispatch fresh object absence " + json.dumps(object_before, sort_keys=True))
            server_address = production.succeed("python3 -c 'import socket; print(socket.gethostbyname(\"server\"))'").strip()
            completion_rate_limit(production, server, server_address, target, enabled=True)
            completion_metadata_boundary(production, target, [], before=True)
        job_id = sql("INSERT INTO build_jobs(derivation_id,environment_id,status,queue_position,max_retries) VALUES (%s,%s,'queued',1000,0) RETURNING id", (derivation_id, environment_id))[0][0]
        fixture_jobs.append(str(job_id))
        fixture_derivations.append(derivation_id)
        fixture_ids.update(job=str(job_id), scan=None, selected_caches=[cache_id])
        snapshot(f"{variant}: queued", {"server": server})
        if split:
            assert sql("SELECT niks3_read_basic_password LIKE 'enc:v1:%%' FROM cache_destinations WHERE id=%s", (cache_id,))[0][0]
            wait_row("SELECT cache_dispatch_recorded_at IS NOT NULL FROM build_jobs WHERE id=%s", (job_id,),
                     lambda rows: rows and rows[0][0], "split-PKI dispatch before bounded CLI observation", interval=0.1)
            projection = production_cli_projection(builder, expected_digest=saved["saved_ca_sha256"])
            print("Niks3 mtls-split: remote builder CLI protected mTLS/no-token/no-AWS proof " + json.dumps(projection, sort_keys=True))
            # Hold the root 429 until the driver sees both states committed by
            # the same completion transaction. A two-second callback retry can
            # race a slow driver; leaving the injection active makes that race
            # deterministic without sleeps, rebuilding or changing providers.
            pending_rows = wait_row("""SELECT j.status,p.id,p.status,p.cache_destination_id,
                p.cache_destination_source,p.store_path,p.error_message,j.completed_at
                FROM build_jobs j JOIN cache_push_jobs p ON p.derivation_id=j.derivation_id
                WHERE j.id=%s AND p.cache_destination_id=%s""", (job_id, cache_id),
                lambda rows: len(rows) == 1 and rows[0][:1] == ("success",)
                and rows[0][2:7] == ("pending", cache_id, "database", target["out"], "cache_rate_limited"),
                "split-PKI atomic success plus rate-limited pending publication", timeout=180, interval=0.1)
            pending_identity, completed_at = pending_rows[0][1], pending_rows[0][7]
            session_id = sql("SELECT builder_session_id FROM build_jobs WHERE id=%s", (job_id,))[0][0]
            assert completed_at is not None and session_id is not None
            assert not sql("SELECT id FROM cache_push_jobs WHERE derivation_id=%s AND status='completed'", (derivation_id,))
            pending_receipt = completion_receipt(server, builder_id, session_id, job_id,
                                                 cache_id, target, published_name, pending=True)
            print("Niks3 mtls-split: actual pending completion receipt " + json.dumps(pending_receipt, sort_keys=True))
            # The server must not materialize the output during completion. This
            # absence is checked before the explicit CVE import phase below.
            server.fail(f"test -e {shlex.quote(target['out'])}")
            completion_rate_limit(production, server, server_address, target, enabled=False)
            wait_row("SELECT status FROM cache_push_jobs WHERE id=%s", (pending_identity,),
                     lambda rows: rows == [("completed",)], "split-PKI builder POST-only completion recovery", timeout=180)
            assert sql("SELECT status,builder_session_id,completed_at FROM build_jobs WHERE id=%s", (job_id,))[0] == (
                "success", session_id, completed_at), "retry changed the successful build identity or completion time"
            assert sql("SELECT id FROM build_jobs WHERE derivation_id=%s", (derivation_id,)) == [(job_id,)], "completion retry created another build"
            confirmed_receipt = completion_receipt(server, builder_id, session_id, job_id,
                                                   cache_id, target, published_name, pending=False)
            print("Niks3 mtls-split: actual confirmed completion receipt " + json.dumps(confirmed_receipt, sort_keys=True))
            callback_deadline = time.monotonic() + 10
            while True:
                callbacks = completion_callbacks(server, builder_id, job_id)
                if any(not record["driver"] and record["status"] == 200 for record in callbacks):
                    break
                assert time.monotonic() < callback_deadline, "packaged builder confirmed receipt missing"
                time.sleep(0.1)
            automatic = [record for record in callbacks if not record["driver"]]
            assert [record["status"] for record in callbacks if record["driver"]] == [202, 200]
            assert 2 <= len(automatic) <= 5 and automatic[0]["status"] == 202 and automatic[-1]["status"] == 200
            assert all(record["status"] == 202 for record in automatic[:-1]), "completion recovery returned a conflict or false confirmation"
            assert automatic[-1]["end"] - automatic[0]["end"] + automatic[0]["duration"] <= 600
            traffic = completion_metadata_boundary(production, target, callbacks)
            stages = publication_stage_projection(sql("SELECT logs FROM build_jobs WHERE id=%s", (job_id,))[0][0], cache_id, published_name)
            assert stages[0]["outcome"] == "cache_rate_limited" and stages[-1]["outcome"] == "success"
            assert len(stages) == len(callbacks), "completion stage/callback evidence mismatch"
            print("Niks3 mtls-split: completion-only native metadata traffic " + json.dumps(traffic, sort_keys=True))
            print("Niks3 mtls-split: availability display stages (identity from DB/callbacks) " + json.dumps(stages, sort_keys=True))
            server.fail(f"test -e {shlex.quote(target['out'])}")
        wait_row("SELECT status,builder_id FROM build_jobs WHERE id=%s", (job_id,),
                 lambda rows: rows and rows[0][0] in ("success", "failed"), f"{variant} remote completion")
        assert sql("SELECT status,builder_id FROM build_jobs WHERE id=%s", (job_id,))[0] == ("success", builder_id), f"{variant}: remote build failed"
        dispatch = sql("SELECT dispatched_cache_destination_id,cache_dispatch_recorded_at FROM build_jobs WHERE id=%s", (job_id,))[0]
        assert dispatch[0] == cache_id and dispatch[1] is not None, f"{variant}: dispatch identity was not bound"
        publication = sql("SELECT status,cache_destination,cache_destination_id,cache_destination_source FROM cache_push_jobs WHERE derivation_id=%s", (derivation_id,))
        assert publication and all(row == ("completed", published_name, cache_id, "database") for row in publication), f"{variant}: server did not verify selected destination"
        # Human-readable publishing lines attest presentation only. The rows
        # above and exact-job callbacks independently bind destination identity.
        job_logs = sql("SELECT logs FROM build_jobs WHERE id=%s", (job_id,))[0][0]
        log_lines = job_logs.split("\n")
        publish_start = f"📤 Publishing build output to {published_name} (Niks3)..."
        publish_success = f"✅ Build output published to {published_name}"
        assert log_lines.count(publish_start) == log_lines.count(publish_success) == 1, f"{variant}: publishing display missing or duplicated"
        assert log_lines.index(publish_start) < log_lines.index(publish_success), f"{variant}: publishing display order invalid"
        assert dispatch[0] != global_id
        if split:
            stored = sql("SELECT niks3_auth_token,niks3_write_client_key,s3_access_key_id,s3_secret_access_key,s3_session_token FROM cache_destinations WHERE id=%s", (cache_id,))[0]
            assert stored[0] is None and stored[1].startswith("enc:v1:") and all(value is None for value in stored[2:])
            object_after = split_object_boundary(production, target, before=False)
            assert object_after["logical_nar_uploads"] == object_after["unique_upload_ids"] == 1
            assert object_after["nar_puts"] == object_after["multipart_parts"] == 2
            assert object_after["narinfo_puts"] == 1
            build_proof = selected_build_start_proof(builder, job_id, derivation_id)
            print("Niks3 mtls-split: one selected build-engine start " + json.dumps(build_proof, sort_keys=True))
            api_records = production_observations(production, "api")[api_baseline:]
            assert any(record["post"] and record["write_subject"] and record["client_certificate"] and 200 <= record["status"] < 300 for record in api_records)
            assert all(not record["authorization"] for record in api_records)
            s3_records = production_observations(production, "s3")[s3_baseline:]
            puts = [record for record in s3_records if record["put"] and not record["backend"] and 200 <= record["status"] < 300]
            assert puts and all(record["signature_validated"] and not record["authorization"] and not record["client_certificate"] for record in puts)
            print("Niks3 mtls-split: fresh exact-output CF publication " + json.dumps(object_after, sort_keys=True))
        print(f"Niks3 {variant}: remote completion, dispatch binding, and selected publication metadata available")
        snapshot(f"{variant}: publication completed", {"server": server})
        builder.succeed(f"test -e {shlex.quote(target['out'])}")
        agent.fail(f"test -e {shlex.quote(target['out'])}")

        # Verify both configured signatures on the actual published narinfo.
        nar_hash = Path(target["out"]).name.split("-", 1)[0]
        cert_args = "--cert /etc/niks3-fixtures/read.crt --key /etc/niks3-fixtures/read.key" if private else ""
        narinfo = basic_read_narinfo(agent, target) if split else agent.succeed(f"curl --fail --silent {cert_args} {read_url}/{nar_hash}.narinfo")
        if split:
            # Correlate actual provider metadata with the pre-dispatch hash
            # preflight. Do not emit NAR URLs, including unexpected signed URLs.
            assert [line for line in narinfo.splitlines() if line.startswith("StorePath:")] == ["StorePath: " + target["out"]]
            assert [line for line in narinfo.splitlines() if line.startswith("URL:")] == ["URL: " + target["nar_key"]]
            run_basic_read_guard(production, builder, target, signing_keys, narinfo)
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
        if split:
            assert_withheld(pending_id, "Basic read reached old Niks3-capable agent", basic_capable=False)
            print("Niks3 Basic: signed supports_niks3=true/basic=false withholds cache and target; pending request unclaimed")
        if private or split:
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
    if production is not None:
        secrets += [(pki / name).read_text() for name in ("basic-username", "basic-password")]
        secrets.append(base64.b64encode(((pki / "basic-username").read_text() + ":" + (pki / "basic-password").read_text()).encode()).decode())
        secrets.append("private-server-only-api-token-production-470")
        secrets += [path.read_text().splitlines()[1] for path in pki.glob("*.key") if "BEGIN PRIVATE KEY" in path.read_text()]
    for name, machine in machines.items():
        logs = machine.succeed("journalctl --no-pager -u crystal-forge-server -u crystal-forge-builder -u crystal-forge-agent -u niks3 -u nginx -u garage -u niks3-production -u niks3-production-secrets -u niks3-production-bridge")
        assert not any(secret in logs for secret in secrets), f"credential leaked in {name} service log"
        assert "X-Amz-Signature=" not in logs, f"presigned upload URL leaked in {name} service log"
        assert "-----BEGIN CERTIFICATE-----" not in logs, f"CA/certificate PEM leaked in {name} service log"
        snapshot("immediately before exact cleanup assertion", {name: machine})
        machine.succeed("test -z \"$(find /tmp /var/lib/crystal-forge /var/lib/crystal-forge-agent -maxdepth 3 -type d -name 'cf-cache-*' -print 2>/dev/null)\"")
    with db.cursor() as cursor:
        cursor.execute("SELECT COALESCE(scan_metadata::text,'') FROM cve_scans UNION ALL SELECT message FROM cve_scan_diagnostic_events")
        diagnostics = cursor.fetchall()
        assert len(diagnostics) >= len(targets), "scan audit had no persisted evidence"
        assert not any(secret in row[0] for row in diagnostics for secret in secrets), "credential leaked in persisted scan diagnostics"
    db.close()
    print(f"Niks3: all {len(targets)} remote-builder/agent variants, split-CA negatives and credential audits passed")
