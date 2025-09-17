import os
import time

import pytest

from cf_test import CFTestClient

pytestmark = [pytest.mark.s3cache]


@pytest.fixture(scope="session")
def s3_server():
    import cf_test

    return cf_test._driver_machines["s3Server"]


@pytest.fixture(scope="session")
def s3_cache():
    import cf_test

    return cf_test._driver_machines["s3Cache"]


@pytest.fixture
def failed_derivation_data(cf_client):
    """
    Creates a failed derivation scenario for testing cache push error handling.

    Returns a dictionary with the created test data IDs for cleanup.
    """
    # Insert test flake
    flake_result = cf_client.execute_sql(
        """INSERT INTO flakes (name, repo_url) 
           VALUES ('test-failed-flake', 'http://test-failed') 
           RETURNING id""",
    )
    flake_id = flake_result[0]["id"]

    # Insert test commit
    commit_result = cf_client.execute_sql(
        """INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) 
           VALUES (%s, 'failed123abc456', NOW()) 
           RETURNING id""",
        (flake_id,),
    )
    commit_id = commit_result[0]["id"]

    # Insert failed derivation (status_id = 12 for failed build)
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, derivation_path,
               scheduled_at, completed_at, attempt_count, started_at,
               evaluation_duration_ms, error_message, pname, version, status_id
           ) VALUES (
               %s, 'package', '/nix/store/l46k596qypwijbp4qnbzz93gn86rbxbf-dbus-1.drv',
               '/nix/store/l46k596qypwijbp4qnbzz93gn86rbxbf-dbus-1.drv',
               NOW() - INTERVAL '1 hour', NOW() - INTERVAL '30 minutes', 0,
               NOW() - INTERVAL '35 minutes', 1795,
               'nix-store --realise failed with exit code: 1',
               'dbus', '1', 12
           ) RETURNING id""",
        (commit_id,),
    )
    derivation_id = derivation_result[0]["id"]

    # Return test data for use in tests and cleanup
    test_data = {
        "flake_id": flake_id,
        "commit_id": commit_id,
        "derivation_id": derivation_id,
        "derivation_path": "/nix/store/l46k596qypwijbp4qnbzz93gn86rbxbf-dbus-1.drv",
        "error_message": "nix-store --realise failed with exit code: 1",
        "pname": "dbus",
        "version": "1",
        "status_id": 12,  # failed build
    }

    yield test_data

    # Cleanup after test
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


@pytest.fixture
def completed_derivation_data(cf_client):
    """
    Creates a completed derivation using a real package from the Nix store for cache push testing,
    and enqueues a pending cache push job.
    """

    # Get test package info from environment variables set by the VM test
    package_drv_path = os.environ.get("CF_TEST_PACKAGE_DRV")
    package_name = os.environ.get("CF_TEST_PACKAGE_NAME", "hello")
    package_version = os.environ.get("CF_TEST_PACKAGE_VERSION", "2.12.1")

    if not package_drv_path:
        pytest.skip("CF_TEST_PACKAGE_DRV environment variable not set")

    # Insert test flake
    flake_result = cf_client.execute_sql(
        """INSERT INTO flakes (name, repo_url)
           VALUES ('test-completed-flake', 'http://test-completed')
           RETURNING id"""
    )
    flake_id = flake_result[0]["id"]

    # Insert test commit
    commit_result = cf_client.execute_sql(
        """INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
           VALUES (%s, 'completed123abc456', NOW())
           RETURNING id""",
        (flake_id,),
    )
    commit_id = commit_result[0]["id"]

    # Insert completed derivation (status_id = 10 for build-complete)
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, derivation_path,
               scheduled_at, completed_at, attempt_count, started_at,
               evaluation_duration_ms, pname, version, status_id
           ) VALUES (
               %s, 'package', %s, %s,
               NOW() - INTERVAL '1 hour', NOW() - INTERVAL '30 minutes', 0,
               NOW() - INTERVAL '35 minutes', 1500,
               %s, %s, 10
           ) RETURNING id""",
        (
            commit_id,
            f"{package_name}-{package_version}",
            package_drv_path,  # ⬅ NOTE: this is a .drv path (fine here)
            package_name,
            package_version,
        ),
    )
    derivation_id = derivation_result[0]["id"]

    # enqueue a pending cache push job for this derivation
    # leave store_path NULL so the worker can resolve/build the out path
    job_row = cf_client.execute_sql(
        """
        INSERT INTO cache_push_jobs (derivation_id, status, cache_destination)
        VALUES (%s, 'pending', 's3://crystal-forge-cache')
        ON CONFLICT ON CONSTRAINT idx_cache_push_jobs_derivation_unique DO NOTHING
        RETURNING id
        """,
        (derivation_id,),
    )
    cache_push_job_id = job_row[0]["id"] if job_row else None

    # Return test data for use in tests and cleanup
    test_data = {
        "flake_id": flake_id,
        "commit_id": commit_id,
        "derivation_id": derivation_id,
        "cache_push_job_id": cache_push_job_id,
        "derivation_path": package_drv_path,
        "derivation_name": f"{package_name}-{package_version}",
        "pname": package_name,
        "version": package_version,
        "status_id": 10,  # build-complete
    }

    yield test_data

    # Cleanup after test (delete job first due to FK)
    cf_client.execute_sql(
        "DELETE FROM cache_push_jobs WHERE derivation_id = %s", (derivation_id,)
    )
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


@pytest.fixture(scope="session")
def cf_client(cf_config):
    return CFTestClient(cf_config)


# def test_s3_connectivity(s3_server, s3_cache):
#     """Test basic S3 connectivity between builder and MinIO"""
#
#     # Test network connectivity
#     s3_server.succeed("ping -c 1 s3Cache")
#     s3_server.log("✅ Network connectivity to S3 cache established")
#
#     # Test MinIO is responding
#     s3_cache.succeed("curl -f http://localhost:9000/minio/health/live")
#     s3_server.log("✅ MinIO health check passed")
#
#     # Test AWS CLI can reach MinIO from builder
#     s3_server.succeed(
#         """
#         AWS_ENDPOINT_URL=http://s3Cache:9000 \
#         AWS_ACCESS_KEY_ID=minioadmin \
#         AWS_SECRET_ACCESS_KEY=minioadmin \
#         aws s3 ls s3://crystal-forge-cache/ || true
#         """
#     )
#     s3_server.log("✅ AWS CLI connectivity to MinIO verified")


# def test_builder_s3_cache_config(s3_server):
#     """Test that builder has correct S3 cache configuration"""
#
#     # Check Crystal Forge config file
#     config_content = s3_server.succeed("cat /var/lib/crystal-forge/config.toml")
#     s3_server.log(f"Crystal Forge config excerpt: {config_content[:500]}...")
#
#     # Verify S3 cache configuration
#     assert 'cache_type = "S3"' in config_content, "S3 cache type not configured"
#     assert "s3Cache:9000" in config_content, "S3 endpoint not configured"
#     assert "push_after_build = true" in config_content, "Cache push not enabled"
#
#     s3_server.log("✅ S3 cache configuration verified")
#
#     # Verify builder service has AWS environment variables
#     env_output = s3_server.succeed(
#         "systemctl show crystal-forge-builder.service --property=Environment"
#     )
#     assert (
#         "AWS_ENDPOINT_URL=http://s3Cache:9000" in env_output
#     ), "AWS endpoint not in environment"
#     assert (
#         "AWS_ACCESS_KEY_ID=minioadmin" in env_output
#     ), "AWS credentials not in environment"
#
#     s3_server.log("✅ AWS environment variables configured")


def test_cache_push_on_build_complete(
    completed_derivation_data, s3_server, s3_cache, cf_client
):
    """
    When a derivation is inserted with status_id=10 (build-complete),
    the builder's cache-push loop should upload its binary cache to MinIO.

    Success criterion: a .narinfo object for the package appears in the S3 bucket.
    """
    pkg_name = completed_derivation_data["pname"]
    pkg_version = completed_derivation_data["version"]
    drv_path = completed_derivation_data["derivation_path"]
    deriv_id = completed_derivation_data["derivation_id"]

    s3_server.log(
        f"🧪 Verifying cache push for derivation_id={deriv_id}, drv={drv_path}, "
        f"pkg={pkg_name}-{pkg_version}"
    )

    # Sanity check the DB row is still build-complete (status_id=10)
    status_row = cf_client.execute_sql(
        "SELECT status_id FROM derivations WHERE id = %s",
        (deriv_id,),
    )
    assert status_row, "Derivation row not found after fixture insert"
    assert (
        status_row[0]["status_id"] == 10
    ), "Derivation is not build-complete (status_id != 10)"

    # The Nix binary cache layout places .narinfo files under a 'narinfo/' prefix and
    # the filename includes the package name-version. We poll for that object.
    #
    # NOTE: We match case-insensitively on '{name}-{version}.narinfo' to avoid coupling
    # to the output hash prefix present in narinfo filenames.
    poll_script = r"""
set -euo pipefail
export AWS_ENDPOINT_URL=http://s3Cache:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin

name_ver="%s-%s"
deadline=$((SECONDS + 180))  # wait up to 3 minutes

while (( SECONDS < deadline )); do
  # List recursively and look for a narinfo containing name-version.
  if aws s3 ls --recursive s3://crystal-forge-cache/ 2>/dev/null | grep -i -E "/narinfo/.*${name_ver}\.narinfo$" >/dev/null; then
    echo "FOUND"
    exit 0
  fi
  sleep 5
done

exit 1
""" % (
        pkg_name,
        pkg_version,
    )

    try:
        s3_server.succeed(poll_script)
        s3_server.log("✅ Cache push detected: .narinfo present in crystal-forge-cache")
    except Exception:
        # Dump helpful diagnostics before failing the test
        s3_server.log(
            "❌ Cache push not detected within timeout. Collecting diagnostics..."
        )

        # 1) Show recent builder logs
        try:
            logs = s3_server.succeed(
                "journalctl -u crystal-forge-builder.service --no-pager -n 200 || true"
            )
            s3_server.log(
                "---- crystal-forge-builder.service logs (last 200 lines) ----\n" + logs
            )
        except Exception:
            pass

        # 2) Show bucket inventory to see what's actually there
        try:
            listing = s3_server.succeed(
                r"""
export AWS_ENDPOINT_URL=http://s3Cache:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
aws s3 ls --recursive s3://crystal-forge-cache/ || true
"""
            )
            s3_server.log(
                "---- S3 bucket listing (crystal-forge-cache) ----\n" + listing
            )
        except Exception:
            pass

        # 3) Echo DB state for the derivation in question
        try:
            row = cf_client.execute_sql(
                """
                SELECT id, derivation_name, derivation_path, status_id, completed_at
                FROM derivations
                WHERE id = %s
                """,
                (deriv_id,),
            )
            s3_server.log(f"---- DB row for derivation_id={deriv_id} ----\n{row}")
        except Exception:
            pass

        # Finally, fail the test
        assert False, (
            f"Did not find .narinfo for {pkg_name}-{pkg_version} in MinIO within timeout. "
            "See logs above for details."
        )
