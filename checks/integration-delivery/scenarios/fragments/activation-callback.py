# Runs AFTER the role/browser fragment: this separate enrollment must not alter
# its exact membership assertions. Proves real systemd ExecStopPost and signed
# production reporting, not invocation of Nix or switch-to-configuration.
import re
import uuid


def callback_snapshot():
    sid = literal(receipts()["callback.systemId"]) + "::uuid"
    hostname = literal(receipts()["callback.hostname"])
    return {
        "system": sql_json(controller, "SELECT * FROM systems WHERE id=" + sid),
        "pending": sql_json(controller, "SELECT * FROM pending_system_deployments WHERE system_id=" + sid + " ORDER BY id"),
        "events": sql_json(controller, "SELECT * FROM system_events WHERE system_id=" + sid + " ORDER BY id"),
        "states": sql_json(controller, "SELECT * FROM system_states WHERE hostname=" + hostname + " ORDER BY id"),
        "heartbeats": sql_json(controller, "SELECT h.* FROM agent_heartbeats h JOIN system_states s "
            "ON s.id=h.system_state_id WHERE s.hostname=" + hostname + " ORDER BY h.id"),
    }


def callback_responses():
    return json.loads(controller.succeed("cat /run/cf-it-callback/responses.json"))


def callback_request_count():
    return json.loads(controller.succeed("cat /run/cf-it-callback/request-count.json"))


def callback_unit(exit_status, label, deployment_id=None):
    unit = "cf-it-activation-callback-" + label + ".service"
    # Exact production callback parser/prefix. Config is imported by NAME from
    # systemd-run's environment; no seed, signature or config value enters argv.
    callback = ('ExecStopPost=-:"' + FACTS["agentBinary"] +
                '" --report-activation-result --target "' + FACTS["outputPath"] + '"')
    if deployment_id is not None:
        # Same unquoted UUID suffix as the production callback property.
        callback += " --deployment-id " + deployment_id
    command = shlex.join([
        "systemd-run", "--unit", unit, "--wait", "--same-dir",
        "--property", callback, "--setenv=CRYSTAL_FORGE_CONFIG",
        "--", "/bin/sh", "-c", "exit " + str(exit_status),
    ])
    # --wait observes completion INCLUDING ExecStopPost and propagates the main
    # process status. Unlike production --collect, retain failed-unit properties
    # to check the reporter's own exit despite its ignored-failure prefix.
    status, output = controller.execute("CRYSTAL_FORGE_CONFIG=/run/cf-it-callback/config.toml " + command, timeout=30)
    expect_equal(status, exit_status, "systemd-run observes completed main process status")
    properties = {"systemdRunExitStatus": status, "output": output}
    if exit_status != 0:
        raw = controller.succeed("systemctl show " + unit +
            " -p Result -p ExecMainCode -p ExecMainStatus -p ExecStopPost")
        properties.update(line.split("=", 1) for line in raw.splitlines() if "=" in line)
        expect_equal(properties["Result"], "exit-code", "observed systemd service result")
        expect_equal(properties["ExecMainCode"], "1", "systemd observed CLD_EXITED")
        expect_equal(properties["ExecMainStatus"], str(exit_status), "observed main process exit status")
        post = properties["ExecStopPost"]
        expect_equal(post.count("path="), 1, "one real production reporter callback")
        expect("path=" + FACTS["agentBinary"] + " ;" in post, "systemd executed the selected production agent")
        expect(bool(re.search(r"; pid=[1-9][0-9]* ; code=exited ; status=0(?: ;| \})", post)),
               "ExecStopPost reporter exited successfully despite ignored-failure prefix")
    # A successful transient unit may already be garbage-collected. The wait
    # result, response recorder and persistence snapshot are its authorities.
    receipt("callback." + label + ".unitResult", properties)
    return unit


with case("activation-callback-enrollment-and-stopped-heartbeat-process"):
    directory = "/run/cf-it-callback"
    controller.succeed("install -d -m 0700 " + directory)
    public_key = controller.succeed("node /etc/cf-it/failure-report.cjs keygen " + directory + "/private.pem").strip()
    # The production reporter derives its authenticated identity from gethostname.
    hostname = controller.succeed("hostname").strip()
    expect(hostname != receipts()["failure.hostname"], "callback enrollment is independent of browser probe")
    probe = delivery_api("POST", "/api/v1/systems", {
        "hostname": hostname, "public_key": public_key, "environment": "delivery-proof",
        "flake_name": FACTS["flake"]["name"], "system_configuration_name": FACTS["target"],
        "deployment_policy": "manual",
    }, expected=201)
    receipt("callback.systemId", probe["id"])
    receipt("callback.hostname", hostname)
    controller.succeed("node /etc/cf-it/activation-callback.cjs config " + directory + " " + directory + "/private.pem")
    controller.succeed("test $(stat -c %a " + directory + "/seed) = 600 && "
                       "test $(stat -c %a " + directory + "/config.toml) = 600")
    controller.succeed("systemd-run --unit=cf-it-callback-relay /run/current-system/sw/bin/node "
                       "/etc/cf-it/activation-callback.cjs relay " + directory)
    controller.wait_for_open_port(3101)
    controller.succeed("systemctl start cf-it-callback-heartbeat.service")
    controller.wait_for_unit("cf-it-callback-heartbeat.service")
    wait_for(lambda: sql_json(controller, "SELECT id FROM system_states WHERE hostname=" + literal(hostname)),
             "representative production heartbeat agent reports its authenticated startup state", timeout=120)
    controller.succeed("systemctl stop cf-it-callback-heartbeat.service")
    controller.succeed("test \"$(systemctl show cf-it-callback-heartbeat.service -p ActiveState --value)\" = inactive && "
                       "test \"$(systemctl show cf-it-callback-heartbeat.service -p MainPID --value)\" = 0")
    expect_equal(callback_responses(), [], "heartbeat setup sent no failure reports")
    expect_equal(callback_request_count(), 0, "heartbeat setup attempted no failure reports")
    receipt("callback.heartbeatStopped", True)

with case("systemd-exec-stop-post-reports-observed-activation-exit-17"):
    request = delivery_api("POST", "/api/v1/systems/" + receipts()["callback.systemId"] + "/deploy", {
        "commit_sha": FACTS["sourceRevision"], "action": "deploy", "request_id": str(uuid.uuid4()),
    }, expected=202)
    before = callback_snapshot()
    expect_equal(next(row for row in before["pending"] if row["id"] == request["deployment_id"])["status"],
                 "pending", "real API creates callback deployment prerequisite")
    callback_unit(17, "failure")
    expect_equal(callback_request_count(), 1, "one detached production failure report request")
    expect_equal(callback_responses(), [{"status": 200, "body": {
        "status": "ok", "message": "Deployment failure recorded",
    }}], "real signed production report receives the real server response")
    after = callback_snapshot()
    pending = next(row for row in after["pending"] if row["id"] == request["deployment_id"])
    expect_equal(pending["status"], "failed", "callback failed exact pending deployment")
    expect(bool(pending["failed_at"]) and bool(pending["completed_at"]), "callback persisted terminal timestamps")
    events = [row for row in after["events"] if row["deployment_id"] == request["deployment_id"]]
    expect_equal(len(events), 1, "callback invents no evaluation, activation-start or rollback events")
    expect_equal(len(after["events"]), len(before["events"]) + 1, "only the observed failure event was added")
    expect_equal(events[0]["event_type"], "cf_deployment_failed", "callback failure event")
    expect_equal(events[0]["new_store_path"], FACTS["outputPath"], "callback target identity")
    expect_equal(events[0]["new_generation"], None, "no Nix generation was activated")
    typed = events[0]["metadata"]["deployment_failure"]
    expect_equal(typed["stage"], "activation", "observed execution failure stage")
    expect_equal(typed["code"], "activation_execution", "observed execution failure code")
    expect_equal(typed["exit_code"], 17, "observed numeric exit status persisted")
    # Manual failure legitimately clears the matching desired target and updates
    # its timestamp; it does not change enrollment, policy or observed state.
    expect_equal(before["system"][0]["desired_target"], FACTS["outputPath"], "manual request target before failure")
    expected_system = {**before["system"][0], "desired_target": None, "desired_target_set_at": None,
                       "updated_at": after["system"][0]["updated_at"]}
    expect_equal(after["system"], [expected_system], "only the failed manual desired target is cleared")
    for field in ("states", "heartbeats"):
        expect_equal(after[field], before[field], "detached reporter does not fabricate " + field)
    controller.succeed("test \"$(systemctl show cf-it-callback-heartbeat.service -p ActiveState --value)\" = inactive")
    receipt("callback.failure", {"deploymentId": request["deployment_id"], "eventId": events[0]["id"],
                                 "failure": typed, "response": callback_responses()[0]})

with case("successful-systemd-exec-stop-post-sends-no-report-and-does-not-mutate"):
    request = delivery_api("POST", "/api/v1/systems/" + receipts()["callback.systemId"] + "/deploy", {
        "commit_sha": FACTS["sourceRevision"], "action": "deploy", "request_id": str(uuid.uuid4()),
    }, expected=202)
    before = callback_snapshot()
    expect_equal(next(row for row in before["pending"] if row["id"] == request["deployment_id"])["status"],
                 "pending", "success control has a reportable pending deployment")
    responses = callback_responses()
    requests = callback_request_count()
    controller.wait_for_unit("cf-it-callback-relay.service")
    callback_unit(0, "success")
    controller.wait_for_unit("cf-it-callback-relay.service")
    expect_equal(callback_request_count(), requests, "successful callback sends no request, including failed attempts")
    expect_equal(callback_responses(), responses, "successful production callback sends no failure report")
    expect_equal(callback_snapshot(), before, "successful callback leaves all protected probe records unchanged")
    controller.succeed("test \"$(systemctl show cf-it-callback-heartbeat.service -p ActiveState --value)\" = inactive")
    receipt("callback.successNoMutation", True)

with case("late-activation-failure-after-observed-success-uses-exact-deployment-id"):
    # ORDERING: a switch restarts the heartbeat agent, whose report of the new
    # target can finalize the request as succeeded before the unit exits nonzero.
    # The succeeded row below is seeded with the production row transition
    # (mark_pending_deployment_succeeded_tx); this case does not claim to execute
    # the heartbeat path itself, nor the started-response-to-argv hop.
    system = receipts()["callback.systemId"]
    first = delivery_api("POST", "/api/v1/systems/" + system + "/deploy", {
        "commit_sha": FACTS["sourceRevision"], "action": "deploy", "request_id": str(uuid.uuid4()),
    }, expected=202)["deployment_id"]
    controller.succeed("runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 -d crystal_forge -c "
        + shlex.quote("UPDATE pending_system_deployments SET status='succeeded', completed_at=NOW() "
                      "WHERE id=" + literal(first) + "::uuid AND status='pending'"))
    # A distinct newer request for the SAME target must survive the late report.
    newer = delivery_api("POST", "/api/v1/systems/" + system + "/deploy", {
        "commit_sha": FACTS["sourceRevision"], "action": "deploy", "request_id": str(uuid.uuid4()),
    }, expected=202)["deployment_id"]
    expect(newer != first, "second distinct request has its own identity")
    before = callback_snapshot()
    status = {row["id"]: row["status"] for row in before["pending"]}
    expect_equal((status[first], status[newer]), ("succeeded", "pending"),
                 "observed success precedes the late failure; a newer same-target request is pending")
    success_events = [row for row in before["events"] if row["event_type"] == "cf_deployment_succeeded"
                      and row["deployment_id"] == first]
    requests = callback_request_count()
    callback_unit(17, "late", first)
    expect_equal(callback_request_count(), requests + 1, "one late production failure report")
    expect_equal(callback_responses()[-1], {"status": 200, "body": {
        "status": "ok", "message": "Deployment failure recorded"}}, "exact-ID late report is recorded")
    after = callback_snapshot()
    status = {row["id"]: row["status"] for row in after["pending"]}
    expect_equal(status[first], "failed", "late observed execution failure corrected the exact request")
    expect_equal(status[newer], "pending", "newer same-target request is untouched")
    failures = [row for row in after["events"] if row["event_type"] == "cf_deployment_failed"
                and row["deployment_id"] == first]
    expect_equal(len(failures), 1, "one failure event for the exact request")
    typed = failures[0]["metadata"]["deployment_failure"]
    expect_equal((typed["stage"], typed["code"], typed["exit_code"]), ("activation", "activation_execution", 17),
                 "late failure keeps stage, code and numeric exit status")
    expect_equal([row for row in after["events"] if row["event_type"] == "cf_deployment_succeeded"
                  and row["deployment_id"] == first], success_events, "observed success evidence is not deleted")
    expect(not [row for row in after["events"] if row["deployment_id"] == newer
                and row["event_type"] == "cf_deployment_failed"], "newer request gained no failure event")
    expect_equal(after["system"][0]["desired_target"], FACTS["outputPath"],
                 "late report did not erase the newer request's desired target")

with case("callback-with-foreign-or-terminal-deployment-id-never-mutates"):
    system = receipts()["callback.systemId"]
    before = callback_snapshot()
    requests = callback_request_count()
    # A foreign ID must never fall back to matching the target path, and an
    # already-failed request is terminal for a repeated callback.
    failed = next(row["id"] for row in before["pending"] if row["status"] == "failed"
                  and row["id"] != receipts()["callback.failure"]["deploymentId"])
    for label, identity in (("foreign", str(uuid.uuid4())), ("terminal", failed)):
        callback_unit(17, "no-match-" + label, identity)
        expect_equal(callback_responses()[-1], {"status": 200, "body": {
            "status": "ok", "message": "No matching pending deployment"}}, label + " ID is a recorded no-op")
        expect_equal(callback_snapshot(), before, label + " ID leaves every protected record unchanged")
    expect_equal(callback_request_count(), requests + 2, "both no-match reports reached the real endpoint")

with case("signed-failure-reports-are-normalized-before-persistence"):
    # An authenticated agent is not trusted to bound or sanitize what the server
    # stores. Send real signed reports and inspect the rows that PostgreSQL holds,
    # not only what the API would display. The marker is planted in every field,
    # including the legacy `error` and `display_error`, and must reach no row.
    system = receipts()["callback.systemId"]
    hostname = receipts()["callback.hostname"]
    marker = "PERSISTED-SECRET-MARKER"
    huge = "x" * 40000
    allowed_metadata = {"target_store_path", "pending_deployment_id", "deployment_failure"}
    reports = {
        "unknown-fields": ({"stage": "cache_acquisition", "code": "dns", "exit_code": 1,
            "attempt_count": 3, "max_attempts": 3, "future_field": marker,
            "diagnostics": {"lines": ["error: lookup failed"], "cache_endpoint": None,
                            "suppression_reason": None, "extra": marker}}, "classified"),
        "userinfo": ({"stage": "cache_acquisition", "code": "dns", "diagnostics": {
            "lines": ["error: unable https://user:pw@cache.test/p?token=" + marker + " failed"],
            "cache_endpoint": "https://user:pw@cache.test/path?token=" + marker,
            "suppression_reason": None}}, "classified"),
        "oversized-diagnostics": ({"stage": "cache_acquisition", "code": "dns", "diagnostics": {
            "lines": ["error: " + huge], "cache_endpoint": None, "suppression_reason": None}},
            "incomplete_report"),
        "oversized-unknown": ({"stage": "cache_acquisition", "code": "dns", "junk": huge},
            "incomplete_report"),
        "malformed": ({"stage": "activation", "code": "activation_execution",
            "exit_code": marker, "diagnostics": "not-an-object"}, "incomplete_report"),
    }
    for label, (failure, expected_status) in reports.items():
        deployment_id = delivery_api("POST", "/api/v1/systems/" + system + "/deploy", {
            "commit_sha": FACTS["sourceRevision"], "action": "deploy", "request_id": str(uuid.uuid4()),
        }, expected=202)["deployment_id"]
        _vm_write(controller, "/tmp/cf-it/ingress-report.json", json.dumps({
            "hostname": hostname, "target_store_path": FACTS["outputPath"],
            "error": marker, "display_error": marker, "failure": failure}))
        result = json.loads(controller.succeed("node /etc/cf-it/failure-report.cjs report " + shlex.join([
            "/run/cf-it-callback/private.pem", hostname, "/tmp/cf-it/ingress-report.json"])))
        expect_equal(result, {"status": 200, "body": {"status": "ok", "message": "Deployment failure recorded"}},
                     label + ": signed report accepted")
        rows = sql_json(controller, "SELECT metadata, octet_length(metadata::text) AS size FROM system_events "
            "WHERE deployment_id=" + literal(deployment_id) + "::uuid AND event_type='cf_deployment_failed'")
        expect_equal(len(rows), 1, label + ": one failure event")
        metadata, size = rows[0]["metadata"], rows[0]["size"]
        expect(set(metadata) <= allowed_metadata, label + ": unexpected metadata keys " + str(sorted(metadata)))
        stored = json.dumps(metadata)
        for private in (marker, "future_field", "junk", "xxxx", "pw@", "not-an-object"):
            expect(private not in stored, label + ": " + private + " was persisted")
        expect(size < 2048, label + ": stored " + str(size) + " bytes")
        history = delivery_api("GET", "/api/v1/systems/" + system + "/history")
        entry = next(row for row in history if row["deployment_id"] == deployment_id
                     and row["event_type"] == "cf_deployment_failed")
        expect_equal(entry["failure"]["reason_status"], expected_status, label + ": fixed status survives normalization")
        pending = sql_json(controller, "SELECT failure_message FROM pending_system_deployments WHERE id="
                           + literal(deployment_id) + "::uuid")
        expect_equal(pending[0]["failure_message"], entry["failure"]["summary"],
                     label + ": pending row stores only the fixed summary")
        diagnostics = entry["failure"].get("diagnostics")
        if label == "unknown-fields":
            expect_equal(diagnostics["lines"], ["error: lookup failed"], label + ": diagnostics keep only known fields")
        if label == "userinfo":
            expect_equal(diagnostics["lines"], ["error: unable https://cache.test/p failed"], label + ": URL secrets removed")
            expect_equal(diagnostics["cache_endpoint"], "https://cache.test/path", label + ": endpoint sanitized")
        if expected_status == "incomplete_report":
            expect(diagnostics is None, label + ": incomplete report keeps no diagnostics")
        if label.startswith("oversized"):
            expect(size < 400, label + ": oversized report left a small fixed marker (" + str(size) + " bytes)")
    # Whole-table scan: the marker must not appear in any column of either table.
    for table in ("system_events", "pending_system_deployments"):
        found = sql_json(controller, "SELECT count(*) AS n FROM " + table + " t WHERE to_jsonb(t)::text LIKE "
                         + literal("%" + marker + "%"))
        expect_equal(found[0]["n"], 0, "marker persisted in " + table)

with case("self-declared-incomplete-report-cannot-override-observed-success"):
    # A signed report with a valid typed cause and the exact ID of a succeeded
    # request may correct that success to failed only when the report is
    # complete. One that declares itself incomplete must change nothing.
    system = receipts()["callback.systemId"]
    hostname = receipts()["callback.hostname"]
    deployment_id = delivery_api("POST", "/api/v1/systems/" + system + "/deploy", {
        "commit_sha": FACTS["sourceRevision"], "action": "deploy", "request_id": str(uuid.uuid4()),
    }, expected=202)["deployment_id"]
    controller.succeed("runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 -d crystal_forge -c "
        + shlex.quote("UPDATE pending_system_deployments SET status='succeeded', completed_at=NOW() "
                      "WHERE id=" + literal(deployment_id) + "::uuid AND status='pending'"))
    before = callback_snapshot()
    expect_equal(next(row for row in before["pending"] if row["id"] == deployment_id)["status"],
                 "succeeded", "prerequisite: observed success")
    _vm_write(controller, "/tmp/cf-it/incomplete-report.json", json.dumps({
        "hostname": hostname, "target_store_path": FACTS["outputPath"], "error": "x",
        "deployment_id": deployment_id,
        "failure": {"stage": "activation", "code": "activation_execution", "exit_code": 17,
                    "incomplete": True}}))
    result = json.loads(controller.succeed("node /etc/cf-it/failure-report.cjs report " + shlex.join([
        "/run/cf-it-callback/private.pem", hostname, "/tmp/cf-it/incomplete-report.json"])))
    expect_equal(result, {"status": 200, "body": {"status": "ok", "message": "No matching pending deployment"}},
                 "self-declared incomplete report is a recorded no-op")
    expect_equal(callback_snapshot(), before, "observed success and every protected record unchanged")
