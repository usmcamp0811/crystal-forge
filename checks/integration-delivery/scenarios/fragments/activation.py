# Manual live-switch contribution. All target/state identities are observed;
# neither an accepted UI request nor the agent's detached Started result proves
# activation. These helpers belong only to this connected Delivery lifecycle.
def target_observation():
    link = targetA.succeed("readlink /nix/var/nix/profiles/system").strip()
    return {
        "storePath": targetA.succeed("readlink -f /run/current-system").strip(),
        "profile": targetA.succeed("readlink -f /nix/var/nix/profiles/system").strip(),
        "profileLink": link, "generation": int(link.split("-")[-2]),
        "booted": targetA.succeed("readlink -f /run/booted-system").strip(),
        "bootId": targetA.succeed("cat /proc/sys/kernel/random/boot_id").strip(),
    }


def current_target_states():
    return sql_json(controller, "SELECT ss.id::text AS state_id, s.id::text AS system_id, "
        "ss.store_path, ss.generation FROM system_states ss JOIN systems s ON s.hostname=ss.hostname "
        "WHERE s.id=" + literal(receipts()["delivery.targetId"]) + "::uuid ORDER BY ss.id")


def require_post_activation(phase, intended, previous, deployment_id):
    targetA.wait_until_succeeds("test \"$(readlink -f /run/current-system)\" = " +
                              shlex.quote(intended["outputPath"]), timeout=240)
    observed = target_observation()
    expect_equal(observed["storePath"], intended["outputPath"], "actual running CF output")
    expect_equal(observed["profile"], intended["outputPath"], "actual persistent profile output")
    expect_equal(targetA.succeed("cat /etc/cf-it-revision").strip(), intended["nonce"],
                 "activated /etc revision marker")
    expect_equal(observed["bootId"], receipts()["delivery.initial"]["bootId"], "live switch did not reboot")
    expect_equal(observed["booted"], receipts()["delivery.initial"]["booted"], "booted identity remains independent")
    targetA.succeed("systemctl is-active --quiet crystal-forge-agent.service backdoor.service && "
                    "iptables -C OUTPUT -o eth0 -j REJECT && ip6tables -C OUTPUT -o eth0 -j REJECT")
    expect_equal(targetA.succeed("cat /run/cf-it-agent.pub").strip(),
                 receipts()["delivery.targetAgentPublicKey"], "same real agent credential after activation")

    def changed_report():
        rows = current_target_states()
        return rows[-1] if rows and rows[-1]["store_path"] == observed["storePath"] and \
            rows[-1]["generation"] == observed["generation"] and rows[-1]["state_id"] != previous["stateId"] else None

    state = wait_for(changed_report, phase + " genuine agent-observed running generation", timeout=180)
    expect_equal(state["system_id"], receipts()["delivery.targetId"], "changed registered system identity")
    observed["stateId"] = state["state_id"]
    observed["deploymentId"] = deployment_id
    state_snapshot = current_target_states()
    expect_equal([s["state_id"] for s in state_snapshot], previous["stateIds"] + [state["state_id"]],
                 "one genuine full-state transition, not repeated activation reports")
    observed["stateIds"] = [s["state_id"] for s in state_snapshot]

    def subsequent_heartbeats():
        rows = sql_json(controller, "SELECT id::text AS id, system_state_id::text AS state_id "
            "FROM agent_heartbeats WHERE system_state_id=" + str(int(state["state_id"])) + " ORDER BY id")
        return rows if len(rows) >= 2 else None

    heartbeats = wait_for(subsequent_heartbeats, phase + " two heartbeats on newly running state", timeout=180)
    expect_equal(current_target_states(), state_snapshot, "unchanged post-activation heartbeats insert no full states")
    expect(len({h["id"] for h in heartbeats}) >= 2, "post-activation heartbeats must be distinct")
    expect(all(h["state_id"] == state["state_id"] for h in heartbeats), "heartbeats reference old state")
    observed["heartbeatIds"] = [h["id"] for h in heartbeats]
    controller.fail("journalctl -u crystal-forge-server --no-pager | "
                    "grep -F 'Database error occurred while checking state'")
    persisted = sql_json(controller, "SELECT id::text, system_id::text, status, target_store_path, "
        "requested_commit_id, requested_derivation_id, evaluation_snapshot_id::text, completed_at "
        "FROM pending_system_deployments WHERE id=" + literal(deployment_id) + "::uuid")
    expect_equal(len(persisted), 1, "exact CF deployment persistence")
    expect_equal(persisted[0]["status"], "succeeded", "actual observed CF completion, not Started")
    expect_equal(persisted[0]["system_id"], receipts()["delivery.targetId"], "completed system identity")
    expect_equal(persisted[0]["target_store_path"], intended["outputPath"], "completed output identity")
    expect_equal(persisted[0]["requested_derivation_id"], intended["derivationId"], "completed exact derivation")
    expect_equal(persisted[0]["requested_commit_id"], intended["commitId"], "completed exact commit row")
    expect(bool(persisted[0]["completed_at"]), "completion has no timestamp")
    # Ordinary deployment can complete without certified rollback authority.
    # Record that binding truthfully; recovery.py requires it before rollback.
    # TASK-499 tracks the native producer/retention authority mismatch.
    history = delivery_api("GET", "/api/v1/systems/" + receipts()["delivery.targetId"] + "/history")
    events = [e for e in history if e["deployment_id"] == deployment_id and
              e["event_type"] == "cf_deployment_succeeded"]
    expect_equal(len(events), 1, "one durable success history event for actual activation")
    event = events[0]
    expect_equal(event["new_store_path"], intended["outputPath"], "history new running output")
    expect_equal(event["previous_store_path"], previous["storePath"], "history previous running output")
    expect_equal(event["new_generation"], observed["generation"], "history actual generation")
    expect_equal(event["commit_hash"], intended["commit"], "history exact commit")
    expect_equal(event["outcome"], "succeeded", "history reports completed activation")
    observed["historyEventId"] = event["id"]
    observed["previousGeneration"] = event["previous_generation"]
    receipt("delivery." + phase + ".deployment", persisted[0])
    receipt("delivery." + phase + ".observed", observed)
    return observed


with case("manual-first-cf-deployment-activates-cold-output-and-reports-new-state"):
    first = receipt("delivery.first", {
        "commit": receipts()["delivery.sourceRevision"], "nonce": receipts()["delivery.sourceNonce"],
        "message": receipts()["delivery.sourceMessage"], "outputPath": receipts()["delivery.outputPath"],
        "derivationId": receipts()["delivery.derivationId"], "jobId": receipts()["delivery.jobId"],
        "commitId": receipts()["delivery.commitId"],
    })
    initial = dict(receipts()["delivery.initial"], stateId=receipts()["delivery.targetStateId"],
                   stateIds=[s["state_id"] for s in current_target_states()])
    receipt("delivery.initialState", initial)
    assert_store_absent(targetA, first["outputPath"])
    receipt("delivery.first.absentBeforeAcquisition", True)
    run_browser(controller, "deploy-first")
    first_observed = require_post_activation("first", first, initial, receipts()["delivery.first.request"]["deployment_id"])
    run_browser(controller, "inspect-first")
    generations = delivery_api("GET", "/api/v1/systems/" + receipts()["delivery.targetId"] + "/generations")
    retained = [g for g in generations["generations"] if g["generation"] == first_observed["generation"]]
    expect_equal(len(retained), 1, "one observed first managed generation")
    receipt("delivery.first.retained", retained[0])

with case("second-authored-revision-builds-and-publishes-through-cf"):
    # The second commit differs in its activation-managed marker as well as its
    # toplevel output. Keep both real builders available; record the actual
    # claimant rather than assuming a scheduler ordering.
    # The served bare repository belongs to git after initial publication.
    # Execute its receive process as that owner, without global safe.directory
    # exceptions or changing ownership under the running HTTP service.
    services.succeed("cat /proc/sys/kernel/random/uuid > /tmp/delivery-source/nonce && "
        "git -C /tmp/delivery-source add nonce && "
        "git -C /tmp/delivery-source commit -m 'Author second deployable revision' && "
        "git -C /tmp/delivery-source push --receive-pack='runuser -u git -- git-receive-pack' /srv/git/delivery.git main")
    commit = services.succeed("git -C /tmp/delivery-source rev-parse HEAD").strip()
    nonce = services.succeed("cat /tmp/delivery-source/nonce").strip()
    expect(commit != first["commit"] and nonce != first["nonce"], "second source must be distinguishable")
    delivery_api("POST", "/api/v1/flakes/" + str(receipts()["delivery.flakeId"]) + "/sync", {})

    def second_build():
        rows = sql_json(controller, "SELECT c.id AS commit_id, d.id AS derivation_id, d.store_path, d.derivation_path, "
            "d.cf_agent_enabled, d.policy_requirements_met, d.error_message, "
            "a.id::text AS attempt_id, a.attempt_number, i.execution_mode, "
            "j.id::text AS job_id, j.status, j.logs, j.builder_id::text, j.builder_session_id::text, "
            "b.current_session_id::text FROM commits c JOIN derivations d ON d.commit_id=c.id "
            "JOIN build_jobs j ON j.derivation_id=d.id JOIN builders b ON b.id=j.builder_id "
            "JOIN evaluation_attempts a ON a.commit_id=c.id AND a.attempt_number=c.evaluation_attempt_count "
            "JOIN evaluation_attempt_inventory i ON i.evaluation_attempt_id=a.id "
            "WHERE c.flake_id=" + str(receipts()["delivery.flakeId"]) + " AND c.git_commit_hash=" +
            literal(commit) + " AND d.derivation_name=" + literal(FACTS["target"]))
        if not rows:
            return None
        expect_equal(len(rows), 1, "one exact second commit/configuration/attempt/derivation/job")
        return rows[0] if rows[0]["status"] in ("success", "failed", "cancelled") else None

    built = wait_for(second_build, "second real CF evaluation/build/publication", timeout=600)
    if built["status"] != "success":
        receipt("delivery.second.failedBuildLog", built["logs"])
    expect_equal(built["status"], "success", "second real build must complete")
    expect(built["builder_id"] in (receipts()["delivery.builderA.id"], receipts()["delivery.builderB.id"]),
           "second job has no authorized builder")
    expect(bool(built["builder_session_id"]), "second build lacks process session")
    expect_equal(built["builder_session_id"], built["current_session_id"], "current claimant session owns second job")
    claimant = "builderA" if built["builder_id"] == receipts()["delivery.builderA.id"] else "builderB"
    expect_equal(built["builder_session_id"], receipts()["delivery." + claimant + ".sessionId"], "registered real process session")
    expect_equal(built["execution_mode"], "native", "second evaluation is native")
    expect_equal(built["cf_agent_enabled"], True, "second configuration enables actual agent")
    expect_equal(built["policy_requirements_met"], True, "second evaluation admits build")
    expect_equal(built["error_message"], None, "second derivation has no evaluation error")
    outputs = controller.succeed("nix-store --query --outputs " + shlex.quote(built["derivation_path"])).splitlines()
    expect_equal(outputs, [built["store_path"]], "independent second derivation output identity")
    expect(built["store_path"] != first["outputPath"], "second output equals first")
    services.succeed("nix-store --check-validity " + shlex.quote(built["store_path"]))
    expect_equal(services.succeed("cat " + shlex.quote(built["store_path"] + "/cf-it-source-nonce")).strip(),
                 nonce, "second CF-published output contains authored nonce")
    metadata = receiver_metadata("daemon", built["store_path"])
    expect(any(s.startswith("delivery-" + claimant + ":") for s in metadata["signatures"]), "second CF claimant signature missing")
    public_key = receipts()["delivery.publisherPublicKeys"][1 if claimant == "builderA" else 2]
    services.succeed(shlex.quote(FACTS["receiverNix"] + "/bin/nix") +
        " store verify --store daemon --no-contents --sigs-needed 1 --option trusted-public-keys " +
        shlex.quote(public_key) + " " + shlex.quote(built["store_path"]))
    receipt("delivery.second", {"commit": commit, "nonce": nonce, "message": "Author second deployable revision",
        "outputPath": built["store_path"], "derivationId": built["derivation_id"], "jobId": built["job_id"],
        "builderId": built["builder_id"], "builderSessionId": built["builder_session_id"],
        "commitId": built["commit_id"], "derivationPath": built["derivation_path"],
        "evaluationAttemptId": built["attempt_id"], "evaluationAttemptNumber": built["attempt_number"],
        "signatures": metadata["signatures"]})

with case("manual-second-deployment-reports-changed-state-and-correlated-heartbeats"):
    second = receipts()["delivery.second"]
    assert_store_absent(targetA, second["outputPath"])
    receipt("delivery.second.absentBeforeAcquisition", True)
    run_browser(controller, "deploy-second")
    require_post_activation("second", second, receipts()["delivery.first.observed"],
                            receipts()["delivery.second.request"]["deployment_id"])
    run_browser(controller, "inspect-second")
