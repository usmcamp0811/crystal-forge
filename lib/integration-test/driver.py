# Shared NixOS driver helpers for Crystal Forge integration checks.
#
# The NixOS test driver executes this file with its own symbol table
# (`machines`, `start_all`, `driver`, and one variable per node). The Nix
# wrapper defines CF_IT_META, CF_IT_FACTS, and CF_IT_BODY before this file.
#
# Execution contract:
#
# - The scenario body (CF_IT_BODY) runs once. Every `case(...)` block is a
#   named, mandatory case. The first failure stops the scenario.
# - The outcome is written to $out/result.json with status "passed" or
#   "failed". INVARIANT: this file records failures instead of re-raising so
#   that evidence survives. The gate derivation in lib/default.nix fails
#   unless the status is exactly "passed". Nothing here may convert a failure
#   into "passed".
# - Expected values come from FACTS (authored input). Generated IDs and
#   paths are stored with `receipt(...)` and asserted by relationship only.
# - Every wait is bounded and names the condition it waits for.

import base64
import contextlib
import json
import shlex
import time
import traceback

OUT = driver.out_dir  # noqa: F821 - provided by the NixOS test driver
META = json.load(open(CF_IT_META))  # noqa: F821
FACTS = json.load(open(CF_IT_FACTS))  # noqa: F821

_RESULT = {
    "owner": META["owner"],
    "scenario": META["name"],
    "contracts": META["contracts"],
    "sourceRevision": META["sourceRevision"],
    "renderProfile": META["renderProfile"]["name"],
    "status": "running",
    "cases": [],
    "faults": [],
    "receipts": {},
    "error": None,
}


class ScenarioFailure(AssertionError):
    """Raised for a failed scenario assertion with a readable message."""


def _log(message):
    print(f"[cf-it {META['owner']}/{META['name']}] {message}", flush=True)


def expect(condition, message):
    """Fails the current case with `message` when `condition` is false."""
    if not condition:
        raise ScenarioFailure(message)


def expect_equal(actual, expected, what):
    """Fails unless `actual == expected`; the message names both values."""
    if actual != expected:
        raise ScenarioFailure(f"{what}: expected {expected!r}, got {actual!r}")


@contextlib.contextmanager
def case(name):
    """Runs one named mandatory case and records its status and duration."""
    started = time.monotonic()
    entry = {"name": name, "status": "running", "seconds": 0}
    _RESULT["cases"].append(entry)
    _log(f"START case {name}")
    try:
        yield
    except BaseException:
        entry["status"] = "failed"
        raise
    finally:
        entry["seconds"] = round(time.monotonic() - started, 1)
        if entry["status"] == "running":
            entry["status"] = "passed"
        _log(f"END case {name}: {entry['status']} ({entry['seconds']}s)")


def receipt(key, value):
    """Records a runtime-generated identity (ID, store path, URL).

    A key is write-once. Recording a different value for an existing key
    fails, because two values for one identity indicate a wrong selection.
    """
    old = _RESULT["receipts"].get(key)
    if old is not None and old != value:
        raise ScenarioFailure(f"receipt {key!r} changed from {old!r} to {value!r}")
    _RESULT["receipts"][key] = value
    return value


def receipts():
    """Returns a copy of all recorded receipts."""
    return dict(_RESULT["receipts"])


def wait_for(predicate, what, timeout, interval=1.0):
    """Polls `predicate()` until it returns a truthy value or `timeout` s pass.

    Returns the truthy value. Exceptions from `predicate` count as "not yet"
    and the last one is included in the timeout message.
    """
    deadline = time.monotonic() + timeout
    last_error = None
    while True:
        try:
            value = predicate()
            if value:
                return value
        except Exception as exc:  # noqa: BLE001 - reported on timeout
            last_error = exc
        if time.monotonic() >= deadline:
            detail = f"; last error: {last_error!r}" if last_error else ""
            raise ScenarioFailure(f"timed out after {timeout}s waiting for {what}{detail}")
        time.sleep(interval)


def _vm_write(node, path, text):
    data = base64.b64encode(text.encode()).decode()
    node.succeed(f"mkdir -p $(dirname {shlex.quote(path)}) && echo {data} | base64 -d > {shlex.quote(path)}")


def sql_json(node, query, database="crystal_forge"):
    """Runs a read query as the postgres superuser; returns a list of dicts.

    Use only for authoritative reads of persisted results. Seeding the
    successful result of the action under test is prohibited by the contract.
    """
    _vm_write(node, "/tmp/cf-it/query.sql", f"SELECT coalesce(json_agg(t), '[]'::json) FROM ({query}) t;\n")
    out = node.succeed(
        f"runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 -d {shlex.quote(database)} -At -f /tmp/cf-it/query.sql"
    )
    return json.loads(out.strip() or "[]")


def http(node, method, path, body=None, session=None, csrf=False, base="http://localhost:3000"):
    """Sends one HTTP request from `node` with curl; returns (status, body).

    `session` names a cookie jar on the node (/tmp/cf-it/jar-<session>.txt)
    that persists cookies between calls. With `csrf=True` the request sends
    the double-submit `x-csrf-token` header from the jar's `__Host-cf-csrf`
    cookie. The body is parsed as JSON when possible.
    """
    args = ["curl", "-sS", "-o", "/tmp/cf-it/http-body", "-w", "%{http_code}", "-X", method]
    if session:
        jar = f"/tmp/cf-it/jar-{session}.txt"
        args += ["-b", jar, "-c", jar]
        if csrf:
            token = node.succeed(
                f"awk '$6 == \"__Host-cf-csrf\" {{ print $7 }}' {jar} 2>/dev/null | tail -n1"
            ).strip()
            expect(token, f"session {session!r} has no __Host-cf-csrf cookie for a CSRF request")
            args += ["-H", f"x-csrf-token: {token}"]
    if body is not None:
        _vm_write(node, "/tmp/cf-it/http-request.json", json.dumps(body))
        args += ["-H", "content-type: application/json", "--data-binary", "@/tmp/cf-it/http-request.json"]
    args.append(base + path)
    node.succeed("mkdir -p /tmp/cf-it")
    status = int(node.succeed(shlex.join(args)).strip() or "0")
    text = node.succeed("cat /tmp/cf-it/http-body")
    try:
        return status, json.loads(text)
    except ValueError:
        return status, text


def run_browser(node, spec, viewport="desktop", base_url="http://localhost:3000"):
    """Runs the browser spec /etc/cf-it/specs/<spec>.cjs on `node`.

    Current receipts are passed to the spec. Receipts emitted by the spec are
    merged with `receipt` (write-once). Artifacts are copied to
    $out/browser/<spec>/ whether the spec passes or fails. A failing spec
    raises ScenarioFailure with the spec's error and failing step.
    """
    work = f"/tmp/cf-it/browser/{spec}"
    _vm_write(node, "/tmp/cf-it/receipts-in.json", json.dumps(receipts()))
    status, output = node.execute(
        "cf-browser "
        + shlex.join([
            "--spec", f"/etc/cf-it/specs/{spec}.cjs",
            "--case", spec,
            "--base-url", base_url,
            "--out", work,
            "--facts", "/etc/cf-it/facts.json",
            "--receipts", "/tmp/cf-it/receipts-in.json",
            "--viewport", viewport,
        ])
        + " 2>&1",
        timeout=600,
    )
    print(output, flush=True)
    node.execute(f"mkdir -p {work}")
    node.copy_from_vm(work, "browser")
    result_path = OUT / "browser" / spec / "result.json"
    result = json.load(open(result_path)) if result_path.exists() else {}
    for key, value in (result.get("receipts") or {}).items():
        receipt(key, value)
    if status != 0:
        raise ScenarioFailure(
            f"browser spec {spec!r} exited {status} at step {result.get('step')!r}: "
            f"{result.get('error') or output[-2000:]}"
        )
    return result


def assert_store_absent(node, path):
    """Fails unless `path` is neither valid nor present in `node`'s store.

    Both checks are required because host store paths are visible as files
    in every VM even when the VM's Nix database does not register them.
    """
    valid, _ = node.execute(f"nix-store --check-validity {shlex.quote(path)} 2>/dev/null")
    present, _ = node.execute(f"test -e {shlex.quote(path)}")
    expect(valid != 0, f"{path} is already valid in {node.name}'s store")
    expect(present != 0, f"{path} already exists as a file in {node.name}'s store")


def _record_fault(kind, node, detail):
    entry = {"kind": kind, "node": node.name, "detail": detail, "at": time.time()}
    _RESULT["faults"].append(entry)
    _log(f"FAULT {kind} on {node.name}: {detail}")


@contextlib.contextmanager
def unit_stopped(node, unit):
    """Injects a controlled outage: stops `unit`, then starts it again."""
    _record_fault("unit-stopped", node, unit)
    node.succeed(f"systemctl stop {shlex.quote(unit)}")
    try:
        yield
    finally:
        node.succeed(f"systemctl start {shlex.quote(unit)}")


def unit_killed(node, unit, signal="KILL"):
    """Injects an abrupt process failure with `systemctl kill -s <signal>`."""
    _record_fault("unit-killed", node, f"{unit} signal={signal}")
    node.succeed(f"systemctl kill -s {shlex.quote(signal)} {shlex.quote(unit)}")


@contextlib.contextmanager
def network_blocked(node, host, port=None):
    """Injects a network partition: `node` rejects traffic to `host`[:port]."""
    rule = f"OUTPUT -d $(getent ahostsv4 {shlex.quote(host)} | awk 'NR==1{{print $1}}') -p tcp"
    if port is not None:
        rule += f" --dport {int(port)}"
    rule += " -j REJECT"
    _record_fault("network-blocked", node, f"{host}:{port or '*'}")
    node.succeed(f"iptables -I {rule}")
    try:
        yield
    finally:
        node.succeed(f"iptables -D {rule}")


def _collect_diagnostics():
    for m in machines:  # noqa: F821
        if not getattr(m, "booted", False):
            continue
        name = m.name
        try:
            m.execute(
                "mkdir -p /tmp/cf-it/diag && "
                "systemctl --no-pager --failed > /tmp/cf-it/diag/failed-units.txt 2>&1; "
                "journalctl -b --no-pager -o short-iso -u 'crystal-forge*' -u postgresql "
                "> /tmp/cf-it/diag/crystal-forge.journal 2>&1; "
                "journalctl -b --no-pager -o short-iso -p warning "
                "> /tmp/cf-it/diag/warnings.journal 2>&1",
                timeout=120,
            )
            m.execute(f"rm -rf /tmp/cf-it/{name} && cp -r /tmp/cf-it/diag /tmp/cf-it/{name}", timeout=60)
            m.copy_from_vm(f"/tmp/cf-it/{name}", "journals")
        except Exception as exc:  # noqa: BLE001 - diagnostics are best effort
            _log(f"diagnostic collection failed on {name}: {exc!r}")


def _write_result():
    with open(OUT / "result.json", "w") as handle:
        json.dump(_RESULT, handle, indent=2, sort_keys=True)
    with open(OUT / "meta.json", "w") as handle:
        json.dump(META, handle, indent=2, sort_keys=True)


_log(f"source {META['sourceRevision']} profile {META['renderProfile']['name']}")
try:
    _body = open(CF_IT_BODY).read()  # noqa: F821
    exec(compile(_body, CF_IT_BODY, "exec"), globals())  # noqa: F821
    _RESULT["status"] = "passed"
except BaseException as _exc:  # noqa: BLE001 - recorded; the gate fails
    _RESULT["status"] = "failed"
    _RESULT["error"] = f"{type(_exc).__name__}: {_exc}"
    _RESULT["traceback"] = traceback.format_exc()
    _log("FAILED: " + _RESULT["error"])
    print(_RESULT["traceback"], flush=True)
finally:
    _collect_diagnostics()
    _write_result()
    _log(f"status {_RESULT['status']}; evidence in {OUT}")
