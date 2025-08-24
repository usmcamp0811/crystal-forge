import pytest
from cf_test import CFTestClient, CFTestConfig


# This fixture is available to ALL test files
@pytest.fixture(scope="session")
def cf_client():
    """Provides the Crystal Forge test client to all tests"""
    return CFTestClient()


# Custom markers (test categories)
def pytest_configure(config):
    config.addinivalue_line("markers", "database: Database tests")
    config.addinivalue_line("markers", "smoke: Quick smoke tests")


# Auto-add markers based on filename
def pytest_collection_modifyitems(config, items):
    for item in items:
        if "database" in item.fspath.basename:
            item.add_marker(pytest.mark.database)
