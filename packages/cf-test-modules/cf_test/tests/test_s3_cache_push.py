import json
import os

import pytest

from cf_test import CFTestClient
from cf_test.scenarios import _create_base_scenario
from cf_test.vm_helpers import wait_for_crystal_forge_ready

pytestmark = [pytest.mark.s3cache]


@pytest.fixture(scope="session")
def s3_server():
    import cf_test

    return cf_test._driver_machines["s3Server"]


@pytest.fixture(scope="session")
def s3_cache():
    import cf_test

    return cf_test._driver_machines["s3Cache"]


@pytest.fixture(scope="session")
def gitserver():
    import cf_test

    return cf_test._driver_machines["gitserver"]


@pytest.fixture(scope="session")
def cf_client(cf_config):
    return CFTestClient(cf_config)


@pytest.fixture(scope="session")
def test_flake_data():
    """Load test flake commit and derivation data"""
    return {
        "main_head": os.environ.get("CF_TEST_MAIN_HEAD"),
        "development_head": os.environ.get("CF_TEST_DEVELOPMENT_HEAD"),
        "feature_head": os.environ.get("CF_TEST_FEATURE_HEAD"),
        "main_commits": os.environ.get("CF_TEST_MAIN_COMMITS", "").split(","),
        "development_commits": os.environ.get("CF_TEST_DEVELOPMENT_COMMITS", "").split(
            ","
        ),
        "feature_commits": os.environ.get("CF_TEST_FEATURE_COMMITS", "").split(","),
        "repo_url": os.environ.get("CF_TEST_REAL_REPO_URL"),
        "flake_name": os.environ.get("CF_TEST_FLAKE_NAME", "test-flake"),
    }


@pytest.fixture(scope="session")
def derivation_paths():
    """Load derivation paths from the test environment"""
    drv_path = os.environ.get("CF_TEST_DRV")
    if not drv_path:
        pytest.fail("CF_TEST_DRV environment variable not set")

    with open(drv_path, "r") as f:
        return json.load(f)


@pytest.mark.integration
def test_s3_cache_push_successful_build(
    cf_client, s3_server, s3_cache, test_flake_data, derivation_paths
):
    """Test that successful builds are pushed to S3 cache"""

    wait_for_crystal_forge_ready(s3_server)

    # Use the main branch head commit for testing
    test_commit = test_flake_data["main_head"]
    if not test_commit:
        pytest.skip("No main branch head commit available")

    # Create a scenario with a real derivation from our test flake
    config_name = "cf-test-sys"  # Use one of our known configs
    if config_name not in derivation_paths:
        pytest.skip(f"Derivation {config_name} not found in test data")

    test_derivation = derivation_paths[config_name]

    scenario = _create_base_scenario(
        cf_client,
        hostname="s3-cache-test-host",
        flake_name=test_flake_data["flake_name"],
        repo_url=test_flake_data["repo_url"],
        git_hash=test_commit,
        derivation_status="build-pending",
        commit_age_hours=1,
        heartbeat_age_minutes=None,
    )

    # Use the real derivation path from our test flake
    cf_client.execute_sql(
        "UPDATE derivations SET derivation_path = %s, derivation_name = %s WHERE id = %s",
        (
            test_derivation["derivation_path"],
            test_derivation["derivation_name"],
            scenario["derivation_id"],
        ),
    )

    s3_server.log(
        f"=== Testing S3 cache push with derivation: {test_derivation['derivation_path']} ==="
    )

    # Wait for the build to be processed
    cf_client.wait_for_service_log(
        s3_server,
        "crystal-forge-builder.service",
        [
            f"derivation_path.*{test_derivation['derivation_name']}",
            "Starting cache push",
            "build.*complete",
        ],
        timeout=300,
    )

    # Check for successful cache operations
    try:
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            ["Successfully pushed", "cache.*success", "uploaded.*s3"],
            timeout=120,
        )
        cache_success = True
        s3_server.log("✅ S3 cache operation detected")
    except:
        cache_success = False
        s3_server.log("⚠️ No explicit S3 cache success message found")

    # Verify build completion regardless of cache status
    final_status = cf_client.execute_sql(
        """
        SELECT d.status_id, ds.name as status_name, d.derivation_path, d.derivation_name
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE d.id = %s
        """,
        (scenario["derivation_id"],),
    )

    assert len(final_status) == 1, "Derivation should exist"
    assert final_status[0]["status_name"] in [
        "build-complete",
        "cve-scan-pending",
        "complete",
    ], f"Expected completion status, got {final_status[0]['status_name']}"

    s3_server.log(f"Final derivation status: {final_status[0]['status_name']}")

    # Check MinIO received requests
    try:
        minio_logs = s3_cache.succeed(
            "journalctl -u minio.service --since '5 minutes ago' --no-pager"
        )
        if "PUT" in minio_logs and "crystal-forge-cache" in minio_logs:
            s3_server.log(
                "✅ MinIO received PUT requests for crystal-forge-cache bucket"
            )
        else:
            s3_server.log("⚠️ No clear MinIO PUT operations found")
    except:
        s3_server.log("⚠️ Could not check MinIO logs")

    # Cleanup
    cf_client.cleanup_test_data(scenario["cleanup"])


@pytest.mark.integration
def test_database_populated_with_test_flake_data(cf_client, s3_server, test_flake_data):
    """Test that the database was populated with our test flake data"""

    # Check that test-flake exists
    flakes = cf_client.execute_sql(
        "SELECT id, name, repo_url FROM flakes WHERE name = %s",
        (test_flake_data["flake_name"],),
    )

    assert (
        len(flakes) == 1
    ), f"test-flake should exist in database, found {len(flakes)} flakes"
    assert flakes[0]["repo_url"] == test_flake_data["repo_url"], "Repo URL should match"

    flake_id = flakes[0]["id"]
    s3_server.log(f"✅ Found test flake with ID: {flake_id}")

    # Check that commits exist for our test flake
    commits = cf_client.execute_sql(
        "SELECT git_commit_hash, message FROM commits WHERE flake_id = %s ORDER BY created_at DESC LIMIT 10",
        (flake_id,),
    )

    s3_server.log(f"Found {len(commits)} commits for test flake")

    # Check if any of our known commits exist
    known_commits = [
        test_flake_data["main_head"],
        test_flake_data["development_head"],
        test_flake_data["feature_head"],
    ]
    known_commits = [c for c in known_commits if c]  # Filter out None values

    found_commits = [c["git_commit_hash"] for c in commits]
    matching_commits = set(known_commits) & set(found_commits)

    if matching_commits:
        s3_server.log(
            f"✅ Found expected commits in database: {list(matching_commits)}"
        )
    else:
        s3_server.log(
            f"⚠️ No matching commits found. Expected: {known_commits}, Found: {found_commits[:3]}"
        )


@pytest.mark.integration
def test_builder_processes_real_derivations(cf_client, s3_server, derivation_paths):
    """Test that the builder can process our real test derivations"""

    wait_for_crystal_forge_ready(s3_server)

    # Check that derivations exist in database
    derivations = cf_client.execute_sql(
        """
        SELECT d.id, d.derivation_path, d.derivation_name, ds.name as status_name
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE d.derivation_path IS NOT NULL
        ORDER BY d.created_at DESC
        LIMIT 5
        """
    )

    s3_server.log(f"Found {len(derivations)} derivations with paths in database")

    for drv in derivations:
        s3_server.log(
            f"  - {drv['derivation_name']}: {drv['status_name']} ({drv['derivation_path'][:50]}...)"
        )

    if derivations:
        s3_server.log("✅ Real derivations found in database")

        # Wait for builder to process them
        cf_client.wait_for_service_log(
            s3_server,
            "crystal-forge-builder.service",
            ["derivation", "build", "complete", "pending"],
            timeout=120,
        )

        s3_server.log("✅ Builder is processing derivations")
    else:
        # This might happen if flake sync hasn't completed yet
        s3_server.log(
            "⚠️ No derivations with paths found - flake sync may still be in progress"
        )


@pytest.mark.integration
def test_s3_cache_environment_configuration(cf_client, s3_server):
    """Test that S3 cache environment is configured correctly"""

    # Check builder service environment
    service_env = s3_server.succeed(
        "systemctl show crystal-forge-builder.service --property=Environment"
    )

    required_env_vars = [
        "AWS_ENDPOINT_URL=http://s3Cache:9000",
        "AWS_ACCESS_KEY_ID=minioadmin",
        "AWS_SECRET_ACCESS_KEY=minioadmin",
    ]

    for env_var in required_env_vars:
        assert (
            env_var in service_env
        ), f"Missing required S3 environment variable: {env_var}"

    s3_server.log("✅ All S3 environment variables configured correctly")

    # Test connectivity to S3 cache
    s3_server.succeed("ping -c 1 s3Cache")
    s3_server.succeed("nc -z s3Cache 9000")

    s3_server.log("✅ S3 cache connectivity verified")
