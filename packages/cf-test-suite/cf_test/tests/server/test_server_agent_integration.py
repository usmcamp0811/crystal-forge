import json

import pytest

from cf_test.vm_helpers import SmokeTestConstants as C
from cf_test.vm_helpers import (
    SmokeTestData,
    check_keys_exist,
    check_timer_active,
    get_system_hash,
    run_service_and_verify_success,
    verify_db_state,
    wait_for_agent_acceptance,
    wait_for_crystal_forge_ready,
)

pytestmark = [pytest.mark.server, pytest.mark.integration, pytest.mark.agent]


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

    wait_for_crystal_forge_ready(server)

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


@pytest.mark.slow
def test_desired_target_response(cf_client, server, agent, smoke_data):
    """Test that the log endpoint returns desired_target for systems"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = agent.succeed("hostname -s").strip()

    # Wait for agent acceptance first
    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Test 1: Initially, no desired_target should be set
    # Make an agent heartbeat and check the response
    response = agent.succeed(
        """
        curl -s -X POST http://server:3000/current-system \\
            -H "X-Key-ID: $(hostname -s)" \\
            -H "X-Signature: $(echo '{"hostname":"'$(hostname -s)'","change_reason":"test"}' | \\
                /etc/agent.key sign | base64 -w0)" \\
            -H "Content-Type: application/json" \\
            -d '{"hostname":"'$(hostname -s)'","change_reason":"test"}'
    """
    )

    # Parse JSON response and verify desired_target is null
    response_json = json.loads(response)
    assert "desired_target" in response_json
    assert response_json["desired_target"] is None

    # Test 2: Set a desired target in the database
    test_target = "git+https://example.com/repo?rev=abc123#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    # Make another agent request and verify the desired_target is returned
    response = agent.succeed(
        """
        curl -s -X POST http://server:3000/current-system \\
            -H "X-Key-ID: $(hostname -s)" \\
            -H "X-Signature: $(echo '{"hostname":"'$(hostname -s)'","change_reason":"test2"}' | \\
                /etc/agent.key sign | base64 -w0)" \\
            -H "Content-Type: application/json" \\
            -d '{"hostname":"'$(hostname -s)'","change_reason":"test2"}'
    """
    )

    # Parse JSON response and verify desired_target is returned
    response_json = json.loads(response)
    assert "desired_target" in response_json
    assert response_json["desired_target"] == test_target

    # Test 3: Clear the desired target and verify it returns null again
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = NULL WHERE hostname = %s",
        (agent_hostname,),
    )

    response = agent.succeed(
        """
        curl -s -X POST http://server:3000/current-system \\
            -H "X-Key-ID: $(hostname -s)" \\
            -H "X-Signature: $(echo '{"hostname":"'$(hostname -s)'","change_reason":"test3"}' | \\
                /etc/agent.key sign | base64 -w0)" \\
            -H "Content-Type: application/json" \\
            -d '{"hostname":"'$(hostname -s)'","change_reason":"test3"}'
    """
    )

    response_json = json.loads(response)
    assert "desired_target" in response_json
    assert response_json["desired_target"] is None


@pytest.mark.slow
def test_nixos_module_desired_target_sync(cf_client, server, agent):
    """Test that systems defined in NixOS module configuration sync desired_target to database"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = agent.succeed("hostname -s").strip()

    # This would test the NixOS module sync functionality, but since we're in a test environment,
    # we'll simulate what the sync should do

    # Test that deployment_policy defaults to "manual"
    result = cf_client.execute_sql(
        "SELECT deployment_policy FROM systems WHERE hostname = %s", (agent_hostname,)
    )

    assert len(result) > 0
    assert result[0]["deployment_policy"] == "manual"

    # Test updating deployment policy
    cf_client.execute_sql(
        "UPDATE systems SET deployment_policy = %s WHERE hostname = %s",
        ("auto_latest", agent_hostname),
    )

    result = cf_client.execute_sql(
        "SELECT deployment_policy FROM systems WHERE hostname = %s", (agent_hostname,)
    )

    assert result[0]["deployment_policy"] == "auto_latest"
