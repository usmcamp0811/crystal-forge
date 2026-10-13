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
TOKENS.update({"lifecycle-public": "i" * 64, "lifecycle-private": "j" * 64})
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
    ROOT.mkdir(mode=0o700, exist_ok=True)
    private(ROOT / "cf-key.env", "CRYSTAL_FORGE_SECRET_KEY=" + secrets.token_hex(32) + "\n")
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
    assert len(loaded["server"]["attic_metrics_endpoints"]) == 22
    assert loaded["server"]["allow_private_cache_test_targets"] is True


def metrics(session, destination_id):
    return request(session, "GET", "http://127.0.0.1:8000/api/v1/caches/" + str(destination_id) + "/metrics")


def absent(response, status, reason, destination_id):
    """Checks absent totals and reports only validated outcome labels on mismatch.

    Invalid JSON and non-object responses remain unknown. Provider response
    bytes, headers and exception text never enter the diagnostic boundary.
    """
    assert type(destination_id) is int, "metric fixture ID must be numeric"
    statuses = {"available", "unsupported", "unavailable", "error"}
    reasons = {"native_stats", "attic_inventory", "disabled", "requires_admin",
               "provider_totals_unsupported", "unsupported_cache_type",
               "destination_unavailable", "invalid_configuration",
               "transport_unavailable", "response_unavailable", "invalid_stats",
               "stats_too_large", "endpoint_unavailable", "access_denied",
               "unexpected_status", "deadline_exceeded"}
    try:
        value = response.json()
    except ValueError:
        value = {}
    observed = value if type(value) is dict else {}
    observed_status = observed.get("status")
    observed_reason = observed.get("reason_code")
    if (response.status_code != 200 or observed_status != status
            or observed_reason != reason):
        evidence = {"fixture_id": destination_id,
                    "expected_status": status if type(status) is str and status in statuses else "unknown",
                    "expected_reason": reason if type(reason) is str and reason in reasons else "unknown",
                    "observed_status": observed_status if type(observed_status) is str and observed_status in statuses else "unknown",
                    "observed_reason": observed_reason if type(observed_reason) is str and observed_reason in reasons else "unknown",
                    "http_status": response.status_code if type(response.status_code) is int and 100 <= response.status_code <= 599 else "unknown"}
        print("ATTIC_METRICS " + json.dumps(evidence, sort_keys=True), flush=True)
        raise AssertionError("wrong static metric outcome for fixture " + str(destination_id))
    assert response.headers.get("Cache-Control") == "no-store"
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
        absent(metrics(session, destination_id), status, reason, destination_id)


def cf_gates():
    admin, viewer, unassigned = cf_api(), cf_api("viewer"), cf_api("unassigned")
    # Remove the selected token to discriminate auth-before-file from a network
    # failure. Admin sees configuration failure; Viewer still gets its gate.
    path = Path("/run/cf-metrics-tokens/populated")
    path.rename(path.with_suffix(".held"))
    try:
        absent(metrics(admin, 4701), "error", "invalid_configuration", 4701)
        absent(metrics(viewer, 4701), "unavailable", "requires_admin", 4701)
        absent(metrics(admin, 4704), "unavailable", "disabled", 4704)
        absent(metrics(viewer, 4704), "unavailable", "disabled", 4704)
        absent(metrics(admin, 4705), "unsupported", "provider_totals_unsupported", 4705)
        absent(metrics(viewer, 4705), "unsupported", "provider_totals_unsupported", 4705)
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
            absent(metrics(admin, 4716), "error", "invalid_configuration", 4716)
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


NATIVE = "https://attic-metrics.test:8888/"
CF = "http://127.0.0.1:8000/api/v1/caches"


def mint(*permissions):
    return run(["atticadm", "--config", "/etc/atticd.toml", "make-token",
                "--sub", "retained-fixture", "--validity", "1d", *permissions])


def lifecycle_attic_setup():
    """Create real native identities only after the original accounting proofs."""
    admin = mint("--pull", "*", "--create-cache", "*")
    headers = {"Authorization": "Bearer " + admin}
    native = client()
    source = request(native, "GET", NATIVE + "_api/v1/cache-config/populated",
                     headers=headers)
    assert source.status_code == 200, "native source configuration unavailable"
    source = source.json()
    bundle = {}
    for kind, public in (("public", True), ("private", False)):
        name = "lifecycle-" + kind
        response = request(native, "POST", NATIVE + "_api/v1/cache-config/" + name,
                           headers=headers, json={
                               "keypair": "Generate", "is_public": public,
                               "store_dir": source["store_dir"], "priority": source["priority"],
                               "upstream_cache_key_names": source["upstream_cache_key_names"],
                           })
        assert response.status_code == 200, "native lifecycle creation failed"
        writer = mint("--pull", name, "--push", name)
        config = request(native, "GET", NATIVE + "_api/v1/cache-config/" + name,
                         headers={"Authorization": "Bearer " + writer})
        assert config.status_code == 200, "native scoped JWT failed"
        config = config.json()
        assert config["is_public"] == public and config["store_dir"] == "/nix/store"
        assert config["substituter_endpoint"] == NATIVE + name, "native advertised authority mismatch"
        bundle[kind] = {"name": name, "writer": writer, "config": config}
    bundle["reader"] = mint("--pull", "lifecycle-public")
    private(ROOT / "native-lifecycle.json", json.dumps(bundle))
    # The denied private-cache clone retains a valid JWT for a different cache.
    assert request(native, "GET", NATIVE + "_api/v1/cache-config/lifecycle-private",
                   headers={"Authorization": "Bearer " + bundle["reader"]}).status_code in (401, 403)


def raw_cf(connection):
    """Keep complete raw rows private; comparisons include ciphertext and scopes."""
    return {table: sorted(
        (row[0] for row in connection.execute(sql.SQL("SELECT to_jsonb(t) FROM {} t").format(
            sql.Identifier(table)))), key=lambda row: json.dumps(row, sort_keys=True))
        for table in ("cache_destinations", "cache_destination_environments", "cache_push_jobs")}


def assert_redacted(value, secrets_to_hide):
    assert value["attic_token_configured"] is True, "stored JWT configured flag absent"
    assert value.get("attic_token") is None, "GET exposed retained JWT"
    serialized = json.dumps(value)
    assert all(secret not in serialized for secret in secrets_to_hide), "API exposed private fixture data"


def lifecycle_cf_setup():
    """Create actual encrypted CF rows through validated Admin POST bodies."""
    import stat
    bundle_file = ROOT / "native-lifecycle.json"
    assert stat.S_IMODE(bundle_file.stat().st_mode) == 0o600
    bundle = json.loads(bundle_file.read_text())
    session = cf_api()
    # Read only the disposable service's active fixture key. The same bytes are
    # supplied to the example as a child-local environment, never in argv.
    pid = run(["systemctl", "show", "crystal-forge-server", "-p", "MainPID", "--value"])
    environment = dict(entry.split(b"=", 1) for entry in Path("/proc/" + pid + "/environ").read_bytes().split(b"\0") if b"=" in entry)
    assert b"CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY" not in environment, "fixture key precedence changed"
    key = environment[b"CRYSTAL_FORGE_SECRET_KEY"].decode()
    private(ROOT / "cf-key", key)
    protected = [key, bundle["reader"], bundle["public"]["writer"], bundle["private"]["writer"]]
    fixtures = {}
    response = request(session, "POST", "http://127.0.0.1:8000/api/v1/environments",
                       json={"name": "retained-native", "color_hex": "#334455", "is_active": True})
    assert response.status_code == 201, "native fixture environment creation failed"
    scope = response.json()["id"]
    for identity, kind, unauthorized in (
            (1, "public", False), (2, "private", False), (3, "private", True),
            (4, "private", False), (5, "private", False), (6, "private", False),
            (7, "private", False), (8, "private", False)):
        native = bundle[kind]
        body = {"name": "retained-native-" + str(identity), "cache_type": "Attic",
                "push_to": NATIVE, "enabled": True, "attic_cache_name": native["name"],
                "attic_public_key": native["config"]["public_key"],
                "attic_token": bundle["reader"] if unauthorized else native["writer"],
                "attic_ignore_upstream_cache_filter": True, "attic_jobs": 1,
                "max_retries": 0, "retry_delay_seconds": 0, "push_timeout_seconds": 30,
                "force_repush": False, "require_sigs": True, "environment_ids": [scope]}
        response = request(session, "POST", CF, json=body)
        assert response.status_code == 201, "validated CF lifecycle creation failed"
        value = response.json()
        assert value["id"] == identity, "startup metrics binding differs from created ID"
        assert_redacted(value, protected)
        # Save and Test share this complete public patch. Retained credentials
        # are omitted, not replaced with null, empty strings or decrypted GETs.
        fixtures[str(identity)] = {field: value for field, value in body.items() if field != "attic_token"}
        with db("crystal_forge") as connection:
            stored, = connection.execute("SELECT attic_token FROM cache_destinations WHERE id=%s", (identity,)).fetchone()
            assert stored.startswith("enc:v1:") and stored != body["attic_token"], "Admin POST did not encrypt native JWT"
    private(ROOT / "lifecycle-patches", json.dumps(fixtures))
    with db("crystal_forge") as connection:
        private(ROOT / "lifecycle-before", json.dumps(raw_cf(connection)))


def retained_test(session, identity, patch=None, expected=True):
    patches = json.loads((ROOT / "lifecycle-patches").read_text())
    patch = patches[str(identity)] if patch is None else patch
    assert "attic_token" not in patch, "retained Test sent credential bytes"
    response = request(session, "POST", CF + "/" + str(identity) + "/test-credentials", json=patch)
    assert response.status_code == 200, "stored-ID native Test failed"
    result = response.json()
    assert result["ok"] == expected, "native Test contradicted retained authority"
    assert result["write_auth_valid"] is None, "read Test claimed write authorization"
    if expected:
        assert result["stage"] == "complete" and result["cache_access_valid"] is True
        assert result["token_auth_valid"] == (True if identity != 1 else None)
    else:
        assert result["stage"] == "authentication" and result["cache_access_valid"] is False
    assert result.get("tested_url") is None, "native Test exposed upstream URL"


def lifecycle_readonly():
    """Exercise GET/Test/Cancel/reopen without simulating an undo write."""
    session = cf_api()
    bundle = json.loads((ROOT / "native-lifecycle.json").read_text())
    protected = [(ROOT / "cf-key").read_text(), bundle["reader"],
                 bundle["public"]["writer"], bundle["private"]["writer"]]
    with db("crystal_forge") as connection:
        before = raw_cf(connection)
        assert before == json.loads((ROOT / "lifecycle-before").read_text())
        for identity in range(1, 9):
            value = request(session, "GET", CF + "/" + str(identity))
            assert value.status_code == 200
            assert_redacted(value.json(), protected)
            retained_test(session, identity, expected=identity != 3)
            # Cancel means discard the local draft and send no PUT. Reopen is a
            # fresh GET followed by the same retained-ID native Test.
            assert raw_cf(connection) == before, "GET/Test/Cancel changed raw CF state"
            reopened = request(session, "GET", CF + "/" + str(identity))
            assert reopened.status_code == 200 and reopened.json() == value.json()
            retained_test(session, identity, expected=identity != 3)
            assert raw_cf(connection) == before, "reopen Test changed raw CF state"
        # A genuine wrong-scope replacement is tested in memory, then cancelled
        # without Save. Retained Test must still use the original writer JWT.
        draft = dict(json.loads((ROOT / "lifecycle-patches").read_text())["2"],
                     attic_token=bundle["reader"])
        response = request(session, "POST", CF + "/2/test-credentials", json=draft)
        assert response.status_code == 200 and response.json()["ok"] is False
        assert response.json()["stage"] == "authentication"
        assert raw_cf(connection) == before, "replacement Test persisted draft JWT"
        retained_test(session, 2)
        assert raw_cf(connection) == before, "replacement Cancel changed retained JWT"


def lifecycle_save():
    """Permit only Save's timestamp and assignment-row creation timestamps."""
    patches = json.loads((ROOT / "lifecycle-patches").read_text())
    session = cf_api()
    with db("crystal_forge") as connection:
        for identity in range(1, 9):
            before = raw_cf(connection)
            response = request(session, "PUT", CF + "/" + str(identity), json=patches[str(identity)])
            assert response.status_code == 200, "unchanged retained Save failed"
            after = raw_cf(connection)
            assert before["cache_push_jobs"] == after["cache_push_jobs"], "Save changed publication jobs"
            for old, new in zip(before["cache_destinations"], after["cache_destinations"], strict=True):
                if old["id"] == identity:
                    assert new["updated_at"] > old["updated_at"], "intentional Save did not advance timestamp"
                    new = dict(new, updated_at=old["updated_at"])
                assert new == old, "unchanged Save rewrote credential or configuration"
            def membership(rows):
                return sorted(json.dumps({key: value for key, value in row.items()
                    if key != "created_at" or row["cache_destination_id"] != identity}, sort_keys=True) for row in rows)
            assert membership(before["cache_destination_environments"]) == membership(after["cache_destination_environments"]), "Save changed scope membership or unrelated timestamps"
            retained_test(session, identity, expected=identity != 3)
            reopened = request(session, "GET", CF + "/" + str(identity))
            assert reopened.status_code == 200 and reopened.json()["attic_token_configured"] is True
            assert raw_cf(connection) == after, "post-Save Test/reopen persisted changes"
        private(ROOT / "lifecycle-saved-hashes", json.dumps(cf_snapshot(connection)))


def lifecycle_metrics(published=False):
    """Metrics authority is independent of the genuine retained upload JWT."""
    session = cf_api()
    with db("crystal_forge") as connection:
        before = raw_cf(connection)
        # ID 5 targets port 9 outside the native VM's permitted ingress ports.
        # The firewall silently drops the connection, so the existing eight-
        # second observation deadline expires instead of a fast TCP refusal.
        for identity, status, reason in ((4, "unsupported", "provider_totals_unsupported"),
                (5, "unavailable", "deadline_exceeded"), (6, "unavailable", "access_denied"),
                (7, "unavailable", "access_denied"), (8, "error", "invalid_configuration")):
            absent(metrics(session, identity), status, reason, identity)
            retained_test(session, identity)
        for identity in (1, 2, 3):
            value = metrics(session, identity).json()
            assert value["status"] == "available" and value["path_count"] == (1 if published else 0)
            retained_test(session, identity, expected=identity != 3)
        path = Path("/run/cf-metrics-tokens/lifecycle-private")
        original = path.read_bytes()
        try:
            path.write_bytes(b"invalid-format.")
            absent(metrics(session, 2), "error", "invalid_configuration", 2)
            retained_test(session, 2)
        finally:
            path.write_bytes(original)
        assert raw_cf(connection) == before, "metric failure or native Test rewrote retained state"


def lifecycle_grant(revoke):
    with db("attic") as connection:
        connection.execute("REVOKE SELECT (id,nar_size,state) ON nar FROM cf_attic_metrics" if revoke else
                           "GRANT SELECT (id,nar_size,state) ON nar TO cf_attic_metrics")


def lifecycle_unavailable():
    with db("crystal_forge") as connection:
        before = raw_cf(connection)
        session = cf_api()
        absent(metrics(session, 2), "error", "unexpected_status", 2)
        retained_test(session, 2)
        assert raw_cf(connection) == before, "provider DB denial changed stored upload JWT"


def lifecycle_publish():
    """Upload with the selected real CF row; no JSON credential-storage surrogate."""
    bundle = json.loads((ROOT / "native-lifecycle.json").read_text())
    session = cf_api()
    environment = dict(os.environ, CRYSTAL_FORGE_SECRET_KEY=(ROOT / "cf-key").read_text(),
                       ATTIC_TOKEN=bundle["public"]["writer"], ATTIC_SERVER_URL="http://127.0.0.1:9/")
    environment.pop("CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY", None)
    with db("crystal_forge") as connection:
        before = raw_cf(connection)
        for identity, kind, authorized in ((1, "public", True), (2, "private", True), (3, "private", False)):
            source = ROOT / ("publication-source-" + str(identity))
            private(source, secrets.token_hex(4096))
            path = run(["nix-store", "--add", str(source)])
            hash_part = Path(path).name.split("-", 1)[0]
            native = bundle[kind]
            # A stale but authorized ambient JWT must not rescue the selected
            # unauthorized clone. The database credential remains authoritative.
            environment["ATTIC_TOKEN"] = native["writer"]
            headers = {"Authorization": "Bearer " + native["writer"]}
            url = native["config"]["substituter_endpoint"] + "/" + hash_part + ".narinfo"
            assert request(client(), "GET", url, headers=headers).status_code == 404, "fresh output already published"
            derivation = dict(id=1, commit_id=None, derivation_type="Package", derivation_name="retained-native",
                derivation_path=None, scheduled_at=None, completed_at=None, started_at=None,
                attempt_count=0, evaluation_duration_ms=None, error_message=None, pname=None,
                version=None, status_id=0, derivation_target=None, store_path=path)
            payload = {"database_destination": {"database_url": "postgresql://crystal_forge@127.0.0.1/crystal_forge",
                "id": identity, "encryption_key_file": str(ROOT / "cf-key")},
                "database": True, "derivation": derivation, "path": path}
            result = subprocess.run(["attic-server-publication"], input=json.dumps(payload).encode(),
                env=environment, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=120, check=False)
            assert result.returncode == (0 if authorized else 1), "DB-selected publication contradicted native JWT authority"
            assert result.stdout.strip() == (b"publication_success=true" if authorized else b"publication_success=false"), "publication outcome missing"
            response = request(client(), "GET", url, headers=headers)
            assert response.status_code == (200 if authorized else 404), "native narinfo contradicted selected upload"
            if authorized:
                nar = next(line[5:] for line in response.text.splitlines() if line.startswith("URL: "))
                assert request(client(), "GET", native["config"]["substituter_endpoint"] + "/" + nar, headers=headers).status_code == 200, "genuine NAR unavailable"
            retained_test(session, identity, expected=identity != 3)
            assert raw_cf(connection) == before, "function-level publication changed raw CF destinations/jobs"
        for identity in (1, 2, 3):
            value = metrics(session, identity).json()
            assert value["status"] == "available" and value["path_count"] == 1 and value["storage_bytes"] > 0
        assert raw_cf(connection) == before


def lifecycle_attic_baseline(published=False):
    with db("attic") as connection:
        filename = "lifecycle-published-hashes" if published else "lifecycle-native-hashes"
        private(ROOT / filename, json.dumps({"tables": attic_snapshot(connection),
            "write_requests": native_write_requests()}))


def native_records():
    return [json.loads(line) for line in Path("/var/log/nginx/native-safe.log").read_text().splitlines()]


def native_write_requests():
    return sum(row["method"] not in ("GET", "HEAD") for row in native_records())


def lifecycle_final():
    with db("crystal_forge") as connection:
        assert cf_snapshot(connection) == json.loads((ROOT / "lifecycle-saved-hashes").read_text()), "lifecycle changed protected CF state"
        ciphertexts = [row[0] for row in connection.execute(
            "SELECT attic_token FROM cache_destinations WHERE id BETWEEN 1 AND 8")]
    bundle = json.loads((ROOT / "native-lifecycle.json").read_text())
    protected = ciphertexts + [(ROOT / "cf-key").read_text(), bundle["reader"],
                              bundle["public"]["writer"], bundle["private"]["writer"]]
    journal = run(["journalctl", "-u", "crystal-forge-server", "--no-pager"])
    assert all(value not in journal for value in protected), "CF journal exposed retained private material"


def lifecycle_attic_final(published=False):
    filename = "lifecycle-published-hashes" if published else "lifecycle-native-hashes"
    baseline = json.loads((ROOT / filename).read_text())
    with db("attic") as connection:
        assert attic_snapshot(connection) == baseline["tables"], "read lifecycle mutated native objects"
    assert native_write_requests() == baseline["write_requests"], "Test or metrics attempted native writes"
    records = native_records()
    assert records and all(not row["cookie"] and not row["csrf"] for row in records), "CF session leaked to native Attic"
    if published:
        assert any(row["method"] == "PUT" and row["authorization"] and row["status"] == 200 for row in records), "genuine native upload not observed"
        assert any(row["authorization"] and row["status"] in (401, 403) for row in records), "native denial not observed"
    bundle = json.loads((ROOT / "native-lifecycle.json").read_text())
    journal = run(["journalctl", "-u", "atticd", "-u", "cf-attic-metrics", "-u", "nginx", "--no-pager"])
    assert all(token not in journal for token in [bundle["reader"], bundle["public"]["writer"], bundle["private"]["writer"]]), "native journal exposed JWT"


def main():
    global STAGE
    STAGE = sys.argv[1]
    commands = {
        "attic-setup": attic_setup, "attic-verify": attic_verify, "cf-tokens": cf_tokens,
        "module-credentials": module_credentials, "module-lifecycle": module_lifecycle,
        "cf-setup": cf_setup, "cf-gates": cf_gates, "corrupt": corrupt,
        "restore": lambda: corrupt(True), "cf-corrupt": lambda: absent(metrics(cf_api(), 4701), "error", "unexpected_status", 4701),
        "final-attic": final_attic, "final-cf": final_cf,
        "lifecycle-attic-setup": lifecycle_attic_setup, "lifecycle-cf-setup": lifecycle_cf_setup,
        "lifecycle-readonly": lifecycle_readonly, "lifecycle-save": lifecycle_save,
        "lifecycle-metrics": lifecycle_metrics, "lifecycle-unavailable": lifecycle_unavailable,
        "lifecycle-revoke": lambda: lifecycle_grant(True), "lifecycle-grant": lambda: lifecycle_grant(False),
        "lifecycle-publish": lifecycle_publish, "lifecycle-final": lifecycle_final,
        "lifecycle-attic-baseline": lifecycle_attic_baseline, "lifecycle-attic-final": lifecycle_attic_final,
        "lifecycle-published-baseline": lambda: lifecycle_attic_baseline(True),
        "lifecycle-published-final": lambda: lifecycle_attic_final(True),
        "lifecycle-metrics-published": lambda: lifecycle_metrics(True),
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
