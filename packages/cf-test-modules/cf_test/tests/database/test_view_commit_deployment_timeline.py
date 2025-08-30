import json
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import (_create_base_scenario, scenario_agent_restart,
                               scenario_behind, scenario_compliance_drift,
                               scenario_eval_failed, scenario_rollback,
                               scenario_up_to_date)

VIEW_DEPLOYMENT_TIMELINE = "view_commit_deployment_timeline"

DEPLOYMENT_TIMELINE_SCENARIO_CONFIGS = [
    {
        "id": "up_to_date",
        "builder": scenario_up_to_date,
        "expected": {
            "has_commits": True,
            "has_successful_evaluations": True,
            "has_deployments": True,
            "evaluation_statuses": ["complete"]
        },
    },
    {
        "id": "behind",
        "builder": scenario_behind,
        "expected": {
            "has_commits": True,
            "has_successful_evaluations": True,
            "commit_count": 1,  # Behind scenario creates base commit
        },
    },
    {
        "id": "eval_failed",
        "builder": scenario_eval_failed,
        "expected": {
            "commit_count": 2,  # Working commit + failed commit
            "has_failed_evaluations": True,
            "has_successful_evaluations": True,
        },
    },
    {
        "id": "rollback",
        "builder": scenario_rollback,
        "expected": {
            "commit_count": 2,  # Old stable + new problematic
            "has_successful_evaluations": True,
            "has_deployments": True,
        },
    },
    {
        "id": "compliance_drift", 
        "builder": scenario_compliance_drift,
        "expected": {
            "commit_count": 6,  # Only 6 commits within 30-day window (30-day commit filtered out)
            "has_successful_evaluations": True,
            "has_old_deployments": False,  # Ancient deployment is outside 30-day window
        },
    },
    {
        "id": "agent_restart",
        "builder": scenario_agent_restart,
        "expected": {
            "has_commits": True,
            "has_successful_evaluations": True,
            "has_deployments": True,  # Should have deployments but maybe not recent
        },
    },
]


def _get_commit_hashes_from_timeline_scenario(
    scenario_data: Dict[str, Any], scenario_id: str
) -> List[str]:
    """Extract commit hashes from scenario data"""
    cleanup = scenario_data.get("cleanup", {})
    commit_patterns = cleanup.get("commits", [])
    
    # Extract hash patterns from commit cleanup
    hashes = []
    for pattern in commit_patterns:
        if "git_commit_hash" in pattern and "=" in pattern:
            # Pattern like "git_commit_hash = 'abc123'"
            parts = pattern.split("=")
            if len(parts) >= 2:
                hash_part = parts[1].strip().strip("'\"")
                if hash_part and not hash_part.startswith("IN"):
                    hashes.append(hash_part)
        elif "git_commit_hash IN" in pattern:
            # Pattern like "git_commit_hash IN ('hash1','hash2')"
            start = pattern.find("(") + 1
            end = pattern.find(")")
            if start > 0 and end > start:
                hash_list = pattern[start:end]
                for h in hash_list.split(","):
                    clean_hash = h.strip().strip("'\"")
                    if clean_hash:
                        hashes.append(clean_hash)
    
    return hashes


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
@pytest.mark.parametrize("scenario_config", DEPLOYMENT_TIMELINE_SCENARIO_CONFIGS, ids=lambda x: x["id"])
def test_deployment_timeline_scenarios(
    cf_client: CFTestClient, clean_test_data, scenario_config: Dict[str, Any]
):
    """Test deployment timeline view with scenarios"""
    builder = scenario_config["builder"]
    expected = scenario_config["expected"]
    scenario_id = scenario_config["id"]

    # Build the scenario
    scenario_data = builder(cf_client)

    # Get commit hashes to filter the view
    commit_hashes = _get_commit_hashes_from_timeline_scenario(scenario_data, scenario_id)
    
    # Query the deployment timeline view
    if commit_hashes:
        rows = cf_client.execute_sql(
            f"""
            SELECT flake_name, commit_id, git_commit_hash, short_hash, commit_timestamp,
                   total_evaluations, successful_evaluations, evaluation_statuses,
                   evaluated_targets, first_deployment, last_deployment,
                   total_systems_deployed, currently_deployed_systems,
                   deployed_systems, currently_deployed_systems_list
            FROM {VIEW_DEPLOYMENT_TIMELINE}
            WHERE git_commit_hash = ANY(%s)
            ORDER BY commit_timestamp DESC
            """,
            (commit_hashes,)
        )
    else:
        # Fallback: query by hostname pattern
        if "hostname" in scenario_data:
            hostname = scenario_data["hostname"]
            rows = cf_client.execute_sql(
                f"""
                SELECT flake_name, commit_id, git_commit_hash, short_hash, commit_timestamp,
                       total_evaluations, successful_evaluations, evaluation_statuses,
                       evaluated_targets, first_deployment, last_deployment,
                       total_systems_deployed, currently_deployed_systems,
                       deployed_systems, currently_deployed_systems_list
                FROM {VIEW_DEPLOYMENT_TIMELINE}
                WHERE evaluated_targets LIKE %s
                ORDER BY commit_timestamp DESC
                """,
                (f"%{hostname}%",)
            )
        else:
            rows = []

    # Save results for debugging
    try:
        log_path = Path("/tmp/cf_deployment_timeline_scenario_results.json")
        log_path.parent.mkdir(parents=True, exist_ok=True)
        with log_path.open("a", encoding="utf-8") as fh:
            fh.write(
                json.dumps({"scenario": scenario_id, "commit_hashes": commit_hashes, "rows": rows}, default=str) + "\n"
            )
    except Exception:
        pass

    # Validate results
    assert len(rows) > 0, f"No commits found for scenario {scenario_id} with hashes {commit_hashes}"

    # Validate expectations
    if "has_commits" in expected and expected["has_commits"]:
        assert len(rows) > 0, "Expected commits in timeline"

    if "commit_count" in expected:
        commit_ids = set(r["commit_id"] for r in rows)
        assert len(commit_ids) >= expected["commit_count"], (
            f"Expected at least {expected['commit_count']} commits, got {len(commit_ids)}"
        )

    if "has_successful_evaluations" in expected and expected["has_successful_evaluations"]:
        successful_rows = [r for r in rows if r["successful_evaluations"] > 0]
        assert len(successful_rows) > 0, "Expected commits with successful evaluations"

    if "has_failed_evaluations" in expected and expected["has_failed_evaluations"]:
        # Look for commits where total_evaluations > successful_evaluations
        failed_rows = [r for r in rows if r["total_evaluations"] > r["successful_evaluations"]]
        assert len(failed_rows) > 0, "Expected commits with failed evaluations"

    if "has_deployments" in expected and expected["has_deployments"]:
        deployed_rows = [r for r in rows if r["total_systems_deployed"] > 0]
        assert len(deployed_rows) > 0, "Expected commits with deployments"

    if "has_recent_deployments" in expected and expected["has_recent_deployments"]:
        now = datetime.now(UTC)
        recent_cutoff = now - timedelta(hours=1)
        recent_rows = [r for r in rows if r["first_deployment"] and r["first_deployment"] > recent_cutoff]
        assert len(recent_rows) > 0, "Expected commits with recent deployments"

    if "has_old_deployments" in expected and expected["has_old_deployments"]:
        now = datetime.now(UTC)
        old_cutoff = now - timedelta(days=1)
        old_rows = [r for r in rows if r["first_deployment"] and r["first_deployment"] < old_cutoff]
        assert len(old_rows) > 0, "Expected commits with old deployments"

    if "evaluation_statuses" in expected:
        expected_statuses = expected["evaluation_statuses"]
        found_statuses = []
        for row in rows:
            if row["evaluation_statuses"]:
                found_statuses.extend(row["evaluation_statuses"].split(", "))
        
        for expected_status in expected_statuses:
            assert expected_status in found_statuses, (
                f"Expected evaluation status '{expected_status}' not found. "
                f"Available statuses: {found_statuses}"
            )


@pytest.mark.views
@pytest.mark.database
def test_deployment_timeline_view_basic_functionality(cf_client: CFTestClient):
    """Basic smoke test for the deployment timeline view"""
    result = cf_client.execute_sql(
        f"""
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW_DEPLOYMENT_TIMELINE,),
    )

    expected_columns = {
        "flake_name", "commit_id", "git_commit_hash", "short_hash", "commit_timestamp",
        "total_evaluations", "successful_evaluations", "evaluation_statuses",
        "evaluated_targets", "first_deployment", "last_deployment",
        "total_systems_deployed", "currently_deployed_systems",
        "deployed_systems", "currently_deployed_systems_list"
    }
    actual_columns = {row["column_name"] for row in result}
    assert expected_columns.issubset(
        actual_columns
    ), f"View missing expected columns. Missing: {expected_columns - actual_columns}"


@pytest.mark.views
@pytest.mark.database
def test_deployment_timeline_view_performance(cf_client: CFTestClient):
    """Test that deployment timeline view performs reasonably well"""
    import time

    start_time = time.time()
    result = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW_DEPLOYMENT_TIMELINE}")
    query_time = time.time() - start_time

    assert query_time < 15.0, f"Deployment timeline view query took too long: {query_time:.2f} seconds"
    assert len(result) == 1


@pytest.mark.views
@pytest.mark.database
def test_deployment_timeline_ordering(cf_client: CFTestClient, clean_test_data):
    """Test that commits are ordered by timestamp descending"""
    
    now = datetime.now(UTC)
    
    # Create commits at different times within last 30 days (view filter)
    scenarios = []
    for i, hours_ago in enumerate([24, 168, 336]):  # 1 day, 1 week, 2 weeks ago
        scenario = _create_base_scenario(
            cf_client,
            hostname=f"test-timeline-order-{i}",
            flake_name=f"timeline-order-{i}",
            repo_url=f"https://example.com/timeline-order-{i}.git",
            git_hash=f"timeline-{i}-{int((now - timedelta(hours=hours_ago)).timestamp())}",
            commit_age_hours=hours_ago,
            heartbeat_age_minutes=None
        )
        scenarios.append(scenario)
    
    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT git_commit_hash, commit_timestamp
        FROM {VIEW_DEPLOYMENT_TIMELINE}
        WHERE git_commit_hash LIKE 'timeline-%'
        ORDER BY commit_timestamp DESC
        """
    )
    
    assert len(rows) >= 3, f"Expected at least 3 timeline test commits, got {len(rows)}"
    
    # Verify timestamps are in descending order
    for i in range(len(rows) - 1):
        assert rows[i]["commit_timestamp"] >= rows[i + 1]["commit_timestamp"], (
            f"Commits not in timestamp descending order: "
            f"{rows[i]['commit_timestamp']} should be >= {rows[i + 1]['commit_timestamp']}"
        )
    
    # Clean up
    for scenario in scenarios:
        cf_client.cleanup_test_data(scenario["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_deployment_timeline_30_day_filter(cf_client: CFTestClient, clean_test_data):
    """Test that view only shows commits from last 30 days"""
    
    now = datetime.now(UTC)
    
    # Create a commit older than 30 days
    old_scenario = _create_base_scenario(
        cf_client,
        hostname="test-old-commit-filter",
        flake_name="old-commit-filter",
        repo_url="https://example.com/old-commit-filter.git",
        git_hash=f"old-commit-{int((now - timedelta(days=35)).timestamp())}",
        commit_age_hours=24 * 35,  # 35 days old
        heartbeat_age_minutes=None
    )
    
    # Create a recent commit (within 30 days)
    recent_scenario = _create_base_scenario(
        cf_client,
        hostname="test-recent-commit-filter",
        flake_name="recent-commit-filter", 
        repo_url="https://example.com/recent-commit-filter.git",
        git_hash=f"recent-commit-{int((now - timedelta(days=7)).timestamp())}",
        commit_age_hours=24 * 7,  # 7 days old
        heartbeat_age_minutes=None
    )
    
    # Query the view
    old_rows = cf_client.execute_sql(
        f"""
        SELECT git_commit_hash, commit_timestamp
        FROM {VIEW_DEPLOYMENT_TIMELINE}
        WHERE git_commit_hash LIKE 'old-commit-%'
        """
    )
    
    recent_rows = cf_client.execute_sql(
        f"""
        SELECT git_commit_hash, commit_timestamp  
        FROM {VIEW_DEPLOYMENT_TIMELINE}
        WHERE git_commit_hash LIKE 'recent-commit-%'
        """
    )
    
    # Old commit should not appear (filtered out by 30-day limit)
    assert len(old_rows) == 0, f"Expected no old commits (>30 days), but found {len(old_rows)}"
    
    # Recent commit should appear
    assert len(recent_rows) >= 1, f"Expected recent commit to appear, but found {len(recent_rows)}"
    
    # Clean up
    cf_client.cleanup_test_data(old_scenario["cleanup"])
    cf_client.cleanup_test_data(recent_scenario["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_deployment_approximation_logic(cf_client: CFTestClient, clean_test_data):
    """Test that deployment approximation works based on system_states timestamps"""
    
    now = datetime.now(UTC)
    commit_time = now - timedelta(hours=2)
    
    # Create base scenario
    scenario = _create_base_scenario(
        cf_client,
        hostname="test-deploy-approx",
        flake_name="deploy-approx",
        repo_url="https://example.com/deploy-approx.git",
        git_hash=f"deploy-approx-{int(commit_time.timestamp())}",
        commit_age_hours=2,
        heartbeat_age_minutes=None
    )
    
    # Update system_states to have timestamps AFTER commit time (simulating deployment)
    deployment_time = commit_time + timedelta(minutes=30)  # Deployed 30 min after commit
    cf_client.execute_sql(
        """
        UPDATE system_states 
        SET timestamp = %s
        WHERE hostname = %s
        """,
        (deployment_time, "test-deploy-approx")
    )
    
    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT git_commit_hash, first_deployment, total_systems_deployed,
               deployed_systems, commit_timestamp
        FROM {VIEW_DEPLOYMENT_TIMELINE}
        WHERE git_commit_hash LIKE 'deploy-approx-%'
        """
    )
    
    assert len(rows) == 1, f"Expected 1 commit, got {len(rows)}"
    
    row = rows[0]
    assert row["total_systems_deployed"] == 1, "Expected 1 system deployed"
    assert row["deployed_systems"] == "test-deploy-approx", "Expected correct system name"
    assert row["first_deployment"] == deployment_time, "Expected correct deployment time"
    
    # Clean up
    cf_client.cleanup_test_data(scenario["cleanup"])


@pytest.mark.views
@pytest.mark.database  
def test_currently_active_systems_logic(cf_client: CFTestClient, clean_test_data):
    """Test that currently active systems are correctly identified"""
    
    # Create scenario with system that was deployed and is still active
    scenario = _create_base_scenario(
        cf_client,
        hostname="test-currently-active",
        flake_name="currently-active",
        repo_url="https://example.com/currently-active.git", 
        git_hash="currently-active-123",
        commit_age_hours=1,
        heartbeat_age_minutes=5  # Recent heartbeat = currently active
    )
    
    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT git_commit_hash, currently_deployed_systems, 
               currently_deployed_systems_list, total_systems_deployed
        FROM {VIEW_DEPLOYMENT_TIMELINE}
        WHERE git_commit_hash = 'currently-active-123'
        """
    )
    
    assert len(rows) == 1, f"Expected 1 commit, got {len(rows)}"
    
    row = rows[0]
    assert row["currently_deployed_systems"] >= 1, "Expected at least 1 currently active system"
    assert "test-currently-active" in (row["currently_deployed_systems_list"] or ""), "Expected system in active list"
    
    # Clean up
    cf_client.cleanup_test_data(scenario["cleanup"])
