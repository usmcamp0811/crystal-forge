import json
from datetime import datetime, timedelta

import pytest

from cf_test import CFTestClient, assert_view_has_data


@pytest.mark.views
@pytest.mark.database
class TestSystemsStatusView:
    """Test suite for view_systems_status_table"""

    def test_view_exists(self, cf_client: CFTestClient):
        """Test that the systems status view exists and has expected columns"""
        expected_columns = [
            "hostname",
            "connectivity_status",
            "connectivity_status_text",
            "update_status",
            "update_status_text",
            "overall_status",
            "last_seen",
            "agent_version",
            "uptime",
            "ip_address",
            "current_derivation_path",
            "latest_commit_hash",
        ]

        # Check if view exists by trying to query it
        result = cf_client.execute_sql(
            "SELECT * FROM view_systems_status_table LIMIT 0"
        )
        assert result is not None, "view_systems_status_table should exist"

    def test_never_seen_system_status(self, cf_client: CFTestClient):
        """Test system that is registered but never reported in"""
        # Setup: Create a system that's registered but never reported
        test_hostname = "test-never-seen-01"

        # Create flake for this system
        flake_data = {
            "flakes": [
                {
                    "name": "test-app",
                    "repo_url": "https://github.com/test/app.git",
                    "created_at": "NOW()",
                    "updated_at": "NOW()",
                }
            ]
        }
        flake_ids = cf_client.setup_test_data(flake_data)
        flake_id = flake_ids["flakes"][0]

        # Register system but don't create any system_states or heartbeats
        system_sql = """
            INSERT INTO systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, true, '/nix/store/test.drv', 'fake-key')
        """
        cf_client.execute_sql(system_sql, (test_hostname, flake_id))

        # Test the view
        view_sql = """
            SELECT hostname, connectivity_status, connectivity_status_text, 
                   update_status, update_status_text, overall_status
            FROM view_systems_status_table 
            WHERE hostname = %s
        """

        results = cf_client.execute_sql(view_sql, (test_hostname,))
        assert len(results) == 1, f"Expected 1 result for {test_hostname}"

        result = results[0]
        assert result["hostname"] == test_hostname
        assert result["connectivity_status"] == "never_seen"
        assert result["update_status"] == "never_seen"
        assert result["overall_status"] == "never_seen"
        assert "never seen" in result["connectivity_status_text"].lower()

        # Save test evidence
        cf_client.save_artifact(
            json.dumps(result, indent=2, default=str),
            "never_seen_system_test.json",
            "Never seen system test results",
        )

        # Cleanup
        cf_client.cleanup_test_data(
            {
                "systems": [f"hostname = '{test_hostname}'"],
                "flakes": [f"id = {flake_id}"],
            }
        )

    def test_up_to_date_system_status(self, cf_client: CFTestClient):
        """Test system that is up to date with latest commit and sending heartbeats"""
        test_hostname = "test-uptodate-01"

        # Setup complete scenario
        # 1. Create flake
        flake_data = {
            "flakes": [
                {
                    "name": "prod-app",
                    "repo_url": "https://github.com/company/prod-app.git",
                    "created_at": "NOW()",
                    "updated_at": "NOW()",
                }
            ]
        }
        flake_ids = cf_client.setup_test_data(flake_data)
        flake_id = flake_ids["flakes"][0]

        # 2. Create commit
        commit_sql = """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, NOW() - INTERVAL '1 hour', 0)
            RETURNING id
        """
        commit_results = cf_client.execute_sql(commit_sql, (flake_id, "abc123current"))
        commit_id = commit_results[0]["id"]

        # 3. Create successful derivation
        derivation_path = "/nix/store/abc123cu-nixos-system-test-uptodate-01.drv"
        deriv_sql = """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, attempt_count, scheduled_at, completed_at
            )
            VALUES (%s, 'nixos', %s, %s, 10, 0, NOW() - INTERVAL '50 minutes', NOW() - INTERVAL '45 minutes')
            RETURNING id
        """
        deriv_results = cf_client.execute_sql(
            deriv_sql, (commit_id, test_hostname, derivation_path)
        )
        deriv_id = deriv_results[0]["id"]

        # 4. Register system
        system_sql = """
            INSERT INTO systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, true, %s, 'fake-key')
        """
        cf_client.execute_sql(system_sql, (test_hostname, flake_id, derivation_path))

        # 5. Create recent system state (system is on current derivation)
        state_sql = """
            INSERT INTO system_states (
                hostname, change_reason, derivation_path, os, kernel,
                memory_gb, uptime_secs, cpu_brand, cpu_cores,
                primary_ip_address, nixos_version, agent_compatible,
                timestamp
            )
            VALUES (
                %s, 'deployment', %s, 'NixOS', '6.6.89',
                32.0, 3600, 'Intel Xeon', 16,
                '192.168.1.100', '25.05', true,
                NOW() - INTERVAL '10 minutes'
            )
            RETURNING id
        """
        state_results = cf_client.execute_sql(
            state_sql, (test_hostname, derivation_path)
        )
        state_id = state_results[0]["id"]

        # 6. Create recent heartbeat
        heartbeat_sql = """
            INSERT INTO agent_heartbeats (
                system_state_id, timestamp, agent_version, agent_build_hash
            )
            VALUES (%s, NOW() - INTERVAL '2 minutes', '2.0.0', 'build123')
        """
        cf_client.execute_sql(heartbeat_sql, (state_id,))

        # Test the view
        view_sql = """
            SELECT hostname, connectivity_status, connectivity_status_text,
                   update_status, update_status_text, overall_status,
                   last_seen, agent_version, ip_address, uptime,
                   current_derivation_path, latest_commit_hash
            FROM view_systems_status_table 
            WHERE hostname = %s
        """

        results = cf_client.execute_sql(view_sql, (test_hostname,))
        assert len(results) == 1, f"Expected 1 result for {test_hostname}"

        result = results[0]
        assert result["hostname"] == test_hostname
        assert result["connectivity_status"] == "online"
        assert result["update_status"] == "up_to_date"
        assert result["overall_status"] == "up_to_date"
        assert result["agent_version"] == "2.0.0"
        assert result["ip_address"] == "192.168.1.100"
        assert result["current_derivation_path"] == derivation_path
        assert result["latest_commit_hash"] == "abc123current"
        assert "active" in result["connectivity_status_text"].lower()
        assert "latest version" in result["update_status_text"].lower()

        # Save test evidence
        cf_client.save_artifact(
            json.dumps(result, indent=2, default=str),
            "up_to_date_system_test.json",
            "Up to date system test results",
        )

        # Cleanup
        cf_client.cleanup_test_data(
            {
                "agent_heartbeats": [f"system_state_id = {state_id}"],
                "system_states": [f"hostname = '{test_hostname}'"],
                "systems": [f"hostname = '{test_hostname}'"],
                "derivations": [f"id = {deriv_id}"],
                "commits": [f"id = {commit_id}"],
                "flakes": [f"id = {flake_id}"],
            }
        )

    def test_behind_system_status(self, cf_client: CFTestClient):
        """Test system that is behind the latest commit"""
        test_hostname = "test-behind-01"

        # Setup scenario where system is on old commit
        # 1. Create flake
        flake_data = {
            "flakes": [
                {
                    "name": "behind-app",
                    "repo_url": "https://github.com/test/behind-app.git",
                    "created_at": "NOW()",
                    "updated_at": "NOW()",
                }
            ]
        }
        flake_ids = cf_client.setup_test_data(flake_data)
        flake_id = flake_ids["flakes"][0]

        # 2. Create old commit (what system is currently on)
        old_commit_sql = """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, NOW() - INTERVAL '2 days', 0)
            RETURNING id
        """
        old_commit_results = cf_client.execute_sql(
            old_commit_sql, (flake_id, "old456commit")
        )
        old_commit_id = old_commit_results[0]["id"]

        # 3. Create new commit (latest available)
        new_commit_sql = """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, NOW() - INTERVAL '1 hour', 0)
            RETURNING id
        """
        new_commit_results = cf_client.execute_sql(
            new_commit_sql, (flake_id, "new789commit")
        )
        new_commit_id = new_commit_results[0]["id"]

        # 4. Create old derivation (what system is running)
        old_derivation_path = "/nix/store/old456co-nixos-system-test-behind-01.drv"
        old_deriv_sql = """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, attempt_count, scheduled_at, completed_at
            )
            VALUES (%s, 'nixos', %s, %s, 10, 0, NOW() - INTERVAL '2 days', NOW() - INTERVAL '47 hours')
        """
        cf_client.execute_sql(
            old_deriv_sql, (old_commit_id, test_hostname, old_derivation_path)
        )

        # 5. Create new derivation (latest available)
        new_derivation_path = "/nix/store/new789co-nixos-system-test-behind-01.drv"
        new_deriv_sql = """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, attempt_count, scheduled_at, completed_at
            )
            VALUES (%s, 'nixos', %s, %s, 10, 0, NOW() - INTERVAL '50 minutes', NOW() - INTERVAL '45 minutes')
            RETURNING id
        """
        new_deriv_results = cf_client.execute_sql(
            new_deriv_sql, (new_commit_id, test_hostname, new_derivation_path)
        )
        new_deriv_id = new_deriv_results[0]["id"]

        # 6. Register system
        system_sql = """
            INSERT INTO systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, true, %s, 'fake-key')
        """
        cf_client.execute_sql(
            system_sql, (test_hostname, flake_id, old_derivation_path)
        )

        # 7. System state shows it's on old derivation
        state_sql = """
            INSERT INTO system_states (
                hostname, change_reason, derivation_path, os, kernel,
                primary_ip_address, nixos_version, agent_compatible,
                timestamp
            )
            VALUES (
                %s, 'heartbeat', %s, 'NixOS', '6.6.89',
                '192.168.1.101', '25.05', true,
                NOW() - INTERVAL '5 minutes'
            )
            RETURNING id
        """
        state_results = cf_client.execute_sql(
            state_sql, (test_hostname, old_derivation_path)
        )
        state_id = state_results[0]["id"]

        # 8. Recent heartbeat
        heartbeat_sql = """
            INSERT INTO agent_heartbeats (
                system_state_id, timestamp, agent_version, agent_build_hash
            )
            VALUES (%s, NOW() - INTERVAL '1 minute', '2.0.0', 'build123')
        """
        cf_client.execute_sql(heartbeat_sql, (state_id,))

        # Test the view
        view_sql = """
            SELECT hostname, connectivity_status, update_status, overall_status,
                   current_derivation_path, latest_derivation_path,
                   latest_commit_hash, drift_hours
            FROM view_systems_status_table 
            WHERE hostname = %s
        """

        results = cf_client.execute_sql(view_sql, (test_hostname,))
        assert len(results) == 1, f"Expected 1 result for {test_hostname}"

        result = results[0]
        assert result["hostname"] == test_hostname
        assert result["connectivity_status"] == "online"
        assert result["update_status"] == "behind"
        assert result["overall_status"] == "behind"
        assert result["current_derivation_path"] == old_derivation_path
        assert result["latest_derivation_path"] == new_derivation_path
        assert result["latest_commit_hash"] == "new789commit"

        # Should have positive drift (system is behind)
        if result["drift_hours"] is not None:
            assert (
                float(result["drift_hours"]) > 0
            ), "Drift should be positive when system is behind"

        # Save test evidence
        cf_client.save_artifact(
            json.dumps(result, indent=2, default=str),
            "behind_system_test.json",
            "Behind system test results",
        )

        # Cleanup
        cf_client.cleanup_test_data(
            {
                "agent_heartbeats": [f"system_state_id = {state_id}"],
                "system_states": [f"hostname = '{test_hostname}'"],
                "systems": [f"hostname = '{test_hostname}'"],
                "derivations": [f"derivation_name = '{test_hostname}'"],
                "commits": [f"id IN ({old_commit_id}, {new_commit_id})"],
                "flakes": [f"id = {flake_id}"],
            }
        )

    def test_offline_system_status(self, cf_client: CFTestClient):
        """Test system that hasn't sent heartbeats recently"""
        test_hostname = "test-offline-01"

        # Setup system that was online but went offline
        # 1. Create flake
        flake_data = {
            "flakes": [
                {
                    "name": "offline-app",
                    "repo_url": "https://github.com/test/offline-app.git",
                    "created_at": "NOW()",
                    "updated_at": "NOW()",
                }
            ]
        }
        flake_ids = cf_client.setup_test_data(flake_data)
        flake_id = flake_ids["flakes"][0]

        # 2. Create commit and derivation
        commit_sql = """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, NOW() - INTERVAL '2 hours', 0)
            RETURNING id
        """
        commit_results = cf_client.execute_sql(commit_sql, (flake_id, "offline123"))
        commit_id = commit_results[0]["id"]

        derivation_path = "/nix/store/offline12-nixos-system-test-offline-01.drv"
        deriv_sql = """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, completed_at
            )
            VALUES (%s, 'nixos', %s, %s, 10, NOW() - INTERVAL '90 minutes')
        """
        cf_client.execute_sql(deriv_sql, (commit_id, test_hostname, derivation_path))

        # 3. Register system
        system_sql = """
            INSERT INTO systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, true, %s, 'fake-key')
        """
        cf_client.execute_sql(system_sql, (test_hostname, flake_id, derivation_path))

        # 4. Old system state (over 30 minutes ago)
        state_sql = """
            INSERT INTO system_states (
                hostname, change_reason, derivation_path, os,
                primary_ip_address, agent_compatible,
                timestamp
            )
            VALUES (
                %s, 'deployment', %s, 'NixOS',
                '192.168.1.102', true,
                NOW() - INTERVAL '45 minutes'
            )
            RETURNING id
        """
        state_results = cf_client.execute_sql(
            state_sql, (test_hostname, derivation_path)
        )
        state_id = state_results[0]["id"]

        # 5. Old heartbeat (over 30 minutes ago)
        heartbeat_sql = """
            INSERT INTO agent_heartbeats (
                system_state_id, timestamp, agent_version, agent_build_hash
            )
            VALUES (%s, NOW() - INTERVAL '35 minutes', '2.0.0', 'build123')
        """
        cf_client.execute_sql(heartbeat_sql, (state_id,))

        # Test the view
        view_sql = """
            SELECT hostname, connectivity_status, connectivity_status_text,
                   overall_status, last_seen, agent_version
            FROM view_systems_status_table 
            WHERE hostname = %s
        """

        results = cf_client.execute_sql(view_sql, (test_hostname,))
        assert len(results) == 1, f"Expected 1 result for {test_hostname}"

        result = results[0]
        assert result["hostname"] == test_hostname
        assert result["connectivity_status"] == "offline"
        assert result["overall_status"] == "offline"
        assert (
            "heartbeat" in result["connectivity_status_text"].lower()
            or "offline" in result["connectivity_status_text"].lower()
        )

        # Save test evidence
        cf_client.save_artifact(
            json.dumps(result, indent=2, default=str),
            "offline_system_test.json",
            "Offline system test results",
        )

        # Cleanup
        cf_client.cleanup_test_data(
            {
                "agent_heartbeats": [f"system_state_id = {state_id}"],
                "system_states": [f"hostname = '{test_hostname}'"],
                "systems": [f"hostname = '{test_hostname}'"],
                "derivations": [f"derivation_name = '{test_hostname}'"],
                "commits": [f"id = {commit_id}"],
                "flakes": [f"id = {flake_id}"],
            }
        )

    def test_evaluation_failed_system_status(self, cf_client: CFTestClient):
        """Test system where latest commit evaluation failed"""
        test_hostname = "test-eval-failed-01"

        # Setup scenario where latest commit evaluation failed
        # 1. Create flake
        flake_data = {
            "flakes": [
                {
                    "name": "failed-app",
                    "repo_url": "https://github.com/test/failed-app.git",
                    "created_at": "NOW()",
                    "updated_at": "NOW()",
                }
            ]
        }
        flake_ids = cf_client.setup_test_data(flake_data)
        flake_id = flake_ids["flakes"][0]

        # 2. Create old working commit
        old_commit_sql = """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, NOW() - INTERVAL '1 day', 0)
            RETURNING id
        """
        old_commit_results = cf_client.execute_sql(
            old_commit_sql, (flake_id, "working123")
        )
        old_commit_id = old_commit_results[0]["id"]

        # 3. Create new failing commit
        new_commit_sql = """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, NOW() - INTERVAL '1 hour', 0)
            RETURNING id
        """
        new_commit_results = cf_client.execute_sql(
            new_commit_sql, (flake_id, "broken456")
        )
        new_commit_id = new_commit_results[0]["id"]

        # 4. Old working derivation
        old_derivation_path = (
            "/nix/store/working12-nixos-system-test-eval-failed-01.drv"
        )
        old_deriv_sql = """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, completed_at
            )
            VALUES (%s, 'nixos', %s, %s, 10, NOW() - INTERVAL '20 hours')
        """
        cf_client.execute_sql(
            old_deriv_sql, (old_commit_id, test_hostname, old_derivation_path)
        )

        # 5. New failed derivation
        failed_deriv_sql = """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name,
                status_id, completed_at, error_message
            )
            VALUES (%s, 'nixos', %s, 6, NOW() - INTERVAL '30 minutes', 'Build failed: syntax error')
            RETURNING id
        """
        failed_deriv_results = cf_client.execute_sql(
            failed_deriv_sql, (new_commit_id, test_hostname)
        )
        failed_deriv_id = failed_deriv_results[0]["id"]

        # 6. Register system (still on old working version)
        system_sql = """
            INSERT INTO systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, true, %s, 'fake-key')
        """
        cf_client.execute_sql(
            system_sql, (test_hostname, flake_id, old_derivation_path)
        )

        # 7. System state (on old working derivation)
        state_sql = """
            INSERT INTO system_states (
                hostname, change_reason, derivation_path, os,
                primary_ip_address, agent_compatible,
                timestamp
            )
            VALUES (
                %s, 'heartbeat', %s, 'NixOS',
                '192.168.1.103', true,
                NOW() - INTERVAL '5 minutes'
            )
            RETURNING id
        """
        state_results = cf_client.execute_sql(
            state_sql, (test_hostname, old_derivation_path)
        )
        state_id = state_results[0]["id"]

        # 8. Recent heartbeat
        heartbeat_sql = """
            INSERT INTO agent_heartbeats (
                system_state_id, timestamp, agent_version, agent_build_hash
            )
            VALUES (%s, NOW() - INTERVAL '2 minutes', '2.0.0', 'build123')
        """
        cf_client.execute_sql(heartbeat_sql, (state_id,))

        # Test the view
        view_sql = """
            SELECT hostname, connectivity_status, update_status, update_status_text,
                   overall_status, latest_derivation_status
            FROM view_systems_status_table 
            WHERE hostname = %s
        """

        results = cf_client.execute_sql(view_sql, (test_hostname,))
        assert len(results) == 1, f"Expected 1 result for {test_hostname}"

        result = results[0]
        assert result["hostname"] == test_hostname
        assert result["connectivity_status"] == "online"
        assert result["update_status"] == "evaluation_failed"
        assert result["overall_status"] == "evaluation_failed"
        assert "failed" in result["update_status_text"].lower()

        # Save test evidence
        cf_client.save_artifact(
            json.dumps(result, indent=2, default=str),
            "evaluation_failed_system_test.json",
            "Evaluation failed system test results",
        )

        # Cleanup
        cf_client.cleanup_test_data(
            {
                "agent_heartbeats": [f"system_state_id = {state_id}"],
                "system_states": [f"hostname = '{test_hostname}'"],
                "systems": [f"hostname = '{test_hostname}'"],
                "derivations": [f"derivation_name = '{test_hostname}'"],
                "commits": [f"id IN ({old_commit_id}, {new_commit_id})"],
                "flakes": [f"id = {flake_id}"],
            }
        )

    def test_multiple_systems_overview(self, cf_client: CFTestClient):
        """Test view with multiple systems in different states"""
        # This test creates several systems in different states and verifies
        # the overall view behavior

        # Create flake for all test systems
        flake_data = {
            "flakes": [
                {
                    "name": "multi-test-app",
                    "repo_url": "https://github.com/test/multi-app.git",
                    "created_at": "NOW()",
                    "updated_at": "NOW()",
                }
            ]
        }
        flake_ids = cf_client.setup_test_data(flake_data)
        flake_id = flake_ids["flakes"][0]

        # Quick setup for multiple systems
        test_systems = [
            "test-multi-never-seen",
            "test-multi-up-to-date",
            "test-multi-behind",
            "test-multi-offline",
        ]

        # Register all systems
        for hostname in test_systems:
            system_sql = """
                INSERT INTO systems (hostname, flake_id, is_active, derivation, public_key)
                VALUES (%s, %s, true, '/nix/store/test.drv', 'fake-key')
            """
            cf_client.execute_sql(system_sql, (hostname, flake_id))

        # Only create states for some systems (not never-seen)
        # This creates a realistic mixed scenario

        # Test the view returns all registered systems
        view_sql = """
            SELECT hostname, overall_status, connectivity_status, update_status
            FROM view_systems_status_table 
            WHERE hostname LIKE 'test-multi-%'
            ORDER BY hostname
        """

        results = cf_client.execute_sql(view_sql)
        assert len(results) >= len(test_systems), "Should return all registered systems"

        # Verify we have different statuses
        statuses = [r["overall_status"] for r in results]
        assert "never_seen" in statuses, "Should have never_seen systems"

        # Save comprehensive test evidence
        cf_client.save_artifact(
            json.dumps(results, indent=2, default=str),
            "multiple_systems_overview_test.json",
            "Multiple systems overview test results",
        )

        # Cleanup
        cf_client.cleanup_test_data(
            {
                "systems": ["hostname LIKE 'test-multi-%'"],
                "flakes": [f"id = {flake_id}"],
            }
        )

    @pytest.mark.slow
    def test_view_performance(self, cf_client: CFTestClient):
        """Test view performance with moderate dataset"""
        import time

        # Test basic performance by timing a full table scan
        start_time = time.time()

        results = cf_client.execute_sql(
            """
            SELECT COUNT(*) as total_systems,
                   COUNT(CASE WHEN connectivity_status = 'online' THEN 1 END) as online_systems,
                   COUNT(CASE WHEN overall_status = 'up_to_date' THEN 1 END) as up_to_date_systems
            FROM view_systems_status_table
        """
        )

        end_time = time.time()
        query_time = end_time - start_time

        # Save performance metrics
        perf_data = {
            "query_time_seconds": query_time,
            "results": results[0] if results else {},
            "test_timestamp": datetime.now().isoformat(),
        }

        cf_client.save_artifact(
            json.dumps(perf_data, indent=2),
            "systems_status_view_performance.json",
            "Systems status view performance metrics",
        )

        # Basic performance assertion - should complete within reasonable time
        assert query_time < 5.0, f"View query took {query_time:.2f}s, expected < 5s"
