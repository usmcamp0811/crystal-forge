import pytest


@pytest.mark.smoke
@pytest.mark.database
def test_database_connectivity(cf_client):
    """Smoke test: basic database connectivity"""
    result = cf_client.execute_sql("SELECT 1 as test, NOW() as timestamp")
    assert len(result) == 1
    assert result[0]["test"] == 1
    assert result[0]["timestamp"] is not None


@pytest.mark.smoke
@pytest.mark.views
def test_systems_status_view_exists(cf_client):
    """Smoke test: verify systems status view exists"""
    # This should not raise an exception
    result = cf_client.execute_sql(
        "SELECT COUNT(*) as count FROM view_systems_status_table"
    )
    assert "count" in result[0]


@pytest.mark.smoke
@pytest.mark.views
def test_basic_view_structure(cf_client):
    """Smoke test: verify view has expected basic structure"""
    result = cf_client.execute_sql(
        """
        SELECT column_name 
        FROM information_schema.columns 
        WHERE table_name = 'view_systems_status_table'
        ORDER BY ordinal_position
    """
    )

    columns = [row["column_name"] for row in result]

    # Check for essential columns
    required_columns = [
        "hostname",
        "connectivity_status",
        "update_status",
        "overall_status",
    ]
    for col in required_columns:
        assert col in columns, f"Missing required column: {col}"
