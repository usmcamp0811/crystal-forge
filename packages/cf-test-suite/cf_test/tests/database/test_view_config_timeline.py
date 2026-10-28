from datetime import UTC, datetime, timedelta
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import (
    scenario_multiple_orphaned_systems,
    scenario_progressive_system_updates,
)

VIEW_CONFIG_TIMELINE = "view_config_timeline"
VIEW_COMMIT_TIMELINE = "view_commit_deployment_timeline"  # helper mapping (has commit timestamps & flake_name)


@pytest.fixture(scope="session")
def cf_config():
    return CFTestConfig()


@pytest.fixture(scope="session")
def cf_client(cf_config):
    client = CFTestClient(cf_config)
    client.execute_sql("SELECT 1")
    return client


def _parse_count(config_str: str) -> int:
    # "<N> deployed (<short_hash>)§<idx>"
    try:
        return int(config_str.split(" deployed (", 1)[0])
    except Exception:
        return -1


@pytest.mark.views
@pytest.mark.database
def test_config_timeline_counts_from_current_status(
    cf_client: CFTestClient, clean_test_data
):
    """
    scenario_progressive_system_updates creates 3 systems on a 5-commit flake:
      - fast: on newest commit (idx 4)
      - medium: one behind (idx 3)
      - slow: three behind (idx 1)
    Expect counts by commit timestamp: {idx4:1, idx3:1, idx1:1, others:0}

    The view counts only systems whose status is up_to_date, behind, or ahead.
    A status needs a newest deployable build for the system's configuration, so
    the scenario publishes a deployable build for every commit. The statuses are
    asserted first so a fixture that stops producing deployable builds fails
    here, with the status named, instead of as an unexplained zero count.
    """
    sc = scenario_progressive_system_updates(cf_client)

    statuses = {
        r["hostname"]: (r["deployment_status"], r["commits_behind"])
        for r in cf_client.execute_sql(
            """
            SELECT hostname, deployment_status, commits_behind
            FROM view_system_deployment_status
            WHERE hostname = ANY(%s)
            """,
            (sc["hostnames"],),
        )
    }
    assert statuses == {
        "test-progressive-fast": ("up_to_date", 0),
        "test-progressive-medium": ("behind", 1),
        "test-progressive-slow": ("behind", 3),
    }, f"scenario systems must be deployable, got {statuses}"

    # Resolve flake name and commit timestamps
    [flake] = cf_client.execute_sql(
        "SELECT name FROM public.flakes WHERE id=%s", (sc["flake_id"],)
    )
    flake_name = flake["name"]

    commit_rows = cf_client.execute_sql(
        f"""
        SELECT commit_timestamp AS time
        FROM {VIEW_COMMIT_TIMELINE}
        WHERE flake_name = %s
        ORDER BY time ASC
        """,
        (flake_name,),
    )
    # Oldest..Newest indices 0..4
    assert len(commit_rows) >= 5
    times = [r["time"] for r in commit_rows[-5:]]  # ensure last 5 from this flake

    rows = cf_client.execute_sql(
        f"""
        SELECT time, "Config", flake_name
        FROM {VIEW_CONFIG_TIMELINE}
        WHERE flake_name = %s
        ORDER BY time ASC
        """,
        (flake_name,),
    )
    # keep only the 5 target commits for this flake (same timestamps)
    by_time = {r["time"]: r for r in rows if r["time"] in set(times)}
    assert len(by_time) == 5

    # expected counts by index (ascending time order)
    expected = {4: 1, 3: 1, 1: 1}
    for idx, t in enumerate(times):
        got = _parse_count(by_time[t]["Config"])
        want = expected.get(idx, 0)
        assert got == want, f"commit idx {idx} expected {want}, got {got}"

    # cleanup
    cf_client.cleanup_test_data(sc["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_config_timeline_excludes_known_path_without_deployable_target(
    cf_client: CFTestClient, clean_test_data
):
    """
    A running path that maps to a known commit is still `unknown` when its
    configuration has no deployable build, and `unknown` is not counted.

    This pins the exclusion that motivated modernizing the progressive
    scenario: removing deployability must change the status and zero the
    counts, which is the documented behavior, rather than the count of the
    healthy scenario being edited to match.
    """
    sc = scenario_progressive_system_updates(cf_client)
    cf_client.execute_sql(
        "UPDATE derivations SET cf_agent_enabled = FALSE WHERE id = ANY(%s)",
        (sc["derivation_ids"],),
    )

    rows = cf_client.execute_sql(
        """
        SELECT hostname, deployment_status, current_commit_hash IS NOT NULL AS known
        FROM view_system_deployment_status
        WHERE hostname = ANY(%s)
        """,
        (sc["hostnames"],),
    )
    assert len(rows) == 3
    for r in rows:
        assert r["known"], f"{r['hostname']} running path must still map to a commit"
        assert r["deployment_status"] == "unknown", r

    [flake] = cf_client.execute_sql(
        "SELECT name FROM public.flakes WHERE id=%s", (sc["flake_id"],)
    )
    timeline = cf_client.execute_sql(
        f"""
        SELECT "Config" FROM {VIEW_CONFIG_TIMELINE}
        WHERE flake_name = %s
        """,
        (flake["name"],),
    )
    assert len(timeline) >= 5
    for r in timeline:
        assert _parse_count(r["Config"]) == 0, r["Config"]

    cf_client.cleanup_test_data(sc["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_config_timeline_unknown_not_counted(cf_client: CFTestClient, clean_test_data):
    """
    scenario_multiple_orphaned_systems creates systems with 'unknown' deployment.
    All commits for that flake should show 0 deployed in view_config_timeline.
    """
    sc = scenario_multiple_orphaned_systems(cf_client)

    [flake] = cf_client.execute_sql(
        "SELECT name FROM public.flakes WHERE id=%s", (sc["flake_id"],)
    )
    flake_name = flake["name"]

    rows = cf_client.execute_sql(
        f"""
        SELECT time, "Config", flake_name
        FROM {VIEW_CONFIG_TIMELINE}
        WHERE flake_name = %s
        ORDER BY time DESC
        """,
        (flake_name,),
    )
    assert len(rows) >= 1

    for r in rows:
        assert (
            _parse_count(r["Config"]) == 0
        ), f"expected 0 deployed for unknown systems, got {r['Config']}"

    cf_client.cleanup_test_data(sc["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_config_timeline_includes_zero_deploy_commits(
    cf_client: CFTestClient, clean_test_data
):
    """
    Ensure commits still appear even when 0 deployed (left join & COALESCE).
    """
    sc = scenario_multiple_orphaned_systems(cf_client)

    [flake] = cf_client.execute_sql(
        "SELECT name FROM public.flakes WHERE id=%s", (sc["flake_id"],)
    )
    flake_name = flake["name"]

    rows = cf_client.execute_sql(
        f"""
        SELECT time, "Config"
        FROM {VIEW_CONFIG_TIMELINE}
        WHERE flake_name = %s
        ORDER BY time DESC
        """,
        (flake_name,),
    )
    # There were 3 commits created in the scenario; at least 3 rows expected.
    assert len(rows) >= 3
    # And each should parse to a non-negative integer (0+)
    for r in rows:
        assert _parse_count(r["Config"]) >= 0

    cf_client.cleanup_test_data(sc["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_config_timeline_basic_columns(cf_client: CFTestClient):
    cols = cf_client.execute_sql(
        """
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW_CONFIG_TIMELINE,),
    )
    actual = {r["column_name"] for r in cols}
    assert {"time", "Config", "flake_name"}.issubset(actual)


@pytest.mark.views
@pytest.mark.database
def test_config_timeline_performance(cf_client: CFTestClient):
    import time

    start = time.time()
    _ = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW_CONFIG_TIMELINE}")
    dt = time.time() - start
    assert dt < 15.0, f"query took too long: {dt:.2f}s"


@pytest.mark.views
@pytest.mark.database
def test_config_timeline_ordering(cf_client: CFTestClient, clean_test_data):
    sc = scenario_progressive_system_updates(cf_client)

    [flake] = cf_client.execute_sql(
        "SELECT name FROM public.flakes WHERE id=%s", (sc["flake_id"],)
    )
    flake_name = flake["name"]

    rows = cf_client.execute_sql(
        f"""
        SELECT time
        FROM {VIEW_CONFIG_TIMELINE}
        WHERE flake_name = %s
        ORDER BY time DESC
        """,
        (flake_name,),
    )
    assert len(rows) >= 2
    for i in range(len(rows) - 1):
        assert rows[i]["time"] >= rows[i + 1]["time"]

    cf_client.cleanup_test_data(sc["cleanup"])
