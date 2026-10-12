"""Native function-level publication regression; credentials never leave the VM."""
import json
import os
from pathlib import Path
import subprocess
import concurrent.futures
import time
import uuid

import psycopg
import requests

BASE = "http://127.0.0.1:8081/"


def command(args, **kwargs):
    result = subprocess.run(args, capture_output=True, **kwargs)
    assert result.returncode == 0, "native fixture command failed (output suppressed)"
    return result.stdout.decode().strip()


def mint(*permissions):
    return command(["atticadm", "--config", "/etc/atticd.toml", "make-token",
                    "--sub", "publication-fixture", "--validity", "1d", *permissions])


def main():
    os.umask(0o077)
    admin = mint("--pull", "*", "--push", "*", "--create-cache", "*", "--configure-cache", "*")
    writer = mint("--pull", "*", "--push", "public")
    reader = mint("--pull", "*")
    headers = {"Authorization": "Bearer " + admin}
    # Native API contract from pinned attic/api/v1/cache_config.rs.
    for name, public in [("public", True), ("private", False)]:
        response = requests.post(BASE + "_api/v1/cache-config/" + name, headers=headers,
                                 json={"is_public": public, "keypair": "Generate", "store_dir": "/nix/store",
                                       "priority": 40, "upstream_cache_key_names": []}, timeout=10)
        assert response.status_code == 200, "native cache creation failed"
    home = Path("/var/lib/crystal-forge/.config/attic")
    home.mkdir(parents=True, mode=0o700)
    token_file = home / "writer"
    token_file.write_text(writer)
    (home / "config.toml").write_text(
        'default-server = "local"\n[servers.local]\nendpoint = ' + json.dumps(BASE)
        + '\ntoken-file = ' + json.dumps(str(token_file)) + '\n')
    # The fixture uses actual PostgreSQL storage and the actual wire mapping.
    # It does not claim to cover HTTP dispatch or encrypted-row resolution.
    with psycopg.connect("dbname=postgres user=postgres host=/run/postgresql", autocommit=True) as db:
        db.execute("CREATE TABLE publication_configs (id integer PRIMARY KEY, config jsonb NOT NULL)")
        for identity, token, cache in [(4701, writer, "public"), (4702, reader, "public"),
                                       (4703, mint("--pull", "private", "--push", "private"), "private"),
                                       (4704, reader, "private"), (4705, "invalid-native-jwt", "public")]:
            db.execute("INSERT INTO publication_configs VALUES (%s, %s::jsonb)", (identity, json.dumps({
                "cache_destination_id": identity, "cache_type": "Attic", "push_to": BASE,
                "push_after_build": True, "attic_cache_name": cache, "attic_token": token,
                "max_retries": 0, "push_timeout_seconds": 30,
                "attic_ignore_upstream_cache_filter": True, "attic_jobs": 5, "require_sigs": True,
            })))
        before_rows = db.execute("SELECT * FROM publication_configs ORDER BY id").fetchall()
        # Public read success is not permission to write, even with bad JWTs.
        assert requests.get(BASE + "_api/v1/cache-config/public", timeout=10).status_code == 200
        assert requests.get(BASE + "_api/v1/cache-config/public",
                            headers={"Authorization": "Bearer invalid-native-jwt"}, timeout=10).status_code == 200
        print("public_anonymous_read_success=true write_authorization_proven=false", flush=True)

        stale_config = (home / "config.toml").read_bytes()
        def attempt(identity, expected, driver="attic-publication", conflict=False, retries=0,
                    lifecycle=None, parallel=False, static=False, alias=False):
            push = db.execute("SELECT config FROM publication_configs WHERE id=%s", (identity,)).fetchone()[0]
            push = dict(push, max_retries=retries, retry_delay_seconds=0)
            if lifecycle == "timeout":
                push["push_timeout_seconds"] = 1
            cache = push["attic_cache_name"]
            if alias:
                push["attic_cache_name"] = "ignored-alias:" + cache
            if static:
                push["cache_destination_id"] = None
            source = Path("/run/publication-" + uuid.uuid4().hex)
            source.write_bytes(os.urandom(8192))
            path = command(["nix-store", "--add", str(source)])
            digest = Path(path).name.split("-")[0]
            read_headers = {"Authorization": "Bearer " + writer}
            metadata = BASE + cache + "/" + digest + ".narinfo"
            before = requests.get(metadata, headers=read_headers, timeout=10)
            assert before.status_code == 404, "negative path was already cached"
            if lifecycle:
                command(["systemctl", "kill", "--signal=STOP", "atticd.service"])
            derivation = dict(id=1, commit_id=None, derivation_type="package", derivation_name="publication",
                              derivation_path=None, scheduled_at=None, completed_at=None, started_at=None,
                              attempt_count=0, evaluation_duration_ms=None, error_message=None, pname=None,
                              version=None, status_id=0, derivation_target=None, store_path=path)
            environment = dict(os.environ, ATTIC_TOKEN=writer,
                               ATTIC_SERVER_URL="http://127.0.0.1:9/" if conflict else BASE,
                               ATTIC_REMOTE_NAME="local")
            payload = dict(push=push, derivation=derivation, path=path, cancel=lifecycle == "cancel")
            if driver == "attic-server-publication":
                payload["config"] = {key: value for key, value in push.items() if key != "cache_destination_id"}
                payload["database"] = not static
                payload["derivation"] = dict(derivation, derivation_type="Package")
            process = subprocess.Popen([driver], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       stderr=subprocess.PIPE, env=environment)
            if lifecycle:
                process.stdin.write(json.dumps(payload).encode())
                process.stdin.close()
                expires = time.monotonic() + 10
                child = None
                protected = None
                while time.monotonic() < expires:
                    for task in Path(f"/proc/{process.pid}/task").glob("*/children"):
                        for pid in task.read_text().split():
                            env = Path(f"/proc/{pid}/environ").read_bytes().split(b"\0")
                            values = dict(entry.split(b"=", 1) for entry in env if b"=" in entry)
                            if b"XDG_CONFIG_HOME" in values and b"cf-cache-" in values[b"XDG_CONFIG_HOME"]:
                                child, protected = pid, Path(os.fsdecode(values[b"XDG_CONFIG_HOME"]))
                    if child:
                        break
                    time.sleep(0.005)
                assert child and protected, "native selected child was not observed"
                assert protected.stat().st_mode & 0o777 == 0o700
                assert (protected / "token").stat().st_mode & 0o777 == 0o600
                assert (protected / "attic/config.toml").stat().st_mode & 0o777 == 0o600
                assert b"ATTIC_TOKEN" not in values and b"ATTIC_SERVER_URL" not in values
                assert push["attic_token"].encode() not in Path(f"/proc/{child}/cmdline").read_bytes()
                assert writer.encode() not in Path(f"/proc/{child}/cmdline").read_bytes()
                process.wait(timeout=15)
                assert not Path(f"/proc/{child}").exists(), "native child survived terminal owner outcome"
                assert not protected.exists(), "private selected config survived kill/reap"
                print(f"driver={driver} lifecycle={lifecycle} child_reaped=true protected_cleanup=true", flush=True)
                command(["systemctl", "kill", "--signal=CONT", "atticd.service"])
            else:
                process.communicate(json.dumps(payload).encode(), timeout=120)
            after = requests.get(metadata, headers=read_headers, timeout=10)
            success = process.returncode == 0
            print(f"driver={driver} stored_id={identity} driver_exit={process.returncode} native_inventory_present={after.status_code == 200}", flush=True)
            assert process.returncode not in (2, 3), "private driver input or cleanup failed"
            assert success == expected, "selected read-only JWT incorrectly authorized a fresh native upload"
            assert (after.status_code == 200) == expected, "native inventory contradicts configured writer authority"
            if expected:
                nar = next(line[5:] for line in after.text.splitlines() if line.startswith("URL: "))
                assert requests.get(BASE + cache + "/" + nar, headers=read_headers, timeout=10).status_code == 200
            if not parallel:
                assert not list(Path("/tmp").glob("cf-cache-*")), "publication left protected files"

        for driver in ["attic-publication", "attic-server-publication"]:
            for identity, expected in [(4701, True), (4702, False), (4703, True), (4704, False), (4705, False)]:
                attempt(identity, expected, driver)
            attempt(4701, True, driver, conflict=True)
            attempt(4701, True, driver, alias=True)
            attempt(4702, False, driver, retries=1)
            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
                futures = [executor.submit(attempt, identity, True, driver, parallel=True) for identity in (4701, 4703)]
                for future in futures:
                    future.result()
            assert not list(Path("/tmp").glob("cf-cache-*"))
            for lifecycle in ["timeout", "cancel"]:
                try:
                    attempt(4701, False, driver, lifecycle=lifecycle)
                finally:
                    command(["systemctl", "kill", "--signal=CONT", "atticd.service"])
            assert (home / "config.toml").read_bytes() == stale_config, "selected publication changed persistent CLI config"
            # Static compatibility deliberately retains login and ambient write
            # authority. Its native login may rewrite persistent config.
            attempt(4702, True, driver, static=True)
            stale_config = (home / "config.toml").read_bytes()

        # Chunk publication must keep every explicit input, including a .drv
        # root, under selected credentials. This exercises the shared preparation
        # and real native CLI, not the private HTTP requisite-dispatch function.
        for identity, expected, ignore in [(4701, True, False), (4702, False, True),
                                           (4703, True, True), (4704, False, False)]:
            push = dict(db.execute("SELECT config FROM publication_configs WHERE id=%s", (identity,)).fetchone()[0])
            push["attic_ignore_upstream_cache_filter"] = ignore
            cache = push["attic_cache_name"]
            paths = []
            for _ in range(2):
                source = Path("/run/bulk-publication-" + uuid.uuid4().hex)
                source.write_bytes(os.urandom(8192))
                paths.append(command(["nix-store", "--add", str(source)]))
            name = "bulk-root-" + uuid.uuid4().hex
            paths.append(command(["nix-instantiate", "--expr",
                                  'builtins.derivation { name = "' + name
                                  + '"; system = builtins.currentSystem; builder = "/bin/sh"; args = []; }']))
            assert paths[-1].endswith(".drv"), "native bulk fixture did not produce a derivation root"
            read_headers = {"Authorization": "Bearer " + writer}
            metadata_urls = [BASE + cache + "/" + Path(path).name.split("-")[0] + ".narinfo" for path in paths]
            assert all(requests.get(url, headers=read_headers, timeout=10).status_code == 404 for url in metadata_urls), "bulk path was already cached"
            result = subprocess.run(["attic-bulk-publication"],
                                    input=json.dumps(dict(push=push, paths=paths)).encode(),
                                    env=dict(os.environ, ATTIC_TOKEN=writer,
                                             ATTIC_SERVER_URL="http://127.0.0.1:9/",
                                             ATTIC_REMOTE_NAME="ambient-stale"), capture_output=True, timeout=120)
            assert result.returncode == (0 if expected else 1), "bulk selected publication outcome was incorrect"
            assert b"bulk_args_verified=true token_in_argv=false" in result.stdout, "bulk native argument verification failed"
            inventory = [requests.get(url, headers=read_headers, timeout=10) for url in metadata_urls]
            assert all((response.status_code == 200) == expected for response in inventory), "bulk publication omitted or improperly authorized a root"
            if expected:
                for response in inventory:
                    nar = next(line[5:] for line in response.text.splitlines() if line.startswith("URL: "))
                    assert requests.get(BASE + cache + "/" + nar, headers=read_headers, timeout=10).status_code == 200
            assert not list(Path("/tmp").glob("cf-cache-*")), "bulk publication left protected files"
            print(f"bulk_driver_exit={result.returncode} explicit_paths={len(paths)} native_inventory_count={sum(response.status_code == 200 for response in inventory)}", flush=True)

        assert db.execute("SELECT * FROM publication_configs ORDER BY id").fetchall() == before_rows
        assert (home / "config.toml").read_bytes() == stale_config, "selected publication changed persistent CLI config"
        observations = [json.loads(line) for line in Path("/var/log/nginx/publication.json").read_text().splitlines()]
        assert any(row["method"] == "PUT" and row["authorization"] and row["status"] == 200 for row in observations)
        assert any(row["authorization"] and row["status"] in (401, 403) for row in observations)
        print("native_authorized_upload_observed=true native_denial_observed=true stored_config_unchanged=true", flush=True)
    print("native publication assertions passed", flush=True)


main()
