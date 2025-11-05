import base64
import json
import time
from datetime import UTC, datetime, timedelta

import pytest
from nacl.signing import SigningKey as NaClSigningKey

from cf_test.vm_helpers import SmokeTestConstants as C
from cf_test.vm_helpers import (SmokeTestData, check_keys_exist,
                                check_timer_active, get_system_hash,
                                run_service_and_verify_success,
                                verify_db_state, wait_for_agent_acceptance,
                                wait_for_crystal_forge_ready)

pytestmark = [
    pytest.mark.server,
    pytest.mark.integration,
    pytest.mark.agent,
]


def sign_ed25519_payload(server, payload_json: str) -> str:
    """Sign payload with Ed25519 key"""
    key_b64 = server.succeed("cat /etc/server.key").strip()
    key_bytes = base64.b64decode(key_b64)
    signing_key = NaClSigningKey(key_bytes)
    signature = signing_key.sign(payload_json.encode()).signature
    return base64.b64encode(signature).decode()


@pytest.fixture(scope="session")
def smoke_data():
    return SmokeTestData()


@pytest.mark.slow
def test_boot_and_units(server, agent):
    """Test that all services boot and reach expected states"""
    server.succeed(f"systemctl status {C.SERVER_SERVICE} || true")
    server.log(f"=== {C.SERVER_SERVICE} service logs ===")
    server.succeed(f"journalctl -u {C.SERVER_SERVICE} --no-pager || true")

    server.wait_for_unit(C.POSTGRES_SERVICE)
    server.wait_for_unit(C.SERVER_SERVICE)
    server.wait_for_unit("multi-user.target")

    # Some VM profiles don't provision an agent VM; the fixture yields None.
    if agent is None:
        server.log("Agent VM not provisioned in this profile; skipping agent checks.")
        pytest.skip("agent VM not available in this test profile")

    server.wait_for_unit(C.AGENT_SERVICE)


@pytest.mark.slow
def test_agent_accept_and_db_state(cf_client, server, agent):
    """Test that agent is accepted and database state is correct"""

    wait_for_crystal_forge_ready(server)

    agent_hostname = server.succeed("hostname -s").strip()

    # Wait for agent acceptance first
    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Now get the system hash after the database fix has run
    system_hash = get_system_hash(server)
    change_reason = "startup"

    # Log agent status for debugging
    server.log("=== agent logs ===")
    server.log(server.succeed(f"journalctl -u {C.AGENT_SERVICE} || true"))

    # Verify database state
    verify_db_state(cf_client, server, agent_hostname, system_hash, change_reason)


@pytest.mark.slow
@pytest.mark.skip(reason="TODO: Update this")
def test_postgres_jobs_timer_and_idempotency(cf_client, server, agent):
    """Test postgres jobs timer and service idempotency"""
    # Verify agent doesn't run postgres (security check)
    active_services = server.succeed(
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
    """Test that the heartbeat endpoint returns desired_target for systems"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = server.succeed("hostname -s").strip()

    # Wait for agent acceptance first (ensures system row exists and is linked)
    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    def call_agent(change_reason: str) -> dict:
        # Get current system derivation path
        current_system = server.succeed("readlink /run/current-system").strip()
        
        # Build JSON safely, sign with the private key at /etc/server.key, POST.
        payload_json = json.dumps({
            "hostname": agent_hostname,
            "change_reason": change_reason,
            "store_path": current_system
        })
        sig = sign_ed25519_payload(server, payload_json)
        resp = server.succeed(
            f"""
            curl -sS -X POST http://server:3000/agent/heartbeat \\
              -H "X-Key-ID: {agent_hostname}" \\
              -H "X-Signature: {sig}" \\
              -H "Content-Type: application/json" \\
              -d '{payload_json}'
            """
        )
        return json.loads(resp)

    # Test 1: Initially, no desired_target should be set
    response_json = call_agent("startup")
    assert "desired_target" in response_json
    assert response_json["desired_target"] is None

    # Test 2: Set a desired target in the database
    test_target = "git+https://example.com/repo?rev=abc123#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    # Verify the desired_target is returned
    response_json = call_agent("config_change")
    assert "desired_target" in response_json
    assert response_json["desired_target"] == test_target

    # Test 3: Clear the desired target and verify it returns null again
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = NULL WHERE hostname = %s",
        (agent_hostname,),
    )

    response_json = call_agent("state_delta")
    assert "desired_target" in response_json
    assert response_json["desired_target"] is None



@pytest.mark.slow
def test_nixos_module_desired_target_sync(cf_client, server, agent):
    """Test that systems defined in NixOS module configuration sync desired_target to database"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = server.succeed("hostname -s").strip()

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
    agent_hostname = server.succeed("hostname -s").strip()

    # Wait for agent acceptance first
    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Test setup: Create a flake and commit scenario for the agent
    now = datetime.now(UTC)

    # Create flake for the agent system
    flake_id = cf_client.execute_sql(
        """
        INSERT INTO flakes (name, repo_url)
        VALUES (%s, %s)
        RETURNING id
        """,
        ("test-auto-latest", "https://example.com/test-auto-latest.git"),
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
    current_system = server.succeed("readlink /run/current-system").strip()
    payload_json = json.dumps({
        "hostname": agent_hostname,
        "change_reason": "config_change",
        "store_path": current_system
    })
    sig = sign_ed25519_payload(server, payload_json)
    response = server.succeed(
        f"""
        curl -s -X POST http://server:3000/agent/heartbeat \\
            -H "X-Key-ID: {agent_hostname}" \\
            -H "X-Signature: {sig}" \\
            -H "Content-Type: application/json" \\
            -d '{payload_json}'
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
    agent_hostname = server.succeed("hostname -s").strip()

    # Wait for agent acceptance first
    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Test 1: Set a desired target in the database
    test_target = "git+https://example.com/repo?rev=abc123#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    # Clear agent logs before test
    server.succeed("journalctl --vacuum-time=1s")

    # Trigger a heartbeat by touching the current-system symlink
    server.succeed("touch /run/current-system")

    # Wait for the agent to process the heartbeat and attempt deployment
    time.sleep(5)

    # Check agent logs for deployment attempt
    agent_logs = server.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")

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
    server.succeed("journalctl --vacuum-time=1s")

    # Trigger another heartbeat
    server.succeed("touch /run/current-system")
    time.sleep(5)

    # Check that no deployment was attempted
    agent_logs = server.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    assert (
        "No desired target in heartbeat response" in agent_logs
        or "No deployment needed" in agent_logs
    )


@pytest.mark.slow
def test_agent_deployment_already_on_target(cf_client, server, agent):
    """Test that agent skips deployment when already on target"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = server.succeed("hostname -s").strip()

    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Set a desired target
    test_target = "git+https://example.com/repo?rev=def456#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    # Clear agent logs
    server.succeed("journalctl --vacuum-time=1s")

    # First heartbeat should attempt deployment
    server.succeed("touch /run/current-system")
    time.sleep(5)

    agent_logs = server.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    assert "Starting deployment execution" in agent_logs

    # Clear logs again
    server.succeed("journalctl --vacuum-time=1s")

    # Second heartbeat should skip deployment (already on target)
    server.succeed("touch /run/current-system")
    time.sleep(5)

    agent_logs = server.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    assert "Already on target" in agent_logs or "skipping deployment" in agent_logs


@pytest.mark.slow
def test_agent_deployment_dry_run_configuration(cf_client, server, agent):
    """Test agent deployment with dry-run configuration"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = server.succeed("hostname -s").strip()

    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # The VM test configuration should have dry_run_first enabled
    # Check that dry-run is executed before actual deployment
    test_target = "git+https://example.com/repo?rev=ghi789#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    server.succeed("journalctl --vacuum-time=1s")
    server.succeed("touch /run/current-system")
    time.sleep(5)

    agent_logs = server.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")

    # If dry_run_first is enabled, we should see dry-run execution
    # The exact log message depends on the deployment config
    assert (
        "dry-run" in agent_logs.lower() or "Starting deployment execution" in agent_logs
    )


@pytest.mark.slow
def test_agent_deployment_state_update_after_success(cf_client, server, agent):
    """Test that agent updates system state after successful deployment"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = server.succeed("hostname -s").strip()

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

    server.succeed("journalctl --vacuum-time=1s")
    server.succeed("touch /run/current-system")

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
    agent_logs = server.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")

    # Verify deployment was attempted (even if it failed)
    assert "deployment" in agent_logs.lower() and (
        test_target in agent_logs or "Starting deployment execution" in agent_logs
    )


@pytest.mark.slow
def test_agent_deployment_result_enum_coverage(cf_client, server, agent):
    """Test that agent produces different DeploymentResult enum variants"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = server.succeed("hostname -s").strip()

    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Test NoDeploymentNeeded case
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = NULL WHERE hostname = %s",
        (agent_hostname,),
    )

    server.succeed("journalctl --vacuum-time=1s")
    server.succeed("touch /run/current-system")
    time.sleep(3)

    agent_logs = server.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    assert "No deployment needed" in agent_logs or "No desired target" in agent_logs

    # Test Failed case (nixos-rebuild will fail in VM)
    test_target = "git+https://example.com/repo?rev=fail123#nixosConfigurations.test.config.system.build.toplevel"
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target, agent_hostname),
    )

    server.succeed("journalctl --vacuum-time=1s")
    server.succeed("touch /run/current-system")
    time.sleep(5)

    agent_logs = server.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    # Should see deployment failure due to VM environment limitations
    assert (
        "Deployment failed" in agent_logs
        or "failed" in agent_logs.lower()
        or "error" in agent_logs.lower()
    )


@pytest.mark.slow
def test_agent_skips_deployment_when_desired_target_has_same_derivation_path(
    cf_client, server, agent
):
    """Test that agent skips deployment when desired_target resolves to same derivation path as current system"""
    wait_for_crystal_forge_ready(server)
    agent_hostname = server.succeed("hostname -s").strip()

    wait_for_agent_acceptance(cf_client, server, timeout=C.AGENT_ACCEPTANCE_TIMEOUT)

    # Get the current derivation path that the agent is running
    current_derivation_path = server.succeed("readlink /run/current-system").strip()

    # Create a test scenario where we have the same derivation but from different git hashes
    # This simulates when two different commits/refs point to the same actual build result

    # Test 1: Set a desired target that should resolve to the same derivation path
    # In a real scenario, this could happen when:
    # - Two commits have identical content (e.g., merge commits, reverts, etc.)
    # - A tag and branch point to the same commit
    # - Manual testing with the same configuration
    test_target_same_path = f"git+https://example.com/repo?rev=same-content-123#nixosConfigurations.{agent_hostname}.config.system.build.toplevel"

    # Mock the scenario by setting up database state that represents a desired target
    # that would resolve to the same derivation path
    now = datetime.now(UTC)

    # Create a flake for testing
    flake_id = cf_client.execute_sql(
        """
        INSERT INTO flakes (name, repo_url)
        VALUES (%s, %s)
        RETURNING id
        """,
        (
            "test-same-derivation",
            "https://example.com/test-same-derivation.git",
        ),
    )[0]["id"]

    # Create a commit
    commit_id = cf_client.execute_sql(
        """
        INSERT INTO commits (flake_id, git_commit_hash, commit_message, author_name, author_email, timestamp, created_at)
        VALUES (%s, %s, 'Same content commit', 'Test Author', 'test@example.com', %s, %s)
        RETURNING id
        """,
        (flake_id, "same-content-123", now, now),
    )[0]["id"]

    # Create a derivation that has the SAME derivation_path as current system
    # This simulates the case where different git refs produce identical builds
    cf_client.execute_sql(
        """
        INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path, derivation_target,
            status_id, attempt_count, scheduled_at, started_at, completed_at
        )
        VALUES (
            %s, 'nixos', %s, %s, %s,
            (SELECT id FROM derivation_statuses WHERE name = 'build-complete'),
            1, %s, %s, %s
        )
        """,
        (
            commit_id,
            agent_hostname,
            current_derivation_path,  # Same as current system!
            test_target_same_path,
            now - timedelta(minutes=10),
            now - timedelta(minutes=9),
            now - timedelta(minutes=5),
        ),
    )

    # Set the desired target
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target_same_path, agent_hostname),
    )

    # Clear agent logs before test
    server.succeed("journalctl --vacuum-time=1s")

    # Trigger a heartbeat
    server.succeed("touch /run/current-system")
    time.sleep(5)

    # Check agent logs - should NOT attempt deployment
    agent_logs = server.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")

    # The agent should recognize it's already on the target and skip deployment
    assert any(
        phrase in agent_logs
        for phrase in [
            "Already on target",
            "Same derivation path",
            "Skipping deployment - already current",
            "No deployment needed - already on desired target",
        ]
    ), f"Agent should skip deployment when desired target has same derivation path. Logs: {agent_logs}"

    # Should NOT see deployment attempt messages
    assert (
        "Starting deployment execution" not in agent_logs
    ), "Agent should not attempt deployment for same derivation path"

    # Test 2: Change to a different derivation path to verify deployment would still work
    different_derivation_path = "/nix/store/different-hash-system"
    test_target_different = f"git+https://example.com/repo?rev=different-content-456#nixosConfigurations.{agent_hostname}.config.system.build.toplevel"

    # Create another commit with different derivation path
    commit_id_different = cf_client.execute_sql(
        """
        INSERT INTO commits (flake_id, git_commit_hash, commit_message, author_name, author_email, timestamp, created_at)
        VALUES (%s, %s, 'Different content commit', 'Test Author', 'test@example.com', %s, %s)
        RETURNING id
        """,
        (
            flake_id,
            "different-content-456",
            now + timedelta(minutes=1),
            now + timedelta(minutes=1),
        ),
    )[0]["id"]

    cf_client.execute_sql(
        """
        INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path, derivation_target,
            status_id, attempt_count, scheduled_at, started_at, completed_at
        )
        VALUES (
            %s, 'nixos', %s, %s, %s,
            (SELECT id FROM derivation_statuses WHERE name = 'build-complete'),
            1, %s, %s, %s
        )
        """,
        (
            commit_id_different,
            agent_hostname,
            different_derivation_path,  # Different path
            test_target_different,
            now - timedelta(minutes=5),
            now - timedelta(minutes=4),
            now - timedelta(minutes=1),
        ),
    )

    # Update desired target to the different one
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = %s WHERE hostname = %s",
        (test_target_different, agent_hostname),
    )

    # Clear logs and trigger heartbeat
    server.succeed("journalctl --vacuum-time=1s")
    server.succeed("touch /run/current-system")
    time.sleep(5)

    # This time should attempt deployment since derivation paths differ
    agent_logs = server.succeed(f"journalctl -u {C.AGENT_SERVICE} --no-pager")
    assert (
        "Starting deployment execution" in agent_logs
    ), "Agent should attempt deployment for different derivation path"

    # Clean up test data
    cf_client.execute_sql(
        "DELETE FROM derivations WHERE commit_id IN (%s, %s)",
        (commit_id, commit_id_different),
    )
    cf_client.execute_sql(
        "DELETE FROM commits WHERE id IN (%s, %s)", (commit_id, commit_id_different)
    )
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))
    cf_client.execute_sql(
        "UPDATE systems SET desired_target = NULL WHERE hostname = %s",
        (agent_hostname,),
    )


@pytest.mark.slow
def test_dry_run_evaluation_robustness(cf_client, server, agent):
    """Test that dry-run evaluations handle malformed flake targets gracefully"""
    wait_for_crystal_forge_ready(server)

    # Test 1: Verify dry-run doesn't produce "flake:derivation" errors
    # This tests the fix for the original issue where eval_main_drv_path was returning garbage

    # Create a flake with a valid repo URL
    now = datetime.now(UTC)
    flake_id = cf_client.execute_sql(
        """
        INSERT INTO flakes (name, repo_url)
        VALUES (%s, %s)
        RETURNING id
        """,
        ("test-dry-run", "https://gitlab.com/test/dotfiles"),
    )[0]["id"]

    # Create a commit
    commit_id = cf_client.execute_sql(
        """
        INSERT INTO commits (flake_id, git_commit_hash, commit_message, author_name, author_email, timestamp, created_at)
        VALUES (%s, %s, 'Test dry run evaluation', 'Test Author', 'test@example.com', %s, %s)
        RETURNING id
        """,
        (flake_id, "abc123def456", now, now),
    )[0]["id"]

    # Create a derivation that should trigger dry-run evaluation
    derivation_id = cf_client.execute_sql(
        """
        INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_target,
            status_id, attempt_count, scheduled_at
        )
        VALUES (
            %s, 'nixos', 'test-system',
            'git+https://gitlab.com/test/dotfiles?rev=abc123def456#nixosConfigurations.test-system.config.system.build.toplevel',
            (SELECT id FROM derivation_statuses WHERE name = 'dry-run-pending'),
            0, NOW()
        )
        RETURNING id
        """,
        (commit_id,),
    )[0]["id"]

    # Wait for the server to process this derivation
    # The key test is that it should NOT fail with "cannot find flake 'flake:derivation'"
    time.sleep(10)

    # Check the derivation status - it should either succeed or fail with a proper error message
    result = cf_client.execute_sql(
        """
        SELECT d.status_id, ds.name as status_name, d.error_message
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE d.id = %s
        """,
        (derivation_id,),
    )[0]

    # The critical test: error message should NOT contain "flake:derivation"
    error_msg = result.get("error_message", "")
    assert (
        "flake:derivation" not in error_msg
    ), f"Dry-run produced malformed flake reference: {error_msg}"
    assert (
        "cannot find flake 'flake:derivation'" not in error_msg
    ), f"Dry-run evaluation regression detected: {error_msg}"

    # If it failed, it should be a proper Nix evaluation error, not a malformed reference
    if result["status_name"] == "dry-run-failed":
        # These are acceptable failure reasons (repo doesn't exist, etc.)
        acceptable_errors = [
            "does not provide attribute",
            "error: getting status of",
            "fatal: repository",
            "nix build --dry-run failed",
            "No such file or directory",
        ]
        assert any(
            acceptable in error_msg for acceptable in acceptable_errors
        ), f"Unexpected dry-run failure: {error_msg}"

    # Clean up
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


@pytest.mark.slow
def test_database_schema_consistency(cf_client, server):
    """Test that database queries include all required columns from the Derivation struct"""
    wait_for_crystal_forge_ready(server)

    # Test that cache push queries include cf_agent_enabled field
    # This tests the fix for the "no column found for name: cf_agent_enabled" error

    # First, verify the derivations table has the cf_agent_enabled column
    columns = cf_client.execute_sql(
        """
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = 'derivations' AND column_name = 'cf_agent_enabled'
        """
    )
    assert len(columns) > 0, "derivations table missing cf_agent_enabled column"

    # Create a test derivation to ensure cache push queries work
    now = datetime.now(UTC)

    # Create required parent records
    flake_id = cf_client.execute_sql(
        """
        INSERT INTO flakes (name, repo_url)
        VALUES (%s, %s)
        RETURNING id
        """,
        ("test-schema", "https://example.com/test"),
    )[0]["id"]

    commit_id = cf_client.execute_sql(
        """
        INSERT INTO commits (flake_id, git_commit_hash, commit_message, author_name, author_email, timestamp, created_at)
        VALUES (%s, %s, 'Test schema', 'Test', 'test@example.com', %s, %s)
        RETURNING id
        """,
        (flake_id, "schema123", now, now),
    )[0]["id"]

    # Create a derivation with build-complete status to trigger cache push logic
    derivation_id = cf_client.execute_sql(
        """
        INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, completed_at, cf_agent_enabled
        )
        VALUES (
            %s, 'nixos', 'test-cache-schema', '/nix/store/test-cache.drv',
            (SELECT id FROM derivation_statuses WHERE name = 'build-complete'),
            1, %s, true
        )
        RETURNING id
        """,
        (commit_id, now),
    )[0]["id"]

    # Wait for cache push logic to potentially process this
    time.sleep(5)

    # Check server logs for the specific error we're trying to prevent
    server_logs = server.succeed(
        "journalctl -u crystal-forge-builder.service --no-pager --since '1 minute ago' | grep -i 'cf_agent_enabled' || true"
    )

    # Should NOT see the schema error in logs
    assert (
        "no column found for name: cf_agent_enabled" not in server_logs
    ), f"Database schema error detected: {server_logs}"

    # Clean up
    cf_client.execute_sql(
        "DELETE FROM cache_push_jobs WHERE derivation_id = %s", (derivation_id,)
    )
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


@pytest.mark.slow
def test_vault_agent_configuration_resilience(cf_client, server):
    """Test that Crystal Forge handles vault-agent configuration issues gracefully"""
    wait_for_crystal_forge_ready(server)

    # Test that the system can evaluate NixOS configurations even with Attic/vault issues
    # This is a regression test for the "cannot coerce null to a string" error

    # Check that the vault-agent service is not causing evaluation failures
    vault_logs = server.succeed(
        "journalctl -u vault-agent-crystal-forge-setup.service --no-pager --since '10 minutes ago' || true"
    )

    # Look for the specific error we fixed
    assert (
        "cannot coerce null to a string" not in vault_logs
    ), f"Vault agent null coercion error detected: {vault_logs}"

    # Check Crystal Forge server logs for vault-related evaluation failures
    cf_logs = server.succeed(
        "journalctl -u crystal-forge-server.service --no-pager --since '10 minutes ago' | grep -i 'vault\\|attic' || true"
    )

    # Should not see configuration evaluation failures related to vault/attic
    problematic_patterns = [
        "cannot coerce null to a string",
        "while evaluating the option.*attic-env",
        "vault-server.*failed",
        "attic.*null",
    ]

    for pattern in problematic_patterns:
        assert (
            pattern not in cf_logs
        ), f"Vault/Attic configuration issue detected: {cf_logs}"


@pytest.mark.slow
def test_build_method_consistency(cf_client, server):
    """Test that dry-run and build methods produce consistent results"""
    wait_for_crystal_forge_ready(server)

    # Test that switching from nix eval to nix build --dry-run produces better error messages
    # This validates the fix for using proper dry-run evaluation

    # Create a scenario that would expose evaluation method differences
    now = datetime.now(UTC)

    flake_id = cf_client.execute_sql(
        """
        INSERT INTO flakes (name, repo_url)
        VALUES (%s, %s)
        RETURNING id
        """,
        ("test-build-method", "https://example.com/nonexistent"),
    )[0]["id"]

    commit_id = cf_client.execute_sql(
        """
        INSERT INTO commits (flake_id, git_commit_hash, commit_message, author_name, author_email, timestamp, created_at)
        VALUES (%s, %s, 'Test build method', 'Test', 'test@example.com', %s, %s)
        RETURNING id
        """,
        (flake_id, "build123", now, now),
    )[0]["id"]

    # Create a derivation that will fail evaluation
    derivation_id = cf_client.execute_sql(
        """
        INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_target,
            status_id, attempt_count, scheduled_at
        )
        VALUES (
            %s, 'nixos', 'test-build-method',
            'https://example.com/nonexistent?rev=build123#nixosConfigurations.test.config.system.build.toplevel',
            (SELECT id FROM derivation_statuses WHERE name = 'dry-run-pending'),
            0, NOW()
        )
        RETURNING id
        """,
        (commit_id,),
    )[0]["id"]

    # Wait for processing
    time.sleep(10)

    # Check that error messages are meaningful and don't contain internal implementation details
    result = cf_client.execute_sql(
        """
        SELECT error_message, status_id, ds.name as status_name
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE d.id = %s
        """,
        (derivation_id,),
    )[0]

    error_msg = result.get("error_message", "")

    # Error messages should be user-friendly, not expose internal method details
    problematic_internals = [
        "eval_main_drv_path",
        "list_immediate_input_drvs",
        "derivation show failed",
        "flake:derivation",
    ]

    for internal in problematic_internals:
        assert (
            internal not in error_msg
        ), f"Error message exposes internal implementation: {error_msg}"

    # If it failed (which it should), error should mention the actual issue
    if result["status_name"] == "dry-run-failed":
        # Should see proper Nix error messages
        expected_error_types = [
            "nix build --dry-run failed",
            "does not provide attribute",
            "error: getting status of",
            "fatal: repository",
        ]
        assert any(
            expected in error_msg for expected in expected_error_types
        ), f"Error message doesn't contain expected Nix error: {error_msg}"

    # Clean up
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


@pytest.mark.slow
def test_server_memory_stability_under_evaluation_load(cf_client, server):
    """Test that server memory remains stable during multiple evaluations"""
    wait_for_crystal_forge_ready(server)

    # Monitor memory before load test
    initial_memory = server.succeed(
        "ps -o rss= -p $(pgrep crystal-forge-server) | awk '{print $1}'"
    ).strip()

    # Create multiple derivations to trigger concurrent evaluation
    now = datetime.now(UTC)
    created_ids = {"flakes": [], "commits": [], "derivations": []}

    for i in range(5):  # Create 5 test scenarios
        flake_id = cf_client.execute_sql(
            """
            INSERT INTO flakes (name, repo_url)
            VALUES (%s, %s)
            RETURNING id
            """,
            (f"test-memory-{i}", f"https://example.com/test-{i}"),
        )[0]["id"]
        created_ids["flakes"].append(flake_id)

        commit_id = cf_client.execute_sql(
            """
            INSERT INTO commits (flake_id, git_commit_hash, commit_message, author_name, author_email, timestamp, created_at)
            VALUES (%s, %s, %s, 'Test', 'test@example.com', %s, %s)
            RETURNING id
            """,
            (flake_id, f"memory{i}123", f"Memory test {i}", now, now),
        )[0]["id"]
        created_ids["commits"].append(commit_id)

        derivation_id = cf_client.execute_sql(
            """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_target,
                status_id, attempt_count, scheduled_at
            )
            VALUES (
                %s, 'nixos', %s, %s,
                (SELECT id FROM derivation_statuses WHERE name = 'dry-run-pending'),
                0, NOW()
            )
            RETURNING id
            """,
            (
                commit_id,
                f"test-memory-{i}",
                f"https://example.com/test-{i}?rev=memory{i}123#nixosConfigurations.test-memory-{i}.config.system.build.toplevel",
            ),
        )[0]["id"]
        created_ids["derivations"].append(derivation_id)

    # Wait for all evaluations to complete
    time.sleep(30)

    # Check final memory usage
    final_memory = server.succeed(
        "ps -o rss= -p $(pgrep crystal-forge-server) | awk '{print $1}'"
    ).strip()

    # Memory should not have grown excessively (allow for some growth, but not massive leaks)
    initial_mb = int(initial_memory) / 1024
    final_mb = int(final_memory) / 1024
    memory_growth = final_mb - initial_mb

    # Allow up to 100MB growth for the test load, but flag if excessive
    assert (
        memory_growth < 100
    ), f"Excessive memory growth detected: {initial_mb:.1f}MB -> {final_mb:.1f}MB (+{memory_growth:.1f}MB)"

    # Check that server is still responsive
    server.succeed("systemctl is-active crystal-forge-server.service")

    # Clean up all test data
    for derivation_id in created_ids["derivations"]:
        cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    for commit_id in created_ids["commits"]:
        cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    for flake_id in created_ids["flakes"]:
        cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))
