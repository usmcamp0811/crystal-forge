import os
import time
from datetime import UTC, datetime, timedelta

import pytest

from cf_test.scenarios import _create_base_scenario, scenario_dry_run_failed
from cf_test.vm_helpers import SmokeTestConstants as C
from cf_test.vm_helpers import wait_for_crystal_forge_ready

pytestmark = [pytest.mark.server, pytest.mark.integration]


def test_derivation_reset_on_server_startup(cf_client, server):
    """Test that server resets derivations properly on startup"""

    wait_for_crystal_forge_ready(server)

    # Get real git info from environment
    real_commit_hash = os.getenv(
        "CF_TEST_REAL_COMMIT_HASH", "ebcc48fbf1030fc2065fc266da158af1d0b3943c"
    )
    real_repo_url = os.getenv("CF_TEST_REAL_REPO_URL", "http://gitserver/crystal-forge")

    # Create various derivation states to test reset logic
    test_scenarios = []

    # 1. dry-run-pending with low attempts (should advance to build-pending if it has a path)
    scenario1 = _create_base_scenario(
        cf_client,
        hostname="test-reset-pending-low",
        flake_name="reset-test-1",
        repo_url=real_repo_url,  # Use real repo
        git_hash=real_commit_hash,  # Use real hash
        derivation_status="dry-run-pending",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )
    cf_client.execute_sql(
        """
        UPDATE derivations 
        SET attempt_count = 4,
            derivation_path = '/nix/store/test-pending-low.drv'
        WHERE id = %s
        """,
        (scenario1["derivation_id"],),
    )
    test_scenarios.append(scenario1)

    # 2. dry-run-failed with high attempts and NO path (should stay dry-run-failed - terminal)
    scenario2 = _create_base_scenario(
        cf_client,
        hostname="test-reset-failed-terminal",
        flake_name="reset-test-2",
        repo_url=real_repo_url,  # Use real repo
        git_hash=real_commit_hash,  # Use real hash
        derivation_status="dry-run-failed",
        derivation_error="Terminal failure",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )
    cf_client.execute_sql(
        """
        UPDATE derivations 
        SET derivation_path = NULL, 
            attempt_count = 5 
        WHERE id = %s
        """,
        (scenario2["derivation_id"],),
    )
    test_scenarios.append(scenario2)

    # 3. dry-run-failed with low attempts and NO path (should reset to dry-run-pending)
    scenario3 = _create_base_scenario(
        cf_client,
        hostname="test-reset-failed-low",
        flake_name="reset-test-3",
        repo_url=real_repo_url,  # Use real repo
        git_hash=real_commit_hash,  # Use real hash
        derivation_status="dry-run-failed",
        derivation_error="Temporary failure",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )
    cf_client.execute_sql(
        """
        UPDATE derivations 
        SET attempt_count = 2, 
            derivation_path = NULL 
        WHERE id = %s
        """,
        (scenario3["derivation_id"],),
    )
    test_scenarios.append(scenario3)

    # 4. derivation with path but failed build (should reset to build-pending)
    scenario4 = _create_base_scenario(
        cf_client,
        hostname="test-reset-build-failed",
        flake_name="reset-test-4",
        repo_url=real_repo_url,  # Use real repo
        git_hash=real_commit_hash,  # Use real hash
        derivation_status="build-failed",
        derivation_error="Build failed",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )
    # Give it a derivation path and low attempt count
    cf_client.execute_sql(
        """
        UPDATE derivations 
        SET derivation_path = '/nix/store/test-build-failed.drv',
            attempt_count = 3 
        WHERE id = %s
        """,
        (scenario4["derivation_id"],),
    )
    test_scenarios.append(scenario4)

    server.log("=== Pre-restart derivation states ===")
    initial_states = cf_client.execute_sql(
        """
        SELECT d.id, d.derivation_name, d.status_id, d.attempt_count, d.derivation_path
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE d.derivation_name LIKE 'test-reset-%'
        ORDER BY d.derivation_name
        """
    )
    for state in initial_states:
        server.log(
            f"  {state['derivation_name']}: status_id={state['status_id']}, attempts={state['attempt_count']}, path={state['derivation_path'] is not None}"
        )

    # Temporarily disable the evaluation loops to prevent interference
    server.log("=== Stopping evaluation loops to isolate reset test ===")
    server.succeed("systemctl stop crystal-forge-server.service")

    # Restart the server to trigger reset_non_terminal_derivations
    server.log("=== Restarting server to trigger reset ===")
    server.succeed(f"systemctl start {C.SERVER_SERVICE}")

    # Wait for service to be active and check logs for startup
    server.wait_for_unit(C.SERVER_SERVICE)

    # Wait specifically for the reset to complete
    cf_client.wait_for_service_log(
        server, C.SERVER_SERVICE, "💡 Total derivations processed:", timeout=30
    )

    # Give a moment for database commits to complete
    time.sleep(2)

    # Stop the server again to prevent evaluation loops from running
    server.succeed("systemctl stop crystal-forge-server.service")

    server.log("=== Post-restart derivation states ===")
    final_states = cf_client.execute_sql(
        """
        SELECT d.id, d.derivation_name, d.status_id, ds.name as status_name, 
               d.attempt_count as attempt_count, d.derivation_path IS NOT NULL as has_path
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id  
        WHERE d.derivation_name LIKE 'test-reset-%'
        ORDER BY d.derivation_name
        """
    )

    states_by_name = {state["derivation_name"]: state for state in final_states}

    for state in final_states:
        server.log(
            f"  {state['derivation_name']}: {state['status_name']} (attempts: {state['attempt_count']}, has_path: {state['has_path']})"
        )

    # Assertions based on reset logic

    # 1. dry-run-pending with path and low attempts should advance to build-pending
    pending_low = states_by_name["test-reset-pending-low"]
    assert (
        pending_low["status_name"] == "build-pending"
    ), f"Expected build-pending (advanced from dry-run-pending), got {pending_low['status_name']}"

    # 2. dry-run-failed with no path and 5+ attempts should stay dry-run-failed (terminal)
    failed_terminal = states_by_name["test-reset-failed-terminal"]
    assert (
        failed_terminal["status_name"] == "dry-run-failed"
    ), f"Expected dry-run-failed, got {failed_terminal['status_name']}"
    assert (
        failed_terminal["attempt_count"] == 5
    ), f"Expected 5 attempts, got {failed_terminal['attempt_count']}"

    # 3. build-failed with path and <5 attempts should reset to build-pending
    build_failed = states_by_name["test-reset-build-failed"]
    assert (
        build_failed["status_name"] == "build-pending"
    ), f"Expected reset to build-pending, got {build_failed['status_name']}"
    assert build_failed["has_path"] == True, f"Expected to keep derivation path"

    # Restart server normally for cleanup
    server.succeed(f"systemctl start {C.SERVER_SERVICE}")
    server.wait_for_unit(C.SERVER_SERVICE)

    # Cleanup
    for scenario in test_scenarios:
        cf_client.cleanup_test_data(scenario["cleanup"])


def test_derivation_reset_background_loop(cf_client, server):
    """Test that background loop resets derivations properly"""

    # Create a derivation that should be reset by background loop
    scenario = _create_base_scenario(
        cf_client,
        hostname="test-background-reset",
        flake_name="background-reset-test",
        repo_url="https://example.com/background-reset.git",
        git_hash="background123",
        derivation_status="dry-run-pending",  # Non-terminal state
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )

    # Make the derivation look "stuck" by giving it an old started_at time
    # Use attempt_count = 3 (< 5) so it can be reset, not made terminal
    cf_client.execute_sql(
        """
        UPDATE derivations 
        SET started_at = NOW() - INTERVAL '2 hours',
            attempt_count = 3, derivation_path = NULL
        WHERE id = %s
        """,
        (scenario["derivation_id"],),
    )

    # Wait for background loop to run (should be ~1-2 minutes per your loop)
    server.log("=== Waiting for background loop to reset stuck derivation ===")

    # Check periodically for reset (max 3 minutes)
    max_wait = 180  # 3 minutes
    check_interval = 10  # Check every 10 seconds
    reset_detected = False

    for attempt in range(max_wait // check_interval):
        time.sleep(check_interval)

        result = cf_client.execute_sql(
            """
            SELECT d.status_id, ds.name as status_name, d.attempt_count
            FROM derivations d
            JOIN derivation_statuses ds ON d.status_id = ds.id
            WHERE d.id = %s
            """,
            (scenario["derivation_id"],),
        )
        server.log(f"Query Returned: {result}")

        if result:
            status = result[0]
            server.log(
                f"  Attempt {attempt + 1}: status={status['status_name']}, attempts={status['attempt_count']}"
            )

            # Check if it was reset to dry-run-pending
            if status["status_name"] == "dry-run-pending":
                reset_detected = True
                server.log("=== Background reset detected! ===")
                break

    assert (
        reset_detected
    ), "Background loop did not reset stuck derivation within 3 minutes"

    # Cleanup
    cf_client.cleanup_test_data(scenario["cleanup"])


def test_attempt_count_terminal_logic(cf_client, server):
    """Test that derivations with attempt_count >= 5 become terminal"""

    # Create derivation that will hit the attempt limit
    scenario = _create_base_scenario(
        cf_client,
        hostname="test-attempt-limit",
        flake_name="attempt-limit-test",
        repo_url="https://example.com/attempt-limit.git",
        git_hash="attempt123",
        derivation_status="dry-run-failed",
        derivation_error="Will hit attempt limit",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )

    # Set attempt count to exactly 5 (terminal threshold)
    cf_client.execute_sql(
        "UPDATE derivations SET attempt_count = 5, derivation_path = NULL WHERE id = %s",
        (scenario["derivation_id"],),
    )

    # Restart server to trigger reset
    server.succeed(f"systemctl restart {C.SERVER_SERVICE}")
    server.wait_for_unit(C.SERVER_SERVICE)
    time.sleep(5)

    # Verify it stays in terminal state (not reset)
    result = cf_client.execute_sql(
        """
        SELECT d.status_id, ds.name as status_name, d.attempt_count
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE d.id = %s
        """,
        (scenario["derivation_id"],),
    )

    assert result, "Derivation should still exist"
    status = result[0]

    # Should stay dry-run-failed with 5 attempts
    assert (
        status["status_name"] == "dry-run-failed"
    ), f"Expected terminal dry-run-failed, got {status['status_name']}"
    assert (
        status["attempt_count"] == 5
    ), f"Expected 5 attempts, got {status['attempt_count']}"

    # Cleanup
    cf_client.cleanup_test_data(scenario["cleanup"])
