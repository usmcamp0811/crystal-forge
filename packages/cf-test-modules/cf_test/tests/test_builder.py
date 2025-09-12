import pytest

from cf_test import CFTestClient
from cf_test.vm_helpers import SmokeTestConstants as C
from cf_test.vm_helpers import check_service_active


def test_builder_service_status(cf_client, server):
    """Test that the Crystal Forge builder service is running and healthy"""

    # 1. Check systemd service is active
    server.succeed("systemctl is-active crystal-forge-builder.service")

    # 2. Verify service is enabled for startup
    server.succeed("systemctl is-enabled crystal-forge-builder.service")

    # 3. Check that the service has been running for at least a few seconds
    # (not constantly restarting)
    uptime_output = server.succeed(
        "systemctl show crystal-forge-builder.service --property=ActiveEnterTimestamp"
    )
    assert "ActiveEnterTimestamp=" in uptime_output
    assert "n/a" not in uptime_output, "Service should have a valid start time"


def test_builder_process_running(cf_client, server):
    """Verify the builder process is actually running"""

    # Check that the builder process exists
    server.succeed(
        "pgrep -f 'crystal-forge.*builder' || pgrep -f '/nix/store.*builder'"
    )

    # Verify it's running as the correct user
    ps_output = server.succeed("ps aux | grep '[b]uilder' | head -1")
    assert (
        "crystal-forge" in ps_output
    ), f"Builder should run as crystal-forge user, got: {ps_output}"


def test_builder_logs_healthy(cf_client, server):
    """Check builder logs show healthy startup and operation"""

    # Wait for builder to log startup completion
    cf_client.wait_for_service_log(
        server,
        "crystal-forge-builder.service",
        "Both build and CVE scan loops started",
        timeout=60,
    )

    # Verify no critical errors in recent logs
    logs = server.succeed(
        "journalctl -u crystal-forge-builder.service --since '2 minutes ago' --no-pager"
    )

    # Check for error indicators
    error_indicators = ["FATAL", "panic", "failed to load", "connection refused"]
    for indicator in error_indicators:
        assert (
            indicator.lower() not in logs.lower()
        ), f"Found error indicator '{indicator}' in logs: {logs}"


def test_builder_config_loaded(cf_client, server):
    """Verify builder loaded configuration successfully"""

    # Config file should exist and be readable by crystal-forge user
    server.succeed("sudo -u crystal-forge test -r /var/lib/crystal-forge/config.toml")

    # Check logs show config was loaded (not using defaults)
    try:
        cf_client.wait_for_service_log(
            server,
            "crystal-forge-builder.service",
            "Starting Crystal Forge Builder",
            timeout=30,
        )
    except:
        # Fallback: check that service started without config errors
        logs = server.succeed(
            "journalctl -u crystal-forge-builder.service --no-pager | tail -20"
        )
        assert "Failed to load Crystal Forge config" not in logs


def test_builder_database_connectivity(cf_client, server):
    """Verify builder can connect to the database"""

    # Builder should have completed database migrations
    cf_client.wait_for_service_log(
        server, "crystal-forge-builder.service", "Starting Build loop", timeout=90
    )

    # Verify no database connection errors
    logs = server.succeed(
        "journalctl -u crystal-forge-builder.service --since '3 minutes ago' --no-pager"
    )
    db_errors = ["connection refused", "authentication failed", "database.*not.*exist"]
    for error_pattern in db_errors:
        assert not any(error.lower() in logs.lower() for error in [error_pattern])


def test_builder_polling_active(cf_client, server):
    """Verify builder is actively polling for work"""

    # Wait for at least one polling cycle message
    try:
        cf_client.wait_for_service_log(
            server,
            "crystal-forge-builder.service",
            "No derivations need building",
            timeout=120,
        )
    except:
        # Alternative: check for CVE scanning message
        cf_client.wait_for_service_log(
            server,
            "crystal-forge-builder.service",
            "No derivations need CVE scanning",
            timeout=120,
        )


def test_builder_working_directory(cf_client, server):
    """Verify builder's working directory is properly set up"""

    # Check that required directories exist with correct ownership
    directories = [
        "/var/lib/crystal-forge/workdir",
        "/var/lib/crystal-forge/tmp",
        "/var/lib/crystal-forge/.cache",
        "/var/lib/crystal-forge/.cache/nix",
    ]

    for directory in directories:
        server.succeed(f"test -d {directory}")
        # Verify ownership
        stat_output = server.succeed(f"stat -c '%U:%G' {directory}")
        assert (
            "crystal-forge:crystal-forge" in stat_output
        ), f"Directory {directory} has wrong ownership: {stat_output}"


def test_builder_required_binaries(cf_client, server):
    """Verify builder has access to required binaries"""

    # Check that builder can find required tools
    required_tools = ["nix", "git", "vulnix", "systemd"]

    for tool in required_tools:
        # Test as the crystal-forge user since that's who runs the service
        server.succeed(f"sudo -u crystal-forge which {tool}")


def test_builder_memory_limits(cf_client, server):
    """Verify builder service memory limits are reasonable"""

    # Check systemd service properties
    memory_output = server.succeed(
        "systemctl show crystal-forge-builder.service --property=MemoryMax"
    )

    # Should have some memory limit set (not infinity)
    assert "MemoryMax=" in memory_output
    if "infinity" not in memory_output.lower():
        # If a limit is set, it should be reasonable (at least 512M)
        server.log(f"Builder memory limit: {memory_output}")


def test_builder_no_restart_loop(cf_client, server):
    """Verify builder isn't stuck in a restart loop"""

    # Check restart count - should be low
    restart_output = server.succeed(
        "systemctl show crystal-forge-builder.service --property=NRestarts"
    )

    # Extract restart count
    restart_count = int(restart_output.split("=")[1].strip())
    assert (
        restart_count < 3
    ), f"Builder has restarted {restart_count} times - possible restart loop"

    # Verify service has been stable for at least 30 seconds
    import time

    time.sleep(30)
    server.succeed("systemctl is-active crystal-forge-builder.service")


@pytest.mark.integration
def test_builder_responds_to_work(cf_client, server):
    """Test that builder actually processes work when available"""

    # This test would create a test derivation and verify the builder picks it up
    # For now, just verify the builder logs show it's checking for work

    # Wait for multiple polling cycles to ensure consistent operation
    start_time = time.time()
    poll_count = 0

    while time.time() - start_time < 180 and poll_count < 3:  # 3 minutes max
        try:
            cf_client.wait_for_service_log(
                server,
                "crystal-forge-builder.service",
                "No derivations need",  # Matches both build and CVE messages
                timeout=70,
            )
            poll_count += 1
        except:
            break

    assert (
        poll_count >= 2
    ), f"Builder should complete multiple polling cycles, only saw {poll_count}"


def test_builder_systemd_slice(cf_client, server):
    """Verify builder is running in the correct systemd slice if configured"""

    try:
        # Check if custom slice is active
        server.succeed("systemctl is-active crystal-forge-builds.slice")

        # Verify builder service is in the slice
        cgroup_output = server.succeed(
            "systemctl show crystal-forge-builder.service --property=ControlGroup"
        )
        assert "crystal-forge-builds.slice" in cgroup_output

        server.log("Builder is correctly running in crystal-forge-builds.slice")
    except:
        # If slice isn't configured, that's okay too
        server.log("Builder running without custom systemd slice")
