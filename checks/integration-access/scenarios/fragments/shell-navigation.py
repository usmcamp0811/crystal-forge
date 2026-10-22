# Compose after Access23 in the same local-auth lifecycle. All browser names
# below map to shell-navigation.cjs; each invocation gets a fresh context.
# Existing inputs: FACTS.admin, admin.userId and the standard driver helpers.
# TASK-490.4 requires Tab/Enter, narrow menu/Close menu and sidebar persistence.
# Run these assertions directly; no fact can disable an obligation. The visual
# comparison remains an unresolved handoff item until its fixed threshold and
# exact crop have a justified decision. This fragment supplies no visual proof.


def shell_browser(phase, viewport="desktop"):
    receipt("shell.request." + phase, True)
    run_browser(controller, "shell-" + phase, viewport=viewport)
    expect_equal(receipts()["shell.browser." + phase], True, phase + " browser completed")


with case("shell-protected-routes-redirect-without-session"):
    shell_browser("protected")

with case("shell-primary-navigation-and-profile-reach-headings"):
    shell_browser("routes")
    shell_browser("keyboard")
    shell_browser("mobile", "narrow")
    shell_browser("sidebarPersistence")
    expect_equal(sql_json(controller,
        "SELECT sidebar_collapsed FROM user_preferences WHERE user_id = '%s'::uuid"
        % receipts()["admin.userId"]), [{"sidebar_collapsed": True}],
        "sidebar UI action persisted for this Admin")
    receipt("shell.sql.sidebarCollapsed", True)

with case("shell-unknown-route-returns-to-dashboard"):
    shell_browser("unknown")
