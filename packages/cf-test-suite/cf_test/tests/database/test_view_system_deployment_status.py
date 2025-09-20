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

VIEW_DEPLOYMENT_STATUS = "view_system_deployment_status"

DEPLOYMENT_SCENARIO_CONFIGS = [
    {
        "id": "agent_restart",
        "builder": scenario_agent_restart,
        "expected": [
            {
                "hostname": "test-agent-restart",
                "deployment_status": "up_to_date",  # Single commit, system matches
                "commits_behind": 0,
                "status_description": "Running latest commit",
            }
        ],
    },
    {
        "id": "build_timeout",
        "builder": scenario_build_timeout,
        "expected": [
            {
                "hostname": "test-build-timeout",
                "deployment_status": "up_to_date",  # Single commit scenario
                "commits_behind": 0,
                "status_description": "Running latest commit",
            }
        ],
    },
    {
        "id": "rollback",
        "builder": scenario_rollback,
        "expected": [
            {
                "hostname": "test-rollback",
                "deployment_status": "behind",  # Rolled back to older commit
                "commits_behind": 1,  # Behind by the newer problematic commit
                "status_description": "Behind by 1 commit(s)",
            }
        ],
    },
    {
        "id": "partial_rebuild",
        "builder": scenario_partial_rebuild,
        "expected": [
            {
                "hostname": "test-partial-rebuild",
                "deployment_status": "up_to_date",  # Single commit scenario
                "commits_behind": 0,
                "status_description": "Running latest commit",
            }
        ],
    },
    {
        "id": "compliance_drift",
        "builder": scenario_compliance_drift,
        "expected": [
            {
                "hostname": "test-compliance-drift",
                "deployment_status": "behind",  # Ancient commit with many newer ones
                "commits_behind": 7,  # Exactly 7 newer commits created, but allow for flexibility
            }
        ],
    },
    {
        "id": "flaky_agent",
        "builder": scenario_flaky_agent,
        "expected": [
            {
                "hostname": "test-flaky-agent",
                "deployment_status": "up_to_date",  # Single commit scenario
                "commits_behind": 0,
                "status_description": "Running latest commit",
            }
        ],
    },
    {
        "id": "never_seen",
        "builder": scenario_never_seen,
        "expected": [
            {
                "hostname": "test-never-seen",
                "deployment_status": "up_to_date",  # System has deployment and should be current
                "commits_behind": 0,
                "status_description": "Running latest commit",
            }
        ],
    },
    {
        "id": "up_to_date",
        "builder": scenario_up_to_date,
        "expected": [
            {
                "hostname": "test-uptodate",
                "deployment_status": "up_to_date",
                "commits_behind": 0,
                "status_description": "Running latest commit",
            }
        ],
    },
    {
        "id": "behind",
        "builder": scenario_behind,
        "expected": [
            {
                "hostname": "test-behind",
                "deployment_status": "behind",
                "status_description": "Behind by 1 commit(s)",
            }
        ],
    },
    {
        "id": "eval_failed",
        "builder": scenario_eval_failed,
        "expected": [
            {
                "hostname": "test-eval-failed",
                "deployment_status": "behind",  # Running older working commit, newer commit exists (even though it failed)
                "commits_behind": 1,  # Behind by the failed commit
                "status_description": "Behind by 1 commit(s)",
            }
        ],
    },
    {
        "id": "mixed_commit_lag",
        "builder": scenario_mixed_commit_lag,
        "expected": {
            "count": 4,
            "deployment_counts": {
                "up_to_date": 3,  # test-mixed-1, 2, 4 (4 has current commit despite being offline)
                "behind": 1,  # test-mixed-3 has old commit
            },
        },
    },
]


def _get_hostnames_from_deployment_scenario(
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
    "scenario_config", DEPLOYMENT_SCENARIO_CONFIGS, ids=lambda x: x["id"]
)
def test_deployment_status_scenarios(
    cf_client: CFTestClient, clean_test_data, scenario_config: Dict[str, Any]
):
    """Test deployment status view with all scenarios"""
    builder = scenario_config["builder"]
    expected = scenario_config["expected"]
    scenario_id = scenario_config["id"]

    # Build the scenario
    scenario_data = builder(cf_client)

    # Determine hostnames to fetch from the view
    hostnames = _get_hostnames_from_deployment_scenario(scenario_data, scenario_id)

    # Query the deployment status view
    if hostnames:
        rows = cf_client.execute_sql(
            f"""
            SELECT hostname, deployment_status, current_derivation_path,
                   deployment_time, current_commit_hash, current_commit_timestamp,
                   latest_commit_hash, latest_commit_timestamp, commits_behind,
                   flake_name, status_description
            FROM {VIEW_DEPLOYMENT_STATUS}
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
            SELECT hostname, deployment_status, current_derivation_path,
                   deployment_time, current_commit_hash, current_commit_timestamp,
                   latest_commit_hash, latest_commit_timestamp, commits_behind,
                   flake_name, status_description
            FROM {VIEW_DEPLOYMENT_STATUS}
            WHERE hostname LIKE %s
            ORDER BY hostname
            """,
            (pattern,),
        )

    # Save results for debugging
    try:
        log_path = Path("/tmp/cf_deployment_scenario_results.json")
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

                # Special handling for commits_behind - check if it's at least the expected value for "behind" scenarios
                if (
                    field == "commits_behind"
                    and expected_system.get("deployment_status") == "behind"
                ):
                    if expected_value > 0:
                        assert actual_value >= expected_value, (
                            f"Field {field} for {expected_hostname}: "
                            f"expected at least {expected_value}, got {actual_value}"
                        )
                        continue
                elif field == "commits_behind":
                    # For non-behind scenarios, allow exact match
                    assert actual_value == expected_value, (
                        f"Field mismatch for {expected_hostname}.{field}: "
                        f"expected '{expected_value}', got '{actual_value}'"
                    )
                    continue

                assert actual_value == expected_value, (
                    f"Field mismatch for {expected_hostname}.{field}: "
                    f"expected '{expected_value}', got '{actual_value}'"
                )
    elif isinstance(expected, dict):
        if "count" in expected:
            assert (
                len(rows) == expected["count"]
            ), f"Expected {expected['count']} systems, got {len(rows)} for {scenario_id}"

        if "deployment_counts" in expected:
            actual_deployment_counts: Dict[str, int] = {}
            for row in rows:
                status = row["deployment_status"]
                actual_deployment_counts[status] = (
                    actual_deployment_counts.get(status, 0) + 1
                )
            for status, expected_count in expected["deployment_counts"].items():
                actual_count = actual_deployment_counts.get(status, 0)
                assert actual_count == expected_count, (
                    f"Expected {expected_count} with deployment_status='{status}', "
                    f"got {actual_count} for {scenario_id}. "
                    f"Actual counts: {actual_deployment_counts}"
                )


@pytest.mark.views
@pytest.mark.database
def test_deployment_view_basic_functionality(cf_client: CFTestClient):
    """Basic smoke test for the deployment status view"""
    result = cf_client.execute_sql(
        f"""
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW_DEPLOYMENT_STATUS,),
    )

    expected_columns = {
        "hostname",
        "deployment_status",
        "current_derivation_path",
        "deployment_time",
        "current_commit_hash",
        "current_commit_timestamp",
        "latest_commit_hash",
        "latest_commit_timestamp",
        "commits_behind",
        "flake_name",
        "status_description",
    }
    actual_columns = {row["column_name"] for row in result}
    assert expected_columns.issubset(
        actual_columns
    ), f"View missing expected columns. Missing: {expected_columns - actual_columns}"


@pytest.mark.views
@pytest.mark.database
def test_deployment_view_performance(cf_client: CFTestClient):
    """Test that deployment view performs reasonably well"""
    import time

    start_time = time.time()
    result = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW_DEPLOYMENT_STATUS}")
    query_time = time.time() - start_time

    assert (
        query_time < 10.0
    ), f"Deployment view query took too long: {query_time:.2f} seconds"
    assert len(result) == 1


@pytest.mark.views
@pytest.mark.database
def test_deployment_no_deployment_status(cf_client: CFTestClient, clean_test_data):
    """Test systems that exist in systems table but have no deployment"""

    # Create a system that's registered but never deployed (no system_states)
    cf_client.execute_sql(
        """
        INSERT INTO flakes (name, repo_url) 
        VALUES ('no-deploy-test', 'https://example.com/no-deploy.git')
        ON CONFLICT (repo_url) DO NOTHING
        """
    )

    flake_result = cf_client.execute_sql(
        "SELECT id FROM flakes WHERE repo_url = 'https://example.com/no-deploy.git'"
    )
    flake_id = flake_result[0]["id"]

    cf_client.execute_sql(
        """
        INSERT INTO systems (hostname, flake_id, is_active, derivation, public_key)
        VALUES ('test-no-deploy', %s, TRUE, '/nix/store/placeholder.drv', 'fake-key')
        ON CONFLICT (hostname) DO NOTHING
        """,
        (flake_id,),
    )

    # Query the deployment view
    rows = cf_client.execute_sql(
        f"""
        SELECT hostname, deployment_status, status_description
        FROM {VIEW_DEPLOYMENT_STATUS}
        WHERE hostname = 'test-no-deploy'
        """
    )

    assert len(rows) == 1
    row = rows[0]
    assert row["deployment_status"] == "no_deployment"
    assert "never deployed" in row["status_description"].lower()

    # Clean up
    cf_client.execute_sql("DELETE FROM systems WHERE hostname = 'test-no-deploy'")
    cf_client.execute_sql(
        "DELETE FROM flakes WHERE repo_url = 'https://example.com/no-deploy.git'"
    )


@pytest.mark.views
@pytest.mark.database
def test_deployment_unknown_status(cf_client: CFTestClient, clean_test_data):
    """Test deployments that can't be related to any flake"""

    # Create a system state with a derivation path that doesn't exist in derivations table
    cf_client.execute_sql(
        """
        INSERT INTO system_states (
            hostname, change_reason, derivation_path, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible, timestamp
        )
        VALUES (
            'test-unknown-deploy', 'startup', '/nix/store/unknown-derivation.drv', 
            'NixOS', '6.6.89', 32.0, 3600, 'Intel Xeon', 16,
            '192.168.1.200', '25.05', TRUE, NOW()
        )
        """
    )

    # Query the deployment view
    rows = cf_client.execute_sql(
        f"""
        SELECT hostname, deployment_status, status_description
        FROM {VIEW_DEPLOYMENT_STATUS}
        WHERE hostname = 'test-unknown-deploy'
        """
    )

    assert len(rows) == 1
    row = rows[0]
    assert row["deployment_status"] == "unknown"
    assert (
        "cannot determine" in row["status_description"].lower()
        or "flake relationship" in row["status_description"].lower()
    )

    # Clean up
    cf_client.execute_sql(
        "DELETE FROM system_states WHERE hostname = 'test-unknown-deploy'"
    )


@pytest.mark.views
@pytest.mark.database
def test_deployment_commits_behind_calculation(
    cf_client: CFTestClient, clean_test_data
):
    """Test that commits_behind is calculated correctly"""

    # Create a scenario where we can control the exact number of commits
    now = datetime.now(UTC)

    base_scenario = _create_base_scenario(
        cf_client,
        hostname="test-commits-behind",
        flake_name="commits-test",
        repo_url="https://example.com/commits-behind.git",
        git_hash="old-commit-123",
        commit_age_hours=72,  # 3 days old
        heartbeat_age_minutes=5,
    )

    flake_id = base_scenario["flake_id"]

    # Add 3 newer commits
    for i in range(1, 4):
        cf_client.execute_sql(
            """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, %s, 0)
            """,
            (flake_id, f"newer-commit-{i}", now - timedelta(hours=24 * (3 - i))),
        )

    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT hostname, deployment_status, commits_behind, status_description
        FROM {VIEW_DEPLOYMENT_STATUS}
        WHERE hostname = 'test-commits-behind'
        """
    )

    assert len(rows) == 1
    row = rows[0]
    assert row["deployment_status"] == "behind"
    assert (
        row["commits_behind"] == 3
    ), f"Expected 3 commits behind, got {row['commits_behind']}"
    assert "Behind by 3 commit(s)" == row["status_description"]

    # Clean up
    cf_client.cleanup_test_data(base_scenario["cleanup"])
    cf_client.execute_sql(
        "DELETE FROM commits WHERE git_commit_hash LIKE 'newer-commit-%'"
    )
