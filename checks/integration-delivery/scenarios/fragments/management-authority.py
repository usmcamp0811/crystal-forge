# Append after normal activation/rollback/recovery: first is a genuinely built,
# published, agent-enabled artifact. Use a new unbooted registration so the real
# target and every existing history assertion remain intact. Browser requests
# race real APIs; SQL checks their authoritative result, never seeds a request.
with case("management-pending-deployment-conversion-and-concurrent-replay"):
    # The activated target generation need not keep the bootstrap keygen tool.
    # The unchanged builder role explicitly includes it on PATH.
    builderA.succeed("cf-keygen -y -f /tmp/cf-it-conversion.key")
    conversion_key = builderA.succeed("cat /tmp/cf-it-conversion.pub").strip()
    conversion = delivery_api("POST", "/api/v1/systems", {
        "hostname": "conversion-probe", "public_key": conversion_key, "environment": "delivery-proof",
        "flake_name": FACTS["flake"]["name"], "system_configuration_name": FACTS["target"],
        "deployment_policy": "manual",
    }, expected=201)
    receipt("management.conversion.systemId", conversion["id"])
    run_browser(controller, "management-authority")
    system_id = literal(conversion["id"]) + "::uuid"
    expect_equal(sql_json(controller, "SELECT management_mode, public_key, desired_target, desired_derivation_id "
        "FROM systems WHERE id=" + system_id), [{"management_mode": "external", "public_key": None,
        "desired_target": None, "desired_derivation_id": None}], "conversion atomically removes authority")
    pending = sql_json(controller, "SELECT id::text, status FROM pending_system_deployments WHERE system_id=" + system_id)
    expect(bool(pending), "positive managed control created no pending deployment")
    expect(all(p["status"] == "superseded" for p in pending), "conversion left a deliverable request")
    reservations = sql_json(controller, "SELECT state FROM deployment_request_reservations WHERE system_id=" + system_id)
    expect(bool(reservations), "positive managed control created no idempotency reservation")
    expect(all(r["state"] == "invalidated" for r in reservations), "replay reservation retains authority")
    expect_equal(sql_json(controller, "SELECT system_id FROM composite_legacy_desired_targets WHERE system_id=" + system_id),
        [], "legacy bridge cannot bypass conversion")
    receipt("management.conversion.pending", pending)
    receipt("management.conversion.reservations", reservations)
