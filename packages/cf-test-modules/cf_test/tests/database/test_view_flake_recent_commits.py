from datetime import UTC, datetime, timedelta

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.scenarios import _create_base_scenario

VIEW_RECENT_COMMITS = "view_flake_recent_commits"


@pytest.fixture(scope="session")
def cf_config():
    return CFTestConfig()


@pytest.fixture(scope="session")
def cf_client(cf_config):
    client = CFTestClient(cf_config)
    client.execute_sql("SELECT 1")
    return client


@pytest.mark.views
@pytest.mark.database
def test_view_flake_recent_commits_columns(cf_client: CFTestClient):
    """Ensure the recent commits view has expected columns"""
    rows = cf_client.execute_sql(
        """
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = %s
        """,
        (VIEW_RECENT_COMMITS,),
    )
    actual = {r["column_name"] for r in rows}
    expected = {
        "flake",
        "commit",
        "commit_timestamp",
        "attempt_count",
        "attempt_status",
        "minutes_since_commit",
        "age_interval",
    }
    assert expected.issubset(actual), f"Missing columns: {expected - actual}"


@pytest.mark.views
@pytest.mark.database
def test_view_flake_recent_commits_attempt_status_logic(
    cf_client: CFTestClient, clean_test_data
):
    """Check attempt_status logic (retries vs ok vs failed/stuck)"""
    # Create a commit with attempt_count=6 (over threshold)
    scenario1 = _create_base_scenario(
        cf_client,
        hostname="recent-commits-failed",
        flake_name="recent-commits-test",
        repo_url="https://example.com/recent-commits.git",
        git_hash="recent-commits-123",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )
    commit_id = scenario1["commit_id"]
    cf_client.execute_sql(
        "UPDATE commits SET attempt_count=6 WHERE id=%s", (commit_id,)
    )

    # Query the view
    rows = cf_client.execute_sql(
        f"""
        SELECT commit, attempt_count, attempt_status
        FROM {VIEW_RECENT_COMMITS}
        WHERE flake = %s
        ORDER BY commit_timestamp DESC
        """,
        ("recent-commits-test",),
    )
    assert len(rows) > 0, "Expected at least one row in view"
    row = rows[0]
    assert row["attempt_count"] == 6
    assert (
        row["attempt_status"] == "⚠︎ failed/stuck threshold"
    ), f"Expected stuck status, got {row['attempt_status']}"

    # Clean up
    cf_client.cleanup_test_data(scenario1["cleanup"])


@pytest.mark.views
@pytest.mark.database
def test_view_flake_recent_commits_performance(cf_client: CFTestClient):
    """Ensure the view performs within reasonable bounds"""
    import time

    start = time.time()
    _ = cf_client.execute_sql(f"SELECT COUNT(*) FROM {VIEW_RECENT_COMMITS}")
    dt = time.time() - start
    assert dt < 10.0, f"Recent commits view query took too long: {dt:.2f}s"
