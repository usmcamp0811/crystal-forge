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


@pytest.mark.views
@pytest.mark.database
@pytest.mark.parametrize(
    "build,expect",
    [
        (
            scenario_never_seen,
            {
                "overall": "never_seen",
                "connectivity": "never_seen",
                "update": "never_seen",
            },
        ),
        (
            scenario_up_to_date,
            {"overall": "up_to_date", "connectivity": "online", "update": "up_to_date"},
        ),
        (
            scenario_behind,
            {"overall": "behind", "connectivity": "online", "update": "behind"},
        ),
        (
            scenario_offline,
            {"overall": "offline", "connectivity": "offline", "update": None},
        ),
        (
            scenario_eval_failed,
            {
                "overall": "evaluation_failed",
                "connectivity": "online",
                "update": "evaluation_failed",
            },
        ),
    ],
)
def test_status_scenarios(cf_client: CFTestClient, build, expect):
    data = build(cf_client)
    # hostname was embedded in scenario name
    host = (
        [
            k
            for k in [
                "test-never-seen",
                "test-uptodate",
                "test-behind",
                "test-offline",
                "test-eval-failed",
            ]
            if k in json.dumps(data)
        ].pop()
        if isinstance(data, dict)
        else None
    )

    row = cf_client.execute_sql(
        """
        SELECT hostname, connectivity_status, update_status, overall_status,
               current_derivation_path, latest_derivation_path, latest_commit_hash
        FROM view_systems_status_table
        WHERE hostname LIKE %s
    """,
        (f"{host}%",),
    )
    assert row, "expected one row"
    r = row[0]
    if expect["overall"]:
        assert r["overall_status"] == expect["overall"]
    if expect["connectivity"]:
        assert r["connectivity_status"] == expect["connectivity"]
    if expect["update"]:
        assert r["update_status"] == expect["update"]

    cf_client.save_artifact(
        json.dumps(r, indent=2, default=str),
        f"{host}_scenario_result.json",
        f"{host} scenario result",
    )
    cf_client.cleanup_test_data(data["cleanup"])
