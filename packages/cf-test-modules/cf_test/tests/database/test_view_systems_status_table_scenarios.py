import json
from datetime import UTC, datetime, timedelta
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

VIEW_STATUS = "public.view_systems_status_table"

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
            "connectivity_counts": {"offline": 2, "online": 7},  # 2 overdue = offline
            "update_counts": {"up_to_date": 9},  # all on same latest commit
        },
    },
    {
        "id": "mixed_commit_lag",
        "builder": lambda client: scenario_mixed_commit_lag(client),
        "expected": {
            "count": 4,
            "connectivity_counts": {"online": 4},  # all have recent heartbeats
            "update_counts": {
                "up_to_date": 1,
                "behind": 3,
            },  # based on commit_lags (0,1,3,3)
        },
    },
    {
        "id": "flake_time_series",
        "builder": lambda client: scenario_flake_time_series(
            client,
            num_systems=5,
            num_commits=3,
            days=7,
            base_hostname="test-timeseries",
        ),
        "expected": {
            "count": 5,
            "connectivity_counts": {"online": 5},  # all should have recent heartbeats
            "update_counts": {"up_to_date": 5},  # all should be on latest
        },
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
        # Pattern matching fallback
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

    try:
        # Get hostnames to query
        hostnames = _get_hostnames_from_scenario(scenario_data, scenario_id)

        # Query the actual view columns
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

        # Save results for debugging
        cf_client.save_artifact(
            json.dumps(rows, indent=2, default=str),
            f"{scenario_id}_results.json",
            f"{scenario_id} view results",
        )

        # Handle different expectation formats
        if isinstance(expected, list):
            # Single system scenarios
            assert len(rows) == len(
                expected
            ), f"Expected {len(expected)} systems, got {len(rows)} for {scenario_id}"

            # Match each expected system with actual results
            for expected_system in expected:
                expected_hostname = expected_system["hostname"]

                # Find matching row
                matching_row = next(
                    (row for row in rows if row["hostname"] == expected_hostname), None
                )
                assert (
                    matching_row is not None
                ), f"No result found for hostname {expected_hostname}"

                # Check each expected field
                for field, expected_value in expected_system.items():
                    if field == "hostname":
                        continue

                    actual_value = matching_row.get(field)
                    assert actual_value == expected_value, (
                        f"Field mismatch for {expected_hostname}.{field}: "
                        f"expected '{expected_value}', got '{actual_value}'"
                    )

        elif isinstance(expected, dict):
            # Multi-system scenarios
            if "count" in expected:
                assert (
                    len(rows) == expected["count"]
                ), f"Expected {expected['count']} systems, got {len(rows)} for {scenario_id}"

            if "connectivity_counts" in expected:
                # Count systems by connectivity status
                actual_connectivity_counts = {}
                for row in rows:
                    status = row["connectivity_status"]
                    actual_connectivity_counts[status] = (
                        actual_connectivity_counts.get(status, 0) + 1
                    )

                for status, expected_count in expected["connectivity_counts"].items():
                    actual_count = actual_connectivity_counts.get(status, 0)
                    assert actual_count == expected_count, (
                        f"Expected {expected_count} systems with connectivity_status='{status}', "
                        f"got {actual_count} for {scenario_id}"
                    )

            if "update_counts" in expected:
                # Count systems by update status
                actual_update_counts = {}
                for row in rows:
                    status = row["update_status"]
                    actual_update_counts[status] = (
                        actual_update_counts.get(status, 0) + 1
                    )

                for status, expected_count in expected["update_counts"].items():
                    actual_count = actual_update_counts.get(status, 0)
                    assert actual_count == expected_count, (
                        f"Expected {expected_count} systems with update_status='{status}', "
                        f"got {actual_count} for {scenario_id}"
                    )

    finally:
        # Always cleanup
        if "cleanup_fn" in scenario_data:
            scenario_data["cleanup_fn"]()
        elif "cleanup" in scenario_data:
            cf_client.cleanup_test_data(scenario_data["cleanup"])


# Additional smoke tests
@pytest.mark.views
@pytest.mark.database
def test_view_basic_functionality(cf_client: CFTestClient):
    """Basic smoke test for the view"""
    # Test that view exists and has expected columns
    result = cf_client.execute_sql(
        f"""
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = 'view_systems_status_table'
        ORDER BY ordinal_position
        """
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

    assert expected_columns.issubset(actual_columns), (
        f"View missing expected columns. "
        f"Missing: {expected_columns - actual_columns}"
    )


@pytest.mark.views
@pytest.mark.database
def test_view_performance(cf_client: CFTestClient):
    """Test that view performs reasonably well"""
    import time

    start_time = time.time()
    result = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW_STATUS}")
    end_time = time.time()

    query_time = end_time - start_time
    assert query_time < 5.0, f"View query took too long: {query_time:.2f} seconds"

    # View should return some result (even if empty)
    assert len(result) == 1
