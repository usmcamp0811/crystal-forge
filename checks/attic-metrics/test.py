"""Verify native inventory inside disposable guests without exporting secrets.

Only named stages, public fixture totals and request counts leave the guests.
Native CLI output, SQL errors, raw snapshots, JWTs and cookies stay private.
"""

import hashlib
import json
import os
from pathlib import Path
import pwd
import secrets
import subprocess
import sys
import time
from urllib.parse import quote

import psycopg
from psycopg import sql
import requests

ROOT = Path("/run/attic-metrics-test")
TOKENS = {name: letter * 64 for name, letter in (
    ("populated", "a"), ("empty", "b"), ("secondary", "c"),
    ("missing", "d"), ("deleted", "e"), ("shared", "f"))}
# Exact inclusive token boundaries use both URL-safe punctuation characters.
# These public synthetic values remain in protected runtime fixture files.
TOKENS.update({"Team+1": "Z_-9" * 256, "minimum": "A_-0" * 8})
CA = "/etc/ssl/certs/ca-certificates.crt"
STAGE = "startup"


def private(path, text):
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as output:
        output.write(text)


def run(command, env=None, timeout=120):
    result = subprocess.run(command, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, timeout=timeout, check=False)
    assert result.returncode == 0, "private subprocess failed"
    return result.stdout.decode().strip()


def db(name):
    return psycopg.connect(dbname=name, user="postgres", host="/run/postgresql", autocommit=True)


def client():
    result = requests.Session()
    result.trust_env = False
    result.verify = CA
    return result


def request(session, method, url, **kwargs):
    return session.request(method, url, timeout=10, allow_redirects=False, **kwargs)


def digest(connection, tables):
    """Hash complete sorted rows privately, including future schema columns."""
    result = {}
    for table in tables:
        rows = connection.execute(sql.SQL("SELECT to_jsonb(t) FROM {} t").format(
            sql.Identifier(table))).fetchall()
        encoded = sorted(json.dumps(row[0], sort_keys=True, default=str) for row in rows)
        result[table] = hashlib.sha256(json.dumps(encoded).encode()).hexdigest()
    return result


def attic_snapshot(connection):
    return digest(connection, ["cache", "object", "nar", "chunk", "chunkref"])


def cf_snapshot(connection):
    return digest(connection, ["cache_destinations", "cache_destination_environments", "cache_push_jobs"])


def attic_setup():
    ROOT.mkdir(mode=0o700, exist_ok=True)
    native = client()
    deadline = time.monotonic() + 90
    while True:
        with db("attic") as connection:
            ready = connection.execute("SELECT to_regclass('public.object') IS NOT NULL").fetchone()[0]
        if ready:
            break
        assert time.monotonic() < deadline, "native migrations not ready"
        time.sleep(1)
    jwt = run(["atticadm", "--config", "/etc/atticd.toml", "make-token",
               "--sub", "inventory-fixture", "--validity", "1d", "--pull", "*",
               "--push", "*", "--create-cache", "*"])
    for name in ("populated", "empty", "secondary", "deleted", "shared", "Team+1", "minimum"):
        response = request(native, "POST", "http://127.0.0.1:8080/_api/v1/cache-config/" + quote(name, safe=""),
                           headers={"Authorization": "Bearer " + jwt},
                           json={"keypair": "Generate", "is_public": False,
                                 "store_dir": "/nix/store", "priority": 40,
                                 "upstream_cache_key_names": []})
        assert response.status_code == 200, "native cache creation failed"
    private(ROOT / "token", jwt + "\n")
    private(ROOT / "config/attic/config.toml",
            '[servers.native]\nendpoint = "http://127.0.0.1:8080/"\ntoken-file = ' +
            json.dumps(str(ROOT / "token")) + '\n')
    env = dict(os.environ, HOME=str(ROOT), XDG_CONFIG_HOME=str(ROOT / "config"))
    paths = [Path("/etc/publication-" + part).read_text().strip() for part in ("first", "second")]
    run(["attic", "login", "native", "http://127.0.0.1:8080/"], env)
    run(["attic", "push", "native:populated", *paths], env)
    run(["attic", "push", "native:secondary", paths[0]], env)
    run(["attic", "push", "native:Team+1", paths[0]], env)
    run(["attic", "push", "native:minimum", paths[0]], env)
    # Native CLI proves genuine chunk/NAR publication. Clone one object's
    # metadata under a fresh hash to model two paths sharing that actual NAR.
    # This avoids assuming the client supports cross-path DAG registration.
    with db("attic") as connection:
        rows = connection.execute("""
            SELECT o.id, o.nar_id, n.nar_size FROM object o JOIN nar n ON n.id=o.nar_id
            JOIN cache c ON c.id=o.cache_id WHERE c.name=%s ORDER BY o.id
        """, ("populated",)).fetchall()
        assert len(rows) == 2 and rows[0][1] != rows[1][1], "need two distinct native NARs"
        assert rows[0][2] == rows[1][2] and rows[0][2] > 0, "need equal native NAR sizes"
        columns = [r[0] for r in connection.execute("""
            SELECT column_name FROM information_schema.columns
            WHERE table_schema='public' AND table_name='object' AND column_name <> 'id'
            ORDER BY ordinal_position
        """)]
        expressions = [sql.Placeholder() if col in ("store_path_hash", "store_path")
                       else sql.Identifier(col) for col in columns]
        connection.execute(sql.SQL("INSERT INTO object ({}) SELECT {} FROM object WHERE id=%s").format(
            sql.SQL(",").join(map(sql.Identifier, columns)), sql.SQL(",").join(expressions)),
            ("0" * 32, "/nix/store/" + "0" * 32 + "-fixture-shared-nar", rows[0][0]))
        shared_id, = connection.execute("SELECT id FROM cache WHERE name=%s", ("shared",)).fetchone()
        shared_expressions = [sql.Placeholder() if col in ("cache_id", "store_path_hash", "store_path")
                              else sql.Identifier(col) for col in columns]
        for marker in ("1", "2"):
            connection.execute(sql.SQL("INSERT INTO object ({}) SELECT {} FROM object WHERE id=%s").format(
                sql.SQL(",").join(map(sql.Identifier, columns)), sql.SQL(",").join(shared_expressions)),
                (shared_id, marker * 32, "/nix/store/" + marker * 32 + "-fixture-shared-nar", rows[0][0]))
        connection.execute("UPDATE cache SET deleted_at=NOW() WHERE name=%s", ("deleted",))
        connection.execute("CREATE ROLE cf_attic_metrics LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS")
        password = secrets.token_hex(32)
        connection.execute(sql.SQL("ALTER ROLE cf_attic_metrics PASSWORD {}").format(sql.Literal(password)))
        for database, in connection.execute("SELECT datname FROM pg_database WHERE NOT datistemplate"):
            connection.execute(sql.SQL("REVOKE CONNECT, TEMPORARY ON DATABASE {} FROM PUBLIC").format(sql.Identifier(database)))
        connection.execute("REVOKE ALL ON SCHEMA public FROM PUBLIC")
        # PUBLIC revocation also removes the native service's inherited access.
        # Keep Attic operational with explicit grants to its existing role.
        connection.execute("GRANT CONNECT ON DATABASE attic TO attic")
        connection.execute("GRANT USAGE ON SCHEMA public TO attic")
        connection.execute("GRANT CONNECT ON DATABASE attic TO cf_attic_metrics")
        connection.execute("GRANT USAGE ON SCHEMA public TO cf_attic_metrics")
        connection.execute("GRANT SELECT (id,name,deleted_at) ON cache TO cf_attic_metrics")
        connection.execute("GRANT SELECT (cache_id,nar_id) ON object TO cf_attic_metrics")
        connection.execute("GRANT SELECT (id,nar_size,state) ON nar TO cf_attic_metrics")
        connection.execute("ALTER ROLE cf_attic_metrics SET default_transaction_read_only=on")
        private(ROOT / "baseline-attic", json.dumps(attic_snapshot(connection)))
        private(ROOT / "native-nar", json.dumps({"id": rows[0][1], "size": rows[0][2]}))
    secret_dir = Path("/run/attic-metrics-secrets")
    private(secret_dir / "database", "postgresql://cf_attic_metrics:" + password + "@127.0.0.1/attic")
    private(secret_dir / "tokens", json.dumps(TOKENS))
    # Remove writer credentials before observation. No metric operation can
    # invoke upload using retained fixture configuration.
    (ROOT / "token").unlink()
    (ROOT / "config/attic/config.toml").unlink()


def expected():
    """Sum independent bound row reads in Python, rather than adapter SQL."""
    result = {}
    with db("attic") as connection:
        for name in ("populated", "empty", "secondary", "shared", "Team+1", "minimum"):
            cache_id, = connection.execute("SELECT id FROM cache WHERE name=%s AND deleted_at IS NULL", (name,)).fetchone()
            rows = connection.execute("""
                SELECT n.nar_size FROM object o JOIN nar n ON o.nar_id=n.id
                WHERE o.cache_id=%s AND n.state=%s ORDER BY o.id
            """, (cache_id, "V")).fetchall()
            result[name] = {"count": len(rows), "bytes": sum(row[0] for row in rows)}
    assert result["empty"] == {"count": 0, "bytes": 0}
    assert result["populated"]["count"] == 3
    assert result["populated"]["bytes"] == result["secondary"]["bytes"] * 3
    assert result["secondary"]["count"] == 1
    assert result["shared"]["count"] == 2
    assert result["shared"]["bytes"] == result["secondary"]["bytes"] * 2
    assert result["Team+1"] == result["secondary"]
    assert result["minimum"] == result["secondary"]
    return result


def adapter(name, token=None, headers=None):
    values = {} if token is None else {"Authorization": "Bearer " + token}
    values.update(headers or {})
    return request(client(), "GET", "http://127.0.0.1:9121/v1/caches/" + quote(name, safe="") + "/inventory", headers=values)


def attic_verify():
    assert len(TOKENS["minimum"]) == 32 and len(TOKENS["Team+1"]) == 1024
    for name, totals in expected().items():
        response = adapter(name, TOKENS[name])
        assert response.status_code == 200, "native adapter read failed"
        assert response.json() == {"schema_version": 1, "cache": name,
                                   "cached_path_count": totals["count"], "logical_nar_bytes": totals["bytes"]}
        if name == "Team+1":
            assert response.request.url.endswith("/Team%2B1/inventory"), "plus cache was not encoded as a path segment"
    for name, token, status, error in (
            ("populated", None, 401, "unauthorized"),
            ("populated", "z" * 64, 401, "unauthorized"),
            ("secondary", TOKENS["populated"], 401, "unauthorized"),
            ("missing", TOKENS["missing"], 404, "not_found"),
            ("deleted", TOKENS["deleted"], 404, "not_found")):
        response = adapter(name, token)
        assert response.status_code == status and response.json() == {"error": error}
    assert adapter("populated", headers={"X-Forwarded-Authorization": "Bearer " + TOKENS["populated"]}).status_code == 401
    # Remove a required grant temporarily to prove token denial occurs before
    # querying the database. Correct auth now gets 503, wrong auth stays 401.
    with db("attic") as connection:
        connection.execute("REVOKE SELECT (id,nar_size,state) ON nar FROM cf_attic_metrics")
        try:
            assert adapter("populated", TOKENS["populated"]).status_code == 503
            assert adapter("populated", "z" * 64).status_code == 401
            assert adapter("populated").status_code == 401
            for token in ["x" * 31, "x" * 1025] + ["x" * 31 + char for char in ".+~=/"]:
                response = adapter("populated", token)
                assert response.status_code == 401 and response.json() == {"error": "unauthorized"}, \
                    "invalid boundary or alphabet reached database"
            # Exact spelling remains part of token scope even for an otherwise
            # valid maximum-length credential and a URL-encoded plus name.
            response = adapter("team+1", TOKENS["Team+1"])
            assert response.status_code == 401 and response.json() == {"error": "unauthorized"}
        finally:
            connection.execute("GRANT SELECT (id,nar_size,state) ON nar TO cf_attic_metrics")
    allowed = {"cache": {"id", "name", "deleted_at"}, "object": {"cache_id", "nar_id"},
               "nar": {"id", "nar_size", "state"}}
    with db("attic") as connection:
        role = connection.execute("""SELECT rolsuper,rolcreatedb,rolcreaterole,rolinherit,rolreplication,rolbypassrls
                                     FROM pg_roles WHERE rolname='cf_attic_metrics'""").fetchone()
        assert role == (False,) * 6
        assert connection.execute("SELECT COUNT(*) FROM pg_auth_members WHERE member=(SELECT oid FROM pg_roles WHERE rolname='cf_attic_metrics')").fetchone()[0] == 0
        columns = connection.execute("""SELECT table_name,column_name FROM information_schema.columns
                                        WHERE table_schema='public'""").fetchall()
        for table, column in columns:
            granted, = connection.execute("SELECT has_column_privilege(%s,%s,%s,'SELECT')",
                                          ("cf_attic_metrics", "public." + table, column)).fetchone()
            assert granted == (column in allowed.get(table, set())), "unexpected column privilege"
        for table in {row[0] for row in columns}:
            for privilege in ("SELECT", "INSERT", "UPDATE", "DELETE", "TRUNCATE", "REFERENCES", "TRIGGER"):
                granted, = connection.execute("SELECT has_table_privilege(%s,%s,%s)",
                                              ("cf_attic_metrics", "public." + table, privilege)).fetchone()
                assert not granted, "unexpected table privilege"
        assert not connection.execute("SELECT has_schema_privilege('cf_attic_metrics','public','CREATE')").fetchone()[0]
        for database in ("postgres", "unrelated"):
            assert not connection.execute("SELECT has_database_privilege('cf_attic_metrics',%s,'CONNECT')", (database,)).fetchone()[0]
        # Planner syntax and permission resolution must work under the actual
        # restricted role. No ANALYZE, paths, SQL text or plan is exported.
        connection.execute("SET ROLE cf_attic_metrics")
        plan = connection.execute("""EXPLAIN (FORMAT JSON) SELECT c.name,COUNT(n.id),SUM(n.nar_size)
                                  FROM cache c LEFT JOIN object o ON o.cache_id=c.id
                                  LEFT JOIN nar n ON n.id=o.nar_id AND n.state='V'
                                  WHERE c.name=%s AND c.deleted_at IS NULL GROUP BY c.name""", ("populated",)).fetchone()
        assert plan
        connection.execute("RESET ROLE")
    uri = Path("/run/attic-metrics-secrets/database").read_text()
    for statement in ("SELECT keypair FROM cache", "SELECT store_path FROM object",
                      "INSERT INTO cache(name) VALUES ('forbidden')", "UPDATE nar SET nar_size=0", "DELETE FROM object"):
        with psycopg.connect(uri, autocommit=True) as restricted:
            assert restricted.execute("SHOW default_transaction_read_only").fetchone()[0] == "on"
            try:
                restricted.execute(statement)
            except psycopg.Error:
                pass
            else:
                raise AssertionError("restricted operation unexpectedly allowed")
    # Address proof complements the firewall rules; no externally bound adapter
    # or database listener can bypass the private TLS entry point.
    listeners = run(["ss", "-H", "-ltn"])
    for port in (8080, 9121, 5432):
        matching = [line for line in listeners.splitlines() if ":" + str(port) + " " in line]
        assert matching and all("127.0.0.1:" + str(port) in line for line in matching)


def cf_tokens():
    directory = Path("/run/cf-metrics-tokens")
    directory.mkdir(mode=0o700, exist_ok=True)
    user = pwd.getpwnam("crystal-forge")
    os.chown(directory, user.pw_uid, user.pw_gid)
    for name, token in dict(TOKENS, wrong="z" * 64, **{"root-owned": "a" * 64}).items():
        path = directory / name
        private(path, token)
        if name != "root-owned":
            os.chown(path, user.pw_uid, user.pw_gid)


def module_credentials():
    """Check the actual module's sources and runtime copies without disclosure."""
    import stat
    import shlex
    unit = "cf-attic-metrics.service"
    properties = {}
    for name in ("DynamicUser", "RuntimeDirectory", "RuntimeDirectoryMode",
                 "RuntimeDirectoryPreserve", "ExecStartPre", "ExecStart"):
        properties[name] = run(["systemctl", "show", unit, "-p", name, "--value"])
    # systemctl show does not expose LoadCredential source bindings in the
    # unit-file colon syntax. Inspect exact installed directives privately.
    installed = run(["systemctl", "cat", unit])
    bindings = [binding for line in installed.splitlines() if line.startswith("LoadCredential=")
                for binding in shlex.split(line.split("=", 1)[1])]
    assert properties["DynamicUser"] == "yes", "module did not enable DynamicUser"
    assert properties["RuntimeDirectory"] == "cf-attic-metrics", "unexpected runtime directory setting"
    assert properties["RuntimeDirectoryMode"] == "0700", "unexpected runtime directory mode setting"
    assert properties["RuntimeDirectoryPreserve"] == "no", "runtime directory configured for preservation"
    for name, source in (("database-uri", "database"), ("tokens", "tokens")):
        assert name + ":/run/attic-metrics-secrets/" + source in bindings, "unexpected credential source binding"
        assert "install -m0600" in properties["ExecStartPre"], "missing owner-only credential installation"
        assert "/run/cf-attic-metrics/" + name in properties["ExecStartPre"], "unexpected credential installation destination"
        assert "/run/cf-attic-metrics/" + name in properties["ExecStart"], "adapter does not read runtime copies"
    pid = int(run(["systemctl", "show", unit, "-p", "MainPID", "--value"]))
    assert pid > 0, "module process not running"
    # Read only process UID evidence. Do not inspect argv or environment or
    # export process status contents; credentials never enter diagnostics.
    uid_line = next(line for line in Path(f"/proc/{pid}/status").read_text().splitlines()
                    if line.startswith("Uid:"))
    effective_uid = int(uid_line.split()[2])
    assert effective_uid != 0, "adapter process ran as root"
    runtime = Path("/run/cf-attic-metrics")
    info = runtime.stat()
    assert stat.S_ISDIR(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o700, "runtime directory is not mode 0700"
    assert info.st_uid == effective_uid, "runtime directory owner differs from process UID"
    sources = Path("/run/attic-metrics-secrets")
    assert sources.stat().st_uid == 0 and stat.S_IMODE(sources.stat().st_mode) == 0o700, "source directory is not root-owned mode 0700"
    for name, source in (("database-uri", "database"), ("tokens", "tokens")):
        copied, original = runtime / name, sources / source
        info, original_info = copied.lstat(), original.lstat()
        assert stat.S_ISREG(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o600, "runtime credential is not regular mode 0600"
        assert info.st_uid == effective_uid, "runtime credential owner differs from process UID"
        assert stat.S_ISREG(original_info.st_mode) and stat.S_IMODE(original_info.st_mode) == 0o600, "source credential is not regular mode 0600"
        assert original_info.st_uid == 0, "LoadCredential source is not root-owned"
        assert copied.read_bytes() == original.read_bytes(), "module copied different credential bytes"


def module_lifecycle():
    """Prove restart reloads credentials/scopes and stop removes runtime copies."""
    unit = "cf-attic-metrics.service"
    source = Path("/run/attic-metrics-secrets/tokens")
    original = source.read_bytes()
    changed = dict(TOKENS, empty="g" * 64, restartscope="h" * 64)
    changed.pop("secondary")
    temporary = source.with_suffix(".next")
    private(temporary, json.dumps(changed))
    temporary.replace(source)
    try:
        # The running adapter has a restart-scoped snapshot. The old token is
        # still effective and a newly configured scope is not active yet.
        assert adapter("empty", TOKENS["empty"]).status_code == 200
        assert adapter("empty", changed["empty"]).status_code == 401
        assert adapter("restartscope", changed["restartscope"]).status_code == 401
        run(["systemctl", "restart", unit])
        wait_adapter()
        module_credentials()
        assert adapter("empty", TOKENS["empty"]).status_code == 401
        response = adapter("empty", changed["empty"])
        assert response.status_code == 200 and response.json() == {
            "schema_version": 1, "cache": "empty", "cached_path_count": 0, "logical_nar_bytes": 0}
        assert adapter("secondary", TOKENS["secondary"]).status_code == 401
        response = adapter("restartscope", changed["restartscope"])
        # A new, nonexistent cache scope must pass token auth and then receive
        # the native existence outcome, not the unknown-scope 401 outcome.
        assert response.status_code == 404 and response.json() == {"error": "not_found"}
    finally:
        private(temporary, original.decode())
        temporary.replace(source)
        run(["systemctl", "restart", unit])
        wait_adapter()
    module_credentials()
    for name, totals in expected().items():
        response = adapter(name, TOKENS[name])
        assert response.status_code == 200 and response.json() == {
            "schema_version": 1, "cache": name, "cached_path_count": totals["count"],
            "logical_nar_bytes": totals["bytes"]}
    final_attic()
    run(["systemctl", "stop", unit])
    runtime = Path("/run/cf-attic-metrics")
    assert not runtime.exists() and not runtime.is_symlink(), "stop retained runtime directory"
    assert not (runtime / "tokens").exists() and not (runtime / "database-uri").exists()
    assert run(["systemctl", "show", unit, "-p", "MainPID", "--value"]) == "0"
    assert source.read_bytes() == original, "root token source not restored"


def wait_adapter():
    """Wait at most 30 seconds for a real authenticated observation after restart."""
    deadline = time.monotonic() + 30
    while True:
        try:
            response = adapter("populated", TOKENS["populated"])
            if response.status_code == 200:
                return
        except requests.RequestException:
            pass
        assert time.monotonic() < deadline, "module restart readiness failed"
        time.sleep(0.1)


def cf_api(role="admin"):
    session = client()
    password = json.loads((ROOT / "users").read_text())[role]
    response = request(session, "POST", "http://127.0.0.1:8000/api/auth/local/login",
                       json={"username": "inventory-" + role, "password": password})
    assert response.status_code == 200, "CF login failed"
    # Reproduce the browser's trusted loopback Secure cookie handling.
    session.headers["Cookie"] = "; ".join(c.name + "=" + c.value for c in session.cookies)
    session.headers["X-CSRF-Token"] = next(c.value for c in session.cookies if "csrf" in c.name)
    return session


def cf_setup():
    ROOT.mkdir(mode=0o700, exist_ok=True)
    session = client()
    deadline = time.monotonic() + 120
    while True:
        try:
            response = request(session, "GET", "http://127.0.0.1:8000/api/auth/setup-status")
            if response.status_code == 200:
                break
        except requests.RequestException:
            pass
        assert time.monotonic() < deadline, "CF API readiness failed"
        time.sleep(1)
    users = {role: secrets.token_hex(24) for role in ("admin", "viewer", "unassigned")}
    for role, password in users.items():
        response = request(session, "POST", "http://127.0.0.1:8000/api/auth/local/register",
                           json={"username": "inventory-" + role, "password": password,
                                 "email": role + "@inventory.test"})
        assert response.status_code == 201, "CF registration failed"
    private(ROOT / "users", json.dumps(users))
    with db("crystal_forge") as connection:
        connection.execute("INSERT INTO user_role_assignments(user_id,role) SELECT id,'viewer' FROM users WHERE username=%s", ("inventory-viewer",))
        for destination_id in range(4701, 4717):
            name = {4702: "empty", 4703: "secondary", 4707: "missing", 4708: "deleted", 4714: "shared",
                    4715: "Team+1", 4716: "minimum"}.get(destination_id, "populated")
            # Corrupt inactive upload ciphertext is intentional. Metrics must
            # not decrypt/reuse Attic upload credentials or its configured URL.
            connection.execute("""INSERT INTO cache_destinations
                (id,name,cache_type,push_to,enabled,attic_cache_name,attic_token)
                VALUES (%s,%s,'Attic',%s,%s,%s,%s)""",
                               (destination_id, "inventory-" + str(destination_id),
                                "https://unused.invalid/", destination_id != 4704, name,
                                "enc:v1:invalid-inactive-upload-ciphertext"))
        private(ROOT / "baseline-cf", json.dumps(cf_snapshot(connection)))
    # Ensure the actual generated configuration carries the ID bindings. No
    # host DSN or runtime override provides an alternate source of mappings.
    pid = run(["systemctl", "show", "crystal-forge-server", "-p", "MainPID", "--value"])
    environment = Path("/proc/" + pid + "/environ").read_bytes().split(b"\0")
    config_path = next(value.split(b"=", 1)[1].decode() for value in environment
                       if value.startswith(b"CRYSTAL_FORGE_CONFIG="))
    import tomllib
    loaded = tomllib.loads(Path(config_path).read_text())
    assert len(loaded["server"]["attic_metrics_endpoints"]) == 15
    assert loaded["server"]["allow_private_cache_test_targets"] is True


def metrics(session, destination_id):
    return request(session, "GET", "http://127.0.0.1:8000/api/v1/caches/" + str(destination_id) + "/metrics")


def absent(response, status, reason):
    assert response.status_code == 200 and response.headers.get("Cache-Control") == "no-store"
    value = response.json()
    assert value["status"] == status and value["reason_code"] == reason, "wrong static metric outcome"
    assert set(value) == {"status", "reason_code", "storage_bytes", "storage_bytes_basis", "object_count",
                          "object_count_basis", "path_count", "measured_at"}
    assert all(value[field] is None for field in set(value) - {"status", "reason_code"}), "unknown totals became values"


def cf_verify(totals):
    import socket
    for port in (8080, 9121, 5432):
        try:
            with socket.create_connection(("attic-metrics.test", port), timeout=1):
                pass
        except OSError:
            pass
        else:
            raise AssertionError("CF reached a private native origin")
    session = cf_api()
    for destination_id, name in ((4701, "populated"), (4702, "empty"), (4703, "secondary"),
                                 (4714, "shared"), (4715, "Team+1"), (4716, "minimum")):
        response = metrics(session, destination_id)
        assert response.status_code == 200 and response.headers.get("Cache-Control") == "no-store"
        value = response.json()
        assert value["status"] == "available" and value["reason_code"] == "attic_inventory", \
            "CF native inventory unavailable for fixture " + name
        assert value["storage_bytes"] == totals[name]["bytes"]
        assert value["object_count"] == value["path_count"] == totals[name]["count"]
        assert value["storage_bytes_basis"] == "uncompressed_nar_bytes"
        assert value["object_count_basis"] == "cached_nix_paths"
        from datetime import datetime
        assert datetime.fromisoformat(value["measured_at"].replace("Z", "+00:00")).tzinfo
        assert set(value) == {"status", "reason_code", "storage_bytes", "storage_bytes_basis", "object_count",
                              "object_count_basis", "path_count", "measured_at"}, "inventory leaked extra fields"
    for destination_id, status, reason in (
            (4706, "unavailable", "access_denied"), (4707, "unavailable", "endpoint_unavailable"),
            (4708, "unavailable", "endpoint_unavailable"), (4709, "error", "invalid_stats"),
            (4710, "error", "invalid_stats"), (4711, "error", "invalid_stats"),
            (4712, "unavailable", "access_denied"), (4713, "error", "invalid_configuration")):
        absent(metrics(session, destination_id), status, reason)


def cf_gates():
    admin, viewer, unassigned = cf_api(), cf_api("viewer"), cf_api("unassigned")
    # Remove the selected token to discriminate auth-before-file from a network
    # failure. Admin sees configuration failure; Viewer still gets its gate.
    path = Path("/run/cf-metrics-tokens/populated")
    path.rename(path.with_suffix(".held"))
    try:
        absent(metrics(admin, 4701), "error", "invalid_configuration")
        absent(metrics(viewer, 4701), "unavailable", "requires_admin")
        absent(metrics(admin, 4704), "unavailable", "disabled")
        absent(metrics(viewer, 4704), "unavailable", "disabled")
        absent(metrics(admin, 4705), "unsupported", "provider_totals_unsupported")
        absent(metrics(viewer, 4705), "unsupported", "provider_totals_unsupported")
        assert metrics(client(), 4701).status_code == 401
        assert metrics(client(), 999999).status_code == 401
        assert metrics(unassigned, 4701).status_code == 403
        assert metrics(unassigned, 999999).status_code == 403
        assert metrics(admin, 999999).status_code == 404
    finally:
        path.with_suffix(".held").rename(path)
    # CF must use the same exact, untrimmed alphabet as the provider. Updating
    # bytes in place preserves the file's service ownership and mode, isolating
    # token validation from filesystem policy. Provider counts stay unchanged.
    boundary_path = Path("/run/cf-metrics-tokens/minimum")
    original = boundary_path.read_bytes()
    try:
        for invalid in (original + b"\n", original[:-1] + b".", original[:-1] + b"+",
                        b"x" * 31, b"x" * 1025):
            boundary_path.write_bytes(invalid)
            absent(metrics(admin, 4716), "error", "invalid_configuration")
    finally:
        boundary_path.write_bytes(original)


def corrupt(restore=False):
    value = json.loads((ROOT / "native-nar").read_text())
    with db("attic") as connection:
        connection.execute("UPDATE nar SET nar_size=%s WHERE id=%s", (value["size"] if restore else -1, value["id"]))
    if not restore:
        response = adapter("populated", TOKENS["populated"])
        assert response.status_code == 503 and response.json() == {"error": "unavailable"}, "corrupt size must not become zero"


def final_attic():
    with db("attic") as connection:
        assert attic_snapshot(connection) == json.loads((ROOT / "baseline-attic").read_text()), "inventory mutated native database"
    records = [json.loads(line) for line in Path("/var/log/nginx/metrics-safe.log").read_text().splitlines()]
    assert records and any(row["token"] for row in records), "CF did not use separate adapter token"
    assert all(not row["cookie"] and not row["csrf"] for row in records), "CF session header leaked to provider"
    # Native and adapter journals must contain no private runtime credentials.
    secret = Path("/run/attic-metrics-secrets/database").read_text()
    journal = run(["journalctl", "-u", "cf-attic-metrics", "-u", "atticd", "--no-pager"])
    assert secret not in journal and all(token not in journal for token in TOKENS.values())


def final_cf():
    with db("crystal_forge") as connection:
        assert cf_snapshot(connection) == json.loads((ROOT / "baseline-cf").read_text()), "metrics persisted destination/scope/job state"
        assert connection.execute("SELECT COUNT(*) FROM pg_database WHERE datname='attic'").fetchone()[0] == 0
    # The CF guest has no adapter DB URI, JWT or Attic signing key file.
    assert not Path("/run/attic-metrics-secrets").exists()


def main():
    global STAGE
    STAGE = sys.argv[1]
    commands = {
        "attic-setup": attic_setup, "attic-verify": attic_verify, "cf-tokens": cf_tokens,
        "module-credentials": module_credentials, "module-lifecycle": module_lifecycle,
        "cf-setup": cf_setup, "cf-gates": cf_gates, "corrupt": corrupt,
        "restore": lambda: corrupt(True), "cf-corrupt": lambda: absent(metrics(cf_api(), 4701), "error", "unexpected_status"),
        "final-attic": final_attic, "final-cf": final_cf,
    }
    if STAGE == "expected":
        print(json.dumps(expected()))
    elif STAGE == "requests":
        print(len(Path("/var/log/nginx/metrics-safe.log").read_text().splitlines()))
    elif STAGE == "cf-verify":
        cf_verify(json.loads(sys.argv[2]))
        print(STAGE + ": passed")
    else:
        commands[STAGE]()
        print(STAGE + ": passed")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # Never print exception values, tracebacks, URLs, query text or bodies.
        # Fixed assertion messages are safe; transport/database exceptions are
        # identified by class only. The stage gives a bounded repair boundary.
        message = str(error) if isinstance(error, AssertionError) else type(error).__name__
        print(STAGE + ": failed: " + message, file=sys.stderr)
        sys.exit(1)
