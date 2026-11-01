# TASK-508 AC8. The preceding configuration case supplies actual native build,
# verified publication and sealed scanner evidence. This fragment creates only
# a documentation commit; no successful execution or scan is inserted with SQL.
def configuration_exact_artifact_reuse(config, system_ids, origin, job, origin_commit):
    flake = receipts()["delivery.flakeId"]
    managed = receipts()["delivery.targetId"]
    with case("configuration-doc-only-revision-reuses-exact-real-artifact"):
        current_sql = ("SELECT system_id::text,generation,store_path,derivation_id,commit_id "
            "FROM view_current_cve_authority WHERE system_id=" + literal(managed) + "::uuid")
        current_before = sql_json(controller, current_sql)
        expect(bool(current_before), "managed runtime must have real exact Current evidence before doc-only change")
        source_rows = sql_json(controller, "SELECT id::text,derivation_id,status,builder_id::text,builder_session_id::text "
            "FROM build_jobs WHERE derivation_id=" + str(origin["id"]))
        expect_equal(len(source_rows), 1, "one actual source execution before reuse")
        services.succeed("printf '%s\\n' 'AC8 documentation-only artifact reuse proof.' > /tmp/delivery-source/AC8.md && "
            "git -C /tmp/delivery-source add AC8.md && "
            "git -C /tmp/delivery-source commit -m 'Document unchanged Nix artifact' && "
            "git -C /tmp/delivery-source push --receive-pack='runuser -u git -- git-receive-pack' /srv/git/delivery.git main")
        revision = services.succeed("git -C /tmp/delivery-source rev-parse HEAD").strip()
        expect(revision != origin_commit, "documentation commit must have a distinct source identity")
        independently_evaluated = services.succeed("nix eval --offline --raw /tmp/delivery-source#nixosConfigurations."
            + config + ".config.system.build.toplevel.drvPath").strip()
        expect_equal(independently_evaluated, origin["derivation_path"], "independent real Nix evaluation proves unchanged actual drv")
        delivery_api("POST", "/api/v1/flakes/" + str(flake) + "/sync", {})

        def bound():
            rows = sql_json(controller, "SELECT d.id,d.commit_id,d.derivation_path,d.expected_store_path,"
                "d.reused_from_derivation_id,d.reused_build_job_id::text,c.evaluation_status "
                "FROM derivations d JOIN commits c ON c.id=d.commit_id WHERE c.flake_id=" + str(flake)
                + " AND c.git_commit_hash=" + literal(revision) + " AND d.derivation_name=" + literal(config))
            return rows[0] if len(rows) == 1 and rows[0]["reused_build_job_id"] \
                and rows[0]["evaluation_status"] == "complete" else None

        binding = wait_for(bound, "production native B carrier, adopted work and durable completion proof", timeout=300)
        expect(binding["id"] != origin["id"], "B must retain its own configuration/evaluation carrier")
        for field, expected in (("derivation_path", origin["derivation_path"]),
            ("expected_store_path", origin["output"]), ("reused_from_derivation_id", origin["id"]),
            ("reused_build_job_id", job["id"])):
            expect_equal(binding[field], expected, "exact reuse binding " + field)
        expect_equal(sql_json(controller, "SELECT id::text,derivation_id,status,builder_id::text,builder_session_id::text "
            "FROM build_jobs WHERE derivation_id=" + str(origin["id"])), source_rows,
            "reuse preserves actual execution provenance and session")
        expect_equal(sql_json(controller, "SELECT id FROM build_jobs WHERE derivation_id=" + str(binding["id"])),
            [], "doc-only B has no fabricated or duplicate build")
        expect_equal(sql_json(controller, "SELECT id FROM cve_scans WHERE derivation_id=" + str(binding["id"])),
            [], "doc-only B has no relabelled or duplicate producing scan")
        for system_id in system_ids:
            evidence = delivery_api("GET", "/api/v1/systems/" + system_id + "/configuration-assurance")
            expect_equal(evidence["selected_revision"]["sha"], revision, "selected source is B")
            expect_equal(evidence["evaluation"]["state"], "evaluated", "B owns native evaluation evidence")
            artifact = evidence["selected_artifact"]
            expect_equal(artifact["derivation_id"], binding["id"], "selected configuration carrier is B")
            expect_equal(artifact["revision"]["sha"], revision, "evaluated artifact revision stays B")
            expect_equal(artifact["execution_derivation_id"], origin["id"], "actual producing derivation stays A")
            expect_equal(artifact["execution_revision"]["sha"], origin_commit, "actual execution source stays A")
            expect_equal(artifact["build"]["job_id"], job["id"], "one actual producing job")
            expect_equal(artifact["build"]["revision"]["sha"], origin_commit, "job is never labelled B executed")
            selected_scan = evidence["selected_scan"]
            expect_equal(selected_scan["derivation_id"], origin["id"], "scan retains immutable actual source")
            expect_equal(selected_scan["revision"]["sha"], origin_commit, "scan source never becomes B")
            expect_equal(selected_scan["evidence_schema_version"], 1, "selected evidence is actually sealed")
            inventory = delivery_api("GET", "/api/v1/systems/" + system_id
                + "/cve-inventory-page?target=exact_derivation&target_id=" + str(binding["id"]))
            expect_equal(inventory["read_only"], True, "B-selected configuration inventory remains observational")
            expect_equal(inventory["source"]["scan_id"], selected_scan["scan_id"],
                "exact B selection consumes A's real scan without relabelling the source")
            expect_equal(sql_json(controller, "SELECT system_id FROM view_current_cve_authority WHERE system_id="
                + literal(system_id) + "::uuid"), [], "nonmanaged reuse grants no Current authority")
        expect_equal(sql_json(controller, current_sql), current_before,
            "head B cannot rewrite actual managed runtime or exact Current source identity")
        current_scan = sql_json(controller, "SELECT scan.derivation_id,scan.evidence_schema_version,scan.status "
            "FROM view_current_cve_authority authority JOIN cve_scans scan ON scan.id=authority.scan_id "
            "WHERE authority.system_id=" + literal(managed) + "::uuid")
        expect_equal(current_scan, [{"derivation_id": current_before[0]["derivation_id"],
            "evidence_schema_version": 1, "status": "completed"}],
            "normal cadence may refresh a real scan but cannot substitute B as its source")
        receipt("management.reuseBinding", dict(binding, sha=revision, origin_sha=origin_commit))
