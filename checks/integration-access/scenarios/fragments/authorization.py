# Role authorization cases; concatenate after bootstrap and session checks.
# Requires bootstrap's controller, admin, FACTS, case/HTTP/SQL/browser/assertion
# helpers, receipt/receipts helpers, and persisted admin.userId receipt.
# Create Operator/Viewer/target actors only after fresh-install assertions.
# Exports authz_state() for admin-forms.py cancel checks. Controls restore the
# target, retry policy and classification config; design identity u3 stays absent.
# Cookie jars belong to this run; only generated identities leave via receipts.
#
# Requirement (approved design, role catalogue and Server Management page):
# Admin may "Manage users & OIDC" and "Edit server config"; the page is
# "Admin-only". Operator and Viewer hold no such power. The accepted API
# response for a role without the power is HTTP 403 with error "forbidden"
# (docs/knowledge/security/api-authentication-and-authorization.md). A denied
# request MUST NOT change any protected state.
#
# This does NOT define Operator as generally read-only and does not forbid a
# Viewer from reading pages that the requirements permit. Cross-environment
# scope and closed registration are separate decisions and are not covered.
#
# Method: (1) create the actors through the real Admin API and sign each in;
# (2) prove, with Admin, that every probe payload is valid and effective, then
# restore the state; (3) send each probe as Operator and as Viewer with valid
# session and CSRF credentials and compare the protected state before/after.

authz = FACTS["authorization"]
authz_admin_jar = "authz-admin"
# role -> (authored fact, cookie jar, receipt holding the persisted user id)
authz_actors = {
    "operator": (FACTS["operator"], "authz-operator", "authorization.operator.userId"),
    "viewer": (FACTS["viewer"], "authz-viewer", "authorization.viewer.userId"),
}
authz_target = FACTS["target"]

# Administrative audit actions written by the user-management mutations. A
# denied request is rejected before any of them is recorded.
AUTHZ_USER_AUDIT_ACTIONS = (
    "user_created", "user_updated", "user_deleted", "user_enabled",
    "user_disabled", "user_role_assigned", "user_environment_membership_updated",
)


def authz_state():
    """Returns every protected record the selected operations could change.

    Business fields and identities only. Session rows, last_seen_at and
    similar authentication activity are deliberately excluded.
    """
    actions = ", ".join("'%s'" % a for a in AUTHZ_USER_AUDIT_ACTIONS)
    return sql_json(
        controller,
        "SELECT "
        "(SELECT coalesce(json_agg(u ORDER BY u.id), '[]'::json) FROM ("
        "  SELECT id::text AS id, username, email, is_active,"
        "         md5(coalesce(password_hash, '')) AS password_digest FROM users) u) AS users, "
        "(SELECT coalesce(json_agg(r ORDER BY r.user_id, r.role), '[]'::json) FROM ("
        "  SELECT user_id::text AS user_id, role::text AS role"
        "  FROM user_role_assignments) r) AS roles, "
        "(SELECT coalesce(json_agg(m ORDER BY m.user_id, m.environment_id), '[]'::json) FROM ("
        "  SELECT user_id::text AS user_id, environment_id::text AS environment_id"
        "  FROM user_environment_memberships) m) AS environments, "
        "(SELECT coalesce(json_agg(a ORDER BY a.action), '[]'::json) FROM ("
        "  SELECT action, count(*)::int AS n FROM admin_audit_events"
        "  WHERE action IN (%s) GROUP BY action) a) AS user_audit, "
        "(SELECT row_to_json(p) FROM (SELECT max_build_retries, max_evaluation_retries,"
        "  backoff_seconds, transient_only FROM automatic_retry_policy) p) AS retry_policy, "
        "(SELECT row_to_json(c) FROM (SELECT enabled, level, custom_text"
        "  FROM classification_banner_config) c) AS classification" % actions,
    )[0]


def authz_user_roles(user_id):
    rows = sql_json(
        controller,
        "SELECT role::text AS role FROM user_role_assignments "
        "WHERE user_id = '%s'::uuid ORDER BY role" % user_id,
    )
    return [r["role"] for r in rows]


def authz_user_row(user_id):
    rows = sql_json(
        controller,
        "SELECT username, email, is_active FROM users WHERE id = '%s'::uuid" % user_id,
    )
    return rows[0] if rows else None


def authz_sign_in(jar, username_or_email, password):
    status, _ = http(controller, "POST", "/api/auth/local/login",
                     {"username": username_or_email, "password": password}, session=jar)
    expect_equal(status, 200, "real sign-in for jar %s" % jar)


def authz_whoami(jar):
    status, body = http(controller, "GET", "/api/auth/whoami", session=jar)
    expect_equal(status, 200, "whoami for jar %s" % jar)
    return body


def authz_create_payload(email):
    return dict(authz["createProbe"], email=email)


def authz_assert_forbidden(status, body, what):
    expect_equal(status, 403, what + " status")
    expect(isinstance(body, dict) and body.get("error") == "forbidden",
           what + ": expected the documented 'forbidden' error, got %r" % (body,))


def authz_verify_actor(actor):
    """Proves the jar holds a live session for exactly this role before a probe.

    A 403 from an unauthenticated session, a missing user, or a wrong role
    would not prove role authorization.
    """
    fact, jar, receipt_key = authz_actors[actor]
    who = authz_whoami(jar)
    expect_equal(who["is_authenticated"], True, actor + " session is authenticated")
    expect_equal(who["user"]["id"], receipts()[receipt_key], actor + " identity")
    expect_equal(who["roles"], [fact["roleWire"]], actor + " roles")
    expect_equal(authz_user_roles(receipts()[receipt_key]), [fact["role"]],
                 actor + " persisted role assignment")


def authz_probe(actor, operation, method, path, body):
    """Sends one admin-only request as `actor` and proves it was rejected.

    The actor's identity and role are verified first, so the rejection cannot
    come from a missing session. The request carries the real session cookies
    and a matching CSRF header. The protected state is read before and after.
    Each result is recorded as a receipt so that the evidence names the status,
    error code and state outcome of every probe.
    """
    fact, jar, _ = authz_actors[actor]
    authz_verify_actor(actor)
    before = authz_state()
    status, response = http(controller, method, path, body, session=jar, csrf=True)
    authz_assert_forbidden(status, response, "%s %s" % (actor, operation))
    expect_equal(authz_state(), before,
                 "%s %s left protected state unchanged" % (actor, operation))
    receipt("authorization.denied.%s.%s" % (actor, operation),
            {"method": method, "status": status, "error": response["error"],
             "stateUnchanged": True})
    return before


with case("authorization-actors-created-through-admin-api-and-signed-in"):
    authz_sign_in(authz_admin_jar, admin["username"], admin["password"])
    who = authz_whoami(authz_admin_jar)
    expect_equal(who["user"]["id"], receipts()["admin.userId"], "admin identity")
    expect_equal(who["roles"], [FACTS["adminRoleWire"]], "admin role")

    before_users = authz_state()["users"]
    for key, fact in (("operator", FACTS["operator"]), ("viewer", FACTS["viewer"]),
                      ("target", authz_target)):
        status, body = http(
            controller, "POST", "/api/v1/admin/users",
            {"email": fact["email"], "display_name": fact["name"],
             "password": fact["password"], "role": fact["roleWire"], "environments": []},
            session=authz_admin_jar, csrf=True)
        expect_equal(status, 201, "Admin creates the %s user" % key)
        expect_equal(body["role"], fact["roleWire"], "%s role in the create response" % key)
        user_id = receipt("authorization.%s.userId" % key, body["id"])
        # Authoritative persistence, compared with the authored design facts.
        expect_equal(authz_user_row(user_id),
                     {"username": fact["username"], "email": fact["email"], "is_active": True},
                     "persisted %s account" % key)
        expect_equal(authz_user_roles(user_id), [fact["role"]], "persisted %s role" % key)
    after_users = authz_state()["users"]
    expect_equal(len(after_users) - len(before_users), 3, "exactly three accounts were added")

    for actor, (fact, jar, _) in authz_actors.items():
        authz_sign_in(jar, fact["email"], fact["password"])
        authz_verify_actor(actor)

with case("authorization-admin-controls-accept-every-probe-payload"):
    # Each probe payload is first sent as Admin. A 2xx here proves the payload
    # is valid and the target exists, so the later 403 is caused by the role.
    # Every control restores the state it changed.
    target_id = receipts()["authorization.target.userId"]

    # Server configuration: automatic retry policy.
    status, original_retry = http(controller, "GET", "/api/v1/admin/automatic-retry-policy",
                                  session=authz_admin_jar)
    expect_equal(status, 200, "read the installed retry policy")
    expect(original_retry != authz["retryPolicy"], "probe retry policy must differ from the installed one")
    status, saved = http(controller, "PUT", "/api/v1/admin/automatic-retry-policy",
                         authz["retryPolicy"], session=authz_admin_jar, csrf=True)
    expect_equal(status, 200, "Admin saves the probe retry policy")
    expect_equal(saved, authz["retryPolicy"], "saved retry policy response")
    expect_equal(authz_state()["retry_policy"], authz["retryPolicy"], "persisted probe retry policy")
    status, _ = http(controller, "PUT", "/api/v1/admin/automatic-retry-policy",
                     original_retry, session=authz_admin_jar, csrf=True)
    expect_equal(status, 200, "Admin restores the retry policy")
    expect_equal(authz_state()["retry_policy"], original_retry, "restored retry policy")

    # Server configuration: classification banner.
    status, original_banner = http(controller, "GET", "/api/v1/admin/classification-config")
    expect_equal(status, 200, "read the installed classification banner")
    expect(original_banner != authz["classification"], "probe banner must differ from the installed one")
    status, saved = http(controller, "PUT", "/api/v1/admin/classification-config",
                         authz["classification"], session=authz_admin_jar, csrf=True)
    expect_equal(status, 200, "Admin saves the probe classification banner")
    expect_equal(saved, authz["classification"], "saved banner response")
    expect_equal(authz_state()["classification"], authz["classification"], "persisted probe banner")
    status, _ = http(controller, "PUT", "/api/v1/admin/classification-config",
                     original_banner, session=authz_admin_jar, csrf=True)
    expect_equal(status, 200, "Admin restores the classification banner")
    expect_equal(authz_state()["classification"], original_banner, "restored banner")

    # User creation with the escalation payload, then removal of that account.
    status, created = http(controller, "POST", "/api/v1/admin/users",
                           authz_create_payload(authz["createEmails"]["control"]),
                           session=authz_admin_jar, csrf=True)
    expect_equal(status, 201, "Admin accepts the escalation create payload")
    expect_equal(created["role"], authz["createProbe"]["role"], "created control role")
    control_id = created["id"]
    expect_equal(authz_user_roles(control_id), ["admin"], "persisted control role")
    status, _ = http(controller, "DELETE", "/api/v1/admin/users/" + control_id,
                     session=authz_admin_jar, csrf=True)
    expect_equal(status, 204, "Admin removes the control account")
    expect_equal(authz_user_row(control_id), None, "control account is gone")

    # User PATCH with the escalation payload, then restoration.
    status, patched = http(controller, "PATCH", "/api/v1/admin/users/" + target_id,
                           authz["patchProbe"], session=authz_admin_jar, csrf=True)
    expect_equal(status, 200, "Admin accepts the escalation PATCH payload")
    expect_equal(authz_user_roles(target_id), ["admin"], "target role after the Admin control")
    expect_equal(authz_user_row(target_id)["is_active"], False, "target disabled by the Admin control")
    status, _ = http(controller, "PATCH", "/api/v1/admin/users/" + target_id,
                     authz["patchRestore"], session=authz_admin_jar, csrf=True)
    expect_equal(status, 200, "Admin restores the target")
    expect_equal(authz_user_roles(target_id), [authz_target["role"]], "restored target role")
    expect_equal(authz_user_row(target_id)["is_active"], True, "restored target enabled")

    authz_baseline = authz_state()
    authz_original = {"retry": original_retry, "banner": original_banner}
    expect_equal(authz_baseline["retry_policy"], original_retry, "baseline retry policy")
    expect_equal(authz_baseline["classification"], original_banner, "baseline banner")
    # Evidence: the Admin controls that proved each probe payload valid, and a
    # summary of the protected state that every denial is compared against.
    receipt("authorization.adminControls", {
        "retryPolicySaved": 200, "classificationSaved": 200,
        "escalationCreate": 201, "escalationCreateRemoved": 204,
        "escalationPatch": 200, "escalationPatchRestored": 200,
    })
    receipt("authorization.protectedStateBaseline", {
        "users": len(authz_baseline["users"]),
        "roleAssignments": len(authz_baseline["roles"]),
        "environmentMemberships": len(authz_baseline["environments"]),
        "userAdminAuditEvents": {a["action"]: a["n"] for a in authz_baseline["user_audit"]},
        "retryPolicy": authz_baseline["retry_policy"],
        "classificationBanner": authz_baseline["classification"],
    })

with case("authorization-operator-and-viewer-user-create-rejected"):
    for actor in authz_actors:
        email = authz["createEmails"][actor]
        authz_probe(actor, "createUser", "POST", "/api/v1/admin/users",
                    authz_create_payload(email))
        expect_equal(sql_json(controller, "SELECT count(*)::int AS n FROM users WHERE email = '%s'" % email),
                     [{"n": 0}], "%s's rejected account was not created" % actor)

with case("authorization-operator-and-viewer-user-patch-rejected"):
    target_id = receipts()["authorization.target.userId"]
    for actor, (fact, jar, receipt_key) in authz_actors.items():
        # Both another account and the actor's own account: self-escalation
        # is the most direct privilege-escalation attempt.
        authz_probe(actor, "patchOtherUser", "PATCH", "/api/v1/admin/users/" + target_id,
                    authz["patchProbe"])
        authz_probe(actor, "patchOwnUser", "PATCH", "/api/v1/admin/users/" + receipts()[receipt_key],
                    authz["patchProbe"])
        expect_equal(authz_user_roles(receipts()[receipt_key]), [fact["role"]],
                     "%s role after both rejected PATCH requests" % actor)
        expect_equal(authz_user_row(receipts()[receipt_key])["is_active"], True,
                     "%s is still enabled" % actor)
    expect_equal(authz_user_roles(target_id), [authz_target["role"]], "target role after all PATCH probes")
    expect_equal(authz_user_row(target_id)["is_active"], True, "target still enabled")

with case("authorization-operator-and-viewer-admin-configuration-rejected"):
    for actor in authz_actors:
        authz_probe(actor, "retryPolicy", "PUT", "/api/v1/admin/automatic-retry-policy",
                    authz["retryPolicy"])
        authz_probe(actor, "classificationBanner", "PUT", "/api/v1/admin/classification-config",
                    authz["classification"])
    final = authz_state()
    expect_equal(final["retry_policy"], authz_original["retry"], "retry policy is still the installed one")
    expect_equal(final["classification"], authz_original["banner"], "banner is still the installed one")

with case("authorization-viewer-has-no-usable-admin-mutation-in-ui"):
    # Direct-route enforcement for the Viewer is asserted by the API cases
    # above. This case covers the production UI only: no administration entry
    # in the navigation, the production "Access Denied" state at /admin, and no
    # administrative mutation control or request. The Viewer's own browser
    # context signs in; no cookie jar is imported.
    before = authz_state()
    run_browser(controller, "viewer-admin-denied")
    expect_equal(receipts()["authorization.viewer.browserUserId"],
                 receipts()["authorization.viewer.userId"], "browser identity is the persisted Viewer")
    expect_equal(authz_state(), before, "the UI visit left protected state unchanged")
    expect_equal(authz_state(), authz_baseline, "protected state equals the post-control baseline")
    receipt("authorization.protectedStateUnchanged", True)


# CF-AGENT-ACCESS-001 v1.1 sections 4.4/5.3 and the owner's independent
# administrator/IdP grant decision: human-session administration edits local
# contributions, not the IdP contribution. These are explicit prerequisite
# source fixtures, not a simulated successful OIDC callback or PAT admission.
# Run after the original baseline assertions; restore u2 before later fragments.
def authz_source_union_boundary():
    operator_id = receipts()["authorization.operator.userId"]
    identity_id = "52920000-0000-4000-8000-000000000001"
    environment_ids = ["52920000-0000-4000-8000-00000000000%d" % n for n in (2, 3, 4)]
    path = "/api/v1/admin/users/" + operator_id

    def fixture_sql(query):
        # Same VM-only psql fixture mechanism as environment-forms.py; sql_json
        # remains read-only. Never contact a host or pre-existing database.
        controller.succeed("runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 "
                           "-d crystal_forge -c " + shlex.quote(query))

    def sources(user_id=operator_id):
        return {
            "roles": sql_json(controller,
                "SELECT role::text, local_grant, oidc_identity_id::text, legacy_ownership_unresolved "
                "FROM user_role_assignments WHERE user_id = '%s'::uuid ORDER BY role::text" % user_id),
            "environments": sql_json(controller,
                "SELECT environment_id::text, local_grant, oidc_identity_id::text, legacy_ownership_unresolved "
                "FROM user_environment_memberships WHERE user_id = '%s'::uuid ORDER BY environment_id" % user_id),
        }

    def source(key, value, local, identity=None, legacy=False):
        return {key: value, "local_grant": local, "oidc_identity_id": identity,
                "legacy_ownership_unresolved": legacy}

    with case("authorization-admin-replacement-preserves-idp-and-classifies-explicit-category"):
        authz_verify_actor("operator")
        expect_equal(authz_whoami(authz_admin_jar)["user"]["id"], receipts()["admin.userId"],
                     "source replacement uses the existing Admin session")
        fixture_sql("""
            BEGIN;
            INSERT INTO external_identities (id, user_id, provider_key, subject)
            VALUES ('%s', '%s', 'oidc', 'access-source-union-fixture');
            INSERT INTO environments (id, name) VALUES
                ('%s', 'cf-it-source-idp'), ('%s', 'cf-it-source-legacy'), ('%s', 'cf-it-source-local');
            UPDATE user_role_assignments SET oidc_identity_id = '%s'
                WHERE user_id = '%s' AND role = 'operator';
            INSERT INTO user_role_assignments (user_id, role, local_grant, legacy_ownership_unresolved)
                VALUES ('%s', 'viewer', false, true);
            INSERT INTO user_environment_memberships
                (user_id, environment_id, local_grant, oidc_identity_id, legacy_ownership_unresolved)
                VALUES ('%s', '%s', true, '%s', false),
                       ('%s', '%s', false, NULL, true), ('%s', '%s', true, NULL, false);
            COMMIT;
            """ % (identity_id, operator_id, *environment_ids, identity_id, operator_id,
                   operator_id, operator_id, environment_ids[0], identity_id,
                   operator_id, environment_ids[1], operator_id, environment_ids[2]))
        before = sources()
        expect_equal(before, {
            "roles": [source("role", "operator", True, identity_id),
                      source("role", "viewer", False, legacy=True)],
            "environments": [source("environment_id", environment_ids[0], True, identity_id),
                             source("environment_id", environment_ids[1], False, legacy=True),
                             source("environment_id", environment_ids[2], True)],
        }, "explicit overlapping, legacy and local-only prerequisites")

        status, patched = http(controller, "PATCH", path, {"role": "Viewer"},
                               session=authz_admin_jar, csrf=True)
        expect_equal(status, 200, "Admin explicitly replaces the local role")
        expect_equal(patched["role"], "Operator", "PATCH returns highest effective role, not requested Viewer")
        expected_roles = [source("role", "operator", False, identity_id), source("role", "viewer", True)]
        expect_equal(sources(), {"roles": expected_roles, "environments": before["environments"]},
                     "role classification preserves IdP and leaves legacy environment category unresolved")

        status, patched = http(controller, "PATCH", path, {"environments": []},
                               session=authz_admin_jar, csrf=True)
        expect_equal(status, 200, "Admin explicitly replaces local and legacy memberships with empty set")
        expect_equal(patched["role"], "Operator", "environment PATCH still returns effective Operator")
        expect_equal(patched["environments"], ["cf-it-source-idp"], "empty local request preserves IdP environment")
        expect_equal(sources(), {
            "roles": expected_roles,
            "environments": [source("environment_id", environment_ids[0], False, identity_id)],
        }, "explicit category replacement removes legacy/local-only rows and only local part of overlap")
        status, users = http(controller, "GET", "/api/v1/admin/users", session=authz_admin_jar)
        expect_equal(status, 200, "independent Admin list read")
        saved = [u for u in users if u["id"] == operator_id]
        expect_equal(len(saved), 1, "list returns exactly the edited identity")
        expect_equal(saved[0]["role"], "Operator", "subsequent read returns effective role")
        expect_equal(saved[0]["environments"], ["cf-it-source-idp"], "subsequent read returns effective membership")
        receipt("authorization.sourceUnion.adminReplacement", {
            "requestedLocalRole": "Viewer", "effectiveRole": "Operator",
            "effectiveEnvironments": ["cf-it-source-idp"], "categoryClassificationExplicit": True,
        })

    with case("authorization-source-union-does-not-admit-operator-admin-mutations"):
        who = authz_whoami(authz_actors["operator"][1])
        expect_equal(who["is_authenticated"], True, "original Operator session remains authenticated")
        expect_equal(who["user"]["id"], operator_id, "source-union session identity")
        expect_equal(sorted(who["roles"]), ["Operator", "Viewer"], "session reads the effective source union")
        for target in (operator_id, receipts()["authorization.target.userId"]):
            before_state = authz_state()
            before_sources = [sources(operator_id), sources(target)]
            status, body = http(controller, "PATCH", "/api/v1/admin/users/" + target,
                                {"role": "Admin", "environments": ["cf-it-source-idp"]},
                                session=authz_actors["operator"][1], csrf=True)
            authz_assert_forbidden(status, body, "source-union Operator self/cross-user PATCH")
            expect_equal(authz_state(), before_state, "denial preserves every protected user, grant and audit count")
            expect_equal([sources(operator_id), sources(target)], before_sources,
                         "denial preserves actor and target source ownership flags")
        receipt("authorization.sourceUnion.operatorDenied", {"ownUser": 403, "otherUser": 403, "stateUnchanged": True})

        # Restore the existing actor through the real Admin API before removing
        # only the injected IdP prerequisite. Later fragments require Operator
        # with empty explicit memberships.
        status, _ = http(controller, "PATCH", path, {"role": "Operator", "environments": []},
                         session=authz_admin_jar, csrf=True)
        expect_equal(status, 200, "Admin restores the Operator's local contribution")
        fixture_sql("""
            BEGIN;
            DELETE FROM user_environment_memberships WHERE user_id = '%s' AND oidc_identity_id = '%s';
            UPDATE user_role_assignments SET oidc_identity_id = NULL
                WHERE user_id = '%s' AND oidc_identity_id = '%s';
            DELETE FROM external_identities WHERE id = '%s';
            DELETE FROM environments WHERE id IN ('%s', '%s', '%s');
            COMMIT;
            """ % (operator_id, identity_id, operator_id, identity_id, identity_id, *environment_ids))
        expect_equal(sources(), {"roles": [source("role", "operator", True)], "environments": []},
                     "later fragments receive the original Operator role and empty scope")
        authz_verify_actor("operator")


authz_source_union_boundary()
