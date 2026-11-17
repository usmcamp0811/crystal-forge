# TASK-529.2 review P1: append after authorization.py, before Admin forms.
# Requires its authz_* helpers/session, bootstrap helpers, and same-run Operator
# receipt. Owns only public Operator fixture prerequisites in this scenario VM.
# Restores its public password, enabled flag, local Operator/empty environments
# and removes injected identity/environment before later forms/preferences.
# SQL removal below models withdrawing an IdP contribution, not a signed callback;
# real callback/reconciliation semantics remain owned by Rust integration tests.


def admin_source_grants_boundary():
    config = FACTS["adminSourceGrants"]
    operator = FACTS["operator"]
    user_id = receipts()["authorization.operator.userId"]
    identity_id = config["identityId"]
    environment_id = config["environmentId"]
    path = "/api/v1/admin/users/" + user_id

    def fixture_sql(query):
        controller.succeed("runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 "
                           "-d crystal_forge -c " + shlex.quote(query))

    def sources():
        # Full rows protect actor attribution, timestamps and identity ownership,
        # not merely the effective role/name shown by the API.
        return {
            "roles": sql_json(controller,
                "SELECT * FROM user_role_assignments WHERE user_id = '%s' ORDER BY role" % user_id),
            "environments": sql_json(controller,
                "SELECT * FROM user_environment_memberships WHERE user_id = '%s' ORDER BY environment_id" % user_id),
            "identities": sql_json(controller,
                "SELECT * FROM external_identities WHERE user_id = '%s' ORDER BY id" % user_id),
        }

    def account():
        return sql_json(controller,
            "SELECT is_active, md5(password_hash) AS password_digest FROM users WHERE id = '%s'" % user_id)[0]

    def audits():
        return sql_json(controller,
            "SELECT id::text, action, metadata FROM admin_audit_events ORDER BY id")

    def summary():
        status, users = http(controller, "GET", "/api/v1/admin/users", session=authz_admin_jar)
        expect_equal(status, 200, "independent Admin list read")
        selected = [u for u in users if u["id"] == user_id]
        expect_equal(len(selected), 1, "exact public Operator fixture identity")
        return selected[0]

    empty_sources = {key: [] for key in (
        "local_roles", "local_environments", "oidc_roles", "oidc_environments", "legacy_roles", "legacy_environments")}
    expected_sources = dict(empty_sources, oidc_roles=["Operator"], oidc_environments=[config["environment"]])
    authz_verify_actor("operator")
    expect_equal(sources()["identities"], [], "public Operator begins without linked identities")
    try:
        with case("admin-source-unmarked-grant-roundtrip-rejected-without-writes"):
            fixture_sql("""
                BEGIN;
                INSERT INTO external_identities (id, user_id, provider_key, subject)
                    VALUES ('%s', '%s', 'oidc', 'access-pure-idp-fixture');
                INSERT INTO environments (id, name, is_production)
                    VALUES ('%s', '%s', true);
                UPDATE user_role_assignments
                    SET local_grant = false, oidc_identity_id = '%s', legacy_ownership_unresolved = false
                    WHERE user_id = '%s' AND role = 'operator';
                INSERT INTO user_environment_memberships
                    (user_id, environment_id, local_grant, oidc_identity_id, legacy_ownership_unresolved)
                    VALUES ('%s', '%s', false, '%s', false);
                COMMIT;
                """ % (identity_id, user_id, environment_id, config["environment"], identity_id,
                       user_id, user_id, environment_id, identity_id))
            before_sources = sources()
            expect_equal(len(before_sources["roles"]), 1, "one prerequisite role")
            expect_equal(len(before_sources["environments"]), 1, "one prerequisite membership")
            for rows in (before_sources["roles"], before_sources["environments"]):
                expect_equal(rows[0]["local_grant"], False, "no local prerequisite contribution")
                expect_equal(rows[0]["legacy_ownership_unresolved"], False, "no unresolved legacy contribution")
                expect_equal(rows[0]["oidc_identity_id"], identity_id, "exact IdP source identity")
            expect_equal(sql_json(controller, "SELECT is_production FROM environments WHERE id = '%s'" % environment_id),
                         [{"is_production": True}], "membership is in an authored production environment")
            expect_equal(summary()["grant_sources"], expected_sources, "six-vector pure-IdP API prerequisite")
            before = authz_state()
            before_audits = audits()
            # Old clients round-tripped effective fields without expressing local
            # intent. Reject every grant category, including clear, before writes.
            for payload in ({"role": "Operator", "environments": [config["environment"]]},
                            {"environments": []}, {"clear_local_role": True},
                            {"role": "Viewer", "edit_local_grants": False},
                            {"role": "Viewer", "clear_local_role": True, "edit_local_grants": True}):
                status, _ = http(controller, "PATCH", path, payload, session=authz_admin_jar, csrf=True)
                expect_equal(status, 400, "missing intent or contradictory role request is rejected")
                expect_equal(authz_state(), before, "rejected request changes no protected business state")
                expect_equal(sources(), before_sources, "rejected request changes no source ownership")
                expect_equal(audits(), before_audits, "rejected request writes no audit")
            receipt("adminSourceGrants.unmarkedRejected", True)

        with case("admin-source-unrelated-modal-save-preserves-idp-only-grants"):
            before_sources = sources()
            before_account = account()
            before_audits = audits()
            run_browser(controller, "admin-source-unrelated-save")
            expect_equal(receipts()["adminSourceGrants.browserComplete"], True, "real modal/reload completed")
            response = receipts()["adminSourceGrants.response"]
            expect_equal(response["status"], 200, "actual browser PATCH succeeded")
            expect_equal(response["payload"], {"password": config["resetPassword"], "enabled": False},
                         "actual PATCH contains only intentional public password reset and account flag")
            expect_equal(sources(), before_sources, "all role/membership/source identity rows unchanged exactly")
            expect_equal(account()["is_active"], False, "intentional disabled flag persisted")
            expect(account()["password_digest"] != before_account["password_digest"], "intentional password reset persisted")
            expect_equal(summary()["grant_sources"], expected_sources, "subsequent HTTP read remains pure IdP")
            old_ids = {event["id"] for event in before_audits}
            new_audits = [event for event in audits() if event["id"] not in old_ids]
            expect_equal(sorted(event["action"] for event in new_audits),
                         ["user_disabled", "user_updated", "user_updated"], "only account/password audit actions")
            for event in new_audits:
                metadata = json.dumps(event["metadata"])
                for forbidden in ("classifies_legacy", "local_role", "local_environments", "clear_local_role"):
                    expect(forbidden not in metadata, "unrelated audit contains no grant classification")
            receipt("adminSourceGrants.sqlUnchanged", True)
            receipt("adminSourceGrants.auditActions", sorted(event["action"] for event in new_audits))

        with case("admin-source-idp-withdrawal-has-no-accidental-local-fallback"):
            # Remove only the injected identity contribution using source rules.
            # A buggy modal that promoted it to local leaves a row and fails here.
            fixture_sql("""
                BEGIN;
                DELETE FROM user_role_assignments WHERE user_id = '%s' AND oidc_identity_id = '%s'
                    AND NOT local_grant AND NOT legacy_ownership_unresolved;
                UPDATE user_role_assignments SET oidc_identity_id = NULL
                    WHERE user_id = '%s' AND oidc_identity_id = '%s';
                DELETE FROM user_environment_memberships WHERE user_id = '%s' AND oidc_identity_id = '%s'
                    AND NOT local_grant AND NOT legacy_ownership_unresolved;
                UPDATE user_environment_memberships SET oidc_identity_id = NULL
                    WHERE user_id = '%s' AND oidc_identity_id = '%s';
                COMMIT;
                """ % (user_id, identity_id, user_id, identity_id, user_id, identity_id, user_id, identity_id))
            expect_equal(sources()["roles"], [], "no accidental local role after IdP withdrawal")
            expect_equal(sources()["environments"], [], "no accidental local membership after IdP withdrawal")
            saved = summary()
            expect_equal(saved["role"], None, "HTTP read has no fallback role")
            expect_equal(saved["environments"], [], "HTTP read has no fallback membership")
            expect_equal(saved["grant_sources"], empty_sources, "all six grant vectors empty after fixture withdrawal")
            status, _ = http(controller, "PATCH", path, {"enabled": True}, session=authz_admin_jar, csrf=True)
            expect_equal(status, 200, "unrelated re-enable needs no grant intent marker")
            authz_sign_in("admin-source-reset-password", operator["email"], config["resetPassword"])
            who = authz_whoami("admin-source-reset-password")
            expect_equal(who["is_authenticated"], True, "reset-password session is authenticated")
            expect_equal(who["user"]["id"], user_id, "reset-password session has exact fixture identity")
            expect_equal(who["roles"], [],
                         "real reset password works without restoring any role")
            receipt("adminSourceGrants.noLocalFallback", True)
    finally:
        status, _ = http(controller, "PATCH", path,
                         {"role": "Operator", "environments": [], "enabled": True,
                          "password": operator["password"], "edit_local_grants": True},
                         session=authz_admin_jar, csrf=True)
        expect_equal(status, 200, "restore public Operator password, role, environments and enabled flag")
        fixture_sql("""
            BEGIN;
            DELETE FROM user_environment_memberships WHERE user_id = '%s' AND oidc_identity_id = '%s';
            UPDATE user_role_assignments SET oidc_identity_id = NULL
                WHERE user_id = '%s' AND oidc_identity_id = '%s';
            DELETE FROM external_identities WHERE id = '%s';
            DELETE FROM environments WHERE id = '%s';
            COMMIT;
            """ % (user_id, identity_id, user_id, identity_id, identity_id, environment_id))
    restored = sources()
    expect_equal(restored["identities"], [], "injected source identity removed")
    expect_equal(restored["environments"], [], "original empty explicit membership restored")
    expect_equal(summary()["grant_sources"], dict(empty_sources, local_roles=["Operator"]),
                 "original local-only Operator source restored for later cases")
    authz_sign_in(authz_actors["operator"][1], operator["email"], operator["password"])
    authz_verify_actor("operator")
    receipt("adminSourceGrants.fixtureRestored", True)


admin_source_grants_boundary()
