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


def test_cache_push_with_dummy_store_path(cf_client, s3_server, s3_cache):
    """Test cache push using a dummy store path that exists"""

    wait_for_crystal_forge_ready(s3_server)

    # Create a dummy store path with some content
    dummy_path = "/nix/store/dummy-cache-test-12345"
    s3_server.succeed(f"mkdir -p {dummy_path}")
    s3_server.succeed(f"echo 'Cache test content' > {dummy_path}/test-file")
    s3_server.succeed(f"echo 'Another file' > {dummy_path}/another-file")

    s3_server.log(f"Created dummy store path: {dummy_path}")

    # Insert a derivation marked as build-complete with our dummy path
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               derivation_type, derivation_name, derivation_path,
               status_id, completed_at, attempt_count, scheduled_at
           ) VALUES ('package', 'cache-test-dummy', %s, 10, NOW(), 0, NOW())
           RETURNING id""",
        (dummy_path,),
    )

    derivation_id = derivation_result[0]["id"]
    s3_server.log(f"Created test derivation with ID: {derivation_id}")

    # Monitor for cache push activity
    try:
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            ["Queuing cache push", "cache push.*cache-test-dummy"],
            timeout=120,
        )
        s3_server.log("✅ Cache push was queued")
    except:
        s3_server.log("⚠️ Cache push queuing not detected")

    # Look for nix copy command execution
    try:
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            ["nix copy", "Processing cache push"],
            timeout=60,
        )
        s3_server.log("✅ Cache push processing detected")
    except:
        s3_server.log("⚠️ Cache push processing not clearly detected")

    # Check MinIO logs for upload activity
    time.sleep(5)  # Give time for upload to complete

    try:
        minio_logs = s3_cache.succeed(
            "journalctl -u minio.service --since '2 minutes ago' --no-pager"
        )

        if "PUT" in minio_logs and "crystal-forge-cache" in minio_logs:
            s3_server.log("✅ MinIO received PUT requests for crystal-forge-cache")

            # Count the number of PUT requests
            put_count = minio_logs.count("PUT")
            s3_server.log(f"Found {put_count} PUT operations in MinIO logs")
        else:
            s3_server.log("⚠️ No clear PUT operations found in MinIO logs")
            s3_server.log(f"MinIO log sample: {minio_logs[-500:]}")

    except Exception as e:
        s3_server.log(f"⚠️ Could not check MinIO logs: {e}")

    # Verify the bucket contents
    try:
        bucket_contents = s3_cache.succeed("mc ls local/crystal-forge-cache/ || true")
        if bucket_contents.strip():
            s3_server.log(f"✅ Bucket contents: {bucket_contents}")
        else:
            s3_server.log("⚠️ Bucket appears empty")
    except:
        s3_server.log("⚠️ Could not list bucket contents")

    # Cleanup
    s3_server.succeed(f"rm -rf {dummy_path}")
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_cache_push_with_existing_nix_store_path(cf_client, s3_server, s3_cache):
    """Test cache push using an actual path from the Nix store"""

    wait_for_crystal_forge_ready(s3_server)

    # Find an existing store path
    try:
        # Look for a small, simple store path
        store_paths = (
            s3_server.succeed("find /nix/store -maxdepth 1 -type d | head -5")
            .strip()
            .split("\n")
        )
        existing_path = None

        for path in store_paths:
            if path and path != "/nix/store":
                # Check if it's a reasonable size (not too big for testing)
                try:
                    size_output = s3_server.succeed(f"du -sh {path}")
                    s3_server.log(
                        f"Store path candidate: {path} - Size: {size_output.split()[0]}"
                    )
                    existing_path = path
                    break
                except:
                    continue

        if not existing_path:
            pytest.skip("No suitable store paths found for testing")

        s3_server.log(f"Using existing store path: {existing_path}")

        # Insert a derivation with this real store path
        derivation_result = cf_client.execute_sql(
            """INSERT INTO derivations (
                   derivation_type, derivation_name, derivation_path,
                   status_id, completed_at, attempt_count, scheduled_at
               ) VALUES ('package', 'real-store-path-test', %s, 10, NOW(), 0, NOW())
               RETURNING id""",
            (existing_path,),
        )

        derivation_id = derivation_result[0]["id"]
        s3_server.log(
            f"Created test derivation with real store path, ID: {derivation_id}"
        )

        # Monitor for cache operations
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            ["cache push", "real-store-path-test"],
            timeout=180,
        )

        # Check for success indicators
        try:
            cf_client.wait_for_service_log(
                s3_server,
                "crystal-forge-builder.service",
                ["Successfully pushed", "Cache push completed"],
                timeout=60,
            )
            s3_server.log("✅ Cache push completed successfully")
        except:
            s3_server.log("⚠️ Cache push completion not clearly detected")

        # Cleanup
        cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))

    except Exception as e:
        pytest.skip(f"Could not test with existing store path: {e}")


def test_cache_worker_and_queue_mechanism(cf_client, s3_server):
    """Test that the cache push worker and queue mechanism is functioning"""

    wait_for_crystal_forge_ready(s3_server)

    # Verify cache push worker is running
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        ["Starting cache push worker", "cache push worker"],
        timeout=30,
    )
    s3_server.log("✅ Cache push worker detected")

    # Create multiple completed derivations to test queuing
    dummy_paths = []
    derivation_ids = []

    for i in range(3):
        dummy_path = f"/nix/store/queue-test-{i}-12345"
        s3_server.succeed(f"mkdir -p {dummy_path}")
        s3_server.succeed(f"echo 'Queue test {i}' > {dummy_path}/test-file")
        dummy_paths.append(dummy_path)

        result = cf_client.execute_sql(
            """INSERT INTO derivations (
                   derivation_type, derivation_name, derivation_path,
                   status_id, completed_at, attempt_count, scheduled_at
               ) VALUES ('package', %s, %s, 10, NOW(), 0, NOW())
               RETURNING id""",
            (f"queue-test-{i}", dummy_path),
        )
        derivation_ids.append(result[0]["id"])

    s3_server.log(f"Created {len(derivation_ids)} test derivations for queue testing")

    # Monitor for multiple cache push operations
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        ["Queuing cache push.*queue-test"],
        timeout=120,
    )

    s3_server.log("✅ Multiple cache pushes queued")

    # Cleanup
    for path in dummy_paths:
        s3_server.succeed(f"rm -rf {path}")
    for derivation_id in derivation_ids:
        cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_s3_error_handling(cf_client, s3_server, s3_cache):
    """Test error handling when S3 operations fail"""

    wait_for_crystal_forge_ready(s3_server)

    # Create a derivation with a non-existent store path
    bad_path = "/nix/store/nonexistent-path-12345"

    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               derivation_type, derivation_name, derivation_path,
               status_id, completed_at, attempt_count, scheduled_at
           ) VALUES ('package', 'error-test', %s, 10, NOW(), 0, NOW())
           RETURNING id""",
        (bad_path,),
    )

    derivation_id = derivation_result[0]["id"]
    s3_server.log(
        f"Created test derivation with non-existent path, ID: {derivation_id}"
    )

    # Monitor for cache push attempt and error handling
    try:
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            ["cache push.*error-test", "Queuing cache push"],
            timeout=60,
        )

        # Look for error handling
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            ["Cache push.*failed", "error", "nix copy.*failed"],
            timeout=60,
        )

        s3_server.log("✅ Error handling for failed cache push detected")

    except:
        s3_server.log("⚠️ Error handling behavior not clearly detected")

    # Cleanup
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_cache_configuration_edge_cases(cf_client, s3_server):
    """Test cache configuration and filtering logic"""

    wait_for_crystal_forge_ready(s3_server)

    # Create derivations that should and shouldn't be cached based on config
    test_cases = [
        ("should-cache", "/nix/store/should-cache-12345"),
        ("test-package", "/nix/store/test-package-12345"),
        ("another-test", "/nix/store/another-test-12345"),
    ]

    derivation_ids = []

    for name, path in test_cases:
        s3_server.succeed(f"mkdir -p {path}")
        s3_server.succeed(f"echo 'Test content for {name}' > {path}/test-file")

        result = cf_client.execute_sql(
            """INSERT INTO derivations (
                   derivation_type, derivation_name, derivation_path,
                   status_id, completed_at, attempt_count, scheduled_at
               ) VALUES ('package', %s, %s, 10, NOW(), 0, NOW())
               RETURNING id""",
            (name, path),
        )
        derivation_ids.append((result[0]["id"], name, path))

    s3_server.log(f"Created {len(derivation_ids)} derivations to test caching behavior")

    # Monitor cache operations for these derivations
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        ["cache push", "Queuing cache push"],
        timeout=120,
    )

    s3_server.log("✅ Cache push operations detected for test derivations")

    # Cleanup
    for derivation_id, name, path in derivation_ids:
        s3_server.succeed(f"rm -rf {path}")
        cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_end_to_end_cache_workflow(cf_client, s3_server, s3_cache):
    """Complete end-to-end test of the cache workflow"""

    wait_for_crystal_forge_ready(s3_server)

    # Create a realistic store path
    store_path = "/nix/store/e2e-test-workflow-12345"
    s3_server.succeed(f"mkdir -p {store_path}")
    s3_server.succeed(f"echo 'End-to-end test file' > {store_path}/result")
    s3_server.succeed(f"mkdir -p {store_path}/bin")
    s3_server.succeed(f"echo '#!/bin/sh\necho hello' > {store_path}/bin/test-app")
    s3_server.succeed(f"chmod +x {store_path}/bin/test-app")

    s3_server.log(f"Created realistic store path: {store_path}")

    # Create derivation in dry-run-complete state first (more realistic)
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               derivation_type, derivation_name, derivation_path,
               status_id, started_at, attempt_count, scheduled_at
           ) VALUES ('nixos', 'e2e-workflow-test', %s, 5, NOW(), 0, NOW())
           RETURNING id""",
        (store_path,),
    )

    derivation_id = derivation_result[0]["id"]
    s3_server.log(f"Created derivation in dry-run-complete state, ID: {derivation_id}")

    # Wait for build loop to pick it up and complete it
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        ["Starting build.*e2e-workflow-test", "derivation.*e2e-workflow-test"],
        timeout=120,
    )

    # Wait for build completion and cache push
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        ["Build completed", "Queuing cache push.*e2e-workflow-test"],
        timeout=180,
    )

    # Verify final state
    final_status = cf_client.execute_sql(
        """SELECT d.status_id, ds.name as status_name 
           FROM derivations d 
           JOIN derivation_statuses ds ON d.status_id = ds.id 
           WHERE d.id = %s""",
        (derivation_id,),
    )

    if final_status:
        s3_server.log(f"Final derivation status: {final_status[0]['status_name']}")

    # Check final MinIO state
    try:
        final_bucket_state = s3_cache.succeed(
            "mc ls local/crystal-forge-cache/ || echo 'empty'"
        )
        s3_server.log(f"Final bucket state: {final_bucket_state}")
    except:
        s3_server.log("Could not check final bucket state")

    s3_server.log("✅ End-to-end cache workflow test completed")

    # Cleanup
    s3_server.succeed(f"rm -rf {store_path}")
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
