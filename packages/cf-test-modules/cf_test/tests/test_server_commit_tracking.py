import json
import time
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.vm_helpers import SmokeTestConstants as C

pytestmark = pytest.mark.vm_only


@pytest.fixture(scope="session")
def server():
    import cf_test

    return cf_test._driver_machines["server"]


@pytest.fixture(scope="session")
def agent():
    import cf_test

    return cf_test._driver_machines["agent"]


@pytest.fixture(scope="session")
def cf_client(cf_config):
    return CFTestClient(cf_config)


@pytest.mark.vm_only
def test_flake_initialization_commits(cf_client, server):
    """Test that server initializes flake with 5 commits"""

    # Wait for the initialization log message
    cf_client.wait_for_service_log(
        server,
        C.SERVER_SERVICE,
        "✅ Successfully initialized 5 commits for",
        timeout=120,
    )

    # Check database has exactly 5 commits
    rows = cf_client.execute_sql("SELECT COUNT(*) as count FROM commits")
    commit_count = rows[0]["count"]

    assert commit_count == 5, f"Expected 5 commits in database, found {commit_count}"
