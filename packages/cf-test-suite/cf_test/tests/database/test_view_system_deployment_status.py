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
                "deployment_status": "up_to_date",
                "commits_behind": 0,
                "status_description": "Running newest deployable system build",
            }
        ],
    },
    {
        "id": "build_timeout",
        "builder": scenario_build_timeout,
        "expected": [
            {
                "hostname": "test-build-timeout",
                "deployment_status": "unknown",
                "commits_behind": 0,
                "status_description": "Cannot determine deployable flake relationship",
            }
        ],
    },
    {
        "id": "rollback",
        "builder": scenario_rollback,
        "expected": [
            {
                "hostname": "test-rollback",
                "deployment_status": "behind",
                "commits_behind": 1,
                "status_description": "Running an older system build; a newer deployable build is available",
            }
        ],
    },
    {
        "id": "partial_rebuild",
        "builder": scenario_partial_rebuild,
        "expected": [
            {
                "hostname": "test-partial-rebuild",
                "deployment_status": "up_to_date",
                "commits_behind": 0,
                "status_description": "Running newest deployable system build",
            }
        ],
    },
    {
        "id": "compliance_drift",
        "builder": scenario_compliance_drift,
        "expected": [
            {
                "hostname": "test-compliance-drift",
                "deployment_status": "behind",
                "commits_behind": 1,
                "status_description": "Running an older system build; a newer deployable build is available",
            }
        ],
    },
    {
        "id": "flaky_agent",
        "builder": scenario_flaky_agent,
        "expected": [
            {
                "hostname": "test-flaky-agent",
                "deployment_status": "unknown",
                "commits_behind": 0,
                "status_description": "Cannot determine deployable flake relationship",
            }
        ],
    },
    {
        "id": "never_seen",
        "builder": scenario_never_seen,
        "expected": [
            {
                "hostname": "test-never-seen",
                "deployment_status": "up_to_date",
                "commits_behind": 0,
                "status_description": "Running newest deployable system build",
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
                "status_description": "Running newest deployable system build",
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
                "commits_behind": 1,
                "status_description": "Running an older system build; a newer deployable build is available",
            }
        ],
    },
    {
        "id": "eval_failed",
        "builder": scenario_eval_failed,
        "expected": [
            {
                "hostname": "test-eval-failed",
                "deployment_status": "up_to_date",
                "commits_behind": 0,
                "status_description": "Running newest deployable system build",
            }
        ],
    },
    {
        "id": "mixed_commit_lag",
        "builder": scenario_mixed_commit_lag,
        "expected": [
            {"hostname": "test-mixed-1", "deployment_status": "up_to_date", "commits_behind": 0},
            {"hostname": "test-mixed-2", "deployment_status": "up_to_date", "commits_behind": 0},
            {
                "hostname": "test-mixed-3",
                "deployment_status": "behind",
                "commits_behind": 1,
                "status_description": "Running an older system build; a newer deployable build is available",
            },
            {"hostname": "test-mixed-4", "deployment_status": "up_to_date", "commits_behind": 0},
        ],
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


@pytest.fixture
def deployment_artifacts(cf_client, clean_test_data):
    """Publish only this test's eligible builds; remove cache jobs before shared cleanup."""
    cache_job_ids = []

    def publish(derivation_id):
        cf_client.execute_sql(
            """UPDATE derivations SET cf_agent_enabled = TRUE,
                      policy_requirements_met = TRUE, error_message = NULL
               WHERE id = %s""",
            (derivation_id,),
        )
        cache_job_ids.append(
            _one_row(
                cf_client,
                """INSERT INTO cache_push_jobs (derivation_id, status, store_path)
                   SELECT id, 'completed', store_path FROM derivations WHERE id = %s
                   RETURNING id""",
                (derivation_id,),
            )["id"]
        )

    yield publish

    if cache_job_ids:
        cf_client.execute_sql(
            "DELETE FROM cache_push_jobs WHERE id = ANY(%s)", (cache_job_ids,)
        )


def _add_deployable_build(cf_client, publish, commit_id, hostname, path):
    derivation = _one_row(
        cf_client,
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, derivation_path, store_path,
               status_id, attempt_count, completed_at
           ) VALUES (
               %s, 'nixos', %s, %s, %s,
               (SELECT id FROM derivation_statuses WHERE name = 'build-complete'), 0, NOW()
           ) RETURNING id""",
        (commit_id, hostname, path, path),
    )
    publish(derivation["id"])


@pytest.mark.vm_internal
@pytest.mark.views
@pytest.mark.database
@pytest.mark.parametrize(
    "scenario_config", DEPLOYMENT_SCENARIO_CONFIGS, ids=lambda x: x["id"]
)
def test_deployment_status_scenarios(
    cf_client: CFTestClient, deployment_artifacts, scenario_config: Dict[str, Any]
):
    """Test deployment status view with all scenarios"""
    builder = scenario_config["builder"]
    expected = scenario_config["expected"]
    scenario_id = scenario_config["id"]

    # Build the scenario
    scenario_data = builder(cf_client)

    if scenario_id in {
        "agent_restart", "partial_rebuild", "never_seen", "up_to_date", "eval_failed"
    }:
        deployment_artifacts(scenario_data["derivation_id"])
    elif scenario_id == "rollback":
        for derivation_id in scenario_data["derivation_ids"]:
            deployment_artifacts(derivation_id)
    elif scenario_id == "behind":
        deployment_artifacts(scenario_data["derivation_id"])
        _add_deployable_build(
            cf_client,
            deployment_artifacts,
            scenario_data["additional_commit_ids"][0],
            scenario_data["hostname"],
            "/nix/store/new789co-nixos-system-test-behind.drv",
        )
    elif scenario_id == "compliance_drift":
        cf_client.execute_sql(
            """UPDATE derivations
               SET status_id = (SELECT id FROM derivation_statuses WHERE name = 'build-complete')
               WHERE id = %s""",
            (scenario_data["derivation_id"],),
        )
        deployment_artifacts(scenario_data["derivation_id"])
        _add_deployable_build(
            cf_client,
            deployment_artifacts,
            scenario_data["recent_commit_ids"][-1],
            scenario_data["hostname"],
            "/nix/store/newest-compliance-drift-test.drv",
        )
    elif scenario_id == "mixed_commit_lag":
        flake_id = _one_row(
            cf_client, "SELECT flake_id FROM systems WHERE hostname = 'test-mixed-1'", ()
        )["flake_id"]
        for hostname in scenario_data["hostnames"]:
            derivation_id = _one_row(
                cf_client,
                """SELECT d.id FROM derivations d JOIN commits c ON c.id = d.commit_id
                   WHERE c.flake_id = %s AND d.derivation_name = %s
                   ORDER BY d.id DESC LIMIT 1""",
                (flake_id, hostname),
            )["id"]
            deployment_artifacts(derivation_id)
        latest_commit = _one_row(
            cf_client,
            "SELECT id FROM commits WHERE flake_id = %s AND git_commit_hash = 'mix123current'",
            (flake_id,),
        )["id"]
        _add_deployable_build(
            cf_client,
            deployment_artifacts,
            latest_commit,
            "test-mixed-3",
            "/nix/store/mix123current-nixos-system-test-mixed-3.drv",
        )

    # Determine hostnames to fetch from the view
    hostnames = _get_hostnames_from_deployment_scenario(scenario_data, scenario_id)

    # Query the deployment status view
    if hostnames:
        rows = cf_client.execute_sql(
            f"""
            SELECT hostname, deployment_status, current_store_path,
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
            SELECT hostname, deployment_status, current_store_path,
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
        "current_store_path",
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
            hostname, change_reason, store_path, os, kernel,
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
    cf_client: CFTestClient, deployment_artifacts
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

    cf_client.execute_sql(
        """UPDATE derivations
           SET status_id = (SELECT id FROM derivation_statuses WHERE name = 'build-complete')
           WHERE id = %s""",
        (base_scenario["derivation_id"],),
    )
    deployment_artifacts(base_scenario["derivation_id"])

    # Add 3 newer commits
    for i in range(1, 4):
        commit = _one_row(
            cf_client,
            """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, %s, 0) RETURNING id
            """,
            (flake_id, f"newer-commit-{i}", now - timedelta(hours=24 * (3 - i))),
        )
        _add_deployable_build(
            cf_client,
            deployment_artifacts,
            commit["id"],
            "test-commits-behind",
            f"/nix/store/newer-commit-{i}-test-commits-behind.drv",
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
    assert row["status_description"] == (
        "Running an older system build; a newer deployable build is available"
    )
