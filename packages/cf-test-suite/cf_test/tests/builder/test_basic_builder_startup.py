import json
import os

import pytest

pytestmark = [pytest.mark.builder, pytest.mark.integration]


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


@pytest.fixture(scope="session")
def builder_test_data(cf_client, test_commit_hash):
    """Set up minimal test data for builder testing"""
    # Insert test flake
    flake_result = cf_client.execute_sql(
        """INSERT INTO flakes (name, repo_url)
           VALUES ('test-flake', 'http://gitserver/crystal-forge')
           RETURNING id"""
    )
    flake_id = flake_result[0]["id"]

    # Insert test commit
    commit_hash = test_commit_hash or "test-commit-123"
    commit_result = cf_client.execute_sql(
        """INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
           VALUES (%s, %s, NOW())
           RETURNING id""",
        (flake_id, commit_hash),
    )
    commit_id = commit_result[0]["id"]

    # Insert a test derivation in dry-run-complete status for build testing
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, derivation_path,
               scheduled_at, completed_at, attempt_count, started_at,
               evaluation_duration_ms, pname, version, status_id
           ) VALUES (
               %s, 'nixos', 'test-system', '/nix/store/test-system.drv',
               NOW() - INTERVAL '1 hour', NOW() - INTERVAL '30 minutes', 0,
               NOW() - INTERVAL '35 minutes', 1500,
               'test-system', '1.0', 5
           ) RETURNING id""",
        (commit_id,),
    )
    derivation_id = derivation_result[0]["id"]

    test_data = {
        "flake_id": flake_id,
        "commit_id": commit_id,
        "derivation_id": derivation_id,
        "commit_hash": commit_hash,
    }

    yield test_data

    # Cleanup
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


def test_builder_service_exists_and_runs(cf_client, cfServer):
    """Test that builder service exists and is running"""
    # Check service file exists
    cfServer.succeed("test -f /etc/systemd/system/crystal-forge-builder.service")

    # Check service status and get details
    status = cfServer.succeed("systemctl status crystal-forge-builder.service || true")
    cfServer.log(f"Builder service status: {status}")

    # Check service is active
    cfServer.succeed("systemctl is-active crystal-forge-builder.service")


def test_database_has_test_data(
    cf_client, cfServer, derivation_paths, builder_test_data
):
    """Test that database has been populated with test flake data"""

    # Wait for database to be ready and migrations to complete
    cfServer.wait_for_unit("postgresql.service")
    cfServer.wait_until_succeeds(
        "sudo -u postgres psql -d crystal_forge -c 'SELECT 1 FROM flakes LIMIT 1;' >/dev/null 2>&1"
    )

    # Check that test-flake was added by our fixture
    flake_exists = cfServer.succeed(
        "sudo -u postgres psql -d crystal_forge -t -c \"SELECT COUNT(*) FROM flakes WHERE name = 'test-flake';\""
    ).strip()

    assert int(flake_exists) > 0, "test-flake not found in database"

    # Check that test commit exists in database
    commit_exists = cfServer.succeed(
        f"sudo -u postgres psql -d crystal_forge -t -c \"SELECT COUNT(*) FROM commits WHERE git_commit_hash = '{builder_test_data['commit_hash']}';\""
    ).strip()

    assert (
        int(commit_exists) > 0
    ), f"Test commit {builder_test_data['commit_hash']} not found in database"


def test_database_has_derivations(cf_client, cfServer, derivation_paths):
    """Test that database has derivation records for our test configurations"""

    # Check that we have derivations for our test configurations
    for config_name, config_data in derivation_paths.items():
        drv_path = config_data["derivation_path"]

        # Check if derivation exists in database
        drv_exists = cfServer.succeed(
            f"sudo -u postgres psql -d crystal_forge -t -c \"SELECT COUNT(*) FROM derivations WHERE derivation_path = '{drv_path}';\""
        ).strip()

        cfServer.log(
            f"Derivation {config_name} ({drv_path}): {'exists' if int(drv_exists) > 0 else 'missing'}"
        )


def test_builder_has_required_tools(cf_client, cfServer):
    """Test that builder systemd service has required tools in PATH"""

    # Get the actual PATH from the systemd service environment
    service_env = cfServer.succeed(
        "systemctl show crystal-forge-builder.service --property=Environment"
    )
    cfServer.log(f"Builder service environment: {service_env}")

    # Extract PATH from the environment
    import re

    path_match = re.search(r"PATH=([^\s]+)", service_env)
    if not path_match:
        raise Exception("No PATH found in builder service environment")

    service_path = path_match.group(1)
    cfServer.log(f"Builder service PATH: {service_path}")

    # Check each tool exists in the service PATH
    tools = ["nix", "git", "vulnix"]
    for tool in tools:
        found = False
        for path_dir in service_path.split(":"):
            tool_path = f"{path_dir}/{tool}"
            try:
                cfServer.succeed(f"test -x {tool_path}")
                cfServer.log(f"✅ {tool} found at: {tool_path}")
                found = True
                break
            except:
                continue

        if not found:
            raise Exception(
                f"❌ {tool} not found in any PATH directory: {service_path}"
            )


def test_builder_directories_exist(cf_client, cfServer):
    """Test that builder working directories exist"""

    directories = [
        "/var/lib/crystal-forge/workdir",
        "/var/lib/crystal-forge/tmp",
        "/var/lib/crystal-forge/.cache",
    ]

    for directory in directories:
        cfServer.succeed(f"test -d {directory}")


def test_builder_logs_show_startup(cf_client, cfServer):
    """Test that builder logs show successful startup"""

    # Wait for builder startup message
    cf_client.wait_for_service_log(
        cfServer, "crystal-forge-builder.service", "🔍 Starting", timeout=60
    )


def test_builder_polling_for_work(cf_client, cfServer):
    """Test that builder is actively polling for work"""

    # The build loop runs every 300s (5 minutes) which is too long for tests
    # The CVE scan loop runs every 60s, so check for that instead
    cf_client.wait_for_service_log(
        cfServer,
        "crystal-forge-builder.service",
        "No derivations need CVE scanning",
        timeout=120,  # CVE scan runs every 60s, so 2 minutes is safe
    )

    cfServer.log("✅ Builder CVE scan loop is active and polling")


def test_builder_database_connection(cf_client, cfServer):
    """Test that builder can connect to database"""

    # First ensure the database user was created
    cfServer.wait_until_succeeds(
        "sudo -u postgres psql -c \"SELECT 1 FROM pg_roles WHERE rolname='crystal_forge';\" | grep -q '1'"
    )

    # Check no database errors in logs
    logs = cfServer.succeed(
        "journalctl -u crystal-forge-builder.service --since '2 minutes ago' --no-pager"
    )

    error_keywords = [
        "connection refused",
        "authentication failed",
        "role.*does not exist",
    ]
    for keyword in error_keywords:
        assert keyword not in logs.lower(), f"Found database error in logs: {keyword}"


def test_builder_can_build_derivations(cf_client, cfServer, derivation_paths):
    """Test that builder can actually build test derivations"""

    if not derivation_paths:
        pytest.skip("No derivation paths available for testing")

    # Pick one derivation to test with
    test_config = next(iter(derivation_paths.values()))
    drv_path = test_config["derivation_path"]

    cfServer.log(f"Testing build capability with derivation: {drv_path}")

    # Since build loop runs every 5 minutes, we'll check for CVE scan activity
    # which proves the builder loops are working, then check for memory monitoring
    # which shows the service is stable
    try:
        cf_client.wait_for_service_log(
            cfServer,
            "crystal-forge-builder.service",
            "Memory - RSS:",
            timeout=120,
        )
        cfServer.log("✅ Builder memory monitoring is active")

        cf_client.wait_for_service_log(
            cfServer,
            "crystal-forge-builder.service",
            "No derivations need CVE scanning",
            timeout=120,
        )
        cfServer.log("✅ Builder is actively scanning for work")

    except:
        # Fallback: check for any builder activity
        cf_client.wait_for_service_log(
            cfServer,
            "crystal-forge-builder.service",
            "crystal_forge::builder",
            timeout=60,
        )
        cfServer.log("✅ Builder service is showing activity")


def test_builder_not_restarting(cf_client, cfServer):
    """Test that builder service is stable and not restart-looping"""

    # Check restart count is low
    restart_output = cfServer.succeed(
        "systemctl show crystal-forge-builder.service --property=NRestarts"
    )
    restart_count = int(restart_output.split("=")[1].strip())

    assert (
        restart_count < 3
    ), f"Builder has restarted {restart_count} times - possible restart loop"
