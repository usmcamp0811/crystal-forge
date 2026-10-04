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
import json
import shlex
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path

import psycopg2
from nacl.signing import SigningKey
from cryptography.hazmat.primitives.ciphers.aead import AESGCM


TOKEN = "niks3-nonproduction-static-token-470-00000000"
ENCRYPTION_KEY = "niks3-vm-only-cache-encryption-key-470"


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
        wait_row("SELECT status,builder_id FROM build_jobs WHERE id=%s", (job_id,),
                 lambda rows: rows and rows[0][0] in ("success", "failed"), f"{variant} remote completion")
        assert sql("SELECT status,builder_id FROM build_jobs WHERE id=%s", (job_id,))[0] == ("success", builder_id), f"{variant}: remote build failed"
        dispatch = sql("SELECT dispatched_cache_destination_id,cache_dispatch_recorded_at FROM build_jobs WHERE id=%s", (job_id,))[0]
        assert dispatch[0] == cache_id and dispatch[1] is not None, f"{variant}: dispatch identity was not bound"
        publication = sql("SELECT status,cache_destination,cache_destination_id,cache_destination_source FROM cache_push_jobs WHERE derivation_id=%s", (derivation_id,))
        assert publication and all(row == ("completed", published_name, cache_id, "database") for row in publication), f"{variant}: server did not verify selected destination"
        assert dispatch[0] != global_id
        print(f"Niks3 {variant}: remote completion, dispatch binding, and selected publication verified")
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
        server.wait_until_succeeds(f"test -e {shlex.quote(target['out'])}", timeout=120)
        wait_row("SELECT status FROM cve_scans WHERE id=%s", (scan_id,),
                 lambda rows: rows and rows[0][0] in ("completed", "failed"), f"{variant} CVE materialization/process cleanup", timeout=120)
        assert sql("SELECT lease_builder_id FROM cve_scans WHERE id=%s", (scan_id,))[0][0] is None
        scan_status = sql("SELECT status FROM cve_scans WHERE id=%s", (scan_id,))[0][0]
        print(f"Niks3 {variant}: CVE output materialized; scanner terminal status={scan_status}")

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
        agent.succeed("systemctl restart crystal-forge-agent.service")
        agent.wait_until_succeeds(f"test -e {shlex.quote(target['out'])}", timeout=120)
        assert sql("SELECT delivered_at IS NOT NULL FROM pending_system_deployments WHERE id=%s", (pending_id,))[0][0], f"{variant}: packaged agent did not claim deployment"
        print(f"Niks3 {variant}: real agent pulled previously absent output")
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

    # Invalid write identity must fail against the real native-mTLS server.
    # Keep tokens in protected files and suppress output, including URLs.
    builder.succeed("command -v niks3; niks3 --help >/dev/null 2>&1")
    builder.succeed("umask 077; printf '%s' invalid-niks3-token-470-00000000000000000 > /tmp/niks3-invalid-token")
    some_target = next(iter(targets.values()))["out"]
    builder.fail(f"niks3 push --server-url https://cache:5751 --auth-token-path /tmp/niks3-invalid-token {shlex.quote(some_target)} >/dev/null 2>&1")
    builder.fail(f"niks3 push --server-url https://cache:5751 --client-cert /etc/niks3-fixtures/wrong.crt --client-key /etc/niks3-fixtures/wrong.key {shlex.quote(some_target)} >/dev/null 2>&1")
    builder.succeed("rm /tmp/niks3-invalid-token")

    # Audit service logs in memory; do not print a failing secret or full log.
    secrets = [TOKEN, "unrelated-environment-token-470-00000000",
               "proxy-claim-attic-secret", "proxy-claim-s3-secret", "proxy-claim-niks3-secret"]
    secrets += [pem(f"{name}.key").splitlines()[1] for name in ("write", "read", "wrong")]
    for name, machine in machines.items():
        logs = machine.succeed("journalctl --no-pager -u crystal-forge-server -u crystal-forge-builder -u crystal-forge-agent -u niks3 -u nginx -u garage")
        assert not any(secret in logs for secret in secrets), f"credential leaked in {name} service log"
        assert "X-Amz-Signature=" not in logs, f"presigned upload URL leaked in {name} service log"
        machine.succeed("test -z \"$(find /tmp /var/lib/crystal-forge /var/lib/crystal-forge-agent -maxdepth 3 -type d -name 'cf-cache-*' -print 2>/dev/null)\"")
    with db.cursor() as cursor:
        cursor.execute("SELECT COALESCE(scan_metadata::text,'') FROM cve_scans UNION ALL SELECT message FROM cve_scan_diagnostic_events")
        diagnostics = cursor.fetchall()
        assert len(diagnostics) >= len(targets), "scan audit had no persisted evidence"
        assert not any(secret in row[0] for row in diagnostics for secret in secrets), "credential leaked in persisted scan diagnostics"
    db.close()
    print("Niks3: all five remote-builder/agent variants and credential audits passed")
