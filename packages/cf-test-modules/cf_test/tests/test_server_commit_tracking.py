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


@pytest.mark.commits
def test_flake_initialization_commits(cf_client, server):
    """Test that server initializes flake with the expected number of commits"""
    import os

    # Get expected commit count from environment
    # expected_count = int(os.environ.get("CF_TEST_EXPECTED_COMMIT_COUNT", "15"))
    expected_count = 5
    all_commit_hashes = os.environ.get("CF_TEST_ALL_COMMIT_HASHES", "").split(",")

    print(f"Expecting {expected_count} commits")
    print(f"All commit hashes: {all_commit_hashes}")

    # Wait for each commit to be inserted
    for i in range(expected_count):
        cf_client.wait_for_service_log(
            server,
            C.SERVER_SERVICE,
            "✅ Inserted commit",
            timeout=120,
        )
        print(f"Found commit {i + 1}/{expected_count}")

    # Check database has the expected number of commits
    rows = cf_client.execute_sql("SELECT COUNT(*) as count FROM commits")
    commit_count = rows[0]["count"]
    assert (
        commit_count == expected_count
    ), f"Expected {expected_count} commits in database, found {commit_count}"

    # Optionally verify the commit hashes match
    if (
        all_commit_hashes and all_commit_hashes[0]
    ):  # Check if we have real commit hashes
        rows = cf_client.execute_sql(
            "SELECT git_commit_hash FROM commits ORDER BY commit_timestamp"
        )
        db_hashes = [row["git_commit_hash"] for row in rows]

        # Check that all expected hashes are in the database
        for expected_hash in all_commit_hashes:
            if expected_hash:  # Skip empty strings
                assert (
                    expected_hash in db_hashes
                ), f"Expected commit hash {expected_hash} not found in database"
