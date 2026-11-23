# Owner access-token lifecycle cases (TASK-534.3). Compose LAST, after
# management-registration.py, in the same controller VM and Python namespace.
# Requires: controller, FACTS, case/expect/expect_equal/receipt/receipts/http/
# sql_json/run_browser and the private _vm_write from driver.py; authorization.py's
# authz_actors, authz_admin_jar, authz_verify_actor; the authz-admin, authz-operator
# and authz-viewer cookie jars; receipts admin.userId,
# authorization.operator.userId and authorization.viewer.userId.
#
# State contract. The Operator and the Viewer start with empty explicit
# memberships. Case 1 creates two active environments through the Admin API and
# assigns ONLY FACTS.accessTokens.environment to both actors (PATCH with
# edit_local_grants). The memberships stay assigned: no later fragment may rely
# on the empty-scope state. No token exists before case 1.
#
# Authority: TASK-534.3 API contract and acceptance criteria 1-4, 6 and 7. A
# token is the owner's own credential: no Admin check applies, a bearer
# credential never authenticates the management API, create never narrows a
# request, and the secret is returned once while only its SHA-256 is stored.
# Expected values come from authored facts and that contract. The secret is
# held only in Python variables and never goes into a receipt, an assertion
# message or a command line (sql_json and _vm_write move payloads by file).
#
# Not asserted here: the Cache-Control header of create/rotate responses. The
# http() helper returns only (status, body). The browser spec asserts `no-store`
# on the real create, rotate and revoke responses, and the bearer probe below
# asserts it on a 401 through its own curl.
# Browser phases: access-token-{create,rotate,revoke} map to access-tokens.cjs.
# Each phase is a fresh context selected by a write-once request receipt and
# closed by a completion receipt; SQL is checked between phases.

import hashlib
import re

at_facts = FACTS["accessTokens"]
AT_SECRET_FORMAT = re.compile(r"^cfat_v1_([0-9a-f]{32})_[A-Za-z0-9_-]{43}$")
AT_ACTIONS = ("access_token_created", "access_token_rotated", "access_token_revoked")
AT_RESPONSE_FIELDS = {
    "id", "name", "audience", "display_hint", "scopes", "environments",
    "created_at", "expires_at", "revoked_at", "last_used_at", "status",
}
at_read_scopes = sorted(at_facts["readScopes"])
at_secrets = {}  # actor -> plaintext; never leaves this namespace


def at_literal(value):
    return "'" + str(value).replace("'", "''") + "'"


def at_jar(actor):
    return authz_actors[actor][1]


def at_user_id(actor):
    return receipts()[authz_actors[actor][2]]


def at_simple(token_id):
    """Public token id in the 32-hex form used as the audit target."""
    return token_id.replace("-", "")


def at_rows(actor):
    """Complete persisted rows of one owner's tokens, oldest first."""
    return sql_json(
        controller,
        "SELECT t.id::text AS id, t.name, t.audience, t.display_hint, "
        "octet_length(t.secret_hash) AS hash_bytes, encode(t.secret_hash, 'hex') AS hash_hex, "
        "t.revoked_at IS NOT NULL AS revoked, t.revoked_by_user_id::text AS revoked_by, "
        "t.last_used_at IS NULL AS never_used, "
        "round(extract(epoch FROM (t.expires_at - t.created_at)))::bigint AS lifetime_seconds, "
        "(SELECT coalesce(json_agg(s.scope ORDER BY s.scope), '[]'::json) "
        " FROM api_access_token_scopes s WHERE s.token_id = t.id) AS scopes, "
        "(SELECT coalesce(json_agg(e.environment_id::text ORDER BY e.environment_id), '[]'::json) "
        " FROM api_access_token_environments e WHERE e.token_id = t.id) AS environments "
        "FROM api_access_tokens t WHERE t.user_id = %s::uuid ORDER BY t.created_at, t.id"
        % at_literal(at_user_id(actor)),
    )


def at_counts():
    """Row counts of every table a rejected request must leave untouched."""
    return sql_json(
        controller,
        "SELECT (SELECT count(*)::int FROM api_access_tokens) AS tokens, "
        "(SELECT count(*)::int FROM api_access_token_scopes) AS scopes, "
        "(SELECT count(*)::int FROM api_access_token_environments) AS environments, "
        "(SELECT count(*)::int FROM admin_audit_events WHERE action IN (%s)) AS audit"
        % ", ".join(at_literal(a) for a in AT_ACTIONS),
    )[0]


def at_counts_plus(base, tokens=0, scopes=0, environments=0, audit=0):
    return {"tokens": base["tokens"] + tokens, "scopes": base["scopes"] + scopes,
            "environments": base["environments"] + environments, "audit": base["audit"] + audit}


def at_audit():
    """Access-token audit events, oldest first, with their full metadata."""
    return sql_json(
        controller,
        "SELECT action, target, actor_user_id::text AS actor_user_id, actor_identifier, metadata "
        "FROM admin_audit_events WHERE action IN (%s) ORDER BY created_at, id"
        % ", ".join(at_literal(a) for a in AT_ACTIONS),
    )


def at_secret_hits(secret):
    """Counts rows whose text form contains `secret` in every table it must not reach."""
    lit = at_literal(secret)
    return sql_json(
        controller,
        "SELECT (SELECT count(*)::int FROM api_access_tokens x WHERE position(%s in x::text) > 0) AS tokens, "
        "(SELECT count(*)::int FROM api_access_token_scopes x WHERE position(%s in x::text) > 0) AS scopes, "
        "(SELECT count(*)::int FROM api_access_token_environments x WHERE position(%s in x::text) > 0) AS environments, "
        "(SELECT count(*)::int FROM admin_audit_events x WHERE position(%s in x::text) > 0) AS audit"
        % (lit, lit, lit, lit),
    )[0]


def at_create(actor, name, environment_ids, scopes, csrf=True):
    body = {"name": name, "audience": "mcp", "expires_in_days": at_facts["lifetimeDays"],
            "environment_ids": environment_ids, "scopes": scopes}
    return http(controller, "POST", "/api/v1/user/access-tokens", body,
                session=at_jar(actor), csrf=csrf)


def at_assert_secret_format(secret, what):
    # The message never includes the secret.
    expect(isinstance(secret, str) and secret.startswith("cfat_v1_"), what + " has the cfat_v1_ prefix")
    expect(AT_SECRET_FORMAT.match(secret) is not None, what + " has the canonical token format")


def at_assert_fields(actor, body, expected_message_part, what):
    """A rejected grant is a 400 validation_failed naming the offending field."""
    expect_equal(body["error"], "validation_failed", what + " error code")
    message = (body.get("details") or {}).get("fields", {}).get(expected_message_part[0])
    expect(isinstance(message, str) and expected_message_part[1] in message,
           what + ": field message names " + expected_message_part[1])
    return message


def at_bearer_get(secret, jar=None):
    """GETs the token list with ONLY a bearer header (plus a cookie jar if given).

    The header travels by file so the secret is not in the logged command.
    Returns (status, lowercased response headers, parsed body).
    """
    _vm_write(controller, "/tmp/cf-it/bearer-header", "authorization: Bearer %s\n" % secret)
    cookie = ("-b /tmp/cf-it/jar-%s.txt " % jar) if jar else ""
    status = controller.succeed(
        "curl -sS -o /tmp/cf-it/bearer-body -D /tmp/cf-it/bearer-headers -w '%{http_code}' "
        "-H @/tmp/cf-it/bearer-header " + cookie + "http://localhost:3000/api/v1/user/access-tokens"
    )
    headers = controller.succeed("cat /tmp/cf-it/bearer-headers").lower()
    body = json.loads(controller.succeed("cat /tmp/cf-it/bearer-body"))
    controller.succeed("rm -f /tmp/cf-it/bearer-header")
    return int(status.strip()), headers, body


def at_browser(phase):
    receipt("accessTokens.request." + phase, True)
    run_browser(controller, "access-token-" + phase)
    expect_equal(receipts()["accessTokens.browser." + phase], True, phase + " browser completed")


with case("access-token-policy-offers-operator-only-the-assigned-active-environment"):
    expect_equal(at_counts(), {"tokens": 0, "scopes": 0, "environments": 0, "audit": 0},
                 "no access-token state exists before these cases")
    at_environment = {}
    for key in ("environment", "otherEnvironment"):
        status, _ = http(
            controller, "POST", "/api/v1/environments",
            {"name": at_facts[key], "default_policy": "manual", "color_hex": "#82699b", "is_active": True},
            session=authz_admin_jar, csrf=True)
        expect_equal(status, 201, "Admin creates environment " + at_facts[key])
        found = sql_json(controller, "SELECT id::text AS id, is_active FROM environments WHERE name = %s"
                         % at_literal(at_facts[key]))
        expect_equal(len(found), 1, "persisted environment " + at_facts[key])
        expect_equal(found[0]["is_active"], True, at_facts[key] + " is active")
        at_environment[key] = receipt("accessTokens.%s.id" % key, found[0]["id"])
    at_env_id = at_environment["environment"]
    at_other_id = at_environment["otherEnvironment"]

    # Assign exactly one environment to each actor through the Admin API.
    for actor in ("operator", "viewer"):
        authz_verify_actor(actor)
        status, patched = http(
            controller, "PATCH", "/api/v1/admin/users/" + at_user_id(actor),
            {"environments": [at_facts["environment"]], "edit_local_grants": True},
            session=authz_admin_jar, csrf=True)
        expect_equal(status, 200, "Admin assigns the environment to the " + actor)
        expect_equal(patched["role"], authz_actors[actor][0]["roleWire"], actor + " role is unchanged")
        expect_equal(patched["environments"], [at_facts["environment"]], actor + " effective environments")
        members = sql_json(controller,
            "SELECT environment_id::text AS environment_id FROM user_environment_memberships "
            "WHERE user_id = %s::uuid" % at_literal(at_user_id(actor)))
        expect_equal(members, [{"environment_id": at_env_id}], "persisted " + actor + " membership")

    # Only the Operator's own session reaches the policy; no admin route is called.
    authz_verify_actor("operator")
    status, policy = http(controller, "GET", "/api/v1/user/access-token-policy", session=at_jar("operator"))
    expect_equal(status, 200, "Operator reads the access-token policy")
    expect_equal(policy["enabled"], True, "access tokens are enabled")
    expect_equal(policy["audience"], "mcp", "policy audience")
    expect_equal(policy["create_blocked"], None, "a local Operator is not blocked from creating")
    expect_equal(policy["default_lifetime_days"], at_facts["lifetimeDays"], "default lifetime")
    expect(at_facts["lifetimeDays"] in policy["lifetime_options_days"], "authored lifetime is selectable")
    # Compare identity fields only; environments also carry display metadata.
    expect_equal([{"id": e["id"], "name": e["name"]} for e in policy["environments"]],
                 [{"id": at_env_id, "name": at_facts["environment"]}],
                 "only the assigned active environment is offered; the unassigned and the "
                 "other active environments are not")
    offered = {s["scope"]: s for s in policy["scopes"]}
    expect_equal(sorted(offered), sorted(at_facts["readScopes"] + at_facts["writeScopes"]
                                         + at_facts["optionalReadScopes"] + at_facts["adminOnlyScopes"]
                                         + at_facts["unavailableScopes"]),
                 "scope catalogue")
    for scope in at_facts["readScopes"]:
        expect_equal({k: offered[scope][k] for k in ("available", "default_selected", "unavailable_reason")},
                     {"available": True, "default_selected": True, "unavailable_reason": None},
                     scope + " is selectable and preselected")
    for scope in at_facts["writeScopes"]:
        expect_equal({k: offered[scope][k] for k in ("available", "default_selected", "unavailable_reason")},
                     {"available": True, "default_selected": False, "unavailable_reason": None},
                     scope + " is selectable for an Operator but not preselected")
    for scope in at_facts["optionalReadScopes"]:
        expect_equal({k: offered[scope][k] for k in ("available", "default_selected", "unavailable_reason")},
                     {"available": True, "default_selected": False, "unavailable_reason": None},
                     scope + " is selectable but not preselected")
    # The role reason precedes the release reason for Admin-only scopes.
    for scope in at_facts["adminOnlyScopes"]:
        expect_equal({k: offered[scope][k] for k in ("available", "default_selected", "unavailable_reason")},
                     {"available": False, "default_selected": False, "unavailable_reason": "Requires Admin"},
                     scope + " requires Admin")
    for scope in at_facts["unavailableScopes"]:
        expect_equal({k: offered[scope][k] for k in ("available", "default_selected", "unavailable_reason")},
                     {"available": False, "default_selected": False, "unavailable_reason": "Not supported yet"},
                     scope + " is listed unavailable")
    receipt("accessTokens.policy.operator", {
        "environments": [e["name"] for e in policy["environments"]],
        "selectable": sorted(s for s, v in offered.items() if v["available"]),
        "unavailable": sorted(s for s, v in offered.items() if not v["available"]),
    })

with case("access-token-create-returns-secret-once-and-stores-only-its-hash"):
    authz_verify_actor("operator")
    before = at_counts()
    status, created = at_create("operator", at_facts["operatorToken"], [at_env_id], at_facts["readScopes"])
    expect_equal(status, 201, "Operator creates a read-scope token for the assigned environment")
    secret = created.get("token")
    at_assert_secret_format(secret, "returned secret")
    at_secrets["operator"] = secret
    at_operator_token_id = receipt("accessTokens.operator.tokenId", created["id"])
    expect_equal(AT_SECRET_FORMAT.match(secret).group(1), at_simple(at_operator_token_id),
                 "the token carries its public id")
    expect_equal(set(created), AT_RESPONSE_FIELDS | {"token"},
                 "create response is the token metadata plus the one-time token only")
    expect_equal({k: created[k] for k in ("name", "audience", "scopes", "status", "revoked_at", "last_used_at")},
                 {"name": at_facts["operatorToken"], "audience": "mcp", "scopes": at_read_scopes,
                  "status": "active", "revoked_at": None, "last_used_at": None},
                 "create response metadata")
    expect_equal([{k: e[k] for k in ("id", "name", "is_active")} for e in created["environments"]],
                 [{"id": at_env_id, "name": at_facts["environment"], "is_active": True}],
                 "create response environments")

    rows = at_rows("operator")
    expect_equal(len(rows), 1, "exactly one persisted token for the Operator")
    row = rows[0]
    expect_equal(row["id"], at_operator_token_id, "persisted token id")
    expect_equal(row["hash_bytes"], 32, "secret_hash is 32 bytes")
    expect_equal(row["hash_hex"], hashlib.sha256(secret.encode()).hexdigest(),
                 "the stored credential is the SHA-256 of the returned secret")
    expect_equal(row["display_hint"], created["display_hint"], "hint persisted as returned")
    expect_equal(row["display_hint"], "cfat_v1_" + at_simple(at_operator_token_id)[:4] + "\u2026",
                 "display hint is the prefix, four id digits and an ellipsis")
    expect_equal({k: row[k] for k in ("name", "audience", "scopes", "environments", "revoked", "never_used")},
                 {"name": at_facts["operatorToken"], "audience": "mcp", "scopes": at_read_scopes,
                  "environments": [at_env_id], "revoked": False, "never_used": True},
                 "persisted token grant and state")
    expect(abs(row["lifetime_seconds"] - at_facts["lifetimeDays"] * 86400) <= 5,
           "persisted lifetime is the requested number of days")
    expect_equal(at_counts(), at_counts_plus(before, tokens=1, scopes=3, environments=1, audit=1),
                 "create added one token, its three scopes, one environment grant and one audit event")
    expect_equal(at_secret_hits(secret), {"tokens": 0, "scopes": 0, "environments": 0, "audit": 0},
                 "plaintext secret is absent from the token tables and the audit log")

    events = at_audit()
    expect_equal(len(events), 1, "exactly one access-token audit event")
    event = events[0]
    expect_equal({k: event[k] for k in ("action", "target", "actor_user_id", "actor_identifier")},
                 {"action": "access_token_created", "target": at_simple(at_operator_token_id),
                  "actor_user_id": at_user_id("operator"), "actor_identifier": at_user_id("operator")},
                 "created audit event identity")
    metadata = dict(event["metadata"])
    expect(isinstance(metadata.pop("expires_at", None), str), "audit metadata records the expiry")
    # A read-only grant needs no production confirmation (TASK-534.5).
    expect_equal(metadata, {"name": at_facts["operatorToken"], "audience": "mcp",
                            "scopes": at_read_scopes, "environment_ids": [at_env_id],
                            "production_write_confirmed": False},
                 "audit metadata holds the grant and no secret or hash")
    receipt("accessTokens.sql.operatorCreate", {"hashBytes": 32, "hashIsSha256OfSecret": True,
                                                "plaintextHits": 0, "auditEvents": 1})

with case("access-token-rejects-unassigned-environment-missing-csrf-and-viewer-write-scope"):
    authz_verify_actor("operator")
    authz_verify_actor("viewer")
    baseline = at_counts()
    operator_rows = at_rows("operator")

    # Operator: an unassigned environment, alone or beside an assigned one,
    # rejects the whole request. Nothing is narrowed or created.
    for label, environment_ids in (("unassigned", [at_other_id]), ("mixed", [at_env_id, at_other_id])):
        status, body = at_create("operator", at_facts["deniedToken"], environment_ids, at_facts["readScopes"])
        expect_equal(status, 400, "Operator create with the " + label + " environment")
        message = at_assert_fields("operator", body, ("environment_ids", at_other_id), label + " environment")
        expect(at_env_id not in message, label + ": the assigned environment is not reported as invalid")
        expect_equal(at_counts(), baseline, label + " rejection created no row or audit event")
    status, body = at_create("operator", at_facts["deniedToken"], [at_env_id],
                             at_facts["readScopes"] + ["poams:close"])
    expect_equal(status, 400, "Operator create with an unavailable scope")
    at_assert_fields("operator", body, ("scopes", "poams:close"), "unavailable scope")
    expect_equal(at_counts(), baseline, "unavailable-scope rejection created nothing")

    # A valid request without the CSRF header is refused before any write.
    status, _ = at_create("operator", at_facts["deniedToken"], [at_env_id], at_facts["readScopes"], csrf=False)
    expect_equal(status, 403, "Operator create without the CSRF header")
    expect_equal(at_counts(), baseline, "missing-CSRF rejection created nothing")
    expect_equal(at_rows("operator"), operator_rows, "Operator token rows are unchanged by every denial")

    # Viewer: a read-only token for the assigned environment is allowed.
    status, created = at_create("viewer", at_facts["viewerToken"], [at_env_id], at_facts["readScopes"])
    expect_equal(status, 201, "Viewer creates a read-only token")
    at_assert_secret_format(created.get("token"), "Viewer secret")
    at_secrets["viewer"] = created["token"]
    at_viewer_token_id = receipt("accessTokens.viewer.tokenId", created["id"])
    viewer_rows = at_rows("viewer")
    expect_equal(len(viewer_rows), 1, "exactly one persisted Viewer token")
    expect_equal({k: viewer_rows[0][k] for k in ("id", "name", "scopes", "environments", "revoked", "hash_bytes")},
                 {"id": at_viewer_token_id, "name": at_facts["viewerToken"], "scopes": at_read_scopes,
                  "environments": [at_env_id], "revoked": False, "hash_bytes": 32},
                 "persisted Viewer token")
    expect_equal(viewer_rows[0]["hash_hex"], hashlib.sha256(at_secrets["viewer"].encode()).hexdigest(),
                 "Viewer stored credential is the SHA-256 of the returned secret")
    after_viewer = at_counts()
    expect_equal(after_viewer, at_counts_plus(baseline, tokens=1, scopes=3, environments=1, audit=1),
                 "Viewer create added exactly one token")
    expect_equal(at_secret_hits(at_secrets["viewer"]), {"tokens": 0, "scopes": 0, "environments": 0, "audit": 0},
                 "Viewer plaintext secret is absent from the token tables and the audit log")

    # Viewer: write or operational scopes are not selectable.
    status, policy = http(controller, "GET", "/api/v1/user/access-token-policy", session=at_jar("viewer"))
    expect_equal(status, 200, "Viewer reads the access-token policy")
    expect_equal([e["name"] for e in policy["environments"]], [at_facts["environment"]], "Viewer environments")
    for scope in at_facts["writeScopes"] + at_facts["adminOnlyScopes"] + at_facts["unavailableScopes"]:
        listed = [s for s in policy["scopes"] if s["scope"] == scope]
        expect_equal(len(listed), 1, scope + " is in the Viewer catalogue")
        expect_equal(listed[0]["available"], False, scope + " is unavailable to the Viewer")
        expect(isinstance(listed[0]["unavailable_reason"], str) and listed[0]["unavailable_reason"],
               scope + " has a stated reason")
    # The compliance read scopes and compliance:preview (TASK-534.7) write
    # nothing and are selectable by the Viewer.
    for scope in at_facts["optionalReadScopes"]:
        listed = [s for s in policy["scopes"] if s["scope"] == scope]
        expect_equal(len(listed), 1, scope + " is in the Viewer catalogue")
        expect_equal(listed[0]["available"], True, scope + " is selectable by the Viewer")
    for scope in at_facts["writeScopes"]:
        reason = [s["unavailable_reason"] for s in policy["scopes"] if s["scope"] == scope]
        expect_equal(reason, ["Requires Operator"], scope + " requires Operator for the Viewer")
    status, body = at_create("viewer", at_facts["deniedToken"], [at_env_id],
                             at_facts["readScopes"] + ["scans:request"])
    expect_equal(status, 400, "Viewer create with scans:request")
    at_assert_fields("viewer", body, ("scopes", "scans:request"), "Viewer scans:request")
    expect_equal(at_counts(), after_viewer, "Viewer scans:request rejection created nothing")

with case("access-token-owner-reads-hide-the-secret-and-other-users-and-bearer-are-refused"):
    operator_secret = at_secrets["operator"]
    path = "/api/v1/user/access-tokens/" + at_operator_token_id
    status, listing = http(controller, "GET", "/api/v1/user/access-tokens", session=at_jar("operator"))
    expect_equal(status, 200, "Operator lists tokens")
    expect_equal([item["id"] for item in listing["items"]], [at_operator_token_id],
                 "the list holds exactly the Operator's token")
    expect(all("token" not in item for item in listing["items"]), "list items have no token field")
    expect(operator_secret not in json.dumps(listing), "list response omits the plaintext secret")
    status, detail = http(controller, "GET", path, session=at_jar("operator"))
    expect_equal(status, 200, "Operator reads the token detail")
    expect("token" not in detail, "detail has no token field")
    expect(operator_secret not in json.dumps(detail), "detail response omits the plaintext secret")
    expect_equal(detail["effective"], {
        "environments": [{"id": at_env_id, "name": at_facts["environment"], "available": True}],
        "scopes": [{"scope": s, "permitted": True} for s in at_read_scopes]},
        "effective access equals the grant")
    expect_equal([e["action"] for e in detail["recent_events"]], ["access_token_created"], "recent events")

    # Another user's session sees 404 and changes nothing.
    before_rows, before_counts = at_rows("operator"), at_counts()
    for method, suffix, csrf in (("GET", "", False), ("DELETE", "", True), ("POST", "/rotate", True)):
        status, body = http(controller, method, path + suffix, session=at_jar("viewer"), csrf=csrf)
        expect_equal(status, 404, "Viewer %s of the Operator's token" % method)
        expect_equal(body["error"], "resource_not_found", "Viewer %s error code" % method)
    expect_equal(at_rows("operator"), before_rows, "foreign requests left the Operator's token unchanged")
    expect_equal(at_counts(), before_counts, "foreign rotate created nothing")
    status, viewer_listing = http(controller, "GET", "/api/v1/user/access-tokens", session=at_jar("viewer"))
    expect_equal(status, 200, "Viewer lists tokens")
    expect_equal([i["id"] for i in viewer_listing["items"]], [at_viewer_token_id],
                 "the Viewer's list excludes the Operator's token")

    # A bearer credential never authenticates the management API, with or
    # without a valid session cookie.
    for jar in (None, at_jar("operator")):
        status, headers, body = at_bearer_get(operator_secret, jar)
        expect_equal(status, 401, "bearer token %s" % ("with a cookie" if jar else "alone"))
        expect_equal(body["error"], "unauthorized", "bearer rejection error code")
        expect("cache-control: no-store" in headers, "the 401 carries Cache-Control: no-store")
    expect_equal(at_rows("operator"), before_rows, "bearer attempts did not mark the token as used")
    receipt("accessTokens.denied", {"foreignGetDeleteRotate": 404, "bearerAlone": 401, "bearerWithCookie": 401})

with case("access-token-created-events-appear-in-the-admin-audit-log"):
    status, page = http(controller, "GET", "/api/v1/admin/audit-events?action=access_token_created&per_page=200",
                        session=authz_admin_jar)
    expect_equal(status, 200, "Admin reads the audit log")
    expect(all(e["action"] == "access_token_created" for e in page["items"]), "action filter")
    expect_equal(page["total"], 2, "two created events so far")
    by_target = {e["target"]: e for e in page["items"]}
    expect_equal(sorted(by_target), sorted([at_simple(at_operator_token_id), at_simple(at_viewer_token_id)]),
                 "audit targets are the public token ids")
    expect_equal(by_target[at_simple(at_operator_token_id)]["actor"], at_user_id("operator"), "Operator actor")
    expect_equal(by_target[at_simple(at_viewer_token_id)]["actor"], at_user_id("viewer"), "Viewer actor")
    expect(all(s not in json.dumps(page) for s in at_secrets.values()), "audit API omits every secret")

with case("access-token-browser-create-reveals-secret-once-and-persists-only-its-hash"):
    before_rows, before_viewer = at_rows("operator"), at_rows("viewer")
    before_counts, before_events = at_counts(), at_audit()
    at_browser("create")
    expect_equal(receipts()["accessTokens.browser.create.userId"], at_user_id("operator"),
                 "browser identity is the Operator")
    fingerprint = receipts()["accessTokens.browser.create.secretSha256"]
    at_browser_token_id = receipts()["accessTokens.browser.createdId"]
    rows = at_rows("operator")
    known = {r["id"] for r in before_rows}
    created_rows = [r for r in rows if r["id"] not in known]
    expect_equal(len(created_rows), 1, "the UI created exactly one token")
    row = created_rows[0]
    expect_equal(row["id"], at_browser_token_id, "persisted id is the one the create response returned")
    expect_equal({k: row[k] for k in ("name", "audience", "scopes", "environments", "revoked", "never_used", "hash_bytes")},
                 {"name": at_facts["browserToken"], "audience": "mcp", "scopes": at_read_scopes,
                  "environments": [at_env_id], "revoked": False, "never_used": True, "hash_bytes": 32},
                 "browser-created token grant and state")
    expect(abs(row["lifetime_seconds"] - at_facts["lifetimeDays"] * 86400) <= 5, "default lifetime")
    # The secret shown in the browser is the stored credential: its SHA-256
    # fingerprint, recorded without the secret, equals the persisted hash.
    expect_equal(row["hash_hex"], fingerprint, "stored hash equals the fingerprint of the displayed secret")
    expect_equal([r for r in rows if r["id"] != row["id"]], before_rows, "earlier Operator tokens are unchanged")
    expect_equal(at_rows("viewer"), before_viewer, "the Viewer's token is unchanged")
    expect_equal(at_counts(), at_counts_plus(before_counts, tokens=1, scopes=3, environments=1, audit=1),
                 "UI create added exactly one token and one audit event")
    new_events = at_audit()[len(before_events):]
    expect_equal(len(new_events), 1, "one new audit event")
    expect_equal({k: new_events[0][k] for k in ("action", "target", "actor_user_id")},
                 {"action": "access_token_created", "target": at_simple(row["id"]),
                  "actor_user_id": at_user_id("operator")}, "UI create audit event")
    receipt("accessTokens.sql.browserCreate", {"hashMatchesDisplayedSecretFingerprint": True, "auditEvents": 1})

with case("access-token-browser-rotate-revokes-old-and-reveals-replacement"):
    before_rows, before_counts, before_events = at_rows("operator"), at_counts(), at_audit()
    at_browser("rotate")
    expect_equal(receipts()["accessTokens.browser.rotate.userId"], at_user_id("operator"),
                 "browser identity is the Operator")
    at_replacement_id = receipts()["accessTokens.browser.rotate.newId"]
    rotate_fingerprint = receipts()["accessTokens.browser.rotate.secretSha256"]
    rows = {r["id"]: r for r in at_rows("operator")}
    expect_equal(len(rows), len(before_rows) + 1, "rotation added exactly one token row")
    old, new = rows[at_browser_token_id], rows[at_replacement_id]
    expect_equal({k: old[k] for k in ("revoked", "revoked_by")},
                 {"revoked": True, "revoked_by": at_user_id("operator")}, "original token is revoked")
    expect_equal(old["hash_hex"], receipts()["accessTokens.browser.create.secretSha256"],
                 "revocation did not change the original credential")
    expect_equal(new["revoked"], False, "replacement is active")
    expect_equal(new["hash_hex"], rotate_fingerprint,
                 "replacement hash equals the fingerprint of the rotated secret shown in the browser")
    expect(new["hash_hex"] != old["hash_hex"], "replacement has a new credential")
    expect_equal({k: new[k] for k in ("name", "audience", "scopes", "environments", "hash_bytes", "never_used")},
                 {k: old[k] for k in ("name", "audience", "scopes", "environments", "hash_bytes", "never_used")},
                 "replacement keeps name, audience and grant")
    expect(abs(new["lifetime_seconds"] - old["lifetime_seconds"]) <= 5, "replacement keeps the lifetime")
    expect_equal([r for i, r in rows.items() if i not in (at_browser_token_id, at_replacement_id)],
                 [r for r in before_rows if r["id"] != at_browser_token_id],
                 "other Operator tokens are unchanged")
    expect_equal(at_counts(), at_counts_plus(before_counts, tokens=1, scopes=3, environments=1, audit=1),
                 "rotation added one token and exactly one audit event")
    new_events = at_audit()[len(before_events):]
    expect_equal(len(new_events), 1, "rotation wrote only the rotated event, no separate revoked event")
    expect_equal({k: new_events[0][k] for k in ("action", "target", "actor_user_id")},
                 {"action": "access_token_rotated", "target": at_simple(at_replacement_id),
                  "actor_user_id": at_user_id("operator")}, "rotation audit event")
    expect_equal({k: new_events[0]["metadata"].get(k) for k in ("replaced_token_id", "replacement_token_id")},
                 {"replaced_token_id": at_simple(at_browser_token_id),
                  "replacement_token_id": at_simple(at_replacement_id)}, "rotation audit metadata")
    receipt("accessTokens.sql.browserRotate", {"oldRevoked": True, "replacementActive": True,
                                               "hashMatchesRotatedSecretFingerprint": True, "auditEvents": 1})

with case("access-token-browser-revoke-persists-and-lists-revoked"):
    before_rows, before_counts, before_events = at_rows("operator"), at_counts(), at_audit()
    before_viewer = at_rows("viewer")
    expect_equal({r["id"]: r["revoked"] for r in before_rows if r["name"] == at_facts["browserToken"]},
                 {at_browser_token_id: True, at_replacement_id: False}, "replacement is active before the revoke")
    at_browser("revoke")
    expect_equal(receipts()["accessTokens.browser.revoke.userId"], at_user_id("operator"),
                 "browser identity is the Operator")
    rows = {r["id"]: r for r in at_rows("operator")}
    expect_equal(len(rows), len(before_rows), "revoke added no token row")
    expect_equal({k: rows[at_replacement_id][k] for k in ("revoked", "revoked_by", "hash_hex")},
                 {"revoked": True, "revoked_by": at_user_id("operator"),
                  "hash_hex": receipts()["accessTokens.browser.rotate.secretSha256"]},
                 "the replacement is revoked and keeps its credential")
    expect_equal([r for i, r in rows.items() if i != at_replacement_id],
                 [r for r in before_rows if r["id"] != at_replacement_id],
                 "revoke changed only the selected token")
    expect_equal(rows[at_operator_token_id]["revoked"], False, "the API-created token is still active")
    expect_equal(at_rows("viewer"), before_viewer, "the Viewer's token is unchanged")
    expect_equal(at_counts(), at_counts_plus(before_counts, audit=1), "revoke wrote exactly one audit event")
    new_events = at_audit()[len(before_events):]
    expect_equal([{k: e[k] for k in ("action", "target", "actor_user_id")} for e in new_events],
                 [{"action": "access_token_revoked", "target": at_simple(at_replacement_id),
                   "actor_user_id": at_user_id("operator")}], "revoke audit event")
    receipt("accessTokens.sql.browserRevoke", {"replacementRevoked": True, "auditEvents": 1})

with case("access-token-admin-audit-log-lists-created-rotated-and-revoked"):
    for action, expected_targets in (
        ("access_token_created", [at_operator_token_id, at_viewer_token_id, at_browser_token_id]),
        ("access_token_rotated", [at_replacement_id]),
        ("access_token_revoked", [at_replacement_id]),
    ):
        status, page = http(controller, "GET", "/api/v1/admin/audit-events?action=%s&per_page=200" % action,
                            session=authz_admin_jar)
        expect_equal(status, 200, "Admin reads %s events" % action)
        expect_equal(sorted(e["target"] for e in page["items"]),
                     sorted(at_simple(i) for i in expected_targets), action + " targets")
        expect(all(e["action"] == action for e in page["items"]), action + " filter")
        expect(all(s not in json.dumps(page) for s in at_secrets.values()), "audit API omits every secret")
    receipt("accessTokens.audit.adminLog", {"created": 3, "rotated": 1, "revoked": 1})
