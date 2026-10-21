# Reuse the first CF-managed deployment's retained authority, not the initial
# harness boot generation. Rollback may create a new profile generation/state;
# the actual observed identity, not its historical number, is authoritative.
with case("history-ui-rollback-restores-retained-output-and-correlates-current-state"):
    first = receipts()["delivery.first"]
    second_observed = receipts()["delivery.second.observed"]
    # Keep the real product prerequisite mandatory after both independent
    # deployment proofs. Do not fabricate authority or use commit rollback to
    # evade an unavailable retained-generation History action (TASK-499).
    generations = delivery_api("GET", "/api/v1/systems/" + receipts()["delivery.targetId"] + "/generations")
    candidates = [g for g in generations["generations"] if g["generation"] ==
                  receipts()["delivery.first.observed"]["generation"] and g["store_path"] == first["outputPath"]]
    expect_equal(len(candidates), 1, "exact first managed generation remains in current history")
    retained = receipt("delivery.rollback.eligibility", candidates[0])
    expect_equal(retained["is_current"], False, "rollback target is no longer the running generation")
    expect(retained["rollback_eligible"] and bool(retained["generation_snapshot_id"]),
           "TASK-499: CF-managed first generation lacks certified retained rollback authority: " + json.dumps(retained))
    eligible = [g for g in generations["generations"] if g["generation_snapshot_id"] ==
                receipts()["delivery.first.retained"]["generation_snapshot_id"]]
    expect_equal(len(eligible), 1, "retained first generation still exists")
    expect_equal(eligible[0]["rollback_eligible"], True, "retained first generation remains rollback eligible")
    expect_equal(eligible[0]["is_current"], False, "rollback selects previous, not current, generation")
    retained_rows = sql_json(controller, "SELECT snapshot_id::text, derivation_id, commit_id FROM "
        "evaluation_generation_snapshots WHERE id=" + literal(retained["generation_snapshot_id"]) + "::uuid "
        "AND system_id=" + literal(receipts()["delivery.targetId"]) + "::uuid")
    expect_equal(len(retained_rows), 1, "exact server-owned retained authority")
    selector = ("SELECT id::text, system_id::text, source, target_store_path, requested_derivation_id, "
        "requested_commit_id, evaluation_snapshot_id::text FROM pending_system_deployments "
        "WHERE system_id=" + literal(receipts()["delivery.targetId"]) + "::uuid "
        "AND source='manual_rollback_generation'")
    before = {row["id"] for row in sql_json(controller, selector)}
    run_browser(controller, "rollback")
    pending = [row for row in sql_json(controller, selector) if row["id"] not in before]
    expect_equal(len(pending), 1, "UI rollback creates real pending CF work")
    expect_equal(pending[0]["target_store_path"], first["outputPath"], "rollback exact retained output")
    expect_equal(pending[0]["requested_derivation_id"], retained_rows[0]["derivation_id"], "rollback retained derivation")
    expect_equal(pending[0]["requested_commit_id"], retained_rows[0]["commit_id"], "rollback retained commit")
    expect_equal(pending[0]["evaluation_snapshot_id"], retained_rows[0]["snapshot_id"], "rollback retained artifact binding")
    receipt("delivery.rollback.pendingId", pending[0]["id"])
    release_after_started_observation("rollback", first, pending[0]["id"])
    require_post_activation("rollback", first, second_observed, pending[0]["id"])
    run_browser(controller, "inspect-rollback")
    history = delivery_api("GET", "/api/v1/systems/" + receipts()["delivery.targetId"] + "/history")
    successes = [e for e in history if e["event_type"] == "cf_deployment_succeeded"]
    expect_equal({e["deployment_id"] for e in successes}, {
        receipts()["delivery.first.observed"]["deploymentId"],
        receipts()["delivery.second.observed"]["deploymentId"],
        receipts()["delivery.rollback.observed"]["deploymentId"],
    }, "all three completed workflows remain in durable history")
    receipt("delivery.connectedWorkflowCompleted", True)
