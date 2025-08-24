import os

import pytest

from cf_test import CFTestClient, CFTestConfig


@pytest.fixture(scope="session")
def cf_config():
    """Crystal Forge test configuration"""
    return CFTestConfig()


@pytest.fixture(scope="session")
def cf_client(cf_config):
    """Crystal Forge test client"""
    client = CFTestClient(cf_config)

    # Verify database connection at start of session
    try:
        client.execute_sql("SELECT 1")
        print("✅ Database connection verified")
    except Exception as e:
        print(f"❌ Database connection failed: {e}")
        pytest.exit("Database connection required for tests", returncode=1)

    return client


@pytest.fixture(scope="function")
def clean_test_data(cf_client):
    """Fixture that cleans up test data after each test"""
    yield cf_client

    # Cleanup any test data that might have been left behind
    try:
        cf_client.execute_sql(
            """
            DELETE FROM agent_heartbeats WHERE system_state_id IN (
                SELECT id FROM system_states WHERE hostname LIKE 'test-%'
            );
            DELETE FROM system_states WHERE hostname LIKE 'test-%';
            DELETE FROM systems WHERE hostname LIKE 'test-%';
            DELETE FROM derivations WHERE derivation_name LIKE 'test-%';
            DELETE FROM commits WHERE git_commit_hash LIKE '%test%';
            DELETE FROM flakes WHERE name LIKE '%test%' OR repo_url LIKE '%test%';
        """
        )
    except:
        pass  # Ignore cleanup errors


def pytest_configure(config):
    """Register custom markers"""
    config.addinivalue_line("markers", "database: Database-related tests")
    config.addinivalue_line("markers", "views: Database view tests")
    config.addinivalue_line("markers", "integration: Integration tests")
    config.addinivalue_line("markers", "smoke: Quick smoke tests")
    config.addinivalue_line("markers", "slow: Tests that take longer to run")
    config.addinivalue_line(
        "markers", "systems_status: Systems status view specific tests"
    )


def pytest_collection_modifyitems(config, items):
    """Modify test items during collection"""
    for item in items:
        # Auto-mark based on filename
        if "systems_status" in item.fspath.basename:
            item.add_marker(pytest.mark.systems_status)
        if "view" in item.fspath.basename:
            item.add_marker(pytest.mark.views)
        if "database" in item.fspath.basename:
            item.add_marker(pytest.mark.database)
