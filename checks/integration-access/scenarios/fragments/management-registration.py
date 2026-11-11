# Appended after shell/preferences in local-bootstrap. Uses the existing Admin,
# Operator and Viewer cookie jars; no evaluator, successful evidence or keys are
# seeded. Assurance owns declared-output browser registration and real execution.
# Authority: TASK-508 AC 1–4, 7, 13; approved design 18b47eed.
def management_registration():
    def api(method, path, body=None, session="authz-admin", expected=200):
        status, value = http(controller, method, path, body, session=session, csrf=method != "GET")
        expect_equal(status, expected, method + " " + path)
        return value

    def rows():
        return sql_json(controller, "SELECT id::text, hostname, management_mode, public_key, parent_host_id, "
            "environment_id, flake_id, system_configuration_name, deployment_policy, desired_target "
            "FROM systems ORDER BY id")

    with case("management-registration-keyless-shared-configuration-and-filter"):
        api("POST", "/api/v1/environments", {"name": "configuration-assurance", "default_policy": "manual",
            "color_hex": "#82699b", "is_active": True}, expected=201)
        api("POST", "/api/v1/flakes", {"name": "configuration-assurance", "repo_url": "http://localhost:9/unavailable.git",
            "branch": "main", "build_scope": "cf_systems_only"}, expected=201)
        # This Access-only origin is deliberately unavailable. Registration is
        # not claimed as evaluation; Assurance supplies a real declared output.
        ids = {}
        for mode, name in (("external", "external-configuration"), ("reference", "Golden workstation / ISO")):
            body = {"hostname": name, "management_mode": mode, "environment": "configuration-assurance",
                "flake_name": "configuration-assurance", "system_configuration_name": "shared", "deployment_policy": "manual"}
            created = api("POST", "/api/v1/systems", body, expected=201)
            ids[mode] = created["id"]
            expect_equal(created["management_mode"], mode, "registration mode")
            expect_equal(created["public_key_fingerprint"], None, "no generated enrollment identity")
            selected = api("GET", "/api/v1/systems?management_mode=" + mode)
            expect_equal([s["id"] for s in selected["items"]], [created["id"]], "mode filter exact membership")
        expect(ids["external"] != ids["reference"], "shared configuration must not merge resource identity")
        expect_equal([r["public_key"] for r in rows()], [None, None], "no fake persisted keys")
        receipt("management.registration.ids", ids)

    with case("management-registration-invalid-and-unauthorized-writes-preserve-resources"):
        before = rows()
        base = {"hostname": "denied-registration", "management_mode": "reference", "environment": "configuration-assurance",
            "flake_name": "configuration-assurance", "system_configuration_name": "shared", "deployment_policy": "manual"}
        api("POST", "/api/v1/systems", dict(base, management_mode="managed"), expected=400)
        api("POST", "/api/v1/systems", dict(base, management_mode="invented"), expected=422)
        api("POST", "/api/v1/systems", base, session="authz-viewer", expected=403)
        api("POST", "/api/v1/systems", base, session="authz-operator", expected=404)
        for actor in ("authz-operator", "authz-viewer"):
            api("GET", "/api/v1/systems/" + ids["reference"], session=actor, expected=404)
        expect_equal(rows(), before, "denials do not create, enroll or move resources")

    with case("management-edit-conversion-requires-credentials-and-preserves-history"):
        path = "/api/v1/systems/" + ids["external"]
        edit = {"hostname": "external-configuration", "management_mode": "reference", "environment": "configuration-assurance",
            "flake_name": "configuration-assurance", "system_configuration_name": "shared", "deployment_policy": "manual"}
        api("PATCH", path, edit)
        expect_equal(api("GET", path)["management_mode"], "reference", "conversion persists through a fresh read")
        before = rows()
        api("PATCH", path, dict(edit, management_mode="managed"), expected=400)
        api("PATCH", path, dict(edit, management_mode="external"), session="authz-viewer", expected=403)
        expect_equal(rows(), before, "failed conversion cannot grant deployment authority")
        expect_equal(sql_json(controller, "SELECT id FROM pending_system_deployments"), [], "conversion queues no deployment")
        audit = sql_json(controller, "SELECT action FROM admin_audit_events WHERE action IN ('system_created','system_updated') ORDER BY created_at")
        expect_equal(sorted(r["action"] for r in audit), ["system_created", "system_created", "system_updated"], "registration and conversion audited")


management_registration()
