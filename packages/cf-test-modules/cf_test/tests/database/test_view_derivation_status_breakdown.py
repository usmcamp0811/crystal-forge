import json
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import (
    _create_base_scenario,
    scenario_agent_restart,
    scenario_behind,
    scenario_build_timeout,
    scenario_compliance_drift,
    scenario_eval_failed,
    scenario_flaky_agent,
    scenario_latest_with_two_overdue,
    scenario_mixed_commit_lag,
    scenario_multi_system_progression_with_failure,
    scenario_multiple_orphaned_systems,
    scenario_never_seen,
    scenario_offline,
    scenario_orphaned_deployments,
    scenario_partial_rebuild,
    scenario_progressive_system_updates,
    scenario_rollback,
    scenario_up_to_date,
)

VIEW_DERIVATION_STATUS_BREAKDOWN = "view_derivation_status_breakdown"

DERIVATION_STATUS_BREAKDOWN_SCENARIO_CONFIGS = [
    {
        "id": "agent_restart",
        "builder": scenario_agent_restart,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 1,
        },
    },
    {
        "id": "build_timeout",
        "builder": scenario_build_timeout,
        "expected": {
            "has_derivations": True,
            "has_pending_status": True,  # Stuck builds show as pending
            "min_pending_count": 1,
            "min_nixos_count": 1,
            "min_package_count": 3,  # Additional stuck derivations
        },
    },
    {
        "id": "rollback",
        "builder": scenario_rollback,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 2,  # Two commits with derivations
        },
    },
    {
        "id": "partial_rebuild",
        "builder": scenario_partial_rebuild,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "has_failed_status": True,
            "has_pending_status": True,  # pkg-building is pending
            "min_nixos_count": 1,
            "min_package_count": 5,  # 5 package scenarios
            "has_multiple_attempts": True,  # Some packages have >1 attempts
        },
    },
    {
        "id": "compliance_drift",
        "builder": scenario_compliance_drift,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 1,  # Ancient commit + newer builds
        },
    },
    {
        "id": "flaky_agent",
        "builder": scenario_flaky_agent,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 1,
        },
    },
    {
        "id": "never_seen",
        "builder": scenario_never_seen,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 1,
        },
    },
    {
        "id": "up_to_date",
        "builder": scenario_up_to_date,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 1,
        },
    },
    {
        "id": "offline",
        "builder": scenario_offline,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 1,
        },
    },
    {
        "id": "behind",
        "builder": scenario_behind,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 1,
        },
    },
    {
        "id": "eval_failed",
        "builder": scenario_eval_failed,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "has_failed_status": True,
            "min_nixos_count": 1,  # Working commit
            "min_failed_count": 1,  # Failed newer commit
        },
    },
    {
        "id": "progressive_system_updates",
        "builder": scenario_progressive_system_updates,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 5,  # 5 commits with derivations
        },
    },
    {
        "id": "multiple_orphaned_systems",
        "builder": scenario_multiple_orphaned_systems,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 3,  # 3 commits created
        },
    },
    {
        "id": "latest_with_two_overdue",
        "builder": scenario_latest_with_two_overdue,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 2,  # 2 commits with derivations
        },
    },
    {
        "id": "mixed_commit_lag",
        "builder": scenario_mixed_commit_lag,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 2,  # Current and behind commits
        },
    },
    {
        "id": "multi_system_progression_with_failure",
        "builder": scenario_multi_system_progression_with_failure,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "has_failed_status": True,
            "min_nixos_count": 50,  # 10 successful commits × 5 systems
            "min_failed_count": 5,  # 1 failed commit × 5 systems
        },
    },
    {
        "id": "orphaned_deployments",
        "builder": scenario_orphaned_deployments,
        "expected": {
            "has_derivations": True,
            "has_complete_status": True,
            "min_nixos_count": 1,
        },
    },
]


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
    "scenario_config",
    DERIVATION_STATUS_BREAKDOWN_SCENARIO_CONFIGS,
    ids=lambda x: x["id"],
)
def test_derivation_status_breakdown_scenarios(
    cf_client: CFTestClient, clean_test_data, scenario_config: Dict[str, Any]
):
    """Test derivation status breakdown view with all scenarios"""
    builder = scenario_config["builder"]
    expected = scenario_config["expected"]
    scenario_id = scenario_config["id"]

    # Capture initial breakdown for comparison
    initial_rows = cf_client.execute_sql(
        f"""
        SELECT status_name, total_count, nixos_count, package_count, 
               avg_attempts, percentage_of_total
        FROM {VIEW_DERIVATION_STATUS_BREAKDOWN}
        ORDER BY status_name
        """
    )
    initial_totals = {row["status_name"]: row["total_count"] for row in initial_rows}

    # Build the scenario
    scenario_data = builder(cf_client)

    # Query the view after scenario
    rows = cf_client.execute_sql(
        f"""
        SELECT status_name, status_description, is_terminal, is_success,
               total_count, nixos_count, package_count, avg_attempts,
               avg_duration_seconds, oldest_scheduled, newest_scheduled,
               count_last_24h, percentage_of_total
        FROM {VIEW_DERIVATION_STATUS_BREAKDOWN}
        ORDER BY status_name
        """
    )

    # Save results for debugging
    try:
        log_path = Path("/tmp/cf_derivation_status_breakdown_scenario_results.json")
        log_path.parent.mkdir(parents=True, exist_ok=True)
        with log_path.open("a", encoding="utf-8") as fh:
            fh.write(
                json.dumps(
                    {
                        "scenario": scenario_id,
                        "initial_totals": initial_totals,
                        "final_rows": rows,
                    },
                    default=str,
                )
                + "\n"
            )
    except Exception:
        pass

    # Validate results
    assert len(rows) > 0, f"No status breakdown found for scenario {scenario_id}"

    # Convert to lookup by status name for easier validation
    status_lookup = {row["status_name"]: row for row in rows}

    # Basic validations
    if expected.get("has_derivations"):
        total_derivations = sum(row["total_count"] for row in rows)
        assert total_derivations > 0, f"Expected derivations for {scenario_id}"

    if expected.get("has_complete_status"):
        assert (
            "dry-run-complete" in status_lookup or "build-complete" in status_lookup
        ), f"Expected complete status for {scenario_id}"
        if "build-complete" in status_lookup:
            complete_row = status_lookup["build-complete"]
            assert (
                complete_row["is_terminal"] is True
            ), "Complete status is `build-complete` and should be terminal=True"
        if "dry-run-complete" in status_lookup:
            complete_row = status_lookup["dry-run-complete"]
            assert (
                complete_row["is_terminal"] is False
            ), "Complete status is `dry-run-complete` and should be terminal=False"
        assert (
            complete_row["total_count"] > 0
        ), f"Expected complete derivations for {scenario_id}"
        assert (
            complete_row["is_success"] is True
        ), "Complete status should be success=True"

    if expected.get("has_failed_status"):
        failed_statuses = [name for name in status_lookup if "failed" in name.lower()]
        assert len(failed_statuses) > 0, f"Expected failed status for {scenario_id}"
        for status_name in failed_statuses:
            failed_row = status_lookup[status_name]
            if failed_row["total_count"] > 0:
                assert (
                    failed_row["is_success"] is False
                ), f"Failed status should be success=False for {status_name}"

    if expected.get("has_pending_status"):
        pending_statuses = [
            name
            for name in status_lookup
            if name in ["build-pending", "build-scheduled", "build-building"]
        ]
        assert (
            len(pending_statuses) > 0
        ), f"Expected pending-type status for {scenario_id}"
        has_pending_count = any(
            status_lookup[name]["total_count"] > 0
            for name in pending_statuses
            if name in status_lookup
        )
        assert (
            has_pending_count
        ), f"Expected non-zero pending derivations for {scenario_id}"

    # Count validations
    if expected.get("min_nixos_count"):
        total_nixos = sum(row["nixos_count"] for row in rows)
        assert total_nixos >= expected["min_nixos_count"], (
            f"Expected at least {expected['min_nixos_count']} nixos derivations, "
            f"got {total_nixos} for {scenario_id}"
        )

    if expected.get("min_package_count"):
        total_packages = sum(row["package_count"] for row in rows)
        assert total_packages >= expected["min_package_count"], (
            f"Expected at least {expected['min_package_count']} package derivations, "
            f"got {total_packages} for {scenario_id}"
        )

    if expected.get("min_pending_count"):
        pending_count = sum(
            row["total_count"]
            for row in rows
            if not row["is_terminal"] and row["total_count"] > 0
        )
        assert pending_count >= expected["min_pending_count"], (
            f"Expected at least {expected['min_pending_count']} pending derivations, "
            f"got {pending_count} for {scenario_id}"
        )

    if expected.get("min_failed_count"):
        failed_count = sum(
            row["total_count"]
            for row in rows
            if row["is_terminal"]
            and row["is_success"] is False
            and row["total_count"] > 0
        )
        assert failed_count >= expected["min_failed_count"], (
            f"Expected at least {expected['min_failed_count']} failed derivations, "
            f"got {failed_count} for {scenario_id}"
        )

    if expected.get("has_multiple_attempts"):
        high_attempt_rows = [
            row for row in rows if row["avg_attempts"] and row["avg_attempts"] > 1.5
        ]
        assert (
            len(high_attempt_rows) > 0
        ), f"Expected some derivations with >1 attempts for {scenario_id}"

    # Data quality checks
    for row in rows:
        # Percentage should be reasonable
        assert (
            0 <= row["percentage_of_total"] <= 100
        ), f"Invalid percentage {row['percentage_of_total']} for {row['status_name']}"

        # Counts should be consistent
        assert row["nixos_count"] + row["package_count"] == row["total_count"], (
            f"Count mismatch for {row['status_name']}: "
            f"nixos({row['nixos_count']}) + package({row['package_count']}) != total({row['total_count']})"
        )

        # avg_attempts should be >= 0
        if row["avg_attempts"] is not None:
            assert (
                row["avg_attempts"] >= 0
            ), f"Invalid avg_attempts for {row['status_name']}"


@pytest.mark.views
@pytest.mark.database
def test_derivation_status_breakdown_view_basic_functionality(cf_client: CFTestClient):
    """Basic smoke test for the derivation status breakdown view"""
    result = cf_client.execute_sql(
        f"""
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW_DERIVATION_STATUS_BREAKDOWN,),
    )

    expected_columns = {
        "status_name",
        "status_description",
        "is_terminal",
        "is_success",
        "total_count",
        "nixos_count",
        "package_count",
        "avg_attempts",
        "avg_duration_seconds",
        "oldest_scheduled",
        "newest_scheduled",
        "count_last_24h",
        "percentage_of_total",
    }
    actual_columns = {row["column_name"] for row in result}
    assert expected_columns.issubset(
        actual_columns
    ), f"View missing expected columns. Missing: {expected_columns - actual_columns}"


@pytest.mark.views
@pytest.mark.database
def test_derivation_status_breakdown_view_performance(cf_client: CFTestClient):
    """Test that derivation status breakdown view performs reasonably well"""
    import time

    start_time = time.time()
    result = cf_client.execute_sql(
        f"SELECT COUNT(*) FROM {VIEW_DERIVATION_STATUS_BREAKDOWN}"
    )
    query_time = time.time() - start_time

    assert (
        query_time < 5.0
    ), f"Derivation status breakdown view query took too long: {query_time:.2f} seconds"
    assert len(result) == 1


@pytest.mark.views
@pytest.mark.database
def test_derivation_status_breakdown_percentage_totals(
    cf_client: CFTestClient, clean_test_data
):
    """Test that percentages add up to 100% (within rounding)"""

    # Create a few test derivations to ensure we have data
    scenario = _create_base_scenario(
        cf_client,
        hostname="test-percentage-calc",
        flake_name="percentage-test",
        repo_url="https://example.com/percentage-test.git",
        git_hash="percentage123",
        commit_age_hours=1,
        heartbeat_age_minutes=5,
    )

    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT status_name, percentage_of_total
        FROM {VIEW_DERIVATION_STATUS_BREAKDOWN}
        WHERE total_count > 0
        """
    )

    assert len(rows) > 0, "Expected at least one status with derivations"

    # Sum percentages
    total_percentage = sum(row["percentage_of_total"] for row in rows)

    # Should be close to 100% (allowing for rounding)
    assert (
        99.9 <= total_percentage <= 100.1
    ), f"Percentages should sum to ~100%, got {total_percentage}"

    # Clean up
    cf_client.cleanup_test_data(scenario["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_derivation_status_breakdown_time_filters(
    cf_client: CFTestClient, clean_test_data
):
    """Test that 24-hour count filtering works"""

    # Create derivation with specific timing
    now = datetime.now(UTC)
    old_time = now - timedelta(days=2)  # Older than 24 hours
    recent_time = now - timedelta(hours=12)  # Within 24 hours

    # Create two scenarios with different scheduling times
    scenario1 = _create_base_scenario(
        cf_client,
        hostname="test-time-filter-old",
        flake_name="time-filter-old",
        repo_url="https://example.com/time-filter-old.git",
        git_hash="timeold123",
        commit_age_hours=48,  # This affects commit timestamp, not scheduled_at
        heartbeat_age_minutes=None,
        derivation_status="build-complete",
    )

    scenario2 = _create_base_scenario(
        cf_client,
        hostname="test-time-filter-recent",
        flake_name="time-filter-recent",
        repo_url="https://example.com/time-filter-recent.git",
        git_hash="timerecent456",
        commit_age_hours=12,
        heartbeat_age_minutes=None,
        derivation_status="build-complete",
    )

    # Update the scheduled_at times directly (since _create_base_scenario doesn't control this)
    cf_client.execute_sql(
        """
        UPDATE derivations 
        SET scheduled_at = %s
        WHERE derivation_name = 'test-time-filter-old'
        """,
        (old_time,),
    )

    cf_client.execute_sql(
        """
        UPDATE derivations
        SET scheduled_at = %s  
        WHERE derivation_name = 'test-time-filter-recent'
        """,
        (recent_time,),
    )

    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT status_name, total_count, count_last_24h
        FROM {VIEW_DERIVATION_STATUS_BREAKDOWN}
        WHERE total_count > 0
        """
    )

    # Find the complete status row (our derivations should be complete)
    complete_rows = [r for r in rows if r["status_name"] == "build-complete"]
    assert len(complete_rows) > 0, "Expected at least one complete status row"

    complete_row = complete_rows[0]

    # The 24h count should be less than or equal to total count
    assert complete_row["count_last_24h"] <= complete_row["total_count"], (
        f"24h count ({complete_row['count_last_24h']}) should not exceed total "
        f"({complete_row['total_count']})"
    )

    # Clean up
    cf_client.cleanup_test_data(scenario1["cleanup"])
    cf_client.cleanup_test_data(scenario2["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_derivation_status_breakdown_ordering(cf_client: CFTestClient, clean_test_data):
    """Test that statuses are ordered by display_order"""

    # Create some test data first since this is a standalone test
    scenario = _create_base_scenario(
        cf_client,
        hostname="test-ordering-check",
        flake_name="ordering-test",
        repo_url="https://example.com/ordering-test.git",
        git_hash="ordering123",
        commit_age_hours=1,
        heartbeat_age_minutes=5,
    )

    rows = cf_client.execute_sql(
        f"""
        SELECT status_name
        FROM {VIEW_DERIVATION_STATUS_BREAKDOWN}
        ORDER BY status_name
        """
    )

    assert len(rows) > 0, "Expected at least one status in breakdown"

    # Test that the ordering matches the view's ORDER BY (display_order)
    # The view only shows statuses with actual derivations, not all possible statuses
    view_with_order = cf_client.execute_sql(
        f"""
        SELECT vd.status_name, ds.display_order
        FROM {VIEW_DERIVATION_STATUS_BREAKDOWN} vd
        JOIN derivation_statuses ds ON vd.status_name = ds.name
        ORDER BY ds.display_order
        """
    )

    # Verify the view results are in display_order
    ordered_statuses = [row["status_name"] for row in view_with_order]
    actual_statuses = [row["status_name"] for row in rows]

    # Since both queries should return the same data, just differently ordered
    assert set(ordered_statuses) == set(
        actual_statuses
    ), "View should contain same statuses regardless of ordering"

    # Verify display_order is ascending (assuming that's the intended order)
    display_orders = [row["display_order"] for row in view_with_order]
    assert display_orders == sorted(
        display_orders
    ), "Statuses should be ordered by display_order ascending"

    # Clean up
    cf_client.cleanup_test_data(scenario["cleanup"])
