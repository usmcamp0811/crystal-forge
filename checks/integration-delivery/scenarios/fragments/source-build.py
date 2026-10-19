# Requires this lifecycle's authored Git SHA/nonce, registered target/flake,
# environment-bound cache and two registered, stopped API-only builders.
# Native evaluation is the authority; Nix metadata independently checks its
# output identity. Start builderA only after absence checks, then correlate its
# persisted job/session with actual realization and publication. BuilderB starts
# afterward so it cannot race for this proving job. Journals/result receipts
# retain the exact failed phase; no completion response is fabricated.
with case("source-native-evaluation-records-exact-commit-and-derivation"):
    delivery_api("POST", "/api/v1/flakes/" + str(receipts()["delivery.flakeId"]) + "/sync", {})
    def evaluated_source():
        rows = sql_json(controller,
            "SELECT c.id AS commit_id, c.git_commit_hash, c.evaluation_attempt_count, "
            "d.id AS derivation_id, d.derivation_path, d.expected_store_path, "
            "d.cf_agent_enabled, d.policy_requirements_met, d.policy_results, "
            "j.id::text AS job_id, j.status AS job_status FROM commits c "
            "JOIN derivations d ON d.commit_id=c.id JOIN build_jobs j ON j.derivation_id=d.id "
            "WHERE c.flake_id=" + str(receipts()["delivery.flakeId"]) +
            " AND c.git_commit_hash=" + literal(receipts()["delivery.sourceRevision"]) +
            " AND d.derivation_name=" + literal(FACTS["target"]))
        return rows if rows and rows[0]["expected_store_path"] else None
    rows = wait_for(evaluated_source, "authoritative native evaluation and queued build", timeout=300)
    expect_equal(len(rows), 1, "one derivation/job for the authored commit and configuration")
    evaluated = rows[0]
    expect_equal(evaluated["git_commit_hash"], receipts()["delivery.sourceRevision"], "exact ingested SHA")
    expect_equal(evaluated["cf_agent_enabled"], True, "real configuration enables CF agent")
    expect_equal(evaluated["policy_requirements_met"], True, "build admission")
    expect_equal(evaluated["job_status"], "queued", "unclaimed build before release")
    for field, key in (("commit_id", "commitId"), ("derivation_id", "derivationId"),
                       ("derivation_path", "derivationPath"), ("expected_store_path", "outputPath"),
                       ("job_id", "jobId")):
        receipt("delivery." + key, evaluated[field])
    attempt = sql_json(controller,
        "SELECT a.id::text AS id, a.attempt_number, a.status, i.execution_mode "
        "FROM evaluation_attempts a JOIN evaluation_attempt_inventory i ON i.evaluation_attempt_id=a.id "
        "WHERE a.commit_id=" + str(evaluated["commit_id"]) + " AND a.attempt_number=" +
        str(evaluated["evaluation_attempt_count"]))
    expect_equal(len(attempt), 1, "exact persisted evaluation attempt")
    expect_equal(attempt[0]["execution_mode"], "native", "real native evaluator")
    expect_equal(evaluated["policy_results"]["evaluation_attempt"], attempt[0]["attempt_number"],
                 "derivation belongs to exact attempt")
    receipt("delivery.evaluationAttemptId", attempt[0]["id"])
    receipt("delivery.evaluationAttemptNumber", attempt[0]["attempt_number"])
    outputs = controller.succeed("nix-store --query --outputs " +
                                 shlex.quote(evaluated["derivation_path"])).splitlines()
    expect_equal(outputs, [evaluated["expected_store_path"]], "independent Nix output metadata")

with case("authorized-output-absent-before-builder-release"):
    for node in (controller, services, builderA, builderB, targetA):
        assert_store_absent(node, receipts()["delivery.outputPath"])
    receipt("delivery.outputAbsentBeforeBuild", True)

with case("api-builder-performs-real-authorized-nix-build"):
    builderA.succeed("systemctl start crystal-forge-builder.service")
    builderA.wait_for_unit("crystal-forge-builder.service")
    def completed_job():
        rows = sql_json(controller,
            "SELECT j.status, j.builder_id::text AS builder_id, j.builder_session_id::text AS session_id, "
            "b.current_session_id::text AS current_session_id, d.store_path, j.logs "
            "FROM build_jobs j JOIN builders b ON b.id=j.builder_id "
            "JOIN derivations d ON d.id=j.derivation_id WHERE j.id=" + literal(receipts()["delivery.jobId"]) + "::uuid")
        return rows if rows and rows[0]["status"] in ("success", "failed", "cancelled") else None
    completed = wait_for(completed_job, "real build and publication completion", timeout=600)[0]
    if completed["status"] != "success":
        # This authored fixture has no source/cache secrets. Preserve the real
        # Nix/process stderr as failure evidence, not just the exit code.
        receipt("delivery.failedBuildLog", completed["logs"])
    expect_equal(completed["status"], "success", "real builder job terminal result (see retained journals)")
    expect_equal(completed["builder_id"], receipts()["delivery.builderA.id"], "authorized builder identity")
    expect(bool(completed["session_id"]), "job has no owning process session")
    expect_equal(completed["session_id"], completed["current_session_id"], "current session owns completed job")
    expect_equal(completed["store_path"], receipts()["delivery.outputPath"], "completed authoritative output")
    output = shlex.quote(receipts()["delivery.outputPath"])
    builderA.succeed("nix-store --check-validity " + output + " && test -d " + output)
    expect_equal(builderA.succeed("cat " + output + "/cf-it-source-nonce").strip(),
                 receipts()["delivery.sourceNonce"], "built output contains authored nonce")
    services.succeed("nix-store --check-validity " + output + " && test -d " + output)
    receipt("delivery.builderA.sessionId", completed["session_id"])
    receipt("delivery.buildCompleted", True)
    builderA.succeed("test -r /var/lib/crystal-forge/config.toml && "
                     "! grep -q '^\\[database\\]' /var/lib/crystal-forge/config.toml")

with case("second-api-builder-establishes-independent-session"):
    builderB.succeed("systemctl start crystal-forge-builder.service")
    builderB.wait_for_unit("crystal-forge-builder.service")
    def second_session():
        rows = sql_json(controller, "SELECT current_session_id::text AS id, last_heartbeat_at "
            "FROM builders WHERE id=" + literal(receipts()["delivery.builderB.id"]) + "::uuid")
        return rows if rows and rows[0]["id"] and rows[0]["last_heartbeat_at"] else None
    second = wait_for(second_session, "second authenticated API builder session/heartbeat", timeout=90)[0]
    expect(second["id"] != receipts()["delivery.builderA.sessionId"], "builder processes share a session")
    receipt("delivery.builderB.sessionId", second["id"])
    expect_equal(sql_json(controller, "SELECT count(*)::int AS n FROM pg_stat_activity "
        "WHERE datname='crystal_forge' AND client_addr IS NOT NULL"), [{"n": 0}],
        "no remote database clients on the controller")
