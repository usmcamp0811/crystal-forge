import time

import pytest

from cf_test import CFTestClient


@pytest.mark.views
@pytest.mark.database
def test_view_exists_columns(cf_client: CFTestClient):
    expected = {
        "hostname",
        "connectivity_status",
        "connectivity_status_text",
        "update_status",
        "update_status_text",
        "overall_status",
        "last_seen",
        "agent_version",
        "uptime",
        "ip_address",
        "current_derivation_path",
        "latest_commit_hash",
    }
    rows = cf_client.execute_sql(
        """
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = 'view_systems_status_table'
        ORDER BY ordinal_position
    """
    )
    cols = {r["column_name"] for r in rows}
    missing = expected - cols
    assert not missing, f"missing columns: {sorted(missing)}"


@pytest.mark.views
@pytest.mark.database
@pytest.mark.slow
def test_view_performance(cf_client: CFTestClient):
    start = time.time()
    res = cf_client.execute_sql(
        """
        SELECT COUNT(*) AS total_systems,
               COUNT(CASE WHEN connectivity_status = 'online' THEN 1 END) AS online_systems,
               COUNT(CASE WHEN overall_status = 'up_to_date' THEN 1 END) AS up_to_date_systems
        FROM view_systems_status_table
    """
    )
    elapsed = time.time() - start
    assert res and "total_systems" in res[0]
    assert elapsed < 5.0, f"query took {elapsed:.2f}s (expected <5s)"
