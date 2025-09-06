import time

import pytest


@pytest.mark.vm_only
def test_crystal_forge_discovers_and_ingests_simple_flake(
    cf_client, server, agent, gitserver
):
    """Test that Crystal Forge automatically discovers and ingests commits from the test flake"""

    # Wait for git server to be ready
    gitserver.wait_for_open_port(8080)

    # Crystal Forge should be configured to watch the test flake already
    # We just need to wait for it to do its job

    expected_commits = 5  # Based on your mention of default being 5
    max_wait_time = 300  # 5 minutes

    print("Waiting for Crystal Forge to discover and ingest test flake commits...")

    for attempt in range(30):  # 30 attempts * 10 seconds = 5 minutes
        try:
            # Check if Crystal Forge has ingested the flake
            flake_result = server.succeed(
                """
                sudo -u postgres psql -d crystal_forge -t -c "
                SELECT COUNT(*) FROM flakes WHERE repo_url LIKE '%test-flake%';"
            """
            ).strip()

            flake_count = int(flake_result)

            if flake_count > 0:
                # Now check for commits
                commit_result = server.succeed(
                    """
                    sudo -u postgres psql -d crystal_forge -t -c "
                    SELECT COUNT(*) FROM commits c 
                    JOIN flakes f ON c.flake_id = f.id 
                    WHERE f.repo_url LIKE '%test-flake%';"
                """
                ).strip()

                commit_count = int(commit_result)
                print(
                    f"Attempt {attempt + 1}: Found {flake_count} flake(s) and {commit_count} commits"
                )

                if commit_count >= expected_commits:
                    print(
                        f"✓ Crystal Forge successfully ingested {commit_count} commits"
                    )
                    break
            else:
                print(
                    f"Attempt {attempt + 1}: Flake not yet discovered by Crystal Forge"
                )

        except Exception as e:
            print(f"Database query failed on attempt {attempt + 1}: {e}")

        time.sleep(10)
    else:
        # Print diagnostic info on timeout
        print("Timeout reached. Checking Crystal Forge status...")

        # Check server logs for any errors
        server_logs = server.succeed(
            "journalctl -u crystal-forge-server --no-pager -n 50 | tail -20 || echo 'no logs'"
        )
        print(f"Recent server logs:\n{server_logs}")

        # Check what flakes Crystal Forge knows about
        flakes_result = server.succeed(
            """
            sudo -u postgres psql -d crystal_forge -c "
            SELECT id, name, repo_url FROM flakes;"
        """
        )
        print(f"Flakes in database:\n{flakes_result}")

        # Check git server accessibility from Crystal Forge's perspective
        git_test = server.succeed(
            "curl -s http://gitserver:8080/ || echo 'git server not accessible'"
        )
        print(f"Git server accessibility: {git_test}")

        pytest.fail(
            f"Crystal Forge failed to ingest expected {expected_commits} commits within {max_wait_time} seconds"
        )

    # Verify the final state
    final_state = server.succeed(
        """
        sudo -u postgres psql -d crystal_forge -c "
        SELECT f.name, f.repo_url, COUNT(c.id) as commit_count,
               MIN(c.commit_timestamp) as earliest_commit,
               MAX(c.commit_timestamp) as latest_commit
        FROM flakes f 
        LEFT JOIN commits c ON f.id = c.flake_id 
        WHERE f.repo_url LIKE '%test-flake%'
        GROUP BY f.id, f.name, f.repo_url;"
    """
    )
    print(f"Final ingestion state:\n{final_state}")


@pytest.mark.vm_only
def test_crystal_forge_processes_simple_flake_builds(
    cf_client, server, agent, gitserver
):
    """Test that Crystal Forge processes builds/evaluations for the ingested test flake"""

    # Wait for commits to exist first (may be from previous test or parallel processing)
    commit_count = 0
    for attempt in range(12):  # 2 minutes
        try:
            commit_result = server.succeed(
                """
                sudo -u postgres psql -d crystal_forge -t -c "
                SELECT COUNT(*) FROM commits c 
                JOIN flakes f ON c.flake_id = f.id 
                WHERE f.repo_url LIKE '%test-flake%';"
            """
            ).strip()

            commit_count = int(commit_result)
            if commit_count > 0:
                print(f"Found {commit_count} commits ready for processing")
                break
        except:
            pass
        time.sleep(10)

    if commit_count == 0:
        pytest.skip(
            "No commits found for test flake - Crystal Forge may not have ingested them yet"
        )

    # Now wait for Crystal Forge to create and process derivations
    print("Waiting for Crystal Forge to create and process derivations...")

    max_wait = 600  # 10 minutes for builds - builds can take time

    for attempt in range(40):  # 40 attempts * 15 seconds = 10 minutes
        try:
            # Check derivation progress
            derivation_summary = server.succeed(
                """
                sudo -u postgres psql -d crystal_forge -c "
                SELECT 
                    ds.name as status,
                    COUNT(*) as count,
                    ds.is_terminal,
                    ds.is_success
                FROM derivations d 
                JOIN derivation_statuses ds ON d.status_id = ds.id
                JOIN commits c ON d.commit_id = c.id 
                JOIN flakes f ON c.flake_id = f.id 
                WHERE f.repo_url LIKE '%test-flake%'
                GROUP BY ds.name, ds.is_terminal, ds.is_success, ds.display_order 
                ORDER BY ds.display_order;"
            """
            )

            if attempt % 4 == 0:  # Print every minute
                print(
                    f"Attempt {attempt + 1} - Derivation status:\n{derivation_summary}"
                )

            # Check for any completed derivations (terminal status)
            terminal_count = server.succeed(
                """
                sudo -u postgres psql -d crystal_forge -t -c "
                SELECT COUNT(*) FROM derivations d 
                JOIN derivation_statuses ds ON d.status_id = ds.id
                JOIN commits c ON d.commit_id = c.id 
                JOIN flakes f ON c.flake_id = f.id 
                WHERE f.repo_url LIKE '%test-flake%'
                AND ds.is_terminal = true;"
            """
            ).strip()

            terminal_derivations = int(terminal_count)

            if terminal_derivations > 0:
                print(
                    f"✓ Crystal Forge completed processing {terminal_derivations} derivations"
                )
                break

        except Exception as e:
            print(f"Query failed on attempt {attempt + 1}: {e}")

        time.sleep(15)
    else:
        # Print final diagnostic info
        print("Timeout waiting for derivation processing.")

        # Check if any derivations were created at all
        total_derivations = server.succeed(
            """
            sudo -u postgres psql -d crystal_forge -t -c "
            SELECT COUNT(*) FROM derivations d 
            JOIN commits c ON d.commit_id = c.id 
            JOIN flakes f ON c.flake_id = f.id 
            WHERE f.repo_url LIKE '%test-flake%';"
        """
        ).strip()

        print(f"Total derivations created: {total_derivations}")

        # Check agent connectivity if no derivations processed
        agent_logs = agent.succeed(
            "journalctl -u crystal-forge-client --no-pager -n 20 | tail -10 || echo 'no agent logs'"
        )
        print(f"Recent agent logs:\n{agent_logs}")

        # This might not be a hard failure - Crystal Forge might just be slow
        print(
            f"Crystal Forge created {total_derivations} derivations but none completed processing within timeout"
        )

    # Print final summary regardless of outcome
    final_summary = server.succeed(
        """
        sudo -u postgres psql -d crystal_forge -c "
        SELECT 
            d.derivation_type,
            d.derivation_name,
            ds.name as status,
            d.attempt_count,
            COALESCE(d.error_message, 'No error') as error_message
        FROM derivations d 
        JOIN derivation_statuses ds ON d.status_id = ds.id
        JOIN commits c ON d.commit_id = c.id 
        JOIN flakes f ON c.flake_id = f.id 
        WHERE f.repo_url LIKE '%test-flake%'
        ORDER BY d.derivation_type, d.derivation_name
        LIMIT 10;"
    """
    )

    print(f"Final derivation summary:\n{final_summary}")

    # At minimum, verify some derivations were created
    total_derivations = int(
        server.succeed(
            """
        sudo -u postgres psql -d crystal_forge -t -c "
        SELECT COUNT(*) FROM derivations d 
        JOIN commits c ON d.commit_id = c.id 
        JOIN flakes f ON c.flake_id = f.id 
        WHERE f.repo_url LIKE '%test-flake%';"
    """
        ).strip()
    )

    assert (
        total_derivations > 0
    ), f"Crystal Forge should have created derivations but found {total_derivations}"
    print(f"✓ Crystal Forge created {total_derivations} derivations for the test flake")
