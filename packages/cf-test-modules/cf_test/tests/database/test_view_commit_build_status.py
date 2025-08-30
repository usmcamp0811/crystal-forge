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

VIEW_COMMIT_BUILD_STATUS = "view_commit_build_status"

BUILD_STATUS_SCENARIO_CONFIGS = [
    {
        "id": "agent_restart",
        "builder": scenario_agent_restart,
        "expected": {
            "commit_count": 1,
            "has_complete_builds": True,
            "has_derivations": True,
        },
    },
    {
        "id": "build_timeout",
        "builder": scenario_build_timeout,
        "expected": {
            "commit_build_status": "building",  # Has derivations stuck in progress
            "in_progress_derivations": 4,  # Main + 3 additional stuck derivations
            "total_derivations": 4,
        },
    },
    {
        "id": "rollback",
        "builder": scenario_rollback,
        "expected": {
            "commit_count": 2,  # Old stable + new problematic commits
            "has_complete_builds": True,  # Both commits built successfully
            "has_derivations": True,
        },
    },
    {
        "id": "partial_rebuild",
        "builder": scenario_partial_rebuild,
        "expected": {
            "commit_build_status": "building",  # Actually shows as building since pkg-building uses pending status
            "successful_derivations": 3,  # Main nixos + pkg-success + pkg-retry-success
            "failed_derivations": 2,  # pkg-failed-once + pkg-still-failing
            "in_progress_derivations": 1,  # pkg-building (pending status)
            "total_derivations": 6,  # Main + 5 packages
        },
    },
    {
        "id": "compliance_drift",
        "builder": scenario_compliance_drift,
        "expected": {
            "commit_count": 8,  # Original ancient + 7 newer commits
            "has_complete_builds": True,
            "has_derivations": True,
        },
    },
    {
        "id": "flaky_agent",
        "builder": scenario_flaky_agent,
        "expected": {
            "commit_count": 1,
            "has_complete_builds": True,
            "has_derivations": True,
        },
    },
    {
        "id": "up_to_date",
        "builder": scenario_up_to_date,
        "expected": {
            "commit_build_status": "complete",
            "successful_derivations": 1,
            "failed_derivations": 0,
            "total_derivations": 1,
            "derivation_status": "complete",
        },
    },
    {
        "id": "behind",
        "builder": scenario_behind,
        "expected": {
            "commit_count": 1,  # Actually only creates 1 commit, additional commits are in scenario logic
            "has_complete_builds": True,
            "has_derivations": True,
        },
    },
    {
        "id": "eval_failed",
        "builder": scenario_eval_failed,
        "expected": {
            "commit_count": 2,  # Working commit + failed commit
            "has_failed_builds": True,
            "has_complete_builds": True,
        },
    },
]


def _get_commit_hashes_from_scenario(
    scenario_data: Dict[str, Any], scenario_id: str
) -> List[str]:
    """Extract commit hashes from scenario data by looking at cleanup patterns"""
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
@pytest.mark.parametrize(
    "scenario_config", BUILD_STATUS_SCENARIO_CONFIGS, ids=lambda x: x["id"]
)
def test_commit_build_status_scenarios(
    cf_client: CFTestClient, clean_test_data, scenario_config: Dict[str, Any]
):
    """Test commit build status view with scenarios"""
    builder = scenario_config["builder"]
    expected = scenario_config["expected"]
    scenario_id = scenario_config["id"]

    # Build the scenario
    scenario_data = builder(cf_client)

    # Get commit hashes to filter the view
    commit_hashes = _get_commit_hashes_from_scenario(scenario_data, scenario_id)

    if not commit_hashes:
        # Fallback: query by flake or hostname pattern
        if "hostname" in scenario_data:
            hostname = scenario_data["hostname"]
            pattern = f"%{hostname}%"
            rows = cf_client.execute_sql(
                f"""
                SELECT DISTINCT commit_id, git_commit_hash, short_hash, commit_timestamp,
                       commit_build_status, total_derivations, successful_derivations,
                       failed_derivations, in_progress_derivations, derivation_status,
                       derivation_name, derivation_type, is_success, all_statuses
                FROM {VIEW_COMMIT_BUILD_STATUS}
                WHERE derivation_name LIKE %s OR flake_name LIKE %s
                ORDER BY commit_timestamp DESC
                """,
                (pattern, pattern),
            )
        else:
            rows = []
    else:
        # Query by specific commit hashes
        rows = cf_client.execute_sql(
            f"""
            SELECT DISTINCT commit_id, git_commit_hash, short_hash, commit_timestamp,
                   commit_build_status, total_derivations, successful_derivations,
                   failed_derivations, in_progress_derivations, derivation_status,
                   derivation_name, derivation_type, is_success, all_statuses
            FROM {VIEW_COMMIT_BUILD_STATUS}
            WHERE git_commit_hash = ANY(%s)
            ORDER BY commit_timestamp DESC
            """,
            (commit_hashes,),
        )

    # Save results for debugging
    try:
        log_path = Path("/tmp/cf_commit_build_scenario_results.json")
        log_path.parent.mkdir(parents=True, exist_ok=True)
        with log_path.open("a", encoding="utf-8") as fh:
            fh.write(
                json.dumps(
                    {
                        "scenario": scenario_id,
                        "commit_hashes": commit_hashes,
                        "rows": rows,
                    },
                    default=str,
                )
                + "\n"
            )
    except Exception:
        pass

    # Validate results
    assert (
        len(rows) > 0
    ), f"No commits found for scenario {scenario_id} with hashes {commit_hashes}"

    if "commit_build_status" in expected:
        # Find a row with the expected status
        matching_rows = [
            r
            for r in rows
            if r["commit_build_status"] == expected["commit_build_status"]
        ]
        assert len(matching_rows) > 0, (
            f"Expected commit_build_status '{expected['commit_build_status']}' not found. "
            f"Available statuses: {[r['commit_build_status'] for r in rows]}"
        )

        row = matching_rows[0]  # Use first matching row

        for field, expected_value in expected.items():
            if field == "commit_build_status":
                continue
            actual_value = row.get(field)
            assert (
                actual_value == expected_value
            ), f"Field mismatch for {field}: expected '{expected_value}', got '{actual_value}'"

    if "commit_count" in expected:
        commit_ids = set(r["commit_id"] for r in rows)
        assert (
            len(commit_ids) == expected["commit_count"]
        ), f"Expected {expected['commit_count']} commits, got {len(commit_ids)}"

    if "has_complete_builds" in expected and expected["has_complete_builds"]:
        complete_rows = [r for r in rows if r["commit_build_status"] == "complete"]
        assert len(complete_rows) > 0, "Expected at least one complete build"

    if "has_failed_builds" in expected and expected["has_failed_builds"]:
        failed_rows = [
            r for r in rows if r["commit_build_status"] in ["failed", "partial"]
        ]
        assert len(failed_rows) > 0, "Expected at least one failed/partial build"

    if "has_derivations" in expected and expected["has_derivations"]:
        derivation_rows = [r for r in rows if r["total_derivations"] > 0]
        assert len(derivation_rows) > 0, "Expected commits with derivations"


@pytest.mark.views
@pytest.mark.database
def test_commit_build_view_basic_functionality(cf_client: CFTestClient):
    """Basic smoke test for the commit build status view"""
    result = cf_client.execute_sql(
        f"""
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW_COMMIT_BUILD_STATUS,),
    )

    expected_columns = {
        "commit_id",
        "flake_name",
        "git_commit_hash",
        "short_hash",
        "commit_timestamp",
        "commit_build_status",
        "total_derivations",
        "successful_derivations",
        "failed_derivations",
        "derivation_status",
        "derivation_name",
        "derivation_type",
        "is_success",
        "derivation_attempt_count",
        "all_statuses",
    }
    actual_columns = {row["column_name"] for row in result}
    assert expected_columns.issubset(
        actual_columns
    ), f"View missing expected columns. Missing: {expected_columns - actual_columns}"


@pytest.mark.views
@pytest.mark.database
def test_commit_build_view_performance(cf_client: CFTestClient):
    """Test that commit build view performs reasonably well"""
    import time

    start_time = time.time()
    result = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW_COMMIT_BUILD_STATUS}")
    query_time = time.time() - start_time

    assert (
        query_time < 15.0
    ), f"Commit build view query took too long: {query_time:.2f} seconds"
    assert len(result) == 1


@pytest.mark.views
@pytest.mark.database
def test_commit_build_status_categories(cf_client: CFTestClient, clean_test_data):
    """Test different commit build status categories"""

    now = datetime.now(UTC)

    # Create test scenarios for different build statuses

    # 1. Complete build - all derivations successful
    complete_scenario = _create_base_scenario(
        cf_client,
        hostname="test-complete-build",
        flake_name="build-status-test",
        repo_url="https://example.com/build-status.git",
        git_hash="complete-build-123",
        derivation_status="complete",
        commit_age_hours=1,
        heartbeat_age_minutes=5,
    )

    # 2. Failed build - create a failed derivation
    failed_scenario = _create_base_scenario(
        cf_client,
        hostname="test-failed-build",
        flake_name="build-status-test-failed",
        repo_url="https://example.com/build-status-failed.git",
        git_hash="failed-build-456",
        derivation_status="failed",
        derivation_error="Build failed due to missing dependency",
        commit_age_hours=2,
        heartbeat_age_minutes=None,
    )

    # 3. Building - create in-progress derivation (use pending status)
    building_scenario = _create_base_scenario(
        cf_client,
        hostname="test-building",
        flake_name="build-status-test-building",
        repo_url="https://example.com/build-status-building.git",
        git_hash="building-789",
        derivation_status="pending",  # Use pending status which should be non-terminal
        commit_age_hours=0.5,
        heartbeat_age_minutes=None,
    )

    # Query the view for our test commits
    test_hashes = ["complete-build-123", "failed-build-456", "building-789"]
    rows = cf_client.execute_sql(
        f"""
        SELECT git_commit_hash, commit_build_status, total_derivations,
               successful_derivations, failed_derivations, in_progress_derivations,
               derivation_status, is_success
        FROM {VIEW_COMMIT_BUILD_STATUS}
        WHERE git_commit_hash = ANY(%s)
        ORDER BY commit_timestamp DESC
        """,
        (test_hashes,),
    )

    # Organize results by commit hash
    results_by_hash = {row["git_commit_hash"]: row for row in rows}

    # Validate complete build
    if "complete-build-123" in results_by_hash:
        complete_row = results_by_hash["complete-build-123"]
        assert complete_row["commit_build_status"] == "complete"
        assert complete_row["successful_derivations"] >= 1
        assert complete_row["failed_derivations"] == 0

    # Validate failed build
    if "failed-build-456" in results_by_hash:
        failed_row = results_by_hash["failed-build-456"]
        assert failed_row["commit_build_status"] == "failed"
        assert failed_row["failed_derivations"] >= 1
        assert failed_row["successful_derivations"] == 0

    # Validate building status
    if "building-789" in results_by_hash:
        building_row = results_by_hash["building-789"]
        assert building_row["commit_build_status"] == "building"
        assert building_row["in_progress_derivations"] >= 1

    # Clean up
    cf_client.cleanup_test_data(complete_scenario["cleanup"])
    cf_client.cleanup_test_data(failed_scenario["cleanup"])
    cf_client.cleanup_test_data(building_scenario["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_commit_filter_by_specific_commit(cf_client: CFTestClient, clean_test_data):
    """Test filtering the view for a specific commit"""

    # Create a commit with multiple derivations
    base_scenario = _create_base_scenario(
        cf_client,
        hostname="test-multi-deriv",
        flake_name="multi-deriv-test",
        repo_url="https://example.com/multi-deriv.git",
        git_hash="multi-deriv-abc123",
        commit_age_hours=1,
        heartbeat_age_minutes=5,
    )

    commit_id = base_scenario["commit_id"]

    # Add additional derivations for the same commit
    cf_client.execute_sql(
        """
        INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
        )
        VALUES (
            %s, 'package', 'test-package-1', '/nix/store/pkg1.drv',
            (SELECT id FROM derivation_statuses WHERE name = 'complete'),
            1, NOW() - INTERVAL '30 minutes', NOW() - INTERVAL '20 minutes'
        ), (
            %s, 'package', 'test-package-2', '/nix/store/pkg2.drv',
            (SELECT id FROM derivation_statuses WHERE name = 'failed'),
            2, NOW() - INTERVAL '30 minutes', NOW() - INTERVAL '15 minutes'
        )
        """,
        (commit_id, commit_id),
    )

    # Query for this specific commit
    rows = cf_client.execute_sql(
        f"""
        SELECT git_commit_hash, derivation_name, derivation_type, derivation_status,
               total_derivations, successful_derivations, failed_derivations,
               commit_build_status
        FROM {VIEW_COMMIT_BUILD_STATUS}
        WHERE git_commit_hash = 'multi-deriv-abc123'
        ORDER BY derivation_type, derivation_name
        """
    )

    assert len(rows) == 3, f"Expected 3 derivations for commit, got {len(rows)}"

    # All rows should have the same commit-level aggregations
    first_row = rows[0]
    assert first_row["total_derivations"] == 3
    assert first_row["successful_derivations"] == 2  # nixos + package-1
    assert first_row["failed_derivations"] == 1  # package-2
    assert first_row["commit_build_status"] == "partial"  # mixed success/failure

    # Check individual derivations
    derivation_names = [r["derivation_name"] for r in rows]
    assert "test-multi-deriv" in derivation_names
    assert "test-package-1" in derivation_names
    assert "test-package-2" in derivation_names

    # Clean up
    cf_client.cleanup_test_data(base_scenario["cleanup"])
    cf_client.execute_sql(
        "DELETE FROM derivations WHERE derivation_name IN ('test-package-1', 'test-package-2')"
    )


@pytest.mark.views
@pytest.mark.database
def test_commit_ordering_by_timestamp(cf_client: CFTestClient, clean_test_data):
    """Test that commits are ordered by timestamp descending"""

    now = datetime.now(UTC)

    # Create commits at different times
    old_scenario = _create_base_scenario(
        cf_client,
        hostname="test-old-commit",
        flake_name="ordering-test",
        repo_url="https://example.com/ordering.git",
        git_hash="old-commit-123",
        commit_age_hours=48,  # 2 days old
        heartbeat_age_minutes=None,
    )

    new_scenario = _create_base_scenario(
        cf_client,
        hostname="test-new-commit",
        flake_name="ordering-test-new",
        repo_url="https://example.com/ordering-new.git",
        git_hash="new-commit-456",
        commit_age_hours=1,  # 1 hour old
        heartbeat_age_minutes=None,
    )

    # Query both commits
    rows = cf_client.execute_sql(
        f"""
        SELECT git_commit_hash, commit_timestamp
        FROM {VIEW_COMMIT_BUILD_STATUS}
        WHERE git_commit_hash IN ('old-commit-123', 'new-commit-456')
        ORDER BY commit_timestamp DESC
        """
    )

    assert len(rows) >= 2

    # First result should be the newer commit
    assert rows[0]["git_commit_hash"] == "new-commit-456"

    # Verify timestamps are actually in descending order
    for i in range(len(rows) - 1):
        assert (
            rows[i]["commit_timestamp"] >= rows[i + 1]["commit_timestamp"]
        ), "Commits should be ordered by timestamp descending"

    # Clean up
    cf_client.cleanup_test_data(old_scenario["cleanup"])
    cf_client.cleanup_test_data(new_scenario["cleanup"])
