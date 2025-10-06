from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import Dict, List

import pytest

from cf_test import CFTestClient

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


def _parent_of_pkgs(client: CFTestClient, root_ids: List[int]) -> Dict[int, int]:
    if not root_ids:
        return {}
    qmarks = ", ".join(["%s"] * len(root_ids))
    rows = client.execute_sql(
        f"""
        SELECT derivation_id, depends_on_id
        FROM public.derivation_dependencies
        WHERE derivation_id IN ({qmarks})
        """,
        tuple(root_ids),
    )
    return {r["depends_on_id"]: r["derivation_id"] for r in rows}


@pytest.mark.views
@pytest.mark.database
def test_build_queue_single_group_ordering(cf_client: CFTestClient, clean_test_data):
    """
    Create ONE NixOS root + 3 package deps (statuses eligible),
    and assert the view returns exactly those 4, with packages first then nixos.

    NOTE: We MUST enforce the queue order in our SELECT's ORDER BY instead of
    relying on any internal ORDER BY inside the view definition.
    """
    now = datetime.now(UTC)
    # The view filters for status_id IN (5, 12)
    status = _status_id_map(cf_client, ["dry-run-complete", "build-failed"])
    assert status, "Could not resolve status ids"

    test_status_id = status.get("dry-run-complete")
    assert test_status_id is not None, "Could not find dry-run-complete status"

    flake_id = _mk_flake(
        cf_client,
        "validate-nixos-queue-single",
        "https://example.com/validate-nixos-queue-single.git",
    )
    commit_id = _mk_commit(
        cf_client, flake_id, "validate-queue-single", now - timedelta(hours=1)
    )

    # Root
    nixos_id = _mk_derivation(
        cf_client,
        commit_id=commit_id,
        dtype="nixos",
        name="validate-nixos-queue-single",
        path="/nix/store/single-nixos-system.drv",
        status_id=test_status_id,
    )

    # 3 packages
    pkg_ids: List[int] = []
    for i in range(3):
        pkg_id = _mk_derivation(
            cf_client,
            commit_id=commit_id,
            dtype="package",
            name=f"validate-nixos-queue-single-pkg-{i+1}",
            path=f"/nix/store/single-pkg-{i+1}.drv",
            status_id=test_status_id,
        )
        _mk_dep(cf_client, nixos_id, pkg_id)
        pkg_ids.append(pkg_id)

    parent_of = _parent_of_pkgs(cf_client, [nixos_id])

    # Query the view specifically for our rows (by ids, not name pattern)
    ids = [nixos_id] + pkg_ids
    rows = cf_client.execute_sql(
        f"""
        SELECT id, derivation_type, derivation_name, status_id
        FROM {VIEW_NIXOS_QUEUE}
        WHERE id = ANY(%s)
        -- Enforce queue semantics (packages first, then nixos) deterministically.
        ORDER BY
          CASE WHEN derivation_type = 'package' THEN 0 ELSE 1 END,
          id
        """,
        (ids,),
    )

    assert (
        len(rows) == 4
    ), f"Expected 4 rows (3 pkgs + 1 nixos), got {len(rows)}: {rows}"
    assert all(
        r["status_id"] == test_status_id for r in rows
    ), f"Found incorrect status_id in results (expected {test_status_id})"

    # Validate grouping and order:
    def root_of(row):
        return row["id"] if row["derivation_type"] == "nixos" else parent_of[row["id"]]

    group = [r for r in rows if root_of(r) == nixos_id]
    assert (
        len(group) == 4
    ), f"Expected all 4 rows to belong to the same root, got {group}"
    assert all(
        r["derivation_type"] == "package" for r in group[:-1]
    ), f"Non-package found before nixos: {group}"
    assert (
        group[-1]["derivation_type"] == "nixos"
    ), f"Group does not end with nixos row: {group}"
