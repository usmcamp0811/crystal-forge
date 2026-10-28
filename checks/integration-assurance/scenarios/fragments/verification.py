# Definitions only; concatenate after triage.py and poam-plan.py.
# built_not_deployed: after corrected completed scan, before corrected activation.
# verify_close: after corrected real deployment + assurance_cve_current.
# Register verification.cjs as browserSpecs."poam-verify-close". All writes use
# production APIs; SQL only observes. No closed/completed evidence is seeded.


def assurance_poam_built_not_deployed():
    """Checks built-clean cannot replace Current or close the original plan.

    Exercises only the invalid-state 409 while in_progress. Verification in
    awaiting_verification can audit a rejected attempt and bump revision. This
    bounded case does not claim that broader no-mutation acceptance criterion.
    """
    with case("poam-clean-build-keeps-vulnerable-current-and-original-baseline"):
        assurance_cve_built_not_deployed()
        detail = _assurance_poam_read()
        _assurance_poam_assert_detail(detail, "vulnerable", "in_progress")
        expect_equal(detail["closed_at"], None, "built-clean does not close plan")
        expect_equal(detail["closure_attempt_id"], None, "built-clean is not closure evidence")
        before = _assurance_remediation_snapshot()
        status, error = http(controller, "POST", "/api/v1/poams/" + detail["id"] + "/verify",
            {"revision": detail["revision"]}, session="delivery-admin", csrf=True)
        expect_equal(status, 409, "verification requires awaiting_verification")
        expect_equal(error["error"], "invalid_transition", "specific premature lifecycle rejection")
        expect_equal(_assurance_remediation_snapshot(), before, "invalid-state verification has no remediation write")
        receipt("assurance.remediation.builtNotDeployedProven", True)


def _assurance_poam_attempt(attempt_id):
    """Checks one persisted sealed PASS attempt against exact same-run evidence."""
    r = receipts()
    rows = sql_json(controller, "SELECT to_jsonb(a) AS row FROM poam_verification_attempts a WHERE id=" +
                    literal(attempt_id) + "::uuid")
    expect_equal(len(rows), 1, "one persisted verification attempt")
    attempt = rows[0]["row"]
    for field, value in (("poam_id", r["assurance.remediation.poamId"]), ("outcome", "accepted"),
                         ("attempted_by", r["delivery.adminId"])):
        expect_equal(attempt[field], value, "verification attempt " + field)
    expect(bool(attempt["sealed_at"]), "verification attempt not sealed")
    items = sql_json(controller, "SELECT to_jsonb(i) AS row FROM poam_cve_verification_items i WHERE attempt_id=" +
                     literal(attempt_id) + "::uuid")
    expect_equal(len(items), 1, "exact verification finding set")
    item = items[0]["row"]
    for field, value in r["assurance.remediation.baseline"].items():
        expect_equal(item[field], value, "verification retains baseline " + field)
    for field, value in (("cve_finding_id", r["assurance.remediation.findingId"]),
        ("system_id", r["assurance.vulnerable.systemId"]), ("canonical_cve_id", FACTS["cve"]["cveId"]),
        ("canonical_package_name", FACTS["cve"]["package"]), ("result", "pass"),
        ("scan_id", r["assurance.corrected.scanId"]), ("scan_derivation_id", r["assurance.corrected.derivationId"]),
        ("generation", r["assurance.corrected.deployment"]["generation"]),
        ("target_store_path", r["assurance.corrected.outputPath"]), ("occurrence_present", False)):
        expect_equal(item[field], value, "verification actual corrected Current " + field)
    newer = sql_json(controller, "SELECT scan_completed_at > baseline_scan_completed_at AS newer "
                     "FROM poam_cve_verification_items WHERE attempt_id=" + literal(attempt_id) + "::uuid")
    expect_equal(newer, [{"newer": True}], "authoritative verification is later than opened-against evidence")
    return {"attempt": attempt, "item": item}


def assurance_poam_verify_close():
    """Verifies and closes only after real corrected activation and a newer clean scan.

    Returns the authoritative completed detail. Records verifyAttemptId,
    closureAttemptId, verificationEvidence, closureEvidence and closedDetail.
    The separate browser export proves completed history and exact Current
    identity survive reload; closure does its own fresh authoritative check.
    """
    with case("poam-newer-clean-current-explicit-verification-and-authoritative-close"):
        r = receipts()
        expect_equal(r["assurance.remediation.builtNotDeployedProven"], True, "earlier undeployed continuity boundary")
        assurance_cve_current("corrected")
        assurance_cve_inventory("corrected", current=True)
        path = "/api/v1/poams/" + r["assurance.remediation.poamId"]
        detail = _assurance_poam_read()
        _assurance_poam_assert_detail(detail, "corrected", "in_progress")
        expect_equal(detail["closed_at"], None, "actual clean scan still does not automatically close plan")
        detail = delivery_api("POST", path + "/transition", {
            "revision": detail["revision"], "status": "awaiting_verification"})
        _assurance_poam_assert_detail(detail, "corrected", "awaiting_verification")
        verified = delivery_api("POST", path + "/verify", {"revision": detail["revision"]})
        expect_equal(verified["outcome"], "accepted", "actual clean Current permits verification")
        expect_equal(verified["items"], [], "CVE verification does not mix policy findings")
        expect_equal(verified["cve_items"], [{"cve_finding_id": r["assurance.remediation.findingId"],
            "result": "pass", "scan_id": r["assurance.corrected.scanId"]}], "exact verified finding and source")
        evidence = _assurance_poam_attempt(verified["attempt_id"])
        receipt("assurance.remediation.verifyAttemptId", verified["attempt_id"])
        receipt("assurance.remediation.verificationEvidence", evidence)
        detail = _assurance_poam_read()
        _assurance_poam_assert_detail(detail, "corrected", "awaiting_verification")
        expect_equal(detail["closed_at"], None, "successful verification alone does not close plan")
        closed = delivery_api("POST", path + "/close", {"revision": detail["revision"]})
        expect_equal(closed["status"], "completed", "authoritative close succeeds")
        closure = closed["closure_attempt_id"]
        expect(closure != verified["attempt_id"], "closure must reverify Current rather than reuse an old result")
        receipt("assurance.remediation.closureAttemptId", closure)
        receipt("assurance.remediation.closureEvidence", _assurance_poam_attempt(closure))
        expect_equal(_assurance_poam_attempt(verified["attempt_id"]), evidence, "close preserves earlier sealed verification")
        persisted = _assurance_poam_read()
        _assurance_poam_assert_detail(persisted, "corrected", "completed")
        expect_equal(persisted["closure_attempt_id"], closure, "closure attempt survives authoritative read")
        expect(bool(persisted["closed_at"]), "completed plan lacks closure timestamp")
        link = _assurance_poam_baseline()
        expect_equal(link["retirement_reason"], "closed:" + closure, "closure retires original link with provenance")
        expect_equal(link["retired_by"], r["delivery.adminId"], "closure actor on original link")
        disposition = sql_json(controller, "SELECT retired_by::text, retirement_reason FROM cve_system_dispositions "
            "WHERE id=" + literal(r["assurance.remediation.dispositionId"]) + "::uuid")
        expect_equal(disposition, [{"retired_by": r["delivery.adminId"], "retirement_reason": "poam_closed"}],
                     "authoritative close retires original host schedule")
        receipt("assurance.remediation.closedDetail", persisted)
        run_browser(controller, "poam-verify-close")
        return persisted


def assurance_poam_viewer_denials(viewer_session="cve-viewer"):
    """Optionally checks scoped Viewer plan, verification and closure denials.

    Call after maintain, before verify_close, once the owner has supplied the
    real Viewer session. Valid CSRF and a successful detail read distinguish
    role denial from authentication, visibility, or lifecycle failure.
    """
    with case("poam-scoped-viewer-cannot-write-verify-or-close"):
        status, who = http(controller, "GET", "/api/auth/whoami", session=viewer_session)
        expect_equal(status, 200, "Viewer live session")
        expect_equal(who["is_authenticated"], True, "Viewer authenticated")
        expect_equal(who["roles"], ["Viewer"], "Viewer exact role")
        path = "/api/v1/poams/" + receipts()["assurance.remediation.poamId"]
        status, detail = http(controller, "GET", path, session=viewer_session)
        expect_equal(status, 200, "Viewer can read exact plan scope")
        expect_equal(detail["id"], receipts()["assurance.remediation.poamId"], "Viewer selected plan")
        before = _assurance_remediation_snapshot()
        for suffix, payload in (("/notes", {"text": FACTS["remediation"]["note"]}),
                                ("/verify", {}), ("/close", {})):
            status, _ = http(controller, "POST", path + suffix,
                dict(payload, revision=detail["revision"]), session=viewer_session, csrf=True)
            expect_equal(status, 403, "Viewer role denial with valid CSRF: " + suffix)
            expect_equal(_assurance_remediation_snapshot(), before,
                         "Viewer denied " + suffix + " preserves all remediation rows")
        receipt("assurance.remediation.viewerPlanDenialsProven", True)
