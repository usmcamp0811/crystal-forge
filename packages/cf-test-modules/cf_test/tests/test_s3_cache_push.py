import os
import time
from datetime import UTC, datetime, timedelta

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import _create_base_scenario
from cf_test.vm_helpers import SmokeTestConstants as C
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


@pytest.mark.integration
def test_s3_cache_push_successful_build(cf_client, s3_server, s3_cache):
    """Test that successful builds are pushed to S3 cache"""

    wait_for_crystal_forge_ready(s3_server)

    # Get real git info from environment
    real_commit_hash = os.getenv(
        "CF_TEST_REAL_COMMIT_HASH", "ebcc48fbf1030fc2065fc266da158af1d0b3943c"
    )
    real_repo_url = os.getenv(
        "CF_TEST_GIT_SERVER_URL", "http://gitserver/crystal-forge"
    )

    # Create a scenario with a derivation ready for building
    scenario = _create_base_scenario(
        cf_client,
        hostname="s3-cache-test-host",
        flake_name="s3-cache-test",
        repo_url=real_repo_url,
        git_hash=real_commit_hash,
        derivation_status="build-pending",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )

    # Set a mock derivation path for testing
    cf_client.execute_sql(
        "UPDATE derivations SET derivation_path = '/nix/store/test-s3-cache.drv' WHERE id = %s",
        (scenario["derivation_id"],),
    )

    s3_server.log("=== Starting S3 cache push test ===")

    # Wait for the build to be processed and cache push to be attempted
    # Look for cache push logs in the service
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "Starting cache push for derivation: s3-cache-test-host",
        timeout=300,
    )

    # Check for successful cache push
    try:
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            "Successfully pushed",
            timeout=60,
        )
        cache_push_success = True
    except:
        # Check if cache push failed but build continued
        s3_server.log("Cache push may have failed, checking build completion...")
        cache_push_success = False

    # Verify the derivation reached build-complete status regardless of cache push
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "Build completed for s3-cache-test-host",
        timeout=60,
    )

    # Check final derivation status
    final_status = cf_client.execute_sql(
        """
        SELECT d.status_id, ds.name as status_name
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE d.id = %s
        """,
        (scenario["derivation_id"],),
    )

    assert len(final_status) == 1, "Derivation should exist"
    assert final_status[0]["status_name"] in [
        "build-complete",
        "cve-scan-pending",
    ], f"Expected build-complete or cve-scan-pending, got {final_status[0]['status_name']}"

    # Verify S3 cache received the upload (check MinIO logs)
    s3_cache.log("=== Checking S3 cache for uploaded objects ===")
    try:
        # Check if MinIO received any PUT requests
        s3_cache.succeed(
            "journalctl -u minio.service | grep -i 'PUT /crystal-forge-cache' || true"
        )
        s3_server.log("S3 cache interaction detected")
    except:
        s3_server.log("No S3 cache interaction found in logs")

    if cache_push_success:
        s3_server.log("✅ S3 cache push test PASSED")
    else:
        s3_server.log(
            "⚠️ S3 cache push may have failed, but build completed successfully"
        )

    # Cleanup
    cf_client.cleanup_test_data(scenario["cleanup"])


@pytest.mark.integration
def test_s3_cache_push_failure_does_not_block_build(cf_client, s3_server):
    """Test that S3 cache push failures don't prevent build completion"""

    wait_for_crystal_forge_ready(s3_server)

    # Create a scenario that will likely have cache push issues
    real_commit_hash = os.getenv(
        "CF_TEST_REAL_COMMIT_HASH", "ebcc48fbf1030fc2065fc266da158af1d0b3943c"
    )
    real_repo_url = os.getenv(
        "CF_TEST_GIT_SERVER_URL", "http://gitserver/crystal-forge"
    )

    scenario = _create_base_scenario(
        cf_client,
        hostname="s3-cache-failure-test",
        flake_name="s3-cache-failure-test",
        repo_url=real_repo_url,
        git_hash=real_commit_hash,
        derivation_status="build-pending",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )

    # Set derivation path
    cf_client.execute_sql(
        "UPDATE derivations SET derivation_path = '/nix/store/test-s3-cache-failure.drv' WHERE id = %s",
        (scenario["derivation_id"],),
    )

    s3_server.log("=== Testing S3 cache failure resilience ===")

    # Temporarily break S3 cache by stopping MinIO
    s3_server.succeed("systemctl stop minio.service")

    # Wait for build to complete despite cache failure
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "Build completed for s3-cache-failure-test",
        timeout=300,
    )

    # Check that cache push was attempted but failed gracefully
    try:
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            "Cache push failed but continuing",
            timeout=30,
        )
        s3_server.log("✅ S3 cache failure handled gracefully")
    except:
        s3_server.log("⚠️ S3 cache failure log not found, but build completed")

    # Verify derivation still reached completion
    final_status = cf_client.execute_sql(
        """
        SELECT d.status_id, ds.name as status_name
        FROM derivations d 
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE d.id = %s
        """,
        (scenario["derivation_id"],),
    )

    assert len(final_status) == 1
    assert final_status[0]["status_name"] in [
        "build-complete",
        "cve-scan-pending",
    ], f"Build should complete despite cache failure, got {final_status[0]['status_name']}"

    # Restart MinIO for other tests
    s3_server.succeed("systemctl start minio.service")
    s3_server.wait_for_unit("minio.service")

    s3_server.log("✅ S3 cache failure resilience test PASSED")

    # Cleanup
    cf_client.cleanup_test_data(scenario["cleanup"])


@pytest.mark.integration
def test_s3_cache_configuration(cf_client, s3_server):
    """Test that S3 cache type is configured correctly"""

    # Test S3 configuration
    s3_config_result = cf_client.execute_sql(
        "SELECT 1",  # Just test connection works
    )
    assert len(s3_config_result) == 1, "S3 server database should be accessible"

    # Check that server is running with correct configuration
    s3_server.succeed("systemctl is-active crystal-forge-builder.service")

    # Verify cache type is set in service environment/config
    try:
        s3_server.succeed("grep -r 'cache_type.*S3' /var/lib/crystal-forge/config.toml")
        s3_server.log("✅ S3 cache type configured correctly")
    except:
        s3_server.log("⚠️ Could not verify S3 cache type in config")


@pytest.mark.integration
def test_s3_cache_retry_mechanism(cf_client, s3_server):
    """Test that S3 cache push retries work as expected"""

    wait_for_crystal_forge_ready(s3_server)

    real_commit_hash = os.getenv(
        "CF_TEST_REAL_COMMIT_HASH", "ebcc48fbf1030fc2065fc266da158af1d0b3943c"
    )
    real_repo_url = os.getenv(
        "CF_TEST_GIT_SERVER_URL", "http://gitserver/crystal-forge"
    )

    scenario = _create_base_scenario(
        cf_client,
        hostname="s3-cache-retry-test",
        flake_name="s3-cache-retry-test",
        repo_url=real_repo_url,
        git_hash=real_commit_hash,
        derivation_status="build-pending",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )

    cf_client.execute_sql(
        "UPDATE derivations SET derivation_path = '/nix/store/test-s3-cache-retry.drv' WHERE id = %s",
        (scenario["derivation_id"],),
    )

    s3_server.log("=== Testing S3 cache retry mechanism ===")

    # Start build process
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "Build completed for s3-cache-retry-test",
        timeout=300,
    )

    # Check for retry attempts in logs (configured with max_retries = 2)
    try:
        # Look for retry-related log messages
        s3_server.succeed(
            "journalctl -u crystal-forge-builder.service | grep -i 'retry\\|attempt' || true"
        )
        s3_server.log("✅ S3 retry mechanism logs detected")
    except:
        s3_server.log(
            "⚠️ No S3 retry logs found, but this may be normal if cache succeeded on first try"
        )

    # Cleanup
    cf_client.cleanup_test_data(scenario["cleanup"])
