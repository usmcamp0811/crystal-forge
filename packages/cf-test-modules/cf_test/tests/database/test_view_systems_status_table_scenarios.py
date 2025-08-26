import json

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import (
    scenario_behind,
    scenario_eval_failed,
    scenario_latest_with_two_overdue,
    scenario_mixed_commit_lag,
    scenario_never_seen,
    scenario_offline,
    scenario_up_to_date,
)

VIEW_STATUS = "public.view_systems_status_table"


@pytest.mark.vm_internal
@pytest.mark.views
@pytest.mark.database
@pytest.mark.parametrize(
    "build,hostname,expect",
    [
        (
            scenario_never_seen,
            "test-never-seen",
            {
                "overall": "never_seen",
                "connectivity": "never_seen",
                "update": "never_seen",
            },
        ),
        (
            scenario_up_to_date,
            "test-uptodate",
            {"overall": "up_to_date", "connectivity": "online", "update": "up_to_date"},
        ),
        (
            scenario_behind,
            "test-behind",
            {"overall": "behind", "connectivity": "online", "update": "behind"},
        ),
        (
            scenario_offline,
            "test-offline",
            {"overall": "offline", "connectivity": "offline", "update": None},
        ),
        (
            scenario_eval_failed,
            "test-eval-failed",
            {
                "overall": "evaluation_failed",
                "connectivity": "online",
                "update": "evaluation_failed",
            },
        ),
    ],
)
def test_status_scenarios(cf_client: CFTestClient, build, hostname, expect):
    data = build(cf_client, hostname=hostname)
    host = data.get("hostname", hostname)

    row = cf_client.execute_sql(
        f"""
        SELECT hostname, connectivity_status, update_status, overall_status,
               current_derivation_path, latest_derivation_path, latest_commit_hash
        FROM {VIEW_STATUS}
        WHERE hostname = %s
        """,
        (host,),
    )
    assert row and len(row) == 1, "expected exactly one row"
    r = row[0]

    if expect["overall"] is not None:
        assert r["overall_status"] == expect["overall"]
    if expect["connectivity"] is not None:
        assert r["connectivity_status"] == expect["connectivity"]
    if expect["update"] is not None:
        assert r["update_status"] == expect["update"]

    cf_client.save_artifact(
        json.dumps(r, indent=2, default=str),
        f"{host}_scenario_result.json",
        f"{host} scenario result",
    )

    cf_client.cleanup_test_data(data["cleanup"])


@pytest.fixture(scope="session")
def cf_config():
    return CFTestConfig()


@pytest.fixture(scope="session")
def cf_client(cf_config):
    client = CFTestClient(cf_config)
    client.execute_sql("SELECT 1")
    return client


@pytest.mark.views
@pytest.mark.database
def test_view_latest_with_two_overdue(cf_client: CFTestClient):
    res = scenario_latest_with_two_overdue(cf_client)
    try:
        rows = cf_client.execute_sql(
            """
            SELECT hostname, last_seen, current_derivation_path
            FROM view_systems_status_table
            WHERE hostname = ANY(%s)
            """,
            (res["hostnames"],),
        )
        assert len(rows) == 9

        # Exactly 2 overdue: last_seen <= now - 60 minutes
        [overdue] = cf_client.execute_sql(
            """
            SELECT COUNT(*) AS n
            FROM view_systems_status_table
            WHERE hostname = ANY(%s)
              AND (last_seen::timestamptz) <= (CURRENT_TIMESTAMP - INTERVAL '60 minutes')
            """,
            (res["hostnames"],),
        )
        assert overdue["n"] == 2

        # Everyone on latest commit (compare to latest derivation for this flake)
        [latest_drv_row] = cf_client.execute_sql(
            """
            SELECT d.derivation_path
            FROM commits c
            JOIN derivations d ON d.commit_id = c.id
            WHERE c.flake_id = %s
            ORDER BY c.commit_timestamp DESC
            LIMIT 1
            """,
            (res["flake_id"],),
        )
        [mismatch] = cf_client.execute_sql(
            """
            SELECT COUNT(*) AS n
            FROM view_systems_status_table
            WHERE hostname = ANY(%s)
              AND current_derivation_path <> %s
            """,
            (res["hostnames"], latest_drv_row["derivation_path"]),
        )
        assert mismatch["n"] == 0
    finally:
        res["cleanup_fn"]()


@pytest.mark.views
@pytest.mark.database
def test_view_mixed_commit_lag(cf_client: CFTestClient):
    res = scenario_mixed_commit_lag(cf_client)
    try:
        rows = cf_client.execute_sql(
            """
            SELECT hostname, last_seen, current_derivation_path
            FROM view_systems_status_table
            WHERE hostname = ANY(%s)
            """,
            (res["hostnames"],),
        )
        assert len(rows) == 4

        # All heartbeats are recent (<= 15 minutes): last_seen >= now - 15 minutes
        [recent] = cf_client.execute_sql(
            """
            SELECT COUNT(*) AS n
            FROM view_systems_status_table
            WHERE hostname = ANY(%s)
              AND (last_seen::timestamptz) >= (CURRENT_TIMESTAMP - INTERVAL '15 minutes')
            """,
            (res["hostnames"],),
        )
        assert recent["n"] == 4

        # Derivation paths ordered newest → oldest for this flake
        drv_rows = cf_client.execute_sql(
            """
            SELECT d.derivation_path
            FROM commits c
            JOIN derivations d ON d.commit_id = c.id
            WHERE c.flake_id = %s
            ORDER BY c.commit_timestamp DESC
            """,
            (res["flake_id"],),
        )
        assert len(drv_rows) >= 4

        latest = drv_rows[0]["derivation_path"]
        prev = drv_rows[1]["derivation_path"]
        third = drv_rows[3]["derivation_path"]  # 3 commits behind

        [counts] = cf_client.execute_sql(
            """
            SELECT
              SUM(CASE WHEN current_derivation_path = %s THEN 1 ELSE 0 END) AS on_latest,
              SUM(CASE WHEN current_derivation_path = %s THEN 1 ELSE 0 END) AS on_prev,
              SUM(CASE WHEN current_derivation_path = %s THEN 1 ELSE 0 END) AS on_third
            FROM view_systems_status_table
            WHERE hostname = ANY(%s)
            """,
            (latest, prev, third, res["hostnames"]),
        )
        assert counts["on_latest"] == 1
        assert counts["on_prev"] == 1
        assert counts["on_third"] == 2
    finally:
        res["cleanup_fn"]()
