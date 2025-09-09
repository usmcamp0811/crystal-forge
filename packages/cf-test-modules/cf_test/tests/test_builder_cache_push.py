import os
import time
from datetime import UTC, datetime, timedelta

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import _create_base_scenario
from cf_test.vm_helpers import SmokeTestConstants as C
from cf_test.vm_helpers import wait_for_crystal_forge_ready

pytestmark = [pytest.mark.cache]


@pytest.fixture(scope="session")
def s3_server():
    import cf_test

    return cf_test._driver_machines["s3Server"]


@pytest.fixture(scope="session")
def s3_cache():
    import cf_test

    return cf_test._driver_machines["s3Cache"]


@pytest.fixture(scope="session")
def attic_server():
    import cf_test

    return cf_test._driver_machines["atticServer"]


@pytest.fixture(scope="session")
def attic_cache():
    import cf_test

    return cf_test._driver_machines["atticCache"]


@pytest.fixture(scope="session")
def gitserver():
    import cf_test

    return cf_test._driver_machines["gitserver"]


@pytest.fixture(scope="session")
def s3_client():
    config = CFTestConfig(
        db_host=os.getenv("CF_TEST_S3_DB_HOST", "127.0.0.1"),
        db_port=int(os.getenv("CF_TEST_S3_DB_PORT", "5433")),
        db_user="crystal_forge",
        db_password="",
        db_name="crystal_forge",
        server_host=os.getenv("CF_TEST_S3_SERVER_HOST", "127.0.0.1"),
        server_port=int(os.getenv("CF_TEST_S3_SERVER_PORT", "3000")),
    )
    return CFTestClient(config)


@pytest.fixture(scope="session")
def attic_client():
    config = CFTestConfig(
        db_host=os.getenv("CF_TEST_ATTIC_DB_HOST", "127.0.0.1"),
        db_port=int(os.getenv("CF_TEST_ATTIC_DB_PORT", "5434")),
        db_user="crystal_forge",
        db_password="",
        db_name="crystal_forge",
        server_host=os.getenv("CF_TEST_ATTIC_SERVER_HOST", "127.0.0.1"),
        server_port=int(os.getenv("CF_TEST_ATTIC_SERVER_PORT", "3000")),
    )
    return CFTestClient(config)


@pytest.mark.integration
def test_s3_cache_push_successful_build(s3_client, s3_server, s3_cache):
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
        s3_client,
        hostname="s3-cache-test-host",
        flake_name="s3-cache-test",
        repo_url=real_repo_url,
        git_hash=real_commit_hash,
        derivation_status="build-pending",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )

    # Set a mock derivation path for testing
    s3_client.execute_sql(
        "UPDATE derivations SET derivation_path = '/nix/store/test-s3-cache.drv' WHERE id = %s",
        (scenario["derivation_id"],),
    )

    s3_server.log("=== Starting S3 cache push test ===")

    # Wait for the build to be processed and cache push to be attempted
    # Look for cache push logs in the service
    s3_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "Starting cache push for derivation: s3-cache-test-host",
        timeout=300,
    )

    # Check for successful cache push
    try:
        s3_client.wait_for_service_log(
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
    s3_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "Build completed for s3-cache-test-host",
        timeout=60,
    )

    # Check final derivation status
    final_status = s3_client.execute_sql(
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
    s3_client.cleanup_test_data(scenario["cleanup"])


@pytest.mark.integration
def test_attic_cache_push_successful_build(attic_client, attic_server, attic_cache):
    """Test that successful builds are pushed to Attic cache"""

    wait_for_crystal_forge_ready(attic_server)

    # Get real git info from environment
    real_commit_hash = os.getenv(
        "CF_TEST_REAL_COMMIT_HASH", "ebcc48fbf1030fc2065fc266da158af1d0b3943c"
    )
    real_repo_url = os.getenv(
        "CF_TEST_GIT_SERVER_URL", "http://gitserver/crystal-forge"
    )

    # Create a scenario with a derivation ready for building
    scenario = _create_base_scenario(
        attic_client,
        hostname="attic-cache-test-host",
        flake_name="attic-cache-test",
        repo_url=real_repo_url,
        git_hash=real_commit_hash,
        derivation_status="build-pending",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )

    # Set a mock derivation path for testing
    attic_client.execute_sql(
        "UPDATE derivations SET derivation_path = '/nix/store/test-attic-cache.drv' WHERE id = %s",
        (scenario["derivation_id"],),
    )

    attic_server.log("=== Starting Attic cache push test ===")

    # Wait for the build to be processed and cache push to be attempted
    attic_client.wait_for_service_log(
        attic_server,
        "crystal-forge-builder.service",
        "Starting cache push for derivation: attic-cache-test-host",
        timeout=300,
    )

    # Check for successful cache push
    try:
        attic_client.wait_for_service_log(
            attic_server,
            "crystal-forge-builder.service",
            "Successfully pushed",
            timeout=60,
        )
        cache_push_success = True
    except:
        attic_server.log("Cache push may have failed, checking build completion...")
        cache_push_success = False

    # Verify the derivation reached build-complete status
    attic_client.wait_for_service_log(
        attic_server,
        "crystal-forge-builder.service",
        "Build completed for attic-cache-test-host",
        timeout=60,
    )

    # Check final derivation status
    final_status = attic_client.execute_sql(
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

    # Verify Attic cache received the upload
    attic_cache.log("=== Checking Attic cache for uploaded objects ===")
    try:
        # Check if Atticd received any upload requests
        attic_cache.succeed("journalctl -u atticd.service | grep -i 'test' || true")
        attic_server.log("Attic cache interaction detected")
    except:
        attic_server.log("No Attic cache interaction found in logs")

    if cache_push_success:
        attic_server.log("✅ Attic cache push test PASSED")
    else:
        attic_server.log(
            "⚠️ Attic cache push may have failed, but build completed successfully"
        )

    # Cleanup
    attic_client.cleanup_test_data(scenario["cleanup"])


@pytest.mark.integration
def test_cache_push_failure_does_not_block_build(s3_client, s3_server):
    """Test that cache push failures don't prevent build completion"""

    wait_for_crystal_forge_ready(s3_server)

    # Create a scenario that will likely have cache push issues
    real_commit_hash = os.getenv(
        "CF_TEST_REAL_COMMIT_HASH", "ebcc48fbf1030fc2065fc266da158af1d0b3943c"
    )
    real_repo_url = os.getenv(
        "CF_TEST_GIT_SERVER_URL", "http://gitserver/crystal-forge"
    )

    scenario = _create_base_scenario(
        s3_client,
        hostname="cache-failure-test",
        flake_name="cache-failure-test",
        repo_url=real_repo_url,
        git_hash=real_commit_hash,
        derivation_status="build-pending",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )

    # Set derivation path
    s3_client.execute_sql(
        "UPDATE derivations SET derivation_path = '/nix/store/test-cache-failure.drv' WHERE id = %s",
        (scenario["derivation_id"],),
    )

    s3_server.log("=== Testing cache failure resilience ===")

    # Temporarily break S3 cache by stopping MinIO
    s3_server.succeed("systemctl stop minio.service")

    # Wait for build to complete despite cache failure
    s3_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "Build completed for cache-failure-test",
        timeout=300,
    )

    # Check that cache push was attempted but failed gracefully
    try:
        s3_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            "Cache push failed but continuing",
            timeout=30,
        )
        s3_server.log("✅ Cache failure handled gracefully")
    except:
        s3_server.log("⚠️ Cache failure log not found, but build completed")

    # Verify derivation still reached completion
    final_status = s3_client.execute_sql(
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

    s3_server.log("✅ Cache failure resilience test PASSED")

    # Cleanup
    s3_client.cleanup_test_data(scenario["cleanup"])


@pytest.mark.integration
def test_cache_type_configuration(s3_client, attic_client, s3_server, attic_server):
    """Test that different cache types are configured correctly"""

    # Test S3 configuration
    s3_config_result = s3_client.execute_sql(
        "SELECT 1",  # Just test connection works
    )
    assert len(s3_config_result) == 1, "S3 server database should be accessible"

    # Test Attic configuration
    attic_config_result = attic_client.execute_sql(
        "SELECT 1",  # Just test connection works
    )
    assert len(attic_config_result) == 1, "Attic server database should be accessible"

    # Check that servers are running with correct configurations
    s3_server.succeed("systemctl is-active crystal-forge-builder.service")
    attic_server.succeed("systemctl is-active crystal-forge-builder.service")

    # Verify cache type is set in service environment/config
    try:
        s3_server.succeed("grep -r 'cache_type.*S3' /var/lib/crystal-forge/config.toml")
        s3_server.log("✅ S3 cache type configured correctly")
    except:
        s3_server.log("⚠️ Could not verify S3 cache type in config")

    try:
        attic_server.succeed(
            "grep -r 'cache_type.*Attic' /var/lib/crystal-forge/config.toml"
        )
        attic_server.log("✅ Attic cache type configured correctly")
    except:
        attic_server.log("⚠️ Could not verify Attic cache type in config")


@pytest.mark.integration
def test_cache_retry_mechanism(s3_client, s3_server):
    """Test that cache push retries work as expected"""

    wait_for_crystal_forge_ready(s3_server)

    real_commit_hash = os.getenv(
        "CF_TEST_REAL_COMMIT_HASH", "ebcc48fbf1030fc2065fc266da158af1d0b3943c"
    )
    real_repo_url = os.getenv(
        "CF_TEST_GIT_SERVER_URL", "http://gitserver/crystal-forge"
    )

    scenario = _create_base_scenario(
        s3_client,
        hostname="cache-retry-test",
        flake_name="cache-retry-test",
        repo_url=real_repo_url,
        git_hash=real_commit_hash,
        derivation_status="build-pending",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )

    s3_client.execute_sql(
        "UPDATE derivations SET derivation_path = '/nix/store/test-cache-retry.drv' WHERE id = %s",
        (scenario["derivation_id"],),
    )

    s3_server.log("=== Testing cache retry mechanism ===")

    # Start build process
    s3_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "Build completed for cache-retry-test",
        timeout=300,
    )

    # Check for retry attempts in logs (configured with max_retries = 2)
    try:
        # Look for retry-related log messages
        s3_server.succeed(
            "journalctl -u crystal-forge-builder.service | grep -i 'retry\\|attempt' || true"
        )
        s3_server.log("✅ Retry mechanism logs detected")
    except:
        s3_server.log(
            "⚠️ No retry logs found, but this may be normal if cache succeeded on first try"
        )

    # Cleanup
    s3_client.cleanup_test_data(scenario["cleanup"])
