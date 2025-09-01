import time
from typing import List, Tuple

import pytest

from cf_test.vm_helpers import SmokeTestConstants as C

# Marks: VM-only, integration against the running server loop
pytestmark = [
    pytest.mark.vm_only,
    pytest.mark.integration,
    pytest.mark.database,
    pytest.mark.slow,
]


def _get_status_ids(cf_client, names: List[str]) -> dict:
    rows = cf_client.execute_sql(
        "SELECT id, name FROM derivation_statuses WHERE name = ANY(%s)",
        (names,),
    )
    return {r["name"]: r["id"] for r in rows}


def _get_non_terminal_status_ids(
    cf_client, exclude: List[str]
) -> List[Tuple[int, str]]:
    rows = cf_client.execute_sql(
        """
        SELECT id, name
        FROM derivation_statuses
        WHERE is_terminal = FALSE
          AND name <> ALL(%s)
        ORDER BY display_order
        """,
        (exclude,),
    )
    return [(r["id"], r["name"]) for r in rows]


def _insert_test_flake_and_commit(cf_client, ts_tag: int) -> Tuple[int, int]:
    flake = cf_client.execute_sql(
        """
        INSERT INTO flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE SET name=EXCLUDED.name
        RETURNING id
        """,
        (f"reset-nt-{ts_tag}", f"https://example.com/reset-nt-{ts_tag}.git"),
    )[0]["id"]

    commit = cf_client.execute_sql(
        """
        INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
        VALUES (%s, %s, NOW() - INTERVAL '2 hours', 0)
        RETURNING id
        """,
        (flake, f"timing-{ts_tag}"),
    )[0]["id"]

    return flake, commit


def _insert_derivations_in_states(
    cf_client, commit_id: int, states: List[Tuple[int, str]], ts_tag: int
) -> List[int]:
    ids = []
    for i, (status_id, name) in enumerate(states[:3], start=1):
        # Make updated_at old so they qualify for "stuck" reset logic
        row = cf_client.execute_sql(
            """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, created_at, updated_at
            )
            VALUES (%s, 'nixos', %s, %s, %s, NOW() - INTERVAL '3 hours', NOW() - INTERVAL '90 minutes')
            RETURNING id
            """,
            (
                commit_id,
                f"validate-reset-{ts_tag}-{i}",
                f"/nix/store/{ts_tag:08x}-nixos-system-validate-reset-{i}.drv",
                status_id,
            ),
        )[0]["id"]
        ids.append(row)
    return ids


def _all_now_pending(cf_client, ids: List[int], pending_id: int) -> bool:
    rows = cf_client.execute_sql(
        "SELECT id, status_id FROM derivations WHERE id = ANY(%s)",
        (ids,),
    )
    return all(r["status_id"] == pending_id for r in rows)


@pytest.mark.timeout(240)
def test_non_terminal_derivations_are_reset_to_pending(cf_client, server):
    """
    Integration test:
      1) Seed a flake/commit and a few derivations stuck in non-terminal states.
      2) Let the running server's evaluation loop call `reset_non_terminal_derivations`.
      3) Verify those derivations transition to 'pending'.
    """

    server.wait_for_unit(C.POSTGRES_SERVICE)
    server.wait_for_unit(C.SERVER_SERVICE)
    # Arrange
    ts_tag = int(time.time())

    pending_id = _get_status_ids(cf_client, ["pending"])["pending"]
    non_terms = _get_non_terminal_status_ids(cf_client, exclude=["pending"])
    assert non_terms, "No non-terminal statuses found to test against"

    _, commit_id = _insert_test_flake_and_commit(cf_client, ts_tag)
    target_ids = _insert_derivations_in_states(cf_client, commit_id, non_terms, ts_tag)

    # Precondition: ensure at least one is not 'pending'
    pre = cf_client.execute_sql(
        "SELECT COUNT(*) AS c FROM derivations WHERE id = ANY(%s) AND status_id <> %s",
        (target_ids, pending_id),
    )[0]["c"]
    assert pre > 0, "Setup failed: all test derivations are already pending"

    # Act — wait for the server loop to perform the reset
    # (server service is already running in the VM; just poll)
    deadline = time.time() + 120
    last = None
    while time.time() < deadline:
        if _all_now_pending(cf_client, target_ids, pending_id):
            break
        # Optional: nudge logs for easier debugging
        try:
            server.log("⏳ waiting for reset_non_terminal_derivations …")
        except Exception:
            pass
        time.sleep(2)

    # Assert
    rows = cf_client.execute_sql(
        """
        SELECT id, status_id
        FROM derivations
        WHERE id = ANY(%s)
        ORDER BY id
        """,
        (target_ids,),
    )
    not_pending = [r for r in rows if r["status_id"] != pending_id]
    assert not not_pending, f"Some derivations did not reset to pending: {not_pending}"
