# Definitions only; concatenate after triage.py. Uses the existing delivery_api
# and http helpers with the local-to-Assurance delivery-admin session.
# Call maintain after triage_create and before corrected build. Register
# poam-plan.cjs as browserSpecs."poam-maintain"; its run export checks reload.


def _assurance_poam_read():
    return delivery_api("GET", "/api/v1/poams/" + receipts()["assurance.remediation.poamId"])


def _assurance_poam_baseline():
    """Checks complete stored baseline and stable link identity across revisions."""
    r = receipts()
    rows = sql_json(controller, "SELECT to_jsonb(l) AS row FROM poam_cve_finding_links l WHERE poam_id=" +
                    literal(r["assurance.remediation.poamId"]) + "::uuid ORDER BY id")
    expect_equal(len(rows), 1, "revision change cannot duplicate or replace plan link")
    link = rows[0]["row"]
    expect_equal(link["id"], r["assurance.remediation.linkId"], "original link identity")
    expect_equal(link["cve_finding_id"], r["assurance.remediation.findingId"], "stable finding identity")
    expect_equal({k: v for k, v in link.items() if k.startswith("baseline_")},
                 r["assurance.remediation.baseline"], "entire immutable opened-against baseline")
    findings = sql_json(controller, "SELECT id::text, system_id::text, canonical_cve_id, canonical_package_name "
        "FROM poam_cve_findings WHERE system_id=" + literal(r["assurance.vulnerable.systemId"]) +
        "::uuid AND canonical_cve_id=" + literal(FACTS["cve"]["cveId"]) +
        " AND canonical_package_name=" + literal(FACTS["cve"]["package"]))
    expect_equal(findings, [{"id": r["assurance.remediation.findingId"],
        "system_id": r["assurance.vulnerable.systemId"], "canonical_cve_id": FACTS["cve"]["cveId"],
        "canonical_package_name": FACTS["cve"]["package"]}], "one durable exact finding")
    return link


def _assurance_poam_assert_detail(detail, variant, expected_status):
    """Checks authored plan metadata, exact current evidence, and API baseline."""
    from datetime import datetime

    r = receipts()
    plan = FACTS["remediation"]
    expect_equal(detail["id"], r["assurance.remediation.poamId"], "same remediation episode")
    expect_equal(detail["status"], expected_status, "approved lifecycle state")
    expect_equal(detail["title"], plan["title"], "authored plan title")
    expect_equal(detail["target_date"], plan["targetDate"], "authored target date")
    expect_equal(detail["assignee"]["kind"], "user", "typed user assignee")
    expect_equal(detail["assignee"]["user_id"], r["delivery.adminId"], "same assigned administrator")
    expect_equal(detail["findings"], [], "CVE plan does not mix policy finding family")
    expect_equal(len(detail["cve_findings"]), 1, "one exact CVE finding in API")
    finding = detail["cve_findings"][0]
    expect_equal(finding["id"], r["assurance.remediation.findingId"], "API stable finding")
    expect_equal(finding["link_id"], r["assurance.remediation.linkId"], "API original link")
    for field in ("system_id", "canonical_cve_id", "canonical_package_name"):
        expected = {"system_id": r["assurance.vulnerable.systemId"],
                    "canonical_cve_id": FACTS["cve"]["cveId"],
                    "canonical_package_name": FACTS["cve"]["package"]}[field]
        expect_equal(finding[field], expected, "API exact " + field)
    for field, value in r["assurance.remediation.baseline"].items():
        # CveFindingView omits baseline_scan_derivation_id. SQL checks that
        # field; timestamps use different RFC3339 formatting in the API.
        if field == "baseline_scan_completed_at":
            expect_equal(datetime.fromisoformat(finding[field].replace("Z", "+00:00")),
                         datetime.fromisoformat(value.replace("Z", "+00:00")), "API exact baseline scan time")
        elif field in finding:
            expect_equal(finding[field], value, "API immutable " + field)
    key = "assurance." + variant + "."
    for field, value in (("current_scan_id", r[key + "scanId"]),
        ("current_derivation_id", r[key + "derivationId"]),
        ("current_target_store_path", r[key + "outputPath"]),
        ("current_generation", r[key + "deployment"]["generation"])):
        expect_equal(finding[field], value, "plan exact Current " + field)
    expect_equal(finding["resolution_state"], "pass" if variant == "corrected" else "fail",
                 "clean Current differs from still affected baseline")
    _assurance_poam_baseline()
    return finding


def assurance_poam_maintain():
    """Persists authored edits, note, milestone and in_progress via production APIs.

    Requires triage_create in this run. Captures maintainedDetail and milestoneId
    receipts, then exercises the real detail reload with poam-maintain.
    """
    with case("poam-real-cve-plan-maintenance-and-reload"):
        r = receipts()
        plan = FACTS["remediation"]
        path = "/api/v1/poams/" + r["assurance.remediation.poamId"]
        detail = _assurance_poam_read()
        _assurance_poam_assert_detail(detail, "vulnerable", "open")
        expect_equal(detail["plan"], plan["initialPlan"], "UI-authored original plan persisted")
        detail = delivery_api("PATCH", path, {"revision": detail["revision"], "plan": plan["updatedPlan"]})
        detail = delivery_api("POST", path + "/transition", {
            "revision": detail["revision"], "status": "in_progress"})
        detail = delivery_api("POST", path + "/notes", {
            "revision": detail["revision"], "text": plan["note"]})
        detail = delivery_api("POST", path + "/milestones", {
            "revision": detail["revision"], "title": plan["milestone"], "target_date": plan["targetDate"]}, expected=201)
        # Re-read rather than treating a mutation response as persistence proof.
        detail = _assurance_poam_read()
        _assurance_poam_assert_detail(detail, "vulnerable", "in_progress")
        expect_equal(detail["plan"], plan["updatedPlan"], "authored plan update persisted")
        notes = [a for a in detail["activity"] if a["kind"] == "note" and a["payload"].get("text") == plan["note"]]
        expect_equal(len(notes), 1, "one authored durable note")
        expect_equal(notes[0]["actor_user_id"], r["delivery.adminId"], "note actor identity")
        milestones = [m for m in detail["milestones"] if m["title"] == plan["milestone"]]
        expect_equal(len(milestones), 1, "one authored maintenance milestone")
        milestone = milestones[0]
        for field, value in (("created_by", r["delivery.adminId"]), ("updated_by", r["delivery.adminId"]),
                             ("target_date", plan["targetDate"]), ("completed_at", None)):
            expect_equal(milestone[field], value, "persisted milestone " + field)
        stored = sql_json(controller, "SELECT title, plan, status, target_date, owner_user_id::text "
            "FROM poams WHERE id=" + literal(detail["id"]) + "::uuid")
        expect_equal(stored, [{"title": plan["title"], "plan": plan["updatedPlan"], "status": "in_progress",
            "target_date": plan["targetDate"], "owner_user_id": r["delivery.adminId"]}], "stored plan metadata")
        receipt("assurance.remediation.milestoneId", milestone["id"])
        receipt("assurance.remediation.maintainedDetail", detail)
        run_browser(controller, "poam-maintain")
        return detail
