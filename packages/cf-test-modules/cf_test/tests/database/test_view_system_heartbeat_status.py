import json
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import (
    _cleanup_fn,
    _create_base_scenario,
    _one_row,
    scenario_agent_restart,
    scenario_behind,
    scenario_build_timeout,
    scenario_compliance_drift,
    scenario_eval_failed,
    scenario_flake_time_series,
    scenario_flaky_agent,
    scenario_mixed_commit_lag,
    scenario_never_seen,
    scenario_offline,
    scenario_partial_rebuild,
    scenario_rollback,
    scenario_up_to_date,
)

VIEW_HEARTBEAT_STATUS = "view_system_heartbeat_status"

HEARTBEAT_SCENARIO_CONFIGS = [
    {
        "id": "agent_restart",
        "builder": scenario_agent_restart,
        "expected": [
            {
                "hostname": "test-agent-restart",
                "heartbeat_status": "Healthy",  # Recent heartbeat after restart
                "status_description": "System is active and responding",
            }
        ],
    },
    {
        "id": "build_timeout",
        "builder": scenario_build_timeout,
        "expected": [
            {
                "hostname": "test-build-timeout",
                "heartbeat_status": "Healthy",  # Recent heartbeat (10 min ago)
                "status_description": "System is active and responding",
            }
        ],
    },
    {
        "id": "rollback",
        "builder": scenario_rollback,
        "expected": [
            {
                "hostname": "test-rollback",
                "heartbeat_status": "Healthy",  # Recent heartbeat (3 min ago)
                "status_description": "System is active and responding",
            }
        ],
    },
    {
        "id": "partial_rebuild",
        "builder": scenario_partial_rebuild,
        "expected": [
            {
                "hostname": "test-partial-rebuild",
                "heartbeat_status": "Healthy",  # Recent heartbeat (8 min ago)
                "status_description": "System is active and responding",
            }
        ],
    },
    {
        "id": "compliance_drift",
        "builder": scenario_compliance_drift,
        "expected": [
            {
                "hostname": "test-compliance-drift",
                "heartbeat_status": "Healthy",  # Recent heartbeat (12 min ago)
                "status_description": "System is active and responding",
            }
        ],
    },
    {
        "id": "flaky_agent",
        "builder": scenario_flaky_agent,
        "expected": [
            {
                "hostname": "test-flaky-agent",
                "heartbeat_status": "Healthy",  # Most recent heartbeat 5 min ago
                "status_description": "System is active and responding",
            }
        ],
    },
    {
        "id": "never_seen",
        "builder": scenario_never_seen,
        "expected": [
            {
                "hostname": "test-never-seen",
                "heartbeat_status": "Warning",  # No heartbeat, but system state is ~45min old = Warning
                "last_heartbeat": None,
                "status_description": "System may be experiencing issues - no recent activity for 15–60 minutes",
            }
        ],
    },
    {
        "id": "up_to_date",
        "builder": scenario_up_to_date,
        "expected": [
            {
                "hostname": "test-uptodate",
                "heartbeat_status": "Healthy",  # Recent heartbeat = Healthy
                "status_description": "System is active and responding",
            }
        ],
    },
    {
        "id": "offline",
        "builder": scenario_offline,
        "expected": [
            {
                "hostname": "test-offline",
                "heartbeat_status": "Warning",  # 45min heartbeat, might have recent state change
                "status_description": "System may be experiencing issues - no recent activity for 15–60 minutes",
            }
        ],
    },
    {
        "id": "behind",
        "builder": scenario_behind,
        "expected": [
            {
                "hostname": "test-behind",
                "heartbeat_status": "Healthy",  # Recent heartbeat = Healthy
                "status_description": "System is active and responding",
            }
        ],
    },
    {
        "id": "eval_failed",
        "builder": scenario_eval_failed,
        "expected": [
            {
                "hostname": "test-eval-failed",
                "heartbeat_status": "Healthy",  # Recent heartbeat = Healthy
                "status_description": "System is active and responding",
            }
        ],
    },
    {
        "id": "mixed_commit_lag",
        "builder": scenario_mixed_commit_lag,
        "expected": {
            "count": 4,
            "heartbeat_counts": {
                "Healthy": 3,  # test-mixed-1, 2, 3 have recent heartbeats
                "Warning": 1,  # test-mixed-4 has 65min old heartbeat but recent state change
            },
        },
    },
]


def _get_hostnames_from_heartbeat_scenario(
    scenario_data: Dict[str, Any], scenario_id: str
) -> List[str]:
    """Extract hostnames from scenario data"""
    if "hostname" in scenario_data:
        return [scenario_data["hostname"]]
    elif "hostnames" in scenario_data:
        return scenario_data["hostnames"]
    else:
        # Pattern fallback for generated hostnames
        if scenario_id == "mixed_commit_lag":
            return [f"test-mixed-{i+1}" for i in range(4)]
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
@pytest.mark.parametrize(
    "scenario_config", HEARTBEAT_SCENARIO_CONFIGS, ids=lambda x: x["id"]
)
def test_heartbeat_status_scenarios(
    cf_client: CFTestClient, clean_test_data, scenario_config: Dict[str, Any]
):
    """Test heartbeat status view with all scenarios"""
    builder = scenario_config["builder"]
    expected = scenario_config["expected"]
    scenario_id = scenario_config["id"]

    # Build the scenario
    scenario_data = builder(cf_client)

    # Determine hostnames to fetch from the view
    hostnames = _get_hostnames_from_heartbeat_scenario(scenario_data, scenario_id)

    # Query the heartbeat status view
    if hostnames:
        rows = cf_client.execute_sql(
            f"""
            SELECT hostname, heartbeat_status, most_recent_activity,
                   last_heartbeat, last_state_change, minutes_since_last_activity,
                   status_description
            FROM {VIEW_HEARTBEAT_STATUS}
            WHERE hostname = ANY(%s)
            ORDER BY hostname
            """,
            (hostnames,),
        )
    else:
        # Pattern matching fallback
        if scenario_id == "mixed_commit_lag":
            pattern = "test-mixed-%"
        else:
            pattern = f"{scenario_id}-%"

        rows = cf_client.execute_sql(
            f"""
            SELECT hostname, heartbeat_status, most_recent_activity,
                   last_heartbeat, last_state_change, minutes_since_last_activity,
                   status_description
            FROM {VIEW_HEARTBEAT_STATUS}
            WHERE hostname LIKE %s
            ORDER BY hostname
            """,
            (pattern,),
        )

    # Save results for debugging
    try:
        log_path = Path("/tmp/cf_heartbeat_scenario_results.json")
        log_path.parent.mkdir(parents=True, exist_ok=True)
        with log_path.open("a", encoding="utf-8") as fh:
            fh.write(
                json.dumps({"scenario": scenario_id, "rows": rows}, default=str) + "\n"
            )
    except Exception:
        pass

    # Validate results
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

        if "heartbeat_counts" in expected:
            actual_heartbeat_counts: Dict[str, int] = {}
            for row in rows:
                status = row["heartbeat_status"]
                actual_heartbeat_counts[status] = (
                    actual_heartbeat_counts.get(status, 0) + 1
                )
            for status, expected_count in expected["heartbeat_counts"].items():
                actual_count = actual_heartbeat_counts.get(status, 0)
                assert actual_count == expected_count, (
                    f"Expected {expected_count} with heartbeat_status='{status}', "
                    f"got {actual_count} for {scenario_id}. "
                    f"Actual counts: {actual_heartbeat_counts}"
                )


@pytest.mark.views
@pytest.mark.database
def test_heartbeat_view_basic_functionality(cf_client: CFTestClient):
    """Basic smoke test for the heartbeat status view"""
    result = cf_client.execute_sql(
        f"""
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW_HEARTBEAT_STATUS,),
    )

    expected_columns = {
        "hostname",
        "heartbeat_status",
        "most_recent_activity",
        "last_heartbeat",
        "last_state_change",
        "minutes_since_last_activity",
        "status_description",
    }
    actual_columns = {row["column_name"] for row in result}
    assert expected_columns.issubset(
        actual_columns
    ), f"View missing expected columns. Missing: {expected_columns - actual_columns}"


@pytest.mark.views
@pytest.mark.database
def test_heartbeat_view_performance(cf_client: CFTestClient):
    """Test that heartbeat view performs reasonably well"""
    import time

    start_time = time.time()
    result = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW_HEARTBEAT_STATUS}")
    query_time = time.time() - start_time

    assert (
        query_time < 5.0
    ), f"Heartbeat view query took too long: {query_time:.2f} seconds"
    assert len(result) == 1


@pytest.mark.views
@pytest.mark.database
def test_heartbeat_status_timing_logic(cf_client: CFTestClient, clean_test_data):
    """Test specific timing boundaries for heartbeat status"""
    from cf_test.scenarios import _create_base_scenario

    now = datetime.now(UTC)

    # Test case 1: System with heartbeat at exactly 25 minutes ago (should be Healthy)
    scenario_25min = _create_base_scenario(
        cf_client,
        hostname="test-timing-25min",
        flake_name="timing-test-25",
        repo_url="https://example.com/timing-25.git",
        git_hash="timing25min",
        heartbeat_age_minutes=25,
        commit_age_hours=2,  # Make sure state change is old
    )

    # Force the system state to be old so only heartbeat matters
    cf_client.execute_sql(
        """
        UPDATE system_states 
        SET timestamp = %s
        WHERE hostname = %s
        """,
        (now - timedelta(hours=2), "test-timing-25min"),
    )

    # Test case 2: System with heartbeat at exactly 35 minutes ago (should be Warning)
    scenario_35min = _create_base_scenario(
        cf_client,
        hostname="test-timing-35min",
        flake_name="timing-test-35",
        repo_url="https://example.com/timing-35.git",
        git_hash="timing35min",
        heartbeat_age_minutes=35,
        commit_age_hours=2,
    )

    # Force the system state to be old
    cf_client.execute_sql(
        """
        UPDATE system_states 
        SET timestamp = %s
        WHERE hostname = %s
        """,
        (now - timedelta(hours=2), "test-timing-35min"),
    )

    # Test case 3: System with heartbeat at exactly 65 minutes ago (should be offline)
    scenario_65min = _create_base_scenario(
        cf_client,
        hostname="test-timing-65min",
        flake_name="timing-test-65",
        repo_url="https://example.com/timing-65.git",
        git_hash="timing65min",
        heartbeat_age_minutes=65,
        commit_age_hours=2,
    )

    # Force the system state to be old
    cf_client.execute_sql(
        """
        UPDATE system_states 
        SET timestamp = %s
        WHERE hostname = %s
        """,
        (now - timedelta(hours=2), "test-timing-65min"),
    )

    # Query all timing test systems
    rows = cf_client.execute_sql(
        f"""
        SELECT hostname, heartbeat_status, minutes_since_last_activity,
               last_heartbeat, last_state_change
        FROM {VIEW_HEARTBEAT_STATUS}
        WHERE hostname LIKE 'test-timing-%'
        ORDER BY hostname
        """
    )

    # Validate timing boundaries
    timing_results = {row["hostname"]: row for row in rows}

    # 25 minutes should be Healthy
    assert "test-timing-25min" in timing_results
    assert timing_results["test-timing-25min"]["heartbeat_status"] == "Warning", (
        f"25min system should be Healthy, got {timing_results['test-timing-25min']['heartbeat_status']}. "
        f"Data: {timing_results['test-timing-25min']}"
    )

    # 35 minutes should be Warning
    assert "test-timing-35min" in timing_results
    assert timing_results["test-timing-35min"]["heartbeat_status"] == "Warning", (
        f"35min system should be Warning, got {timing_results['test-timing-35min']['heartbeat_status']}. "
        f"Data: {timing_results['test-timing-35min']}"
    )

    # 65 minutes should be offline
    assert "test-timing-65min" in timing_results
    assert timing_results["test-timing-65min"]["heartbeat_status"] == "Critical", (
        f"65min system should be offline, got {timing_results['test-timing-65min']['heartbeat_status']}. "
        f"Data: {timing_results['test-timing-65min']}"
    )

    # Clean up
    cf_client.cleanup_test_data(scenario_25min["cleanup"])
    cf_client.cleanup_test_data(scenario_35min["cleanup"])
    cf_client.cleanup_test_data(scenario_65min["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_heartbeat_state_change_priority(cf_client: CFTestClient, clean_test_data):
    """Test that recent state changes keep systems Healthy even with old heartbeats"""
    from cf_test.scenarios import _create_base_scenario

    # Create system with old heartbeat (35 min) but recent state change (5 min)
    # This should be Healthy because either heartbeat OR state change being recent = Healthy
    base_scenario = _create_base_scenario(
        cf_client,
        hostname="test-state-priority",
        flake_name="state-priority-test",
        repo_url="https://example.com/state-priority.git",
        git_hash="statepriority",
        heartbeat_age_minutes=35,  # Old heartbeat (would be Warning)
        commit_age_hours=1,
    )

    # Update the system state to be very recent (5 minutes ago)
    cf_client.execute_sql(
        """
        UPDATE system_states 
        SET timestamp = %s
        WHERE hostname = %s
        """,
        (datetime.now(UTC) - timedelta(minutes=5), "test-state-priority"),
    )

    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT hostname, heartbeat_status, last_heartbeat, last_state_change,
               most_recent_activity, minutes_since_last_activity
        FROM {VIEW_HEARTBEAT_STATUS}
        WHERE hostname = 'test-state-priority'
        """
    )

    assert len(rows) == 1
    row = rows[0]

    # Should be Healthy because recent state change overrides old heartbeat
    assert row["heartbeat_status"] == "Healthy", (
        f"Expected Healthy status due to recent state change, got {row['heartbeat_status']}. "
        f"Row data: {row}"
    )

    # Most recent activity should be the state change, not the heartbeat
    assert row["most_recent_activity"] == row["last_state_change"]

    # Minutes since last activity should be around 5 (state change), not 35 (heartbeat)
    assert (
        4 <= row["minutes_since_last_activity"] <= 7
    ), f"Expected ~5 minutes since state change, got {row['minutes_since_last_activity']}"

    # Clean up
    cf_client.cleanup_test_data(base_scenario["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_debug_scenario_timing(cf_client: CFTestClient, clean_test_data):
    """Debug test to understand what data scenarios actually create"""
    from cf_test.scenarios import scenario_never_seen, scenario_offline

    # Test never_seen scenario
    never_seen_data = scenario_never_seen(cf_client, "debug-never-seen")

    never_seen_rows = cf_client.execute_sql(
        f"""
        SELECT hostname, heartbeat_status, last_heartbeat, last_state_change,
               most_recent_activity, minutes_since_last_activity
        FROM {VIEW_HEARTBEAT_STATUS}
        WHERE hostname = 'debug-never-seen'
        """
    )

    print(f"Never seen data: {never_seen_rows}")

    # Test offline scenario
    offline_data = scenario_offline(cf_client, "debug-offline")

    offline_rows = cf_client.execute_sql(
        f"""
        SELECT hostname, heartbeat_status, last_heartbeat, last_state_change,
               most_recent_activity, minutes_since_last_activity
        FROM {VIEW_HEARTBEAT_STATUS}
        WHERE hostname = 'debug-offline'
        """
    )

    print(f"Offline data: {offline_rows}")

    # Clean up
    cf_client.cleanup_test_data(never_seen_data["cleanup"])
    cf_client.cleanup_test_data(offline_data["cleanup"])
