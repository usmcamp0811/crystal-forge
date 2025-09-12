import pytest

from cf_test import CFTestClient

pytestmark = [pytest.mark.s3cache, pytest.mark.integration]


@pytest.fixture(scope="session")
def s3_server():
    import cf_test

    return cf_test._driver_machines["s3Server"]


@pytest.fixture(scope="session")
def cf_client(cf_config):
    return CFTestClient(cf_config)


def test_builder_service_exists_and_runs(cf_client, s3_server):
    """Test that builder service exists and is running"""

    # Check service file exists
    s3_server.succeed("test -f /etc/systemd/system/crystal-forge-builder.service")

    # Check service status and get details
    status = s3_server.succeed("systemctl status crystal-forge-builder.service || true")
    s3_server.log(f"Builder service status: {status}")

    # Check service is active
    s3_server.succeed("systemctl is-active crystal-forge-builder.service")

    # Check if process exists - if not, get detailed logs
    try:
        s3_server.succeed("pgrep -f crystal-forge.*builder")
    except:
        s3_server.log("Builder process not found - checking logs...")
        logs = s3_server.succeed(
            "journalctl -u crystal-forge-builder.service --no-pager -n 20"
        )
        s3_server.log(f"Recent builder logs: {logs}")

        # Try to see what processes ARE running
        processes = s3_server.succeed("ps aux | grep crystal-forge")
        s3_server.log(f"Crystal Forge processes: {processes}")

        # Check if binary exists
        binary_check = s3_server.succeed(
            "ls -la /nix/store/*/bin/*builder* || echo 'no builder binary'"
        )
        s3_server.log(f"Builder binary check: {binary_check}")

        raise Exception("Builder process not running despite active service")


def test_builder_has_required_tools(cf_client, s3_server):
    """Test that builder can access required binaries"""

    tools = ["nix", "git", "vulnix"]
    for tool in tools:
        s3_server.succeed(f"sudo -u crystal-forge which {tool}")


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

    # Wait for polling message
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        "No derivations need building",
        timeout=120,
    )


def test_builder_database_connection(cf_client, s3_server):
    """Test that builder can connect to database"""

    # Test database connection as crystal-forge user
    s3_server.succeed("sudo -u crystal-forge psql -d crystal_forge -c 'SELECT 1;'")

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
