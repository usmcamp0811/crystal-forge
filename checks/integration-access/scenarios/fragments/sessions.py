# Session protection and sign-out cases; concatenate after bootstrap completes.
# Requires bootstrap's controller, admin, case/HTTP/SQL/browser/assertion helpers,
# receipt/receipts helpers, and persisted admin.userId receipt in this same run.
# Bootstrap's fresh-user/one-session assertions must precede these new sessions.
# Authority: the owner's explicit session-proof request.
# Cookie jars belong to this run; only generated IDs leave through receipts.

session_user_id = receipts()["admin.userId"]


def session_rows():
    # last_seen_at can change on authentication; compare only lifecycle fields
    # that these requests are required to leave unchanged.
    return sql_json(
        controller,
        "SELECT id::text, user_id::text, expires_at::text, "
        "invalidated_at::text FROM user_sessions "
        "WHERE user_id = '%s'::uuid ORDER BY id" % session_user_id,
    )


def sign_in_session(jar, key):
    before = session_rows()
    status, _ = http(controller, "POST", "/api/auth/local/login",
                     {"username": admin["username"], "password": admin["password"]},
                     session=jar)
    expect_equal(status, 200, "real session sign-in")
    status, body = http(controller, "GET", "/api/v1/user/sessions", session=jar)
    expect_equal(status, 200, "list authenticated sessions")
    current = [s for s in body["sessions"] if s["current"]]
    expect_equal(len(current), 1, "one current session for the named cookie jar")
    session_id = receipt(key, current[0]["id"])
    added = [r for r in session_rows() if r["id"] not in {r["id"] for r in before}]
    expect_equal(len(added), 1, "one persisted session created by this login")
    expect_equal(added[0]["id"], session_id, "current session vs login-created DB row")
    expect_equal(added[0]["invalidated_at"], None, "new session active")
    return session_id


with case("session-missing-csrf-rejects-revocation"):
    retained_id = sign_in_session("session-retained", "sessions.retainedId")
    revoked_id = sign_in_session("session-second", "sessions.revokedId")
    expect(retained_id != revoked_id, "two logins reused a session identity")
    revoke_path = "/api/v1/user/sessions/" + revoked_id
    before_revoke = session_rows()
    status, _ = http(controller, "DELETE", revoke_path, session="session-retained")
    expect_equal(status, 403, "revocation without CSRF")
    expect_equal(session_rows(), before_revoke, "missing-CSRF protected session rows")

with case("session-mismatched-csrf-rejects-revocation"):
    # The real session/CSRF cookies stay in their named jar. Send an authored
    # wrong header without exposing either cookie in diagnostics or receipts.
    status = controller.succeed(
        "curl -sS -o /tmp/cf-it/mismatched-csrf-body -w '%{http_code}' "
        "-X DELETE -b /tmp/cf-it/jar-session-retained.txt "
        "-H 'x-csrf-token: deliberately-mismatched-token' "
        "http://localhost:3000" + revoke_path
    )
    expect_equal(int(status.strip()), 403, "revocation with mismatched CSRF")
    expect_equal(session_rows(), before_revoke, "mismatched-CSRF protected session rows")

with case("session-valid-csrf-persists-selected-revocation"):
    status, _ = http(controller, "DELETE", revoke_path,
                     session="session-retained", csrf=True)
    expect_equal(status, 204, "valid-CSRF revocation")
    after_revoke = session_rows()
    expect_equal([r["id"] for r in after_revoke], [r["id"] for r in before_revoke],
                 "revocation creates/deletes no session rows")
    for before, after in zip(before_revoke, after_revoke):
        if before["id"] == revoked_id:
            expect(after["invalidated_at"] is not None, "selected session was not invalidated")
            expect_equal({k: v for k, v in after.items() if k != "invalidated_at"},
                         {k: v for k, v in before.items() if k != "invalidated_at"},
                         "revoked session identity and expiry")
        else:
            expect_equal(after, before, "non-selected session lifecycle unchanged")
    receipt("sessions.revocationPersisted", True)

with case("session-revoked-request-denied-retained-session-valid"):
    status, _ = http(controller, "GET", "/api/v1/user/sessions", session="session-second")
    expect_equal(status, 401, "revoked session's next authenticated operation")
    status, body = http(controller, "GET", "/api/v1/user/sessions", session="session-retained")
    expect_equal(status, 200, "retained session remains usable")
    expect_equal([s["id"] for s in body["sessions"] if s["current"]],
                 [retained_id], "retained identity")
    expect_equal(session_rows(), after_revoke, "rejected operation has no lifecycle effect")

with case("session-browser-sign-out-invalidates-exact-session"):
    before_browser = session_rows()
    run_browser(controller, "session-sign-out")
    browser_id = receipts()["sessions.browserSignOutId"]
    after_browser = session_rows()
    added = [r for r in after_browser if r["id"] not in {r["id"] for r in before_browser}]
    expect_equal(len(added), 1, "one new browser-context session")
    expect_equal(added[0]["id"], browser_id, "signed-out browser's exact row")
    expect(added[0]["invalidated_at"] is not None, "UI sign-out did not invalidate its row")
    expect_equal([r for r in after_browser if r["id"] != browser_id], before_browser,
                 "UI sign-out leaves other session lifecycle rows unchanged")
    receipt("sessions.browserSignOutPersisted", True)
