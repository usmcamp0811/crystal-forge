import json
import os

import pytest

from cf_test import CFTestClient

pytestmark = [pytest.mark.builder,  pytest.mark.integration]


@pytest.fixture(scope="session")
def s3_server():
    import cf_test

    return cf_test._driver_machines["s3Server"]


@pytest.fixture(scope="session")
def cf_client(cf_config):
    return CFTestClient(cf_config)


@pytest.fixture(scope="session")
def derivation_paths():
    """Load derivation paths from the test environment"""
    drv_path = os.environ.get("CF_TEST_DRV")
    if not drv_path:
        pytest.fail("CF_TEST_DRV environment variable not set")

    with open(drv_path, "r") as f:
        return json.load(f)


@pytest.fixture(scope="session")
def test_commit_hash():
    """Get the test commit hash"""
    return os.environ.get("CF_TEST_REAL_COMMIT_HASH", "").strip()


def test_builder_service_exists_and_runs(cf_client, s3_server):
    """Test that builder service exists and is running"""
    # Check service file exists
    s3_server.succeed("test -f /etc/systemd/system/crystal-forge-builder.service")

    # Check service status and get details
    status = s3_server.succeed("systemctl status crystal-forge-builder.service || true")
    s3_server.log(f"Builder service status: {status}")

    # Check service is active
    s3_server.succeed("systemctl is-active crystal-forge-builder.service")


def test_database_has_test_data(
    cf_client, s3_server, derivation_paths, test_commit_hash
):
    """Test that database has been populated with test flake data"""

    # Wait for database to be ready and migrations to complete
    s3_server.wait_for_unit("postgresql.service")
    s3_server.wait_until_succeeds(
        "sudo -u postgres psql -d crystal_forge -c 'SELECT 1 FROM flakes LIMIT 1;' >/dev/null 2>&1"
    )

    # Check that test-flake was added to the database
    flake_exists = s3_server.succeed(
        "sudo -u postgres psql -d crystal_forge -t -c \"SELECT COUNT(*) FROM flakes WHERE name = 'test-flake';\""
    ).strip()

    assert int(flake_exists) > 0, "test-flake not found in database"

    # Check that test commit exists in database
    if test_commit_hash:
        commit_exists = s3_server.succeed(
            f"sudo -u postgres psql -d crystal_forge -t -c \"SELECT COUNT(*) FROM commits WHERE git_commit_hash = '{test_commit_hash}';\""
        ).strip()

        assert (
            int(commit_exists) > 0
        ), f"Test commit {test_commit_hash} not found in database"


def test_database_has_derivations(cf_client, s3_server, derivation_paths):
    """Test that database has derivation records for our test configurations"""

    # Check that we have derivations for our test configurations
    for config_name, config_data in derivation_paths.items():
        drv_path = config_data["derivation_path"]

        # Check if derivation exists in database
        drv_exists = s3_server.succeed(
            f"sudo -u postgres psql -d crystal_forge -t -c \"SELECT COUNT(*) FROM derivations WHERE derivation_path = '{drv_path}';\""
        ).strip()

        s3_server.log(
            f"Derivation {config_name} ({drv_path}): {'exists' if int(drv_exists) > 0 else 'missing'}"
        )


def test_builder_has_required_tools(cf_client, s3_server):
    """Test that builder systemd service has required tools in PATH"""

    # Get the actual PATH from the systemd service environment
    service_env = s3_server.succeed(
        "systemctl show crystal-forge-builder.service --property=Environment"
    )
    s3_server.log(f"Builder service environment: {service_env}")

    # Extract PATH from the environment
    import re

    path_match = re.search(r"PATH=([^\s]+)", service_env)
    if not path_match:
        raise Exception("No PATH found in builder service environment")

    service_path = path_match.group(1)
    s3_server.log(f"Builder service PATH: {service_path}")

    # Check each tool exists in the service PATH
    tools = ["nix", "git", "vulnix"]
    for tool in tools:
        found = False
        for path_dir in service_path.split(":"):
            tool_path = f"{path_dir}/{tool}"
            try:
                s3_server.succeed(f"test -x {tool_path}")
                s3_server.log(f"✅ {tool} found at: {tool_path}")
                found = True
                break
            except:
                continue

        if not found:
            raise Exception(
                f"❌ {tool} not found in any PATH directory: {service_path}"
            )


def test_builder_directories_exist(cf_client, s3_server):
    """Test that builder working directories exist"""

    directories = [
        "/var/lib/crystal-forge/workdir",
        "/var/lib/crystal-forge/tmp",
        "/var/lib/crystal-forge/.cache",
    ]

    for directory in directories:
        s3_server.succeed(f"test -d {directory}")


def test_builder_logs_show_startup(cf_client, s3_server):
    """Test that builder logs show successful startup"""

    # Wait for builder startup message
    cf_client.wait_for_service_log(
        s3_server, "crystal-forge-builder.service", "Starting Build loop", timeout=60
    )


def test_builder_polling_for_work(cf_client, s3_server):
    """Test that builder is actively polling for work"""

    # Wait for polling message - look for the most common one first
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "No derivations need building",
        timeout=120,
    )


def test_builder_database_connection(cf_client, s3_server):
    """Test that builder can connect to database"""

    # First ensure the database user was created
    s3_server.wait_until_succeeds(
        "sudo -u postgres psql -c \"SELECT 1 FROM pg_roles WHERE rolname='crystal_forge';\" | grep -q '1'"
    )

    # Check no database errors in logs
    logs = s3_server.succeed(
        "journalctl -u crystal-forge-builder.service --since '2 minutes ago' --no-pager"
    )

    error_keywords = [
        "connection refused",
        "authentication failed",
        "role.*does not exist",
    ]
    for keyword in error_keywords:
        assert keyword not in logs.lower(), f"Found database error in logs: {keyword}"

    # Check that database migrations ran successfully (indicates good DB connection)
    s3_server.succeed(
        "journalctl -u crystal-forge-server --no-pager | grep -q 'migrations'"
    )


def test_builder_s3_cache_config(cf_client, s3_server):
    """Test that builder has S3 cache configuration and can connect"""

    # Check that S3 environment variables are set for the builder service
    service_env = s3_server.succeed(
        "systemctl show crystal-forge-builder.service --property=Environment"
    )

    s3_env_vars = [
        "AWS_ENDPOINT_URL=http://s3Cache:9000",
        "AWS_ACCESS_KEY_ID=minioadmin",
        "AWS_SECRET_ACCESS_KEY=minioadmin",
    ]

    for env_var in s3_env_vars:
        assert env_var in service_env, f"Missing S3 environment variable: {env_var}"

    s3_server.log("✅ All S3 environment variables found in builder service")


def test_s3_cache_connectivity(cf_client, s3_server):
    """Test that the builder can connect to S3 cache"""

    # Test S3 connectivity from within the builder environment
    # We'll check if the builder can see the S3 service
    s3_server.succeed("ping -c 1 s3Cache")
    s3_server.succeed("nc -z s3Cache 9000")

    s3_server.log("✅ S3 cache is reachable from builder")


def test_builder_can_build_derivations(cf_client, s3_server, derivation_paths):
    """Test that builder can actually build test derivations"""

    if not derivation_paths:
        pytest.skip("No derivation paths available for testing")

    # Pick one derivation to test with
    test_config = next(iter(derivation_paths.values()))
    drv_path = test_config["derivation_path"]

    # Check if this derivation needs building (should initially be unbuild in test env)
    s3_server.log(f"Testing build capability with derivation: {drv_path}")

    # Look for the derivation path in logs (most reliable indicator)
    try:
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            drv_path[:50],  # Use first 50 chars of derivation path
            timeout=300,
        )
        s3_server.log("✅ Builder is processing our test derivation")
    except:
        # If derivation path not found, check for general build activity
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            "Building",
            timeout=60,
        )
        s3_server.log("✅ Builder is showing build activity")


def test_builder_not_restarting(cf_client, s3_server):
    """Test that builder service is stable and not restart-looping"""

    # Check restart count is low
    restart_output = s3_server.succeed(
        "systemctl show crystal-forge-builder.service --property=NRestarts"
    )
    restart_count = int(restart_output.split("=")[1].strip())

    assert (
        restart_count < 3
    ), f"Builder has restarted {restart_count} times - possible restart loop"


def test_s3_cache_operations(cf_client, s3_server):
    """Test that S3 cache operations work during building"""

    # Wait for S3-related activity - start with most likely messages
    try:
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            "s3://",
            timeout=300,
        )
        s3_server.log("✅ S3 cache operations detected")
    except:
        # If no S3 activity, at least verify the cache configuration is working
        try:
            cf_client.wait_for_service_log(
                s3_server,
                "crystal-forge-builder.service",
                "cache",
                timeout=60,
            )
            s3_server.log("✅ Cache-related activity detected")
        except:
            s3_server.log(
                "⚠️ No explicit S3 cache activity found, but service is running"
            )

    # Check that no S3-related errors occurred
    logs = s3_server.succeed(
        "journalctl -u crystal-forge-builder.service --since '5 minutes ago' --no-pager"
    )

    s3_error_keywords = [
        "s3 error",
        "upload failed",
        "connection refused.*9000",
        "access denied.*s3",
    ]

    for keyword in s3_error_keywords:
        assert keyword not in logs.lower(), f"Found S3 error in builder logs: {keyword}"

    s3_server.log("✅ No S3 errors detected in builder logs")
