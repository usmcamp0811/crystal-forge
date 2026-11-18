# Measure the exact checked-in production query against representative history
# in this scenario's disposable database. No baseline or speedup is claimed.
import hashlib
import re


with case("deployment-progress-plans-with-substantial-failure-history"):
    public_key = controller.succeed("node /etc/cf-it/failure-report.cjs keygen /tmp/cf-it/performance-probe.key").strip()
    perf = delivery_api("POST", "/api/v1/systems", {
        "hostname": "progress-performance-probe", "environment": "delivery-proof",
        "public_key": public_key,
        "flake_name": FACTS["flake"]["name"], "system_configuration_name": FACTS["target"],
        "deployment_policy": "manual",
    }, expected=201)
    sid = literal(perf["id"]) + "::uuid"
    fixture_sql("INSERT INTO pending_system_deployments "
        "(system_id,target_store_path,status,issued_at,expires_at,completed_at) SELECT " + sid + "," +
        literal(FACTS["outputPath"]) + ",'failed',NOW()-n*INTERVAL '1 minute',"
        "NOW()-n*INTERVAL '1 minute',NOW()-n*INTERVAL '1 minute' FROM generate_series(10,20009) n; "
        "INSERT INTO system_events (system_id,event_type,dedupe_key,occurred_at,source,deployment_id,metadata) "
        "SELECT p.system_id,'cf_deployment_failed',p.id::text||'-'||n,"
        "p.issued_at+n*INTERVAL '1 second','agent_report',p.id,"
        "'{\"deployment_failure\":{\"stage\":\"cache_acquisition\",\"code\":\"dns\"}}'::jsonb "
        "FROM pending_system_deployments p CROSS JOIN generate_series(1,5) n WHERE p.system_id=" + sid + "; "
        "INSERT INTO pending_system_deployments (system_id,target_store_path) VALUES (" + sid + "," +
        literal(FACTS["outputPath"]) + "); ANALYZE pending_system_deployments; ANALYZE system_events;")
    counts = sql_json(controller, "SELECT (SELECT count(*) FROM pending_system_deployments WHERE system_id=" +
        sid + ") AS deployments,(SELECT count(*) FROM system_events WHERE system_id=" + sid + ") AS events")[0]
    expect_equal(counts, {"deployments": 20001, "events": 100000}, "representative performance history")
    source = controller.succeed("cat /etc/cf-it/system_events.rs")
    query = re.search(r'pub async fn get_system_deployment_progress_row\(.*?r#"(.*?)"#', source, re.S).group(1)
    expect("WHERE psd.status = 'failed'" in query, "production lateral lookup has failed-status guard")
    query = query.replace("$1", sid)
    artifacts = OUT / "progress-query-performance"
    artifacts.mkdir(exist_ok=True)
    (artifacts / "query.sql").write_text(query)
    measurements = {"counts": counts, "query_sha256": hashlib.sha256(query.encode()).hexdigest(), "states": {}}
    for state in ("pending", "succeeded", "failed"):
        fixture_sql("UPDATE pending_system_deployments SET status=" + literal(state) +
            ",completed_at=NOW(),expires_at=NOW()+INTERVAL '2 hours' WHERE system_id=" + sid +
            " AND issued_at=(SELECT max(issued_at) FROM pending_system_deployments WHERE system_id=" + sid + ");")
        if state == "failed":
            fixture_sql("INSERT INTO system_events (system_id,event_type,dedupe_key,occurred_at,source,deployment_id,metadata) "
                "SELECT system_id,'cf_deployment_failed','perf-current',NOW(),'agent_report',id,"
                "'{\"deployment_failure\":{\"stage\":\"activation\",\"code\":\"unknown\"}}'::jsonb "
                "FROM pending_system_deployments WHERE system_id=" + sid + " ORDER BY issued_at DESC LIMIT 1; ANALYZE system_events;")
        _vm_write(controller, "/tmp/cf-it/performance-plan.sql", "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) " + query + ";")
        plan = json.loads(controller.succeed("runuser -u postgres -- psql -X -qAt -v ON_ERROR_STOP=1 "
            "-d crystal_forge -f - < /tmp/cf-it/performance-plan.sql"))
        (artifacts / (state + ".json")).write_text(json.dumps(plan, indent=2))
        text = fixture_sql("EXPLAIN (ANALYZE, BUFFERS) " + query + ";")
        (artifacts / (state + ".txt")).write_text(text)
        measurements["states"][state] = plan
        receipt("failure.progressPerformance." + state, {
            "planningMs": plan[0]["Planning Time"], "executionMs": plan[0]["Execution Time"],
            "historicalEvents": 100000, "currentFailureEvents": int(state == "failed"),
        })
        def event_scans(node):
            found = [node] if node.get("Relation Name") == "system_events" else []
            return found + [scan for child in node.get("Plans", []) for scan in event_scans(child)]
        scans = event_scans(plan[0]["Plan"])
        expect(bool(scans), "actual plan contains the production failure-event lookup")
        expect_equal(sum(scan["Actual Loops"] for scan in scans), int(state == "failed"),
                     state + " executes failure-event scan only when failed")
    (artifacts / "measurements.json").write_text(json.dumps(measurements, indent=2))
    receipt("failure.progressPerformance", {"counts": counts, "artifacts": str(artifacts)})
