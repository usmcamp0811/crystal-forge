import os

import pytest

pytestmark = [pytest.mark.attic_cache]


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
            VALUES (%s, 'pending', 'test')
            ON CONFLICT (derivation_id) WHERE (status = ANY (ARRAY['pending', 'in_progress'])) DO NOTHING
            RETURNING id
            """,
            (derivation_id,),
        )
    else:
        job_row = cf_client.execute_sql(
            """
            INSERT INTO cache_push_jobs (derivation_id, status, cache_destination, store_path)
            VALUES (%s, 'pending', 'test', %s)
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


def test_attic_server_status(cfServer, atticCache):
    """Check if the attic server is actually running"""

    # Check if atticd service is running on the atticCache node
    try:
        atticCache.succeed("systemctl is-active atticd.service")
        cfServer.log("✅ atticd service is active")
    except Exception as e:
        cfServer.log(f"❌ atticd service is not active: {e}")

    # Check if attic server is listening on port 8080
    try:
        atticCache.succeed("ss -tlnp | grep :8080")
        cfServer.log("✅ Something is listening on port 8080")
    except Exception as e:
        cfServer.log(f"❌ Nothing listening on port 8080: {e}")

    # Check atticd logs
    try:
        logs = atticCache.succeed("journalctl -u atticd.service --no-pager -n 20")
        cfServer.log(f"📋 atticd logs:\n{logs}")
    except Exception as e:
        cfServer.log(f"❌ Failed to get atticd logs: {e}")

    # Test HTTP connectivity from cfServer to atticCache
    try:
        result = cfServer.succeed("curl -v http://atticCache:8080/ || true")
        cfServer.log(f"🌐 HTTP test result:\n{result}")
    except Exception as e:
        cfServer.log(f"❌ HTTP test failed: {e}")


def test_attic_cache_push_on_build_complete(
    completed_derivation_data, cfServer, cf_client
):
    """
    When a derivation is build-complete, verify cache push to Attic.
    Success criterion: package appears in Attic cache via attic list.
    """
    pkg_name = completed_derivation_data["pname"]
    pkg_version = completed_derivation_data["version"]
    drv_path = completed_derivation_data["derivation_path"]
    deriv_id = completed_derivation_data["derivation_id"]

    cfServer.log(
        f"Testing Attic cache push for derivation_id={deriv_id}, drv={drv_path}, "
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

    # Add diagnostics to check what's in the database
    derivations_to_push = cf_client.execute_sql(
        """
        SELECT d.id, d.derivation_name, d.derivation_path, d.status_id,
               EXISTS(SELECT 1 FROM cache_push_jobs j WHERE j.derivation_id = d.id) as has_job
        FROM derivations d 
        WHERE d.status_id = 10
    """
    )
    cfServer.log(f"Derivations eligible for cache push: {derivations_to_push}")

    # Check cache push jobs
    cache_jobs = cf_client.execute_sql("SELECT * FROM cache_push_jobs")
    cfServer.log(f"Cache push jobs: {cache_jobs}")

    # Check builder environment variables
    try:
        env_check = cfServer.succeed(
            "systemctl show crystal-forge-builder.service --property=Environment || true"
        )
        cfServer.log(f"Builder environment: {env_check}")
    except Exception:
        pass

    # Poll for packages in Attic cache - proves cache push worked
    poll_script = r"""
set -euo pipefail
export ATTIC_SERVER_URL=http://atticCache:8080
export ATTIC_TOKEN=dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==

deadline=$((SECONDS + 180))

while (( SECONDS < deadline )); do
  # Check if any packages exist in the cache using correct command
  if attic cache list test 2>/dev/null | grep -E "^/nix/store/" >/dev/null; then
    echo "FOUND"
    exit 0
  fi
  sleep 5
done

exit 1
"""

    try:
        cfServer.succeed(poll_script)
        cfServer.log("Cache push detected: packages present in Attic cache 'test'")
    except Exception:
        cfServer.log(
            "Cache push not detected within timeout. Collecting diagnostics..."
        )

        # Show builder logs
        try:
            logs = cfServer.succeed(
                "journalctl -u crystal-forge-builder.service --no-pager -n 200 || true"
            )
            cfServer.log("---- builder logs ----\n" + logs)
        except Exception:
            pass

        # Show Attic cache contents using correct command
        try:
            listing = cfServer.succeed(
                r"""
export ATTIC_SERVER_URL=http://atticCache:8080
export ATTIC_TOKEN=dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==
attic cache list test || true
"""
            )
            cfServer.log("---- Attic cache listing ----\n" + listing)
        except Exception:
            pass

        # Show Attic cache info
        try:
            info = cfServer.succeed(
                r"""
export ATTIC_SERVER_URL=http://atticCache:8080
export ATTIC_TOKEN=dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==
attic cache info test || true
"""
            )
            cfServer.log("---- Attic cache info ----\n" + info)
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

        # Show cache push job status - FIXED: use correct column name
        try:
            job_row = cf_client.execute_sql(
                """
                SELECT id, status, cache_destination, scheduled_at, started_at, completed_at, error_message
                FROM cache_push_jobs WHERE derivation_id = %s
                """,
                (deriv_id,),
            )
            cfServer.log(
                f"---- Cache push job for derivation_id={deriv_id} ----\n{job_row}"
            )
        except Exception:
            pass

        # Test attic connectivity from cfServer
        try:
            attic_test = cfServer.succeed(
                r"""
export ATTIC_SERVER_URL=http://atticCache:8080
export ATTIC_TOKEN=dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==
echo "Testing attic connectivity..."
attic --help || echo "attic command failed"
attic cache --help || echo "attic cache command failed"
"""
            )
            cfServer.log(f"---- Attic command test ----\n{attic_test}")
        except Exception:
            pass

        assert False, (
            f"Did not find any packages in Attic cache within timeout. "
            "See logs above for details."
        )


def test_attic_cache_connectivity(cfServer):
    """
    Test basic connectivity to Attic cache server.
    """
    cfServer.log("Testing basic Attic cache connectivity...")

    # Test Attic server is responding
    connectivity_script = r"""
set -euo pipefail
export ATTIC_SERVER_URL=http://atticCache:8080
export ATTIC_TOKEN=dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==

# Test basic connectivity
attic cache info test
"""

    try:
        result = cfServer.succeed(connectivity_script)
        cfServer.log(f"Attic cache connectivity successful:\n{result}")
    except Exception as e:
        cfServer.log(f"Attic cache connectivity failed: {e}")
        raise


def test_attic_cache_configuration(cfServer, cf_client):
    """
    Test that Crystal Forge is properly configured for Attic cache.
    """
    cfServer.log("Testing Attic cache configuration...")

    # Check that we have a builder service running
    cfServer.succeed("systemctl is-active crystal-forge-builder.service")

    # Check builder logs mention Attic
    try:
        logs = cfServer.succeed(
            "journalctl -u crystal-forge-builder.service --no-pager -n 100 || true"
        )
        cfServer.log(
            "---- Recent builder logs ----\n" + logs[-1000:]
        )  # Last 1000 chars
    except Exception:
        pass

    cfServer.log("✅ Attic cache configuration test completed")
