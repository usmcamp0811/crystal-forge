"""Owns native credentials and read-only probe evidence in disposable test VMs.

Commands contain file paths only. Runtime JWTs, API cookies, private keys and
database snapshots never enter driver output or retained screenshot artifacts.
The deterministic Garage/Niks3 keys are public, test-only protocol fixtures.
"""

import datetime
import base64
import hashlib
import hmac
import json
import os
from pathlib import Path
import re
import secrets
import socket
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from urllib.parse import quote

import requests


CREDENTIALS = Path("/etc/cache-fixture-credentials")
FIXTURE = Path("/run/cf-cache-credential-fixture.json")
ATTIC_FIXTURE = Path("/run/cf-attic-fixture.json")
HTTP_FIXTURE = Path("/run/cf-http-fixture.json")
LEGACY_TOKEN = Path("/run/cf-legacy-attic-token.json")
SEED_REQUEST = Path("/run/cf-cache-seed-request.json")
PHASE_REQUEST = Path("/run/cf-cache-phase-request.json")
PHASE_ACK = Path("/run/cf-cache-phase-ack.json")
PHASE_PROOF = Path("/run/cf-cache-phase-proof.json")
SOURCE_KINDS = ("attic", "s3", "nix", "niks3", "nix_basic", "http_basic", "legacy_query")
LEGACY_KINDS = ("legacy_plain", "legacy_encrypted", "legacy_missing")
CA = "/etc/ssl/certs/ca-certificates.crt"
STAGE = "startup"


def protected_json(path, value):
    """Creates a private file before writing any plaintext credentials."""
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as output:
        json.dump(value, output)


def protected_replace(path, value):
    """Publishes runtime phase state atomically without exposing partial JSON."""
    temporary = path.with_suffix(".next")
    protected_json(temporary, value)
    temporary.replace(path)


def session():
    client = requests.Session()
    client.trust_env = False
    client.verify = CA
    return client


def request(client, method, url, **kwargs):
    return client.request(method, url, timeout=8, allow_redirects=False, **kwargs)


def mint(subject, create=False):
    command = [
        "atticadm", "--config", "/etc/atticd.toml", "make-token",
        "--sub", subject, "--validity", "1d", "--pull", "web-ui-private",
    ]
    if create:
        command += ["--create-cache", "web-ui-private"]
    result = subprocess.run(command, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, timeout=15, check=False)
    assert result.returncode == 0, "Attic token mint failed"
    return result.stdout.decode().strip()


def bootstrap_http():
    """Creates a runtime Basic ACL and real static Nix cache metadata."""
    username = "cache-fixture-reader"
    password = secrets.token_hex(24)
    # The password enters openssl only through stdin. stdout is the crypt hash,
    # not a credential-bearing command argument or a service log line.
    hashed = subprocess.run(
        ["openssl", "passwd", "-apr1", "-stdin"], input=password.encode() + b"\n",
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=8, check=False,
    )
    assert hashed.returncode == 0, "Basic fixture password hashing failed"
    acl = Path("/run/cache-http-fixture.htpasswd")
    descriptor = os.open(acl, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o640)
    with os.fdopen(descriptor, "w") as output:
        output.write(username + ":" + hashed.stdout.decode().strip() + "\n")
    import grp
    os.chown(acl, 0, grp.getgrnam("nginx").gr_gid)
    metadata = Path("/run/cache-http-metadata")
    metadata.mkdir(mode=0o755)
    (metadata / "nix-cache-info").write_text("StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 40\n")
    protected_json(HTTP_FIXTURE, {"username": username, "password": password,
                                  "query_token": secrets.token_hex(24)})


def bootstrap_attic():
    """Creates a private cache through native Attic with runtime-minted JWTs."""
    client = session()
    admin = mint("web-ui-setup", create=True)
    response = request(
        client, "POST", "https://atticCache:9443/_api/v1/cache-config/web-ui-private",
        headers={"Authorization": "Bearer " + admin},
        json={"keypair": "Generate", "is_public": False,
              "store_dir": "/nix/store", "priority": 40,
              "upstream_cache_key_names": []},
    )
    assert response.status_code == 200, "Native private Attic creation failed"
    token = mint("web-ui-read")
    replacement = mint("web-ui-read-replacement")
    assert token != replacement, "Replacement JWT must be distinct"
    config = request(
        client, "GET", "https://atticCache:9443/_api/v1/cache-config/web-ui-private",
        headers={"Authorization": "Bearer " + token},
    )
    assert config.status_code == 200, "Native Attic config read failed"
    assert config.json()["is_public"] is False, "Attic fixture must remain private"
    protected_json(ATTIC_FIXTURE, {
        "server_url": "https://atticCache:9443",
        "cache_name": "web-ui-private",
        "read_url": "https://atticCache:9443/web-ui-private/nix-cache-info",
        "public_key": config.json()["public_key"],
        "token": token, "replacement_token": replacement,
        "legacy_token": mint("web-ui-legacy-read"),
    })


def s3_list(client, fixture, replacement=False, invalid=False):
    """Signs a native path-style ListObjectsV2, including the preserved Host."""
    field_prefix = "replacement_" if replacement else ""
    key_id = fixture[field_prefix + "access_key_id"]
    secret = fixture[field_prefix + "secret_access_key"]
    if invalid:
        secret = "0" * 64
    now = datetime.datetime.now(datetime.timezone.utc)
    stamp, day = now.strftime("%Y%m%dT%H%M%SZ"), now.strftime("%Y%m%d")
    host = requests.utils.urlparse(fixture["endpoint"]).netloc
    path = "/" + fixture["bucket"]
    query = "list-type=2&max-keys=1"
    empty_hash = hashlib.sha256(b"").hexdigest()
    headers = {"Host": host, "x-amz-date": stamp,
               "x-amz-content-sha256": empty_hash}
    signed_headers = "host;x-amz-content-sha256;x-amz-date"
    canonical = "\n".join([
        "GET", path, query,
        f"host:{host}\nx-amz-content-sha256:{empty_hash}\nx-amz-date:{stamp}\n",
        signed_headers, empty_hash,
    ])
    scope = f"{day}/{fixture['region']}/s3/aws4_request"
    to_sign = "\n".join(["AWS4-HMAC-SHA256", stamp, scope,
                         hashlib.sha256(canonical.encode()).hexdigest()])
    signing = ("AWS4" + secret).encode()
    for part in [day, fixture["region"], "s3", "aws4_request"]:
        signing = hmac.new(signing, part.encode(), hashlib.sha256).digest()
    signature = hmac.new(signing, to_sign.encode(), hashlib.sha256).hexdigest()
    headers["Authorization"] = (
        f"AWS4-HMAC-SHA256 Credential={key_id}/{scope}, "
        f"SignedHeaders={signed_headers}, Signature={signature}"
    )
    return request(client, "GET", fixture["endpoint"] + path + "?" + query,
                   headers=headers)


def assemble():
    """Proves authentication against native services before exposing the file."""
    global STAGE
    attic = json.loads(ATTIC_FIXTURE.read_text())
    protected_json(LEGACY_TOKEN, {"token": attic.pop("legacy_token")})
    basic = json.loads(HTTP_FIXTURE.read_text())
    # This alias belongs only to the disposable VM. Its certificate SAN and
    # native route are verified below; the server later resolves/pins this DNS
    # authority through the unchanged production target-policy code.
    hosts_link = Path("/etc/hosts")
    hosts_file = Path("/run/cache-fixture-hosts")
    hosts_file.write_text(hosts_link.read_text() + "\n"
                          + socket.gethostbyname("cache") + " cache-alt\n")
    # NixOS installs /etc/hosts as a store symlink. Replace only the disposable
    # VM's link; never attempt to write its immutable store target.
    assert hosts_link.is_symlink(), "Expected isolated NixOS hosts symlink"
    hosts_link.unlink()
    hosts_link.symlink_to(hosts_file)
    client = session()
    STAGE = "native Attic authenticated read"
    assert request(client, "GET", attic["read_url"]).status_code in (401, 403, 404), \
        "Private Attic must reject anonymous reads"
    for field in ("token", "replacement_token"):
        response = request(client, "GET", attic["read_url"],
                           headers={"Authorization": "Bearer " + attic[field]})
        assert response.status_code == 200, "Native Attic read failed"
        assert "StoreDir: /nix/store" in response.text, "Attic metadata mismatch"
    s3 = {
        "endpoint": "https://cache:9443", "region": "garage", "bucket": "nix-cache",
        "access_key_id": "GK0123456789abcdef01234567",
        "secret_access_key": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "replacement_access_key_id": "GKabcdef0123456789abcdef01",
        "replacement_secret_access_key": "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
    }
    STAGE = "native Garage SigV4 ListObjectsV2"
    for replacement in (False, True):
        response = s3_list(client, s3, replacement=replacement)
        assert response.status_code == 200, "Native Garage signed list failed"
        assert ET.fromstring(response.content).tag.endswith("ListBucketResult"), \
            "Garage must return native S3 XML"
    assert s3_list(client, s3, invalid=True).status_code == 403, \
        "Garage must reject incorrect signatures"
    assert request(client, "GET", s3["endpoint"] + "/nix-cache?list-type=2").status_code == 403, \
        "Garage must reject anonymous bucket listing"
    niks3 = {
        "server_url": "https://cache:5751", "substituter_url": "https://cache:5752",
        "public_keys": [(CREDENTIALS / f"signing-{i}.pub").read_text().strip()
                        for i in range(2)],
        "token": "niks3-nonproduction-static-token-470-00000000",
        "ca_cert": (CREDENTIALS / "ca.crt").read_text(),
    }
    for plane, identity in (("write", "write"), ("read", "read")):
        niks3[plane + "_client_cert"] = (CREDENTIALS / f"{identity}.crt").read_text()
        niks3[plane + "_client_key"] = (CREDENTIALS / f"{identity}.key").read_text()
    STAGE = "native Niks3 discovery and read mTLS"
    config = request(client, "GET", niks3["server_url"] + "/api/cache-config")
    assert config.status_code == 200, "Native Niks3 discovery failed"
    assert config.json()["substituter_url"] == niks3["substituter_url"], \
        "Native Niks3 discovery must match private read URL"
    read_url = niks3["substituter_url"] + "/nix-cache-info"
    assert request(client, "GET", read_url).status_code in (400, 401, 403), \
        "Niks3 private read must reject anonymous clients"
    response = request(client, "GET", read_url,
                       cert=(str(CREDENTIALS / "read.crt"), str(CREDENTIALS / "read.key")))
    assert response.status_code == 200, "Native Niks3 mTLS read failed"
    assert "StoreDir: /nix/store" in response.text, "Niks3 metadata mismatch"
    assert request(client, "GET", read_url,
                   cert=(str(CREDENTIALS / "write.crt"), str(CREDENTIALS / "write.key"))).status_code == 403, \
        "Write subject must not authorize private reads"
    nix = {"url": "https://cache:5753/nix-cache-info"}
    assert request(client, "GET", nix["url"]).status_code == 200, \
        "Public Nix HTTPS metadata must succeed without authentication"
    STAGE = "native TLS HTTP Basic metadata"
    basic_url = "https://cache:9444/basic/nix-cache-info"
    assert request(client, "GET", basic_url).status_code == 401, \
        "Basic metadata must reject anonymous reads"
    response = request(client, "GET", basic_url, auth=(basic["username"], basic["password"]))
    assert response.status_code == 200 and "StoreDir: /nix/store" in response.text, \
        "Native Basic metadata read failed"
    assert request(client, "GET", basic_url,
                   auth=(basic["username"], "incorrect-fixture-password")).status_code == 401, \
        "Basic metadata must reject incorrect credentials"
    changed_url = "https://cache-alt:9444/authority/nix-cache-info"
    assert request(client, "GET", changed_url).status_code == 200, \
        "Changed authority must expose real public metadata"
    basic_public = {"url": basic_url, "authority_change_url": changed_url}
    protected_json(FIXTURE, {"version": 1, "attic": attic, "s3": s3,
                             "nix": nix, "niks3": niks3,
                             "nix_basic": dict(basic_public), "http_basic": dict(basic_public),
                             "checkpoint": {"request_path": str(PHASE_REQUEST), "ack_path": str(PHASE_ACK)},
                             "seed_request_path": str(SEED_REQUEST),
                             "legacy_query": {
                                 "sanitized_url": "https://cache:9444/legacy-query/nix-cache-info?fixture=legacy",
                                 "query_parameter": "token", "query_value_local_only": True,
                             }})
    ATTIC_FIXTURE.unlink()
    print("Native fixture ready: private Attic; two Garage SigV4 identities; Niks3 read mTLS; public Nix HTTPS; native HTTP Basic")


def snapshot_digest(raw, intentional_save=False):
    """Hashes a raw snapshot with only the explicit intentional-Save exceptions.

    Strict mode retains every destination and assignment field, including all
    timestamps. Intentional Save may change destination name/updated_at and
    renew assignment created_at when the existing backend recreates rows.
    Membership pairs, ciphertext, raw URI and every other current or future
    column remain part of the hash. This policy must never be used for Test,
    Cancel, failed Save or a retained Test against its new saved baseline.
    """
    if intentional_save:
        row = {key: value for key, value in raw["row"].items()
               if key not in ("name", "updated_at")}
        assignments = []
        for assignment in raw["assignments"]:
            assert "cache_destination_id" in assignment and "environment_id" in assignment
            assignments.append({key: value for key, value in assignment.items()
                                if key != "created_at"})
        # Preserve duplicate cardinality and every other assignment column;
        # comparing only a set of environment IDs would miss identity changes.
        assignments.sort(key=lambda value: (value["cache_destination_id"], value["environment_id"]))
        raw = {"row": row, "assignments": assignments}
    return hashlib.sha256(json.dumps(raw, sort_keys=True).encode()).digest()


def snapshot(destination_id, intentional_save=False, uri_only=False):
    """Hashes private SQL state using strict mode unless a Save is intentional."""
    assert isinstance(destination_id, int) and destination_id > 0
    assert not (intentional_save and uri_only), "URI hashes must not apply Save exceptions"
    projection = "to_jsonb(cd)"
    if uri_only:
        projection = "to_jsonb(cd.push_to)"
    assignments = """(SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY environment_id), '[]'::jsonb)
         FROM cache_destination_environments e WHERE cache_destination_id = cd.id)"""
    if uri_only:
        # URI retention is a separate invariant from scope retention. Hash only
        # the exact stored URI here; whole-row snapshots still include every
        # assignment field and timestamp.
        assignments = "'[]'::jsonb"
    query = f"""
      SELECT jsonb_build_object('row', {projection}, 'assignments',
        {assignments})
      FROM cache_destinations cd WHERE cd.id = {destination_id};
    """
    result = subprocess.run(
        ["sudo", "-u", "postgres", "psql", "-d", "crystal_forge", "-At",
         "-v", "ON_ERROR_STOP=1", "-c", query],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=8, check=False,
    )
    assert result.returncode == 0 and result.stdout.strip(), "Private snapshot failed"
    return snapshot_digest(json.loads(result.stdout), intentional_save=intentional_save)


def snapshot_policy_regressions():
    """Checks Save exceptions without a database, network or credential values."""
    import copy
    original = {"row": {"id": 1, "name": "original", "updated_at": "before",
                         "push_to": "opaque-uri", "ciphertext": "opaque-ciphertext"},
                "assignments": [{"cache_destination_id": 1, "environment_id": "environment-a",
                                 "created_at": "before", "future_scope_column": "preserved"}]}
    saved = copy.deepcopy(original)
    saved["row"].update(name="intentional edit", updated_at="after")
    saved["assignments"][0]["created_at"] = "after"
    expected_save = snapshot_digest(original, intentional_save=True)
    assert snapshot_digest(saved, intentional_save=True) == expected_save
    assert snapshot_digest(saved) != snapshot_digest(original), "Strict mode must reject Save metadata changes"
    for location, key in (("row", "updated_at"), ("assignments", "created_at")):
        changed = copy.deepcopy(original)
        target = changed["row"] if location == "row" else changed["assignments"][0]
        target[key] = "unexpected Test/Cancel timestamp"
        assert snapshot_digest(changed) != snapshot_digest(original), "Test/Cancel must retain all timestamps"
    variants = []
    changed = copy.deepcopy(saved)
    changed["assignments"].append({**changed["assignments"][0], "environment_id": "environment-b"})
    variants.append(changed)
    changed = copy.deepcopy(saved)
    changed["assignments"] = []
    variants.append(changed)
    for key, value in (("cache_destination_id", 2), ("environment_id", "environment-b"),
                       ("future_scope_column", "changed")):
        changed = copy.deepcopy(saved)
        changed["assignments"][0][key] = value
        variants.append(changed)
    changed = copy.deepcopy(saved)
    del changed["assignments"][0]["future_scope_column"]
    variants.append(changed)
    for key in ("push_to", "ciphertext"):
        changed = copy.deepcopy(saved)
        changed["row"][key] = "unexpected change"
        variants.append(changed)
    for changed in variants:
        assert snapshot_digest(changed, intentional_save=True) != expected_save, \
            "Save exceptions must not permit scope/configuration/ciphertext/URI changes"
    print("Snapshot policy regressions passed: exact membership, future columns and strict Test/Cancel timestamps")


def field_hashes(destination_id):
    """Hashes individual raw fields privately for value-free failure diagnosis."""
    assert type(destination_id) is int and destination_id > 0
    query = f"""
      SELECT jsonb_build_object('row', to_jsonb(cd), 'assignments',
        (SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY environment_id), '[]'::jsonb)
         FROM cache_destination_environments e WHERE cache_destination_id = cd.id))
      FROM cache_destinations cd WHERE cd.id = {destination_id};
    """
    result = subprocess.run(
        ["sudo", "-u", "postgres", "psql", "-d", "crystal_forge", "-At",
         "-v", "ON_ERROR_STOP=1", "-c", query],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=8, check=False,
    )
    assert result.returncode == 0 and result.stdout.strip()
    raw = json.loads(result.stdout)
    values = {"destination." + key: value for key, value in raw["row"].items()}
    for key in {key for row in raw["assignments"] for key in row}:
        values["assignments." + key] = [row[key] for row in raw["assignments"]]
    return {key: hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()
            for key, value in values.items()}


def api_client(wait=False):
    """Waits for browser-owned registration without creating an admin early."""
    client = session()
    origin = "http://127.0.0.1:3000"
    deadline = time.monotonic() + (600 if wait else 8)
    if wait:
        # Poll a read-only setup endpoint instead of consuming failed-login
        # attempts or racing the browser's first-admin registration transaction.
        while True:
            setup = request(client, "GET", origin + "/api/auth/setup-status")
            if setup.status_code == 200 and setup.json().get("requires_setup") is False:
                break
            assert time.monotonic() < deadline, "Browser registration did not complete"
            time.sleep(2)
    while True:
        login = request(client, "POST", origin + "/api/auth/local/login",
                        json={"username": "cf-ui-admin", "password": "testpassword123"})
        if login.status_code == 200 or time.monotonic() >= deadline:
            break
        time.sleep(2)
    assert login.status_code == 200, "VM-local login failed"
    # Local HTTP is a browser-trusted loopback fixture. requests does not send
    # Secure cookies over HTTP, so reproduce the browser's loopback cookie jar.
    client.headers["Cookie"] = "; ".join(f"{c.name}={c.value}" for c in client.cookies)
    csrf = next((c.value for c in client.cookies if "csrf" in c.name), None)
    assert csrf, "VM-local CSRF cookie absent"
    client.headers["X-CSRF-Token"] = csrf
    whoami = request(client, "GET", origin + "/api/auth/whoami")
    assert whoami.status_code == 200 and "Admin" in whoami.json()["roles"], \
        "Native API verification requires real admin authorization"
    return client, origin


def native_configs(fixture):
    """Returns real create payloads and independent replacement patches."""
    attic, s3, niks3 = (fixture[name] for name in ("attic", "s3", "niks3"))
    configs = [
        ("Attic", {"push_to": attic["read_url"], "attic_cache_name": attic["cache_name"],
                   "attic_public_key": attic["public_key"], "attic_token": attic["token"]},
         {"attic_token": attic["replacement_token"]}),
        ("S3", {"push_to": "s3://" + s3["bucket"], "s3_endpoint_url": s3["endpoint"],
                "s3_region": s3["region"], "s3_access_key_id": s3["access_key_id"],
                "s3_secret_access_key": s3["secret_access_key"]},
         {"s3_access_key_id": s3["replacement_access_key_id"],
          "s3_secret_access_key": s3["replacement_secret_access_key"]}),
        ("Nix", {"push_to": fixture["nix"]["url"]}, None),
    ]
    for mode in ("token", "mtls"):
        auth = ({"niks3_auth_token": niks3["token"]} if mode == "token" else {
            "niks3_write_client_cert": niks3["write_client_cert"],
            "niks3_write_client_key": niks3["write_client_key"],
            "niks3_write_ca_cert": niks3["ca_cert"],
        })
        configs.append(("Niks3", {
            "push_to": niks3["substituter_url"], "niks3_server_url": niks3["server_url"],
            "niks3_public_keys": niks3["public_keys"], "niks3_write_auth_mode": mode,
            "niks3_read_auth_mode": "mtls", "niks3_read_client_cert": niks3["read_client_cert"],
            "niks3_read_client_key": niks3["read_client_key"], "niks3_read_ca_cert": niks3["ca_cert"],
            **auth,
        }, None))
    return configs


def http_configs(fixture):
    """Constructs legacy raw URLs only at runtime for real API persistence."""
    private = json.loads(HTTP_FIXTURE.read_text())
    raw_basic = "https://" + quote(private["username"], safe="") + ":" \
        + quote(private["password"], safe="") + "@cache:9444/basic/nix-cache-info"
    raw_query = "https://cache:9444/legacy-query/nix-cache-info?token=" \
        + quote(private["query_token"], safe="") + "&fixture=legacy"
    return [("nix_basic", "Nix", {"push_to": raw_basic}),
            ("http_basic", "Http", {"push_to": raw_basic}),
            ("legacy_query", "Http", {"push_to": raw_query})]


def environment_scope(client, origin):
    response = request(client, "GET", origin + "/api/v1/environments")
    assert response.status_code == 200
    values = response.json()
    if isinstance(values, dict):
        values = values.get("environments", values.get("items", []))
    return [values[0]["id"]]


def historical_envelope(token, raw_key):
    """Builds a historical enc:v1 envelope independently from production Rust.

    The wire format is AES-256-GCM with SHA-256(raw process key), a random
    96-bit nonce, empty AAD, and standard base64 nonce.ciphertext-plus-tag.
    Neither the key, token nor resulting ciphertext may enter driver output.
    """
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM
    nonce = os.urandom(12)
    encrypted = AESGCM(hashlib.sha256(raw_key.encode()).digest()).encrypt(nonce, token.encode(), b"")
    return "enc:v1:" + base64.b64encode(nonce).decode() + "." + base64.b64encode(encrypted).decode()


def process_encryption_key(pid):
    """Reads only the effective process encryption key into private memory."""
    env = dict(value.split("=", 1) for value in Path(f"/proc/{pid}/environ").read_bytes().decode().split("\0") if "=" in value)
    key = env.get("CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY", env.get("CRYSTAL_FORGE_SECRET_KEY"))
    assert key, "Fixture server encryption key must be configured"
    return key


def insert_legacy_attic(pg, attic, token, raw_key, scope, prefix="task470-retained-native-"):
    """Inserts separate disabled legacy rows without the current create API.

    Parameterized SQL supplies canonical native metadata and exact membership.
    The three rows contain plaintext, independently encrypted historical data,
    and NULL respectively. Only IDs/storage-kind labels are returned to UI.
    """
    stored = {"legacy_plain": token, "legacy_encrypted": historical_envelope(token, raw_key), "legacy_missing": None}
    result = {}
    with pg.cursor() as cursor:
        for kind in LEGACY_KINDS:
            name = prefix + kind
            url = attic["read_url"] + "?fixture_case=" + kind
            cursor.execute("""INSERT INTO cache_destinations
                (name,cache_type,push_to,enabled,attic_token,attic_cache_name,attic_public_key)
                VALUES (%s,'Attic',%s,false,%s,%s,%s) RETURNING id""",
                (name, url, stored[kind], attic["cache_name"], attic["public_key"]))
            destination_id = cursor.fetchone()[0]
            for environment_id in scope:
                cursor.execute("INSERT INTO cache_destination_environments (cache_destination_id,environment_id) VALUES (%s,%s)",
                               (destination_id, environment_id))
            result[kind] = {"id": destination_id, "name": name, "storage": kind,
                            "token_expected": kind != "legacy_missing"}
    return result


def seed():
    """Seeds seven destinations only when the retained browser workflow requests.

    Provider startup and login readiness do not authorize row creation. Earlier
    browser workflows must see their original cache data. Step25 sends only a
    private nonsecret marker after its shared Add and Niks3 security workflows.
    """
    global STAGE
    STAGE = "waiting for retained workflow25 seed request"
    deadline = time.monotonic() + 2700
    while not SEED_REQUEST.exists():
        assert time.monotonic() < deadline, "Retained workflow did not request native fixture seed"
        time.sleep(0.1)
    assert SEED_REQUEST.stat().st_mode & 0o777 == 0o600, "Seed request must be private"
    assert json.loads(SEED_REQUEST.read_text()) == {
        "version": 1, "step": "25-caches-modal-attic", "workflow": "retained-cache-credentials",
    }, "Seed request must identify the retained workflow"
    print("Retained workflow25 requested native seed; starting real API creation", flush=True)
    before = subprocess.run(
        ["sudo", "-u", "postgres", "psql", "-d", "crystal_forge", "-At", "-v", "ON_ERROR_STOP=1",
         "-c", "SELECT count(*) FROM cache_destinations WHERE name LIKE 'task470-retained-native-%'"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=8, check=False,
    )
    assert before.returncode == 0 and before.stdout.strip() == b"0", "Native fixture rows must not exist before retained workflow seed"
    print("Native row count before retained workflow seed: 0", flush=True)
    STAGE = "waiting for browser-owned VM admin"
    client, origin = api_client(wait=True)
    fixture = json.loads(FIXTURE.read_text())
    scope = environment_scope(client, origin)
    hashes = {}
    # Niks3 token writes and private mTLS reads exercise independent identities.
    for index, (cache_type, config, _) in enumerate(native_configs(fixture)[:4]):
        STAGE = f"native browser fixture {cache_type} real API create"
        response = request(client, "POST", origin + "/api/v1/caches", json={
            "name": f"task470-retained-native-{cache_type.lower()}",
            "cache_type": cache_type, "enabled": False,
            "environment_ids": scope, **config,
        })
        assert response.status_code in (200, 201), "Native browser fixture create failed"
        destination_id = response.json()["id"]
        fixture[cache_type.lower()]["id"] = destination_id
        hashes[str(destination_id)] = {"row": snapshot(destination_id).hex()}
    for kind, cache_type, config in http_configs(fixture):
        STAGE = f"native browser fixture {kind} real API create"
        response = request(client, "POST", origin + "/api/v1/caches", json={
            "name": "task470-retained-native-" + kind, "cache_type": cache_type,
            "enabled": False, "environment_ids": scope, **config,
        })
        assert response.status_code in (200, 201), "Native legacy URL fixture create failed"
        destination_id = response.json()["id"]
        fixture[kind]["id"] = destination_id
        hashes[str(destination_id)] = {
            "row": snapshot(destination_id).hex(),
            "post_save_row": snapshot(destination_id, intentional_save=True).hex(),
            "uri": snapshot(destination_id, uri_only=True).hex(),
            "saved_name": "task470-retained-native-" + kind + "-roundtrip",
            "fields": field_hashes(destination_id),
        }
    # Legacy compatibility must not be manufactured by the current API writer.
    import psycopg2
    server = subprocess.run(["systemctl", "show", "crystal-forge-server.service", "-p", "MainPID", "--value"],
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=8, check=False)
    assert server.returncode == 0
    legacy_token = json.loads(LEGACY_TOKEN.read_text())["token"]
    pg = psycopg2.connect(dbname="crystal_forge", user="postgres", host="/run/postgresql")
    with pg:
        fixture["legacy_attic"] = insert_legacy_attic(pg, fixture["attic"], legacy_token,
                                                    process_encryption_key(int(server.stdout)), scope)
    pg.close()
    for kind, value in fixture["legacy_attic"].items():
        destination_id = value["id"]
        expected = {"row": snapshot(destination_id).hex()}
        if kind != "legacy_missing":
            expected.update(post_save_row=snapshot(destination_id, intentional_save=True).hex(),
                            uri=snapshot(destination_id, uri_only=True).hex(),
                            saved_name=value["name"] + "-roundtrip", fields=field_hashes(destination_id))
        hashes[str(destination_id)] = expected
    LEGACY_TOKEN.unlink()
    protected_json(Path("/run/cf-cache-seed-snapshots.json"), hashes)
    fixture["seed_complete"] = True
    temporary = FIXTURE.with_suffix(".ready")
    protected_json(temporary, fixture)
    temporary.replace(FIXTURE)
    print("Native browser fixture seeded through real API; IDs ready")


def checkpoints():
    """Acknowledges browser phases only after private raw database verification.

    The browser supplies phase names and IDs, never row values or hashes. Every
    original row must match its complete seed snapshot before any Save. Clone
    phases prove replacement Test non-mutation independently from Save rotation.
    """
    global STAGE
    deadline = time.monotonic() + 2700
    sequence = 1
    source_checked = False
    legacy_checked = False
    legacy_states = {}
    clones = {}
    while time.monotonic() < deadline:
        if not PHASE_REQUEST.exists():
            time.sleep(0.1)
            continue
        message = json.loads(PHASE_REQUEST.read_text())
        if message.get("sequence", 0) < sequence:
            time.sleep(0.1)
            continue
        assert message.get("sequence") == sequence, "Unexpected browser checkpoint sequence"
        assert PHASE_REQUEST.stat().st_mode & 0o777 == 0o600, "Phase request must be private"
        phase = message.get("phase")
        STAGE = "private database checkpoint " + str(phase)
        ids = message.get("ids")
        assert isinstance(ids, list) and all(type(value) is int and value > 0 for value in ids)
        assert len(set(ids)) == len(ids), "Checkpoint IDs must be distinct"
        fixture = json.loads(FIXTURE.read_text())
        source_ids = sorted(fixture[kind]["id"] for kind in SOURCE_KINDS)
        legacy_ids = sorted(value["id"] for value in fixture["legacy_attic"].values())
        if not source_checked:
            assert phase == "source-pre-save" and sorted(ids) == source_ids, \
                "First checkpoint must cover all seven original destinations"
            hashes = json.loads(Path("/run/cf-cache-seed-snapshots.json").read_text())
            for destination_id in source_ids:
                assert snapshot(destination_id).hex() == hashes[str(destination_id)]["row"], \
                    "Pre-Save Test/Cancel changed complete raw destination or assignments"
            source_checked = True
            print("Private pre-Save checkpoint passed: all seven complete raw rows and assignments unchanged")
        elif phase == "legacy-pre-save":
            assert not legacy_checked and sorted(ids) == legacy_ids
            hashes = json.loads(Path("/run/cf-cache-seed-snapshots.json").read_text())
            for destination_id in legacy_ids:
                assert snapshot(destination_id).hex() == hashes[str(destination_id)]["row"], \
                    "Legacy Test/Cancel changed plaintext/historical/NULL raw row or assignments"
            legacy_checked = True
            print("Private legacy pre-Save checkpoint passed: plaintext/historical/NULL complete raw rows unchanged")
        elif phase in ("legacy-saved", "legacy-retained"):
            assert legacy_checked and len(ids) == 1 and ids[0] in legacy_ids
            destination_id = ids[0]
            kind = next(kind for kind, value in fixture["legacy_attic"].items() if value["id"] == destination_id)
            assert kind != "legacy_missing"
            if phase == "legacy-saved":
                expected = json.loads(Path("/run/cf-cache-seed-snapshots.json").read_text())[str(destination_id)]
                assert snapshot(destination_id, intentional_save=True).hex() == expected["post_save_row"], \
                    "Unrelated legacy Save rewrote plaintext/ciphertext/configuration/scope"
                legacy_states[destination_id] = {"kind": kind, "stage": "saved", "hash": snapshot(destination_id)}
            else:
                state = legacy_states[destination_id]
                assert state["stage"] == "saved" and snapshot(destination_id) == state["hash"]
                state["stage"] = "retained"
                print("Private unrelated-Save/retained-Test legacy checkpoint passed: " + kind)
        else:
            assert len(ids) == 1 and ids[0] not in source_ids, "Clone checkpoint requires one new ID"
            destination_id = ids[0]
            if phase == "replacement-baseline":
                assert destination_id not in clones and len(clones) < 2
                # Accept only real disabled fixture clones, not another original
                # or an unrelated row. Query output stays private to this process.
                result = subprocess.run(
                    ["sudo", "-u", "postgres", "psql", "-d", "crystal_forge", "-At",
                     "-v", "ON_ERROR_STOP=1", "-c",
                     f"SELECT name, cache_type FROM cache_destinations WHERE id = {destination_id} AND enabled = false"],
                    stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=8, check=False,
                )
                assert result.returncode == 0
                identity = result.stdout.decode().strip().split("|")
                assert len(identity) == 2 and identity[1] in ("Attic", "S3")
                assert identity[0].startswith("task470-native-replacement-" + identity[1].lower() + "-")
                assert identity[1] not in [value["type"] for value in clones.values()]
                clones[destination_id] = {"type": identity[1], "stage": "baseline",
                                          "hash": snapshot(destination_id)}
            elif phase == "replacement-pre-save":
                clone = clones[destination_id]
                assert clone["stage"] == "baseline"
                assert snapshot(destination_id) == clone["hash"], \
                    "Replacement draft Test/Cancel mutated complete raw clone before Save"
                clone["stage"] = "tested"
                print("Private replacement pre-Save checkpoint passed: " + clone["type"])
            elif phase == "replacement-saved":
                clone = clones[destination_id]
                assert clone["stage"] == "tested"
                saved = snapshot(destination_id)
                assert saved != clone["hash"], "Real replacement Save did not change stored row"
                clone.update(stage="saved", hash=saved)
            elif phase == "replacement-retained":
                clone = clones[destination_id]
                assert clone["stage"] == "saved"
                assert snapshot(destination_id) == clone["hash"], \
                    "Retained replacement Test changed complete raw clone after Save"
                clone["stage"] = "retained"
                print("Private replacement retained-Test checkpoint passed: " + clone["type"])
            else:
                raise AssertionError("Unexpected browser checkpoint phase")
        complete = (len(clones) == 2 and all(clone["stage"] == "retained" for clone in clones.values())
                    and legacy_checked and len(legacy_states) == 2
                    and all(state["stage"] == "retained" for state in legacy_states.values()))
        if complete:
            protected_json(PHASE_PROOF, {
                "source_pre_save": "passed", "complete_source_rows": 7,
                "source_assignments_and_timestamps": "unchanged",
                "legacy_direct_sql_pre_save": "passed",
                "legacy_complete_raw_rows": 3,
                "legacy_unrelated_save_credentials_preserved": "passed",
                "replacement_clones": [{"cache_type": clone["type"],
                                        "pre_save_raw_immutability": "passed",
                                        "post_save_retained_raw_immutability": "passed"}
                                       for clone in clones.values()],
            })
        protected_replace(PHASE_ACK, {"sequence": sequence, "phase": phase, "ok": True,
                                      "id_count": len(ids)})
        sequence += 1
        if complete:
            return
    raise AssertionError("Browser database checkpoint deadline exceeded")


def verify_api():
    """Exercises stored-ID native probes and proves non-mutation in PostgreSQL."""
    global STAGE
    STAGE = "VM-local bootstrap admin login"
    client, origin = api_client()
    fixture = json.loads(FIXTURE.read_text())
    assert fixture.get("seed_complete"), "Browser fixtures were not seeded"
    # The whole-row proof occurs before Save, not after intentional edits. After
    # Save only the three explicit name edits may change name/updated_at; every
    # other raw destination field and exact assignment membership must still
    # match. Only assignment created_at may renew on these intentional Saves.
    assert PHASE_PROOF.exists(), "Required pre-Save and clone checkpoints were not completed"
    phase_proof = json.loads(PHASE_PROOF.read_text())
    assert phase_proof["source_pre_save"] == "passed" and phase_proof["complete_source_rows"] == 7
    STAGE = "native browser post-Save raw configuration retention"
    hashes = json.loads(Path("/run/cf-cache-seed-snapshots.json").read_text())
    post_save_failures = []
    for destination_id, expected in hashes.items():
        permits_name_save = "post_save_row" in expected
        if permits_name_save:
            current = field_hashes(int(destination_id))
            changed = sorted(key for key in set(current) | set(expected["fields"])
                             if current.get(key) != expected["fields"].get(key)
                             and key not in ("destination.name", "destination.updated_at", "assignments.created_at"))
            if changed:
                # Column names and destination IDs are nonsecret. Never print
                # either hash or a raw field value when the strict proof fails.
                print("Post-Save unexpected fields at destination " + destination_id + ": " + ", ".join(changed))
        if snapshot(int(destination_id), intentional_save=permits_name_save).hex() != expected.get("post_save_row", expected["row"]):
            post_save_failures.append(destination_id)
        if "uri" in expected:
            assert snapshot(int(destination_id), uri_only=True).hex() == expected["uri"], \
                "Browser changed retained raw URI credentials"
            name = subprocess.run(
                ["sudo", "-u", "postgres", "psql", "-d", "crystal_forge", "-At",
                 "-v", "ON_ERROR_STOP=1", "-c",
                 f"SELECT name FROM cache_destinations WHERE id = {int(destination_id)}"],
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=8, check=False,
            )
            assert name.returncode == 0 and name.stdout.decode().strip() == expected["saved_name"], \
                "Browser must persist only the explicitly requested round-trip name"
    assert not post_save_failures, "Browser Save changed configuration or assignment membership beyond explicit Save exceptions"
    environment_ids = environment_scope(client, origin)
    proof = [{"browser_phase_checkpoints": phase_proof,
              "post_save_source_configuration_and_scope": "passed",
              "post_save_assignment_membership": "exact",
              "assignment_created_at_policy": "renewal_only_on_intentional_save"}]
    configs = native_configs(fixture)
    for index, (cache_type, config, replacement) in enumerate(configs):
        STAGE = f"real API {cache_type} retained credential probe"
        create = {"name": f"task470-native-proof-{index}", "cache_type": cache_type,
                  "enabled": False, "environment_ids": environment_ids, **config}
        response = request(client, "POST", origin + "/api/v1/caches", json=create)
        assert response.status_code in (200, 201), "Real native API create failed"
        destination_id = response.json()["id"]
        url = origin + f"/api/v1/caches/{destination_id}"
        try:
            before = snapshot(destination_id)
            for patch in ({}, {"name": create["name"] + "-draft"}, replacement):
                if patch is None:
                    continue
                response = request(client, "POST", url + "/test-credentials", json=patch)
                assert response.status_code == 200, "Stored-ID native probe HTTP failure"
                result = response.json()
                assert result["ok"] is True and result["status_code"] == 200, \
                    "Stored-ID native read probe must succeed"
                if cache_type == "Niks3":
                    assert result["write_auth_valid"] is None, \
                        "Public Niks3 metadata does not prove write authorization"
                    assert result["read_endpoint_reachable"] is True
                assert snapshot(destination_id) == before, \
                    "Probe changed exact ciphertext/configuration/timestamps/assignments"
            if replacement:
                # A valid replacement was probed without persistence (Cancel).
                # Saving that same patch rotates credentials; a retained probe
                # must then authenticate with the new identity without writes.
                response = request(client, "PUT", url, json=replacement)
                assert response.status_code == 200, "Native replacement save failed"
                saved = snapshot(destination_id)
                assert saved != before, "Replacement save must change stored credentials"
                response = request(client, "POST", url + "/test-credentials", json={})
                assert response.status_code == 200 and response.json()["ok"] is True
                assert snapshot(destination_id) == saved, "Retained replacement probe mutated state"
            redacted = request(client, "GET", url)
            assert redacted.status_code == 200
            for field in ("attic_token", "s3_access_key_id", "s3_secret_access_key",
                          "niks3_auth_token", "niks3_write_client_key", "niks3_read_client_key"):
                assert not redacted.json().get(field), "GET exposed a stored credential"
            proof.append({"cache_type": cache_type, "write_mode": config.get("niks3_write_auth_mode"),
                          "stored_read_probe": "passed", "exact_database_immutability": "passed",
                          "replacement_cancel_save": "passed" if replacement else "not_applicable"})
        finally:
            deleted = request(client, "DELETE", url)
            assert deleted.status_code in (200, 204), "Native proof record cleanup failed"
    proof.extend(verify_http_api(client, origin, fixture))
    Path("/tmp/screenshots/native-cache-proof.json").write_text(json.dumps(proof, indent=2))
    for kind in ("attic", "s3", "nix", "niks3", "nix_basic", "http_basic", "legacy_query"):
        response = request(client, "DELETE", origin + f"/api/v1/caches/{fixture[kind]['id']}")
        assert response.status_code in (200, 204), "Native browser fixture cleanup failed"
    for value in fixture["legacy_attic"].values():
        response = request(client, "DELETE", origin + f"/api/v1/caches/{value['id']}")
        assert response.status_code in (200, 204), "Direct legacy fixture cleanup failed"
    HTTP_FIXTURE.unlink()
    print("Native stored-ID API proof passed: Attic, S3, public Nix, Niks3 token/private and mTLS/private, Nix/Http Basic, legacy query retention")


def verify_http_api(client, origin, fixture):
    """Proves retained Basic auth, authority non-forwarding and query refusal."""
    global STAGE
    proof = []
    private = json.loads(HTTP_FIXTURE.read_text())
    private_values = (private["username"], private["password"], private["query_token"])
    for kind in ("nix_basic", "http_basic", "legacy_query"):
        STAGE = "real API " + kind + " stored URL probe"
        destination_id = fixture[kind]["id"]
        url = origin + f"/api/v1/caches/{destination_id}"
        before = snapshot(destination_id)
        redacted = request(client, "GET", url)
        assert redacted.status_code == 200, "Legacy destination GET failed"
        body = redacted.json()
        assert not any(value in json.dumps(body) for value in private_values), \
            "GET leaked raw legacy URL credential material"
        safe_url = fixture[kind].get("url", fixture[kind].get("sanitized_url"))
        assert body["push_to"] == safe_url, "GET must return only sanitized URL"
        assert "@" not in body["push_to"], "GET leaked URL userinfo"
        # Exercise a real failed atomic Save against this same scoped source.
        # A missing environment must not change any raw field or timestamp.
        import uuid
        STAGE = "real API " + kind + " failed-Save rollback"
        failed = request(client, "PUT", url, json={
            "name": body["name"] + "-failed-save", "environment_ids": [str(uuid.uuid4())],
        })
        assert failed.status_code in (400, 404, 409, 500), "Invalid scoped Save must fail"
        assert snapshot(destination_id) == before, "Failed Save changed complete raw destination or assignments"
        print("Failed-Save complete raw snapshot unchanged: " + kind)
        STAGE = "real API " + kind + " retained metadata Test"
        response = request(client, "POST", url + "/test-credentials", json={})
        print("Native retained metadata Test: " + kind + " HTTP " + str(response.status_code))
        if kind == "legacy_query":
            assert body["legacy_query_credentials_configured"] is True
            assert response.status_code == 400, "Legacy query Test must fail before connection"
            error = response.json()
            assert not any(value in json.dumps(error) for value in private_values), \
                "Legacy query refusal exposed retained credentials"
            assert re.search(r"query|legacy", str(error), re.I), "Legacy query refusal needs static explanation"
            assert snapshot(destination_id) == before, "Legacy refusal mutated raw database row"
            original_uri = snapshot(destination_id, uri_only=True)
            original_config = snapshot(destination_id, intentional_save=True)
            saved = request(client, "PUT", url, json={"name": body["name"] + "-unrelated-save"})
            assert saved.status_code == 200, "Unrelated legacy update failed"
            assert snapshot(destination_id, uri_only=True) == original_uri, \
                "Unrelated Save changed retained legacy URI"
            assert snapshot(destination_id, intentional_save=True) == original_config, \
                "Unrelated legacy Save changed configuration/ciphertext/assignments"
            saved_snapshot = snapshot(destination_id)
            response = request(client, "POST", url + "/test-credentials", json={})
            assert response.status_code == 400 and snapshot(destination_id) == saved_snapshot
            proof.append({"cache_type": "Http", "legacy_query": "refused_before_connection",
                          "raw_uri_retention": "passed", "unrelated_save": "passed",
                          "failed_save_raw_immutability": "passed"})
        else:
            assert body["http_basic_auth_configured"] is True
            assert response.status_code == 200 and response.json()["ok"] is True, \
                "Stored Basic read probe must authenticate against native TLS ACL"
            # Stored-ID probes deliberately omit tested_url. If a future
            # response includes it, require the exact sanitized fixture URL.
            tested_url = response.json().get("tested_url")
            assert tested_url is None or tested_url == safe_url, "Test response leaked URL credentials"
            assert not any(value in json.dumps(response.json()) for value in private_values), \
                "Basic probe response exposed retained credentials"
            assert snapshot(destination_id) == before, "Stored Basic Test mutated raw row"
            changed = request(client, "POST", url + "/test-credentials",
                              json={"push_to": fixture[kind]["authority_change_url"]})
            assert changed.status_code == 200 and changed.json()["ok"] is True, \
                "Changed authority public metadata must succeed without stored Basic"
            assert snapshot(destination_id) == before, "Changed authority Test mutated raw row"
            raw_uri = snapshot(destination_id, uri_only=True)
            original_config = snapshot(destination_id, intentional_save=True)
            saved = request(client, "PUT", url, json={"name": body["name"], "push_to": safe_url})
            assert saved.status_code == 200, "Sanitized Basic URL round-trip Save failed"
            assert snapshot(destination_id, uri_only=True) == raw_uri, \
                "Sanitized URL Save discarded retained Basic credentials"
            assert snapshot(destination_id, intentional_save=True) == original_config, \
                "Sanitized Basic URL Save changed configuration/ciphertext/assignments"
            saved_snapshot = snapshot(destination_id)
            retained = request(client, "POST", url + "/test-credentials", json={})
            assert retained.status_code == 200 and retained.json()["ok"] is True, \
                "Stored Basic authentication failed after sanitized URL Save"
            assert snapshot(destination_id) == saved_snapshot, \
                "Post-Save Basic Test mutated raw database row"
            proof.append({"cache_type": "Nix" if kind == "nix_basic" else "Http",
                          "stored_basic_probe": "passed", "exact_database_immutability": "passed",
                          "sanitized_url_save_retained_auth": "passed",
                          "failed_save_raw_immutability": "passed",
                          "authority_change": "native_observer_required"})
    return proof


def observe(native_only=False):
    """Checks native route counters without exporting headers or query values."""
    rows = Path("/run/cf-cache-observer.log").read_text().splitlines()
    basic = [row for row in rows if row.startswith("/basic/")]
    assert any("status=200 auth=present" in row for row in basic), \
        "Native Basic ACL success was not observed"
    assert any("status=401 auth=absent" in row for row in basic), \
        "Native Basic ACL anonymous rejection was not observed"
    assert any("status=401 auth=present" in row for row in basic), \
        "Native Basic ACL incorrect-password rejection was not observed"
    assert not any(row.startswith("/legacy-query/") for row in rows), \
        "Legacy query Test replayed an HTTP request"
    changed = [row for row in rows if row.startswith("/authority/")]
    # One infrastructure GET and at least two actual stored-ID URL overrides.
    assert len(changed) >= (1 if native_only else 3), "Native changed-authority probes were not observed"
    assert all(row.endswith("auth=absent") for row in changed), \
        "Stored Basic Authorization was forwarded to a changed authority"
    result = {"legacy_query_native_request_count": 0,
              "changed_authority_requests": len(changed), "authorization_forwarded": False}
    if not native_only:
        attic_rows = Path("/run/cf-attic-observer.log").read_text().splitlines()
        assert not any(row.endswith("case=missing") for row in attic_rows), "Missing Attic credential probe contacted provider"
        for kind in ("plain", "encrypted"):
            assert any("status=200 auth=present" in row and row.endswith("case=" + kind) for row in attic_rows), \
                "Direct legacy Attic read must authenticate against native private provider"
        result.update(legacy_attic_plain_native_auth="passed", legacy_attic_encrypted_native_auth="passed",
                      legacy_attic_missing_native_requests=0)
        Path("/tmp/screenshots/native-http-observer-proof.json").write_text(json.dumps(result, indent=2))
    print("Native HTTP observer passed: no legacy query replay; no Basic Authorization at changed authority")


if __name__ == "__main__":
    try:
        STAGE = sys.argv[1]
        {"attic": bootstrap_attic, "http-setup": bootstrap_http, "assemble": assemble,
         "seed": seed, "verify": verify_api, "observe": observe,
         "checkpoints": checkpoints,
         "snapshot-policy-regressions": snapshot_policy_regressions,
         "observe-native": lambda: observe(native_only=True)}[STAGE]()
    except Exception as error:
        # Never print exception text, response bodies, local variables or
        # tracebacks: libraries can put credentials in those diagnostics.
        print(f"Native cache fixture failed at {STAGE} ({type(error).__name__})", file=sys.stderr)
        if isinstance(error, AssertionError) and error.args:
            # Fixture assertion messages are static contracts, never response
            # bodies or credential values. Other exception text stays hidden.
            print("Fixture assertion: " + str(error.args[0]), file=sys.stderr)
        sys.exit(1)
