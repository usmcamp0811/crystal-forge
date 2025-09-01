import json

import pytest

from cf_test import CFTestClient
from cf_test.scenarios import (
    scenario_behind,
    scenario_eval_failed,
    scenario_never_seen,
    scenario_offline,
    scenario_up_to_date,
)


@pytest.mark.harness
def test_validate_scenario_eval_failed(cf_client, clean_test_data):
    """Validate that scenario_eval_failed creates the expected data"""
    import time

    hostname = f"validate-scenario-{int(time.time())}"

    print(f"\n=== Testing scenario_eval_failed for {hostname} ===")
    data = scenario_eval_failed(cf_client, hostname)
    print(f"✅ Scenario returned: {data}")

    print("\n=== Validating created data ===")

    # Use the flake_id from the returned data instead of searching by repo_url
    flake = cf_client.execute_sql(
        "SELECT * FROM flakes WHERE id = %s", (data["flake_id"],)
    )
    assert len(flake) == 1, f"Expected 1 flake, got {len(flake)}"
    print(f"✅ Flake: {flake[0]}")

    # Get commits for this flake
    commits = cf_client.execute_sql(
        """
        SELECT git_commit_hash, commit_timestamp FROM commits
        WHERE flake_id = %s
        ORDER BY commit_timestamp ASC
        """,
        (data["flake_id"],),
    )
    assert len(commits) == 2, f"Expected 2 commits, got {len(commits)}"
    assert commits[0]["git_commit_hash"].startswith("working123")
    assert commits[1]["git_commit_hash"].startswith("broken456")
    print(f"✅ Commits: {commits}")

    # Get derivations for this hostname
    derivations = cf_client.execute_sql(
        """
        SELECT d.derivation_name, d.derivation_path, ds.name as status, c.git_commit_hash
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        JOIN commits c ON d.commit_id = c.id
        WHERE d.derivation_name = %s OR d.derivation_name LIKE %s
        ORDER BY c.commit_timestamp ASC
        """,
        (hostname, f"{hostname}-%"),
    )
    assert (
        len(derivations) >= 1
    ), f"Expected at least 1 derivation, got {len(derivations)}"

    # Find the complete and failed derivations
    complete_derivs = [d for d in derivations if d["status"] == "complete"]
    failed_derivs = [d for d in derivations if d["status"] == "failed"]

    assert len(complete_derivs) >= 1, "Expected at least 1 complete derivation"
    assert len(failed_derivs) >= 1, "Expected at least 1 failed derivation"

    print(f"✅ Complete derivations: {complete_derivs}")
    print(f"✅ Failed derivations: {failed_derivs}")

    # Verify system exists
    systems = cf_client.execute_sql(
        "SELECT * FROM systems WHERE hostname = %s", (hostname,)
    )
    assert len(systems) == 1, f"Expected 1 system, got {len(systems)}"

    # Verify system states
    states = cf_client.execute_sql(
        "SELECT * FROM system_states WHERE hostname = %s", (hostname,)
    )
    assert len(states) == 1, f"Expected 1 system state, got {len(states)}"

    # Verify heartbeats
    heartbeats = cf_client.execute_sql(
        """
        SELECT h.* FROM agent_heartbeats h
        JOIN system_states s ON h.system_state_id = s.id
        WHERE s.hostname = %s
        """,
        (hostname,),
    )
    assert len(heartbeats) == 1, f"Expected 1 heartbeat, got {len(heartbeats)}"

    print("✅ All validations passed")
    # Clean up will happen automatically via the fixture


@pytest.mark.harness
def test_scenario_creates_data(cf_client):
    """Scenario should create data and be fully cleaned up"""
    import time

    from cf_test.scenarios import scenario_eval_failed

    hostname = f"validate-{int(time.time())}"
    data = scenario_eval_failed(cf_client, hostname)

    # Touch the view to ensure it resolves without error
    _ = cf_client.execute_sql(
        """
        SELECT hostname, heartbeat_status, last_state_change, status_description
        FROM view_system_heartbeat_status
        WHERE hostname = %s
        """,
        (hostname,),
    )

    # Ensure no DB artifacts remain
    cf_client.cleanup_test_data(data["cleanup"])
