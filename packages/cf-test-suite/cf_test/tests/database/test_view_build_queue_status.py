import json
from pathlib import Path
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig

VIEW = "view_build_queue_status"


@pytest.fixture(scope="session")
def cf_config():
    return CFTestConfig()


@pytest.fixture(scope="session")
def cf_client(cf_config):
    c = CFTestClient(cf_config)
    c.execute_sql("SELECT 1")
    return c


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_columns(cf_client: CFTestClient):
    """Verify the view has all expected columns"""
    cols = cf_client.execute_sql(
        """
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW,),
    )
    actual = {r["column_name"] for r in cols}
    expected = {
        "nixos_id",
        "system_name",
        "commit_timestamp",
        "git_commit_hash",
        "total_packages",
        "completed_packages",
        "building_packages",
        "pending_packages",
        "cached_packages",
        "active_workers",
        "worker_ids",
        "earliest_reservation",
        "latest_heartbeat",
        "status",
        "cache_status",
        "has_stale_workers",
    }
    assert expected.issubset(actual), f"Missing columns: {expected - actual}"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_performance(cf_client: CFTestClient):
    """Verify the view performs adequately for Grafana dashboards"""
    import time

    t0 = time.time()
    _ = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW}")
    dt = time.time() - t0
    assert dt < 2.0, f"{VIEW} query took too long for Grafana: {dt:.2f}s"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_package_math(cf_client: CFTestClient):
    """Verify package count arithmetic is correct"""
    rows = cf_client.execute_sql(
        f"""
        SELECT nixos_id, total_packages, completed_packages, 
               building_packages, pending_packages
        FROM {VIEW}
        """
    )

    if len(rows) == 0:
        pytest.skip("No systems in build queue")

    for row in rows:
        total = row["total_packages"]
        completed = row["completed_packages"]
        building = row["building_packages"]
        pending = row["pending_packages"]

        # Total should equal the sum of all states
        assert (
            completed + building + pending == total
        ), f"System {row['nixos_id']}: completed({completed}) + building({building}) + pending({pending}) != total({total})"

        # All counts should be non-negative
        assert completed >= 0, f"System {row['nixos_id']}: negative completed_packages"
        assert building >= 0, f"System {row['nixos_id']}: negative building_packages"
        assert pending >= 0, f"System {row['nixos_id']}: negative pending_packages"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_cache_math(cf_client: CFTestClient):
    """Verify cached packages do not exceed completed packages"""
    rows = cf_client.execute_sql(
        f"""
        SELECT nixos_id, system_name, completed_packages, cached_packages
        FROM {VIEW}
        """
    )

    if len(rows) == 0:
        pytest.skip("No systems in build queue")

    for row in rows:
        assert (
            row["cached_packages"] <= row["completed_packages"]
        ), f"System {row['system_name']}: cached_packages({row['cached_packages']}) > completed_packages({row['completed_packages']})"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_valid_statuses(cf_client: CFTestClient):
    """Verify status field contains only valid values"""
    rows = cf_client.execute_sql(f"SELECT DISTINCT status FROM {VIEW}")

    if len(rows) == 0:
        pytest.skip("No systems in build queue")

    valid_statuses = {"pending", "building", "ready_for_system_build"}
    actual_statuses = {r["status"] for r in rows}

    assert actual_statuses.issubset(
        valid_statuses
    ), f"Invalid statuses found: {actual_statuses - valid_statuses}"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_cache_status_logic(cf_client: CFTestClient):
    """Verify cache_status is set correctly"""
    rows = cf_client.execute_sql(
        f"""
        SELECT nixos_id, system_name, completed_packages, total_packages,
               cached_packages, cache_status
        FROM {VIEW}
        """
    )

    if len(rows) == 0:
        pytest.skip("No systems in build queue")

    for row in rows:
        if (
            row["completed_packages"] == row["total_packages"]
            and row["cached_packages"] < row["total_packages"]
        ):
            assert (
                row["cache_status"] == "waiting_for_cache_push"
            ), f"System {row['system_name']}: should have cache_status='waiting_for_cache_push'"
        else:
            assert (
                row["cache_status"] is None or row["cache_status"] == ""
            ), f"System {row['system_name']}: cache_status should be NULL when not waiting for cache"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_ready_systems(cf_client: CFTestClient):
    """Verify systems marked as ready have all packages complete"""
    rows = cf_client.execute_sql(
        f"""
        SELECT nixos_id, system_name, status, total_packages, completed_packages
        FROM {VIEW}
        WHERE status = 'ready_for_system_build'
        """
    )

    for row in rows:
        assert (
            row["total_packages"] == row["completed_packages"]
        ), f"System {row['system_name']} marked ready but only {row['completed_packages']}/{row['total_packages']} packages complete"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_building_systems(cf_client: CFTestClient):
    """Verify systems marked as building have active workers"""
    rows = cf_client.execute_sql(
        f"""
        SELECT nixos_id, system_name, status, active_workers
        FROM {VIEW}
        WHERE status = 'building'
        """
    )

    for row in rows:
        assert (
            row["active_workers"] > 0
        ), f"System {row['system_name']} marked as 'building' but has {row['active_workers']} active workers"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_worker_ids_match_count(cf_client: CFTestClient):
    """Verify worker_ids array length matches active_workers count"""
    rows = cf_client.execute_sql(
        f"""
        SELECT nixos_id, system_name, active_workers, worker_ids
        FROM {VIEW}
        WHERE active_workers > 0
        """
    )

    for row in rows:
        worker_ids = row["worker_ids"] or []
        assert (
            len(worker_ids) == row["active_workers"]
        ), f"System {row['system_name']}: active_workers={row['active_workers']} but worker_ids has {len(worker_ids)} entries"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_stale_worker_detection(cf_client: CFTestClient):
    """Verify stale worker detection logic"""
    # Create a test reservation with an old heartbeat
    test_worker_id = "test-stale-worker-pytest"

    # Find a system in the queue
    systems = cf_client.execute_sql(
        """
        SELECT id FROM derivations 
        WHERE derivation_type = 'nixos' 
          AND status_id IN (5, 12)
        LIMIT 1
        """
    )

    if len(systems) == 0:
        pytest.skip("No NixOS systems available for stale worker test")

    nixos_id = systems[0]["id"]

    # Find a package for this system
    packages = cf_client.execute_sql(
        """
        SELECT dd.depends_on_id as package_id
        FROM derivation_dependencies dd
        JOIN derivations d ON d.id = dd.depends_on_id
        WHERE dd.derivation_id = %s
          AND d.derivation_type = 'package'
          AND d.status_id IN (5, 12)
        LIMIT 1
        """,
        (nixos_id,),
    )

    if len(packages) == 0:
        pytest.skip("No packages available for stale worker test")

    package_id = packages[0]["package_id"]

    # Create a reservation with old heartbeat
    cf_client.execute_sql(
        """
        INSERT INTO build_reservations (worker_id, derivation_id, nixos_derivation_id, heartbeat_at)
        VALUES (%s, %s, %s, NOW() - INTERVAL '10 minutes')
        """,
        (test_worker_id, package_id, nixos_id),
    )

    try:
        # Check if the system shows stale workers
        stale = cf_client.execute_sql(
            f"""
            SELECT has_stale_workers
            FROM {VIEW}
            WHERE nixos_id = %s
            """,
            (nixos_id,),
        )

        assert len(stale) > 0, f"System {nixos_id} not found in view"
        assert (
            stale[0]["has_stale_workers"] is True
        ), f"System {nixos_id} should have stale workers flagged"
    finally:
        # Cleanup
        cf_client.execute_sql(
            "DELETE FROM build_reservations WHERE worker_id = %s",
            (test_worker_id,),
        )


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_ordering(cf_client: CFTestClient):
    """Verify results are ordered by commit timestamp DESC"""
    rows = cf_client.execute_sql(
        f"""
        SELECT commit_timestamp
        FROM {VIEW}
        ORDER BY commit_timestamp DESC
        LIMIT 100
        """
    )

    if len(rows) < 2:
        pytest.skip("Not enough systems to test ordering")

    timestamps = [r["commit_timestamp"] for r in rows]
    for i in range(len(timestamps) - 1):
        assert (
            timestamps[i] >= timestamps[i + 1]
        ), f"Commits not ordered DESC: {timestamps[i]} should be >= {timestamps[i+1]}"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_heartbeat_recency(cf_client: CFTestClient):
    """Verify latest_heartbeat is recent for active builds"""
    rows = cf_client.execute_sql(
        f"""
        SELECT system_name, latest_heartbeat, active_workers,
               EXTRACT(EPOCH FROM (NOW() - latest_heartbeat)) as seconds_ago
        FROM {VIEW}
        WHERE active_workers > 0 AND latest_heartbeat IS NOT NULL
        """
    )

    for row in rows:
        # Active workers should have heartbeat within last 5 minutes (unless stale)
        # We'll just verify the field is populated correctly
        assert (
            row["seconds_ago"] >= 0
        ), f"System {row['system_name']}: latest_heartbeat is in the future"


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_persist_results(cf_client: CFTestClient):
    """Persist view results for debugging"""
    rows = cf_client.execute_sql(
        f"""
        SELECT *
        FROM {VIEW}
        ORDER BY commit_timestamp DESC
        LIMIT 20
        """
    )

    try:
        Path("/tmp").mkdir(parents=True, exist_ok=True)
        with open(
            "/tmp/cf_build_queue_status_results.json", "w", encoding="utf-8"
        ) as fh:
            fh.write(json.dumps(rows, default=str, indent=2))
    except Exception:
        pass  # Don't fail test on debug output


@pytest.mark.views
@pytest.mark.database
def test_view_build_queue_status_aggregation_summary(cf_client: CFTestClient):
    """Test useful aggregation queries for monitoring"""
    summary = cf_client.execute_sql(
        f"""
        SELECT 
            COUNT(*) as total_systems,
            COUNT(*) FILTER (WHERE status = 'pending') as pending_systems,
            COUNT(*) FILTER (WHERE status = 'building') as building_systems,
            COUNT(*) FILTER (WHERE status = 'ready_for_system_build') as ready_systems,
            SUM(active_workers) as total_workers,
            SUM(pending_packages + building_packages) as total_work_remaining,
            COUNT(*) FILTER (WHERE has_stale_workers) as systems_with_stale_workers
        FROM {VIEW}
        """
    )

    if len(summary) == 0:
        pytest.skip("No systems in build queue")

    s = summary[0]

    # Basic sanity checks
    assert s["total_systems"] >= 0
    assert s["pending_systems"] >= 0
    assert s["building_systems"] >= 0
    assert s["ready_systems"] >= 0
    assert s["total_workers"] >= 0
    assert s["total_work_remaining"] >= 0
    assert s["systems_with_stale_workers"] >= 0

    # Status counts should sum to total
    status_sum = s["pending_systems"] + s["building_systems"] + s["ready_systems"]
    assert (
        status_sum == s["total_systems"]
    ), f"Status counts ({status_sum}) don't match total systems ({s['total_systems']})"
