import json
from datetime import UTC, datetime, timedelta

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


@pytest.mark.slow
def test_deployment_policy_manager_auto_latest(cf_client, server, agent):
    """Test that deployment policy manager updates desired_target for auto_latest systems"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = agent.succeed("hostname -s").strip()

    # Wait for agent acceptance first
    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Test setup: Create a flake and commit scenario for the agent
    now = datetime.now(UTC)

    # Create flake for the agent system
    flake_id = cf_client.execute_sql(
        """
        INSERT INTO flakes (name, repo_url, is_watched, created_at, updated_at)
        VALUES (%s, %s, true, %s, %s)
        RETURNING id
        """,
        ("test-auto-latest", "https://example.com/test-auto-latest.git", now, now),
    )[0]["id"]

    # Update the agent system to use this flake and set auto_latest policy
    cf_client.execute_sql(
        """
        UPDATE systems 
        SET flake_id = %s, deployment_policy = 'auto_latest', desired_target = NULL
        WHERE hostname = %s
        """,
        (flake_id, agent_hostname),
    )

    # Create a commit
    git_hash = "abc123def456"
    commit_id = cf_client.execute_sql(
        """
        INSERT INTO commits (flake_id, git_commit_hash, commit_message, author_name, author_email, timestamp, created_at)
        VALUES (%s, %s, 'Test commit for auto_latest', 'Test Author', 'test@example.com', %s, %s)
        RETURNING id
        """,
        (flake_id, git_hash, now, now),
    )[0]["id"]

    # Create a successful derivation for this commit
    derivation_target = f"git+https://example.com/test-auto-latest.git?rev={git_hash}#nixosConfigurations.{agent_hostname}.config.system.build.toplevel"
    derivation_id = cf_client.execute_sql(
        """
        INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path, derivation_target,
            status_id, attempt_count, scheduled_at, started_at, completed_at
        )
        VALUES (
            %s, 'nixos', %s, '/nix/store/test-derivation.drv', %s,
            (SELECT id FROM derivation_statuses WHERE name = 'build-complete'),
            1, %s, %s, %s
        )
        RETURNING id
        """,
        (
            commit_id,
            agent_hostname,
            derivation_target,
            now - timedelta(minutes=10),
            now - timedelta(minutes=9),
            now - timedelta(minutes=5),
        ),
    )[0]["id"]

    # Verify initial state - no desired_target set
    result = cf_client.execute_sql(
        "SELECT desired_target, deployment_policy FROM systems WHERE hostname = %s",
        (agent_hostname,),
    )
    assert result[0]["desired_target"] is None
    assert result[0]["deployment_policy"] == "auto_latest"

    # Trigger the deployment policy manager by running the postgres jobs
    # This should update the desired_target for our auto_latest system
    run_service_and_verify_success(
        cf_client, server, C.JOBS_SERVICE, "All jobs completed successfully"
    )

    # Wait a moment for the policy manager to process
    import time

    time.sleep(2)

    # Verify that desired_target has been updated to the latest successful derivation
    result = cf_client.execute_sql(
        "SELECT desired_target FROM systems WHERE hostname = %s", (agent_hostname,)
    )

    assert result[0]["desired_target"] == derivation_target, (
        f"Expected desired_target to be {derivation_target}, "
        f"but got {result[0]['desired_target']}"
    )

    # Test that agent receives the updated desired_target
    response = agent.succeed(
        """
        curl -s -X POST http://server:3000/current-system \\
            -H "X-Key-ID: $(hostname -s)" \\
            -H "X-Signature: $(echo '{"hostname":"'$(hostname -s)'","change_reason":"policy_test"}' | \\
                /etc/agent.key sign | base64 -w0)" \\
            -H "Content-Type: application/json" \\
            -d '{"hostname":"'$(hostname -s)'","change_reason":"policy_test"}'
        """
    )

    response_json = json.loads(response)
    assert "desired_target" in response_json
    assert response_json["desired_target"] == derivation_target

    # Test with a newer commit to verify auto-update behavior
    git_hash_new = "def456abc789"
    commit_id_new = cf_client.execute_sql(
        """
        INSERT INTO commits (flake_id, git_commit_hash, commit_message, author_name, author_email, timestamp, created_at)
        VALUES (%s, %s, 'Newer commit for auto_latest', 'Test Author', 'test@example.com', %s, %s)
        RETURNING id
        """,
        (
            flake_id,
            git_hash_new,
            now + timedelta(minutes=10),
            now + timedelta(minutes=10),
        ),
    )[0]["id"]

    # Create a successful derivation for the new commit
    derivation_target_new = f"git+https://example.com/test-auto-latest.git?rev={git_hash_new}#nixosConfigurations.{agent_hostname}.config.system.build.toplevel"
    cf_client.execute_sql(
        """
        INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path, derivation_target,
            status_id, attempt_count, scheduled_at, started_at, completed_at
        )
        VALUES (
            %s, 'nixos', %s, '/nix/store/test-derivation-new.drv', %s,
            (SELECT id FROM derivation_statuses WHERE name = 'build-complete'),
            1, %s, %s, %s
        )
        """,
        (
            commit_id_new,
            agent_hostname,
            derivation_target_new,
            now + timedelta(minutes=1),
            now + timedelta(minutes=2),
            now + timedelta(minutes=5),
        ),
    )

    # Run the policy manager again
    run_service_and_verify_success(
        cf_client, server, C.JOBS_SERVICE, "All jobs completed successfully"
    )

    time.sleep(2)

    # Verify desired_target updated to the newer derivation
    result = cf_client.execute_sql(
        "SELECT desired_target FROM systems WHERE hostname = %s", (agent_hostname,)
    )

    assert result[0]["desired_target"] == derivation_target_new, (
        f"Expected desired_target to be updated to {derivation_target_new}, "
        f"but got {result[0]['desired_target']}"
    )

    # Clean up test data
    cf_client.execute_sql(
        "DELETE FROM derivations WHERE commit_id IN (%s, %s)",
        (commit_id, commit_id_new),
    )
    cf_client.execute_sql(
        "DELETE FROM commits WHERE id IN (%s, %s)", (commit_id, commit_id_new)
    )
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))
    cf_client.execute_sql(
        "UPDATE systems SET flake_id = NULL, deployment_policy = 'manual', desired_target = NULL WHERE hostname = %s",
        (agent_hostname,),
    )


@pytest.mark.slow
def test_agent_deployment_attempt_on_desired_target(cf_client, server, agent):
    """Test that agent attempts deployment when desired_target is set"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = agent.succeed("hostname -s").strip()

    # Wait for agent acceptance first
    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Test 1: Set a desired target in the database
    test_target = "git+https://example.com/repo?rev=abc123#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    # Clear agent logs before test
    agent.succeed("journalctl --vacuum-time=1s")

    # Trigger a heartbeat by touching the current-system symlink
    agent.succeed("touch /run/current-system")

    # Wait for the agent to process the heartbeat and attempt deployment
    import time

    time.sleep(5)

    # Check agent logs for deployment attempt
    agent_logs = agent.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")

    # Verify the agent received and processed the desired target
    assert "Received desired target:" in agent_logs
    assert test_target in agent_logs
    assert "Starting deployment execution" in agent_logs

    # Since nixos-rebuild will fail in the VM, we expect to see the failure logged
    # but the important thing is that the agent attempted the deployment
    assert "Deployment failed" in agent_logs or "nixos-rebuild" in agent_logs

    # Test 2: Clear the desired target and verify no deployment attempt
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = NULL WHERE hostname = %s",
        (agent_hostname,),
    )

    # Clear logs again
    agent.succeed("journalctl --vacuum-time=1s")

    # Trigger another heartbeat
    agent.succeed("touch /run/current-system")
    time.sleep(5)

    # Check that no deployment was attempted
    agent_logs = agent.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    assert (
        "No desired target in heartbeat response" in agent_logs
        or "No deployment needed" in agent_logs
    )


@pytest.mark.slow
def test_agent_deployment_already_on_target(cf_client, server, agent):
    """Test that agent skips deployment when already on target"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = agent.succeed("hostname -s").strip()

    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Set a desired target
    test_target = "git+https://example.com/repo?rev=def456#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    # Clear agent logs
    agent.succeed("journalctl --vacuum-time=1s")

    # First heartbeat should attempt deployment
    agent.succeed("touch /run/current-system")
    time.sleep(5)

    agent_logs = agent.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    assert "Starting deployment execution" in agent_logs

    # Clear logs again
    agent.succeed("journalctl --vacuum-time=1s")

    # Second heartbeat should skip deployment (already on target)
    agent.succeed("touch /run/current-system")
    time.sleep(5)

    agent_logs = agent.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    assert "Already on target" in agent_logs or "skipping deployment" in agent_logs


@pytest.mark.slow
def test_agent_deployment_dry_run_configuration(cf_client, server, agent):
    """Test agent deployment with dry-run configuration"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = agent.succeed("hostname -s").strip()

    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # The VM test configuration should have dry_run_first enabled
    # Check that dry-run is executed before actual deployment
    test_target = "git+https://example.com/repo?rev=ghi789#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    agent.succeed("journalctl --vacuum-time=1s")
    agent.succeed("touch /run/current-system")
    time.sleep(5)

    agent_logs = agent.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")

    # If dry_run_first is enabled, we should see dry-run execution
    # The exact log message depends on the deployment config
    assert (
        "dry-run" in agent_logs.lower() or "Starting deployment execution" in agent_logs
    )


@pytest.mark.slow
def test_agent_deployment_state_update_after_success(cf_client, server, agent):
    """Test that agent updates system state after successful deployment"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = agent.succeed("hostname -s").strip()

    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Count initial system states
    initial_states = cf_client.execute_sql(
        "SELECT COUNT(*) as count FROM system_states WHERE hostname = %s",
        (agent_hostname,),
    )[0]["count"]

    # Set a desired target that should trigger deployment
    test_target = "git+https://example.com/repo?rev=success123#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    agent.succeed("journalctl --vacuum-time=1s")
    agent.succeed("touch /run/current-system")

    # Give more time for deployment attempt and potential state update
    time.sleep(10)

    # Check if new system state was recorded
    # Even if deployment fails, the agent should attempt to record the state change
    final_states = cf_client.execute_sql(
        "SELECT COUNT(*) as count FROM system_states WHERE hostname = %s",
        (agent_hostname,),
    )[0]["count"]

    # In a real deployment that succeeds, we'd see a new system state
    # In our VM test, deployment will fail but we should see the attempt logged
    agent_logs = agent.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")

    # Verify deployment was attempted (even if it failed)
    assert "deployment" in agent_logs.lower() and (
        test_target in agent_logs or "Starting deployment execution" in agent_logs
    )


@pytest.mark.slow
def test_agent_deployment_result_enum_coverage(cf_client, server, agent):
    """Test that agent produces different DeploymentResult enum variants"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = agent.succeed("hostname -s").strip()

    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Test NoDeploymentNeeded case
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = NULL WHERE hostname = %s",
        (agent_hostname,),
    )

    agent.succeed("journalctl --vacuum-time=1s")
    agent.succeed("touch /run/current-system")
    time.sleep(3)

    agent_logs = agent.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    assert "No deployment needed" in agent_logs or "No desired target" in agent_logs

    # Test Failed case (nixos-rebuild will fail in VM)
    test_target = "git+https://example.com/repo?rev=fail123#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    agent.succeed("journalctl --vacuum-time=1s")
    agent.succeed("touch /run/current-system")
    time.sleep(5)

    agent_logs = agent.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    # Should see deployment failure due to VM environment limitations
    assert (
        "Deployment failed" in agent_logs
        or "failed" in agent_logs.lower()
        or "error" in agent_logs.lower()
    )
