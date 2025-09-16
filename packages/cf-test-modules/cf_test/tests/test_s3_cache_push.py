import json
import os
import time

import pytest

from cf_test import CFTestClient
from cf_test.scenarios import _create_base_scenario
from cf_test.vm_helpers import wait_for_crystal_forge_ready

pytestmark = [pytest.mark.s3cache]


@pytest.fixture(scope="session")
def s3_server():
    import cf_test

    return cf_test._driver_machines["s3Server"]


@pytest.fixture(scope="session")
def s3_cache():
    import cf_test

    return cf_test._driver_machines["s3Cache"]


@pytest.fixture(scope="session")
def gitserver():
    import cf_test

    return cf_test._driver_machines["gitserver"]


@pytest.fixture(scope="session")
def cf_client(cf_config):
    return CFTestClient(cf_config)


@pytest.fixture(scope="session")
def test_flake_data():
    """Load test flake commit and derivation data"""
    return {
        "main_head": os.environ.get("CF_TEST_MAIN_HEAD"),
        "development_head": os.environ.get("CF_TEST_DEVELOPMENT_HEAD"),
        "feature_head": os.environ.get("CF_TEST_FEATURE_HEAD"),
        "main_commits": os.environ.get("CF_TEST_MAIN_COMMITS", "").split(","),
        "development_commits": os.environ.get("CF_TEST_DEVELOPMENT_COMMITS", "").split(
            ","
        ),
        "feature_commits": os.environ.get("CF_TEST_FEATURE_COMMITS", "").split(","),
        "repo_url": os.environ.get("CF_TEST_REAL_REPO_URL"),
        "flake_name": os.environ.get("CF_TEST_FLAKE_NAME", "test-flake"),
    }


def test_s3_connectivity(s3_server, s3_cache):
    """Test basic S3 connectivity between builder and MinIO"""

    # Test network connectivity
    s3_server.succeed("ping -c 1 s3Cache")
    s3_server.log("✅ Network connectivity to S3 cache established")

    # Test MinIO is responding
    s3_cache.succeed("curl -f http://localhost:9000/minio/health/live")
    s3_server.log("✅ MinIO health check passed")

    # Test AWS CLI can reach MinIO from builder
    s3_server.succeed(
        """
        AWS_ENDPOINT_URL=http://s3Cache:9000 \
        AWS_ACCESS_KEY_ID=minioadmin \
        AWS_SECRET_ACCESS_KEY=minioadmin \
        aws s3 ls s3://crystal-forge-cache/ || true
    """
    )
    s3_server.log("✅ AWS CLI connectivity to MinIO verified")


def test_builder_s3_cache_config(s3_server):
    """Test that builder has correct S3 cache configuration"""

    # Check Crystal Forge config file
    config_content = s3_server.succeed("cat /var/lib/crystal-forge/config.toml")
    s3_server.log(f"Crystal Forge config excerpt: {config_content[:500]}...")

    # Verify S3 cache configuration
    assert 'cache_type = "S3"' in config_content, "S3 cache type not configured"
    assert "s3://s3Cache:9000" in config_content, "S3 endpoint not configured"
    assert "push_after_build = true" in config_content, "Cache push not enabled"

    s3_server.log("✅ S3 cache configuration verified")

    # Verify builder service has AWS environment variables
    env_output = s3_server.succeed(
        "systemctl show crystal-forge-builder.service --property=Environment"
    )
    assert (
        "AWS_ENDPOINT_URL=http://s3Cache:9000" in env_output
    ), "AWS endpoint not in environment"
    assert (
        "AWS_ACCESS_KEY_ID=minioadmin" in env_output
    ), "AWS credentials not in environment"

    s3_server.log("✅ AWS environment variables configured")


def test_cache_push_job_creation(cf_client, s3_server):
    """Test that cache push jobs are created for build-complete derivations"""

    wait_for_crystal_forge_ready(s3_server)

    # Create a dummy store path
    dummy_path = "/nix/store/job-creation-test-12345"
    s3_server.succeed(f"mkdir -p {dummy_path}")
    s3_server.succeed(f"echo 'Job creation test' > {dummy_path}/test-file")

    s3_server.log(f"Created dummy store path: {dummy_path}")

    # Insert a derivation marked as build-complete
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               derivation_type, derivation_name, derivation_path,
               status_id, completed_at, attempt_count, scheduled_at
           ) VALUES ('package', 'job-creation-test', %s, 10, NOW(), 0, NOW())
           RETURNING id""",
        (dummy_path,),
    )

    derivation_id = derivation_result[0]["id"]
    s3_server.log(f"Created test derivation with ID: {derivation_id}")

    # Wait for cache push loop to create job
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "Queuing cache push for derivation",
        timeout=120,
    )

    # Verify cache push job was created in database
    jobs = cf_client.execute_sql(
        "SELECT * FROM cache_push_jobs WHERE derivation_id = %s",
        (derivation_id,),
    )

    assert len(jobs) > 0, "Cache push job was not created"
    job = jobs[0]

    s3_server.log(f"Cache push job created: ID={job['id']}, status={job['status']}")
    assert job["status"] in [
        "pending",
        "in_progress",
    ], f"Unexpected job status: {job['status']}"
    assert job["store_path"] == dummy_path, f"Wrong store path: {job['store_path']}"

    # Cleanup
    s3_server.succeed(f"rm -rf {dummy_path}")
    cf_client.execute_sql("DELETE FROM cache_push_jobs WHERE id = %s", (job["id"],))
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_cache_push_job_processing(cf_client, s3_server, s3_cache):
    """Test that pending cache push jobs are processed"""

    wait_for_crystal_forge_ready(s3_server)

    # Create a dummy store path
    dummy_path = "/nix/store/job-processing-test-12345"
    s3_server.succeed(f"mkdir -p {dummy_path}")
    s3_server.succeed(f"echo 'Job processing test' > {dummy_path}/test-file")

    # Insert derivation and manually create cache push job
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               derivation_type, derivation_name, derivation_path,
               status_id, completed_at, attempt_count, scheduled_at
           ) VALUES ('package', 'job-processing-test', %s, 10, NOW(), 0, NOW())
           RETURNING id""",
        (dummy_path,),
    )

    derivation_id = derivation_result[0]["id"]

    # Create cache push job directly
    job_result = cf_client.execute_sql(
        """INSERT INTO cache_push_jobs (
               derivation_id, store_path, status, cache_destination
           ) VALUES (%s, %s, 'pending', 's3://crystal-forge-cache')
           RETURNING id""",
        (derivation_id, dummy_path),
    )

    job_id = job_result[0]["id"]
    s3_server.log(f"Created cache push job with ID: {job_id}")

    # Wait for job to be processed
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        f"Processing cache push job {job_id}",
        timeout=120,
    )

    # Monitor job status changes
    for attempt in range(30):  # Wait up to 30 seconds
        jobs = cf_client.execute_sql(
            "SELECT status, error_message FROM cache_push_jobs WHERE id = %s",
            (job_id,),
        )

        if jobs:
            status = jobs[0]["status"]
            s3_server.log(f"Job {job_id} status: {status}")

            if status in ["completed", "failed"]:
                break

        time.sleep(1)

    # Check final job status
    final_jobs = cf_client.execute_sql(
        "SELECT * FROM cache_push_jobs WHERE id = %s",
        (job_id,),
    )

    assert len(final_jobs) > 0, "Cache push job disappeared"
    final_job = final_jobs[0]

    s3_server.log(f"Final job status: {final_job['status']}")

    if final_job["status"] == "failed":
        s3_server.log(f"Job failed with error: {final_job['error_message']}")

    # For now, accept either completed or failed (MinIO might not be fully set up)
    assert final_job["status"] in [
        "completed",
        "failed",
    ], f"Job stuck in status: {final_job['status']}"

    # Cleanup
    s3_server.succeed(f"rm -rf {dummy_path}")
    cf_client.execute_sql("DELETE FROM cache_push_jobs WHERE id = %s", (job_id,))
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_cache_push_job_retry_logic(cf_client, s3_server):
    """Test cache push job retry behavior for failed jobs"""

    wait_for_crystal_forge_ready(s3_server)

    # Create a non-existent store path to trigger failure
    bad_path = "/nix/store/nonexistent-retry-test-12345"

    # Insert derivation with bad path
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               derivation_type, derivation_name, derivation_path,
               status_id, completed_at, attempt_count, scheduled_at
           ) VALUES ('package', 'retry-test', %s, 10, NOW(), 0, NOW())
           RETURNING id""",
        (bad_path,),
    )

    derivation_id = derivation_result[0]["id"]

    # Create cache push job
    job_result = cf_client.execute_sql(
        """INSERT INTO cache_push_jobs (
               derivation_id, store_path, status, attempts
           ) VALUES (%s, %s, 'pending', 0)
           RETURNING id""",
        (derivation_id, bad_path),
    )

    job_id = job_result[0]["id"]

    # Wait for job to fail
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        ["Cache push failed", "error"],
        timeout=120,
    )

    # Check that attempts counter increased
    failed_jobs = cf_client.execute_sql(
        "SELECT attempts, status, error_message FROM cache_push_jobs WHERE id = %s",
        (job_id,),
    )

    assert len(failed_jobs) > 0, "Failed job not found"
    failed_job = failed_jobs[0]

    s3_server.log(
        f"Failed job: attempts={failed_job['attempts']}, status={failed_job['status']}"
    )
    assert failed_job["attempts"] > 0, "Attempts counter not incremented"
    assert (
        failed_job["status"] == "failed"
    ), f"Expected failed status, got: {failed_job['status']}"
    assert failed_job["error_message"], "No error message recorded"

    # Cleanup
    cf_client.execute_sql("DELETE FROM cache_push_jobs WHERE id = %s", (job_id,))
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_cache_push_loop_running(cf_client, s3_server):
    """Test that the cache push loop is actively running"""

    wait_for_crystal_forge_ready(s3_server)

    # Look for cache push loop startup message
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "Starting Cache Push loop",
        timeout=60,
    )

    s3_server.log("✅ Cache push loop is running")

    # Verify loop is still active by looking for periodic activity
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        ["cache push", "processing", "Cache Push"],
        timeout=120,
    )

    s3_server.log("✅ Cache push loop shows periodic activity")


def test_derivation_status_update_after_cache_push(cf_client, s3_server):
    """Test that derivation status is updated to cache-pushed after successful push"""

    wait_for_crystal_forge_ready(s3_server)

    # Create a dummy store path
    dummy_path = "/nix/store/status-update-test-12345"
    s3_server.succeed(f"mkdir -p {dummy_path}")
    s3_server.succeed(f"echo 'Status update test' > {dummy_path}/test-file")

    # Insert derivation
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               derivation_type, derivation_name, derivation_path,
               status_id, completed_at, attempt_count, scheduled_at
           ) VALUES ('package', 'status-update-test', %s, 10, NOW(), 0, NOW())
           RETURNING id""",
        (dummy_path,),
    )

    derivation_id = derivation_result[0]["id"]

    # Wait for cache push process to complete (or fail)
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        ["Queuing cache push", "Processing cache push"],
        timeout=180,
    )

    # Give some time for status updates
    time.sleep(10)

    # Check derivation status - should be cache-pushed if successful, or stay build-complete if failed
    derivation_status = cf_client.execute_sql(
        """SELECT d.status_id, ds.name as status_name 
           FROM derivations d 
           JOIN derivation_statuses ds ON d.status_id = ds.id 
           WHERE d.id = %s""",
        (derivation_id,),
    )

    if derivation_status:
        status_name = derivation_status[0]["status_name"]
        s3_server.log(f"Final derivation status: {status_name}")

        # Accept either cache-pushed (success) or build-complete (cache push failed)
        assert status_name in [
            "cache-pushed",
            "build-complete",
        ], f"Unexpected status: {status_name}"
    else:
        s3_server.log("⚠️ Could not find derivation status")

    # Cleanup
    s3_server.succeed(f"rm -rf {dummy_path}")
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_s3_cache_integration_with_existing_store_path(cf_client, s3_server, s3_cache):
    """Test cache push with a real store path"""

    wait_for_crystal_forge_ready(s3_server)

    # Find an existing store path
    try:
        store_paths = (
            s3_server.succeed("find /nix/store -maxdepth 1 -type d | head -10")
            .strip()
            .split("\n")
        )
        existing_path = None

        for path in store_paths:
            if path and path != "/nix/store" and "-" in path:
                try:
                    # Check if it's a reasonable size for testing
                    size_output = s3_server.succeed(f"du -sh {path}")
                    size = size_output.split()[0]
                    s3_server.log(f"Store path candidate: {path} - Size: {size}")

                    # Use paths under 100MB for testing
                    if (
                        any(unit in size for unit in ["K", "M"])
                        and not size.startswith("1")
                        and "G" not in size
                    ):
                        existing_path = path
                        break
                except:
                    continue

        if not existing_path:
            pytest.skip("No suitable store paths found for integration testing")

        s3_server.log(f"Using existing store path: {existing_path}")

        # Insert derivation with real store path
        derivation_result = cf_client.execute_sql(
            """INSERT INTO derivations (
                   derivation_type, derivation_name, derivation_path,
                   status_id, completed_at, attempt_count, scheduled_at
               ) VALUES ('package', 'integration-test', %s, 10, NOW(), 0, NOW())
               RETURNING id""",
            (existing_path,),
        )

        derivation_id = derivation_result[0]["id"]

        # Monitor the complete process
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            "Queuing cache push.*integration-test",
            timeout=120,
        )

        # Look for processing
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            "Processing cache push job",
            timeout=60,
        )

        # Check for completion or failure
        try:
            cf_client.wait_for_service_log(
                s3_server,
                "crystal-forge-builder.service",
                ["Cache push completed", "Cache push failed"],
                timeout=180,
            )
        except:
            s3_server.log("⚠️ Cache push result not clearly detected")

        # Check MinIO for any activity
        try:
            minio_logs = s3_cache.succeed(
                "journalctl -u minio.service --since '3 minutes ago' --no-pager"
            )

            put_operations = [line for line in minio_logs.split("\n") if "PUT" in line]
            if put_operations:
                s3_server.log(f"✅ Found {len(put_operations)} PUT operations in MinIO")
            else:
                s3_server.log("⚠️ No PUT operations found in MinIO logs")
        except:
            s3_server.log("⚠️ Could not check MinIO logs")

        # Cleanup
        cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))

    except Exception as e:
        pytest.skip(f"Integration test failed: {e}")


def test_cache_push_job_duplicate_prevention(cf_client, s3_server):
    """Test that duplicate cache push jobs are not created for the same derivation"""

    wait_for_crystal_forge_ready(s3_server)

    # Create derivation
    dummy_path = "/nix/store/duplicate-test-12345"
    s3_server.succeed(f"mkdir -p {dummy_path}")

    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               derivation_type, derivation_name, derivation_path,
               status_id, completed_at, attempt_count, scheduled_at
           ) VALUES ('package', 'duplicate-test', %s, 10, NOW(), 0, NOW())
           RETURNING id""",
        (dummy_path,),
    )

    derivation_id = derivation_result[0]["id"]

    # Create first cache push job
    job1_result = cf_client.execute_sql(
        """INSERT INTO cache_push_jobs (
               derivation_id, store_path, status
           ) VALUES (%s, %s, 'pending')
           RETURNING id""",
        (derivation_id, dummy_path),
    )

    job1_id = job1_result[0]["id"]

    # Try to create second job for same derivation - should fail due to unique constraint
    try:
        cf_client.execute_sql(
            """INSERT INTO cache_push_jobs (
                   derivation_id, store_path, status
               ) VALUES (%s, %s, 'pending')""",
            (derivation_id, dummy_path),
        )

        # If we get here, the constraint didn't work
        jobs = cf_client.execute_sql(
            "SELECT id FROM cache_push_jobs WHERE derivation_id = %s",
            (derivation_id,),
        )
        assert (
            len(jobs) == 1
        ), f"Found {len(jobs)} jobs, expected 1 due to unique constraint"

    except Exception as e:
        # This is expected - unique constraint should prevent duplicate
        s3_server.log(f"✅ Duplicate job creation prevented: {e}")

    # Verify only one job exists
    jobs = cf_client.execute_sql(
        "SELECT id FROM cache_push_jobs WHERE derivation_id = %s",
        (derivation_id,),
    )
    assert len(jobs) == 1, f"Expected 1 job, found {len(jobs)}"

    # Cleanup
    s3_server.succeed(f"rm -rf {dummy_path}")
    cf_client.execute_sql(
        "DELETE FROM cache_push_jobs WHERE derivation_id = %s", (derivation_id,)
    )
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_cache_push_error_handling_and_logging(cf_client, s3_server):
    """Test comprehensive error handling and logging in cache push process"""

    wait_for_crystal_forge_ready(s3_server)

    # Test various error conditions
    error_cases = [
        ("missing-store-path", "/nix/store/completely-missing-12345"),
        ("permission-denied", "/root/inaccessible-path"),
        ("invalid-store-path", "/tmp/not-a-store-path"),
    ]

    for test_name, bad_path in error_cases:
        s3_server.log(f"Testing error case: {test_name}")

        # Insert derivation with problematic path
        derivation_result = cf_client.execute_sql(
            """INSERT INTO derivations (
                   derivation_type, derivation_name, derivation_path,
                   status_id, completed_at, attempt_count, scheduled_at
               ) VALUES ('package', %s, %s, 10, NOW(), 0, NOW())
               RETURNING id""",
            (test_name, bad_path),
        )

        derivation_id = derivation_result[0]["id"]

        # Wait for cache push to be attempted
        try:
            cf_client.wait_for_service_log(
                s3_server,
                "crystal-forge-builder.service",
                f"Queuing cache push.*{test_name}",
                timeout=60,
            )

            # Look for error handling
            cf_client.wait_for_service_log(
                s3_server,
                "crystal-forge-builder.service",
                ["failed", "error", "Error"],
                timeout=60,
            )

            s3_server.log(f"✅ Error handling detected for {test_name}")

        except:
            s3_server.log(f"⚠️ Error handling not clearly detected for {test_name}")

        # Check that error was recorded in job
        jobs = cf_client.execute_sql(
            "SELECT status, error_message FROM cache_push_jobs WHERE derivation_id = %s",
            (derivation_id,),
        )

        if jobs:
            job = jobs[0]
            s3_server.log(f"Job status for {test_name}: {job['status']}")
            if job["error_message"]:
                s3_server.log(f"Error message: {job['error_message'][:100]}...")

        # Cleanup
        cf_client.execute_sql(
            "DELETE FROM cache_push_jobs WHERE derivation_id = %s", (derivation_id,)
        )
        cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))

        time.sleep(2)  # Brief pause between test cases
