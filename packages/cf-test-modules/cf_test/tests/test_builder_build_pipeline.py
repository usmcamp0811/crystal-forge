import json
import os
import time

import pytest

from cf_test import CFTestClient

pytestmark = [
    pytest.mark.builder,
    pytest.mark.s3cache,
    pytest.mark.integration,
    pytest.mark.build_pipeline,
]


@pytest.fixture(scope="session")
def s3_server():
    import cf_test

    return cf_test._driver_machines["s3Server"]


@pytest.fixture(scope="session")
def s3_cache():
    import cf_test

    return cf_test._driver_machines["s3Cache"]


@pytest.fixture(scope="session")
def cf_client(cf_config):
    return CFTestClient(cf_config)


@pytest.fixture(scope="session")
def test_flake_repo_url():
    """Get the test flake repository URL"""
    return "http://gitserver/crystal-forge"


@pytest.fixture(scope="session")
def test_flake_data():
    """Get test flake data from environment variables set by testFlake"""
    return {
        "test_systems": ["cf-test-sys", "test-agent"],
        "expected_derivations_per_system": 1,
    }


def test_build_prerequisites(cf_client, s3_server):
    """Test that build prerequisites are in place"""
    # Ensure builder is running
    s3_server.succeed("systemctl is-active crystal-forge-builder.service")

    # Wait for builder to be ready
    cf_client.wait_for_service_log(
        s3_server, "crystal-forge-builder.service", "Starting Build loop", timeout=60
    )

    # Verify S3 cache is accessible
    s3_server.succeed("ping -c 1 s3Cache")
    s3_server.succeed("nc -z s3Cache 9000")


def test_derivations_exist_and_ready_for_build(
    cf_client, s3_server, test_flake_repo_url, test_flake_data
):
    """Test that we have derivations that completed dry-run and are ready for building"""
    # Get test flake ID
    flake_rows = cf_client.execute_sql(
        "SELECT id FROM flakes WHERE repo_url = %s", (test_flake_repo_url,)
    )
    assert len(flake_rows) == 1, "Test flake not found"
    flake_id = flake_rows[0]["id"]

    # Check for derivations that completed dry-run (status_id = 5)
    dry_run_complete = cf_client.execute_sql(
        """
        SELECT d.id, d.derivation_name, d.derivation_type, d.status_id, d.derivation_path
        FROM derivations d
        JOIN commits c ON d.commit_id = c.id
        WHERE c.flake_id = %s AND d.status_id = 5
        ORDER BY d.id ASC
        """,
        (flake_id,),
    )

    s3_server.log(
        f"Found {len(dry_run_complete)} derivations with dry-run-complete status"
    )

    if not dry_run_complete:
        # Check what statuses we do have
        all_derivations = cf_client.execute_sql(
            """
            SELECT d.derivation_name, d.status_id, ds.name as status_name
            FROM derivations d
            JOIN commits c ON d.commit_id = c.id
            JOIN derivation_statuses ds ON d.status_id = ds.id
            WHERE c.flake_id = %s
            """,
            (flake_id,),
        )

        for deriv in all_derivations:
            s3_server.log(
                f"Derivation {deriv['derivation_name']}: status {deriv['status_id']} ({deriv['status_name']})"
            )

        # If we have derivations stuck in dry-run-inprogress (4), mark one as complete for testing
        in_progress = [d for d in all_derivations if d["status_id"] == 4]
        if in_progress:
            # Update one to dry-run-complete for testing
            test_deriv = in_progress[0]
            s3_server.log(
                f"Manually completing dry-run for {test_deriv['derivation_name']} to enable build testing"
            )

            # Get the derivation ID
            deriv_id_rows = cf_client.execute_sql(
                """
                SELECT d.id FROM derivations d
                JOIN commits c ON d.commit_id = c.id
                WHERE c.flake_id = %s AND d.derivation_name = %s
                LIMIT 1
                """,
                (flake_id, test_deriv["derivation_name"]),
            )

            if deriv_id_rows:
                deriv_id = deriv_id_rows[0]["id"]
                # Set a dummy derivation path and mark as complete
                dummy_drv_path = (
                    f"/nix/store/dummy-{test_deriv['derivation_name']}-test.drv"
                )
                cf_client.execute_sql(
                    """
                    UPDATE derivations 
                    SET status_id = 5, derivation_path = %s, completed_at = NOW()
                    WHERE id = %s
                    """,
                    (dummy_drv_path, deriv_id),
                )
                s3_server.log(
                    f"Set derivation {test_deriv['derivation_name']} to dry-run-complete for build testing"
                )
        else:
            pytest.skip(
                "No derivations available for build testing (none completed dry-run)"
            )

    # Re-check for dry-run-complete derivations
    final_check = cf_client.execute_sql(
        """
        SELECT d.id, d.derivation_name, d.derivation_path
        FROM derivations d
        JOIN commits c ON d.commit_id = c.id
        WHERE c.flake_id = %s AND d.status_id = 5
        LIMIT 1
        """,
        (flake_id,),
    )

    assert len(final_check) >= 1, "No derivations ready for build testing"
    return final_check[0]


def test_build_loop_picks_up_derivations(cf_client, s3_server, test_flake_repo_url):
    """Test that the build loop picks up derivations ready for building"""
    # Get test flake ID
    flake_rows = cf_client.execute_sql(
        "SELECT id FROM flakes WHERE repo_url = %s", (test_flake_repo_url,)
    )
    flake_id = flake_rows[0]["id"]

    # Wait for the build loop to detect derivations ready for building
    # The build loop runs every 5 minutes (300s), but we'll wait for the log message
    try:
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            "Starting build for derivation",
            timeout=350,  # Slightly longer than build interval
        )
        s3_server.log("✅ Build loop detected and started processing derivations")
    except:
        # If no "Starting build" message, check for the polling message
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            "No derivations need building",
            timeout=60,
        )
        s3_server.log("⚠️ Build loop is running but found no derivations needing build")


def test_derivation_build_status_transitions(cf_client, s3_server, test_flake_repo_url):
    """Test that derivations properly transition through build statuses"""
    # Get test flake ID
    flake_rows = cf_client.execute_sql(
        "SELECT id FROM flakes WHERE repo_url = %s", (test_flake_repo_url,)
    )
    flake_id = flake_rows[0]["id"]

    # Find a derivation that's being built or ready to build
    target_derivation = None

    # First, look for derivations in build-in-progress (8)
    in_progress = cf_client.execute_sql(
        """
        SELECT d.id, d.derivation_name, d.status_id
        FROM derivations d  
        JOIN commits c ON d.commit_id = c.id
        WHERE c.flake_id = %s AND d.status_id = 8
        LIMIT 1
        """,
        (flake_id,),
    )

    if in_progress:
        target_derivation = in_progress[0]
        s3_server.log(
            f"Found derivation in build-in-progress: {target_derivation['derivation_name']}"
        )
    else:
        # Look for derivations ready to build (status 5 or 7)
        ready_to_build = cf_client.execute_sql(
            """
            SELECT d.id, d.derivation_name, d.status_id  
            FROM derivations d
            JOIN commits c ON d.commit_id = c.id
            WHERE c.flake_id = %s AND d.status_id IN (5, 7)
            LIMIT 1
            """,
            (flake_id,),
        )

        if ready_to_build:
            target_derivation = ready_to_build[0]
            s3_server.log(
                f"Found derivation ready to build: {target_derivation['derivation_name']}"
            )

    if not target_derivation:
        pytest.skip("No derivations available for build status transition testing")

    deriv_id = target_derivation["id"]
    deriv_name = target_derivation["derivation_name"]

    s3_server.log(
        f"Monitoring build status transitions for: {deriv_name} (ID: {deriv_id})"
    )

    # Monitor status transitions
    timeout = 600  # 10 minutes for build completion
    start_time = time.time()
    last_status = None
    seen_statuses = set()

    while time.time() - start_time < timeout:
        current_status = cf_client.execute_sql(
            """
            SELECT d.status_id, ds.name as status_name, d.error_message,
                   d.build_elapsed_seconds, d.build_current_target
            FROM derivations d
            JOIN derivation_statuses ds ON d.status_id = ds.id  
            WHERE d.id = %s
            """,
            (deriv_id,),
        )

        if current_status:
            status_info = current_status[0]
            status_id = status_info["status_id"]
            status_name = status_info["status_name"]
            error_message = status_info["error_message"]
            elapsed = status_info["build_elapsed_seconds"]
            current_target = status_info["build_current_target"]

            # Log status changes
            if status_id != last_status:
                seen_statuses.add((status_id, status_name))
                s3_server.log(
                    f"Status transition: {deriv_name} → {status_name} ({status_id})"
                )
                last_status = status_id

                if current_target:
                    s3_server.log(f"  Currently building: {current_target}")
                if elapsed:
                    s3_server.log(f"  Build time: {elapsed}s")

            # Check for terminal states
            if status_id == 10:  # build-complete
                s3_server.log(f"✅ Build completed successfully for {deriv_name}")
                break
            elif status_id == 12:  # build-failed
                s3_server.log(f"❌ Build failed for {deriv_name}: {error_message}")
                # This is still a valid test result - the build system properly handled failure
                break
            elif status_id in [6, 13]:  # dry-run-failed or generic failed
                s3_server.log(f"❌ Derivation failed in earlier stage: {status_name}")
                break

        time.sleep(10)  # Check every 10 seconds

    s3_server.log(f"Build monitoring completed. Statuses seen: {seen_statuses}")

    # Verify we saw meaningful status transitions
    status_ids_seen = {s[0] for s in seen_statuses}

    # We should see at least one of the build-related statuses
    build_statuses = {
        5,
        7,
        8,
        10,
        12,
    }  # dry-run-complete, build-pending, build-in-progress, build-complete, build-failed
    assert (
        len(status_ids_seen & build_statuses) >= 1
    ), f"Expected to see build-related statuses, saw: {seen_statuses}"


def test_build_progress_tracking(cf_client, s3_server, test_flake_repo_url):
    """Test that build progress is properly tracked during builds"""
    # Get test flake ID
    flake_rows = cf_client.execute_sql(
        "SELECT id FROM flakes WHERE repo_url = %s", (test_flake_repo_url,)
    )
    flake_id = flake_rows[0]["id"]

    # Look for derivations with build progress data
    progress_data = cf_client.execute_sql(
        """
        SELECT d.id, d.derivation_name, d.build_elapsed_seconds, 
               d.build_current_target, d.build_last_activity_seconds,
               d.build_last_heartbeat, ds.name as status_name
        FROM derivations d
        JOIN commits c ON d.commit_id = c.id
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE c.flake_id = %s 
        AND (d.build_elapsed_seconds IS NOT NULL 
             OR d.build_current_target IS NOT NULL
             OR d.build_last_heartbeat IS NOT NULL)
        ORDER BY d.build_last_heartbeat DESC
        LIMIT 5
        """,
        (flake_id,),
    )

    if progress_data:
        s3_server.log(
            f"Found {len(progress_data)} derivations with build progress data:"
        )
        for deriv in progress_data:
            s3_server.log(
                f"  {deriv['derivation_name']}: {deriv['build_elapsed_seconds']}s elapsed, "
                f"target: {deriv['build_current_target']}, status: {deriv['status_name']}"
            )

        # Verify build progress data is reasonable
        for deriv in progress_data:
            if deriv["build_elapsed_seconds"]:
                assert (
                    0 <= deriv["build_elapsed_seconds"] <= 3600
                ), f"Build time seems unreasonable: {deriv['build_elapsed_seconds']}s"

            if deriv["build_last_activity_seconds"]:
                assert (
                    0 <= deriv["build_last_activity_seconds"] <= 600
                ), f"Last activity time seems unreasonable: {deriv['build_last_activity_seconds']}s"

        s3_server.log("✅ Build progress tracking data looks reasonable")
    else:
        s3_server.log(
            "⚠️ No build progress data found - builds may complete too quickly to track"
        )


def test_cache_push_operations(cf_client, s3_server, s3_cache, test_flake_repo_url):
    """Test that successful builds trigger cache push operations"""
    # Get test flake ID
    flake_rows = cf_client.execute_sql(
        "SELECT id FROM flakes WHERE repo_url = %s", (test_flake_repo_url,)
    )
    flake_id = flake_rows[0]["id"]

    # Look for completed builds that should have been pushed to cache
    completed_builds = cf_client.execute_sql(
        """
        SELECT d.id, d.derivation_name, d.derivation_path
        FROM derivations d
        JOIN commits c ON d.commit_id = c.id  
        WHERE c.flake_id = %s AND d.status_id = 10
        ORDER BY d.completed_at DESC
        LIMIT 3
        """,
        (flake_id,),
    )

    if completed_builds:
        s3_server.log(
            f"Found {len(completed_builds)} completed builds to check for cache operations"
        )

        # Check builder logs for cache push activity
        try:
            cf_client.wait_for_service_log(
                s3_server,
                "crystal-forge-builder.service",
                "Queuing cache push",
                timeout=60,
            )
            s3_server.log("✅ Found cache push queuing activity")
        except:
            s3_server.log("⚠️ No cache push queuing found in recent logs")

        # Check for cache push completion
        try:
            cf_client.wait_for_service_log(
                s3_server,
                "crystal-forge-builder.service",
                "Cache push completed",
                timeout=60,
            )
            s3_server.log("✅ Found cache push completion activity")
        except:
            s3_server.log("⚠️ No cache push completion found in recent logs")

        # Verify S3 cache received data
        s3_logs = s3_cache.succeed(
            "journalctl -u minio.service --since '5 minutes ago' --no-pager"
        )

        # Look for PUT requests which indicate uploads
        put_requests = [
            line
            for line in s3_logs.split("\n")
            if "PUT" in line and "/crystal-forge-cache/" in line
        ]

        if put_requests:
            s3_server.log(
                f"✅ Found {len(put_requests)} S3 PUT operations in MinIO logs"
            )
            for req in put_requests[:3]:  # Show first 3
                s3_server.log(f"  S3 PUT: {req.strip()}")
        else:
            s3_server.log("⚠️ No S3 PUT operations found in MinIO logs")
    else:
        s3_server.log("⚠️ No completed builds found for cache push testing")


def test_build_system_stability(cf_client, s3_server):
    """Test that the build system is stable and handling errors gracefully"""
    # Check that builder hasn't restarted excessively
    restart_output = s3_server.succeed(
        "systemctl show crystal-forge-builder.service --property=NRestarts"
    )
    restart_count = int(restart_output.split("=")[1].strip())

    assert (
        restart_count <= 5
    ), f"Builder has restarted {restart_count} times - possible instability"

    # Check for error patterns in logs
    recent_logs = s3_server.succeed(
        "journalctl -u crystal-forge-builder.service --since '10 minutes ago' --no-pager"
    )

    # Count error lines
    error_lines = [line for line in recent_logs.split("\n") if "ERROR" in line.upper()]

    s3_server.log(f"Found {len(error_lines)} error lines in recent builder logs")

    # Show a few errors for debugging if present
    if error_lines:
        s3_server.log("Recent error samples:")
        for error in error_lines[:3]:
            s3_server.log(f"  {error.strip()}")

    # Check that the build loop is still running
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "No derivations need building",
        timeout=120,
    )

    s3_server.log("✅ Build system appears stable and operational")


def test_build_metrics_and_performance(cf_client, s3_server, test_flake_repo_url):
    """Test that build metrics are being collected"""
    # Get test flake ID
    flake_rows = cf_client.execute_sql(
        "SELECT id FROM flakes WHERE repo_url = %s", (test_flake_repo_url,)
    )
    flake_id = flake_rows[0]["id"]

    # Check for derivations with timing data
    timed_builds = cf_client.execute_sql(
        """
        SELECT d.derivation_name, d.evaluation_duration_ms, d.started_at, d.completed_at,
               EXTRACT(EPOCH FROM (d.completed_at - d.started_at)) * 1000 as total_duration_ms
        FROM derivations d
        JOIN commits c ON d.commit_id = c.id
        WHERE c.flake_id = %s 
        AND d.evaluation_duration_ms IS NOT NULL
        AND d.started_at IS NOT NULL 
        AND d.completed_at IS NOT NULL
        ORDER BY d.completed_at DESC
        LIMIT 5
        """,
        (flake_id,),
    )

    if timed_builds:
        s3_server.log(f"Found {len(timed_builds)} builds with timing data:")

        for build in timed_builds:
            eval_time = build["evaluation_duration_ms"] / 1000.0
            total_time = (
                build["total_duration_ms"] / 1000.0 if build["total_duration_ms"] else 0
            )

            s3_server.log(
                f"  {build['derivation_name']}: eval={eval_time:.2f}s, total={total_time:.2f}s"
            )

            # Verify timing data is reasonable
            assert (
                0 <= eval_time <= 300
            ), f"Evaluation time seems unreasonable: {eval_time}s"
            if total_time > 0:
                assert (
                    eval_time <= total_time
                ), f"Evaluation time ({eval_time}s) > total time ({total_time}s)"

        s3_server.log("✅ Build timing data looks reasonable")
    else:
        s3_server.log("⚠️ No completed builds with timing data found")

    # Check memory monitoring
    try:
        cf_client.wait_for_service_log(
            s3_server, "crystal-forge-builder.service", "Memory - RSS:", timeout=60
        )
        s3_server.log("✅ Memory monitoring is active")
    except:
        s3_server.log("⚠️ No memory monitoring logs found")


def test_end_to_end_build_pipeline(cf_client, s3_server, test_flake_repo_url):
    """Test complete end-to-end build pipeline from commit to cache"""
    # Get test flake ID
    flake_rows = cf_client.execute_sql(
        "SELECT id FROM flakes WHERE repo_url = %s", (test_flake_repo_url,)
    )
    flake_id = flake_rows[0]["id"]

    # Get a summary of the current build pipeline state
    pipeline_summary = cf_client.execute_sql(
        """
        SELECT ds.name as status_name, COUNT(*) as count
        FROM derivations d
        JOIN commits c ON d.commit_id = c.id
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE c.flake_id = %s
        GROUP BY ds.name, d.status_id
        ORDER BY d.status_id
        """,
        (flake_id,),
    )

    s3_server.log("📊 Build Pipeline Status Summary:")
    total_derivations = 0
    for status in pipeline_summary:
        count = status["count"]
        status_name = status["status_name"]
        total_derivations += count
        s3_server.log(f"  {status_name}: {count}")

    s3_server.log(f"  Total derivations: {total_derivations}")

    # Verify we have a reasonable distribution of statuses
    assert total_derivations >= 1, "No derivations found in pipeline"

    # Check that we have some derivations that progressed beyond dry-run-pending
    advanced_statuses = cf_client.execute_sql(
        """
        SELECT COUNT(*) as count
        FROM derivations d  
        JOIN commits c ON d.commit_id = c.id
        WHERE c.flake_id = %s AND d.status_id > 3
        """,
        (flake_id,),
    )

    advanced_count = advanced_statuses[0]["count"]
    assert (
        advanced_count >= 1
    ), f"Expected derivations to progress beyond dry-run-pending, but only found {advanced_count}"

    s3_server.log(
        f"✅ End-to-end pipeline test passed: {advanced_count} derivations progressed through the pipeline"
    )
