# Same-run prerequisites: delivery setup, scanner registration, author_revision,
# normal feed restored, all original remediation checks completed. No extra VM.
# Browser registers the declared agent-disabled output; a second API registration
# shares it. Only real native evaluation/build/post-build scanning creates proof.
def configuration_assurance():
    config = "assuranceOnly"
    with case("configuration-keyless-browser-registration-and-shared-api-identity"):
        # A controlled parent-side unit supplies real ActiveState, not a guest
        # machine or an attested configuration. The agent must collect/sign it.
        targetA.succeed("systemd-run --unit=microvm@external-assurance.service sleep infinity")
        targetA.wait_for_unit("microvm@external-assurance.service")
        run_browser(controller, "configuration-register")
        external_id = receipts()["management.externalId"]
        reference = delivery_api("POST", "/api/v1/systems", {
            "hostname": "Golden workstation / ISO", "management_mode": "reference", "environment": "delivery-proof",
            "flake_name": FACTS["flake"]["name"], "system_configuration_name": config, "deployment_policy": "manual",
        }, expected=201)
        receipt("management.referenceId", reference["id"])
        expect(external_id != reference["id"], "shared configuration collapsed registration identity")
        for system_id in (external_id, reference["id"]):
            expect_equal(sql_json(controller, "SELECT public_key FROM systems WHERE id=" + literal(system_id) + "::uuid"),
                [{"public_key": None}], "keyless registration persists no fake key")

    with case("configuration-signed-parent-unit-observation-is-not-attestation"):
        def observed():
            rows = sql_json(controller, "SELECT guest_id::text, parent_host_id::text, guest_identity, unit, "
                "unit_active_state, runner_store_path, observed_at, received_at FROM guest_observations WHERE guest_id=" +
                literal(external_id) + "::uuid ORDER BY id DESC LIMIT 1")
            return rows[0] if rows else None
        observation = wait_for(observed, "parent pull then signed collected unit observation", timeout=150)
        actual_state = targetA.succeed("systemctl show --property=ActiveState --value microvm@external-assurance.service").strip()
        expect_equal(actual_state, "active", "controlled real host unit")
        for field, expected in (("guest_id", external_id), ("parent_host_id", receipts()["delivery.targetId"]),
            ("guest_identity", "external-assurance"), ("unit", "microvm@external-assurance.service"),
            ("unit_active_state", actual_state), ("runner_store_path", None)):
            expect_equal(observation[field], expected, "signed parent observation " + field)
        latest = delivery_api("GET", "/api/v1/systems/" + external_id + "/guest-observations/latest")
        expect_equal(latest["unit_active_state"], "active", "latest read preserves real unit observation")
        expect_equal(latest["runner_store_path"], None, "unit state never invents runner or guest closure")
        expect_equal(delivery_api("GET", "/api/v1/systems/" + reference["id"] + "/guest-observations/latest"),
            None, "reference has no runtime observation")
        receipt("management.parentObservation", observation)

    with case("configuration-agent-disabled-native-build-shared-derivation"):
        remediation_before = _assurance_remediation_snapshot()
        commit, nonce = author_revision(FACTS["cve"]["vulnerableVersion"], "Author agent-disabled configuration assurance")
        def evaluated():
            rows = sql_json(controller, "SELECT d.id, d.derivation_path, d.expected_store_path, d.cf_agent_enabled "
                "FROM derivations d JOIN commits c ON c.id=d.commit_id WHERE c.flake_id=" +
                str(receipts()["delivery.flakeId"]) + " AND c.git_commit_hash=" + literal(commit) +
                " AND d.derivation_name=" + literal(config))
            return rows[0] if len(rows) == 1 and rows[0]["expected_store_path"] else None
        artifact = wait_for(evaluated, "agent-disabled native configuration", timeout=300)
        expect_equal(artifact["cf_agent_enabled"], False, "native fact must not be forged for admission")
        def built():
            rows = sql_json(controller, "SELECT id::text, status, builder_id::text, builder_session_id::text "
                "FROM build_jobs WHERE derivation_id=" + str(artifact["id"]))
            return rows if rows and all(j["status"] in ("success", "failed", "cancelled") for j in rows) else None
        jobs = wait_for(built, "shared nonmanaged real build", timeout=600)
        expect_equal(len(jobs), 1, "two registrations reuse one derivation build")
        expect_equal(jobs[0]["status"], "success", "agent-disabled build actually completes")
        output = artifact["expected_store_path"]
        expect_equal(controller.succeed("nix-store --query --outputs " + shlex.quote(artifact["derivation_path"])).splitlines(),
            [output], "independent Nix derivation output")
        for node in (builderA, services):
            node.succeed("nix-store --check-validity " + shlex.quote(output))
            expect_equal(node.succeed("cat " + shlex.quote(output + "/cf-it-source-nonce")).strip(), nonce, "actual authored built bytes")
        # NixOS renders a disabled unit as a /dev/null mask; absent or masked
        # both mean the built configuration cannot start the CF agent.
        unit = shlex.quote(output + "/etc/systemd/system/crystal-forge-agent.service")
        builderA.succeed("test ! -e " + unit + " || test \"$(readlink -f " + unit + ")\" = /dev/null")
        closure = builderA.succeed("nix-store --query --requisites " + shlex.quote(output)).splitlines()
        package = [p for p in closure if p.endswith("-" + FACTS["cve"]["package"] + "-" + FACTS["cve"]["vulnerableVersion"])]
        expect_equal(len(package), 1, "actual closure includes the independently authored vulnerable package")
        package_drv = builderA.succeed("nix-store --query --deriver " + shlex.quote(package[0])).strip()
        receipt("management.artifact", dict(artifact, commit=commit, output=output, job=jobs[0]["id"]))

    with case("configuration-real-scanner-exact-sealed-evidence-without-current-authority"):
        def scanned():
            rows = sql_json(controller, "SELECT id::text, status, evidence_schema_version, result_digest_sha256, "
                "target_drv_path, target_outputs, completed_build_job_id::text, lease_builder_id::text, "
                "lease_builder_session_id::text, scanner_name, source_trigger FROM cve_scans WHERE completed_build_job_id=" +
                literal(jobs[0]["id"]) + "::uuid")
            return rows if rows and all(s["status"] in ("completed", "failed") for s in rows) else None
        scans = wait_for(scanned, "real nonmanaged post-build scan", timeout=600)
        expect_equal(len(scans), 1, "one producing-build scan obligation")
        scan = scans[0]
        for field, expected in (("status", "completed"), ("evidence_schema_version", 1), ("scanner_name", "vulnix"),
            ("source_trigger", "post_build"), ("target_drv_path", artifact["derivation_path"]),
            ("target_outputs", [{"name": "out", "store_path": output}]),
            ("lease_builder_id", jobs[0]["builder_id"]), ("lease_builder_session_id", jobs[0]["builder_session_id"])):
            expect_equal(scan[field], expected, "real scan " + field)
        expect_equal(len(scan["result_digest_sha256"] or ""), 64, "server-sealed exact evidence")
        observations = sql_json(controller, "SELECT canonical_cve_id, canonical_package_name, observed_package_version, "
            "observed_derivation_path, is_affected FROM cve_scan_vulnerability_observations WHERE scan_id=" + literal(scan["id"]) + "::uuid")
        expect_equal(observations, [{"canonical_cve_id": FACTS["cve"]["cveId"], "canonical_package_name": FACTS["cve"]["package"],
            "observed_package_version": FACTS["cve"]["vulnerableVersion"], "observed_derivation_path": package_drv,
            "is_affected": True}], "entire independently expected real observation set")
        for system_id, mode in ((external_id, "external"), (reference["id"], "reference")):
            path = "/api/v1/systems/" + system_id
            evidence = delivery_api("GET", path + "/configuration-assurance")
            expect_equal(evidence["management_mode"], mode, "evidence registration mode")
            expect_equal(evidence["selected_revision"]["sha"], commit, "exact selected Git source")
            expect_equal(evidence["selected_artifact"]["derivation_id"], artifact["id"], "shared artifact identity")
            expect_equal(evidence["selected_scan"]["scan_id"], scan["id"], "shared exact sealed scan")
            inventory = delivery_api("GET", path + "/cve-inventory-page?target=exact_derivation&target_id=" + str(artifact["id"]))
            expect_equal(inventory["read_only"], True, "configuration inventory has no mutation authority")
            expect_equal(inventory["source"]["scan_id"], scan["id"], "exact inventory source")
            expect_equal([(v["cve_id"], v["canonical_package_name"]) for v in inventory["vulnerabilities"]],
                [(FACTS["cve"]["cveId"], FACTS["cve"]["package"])], "artifact inventory retains real findings")
            expect_equal(sql_json(controller, "SELECT scan_id FROM view_current_cve_authority WHERE system_id=" + literal(system_id) + "::uuid"),
                [], "configuration evidence cannot become runtime Current")
            expect_equal(sql_json(controller, "SELECT ss.id FROM system_states ss JOIN systems s ON s.hostname=ss.hostname WHERE s.id=" + literal(system_id) + "::uuid"),
                [], "no fabricated running store path or generation")
        receipt("management.scanId", scan["id"])
        expect_equal(_assurance_remediation_snapshot(), remediation_before,
            "real configuration scan creates no runtime remediation, verification or closure")

    with case("configuration-artifact-cannot-authorize-poam-creation"):
        before = _assurance_remediation_snapshot()
        for system_id in (external_id, reference["id"]):
            # Deliberately attempt to promote a real artifact occurrence into
            # runtime context. The exact scan exists; only authority is absent.
            status, error = http(controller, "POST", "/api/v1/poams/cves", {
                "observation": {"system_id": system_id, "scan_id": scan["id"], "occurrence_derivation_path": package_drv,
                    "canonical_cve_id": FACTS["cve"]["cveId"], "canonical_package_name": FACTS["cve"]["package"]},
                "title": "Forbidden artifact-only remediation", "plan": "Configuration evidence is not running evidence.",
                "risk": "high", "target_date": "2027-02-01",
            }, session="delivery-admin", csrf=True)
            expect_equal(status, 409, "configuration occurrence cannot authorize a runtime plan")
            expect_equal(error["error"], "stale_cve_observation", "denial is evidence authority, not invalid input")
        expect_equal(_assurance_remediation_snapshot(), before, "artifact bypass changes no remediation rows")

    def completed_configuration_scan(scan_id, trigger):
        def terminal():
            rows = sql_json(controller, "SELECT id::text, derivation_id, status, evidence_schema_version, result_digest_sha256, "
                "target_drv_path, target_outputs, completed_build_job_id::text, lease_builder_id::text, "
                "lease_builder_session_id::text, source_trigger FROM cve_scans WHERE id=" + literal(scan_id) + "::uuid")
            return rows[0] if len(rows) == 1 and rows[0]["status"] in ("completed", "failed") else None
        result = wait_for(terminal, "real remote " + trigger + " configuration scan completion", timeout=600)
        for field, expected in (("status", "completed"), ("derivation_id", artifact["id"]), ("evidence_schema_version", 1),
            ("target_drv_path", artifact["derivation_path"]), ("target_outputs", [{"name": "out", "store_path": output}]),
            ("completed_build_job_id", jobs[0]["id"]), ("lease_builder_id", jobs[0]["builder_id"]),
            ("lease_builder_session_id", jobs[0]["builder_session_id"]), ("source_trigger", trigger)):
            expect_equal(result[field], expected, "completed " + trigger + " configuration scan " + field)
        expect_equal(len(result["result_digest_sha256"] or ""), 64, trigger + " result is actually sealed")
        actual = sql_json(controller, "SELECT canonical_cve_id, canonical_package_name, observed_package_version, "
            "observed_derivation_path, is_affected FROM cve_scan_vulnerability_observations WHERE scan_id=" + literal(scan_id) + "::uuid")
        expect_equal(actual, observations, trigger + " executes against the real authored vulnerable closure and normal feed")
        return result

    with case("configuration-external-manual-remote-scan-executes-and-seals"):
        manual = delivery_api("POST", "/api/v1/systems/" + external_id + "/cve-scan?expected_derivation_id=" + str(artifact["id"]), {}, expected=202)
        expect_equal(manual["derivation_id"], artifact["id"], "external manual request retains exact intent")
        completed_configuration_scan(manual["scan_id"], "manual")

    with case("configuration-browser-tabs-edit-filter-and-rescan-request"):
        run_browser(controller, "configuration-evidence")
        # Acceptance alone is insufficient in this remote-only topology. The
        # existing producer must execute and seal the real scanner result.
        requested = receipts()["management.requestedScan"]
        rows = sql_json(controller, "SELECT id::text, derivation_id, status FROM cve_scans WHERE id=" + literal(requested) + "::uuid")
        expect_equal(len(rows), 1, "browser scan intent persisted")
        expect_equal(rows[0]["derivation_id"], artifact["id"], "browser scan exact target")
        completed_configuration_scan(requested, "manual")
        receipt("management.manualScanLifecycle", rows[0])
        converted = delivery_api("GET", "/api/v1/systems/" + external_id + "/configuration-assurance")
        expect_equal(converted["management_mode"], "reference", "browser mode conversion survives fresh API read")
        expect_equal(delivery_api("GET", "/api/v1/systems/" + external_id)["parent_host_id"], None,
            "reference conversion clears the parent association")
        expect(any(s["scan_id"] == scan["id"] for s in converted["history"]["scans"]), "conversion erased real scan history")
        expect_equal(delivery_api("GET", "/api/v1/systems/" + external_id + "/guest-observations/latest"),
            None, "conversion suppresses historical parent report as current observation")
        expect(bool(sql_json(controller, "SELECT id FROM guest_observations WHERE guest_id=" + literal(external_id) + "::uuid")),
            "conversion erased historical parent observations")

    with case("configuration-periodic-remote-scan-executes-without-local-vulnix"):
        policy = sql_json(controller, "SELECT deployed_interval,recent_interval,archived_enabled FROM scan_schedule_policy WHERE id=1")[0]
        prior = [row["id"] for row in sql_json(controller, "SELECT id::text FROM cve_scans WHERE derivation_id=" + str(artifact["id"]))]
        try:
            # Change only the existing scheduling policy. Do not manufacture an
            # old completed scan or a successful build. The one-second cadence
            # makes the actual sealed manual result become due in this VM run.
            sql_json(controller, "UPDATE scan_schedule_policy SET deployed_interval='1 second',recent_interval='never',archived_enabled=false WHERE id=1 RETURNING id")
            def periodic_created():
                rows = sql_json(controller, "SELECT id::text FROM cve_scans WHERE derivation_id=" + str(artifact["id"]) +
                    " AND source_trigger='periodic' ORDER BY created_at,id")
                return next((row["id"] for row in rows if row["id"] not in prior), None)
            periodic = wait_for(periodic_created, "periodic admission with controller-local vulnix inaccessible", timeout=150)
            receipt("management.periodicScan", completed_configuration_scan(periodic, "periodic"))
        finally:
            sql_json(controller, "UPDATE scan_schedule_policy SET deployed_interval=" + literal(policy["deployed_interval"]) +
                ",recent_interval=" + literal(policy["recent_interval"]) + ",archived_enabled=" +
                ("true" if policy["archived_enabled"] else "false") + " WHERE id=1 RETURNING id")
        for system_id in (external_id, reference["id"]):
            expect_equal(sql_json(controller, "SELECT scan_id FROM view_current_cve_authority WHERE system_id=" + literal(system_id) + "::uuid"),
                [], "periodic configuration evidence grants no runtime authority")
        expect_equal(sql_json(controller, "SELECT count(*)::int AS n FROM build_jobs WHERE derivation_id=" + str(artifact["id"])),
            [{"n": 1}], "manual and periodic scans reuse the actual producing job without rebuilding")

    configuration_exact_artifact_reuse(config, (external_id, reference["id"]), artifact, jobs[0], commit)

    with case("configuration-renamed-head-reports-missing-and-preserves-exact-history"):
        services.succeed("sed -i 's/assuranceOnly = target.extendModules/renamedAssurance = target.extendModules/' "
            "/tmp/delivery-source/flake.nix && git -C /tmp/delivery-source add flake.nix && "
            "git -C /tmp/delivery-source commit -m 'Rename configuration without rewriting retained evidence' && "
            "git -C /tmp/delivery-source push --receive-pack='runuser -u git -- git-receive-pack' /srv/git/delivery.git main")
        renamed_commit = services.succeed("git -C /tmp/delivery-source rev-parse HEAD").strip()
        delivery_api("POST", "/api/v1/flakes/" + str(receipts()["delivery.flakeId"]) + "/sync", {})
        for system_id in (external_id, reference["id"]):
            def removed():
                value = delivery_api("GET", "/api/v1/systems/" + system_id + "/configuration-assurance")
                return value if value["selected_revision"] and value["selected_revision"]["sha"] == renamed_commit \
                    and value["evaluation"]["availability"] == "removed" else None
            history = wait_for(removed, "real native discovery of renamed configuration", timeout=300)
            expect_equal(history["selected_artifact"], None, "missing head does not substitute historical artifact")
            expect_equal(history["selected_scan"], None, "missing head does not substitute historical scan")
            expect(any(s["scan_id"] == scan["id"] and s["revision"]["sha"] == commit for s in history["history"]["scans"]),
                "removed configuration lost exact historical scan")
        receipt("management.renamedRevision", renamed_commit)
        run_browser(controller, "configuration-removed")
        expect_equal(sql_json(controller, "SELECT count(*)::int AS n FROM build_jobs WHERE derivation_id=" + str(artifact["id"])),
            [{"n": 1}], "subsequent browser requests and conversions do not duplicate the shared build")


configuration_assurance()
