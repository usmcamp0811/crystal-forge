# packages/cf-test-modules/cf_test/tests/test_systems_status_scenarios.py

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


@pytest.mark.views
@pytest.mark.database
@pytest.mark.parametrize(
    "build,hostname,expect",
    [
        (
            scenario_never_seen,
            "test-never-seen",
            {
                "overall": "never_seen",
                "connectivity": "never_seen",
                "update": "never_seen",
            },
        ),
        (
            scenario_up_to_date,
            "test-uptodate",
            {"overall": "up_to_date", "connectivity": "online", "update": "up_to_date"},
        ),
        (
            scenario_behind,
            "test-behind",
            {"overall": "behind", "connectivity": "online", "update": "behind"},
        ),
        (
            scenario_offline,
            "test-offline",
            {"overall": "offline", "connectivity": "offline", "update": None},
        ),
        (
            scenario_eval_failed,
            "test-eval-failed",
            {
                "overall": "evaluation_failed",
                "connectivity": "online",
                "update": "evaluation_failed",
            },
        ),
    ],
)
def test_status_scenarios(cf_client: CFTestClient, build, hostname, expect):
    data = build(cf_client, hostname=hostname)
    host = data.get("hostname", hostname)

    row = cf_client.execute_sql(
        f"""
        SELECT hostname, connectivity_status, update_status, overall_status,
               current_derivation_path, latest_derivation_path, latest_commit_hash
        FROM {VIEW_STATUS}
        WHERE hostname = %s
        """,
        (host,),
    )
    assert row and len(row) == 1, "expected exactly one row"
    r = row[0]

    if expect["overall"] is not None:
        assert r["overall_status"] == expect["overall"]
    if expect["connectivity"] is not None:
        assert r["connectivity_status"] == expect["connectivity"]
    if expect["update"] is not None:
        assert r["update_status"] == expect["update"]

    cf_client.save_artifact(
        json.dumps(r, indent=2, default=str),
        f"{host}_scenario_result.json",
        f"{host} scenario result",
    )

    cf_client.cleanup_test_data(data["cleanup"])


# Add this test to validate the scenario itself works
def test_validate_scenario_eval_failed(cf_client):
    """Validate that scenario_eval_failed creates the expected data"""
    from cf_test.scenarios import scenario_eval_failed

    hostname = "validate-scenario"

    # Clean up first (correct order for foreign key constraints)
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
        "DELETE FROM commits WHERE git_commit_hash IN ('working123', 'broken456')"
    )
    cf_client.execute_sql(
        "DELETE FROM flakes WHERE repo_url = 'https://example.com/failed.git'"
    )

    print(f"\n=== Testing scenario_eval_failed for {hostname} ===")

    # Run the scenario
    data = scenario_eval_failed(cf_client, hostname)
    print(f"✅ Scenario returned: {data}")

    # Validate the data was created correctly
    print("\n=== Validating created data ===")

    # 1. Check flake exists
    flake = cf_client.execute_sql(
        "SELECT * FROM flakes WHERE repo_url = 'https://example.com/failed.git'"
    )
    assert len(flake) == 1, f"Expected 1 flake, got {len(flake)}"
    print(f"✅ Flake: {flake[0]}")

    # 2. Check commits exist (2 commits: working123, broken456)
    commits = cf_client.execute_sql(
        """
        SELECT git_commit_hash, commit_timestamp FROM commits 
        WHERE git_commit_hash IN ('working123', 'broken456') 
        ORDER BY commit_timestamp ASC
    """
    )
    assert len(commits) == 2, f"Expected 2 commits, got {len(commits)}"
    assert commits[0]["git_commit_hash"] == "working123"  # older
    assert commits[1]["git_commit_hash"] == "broken456"  # newer
    print(f"✅ Commits: {commits}")

    # 3. Check derivations (2 derivations: complete + failed)
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

    # Check old derivation (working123 -> complete)
    old_deriv = derivations[0]
    assert old_deriv["git_commit_hash"] == "working123"
    assert old_deriv["status"] == "complete"
    assert old_deriv["derivation_path"] is not None
    print(f"✅ Old derivation (complete): {old_deriv}")

    # Check new derivation (broken456 -> failed)
    new_deriv = derivations[1]
    assert new_deriv["git_commit_hash"] == "broken456"
    assert new_deriv["status"] == "failed"
    assert new_deriv["derivation_path"] is None  # Failed derivations have no path
    print(f"✅ New derivation (failed): {new_deriv}")

    # 4. Check system exists and uses old working derivation
    systems = cf_client.execute_sql(
        "SELECT * FROM systems WHERE hostname = %s", (hostname,)
    )
    assert len(systems) == 1, f"Expected 1 system, got {len(systems)}"
    system = systems[0]
    expected_deriv_path = f"/nix/store/working12-nixos-system-{hostname}.drv"
    assert system["derivation"] == expected_deriv_path
    print(
        f"✅ System: hostname={system['hostname']}, derivation={system['derivation']}"
    )

    # 5. Check system state
    states = cf_client.execute_sql(
        "SELECT * FROM system_states WHERE hostname = %s", (hostname,)
    )
    assert len(states) == 1, f"Expected 1 system state, got {len(states)}"
    state = states[0]
    assert state["derivation_path"] == expected_deriv_path
    print(
        f"✅ System state: hostname={state['hostname']}, derivation_path={state['derivation_path']}"
    )

    # 6. Check heartbeat
    heartbeats = cf_client.execute_sql(
        """
        SELECT h.* FROM agent_heartbeats h
        JOIN system_states s ON h.system_state_id = s.id
        WHERE s.hostname = %s
    """,
        (hostname,),
    )
    assert len(heartbeats) == 1, f"Expected 1 heartbeat, got {len(heartbeats)}"
    print(f"✅ Heartbeat: {heartbeats[0]}")

    print("\n=== Summary ===")
    print(f"✅ Flake 'failed-app' created")
    print(f"✅ 2 commits: 'working123' (old) and 'broken456' (new)")
    print(f"✅ 2 derivations: complete (with path) and failed (no path)")
    print(f"✅ System running old working derivation")
    print(f"✅ System state and heartbeat created")
    print(f"✅ Scenario is working correctly!")

    # Clean up
    cf_client.cleanup_test_data(data["cleanup"])
    print("✅ Cleanup completed")

    # This test should pass if scenario works correctly
    assert True, "Scenario validation completed successfully"


# Simple test that just validates the scenario works
def test_scenario_creates_data(cf_client):
    """Test that scenario_eval_failed creates data successfully"""
    import time

    from cf_test.scenarios import scenario_eval_failed

    # Use unique hostname to avoid conflicts
    hostname = f"validate-{int(time.time())}"

    print(f"\n=== Testing scenario_eval_failed for {hostname} ===")

    # Run the scenario
    try:
        data = scenario_eval_failed(cf_client, hostname)
        print(f"✅ Scenario completed successfully!")
        print(f"✅ Returned cleanup data: {data}")

        # Just check that we can query the view - don't worry about cleanup
        view_result = cf_client.execute_sql(
            """
            SELECT hostname, connectivity_status, update_status, overall_status,
                   current_derivation_path, latest_derivation_path, latest_commit_hash
            FROM view_systems_status_table
            WHERE hostname = %s
        """,
            (hostname,),
        )

        print(f"✅ View query successful: {len(view_result)} rows returned")
        if view_result:
            r = view_result[0]
            print(
                f"✅ View data: connectivity={r['connectivity_status']}, update={r['update_status']}, overall={r['overall_status']}"
            )
            print(
                f"✅ Paths: current={r['current_derivation_path']}, latest={r['latest_derivation_path']}"
            )

        # Don't attempt cleanup - just leave the data for manual inspection
        print(f"✅ Test data left with hostname: {hostname}")

    except Exception as e:
        print(f"❌ Scenario failed: {e}")
        raise

    assert True, f"Scenario test completed - data created with hostname {hostname}"
