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
PRODUCTION_PKI = Path("/etc/cache-fixture-production-pki")
NIKS3_OBSERVER = Path("/run/cf-niks3-observer/production.log")
NIKS3_TOKEN_OBSERVER = Path("/run/cf-niks3-observer/token.log")
NIKS3_BASIC_WRITE_OBSERVER = Path("/run/cf-niks3-observer/basic-write.log")
NIKS3_BASIC_READ_OBSERVER = Path("/run/cf-niks3-observer/basic-read.log")
METRICS_OBSERVERS = {"v16": Path("/run/cf-niks3-observer/metrics-v16.log"), "v18": Path("/run/cf-niks3-observer/metrics-v18.log")}
FIXTURE = Path("/run/cf-cache-credential-fixture.json")
ATTIC_FIXTURE = Path("/run/cf-attic-fixture.json")
HTTP_FIXTURE = Path("/run/cf-http-fixture.json")
NIKS3_BASIC_FIXTURE = Path("/run/cf-niks3-basic-read-fixture.json")
LEGACY_TOKEN = Path("/run/cf-legacy-attic-token.json")
SEED_REQUEST = Path("/run/cf-cache-seed-request.json")
PHASE_REQUEST = Path("/run/cf-cache-phase-request.json")
PHASE_ACK = Path("/run/cf-cache-phase-ack.json")
PHASE_PROOF = Path("/run/cf-cache-phase-proof.json")
ATTIC_OBSERVER = Path("/run/cf-attic-observer/requests.log")
ATTIC_SETUP_PROOF = Path("/run/cf-attic-setup-proof.json")
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


def mint(subject, create=False, push=False, cache_name="web-ui-private"):
    command = [
        "atticadm", "--config", "/etc/atticd.toml", "make-token",
        "--sub", subject, "--validity", "1d", "--pull", cache_name,
    ]
    if create:
        command += ["--create-cache", cache_name]
    if push:
        command += ["--push", cache_name]
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
    # The native Niks3 read endpoint has its own runtime ACL. Both fields use
    # legal whitespace so a successful request proves that no consumer trimmed
    # the complete Basic pair. These values never appear in a URL or logs.
    basic = {"username": "native reader", "password": "  native-basic-" + secrets.token_hex(24) + "  ",
             "replacement_username": "replacement reader", "replacement_password": "  native-replacement-" + secrets.token_hex(24) + "  "}
    descriptor = os.open("/run/cf-niks3-basic.htpasswd", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o640)
    with os.fdopen(descriptor, "w") as output:
        for prefix in ("", "replacement_"):
            digest = subprocess.run(["openssl", "passwd", "-apr1", "-stdin"],
                                    input=basic[prefix + "password"].encode() + b"\n",
                                    stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=8, check=False)
            assert digest.returncode == 0, "Native Basic ACL hashing failed"
            output.write(basic[prefix + "username"] + ":" + digest.stdout.decode().strip() + "\n")
    os.chown("/run/cf-niks3-basic.htpasswd", 0, grp.getgrnam("nginx").gr_gid)
    protected_json(NIKS3_BASIC_FIXTURE, basic)


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
    assert config.json()["store_dir"] == "/nix/store"
    assert config.json()["api_endpoint"] == "https://atticCache:9443/", \
        "Native CLI must receive the reachable TLS API endpoint"
    # A separate fixture-only writer publishes before any browser/API Test.
    # Login retains token-file configuration; no JWT enters argv or diagnostics.
    writer = mint("web-ui-publication-setup", push=True)
    directory = Path("/run/cf-attic-setup-client")
    directory.mkdir(mode=0o700)
    config_dir = directory / "config" / "attic"
    config_dir.mkdir(parents=True, mode=0o700)
    token_file = directory / "token"
    for path, text in ((token_file, writer + "\n"),
                       (config_dir / "config.toml", '[servers.native]\nendpoint = "https://atticCache:9443/"\ntoken-file = ' + json.dumps(str(token_file)) + '\n')):
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w") as output:
            output.write(text)
    env = {"PATH": os.environ["PATH"], "HOME": str(directory),
           "XDG_CONFIG_HOME": str(directory / "config"), "SSL_CERT_FILE": CA,
           "NIX_SSL_CERT_FILE": CA}
    published = Path("/etc/cache-fixture-publication-path").read_text().strip()
    for command in (["attic", "login", "native", "https://atticCache:9443/"],
                    ["attic", "cache", "info", "native:web-ui-private"],
                    ["attic", "push", "native:web-ui-private", published]):
        result = subprocess.run(command, env=env, stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL, timeout=120, check=False)
        assert result.returncode == 0, "Native Attic setup CLI login/cache-info/push failed"
    narinfo = request(client, "GET", "https://atticCache:9443/web-ui-private/" + Path(published).name[:32] + ".narinfo",
                      headers={"Authorization": "Bearer " + token})
    assert narinfo.status_code == 200 and "StorePath: " + published in narinfo.text, \
        "Native CLI publication must be readable before Test"
    token_file.unlink()
    (config_dir / "config.toml").unlink()
    setup = {"real_cli_login_cache_info_push": "passed", "published_narinfo_status": 200,
             "publication_phase": "fixture_setup_before_test"}
    protected_json(ATTIC_SETUP_PROOF, setup)
    protected_json(ATTIC_FIXTURE, {
        "server_url": "https://atticCache:9443",
        "cache_name": "web-ui-private",
        "read_url": "https://atticCache:9443/web-ui-private/nix-cache-info",
        "public_key": config.json()["public_key"],
        "token": token, "replacement_token": replacement,
        "legacy_token": mint("web-ui-legacy-read"),
        "denied_token": mint("web-ui-denied", cache_name="web-ui-other-cache"),
        "missing_cache_name": "web-ui-nonexistent",
        "missing_cache_token": mint("web-ui-missing", cache_name="web-ui-nonexistent"),
        "setup_proof": setup,
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
                          + socket.gethostbyname("cache") + " cache-alt read-cache.test\n")
    # NixOS installs /etc/hosts as a store symlink. Replace only the disposable
    # VM's link; never attempt to write its immutable store target.
    assert hosts_link.is_symlink(), "Expected isolated NixOS hosts symlink"
    hosts_link.unlink()
    hosts_link.symlink_to(hosts_file)
    client = session()
    STAGE = "native Attic authenticated read"
    cache_config_url = attic["server_url"] + "/_api/v1/cache-config/" + attic["cache_name"]
    assert request(client, "GET", attic["read_url"]).status_code in (401, 403, 404), \
        "Private Attic must reject anonymous reads"
    for field in ("token", "replacement_token"):
        response = request(client, "GET", attic["read_url"],
                           headers={"Authorization": "Bearer " + attic[field]})
        assert response.status_code == 200, "Native Attic read failed"
        assert "StoreDir: /nix/store" in response.text, "Attic metadata mismatch"
        assert request(client, "GET", attic["server_url"] + "/",
                       headers={"Authorization": "Bearer " + attic[field]}).status_code == 404, \
            "Authenticated root404 must discriminate the old generic probe"
        config = request(client, "GET", cache_config_url,
                         headers={"Authorization": "Bearer " + attic[field]})
        assert config.status_code == 200 and config.json()["is_public"] is False
        assert config.json()["store_dir"] == "/nix/store" and config.json()["public_key"] == attic["public_key"]
    denial_statuses = []
    for token in (attic["denied_token"], "invalid-fixture-token"):
        denied = request(client, "GET", cache_config_url, headers={"Authorization": "Bearer " + token})
        assert denied.status_code in (401, 403), "Native private cache must reject wrong token/permission"
        denial_statuses.append(denied.status_code)
    missing = request(client, "GET", attic["server_url"] + "/_api/v1/cache-config/" + attic["missing_cache_name"],
                      headers={"Authorization": "Bearer " + attic["missing_cache_token"]})
    assert missing.status_code == 404 and "NoSuchCache" in missing.text, \
        "A discovery-authorized nonexistent cache must return native NoSuchCache, not 401"
    protected_json(Path("/run/cf-attic-public-proof.json"), {
        **attic["setup_proof"], "authenticated_root_status": 404,
        "private_cache_config_status": 200, "native_typed_cache_config": "passed",
        "denied_identity_and_invalid_token_statuses": denial_statuses, "visible_missing_cache_status": 404,
        "stored_id_test": "requires_authoritative_browser_gate",
    })
    print("Native Attic setup: CLI publication passed; root404/config200; wrong credentials401/403; visible missing cache404")
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
    STAGE = "native certificate-required Niks3 discovery"
    production_mtls = production_discovery_fixture(client, niks3["public_keys"])
    production_basic = production_basic_fixture(production_mtls)
    metrics = native_metrics_fixture()
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
                             "production_write_mtls": production_mtls,
                             "production_basic": production_basic,
                             "metrics": metrics,
                             "nix_basic": dict(basic_public), "http_basic": dict(basic_public),
                             "checkpoint": {"request_path": str(PHASE_REQUEST), "ack_path": str(PHASE_ACK)},
                             "seed_request_path": str(SEED_REQUEST),
                             "legacy_query": {
                                 "sanitized_url": "https://cache:9444/legacy-query/nix-cache-info?fixture=legacy",
                                 "query_parameter": "token", "query_value_local_only": True,
                             }})
    ATTIC_FIXTURE.unlink()
    print("Native fixture ready: private Attic; two Garage SigV4 identities; Niks3 read mTLS; public Nix HTTPS; native HTTP Basic")


def production_discovery_fixture(client, public_keys):
    """Proves native ingress mTLS and separates API/S3 trust from client issuance.

    This function makes GET requests only. Root B belongs in the CLI's server
    trust bundle because uploads can use separately signed S3 hosts. Discovery
    contacts only the API signed by root A; it cannot establish S3 or push access.
    Root C issues client identities and is not server trust material.
    """
    from cryptography import x509
    from cryptography.hazmat.primitives import hashes
    from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
    api_ca = (PRODUCTION_PKI / "api-ca.crt").read_text()
    s3_ca = (PRODUCTION_PKI / "s3-ca.crt").read_text()
    client_ca = (PRODUCTION_PKI / "client-ca.crt").read_text()
    certificates = [x509.load_pem_x509_certificate(value.encode()) for value in (api_ca, s3_ca, client_ca)]
    roots = [cert.fingerprint(hashes.SHA256()) for cert in certificates]
    assert len(set(roots)) == 3, "API, S3 and client issuer CAs must be distinct"
    root_keys = [cert.public_key().public_bytes(Encoding.DER, PublicFormat.SubjectPublicKeyInfo)
                 for cert in certificates]
    assert len(set(root_keys)) == 3, "Role-separated CAs must not share their signing key"
    legacy = x509.load_pem_x509_certificate((CREDENTIALS / "ca.crt").read_bytes())
    assert root_keys[0] != legacy.public_key().public_bytes(Encoding.DER, PublicFormat.SubjectPublicKeyInfo), \
        "API root A must not be pretrusted through the legacy fixture CA"
    bundle = PRODUCTION_PKI / "server-ca-bundle.pem"
    bundle_certs = x509.load_pem_x509_certificates(bundle.read_bytes())
    assert {cert.fingerprint(hashes.SHA256()) for cert in bundle_certs} == set(roots[:2]), \
        "Server trust must contain API root A and S3 root B, not client issuer C"
    server_url = "https://cache:5754"
    url = server_url + "/api/cache-config"
    identity = (str(PRODUCTION_PKI / "write-client.crt"), str(PRODUCTION_PKI / "write-client.key"))
    replacement = (str(PRODUCTION_PKI / "replacement-write-client.crt"), str(PRODUCTION_PKI / "replacement-write-client.key"))
    client_certificates = [x509.load_pem_x509_certificate(Path(cert[0]).read_bytes()) for cert in (identity, replacement)]
    assert all(cert.subject.get_attributes_for_oid(x509.NameOID.COMMON_NAME)[0].value == "write" for cert in client_certificates)
    assert client_certificates[0].public_key().public_bytes(Encoding.DER, PublicFormat.SubjectPublicKeyInfo) != client_certificates[1].public_key().public_bytes(Encoding.DER, PublicFormat.SubjectPublicKeyInfo), \
        "Replacement discovery must use a distinct client key"
    native = session()
    native.verify = str(bundle)
    for cert in (identity, replacement):
        response = request(native, "GET", url, cert=cert)
        assert response.status_code == 200
        body = response.json()
        assert body["substituter_url"] == "https://cache:5753"
        assert body["public_keys"] == public_keys
    # Root A alone is sufficient for this GET. That success must never be
    # described as proof that a presigned S3 server under root B is reachable.
    native.verify = str(PRODUCTION_PKI / "api-ca.crt")
    assert request(native, "GET", url, cert=identity).status_code == 200
    native.verify = str(bundle)
    failures = []
    for cert in (None, (str(CREDENTIALS / "write.crt"), str(CREDENTIALS / "write.key"))):
        try:
            reply = request(native, "GET", url, **({"cert": cert} if cert else {}))
            assert reply.status_code >= 400, "Discovery ingress must require an approved client certificate"
            failures.append("rejected")
        except requests.exceptions.SSLError:
            failures.append("rejected")
    assert failures == ["rejected", "rejected"]
    # The pinned native server has no pins API. Do not substitute GC or an
    # upload attempt for the deliberately untested write-authorization state.
    pins = request(native, "GET", server_url + "/api/pins", cert=identity)
    assert pins.status_code == 404, "Pinned Niks3 must not gain a fabricated pins API"
    token_url = "https://cache:5756"
    assert request(client, "GET", token_url + "/api/cache-config").status_code == 200
    Path("/run/cf-production-discovery-public-proof.json").write_text(json.dumps({
        "native_config_mtls": "passed", "replacement_client": "passed",
        "missing_and_wrong_client": "rejected", "server_ca_roles": ["API_A", "S3_B"],
        "client_issuer": "C", "api_root_alone_discovery": "passed",
        "pins_api": "absent", "test_upload_count": 0, "write_authorization": "untested",
        "full_s3_push": "owned_by_separate_topology_vm",
    }))
    return {
        "server_url": server_url, "substituter_url": "https://cache:5753", "public_keys": public_keys,
        "write_client_cert": (PRODUCTION_PKI / "write-client.crt").read_text(),
        "write_client_key": (PRODUCTION_PKI / "write-client.key").read_text(),
        "server_ca_bundle": bundle.read_text(),
        "replacement_write_client_cert": (PRODUCTION_PKI / "replacement-write-client.crt").read_text(),
        "replacement_write_client_key": (PRODUCTION_PKI / "replacement-write-client.key").read_text(),
        "wrong_client_cert": (CREDENTIALS / "write.crt").read_text(),
        "wrong_client_key": (CREDENTIALS / "write.key").read_text(),
        "token_discovery_server_url": token_url,
    }


def production_basic_fixture(production_mtls):
    """Checks native Basic reads over root-B TLS without uploading or redirecting."""
    source = {key: value for key, value in production_mtls.items() if key not in ("id", "observer_baselines", "token_discovery_server_url")}
    source.update(json.loads(NIKS3_BASIC_FIXTURE.read_text()))
    source.update(server_url="https://cache:5758", substituter_url="https://read-cache.test:5757",
                  public_keys=[(PRODUCTION_PKI / f"signing-{index}.pub").read_text().strip() for index in range(2)])
    writer = session()
    writer.verify = str(PRODUCTION_PKI / "server-ca-bundle.pem")
    config = request(writer, "GET", source["server_url"] + "/api/cache-config",
                     cert=(str(PRODUCTION_PKI / "write-client.crt"), str(PRODUCTION_PKI / "write-client.key")))
    assert config.status_code == 200 and config.json()["substituter_url"] == source["substituter_url"]
    assert config.json()["public_keys"] == source["public_keys"]
    # Basic has no custom read-CA override. The browser/server VM system bundle
    # explicitly trusts public root B, while API root A remains custom trust.
    reader = session()
    url = source["substituter_url"] + "/nix-cache-info"
    for prefix in ("", "replacement_"):
        response = request(reader, "GET", url, auth=(source[prefix + "username"], source[prefix + "password"]))
        assert response.status_code == 200 and "StoreDir: /nix/store" in response.text
    assert request(reader, "GET", url).status_code == 401
    assert request(reader, "GET", url, auth=(source["username"], "incorrect-native-password")).status_code == 401
    assert request(reader, "GET", url, auth=(source["username"], source["password"].strip())).status_code == 401, \
        "The native ACL must discriminate trimmed Basic passwords"
    Path("/run/cf-niks3-basic-public-proof.json").write_text(json.dumps({
        "native_write_mtls_discovery": "passed", "native_basic_original_and_replacement": "passed",
        "anonymous_wrong_and_trimmed_password": "rejected", "tls_server_trust": "system_root_B",
        "url_userinfo": "absent", "setup_requests": "read_only", "upload_count": 0,
    }))
    return source


def native_metrics_fixture():
    """Checks genuine native stats capabilities without upload or enumeration."""
    client = session()
    client.verify = str(PRODUCTION_PKI / "server-ca-bundle.pem")
    identity = (str(PRODUCTION_PKI / "write-client.crt"), str(PRODUCTION_PKI / "write-client.key"))
    missing = request(client, "GET", "https://cache:5754/api/cache-stats", cert=identity)
    assert missing.status_code == 404, "Production-pinned Niks3 1.6 must not fabricate stats"
    available = request(client, "GET", "https://cache:5760/api/cache-stats", cert=identity)
    assert available.status_code == 200, "Test-only native Niks3 1.8 stats endpoint must be available"
    stats = available.json()
    assert all(type(stats[field]) is int and stats[field] >= 0 for field in ("objects", "logical_bytes"))
    # This isolated metrics database has no uploads. Its real empty totals are
    # evidence from the native API, not placeholder values supplied by the test.
    assert stats == {"objects": 0, "logical_bytes": 0}
    Path("/run/cf-native-metrics-public-proof.json").write_text(json.dumps({
        "production_package": "1.6.0", "native_v16_stats_status": 404,
        "test_only_remote": "1.8.0", "native_v18_stats_status": 200,
        "native_empty_objects": stats["objects"], "native_empty_logical_bytes": stats["logical_bytes"],
        "uploads_or_inventory_enumeration": "not_performed",
    }))
    return {"v16": {"server_url": "https://cache:5754", "native_status": 404},
            "v18": {"server_url": "https://cache:5760", "native_status": 200}}


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
        ("Attic", {"push_to": attic["server_url"], "attic_cache_name": attic["cache_name"],
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
            # Production persists a server/base URL and a separate cache name.
            # Private phase checkpoints correlate requests, never URL markers.
            url = attic["server_url"]
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


def attic_requests():
    """Reads only the shared method/path/status/auth-presence access records."""
    return ATTIC_OBSERVER.read_text().splitlines()


def check_attic_read_delta(before, cache_name=None, status=None, no_network=False):
    """Requires an exact read-only request delta after a synchronous Test.

    Setup publication runs before these checkpoints. A Test must not upload,
    fetch the root, or use a metadata URL. NULL credentials must stop locally.
    The shared log never records queries, credentials or upstream bodies.
    """
    deadline = time.monotonic() + 2
    while True:
        after = attic_requests()
        assert after[:len(before)] == before, "Native access log was reset during Test"
        delta = after[len(before):]
        if delta or no_network or time.monotonic() >= deadline:
            break
        time.sleep(0.02)
    if no_network:
        assert not delta, "Missing credential Test contacted native Attic"
    else:
        assert len(delta) == 1, "Attic Test must make exactly one native read-only request"
        expected_path = "GET /_api/v1/cache-config/" + quote(cache_name, safe="") + " "
        assert delta[0].startswith(expected_path), "Attic Test used the wrong native endpoint or uploaded"
        if status is not None:
            statuses = status if isinstance(status, tuple) else (status,)
            assert any("status=" + str(value) + " " in delta[0] for value in statuses), "Native Attic Test status mismatch"
        assert delta[0].endswith("auth=present"), "Private Attic Test omitted Authorization"
    return len(delta)


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
    # Keep the original seven independently guarded. This extra disabled row
    # exists only after the same late seed handshake and carries no read secret.
    production = fixture["production_write_mtls"]
    response = request(client, "POST", origin + "/api/v1/caches", json={
        "name": "task470-retained-native-production-write-mtls", "cache_type": "Niks3", "enabled": False,
        "environment_ids": scope, "push_to": production["substituter_url"],
        "niks3_server_url": production["server_url"], "niks3_public_keys": production["public_keys"],
        "niks3_write_auth_mode": "mtls", "niks3_write_client_cert": production["write_client_cert"],
        "niks3_write_client_key": production["write_client_key"], "niks3_write_ca_cert": production["server_ca_bundle"],
        "niks3_read_auth_mode": "none",
    })
    assert response.status_code in (200, 201), "Late native stored-mTLS discovery row creation failed"
    production["id"] = response.json()["id"]
    hashes[str(production["id"])] = {"row": snapshot(production["id"]).hex()}
    # Enabled metrics rows belong to a dedicated empty environment. They can
    # be observed by Admin without becoming destinations for seeded builds.
    environment = request(client, "POST", origin + "/api/v1/environments", json={
        "name": "task470-native-metrics-only", "description": "Disposable native stats observations",
        "color_hex": "#7755aa", "is_active": True, "is_production": False,
    })
    assert environment.status_code in (200, 201)
    fixture["metrics"]["environment_id"] = environment.json()["id"]
    for version in ("v16", "v18"):
        response = request(client, "POST", origin + "/api/v1/caches", json={
            "name": "task470-retained-native-metrics-" + version, "cache_type": "Niks3", "enabled": True,
            "environment_ids": [fixture["metrics"]["environment_id"]], "push_to": "https://cache:5753",
            "niks3_server_url": fixture["metrics"][version]["server_url"], "niks3_public_keys": production["public_keys"],
            "niks3_write_auth_mode": "mtls", "niks3_write_client_cert": production["write_client_cert"],
            "niks3_write_client_key": production["write_client_key"], "niks3_write_ca_cert": production["server_ca_bundle"],
            "niks3_read_auth_mode": "none",
        })
        assert response.status_code in (200, 201)
        fixture["metrics"][version]["id"] = response.json()["id"]
        hashes[str(response.json()["id"])] = {"row": snapshot(response.json()["id"]).hex()}
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
    # This boundary excludes the genuine setup push. Every later browser native
    # request must be explained by an ID-scoped Test checkpoint, including
    # negative replacements. Cancel and Save must not contact the provider.
    fixture["attic"]["browser_observer_baseline"] = len(attic_requests())
    production["observer_baselines"] = {
        "production": len(NIKS3_OBSERVER.read_text().splitlines()),
        "token": len(NIKS3_TOKEN_OBSERVER.read_text().splitlines()),
    }
    fixture["production_basic"]["observer_baselines"] = {
        "write": len(NIKS3_BASIC_WRITE_OBSERVER.read_text().splitlines()),
        "read": len(NIKS3_BASIC_READ_OBSERVER.read_text().splitlines()),
    }
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
    attic_pending = None
    attic_probes = []
    discovery_pending = None
    discovery_probes = []
    discovery_checked = False
    basic_pending = None
    basic_id = None
    basic_baseline = None
    basic_probes = []
    basic_checked = False
    metrics_pending = None
    metrics_checked = False
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
        discovery_ids = sorted(source_ids + legacy_ids + [fixture["production_write_mtls"]["id"]])
        if phase == "metrics-before":
            assert metrics_pending is None and sorted(ids) == sorted(fixture["metrics"][version]["id"] for version in ("v16", "v18"))
            metrics_pending = {value: snapshot(value) for value in ids}
        elif phase == "metrics-after":
            assert metrics_pending is not None and sorted(ids) == sorted(metrics_pending)
            assert all(snapshot(value) == digest for value, digest in metrics_pending.items()), "Metrics UI changed a raw cache, usage or assignment field"
            metrics_pending = None
            metrics_checked = True
        elif phase == "basic-added":
            assert basic_id is None and len(ids) == 1 and ids[0] not in discovery_ids
            basic_id = ids[0]
            result = subprocess.run([
                "sudo", "-u", "postgres", "psql", "-d", "crystal_forge", "-At", "-v", "ON_ERROR_STOP=1", "-c",
                f"SELECT to_jsonb(cd) FROM cache_destinations cd WHERE id={basic_id}",
            ], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=8, check=False)
            assert result.returncode == 0
            row = json.loads(result.stdout)
            assert row["name"].startswith("task470-native-basic-ui-") and row["cache_type"] == "Niks3"
            assert row["niks3_read_auth_mode"] == "basic" and row["niks3_write_auth_mode"] == "mtls"
            assert row["push_to"] == "https://read-cache.test:5757" and "@" not in row["push_to"]
            assert all(row[field].startswith("enc:v1:") for field in ("niks3_read_basic_username", "niks3_read_basic_password", "niks3_write_client_key")), \
                "UI Save must encrypt both Basic fields and the write private key"
            basic_baseline = snapshot(basic_id)
            fixture["production_basic"]["id"] = basic_id
            protected_replace(FIXTURE, fixture)
        elif phase == "basic-before":
            assert basic_pending is None and ids == ([] if basic_id is None else [basic_id])
            known = discovery_ids + ([] if basic_id is None else [basic_id])
            basic_pending = {"rows": {value: snapshot(value) for value in known},
                             "write": NIKS3_BASIC_WRITE_OBSERVER.read_text().splitlines(),
                             "read": NIKS3_BASIC_READ_OBSERVER.read_text().splitlines()}
        elif phase in ("basic-after-write", "basic-after-read", "basic-after-read-denied", "basic-after-read-redirect", "basic-after-discovery", "basic-after-cancel"):
            assert basic_pending is not None and ids == ([] if basic_id is None else [basic_id])
            assert all(snapshot(value) == digest for value, digest in basic_pending["rows"].items()), \
                "Scoped Basic Test/Discover/Cancel mutated raw rows, ciphertext, timestamps or assignments"
            deltas = {}
            for name, log in (("write", NIKS3_BASIC_WRITE_OBSERVER), ("read", NIKS3_BASIC_READ_OBSERVER)):
                rows = log.read_text().splitlines()
                before = basic_pending[name]
                assert rows[:len(before)] == before
                deltas[name] = rows[len(before):]
            if phase in ("basic-after-write", "basic-after-discovery"):
                assert len(deltas["write"]) == 1 and deltas["write"][0].startswith("GET /api/cache-config status=200 ")
                assert deltas["write"][0].endswith("auth=absent") and "client=present " in deltas["write"][0]
                assert not deltas["read"], "Write Test or Discovery contacted the read plane"
            elif phase in ("basic-after-read", "basic-after-read-denied"):
                status = 401 if phase == "basic-after-read-denied" else 200
                assert len(deltas["read"]) == 1 and deltas["read"][0].startswith(f"GET /nix-cache-info status={status} ")
                assert deltas["read"][0].endswith("auth=present") and "client=absent " in deltas["read"][0]
                assert not deltas["write"], "Read Test contacted the write API"
            elif phase == "basic-after-read-redirect":
                assert len(deltas["read"]) == 1 and deltas["read"][0].startswith("GET /redirect/nix-cache-info status=302 ")
                assert deltas["read"][0].endswith("auth=present") and "client=absent " in deltas["read"][0]
                assert not deltas["write"], "Read redirect contacted the write API"
            else:
                assert not deltas["write"] and not deltas["read"], "Cancel contacted a provider"
            basic_probes.append({"phase": phase, "whole_raw_rows": len(basic_pending["rows"]),
                                 "raw_immutability": "passed", "write_gets": len(deltas["write"]),
                                 "read_gets": len(deltas["read"]), "upload_count": 0})
            basic_pending = None
        elif phase == "basic-complete":
            assert basic_pending is None and not basic_checked and ids == [basic_id]
            assert snapshot(basic_id) == basic_baseline, "Tests or replacement drafts changed the saved Basic ciphertext"
            for required in ("basic-after-write", "basic-after-read", "basic-after-read-denied", "basic-after-read-redirect", "basic-after-discovery", "basic-after-cancel"):
                assert any(probe["phase"] == required for probe in basic_probes)
            basic_checked = True
            print("Native five-rail Basic UI Save and scoped Test/Discover/Cancel proof passed: encrypted pair, isolated planes and exact raw immutability")
        elif phase == "discovery-before":
            assert discovery_pending is None and sorted(ids) == discovery_ids
            discovery_pending = {
                "rows": {value: snapshot(value) for value in ids},
                "production": NIKS3_OBSERVER.read_text().splitlines(),
                "token": NIKS3_TOKEN_OBSERVER.read_text().splitlines(),
            }
        elif phase in ("discovery-after", "discovery-token-after", "discovery-rejected", "discovery-cancel"):
            assert discovery_pending is not None and sorted(ids) == discovery_ids
            assert all(snapshot(value) == digest for value, digest in discovery_pending["rows"].items()), \
                "Discovery/replacement/Cancel changed raw credentials, configuration, timestamps or assignments"
            deltas = {}
            for name, log in (("production", NIKS3_OBSERVER), ("token", NIKS3_TOKEN_OBSERVER)):
                rows = log.read_text().splitlines()
                before = discovery_pending[name]
                assert rows[:len(before)] == before, "Native discovery observer was reset"
                delta = rows[len(before):]
                assert all(row.startswith("GET /api/cache-config ") and row.endswith("auth=absent") for row in delta), \
                    "Discovery sent Authorization, uploaded, used pins or contacted another endpoint"
                deltas[name] = delta
            if phase == "discovery-after":
                assert len(deltas["production"]) == 1 and "status=200 " in deltas["production"][0]
                assert "client=present " in deltas["production"][0], "Production Discovery did not present its write identity"
                assert not deltas["token"]
            elif phase == "discovery-token-after":
                assert len(deltas["token"]) == 1 and "status=200 " in deltas["token"][0]
                assert "client=absent " in deltas["token"][0], "Token-mode Discovery borrowed a stored read client identity"
                assert not deltas["production"]
            elif phase == "discovery-rejected":
                # TLS can reject before an HTTP record exists. If a GET reached
                # nginx, it must be a failed cache-config GET with no Bearer.
                assert len(deltas["production"]) <= 1 and not deltas["token"]
                assert not any("status=200 " in row for row in deltas["production"])
            else:
                assert not deltas["production"] and not deltas["token"], "Cancel contacted native discovery"
            discovery_probes.append({"phase": phase, "whole_raw_rows": len(ids),
                                     "raw_immutability": "passed", "production_gets": len(deltas["production"]),
                                     "token_gets": len(deltas["token"]), "authorization_forwarded": False, "upload_count": 0})
            discovery_pending = None
        elif phase == "discovery-complete":
            assert discovery_pending is None and sorted(ids) == discovery_ids
            assert not discovery_checked
            assert sum(value["phase"] == "discovery-after" for value in discovery_probes) >= 4
            assert any(value["phase"] == "discovery-token-after" for value in discovery_probes)
            assert any(value["phase"] == "discovery-rejected" for value in discovery_probes)
            assert any(value["phase"] == "discovery-cancel" for value in discovery_probes)
            discovery_checked = True
            print("Native Discovery/Cancel checkpoints passed: cert-required ingress, retained/replacement snapshots, no Bearer/uploads and exact raw rows")
        elif phase == "attic-probe-before":
            assert attic_pending is None and len(ids) == 1
            assert ids[0] == fixture["attic"]["id"] or ids[0] in legacy_ids or clones.get(ids[0], {}).get("type") == "Attic"
            attic_pending = {"id": ids[0], "raw": snapshot(ids[0]), "requests": attic_requests()}
        elif phase in ("attic-probe-after", "attic-probe-no-network", "attic-probe-authentication"):
            assert attic_pending is not None and ids == [attic_pending["id"]]
            assert snapshot(ids[0]) == attic_pending["raw"], "Attic Test changed complete raw state"
            calls = check_attic_read_delta(attic_pending["requests"], fixture["attic"]["cache_name"],
                                          status=(401, 403) if phase == "attic-probe-authentication" else 200,
                                          no_network=phase == "attic-probe-no-network")
            # IDs correlate each private checkpoint with an existing fixture
            # identity. Do not modify the persisted URL to label HTTP requests.
            attic_probes.append({"destination_id": ids[0], "native_get_count": calls,
                                 "complete_raw_immutability": "passed", "upload_count": 0})
            attic_pending = None
        elif not source_checked:
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
                    and all(state["stage"] == "retained" for state in legacy_states.values())
                    and discovery_checked and basic_checked and metrics_checked)
        if complete:
            observed = attic_requests()[fixture["attic"]["browser_observer_baseline"]:]
            assert len(observed) == sum(probe["native_get_count"] for probe in attic_probes), \
                "Uncheckpointed browser/Cancel/Save contacted native Attic"
            assert all(row.startswith("GET /_api/v1/cache-config/") for row in observed), \
                "Browser Test/Cancel/Save uploaded or contacted the generic root"
            protected_json(PHASE_PROOF, {
                "source_pre_save": "passed", "complete_source_rows": 7,
                "source_assignments_and_timestamps": "unchanged",
                "legacy_direct_sql_pre_save": "passed",
                "legacy_complete_raw_rows": 3,
                "legacy_unrelated_save_credentials_preserved": "passed",
                "attic_server_base_probes": attic_probes,
                "attic_test_upload_count": 0,
                "attic_browser_uncheckpointed_requests": 0,
                "niks3_discovery_checkpoints": discovery_probes,
                "niks3_discovery_upload_count": 0,
                "niks3_discovery_write_authorization": "untested",
                "niks3_basic_ui_checkpoints": basic_probes,
                "niks3_basic_ui_encrypted_save": "passed",
                "native_metrics_ui_raw_immutability": "passed",
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
                native_before = attic_requests() if cache_type == "Attic" else None
                response = request(client, "POST", url + "/test-credentials", json=patch)
                assert response.status_code == 200, "Stored-ID native probe HTTP failure"
                result = response.json()
                assert result["ok"] is True and result["status_code"] == 200, \
                    "Stored-ID native read probe must succeed"
                if cache_type == "Niks3":
                    assert result["write_auth_valid"] is None, \
                        "Public Niks3 metadata does not prove write authorization"
                    assert result["read_endpoint_reachable"] is True
                if cache_type == "Attic":
                    assert_attic_result(result, "complete", True)
                    check_attic_read_delta(native_before, fixture["attic"]["cache_name"], status=200)
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
                native_before = attic_requests() if cache_type == "Attic" else None
                response = request(client, "POST", url + "/test-credentials", json={})
                assert response.status_code == 200 and response.json()["ok"] is True
                assert snapshot(destination_id) == saved, "Retained replacement probe mutated state"
                if cache_type == "Attic":
                    assert_attic_result(response.json(), "complete", True)
                    check_attic_read_delta(native_before, fixture["attic"]["cache_name"], status=200)
            redacted = request(client, "GET", url)
            assert redacted.status_code == 200
            for field in ("attic_token", "s3_access_key_id", "s3_secret_access_key",
                          "niks3_auth_token", "niks3_write_client_key", "niks3_read_client_key"):
                assert not redacted.json().get(field), "GET exposed a stored credential"
            if cache_type == "Attic":
                proof.append(verify_attic_api(client, url, fixture["attic"]))
            proof.append({"cache_type": cache_type, "write_mode": config.get("niks3_write_auth_mode"),
                          "stored_read_probe": "passed", "exact_database_immutability": "passed",
                          "replacement_cancel_save": "passed" if replacement else "not_applicable"})
        finally:
            deleted = request(client, "DELETE", url)
            assert deleted.status_code in (200, 204), "Native proof record cleanup failed"
    proof.extend(verify_http_api(client, origin, fixture))
    proof.append({"cache_type": "Niks3", "write_mode": "mtls", "discovery": phase_proof["niks3_discovery_checkpoints"],
                  "custom_server_ca_bundle": "API_A_and_S3_B", "client_issuer": "C",
                  "read_credentials_sent_to_discovery": False, "authorization_forwarded": False,
                  "upload_count": 0, "write_authorization": "untested"})
    basic_id = fixture["production_basic"]["id"]
    redacted = request(client, "GET", origin + f"/api/v1/caches/{basic_id}")
    assert redacted.status_code == 200 and redacted.json()["niks3_read_basic_configured"] is True
    assert not redacted.json().get("niks3_read_basic_username") and not redacted.json().get("niks3_read_basic_password")
    assert not redacted.json().get("niks3_write_client_key")
    proof.append({"cache_type": "Niks3", "read_mode": "basic", "five_rail_ui_save": phase_proof["niks3_basic_ui_encrypted_save"],
                  "scoped_checkpoints": phase_proof["niks3_basic_ui_checkpoints"], "basic_pair_get_redacted": "passed",
                  "password_whitespace": "preserved", "write_authorization": "untested", "test_upload_count": 0})
    proof.extend(verify_metrics_api(client, origin, fixture))
    Path("/tmp/screenshots/native-cache-proof.json").write_text(json.dumps(proof, indent=2))
    for kind in ("attic", "s3", "nix", "niks3", "nix_basic", "http_basic", "legacy_query"):
        response = request(client, "DELETE", origin + f"/api/v1/caches/{fixture[kind]['id']}")
        assert response.status_code in (200, 204), "Native browser fixture cleanup failed"
    for value in fixture["legacy_attic"].values():
        response = request(client, "DELETE", origin + f"/api/v1/caches/{value['id']}")
        assert response.status_code in (200, 204), "Direct legacy fixture cleanup failed"
    response = request(client, "DELETE", origin + f"/api/v1/caches/{fixture['production_write_mtls']['id']}")
    assert response.status_code in (200, 204), "Stored-mTLS discovery fixture cleanup failed"
    for destination_id in [basic_id] + [fixture["metrics"][version]["id"] for version in ("v16", "v18")]:
        assert request(client, "DELETE", origin + f"/api/v1/caches/{destination_id}").status_code in (200, 204)
    assert request(client, "DELETE", origin + f"/api/v1/environments/{fixture['metrics']['environment_id']}").status_code in (200, 204)
    HTTP_FIXTURE.unlink()
    print("Native stored-ID API proof passed: Attic, S3, public Nix, Niks3 token/private and mTLS/private, Nix/Http Basic, legacy query retention")


def verify_metrics_api(client, origin, fixture):
    """Checks actual remote stats and whole-row non-mutation with safe totals."""
    proof = []
    for version in ("v16", "v18"):
        destination_id = fixture["metrics"][version]["id"]
        baseline = snapshot(destination_id)
        before = METRICS_OBSERVERS[version].read_text().splitlines()
        response = request(client, "GET", origin + f"/api/v1/caches/{destination_id}/metrics")
        assert response.status_code == 200 and response.headers.get("Cache-Control") == "no-store"
        metrics = response.json()
        if version == "v16":
            assert metrics["status"] == "unavailable" and metrics["reason_code"] == "endpoint_unavailable"
            assert all(metrics[field] is None for field in ("storage_bytes", "object_count", "path_count", "measured_at")), \
                "Missing stats must remain unavailable, never fabricated zeros"
        else:
            assert metrics["status"] == "available" and metrics["reason_code"] == "native_stats"
            assert metrics["storage_bytes"] == 0 and metrics["object_count"] == 0
            assert metrics["storage_bytes_basis"] == "reported_logical"
            assert metrics["object_count_basis"] == "live_tracked_objects"
            assert metrics["path_count"] is None and metrics["measured_at"]
        assert snapshot(destination_id) == baseline, "Metrics changed a raw destination, usage or assignment field"
        delta = METRICS_OBSERVERS[version].read_text().splitlines()[len(before):]
        assert len(delta) == 1 and delta[0].startswith("GET /api/cache-stats ") and delta[0].endswith("auth=absent")
        proof.append({"native_version": "1.6.0" if version == "v16" else "1.8.0", "status": metrics["status"],
                      "complete_raw_immutability": "passed", "native_stats_get_count": 1,
                      "write_authorization": "untested", "upload_count": 0})
    return proof


def assert_attic_result(result, stage, accessible):
    """Requires explicit native cache-read evidence without write claims."""
    assert result["probe_kind"] == "attic_cache_config"
    assert result["stage"] == stage
    assert result["cache_access_valid"] is accessible
    assert result["write_auth_valid"] is None, "A read-only Test cannot prove write authorization"
    if accessible:
        assert result["token_auth_valid"] is True, "Private native success must authenticate"
    else:
        assert result["token_auth_valid"] is not True


def verify_attic_api(client, url, attic):
    """Checks wrong replacements and visible missing caches without persistence.

    The nonexistent-cache token has pull permission for that exact name. This
    makes native NoSuchCache404 discriminating; a hidden-cache401 is not proof
    that a cache does not exist. Provider deltas exclude setup publication.
    """
    destination_id = int(url.rsplit("/", 1)[1])
    before = snapshot(destination_id)
    cases = [({"attic_token": attic["denied_token"]}, "authentication", (401, 403), attic["cache_name"]),
             ({"attic_token": "invalid-fixture-token"}, "authentication", (401, 403), attic["cache_name"]),
             ({"attic_cache_name": attic["missing_cache_name"], "attic_token": attic["missing_cache_token"]},
              "cache_not_found", (404,), attic["missing_cache_name"])]
    for patch, stage, statuses, name in cases:
        native_before = attic_requests()
        response = request(client, "POST", url + "/test-credentials", json=patch)
        assert response.status_code == 200, "Native operational failures use a structured Test result"
        result = response.json()
        assert result["ok"] is False and result["status_code"] in statuses
        assert_attic_result(result, stage, False)
        assert snapshot(destination_id) == before, "Failed replacement/cache-name Test mutated stored row"
        assert not any(token in json.dumps(result) for token in (attic["token"], attic["replacement_token"], attic["denied_token"], attic["missing_cache_token"])), \
            "Attic result exposed a credential"
        check_attic_read_delta(native_before, name, status=result["status_code"])
    native_before = attic_requests()
    retained = request(client, "POST", url + "/test-credentials", json={})
    assert retained.status_code == 200 and retained.json()["ok"] is True
    assert_attic_result(retained.json(), "complete", True)
    check_attic_read_delta(native_before, attic["cache_name"], status=200)
    assert snapshot(destination_id) == before
    return {"cache_type": "Attic", "persisted_endpoint_shape": "server_base",
            "setup_publication": attic["setup_proof"], "root404_config200": "passed",
            "wrong_token_and_replacement": "authentication", "visible_missing_cache": "cache_not_found",
            "failed_test_raw_immutability": "passed", "retained_test_after_failure": "passed",
            "test_upload_count": 0, "write_authorization": "untested"}


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
        phase = json.loads(PHASE_PROOF.read_text())
        fixture = json.loads(FIXTURE.read_text())
        probes = phase["attic_server_base_probes"]
        browser_rows = Path("/run/cf-attic-observer.log").read_text().splitlines()[fixture["attic"]["browser_observer_baseline"]:]
        assert len(browser_rows) == sum(probe["native_get_count"] for probe in probes), \
            "Final browser observer found an uncheckpointed Attic request"
        assert all(row.startswith("GET /_api/v1/cache-config/") for row in browser_rows), \
            "Final browser observer found an Attic upload or generic-root request"
        for kind, value in fixture["legacy_attic"].items():
            correlated = [probe for probe in probes if probe["destination_id"] == value["id"]]
            assert correlated, "Every legacy row requires a private native request checkpoint"
            assert all(probe["native_get_count"] == (1 if value["token_expected"] else 0) for probe in correlated)
        assert phase["attic_test_upload_count"] == 0
        discovery = phase["niks3_discovery_checkpoints"]
        for name, log in (("production", NIKS3_OBSERVER), ("token", NIKS3_TOKEN_OBSERVER)):
            rows = log.read_text().splitlines()[fixture["production_write_mtls"]["observer_baselines"][name]:]
            assert len(rows) == sum(probe[name + "_gets"] for probe in discovery), \
                "An uncheckpointed operation contacted native Discovery"
            assert all(row.startswith("GET /api/cache-config ") and row.endswith("auth=absent") for row in rows), \
                "Discovery uploaded, used pins or forwarded Authorization"
        assert phase["niks3_discovery_upload_count"] == 0
        for name, log in (("write", NIKS3_BASIC_WRITE_OBSERVER), ("read", NIKS3_BASIC_READ_OBSERVER)):
            rows = log.read_text().splitlines()[fixture["production_basic"]["observer_baselines"][name]:]
            probes = phase["niks3_basic_ui_checkpoints"]
            assert len(rows) == sum(probe[name + "_gets"] for probe in probes)
            expected = "GET /api/cache-config " if name == "write" else "GET /nix-cache-info "
            assert all(row.startswith(expected) or name == "read" and row.startswith("GET /redirect/nix-cache-info ") for row in rows), \
                "Scoped Basic UI probe uploaded or followed a redirect"
        for log in METRICS_OBSERVERS.values():
            assert all(row.startswith("GET /api/cache-stats ") and row.endswith("auth=absent") for row in log.read_text().splitlines()), \
                "Metrics enumerated, uploaded or forwarded Authorization"
        result.update(legacy_attic_plain_native_auth="passed", legacy_attic_encrypted_native_auth="passed",
                      legacy_attic_missing_native_requests=0, attic_test_upload_count=0,
                      attic_browser_uncheckpointed_requests=0,
                      attic_probe_endpoint="_api/v1/cache-config/<cache>")
        result.update(niks3_discovery_upload_count=0, niks3_discovery_authorization_forwarded=False,
                      niks3_discovery_read_identity_borrowed=False, niks3_discovery_write_authorization="untested")
        result.update(niks3_basic_ui_upload_count=0, niks3_basic_ui_plane_isolation="passed", native_metrics_read_only="passed")
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
