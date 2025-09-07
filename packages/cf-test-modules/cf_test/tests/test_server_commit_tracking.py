import json
import time
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig
from cf_test.vm_helpers import SmokeTestConstants as C
from cf_test.vm_helpers import SmokeTestData, verify_commits_exist, verify_flake_in_db

pytestmark = pytest.mark.vm_only


@pytest.fixture(scope="session")
def smoke_data():
    return SmokeTestData()


@pytest.fixture(scope="session")
def server():
    import cf_test

    return cf_test._driver_machines["server"]


@pytest.fixture(scope="session")
def agent():
    import cf_test

    return cf_test._driver_machines["agent"]


@pytest.fixture(scope="session")
def gitserver():
    import cf_test

    return cf_test._driver_machines["gitserver"]


@pytest.fixture(scope="session")
def cf_client(cf_config):
    return CFTestClient(cf_config)


@pytest.mark.commits
def test_flake_initialization_commits(cf_client, server):
    """Test that server initializes flake with 5 commits (default initial_commit_depth)"""

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


@pytest.mark.slow
@pytest.mark.commits
def test_flake_polling_picks_up_new_commit(cf_client, server, gitserver):
    """Test that polling picks up a new commit pushed to the git repository"""

    # Wait for initial commits to be processed first
    cf_client.wait_for_service_log(
        server,
        C.SERVER_SERVICE,
        "✅ Successfully initialized 5 commits for",
        timeout=120,
    )

    # Get initial commit count
    initial_rows = cf_client.execute_sql("SELECT COUNT(*) as count FROM commits")
    initial_count = initial_rows[0]["count"]
    print(f"Initial commit count: {initial_count}")

    # Clone the repository on gitserver and make a new commit
    gitserver.succeed("cd /tmp && rm -rf test-clone")
    gitserver.succeed("cd /tmp && git clone /srv/git/crystal-forge.git test-clone")
    gitserver.succeed("cd /tmp/test-clone && git config user.name 'Test User'")
    gitserver.succeed("cd /tmp/test-clone && git config user.email 'test@example.com'")

    # Make a change and commit it
    gitserver.succeed("cd /tmp/test-clone && echo '# Test polling commit' >> flake.nix")
    gitserver.succeed("cd /tmp/test-clone && git add flake.nix")
    gitserver.succeed("cd /tmp/test-clone && git commit -m 'Test polling commit'")

    # Get the new commit hash
    new_commit_hash = gitserver.succeed(
        "cd /tmp/test-clone && git rev-parse HEAD"
    ).strip()
    print(f"Created new commit: {new_commit_hash}")

    # Push the commit back to the bare repository (now writable)
    gitserver.succeed("cd /tmp/test-clone && git push origin main")

    # Wait for the polling interval (1 minute) plus some buffer
    print("Waiting for polling interval (1 minute)...")
    import time

    time.sleep(70)  # Wait 70 seconds to ensure polling happens

    # Wait for the new commit to be processed
    cf_client.wait_for_service_log(
        server, C.SERVER_SERVICE, f"✅ Inserted commit {new_commit_hash}", timeout=30
    )

    # Verify the new commit is in the database
    final_rows = cf_client.execute_sql("SELECT COUNT(*) as count FROM commits")
    final_count = final_rows[0]["count"]
    print(f"Final commit count: {final_count}")

    assert (
        final_count == initial_count + 1
    ), f"Expected {initial_count + 1} commits, got {final_count}"

    # Verify the specific commit hash is in the database
    commit_rows = cf_client.execute_sql(
        "SELECT git_commit_hash FROM commits WHERE git_commit_hash = %s",
        (new_commit_hash,),
    )
    assert len(commit_rows) == 1, f"New commit {new_commit_hash} not found in database"

    print("✅ Polling test passed: new commit was detected and processed")


@pytest.mark.slow
@pytest.mark.commits
def test_webhook_and_commit_ingest(cf_client, server, smoke_data):
    """Test webhook processing and commit ingestion"""
    # Send webhook
    cf_client.send_webhook(server, C.API_PORT, smoke_data.webhook_payload)

    # Wait for webhook processing
    cf_client.wait_for_service_log(
        server, C.SERVER_SERVICE, smoke_data.webhook_commit, timeout=90
    )

    # Verify flake was created
    verify_flake_in_db(cf_client, server, smoke_data.git_server_url)

    # Verify commits were ingested
    verify_commits_exist(cf_client, server)

    # Cleanup webhook test - delete in correct order to respect foreign keys
    cf_client.execute_sql(
        "DELETE FROM commits WHERE flake_id IN (SELECT id FROM flakes WHERE repo_url = %s)",
        (smoke_data.git_server_url,),
    )
    cf_client.execute_sql(
        "DELETE FROM systems WHERE flake_id IN (SELECT id FROM flakes WHERE repo_url = %s)",
        (smoke_data.git_server_url,),
    )
    cf_client.execute_sql(
        "DELETE FROM flakes WHERE repo_url = %s", (smoke_data.git_server_url,)
    )
