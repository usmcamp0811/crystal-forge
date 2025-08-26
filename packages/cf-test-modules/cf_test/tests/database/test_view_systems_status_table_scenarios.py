import json
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any, Dict, List, Optional

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import (
    scenario_behind,
    scenario_eval_failed,
    scenario_flake_time_series,
    scenario_latest_with_two_overdue,
    scenario_mixed_commit_lag,
    scenario_never_seen,
    scenario_offline,
    scenario_up_to_date,
)

VIEW_STATUS = "view_systems_status_table"

SCENARIO_CONFIGS = [
    {
        "id": "never_seen",
        "builder": scenario_never_seen,
        "expected": [
            {
                "hostname": "test-never-seen",
                "connectivity_status": "never_seen",
                "update_status": "never_seen",
                "overall_status": "never_seen",
            }
        ],
    },
    {
        "id": "up_to_date",
        "builder": scenario_up_to_date,
        "expected": [
            {
                "hostname": "test-uptodate",
                "connectivity_status": "online",
                "update_status": "up_to_date",
                "overall_status": "up_to_date",
            }
        ],
    },
    {
        "id": "behind",
        "builder": scenario_behind,
        "expected": [
            {
                "hostname": "test-behind",
                "connectivity_status": "online",
                "update_status": "behind",
                "overall_status": "behind",
            }
        ],
    },
    {
        "id": "offline",
        "builder": scenario_offline,
        "expected": [
            {
                "hostname": "test-offline",
                "connectivity_status": "offline",
                "update_status": None,  # offline systems don't get update status
                "overall_status": "offline",
            }
        ],
    },
    {
        "id": "eval_failed",
        "builder": scenario_eval_failed,
        "expected": [
            {
                "hostname": "test-eval-failed",
                "connectivity_status": "online",
                "update_status": "evaluation_failed",
                "overall_status": "evaluation_failed",
            }
        ],
    },
    {
        "id": "latest_with_two_overdue",
        "builder": lambda client: scenario_latest_with_two_overdue(client),
        "expected": {
            "count": 9,
            "connectivity_counts": {"offline": 2, "online": 7},
            "update_counts": {"up_to_date": 7},  # only count ONLINE rows
        },
    },
    {
        "id": "mixed_commit_lag",
        "builder": scenario_mixed_commit_lag,
        "expected": {
            "count": 4,
            "connectivity_counts": {"online": 3, "offline": 1},
            "update_counts": {"up_to_date": 2, "behind": 1},  # ONLINE only
        },
    },
    {
        "id": "flake_time_series",
        "builder": lambda client: scenario_flake_time_series(
            client, days=7, heartbeat_interval_minutes=15, heartbeat_hours=24
        ),
        "expected": None,  # smoke test only
    },
]


def _get_hostnames_from_scenario(
    scenario_data: Dict[str, Any], scenario_id: str
) -> List[str]:
    """Extract hostnames from scenario data"""
    if "hostname" in scenario_data:
        return [scenario_data["hostname"]]
    elif "hostnames" in scenario_data:
        return scenario_data["hostnames"]
    else:
        # Pattern fallback for generated hostnames
        if scenario_id == "latest_with_two_overdue":
            return [f"test-latest-{i+1}" for i in range(9)]
        elif scenario_id == "mixed_commit_lag":
            return [f"test-mixed-{i+1}" for i in range(4)]
        elif scenario_id == "flake_time_series":
            return [f"test-timeseries-{i+1}" for i in range(5)]
        return []


@pytest.fixture(scope="session")
def cf_config():
    return CFTestConfig()


@pytest.fixture(scope="session")
def cf_client(cf_config):
    client = CFTestClient(cf_config)
    client.execute_sql("SELECT 1")
    return client


@pytest.mark.vm_internal
@pytest.mark.views
@pytest.mark.database
@pytest.mark.parametrize("scenario_config", SCENARIO_CONFIGS, ids=lambda x: x["id"])
def test_all_scenarios(
    cf_client: CFTestClient, clean_test_data, scenario_config: Dict[str, Any]
):
    """Unified test for all scenarios using actual view columns"""
    builder = scenario_config["builder"]
    expected = scenario_config["expected"]
    scenario_id = scenario_config["id"]

    # Build the scenario
    scenario_data = builder(cf_client)

    # Determine hostnames to fetch from the view
    hostnames = _get_hostnames_from_scenario(scenario_data, scenario_id)

    # Query the view for rows that match the scenario systems
    if hostnames:
        rows = cf_client.execute_sql(
            f"""
            SELECT hostname, connectivity_status, connectivity_status_text,
                   update_status, update_status_text, overall_status,
                   last_seen, agent_version, uptime, ip_address, os, kernel, nixos_version,
                   current_derivation_path, current_deployment_time,
                   latest_commit_hash, latest_commit_timestamp, latest_derivation_path,
                   latest_derivation_status, drift_hours
            FROM {VIEW_STATUS}
            WHERE hostname = ANY(%s)
            ORDER BY hostname
            """,
            (hostnames,),
        )
    else:
        # Pattern matching fallback
        if scenario_id == "latest_with_two_overdue":
            pattern = "test-latest-%"
        elif scenario_id == "mixed_commit_lag":
            pattern = "test-mixed-%"
        elif scenario_id == "flake_time_series":
            pattern = "test-timeseries-%"
        else:
            pattern = f"{scenario_id}-%"

        rows = cf_client.execute_sql(
            f"""
            SELECT hostname, connectivity_status, connectivity_status_text,
                   update_status, update_status_text, overall_status,
                   last_seen, agent_version, uptime, ip_address, os, kernel, nixos_version,
                   current_derivation_path, current_deployment_time,
                   latest_commit_hash, latest_commit_timestamp, latest_derivation_path,
                   latest_derivation_status, drift_hours
            FROM {VIEW_STATUS}
            WHERE hostname LIKE %s
            ORDER BY hostname
            """,
            (pattern,),
        )

    # Save results for debugging without relying on CFTestClient helpers
    try:
        Path("/tmp/cf_view_scenario_results.json").write_text(
            json.dumps({"scenario": scenario_id, "rows": rows}, default=str, indent=2),
            encoding="utf-8",
        )
    except Exception:
        pass

    # Validate
    if expected is None:
        assert isinstance(rows, list)
    elif isinstance(expected, list):
        assert len(rows) == len(expected), (
            f"Expected {len(expected)} rows, got {len(rows)} "
            f"for scenario {scenario_id}: {[r['hostname'] for r in rows]}"
        )
        for expected_system in expected:
            expected_hostname = expected_system["hostname"]
            matching_row = next(
                (row for row in rows if row["hostname"] == expected_hostname), None
            )
            assert matching_row is not None, f"No result found for {expected_hostname}"
            for field, expected_value in expected_system.items():
                if field == "hostname":
                    continue
                actual_value = matching_row.get(field)
                assert actual_value == expected_value, (
                    f"Field mismatch for {expected_hostname}.{field}: "
                    f"expected '{expected_value}', got '{actual_value}'"
                )
    elif isinstance(expected, dict):
        if "count" in expected:
            assert (
                len(rows) == expected["count"]
            ), f"Expected {expected['count']} systems, got {len(rows)} for {scenario_id}"

        if "connectivity_counts" in expected:
            actual_connectivity_counts: Dict[str, int] = {}
            for row in rows:
                status = row["connectivity_status"]
                actual_connectivity_counts[status] = (
                    actual_connectivity_counts.get(status, 0) + 1
                )
            for status, expected_count in expected["connectivity_counts"].items():
                actual_count = actual_connectivity_counts.get(status, 0)
                assert actual_count == expected_count, (
                    f"Expected {expected_count} with connectivity_status='{status}', "
                    f"got {actual_count} for {scenario_id}"
                )

        if "update_counts" in expected:
            # Count update status for ONLINE systems only
            actual_update_counts: Dict[Optional[str], int] = {}
            for row in rows:
                if row["connectivity_status"] != "online":
                    continue
                status = row["update_status"]
                actual_update_counts[status] = actual_update_counts.get(status, 0) + 1
            for status, expected_count in expected["update_counts"].items():
                actual_count = actual_update_counts.get(status, 0)
                assert actual_count == expected_count, (
                    f"Expected {expected_count} with update_status='{status}', "
                    f"got {actual_count} for {scenario_id}"
                )


@pytest.mark.views
@pytest.mark.database
def test_view_basic_functionality(cf_client: CFTestClient):
    """Basic smoke test for the view"""
    result = cf_client.execute_sql(
        f"""
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW_STATUS,),
    )

    expected_columns = {
        "hostname",
        "connectivity_status",
        "connectivity_status_text",
        "update_status",
        "update_status_text",
        "overall_status",
        "last_seen",
        "agent_version",
        "uptime",
        "ip_address",
        "os",
        "kernel",
        "nixos_version",
        "current_derivation_path",
        "current_deployment_time",
        "latest_commit_hash",
        "latest_commit_timestamp",
        "latest_derivation_path",
        "latest_derivation_status",
        "drift_hours",
    }
    actual_columns = {row["column_name"] for row in result}
    assert expected_columns.issubset(
        actual_columns
    ), f"View missing expected columns. Missing: {expected_columns - actual_columns}"


@pytest.mark.views
@pytest.mark.database
def test_view_performance(cf_client: CFTestClient):
    """Test that view performs reasonably well"""
    import time

    start_time = time.time()
    result = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW_STATUS}")
    query_time = time.time() - start_time

    assert query_time < 5.0, f"View query took too long: {query_time:.2f} seconds"
    assert len(result) == 1
