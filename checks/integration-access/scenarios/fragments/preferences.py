# Compose after Access23 and shell-navigation.py. Uses existing Admin u1 and
# Operator u2, never Dana (admin-forms deliberately disables that account).
# Required facts.preferences:
#   appearance = {theme, density, sidebar_collapsed, default_systems_view}
#   notifications = {deploy_failures, build_failures, critical_cves,
#                    policy_violations, heartbeat_lost, weekly_digest,
#                    delivery_channel}
# Values are authored inputs, not copied from the producer. Local email-off
# composition must use weekly_digest=False and delivery_channel="in_app".
# Suggested changed targets: light/compact/True/table and
# False/False/False/False/True/False/in_app respectively.
# Existing receipts: admin.userId, authorization.operator.userId.
# Each preferences-<phase> browser name maps to preferences.cjs. The driver
# checks SQL before advancing. Browser contexts never share cookies/storage.
# TASK-490.4 requires real persistence and identity isolation. Transport
# failures exercise the concrete client error states without synthetic producer
# responses. Missing authored targets fail their case; they cannot skip it.

preferences_facts = FACTS["preferences"]
preferences_a = receipts()["admin.userId"]
preferences_b = receipts()["authorization.operator.userId"]
preferences_appearance_columns = "theme, density, sidebar_collapsed, default_systems_view"
preferences_notification_columns = (
    "deploy_failures, build_failures, critical_cves, policy_violations, "
    "heartbeat_lost, weekly_digest, delivery_channel"
)


def preferences_rows(table, user_id, columns="*"):
    # IDs originate only in same-run real creation receipts. Quote anyway so
    # a malformed receipt cannot turn this read-only query into another query.
    quoted_id = "'" + str(user_id).replace("'", "''") + "'::uuid"
    return sql_json(controller, "SELECT " + columns + " FROM " + table
                    + " WHERE user_id = " + quoted_id)


def preferences_snapshot(user_id):
    """Captures complete persisted preference rows, including timestamps."""
    return {
        "appearance": preferences_rows("user_preferences", user_id),
        "notifications": preferences_rows("user_notification_preferences", user_id),
    }


def preferences_browser(phase):
    receipt("preferences.request." + phase, True)
    run_browser(controller, "preferences-" + phase)
    expect_equal(receipts()["preferences.browser." + phase], True, phase + " browser completed")


def preferences_read_only_browser(phase):
    """Checks that a failed transport or delayed read changes no preference row."""
    before = preferences_snapshot(preferences_a)
    preferences_browser(phase)
    expect_equal(preferences_snapshot(preferences_a), before,
                 phase + " left all A preference fields unchanged")
    expect_equal(preferences_snapshot(preferences_b), preferences_b_before,
                 phase + " left all B preference fields unchanged")
    receipt("preferences.sql." + phase + ".unchanged", True)


with case("preferences-independent-user-a-baseline"):
    # Initialize A through the real UI before taking cross-feature snapshots.
    preferences_browser("baseline-a")
    expect_equal(len(preferences_rows("user_preferences", preferences_a)), 1,
                 "A appearance initialized through UI")


with case("preferences-independent-user-b-baseline"):
    # A first UI visit may legitimately initialize B's appearance row. Take the
    # baseline after that visit, before any A change. No B value is authored by
    # the test; subsequent SQL and independent UI reads must retain this state.
    preferences_browser("baseline-b")
    preferences_b_before = preferences_snapshot(preferences_b)
    expect_equal(len(preferences_b_before["appearance"]), 1, "B appearance initialized through UI")
    receipt("preferences.sql.userBBaseline", preferences_b_before)

with case("preferences-profile-appearance-persists-and-reloads-per-user"):
    preferences_a_before = preferences_rows("user_preferences", preferences_a,
                                          preferences_appearance_columns)
    expect(preferences_a_before != [preferences_facts["appearance"]],
           "authored appearance target must change A's current values")
    preferences_browser("appearance")
    expect_equal(preferences_rows("user_preferences", preferences_a, preferences_appearance_columns),
                 [preferences_facts["appearance"]], "exact authored appearance persisted")
    expect_equal(preferences_snapshot(preferences_b), preferences_b_before,
                 "A appearance saves left all B preference fields unchanged")
    receipt("preferences.sql.appearance", preferences_facts["appearance"])
    preferences_read_only_browser("appearance-read-failure")
    preferences_read_only_browser("appearance-save-failure")

with case("preferences-notification-settings-persist-and-reload-per-user"):
    preferences_a_appearance = preferences_rows("user_preferences", preferences_a)
    expect(preferences_rows("user_notification_preferences", preferences_a,
                            preferences_notification_columns) != [preferences_facts["notifications"]],
           "authored notification target must change A's persisted values")
    preferences_browser("notifications")
    expect_equal(preferences_rows("user_notification_preferences", preferences_a,
                                  preferences_notification_columns),
                 [preferences_facts["notifications"]], "exact authored notification settings persisted")
    expect_equal(preferences_rows("user_preferences", preferences_a), preferences_a_appearance,
                 "notification saves left A appearance unchanged")
    expect_equal(preferences_snapshot(preferences_b), preferences_b_before,
                 "A notification saves left all B preference fields unchanged")
    receipt("preferences.sql.notifications", preferences_facts["notifications"])
    preferences_read_only_browser("notification-read-failure")
    preferences_read_only_browser("notification-save-failure")
    preferences_read_only_browser("notification-loading")

with case("preferences-user-b-independent-reload-retains-baseline"):
    preferences_browser("verify-b")
    expect_equal(preferences_snapshot(preferences_b), preferences_b_before,
                 "B independent sign-in/reload left exact persisted baseline unchanged")
    receipt("preferences.sql.userBUnchanged", True)
