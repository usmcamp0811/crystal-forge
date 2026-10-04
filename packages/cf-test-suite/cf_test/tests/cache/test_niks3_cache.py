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
from pathlib import Path

import psycopg2
from nacl.signing import SigningKey
from cryptography.hazmat.primitives.ciphers.aead import AESGCM


TOKEN = "niks3-nonproduction-static-token-470-00000000"
ENCRYPTION_KEY = "niks3-vm-only-cache-encryption-key-470"


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
    secrets = [TOKEN, "unrelated-environment-token-470-00000000"]
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
