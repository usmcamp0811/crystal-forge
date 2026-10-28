"""No-DSN tests: fake PostgreSQL connections and ephemeral loopback HTTP."""

from contextlib import nullcontext
from decimal import Decimal
import io
import json
from pathlib import Path
import socket
import tempfile
import threading
import time
import unittest

import attic_metrics as m


class FakeConnection:
    def __init__(self, row):
        self.row, self.calls = row, []

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        pass

    def transaction(self):
        return nullcontext()

    def execute(self, sql, params=None):
        self.calls.append((sql, params))
        return self

    def fetchone(self):
        return self.row


class InventoryTests(unittest.TestCase):
    def test_snapshot_contract_empty_populated_shared(self):
        for count, total in [(0, 0), (3, 42), (2, 20)]:
            connection = FakeConnection(("team", count, Decimal(total), 0))
            def connect(uri, **kwargs):
                self.assertEqual(uri, "fake-only")
                self.assertEqual(kwargs, {"connect_timeout": 2, "autocommit": True,
                    "options": "-c statement_timeout=3000 -c lock_timeout=1000"})
                return connection
            self.assertEqual(m.inventory("fake-only", "team", connect), {
                "schema_version": 1, "cache": "team", "cached_path_count": count,
                "logical_nar_bytes": total})
            self.assertEqual(connection.calls, [("SET TRANSACTION READ ONLY", None),
                                              (m.INVENTORY_SQL, ("team",))])
        sql = m.INVENTORY_SQL
        self.assertIn("COUNT(n.id)", sql)
        self.assertIn("SUM(n.nar_size)", sql)
        self.assertIn("n.state = 'V'", sql)
        self.assertIn("c.deleted_at IS NULL", sql)
        self.assertIn("GROUP BY c.name", sql)
        self.assertEqual(sql.count("LEFT JOIN"), 2)
        self.assertNotIn("DISTINCT", sql)
        self.assertNotIn("chunk", sql)

    def test_absent_and_invalid(self):
        self.assertIsNone(m.inventory("fake", "team", lambda *a, **k: FakeConnection(None)))
        for row in [("other", 0, 0, 0), ("team", 2, 1, 1), ("team", 0, None, 0),
                    ("team", -1, 0, 0), ("team", 0, -1, 0),
                    ("team", 0, m.U64_MAX + 1, 0), ("team", True, 0, 0),
                    ("team", 0, Decimal("1.5"), 0), ("team", 0, Decimal("NaN"), 0)]:
            with self.subTest(row=row), self.assertRaises(ValueError):
                m.inventory("fake", "team", lambda *a, **k: FakeConnection(row))
        self.assertEqual(m.unsigned(Decimal(m.U64_MAX)), m.U64_MAX)


class ConfigTests(unittest.TestCase):
    def test_protected_bounds_and_validation(self):
        with tempfile.TemporaryDirectory() as directory:
            db, tokens = Path(directory)/"db", Path(directory)/"tokens"
            db.write_text("postgresql://metrics:synthetic@127.0.0.1/attic")
            tokens.write_text(json.dumps({"team": "x" * 32}))
            for path in (db, tokens):
                path.chmod(0o600)
            self.assertEqual(m.load_config(db, tokens)[1], {"team": b"x" * 32})
            for mapping in [{"team": ""}, {"team": "x" * 31}, {"team": "x"*32+"\n"},
                            {"../team": "x"*32}, {"a"*51: "x"*32},
                            {"_team": "x"*32}, {"-team": "x"*32},
                            {"team": "x"*1025},
                            *({"team": "x"*32 + c} for c in ".~+/="),
                            {"team": 123}, [], {"team": "x"*32, "other": "x"*32}]:
                tokens.write_text(json.dumps(mapping))
                with self.assertRaises(ValueError):
                    m.load_config(db, tokens)
            tokens.write_text('{"team":"' + "x"*32 + '","team":"' + "y"*32 + '"}')
            with self.assertRaises(ValueError):
                m.load_config(db, tokens)
            for token in ("a_b-" * 8, "x"*1024):
                tokens.write_text(json.dumps({"Team+1": token}))
                self.assertEqual(m.load_config(db, tokens)[1], {"Team+1": token.encode()})
            tokens.chmod(0o644)
            with self.assertRaises(ValueError):
                m.load_config(db, tokens)
            tokens.chmod(0o600)
            for size in (8192, 8193):
                db.write_text("x" * size)
                if size == 8192:
                    self.assertEqual(len(m.protected_read(db, 8192)), size)
                else:
                    with self.assertRaises(ValueError):
                        m.protected_read(db, 8192)
            tokens.write_text("x"*65537)
            with self.assertRaises(ValueError):
                m.protected_read(tokens, 65536)
            db.write_text("postgresql://localhost/attic\n")
            with self.assertRaises(ValueError):
                m.load_config(db, tokens)
            link = Path(directory)/"link"
            link.symlink_to(db)
            with self.assertRaises(OSError):
                m.protected_read(link, 8192)

    def test_loopback_only(self):
        for address in ("0.0.0.0", "::", "192.0.2.1", "localhost"):
            with self.assertRaises(ValueError):
                m.Server((address, 0), "fake", {})


class HttpTests(unittest.TestCase):
    def setUp(self):
        self.calls = []
        self.result = {"schema_version": 1, "cache": "team", "cached_path_count": 2,
                       "logical_nar_bytes": 20}
        def provider(uri, cache):
            self.calls.append((uri, cache))
            if isinstance(self.result, Exception):
                raise self.result
            return self.result
        self.server = m.Server(("127.0.0.1", 0), "fake-only",
                               {"team": b"x"*32, "other": b"y"*32}, provider)
        self.thread = threading.Thread(target=self.server.serve_forever)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.thread.join()
        self.server.server_close()

    def request(self, path="/v1/caches/team/inventory", headers=None, method="GET"):
        if headers is None:
            headers = "Authorization: Bearer " + "x"*32 + "\r\n"
        with socket.create_connection(self.server.server_address, timeout=2) as client:
            client.sendall((f"{method} {path} HTTP/1.0\r\n" + headers + "\r\n").encode("latin1"))
            chunks = []
            while chunk := client.recv(65536):
                chunks.append(chunk)
        response = b"".join(chunks)
        status = int(response.split(b" ")[1])
        body = response.split(b"\r\n\r\n", 1)[1]
        return status, json.loads(body)

    def test_auth_isolation_and_percent_encoding(self):
        for headers in ("", "Authorization: Basic abc\r\n",
                        "Authorization: Bearer " + "y"*32 + "\r\n",
                        "Authorization: Bearer " + "x"*32 + "\r\nAuthorization: Bearer " + "x"*32 + "\r\n",
                        "Authorization: Bearer " + "x"*1025 + "\r\n",
                        "Authorization: Bearer " + "x"*32 + " \r\n",
                        "Authorization: Bearer  " + "x"*32 + "\r\n"):
            self.assertEqual(self.request(headers=headers)[0], 401)
        self.assertEqual(self.request("/v1/caches/unknown/inventory")[0], 401)
        self.assertEqual(self.request("/v1/caches/other/inventory")[0], 401)
        self.assertEqual(self.calls, [])
        self.result = dict(self.result, cache="other")
        self.assertEqual(self.request("/v1/caches/other/inventory",
                         "Authorization: Bearer " + "y"*32 + "\r\n")[0], 200)
        self.calls.clear()
        self.result = dict(self.result, cache="team")
        self.assertEqual(self.request("/v1/caches/%74eam/inventory"), (200, self.result))
        self.assertEqual(self.calls, [("fake-only", "team")])
        self.result = None
        self.assertEqual(self.request(), (404, {"error": "not_found"}))

    def test_cache_and_token_grammar(self):
        for token in ("x"*32, "a_b-"*256):
            self.server.tokens["Team+1"] = token.encode()
            self.result = dict(self.result, cache="Team+1")
            self.assertEqual(self.request("/v1/caches/Team%2B1/inventory",
                             "Authorization: Bearer " + token + "\r\n"), (200, self.result))
        self.calls.clear()
        for token in ("x"*31, "x"*1025, *("x"*32+c for c in ".~+/=")):
            self.assertEqual(self.request("/v1/caches/Team%2B1/inventory",
                             "Authorization: Bearer " + token + "\r\n")[0], 401)
        for cache in ("_team", "-team", "%2Bteam", "a"*51):
            self.assertEqual(self.request(f"/v1/caches/{cache}/inventory")[0], 400)
        self.assertEqual(self.calls, [])

    def test_bad_uri_headers_methods_bounds(self):
        for path in ("/", "//v1/caches/team/inventory", "/v1/caches/%GG/inventory", "/v1/caches/%00/inventory",
                     "/v1/caches/%ff/inventory", "/v1/caches/a%2fb/inventory",
                     "/v1/caches/team/inventory?x=1", "/" + "x"*1024):
            self.assertEqual(self.request(path)[0], 400)
        for headers in ("Authorization: Bearer xx\x01\r\n", "X-A: a\r\n folded\r\n",
                        "X-A: " + "x"*65536 + "\r\n", "Content-Length: 1\r\n",
                        "Transfer-Encoding: chunked\r\n"):
            self.assertEqual(self.request(headers=headers)[0], 400)
        self.assertEqual(self.request(method="POST")[0], 400)
        self.assertEqual(self.calls, [])

    def test_failure_is_static(self):
        self.result = RuntimeError("provider-secret-uri-password-header")
        self.assertEqual(self.request(), (503, {"error": "unavailable"}))

    def test_four_slots_before_spawn_and_release(self):
        for _ in range(4):
            self.assertTrue(self.server.slots.acquire(False))
        self.assertEqual(self.request(), (503, {"error": "unavailable"}))
        self.assertEqual(self.calls, [])
        for _ in range(4):
            self.server.slots.release()
        self.assertEqual(self.request()[0], 200)

    def test_real_four_worker_admission(self):
        entered = threading.Barrier(5)
        release = threading.Event()
        results = []
        def provider(_uri, _cache):
            entered.wait(timeout=3)
            release.wait(timeout=3)
            return self.result
        self.server.provider = provider
        workers = [threading.Thread(target=lambda: results.append(self.request()[0]))
                   for _ in range(4)]
        for worker in workers:
            worker.start()
        try:
            entered.wait(timeout=3)
            self.assertEqual(self.request(), (503, {"error": "unavailable"}))
        finally:
            release.set()
            for worker in workers:
                worker.join(timeout=3)
        self.assertEqual(sorted(results), [200]*4)


class ReaderTests(unittest.TestCase):
    def test_budget_validation(self):
        for budget in (0, -1, float("inf"), float("nan"), True, "8"):
            with self.assertRaises(ValueError):
                m.BoundedReader(None, None, budget)

    def test_pending_records_and_shared_byte_limit(self):
        receiver, sender = socket.socketpair()
        with receiver, sender:
            # This case uses real BufferedReader semantics without a network wait.
            reader = m.BoundedReader(io.BufferedReader(io.BytesIO(b"GET / HTTP/1.0\r\nX-A: a\r\n\r\n")), receiver)
            self.assertEqual(reader.readline(1100), b"GET / HTTP/1.0\r\n")
            remaining = reader.remaining
            self.assertEqual(reader.readline(), b"X-A: a\r\n")
            self.assertEqual(reader.remaining, remaining)
            self.assertEqual(reader.pending, b"\r\n")
            reader.deadline = time.monotonic() - 1
            with self.assertRaises(ValueError):
                reader.readline()
            oversized = m.BoundedReader(io.BufferedReader(io.BytesIO(b"x"*65537)), receiver)
            with self.assertRaises(ValueError):
                oversized.readline()
            self.assertEqual(oversized.remaining, 0)
            self.assertEqual(len(oversized.pending), 65536)
            limited = m.BoundedReader(io.BufferedReader(io.BytesIO(b"x"*1200)), receiver)
            self.assertEqual(len(limited.readline(1100)), 1100)
            self.assertEqual(limited.remaining, 65536 - 1100)

    def test_socketpair_trickle_absolute_deadline(self):
        # The finite sender lasts four seconds, with gaps shorter than the read
        # budget. The scheduling allowance still excludes a per-recv timeout.
        for prefix in (b"", b"GET / HTTP/1.0\r\n"):
            with self.subTest(header=bool(prefix)):
                receiver, sender = socket.socketpair()
                stop = threading.Event()
                ready = threading.Event()
                done = threading.Event()
                outcome = []
                budget = 0.3
                allowance = 1.2
                sender.sendall(prefix)
                def consume():
                    started = time.monotonic()
                    try:
                        # Start the deadline in its consuming thread. Producer
                        # and assertions wait until that reader is ready.
                        with receiver.makefile("rb") as raw:
                            reader = m.BoundedReader(raw, receiver, budget=budget)
                            ready.set()
                            if prefix:
                                reader.readline(1100)
                            reader.readline(1100 if not prefix else 65537)
                            outcome.append("unexpected success")
                    except (OSError, ValueError):
                        outcome.append(time.monotonic() - started)
                    finally:
                        done.set()
                def trickle():
                    if not ready.wait(timeout=3):
                        return
                    for _ in range(200):
                        if stop.wait(0.02):
                            break
                        try:
                            sender.sendall(b"x")
                        except OSError:
                            break
                consumer = threading.Thread(target=consume)
                producer = threading.Thread(target=trickle)
                consumer.start()
                producer.start()
                try:
                    self.assertTrue(ready.wait(3), "consumer did not initialize reader")
                    self.assertTrue(done.wait(allowance), "trickle exceeded absolute read budget")
                    self.assertEqual(len(outcome), 1)
                    self.assertIsInstance(outcome[0], float)
                    self.assertGreaterEqual(outcome[0], budget * 0.7)
                    self.assertLess(outcome[0], allowance)
                finally:
                    stop.set()
                    receiver.shutdown(socket.SHUT_RDWR)
                    sender.close()
                    producer.join(timeout=1)
                    consumer.join(timeout=1)
                    receiver.close()
                self.assertFalse(producer.is_alive())
                self.assertFalse(consumer.is_alive())


if __name__ == "__main__":
    unittest.main()
