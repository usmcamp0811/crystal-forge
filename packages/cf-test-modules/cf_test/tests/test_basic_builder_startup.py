import pytest

from cf_test import CFTestClient

pytestmark = [pytest.mark.builder, pytest.mark.s3cache, pytest.mark.integration]


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

    # Check each tool exists in the service PATH by testing if the binary file exists
    tools = ["nix", "git", "vulnix"]
    for tool in tools:
        # Check each PATH directory for the tool
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

    # Wait for polling message
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
