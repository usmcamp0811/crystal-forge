# Load this fragment once into the shared driver after setup defines literal.
# It defines semantic cases; the coordinator calls them at lifecycle boundaries.
# No setup, case registry, scan rows, findings, or deployment state is seeded.
# FACTS["cve"] is fixtures/vuln-data/facts.json, independent of CF responses.
#
# Required same-run receipts, for each variant "vulnerable" and "corrected":
# assurance.<variant>.{systemId,derivationId,outputPath,jobId,builderId,
# builderSessionId}. completed(variant, builder) reads that builder's actual
# Nix closure and records scanId, executionId, packageOutputPath,
# packageDerivationPath, scanEvidence and observations for downstream cases.
#
# Call order: completed("vulnerable", builderA); deploy the vulnerable output;
# current("vulnerable"); downstream finding/triage baseline; build corrected;
# completed("corrected", builderA); built_not_deployed(); deploy corrected;
# current("corrected"); historical("vulnerable"). Never call Current from a
# build completion callback. Before current(), setup must record an actual
# deployment/target-observation receipt assurance.<variant>.deployment with
# {systemId,derivationId,outputPath,generation,stateId}. generation and output
# must come from the live target after activation, and stateId from its actual
# persisted agent report. Current rechecks the latest report and authority view.
#
# missing(variant) is called on an evaluated, never-scanned target before
# build admission creates a scan intent. Merely pausing its scanner is not
# enough: awaiting_build is already an attempt. failed(variant, scan_id) follows
# a real injected scanner fault and queued rescan; it preserves the
# earlier completed source. These cases never manufacture failures or success.
# fault_rescan(variant) uses faultFeedRoot and proves an actual transient failure
# and requeue, NOT a terminal failed scan. The real pinned scanner exits 1 on
# malformed upstream JSON; cf-builder treats that exit as transient. A separate
# deterministic fault is required before calling failed(). Clear the builder's
# upstream cache first: warm vulnix skips feed downloads for two hours. Restore
# the normal feed afterward; do not reuse this variant's original source receipt
# as its newest source after the repaired rescan completes.
# API cases accept the existing setup's authenticated cookie-jar name; the
# default reuses delivery-lifecycle.py's same-scenario administrator session.
# The coordinator registers cve.cjs as "cve" and runs it while vulnerable is
# deployed (after built_not_deployed, before deploying corrected).
# Its scan/current exports permit earlier production UI evidence; see the
# fixture README for browserSpecs adapters and the complete setup contract.


def assurance_cve_completed(variant, builder, session="delivery-admin"):
    """Checks one real post-build scanner completion and captures exact evidence."""
    expect(variant in ("vulnerable", "corrected"), "unknown CVE fixture variant")
    key = "assurance." + variant + "."
    r = receipts()
    facts = FACTS["cve"]
    with case("cve-" + variant + "-real-builder-schema1-completion"):
        # Read the real runtime closure, not package identities from CF output.
        output = shlex.quote(r[key + "outputPath"])
        builder.succeed("nix-store --check-validity " + output)
        closure = builder.succeed("nix-store --query --requisites " + output).splitlines()
        version = facts["vulnerableVersion" if variant == "vulnerable" else "correctedVersion"]
        packages = [p for p in closure if p.endswith("-" + facts["package"] + "-" + version)]
        expect_equal(len(packages), 1, "one authored package in the actual runtime closure")
        other = facts["correctedVersion" if variant == "vulnerable" else "vulnerableVersion"]
        expect(not any(p.endswith("-" + facts["package"] + "-" + other) for p in closure),
               "generation contains the other fixture package version")
        package_drv = builder.succeed("nix-store --query --deriver " + shlex.quote(packages[0])).strip()
        expect(package_drv.endswith(".drv"), "fixture output has no real deriver")
        expect_equal(builder.succeed("nix-store --query --outputs " + shlex.quote(package_drv)).splitlines(),
                     packages, "independent package output identity")
        target_drv = builder.succeed("nix-store --query --deriver " + output).strip()
        expect(target_drv.endswith(".drv"), "target output has no real deriver")

        def terminal_scan():
            rows = sql_json(controller,
                "SELECT s.id::text AS id, s.derivation_id, s.status, s.evidence_schema_version, "
                "s.scanner_name, s.scanner_version, s.source_trigger, "
                "s.completed_build_job_id::text AS job_id, s.execution_id::text AS execution_id, "
                "s.lease_builder_id::text AS builder_id, "
                "s.lease_builder_session_id::text AS session_id, s.target_drv_path, s.target_outputs, "
                "s.result_digest_sha256, s.execution_outcome, s.closure_provenance, s.failure_class, "
                "s.completed_at, s.total_vulnerabilities, s.critical_count, s.high_count, "
                "(s.lease_started_at IS NOT NULL AND s.lease_started_at <= s.lease_heartbeat_at "
                "AND s.lease_heartbeat_at <= s.completed_at AND s.completed_at < s.lease_expires_at) "
                "AS lease_valid_at_completion, "
                "s.medium_count, s.low_count, j.status AS build_status, j.derivation_id AS build_derivation_id, "
                "j.builder_id::text AS build_builder_id, j.builder_session_id::text AS build_session_id, "
                "d.store_path AS built_output FROM cve_scans s "
                "JOIN build_jobs j ON j.id=s.completed_build_job_id "
                "JOIN derivations d ON d.id=s.derivation_id WHERE s.derivation_id=" +
                str(int(r[key + "derivationId"])) + " AND s.source_trigger='post_build' "
                "AND s.completed_build_job_id=" + literal(r[key + "jobId"]) + "::uuid")
            return rows if rows and all(s["status"] in ("completed", "failed") for s in rows) else None

        scans = wait_for(terminal_scan, "exact post-build CVE scan terminal result", timeout=600)
        expect_equal(len(scans), 1, "one scan intent for this exact producing build")
        scan = scans[0]
        for field, expected in (
            ("status", "completed"), ("evidence_schema_version", 1),
            ("scanner_name", "vulnix"), ("scanner_version", facts["scannerVersion"]),
            ("source_trigger", "post_build"), ("execution_outcome", "completed"),
            ("failure_class", None), ("build_status", "success"),
            ("lease_valid_at_completion", True),
            ("derivation_id", r[key + "derivationId"]), ("build_derivation_id", r[key + "derivationId"]),
            ("job_id", r[key + "jobId"]), ("builder_id", r[key + "builderId"]),
            ("build_builder_id", r[key + "builderId"]), ("session_id", r[key + "builderSessionId"]),
            ("build_session_id", r[key + "builderSessionId"]), ("built_output", r[key + "outputPath"]),
            ("target_drv_path", target_drv),
            ("target_outputs", [{"name": "out", "store_path": r[key + "outputPath"]}]),
        ):
            expect_equal(scan[field], expected, "completed scan " + field)
        expect(bool(scan["completed_at"] and scan["execution_id"]), "completion lacks time/execution identity")
        digest = scan["result_digest_sha256"] or ""
        expect(len(digest) == 64 and all(c in "0123456789abcdef" for c in digest),
               "completion lacks a server-sealed schema-1 digest")
        expect(scan["closure_provenance"] in ("server_local_verified", "unverified_remote"),
               "remote completion lacks explicit closure provenance")
        # Preserve the distinction: signed remote evidence is not local proof.
        receipt(key + "closureProvenance", scan["closure_provenance"])
        observations = sql_json(controller,
            "SELECT canonical_cve_id, canonical_package_name, observed_package_name, "
            "observed_package_version, observed_derivation_path, is_affected, is_whitelisted, "
            "whitelist_reason, detection_method FROM cve_scan_vulnerability_observations "
            "WHERE scan_id=" + literal(scan["id"]) + "::uuid "
            "ORDER BY observed_derivation_path, canonical_cve_id, canonical_package_name")
        expected = [] if variant == "corrected" else [{
            "canonical_cve_id": facts["cveId"], "canonical_package_name": facts["package"],
            "observed_package_name": facts["package"] + "-" + version,
            "observed_package_version": version, "observed_derivation_path": package_drv,
            "is_affected": True, "is_whitelisted": False, "whitelist_reason": None,
            "detection_method": "vulnix",
        }]
        expect_equal(observations, expected, "entire immutable observation set, including whitelist markers")
        n = len(expected)
        expect_equal([scan[f] for f in ("total_vulnerabilities", "critical_count", "high_count", "medium_count", "low_count")],
                     [n, n, 0, 0, 0], "authored scan counts")
        for field, value in (("scanId", scan["id"]), ("executionId", scan["execution_id"]),
                             ("packageOutputPath", packages[0]), ("packageDerivationPath", package_drv),
                             ("scanEvidence", scan), ("observations", observations)):
            receipt(key + field, value)
        status, detail = http(controller, "GET", "/api/v1/scanning/scans/" + scan["id"], session=session)
        expect_equal(status, 200, "exact persisted scan detail API")
        expect_equal(detail["scan_id"], scan["id"], "detail scan identity")
        expect_equal(detail["derivation_id"], r[key + "derivationId"], "detail derivation identity")
        expect_equal(detail["status"], "completed", "detail terminal evidence")
        expect_equal(detail["build_job_id"], r[key + "jobId"], "detail producing job")
        return scan["id"]


def assurance_cve_inventory(variant, current=False, session="delivery-admin"):
    """Checks exact inventory membership without deriving expected CVEs from CF."""
    key = "assurance." + variant + "."
    r = receipts()
    query = "?target=current" if current else (
        "?target=exact_derivation&target_id=" + str(int(r[key + "derivationId"])))
    status, inventory = http(controller, "GET", "/api/v1/systems/" + r[key + "systemId"] +
                             "/cve-inventory-page" + query, session=session)
    expect_equal(status, 200, "selected system inventory API")
    expect_equal(inventory["system_id"], r[key + "systemId"], "inventory system identity")
    expect_equal(inventory["selection"], {"kind": "current"} if current else {
        "kind": "exact_derivation", "derivation_id": r[key + "derivationId"]}, "inventory selection")
    expect_equal(inventory["source"]["scan_id"], r[key + "scanId"], "exact completed source")
    expect_equal(inventory["evidence_representation"], "schema1_observations", "immutable inventory source")
    expect_equal(inventory["authority"], "exact" if current else "legacy", "selected evidence authority")
    expect_equal(inventory["read_only"], not current, "historical evidence never grants Current authority")
    expect_equal(inventory.get("current_state"), "exact_current_scan" if current else None,
                 "Current versus historical semantics")
    facts = FACTS["cve"]
    expected = [] if variant == "corrected" else [(facts["cveId"], facts["package"], facts["vulnerableVersion"])]
    expect_equal([(v["cve_id"], v["canonical_package_name"], v["installed_version"])
                   for v in inventory["vulnerabilities"]], expected, "exact authored inventory membership")
    expect_equal([v["cvss_score"] for v in inventory["vulnerabilities"]],
                 [] if variant == "corrected" else [facts["cvssV3"]], "authored CVSS score")
    expect_equal(inventory["metadata"]["total_findings"], len(expected), "full finding total")
    expect_equal(inventory["has_more"], False, "authored set fits one complete page")
    return inventory


def assurance_cve_current(variant, session="delivery-admin"):
    """Checks Current only after the owner's real activation/observation receipt."""
    key = "assurance." + variant + "."
    r = receipts()
    with case("cve-" + variant + "-deployed-generation-is-current"):
        deployed = r[key + "deployment"]  # Required; absence is a hard failure.
        for field in ("systemId", "derivationId", "outputPath"):
            expect_equal(deployed[field], r[key + field], "actual deployment " + field)
        expect(int(deployed["generation"]) > 0, "deployment has no observed generation")
        recorded = sql_json(controller,
            "SELECT state.generation, state.store_path, state.generation_matches_current_store_path AS matches "
            "FROM system_states state JOIN systems system ON system.hostname=state.hostname "
            "WHERE system.id=" + literal(r[key + "systemId"]) + "::uuid AND state.id::text=" +
            literal(deployed["stateId"]))
        observed = {"generation": deployed["generation"], "store_path": r[key + "outputPath"], "matches": True}
        expect_equal(recorded, [observed], "actual deployment receipt identifies a persisted agent report")
        # A later honest report of the same running generation is valid. Do not
        # require its row UUID to stay equal to the activation-time report UUID.
        latest = sql_json(controller,
            "SELECT state.generation, state.store_path, "
            "state.generation_matches_current_store_path AS matches FROM systems system "
            "JOIN LATERAL (SELECT * FROM system_states WHERE hostname=system.hostname "
            "ORDER BY timestamp DESC NULLS LAST, id DESC LIMIT 1) state ON true "
            "WHERE system.id=" + literal(r[key + "systemId"]) + "::uuid")
        expect_equal(latest, [observed],
                     "latest agent report binds the actually activated output")
        authority = sql_json(controller,
            "SELECT DISTINCT derivation_id, generation, store_path, scan_id::text AS scan_id "
            "FROM view_current_cve_authority WHERE system_id=" + literal(r[key + "systemId"]) + "::uuid")
        expect_equal(authority, [{"derivation_id": r[key + "derivationId"], "generation": deployed["generation"],
                                 "store_path": r[key + "outputPath"], "scan_id": r[key + "scanId"]}],
                     "Current is exact observed generation, output, and completed scan")
        inventory = assurance_cve_inventory(variant, current=True, session=session)
        expect_equal(inventory["running_target"]["derivation_id"], r[key + "derivationId"], "Current target")
        expect_equal(inventory["running_target"]["generation"], deployed["generation"], "Current generation")
        receipt(key + "currentScanId", r[key + "scanId"])


def assurance_cve_historical(variant, session="delivery-admin"):
    """Checks a formerly Current generation after another generation becomes Current."""
    with case("cve-" + variant + "-historical-selection-is-read-only"):
        r = receipts()
        key = "assurance." + variant + "."
        expect_equal(r[key + "currentScanId"], r[key + "scanId"],
                     "historical generation was previously proven Current")
        authority = sql_json(controller,
            "SELECT DISTINCT derivation_id FROM view_current_cve_authority WHERE system_id=" +
            literal(r[key + "systemId"]) + "::uuid")
        expect_equal(len(authority), 1, "replacement generation has exact Current authority")
        expect(authority[0]["derivation_id"] != r[key + "derivationId"],
               "historical selection still describes the running generation")
        sealed = sql_json(controller,
            "SELECT status, evidence_schema_version, scanner_name, scanner_version, completed_at, "
            "target_drv_path, target_outputs, result_digest_sha256, execution_outcome, closure_provenance "
            "FROM cve_scans WHERE id=" + literal(r[key + "scanId"]) + "::uuid")
        expect_equal(len(sealed), 1, "historical sealed scan still exists")
        expect_equal(sealed[0], {field: r[key + "scanEvidence"][field] for field in sealed[0]},
                     "completed scan seal remains unchanged across generations")
        return assurance_cve_inventory(variant, session=session)


def assurance_cve_built_not_deployed(session="delivery-admin"):
    """Proves a built clean result does not replace the vulnerable Current scan."""
    r = receipts()
    with case("cve-clean-build-does-not-close-deployed-vulnerable-evidence"):
        expect_equal(r["assurance.corrected.systemId"], r["assurance.vulnerable.systemId"],
                     "two generations belong to the same actual system")
        expect(r["assurance.corrected.derivationId"] != r["assurance.vulnerable.derivationId"],
               "corrected build reused the vulnerable derivation")
        expect(r["assurance.corrected.outputPath"] != r["assurance.vulnerable.outputPath"],
               "corrected build reused the vulnerable output")
        expect(r["assurance.corrected.scanId"] != r["assurance.vulnerable.scanId"],
               "two real executions reused one scan identity")
        assurance_cve_inventory("corrected", session=session)
        assurance_cve_current("vulnerable", session=session)
        observations = sql_json(controller,
            "SELECT canonical_cve_id, canonical_package_name, observed_package_name, "
            "observed_package_version, observed_derivation_path, is_affected, is_whitelisted, "
            "whitelist_reason, detection_method FROM cve_scan_vulnerability_observations WHERE scan_id=" +
            literal(r["assurance.vulnerable.scanId"]) + "::uuid "
            "ORDER BY observed_derivation_path, canonical_cve_id, canonical_package_name")
        expect_equal(observations, r["assurance.vulnerable.observations"], "vulnerable evidence remains intact")
        receipt("assurance.corrected.builtNotDeployedProven", True)


def assurance_cve_missing(variant, session="delivery-admin"):
    """Distinguishes an evaluated never-scanned target from a clean completion."""
    key = "assurance." + variant + "."
    r = receipts()
    with case("cve-" + variant + "-never-scanned-is-not-clean"):
        rows = sql_json(controller, "SELECT id FROM cve_scans WHERE derivation_id=" +
                        str(int(r[key + "derivationId"])))
        expect_equal(rows, [], "never-scanned target has no scan attempts")
        status, inventory = http(controller, "GET", "/api/v1/systems/" + r[key + "systemId"] +
            "/cve-inventory-page?target=exact_derivation&target_id=" + str(int(r[key + "derivationId"])),
            session=session)
        expect_equal(status, 200, "never-scanned exact target API")
        expect_equal(inventory["authority"], "no_scan", "missing evidence authority")
        expect_equal(inventory["source"], None, "missing evidence has no completed source")
        expect_equal(inventory.get("attempt"), None, "never scanned has no attempt")
        expect_equal(inventory["evidence_representation"], None, "no schema-1 evidence")
        expect_equal(inventory["vulnerabilities"], [], "missing is empty without claiming clean")


def assurance_cve_failed(variant, scan_id, session="delivery-admin"):
    """Checks a real injected-fault rescan without replacing completed evidence."""
    key = "assurance." + variant + "."
    r = receipts()
    with case("cve-" + variant + "-scanner-failure-is-not-clean"):
        expect(scan_id != r[key + "scanId"], "failure must be a separate real rescan")

        def terminal_failure():
            rows = sql_json(controller,
                "SELECT status, derivation_id, evidence_schema_version, execution_outcome, failure_class, "
                "result_digest_sha256, execution_id::text AS execution_id, scan_metadata->>'error' AS error "
                "FROM cve_scans WHERE id=" + literal(scan_id) + "::uuid")
            return rows if rows and rows[0]["status"] in ("completed", "failed") else None

        row = wait_for(terminal_failure, "actual injected scanner fault terminal result", timeout=600)[0]
        expect_equal(row["status"], "failed", "actual scanner fault")
        expect_equal(row["derivation_id"], r[key + "derivationId"], "failed exact target")
        expect_equal(row["evidence_schema_version"], 0, "failure never seals clean evidence")
        expect_equal(row["execution_outcome"], "failed", "real remote failure outcome")
        expect_equal(row["failure_class"], "deterministic", "terminal scanner failure classification")
        expect_equal(row["result_digest_sha256"], None, "failed scanner has no canonical evidence digest")
        expect(bool(row["error"] and row["execution_id"]), "failed scanner lacks execution diagnostics")
        expect_equal(sql_json(controller, "SELECT scan_id FROM cve_scan_vulnerability_observations WHERE scan_id=" +
                              literal(scan_id) + "::uuid"), [], "failed execution has no sealed observations")
        diagnostics = sql_json(controller,
            "SELECT event_type FROM cve_scan_diagnostic_events WHERE scan_id=" + literal(scan_id) +
            "::uuid AND execution_id=" + literal(row["execution_id"]) +
            "::uuid AND event_type='attempt_failed'")
        expect_equal(diagnostics, [{"event_type": "attempt_failed"}], "fenced scanner failure event")
        inventory = assurance_cve_inventory(variant, session=session)
        expect_equal(inventory["attempt"]["scan_id"], scan_id, "newest failed attempt is distinct from source")
        expect_equal(inventory["attempt"]["status"], "failed", "failed inventory attempt")
        receipt(key + "failedScanId", scan_id)


def assurance_cve_fault_rescan(variant, session="delivery-admin"):
    """Checks the real malformed-feed failure as a transient requeued attempt."""
    key = "assurance." + variant + "."
    r = receipts()
    with case("cve-" + variant + "-enqueue-real-fault-rescan"):
        status, queued = http(controller, "POST", "/api/v1/cves/rescan/" +
                              str(int(r[key + "derivationId"])), session=session, csrf=True)
        expect_equal(status, 202, "exact rescan API admission")
        expect_equal(queued["derivation_id"], r[key + "derivationId"], "rescan target identity")
        expect_equal(queued["enqueued"], True, "fresh fault rescan, not unrelated active work")
        scan_id = queued["scan_id"]
        receipt(key + "faultRescanId", scan_id)
    with case("cve-" + variant + "-upstream-fault-requeues-without-clean-evidence"):
        def requeued_attempt():
            return sql_json(controller,
                "SELECT execution_id::text AS execution_id, attempt_number, message "
                "FROM cve_scan_diagnostic_events WHERE scan_id=" + literal(scan_id) +
                "::uuid AND event_type='attempt_requeued' AND source='server' "
                "ORDER BY attempt_number, id LIMIT 1")

        event = wait_for(requeued_attempt, "actual upstream failure and remote requeue diagnostic", timeout=600)[0]
        expect("vulnix process failed" in event["message"], "unexpected requeue cause")
        expect(bool(event["execution_id"]) and event["attempt_number"] > 0,
               "requeued scanner fault lacks fenced execution identity")
        rows = sql_json(controller,
            "SELECT derivation_id, status, evidence_schema_version, result_digest_sha256 "
            "FROM cve_scans WHERE id=" + literal(scan_id) + "::uuid")
        expect_equal(len(rows), 1, "requeued scan remains the same intent")
        row = rows[0]
        expect_equal(row["derivation_id"], r[key + "derivationId"], "requeued exact target")
        expect(row["status"] in ("pending", "in_progress"), "transient failure became terminal or clean")
        expect_equal(row["evidence_schema_version"], 0, "upstream fault never seals evidence")
        expect_equal(row["result_digest_sha256"], None, "upstream fault has no completed digest")
        inventory = assurance_cve_inventory(variant, session=session)
        expect_equal(inventory["attempt"]["scan_id"], scan_id, "requeue retains a separate attempt/source identity")
        receipt(key + "faultExecutionId", event["execution_id"])
    return scan_id
