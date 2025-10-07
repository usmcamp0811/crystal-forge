import json
from pathlib import Path
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig

VIEW = "view_buildable_derivations"


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
def test_view_buildable_derivations_columns(cf_client: CFTestClient):
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
        "id",
        "derivation_name",
        "derivation_type",
        "derivation_path",
        "pname",
        "version",
        "status_id",
        "nixos_id",
        "nixos_commit_ts",
        "total_packages",
        "completed_packages",
        "cached_packages",
        "active_workers",
        "build_type",
        "queue_position",
    }
    assert expected.issubset(actual), f"Missing columns: {expected - actual}"


@pytest.mark.views
@pytest.mark.database
def test_view_buildable_derivations_performance(cf_client: CFTestClient):
    """Verify the view performs adequately under load"""
    import time

    t0 = time.time()
    _ = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW}")
    dt = time.time() - t0
    assert dt < 5.0, f"{VIEW} query took too long: {dt:.2f}s"


@pytest.mark.views
@pytest.mark.database
def test_view_buildable_derivations_ordering(cf_client: CFTestClient):
    """Verify queue_position is sequential and ordering is correct"""
    rows = cf_client.execute_sql(
        f"""
        SELECT queue_position, nixos_commit_ts, total_packages, build_type
        FROM {VIEW}
        ORDER BY queue_position
        LIMIT 100
        """
    )

    if len(rows) == 0:
        pytest.skip("No buildable derivations in queue")

    # Verify queue_position is sequential starting at 1
    positions = [r["queue_position"] for r in rows]
    assert positions == list(
        range(1, len(rows) + 1)
    ), "queue_position should be sequential"

    # Verify ordering: newest commits first
    timestamps = [r["nixos_commit_ts"] for r in rows if r["nixos_commit_ts"]]
    if len(timestamps) > 1:
        for i in range(len(timestamps) - 1):
            assert (
                timestamps[i] >= timestamps[i + 1]
            ), f"Commits not ordered DESC: {timestamps[i]} should be >= {timestamps[i+1]}"


@pytest.mark.views
@pytest.mark.database
def test_view_buildable_derivations_no_reserved(cf_client: CFTestClient):
    """Verify view excludes derivations that are already reserved"""
    # Create a test reservation
    test_worker_id = "test-worker-pytest"

    # Get a derivation from the view
    rows = cf_client.execute_sql(f"SELECT id, nixos_id FROM {VIEW} LIMIT 1")

    if len(rows) == 0:
        pytest.skip("No buildable derivations available for testing")

    derivation_id = rows[0]["id"]
    nixos_id = rows[0]["nixos_id"]

    # Create a reservation
    cf_client.execute_sql(
        """
        INSERT INTO build_reservations (worker_id, derivation_id, nixos_derivation_id)
        VALUES (%s, %s, %s)
        """,
        (test_worker_id, derivation_id, nixos_id),
    )

    try:
        # Verify the derivation no longer appears in the view
        reserved = cf_client.execute_sql(
            f"SELECT id FROM {VIEW} WHERE id = %s",
            (derivation_id,),
        )
        assert (
            len(reserved) == 0
        ), f"Reserved derivation {derivation_id} should not appear in buildable view"
    finally:
        # Cleanup
        cf_client.execute_sql(
            "DELETE FROM build_reservations WHERE worker_id = %s",
            (test_worker_id,),
        )


@pytest.mark.views
@pytest.mark.database
def test_view_buildable_derivations_build_types(cf_client: CFTestClient):
    """Verify build_type field is either 'package' or 'system'"""
    rows = cf_client.execute_sql(f"SELECT DISTINCT build_type FROM {VIEW}")

    if len(rows) == 0:
        pytest.skip("No buildable derivations in queue")

    build_types = {r["build_type"] for r in rows}
    assert build_types.issubset(
        {"package", "system"}
    ), f"Invalid build_types: {build_types - {'package', 'system'}}"


@pytest.mark.views
@pytest.mark.database
def test_view_buildable_derivations_system_readiness(cf_client: CFTestClient):
    """Verify systems only appear when all packages are complete"""
    rows = cf_client.execute_sql(
        f"""
        SELECT id, build_type, total_packages, completed_packages
        FROM {VIEW}
        WHERE build_type = 'system'
        """
    )

    for row in rows:
        assert (
            row["total_packages"] == row["completed_packages"]
        ), f"System {row['id']} in queue but only {row['completed_packages']}/{row['total_packages']} packages complete"


@pytest.mark.views
@pytest.mark.database
def test_view_buildable_derivations_progress_tracking(cf_client: CFTestClient):
    """Verify progress tracking fields are accurate"""
    rows = cf_client.execute_sql(
        f"""
        SELECT nixos_id, total_packages, completed_packages, cached_packages, active_workers
        FROM {VIEW}
        GROUP BY nixos_id, total_packages, completed_packages, cached_packages, active_workers
        """
    )

    if len(rows) == 0:
        pytest.skip("No buildable derivations in queue")

    for row in rows:
        # Completed packages should not exceed total
        assert (
            row["completed_packages"] <= row["total_packages"]
        ), f"System {row['nixos_id']}: completed_packages ({row['completed_packages']}) > total_packages ({row['total_packages']})"

        # Cached packages should not exceed completed
        assert (
            row["cached_packages"] <= row["completed_packages"]
        ), f"System {row['nixos_id']}: cached_packages ({row['cached_packages']}) > completed_packages ({row['completed_packages']})"

        # Active workers should be non-negative
        assert (
            row["active_workers"] >= 0
        ), f"System {row['nixos_id']}: negative active_workers ({row['active_workers']})"


@pytest.mark.views
@pytest.mark.database
def test_view_buildable_derivations_status_filter(cf_client: CFTestClient):
    """Verify only appropriate statuses appear in the view"""
    rows = cf_client.execute_sql(
        f"""
        SELECT DISTINCT d.status_id, ds.name
        FROM {VIEW} v
        JOIN derivations d ON d.id = v.id
        JOIN derivation_statuses ds ON ds.id = d.status_id
        """
    )

    if len(rows) == 0:
        pytest.skip("No buildable derivations in queue")

    valid_status_ids = {5, 12}  # DryRunComplete, Scheduled
    actual_status_ids = {r["status_id"] for r in rows}

    assert actual_status_ids.issubset(
        valid_status_ids
    ), f"Invalid status_ids in view: {actual_status_ids - valid_status_ids}"


@pytest.mark.views
@pytest.mark.database
def test_view_buildable_derivations_attempt_count_filter(cf_client: CFTestClient):
    """Verify derivations with too many attempts are excluded"""
    rows = cf_client.execute_sql(
        f"""
        SELECT d.attempt_count
        FROM {VIEW} v
        JOIN derivations d ON d.id = v.id
        """
    )

    if len(rows) == 0:
        pytest.skip("No buildable derivations in queue")

    for row in rows:
        assert (
            row["attempt_count"] <= 5
        ), f"Derivation with attempt_count={row['attempt_count']} should not be in buildable view"


@pytest.mark.views
@pytest.mark.database
def test_view_buildable_derivations_smallest_first_within_commit(
    cf_client: CFTestClient,
):
    """Verify smaller systems are prioritized within the same commit"""
    rows = cf_client.execute_sql(
        f"""
        SELECT nixos_commit_ts, total_packages, queue_position
        FROM {VIEW}
        ORDER BY queue_position
        LIMIT 50
        """
    )

    if len(rows) < 2:
        pytest.skip("Not enough derivations to test ordering")

    # Group by commit timestamp
    by_commit: Dict[Any, List[int]] = {}
    for row in rows:
        ts = row["nixos_commit_ts"]
        if ts not in by_commit:
            by_commit[ts] = []
        by_commit[ts].append(row["total_packages"])

    # Within each commit, verify packages are non-decreasing (smallest first)
    for commit_ts, packages in by_commit.items():
        if len(packages) > 1:
            for i in range(len(packages) - 1):
                assert (
                    packages[i] <= packages[i + 1]
                ), f"Commit {commit_ts}: packages not sorted ASC within commit: {packages[i]} > {packages[i+1]}"


@pytest.mark.views
@pytest.mark.database
def test_view_buildable_derivations_persist_results(cf_client: CFTestClient):
    """Persist view results for debugging"""
    rows = cf_client.execute_sql(
        f"""
        SELECT *
        FROM {VIEW}
        ORDER BY queue_position
        LIMIT 20
        """
    )

    try:
        Path("/tmp").mkdir(parents=True, exist_ok=True)
        with open(
            "/tmp/cf_buildable_derivations_results.json", "w", encoding="utf-8"
        ) as fh:
            fh.write(json.dumps(rows, default=str, indent=2))
    except Exception:
        pass  # Don't fail test on debug output
