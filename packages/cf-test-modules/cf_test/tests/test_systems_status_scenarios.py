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


# Add this test to debug the scenario step by step
def test_debug_scenario_step_by_step(cf_client):
    """Debug the scenario_eval_failed step by step"""

    hostname = "debug-step-by-step"

    # Clean up first
    cf_client.execute_sql(
        """
        DELETE FROM agent_heartbeats WHERE system_state_id IN (
            SELECT id FROM system_states WHERE hostname = %s
        );
        DELETE FROM system_states WHERE hostname = %s;
        DELETE FROM systems WHERE hostname = %s;
        DELETE FROM derivations WHERE derivation_name = %s;
        DELETE FROM commits WHERE git_commit_hash IN ('working123', 'broken456');
        DELETE FROM flakes WHERE repo_url = 'https://example.com/failed.git';
    """,
        (hostname, hostname, hostname, hostname),
    )

    print("\n=== Step 1: Check derivation_statuses table ===")
    statuses = cf_client.execute_sql(
        "SELECT id, name FROM public.derivation_statuses ORDER BY id"
    )
    print(f"Available statuses: {statuses}")

    # Check if 'complete' and 'failed' exist
    complete_id = None
    failed_id = None
    for status in statuses:
        if status["name"] == "complete":
            complete_id = status["id"]
        elif status["name"] == "failed":
            failed_id = status["id"]

    print(f"Complete ID: {complete_id}, Failed ID: {failed_id}")

    if complete_id is None or failed_id is None:
        print("❌ Missing required derivation_statuses! Need 'complete' and 'failed'")
        # Let's see what statuses exist
        all_statuses = [s["name"] for s in statuses]
        print(f"Existing statuses: {all_statuses}")

        # Try to create them if missing
        if complete_id is None:
            try:
                result = cf_client.execute_sql(
                    "INSERT INTO public.derivation_statuses (name, description) VALUES ('complete', 'Build completed successfully') RETURNING id"
                )
                complete_id = result[0]["id"]
                print(f"✅ Created 'complete' status with ID {complete_id}")
            except Exception as e:
                print(f"❌ Failed to create 'complete' status: {e}")

        if failed_id is None:
            try:
                result = cf_client.execute_sql(
                    "INSERT INTO public.derivation_statuses (name, description) VALUES ('failed', 'Build failed') RETURNING id"
                )
                failed_id = result[0]["id"]
                print(f"✅ Created 'failed' status with ID {failed_id}")
            except Exception as e:
                print(f"❌ Failed to create 'failed' status: {e}")

    print(f"\n=== Step 2: Insert flake ===")
    try:
        flake_result = cf_client.execute_sql(
            """
            INSERT INTO public.flakes (name, repo_url)
            VALUES ('failed-app', 'https://example.com/failed.git')
            ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
            RETURNING id
        """
        )
        flake_id = flake_result[0]["id"] if flake_result else None
        print(f"Flake ID: {flake_id}")
    except Exception as e:
        print(f"❌ Flake insert failed: {e}")
        return

    print(f"\n=== Step 3: Insert commits ===")
    try:
        old_commit_result = cf_client.execute_sql(
            """
            INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, 'working123', NOW() - INTERVAL '1 day', 0)
            ON CONFLICT (flake_id, git_commit_hash) DO UPDATE SET commit_timestamp = EXCLUDED.commit_timestamp
            RETURNING id
        """,
            (flake_id,),
        )
        old_commit_id = old_commit_result[0]["id"] if old_commit_result else None
        print(f"Old commit ID: {old_commit_id}")

        new_commit_result = cf_client.execute_sql(
            """
            INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, 'broken456', NOW() - INTERVAL '30 minutes', 0)
            ON CONFLICT (flake_id, git_commit_hash) DO UPDATE SET commit_timestamp = EXCLUDED.commit_timestamp
            RETURNING id
        """,
            (flake_id,),
        )
        new_commit_id = new_commit_result[0]["id"] if new_commit_result else None
        print(f"New commit ID: {new_commit_id}")
    except Exception as e:
        print(f"❌ Commit insert failed: {e}")
        return

    print(f"\n=== Step 4: Insert derivations ===")
    old_drv = f"/nix/store/working12-nixos-system-{hostname}.drv"

    try:
        # Insert successful derivation
        old_deriv_result = cf_client.execute_sql(
            """
            INSERT INTO public.derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, attempt_count, scheduled_at, completed_at
            )
            VALUES (%s, 'nixos', %s, %s, %s, 0, NOW() - INTERVAL '20 hours', NOW() - INTERVAL '20 hours')
            ON CONFLICT DO NOTHING
            RETURNING id
        """,
            (old_commit_id, hostname, old_drv, complete_id),
        )
        old_deriv_id = old_deriv_result[0]["id"] if old_deriv_result else None
        print(f"Old derivation ID: {old_deriv_id}")

        # Insert failed derivation
        new_deriv_result = cf_client.execute_sql(
            """
            INSERT INTO public.derivations (
                commit_id, derivation_type, derivation_name, derivation_path, status_id,
                completed_at, error_message, attempt_count
            )
            VALUES (%s, 'nixos', %s, NULL, %s, NOW() - INTERVAL '30 minutes', 'Evaluation failed', 0)
            ON CONFLICT DO NOTHING
            RETURNING id
        """,
            (new_commit_id, hostname, failed_id),
        )
        new_deriv_id = new_deriv_result[0]["id"] if new_deriv_result else None
        print(f"New derivation ID: {new_deriv_id}")
    except Exception as e:
        print(f"❌ Derivation insert failed: {e}")
        import traceback

        traceback.print_exc()
        return

    print(f"\n=== Step 5: Insert system ===")
    try:
        system_result = cf_client.execute_sql(
            """
            INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, TRUE, %s, 'fake-key')
            ON CONFLICT (hostname) DO UPDATE
                SET flake_id = EXCLUDED.flake_id,
                    derivation = EXCLUDED.derivation,
                    is_active = EXCLUDED.is_active
            RETURNING id
        """,
            (hostname, flake_id, old_drv),
        )
        system_id = system_result[0]["id"] if system_result else None
        print(f"System ID: {system_id}")
    except Exception as e:
        print(f"❌ System insert failed: {e}")
        return

    print(f"\n=== Step 6: Insert system state ===")
    try:
        state_result = cf_client.execute_sql(
            """
            INSERT INTO public.system_states (
                hostname, change_reason, derivation_path, os, kernel,
                memory_gb, uptime_secs, cpu_brand, cpu_cores,
                primary_ip_address, nixos_version, agent_compatible, "timestamp"
            )
            VALUES (
                %s, 'startup', %s, 'NixOS', '6.6.89',
                32.0, 3600, 'Intel Xeon', 16,
                '192.168.1.103', '25.05', TRUE, NOW() - INTERVAL '5 minutes'
            )
            RETURNING id
        """,
            (hostname, old_drv),
        )
        state_id = state_result[0]["id"] if state_result else None
        print(f"State ID: {state_id}")
    except Exception as e:
        print(f"❌ System state insert failed: {e}")
        return

    print(f"\n=== Step 7: Insert heartbeat ===")
    try:
        heartbeat_result = cf_client.execute_sql(
            """
            INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
            VALUES (%s, NOW() - INTERVAL '2 minutes', '2.0.0', 'build123')
            RETURNING id
        """,
            (state_id,),
        )
        heartbeat_id = heartbeat_result[0]["id"] if heartbeat_result else None
        print(f"Heartbeat ID: {heartbeat_id}")
    except Exception as e:
        print(f"❌ Heartbeat insert failed: {e}")
        return

    print(f"\n=== Step 8: Check view result ===")
    view_result = cf_client.execute_sql(
        """
        SELECT hostname, connectivity_status, update_status, overall_status,
               current_derivation_path, latest_derivation_path
        FROM view_systems_status_table
        WHERE hostname = %s
    """,
        (hostname,),
    )
    print(f"View result: {view_result}")

    print(f"\n=== Summary ===")
    print(f"Flake ID: {flake_id}")
    print(f"Old commit ID: {old_commit_id}")
    print(f"New commit ID: {new_commit_id}")
    print(f"Old derivation ID: {old_deriv_id}")
    print(f"New derivation ID: {new_deriv_id}")
    print(f"System ID: {system_id}")
    print(f"State ID: {state_id}")
    print(f"Heartbeat ID: {heartbeat_id}")

    # Clean up
    cf_client.execute_sql(
        """
        DELETE FROM agent_heartbeats WHERE id = %s;
        DELETE FROM system_states WHERE id = %s;
        DELETE FROM systems WHERE id = %s;
        DELETE FROM derivations WHERE id IN (%s, %s);
        DELETE FROM commits WHERE id IN (%s, %s);
        DELETE FROM flakes WHERE id = %s;
    """,
        (
            heartbeat_id,
            state_id,
            system_id,
            old_deriv_id,
            new_deriv_id,
            old_commit_id,
            new_commit_id,
            flake_id,
        ),
    )

    assert len(view_result) == 1, f"Expected 1 row in view, got {len(view_result)}"
