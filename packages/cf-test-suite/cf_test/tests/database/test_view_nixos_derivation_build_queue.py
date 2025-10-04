from __future__ import annotations

import time
from datetime import UTC, datetime, timedelta
from typing import Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig

VIEW_NIXOS_QUEUE = "view_nixos_derivation_build_queue"


def _status_id_map(client: CFTestClient, names: List[str]) -> Dict[str, int]:
    rows = client.execute_sql(
        """
        SELECT name, id
        FROM public.derivation_statuses
        WHERE name = ANY(%s)
        """,
        (names,),
    )
    return {r["name"]: r["id"] for r in rows}


def _mk_flake(client: CFTestClient, name: str, repo_url: str) -> int:
    [row] = client.execute_sql(
        """
        INSERT INTO public.flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
        RETURNING id
        """,
        (name, repo_url),
    )
    return row["id"]


def _mk_commit(client: CFTestClient, flake_id: int, git_hash: str, ts) -> int:
    [row] = client.execute_sql(
        """
        INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
        VALUES (%s, %s, %s, 0)
        RETURNING id
        """,
        (flake_id, git_hash, ts),
    )
    return row["id"]


def _mk_derivation(
    client: CFTestClient,
    *,
    commit_id: int,
    dtype: str,
    name: str,
    path: str | None,
    status_id: int,
) -> int:
    [row] = client.execute_sql(
        """
        INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
        )
        VALUES (%s, %s, %s, %s, %s, 0, NOW() AT TIME ZONE 'UTC', NOW() AT TIME ZONE 'UTC')
        RETURNING id
        """,
        (commit_id, dtype, name, path, status_id),
    )
    return row["id"]


def _mk_dep(client: CFTestClient, nixos_id: int, pkg_id: int) -> None:
    client.execute_sql(
        """
        INSERT INTO public.derivation_dependencies (derivation_id, depends_on_id)
        VALUES (%s, %s)
        ON CONFLICT DO NOTHING
        """,
        (nixos_id, pkg_id),
    )


@pytest.mark.views
@pytest.mark.database
def test_view_nixos_queue_ordering_and_filter(cf_client: CFTestClient, clean_test_data):
    """
    Build two groups:
      - Newer NixOS root (A) with 3 package deps
      - Older NixOS root (B) with 2 package deps
    Expect:
      - All rows have status_id in allowed set
      - Rows grouped by nixos_id, groups ordered by nixos_commit_ts DESC
      - Within each group: packages first, then the nixos row
    """
    now = datetime.now(UTC)

    # Use status names that your view is intended to include; these usually map to ids 5,12.
    status = _status_id_map(cf_client, ["build-complete", "dry-run-complete"])
    assert status, "Could not resolve status ids for included statuses"

    allowed_ids = set(status.values())

    # Create a dedicated flake/commits namespace that matches conftest cleanup patterns (“validate-%”)
    flake_id = _mk_flake(
        cf_client,
        "validate-nixos-queue",
        "https://example.com/validate-nixos-queue.git",
    )

    # Newer root A
    commit_a = _mk_commit(
        cf_client, flake_id, "validate-queue-A", now - timedelta(hours=1)
    )
    nixos_a_id = _mk_derivation(
        cf_client,
        commit_id=commit_a,
        dtype="nixos",
        name="validate-nixos-queue-A",
        path="/nix/store/aaaaaaaaaaaa-nixos-system-A.drv",
        status_id=status.get("build-complete", list(allowed_ids)[0]),
    )
    pkgs_a = []
    for i in range(3):
        pkg_id = _mk_derivation(
            cf_client,
            commit_id=commit_a,
            dtype="package",
            name=f"validate-nixos-queue-A-pkg-{i+1}",
            path=f"/nix/store/aaa{i+1:02d}-pkg-A-{i+1}.drv",
            status_id=status.get("build-complete", list(allowed_ids)[0]),
        )
        _mk_dep(cf_client, nixos_a_id, pkg_id)
        pkgs_a.append(pkg_id)

    # Older root B
    commit_b = _mk_commit(
        cf_client, flake_id, "validate-queue-B", now - timedelta(hours=5)
    )
    nixos_b_id = _mk_derivation(
        cf_client,
        commit_id=commit_b,
        dtype="nixos",
        name="validate-nixos-queue-B",
        path="/nix/store/bbbbbbbbbbbb-nixos-system-B.drv",
        status_id=status.get("dry-run-complete", list(allowed_ids)[-1]),
    )
    pkgs_b = []
    for i in range(2):
        pkg_id = _mk_derivation(
            cf_client,
            commit_id=commit_b,
            dtype="package",
            name=f"validate-nixos-queue-B-pkg-{i+1}",
            path=f"/nix/store/bbb{i+1:02d}-pkg-B-{i+1}.drv",
            status_id=status.get("dry-run-complete", list(allowed_ids)[-1]),
        )
        _mk_dep(cf_client, nixos_b_id, pkg_id)
        pkgs_b.append(pkg_id)

    # Query the view for just our data
    rows = cf_client.execute_sql(
        f"""
        SELECT id, derivation_type, derivation_name, status_id,
               nixos_id, nixos_commit_ts, group_order
        FROM {VIEW_NIXOS_QUEUE}
        WHERE derivation_name LIKE 'validate-nixos-queue-%'
        ORDER BY nixos_commit_ts DESC, nixos_id, group_order, pname NULLS LAST, id
        """
    )

    # Expect 3 packages + 1 nixos for A, and 2 packages + 1 nixos for B
    assert len(rows) == 7, f"Expected 7 rows, got {len(rows)}"

    # Split rows by nixos_id in the order they appear (should be A group, then B group)
    nixos_ids_in_order = []
    for r in rows:
        if not nixos_ids_in_order or nixos_ids_in_order[-1] != r["nixos_id"]:
            nixos_ids_in_order.append(r["nixos_id"])
    assert nixos_ids_in_order == [
        nixos_a_id,
        nixos_b_id,
    ], f"Group order wrong: {nixos_ids_in_order}"

    # Verify A group: 3 packages (group_order=0) then the nixos row (group_order=1)
    group_a = [r for r in rows if r["nixos_id"] == nixos_a_id]
    assert [r["group_order"] for r in group_a] == [
        0,
        0,
        0,
        1,
    ], f"A group_order wrong: {group_a}"
    assert all(r["derivation_type"] == "package" for r in group_a[:3])
    assert group_a[-1]["derivation_type"] == "nixos"

    # Verify B group: 2 packages then the nixos row
    group_b = [r for r in rows if r["nixos_id"] == nixos_b_id]
    assert [r["group_order"] for r in group_b] == [
        0,
        0,
        1,
    ], f"B group_order wrong: {group_b}"
    assert all(r["derivation_type"] == "package" for r in group_b[:2])
    assert group_b[-1]["derivation_type"] == "nixos"

    # All rows should be in allowed statuses (the view is supposed to filter to status_id IN (5,12))
    bad = [r for r in rows if r["status_id"] not in allowed_ids]
    assert not bad, f"Found rows with disallowed status_id: {bad}"


@pytest.mark.views
@pytest.mark.database
def test_view_nixos_queue_basic_columns(cf_client: CFTestClient):
    """Smoke test: required columns exist on the view."""
    result = cf_client.execute_sql(
        """
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW_NIXOS_QUEUE,),
    )
    have = {r["column_name"] for r in result}

    expected = {
        # derivations columns (subset that we rely on here):
        "id",
        "commit_id",
        "derivation_type",
        "derivation_name",
        "derivation_path",
        "status_id",
        # helper/order columns from the view:
        "nixos_id",
        "nixos_commit_ts",
        "group_order",
    }
    missing = expected - have
    assert not missing, f"View missing expected columns: {missing}"


@pytest.mark.views
@pytest.mark.database
def test_view_nixos_queue_performance(cf_client: CFTestClient):
    """The view should be reasonably quick to aggregate."""
    start = time.time()
    _ = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW_NIXOS_QUEUE}")
    elapsed = time.time() - start
    assert elapsed < 10.0, f"{VIEW_NIXOS_QUEUE} COUNT(*) took too long: {elapsed:.2f}s"
