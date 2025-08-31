import json

import pytest

from cf_test import CFTestClient
from cf_test.vm_helpers import SmokeTestConstants as C
from cf_test.vm_helpers import (
    SmokeTestData,
    check_keys_exist,
    check_timer_active,
    get_system_hash,
    run_service_and_verify_success,
    verify_commits_exist,
    verify_db_state,
    verify_flake_in_db,
    wait_for_agent_acceptance,
)

pytestmark = pytest.mark.vm_only


@pytest.fixture(scope="session")
def server():
    import cf_test

    return cf_test._driver_machines["server"]


@pytest.fixture(scope="session")
def agent():
    import cf_test

    return cf_test._driver_machines["agent"]


@pytest.fixture(scope="session")
def cf_client(cf_config):
    return CFTestClient(cf_config)


@pytest.fixture(scope="session")
def smoke_data():
    return SmokeTestData()


@pytest.mark.slow  # Use existing marker instead of timeout
def test_boot_and_units(server, agent):
    """Test that all services boot and reach expected states"""
    server.succeed(f"systemctl status {C.SERVER_SERVICE} || true")
    server.log(f"=== {C.SERVER_SERVICE} service logs ===")
    server.succeed(f"journalctl -u {C.SERVER_SERVICE} --no-pager || true")

    server.wait_for_unit(C.POSTGRES_SERVICE)
    server.wait_for_unit(C.SERVER_SERVICE)
    agent.wait_for_unit(C.AGENT_SERVICE)
    server.wait_for_unit("multi-user.target")


def test_keys_and_network(server, agent):
    """Test that SSH keys are present and network connectivity works"""
    # Verify keys exist
    check_keys_exist(agent, C.AGENT_KEY_PATH, C.AGENT_PUB_PATH)
    check_keys_exist(server, C.SERVER_PUB_PATH)

    # Verify network connectivity
    agent.succeed("ping -c1 server")


@pytest.mark.slow
def test_agent_accept_and_db_state(cf_client, server, agent):
    """Test that agent is accepted and database state is correct"""
    agent_hostname = agent.succeed("hostname -s").strip()

    # Wait for agent acceptance first
    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Now get the system hash after the database fix has run
    system_hash = get_system_hash(agent)
    change_reason = "startup"

    # Log agent status for debugging
    agent.log("=== agent logs ===")
    agent.log(agent.succeed(f"journalctl -u {C.AGENT_SERVICE} || true"))

    # Verify database state
    verify_db_state(cf_client, server, agent_hostname, system_hash, change_reason)


@pytest.mark.slow
def test_webhook_and_commit_ingest(cf_client, server, smoke_data):
    """Test webhook processing and commit ingestion"""
    # Send webhook
    cf_client.send_webhook(server, C.API_PORT, smoke_data.webhook_payload)

    # Wait for webhook processing
    cf_client.wait_for_service_log(
        server, C.SERVER_SERVICE, smoke_data.webhook_commit, timeout=90
    )

    # Verify flake was created
    verify_flake_in_db(cf_client, server, smoke_data.git_server_url)

    # Verify commits were ingested
    verify_commits_exist(cf_client, server)


@pytest.mark.slow
def test_postgres_jobs_timer_and_idempotency(cf_client, server, agent):
    """Test postgres jobs timer and service idempotency"""
    # Verify agent doesn't run postgres (security check)
    active_services = agent.succeed(
        "systemctl list-units --type=service --state=active"
    )
    assert "postgresql" not in active_services

    # Verify timer is active
    check_timer_active(server, C.JOBS_TIMER)

    # Test service runs successfully
    run_service_and_verify_success(
        cf_client, server, C.JOBS_SERVICE, "All jobs completed successfully"
    )

    # Test idempotency - second run should also succeed
    run_service_and_verify_success(
        cf_client, server, C.JOBS_SERVICE, "All jobs completed successfully"
    )
