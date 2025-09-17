import os
import pytest

pytestmark = [pytest.mark.s3cache]


@pytest.fixture
def failed_derivation_data(cf_client):
    """
    Creates a failed derivation scenario for testing cache push error handling.
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

    test_data = {
        "flake_id": flake_id,
        "commit_id": commit_id,
        "derivation_id": derivation_id,
        "derivation_path": "/nix/store/l46k596qypwijbp4qnbzz93gn86rbxbf-dbus-1.drv",
        "error_message": "nix-store --realise failed with exit code: 1",
        "pname": "dbus",
        "version": "1",
        "status_id": 12,
    }

    yield test_data

    # Cleanup
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


@pytest.fixture
def completed_derivation_data(cf_client):
    """
    Creates a completed derivation for cache push testing.
    """
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
            package_drv_path,
            package_name,
            package_version,
        ),
    )
    derivation_id = derivation_result[0]["id"]

    # Create cache push job
    hello_store_path = os.environ.get("CF_TEST_PACKAGE_STORE_PATH")
    if not hello_store_path:
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

    test_data = {
        "flake_id": flake_id,
        "commit_id": commit_id,
        "derivation_id": derivation_id,
        "cache_push_job_id": cache_push_job_id,
        "derivation_path": package_drv_path,
        "derivation_name": f"{package_name}-{package_version}",
        "pname": package_name,
        "version": package_version,
        "status_id": 10,
    }

    yield test_data

    # Cleanup
    cf_client.execute_sql(
        "DELETE FROM cache_push_jobs WHERE derivation_id = %s", (derivation_id,)
    )
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


def test_cache_push_on_build_complete(
    completed_derivation_data, cfServer, s3Cache, cf_client
):
    """
    When a derivation is build-complete, verify cache push to MinIO.
    Success criterion: any .narinfo object appears in the S3 bucket.
    """
    pkg_name = completed_derivation_data["pname"]
    pkg_version = completed_derivation_data["version"]
    drv_path = completed_derivation_data["derivation_path"]
    deriv_id = completed_derivation_data["derivation_id"]

    cfServer.log(
        f"Testing cache push for derivation_id={deriv_id}, drv={drv_path}, "
        f"pkg={pkg_name}-{pkg_version}"
    )

    # Verify derivation is build-complete (status_id=10)
    status_row = cf_client.execute_sql(
        "SELECT status_id FROM derivations WHERE id = %s",
        (deriv_id,),
    )
    assert status_row, "Derivation row not found after fixture insert"
    assert (
        status_row[0]["status_id"] == 10
    ), "Derivation is not build-complete (status_id != 10)"

    # Poll for .narinfo files - proves cache push worked
    poll_script = r"""
set -euo pipefail
export AWS_ENDPOINT_URL=http://s3Cache:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin

deadline=$((SECONDS + 180))

while (( SECONDS < deadline )); do
  if aws s3 ls --recursive s3://crystal-forge-cache/ 2>/dev/null | grep -E "\.narinfo$" >/dev/null; then
    echo "FOUND"
    exit 0
  fi
  sleep 5
done

exit 1
"""

    try:
        cfServer.succeed(poll_script)
        cfServer.log("Cache push detected: .narinfo present in crystal-forge-cache")
    except Exception:
        cfServer.log("Cache push not detected within timeout. Collecting diagnostics...")

        # Show builder logs
        try:
            logs = cfServer.succeed(
                "journalctl -u crystal-forge-builder.service --no-pager -n 200 || true"
            )
            cfServer.log("---- builder logs ----\n" + logs)
        except Exception:
            pass

        # Show S3 bucket contents
        try:
            listing = cfServer.succeed(
                r"""
export AWS_ENDPOINT_URL=http://s3Cache:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
aws s3 ls --recursive s3://crystal-forge-cache/ || true
"""
            )
            cfServer.log("---- S3 bucket listing ----\n" + listing)
        except Exception:
            pass

        # Show DB state
        try:
            row = cf_client.execute_sql(
                """
                SELECT id, derivation_name, derivation_path, status_id, completed_at
                FROM derivations WHERE id = %s
                """,
                (deriv_id,),
            )
            cfServer.log(f"---- DB row for derivation_id={deriv_id} ----\n{row}")
        except Exception:
            pass

        assert False, (
            f"Did not find any .narinfo files in MinIO within timeout. "
            "See logs above for details."
        )
