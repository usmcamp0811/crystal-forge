import json
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import (_create_base_scenario, scenario_agent_restart,
                               scenario_behind, scenario_build_timeout,
                               scenario_compliance_drift, scenario_eval_failed,
                               scenario_flaky_agent,
                               scenario_latest_with_two_overdue,
                               scenario_mixed_commit_lag,
                               scenario_multi_system_progression_with_failure,
                               scenario_multiple_orphaned_systems,
                               scenario_never_seen, scenario_offline,
                               scenario_orphaned_deployments,
                               scenario_partial_rebuild,
                               scenario_progressive_system_updates,
                               scenario_rollback, scenario_up_to_date)

VIEW_SYSTEMS_CURRENT_STATE = "view_systems_current_state"

SYSTEMS_CURRENT_STATE_SCENARIO_CONFIGS = [
    {
        "id": "agent_restart",
        "builder": scenario_agent_restart,
        "expected": [
            {
                "hostname": "test-agent-restart",
                "has_last_heartbeat": True,
                "has_deployment": True,
                "has_evaluation": True,
                "is_running_latest_derivation": True,  # Single commit scenario
            }
        ],
    },
    {
        "id": "build_timeout",
        "builder": scenario_build_timeout,
        "expected": [
            {
                "hostname": "test-build-timeout",
                "has_last_heartbeat": True,
                "has_deployment": True,
                "latest_commit_evaluation_status": None,  # Pending builds don't count as evaluated
                "is_running_latest_derivation": False,  # Has deployment but no successful evaluation = behind
            }
        ],
    },
    {
        "id": "rollback",
        "builder": scenario_rollback,
        "expected": [
            {
                "hostname": "test-rollback",
                "has_last_heartbeat": True,
                "has_deployment": True,
                "has_evaluation": True,
                "is_running_latest_derivation": False,  # Rolled back to older commit
            }
        ],
    },
    {
        "id": "partial_rebuild",
        "builder": scenario_partial_rebuild,
        "expected": [
            {
                "hostname": "test-partial-rebuild",
                "has_last_heartbeat": True,
                "has_deployment": True,
                "has_evaluation": True,
                "is_running_latest_derivation": True,  # Single commit scenario
            }
        ],
    },
    {
        "id": "compliance_drift",
        "builder": scenario_compliance_drift,
        "expected": [
            {
                "hostname": "test-compliance-drift",
                "has_last_heartbeat": True,
                "has_deployment": True,
                "has_evaluation": False,  # No evaluation for latest commit with matching hostname
                "is_running_latest_derivation": False,  # Has deployment but behind latest = behind
            }
        ],
    },
    {
        "id": "flaky_agent",
        "builder": scenario_flaky_agent,
        "expected": [
            {
                "hostname": "test-flaky-agent",
                "has_last_heartbeat": True,
                "has_deployment": True,
                "has_evaluation": True,
                "is_running_latest_derivation": True,  # Single commit scenario
            }
        ],
    },
    {
        "id": "never_seen",
        "builder": scenario_never_seen,
        "expected": [
            {
                "hostname": "test-never-seen",
                "has_last_heartbeat": False,  # Never sent heartbeat
                "has_deployment": True,
                "has_evaluation": True,
                "is_running_latest_derivation": True,  # Single commit scenario
            }
        ],
    },
    {
        "id": "up_to_date",
        "builder": scenario_up_to_date,
        "expected": [
            {
                "hostname": "test-uptodate",
                "has_last_heartbeat": True,
                "has_deployment": True,
                "has_evaluation": True,
                "is_running_latest_derivation": True,
            }
        ],
    },
    {
        "id": "offline",
        "builder": scenario_offline,
        "expected": [
            {
                "hostname": "test-offline",
                "has_last_heartbeat": True,  # Old heartbeat, but exists
                "has_deployment": True,
                "has_evaluation": True,
                "is_running_latest_derivation": True,  # Single commit scenario
            }
        ],
    },
    {
        "id": "behind",
        "builder": scenario_behind,
        "expected": [
            {
                "hostname": "test-behind",
                "has_last_heartbeat": True,
                "has_deployment": True,
                "has_evaluation": False,  # Latest commit has no derivation with matching hostname
                "is_running_latest_derivation": False,  # Has deployment but behind latest = behind
            }
        ],
    },
    {
        "id": "eval_failed",
        "builder": scenario_eval_failed,
        "expected": [
            {
                "hostname": "test-eval-failed",
                "has_last_heartbeat": True,
                "has_deployment": True,
                "has_evaluation": False,  # Latest commit failed evaluation, no successful derivation
                "is_running_latest_derivation": False,  # Has deployment but latest failed = behind
            }
        ],
    },
    {
        "id": "progressive_system_updates",
        "builder": scenario_progressive_system_updates,
        "expected": {
            "count": 3,  # fast, medium, slow systems
            "has_deployments": 3,
            "has_evaluations": 1,
            "has_heartbeats": 3,
            "running_latest_count": 1,  # Only fast system runs latest
            "running_older_count": 2,  # medium and slow run older commits
        },
    },
    {
        "id": "multiple_orphaned_systems",
        "builder": scenario_multiple_orphaned_systems,
        "expected": {
            "count": 5,  # 5 orphaned systems
            "has_deployments": 5,
            "has_evaluations": 5,  # Evaluations exist for the flake commits
            "has_heartbeats": 5,
            "running_latest_count": 0,  # All are orphaned (derivation paths don't match)
        },
    },
    {
        "id": "latest_with_two_overdue",
        "builder": scenario_latest_with_two_overdue,
        "expected": {
            "count": 9,  # Default num_systems
            "has_deployments": 9,
            "has_evaluations": 9,
            "has_heartbeats": 9,
            "running_latest_count": 9,  # All systems should be on latest commit
        },
    },
    {
        "id": "mixed_commit_lag",
        "builder": scenario_mixed_commit_lag,
        "expected": {
            "count": 4,
            "has_deployments": 4,
            "has_evaluations": 4,
            "has_heartbeats": 4,
            "running_latest_count": 3,  # test-mixed-1, 2, 4 on current
            "running_older_count": 1,   # test-mixed-3 on old commit
        },
    },
    {
        "id": "multi_system_progression_with_failure",
        "builder": scenario_multi_system_progression_with_failure,
        "expected": {
            "count": 5,
            "has_deployments": 5,
            "has_evaluations": 0,  # Latest commit (10) failed, no successful derivation
            "has_heartbeats": 5,
            "running_latest_count": 0,  # Latest commit failed, so no systems run "latest"
            "running_older_count": 0,   # Can't determine without evaluations (all will be None)
        },
    },
    {
        "id": "orphaned_deployments",
        "builder": scenario_orphaned_deployments,
        "expected": [
            {
                "hostname": "test-orphaned-deploy",
                "has_last_heartbeat": True,
                "has_deployment": True,
                "has_evaluation": True,  # Tracked derivation exists
                "is_running_latest_derivation": False,  # Running orphaned derivation
            }
        ],
    },
]


def _get_hostnames_from_current_state_scenario(
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
        elif scenario_id == "progressive_system_updates":
            return ["test-progressive-fast", "test-progressive-medium", "test-progressive-slow"]
        elif scenario_id == "latest_with_two_overdue":
            return [f"test-latest-{i+1}" for i in range(9)]
        elif scenario_id == "multi_system_progression_with_failure":
            return [f"test-progression-{i}" for i in range(5)]
        elif scenario_id == "multiple_orphaned_systems":
            return [f"test-multi-orphaned-{i}" for i in range(5)]
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
    "scenario_config", SYSTEMS_CURRENT_STATE_SCENARIO_CONFIGS, ids=lambda x: x["id"]
)
def test_systems_current_state_scenarios(
    cf_client: CFTestClient, clean_test_data, scenario_config: Dict[str, Any]
):
    """Test systems current state view with all scenarios"""
    builder = scenario_config["builder"]
    expected = scenario_config["expected"]
    scenario_id = scenario_config["id"]

    # Build the scenario
    scenario_data = builder(cf_client)

    # Determine hostnames to fetch from the view
    hostnames = _get_hostnames_from_current_state_scenario(scenario_data, scenario_id)

    # Query the systems current state view
    if hostnames:
        rows = cf_client.execute_sql(
            f"""
            SELECT system_id, hostname, repo_url, deployed_commit_hash,
                   deployed_commit_timestamp, current_derivation_path, ip_address,
                   uptime_days, os, kernel, agent_version, last_deployed,
                   latest_commit_derivation_path, latest_commit_evaluation_status,
                   is_running_latest_derivation, last_heartbeat, last_seen
            FROM {VIEW_SYSTEMS_CURRENT_STATE}
            WHERE hostname = ANY(%s)
            ORDER BY hostname
            """,
            (hostnames,),
        )
    else:
        # Pattern matching fallback
        pattern_map = {
            "mixed_commit_lag": "test-mixed-%",
            "progressive_system_updates": "test-progressive-%",
            "latest_with_two_overdue": "test-latest-%",
            "multi_system_progression_with_failure": "test-progression-%",
            "multiple_orphaned_systems": "test-multi-orphaned-%",
        }
        pattern = pattern_map.get(scenario_id, f"{scenario_id.replace('_', '-')}-%")

        rows = cf_client.execute_sql(
            f"""
            SELECT system_id, hostname, repo_url, deployed_commit_hash,
                   deployed_commit_timestamp, current_derivation_path, ip_address,
                   uptime_days, os, kernel, agent_version, last_deployed,
                   latest_commit_derivation_path, latest_commit_evaluation_status,
                   is_running_latest_derivation, last_heartbeat, last_seen
            FROM {VIEW_SYSTEMS_CURRENT_STATE}
            WHERE hostname LIKE %s
            ORDER BY hostname
            """,
            (pattern,),
        )

    # Save results for debugging
    try:
        log_path = Path("/tmp/cf_systems_current_state_scenario_results.json")
        log_path.parent.mkdir(parents=True, exist_ok=True)
        with log_path.open("a", encoding="utf-8") as fh:
            fh.write(
                json.dumps({"scenario": scenario_id, "rows": rows}, default=str) + "\n"
            )
    except Exception:
        pass

    # Validate results
    assert (
        len(rows) > 0
    ), f"No systems found for scenario {scenario_id} with hostnames {hostnames}"

    if isinstance(expected, list):
        # Single system or specific system expectations
        assert len(rows) == len(expected), (
            f"Expected {len(expected)} systems, got {len(rows)} "
            f"for scenario {scenario_id}: {[r['hostname'] for r in rows]}"
        )
        
        for expected_system in expected:
            expected_hostname = expected_system["hostname"]
            matching_row = next(
                (row for row in rows if row["hostname"] == expected_hostname), None
            )
            assert matching_row is not None, f"No result found for {expected_hostname}"

            # Validate specific expectations
            for field, expected_value in expected_system.items():
                if field == "hostname":
                    continue
                elif field == "has_last_heartbeat":
                    has_heartbeat = matching_row["last_heartbeat"] is not None
                    assert has_heartbeat == expected_value, (
                        f"Heartbeat expectation failed for {expected_hostname}: "
                        f"expected {expected_value}, got {has_heartbeat}"
                    )
                elif field == "has_deployment":
                    has_deployment = matching_row["current_derivation_path"] is not None
                    assert has_deployment == expected_value, (
                        f"Deployment expectation failed for {expected_hostname}: "
                        f"expected {expected_value}, got {has_deployment}"
                    )
                elif field == "has_evaluation":
                    has_evaluation = matching_row["latest_commit_evaluation_status"] is not None
                    assert has_evaluation == expected_value, (
                        f"Evaluation expectation failed for {expected_hostname}: "
                        f"expected {expected_value}, got {has_evaluation}"
                    )
                elif field == "is_running_latest_derivation":
                    actual_value = matching_row["is_running_latest_derivation"]
                    assert actual_value == expected_value, (
                        f"Latest derivation check failed for {expected_hostname}: "
                        f"expected {expected_value}, got {actual_value}"
                    )
                elif field == "latest_commit_evaluation_status":
                    actual_value = matching_row["latest_commit_evaluation_status"]
                    assert actual_value == expected_value, (
                        f"Evaluation status mismatch for {expected_hostname}: "
                        f"expected {expected_value}, got {actual_value}"
                    )

    elif isinstance(expected, dict):
        # Multi-system scenario with counts
        if "count" in expected:
            assert (
                len(rows) == expected["count"]
            ), f"Expected {expected['count']} systems, got {len(rows)} for {scenario_id}"

        if "has_deployments" in expected:
            deployment_count = sum(1 for r in rows if r["current_derivation_path"] is not None)
            assert deployment_count == expected["has_deployments"], (
                f"Expected {expected['has_deployments']} systems with deployments, "
                f"got {deployment_count} for {scenario_id}"
            )

        if "has_evaluations" in expected:
            evaluation_count = sum(1 for r in rows if r["latest_commit_evaluation_status"] is not None)
            assert evaluation_count == expected["has_evaluations"], (
                f"Expected {expected['has_evaluations']} systems with evaluations, "
                f"got {evaluation_count} for {scenario_id}"
            )

        if "has_heartbeats" in expected:
            heartbeat_count = sum(1 for r in rows if r["last_heartbeat"] is not None)
            assert heartbeat_count == expected["has_heartbeats"], (
                f"Expected {expected['has_heartbeats']} systems with heartbeats, "
                f"got {heartbeat_count} for {scenario_id}"
            )

        if "running_latest_count" in expected:
            latest_count = sum(1 for r in rows if r["is_running_latest_derivation"] is True)
            assert latest_count == expected["running_latest_count"], (
                f"Expected {expected['running_latest_count']} systems running latest derivation, "
                f"got {latest_count} for {scenario_id}"
            )

        if "running_older_count" in expected:
            older_count = sum(1 for r in rows if r["is_running_latest_derivation"] is False)
            assert older_count == expected["running_older_count"], (
                f"Expected {expected['running_older_count']} systems running older derivations, "
                f"got {older_count} for {scenario_id}"
            )


@pytest.mark.views
@pytest.mark.database
def test_systems_current_state_view_basic_functionality(cf_client: CFTestClient):
    """Basic smoke test for the systems current state view"""
    result = cf_client.execute_sql(
        f"""
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW_SYSTEMS_CURRENT_STATE,),
    )

    expected_columns = {
        "system_id",
        "hostname",
        "repo_url",
        "deployed_commit_hash",
        "deployed_commit_timestamp",
        "current_derivation_path",
        "ip_address",
        "uptime_days",
        "os",
        "kernel",
        "agent_version",
        "last_deployed",
        "latest_commit_derivation_path",
        "latest_commit_evaluation_status",
        "is_running_latest_derivation",
        "last_heartbeat",
        "last_seen",
    }
    actual_columns = {row["column_name"] for row in result}
    assert expected_columns.issubset(
        actual_columns
    ), f"View missing expected columns. Missing: {expected_columns - actual_columns}"


@pytest.mark.views
@pytest.mark.database
def test_systems_current_state_view_performance(cf_client: CFTestClient):
    """Test that systems current state view performs reasonably well"""
    import time

    start_time = time.time()
    result = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW_SYSTEMS_CURRENT_STATE}")
    query_time = time.time() - start_time

    assert (
        query_time < 10.0
    ), f"Systems current state view query took too long: {query_time:.2f} seconds"
    assert len(result) == 1


@pytest.mark.views
@pytest.mark.database
def test_systems_current_state_uptime_calculation(cf_client: CFTestClient, clean_test_data):
    """Test that uptime is calculated correctly in days"""
    
    # Create system with specific uptime
    uptime_seconds = 345600  # 4 days exactly
    scenario = _create_base_scenario(
        cf_client,
        hostname="test-uptime-calc",
        flake_name="uptime-test",
        repo_url="https://example.com/uptime-test.git",
        git_hash="uptime123",
        commit_age_hours=1,
        heartbeat_age_minutes=5,
    )

    # Update the system state with specific uptime
    cf_client.execute_sql(
        """
        UPDATE system_states
        SET uptime_secs = %s
        WHERE hostname = %s
        """,
        (uptime_seconds, "test-uptime-calc"),
    )

    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT hostname, uptime_days
        FROM {VIEW_SYSTEMS_CURRENT_STATE}
        WHERE hostname = 'test-uptime-calc'
        """
    )

    assert len(rows) == 1
    row = rows[0]
    
    # Should be exactly 4.0 days
    assert row["uptime_days"] == 4.0, f"Expected 4.0 uptime days, got {row['uptime_days']}"

    # Clean up
    cf_client.cleanup_test_data(scenario["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_systems_current_state_last_seen_logic(cf_client: CFTestClient, clean_test_data):
    """Test that last_seen is the maximum of last_deployed and last_heartbeat"""
    
    now = datetime.now(UTC)
    
    # Create system with heartbeat newer than deployment
    scenario = _create_base_scenario(
        cf_client,
        hostname="test-last-seen",
        flake_name="last-seen-test",
        repo_url="https://example.com/last-seen-test.git",
        git_hash="lastseen123",
        commit_age_hours=2,
        heartbeat_age_minutes=None,  # We'll create custom heartbeat
    )

    # Set deployment time to 1 hour ago
    deployment_time = now - timedelta(hours=1)
    cf_client.execute_sql(
        """
        UPDATE system_states
        SET timestamp = %s
        WHERE hostname = %s
        """,
        (deployment_time, "test-last-seen"),
    )

    # Create heartbeat 30 minutes ago (more recent than deployment)
    heartbeat_time = now - timedelta(minutes=30)
    state_id = scenario["state_id"]
    cf_client.execute_sql(
        """
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
        VALUES (%s, %s, '2.0.0', 'test-build')
        """,
        (state_id, heartbeat_time),
    )

    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT hostname, last_deployed, last_heartbeat, last_seen
        FROM {VIEW_SYSTEMS_CURRENT_STATE}
        WHERE hostname = 'test-last-seen'
        """
    )

    assert len(rows) == 1
    row = rows[0]
    
    # last_seen should be the heartbeat time (more recent)
    assert row["last_seen"] == heartbeat_time, (
        f"Expected last_seen to be heartbeat time {heartbeat_time}, "
        f"got {row['last_seen']}"
    )

    # Clean up
    cf_client.cleanup_test_data(scenario["cleanup"])
    cf_client.execute_sql(
        "DELETE FROM agent_heartbeats WHERE system_state_id = %s",
        (state_id,)
    )


@pytest.mark.views
@pytest.mark.database
def test_systems_current_state_is_running_latest_logic(cf_client: CFTestClient, clean_test_data):
    """Test the is_running_latest_derivation logic"""
    
    # Test case 1: System running latest derivation (should be True)
    scenario_latest = _create_base_scenario(
        cf_client,
        hostname="test-running-latest",
        flake_name="latest-logic-test",
        repo_url="https://example.com/latest-logic.git",
        git_hash="latest123",
        commit_age_hours=1,
        heartbeat_age_minutes=5,
    )

    # Test case 2: System with no evaluation for latest commit (should be NULL)
    scenario_no_eval = _create_base_scenario(
        cf_client,
        hostname="test-no-eval",
        flake_name="no-eval-test",
        repo_url="https://example.com/no-eval.git",
        git_hash="noeval456",
        commit_age_hours=1,
        derivation_status="pending",  # Not complete, so won't be in evaluation
        heartbeat_age_minutes=5,
    )

    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT hostname, is_running_latest_derivation, 
               current_derivation_path, latest_commit_derivation_path,
               latest_commit_evaluation_status
        FROM {VIEW_SYSTEMS_CURRENT_STATE}
        WHERE hostname IN ('test-running-latest', 'test-no-eval')
        ORDER BY hostname
        """
    )

    assert len(rows) == 2

    # Find rows by hostname
    results_by_hostname = {row["hostname"]: row for row in rows}

    # Test running latest
    latest_row = results_by_hostname["test-running-latest"]
    assert latest_row["is_running_latest_derivation"] is True, (
        f"Expected True for system running latest, got {latest_row['is_running_latest_derivation']}"
    )

    # Test no evaluation
    no_eval_row = results_by_hostname["test-no-eval"]
    assert no_eval_row["is_running_latest_derivation"] is None, (
        f"Expected None for system with no evaluation, got {no_eval_row['is_running_latest_derivation']}"
    )
    assert no_eval_row["latest_commit_evaluation_status"] is None, (
        "Expected no evaluation status for pending derivation"
    )

    # Clean up
    cf_client.cleanup_test_data(scenario_latest["cleanup"])
    cf_client.cleanup_test_data(scenario_no_eval["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_systems_current_state_ordering(cf_client: CFTestClient, clean_test_data):
    """Test that systems are ordered by hostname"""
    
    # Create multiple systems with different hostnames
    scenarios = []
    hostnames = ["test-order-zebra", "test-order-alpha", "test-order-beta"]
    
    for hostname in hostnames:
        scenario = _create_base_scenario(
            cf_client,
            hostname=hostname,
            flake_name=f"{hostname}-flake",
            repo_url=f"https://example.com/{hostname}.git",
            git_hash=f"{hostname}123",
            commit_age_hours=1,
            heartbeat_age_minutes=5,
        )
        scenarios.append(scenario)

    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT hostname
        FROM {VIEW_SYSTEMS_CURRENT_STATE}
        WHERE hostname LIKE 'test-order-%'
        ORDER BY hostname  -- This should be the default ordering
        """
    )

    assert len(rows) == 3
    
    # Should be alphabetically ordered
    expected_order = ["test-order-alpha", "test-order-beta", "test-order-zebra"]
    actual_order = [row["hostname"] for row in rows]
    
    assert actual_order == expected_order, (
        f"Expected order {expected_order}, got {actual_order}"
    )

    # Clean up
    for scenario in scenarios:
        cf_client.cleanup_test_data(scenario["cleanup"])
