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

VIEW_STATUS = "public.view_systems_status_table"


@pytest.mark.harness
def test_validate_scenario_eval_failed(cf_client):
    """Validate that scenario_eval_failed creates the expected data"""
    from cf_test.scenarios import scenario_eval_failed

    hostname = "validate-scenario"

    # Pre-clean (commit hashes are timestamp-suffixed)
    cf_client.execute_sql(
        "DELETE FROM agent_heartbeats WHERE system_state_id IN (SELECT id FROM system_states WHERE hostname = %s)",
        (hostname,),
    )
    cf_client.execute_sql("DELETE FROM system_states WHERE hostname = %s", (hostname,))
    cf_client.execute_sql("DELETE FROM systems WHERE hostname = %s", (hostname,))
    cf_client.execute_sql(
        "DELETE FROM derivations WHERE derivation_name = %s", (hostname,)
    )
    cf_client.execute_sql(
        "DELETE FROM commits WHERE git_commit_hash LIKE 'working123-%' OR git_commit_hash LIKE 'broken456-%'"
    )
    cf_client.execute_sql(
        "DELETE FROM flakes WHERE repo_url = 'https://example.com/failed.git'"
    )

    print(f"\n=== Testing scenario_eval_failed for {hostname} ===")
    data = scenario_eval_failed(cf_client, hostname)
    print(f"✅ Scenario returned: {data}")

    print("\n=== Validating created data ===")
    flake = cf_client.execute_sql(
        "SELECT * FROM flakes WHERE repo_url = 'https://example.com/failed.git'"
    )
    assert len(flake) == 1, f"Expected 1 flake, got {len(flake)}"
    print(f"✅ Flake: {flake[0]}")

    # Commits use timestamp suffixes
    commits = cf_client.execute_sql(
        """
        SELECT git_commit_hash, commit_timestamp FROM commits
        WHERE git_commit_hash LIKE 'working123-%' OR git_commit_hash LIKE 'broken456-%'
        ORDER BY commit_timestamp ASC
        """
    )
    assert len(commits) == 2, f"Expected 2 commits, got {len(commits)}"
    assert commits[0]["git_commit_hash"].startswith("working123")
    assert commits[1]["git_commit_hash"].startswith("broken456")
    print(f"✅ Commits: {commits}")

    derivations = cf_client.execute_sql(
        """
        SELECT d.derivation_name, d.derivation_path, ds.name as status, c.git_commit_hash
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        JOIN commits c ON d.commit_id = c.id
        WHERE d.derivation_name = %s
        ORDER BY c.commit_timestamp ASC
        """,
        (hostname,),
    )
    assert len(derivations) == 2, f"Expected 2 derivations, got {len(derivations)}"
    old_deriv, new_deriv = derivations[0], derivations[1]
    assert old_deriv["status"] == "complete" and old_deriv["derivation_path"]
    assert new_deriv["status"] == "failed" and new_deriv["derivation_path"] is None
    print(f"✅ Old derivation (complete): {old_deriv}")
    print(f"✅ New derivation (failed): {new_deriv}")

    systems = cf_client.execute_sql(
        "SELECT * FROM systems WHERE hostname = %s", (hostname,)
    )
    assert len(systems) == 1
    expected_deriv_path = f"/nix/store/working12-nixos-system-{hostname}.drv"
    assert systems[0]["derivation"] == expected_deriv_path

    states = cf_client.execute_sql(
        "SELECT * FROM system_states WHERE hostname = %s", (hostname,)
    )
    assert len(states) == 1 and states[0]["derivation_path"] == expected_deriv_path

    heartbeats = cf_client.execute_sql(
        """
        SELECT h.* FROM agent_heartbeats h
        JOIN system_states s ON h.system_state_id = s.id
        WHERE s.hostname = %s
        """,
        (hostname,),
    )
    assert len(heartbeats) == 1

    # Clean up all created rows
    cf_client.cleanup_test_data(data["cleanup"])
    print("✅ Cleanup completed")
    assert True


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
        SELECT hostname, connectivity_status, update_status, overall_status
        FROM view_systems_status_table
        WHERE hostname = %s
        """,
        (hostname,),
    )

    # Ensure no DB artifacts remain
    cf_client.cleanup_test_data(data["cleanup"])
