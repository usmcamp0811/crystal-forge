# access.lifecycle.local-bootstrap scenario body. See the .nix file for the
# actor, starting state, action, outcome, and negative condition.
# Helpers (case, expect, http, sql_json, run_browser, receipt) come from
# lib/integration-test/driver.py.
#
# Expected: with the normal NixOS module and `server.auth_mode = "local"`,
# the server starts, applies migrations, and exposes local setup-status.
# Current observed result (coverage map): startup fails before migrations
# because the generated socket host produces an empty-URI-host error. The
# generated config also omits auth_mode and AUTH_MODE is unset, so no local
# auth response is observed. Keep the assertions and do not add an override.

admin = FACTS["admin"]

start_all()

with case("fresh-startup-applies-migrations"):
    controller.wait_for_unit("postgresql.service", timeout=180)
    controller.wait_until_succeeds(
        "test -s /var/lib/crystal-forge/config.toml", timeout=60
    )
    controller.succeed(
        r'''mkdir -p /tmp/cf-it/diag && {
          awk '
            /^\[database\]$/ { section = "database"; print; next }
            /^\[server\]$/ { section = "server"; print; next }
            /^\[/ { section = ""; next }
            section == "database" && /^(host|port|user|name) =/ { print }
            section == "database" && /^password =/ { print "password = \"[REDACTED]\"" }
            section == "server" && /^auth_mode =/ { print }
          ' /var/lib/crystal-forge/config.toml
          systemctl show crystal-forge-server.service -p Environment --value |
            tr ' ' '\n' |
            awk -F= '$1 == "AUTH_MODE" { print "systemd AUTH_MODE=" $2; found = 1 }
                     END { if (!found) print "systemd AUTH_MODE=unset" }'
        } > /tmp/cf-it/diag/generated-config-summary.txt'''
    )
    # Stop waiting soon after either successful activation or a recorded
    # service failure. The following wait remains the required startup
    # assertion; this bounded observation only avoids spending the full unit
    # timeout in a rapid systemd restart loop.
    controller.wait_until_succeeds(
        "systemctl is-active --quiet crystal-forge-server.service "
        "|| systemctl is-failed --quiet crystal-forge-server.service "
        "|| test \"$(systemctl show -p NRestarts --value "
        "crystal-forge-server.service)\" -gt 0",
        timeout=90,
    )
    controller.wait_for_unit("crystal-forge-server.service", timeout=5)
    controller.wait_for_open_port(3000, timeout=300)
    rows = sql_json(
        controller,
        "SELECT count(*)::int AS applied, coalesce(bool_and(success), false) AS ok FROM _sqlx_migrations",
    )
    expect_equal(rows[0]["applied"], FACTS["expectedMigrations"], "applied migrations")
    expect(rows[0]["ok"], "a migration is recorded as unsuccessful")
    receipt("migrations.applied", rows[0]["applied"])

with case("fresh-server-reports-local-setup"):
    status, body = http(controller, "GET", "/api/auth/setup-status")
    expect_equal(status, 200, "GET /api/auth/setup-status status")
    expect_equal(
        body.get("auth_mode"),
        FACTS["authMode"],
        'auth mode for services.crystal-forge.server.auth_mode = "local"',
    )
    expect_equal(body.get("requires_setup"), True, "requires_setup on a fresh database")
    expect_equal(body.get("user_count"), 0, "user_count on a fresh database")
    expect_equal(sql_json(controller, "SELECT count(*)::int AS n FROM users")[0]["n"], 0, "persisted users")

with case("browser-registers-and-signs-in-first-admin"):
    run_browser(controller, "bootstrap-sign-in")

with case("persisted-admin-and-session"):
    users = sql_json(
        controller,
        "SELECT u.id::text AS id, u.username, u.email, "
        "array_agg(r.role::text ORDER BY r.role::text) FILTER (WHERE r.role IS NOT NULL) AS roles "
        "FROM users u LEFT JOIN user_role_assignments r ON r.user_id = u.id GROUP BY u.id",
    )
    expect_equal(len(users), 1, "persisted users after bootstrap")
    user = users[0]
    expect_equal(user["username"], admin["username"], "persisted username")
    expect_equal(user["email"], admin["email"], "persisted email")
    expect_equal(user["roles"], [FACTS["adminRoleDb"]], "persisted role assignments")
    expect_equal(user["id"], receipts().get("admin.userId"), "whoami user id vs persisted user id")
    sessions = sql_json(
        controller,
        "SELECT id::text FROM user_sessions WHERE user_id = '%s'::uuid "
        "AND invalidated_at IS NULL AND expires_at > now()" % user["id"],
    )
    expect_equal(len(sessions), 1, "active sessions after one sign-in and one reload")
    status, body = http(controller, "GET", "/api/auth/setup-status")
    expect_equal(status, 200, "GET /api/auth/setup-status status")
    expect_equal(body.get("requires_setup"), False, "requires_setup after bootstrap")
    expect_equal(body.get("user_count"), 1, "user_count after bootstrap")

with case("wrong-password-has-no-session-effect"):
    status, _ = http(
        controller,
        "POST",
        "/api/auth/local/login",
        {"username": admin["username"], "password": admin["password"] + "-wrong"},
        session="wrong-password",
    )
    expect_equal(status, 401, "POST /api/auth/local/login with a wrong password")
    count = sql_json(
        controller,
        "SELECT count(*)::int AS n FROM user_sessions WHERE invalidated_at IS NULL",
    )[0]["n"]
    expect_equal(count, 1, "active sessions after a rejected sign-in")
    jar_has_session, _ = controller.execute(
        "grep -q '__Host-cf-session' /tmp/cf-it/jar-wrong-password.txt"
    )
    expect(jar_has_session != 0, "a rejected sign-in returned a session cookie")
