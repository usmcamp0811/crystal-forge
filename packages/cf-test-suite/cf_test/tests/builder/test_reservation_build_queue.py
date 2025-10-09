import json
import os
import subprocess
import time

import pytest

pytestmark = [
    pytest.mark.builder,
    pytest.mark.integration,
    pytest.mark.reservation_queue,
]


@pytest.fixture(scope="module")
def test_flake_data(cf_client, cfServer):
    """Set up test flake and commit data for reservation queue tests"""
    # Insert test flake
    flake_result = cf_client.execute_sql(
        """INSERT INTO flakes (name, repo_url)
           VALUES ('test-reservation-queue', 'http://gitserver/test-queue')
           RETURNING id"""
    )
    flake_id = flake_result[0]["id"]

    # Insert multiple commits (newest to oldest)
    commits = []
    for i in range(3):
        commit_result = cf_client.execute_sql(
            """INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
               VALUES (%s, %s, NOW() - INTERVAL '%s minutes')
               RETURNING id""",
            (flake_id, f"commit-{i:03d}", i * 10),  # 0, 10, 20 minutes ago
        )
        commits.append(commit_result[0]["id"])

    yield {"flake_id": flake_id, "commit_ids": commits}

    # Cleanup
    for commit_id in commits:
        cf_client.execute_sql(
            "DELETE FROM derivations WHERE commit_id = %s", (commit_id,)
        )
    cf_client.execute_sql("DELETE FROM commits WHERE flake_id = %s", (flake_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


def test_multiple_workers_claiming_from_queue(cf_client, cfServer, test_flake_data):
    """Test that multiple workers can claim different derivations without conflicts"""
    commit_id = test_flake_data["commit_ids"][0]

    # Create 5 package derivations ready to build
    derivation_ids = []
    for i in range(5):
        result = cf_client.execute_sql(
            """INSERT INTO derivations (
                   commit_id, derivation_type, derivation_name, derivation_path,
                   scheduled_at, pname, version, status_id
               ) VALUES (
                   %s, 'package', %s, %s,
                   NOW(), %s, '1.0', 5
               ) RETURNING id""",
            (
                commit_id,
                f"test-package-{i}",
                f"/nix/store/test-package-{i}.drv",
                f"test-package-{i}",
            ),
        )
        derivation_ids.append(result[0]["id"])

    cfServer.log(f"Created {len(derivation_ids)} derivations ready for building")

    # Simulate multiple workers claiming work
    claimed_derivations = []
    for worker_num in range(3):
        worker_id = f"test-worker-{worker_num}"

        # Claim next derivation
        result = cf_client.execute_sql(
            """
            WITH next_work AS (
                SELECT id FROM view_buildable_derivations
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            INSERT INTO build_reservations (worker_id, derivation_id, nixos_derivation_id)
            SELECT %s, id, NULL FROM next_work
            RETURNING derivation_id
            """,
            (worker_id,),
        )

        if result:
            claimed_id = result[0]["derivation_id"]
            claimed_derivations.append(claimed_id)
            cfServer.log(f"Worker {worker_num} claimed derivation {claimed_id}")

            # Mark as in-progress
            cf_client.execute_sql(
                "UPDATE derivations SET status_id = 8 WHERE id = %s",
                (claimed_id,),
            )

    # Verify all claims are unique
    assert len(claimed_derivations) == 3, "Expected 3 workers to claim work"
    assert len(set(claimed_derivations)) == 3, "Workers claimed duplicate derivations!"

    cfServer.log("✅ Multiple workers successfully claimed unique derivations")

    # Verify reservations exist
    reservations = cf_client.execute_sql(
        "SELECT worker_id, derivation_id FROM build_reservations ORDER BY worker_id"
    )

    assert len(reservations) == 3, f"Expected 3 reservations, found {len(reservations)}"

    for i, res in enumerate(reservations):
        cfServer.log(
            f"  Reservation {i}: worker={res['worker_id']}, derivation={res['derivation_id']}"
        )

    # Cleanup
    cf_client.execute_sql(
        "DELETE FROM build_reservations WHERE worker_id LIKE 'test-worker-%'"
    )
    for deriv_id in derivation_ids:
        cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (deriv_id,))


def test_worker_crash_recovery(cf_client, cfServer, test_flake_data):
    """Test that crashed workers have their reservations cleaned up"""
    commit_id = test_flake_data["commit_ids"][0]

    # Create a derivation
    result = cf_client.execute_sql(
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, derivation_path,
               scheduled_at, pname, version, status_id
           ) VALUES (
               %s, 'package', 'crash-test-pkg', '/nix/store/crash-test.drv',
               NOW(), 'crash-test-pkg', '1.0', 5
           ) RETURNING id""",
        (commit_id,),
    )
    derivation_id = result[0]["id"]

    # Create a reservation with an old heartbeat (simulating crashed worker)
    cf_client.execute_sql(
        """INSERT INTO build_reservations (worker_id, derivation_id, reserved_at, heartbeat_at)
           VALUES ('crashed-worker', %s, NOW() - INTERVAL '10 minutes', NOW() - INTERVAL '10 minutes')""",
        (derivation_id,),
    )

    # Mark derivation as in-progress
    cf_client.execute_sql(
        "UPDATE derivations SET status_id = 8, started_at = NOW() - INTERVAL '10 minutes' WHERE id = %s",
        (derivation_id,),
    )

    cfServer.log("Created reservation with stale heartbeat (10 minutes old)")

    # Manually trigger cleanup (simulating the background task)
    stale_threshold = 300  # 5 minutes
    reclaimed = cf_client.execute_sql(
        """
        DELETE FROM build_reservations
        WHERE heartbeat_at < NOW() - make_interval(secs => %s)
        RETURNING derivation_id
        """,
        (float(stale_threshold),),
    )

    assert len(reclaimed) == 1, "Expected to reclaim 1 stale reservation"
    assert reclaimed[0]["derivation_id"] == derivation_id

    cfServer.log(f"✅ Reclaimed stale reservation for derivation {derivation_id}")

    # Reset derivation status
    cf_client.execute_sql(
        "UPDATE derivations SET status_id = 5, started_at = NULL WHERE id = %s",
        (derivation_id,),
    )

    # Verify derivation is back in buildable queue
    buildable = cf_client.execute_sql(
        "SELECT id FROM view_buildable_derivations WHERE id = %s",
        (derivation_id,),
    )

    assert len(buildable) == 1, "Derivation should be back in buildable queue"
    cfServer.log("✅ Derivation reset to buildable status after crash recovery")

    # Cleanup
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_dead_worker_cleanup(cf_client, cfServer, test_flake_data):
    """Test the stale reservation cleanup process"""
    commit_id = test_flake_data["commit_ids"][0]

    # Create multiple derivations with different heartbeat ages
    test_cases = [
        {"name": "fresh-worker", "age_minutes": 1, "should_cleanup": False},
        {"name": "stale-worker-1", "age_minutes": 6, "should_cleanup": True},
        {"name": "stale-worker-2", "age_minutes": 10, "should_cleanup": True},
    ]

    created_derivations = []
    for case in test_cases:
        result = cf_client.execute_sql(
            """INSERT INTO derivations (
                   commit_id, derivation_type, derivation_name, derivation_path,
                   scheduled_at, pname, version, status_id
               ) VALUES (
                   %s, 'package', %s, %s, NOW(), %s, '1.0', 8
               ) RETURNING id""",
            (
                commit_id,
                case["name"],
                f"/nix/store/{case['name']}.drv",
                case["name"],
            ),
        )
        deriv_id = result[0]["id"]
        created_derivations.append(deriv_id)

        # Create reservation with specific heartbeat age
        cf_client.execute_sql(
            """INSERT INTO build_reservations (worker_id, derivation_id, reserved_at, heartbeat_at)
               VALUES (%s, %s, NOW() - INTERVAL '%s minutes', NOW() - INTERVAL '%s minutes')""",
            (case["name"], deriv_id, case["age_minutes"], case["age_minutes"]),
        )

    cfServer.log(f"Created {len(test_cases)} reservations with varying heartbeat ages")

    # Run cleanup with 5-minute threshold
    reclaimed = cf_client.execute_sql(
        """
        DELETE FROM build_reservations
        WHERE heartbeat_at < NOW() - make_interval(secs => 300)
        RETURNING worker_id, derivation_id
        """
    )

    expected_cleanup = sum(1 for c in test_cases if c["should_cleanup"])
    assert (
        len(reclaimed) == expected_cleanup
    ), f"Expected to cleanup {expected_cleanup} stale reservations"

    reclaimed_workers = {r["worker_id"] for r in reclaimed}
    cfServer.log(
        f"✅ Cleaned up {len(reclaimed)} stale reservations: {reclaimed_workers}"
    )

    # Verify fresh worker still has reservation
    fresh_res = cf_client.execute_sql(
        "SELECT COUNT(*) as count FROM build_reservations WHERE worker_id = 'fresh-worker'"
    )
    assert (
        fresh_res[0]["count"] == 1
    ), "Fresh worker reservation should not be cleaned up"

    # Cleanup
    cf_client.execute_sql(
        "DELETE FROM build_reservations WHERE worker_id LIKE '%worker%'"
    )
    for deriv_id in created_derivations:
        cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (deriv_id,))


def test_priority_ordering_newest_first(cf_client, cfServer, test_flake_data):
    """Test that newest commits are prioritized over older ones"""

    # Create derivations across multiple commits (newest to oldest)
    derivations = []
    for i, commit_id in enumerate(test_flake_data["commit_ids"]):
        result = cf_client.execute_sql(
            """INSERT INTO derivations (
                   commit_id, derivation_type, derivation_name, derivation_path,
                   scheduled_at, pname, version, status_id
               ) VALUES (
                   %s, 'package', %s, %s, NOW(), %s, '1.0', 5
               ) RETURNING id""",
            (
                commit_id,
                f"pkg-commit-{i}",
                f"/nix/store/pkg-commit-{i}.drv",
                f"pkg-commit-{i}",
            ),
        )
        derivations.append({"commit_idx": i, "deriv_id": result[0]["id"]})

    # Query the buildable derivations view to check ordering
    queue = cf_client.execute_sql(
        """
        SELECT d.id, d.derivation_name, c.commit_timestamp, v.queue_position
        FROM view_buildable_derivations v
        JOIN derivations d ON v.id = d.id
        JOIN commits c ON d.commit_id = c.id
        WHERE d.derivation_name LIKE 'pkg-commit-%'
        ORDER BY v.queue_position
        """
    )

    cfServer.log(f"Queue ordering (newest first):")
    for i, item in enumerate(queue):
        cfServer.log(f"  Position {item['queue_position']}: {item['derivation_name']}")

    # Verify newest commit is first
    assert (
        queue[0]["derivation_name"] == "pkg-commit-0"
    ), "Newest commit should be first in queue"
    assert (
        queue[-1]["derivation_name"] == "pkg-commit-2"
    ), "Oldest commit should be last in queue"

    cfServer.log("✅ Queue prioritizes newest commits first")

    # Cleanup
    for deriv in derivations:
        cf_client.execute_sql(
            "DELETE FROM derivations WHERE id = %s", (deriv["deriv_id"],)
        )


def test_system_builds_blocked_until_packages_complete(
    cf_client, cfServer, test_flake_data
):
    """Test that NixOS system builds only appear after all packages are complete"""
    commit_id = test_flake_data["commit_ids"][0]

    # Create a NixOS system derivation
    nixos_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, derivation_path,
               scheduled_at, pname, version, status_id
           ) VALUES (
               %s, 'nixos', 'test-system', '/nix/store/test-system.drv',
               NOW(), 'test-system', '1.0', 5
           ) RETURNING id""",
        (commit_id,),
    )
    nixos_id = nixos_result[0]["id"]

    # Create 3 package dependencies
    package_ids = []
    for i in range(3):
        result = cf_client.execute_sql(
            """INSERT INTO derivations (
                   commit_id, derivation_type, derivation_name, derivation_path,
                   scheduled_at, pname, version, status_id, nixos_derivation_id
               ) VALUES (
                   %s, 'package', %s, %s, NOW(), %s, '1.0', 5, %s
               ) RETURNING id""",
            (
                commit_id,
                f"sys-pkg-{i}",
                f"/nix/store/sys-pkg-{i}.drv",
                f"sys-pkg-{i}",
                nixos_id,
            ),
        )
        package_ids.append(result[0]["id"])

    cfServer.log(f"Created NixOS system with {len(package_ids)} package dependencies")

    # Check that system is NOT in buildable queue (packages incomplete)
    buildable_systems = cf_client.execute_sql(
        """
        SELECT id, derivation_name, build_type 
        FROM view_buildable_derivations 
        WHERE build_type = 'system' AND id = %s
        """,
        (nixos_id,),
    )

    assert (
        len(buildable_systems) == 0
    ), "System should NOT be buildable while packages are incomplete"
    cfServer.log("✅ System correctly blocked from building (packages incomplete)")

    # Mark first 2 packages as complete
    for pkg_id in package_ids[:2]:
        cf_client.execute_sql(
            "UPDATE derivations SET status_id = 10, store_path = %s WHERE id = %s",
            (f"/nix/store/completed-{pkg_id}", pkg_id),
        )

    # System should still be blocked
    buildable_systems = cf_client.execute_sql(
        "SELECT id FROM view_buildable_derivations WHERE build_type = 'system' AND id = %s",
        (nixos_id,),
    )
    assert (
        len(buildable_systems) == 0
    ), "System should still be blocked (1 package remaining)"

    # Complete the last package
    cf_client.execute_sql(
        "UPDATE derivations SET status_id = 10, store_path = %s WHERE id = %s",
        (f"/nix/store/completed-{package_ids[2]}", package_ids[2]),
    )

    # NOW system should be buildable
    buildable_systems = cf_client.execute_sql(
        """
        SELECT id, derivation_name, total_packages, completed_packages
        FROM view_buildable_derivations 
        WHERE build_type = 'system' AND id = %s
        """,
        (nixos_id,),
    )

    assert (
        len(buildable_systems) == 1
    ), "System should NOW be buildable (all packages complete)"
    system = buildable_systems[0]
    assert (
        system["total_packages"] == system["completed_packages"]
    ), "All packages should be marked complete"

    cfServer.log(
        f"✅ System became buildable after all {system['total_packages']} packages completed"
    )

    # Cleanup
    for pkg_id in package_ids:
        cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (pkg_id,))
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (nixos_id,))


def test_worker_heartbeat_updates(cf_client, cfServer, test_flake_data):
    """Test that worker heartbeats are properly updated"""
    commit_id = test_flake_data["commit_ids"][0]

    # Create a derivation and reservation
    result = cf_client.execute_sql(
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, derivation_path,
               scheduled_at, pname, version, status_id
           ) VALUES (
               %s, 'package', 'heartbeat-test', '/nix/store/heartbeat-test.drv',
               NOW(), 'heartbeat-test', '1.0', 8
           ) RETURNING id""",
        (commit_id,),
    )
    derivation_id = result[0]["id"]

    worker_id = "heartbeat-test-worker"
    cf_client.execute_sql(
        """INSERT INTO build_reservations (worker_id, derivation_id, reserved_at, heartbeat_at)
           VALUES (%s, %s, NOW() - INTERVAL '2 minutes', NOW() - INTERVAL '2 minutes')""",
        (worker_id, derivation_id),
    )

    # Get initial heartbeat
    initial = cf_client.execute_sql(
        "SELECT heartbeat_at FROM build_reservations WHERE worker_id = %s",
        (worker_id,),
    )
    initial_heartbeat = initial[0]["heartbeat_at"]

    cfServer.log(f"Initial heartbeat: {initial_heartbeat}")

    # Wait a moment
    time.sleep(2)

    # Update heartbeat
    cf_client.execute_sql(
        "UPDATE build_reservations SET heartbeat_at = NOW() WHERE worker_id = %s",
        (worker_id,),
    )

    # Verify heartbeat was updated
    updated = cf_client.execute_sql(
        "SELECT heartbeat_at FROM build_reservations WHERE worker_id = %s",
        (worker_id,),
    )
    updated_heartbeat = updated[0]["heartbeat_at"]

    cfServer.log(f"Updated heartbeat: {updated_heartbeat}")

    assert (
        updated_heartbeat > initial_heartbeat
    ), "Heartbeat should be newer after update"
    cfServer.log("✅ Worker heartbeat successfully updated")

    # Cleanup
    cf_client.execute_sql(
        "DELETE FROM build_reservations WHERE worker_id = %s", (worker_id,)
    )
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_reservation_prevents_double_claiming(cf_client, cfServer, test_flake_data):
    """Test that FOR UPDATE SKIP LOCKED prevents double-claiming"""
    commit_id = test_flake_data["commit_ids"][0]

    # Create a single derivation
    result = cf_client.execute_sql(
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, derivation_path,
               scheduled_at, pname, version, status_id
           ) VALUES (
               %s, 'package', 'double-claim-test', '/nix/store/double-claim.drv',
               NOW(), 'double-claim-test', '1.0', 5
           ) RETURNING id""",
        (commit_id,),
    )
    derivation_id = result[0]["id"]

    # First worker claims it
    result1 = cf_client.execute_sql(
        """
        WITH next_work AS (
            SELECT id FROM view_buildable_derivations WHERE id = %s
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        INSERT INTO build_reservations (worker_id, derivation_id, nixos_derivation_id)
        SELECT 'worker-1', id, NULL FROM next_work
        RETURNING derivation_id
        """,
        (derivation_id,),
    )

    assert len(result1) == 1, "First worker should successfully claim"
    cfServer.log("Worker 1 claimed the derivation")

    # Mark as in-progress
    cf_client.execute_sql(
        "UPDATE derivations SET status_id = 8 WHERE id = %s", (derivation_id,)
    )

    # Second worker tries to claim the same derivation (should fail/skip)
    result2 = cf_client.execute_sql(
        """
        WITH next_work AS (
            SELECT id FROM view_buildable_derivations WHERE id = %s
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        INSERT INTO build_reservations (worker_id, derivation_id, nixos_derivation_id)
        SELECT 'worker-2', id, NULL FROM next_work
        RETURNING derivation_id
        """,
        (derivation_id,),
    )

    assert (
        len(result2) == 0
    ), "Second worker should NOT be able to claim (already reserved)"
    cfServer.log("✅ Second worker correctly prevented from double-claiming")

    # Verify only one reservation exists
    reservations = cf_client.execute_sql(
        "SELECT worker_id FROM build_reservations WHERE derivation_id = %s",
        (derivation_id,),
    )

    assert len(reservations) == 1, "Should only have one reservation"
    assert (
        reservations[0]["worker_id"] == "worker-1"
    ), "Reservation should belong to first worker"

    # Cleanup
    cf_client.execute_sql(
        "DELETE FROM build_reservations WHERE derivation_id = %s", (derivation_id,)
    )
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))


def test_queue_views_exist(cf_client, cfServer):
    """Test that required database views exist"""
    views = [
        "view_buildable_derivations",
        "view_build_queue_status",
    ]

    for view_name in views:
        result = cf_client.execute_sql(
            """
            SELECT COUNT(*) as count FROM information_schema.views 
            WHERE table_name = %s
            """,
            (view_name,),
        )

        assert result[0]["count"] == 1, f"View {view_name} should exist"
        cfServer.log(f"✅ View {view_name} exists")


def test_build_reservations_table_structure(cf_client, cfServer):
    """Test that build_reservations table has correct structure"""
    # Check table exists
    result = cf_client.execute_sql(
        """
        SELECT COUNT(*) as count FROM information_schema.tables 
        WHERE table_name = 'build_reservations'
        """
    )
    assert result[0]["count"] == 1, "build_reservations table should exist"

    # Check required columns
    required_columns = [
        "worker_id",
        "derivation_id",
        "nixos_derivation_id",
        "reserved_at",
        "heartbeat_at",
    ]

    columns = cf_client.execute_sql(
        """
        SELECT column_name FROM information_schema.columns 
        WHERE table_name = 'build_reservations'
        """
    )

    column_names = {col["column_name"] for col in columns}

    for col in required_columns:
        assert col in column_names, f"Column {col} should exist in build_reservations"
        cfServer.log(f"✅ Column {col} exists")

    cfServer.log("✅ build_reservations table structure is correct")
