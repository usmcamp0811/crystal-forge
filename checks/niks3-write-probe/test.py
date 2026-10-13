"""Runs disposable-VM native evidence; never prints credentials or provider prose.

The TLS observer re-originates verified fixture identities over native TLS. It
does not implement native authorization: the real Go handler makes that decision.
Only adversarial routes synthesize responses. Sensitive comparisons stay in the
guest. Entry-point failures report a phase and exception type, without tracebacks.
"""

import base64
import datetime
import hashlib
import http.client
import http.server
import json
import os
from pathlib import Path
import secrets
import ssl
import subprocess
import sys
import threading
import time

ROOT = Path("/run/niks3-probe")
VERSIONS = (("v16", 5751, 9446, 9546, 9746), ("v18", 5758, 9448, 9548, 9748))
PHASE = "initialization"
EVENTS = []
LOCK = threading.Lock()
MARKER = "provider-prose-must-not-escape-470"


class FixtureFailure(Exception):
    """Contains only a static assertion description authored in this fixture."""


def check(value, message):
    """Raises only static, credential-free assertion descriptions."""
    if not value:
        raise FixtureFailure(message)


def private(name, value):
    path = ROOT / name
    with path.open("wb") as file:
        os.chmod(path, 0o600)
        file.write(value.encode() if isinstance(value, str) else value)
    return path


def read(name):
    return (ROOT / name).read_text().strip()


def command(args, *, data=None, timeout=120, success=True, env=None, stage="private-command"):
    result = subprocess.run(args, input=data, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, timeout=timeout, env=env)
    if (result.returncode == 0) != success:
        # Classify captured diagnostics without exporting child output, argv,
        # paths, URLs or credential-bearing provider response bodies.
        categories = {
            "tls_trust": (b"unknown authority", b"certificate verify failed", b"unable to get local issuer"),
            "cli_shape": (b"unknown flag", b"unrecognized", b"unexpected argument"),
            "execution": (b"No such file", b"not found", b"Permission denied"),
            "signature": (b"lacks a signature", b"not signed"),
            "nix_metadata": (b"parsing nix path-info", b"command failed:", b"experimental Nix feature"),
            "provider_denial": (b"server returned 403:", b"server returned 401:", b"<Code>AccessDenied</Code>"),
            "s3_signature": (b"SignatureDoesNotMatch",),
            "s3_invalid_request": (b"InvalidRequest", b"InvalidArgument", b"AuthorizationQueryParametersError"),
            "transport": (b"connection refused", b"no such host", b"timeout", b"proxyconnect"),
            "tls_session": (b"tls:", b"x509:", b"certificate"),
            "http_framing": (b"server returned 411:", b"server returned 413:", b"server returned 501:", b"LengthRequired", b"NotImplemented"),
            "provider_request": (b"server returned 400:", b"server returned 500:", b"server returned 502:", b"unexpected status"),
            "provider_upload": (b"uploading", b"pending closure", b"presigned"),
            "closure_prepare": (b"preparing closures:", b"getting path info:"),
            "closure_create": (b"creating pending closures:", b"creating pending closure:"),
            "object_upload": (b"uploading objects:",),
            "narinfo_sign": (b"signing and uploading narinfos:", b"signing narinfos"),
            "closure_complete": (b"completing pending closure",),
            "json_shape": (b"cannot unmarshal", b"decoding response:", b"invalid character"),
            "stream_end": (b"EOF", b"broken pipe", b"reset by peer"),
            "multipart_shape": (b"NAR files require multipart upload info", b"no ETag in response", b"server returned empty"),
            "multipart_complete": (b"completing multipart upload:",),
            "multipart_part": (b"uploading part ", b"reading part ", b"requesting more parts"),
            "nar_serialization": (b"serializing NAR:", b"opening file ", b"reading directory "),
            "listing_upload": (b"uploading listing ", b"compressing listing:"),
            "build_log_upload": (b"build log", b"compressed log", b"mmap file:"),
            "cancellation": (b"context canceled", b"context deadline exceeded"),
        }
        diagnostic = result.stdout + result.stderr
        category = [name for name, markers in categories.items()
                    if any(marker in diagnostic for marker in markers)] or ["unclassified"]
        print(json.dumps({"stage": stage, "exit_code": result.returncode,
                          "diagnostic_category": category}), flush=True)
        check(False, "private subprocess exit mismatch")
    return result.stdout


def prepare():
    """Creates random tokens, signing keys and three-CA PKI at VM runtime."""
    from cryptography import x509
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import rsa
    from cryptography.x509.oid import NameOID, ExtendedKeyUsageOID
    from nacl.signing import SigningKey

    ROOT.mkdir(mode=0o700)
    now = datetime.datetime.now(datetime.timezone.utc)
    issuers = {}

    def issue(name, cn, issuer=None, server=False):
        key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, cn)])
        parent, signer = issuers[issuer] if issuer else (None, key)
        builder = (x509.CertificateBuilder().subject_name(subject)
                   .issuer_name(parent.subject if parent else subject)
                   .public_key(key.public_key()).serial_number(x509.random_serial_number())
                   .not_valid_before(now - datetime.timedelta(minutes=5))
                   .not_valid_after(now + datetime.timedelta(days=2))
                   .add_extension(x509.BasicConstraints(ca=issuer is None, path_length=None), True)
                   .add_extension(x509.SubjectKeyIdentifier.from_public_key(key.public_key()), False)
                   .add_extension(x509.AuthorityKeyIdentifier.from_issuer_public_key(signer.public_key()), False)
                   .add_extension(x509.KeyUsage(True, False, server, False, False,
                                               issuer is None, issuer is None, None, None), True))
        if issuer:
            builder = builder.add_extension(x509.ExtendedKeyUsage([
                ExtendedKeyUsageOID.SERVER_AUTH if server else ExtendedKeyUsageOID.CLIENT_AUTH]), False)
        if server:
            builder = builder.add_extension(x509.SubjectAlternativeName([
                x509.DNSName("cache"), x509.DNSName("localhost")]), False)
        cert = builder.sign(signer, hashes.SHA256())
        private(name + ".crt", cert.public_bytes(serialization.Encoding.PEM))
        private(name + ".key", key.private_bytes(serialization.Encoding.PEM,
                serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
        if not issuer:
            issuers[name] = (cert, key)

    for ca in ("root-a", "root-b", "root-c", "foreign"):
        issue(ca, ca)
    issue("api", "cache", "root-a", server=True)
    issue("s3", "cache", "root-b", server=True)
    issue("read-server", "cache", "root-b", server=True)
    for name, cn, issuer in (("write", "write", "root-c"),
                             ("read", "read", "root-c"),
                             ("wrong", "wrong", "root-c"),
                             ("wrong-issuer", "write", "foreign")):
        issue(name, cn, issuer)
    private("roots-ab.pem", read("root-a.crt") + "\n" + read("root-b.crt") + "\n")
    private("roots-abc.pem", read("roots-ab.pem") + "\n" + read("root-c.crt") + "\n")
    private("token", secrets.token_urlsafe(48))
    # A runtime JWT-shaped invalid credential exercises native preview logging.
    private("wrong-token", ".".join(base64.urlsafe_b64encode(part).decode().rstrip("=")
            for part in (b'{"alg":"HS256","typ":"JWT"}',
                         json.dumps({"sub": secrets.token_hex(20)}).encode(), os.urandom(32))))
    private("basic-user", secrets.token_hex(12))
    private("basic-password", secrets.token_urlsafe(32))
    signing = SigningKey.generate()
    private("signing.key", "probe:" + base64.b64encode(bytes(signing) + bytes(signing.verify_key)).decode())
    private("signing.pub", "probe:" + base64.b64encode(bytes(signing.verify_key)).decode())
    private("access-key", "GK0123456789abcdef01234567")
    private("secret-key", "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
    names = ["token", "wrong-token", "basic-user", "basic-password", "signing.pub",
             "roots-ab.pem", "roots-abc.pem", "root-a.crt", "root-b.crt"]
    names += [name + suffix for name in ("write", "read", "wrong", "wrong-issuer")
              for suffix in (".crt", ".key")]
    private("client.json", json.dumps({name: read(name) + ("\n" if name.endswith((".pem", ".crt", ".key")) else "")
                                      for name in names}))


def install_client():
    import pwd
    ROOT.mkdir(mode=0o700)
    for name, value in json.loads(Path("/run/probe-client.json").read_text()).items():
        private(name, value)
    private("encryption-key", secrets.token_urlsafe(48))
    owner = pwd.getpwnam("crystal-forge")
    os.chown(ROOT, owner.pw_uid, owner.pw_gid)
    for path in ROOT.iterdir():
        os.chown(path, owner.pw_uid, owner.pw_gid)
    Path("/run/probe-client.json").unlink()


def context(identity=None, ca="roots-ab.pem"):
    ctx = ssl.create_default_context(cafile=str(ROOT / ca))
    if identity:
        ctx.load_cert_chain(str(ROOT / (identity + ".crt")), str(ROOT / (identity + ".key")))
    return ctx


def native_request(port, path, method="GET", token=None, identity=None):
    connection = http.client.HTTPSConnection("cache", port, context=context(identity), timeout=10)
    headers = {"Content-Type": "application/json"}
    if token is not None:
        headers["Authorization"] = "Bearer " + token
    try:
        connection.request(method, path, body=b"{}" if method == "POST" else None, headers=headers)
        response = connection.getresponse()
        return response.status, response.getheader("Content-Type"), response.read(65537)
    finally:
        connection.close()


def native_contract():
    for _, port, _, _, read_port in VERSIONS:
        status, media, body = native_request(port, "/api/cache-config")
        config = json.loads(body)
        check(status == 200 and media == "application/json" and isinstance(config, dict), "native public metadata status/media/JSON contract")
        check(config["substituter_url"] == f"https://cache:{read_port}/public/" and
              config["public_keys"] == [read("signing.pub")] and
              (config.get("oidc_audience") is None or isinstance(config["oidc_audience"], str)),
              "native public metadata field decoder contract")
        name, encoded = config["public_keys"][0].split(":", 1)
        check(name == "probe" and len(base64.b64decode(encoded, validate=True)) == 32, "native metadata Nix signing key format")
        for identity, token in ((None, read("token")), ("write", None)):
            check(native_request(port, "/api/gc/status", token=token, identity=identity) ==
                  (404, "text/plain; charset=utf-8", b"no garbage collection has run yet\n"), "native no-GC fingerprint")
            check(native_request(port, "/api/pending_closures", "POST", token, identity) ==
                  (400, "text/plain; charset=utf-8", b"missing closure key\n"), "native invalid-closure fingerprint")
        for path, method in (("/api/gc/status", "GET"), ("/api/pending_closures", "POST")):
            for token, identity in ((None, None), (read("wrong-token"), None), (None, "wrong")):
                check(native_request(port, path, method, token, identity) ==
                      (401, "text/plain; charset=utf-8", b"Unauthorized\n"), "native denial fingerprint")
        try:
            native_request(port, "/api/gc/status", identity="wrong-issuer")
        except (ssl.SSLError, ConnectionError):
            pass
        else:
            check(False, "native wrong issuer must fail TLS")


class QuietServer(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def handle_error(self, request, client_address):
        # Never print HTTP exception context containing credential-bearing data.
        pass


class Observer(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        self.handle_request()

    def do_POST(self):
        self.handle_request()

    def handle_request(self):
        if self.server.server_port == 9080:
            with LOCK:
                payload = json.dumps(EVENTS).encode()
            self.reply(200, "application/json", payload)
            return
        peer = self.connection.getpeercert()
        cn = next((value for rdn in peer.get("subject", ()) for key, value in rdn
                   if key == "commonName"), None) if peer else None
        identity = {"write": "write", "read": "read", "wrong": "wrong"}.get(cn)
        parts = self.path.split("/", 2)
        case = parts[1]
        path = "/" + parts[2] if len(parts) == 3 else "/"
        payload = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        event = {"version": self.server.version_name, "case": case, "path": path,
                 "method": self.command, "auth": bool(self.headers.get("Authorization")),
                 "identity": identity, "empty_object": payload == b"{}", "status": None}
        with LOCK:
            EVENTS.append(event)
        try:
            status, media, body = self.dispatch(case, path, payload, identity)
            with LOCK:
                event["status"] = status
            self.reply(status, media, body)
        except (ssl.SSLError, OSError, http.client.HTTPException):
            self.reply(502, "text/plain", b"fixture transport failure")

    def reply(self, status, media, body):
        self.send_response(status)
        self.send_header("Content-Type", media)
        self.send_header("Content-Length", str(len(body)))
        if status == 302:
            self.send_header("Location", "/redirect-target/api/gc/status")
        self.end_headers()
        self.wfile.write(body)

    def dispatch(self, case, path, payload, identity):
        if path == "/nix-cache-info":
            check(not payload, "read request has no body")
            if case == "basic":
                expected = "Basic " + base64.b64encode((read("basic-user") + ":" + read("basic-password")).encode()).decode()
                if self.headers.get("Authorization") != expected:
                    return 401, "text/plain", b"read denied"
            elif case == "mtls" and identity != "read":
                return 403, "text/plain", b"read denied"
            return self.forward(path, None, None)
        if case in ("bad-metadata", "invalid-metadata-json") and path == "/api/cache-config":
            if case == "bad-metadata":
                return 400, "text/plain; charset=utf-8", MARKER.encode()
            return 200, "application/json", json.dumps({"substituter_url": "https://cache/",
                                                        "public_keys": ["probe:invalid"]}).encode()
        if case in ("unknown", "generic400", "unknown404", "method405", "proxy403",
                    "redirect302", "oversize", "timeout", "bad-post", "unexpected-post"):
            if path == "/api/cache-config":
                return self.forward(path, None, identity)
            if case == "timeout":
                time.sleep(10)  # Exceeds the production operation's eight-second budget.
            if case == "oversize":
                return 200, "application/json", (MARKER.encode() * 5000)
            if case in ("bad-post", "unexpected-post"):
                if path == "/api/gc/status":
                    return self.forward(path, payload, identity)
                return (400 if case == "bad-post" else 201), "text/plain; charset=utf-8", MARKER.encode()
            code = {"unknown": 200, "generic400": 400, "unknown404": 404,
                    "method405": 405, "proxy403": 403, "redirect302": 302}.get(case, 404)
            return code, "text/plain; charset=utf-8", MARKER.encode()
        check(path in ("/api/cache-config", "/api/gc/status", "/api/pending_closures"), "observer route allowlist")
        if self.command == "POST":
            check(payload == b"{}", "operator POST must contain exactly empty object")
        return self.forward(path, payload, identity)

    def forward(self, path, payload, identity):
        # Real native mTLS checks PeerCertificates. Re-origination uses only the
        # independently verified peer's fixture identity; no forwarded DN/header.
        token = self.headers.get("Authorization")
        connection = http.client.HTTPSConnection("cache", self.server.native_port,
                                                  context=context(identity), timeout=12)
        headers = {"Content-Type": "application/json"}
        if token and path != "/nix-cache-info":
            headers["Authorization"] = token
        try:
            connection.request(self.command, path, body=payload or None, headers=headers)
            response = connection.getresponse()
            return response.status, response.getheader("Content-Type", "text/plain"), response.read(100000)
        finally:
            connection.close()


def serve():
    threads = []
    for version, native_port, optional, required, read_port in VERSIONS:
        for port, mandatory, certificate in ((optional, False, "api"), (required, True, "api"), (read_port, False, "read-server")):
            server = QuietServer(("0.0.0.0", port), Observer)
            server.version_name = version
            server.native_port = native_port
            ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            ctx.minimum_version = ssl.TLSVersion.TLSv1_2
            ctx.load_cert_chain(str(ROOT / (certificate + ".crt")), str(ROOT / (certificate + ".key")))
            ctx.load_verify_locations(cafile=str(ROOT / "root-c.crt"))
            ctx.verify_mode = ssl.CERT_REQUIRED if mandatory else ssl.CERT_OPTIONAL
            server.socket = ctx.wrap_socket(server.socket, server_side=True)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            threads.append(thread)
    QuietServer(("0.0.0.0", 9080), Observer).serve_forever()


def sql_snapshot(database, tables=None):
    """Returns canonical full-field bytes, including ciphertext and timestamps."""
    import psycopg2
    from psycopg2 import sql
    with psycopg2.connect(dbname=database, user="postgres", host="/run/postgresql") as connection:
        with connection.cursor() as cursor:
            if tables is None:
                cursor.execute("SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename")
                tables = [row[0] for row in cursor.fetchall()]
            rows = {}
            for table in tables:
                cursor.execute(sql.SQL("SELECT row_to_json(t)::text FROM {} t").format(sql.Identifier(table)))
                rows[table] = sorted(row[0] for row in cursor.fetchall())
            cursor.execute("SELECT sequencename,last_value FROM pg_sequences WHERE schemaname='public' ORDER BY sequencename")
            sequences = cursor.fetchall() if database.startswith("niks3") else []
    return json.dumps({"tables": rows, "sequences": sequences}, sort_keys=True).encode()


def domain_snapshot():
    import boto3
    from botocore.config import Config
    s3 = boto3.client("s3", endpoint_url="https://cache:3904", region_name="garage",
                      aws_access_key_id=read("access-key"), aws_secret_access_key=read("secret-key"),
                      verify=str(ROOT / "roots-ab.pem"), config=Config(s3={"addressing_style": "path"}))
    objects = []
    for page in s3.get_paginator("list_objects_v2").paginate(Bucket="nix-cache"):
        for obj in page.get("Contents", []):
            body = s3.get_object(Bucket="nix-cache", Key=obj["Key"])["Body"].read()
            objects.append((obj["Key"], obj["Size"], obj["ETag"], hashlib.sha256(body).hexdigest()))
    uploads = []
    for page in s3.get_paginator("list_multipart_uploads").paginate(Bucket="nix-cache"):
        uploads.extend((item["Key"], item["UploadId"]) for item in page.get("Uploads", []))
    check(not uploads, "probe matrix must have zero multipart uploads")
    return json.dumps({"databases": [hashlib.sha256(sql_snapshot(db)).hexdigest()
                                    for db in ("niks3", "niks3-v18")],
                       "objects": sorted(objects), "uploads": sorted(uploads)}, sort_keys=True).encode()


def baseline():
    for database in ("niks3", "niks3-v18"):
        tables = json.loads(sql_snapshot(database))["tables"]
        for name, rows in tables.items():
            if name == "object_stats":
                # Native 1.8 seeds one inventory row even before any upload.
                # Validate its exact empty state; the full snapshot retains it.
                check([json.loads(row) for row in rows] ==
                      [{"id": True, "object_count": 0, "total_bytes": 0}],
                      "native seeded object inventory must report zero objects and bytes")
                continue
            if any(domain in name for domain in ("closure", "object", "multipart", "upload")):
                check(not rows, "native domain baseline must have zero closure/object/upload rows")
    private("domain-baseline", domain_snapshot())


def unchanged():
    check(domain_snapshot() == (ROOT / "domain-baseline").read_bytes(), "native DB/bucket/multipart inventory changed")


def safe_output(body):
    """Rejects runtime secrets, PEM, provider prose and private URL disclosures."""
    text = body.decode(errors="replace") if isinstance(body, bytes) else json.dumps(body)
    for name in ("token", "wrong-token", "basic-user", "basic-password", "encryption-key",
                 "write.key", "read.key", "wrong.key", "wrong-issuer.key"):
        value = read(name)
        check(value not in text, "response leaked runtime credential")
        if name == "wrong-token":
            check(value[:10] not in text and value[-10:] not in text, "response leaked upstream token preview")
        if name.endswith(".key"):
            check(all(line not in text for line in value.splitlines() if len(line) > 40), "response leaked private key payload")
    check(MARKER not in text and "PRIVATE KEY" not in text and "Bearer " not in text,
          "response leaked provider prose or credential material")


def matrix():
    """Uses actual admin cookies and both draft and retained-ID probe envelopes."""
    import requests
    global PHASE
    origin = "http://127.0.0.1:8000"
    client = requests.Session()
    client.trust_env = False
    counts = {"tests": 0, "discoveries": 0, "raw_state_comparisons": 0}

    def api(method, path, payload=None, expected=200):
        response = client.request(method, origin + path, json=payload, timeout=15, allow_redirects=False)
        safe_output(response.content)
        check(response.status_code == expected, "CF API status mismatch")
        return response.json()

    password = secrets.token_urlsafe(32)
    api("POST", "/api/auth/local/register", {"username": "native-probe-admin", "email": "probe@example.invalid", "password": password}, expected=201)
    api("POST", "/api/auth/local/login", {"username": "native-probe-admin", "password": password})
    client.headers["Cookie"] = "; ".join(c.name + "=" + c.value for c in client.cookies)
    csrf = next((c.value for c in client.cookies if "csrf" in c.name), None)
    check(csrf, "admin CSRF cookie missing")
    client.headers["X-CSRF-Token"] = csrf
    check("Admin" in api("GET", "/api/auth/whoami")["roles"], "real admin role required")
    # Create the disposable scope through the real Admin API before snapshots;
    # an empty watched-flake configuration does not seed module environments.
    environment = api("POST", "/api/v1/environments", {
        "name": "write-probe", "description": "Disposable native probe scope",
        "color_hex": "#7755aa", "is_active": True, "is_production": False,
    }, expected=201)
    scope = [environment["id"]]

    def events():
        response = requests.get("http://cache:9080/events", timeout=5)
        check(response.status_code == 200, "private observer unavailable")
        return response.json()

    def create(version, port, read_port, mode):
        return {"name": "probe-" + version + "-" + mode, "cache_type": "Niks3", "enabled": False,
                "environment_ids": scope, "push_to": f"https://cache:{read_port}/public/",
                "niks3_server_url": f"https://cache:{port}/native/", "niks3_public_keys": [read("signing.pub")],
                "niks3_write_auth_mode": mode, "niks3_read_auth_mode": "none",
                **({"niks3_auth_token": read("token")} if mode == "token" else {
                    "niks3_write_client_cert": read("write.crt"), "niks3_write_client_key": read("write.key"),
                    "niks3_write_ca_cert": read("roots-abc.pem")})}

    destinations = []
    for version, _, port, _, read_port in VERSIONS:
        for mode in ("token", "mtls"):
            settings = create(version, port, read_port, mode)
            saved = api("POST", "/api/v1/caches", settings, expected=201)
            destinations.append((version, port, mode, settings, saved["id"]))
    protected_tables = ["cache_destinations", "cache_destination_environments", "cache_push_jobs", "build_jobs"]
    before = sql_snapshot("crystal_forge", protected_tables)
    raw_rows = json.loads(before)["tables"]["cache_destinations"]
    for value in raw_rows:
        row = json.loads(value)
        field = "niks3_auth_token" if row["niks3_write_auth_mode"] == "token" else "niks3_write_client_key"
        check(row[field].startswith("enc:v1:"), "retained credential must actually be encrypted")
    private("cf-baseline", before)

    def immutable():
        check(sql_snapshot("crystal_forge", protected_tables) == before,
              "Test/Discover changed raw destination, scope, timestamp or queue state")
        counts["raw_state_comparisons"] += 1

    def discover(settings, destination_id, patch=None, expected=200):
        immutable()
        cursor = len(events())
        if destination_id:
            path = f"/api/v1/caches/{destination_id}/niks3/discover"
            payload = patch or {}
        else:
            path = "/api/v1/caches/niks3/discover"
            payload = {"server_url": settings["niks3_server_url"],
                       **{key: value for key, value in settings.items() if key.startswith("niks3_write_")},
                       **(patch or {})}
        result = api("POST", path, payload, expected)
        counts["discoveries"] += 1
        immutable()
        observed = events()[cursor:]
        check(all(e["method"] == "GET" and e["path"] == "/api/cache-config" and not e["auth"] for e in observed),
              "Discover contacted protected route or sent authorization")
        if expected == 200:
            check(result["public_keys"] == [read("signing.pub")], "Discover key mismatch")
        return result, observed

    def test(settings, destination_id, patch, probe_scope="write", expected=200):
        immutable()
        cursor = len(events())
        url = "/api/v1/caches/" + (str(destination_id) + "/" if destination_id else "") + "test-credentials"
        payload = {**(settings if destination_id is None else {}), **patch, "probe_scope": probe_scope}
        started = time.monotonic()
        result = api("POST", url, payload, expected)
        counts["tests"] += 1
        check(time.monotonic() - started < 12, "CF probe exceeded bounded overall deadline")
        immutable()
        observed = events()[cursor:]
        for event in observed:
            if event["path"] == "/api/cache-config":
                check(not event["auth"], "metadata borrowed write token or Basic identity")
            if event["method"] == "POST":
                check(event["path"] == "/api/pending_closures" and event["empty_object"], "operator sent a valid closure or mutating route")
        return result, observed

    def positive(result, observed, mode, all_scope=False, certificate_gate=False):
        check(result["ok"] is True, "native positive probe failed")
        check(result["status_code"] == 400, "healthy probe must retain native missing-closure status")
        for field in ("write_api_reachable", "write_authn_valid", "write_authorization_valid", "write_auth_valid"):
            check(result[field] is True and isinstance(result[field], bool), "native write evidence must be true booleans")
        check(result["write_probe_diagnostic_code"] == ("write_route_verified_certificate_gate" if certificate_gate else "write_route_verified"), "native write diagnostic mismatch")
        routes = [(e["method"], e["path"], e["status"]) for e in observed if e["path"].startswith("/api/")]
        expected = [("GET", "/api/gc/status", 404), ("GET", "/api/gc/status", 401),
                    ("POST", "/api/pending_closures", 400), ("POST", "/api/pending_closures", 401)]
        if certificate_gate:
            expected = [expected[0], expected[2]]  # Omission is rejected before HTTP.
        expected.insert(0, ("GET", "/api/cache-config", 200))
        check(result["discovery_valid"] is True, "write scope must validate public metadata independently")
        if all_scope:
            check(result["discovery_valid"] is True and result["read_access_valid"] is True, "all scope must prove read and metadata independently")
        check(routes == expected, "native protected request order/status mismatch")
        reads = [e for e in observed if e["path"] == "/nix-cache-info"]
        check(len(observed) == len(expected) + (1 if all_scope else 0), "scoped write contacted an unrequested plane")
        check(len(reads) == (1 if all_scope else 0), "scoped read request count mismatch")
        if reads:
            check(not reads[0]["auth"] and reads[0]["identity"] is None, "public read borrowed write credentials")
        for event in observed:
            if event["path"] == "/api/cache-config":
                check(not event["auth"] and event["identity"] == ("write" if mode == "mtls" else None),
                      "metadata used token/read identity instead of selected write TLS transport")
            if event["path"].startswith("/api/") and event["path"] != "/api/cache-config":
                if event["status"] == 401:
                    check(not event["auth"] and event["identity"] is None, "omission control retained credentials")
                check(event["identity"] in ("write", None), "write borrowed read identity")
                if mode == "token":
                    check(event["identity"] is None, "token-only write acquired TLS identity")
                else:
                    check(not event["auth"], "mTLS write sent Bearer or Basic")

    for version, port, mode, settings, destination_id in destinations:
        PHASE = version + "-" + mode
        read_port = next(item[4] for item in VERSIONS if item[0] == version)
        # Both real unsaved Add and retained Edit must reach the same contract.
        for selected_id in (None, destination_id):
            positive(*test(settings, selected_id, {}), mode)
            positive(*test(settings, selected_id, {}, "all"), mode, all_scope=True)
        minimal = {key: value for key, value in settings.items() if key in
                   ("cache_type", "niks3_server_url", "niks3_write_auth_mode", "niks3_auth_token",
                    "niks3_write_client_cert", "niks3_write_client_key", "niks3_write_ca_cert")}
        if mode == "token":
            minimal.pop("niks3_write_auth_mode")  # Default Write token mode is intentional.
        positive(*test(minimal, None, {}), mode)
        discover(settings, None)
        discover(settings, destination_id)
        if mode == "token":
            for selected_scope in ("write", "all"):
                result, observed = test(settings, destination_id, {"niks3_auth_token": read("wrong-token")}, selected_scope)
                check(result["discovery_valid"] is True and result["write_api_reachable"] is True,
                      "public metadata must survive a rejected token")
                check(all(result[field] is False for field in ("write_authn_valid", "write_authorization_valid", "write_auth_valid")), "wrong token must fail authentication")
                check([(e["path"], e["status"]) for e in observed if e["path"].startswith("/api/")] ==
                      [("/api/cache-config", 200), ("/api/gc/status", 401), ("/api/gc/status", 401)],
                      "bad token must retain metadata-first denial order and prohibit POST")
            discover(settings, destination_id, {"niks3_auth_token": read("wrong-token")})
            # Missing token is rejected by CF validation before any provider request.
            missing = {k: v for k, v in settings.items() if k != "niks3_auth_token"}
            _, observed = test(missing, None, {}, "all", expected=400)
            check(not observed, "missing token reached provider")
            # Write scope can omit irrelevant Save fields. Public Discover still
            # works, but absent authentication must never remain positive/unknown.
            discover(missing, None)
            result, observed = test(missing, None, {})
            check(result["ok"] is False and all(result[field] is False for field in
                  ("write_authn_valid", "write_authorization_valid", "write_auth_valid")),
                   "missing write token must report false authentication evidence")
            check(result["write_api_reachable"] is None and result["write_probe_diagnostic_code"] == "write_credentials_missing",
                  "local missing token must leave API reach unattempted")
            check(not observed, "missing write token contacted a protected provider route")
            result, observed = test(missing, None, {"niks3_read_auth_mode": "mtls",
                "niks3_read_client_cert": read("read.crt"), "niks3_read_client_key": read("read.key"),
                "niks3_read_ca_cert": read("roots-ab.pem")})
            check(result["write_api_reachable"] is None and result["write_authn_valid"] is False and not observed,
                  "missing Write token borrowed read mTLS identity")
            for selected_scope in ("write", "all"):
                result, observed = test(settings, destination_id,
                    {"niks3_auth_token": read("wrong-token") + "\r\nX-Fixture: " + MARKER,
                     "niks3_server_url": "https://unresolved-probe.invalid/"}, selected_scope)
                check(result["ok"] is False and result["write_api_reachable"] is None and
                      result["write_probe_diagnostic_code"] == "write_credentials_invalid" and
                      all(result[field] is False for field in ("write_authn_valid", "write_authorization_valid", "write_auth_valid")),
                      "invalid Bearer must fail locally before metadata/DNS in both write scopes")
                check(not observed, "invalid Bearer contacted provider")
            _, observed = test(settings, None, {"cache_type": "UnsupportedProbeType",
                "niks3_server_url": "https://unresolved-probe.invalid/"}, expected=400)
            check(not observed, "unknown cache type reached provider before validation")
        else:
            result, observed = test(settings, destination_id, {"niks3_write_client_cert": read("wrong.crt"), "niks3_write_client_key": read("wrong.key")}, "all")
            check(result["discovery_valid"] is True and result["write_authn_valid"] is False and
                  result["write_probe_diagnostic_code"] == "mtls_subject_rejected", "trusted wrong subject must reach native 401")
            check(not any(e["method"] == "POST" for e in observed), "wrong subject must prohibit POST")
            discover(settings, destination_id, {"niks3_write_client_cert": read("wrong.crt"), "niks3_write_client_key": read("wrong.key")})
            result, observed = test(settings, destination_id, {"niks3_write_client_cert": read("wrong-issuer.crt"), "niks3_write_client_key": read("wrong-issuer.key")})
            check(result["ok"] is False and result["write_probe_diagnostic_code"] == "client_identity_invalid", "wrong issuer TLS diagnostic")
            check(len(observed) <= 1 and all(e["method"] == "GET" and e["path"] == "/api/gc/status" and
                      e["identity"] is None and not e["auth"] and e["status"] == 401 for e in observed),
                  "wrong issuer allowed positive HTTP identity or POST instead of anonymous native control")
            check(result["write_api_reachable"] is False, "anonymous native 401 established API metadata reachability")
            _, observed = discover(settings, destination_id, {"niks3_write_client_cert": read("wrong-issuer.crt"),
                                    "niks3_write_client_key": read("wrong-issuer.key")}, expected=400)
            check(not observed, "wrong issuer discovery reached HTTP handler")
            required = next(item[3] for item in VERSIONS if item[0] == version)
            positive(*test(settings, destination_id, {"niks3_server_url": f"https://cache:{required}/native/"}), mode, certificate_gate=True)
            result, _ = test(settings, destination_id, {"niks3_write_ca_cert": read("root-b.crt")})
            check(result["ok"] is False, "B-only write trust borrowed ambient root A")
            _, observed = test(settings, destination_id, {"niks3_write_client_key": "invalid-private-material"}, expected=400)
            check(not observed, "invalid key reached provider")

        # Read and write use separate drafts. Basic/mTLS never authorize WriteApi.
        for read_mode, prefix, read_fields in (
            ("none", "public", {}),
            ("basic", "basic", {"niks3_read_basic_username": read("basic-user"), "niks3_read_basic_password": read("basic-password")}),
            ("mtls", "mtls", {"niks3_read_client_cert": read("read.crt"), "niks3_read_client_key": read("read.key"), "niks3_read_ca_cert": read("roots-ab.pem")})):
            patch = {"push_to": f"https://cache:{read_port}/{prefix}/", "niks3_read_auth_mode": read_mode, **read_fields}
            result, observed = test(settings, destination_id, patch, "read")
            check(result["ok"] is True and result["read_access_valid"] is True, "selected read identity failed")
            check(all(result[field] is None for field in ("write_api_reachable", "write_authn_valid", "write_authorization_valid", "write_auth_valid")), "read scope claimed write evidence")
            check(len(observed) == 1 and observed[0]["path"] == "/nix-cache-info", "read scope contacted write plane")
            check(observed[0]["auth"] == (read_mode == "basic") and
                  observed[0]["identity"] == ("read" if read_mode == "mtls" else None),
                  "read request used an unselected credential mode")
            positive(*test(settings, destination_id, patch), mode)
            if read_mode == "mtls":
                result, observed = test(settings, destination_id, {**patch, "niks3_read_ca_cert": read("root-a.crt")}, "read")
                check(result["ok"] is False and not observed and result["write_api_reachable"] is None,
                      "explicit A-only read trust borrowed ambient B or reached an HTTP handler")

        # Mode transitions are unsaved, effective Update merge decisions.
        transition = ({"niks3_write_auth_mode": "mtls", "niks3_write_client_cert": read("write.crt"),
                       "niks3_write_client_key": read("write.key"), "niks3_write_ca_cert": read("roots-abc.pem")}
                      if mode == "token" else {"niks3_write_auth_mode": "token", "niks3_auth_token": read("token")})
        positive(*test(settings, destination_id, transition), "mtls" if mode == "token" else "token")
        result, observed = test(settings, destination_id, {"niks3_write_auth_mode": "mtls" if mode == "token" else "token"},
                                expected=400 if mode == "token" else 200)
        if mode == "mtls":
            check(result["write_api_reachable"] is None and result["write_probe_diagnostic_code"] == "write_credentials_missing" and
                  all(result[field] is False for field in ("write_authn_valid", "write_authorization_valid", "write_auth_valid")),
                  "unsaved token transition borrowed inactive credentials or skipped local failure evidence")
        check(not observed, "mode transition borrowed inactive stored credentials")
        _, observed = test(settings, destination_id, {"niks3_write_auth_mode": "basic"}, expected=400)
        check(not observed, "write accepted a read-only authentication mode")
        _, observed = test(settings, destination_id, {"niks3_read_auth_mode": "token"}, "read", expected=400)
        check(not observed, "read accepted write token authentication mode")
        result, observed = test(settings, destination_id, {"niks3_read_auth_mode": "mtls",
            "niks3_read_client_cert": read("read.crt"), "niks3_read_client_key": read("read.key"),
            "niks3_read_ca_cert": read("root-a.crt")}, "all")
        check(result["ok"] is False and result["read_access_valid"] is False and result["discovery_valid"] is True and
              result["write_probe_diagnostic_code"] == "write_route_verified" and
              all(result[field] is True for field in ("write_api_reachable", "write_authn_valid", "write_authorization_valid", "write_auth_valid")),
              "All read failure overwrote independent successful write evidence")
        check(len(observed) == 5 and all(e["path"].startswith("/api/") for e in observed),
              "A-only All read trust borrowed B or altered successful write request count")

        for case in ("unknown", "generic400", "unknown404", "method405", "proxy403", "redirect302", "oversize", "timeout", "bad-post", "unexpected-post"):
            result, observed = test(settings, destination_id, {"niks3_server_url": f"https://cache:{port}/{case}/"})
            check(result["ok"] is False and result["write_authorization_valid"] is not True, "adversarial response falsely proved write access")
            if case not in ("bad-post", "unexpected-post"):
                check(not any(e["method"] == "POST" for e in observed), "unknown GET fingerprint permitted POST")
            expected_code = {"method405": "method_rejected", "proxy403": "proxy_access_denied",
                             "oversize": "probe_response_too_large", "timeout": "probe_timeout",
                             "unexpected-post": "unexpected_write_response"}.get(case, "route_unsupported")
            check(result.get("write_probe_diagnostic_code", result.get("diagnostic_code")) == expected_code,
                  "adversarial diagnostic must retain safe failure stage")
            if case == "unexpected-post":
                check(sum(e["method"] == "POST" for e in observed) == 1, "unexpected POST success triggered another POST")
            check(not any(e["case"] == "redirect-target" for e in observed), "probe followed redirect")
        for case in ("bad-metadata", "invalid-metadata-json"):
            result, observed = test(settings, destination_id, {"niks3_server_url": f"https://cache:{port}/{case}/"})
            check(result["ok"] is False and result["write_api_reachable"] is False and
                  result["write_authn_valid"] is True and result["write_authorization_valid"] is True and
                  result["write_auth_valid"] is True and result["write_probe_diagnostic_code"] == "write_route_verified",
                  "protected success must not replace invalid metadata reachability evidence")
            check([e["path"] for e in observed] == ["/api/cache-config", "/api/gc/status", "/api/gc/status",
                                                       "/api/pending_closures", "/api/pending_closures"],
                  "metadata failure changed independent protected probe ordering")
        denial = ({"niks3_auth_token": read("wrong-token")} if mode == "token" else
                  {"niks3_write_client_cert": read("wrong.crt"), "niks3_write_client_key": read("wrong.key")})
        result, observed = test(settings, destination_id, {**denial, "niks3_server_url": f"https://cache:{port}/bad-metadata/"})
        check(result["write_api_reachable"] is False and result["write_authn_valid"] is False and
              not any(e["method"] == "POST" for e in observed), "native GC denial falsely proved API metadata reachability")
    immutable()
    # Inspect CF only. Native upstream previews remain confined to provider VM.
    logs = command(["journalctl", "--no-pager", "-u", "crystal-forge-server", "-o", "cat"])
    safe_output(logs)
    check(b"-----BEGIN" not in logs, "CF logs leaked PEM material")
    check(password.encode() not in logs, "CF logs leaked admin credential")
    check(all(c.value.encode() not in logs for c in client.cookies), "CF logs leaked authenticated cookies")
    print("Native retained/draft write/read matrix and exact CF raw-state immutability passed", flush=True)
    print(json.dumps({"phase": "matrix", "result": "passed", **counts}), flush=True)


def publication():
    """Separately proves genuine CLI upload and signature-required NAR unpack."""
    env = {**os.environ, "SSL_CERT_FILE": str(ROOT / "roots-ab.pem"), "NIX_SSL_CERT_FILE": str(ROOT / "roots-ab.pem")}
    from nacl.signing import SigningKey
    untrusted_key = "untrusted:" + base64.b64encode(bytes(SigningKey.generate().verify_key)).decode()

    def build_output(name):
        import shutil
        builder = str(Path(shutil.which("busybox")).resolve())
        proof = secrets.token_hex(64)
        shell = builder + ' mkdir -p "$out"; ' + builder + ' printf %s ' + proof + ' > "$out/proof"'
        # COMPATIBILITY: Native 1.8 uses simple uploads up to 10 MiB, which the
        # pinned 1.6 CLI cannot consume. Eleven MiB of compressible fixture data
        # selects genuine multipart uploads with bounded memory/transfer costs.
        padded = name.startswith("v18-")
        if padded:
            shell += '; ' + builder + ' dd if=/dev/zero of="$out/multipart-padding" bs=1048576 count=11'
        # Restore Nix string context for the runtime-resolved executable. Without
        # this input dependency, its dynamic-loader closure is absent in sandbox.
        expression = ('let tools = builtins.storePath ' + json.dumps(str(Path(builder).parent.parent)) +
                      '; in builtins.derivation { name = ' + json.dumps(name) + '; system = builtins.currentSystem; '
                      'builder = tools + "/bin/busybox"; args = ["sh" "-c" ' + json.dumps(shell) + ']; }')
        output = command(["nix-build", "--no-out-link", "--expr", expression], env=env, stage="publication-build").decode().strip()
        info = json.loads(command(["nix", "path-info", "--json", output], env=env, stage="publication-path-info"))
        entry = info[output] if isinstance(info, dict) else info[0]
        check(not entry.get("ca"), "publication fixture must require signature, not content-addressed bypass")
        if padded:
            check(entry["narSize"] > 10 * 1024 * 1024, "native 1.8 fixture must select multipart protocol")
        return output, proof.encode()

    for version, native_port, _, _, _ in VERSIONS:
        for mode in ("token", "mtls"):
            output, proof = build_output(version + "-" + mode + "-publication")
            auth = (["--auth-token-path", str(ROOT / "token")] if mode == "token" else
                    ["--client-cert", str(ROOT / "write.crt"), "--client-key", str(ROOT / "write.key")])
            base = ["niks3", "push", "--server-url", f"https://cache:{native_port}"]
            command(base + auth + [output], env=env, stage="publication-authorized-push-" + version + "-" + mode)
            narinfo = "/" + output.split("/")[-1].split("-")[0] + ".narinfo"
            status, _, body = native_request(native_port, narinfo)
            check(status == 200 and b"Sig: probe:" in body, "real upload lacks signed narinfo")
            target = ROOT / (version + "-" + mode + "-unpacked")
            command(["nix", "copy", "--from", f"https://cache:{native_port}", "--to", "local?root=" + str(target),
                     "--option", "trusted-public-keys", read("signing.pub"), "--option", "require-sigs", "true", output], env=env, stage="publication-signed-copy")
            check((target / output.lstrip("/") / "proof").read_bytes() == proof, "authoritative NAR unpack differs")
            if version == "v18":
                padding = (target / output.lstrip("/") / "multipart-padding").read_bytes()
                check(padding == bytes(11 * 1024 * 1024), "authoritative multipart padding differs")
            wrong_root = ROOT / (version + "-" + mode + "-wrong-signature")
            command(["nix", "copy", "--from", f"https://cache:{native_port}", "--to", "local?root=" + str(wrong_root),
                     "--option", "trusted-public-keys", untrusted_key, "--option", "require-sigs", "true", output],
                     env=env, success=False, stage="publication-wrong-signature")
            check(not (wrong_root / output.lstrip("/")).exists(), "wrong key imported authoritative NAR")
            for bad in ("token", "subject"):
                rejected, _ = build_output(version + "-" + mode + "-bad-" + bad)
                rejected_info = "/" + rejected.split("/")[-1].split("-")[0] + ".narinfo"
                check(native_request(native_port, rejected_info)[0] == 404, "negative CLI path already cached")
                bad_auth = (["--auth-token-path", str(ROOT / "wrong-token")] if bad == "token" else
                            ["--client-cert", str(ROOT / "read.crt"), "--client-key", str(ROOT / "read.key")])
                command(base + bad_auth + [rejected], env=env, success=False, stage="publication-denied-push")
                check(native_request(native_port, rejected_info)[0] == 404, "negative CLI silently published fresh path")
    print("Separate genuine CLI publication and signature-required NAR unpack passed", flush=True)
    print(json.dumps({"phase": "publication", "result": "passed", "authorized_pushes": 4,
                      "signed_copies": 4, "wrong_signature_denials": 4, "denied_pushes": 8}), flush=True)


if __name__ == "__main__":
    os.umask(0o077)
    PHASE = sys.argv[1] if len(sys.argv) == 2 else "invalid-phase"
    try:
        {"prepare": prepare, "install-client": install_client, "serve": serve,
         "native-contract": native_contract, "baseline": baseline, "matrix": matrix,
         "unchanged": unchanged, "publication": publication}[PHASE]()
    except Exception as error:
        # Exception strings and tracebacks can include URLs, tokens and PEM.
        print("FAIL native write-probe phase=" + PHASE + " type=" + type(error).__name__, flush=True)
        if isinstance(error, FixtureFailure):
            print("Assertion: " + str(error), flush=True)
        sys.exit(1)
