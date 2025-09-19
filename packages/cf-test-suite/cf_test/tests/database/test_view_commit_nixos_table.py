import json
from pathlib import Path
from typing import Any, Dict, List, Tuple

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import (
    _create_base_scenario,
    scenario_agent_restart,
    scenario_behind,
    scenario_build_timeout,
    scenario_compliance_drift,
    scenario_eval_failed,
    scenario_flaky_agent,
    scenario_latest_with_two_overdue,
    scenario_mixed_commit_lag,
    scenario_multi_system_progression_with_failure,
    scenario_multiple_orphaned_systems,
    scenario_never_seen,
    scenario_offline,
    scenario_orphaned_deployments,
    scenario_partial_rebuild,
    scenario_progressive_system_updates,
    scenario_rollback,
    scenario_up_to_date,
)

VIEW = "view_commit_nixos_table"


NIXOS_TABLE_SCENARIO_CONFIGS: List[Dict[str, Any]] = [
    {
        "id": "agent_restart",
        "builder": scenario_agent_restart,
        "expected": {"expect_complete": True},
    },
    {
        "id": "build_timeout",
        "builder": scenario_build_timeout,
        "expected": {"expect_in_progress": True, "exact_totals": (1, 0, 0, 1)},
    },
    {
        "id": "rollback",
        "builder": scenario_rollback,
        "expected": {"min_commits": 2, "expect_complete": True},
    },
    {
        "id": "partial_rebuild",
        "builder": scenario_partial_rebuild,
        "expected": {"expect_complete": True},
    },
    {
        "id": "compliance_drift",
        "builder": scenario_compliance_drift,
        "expected": {"min_commits": 1, "expect_complete": True},
    },
    {
        "id": "flaky_agent",
        "builder": scenario_flaky_agent,
        "expected": {"expect_complete": True},
    },
    {
        "id": "up_to_date",
        "builder": scenario_up_to_date,
        "expected": {"expect_complete": True},
    },
    {
        "id": "behind",
        "builder": scenario_behind,
        "expected": {"min_commits": 1, "expect_complete": True},
    },
    {
        "id": "eval_failed",
        "builder": scenario_eval_failed,
        "expected": {"expect_failed": True, "expect_complete": True},
    },
    {
        "id": "never_seen",
        "builder": scenario_never_seen,
        "expected": {"expect_complete": True},
    },
    {
        "id": "offline",
        "builder": scenario_offline,
        "expected": {"expect_complete": True},
    },
    {
        "id": "progressive_system_updates",
        "builder": scenario_progressive_system_updates,
        "expected": {"min_commits": 5, "expect_complete": True},
    },
    {
        "id": "multiple_orphaned_systems",
        "builder": scenario_multiple_orphaned_systems,
        "expected": {"min_commits": 3, "expect_complete": True},
    },
    {
        "id": "latest_with_two_overdue",
        "builder": scenario_latest_with_two_overdue,
        "expected": {"min_commits": 2, "expect_complete": True},
    },
    {
        "id": "mixed_commit_lag",
        "builder": scenario_mixed_commit_lag,
        "expected": {"min_commits": 2, "expect_complete": True},
    },
    {
        "id": "multi_system_progression_with_failure",
        "builder": scenario_multi_system_progression_with_failure,
        "expected": {"min_commits": 10, "expect_complete": True},
    },
    {
        "id": "orphaned_deployments",
        "builder": scenario_orphaned_deployments,
        "expected": {"min_commits": 1},
    },
]


def _get_commit_hashes_from_cleanup(scenario_data: Dict[str, Any]) -> List[str]:
    cleanup = scenario_data.get("cleanup", {})
    commit_patterns = cleanup.get("commits", [])
    hashes: List[str] = []
    for pattern in commit_patterns:
        if "git_commit_hash" in pattern and " IN " in pattern:
            start = pattern.find("(") + 1
            end = pattern.find(")")
            if start > 0 and end > start:
                for h in pattern[start:end].split(","):
                    hh = h.strip().strip("'\"")
                    if hh:
                        hashes.append(hh)
        elif "git_commit_hash" in pattern and "=" in pattern:
            parts = pattern.split("=")
            if len(parts) >= 2:
                hh = parts[1].strip().strip("'\"")
                if hh and not hh.startswith("IN"):
                    hashes.append(hh)
    # Some scenarios stash commit hashes directly
    for key in ("git_hash", "current_commit_hash", "latest_commit_hash"):
        if key in scenario_data and scenario_data[key]:
            hashes.append(scenario_data[key])
    if "commit_data" in scenario_data:
        hashes.extend([c["hash"] for c in scenario_data["commit_data"] if "hash" in c])
    return list(dict.fromkeys(hashes))  # dedupe, preserve order


def _query_commit_level_rows(
    cf_client: CFTestClient,
    hashes: List[str],
    flake_name: str | None = None,
    hostname: str | None = None,
) -> List[Dict[str, Any]]:
    if hashes:
        return cf_client.execute_sql(
            f"""
            SELECT DISTINCT commit_id, git_commit_hash, short_hash, commit_timestamp, flake_name,
                            total, successful, failed, in_progress, progress_pct
            FROM {VIEW}
            WHERE git_commit_hash = ANY(%s)
            ORDER BY commit_timestamp DESC
            """,
            (hashes,),
        )
    if flake_name:
        return cf_client.execute_sql(
            f"""
            SELECT DISTINCT commit_id, git_commit_hash, short_hash, commit_timestamp, flake_name,
                            total, successful, failed, in_progress, progress_pct
            FROM {VIEW}
            WHERE flake_name = %s
            ORDER BY commit_timestamp DESC
            """,
            (flake_name,),
        )
    if hostname:
        like = f"%{hostname}%"
        return cf_client.execute_sql(
            f"""
            SELECT DISTINCT commit_id, git_commit_hash, short_hash, commit_timestamp, flake_name,
                            total, successful, failed, in_progress, progress_pct
            FROM {VIEW}
            WHERE derivation_name LIKE %s
            ORDER BY commit_timestamp DESC
            """,
            (like,),
        )
    return []


@pytest.fixture(scope="session")
def cf_config():
    return CFTestConfig()


@pytest.fixture(scope="session")
def cf_client(cf_config):
    c = CFTestClient(cf_config)
    c.execute_sql("SELECT 1")
    return c


@pytest.mark.vm_internal
@pytest.mark.views
@pytest.mark.database
@pytest.mark.parametrize(
    "scenario_config", NIXOS_TABLE_SCENARIO_CONFIGS, ids=lambda x: x["id"]
)
def test_view_commit_nixos_table_with_scenarios(
    cf_client: CFTestClient, clean_test_data, scenario_config: Dict[str, Any]
):
    builder = scenario_config["builder"]
    expected = scenario_config["expected"]
    sc = builder(cf_client)

    commit_hashes = _get_commit_hashes_from_cleanup(sc)

    flake_name = None
    if "flake_id" in sc and sc["flake_id"]:
        rows = cf_client.execute_sql(
            "SELECT name FROM public.flakes WHERE id=%s", (sc["flake_id"],)
        )
        if rows:
            flake_name = rows[0]["name"]

    hostname = sc.get("hostname")

    rows = _query_commit_level_rows(
        cf_client, commit_hashes, flake_name=flake_name, hostname=hostname
    )

    # Persist for debugging
    try:
        Path("/tmp").mkdir(parents=True, exist_ok=True)
        with open("/tmp/cf_nixos_table_results.json", "a", encoding="utf-8") as fh:
            fh.write(
                json.dumps(
                    {
                        "scenario": scenario_config["id"],
                        "hashes": commit_hashes,
                        "rows": rows,
                    },
                    default=str,
                )
                + "\n"
            )
    except Exception:
        pass

    assert (
        len(rows) > 0
    ), f"No nixos rows for scenario {scenario_config['id']} (hashes={commit_hashes}, flake={flake_name})"

    if "min_commits" in expected:
        commit_ids = {r["commit_id"] for r in rows}
        assert (
            len(commit_ids) >= expected["min_commits"]
        ), f"{scenario_config['id']}: expected at least {expected['min_commits']} commits, got {len(commit_ids)}"

    if expected.get("expect_complete"):
        complete = [r for r in rows if r["total"] > 0 and r["successful"] == r["total"]]
        assert (
            len(complete) > 0
        ), f"{scenario_config['id']}: expected at least one complete nixos build"

    if expected.get("expect_in_progress"):
        building = [r for r in rows if r["in_progress"] and r["in_progress"] > 0]
        assert (
            len(building) > 0
        ), f"{scenario_config['id']}: expected in-progress nixos builds"

    if expected.get("expect_failed"):
        failed_only = [r for r in rows if r["failed"] > 0 and r["successful"] == 0]
        assert (
            len(failed_only) > 0
        ), f"{scenario_config['id']}: expected a failed nixos build commit"

    if "exact_totals" in expected:
        t, s, f, ip = expected["exact_totals"]
        # match any commit with these exact totals
        exact = [
            r
            for r in rows
            if (r["total"], r["successful"], r["failed"], r["in_progress"])
            == (t, s, f, ip)
        ]
        assert (
            len(exact) > 0
        ), f"{scenario_config['id']}: expected a commit with totals={expected['exact_totals']}, got {[ (r['total'],r['successful'],r['failed'],r['in_progress']) for r in rows ]}"


@pytest.mark.views
@pytest.mark.database
def test_view_commit_nixos_table_columns(cf_client: CFTestClient):
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
        "commit_id",
        "git_commit_hash",
        "short_hash",
        "commit_timestamp",
        "flake_name",
        "derivation_name",
        "derivation_status",
        "status_order",
        "total",
        "successful",
        "failed",
        "in_progress",
        "progress_pct",
    }
    assert expected.issubset(actual), f"Missing columns: {expected - actual}"


@pytest.mark.views
@pytest.mark.database
def test_view_commit_nixos_table_performance(cf_client: CFTestClient):
    import time

    t0 = time.time()
    _ = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW}")
    dt = time.time() - t0
    assert dt < 15.0, f"{VIEW} query took too long: {dt:.2f}s"
