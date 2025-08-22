"""
Tests for the view_systems_status_table view
"""

from ..test_context import CrystalForgeTestContext


class SystemsStatusTableTests:
    """Test suite for view_systems_status_table"""

    @staticmethod
    def run_all_tests(ctx: CrystalForgeTestContext) -> None:
        """Run all tests for the systems status table view"""
        ctx.logger.log_section("🔍 Testing view_systems_status_table")

        # Test 1: Verify view exists and is queryable
        if not SystemsStatusTableTests._test_view_exists(ctx):
            ctx.logger.log_warning("View does not exist - skipping remaining tests")
            return

        # Test 2: Test status logic scenarios with assertions
        SystemsStatusTableTests._test_status_logic_scenarios(ctx)

        # Test 3: Test heartbeat vs system state interactions
        SystemsStatusTableTests._test_heartbeat_system_state_interactions(ctx)

        # Test 4: Test edge cases and boundary conditions
        SystemsStatusTableTests._test_edge_cases(ctx)

        # Test 5: Verify view performance
        SystemsStatusTableTests._test_view_performance(ctx)

        # Clean up test data
        SystemsStatusTableTests.cleanup_test_data(ctx)

    @staticmethod
    def _test_view_exists(ctx: CrystalForgeTestContext) -> bool:
        """Test that the view exists and can be queried"""
        ctx.logger.log_info("Testing view existence...")

        # First, check if we can connect to the database at all
        try:
            ctx.server.succeed(
                'sudo -u crystal-forge psql crystal_forge -c "SELECT 1;" > /dev/null'
            )
            ctx.logger.log_success("Database connection verified")
        except Exception as e:
            ctx.logger.log_error(f"Cannot connect to database: {e}")
            return False

        # Check if view exists - capture output for debugging
        try:
            view_check_result = ctx.server.succeed(
                "sudo -u crystal-forge psql crystal_forge -t -c "
                "\"SELECT EXISTS (SELECT 1 FROM information_schema.views WHERE table_name = 'view_systems_status_table');\""
            ).strip()

            if view_check_result == "t":
                ctx.logger.log_success("view_systems_status_table exists")

                # Test basic query if view exists
                ctx.server.succeed(
                    "sudo -u crystal-forge psql crystal_forge -c "
                    '"SELECT COUNT(*) FROM view_systems_status_table;"'
                )
                ctx.logger.log_success("Basic view query successful")
                return True
            else:
                ctx.logger.log_warning("view_systems_status_table does not exist")

                # List all views for debugging
                ctx.logger.capture_command_output(
                    ctx.server,
                    "sudo -u crystal-forge psql crystal_forge -c \"SELECT table_name FROM information_schema.views WHERE table_schema = 'public';\"",
                    "existing-views.txt",
                    "List of existing views",
                )
                return False

        except Exception as e:
            ctx.logger.log_error(f"Error checking view existence: {e}")

            # Capture more debug info
            ctx.logger.capture_command_output(
                ctx.server,
                'sudo -u crystal-forge psql crystal_forge -c "\\d"',
                "database-schema.txt",
                "Database schema debug info",
            )
            return False

    @staticmethod
    def _test_status_logic_scenarios(ctx: CrystalForgeTestContext) -> None:
        """Test specific status logic scenarios with assertions"""
        ctx.logger.log_info("Testing status logic scenarios with assertions...")

        test_scenarios = [
            {
                "name": "Recent system state, no heartbeat (should be 'starting')",
                "setup_sql": """
                INSERT INTO system_states (
                    hostname, derivation_path, change_reason, os, kernel,
                    memory_gb, uptime_secs, cpu_brand, cpu_cores,
                    primary_ip_address, nixos_version, agent_compatible,
                    timestamp
                ) VALUES (
                    'test-starting', '/nix/store/test-starting', 'startup', '25.05', '6.6.89',
                    16384, 3600, 'Test CPU', 8, '10.0.0.200', '25.05', true,
                    NOW() - INTERVAL '10 minutes'
                );
                """,
                "expected_status": "starting",
                "expected_status_text": "System starting up",
            },
            {
                "name": "Old system state, no heartbeat (should be 'offline')",
                "setup_sql": """
                INSERT INTO system_states (
                    hostname, derivation_path, change_reason, os, kernel,
                    memory_gb, uptime_secs, cpu_brand, cpu_cores,
                    primary_ip_address, nixos_version, agent_compatible,
                    timestamp
                ) VALUES (
                    'test-offline', '/nix/store/test-offline', 'startup', '25.05', '6.6.89',
                    8192, 3600, 'Test CPU', 4, '10.0.0.201', '25.05', true,
                    NOW() - INTERVAL '2 hours'
                );
                """,
                "expected_status": "offline",
                "expected_status_text": "No heartbeats",
            },
            {
                "name": "Recent heartbeat, recent system state (should be 'online')",
                "setup_sql": """
                INSERT INTO system_states (
                    hostname, derivation_path, change_reason, os, kernel,
                    memory_gb, uptime_secs, cpu_brand, cpu_cores,
                    primary_ip_address, nixos_version, agent_compatible,
                    timestamp
                ) VALUES (
                    'test-online', '/nix/store/test-online', 'startup', '25.05', '6.6.89',
                    16384, 7200, 'Test CPU', 8, '10.0.0.202', '25.05', true,
                    NOW() - INTERVAL '5 minutes'
                );
                
                INSERT INTO agent_heartbeats (
                    system_state_id, timestamp, agent_version, agent_build_hash
                ) VALUES (
                    (SELECT id FROM system_states WHERE hostname = 'test-online' ORDER BY timestamp DESC LIMIT 1),
                    NOW() - INTERVAL '2 minutes',
                    '1.2.3',
                    'abc123def'
                );
                """,
                "expected_status": "online",
                "expected_status_text": "Active",
            },
            {
                "name": "Old heartbeat, recent system state (should be 'starting')",
                "setup_sql": """
                INSERT INTO system_states (
                    hostname, derivation_path, change_reason, os, kernel,
                    memory_gb, uptime_secs, cpu_brand, cpu_cores,
                    primary_ip_address, nixos_version, agent_compatible,
                    timestamp
                ) VALUES (
                    'test-restarted', '/nix/store/test-restarted', 'startup', '25.05', '6.6.89',
                    16384, 1800, 'Test CPU', 8, '10.0.0.203', '25.05', true,
                    NOW() - INTERVAL '5 minutes'
                );
                
                INSERT INTO agent_heartbeats (
                    system_state_id, timestamp, agent_version, agent_build_hash
                ) VALUES (
                    (SELECT id FROM system_states WHERE hostname = 'test-restarted' ORDER BY timestamp DESC LIMIT 1),
                    NOW() - INTERVAL '2 hours',
                    '1.2.3',
                    'abc123def'
                );
                """,
                "expected_status": "starting",
                "expected_status_text": "System restarted",
            },
            {
                "name": "Old heartbeat, old system state (should be 'stale')",
                "setup_sql": """
                INSERT INTO system_states (
                    hostname, derivation_path, change_reason, os, kernel,
                    memory_gb, uptime_secs, cpu_brand, cpu_cores,
                    primary_ip_address, nixos_version, agent_compatible,
                    timestamp
                ) VALUES (
                    'test-stale', '/nix/store/test-stale', 'startup', '25.05', '6.6.89',
                    8192, 86400, 'Test CPU', 4, '10.0.0.204', '25.05', true,
                    NOW() - INTERVAL '2 hours'
                );
                
                INSERT INTO agent_heartbeats (
                    system_state_id, timestamp, agent_version, agent_build_hash
                ) VALUES (
                    (SELECT id FROM system_states WHERE hostname = 'test-stale' ORDER BY timestamp DESC LIMIT 1),
                    NOW() - INTERVAL '1 hour',
                    '1.2.3',
                    'abc123def'
                );
                """,
                "expected_status": "stale",
                "expected_status_text": "Heartbeat overdue",
            },
        ]

        for i, scenario in enumerate(test_scenarios):
            ctx.logger.log_info(f"Testing scenario: {scenario['name']}")

            # Setup test data and query view
            test_sql = f"""
            BEGIN;
            
            {scenario['setup_sql']}
            
            SELECT 
                hostname,
                status,
                status_text,
                last_seen,
                version,
                uptime,
                ip_address
            FROM view_systems_status_table 
            WHERE hostname LIKE 'test-%'
            ORDER BY hostname;
            
            ROLLBACK;
            """

            try:
                result = ctx.server.succeed(
                    f'sudo -u crystal-forge psql crystal_forge -t -c "{test_sql}"'
                )

                # Parse and validate results
                lines = [
                    line.strip() for line in result.strip().split("\n") if line.strip()
                ]

                if not lines:
                    ctx.logger.log_error(
                        f"No results returned for scenario: {scenario['name']}"
                    )
                    continue

                # Find the line for our test system
                test_hostname = scenario["setup_sql"].split("'")[
                    1
                ]  # Extract hostname from INSERT
                matching_line = None
                for line in lines:
                    if test_hostname in line:
                        matching_line = line
                        break

                if not matching_line:
                    ctx.logger.log_error(
                        f"No result found for hostname {test_hostname}"
                    )
                    continue

                # Parse the result line (pipe-separated values)
                parts = [part.strip() for part in matching_line.split("|")]

                if len(parts) < 3:
                    ctx.logger.log_error(f"Invalid result format: {matching_line}")
                    continue

                actual_hostname = parts[0]
                actual_status = parts[1]
                actual_status_text = parts[2]

                # Assertions
                hostname_match = actual_hostname == test_hostname
                status_match = actual_status == scenario["expected_status"]
                status_text_match = (
                    actual_status_text == scenario["expected_status_text"]
                )

                if hostname_match and status_match and status_text_match:
                    ctx.logger.log_success(f"✅ Scenario '{scenario['name']}' PASSED")
                else:
                    ctx.logger.log_error(f"❌ Scenario '{scenario['name']}' FAILED")
                    ctx.logger.log_error(
                        f"  Expected: hostname={test_hostname}, status={scenario['expected_status']}, status_text={scenario['expected_status_text']}"
                    )
                    ctx.logger.log_error(
                        f"  Actual:   hostname={actual_hostname}, status={actual_status}, status_text={actual_status_text}"
                    )

            except Exception as e:
                ctx.logger.log_error(f"❌ Scenario '{scenario['name']}' ERROR: {e}")

        ctx.logger.log_success("Status logic scenarios testing completed")

    @staticmethod
    def _test_heartbeat_system_state_interactions(ctx: CrystalForgeTestContext) -> None:
        """Test the interaction between heartbeats and system states"""
        ctx.logger.log_info("Testing heartbeat and system state interactions...")

        interaction_test_sql = """
        BEGIN;
        
        -- Create a system with multiple states and heartbeats
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES 
        -- Older state
        ('test-multi', '/nix/store/test-multi-old', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.0.300', '25.05', true,
         NOW() - INTERVAL '2 hours'),
        -- Newer state 
        ('test-multi', '/nix/store/test-multi-new', 'config_change', '25.05', '6.6.89',
         16384, 7200, 'Test CPU', 8, '10.0.0.301', '25.05', true,
         NOW() - INTERVAL '10 minutes');

        -- Add heartbeats for both states
        INSERT INTO agent_heartbeats (
            system_state_id, timestamp, agent_version, agent_build_hash
        ) VALUES 
        -- Old heartbeat on old state
        ((SELECT id FROM system_states WHERE hostname = 'test-multi' ORDER BY timestamp ASC LIMIT 1),
         NOW() - INTERVAL '1.5 hours', '1.0.0', 'old123'),
        -- Recent heartbeat on new state  
        ((SELECT id FROM system_states WHERE hostname = 'test-multi' ORDER BY timestamp DESC LIMIT 1),
         NOW() - INTERVAL '5 minutes', '1.2.0', 'new456');

        -- Query the view and validate it uses the latest system state and heartbeat
        SELECT 
            hostname,
            status,
            status_text,
            version,
            ip_address,
            uptime
        FROM view_systems_status_table 
        WHERE hostname = 'test-multi';
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u crystal-forge psql crystal_forge -t -c "{interaction_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            if lines:
                parts = [part.strip() for part in lines[0].split("|")]
                if len(parts) >= 5:
                    hostname = parts[0]
                    status = parts[1]
                    status_text = parts[2]
                    version = parts[3]
                    ip_address = parts[4]

                    # Assertions for latest state/heartbeat usage
                    hostname_correct = hostname == "test-multi"
                    status_should_be_online = status == "online"
                    version_is_latest = version == "1.2.0"
                    ip_is_latest = ip_address == "10.0.0.301"

                    if (
                        hostname_correct
                        and status_should_be_online
                        and version_is_latest
                        and ip_is_latest
                    ):
                        ctx.logger.log_success(
                            "✅ Heartbeat/System State interaction test PASSED"
                        )
                        ctx.logger.log_info(
                            f"  Uses latest system state IP: {ip_address}"
                        )
                        ctx.logger.log_info(
                            f"  Uses latest heartbeat version: {version}"
                        )
                    else:
                        ctx.logger.log_error(
                            "❌ Heartbeat/System State interaction test FAILED"
                        )
                        ctx.logger.log_error(f"  hostname_correct: {hostname_correct}")
                        ctx.logger.log_error(
                            f"  status_should_be_online: {status_should_be_online}"
                        )
                        ctx.logger.log_error(
                            f"  version_is_latest: {version_is_latest} (got {version})"
                        )
                        ctx.logger.log_error(
                            f"  ip_is_latest: {ip_is_latest} (got {ip_address})"
                        )
                else:
                    ctx.logger.log_error("❌ Insufficient columns in result")
            else:
                ctx.logger.log_error("❌ No results returned for interaction test")

        except Exception as e:
            ctx.logger.log_error(
                f"❌ Heartbeat/System State interaction test ERROR: {e}"
            )

    @staticmethod
    def _test_edge_cases(ctx: CrystalForgeTestContext) -> None:
        """Test edge cases and boundary conditions"""
        ctx.logger.log_info("Testing edge cases and boundary conditions...")

        edge_case_tests = [
            {
                "name": "System exactly at 30-minute boundary",
                "setup_sql": """
                INSERT INTO system_states (
                    hostname, derivation_path, change_reason, os, kernel,
                    memory_gb, uptime_secs, cpu_brand, cpu_cores,
                    primary_ip_address, nixos_version, agent_compatible,
                    timestamp
                ) VALUES (
                    'test-boundary', '/nix/store/test-boundary', 'startup', '25.05', '6.6.89',
                    8192, 1800, 'Test CPU', 4, '10.0.0.400', '25.05', true,
                    NOW() - INTERVAL '30 minutes'
                );
                """,
                "expected_status": "offline",
            },
            {
                "name": "System with NULL optional fields",
                "setup_sql": """
                INSERT INTO system_states (
                    hostname, derivation_path, change_reason, os, kernel,
                    memory_gb, uptime_secs, cpu_brand, cpu_cores,
                    primary_ip_address, nixos_version, agent_compatible,
                    timestamp
                ) VALUES (
                    'test-nulls', '/nix/store/test-nulls', 'startup', NULL, NULL,
                    NULL, NULL, NULL, NULL, NULL, NULL, true,
                    NOW() - INTERVAL '5 minutes'
                );
                """,
                "expected_status": "starting",
            },
        ]

        for test_case in edge_case_tests:
            test_sql = f"""
            BEGIN;
            
            {test_case['setup_sql']}
            
            SELECT status FROM view_systems_status_table 
            WHERE hostname LIKE 'test-%' 
            AND hostname = '{test_case['setup_sql'].split("'")[1]}';
            
            ROLLBACK;
            """

            try:
                result = ctx.server.succeed(
                    f'sudo -u crystal-forge psql crystal_forge -t -c "{test_sql}"'
                ).strip()

                if result == test_case["expected_status"]:
                    ctx.logger.log_success(f"✅ Edge case '{test_case['name']}' PASSED")
                else:
                    ctx.logger.log_error(f"❌ Edge case '{test_case['name']}' FAILED")
                    ctx.logger.log_error(
                        f"  Expected: {test_case['expected_status']}, Got: {result}"
                    )

            except Exception as e:
                ctx.logger.log_error(f"❌ Edge case '{test_case['name']}' ERROR: {e}")

    @staticmethod
    def _test_view_performance(ctx: CrystalForgeTestContext) -> None:
        """Test view performance with EXPLAIN ANALYZE"""
        ctx.logger.log_info("Testing view performance...")

        performance_sql = "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT * FROM view_systems_status_table;"

        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u crystal-forge psql crystal_forge -c "{performance_sql}"',
            "view-performance-analysis.txt",
            "View performance analysis",
        )

        # Also test a simple timing
        timing_sql = "\\\\timing on\\nSELECT COUNT(*) FROM view_systems_status_table;\\nSELECT * FROM view_systems_status_table LIMIT 5;"

        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u crystal-forge psql crystal_forge -c "{timing_sql}"',
            "view-timing-test.txt",
            "View timing test",
        )

        ctx.logger.log_success("Performance testing completed")

    @staticmethod
    def cleanup_test_data(ctx: CrystalForgeTestContext) -> None:
        """Clean up any test data that might have been left behind"""
        ctx.logger.log_info("Cleaning up view test data...")

        cleanup_sql = """
        DELETE FROM agent_heartbeats 
        WHERE system_state_id IN (
            SELECT id FROM system_states WHERE hostname LIKE 'test-%'
        );
        DELETE FROM system_states WHERE hostname LIKE 'test-%';
        """

        try:
            ctx.server.succeed(
                f'sudo -u crystal-forge psql crystal_forge -c "{cleanup_sql}"'
            )
            ctx.logger.log_success("View test data cleanup completed")
        except Exception as e:
            ctx.logger.log_warning(f"Could not clean up test data: {e}")
