import json
import os
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


@pytest.fixture(scope="session")
def branch_test_data():
    """Get branch-specific test data from environment variables"""
    return {
        "main": {
            "commits": os.environ.get("CF_TEST_MAIN_COMMITS", "").split(","),
            "expected_count": int(os.environ.get("CF_TEST_MAIN_COMMIT_COUNT", "5")),
        },
        "development": {
            "commits": os.environ.get("CF_TEST_DEVELOPMENT_COMMITS", "").split(","),
            "expected_count": int(
                os.environ.get("CF_TEST_DEVELOPMENT_COMMIT_COUNT", "7")
            ),
        },
        "feature/experimental": {
            "commits": os.environ.get("CF_TEST_FEATURE_COMMITS", "").split(","),
            "expected_count": int(os.environ.get("CF_TEST_FEATURE_COMMIT_COUNT", "3")),
        },
    }


@pytest.mark.commits
def test_flake_initialization_commits(cf_client, server):
    """Test that server initializes flake with 5 commits (default initial_commit_depth)"""

    # Check if initialization already happened by looking for existing commits
    start_rows = cf_client.execute_sql("SELECT COUNT(*) as count FROM commits")
    start_commit_count = start_rows[0]["count"]

    # If we have no commits yet, wait for initialization
    if start_commit_count == 0:
        cf_client.wait_for_service_log(
            server,
            C.SERVER_SERVICE,
            "Successfully initialized 5 commits for",
            timeout=120,
        )
    else:
        # Check if initialization log already exists (meaning it happened earlier)
        try:
            server.succeed(
                "journalctl -u crystal-forge-server.service | grep 'Successfully initialized 5 commits for'"
            )
        except:
            # If no initialization log found but we have commits, something's wrong
            if start_commit_count > 0:
                server.log(
                    f"Found {start_commit_count} commits but no initialization log"
                )

    # Now check the final count
    rows = cf_client.execute_sql("SELECT COUNT(*) as count FROM commits")
    commit_count = rows[0]["count"]

    # Should have exactly 5 commits from initialization (plus any from other tests)
    assert (
        commit_count >= 5
    ), f"Expected at least 5 commits from initialization, found {commit_count}"

    # If we started with 0, we should have added exactly 5
    if start_commit_count == 0:
        assert (
            commit_count == 5
        ), f"Expected exactly 5 commits after initialization, found {commit_count}"


@pytest.mark.commits
@pytest.mark.parametrize(
    "branch_name,repo_url_suffix",
    [
        ("development", "?ref=development"),
        ("feature/experimental", "?ref=feature/experimental"),
    ],
)
def test_branch_specific_flake_initialization(
    cf_client, server, gitserver, branch_test_data, branch_name, repo_url_suffix
):
    """Test that different branches can be tracked independently"""

    # Create a new flake configuration for this branch
    branch_repo_url = f"http://gitserver/crystal-forge{repo_url_suffix}"
    flake_name = f"crystal-forge-{branch_name.replace('/', '-')}"

    # Insert the branch-specific flake into the database
    cf_client.execute_sql(
        "INSERT INTO flakes (name, repo_url) VALUES (%s, %s) ON CONFLICT (repo_url) DO NOTHING",
        (flake_name, branch_repo_url),
    )

    # Get the flake ID
    flake_rows = cf_client.execute_sql(
        "SELECT id FROM flakes WHERE repo_url = %s", (branch_repo_url,)
    )
    assert len(flake_rows) == 1, f"Could not find flake for {branch_repo_url}"
    flake_id = flake_rows[0]["id"]

    # Trigger manual flake sync by calling the server endpoint or waiting for polling
    # For now, we'll wait for automatic polling to pick it up
    print(f"Waiting for {branch_name} branch commits to be synced...")

    # Wait up to 2 minutes for commits to appear
    timeout = 120
    start_time = time.time()
    expected_count = branch_test_data[branch_name]["expected_count"]

    while time.time() - start_time < timeout:
        commit_rows = cf_client.execute_sql(
            "SELECT COUNT(*) as count FROM commits WHERE flake_id = %s", (flake_id,)
        )
        current_count = commit_rows[0]["count"]

        if current_count >= expected_count:
            break

        print(
            f"Branch {branch_name}: {current_count}/{expected_count} commits found, waiting..."
        )
        time.sleep(5)

    # Final verification
    final_commit_rows = cf_client.execute_sql(
        "SELECT COUNT(*) as count FROM commits WHERE flake_id = %s", (flake_id,)
    )
    final_count = final_commit_rows[0]["count"]

    assert (
        final_count >= expected_count
    ), f"Expected at least {expected_count} commits for {branch_name}, found {final_count}"

    # Verify specific commit hashes are present
    expected_commits = branch_test_data[branch_name]["commits"]
    for commit_hash in expected_commits:
        if commit_hash:  # Skip empty strings
            commit_exists = cf_client.execute_sql(
                "SELECT 1 FROM commits WHERE flake_id = %s AND git_commit_hash = %s",
                (flake_id, commit_hash),
            )
            assert (
                len(commit_exists) == 1
            ), f"Commit {commit_hash} not found for branch {branch_name}"

    print(f"Branch {branch_name} verification passed: {final_count} commits found")


@pytest.mark.commits
def test_branch_isolation(cf_client, server, branch_test_data):
    """Test that commits from different branches are properly isolated"""

    # Get all flakes from the database
    flake_rows = cf_client.execute_sql("SELECT id, name, repo_url FROM flakes")

    branch_flakes = {}
    for row in flake_rows:
        if "crystal-forge" in row["repo_url"]:
            if "ref=development" in row["repo_url"]:
                branch_flakes["development"] = row["id"]
            elif "ref=feature" in row["repo_url"]:
                branch_flakes["feature/experimental"] = row["id"]
            elif "ref=" not in row["repo_url"]:
                branch_flakes["main"] = row["id"]

    # Verify each branch has its expected commits and no cross-contamination
    for branch_name, expected_data in branch_test_data.items():
        if branch_name in branch_flakes:
            flake_id = branch_flakes[branch_name]

            # Get all commits for this branch
            commit_rows = cf_client.execute_sql(
                "SELECT git_commit_hash FROM commits WHERE flake_id = %s", (flake_id,)
            )
            actual_hashes = {row["git_commit_hash"] for row in commit_rows}
            expected_hashes = {h for h in expected_data["commits"] if h}

            # Verify all expected commits are present
            missing_commits = expected_hashes - actual_hashes
            assert (
                not missing_commits
            ), f"Branch {branch_name} missing commits: {missing_commits}"

            # Verify no unexpected commits (commits from other branches shouldn't leak in)
            all_other_expected = set()
            for other_branch, other_data in branch_test_data.items():
                if other_branch != branch_name:
                    all_other_expected.update(h for h in other_data["commits"] if h)

            leaked_commits = actual_hashes & all_other_expected
            assert (
                not leaked_commits
            ), f"Branch {branch_name} has commits from other branches: {leaked_commits}"

            print(
                f"Branch isolation verified for {branch_name}: {len(actual_hashes)} unique commits"
            )


@pytest.mark.slow
@pytest.mark.commits
def test_branch_polling_picks_up_new_commit(cf_client, server, gitserver):
    """Test that polling picks up a new commit pushed to a specific branch"""

    # Use development branch for this test
    branch_name = "development"
    repo_url = f"http://gitserver/crystal-forge?ref={branch_name}"

    # Ensure the branch flake exists
    cf_client.execute_sql(
        "INSERT INTO flakes (name, repo_url) VALUES (%s, %s) ON CONFLICT (repo_url) DO NOTHING",
        (f"crystal-forge-{branch_name}", repo_url),
    )

    # Get initial commit count for this branch
    flake_rows = cf_client.execute_sql(
        "SELECT id FROM flakes WHERE repo_url = %s", (repo_url,)
    )
    assert len(flake_rows) == 1
    flake_id = flake_rows[0]["id"]

    initial_rows = cf_client.execute_sql(
        "SELECT COUNT(*) as count FROM commits WHERE flake_id = %s", (flake_id,)
    )
    initial_count = initial_rows[0]["count"]
    print(f"Initial commit count for {branch_name}: {initial_count}")

    # Clone and work on the specific branch
    gitserver.succeed("cd /tmp && rm -rf test-clone-dev")
    gitserver.succeed(
        f"cd /tmp && git clone -b {branch_name} /srv/git/crystal-forge.git test-clone-dev"
    )
    gitserver.succeed("cd /tmp/test-clone-dev && git config user.name 'Test User'")
    gitserver.succeed(
        "cd /tmp/test-clone-dev && git config user.email 'test@example.com'"
    )

    # Make a change and commit it to the development branch
    gitserver.succeed(
        "cd /tmp/test-clone-dev && echo '# Test development polling commit' >> flake.nix"
    )
    gitserver.succeed("cd /tmp/test-clone-dev && git add flake.nix")
    gitserver.succeed(
        "cd /tmp/test-clone-dev && git commit -m 'Test development polling commit'"
    )

    # Get the new commit hash
    new_commit_hash = gitserver.succeed(
        "cd /tmp/test-clone-dev && git rev-parse HEAD"
    ).strip()
    print(f"Created new commit on {branch_name}: {new_commit_hash}")

    # Push the commit back to the development branch
    gitserver.succeed(f"cd /tmp/test-clone-dev && git push origin {branch_name}")

    # Wait for the polling interval plus buffer
    print("Waiting for polling interval (1 minute)...")
    time.sleep(70)

    # Wait for the new commit to be processed
    cf_client.wait_for_service_log(
        server, C.SERVER_SERVICE, f"Inserted commit {new_commit_hash}", timeout=30
    )

    # Verify the new commit is in the database for the correct branch
    final_rows = cf_client.execute_sql(
        "SELECT COUNT(*) as count FROM commits WHERE flake_id = %s", (flake_id,)
    )
    final_count = final_rows[0]["count"]
    print(f"Final commit count for {branch_name}: {final_count}")

    assert (
        final_count == initial_count + 1
    ), f"Expected {initial_count + 1} commits, got {final_count}"

    # Verify the specific commit hash is in the database for this branch
    commit_rows = cf_client.execute_sql(
        "SELECT git_commit_hash FROM commits WHERE flake_id = %s AND git_commit_hash = %s",
        (flake_id, new_commit_hash),
    )
    assert (
        len(commit_rows) == 1
    ), f"New commit {new_commit_hash} not found for branch {branch_name}"

    print(
        f"Branch polling test passed: new commit on {branch_name} was detected and processed"
    )


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
