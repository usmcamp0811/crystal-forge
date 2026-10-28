# Definitions only; concatenate into evidence-remediation-lifecycle after its
# existing controller/helpers. Call assurance_stig_import() AFTER CVE lifecycle
# completion, with no concurrent policy/framework writer. No VM starts on load.
# facts.stigImport comes only from fixtures/stig-import-facts.py. Install its
# controllerPath XML files. Browser wrappers (exports in stig-import.cjs):
# stig-preview-v1r1=previewV1R1, stig-import-v1r1=importV1R1,
# stig-import-v1r2=importV1R2, stig-invalid-truncated=invalidTruncated,
# stig-invalid-non-xccdf=invalidNonXccdf, stig-mapping=mapping,
# stig-mapping-reload=mappingReload. Each run_browser uses a fresh context.
# Requires same-run delivery.adminId and authenticated delivery-admin HTTP jar.
# Authority: TASK-490.19 AC1-4; design 6e4ea520 ImportStigModal review/done,
# PolicyEditor Compliance and PoliciesView InlineMappingEditor. This extends
# assurance.policy.stig-import, retaining input identity/persistence assertions.


def _stig_snapshot():
    """Reads all import-owned rows in one SQL statement's consistent snapshot.

    Compare full rows, not counts: an update or delete/insert with equal counts
    is also a prohibited preview/validation write. Artifact bytes use SHA-256
    plus size to keep snapshots bounded. Session timestamps and unrelated CVE
    evidence are outside this write boundary. Only successful import audit
    events are included; other background audit writers can continue.
    """
    tables = (
        "deployment_policies", "deployment_policy_versions", "compliance_bundles",
        "compliance_bundle_versions", "compliance_bundle_version_policies",
        "compliance_bundle_environments", "compliance_frameworks",
        "compliance_framework_versions", "compliance_requirements",
        "compliance_requirement_versions", "policy_requirement_mappings",
        "compliance_bundle_version_requirements", "compliance_source_object_mappings",
    )
    selects = ["SELECT " + literal(table) + " AS table_name, to_jsonb(t) AS row FROM " +
               table + " t" for table in tables]
    selects += [
        "SELECT 'compliance_source_artifacts', (to_jsonb(t)-'content') || "
        "jsonb_build_object('content_sha256', encode(sha256(content),'hex'), "
        "'content_size', octet_length(content)) FROM compliance_source_artifacts t",
        "SELECT 'admin_audit_events', to_jsonb(t) FROM admin_audit_events t "
        "WHERE action='xccdf_imported'",
    ]
    return sql_json(controller, "SELECT * FROM (" + " UNION ALL ".join(selects) +
                    ") snapshot ORDER BY table_name, row::text")


def _stig_release_rows(release):
    """Verifies the complete source-derived release and returns generated IDs.

    Captured UUIDs are receipts, never expected semantic facts. The source hash
    checks byte provenance only. No requirement about policy trust, a computed
    semantic digest, or draft interchange behavior is introduced here.
    """
    f = FACTS["stigImport"]["releases"][release]
    result = receipts()["assurance.stig." + release + ".import"]
    source = literal(result["source_artifact_id"]) + "::uuid"
    artifacts = sql_json(controller,
        "SELECT id, sha256, octet_length(content) AS size, "
        "encode(sha256(content),'hex') AS content_sha256, imported_by "
        "FROM compliance_source_artifacts WHERE id=" + source)
    expect_equal(len(artifacts), 1, release + " exact source artifact")
    artifact = artifacts[0]
    expect_equal(artifact["sha256"], f["sha256"], "stored input digest")
    expect_equal(artifact["content_sha256"], f["sha256"], "actual persisted source bytes digest")
    expect_equal(artifact["size"], f["byteLength"], "actual persisted source byte length")
    expect_equal(artifact["imported_by"], receipts()["delivery.adminId"], "real importing Admin")
    versions = sql_json(controller,
        "SELECT id, framework_id, version, canonical_release_key FROM compliance_framework_versions "
        "WHERE source_artifact_id=" + source)
    expect_equal(len(versions), 1, release + " one framework release")
    version = versions[0]
    expect_equal(version["version"], f["release"], "input-derived framework version")
    expect_equal(version["canonical_release_key"], f["release"], "input-derived release identity")
    requirements = sql_json(controller,
        "SELECT rv.id, rv.requirement_id, rv.external_id, r.canonical_requirement_key, "
        "rv.title, rv.severity, rv.metadata->>'version' AS version "
        "FROM compliance_requirement_versions rv JOIN compliance_requirements r ON r.id=rv.requirement_id "
        "WHERE rv.kind='rule' AND rv.framework_version_id=" + literal(version["id"]) +
        "::uuid ORDER BY rv.external_id")
    expect_equal([{"id": row["external_id"], "stableKey": row["canonical_requirement_key"],
                   "title": row["title"], "version": row["version"], "severity": row["severity"]}
                  for row in requirements], f["rules"], release + " complete exact source requirement set")
    policies = sql_json(controller,
        "SELECT m.source_identity, pv.id AS policy_version_id, pv.policy_id, pv.name, "
        "pv.source_artifact_id FROM compliance_source_object_mappings m "
        "JOIN deployment_policy_versions pv ON pv.id=m.policy_version_id "
        "WHERE m.object_kind='rule' AND m.source_artifact_id=" + source + " ORDER BY m.source_identity")
    expect_equal([p["source_identity"] for p in policies], [r["id"] for r in f["rules"]],
                 release + " exact imported policy source identities")
    expect_equal(len({p["policy_version_id"] for p in policies}), f["ruleCount"],
                 "create-all makes one distinct policy version per selected rule")
    expect_equal(sorted(result["created_policy_version_ids"]), sorted(p["policy_version_id"] for p in policies),
                 "HTTP import IDs equal authoritative stored policies")
    for policy in policies:
        expect_equal(policy["source_artifact_id"], result["source_artifact_id"], "policy exact source provenance")
    bundle = literal(result["bundle_version_id"]) + "::uuid"
    benchmark = sql_json(controller, "SELECT source_identity, bundle_version_id FROM "
        "compliance_source_object_mappings WHERE object_kind='benchmark' AND source_artifact_id=" + source)
    expect_equal(benchmark, [{"source_identity": f["benchmarkId"],
                             "bundle_version_id": result["bundle_version_id"]}], "benchmark exact bundle provenance")
    membership = sql_json(controller, "SELECT policy_version_id FROM compliance_bundle_version_policies "
                          "WHERE bundle_version_id=" + bundle + " ORDER BY policy_version_id")
    expect_equal([m["policy_version_id"] for m in membership], sorted(p["policy_version_id"] for p in policies),
                 "bundle exact imported policy membership")
    stored_bundle = sql_json(controller, "SELECT bundle_id, source_artifact_id FROM "
                            "compliance_bundle_versions WHERE id=" + bundle)
    expect_equal(stored_bundle, [{"bundle_id": result["bundle_id"],
                                  "source_artifact_id": result["source_artifact_id"]}], "bundle source provenance")
    expect_equal(sql_json(controller, "SELECT name FROM compliance_bundles WHERE id=" +
                          literal(result["bundle_id"]) + "::uuid"), [{"name": f["bundleName"]}],
                 "browser-authored release bundle name persists exactly")
    value = {"frameworkId": version["framework_id"], "frameworkVersionId": version["id"],
              "requirements": requirements, "policies": policies}
    # Browser handoff needs generated identities, not duplicate titles and
    # metadata. Keep the complete rows locally for all lineage/mapping checks;
    # serializing both releases' rows would exceed the VM shell argument limit.
    receipt("assurance.stig." + release + ".identities", {
        "frameworkId": version["framework_id"], "frameworkVersionId": version["id"],
        "requirementVersionIds": [row["id"] for row in requirements],
        "policyVersionIds": [row["policy_version_id"] for row in policies],
    })
    return value


def _stig_mapping_rows():
    target = receipts()["assurance.stig.mappingTarget"]
    return sql_json(controller, "SELECT to_jsonb(m) AS row FROM policy_requirement_mappings m "
        "WHERE policy_version_id=" + literal(target["policyVersionId"]) + "::uuid "
        "AND requirement_version_id=" + literal(target["requirementVersionId"]) + "::uuid ORDER BY id")


def assurance_stig_import():
    """Runs import/atomicity/version/mapping cases in the existing Assurance DB.

    The owner calls this once after the CVE lifecycle. Later policy-family cases
    may consume assurance.stig.* receipts only in this same scenario invocation.
    Required helpers: case/expect/expect_equal/literal/sql_json/http/receipts/
    receipt/run_browser and controller. No top-level driver action runs on load.
    """
    with case("stig-import-real-admin-and-independent-source-prerequisites"):
        status, who = http(controller, "GET", "/api/auth/whoami", session="delivery-admin")
        expect_equal(status, 200, "same-run Admin session")
        expect_equal(who["is_authenticated"], True, "Admin is authenticated")
        expect("Admin" in who["roles"], "STIG import requires actual Admin authority")
        f = FACTS["stigImport"]
        expect(f["releases"]["V1R1"]["sha256"] != f["releases"]["V1R2"]["sha256"],
               "release fixtures must be distinct actual inputs")
        expect_equal(len(set(f["stableSubset"])), 3, "three distinct stable cross-release controls")
        for release in ("V1R1", "V1R2"):
            source = f["releases"][release]
            expect_equal(source["release"], release, "source-derived release")
            expect_equal(sql_json(controller, "SELECT id FROM compliance_source_artifacts WHERE sha256=" +
                                  literal(source["sha256"])), [], "first real import has no preseeded artifact")

    with case("stig-v1r1-preview-and-cancel-write-no-policy-or-framework-rows"):
        before = _stig_snapshot()
        run_browser(controller, "stig-preview-v1r1")
        expect_equal(_stig_snapshot(), before, "preview/cancel preserves every protected row")
        receipt("assurance.stig.previewAtomic", True)

    for name in ("truncated", "non-xccdf"):
        with case("stig-" + name + "-visible-rejection-and-no-partial-import"):
            before = _stig_snapshot()
            run_browser(controller, "stig-invalid-" + name)
            expect_equal(_stig_snapshot(), before, "invalid preview and commit preserve every protected row")
            receipt("assurance.stig.invalid." + name + ".atomic", True)

    with case("stig-v1r1-browser-import-persists-exact-input-identities-and-provenance"):
        run_browser(controller, "stig-import-v1r1")
        first = _stig_release_rows("V1R1")

    with case("stig-v1r2-browser-import-preserves-all-v1r1-version-records"):
        # All existing version/source/mapping rows must survive byte-for-byte.
        # Lineage current pointers may legitimately move to a newer draft.
        immutable_tables = {"deployment_policy_versions", "compliance_bundle_versions",
            "compliance_bundle_version_policies", "compliance_source_artifacts",
            "compliance_source_object_mappings", "compliance_framework_versions",
            "compliance_requirement_versions", "policy_requirement_mappings",
            "compliance_bundle_version_requirements", "compliance_requirements"}
        before = [row for row in _stig_snapshot() if row["table_name"] in immutable_tables]
        run_browser(controller, "stig-import-v1r2")
        second = _stig_release_rows("V1R2")
        after = _stig_snapshot()
        for row in before:
            expect(row in after, "V1R2 rewrote or removed an existing " + row["table_name"] + " row")
        expect_equal(first["frameworkId"], second["frameworkId"], "same benchmark framework lineage")
        expect(first["frameworkVersionId"] != second["frameworkVersionId"], "distinct release versions")
        for stable in f["stableSubset"]:
            a = next(r for r in first["requirements"] if r["canonical_requirement_key"] == stable)
            b = next(r for r in second["requirements"] if r["canonical_requirement_key"] == stable)
            expect_equal(a["requirement_id"], b["requirement_id"], "stable requirement lineage across releases")
            expect(a["id"] != b["id"], "release-specific requirement version")
        receipt("assurance.stig.v1r1Preserved", True)

    with case("stig-manual-requirement-mapping-persists-through-database-and-browser-reload"):
        stable = f["stableSubset"][0]
        a = next(r for r in first["requirements"] if r["canonical_requirement_key"] == stable)
        b = next(r for r in second["requirements"] if r["canonical_requirement_key"] == stable)
        policy = next(p for p in first["policies"] if p["source_identity"] == a["external_id"])
        target = {"policyId": policy["policy_id"], "policyVersionId": policy["policy_version_id"],
                  "policyName": policy["name"], "frameworkId": second["frameworkId"],
                  "frameworkVersionId": second["frameworkVersionId"], "requirementVersionId": b["id"],
                  "externalId": b["external_id"], "requirementTitle": b["title"], "stableKey": stable}
        receipt("assurance.stig.mappingTarget", target)
        expect_equal(_stig_mapping_rows(), [], "UI must create this cross-release mapping")
        run_browser(controller, "stig-mapping")
        rows = _stig_mapping_rows()
        expect_equal(len(rows), 1, "one exact UI-created mapping")
        mapping = rows[0]["row"]
        for field, value in (("relationship", "supports"), ("coverage", "partial"),
                             ("rationale", f["mappingRationale"]), ("provenance", "manual"),
                             ("created_by", receipts()["delivery.adminId"])):
            expect_equal(mapping[field], value, "stored manual mapping " + field)
        receipt("assurance.stig.mappingId", mapping["id"])
        status, mappings = http(controller, "GET", "/api/v1/policy-versions/" +
            target["policyVersionId"] + "/requirement-mappings", session="delivery-admin")
        expect_equal(status, 200, "real authenticated mapping read")
        expect_equal([m["id"] for m in mappings if m["requirement_version_id"] == b["id"]],
                     [mapping["id"]], "API exposes exact SQL mapping identity")
        run_browser(controller, "stig-mapping-reload")
        expect_equal(_stig_mapping_rows(), rows, "fresh-context reload preserves full mapping row")
