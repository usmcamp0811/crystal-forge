"""Serve authenticated logical inventory for the pinned legacy Attic schema."""

import argparse
from decimal import Decimal
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import ipaddress
import json
import math
import os
import re
import socket
import stat
import sys
import threading
import time
from urllib.parse import unquote_to_bytes

import psycopg

U64_MAX = (1 << 64) - 1
CACHE_NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9_+\-]{0,49}\Z")
TOKEN = re.compile(r"[A-Za-z0-9_-]{32,1024}\Z")
HEADER_READ_SECONDS = 8
MAX_AUTHORIZATION_BYTES = 7 + 1024
ERRORS = {400: "bad_request", 401: "unauthorized", 404: "not_found", 503: "unavailable"}

# One statement gives existence and totals from the same MVCC snapshot. Each
# object contributes its NAR size, even when multiple objects share that NAR.
# The extra validity column prevents SUM from hiding NULL or negative sizes.
INVENTORY_SQL = """
SELECT c.name, COUNT(n.id), COALESCE(SUM(n.nar_size), 0),
       COUNT(n.id) FILTER (WHERE n.nar_size IS NULL OR n.nar_size < 0)
FROM public.cache AS c
LEFT JOIN public.object AS o ON o.cache_id = c.id
LEFT JOIN public.nar AS n ON n.id = o.nar_id AND n.state = 'V'
WHERE c.name = %s AND c.deleted_at IS NULL
GROUP BY c.name
"""


def protected_read(path, limit):
    """Read a bounded regular credential file without following symlinks."""
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as source:
        info = os.fstat(source.fileno())
        if (not stat.S_ISREG(info.st_mode) or info.st_mode & 0o077
                or info.st_uid not in (0, os.geteuid()) or info.st_size > limit):
            raise ValueError("invalid configuration")
        data = source.read(limit + 1)
        if len(data) > limit:
            raise ValueError("invalid configuration")
        return data.decode("utf-8", "strict")


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("invalid configuration")
        result[key] = value
    return result


def load_config(database_file, tokens_file):
    """Load restart-scoped credentials; reject controls and duplicate scopes."""
    uri = protected_read(database_file, 8192)
    if (not uri.startswith(("postgresql://", "postgres://"))
            or any(ord(c) < 33 or ord(c) == 127 for c in uri)):
        raise ValueError("invalid configuration")
    tokens = json.loads(protected_read(tokens_file, 65536), object_pairs_hook=unique_pairs)
    if not isinstance(tokens, dict) or not tokens:
        raise ValueError("invalid configuration")
    for cache, token in tokens.items():
        if (not CACHE_NAME.fullmatch(cache) or not isinstance(token, str)
                or not TOKEN.fullmatch(token)):
            raise ValueError("invalid configuration")
    if len(set(tokens.values())) != len(tokens):
        raise ValueError("invalid configuration")
    return uri, {cache: token.encode("ascii") for cache, token in tokens.items()}


def unsigned(value):
    """Accept PostgreSQL integer/numeric aggregates only within the u64 range."""
    if type(value) not in (int, Decimal):
        raise ValueError("invalid inventory")
    if isinstance(value, Decimal) and (not value.is_finite() or value != value.to_integral_value()):
        raise ValueError("invalid inventory")
    if not 0 <= value <= U64_MAX:
        raise ValueError("invalid inventory")
    return int(value)


def inventory(uri, cache, connect=psycopg.connect):
    """Return totals or None for an absent active cache; propagate safe failure."""
    with connect(uri, connect_timeout=2, options="-c statement_timeout=3000 -c lock_timeout=1000",
                 autocommit=True) as connection:
        with connection.transaction():
            connection.execute("SET TRANSACTION READ ONLY")
            row = connection.execute(INVENTORY_SQL, (cache,)).fetchone()
    if row is None:
        return None
    if len(row) != 4 or row[0] != cache or unsigned(row[3]) != 0:
        raise ValueError("invalid inventory")
    return {"schema_version": 1, "cache": cache,
            "cached_path_count": unsigned(row[1]), "logical_nar_bytes": unsigned(row[2])}


class BoundedReader:
    """Bound total request headers and their wall-clock read deadline."""

    def __init__(self, reader, connection, budget=HEADER_READ_SECONDS):
        if type(budget) not in (int, float) or not math.isfinite(budget) or budget <= 0:
            raise ValueError("invalid read budget")
        self.reader, self.connection = reader, connection
        self.remaining = 65536
        self.deadline = time.monotonic() + budget
        self.pending = bytearray()
        self.first_line = True
        self.authorization = []

    def readline(self, size=-1):
        limit = size if size >= 0 else 65537
        while True:
            left = self.deadline - time.monotonic()
            if left <= 0:
                raise ValueError("bad request")
            newline = self.pending.find(b"\n", 0, limit)
            if newline >= 0 or len(self.pending) >= limit:
                length = newline + 1 if newline >= 0 else limit
                line = bytes(self.pending[:length])
                del self.pending[:length]
                break
            if self.remaining <= 0:
                raise ValueError("bad request")
            self.connection.settimeout(left)
            # read1 performs at most one raw read. Buffered readline can perform
            # repeated receives, each renewing a socket timeout under trickle.
            # Charge bytes once on receipt, including pending header records.
            chunk = self.reader.read1(min(4096, self.remaining, limit - len(self.pending)))
            self.remaining -= len(chunk)
            self.pending.extend(chunk)
            if time.monotonic() >= self.deadline:
                raise ValueError("bad request")
            if not chunk:
                line = bytes(self.pending)
                self.pending.clear()
                break
        if not self.first_line and line != b"\r\n":
            if re.fullmatch(rb"[!#$%&'*+.^_`|~0-9A-Za-z-]+:[ -~]*\r\n", line) is None:
                raise ValueError("bad request")
            name, value = line[:-2].split(b":", 1)
            if name.lower() == b"authorization":
                # Do not let email's HTTP parser trim malformed Bearer values.
                self.authorization.append(value[1:].decode("ascii") if value.startswith(b" ")
                                          else value.decode("ascii"))
        self.first_line = False
        if time.monotonic() >= self.deadline:
            raise ValueError("bad request")
        return line

    def close(self):
        self.reader.close()


class Handler(BaseHTTPRequestHandler):
    """Handle exactly one bounded GET without request or exception logging."""

    def setup(self):
        self.request.settimeout(8)
        super().setup()
        self.rfile = BoundedReader(self.rfile, self.connection)

    def log_message(self, *_args):
        pass

    def send_error(self, code, *_args):
        self.reply(code if code in ERRORS else 400)

    def reply(self, code, payload=None):
        body = json.dumps(payload if payload is not None else {"error": ERRORS[code]},
                          separators=(",", ":")).encode("ascii")
        self.close_connection = True
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def handle_one_request(self):
        self.request_version = "HTTP/1.0"
        self.requestline = ""
        self.command = None
        self.close_connection = True
        try:
            self.raw_requestline = self.rfile.readline(1100)
            if not self.raw_requestline:
                return
            if (len(self.raw_requestline) >= 1100
                    or re.fullmatch(rb"GET [!-~]+ HTTP/1\.[01]\r\n", self.raw_requestline) is None):
                self.reply(400)
                return
            if not self.parse_request():
                return
            # Preserve the exact request target: stdlib normalizes leading //.
            self.path = self.raw_requestline.split(b" ")[1].decode("ascii")
            self.connection.settimeout(8)
            self.do_GET()
        except (OSError, ValueError):
            try:
                self.reply(400)
            except OSError:
                pass
        finally:
            self.close_connection = True

    def do_GET(self):
        if (len(self.path) > 1024 or self.headers.get_all("Transfer-Encoding")
                or self.headers.get_all("Content-Length") not in (None, ["0"])):
            self.reply(400)
            return
        # Reject folded/control-bearing headers before inspecting authentication.
        if any(any(ord(c) < 32 or ord(c) == 127 for c in value)
               for _, value in self.headers.items()):
            self.reply(400)
            return
        match = re.fullmatch(r"/v1/caches/([^/?#]+)/inventory", self.path)
        if not match or re.search(r"%(?![0-9a-fA-F]{2})", match[1]):
            self.reply(400)
            return
        try:
            cache = unquote_to_bytes(match[1]).decode("ascii")
        except UnicodeError:
            self.reply(400)
            return
        if not CACHE_NAME.fullmatch(cache):
            self.reply(400)
            return
        auth = self.rfile.authorization
        if (len(auth) != 1 or len(auth[0]) > MAX_AUTHORIZATION_BYTES
                or not auth[0].startswith("Bearer ")):
            self.reply(401)
            return
        supplied = auth[0][7:]
        expected = self.server.tokens.get(cache)
        if (not TOKEN.fullmatch(supplied)
                or not hmac.compare_digest(supplied.encode("ascii"), expected or b"")
                or expected is None):
            self.reply(401)
            return
        try:
            result = self.server.provider(self.server.database_uri, cache)
        except Exception:
            self.reply(503)
            return
        self.reply(404 if result is None else 200, result)


class Server(ThreadingHTTPServer):
    """Reserve at most four workers before spawning any request thread."""

    daemon_threads = True

    def __init__(self, address, uri, tokens, provider=inventory):
        ip = ipaddress.ip_address(address[0])
        if not ip.is_loopback:
            raise ValueError("loopback address required")
        self.address_family = socket.AF_INET6 if ip.version == 6 else socket.AF_INET
        self.database_uri, self.tokens, self.provider = uri, tokens, provider
        self.slots = threading.BoundedSemaphore(4)
        super().__init__(address, Handler)

    def process_request(self, request, client_address):
        if not self.slots.acquire(blocking=False):
            try:
                request.settimeout(0.1)
                body = b'{"error":"unavailable"}'
                request.sendall(b"HTTP/1.0 503 Service Unavailable\r\nContent-Type: application/json\r\n"
                                b"Connection: close\r\nContent-Length: 23\r\n\r\n" + body)
            except OSError:
                pass
            finally:
                self.shutdown_request(request)
            return
        try:
            super().process_request(request, client_address)
        except Exception:
            self.slots.release()
            self.shutdown_request(request)

    def process_request_thread(self, request, client_address):
        try:
            super().process_request_thread(request, client_address)
        finally:
            self.slots.release()

    def handle_error(self, *_args):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--database-uri-file", required=True)
    parser.add_argument("--tokens-file", required=True)
    parser.add_argument("--listen", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=9121)
    args = parser.parse_args()
    try:
        uri, tokens = load_config(args.database_uri_file, args.tokens_file)
        if not 1 <= args.port <= 65535:
            raise ValueError("invalid port")
        with Server((args.listen, args.port), uri, tokens) as server:
            server.serve_forever()
    except Exception:
        print("cf-attic-metrics: startup failed", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
