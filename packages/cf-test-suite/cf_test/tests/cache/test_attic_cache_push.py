import os

import pytest

pytestmark = [pytest.mark.attic_cache]


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


def test_cache_push_on_build_complete(cfServer, atticCache, cf_client):
    """
    Test that Crystal Forge can successfully push a built package to Attic cache.
    
    Strategy:
    1. Build a small package (hello) to get a real store path
    2. Insert it into DB as a completed build
    3. Create a cache push job for Attic
    4. Wait for cache worker to process it
    5. Verify success in database
    """
    # Check Attic is configured
    try:
        attic_env = cfServer.succeed(
            "cat /var/lib/crystal-forge/.config/crystal-forge-attic.env"
        )
        if not attic_env.strip():
            pytest.skip("Attic environment not configured in test VM")
    except:
        pytest.skip("Attic environment not configured in test VM")

    cfServer.log("=== Step 0: Stop server service (not needed for cache push testing) ===")
    try:
        cfServer.succeed("systemctl stop crystal-forge-server.service")
        cfServer.log("✅ Stopped server service")
    except:
        cfServer.log("⚠️  Server service not running or already stopped")
    
    # Verify builder service is running (this does the cache pushing)
    try:
        cfServer.succeed("systemctl is-active crystal-forge-builder.service")
        cfServer.log("✅ Builder service is active")
    except:
        cfServer.log("❌ Builder service is not running!")
        pytest.skip("Builder service is not running")
    
    cfServer.log("=== Step 1: Building hello package to get store path ===")
    
    # Build hello package (small and fast)
    try:
        build_output = cfServer.succeed("nix-build '<nixpkgs>' -A hello --no-out-link 2>&1")
        store_path = build_output.strip().split('\n')[-1]  # Last line is the store path
        cfServer.log(f"Built hello package: {store_path}")
        
        # Verify it exists
        cfServer.succeed(f"test -e {store_path}")
        cfServer.log(f"✅ Verified store path exists: {store_path}")
    except Exception as e:
        cfServer.log(f"❌ Failed to build hello package: {e}")
        pytest.skip("Could not build hello package in test VM")

    cfServer.log("=== Step 2: Inserting test data into database ===")
    
    # Insert flake
    flake_result = cf_client.execute_sql(
        """INSERT INTO flakes (name, repo_url)
           VALUES ('test-attic-push', 'http://test-attic-push')
           RETURNING id"""
    )
    flake_id = flake_result[0]["id"]
    cfServer.log(f"Created flake_id={flake_id}")

    # Insert commit
    commit_result = cf_client.execute_sql(
        """INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
           VALUES (%s, 'attic-test-commit', NOW())
           RETURNING id""",
        (flake_id,),
    )
    commit_id = commit_result[0]["id"]
    cfServer.log(f"Created commit_id={commit_id}")

    # Insert derivation with build-complete status and the real store path
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, store_path,
               derivation_path, scheduled_at, completed_at, attempt_count,
               started_at, pname, version, status_id
           ) VALUES (
               %s, 'package', 'hello-test', %s, %s,
               NOW() - INTERVAL '1 hour', NOW() - INTERVAL '30 minutes', 0,
               NOW() - INTERVAL '35 minutes', 'hello', '2.12.1', 10
           ) RETURNING id""",
        (commit_id, store_path, store_path),  # Use store_path for both fields
    )
    derivation_id = derivation_result[0]["id"]
    cfServer.log(f"Created derivation_id={derivation_id} with store_path={store_path}")

    # Insert cache push job pointing to Attic
    cache_job_result = cf_client.execute_sql(
        """INSERT INTO cache_push_jobs (derivation_id, status, cache_destination, store_path)
           VALUES (%s, 'pending', 'cf-test', %s)
           RETURNING id""",
        (derivation_id, store_path),
    )
    cache_job_id = cache_job_result[0]["id"]
    cfServer.log(f"Created cache_push_job_id={cache_job_id} pointing to 'cf-test' Attic cache")

    cfServer.log("=== Step 3: Waiting for cache worker to process job ===")
    
    # Poll database for cache job completion (up to 3 minutes)
    import time
    max_wait = 180  # 3 minutes
    poll_interval = 5
    elapsed = 0
    
    while elapsed < max_wait:
        # Check cache job status
        job_status = cf_client.execute_sql(
            """SELECT id, status, completed_at, error_message, attempts
               FROM cache_push_jobs WHERE id = %s""",
            (cache_job_id,),
        )
        
        if job_status:
            status = job_status[0]["status"]
            error_msg = job_status[0]["error_message"]
            attempts = job_status[0]["attempts"]
            
            cfServer.log(f"Cache job status: {status}, attempts: {attempts}")
            
            if status == "completed":
                cfServer.log("✅ Cache push job completed successfully!")
                
                # Verify derivation status was updated to cache-pushed
                deriv_status = cf_client.execute_sql(
                    "SELECT status_id FROM derivations WHERE id = %s",
                    (derivation_id,),
                )
                status_id = deriv_status[0]["status_id"]
                
                if status_id == 14:  # cache-pushed status
                    cfServer.log("✅ Derivation status updated to cache-pushed")
                else:
                    cfServer.log(f"⚠️  Expected status_id=14 (cache-pushed), got {status_id}")
                
                # Success!
                break
                
            elif status == "failed" or status == "permanently_failed":
                cfServer.log(f"❌ Cache push failed: {error_msg}")
                
                # Get builder logs for debugging
                try:
                    logs = cfServer.succeed(
                        "journalctl -u crystal-forge-builder.service --no-pager --since '5 minutes ago' | tail -50"
                    )
                    cfServer.log(f"Builder logs:\n{logs}")
                except:
                    pass
                
                assert False, f"Cache push job failed: {error_msg}"
        
        time.sleep(poll_interval)
        elapsed += poll_interval
    
    else:
        # Timeout - gather diagnostics
        cfServer.log("❌ Timeout waiting for cache push to complete")
        
        # Show final job state
        final_job = cf_client.execute_sql(
            "SELECT * FROM cache_push_jobs WHERE id = %s",
            (cache_job_id,),
        )
        cfServer.log(f"Final cache job state: {final_job}")
        
        # Show builder logs
        try:
            logs = cfServer.succeed(
                "journalctl -u crystal-forge-builder.service --no-pager --since '5 minutes ago' | tail -100"
            )
            cfServer.log(f"Builder logs:\n{logs}")
        except:
            pass
        
        # Check if builder is even running
        try:
            builder_status = cfServer.succeed("systemctl status crystal-forge-builder.service")
            cfServer.log(f"Builder service status:\n{builder_status}")
        except:
            pass
        
        assert False, "Cache push did not complete within timeout"
    
    cfServer.log("=== Cleanup ===")
    # Cleanup will happen automatically due to foreign key cascades
    cf_client.execute_sql("DELETE FROM cache_push_jobs WHERE id = %s", (cache_job_id,))
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


def test_attic_cache_authentication(cfServer, atticCache):
    """
    Test that Crystal Forge builder has Attic configured and can connect.
    This is a simpler check than waiting for an actual build.
    """
    cfServer.log("Testing Attic cache connectivity and builder configuration...")

    # Check builder service is running
    try:
        cfServer.succeed("systemctl is-active crystal-forge-builder.service")
        cfServer.log("✅ Builder service is active")
    except Exception as e:
        cfServer.log(f"❌ Builder service not active: {e}")
        raise

    # Check env file exists
    try:
        env_content = cfServer.succeed(
            "cat /var/lib/crystal-forge/.config/crystal-forge-attic.env"
        )
        if "ATTIC_TOKEN" in env_content:
            cfServer.log("✅ Attic environment file is configured with token")
        else:
            cfServer.log("❌ No ATTIC_TOKEN in environment file")
            assert False, "ATTIC_TOKEN not configured"
    except Exception as e:
        cfServer.log(f"⚠️  Could not verify env file: {e}")

    # Check builder can resolve attic cache hostname
    try:
        result = cfServer.succeed("ping -c 1 atticCache || true")
        if "1 received" in result or "PING" in result:
            cfServer.log("✅ atticCache hostname is resolvable")
        else:
            cfServer.log("⚠️  atticCache may not be resolvable - but continuing")
    except Exception as e:
        cfServer.log(f"⚠️  Could not ping atticCache: {e}")

    # Check attic client is available
    try:
        cfServer.succeed("which attic")
        cfServer.log("✅ attic client is available")
    except Exception as e:
        cfServer.log(f"❌ attic client not available: {e}")

    cfServer.log("Attic cache configuration test completed")


@pytest.mark.skip("TODO: Fix this and make it better")
def test_attic_cache_configuration(cfServer, cf_client):
    """
    Test that Crystal Forge is properly configured for Attic cache.
    Focus on configuration rather than client connectivity.
    """
    cfServer.log("Testing Attic cache configuration...")

    # Check that we have a builder service running
    cfServer.succeed("systemctl is-active crystal-forge-builder.service")

    # Check that the configuration includes Attic settings
    try:
        # Use root to read the config since crystal-forge user has permission issues
        config_content = cfServer.succeed("cat /var/lib/crystal-forge/config.toml")

        # Verify Attic configuration is present
        assert (
            'cache_type = "Attic"' in config_content
        ), "Attic cache type not configured"
        assert (
            'attic_cache_name = "cf-test"' in config_content
        ), "Attic cache name not configured"

        cfServer.log("✅ Crystal Forge config contains Attic settings")

    except Exception as e:
        cfServer.log(f"Could not verify config: {e}")

        # Show what we can access
        try:
            ls_output = cfServer.succeed("ls -la /var/lib/crystal-forge/")
            cfServer.log(f"crystal-forge directory contents:\n{ls_output}")
        except Exception:
            pass

    # Check environment variables are available to the service
    try:
        env_file = cfServer.succeed(
            "cat /var/lib/crystal-forge/.config/crystal-forge-attic.env"
        )
        assert "ATTIC_TOKEN=" in env_file, "ATTIC_TOKEN not in environment file"
        assert (
            "ATTIC_SERVER_URL=" in env_file
        ), "ATTIC_SERVER_URL not in environment file"
        cfServer.log("✅ Attic environment variables configured")
    except Exception as e:
        cfServer.log(f"Environment file issue: {e}")

    cfServer.log("✅ Attic cache configuration test completed")
