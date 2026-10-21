# Definitions only. Concatenate after the same-run cve.py and Delivery setup.
# Call assurance_triage_create() after vulnerable Current, BEFORE corrected
# build. Register triage.cjs's run export as browserSpecs."triage-create".
# Required facts.remediation: title, initialPlan, updatedPlan, note, milestone,
# targetDate (2027-02-01). Title is the UI-generated host patch title, not a
# triage input. The browser emits assurance.remediation.{poamId,triageRequest}.


def _assurance_triage_path():
    return ("/api/v1/systems/" + receipts()["assurance.vulnerable.systemId"] +
            "/cves/" + FACTS["cve"]["cveId"] + "/triage")


def _assurance_remediation_snapshot():
    """Reads protected remediation rows for side-effect comparisons.

    Session access timestamps and independent agent/scanner reports are not
    remediation mutations. The owner must serialize other remediation writers
    while a denial or invalid-payload probe runs in this fresh scenario database.
    """
    tables = ("poams", "poam_cve_findings", "poam_cve_finding_links",
              "cve_system_dispositions", "cve_environment_dispositions",
              "poam_activity", "poam_milestones", "poam_verification_attempts",
              "poam_cve_verification_items")
    return {table: sql_json(controller, "SELECT to_jsonb(t) AS row FROM " + table +
                            " t ORDER BY to_jsonb(t)::text") for table in tables}


def assurance_triage_create():
    """Creates a real host schedule through System CVEs and snapshots its baseline.

    Requires vulnerable Current and delivery.adminId from this invocation.
    Checks browser reopen/cancel through API detail comparisons, then verifies
    authored SQL baseline invariants. The browser sequence has no SQL snapshot
    boundary after its first create. Runs the independent Admin invalid-payload
    probe before returning the production-created POA&M ID. No case runs on load.
    """
    from datetime import datetime

    with case("triage-host-schedule-from-real-current-finding"):
        expect_equal(FACTS["remediation"]["targetDate"], "2027-02-01", "authored target date")
        assurance_cve_current("vulnerable")
        system = receipts()["assurance.vulnerable.systemId"]
        f = FACTS["cve"]
        existing = sql_json(controller, "SELECT id FROM poam_cve_findings WHERE system_id=" +
            literal(system) + "::uuid AND canonical_cve_id=" + literal(f["cveId"]) +
            " AND canonical_package_name=" + literal(f["package"]))
        expect_equal(existing, [], "fresh authored finding has no legacy remediation seed")
        run_browser(controller, "triage-create")
        r = receipts()
        poam_id = r["assurance.remediation.poamId"]
        status, triage = http(controller, "GET", _assurance_triage_path() +
            "?package=" + f["package"], session="delivery-admin")
        expect_equal(status, 200, "authoritative persisted triage read")
        expect_equal(triage["canonical_cve_id"], f["cveId"], "triage exact CVE")
        expect_equal(triage["canonical_package_name"], f["package"], "triage exact package")
        expect_equal(triage["scope"]["selected_system_id"], system, "triage selected host")
        expect_equal(triage["effective_source"], "host", "host-specific decision")
        expect_equal(triage["host_disposition"]["state"], "scheduled", "persisted scheduled disposition")
        expect_equal(triage["host_disposition"]["poam_id"], poam_id, "exact created plan")
        expect_equal(triage["environment_disposition"], None, "host schedule does not create environment decision")
        dispositions = sql_json(controller, "SELECT to_jsonb(d) AS row FROM cve_system_dispositions d "
            "WHERE poam_id=" + literal(poam_id) + "::uuid ORDER BY id")
        expect_equal(len(dispositions), 1, "one exact host disposition")
        disposition = dispositions[0]["row"]
        for field, value in (("system_id", system), ("canonical_cve_id", f["cveId"]),
                             ("canonical_package_name", f["package"]), ("state", "scheduled"),
                             ("scheduled_by", r["delivery.adminId"]), ("retired_at", None)):
            expect_equal(disposition[field], value, "stored disposition " + field)
        expect(bool(disposition["scheduled_at"]), "schedule lacks actor timestamp")
        receipt("assurance.remediation.dispositionId", disposition["id"])
        links = sql_json(controller, "SELECT to_jsonb(l) AS row FROM poam_cve_finding_links l "
            "WHERE poam_id=" + literal(poam_id) + "::uuid ORDER BY id")
        expect_equal(len(links), 1, "one real linked CVE finding")
        link = links[0]["row"]
        for field, value in (("system_id", system), ("canonical_cve_id", f["cveId"]),
            ("canonical_package_name", f["package"]), ("linked_by", r["delivery.adminId"]),
            ("retired_at", None), ("baseline_scan_id", r["assurance.vulnerable.scanId"]),
            ("baseline_scan_derivation_id", r["assurance.vulnerable.derivationId"]),
            ("baseline_generation", r["assurance.vulnerable.deployment"]["generation"]),
            ("baseline_target_store_path", r["assurance.vulnerable.outputPath"]),
            ("baseline_occurrence_derivation_path", r["assurance.vulnerable.packageDerivationPath"]),
            ("baseline_observed_package_version", f["vulnerableVersion"])):
            expect_equal(link[field], value, "opened-against " + field)
        scan_time = sql_json(controller, "SELECT completed_at FROM cve_scans WHERE id=" +
                             literal(r["assurance.vulnerable.scanId"]) + "::uuid")[0]["completed_at"]
        expect_equal(link["baseline_scan_completed_at"], scan_time, "exact baseline completion time")
        # Include every stored baseline field, including optional supplemental
        # provenance. Future comparisons must not silently omit a new field.
        receipt("assurance.remediation.baseline", {k: v for k, v in link.items() if k.startswith("baseline_")})
        receipt("assurance.remediation.linkId", link["id"])
        receipt("assurance.remediation.findingId", link["cve_finding_id"])
        receipt("assurance.remediation.triageDetail", triage)
        browser_findings = r["assurance.remediation.browserReopenedPoamDetail"]["cve_findings"]
        expect_equal(len(browser_findings), 1, "browser preserves one exact finding link")
        expect_equal(browser_findings[0]["id"], link["cve_finding_id"], "browser stable finding identity")
        expect_equal(browser_findings[0]["link_id"], link["id"], "browser stable link identity")
        for field in link:
            if field.startswith("baseline_") and field in browser_findings[0]:
                # PostgreSQL JSON and the API use different UTC spellings.
                if field == "baseline_scan_completed_at":
                    expect_equal(datetime.fromisoformat(browser_findings[0][field].replace("Z", "+00:00")),
                                 datetime.fromisoformat(link[field].replace("Z", "+00:00")),
                                 "browser API exact baseline scan time")
                else:
                    expect_equal(browser_findings[0][field], link[field], "browser API baseline " + field)
    assurance_triage_admin_invalid_host_schedule()
    return poam_id


def assurance_triage_admin_invalid_host_schedule(session="delivery-admin"):
    """Rejects missing POA&M metadata while preserving the existing host schedule.

    Called by triage_create after its SQL identity/baseline checks, before any
    downstream plan maintenance or corrected deployment. Uses an authenticated
    Admin and valid CSRF. The nine-table SQL comparison belongs only to this
    invalid request and subsequent read, not the earlier browser reopen.
    """
    with case("triage-admin-invalid-host-schedule-preserves-existing-remediation"):
        r = receipts()
        status, who = http(controller, "GET", "/api/auth/whoami", session=session)
        expect_equal(status, 200, "Admin live session")
        expect_equal(who["is_authenticated"], True, "Admin authenticated")
        expect("Admin" in who["roles"], "invalid payload probe requires real Admin authority")
        path = _assurance_triage_path()
        status, detail = http(controller, "GET", path + "?package=" + FACTS["cve"]["package"], session=session)
        expect_equal(status, 200, "existing host schedule readable before invalid request")
        expect_equal(detail, r["assurance.remediation.triageDetail"], "exact previously verified schedule")

        def evidence_snapshot():
            scan = literal(r["assurance.vulnerable.scanId"]) + "::uuid"
            return {
                "seal": sql_json(controller,
                    "SELECT status, evidence_schema_version, scanner_name, scanner_version, completed_at, "
                    "target_drv_path, target_outputs, result_digest_sha256, execution_outcome, closure_provenance "
                    "FROM cve_scans WHERE id=" + scan),
                "observations": sql_json(controller,
                    "SELECT to_jsonb(o) AS row FROM cve_scan_vulnerability_observations o WHERE scan_id=" +
                    scan + " ORDER BY to_jsonb(o)::text"),
            }

        evidence = evidence_snapshot()
        expect_equal(len(evidence["seal"]), 1, "original completed scan remains present")
        expect_equal(evidence["seal"][0], {field: r["assurance.vulnerable.scanEvidence"][field]
                     for field in evidence["seal"][0]}, "original completed scan seal before invalid request")
        original = r["assurance.vulnerable.observations"]
        expect_equal(len(original), 1, "one independently authored vulnerable occurrence")
        expect_equal([{field: row["row"][field] for field in original[0]}
                      for row in evidence["observations"]], original, "original scan observations before invalid request")
        before = _assurance_remediation_snapshot()
        status, error = http(controller, "POST", path, {
            "canonical_package_name": FACTS["cve"]["package"], "scope": "host", "action": "schedule_patch"
        }, session=session, csrf=True)
        expect_equal(status, 400, "Admin host schedule without POA&M is invalid with valid CSRF")
        expect_equal(error["error"], "invalid_poam_payload", "specific missing POA&M validation")
        status, after = http(controller, "GET", path + "?package=" + FACTS["cve"]["package"], session=session)
        expect_equal(status, 200, "host schedule remains readable after invalid request")
        expect_equal(after, detail, "invalid request preserves exact scheduled link and metadata")
        expect_equal(_assurance_remediation_snapshot(), before,
                     "invalid request and subsequent read preserve all nine remediation tables including baseline")
        expect_equal(evidence_snapshot(), evidence, "invalid request preserves completed scan seal and full observations")
        receipt("assurance.remediation.adminInvalidHostScheduleProven", True)


def assurance_triage_viewer_denials(viewer_session="cve-viewer"):
    """Optionally proves real scoped Viewer write denial without row changes.

    Call only after the owner creates and authenticates the scoped Viewer and
    after triage_create, while vulnerable Current remains deployed. This helper
    never creates an actor or substitutes an unauthenticated request.
    """
    with case("triage-scoped-viewer-cannot-change-host-schedule"):
        status, who = http(controller, "GET", "/api/auth/whoami", session=viewer_session)
        expect_equal(status, 200, "Viewer live session")
        expect_equal(who["is_authenticated"], True, "Viewer authenticated")
        expect_equal(who["roles"], ["Viewer"], "Viewer exact role")
        status, _ = http(controller, "GET", _assurance_triage_path() + "?package=" +
                         FACTS["cve"]["package"], session=viewer_session)
        expect_equal(status, 200, "Viewer has actual selected-system read scope")
        before = _assurance_remediation_snapshot()
        status, _ = http(controller, "POST", _assurance_triage_path(), {
            "canonical_package_name": FACTS["cve"]["package"], "scope": "host", "action": "leave_open"
        }, session=viewer_session, csrf=True)
        expect_equal(status, 403, "scoped Viewer cannot remove host schedule with valid CSRF")
        expect_equal(_assurance_remediation_snapshot(), before, "Viewer denial preserves remediation rows")
        receipt("assurance.remediation.viewerDenialProven", True)
