# TASK-490.3 bounded Admin subset, appended after the existing 16 Access cases.
# Authority: approved AdminView.jsx user creation, role, all-environment scope,
# cancel, disable, retry controls and classification controls. These checks
# assert semantics and persistence, not visual parity or outage draft behavior.
# Each UI mutation returns to this driver for an independent SQL read before
# another mutation starts. Browser names MUST all map to the companion .cjs;
# distinct names retain each phase's screenshots and response records.

admin_form_user = FACTS["adminFormUser"]
admin_forms = FACTS["adminForms"]


def admin_forms_sql_literal(value):
    """Quotes authored strings and generated UUID receipts for SQL reads."""
    return "'" + str(value).replace("'", "''") + "'"


def admin_forms_user_state():
    """Reads exact business state, including role and environment assignments."""
    return sql_json(
        controller,
        "SELECT u.id::text AS id, u.username, u.email, u.is_active, "
        "(SELECT coalesce(json_agg(r.role::text ORDER BY r.role), '[]'::json) "
        " FROM user_role_assignments r WHERE r.user_id = u.id) AS roles, "
        "(SELECT coalesce(json_agg(m.environment_id::text ORDER BY m.environment_id), '[]'::json) "
        " FROM user_environment_memberships m WHERE m.user_id = u.id) AS environments "
        "FROM users u WHERE u.email = " + admin_forms_sql_literal(admin_form_user["email"]),
    )


def admin_forms_assert_user(role, enabled):
    expect_equal(admin_forms_user_state(), [{
        "id": receipts()["adminForms.userId"],
        "username": admin_form_user["username"], "email": admin_form_user["email"],
        "is_active": enabled, "roles": [role], "environments": [],
    }], "exact persisted Admin-form account, role and all-environment assignment")


def admin_forms_config(table, columns):
    return sql_json(controller, "SELECT " + columns + " FROM " + table)


def admin_forms_browser(phase):
    """Requests exactly one phase in a fresh context with separate artifacts.

    A write-once request receipt selects the action because ctx has no spec
    name. The browser completion receipt closes that request. Only the driver
    advances to the next phase, after the phase's independent SQL assertions.
    """
    receipt("adminForms.request." + phase, True)
    run_browser(controller, "admin-form-" + phase)
    expect_equal(receipts()["adminForms.browser." + phase], True, phase + " browser completed")


with case("admin-form-create-user-persists-exact-role-and-reloads"):
    expect_equal(admin_forms_user_state(), [], "Dana Chen does not already exist")
    admin_forms_browser("create")
    admin_forms_assert_user(admin_forms["initialRoleDb"], True)
    # Establish that the authored password is usable before testing disablement.
    status, _ = http(controller, "POST", "/api/auth/local/login", {
        "username": admin_form_user["username"], "password": admin_form_user["password"],
    }, session="admin-forms-user-before-disable")
    expect_equal(status, 200, "created account can sign in with its UI-authored password")
    receipt("adminForms.sql.create", admin_forms_user_state())

with case("admin-form-edit-role-persists-and-reloads"):
    admin_forms_assert_user(admin_forms["initialRoleDb"], True)
    admin_forms_browser("edit")
    admin_forms_assert_user(admin_forms["editedRoleDb"], True)
    receipt("adminForms.sql.edit", admin_forms_user_state())

with case("admin-form-cancel-create-and-edit-leave-rows-unchanged"):
    # Reuse the earlier fragment's complete protected-state snapshot. It
    # includes every user's credential digest, assignments, config and user
    # audit events, but excludes sign-in activity from the fresh browser.
    before_cancel = authz_state()
    admin_forms_browser("cancel")
    expect_equal(authz_state(), before_cancel, "both Cancel actions leave protected rows unchanged")
    expect_equal(sql_json(controller, "SELECT count(*)::int AS n FROM users WHERE email = "
                          + admin_forms_sql_literal(admin_forms["cancelEmail"])),
                 [{"n": 0}], "cancelled new account does not exist")
    admin_forms_assert_user(admin_forms["editedRoleDb"], True)
    receipt("adminForms.sql.cancelUnchanged", True)

with case("admin-form-disable-persists-reloads-and-rejects-sign-in"):
    admin_forms_browser("disable")
    admin_forms_assert_user(admin_forms["editedRoleDb"], False)
    receipt("adminForms.sql.disable", admin_forms_user_state())
    before_sessions = sql_json(controller, "SELECT id::text AS id FROM user_sessions WHERE user_id = "
                               + admin_forms_sql_literal(receipts()["adminForms.userId"])
                               + "::uuid ORDER BY id")
    # One attempt in a new jar: a previously working password now fails only
    # after disablement. The failed attempt MUST NOT create a session row.
    status, _ = http(controller, "POST", "/api/auth/local/login", {
        "username": admin_form_user["username"], "password": admin_form_user["password"],
    }, session="admin-forms-disabled-user")
    expect_equal(status, 401, "disabled account cannot sign in")
    expect_equal(sql_json(controller, "SELECT id::text AS id FROM user_sessions WHERE user_id = "
                          + admin_forms_sql_literal(receipts()["adminForms.userId"])
                          + "::uuid ORDER BY id"), before_sessions,
                 "disabled sign-in created no session")
    receipt("adminForms.disabledSignInStatus", status)

with case("admin-form-retry-policy-persists-and-reloads"):
    before_retry = admin_forms_config("automatic_retry_policy",
        "max_build_retries, max_evaluation_retries, backoff_seconds, transient_only")
    expect(before_retry != [admin_forms["retryPolicy"]], "retry form changes the installed values")
    admin_forms_browser("retry")
    saved_retry = admin_forms_config("automatic_retry_policy",
        "max_build_retries, max_evaluation_retries, backoff_seconds, transient_only")
    expect_equal(saved_retry, [admin_forms["retryPolicy"]], "exact persisted UI retry policy")
    receipt("adminForms.sql.retry", saved_retry)

with case("admin-form-classification-enable-persists-and-reloads"):
    before_classification = admin_forms_config("classification_banner_config", "enabled, level, custom_text")
    expect_equal(len(before_classification), 1, "one classification configuration")
    expect_equal(before_classification[0]["enabled"], False, "classification starts disabled")
    admin_forms_browser("classification-enable")
    enabled_classification = dict(before_classification[0], enabled=True)
    expect_equal(admin_forms_config("classification_banner_config", "enabled, level, custom_text"),
                 [enabled_classification], "toggle saves only enabled and preserves other configuration")
    receipt("adminForms.sql.classification-enable", enabled_classification)

with case("admin-form-classification-marking-persists-and-reloads"):
    admin_forms_browser("classification")
    saved_classification = admin_forms_config("classification_banner_config", "enabled, level, custom_text")
    expect_equal(saved_classification, [admin_forms["classification"]],
                 "exact persisted UI classification level and marking")
    receipt("adminForms.sql.classification", saved_classification)
