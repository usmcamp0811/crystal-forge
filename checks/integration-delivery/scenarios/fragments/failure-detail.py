# Requires delivery setup and source-build receipts (real published output and
# exact commit). Runs before recovery so an unrelated retained-rollback failure
# cannot hide this regression. The unbooted probe preserves targetA history.
import uuid


def failure_probe_snapshot():
    system_id = literal(receipts()["failure.systemId"]) + "::uuid"
    return {
        "system": sql_json(controller, "SELECT * FROM systems WHERE id=" + system_id),
        "pending": sql_json(controller, "SELECT * FROM pending_system_deployments WHERE system_id=" + system_id + " ORDER BY id"),
        "events": sql_json(controller, "SELECT * FROM system_events WHERE system_id=" + system_id + " ORDER BY id"),
    }


with case("authenticated-failure-report-persists-only-safe-display-diagnostics"):
    hostname = "failure-detail-probe"
    key_path = "/tmp/cf-it/failure-probe.key"
    public_key = controller.succeed("node /etc/cf-it/failure-report.cjs keygen " + key_path).strip()
    probe = delivery_api("POST", "/api/v1/systems", {
        "hostname": hostname, "public_key": public_key, "environment": "delivery-proof",
        "flake_name": FACTS["flake"]["name"], "system_configuration_name": FACTS["target"],
        "deployment_policy": "manual",
    }, expected=201)
    receipt("failure.systemId", probe["id"])
    receipt("failure.hostname", hostname)
    # An intentionally unbroken diagnostic exposes page-width overflow. This
    # authored safe text is not evidence that the agent redacts real Nix stderr.
    safe = "Authenticated cache copy failed: " + "D" * 1600 + "END-SAFE-DIAGNOSTIC"
    raw = "PRIVATE-RAW-FAILURE-MUST-NOT-DISPLAY"
    receipt("failure.rawMarker", raw)
    expected_events = []
    for label, display in (("safe", safe), ("legacy", None), ("blank", " \n ")):
        request = delivery_api("POST", "/api/v1/systems/" + probe["id"] + "/deploy", {
            "commit_sha": receipts()["delivery.sourceRevision"], "action": "deploy",
            "request_id": str(uuid.uuid4()),
        }, expected=202)
        deployment_id = request["deployment_id"]
        payload = {"hostname": hostname, "target_store_path": receipts()["delivery.outputPath"], "error": raw}
        if display is not None:
            payload["display_error"] = display
        before = failure_probe_snapshot()
        pending_before = [row for row in before["pending"] if row["id"] == deployment_id]
        expect_equal(len(pending_before), 1, "real API created exact pending request")
        expect_equal(pending_before[0]["status"], "pending", "failure report prerequisite")
        _vm_write(controller, "/tmp/cf-it/failure-report.json", json.dumps(payload))
        status = int(controller.succeed("curl -sS -o /dev/null -w '%{http_code}' "
            "-H 'Content-Type: application/json' -H 'X-Key-ID: " + hostname + "' "
            "--data-binary @/tmp/cf-it/failure-report.json http://localhost:3000/agent/deployment-failed").strip())
        expect_equal(status, 401, "unsigned failure report denied")
        expect_equal(failure_probe_snapshot(), before, "unsigned denial leaves system, pending and events untouched")
        result = json.loads(controller.succeed("node /etc/cf-it/failure-report.cjs report " +
            shlex.join([key_path, hostname, "/tmp/cf-it/failure-report.json"])))
        expect_equal(result, {"status": 200, "body": {"status": "ok", "message": "Deployment failure recorded"}},
                     "signed Ed25519 report accepted and recorded")
        after = failure_probe_snapshot()
        pending = [row for row in after["pending"] if row["id"] == deployment_id][0]
        expect_equal(pending["status"], "failed", "report marks exact pending row failed")
        expect(bool(pending["failed_at"]) and bool(pending["completed_at"]), "failed request lacks terminal timestamps")
        expect_equal(pending["failure_message"], safe if label == "safe" else
                     "Deployment failed (diagnostic details unavailable)", "new pending message is safe or neutral")
        events = [row for row in after["events"] if row["deployment_id"] == deployment_id]
        expect_equal(len(events), 1, "one persisted failure, no invented lifecycle steps")
        event = events[0]
        expect_equal(event["event_type"], "cf_deployment_failed", "persisted failure type")
        expect_equal(event["new_store_path"], payload["target_store_path"], "failure target identity")
        expect_equal(event["new_generation"], None, "no generation activated")
        expect_equal(event["metadata"].get("display_failure_message"), safe if label == "safe" else None,
                     "event stores only explicitly safe diagnostics")
        expect(raw not in json.dumps(event), "new event persisted raw diagnostic")
        progress = delivery_api("GET", "/api/v1/systems/" + probe["id"] + "/deployment-status")
        expect_equal(progress["failure_message"], pending["failure_message"], "banner reads safe event text")
        expected_events.append({"id": event["id"], "deploymentId": deployment_id,
            "title": safe if label == "safe" else "Deployment failed"})

with case("historical-failure-metadata-never-promotes-raw-diagnostics"):
    # Historical data is seeded only to prove read compatibility. Report-produced
    # rows above remain untouched and prove the actual mutation boundary.
    for label, metadata in (("missing", {}), ("blank", {"display_failure_message": " \n ", "failure_message": raw}),
                            ("raw-only", {"failure_message": raw})):
        event_id = str(uuid.uuid4())
        _vm_write(controller, "/tmp/cf-it/historical-failure.sql", "INSERT INTO system_events "
            "(id, system_id, event_type, dedupe_key, occurred_at, source, actor, metadata) VALUES (" +
            literal(event_id) + "::uuid," + literal(probe["id"]) + "::uuid,'cf_deployment_failed'," +
            literal("historical-" + label) + ",NOW() - INTERVAL '1 day','agent_report','agent'," +
            literal(json.dumps(metadata)) + "::jsonb);\n")
        controller.succeed("runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 -d crystal_forge "
                           "-f - < /tmp/cf-it/historical-failure.sql")
        expected_events.append({"id": event_id, "deploymentId": None, "title": "Deployment failed"})
    receipt("failure.expectedEvents", expected_events)
    before_reads = failure_probe_snapshot()
    history = delivery_api("GET", "/api/v1/systems/" + probe["id"] + "/history")
    expect_equal(sorted(row["id"] for row in history), sorted(row["id"] for row in expected_events), "exact failure history membership")
    for expected in expected_events:
        entry = next(row for row in history if row["id"] == expected["id"])
        expect_equal(entry["title"], expected["title"], "safe title or neutral historical fallback")
        expect_equal(entry["change_reason"], expected["title"], "UI history binding excludes raw message")
        expect_equal(entry["outcome"], "failed", "failure outcome")
        expect_equal(entry["deployment_id"], expected["deploymentId"], "failure deployment identity")
    logs = delivery_api("GET", "/api/v1/systems/" + probe["id"] + "/agent-events")
    expect(raw not in json.dumps(history), "history response metadata exposes legacy raw diagnostic")
    expect(raw not in json.dumps(logs), "agent-events response exposes legacy raw diagnostic")
    expect_equal(len(logs), len(expected_events), "one agent-event per failure")
    expect_equal(sorted(row["message"] for row in logs), sorted(
        "Deployment failed: " + row["title"] if row["title"] != "Deployment failed" else "Deployment failed"
        for row in expected_events), "agent-events reads safe field only")
    expect(all(row["level"] == "error" and row["event_type"] == "cf_deployment_failed" and
               row["deployment_related"] for row in logs), "failure logs lose error severity or event identity")
    run_browser(controller, "failure-detail")
    expect_equal(failure_probe_snapshot(), before_reads, "API and browser history/log reads do not mutate persistence")
