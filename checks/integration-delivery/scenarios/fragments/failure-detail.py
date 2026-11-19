# Requires delivery setup/source-build receipts and the harness HTTP, SQL and
# browser helpers. The unbooted probe preserves targetA history. Auth uses real
# same-run local identities; historical SQL fixtures prove read compatibility.
# Expanded TASK-529 authority: structured role projections replace legacy display
# text; retain signed/unsigned, persistence, severity, membership and no-mutation
# checks. Extend the same Delivery scenario for role disclosure and exact-ID logs.
import uuid


def failure_probe_snapshot():
    system_id = literal(receipts()["failure.systemId"]) + "::uuid"
    return {
        "system": sql_json(controller, "SELECT * FROM systems WHERE id=" + system_id),
        "pending": sql_json(controller, "SELECT * FROM pending_system_deployments WHERE system_id=" + system_id + " ORDER BY id"),
        "events": sql_json(controller, "SELECT * FROM system_events WHERE system_id=" + system_id + " ORDER BY id"),
        "states": sql_json(controller, "SELECT * FROM system_states WHERE hostname='failure-detail-probe' ORDER BY id"),
        "heartbeats": sql_json(controller, "SELECT ah.* FROM agent_heartbeats ah JOIN system_states ss ON ss.id=ah.system_state_id WHERE ss.hostname='failure-detail-probe' ORDER BY ah.id"),
    }


def expected_failure_projection(failure, role):
    # Independently authored fixed vocabulary, not an expectation copied from API output.
    if failure is None:
        return {"stage": "Deployment", "summary": "Failure reason unavailable: agent does not support structured reporting", "reason_status": "legacy_agent"}
    if failure == {"stage": "activation"}:
        # The cause is unavailable, but the structured report names a valid stage.
        return {"stage": "Activation", "summary": "Failure reason unavailable: agent supplied an incomplete structured report", "reason_status": "incomplete_report"}
    view = {
        "stage": "Cache acquisition" if failure["stage"] == "cache_acquisition" else "Activation",
        "summary": "DNS resolution of the cache host failed" if failure["code"] == "dns" else "Deployment failed for an unrecognized reason; details are redacted",
        "reason_status": "classified",
    }
    if role in ("Admin", "Operator"):
        view["code"] = failure["code"]
        if failure["code"] == "dns":
            view["remediation_hint"] = "Check DNS configuration and cache hostname."
        for key in ("exit_code", "attempt_count", "max_attempts"):
            if failure.get(key) is not None:
                view[key] = failure[key]
    if role == "Admin" and failure.get("diagnostics") is not None:
        view["diagnostics"] = failure["diagnostics"]
    return view


def expected_stored_failure(failure):
    # Independently authored ingress contract, not copied from the stored rows.
    # No report is stored for a legacy agent. A report without a readable cause
    # keeps only a valid stage plus the incomplete marker. A typed report is
    # stored with every typed field present, including empty optional ones.
    if failure is None:
        return None
    if failure == {"stage": "activation"}:
        return {"incomplete": True, "stage": "activation"}
    return {"exit_code": None, "attempt_count": None, "max_attempts": None, "diagnostics": None, **failure}


def expected_failure_message(view):
    if view["summary"].startswith("Deployment failed"):
        message = view["summary"] if view["stage"] == "Deployment" else view["stage"] + ": " + view["summary"]
    else:
        message = view["stage"] + " failed: " + view["summary"]
    if "code" in view:
        message += " [code=" + view["code"]
        if "exit_code" in view:
            message += ", exit=" + str(view["exit_code"])
        if "attempt_count" in view:
            message += ", attempts=" + str(view["attempt_count"])
            if "max_attempts" in view:
                message += "/" + str(view["max_attempts"])
        elif "max_attempts" in view:
            message += ", max_attempts=" + str(view["max_attempts"])
        message += "]"
        if "remediation_hint" in view:
            message += " " + view["remediation_hint"]
    return message


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
    raw = "PRIVATE-RAW-FAILURE-MUST-NOT-DISPLAY"
    legacy_display = "Authenticated cache copy failed: " + "D" * 1600 + "END-LEGACY-DISPLAY"
    receipt("failure.rawMarker", raw)
    diagnostic = "error: " + "D" * 260 + "END-APPROVED-DIAGNOSTIC"
    dns = {"stage": "cache_acquisition", "code": "dns", "exit_code": 1,
           "attempt_count": 3, "max_attempts": 3,
           "diagnostics": {"lines": [diagnostic], "cache_endpoint": "https://cache.example.test/" + "p" * 220, "suppression_reason": None}}
    unknown = {"stage": "activation", "code": "unknown"}
    expected_events = []
    # Repeated DNS attempts use exactly the same commit, target and diagnostic.
    # A commit-based deduplicator would incorrectly discard a real failure.
    reports = [("dns", dns, legacy_display), ("dns-repeat", dns, legacy_display),
               ("legacy", None, None), ("legacy-display", None, legacy_display), ("blank", None, " \n "),
               ("partial", {"stage": "activation"}, None), ("unknown", unknown, None)]
    for suppression in ("provider_response_body", "incomplete_capture"):
        reports.append((suppression, {**dns, "diagnostics": {
            "lines": [], "cache_endpoint": None, "suppression_reason": suppression}}, None))
    for label, failure, display in reports:
        request = delivery_api("POST", "/api/v1/systems/" + probe["id"] + "/deploy", {
            "commit_sha": receipts()["delivery.sourceRevision"], "action": "deploy",
            "request_id": str(uuid.uuid4()),
        }, expected=202)
        deployment_id = request["deployment_id"]
        payload = {"hostname": hostname, "target_store_path": receipts()["delivery.outputPath"], "error": raw}
        if display is not None:
            payload["display_error"] = display
        if failure is not None:
            payload["failure"] = failure
        before = failure_probe_snapshot()
        pending_before = [row for row in before["pending"] if row["id"] == deployment_id]
        expect_equal(len(pending_before), 1, "real API created exact pending request")
        expect_equal(pending_before[0]["status"], "pending", "failure report prerequisite")
        _vm_write(controller, "/tmp/cf-it/failure-report.json", json.dumps(payload))
        status = int(controller.succeed("curl -sS -o /dev/null -w '%{http_code}' "
            "-H 'Content-Type: application/json' -H 'X-Key-ID: " + hostname + "' "
            "--data-binary @/tmp/cf-it/failure-report.json http://localhost:3000/agent/deployment-failed").strip())
        expect_equal(status, 401, "unsigned failure report denied")
        expect_equal(failure_probe_snapshot(), before, "unsigned denial leaves protected records untouched")
        result = json.loads(controller.succeed("node /etc/cf-it/failure-report.cjs report " +
            shlex.join([key_path, hostname, "/tmp/cf-it/failure-report.json"])))
        expect_equal(result, {"status": 200, "body": {"status": "ok", "message": "Deployment failure recorded"}},
                     "signed Ed25519 report accepted and recorded")
        after = failure_probe_snapshot()
        pending = next(row for row in after["pending"] if row["id"] == deployment_id)
        expect_equal(pending["status"], "failed", "report marks exact pending row failed")
        expect(bool(pending["failed_at"]) and bool(pending["completed_at"]), "failed request lacks terminal timestamps")
        expect_equal(pending["failure_message"], expected_failure_projection(failure, "Viewer")["summary"], "pending stores fixed summary only")
        events = [row for row in after["events"] if row["deployment_id"] == deployment_id]
        expect_equal(len(events), 1, "one persisted failure, no invented lifecycle steps")
        event = events[0]
        expect_equal(event["event_type"], "cf_deployment_failed", "persisted failure type")
        expect_equal(event["new_store_path"], payload["target_store_path"], "failure target identity")
        expect_equal(event["new_generation"], None, "no generation activated")
        expect_equal(event["metadata"].get("deployment_failure"), expected_stored_failure(failure),
                     "normalized report persisted, nothing else")
        expect(set(event["metadata"]) <= {"target_store_path", "pending_deployment_id", "deployment_failure"},
               "event metadata gained unexpected keys: " + str(sorted(event["metadata"])))
        expect(raw not in json.dumps(event) and legacy_display not in json.dumps(event), "event persisted legacy free-form diagnostic")
        progress = delivery_api("GET", "/api/v1/systems/" + probe["id"] + "/deployment-status")
        admin_view = expected_failure_projection(failure, "Admin")
        expect_equal(progress["failure"], admin_view, "progress reads authorized structured context")
        expect_equal(progress["failure_message"], expected_failure_message(admin_view), "progress summary excludes diagnostic lines")
        expected_events.append({"id": event["id"], "deploymentId": deployment_id, "failure": failure, "label": label})

with case("historical-failure-metadata-never-promotes-raw-diagnostics"):
    for label, metadata in (("missing", {}), ("blank", {"display_failure_message": " \n ", "failure_message": raw}),
                            ("raw-only", {"failure_message": raw, "future_unknown_key": raw})):
        event_id = str(uuid.uuid4())
        _vm_write(controller, "/tmp/cf-it/historical-failure.sql", "INSERT INTO system_events "
            "(id, system_id, event_type, dedupe_key, occurred_at, source, actor, metadata) VALUES (" +
            literal(event_id) + "::uuid," + literal(probe["id"]) + "::uuid,'cf_deployment_failed'," +
            literal("historical-" + label) + ",NOW() - INTERVAL '1 day','agent_report','agent'," +
            literal(json.dumps(metadata)) + "::jsonb);\n")
        controller.succeed("runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 -d crystal_forge -f - < /tmp/cf-it/historical-failure.sql")
        expected_events.append({"id": event_id, "deploymentId": None, "failure": None, "label": label})
    for expected in expected_events:
        expected["views"] = {role: expected_failure_projection(expected["failure"], role)
                             for role in ("Admin", "Operator", "Viewer")}
        expected["messages"] = {role: expected_failure_message(view) for role, view in expected["views"].items()}
    receipt("failure.expectedEvents", expected_events)

with case("failure-projection-real-role-identities-and-independent-telemetry"):
    actors = json.loads(controller.succeed("node /etc/cf-it/failure-report.cjs actors"))
    for actor in actors:
        user = delivery_api("POST", "/api/v1/admin/users", {
            "email": actor["email"], "display_name": actor["name"], "password": admin["password"],
            "role": actor["role"], "environments": ["delivery-proof"],
        }, expected=201)
        receipt("failure." + actor["role"] + ".userId", user["id"])
        status, _ = http(controller, "POST", "/api/auth/local/login", {
            "username": actor["email"], "password": admin["password"],
        }, session="failure-" + actor["role"])
        expect_equal(status, 200, "same-run role sign-in")
        status, who = http(controller, "GET", "/api/auth/whoami", session="failure-" + actor["role"])
        expect_equal(status, 200, "role identity read")
        expect_equal(who["user"]["id"], user["id"], "exact authenticated identity")
        expect_equal(who["roles"], [actor["role"]], "exact role membership")
    # Historical telemetry proves Logs does not discard unrelated state/heartbeat
    # records even when they share a timestamp with a failed deployment.
    _vm_write(controller, "/tmp/cf-it/failure-telemetry.sql", "WITH state AS (INSERT INTO system_states "
        "(hostname, store_path, change_reason, timestamp) SELECT 'failure-detail-probe'," +
        literal(receipts()["delivery.outputPath"]) + ",'startup',occurred_at FROM system_events WHERE id=" +
        literal(expected_events[0]["id"]) + "::uuid RETURNING id,timestamp) "
        "INSERT INTO agent_heartbeats (system_state_id,timestamp,agent_version) SELECT id,timestamp,'failure-probe-heartbeat' FROM state;")
    controller.succeed("runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 -d crystal_forge -f - < /tmp/cf-it/failure-telemetry.sql")
    before_reads = failure_probe_snapshot()
    for role, jar in (("Admin", "delivery-admin"), ("Operator", "failure-Operator"), ("Viewer", "failure-Viewer")):
        responses = {}
        for suffix in ("history", "agent-events", "deployment-status"):
            status, body = http(controller, "GET", "/api/v1/systems/" + probe["id"] + "/" + suffix, session=jar)
            expect_equal(status, 200, role + " reads " + suffix)
            responses[suffix] = body
        history, logs = responses["history"], responses["agent-events"]
        expect_equal(sorted(row["id"] for row in history), sorted(row["id"] for row in expected_events), "exact failure history membership")
        failures = [row for row in logs if row["event_type"] == "cf_deployment_failed"]
        expect_equal(len(failures), len(expected_events), "one agent-event per independent failure")
        expect_equal(len(logs), len(expected_events) + 2, "independent state and heartbeat retained")
        for expected in expected_events:
            view = expected_failure_projection(expected["failure"], role)
            message = expected_failure_message(view)
            entry = next(row for row in history if row["id"] == expected["id"])
            log = next(row for row in failures if row["event_id"] == expected["id"])
            expect_equal(entry["failure"], view, "exact role-safe history projection")
            expect_equal(entry["title"], message, "role-safe title")
            expect_equal(entry["change_reason"], message, "role-safe UI history binding")
            expect_equal(entry["outcome"], "failed", "failure outcome")
            expect_equal(entry["deployment_id"], expected["deploymentId"], "failure deployment identity")
            if expected["deploymentId"] is not None:
                expect_equal(entry["commit_hash"], receipts()["delivery.sourceRevision"], "same-commit attempts retain source identity")
            expect_equal(log["failure"], view, "same role-safe log projection")
            expect_equal(log["message"], message, "summary has no duplicated diagnostic detail")
            expect_equal(log.get("deployment_id"), expected["deploymentId"], "log deployment identity")
            expect(log["level"] == "error" and log["deployment_related"], "failure severity or identity lost")
        expect_equal(responses["deployment-status"]["failure"], expected_failure_projection(reports[-1][1], role), "role-safe progress projection")
        text = json.dumps(responses)
        expect(raw not in text and "END-LEGACY-DISPLAY" not in text, "legacy/unknown metadata leaked")
        if role != "Admin":
            expect("END-APPROVED-DIAGNOSTIC" not in text and "cache.example.test" not in text, "diagnostics leaked to non-Admin")
        if role == "Viewer":
            expect("Check DNS configuration" not in text and '"exit_code"' not in text, "operational detail leaked to Viewer")
    # SCOPE: Roles are global, but Operator and Viewer additionally need
    # membership in the system's environment (Role::can_access_system_environment);
    # only Admin bypasses it. An Operator may deploy, yet without membership must
    # not see failure data, nor learn that the system exists (404, not 403).
    outsider = {"email": "failure-outsider@example.test", "name": "Failure Outsider"}
    delivery_api("POST", "/api/v1/admin/users", {
        "email": outsider["email"], "display_name": outsider["name"], "password": admin["password"],
        "role": "Operator", "environments": [],
    }, expected=201)
    status, _ = http(controller, "POST", "/api/auth/local/login", {
        "username": outsider["email"], "password": admin["password"],
    }, session="failure-Outsider")
    expect_equal(status, 200, "outsider sign-in")
    status, who = http(controller, "GET", "/api/auth/whoami", session="failure-Outsider")
    expect_equal((status, who["roles"]), (200, ["Operator"]), "outsider has Operator capability but no membership")
    for suffix in ("history", "agent-events", "deployment-status"):
        path = "/api/v1/systems/" + probe["id"] + "/" + suffix
        for label, session, expected_status in (("outsider", "failure-Outsider", 404), ("unauthenticated", None, 403)):
            status, body = http(controller, "GET", path, session=session)
            expect_equal(status, expected_status, label + " read of " + suffix)
            leaked = json.dumps(body, default=str)
            for marker in (raw, "END-APPROVED-DIAGNOSTIC", "END-LEGACY-DISPLAY", "cache.example.test",
                           "DNS resolution", "Cache acquisition", "cache_acquisition", "Check DNS"):
                expect(marker not in leaked, label + " denial for " + suffix + " leaked failure content: " + marker)
    run_browser(controller, "failure-detail")
    expect_equal(failure_probe_snapshot(), before_reads, "API and browser reads do not mutate protected persistence")
