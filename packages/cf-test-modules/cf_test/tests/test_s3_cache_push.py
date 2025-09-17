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
            package_drv_path,  # NOTE: this is a .drv path (fine here)
            package_name,
            package_version,
        ),
    )
    derivation_id = derivation_result[0]["id"]

    # Get the actual store path for the hello package
    # The environment variable contains the .drv path, but we need the output path
    hello_store_path = os.environ.get("CF_TEST_PACKAGE_STORE_PATH")
    if not hello_store_path:
        # If not provided, we'll let the worker figure it out by leaving store_path NULL
        # The worker can resolve the .drv path to the store path
        job_row = cf_client.execute_sql(
            """
            INSERT INTO cache_push_jobs (derivation_id, status, cache_destination)
            VALUES (%s, 'pending', 's3://crystal-forge-cache')
            ON CONFLICT (derivation_id) WHERE (status = ANY (ARRAY['pending', 'in_progress'])) DO NOTHING
            RETURNING id
            """,
            (derivation_id,),
        )
    else:
        # Use the real store path if available
        job_row = cf_client.execute_sql(
            """
            INSERT INTO cache_push_jobs (derivation_id, status, cache_destination, store_path)
            VALUES (%s, 'pending', 's3://crystal-forge-cache', %s)
            ON CONFLICT (derivation_id) WHERE (status = ANY (ARRAY['pending', 'in_progress'])) DO NOTHING
            RETURNING id
            """,
            (derivation_id, hello_store_path),
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


def test_cache_push_on_build_complete(
    completed_derivation_data, s3_server, s3_cache, cf_client
):
    """
    When a derivation is inserted with status_id=10 (build-complete),
    the builder's cache-push loop should upload its binary cache to MinIO.

    Success criterion: any .narinfo object appears in the S3 bucket.
    """
    pkg_name = completed_derivation_data["pname"]
    pkg_version = completed_derivation_data["version"]
    drv_path = completed_derivation_data["derivation_path"]
    deriv_id = completed_derivation_data["derivation_id"]

    s3_server.log(
        f"Testing cache push for derivation_id={deriv_id}, drv={drv_path}, "
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

    # Poll for any .narinfo files in the bucket - this proves cache push worked
    poll_script = r"""
set -euo pipefail
export AWS_ENDPOINT_URL=http://s3Cache:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin

deadline=$((SECONDS + 180))  # wait up to 3 minutes

while (( SECONDS < deadline )); do
  # Look for any .narinfo files - this proves cache push worked
  if aws s3 ls --recursive s3://crystal-forge-cache/ 2>/dev/null | grep -E "\.narinfo$" >/dev/null; then
    echo "FOUND"
    exit 0
  fi
  sleep 5
done

exit 1
"""

    try:
        s3_server.succeed(poll_script)
        s3_server.log("Cache push detected: .narinfo present in crystal-forge-cache")
    except Exception:
        # Dump helpful diagnostics before failing the test
        s3_server.log(
            "Cache push not detected within timeout. Collecting diagnostics..."
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
            f"Did not find any .narinfo files in MinIO within timeout. "
            "See logs above for details."
        )
