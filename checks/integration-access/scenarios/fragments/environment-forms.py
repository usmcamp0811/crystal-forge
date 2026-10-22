# Concatenate after admin-forms.py, preserving the preceding 23 cases unchanged.
# Runtime prerequisites: controller, FACTS, case, expect, expect_equal, receipt,
# receipts, http, sql_json, run_browser; authorization.py's authz_state(), live
# authz-admin jar, and admin.userId/authorization.operator.userId UUID receipts.
# Operator from .2 MUST remain active, Operator, with [] explicit memberships.
# The chosen environment names MUST be absent; no systems are seeded here.
# Authority: TASK-490.3, approved EnvironmentsView.jsx EnvFormModal and
# DeleteEnvConfirm, AdminView.jsx AdminUserModal, reference source 77251b04.
# Optional gates: approved EnvironmentsView.jsx at 6e4ea520 allows an empty set.
# CF-agent enablement remains a global invariant, not an assignable gate.
# Extends access.configuration.admin/resources in lifecycle-local-bootstrap.
# This fragment does not establish visual parity or unspecified outage states.
#
# Facts: FACTS['environmentForms'] has create/edit/cancel/duplicate draft dicts
# and cancelName:str; full typed JSON interface is in environment-forms.cjs.
# BrowserSpecs: environment-form-{create,optional,edit,cancel,duplicate,membership,
# membership-restore,delete} all map to environment-forms.cjs. Each run has a
# fresh context; requests/completions are write-once boolean receipts. Only
# generated IDs pass between phases, never expected semantic values.
# Outputs: environmentForms.environmentId:str(UUID), response.<phase> dicts,
# policyRequest.<phase>:{required_policy_ids:list[str(UUID)]},
# policyResponse.<phase>:{status:int,body:dict}, invalidSelections:list[dict],
# browser.<phase>:bool, sql.{create,edit}:list[dict], sql.*Unchanged:bool,
# sql.membership/sql.membershipRestored:list[{environment_id:str(UUID)}],
# sql.delete:list (empty), and cancelWrites/duplicateWrites:list (empty).
# All keys above have the environmentForms. prefix. The membership case reads
# SQL between assignment and UI restoration before later preferences may run.

environment_forms = FACTS["environmentForms"]


def environment_forms_literal(value):
    """Quotes authored strings and generated UUIDs for independent SQL reads."""
    return "'" + str(value).replace("'", "''") + "'"


def environment_forms_rows():
    """Reads the complete registry for no-insert and no-write comparisons."""
    return sql_json(controller, "SELECT * FROM environments ORDER BY id")


def environment_forms_state():
    """Includes registry/policy rows omitted by the Admin protected snapshot."""
    return {
        "admin": authz_state(),
        "registry": environment_forms_rows(),
        "policies": sql_json(controller,
            "SELECT * FROM environment_policies ORDER BY environment_id, policy_id"),
    }


def environment_forms_current():
    """Reads the saved identity and every business field changed by the form."""
    return sql_json(controller,
        "SELECT e.id::text AS id, e.name, e.description, e.color_hex, e.is_active, "
        "e.default_policy, e.auto_sync, e.requires_approval, e.is_production, "
        "(SELECT count(*)::int FROM systems s WHERE s.environment_id = e.id) AS systems "
        "FROM environments e WHERE e.id = "
        + environment_forms_literal(receipts()["environmentForms.environmentId"]) + "::uuid")


def environment_forms_assert_current(draft):
    expect_equal(environment_forms_current(), [{
        "id": receipts()["environmentForms.environmentId"],
        "name": draft["name"], "description": draft["description"],
        "color_hex": draft["color_hex"], "is_active": True,
        "default_policy": draft["default_policy"], "auto_sync": draft["auto_sync"],
        "requires_approval": draft["requires_approval"],
        "is_production": draft["is_production"], "systems": 0,
    }], "exact persisted environment fields and zero assigned systems")


def environment_forms_memberships():
    return sql_json(controller,
        "SELECT environment_id::text AS environment_id FROM user_environment_memberships "
        "WHERE user_id = " + environment_forms_literal(receipts()["authorization.operator.userId"])
        + "::uuid ORDER BY environment_id")


def environment_forms_policy_ids():
    return sql_json(controller,
        "SELECT policy_id::text AS policy_id FROM environment_policies WHERE environment_id = "
        + environment_forms_literal(receipts()["environmentForms.environmentId"])
        + "::uuid ORDER BY policy_id")


def environment_forms_browser(phase):
    """Runs one named phase, then returns to Python before the next mutation."""
    receipt("environmentForms.request." + phase, True)
    run_browser(controller, "environment-form-" + phase)
    expect_equal(receipts()["environmentForms.browser." + phase], True,
                 phase + " browser completed")


with case("environment-form-create-persists-exact-fields-and-reloads"):
    names = {environment_forms[key]["name"] for key in ("create", "edit", "cancel")}
    names.add(environment_forms["cancelName"])
    before_environment_rows = environment_forms_rows()
    expect(not any(row["name"] in names for row in before_environment_rows),
           "authored environment identities are unused; no result is seeded")
    expect_equal(environment_forms["duplicate"]["name"], environment_forms["edit"]["name"],
                 "duplicate draft targets the environment this UI workflow creates")
    environment_forms_browser("create")
    environment_forms_assert_current(environment_forms["create"])
    expect_equal(environment_forms_policy_ids(), [], "new environment has zero optional assignments")
    expect_equal(receipts()["environmentForms.policyRequest.create"], {"required_policy_ids": []},
                 "actual production request has no legacy invariant ID")
    expect_equal(len(environment_forms_rows()), len(before_environment_rows) + 1,
                 "UI create inserted exactly one environment")
    receipt("environmentForms.sql.create", environment_forms_current())

with case("environment-form-edit-persists-exact-fields-and-reloads"):
    environment_forms_assert_current(environment_forms["create"])
    before_environment_count = len(environment_forms_rows())
    environment_forms_optional_policy = environment_forms["optionalPolicy"]
    expect_equal(sql_json(controller,
        "SELECT id::text AS id, name, policy_type, enabled FROM deployment_policies WHERE id = "
        + environment_forms_literal(environment_forms_optional_policy["id"]) + "::uuid"),
        [dict(environment_forms_optional_policy, enabled=True)], "authored optional gate exists and is enabled")
    environment_forms_browser("optional")
    environment_forms_assert_current(environment_forms["create"])
    expect_equal(environment_forms_policy_ids(), [{"policy_id": environment_forms_optional_policy["id"]}],
                 "UI selection persists the exact enabled optional gate")
    receipt("environmentForms.sql.optional", environment_forms_policy_ids())
    environment_forms_browser("edit")
    environment_forms_assert_current(environment_forms["edit"])
    expect_equal(environment_forms_policy_ids(), [{"policy_id": environment_forms_optional_policy["id"]}],
                 "metadata edit and reopen preserve the existing optional assignment")
    expect_equal(len(environment_forms_rows()), before_environment_count,
                 "UI edit updates the same environment without inserting")
    receipt("environmentForms.sql.edit", environment_forms_current())
    # A valid Admin session and CSRF credential distinguish selection rejection
    # from authorization failure. Include a valid ID beside each invalid ID so
    # silently dropping only the bad selection cannot satisfy this assertion.
    status, who = http(controller, "GET", "/api/auth/whoami", session="authz-admin")
    expect_equal(status, 200, "assignment probe Admin session")
    expect_equal(who["user"]["id"], receipts()["admin.userId"], "authorized assignment actor")
    expect_equal(who["roles"], [FACTS["adminRoleWire"]], "assignment actor is Admin")
    expect_equal(len(environment_forms["invalidPolicyIds"]), 2,
                 "two explicit invalid probes: disabled built-in and absent policy")
    expect_equal(sql_json(controller,
        "SELECT policy_type, enabled FROM deployment_policies WHERE id = "
        + environment_forms_literal(environment_forms["invalidPolicyIds"][0]) + "::uuid"),
        [{"policy_type": "require_cf_agent", "enabled": False}],
        "first invalid probe is the disabled, nonassignable legacy invariant")
    expect_equal(sql_json(controller,
        "SELECT id FROM deployment_policies WHERE id = "
        + environment_forms_literal(environment_forms["invalidPolicyIds"][1]) + "::uuid"),
        [], "second invalid probe is absent, not an authorization or payload failure")
    invalid_results = []
    for invalid_id in environment_forms["invalidPolicyIds"]:
        before_invalid = environment_forms_state()
        payload = {"required_policy_ids": [environment_forms_optional_policy["id"], invalid_id]}
        status, body = http(controller, "PATCH", "/api/v1/environments/"
            + receipts()["environmentForms.environmentId"] + "/policies", payload,
            session="authz-admin", csrf=True)
        expect_equal(status, 400, "explicit invalid optional policy selection is a validation error")
        expect_equal(body["error"], "validation_error", "established invalid-selection error")
        expect_equal(body["message"], "One or more selected policies are invalid",
                     "established invalid-selection message")
        expect_equal(environment_forms_state(), before_invalid,
                     "rejection preserves the full assignment set and all metadata")
        invalid_results.append({"request": payload, "status": status,
                                "error": body["error"], "assignmentsUnchanged": True})
    receipt("environmentForms.invalidSelections", invalid_results)

with case("environment-form-cancel-create-and-edit-have-no-write"):
    before_environment_cancel = environment_forms_state()
    environment_forms_browser("cancel")
    expect_equal(environment_forms_state(), before_environment_cancel,
                 "both cancels preserve registry, policies and protected Admin state")
    environment_forms_assert_current(environment_forms["edit"])
    expect_equal(receipts()["environmentForms.cancelWrites"], [], "Cancel sent no write")
    receipt("environmentForms.sql.cancelUnchanged", True)

with case("environment-form-duplicate-validation-retains-draft-and-inserts-nothing"):
    before_environment_duplicate = environment_forms_state()
    environment_forms_browser("duplicate")
    expect_equal(environment_forms_state(), before_environment_duplicate,
                 "duplicate validation preserves every environment and protected row")
    environment_forms_assert_current(environment_forms["edit"])
    expect_equal(receipts()["environmentForms.duplicateWrites"], [],
                 "duplicate client validation sent no write")
    receipt("environmentForms.sql.duplicateUnchanged", True)

with case("environment-form-explicit-operator-membership-persists-reloads-and-restores"):
    # Reuse the established Admin jar for a read-only identity check. Actual
    # writes and their reloads use independently authenticated browser contexts.
    status, environment_forms_admin = http(controller, "GET", "/api/auth/whoami",
                                           session="authz-admin")
    expect_equal(status, 200, "existing Admin session remains usable")
    expect_equal(environment_forms_admin["user"]["id"], receipts()["admin.userId"],
                 "existing jar is the same Admin")
    before_membership_admin = authz_state()
    expect_equal(environment_forms_memberships(), [], "Operator starts with empty explicit scope")
    environment_forms_browser("membership")
    expect_equal(environment_forms_memberships(), [{
        "environment_id": receipts()["environmentForms.environmentId"],
    }], "nonempty membership binds the exact UI-created environment UUID")
    expect_equal(authz_state()["environments"], sorted(
        before_membership_admin["environments"] + [{
            "user_id": receipts()["authorization.operator.userId"],
            "environment_id": receipts()["environmentForms.environmentId"],
        }], key=lambda row: (row["user_id"], row["environment_id"])),
        "only the selected Operator's exact membership was added")
    for field in ("users", "roles", "retry_policy", "classification"):
        expect_equal(authz_state()[field], before_membership_admin[field],
                     "membership preserves protected " + field)
    receipt("environmentForms.sql.membership", environment_forms_memberships())
    # SQL proves the nonempty intermediate state before any restoration write.
    environment_forms_browser("membership-restore")
    expect_equal(environment_forms_memberships(), [], "UI restores original explicit memberships")
    restored_membership_admin = authz_state()
    for field in ("users", "roles", "environments", "retry_policy", "classification"):
        expect_equal(restored_membership_admin[field], before_membership_admin[field],
                     "restoration preserves later actor state: " + field)
    receipt("environmentForms.sql.membershipRestored", environment_forms_memberships())

with case("environment-form-delete-unused-tier-confirms-removes-and-reloads"):
    environment_forms_assert_current(environment_forms["edit"])
    expect_equal(sql_json(controller,
        "SELECT user_id::text AS user_id FROM user_environment_memberships WHERE environment_id = "
        + environment_forms_literal(receipts()["environmentForms.environmentId"]) + "::uuid"),
        [], "membership was restored before deleting the unused environment")
    before_delete_rows = environment_forms_rows()
    environment_forms_browser("delete")
    expect_equal(environment_forms_current(), [], "UI-deleted environment UUID is absent")
    expect_equal(environment_forms_policy_ids(), [], "deleted tier leaves no optional gate assignments")
    expect_equal(environment_forms_rows(), [row for row in before_delete_rows
        if row["id"] != receipts()["environmentForms.environmentId"]],
        "delete removed exactly the selected environment and preserved other rows")
    receipt("environmentForms.sql.delete", environment_forms_current())
