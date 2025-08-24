import json

import pytest

from cf_test import CFTestClient
from cf_test.scenarios import (
    scenario_behind,
    scenario_eval_failed,
    scenario_never_seen,
    scenario_offline,
    scenario_up_to_date,
)

VIEW_STATUS = "public.view_systems_status_table"
VIEW_CURRENT = "public.view_systems_current_state"


@pytest.mark.views
@pytest.mark.database
@pytest.mark.parametrize(
    "build,hostname,expect",
    [
        (
            scenario_never_seen,
            "test-never-seen",
            {"overall": None, "connectivity": None, "update": None},
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

    if build.__name__ == "scenario_never_seen":
        # should not appear in status table
        status_rows = cf_client.execute_sql(
            f"SELECT 1 FROM {VIEW_STATUS} WHERE hostname=%s", (host,)
        )
        assert not status_rows
        # but should be present in current-state view
        current = cf_client.execute_sql(
            f"SELECT hostname FROM {VIEW_CURRENT} WHERE hostname=%s", (host,)
        )
        assert current and current[0]["hostname"] == host
    else:
        row = cf_client.execute_sql(
            f"""
            SELECT hostname, connectivity_status, update_status, overall_status,
                   current_derivation_path, latest_derivation_path, latest_commit_hash
            FROM {VIEW_STATUS}
            WHERE hostname = %s
            """,
            (host,),
        )
        assert row and len(row) == 1
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
