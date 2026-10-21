# Same-lifecycle setup above creates fresh source, credentials, target and cache.
# The delivery.* setup receipts are local to THIS Assurance invocation. No
# Delivery run output or database is opened. Only build/deploy prerequisites
# are repeated; Delivery's signature/network/rollback matrix is not composed.
def evaluated_revision(commit):
    rows = sql_json(controller, "SELECT d.id AS derivation_id, d.derivation_path, d.expected_store_path, "
        "d.cf_agent_enabled, d.policy_requirements_met, c.id AS commit_id FROM commits c "
        "JOIN derivations d ON d.commit_id=c.id WHERE c.flake_id=" + str(receipts()["delivery.flakeId"]) +
        " AND c.git_commit_hash=" + literal(commit) + " AND d.derivation_name=" + literal(FACTS["target"]))
    return rows[0] if len(rows) == 1 and rows[0]["expected_store_path"] else None


def author_revision(version, message):
    _vm_write(services, "/tmp/delivery-source/agent-enabled", "true")
    _vm_write(services, "/tmp/delivery-source/package-version", version)
    services.succeed("cat /proc/sys/kernel/random/uuid > /tmp/delivery-source/nonce && "
        "git -C /tmp/delivery-source add . && git -C /tmp/delivery-source commit -m " + shlex.quote(message) +
        " && git -C /tmp/delivery-source push --receive-pack='runuser -u git -- git-receive-pack' /srv/git/delivery.git main")
    commit = services.succeed("git -C /tmp/delivery-source rev-parse HEAD").strip()
    nonce = services.succeed("cat /tmp/delivery-source/nonce").strip()
    delivery_api("POST", "/api/v1/flakes/" + str(receipts()["delivery.flakeId"]) + "/sync", {})
    return commit, nonce


def real_build(variant, version, message):
    with case("cve-" + variant + "-native-evaluation-build-and-publication"):
        commit, nonce = author_revision(version, message)
        evaluated = wait_for(lambda: evaluated_revision(commit), variant + " native evaluation", timeout=300)
        expect_equal(evaluated["cf_agent_enabled"], True, "admitted configuration has real CF agent")
        expect_equal(evaluated["policy_requirements_met"], True, "native build admission")

        def finished():
            rows = sql_json(controller, "SELECT id::text, status, builder_id::text, builder_session_id::text, logs "
                "FROM build_jobs WHERE derivation_id=" + str(evaluated["derivation_id"]) + " ORDER BY created_at, id")
            return rows[0] if len(rows) == 1 and rows[0]["status"] in ("success", "failed", "cancelled") else None

        job = wait_for(finished, variant + " actual CF build/publication", timeout=600)
        if job["status"] != "success":
            receipt("assurance." + variant + ".failedBuildLog", job["logs"])
        expect_equal(job["status"], "success", "real authorized build completed")
        expect_equal(job["builder_id"], receipts()["delivery.builderA.id"], "sole active producing builder")
        session = sql_json(controller, "SELECT current_session_id::text AS id FROM builders WHERE id=" +
                           literal(job["builder_id"]) + "::uuid")[0]["id"]
        expect_equal(job["builder_session_id"], session, "current real process owns build")
        output = evaluated["expected_store_path"]
        expect_equal(controller.succeed("nix-store --query --outputs " + shlex.quote(evaluated["derivation_path"])).splitlines(),
                     [output], "independent evaluated output identity")
        for node in (builderA, services):
            node.succeed("nix-store --check-validity " + shlex.quote(output))
            expect_equal(node.succeed("cat " + shlex.quote(output + "/cf-it-source-nonce")).strip(), nonce,
                         "real built and published authored content")
        key = "assurance." + variant + "."
        for field, value in {"systemId": receipts()["delivery.targetId"], "derivationId": evaluated["derivation_id"],
            "outputPath": output, "jobId": job["id"], "builderId": job["builder_id"], "builderSessionId": session,
            "commit": commit, "commitId": evaluated["commit_id"], "nonce": nonce, "message": message}.items():
            receipt(key + field, value)
        receipt(key + "scanAdmission", sql_json(controller,
            "SELECT id::text, status, source_trigger, completed_build_job_id::text, attempts, scan_metadata "
            "FROM cve_scans WHERE derivation_id=" + str(evaluated["derivation_id"])))
        receipt(key + "publicationRows", sql_json(controller,
            "SELECT id, status, store_path, cache_destination FROM cache_push_jobs WHERE derivation_id=" +
            str(evaluated["derivation_id"])))
        return {"commit": commit, "nonce": nonce, "message": message, "outputPath": output,
                "derivationId": evaluated["derivation_id"], "commitId": evaluated["commit_id"], "jobId": job["id"]}


def deploy_observed(variant, phase, intended):
    with case("cve-" + variant + "-actual-ui-deployment-and-agent-observation"):
        assert_store_absent(targetA, intended["outputPath"])
        receipt("delivery." + phase, intended)
        run_browser(controller, "deploy-" + phase)
        pending = receipts()["delivery." + phase + ".request"]["deployment_id"]
        targetA.wait_until_succeeds("test \"$(readlink -f /run/current-system)\" = " +
                                  shlex.quote(intended["outputPath"]), timeout=240)
        expect_equal(targetA.succeed("readlink -f /nix/var/nix/profiles/system").strip(),
                     intended["outputPath"], "actual persistent profile")
        expect_equal(targetA.succeed("cat /etc/cf-it-revision").strip(), intended["nonce"], "actual activation marker")
        link = targetA.succeed("readlink /nix/var/nix/profiles/system").strip()
        generation = int(link.split("-")[-2])

        def observed():
            rows = sql_json(controller, "SELECT ss.id::text AS id, ss.generation, ss.store_path, p.status, "
                "p.requested_commit_id, p.requested_derivation_id FROM system_states ss "
                "JOIN systems s ON s.hostname=ss.hostname JOIN pending_system_deployments p ON p.system_id=s.id "
                "WHERE p.id=" + literal(pending) + "::uuid ORDER BY ss.id DESC LIMIT 1")
            return rows[0] if rows and rows[0]["store_path"] == intended["outputPath"] and \
                rows[0]["generation"] == generation and rows[0]["status"] == "succeeded" else None

        state = wait_for(observed, variant + " real persisted agent observation", timeout=180)
        expect_equal(state["requested_commit_id"], intended["commitId"], "deployment exact source commit")
        expect_equal(state["requested_derivation_id"], intended["derivationId"], "deployment exact derivation")
        receipt("assurance." + variant + ".deployment", {"systemId": receipts()["delivery.targetId"],
            "derivationId": intended["derivationId"], "outputPath": intended["outputPath"],
            "generation": generation, "stateId": state["id"]})


with case("assurance-scoped-read-actors-created-through-real-admin-api"):
    for account, role, session in ((FACTS["cveViewer"], "Viewer", "cve-viewer"),
                                   (FACTS["cveOperator"], "Operator", "cve-operator")):
        actor = delivery_api("POST", "/api/v1/admin/users", {
            "email": account["email"], "display_name": account["firstName"] + " " + account["lastName"],
            "password": account["password"], "role": role, "environments": ["delivery-proof"],
        }, expected=201)
        receipt("assurance." + session + ".userId", actor["id"])
        status, _ = http(controller, "POST", "/api/auth/local/login", {
            "username": account["email"], "password": account["password"],
        }, session=session)
        expect_equal(status, 200, "same-scenario scoped " + role + " sign-in")

with case("cve-never-scanned-native-target-before-build-admission"):
    # This authored source intentionally omits CF-agent enablement. Native
    # evaluation must persist the exact target but refuse build admission. It
    # gives a genuine never-scanned selection, not an awaiting_build obligation.
    delivery_api("POST", "/api/v1/flakes/" + str(receipts()["delivery.flakeId"]) + "/sync", {})
    missing = wait_for(lambda: evaluated_revision(receipts()["delivery.sourceRevision"]),
                       "unadmitted native target", timeout=300)
    expect_equal(missing["cf_agent_enabled"], False, "authored source is not eligible for CF build admission")
    expect_equal(sql_json(controller, "SELECT id FROM build_jobs WHERE derivation_id=" + str(missing["derivation_id"])),
                 [], "never-scanned target has not entered build admission")
    receipt("assurance.missing.systemId", receipts()["delivery.targetId"])
    receipt("assurance.missing.derivationId", missing["derivation_id"])
assurance_cve_missing("missing")
with case("cve-never-scanned-production-ui"):
    run_browser(controller, "cve-missing")

with case("cve-real-scanner-trusted-local-upstream"):
    controller.succeed("journalctl -u crystal-forge-server --no-pager | grep -F 'vulnix is not available'")
    services.wait_for_open_port(443)
    # Test the same trusted requests bundle and routing before attributing any
    # scanner failure to CF. No -k or disabled certificate validation is used.
    builderA.succeed("curl --fail --cacert /etc/ssl/certs/ca-certificates.crt "
                     "https://nvd.nist.gov/feeds/json/cve/2.0/nvdcve-2.0-2026.json.gz -o /tmp/nvd-feed.gz")
    builderA.succeed("systemctl start crystal-forge-builder.service")
    builderA.wait_for_unit("crystal-forge-builder.service")
    services.succeed(FACTS["feedCommands"]["normal"])

first = real_build("vulnerable", FACTS["cve"]["vulnerableVersion"], "Author vulnerable Assurance generation")
try:
    assurance_cve_completed("vulnerable", builderA)
except Exception:
    receipt("assurance.vulnerable.scanFailureState", sql_json(controller,
        "SELECT id::text, status, source_trigger, completed_build_job_id::text, attempts, "
        "lease_builder_id::text, execution_id::text, scan_metadata FROM cve_scans WHERE derivation_id=" +
        str(first["derivationId"])))
    receipt("assurance.vulnerable.scannerState", sql_json(controller,
        "SELECT id::text, current_session_id::text, status, cve_scanning_enabled, cve_scan_schema_version "
        "FROM builders"))
    receipt("assurance.vulnerable.scanDiagnostics", sql_json(controller,
        "SELECT scan_id::text, event_type, message FROM cve_scan_diagnostic_events"))
    raise
with case("cve-first-real-scan-production-ui"):
    run_browser(controller, "cve-scan")

deploy_observed("vulnerable", "first", first)
assurance_cve_current("vulnerable")
with case("cve-current-finding-production-navigation"):
    run_browser(controller, "cve-current")
assurance_cve_permissions("cve-viewer", "cve-operator")
with case("cve-scoped-viewer-current-read-only-browser"):
    run_browser(controller, "cve-viewer")
assurance_triage_create()
assurance_triage_viewer_denials()
assurance_poam_maintain()
assurance_poam_viewer_denials()

second = real_build("corrected", FACTS["cve"]["correctedVersion"], "Author corrected Assurance generation")
assurance_cve_completed("corrected", builderA)
assurance_cve_built_not_deployed()
assurance_poam_built_not_deployed()
with case("cve-clean-build-keeps-vulnerable-current-production-ui"):
    run_browser(controller, "cve")

deploy_observed("corrected", "second", second)
assurance_cve_current("corrected")
assurance_cve_historical("vulnerable")
assurance_poam_verify_close()
historical_browser_error = None
try:
    with case("cve-corrected-current-and-historical-production-ui"):
        run_browser(controller, "cve-clean")
        run_browser(controller, "cve-historical")
except ScenarioFailure as error:
    # This browser phase only reads evidence. Preserve its failed case/artifacts
    # and fail the owner after the independent scanner fault/recovery cases.
    # Do not turn a presentation defect into success or skip dependent proof.
    historical_browser_error = error

with case("cve-inventory-read-error-and-retry"):
    run_browser(controller, "cve-read-error")

# Valid upstream JSON with an invalid CVSS value exercises deterministic result
# validation. It is separate from the malformed-feed transient failure below.
def select_upstream(mode):
    # Freeze the idle real unit rather than replace its process/session. Scan
    # affinity must keep the exact producing build session across input faults.
    # A transient retry can claim between observing requeue and freezing. Let
    # that real attempt finish, then retry the freeze-and-idle check. Never
    # change upstream data while an execution is active or wait while frozen.
    def freeze_idle():
        builderA.succeed("systemctl freeze crystal-forge-builder.service")
        active = sql_json(controller, "SELECT id FROM cve_scans WHERE status='in_progress'")
        if active:
            builderA.succeed("systemctl thaw crystal-forge-builder.service")
            return False
        return True

    try:
        wait_for(freeze_idle, "idle scanner before upstream switch", timeout=180)
        expect_equal(sql_json(controller, "SELECT id FROM cve_scans WHERE status='in_progress'"), [],
                     "upstream switch requires idle scanner")
        builderA.succeed("rm -rf /var/lib/crystal-forge/.cache/vulnix")
        services.succeed(FACTS["feedCommands"][mode])
    finally:
        builderA.succeed("systemctl thaw crystal-forge-builder.service")


def fault_rebuild(label):
    # A normal replacement build creates the real post-build scan obligation.
    # Manual scan intents have no producing-session affinity and are not a
    # remote-only executor contract. Never forge that affinity with SQL.
    with case("cve-" + label + "-real-replacement-build-and-scan-intent"):
        queued = delivery_api("POST", "/api/v1/build-jobs/" + first["jobId"] + "/requeue")
        expect_equal(queued["outcome"], "created", "new real build attempt for controlled scanner input")
        replacement = queued["attempt_id"]
        expect(replacement != first["jobId"], "rebuild reused original producing job")
        def completed():
            rows = sql_json(controller, "SELECT status, derivation_id, builder_id::text, builder_session_id::text "
                "FROM build_jobs WHERE id=" + literal(replacement) + "::uuid")
            return rows[0] if rows and rows[0]["status"] in ("success", "failed", "cancelled") else None
        built = wait_for(completed, label + " replacement build completion", timeout=300)
        expect_equal(built["status"], "success", "fault affects scan input, not build success")
        expect_equal(built["derivation_id"], first["derivationId"], "replacement exact original target")
        expect_equal(built["builder_id"], receipts()["assurance.vulnerable.builderId"], "replacement builder identity")
        expect_equal(built["builder_session_id"], receipts()["assurance.vulnerable.builderSessionId"], "same live producing session")
        scans = sql_json(controller, "SELECT id::text, source_trigger FROM cve_scans WHERE completed_build_job_id=" +
                         literal(replacement) + "::uuid")
        expect_equal(len(scans), 1, "one replacement post-build obligation")
        expect_equal(scans[0]["source_trigger"], "post_build", "actual build admission owns fault scan")
        receipt("assurance." + label + ".jobId", replacement)
        return receipt("assurance." + label + ".scanId", scans[0]["id"])


scanner_browser_error = None
try:
    select_upstream("terminalFault")
    terminal_scan = fault_rebuild("terminalFault")
    assurance_cve_failed("vulnerable", terminal_scan, expected_error="vulnix returned an invalid CVSS score")
    try:
        with case("cve-terminal-failure-production-browser"):
            run_browser(controller, "cve-failed")
    except ScenarioFailure as error:
        # This exact scan-detail browser phase is read-only. Retain the failed
        # case and fail the gate after independent transient recovery executes.
        scanner_browser_error = error
finally:
    select_upstream("normal")

# The malformed feed is a transient failure, not terminal failed evidence.
# Keep its exact execution separate from the earlier immutable completed source.
try:
    select_upstream("fault")
    fault_scan = fault_rebuild("transientFault")
    with case("cve-malformed-upstream-requeues-without-replacing-completed-source"):
        def requeued():
            return sql_json(controller, "SELECT execution_id::text, message FROM cve_scan_diagnostic_events WHERE scan_id=" +
                literal(fault_scan) + "::uuid AND event_type='attempt_requeued' ORDER BY attempt_number, id LIMIT 1")
        event = wait_for(requeued, "real malformed upstream transient diagnostic", timeout=180)[0]
        expect("vulnix process failed" in event["message"], "wrong transient failure cause")
        rows = sql_json(controller, "SELECT status, evidence_schema_version, result_digest_sha256 FROM cve_scans WHERE id=" +
                        literal(fault_scan) + "::uuid")
        expect(rows[0]["status"] in ("pending", "in_progress"), "transient input became terminal or clean")
        expect_equal(rows[0]["evidence_schema_version"], 0, "transient failure has no sealed evidence")
        expect_equal(rows[0]["result_digest_sha256"], None, "transient failure has no completed digest")
        inventory = assurance_cve_inventory("vulnerable")
        expect_equal(inventory["attempt"]["scan_id"], fault_scan, "failed execution is not the completed source")
        receipt("assurance.vulnerable.faultExecutionId", event["execution_id"])
finally:
    select_upstream("normal")

with case("cve-upstream-recovery-has-new-execution-and-completed-source"):
    def recovered():
        rows = sql_json(controller, "SELECT status, execution_id::text AS execution_id, evidence_schema_version, "
            "result_digest_sha256 FROM cve_scans WHERE id=" + literal(fault_scan) + "::uuid")
        return rows[0] if rows and rows[0]["status"] in ("completed", "failed") else None
    recovery = wait_for(recovered, "actual recovered scan completion", timeout=600)
    expect_equal(recovery["status"], "completed", "normal upstream recovery")
    expect_equal(recovery["evidence_schema_version"], 1, "recovery sealed schema-1 source")
    expect(recovery["execution_id"] != receipts()["assurance.vulnerable.faultExecutionId"],
           "recovery reused failed execution identity")
    expect(bool(recovery["result_digest_sha256"]), "recovered scan has no sealed digest")
    receipt("assurance.recovery.scanId", fault_scan)
    receipt("assurance.recovery.executionId", recovery["execution_id"])
    status, inventory = http(controller, "GET", "/api/v1/systems/" + receipts()["delivery.targetId"] +
        "/cve-inventory-page?target=exact_derivation&target_id=" + str(first["derivationId"]), session="delivery-admin")
    expect_equal(status, 200, "recovered exact target subsequent read")
    expect_equal(inventory["source"]["scan_id"], fault_scan, "recovery selects its new completed source identity")
    expect_equal([(v["cve_id"], v["canonical_package_name"], v["installed_version"]) for v in inventory["vulnerabilities"]],
                 [(FACTS["cve"]["cveId"], FACTS["cve"]["package"], FACTS["cve"]["vulnerableVersion"])],
                 "recovery reproduces authored affected set, not a false clean result")
    expect_equal(sql_json(controller, "SELECT result_digest_sha256 FROM cve_scans WHERE id=" +
        literal(receipts()["assurance.vulnerable.scanId"]) + "::uuid"),
        [{"result_digest_sha256": receipts()["assurance.vulnerable.scanEvidence"]["result_digest_sha256"]}],
        "recovery preserves the original completed evidence seal")
    assurance_cve_current("corrected")
with case("cve-admin-browser-retry-after-recovery"):
    run_browser(controller, "cve-retry")
assurance_cve_retry()
if historical_browser_error is not None or scanner_browser_error is not None:
    raise ScenarioFailure("\n".join(str(error) for error in
        (historical_browser_error, scanner_browser_error) if error is not None))
