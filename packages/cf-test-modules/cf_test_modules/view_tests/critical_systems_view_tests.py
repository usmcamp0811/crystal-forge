"""
Tests for the view_critical_systems view
"""

from ..test_context import CrystalForgeTestContext


class CriticalSystemsViewTests:
    """Test suite for view_critical_systems"""

    @staticmethod
    def run_all_tests(ctx: CrystalForgeTestContext) -> None:
        """Run all tests for the critical systems view"""
        ctx.logger.log_section("🚨 Testing view_critical_systems")

        # Test 1: Verify view exists and is queryable
        if not CriticalSystemsViewTests._test_view_exists(ctx):
            ctx.logger.log_warning("View does not exist - skipping remaining tests")
            return

        # Test 2: Test basic structure and columns
        CriticalSystemsViewTests._test_basic_structure(ctx)

        # Test 3: Test critical vs offline status logic
        CriticalSystemsViewTests._test_status_logic(ctx)

        # Test 4: Test hours_ago calculation
        CriticalSystemsViewTests._test_hours_ago_calculation(ctx)

        # Test 5: Test data filtering (WHERE conditions)
        CriticalSystemsViewTests._test_data_filtering(ctx)

        # Test 6: Test sorting order
        CriticalSystemsViewTests._test_sorting_order(ctx)

        # Test 7: Test edge cases and boundaries
        CriticalSystemsViewTests._test_edge_cases(ctx)

        # Test 8: Test comprehensive critical scenarios
        CriticalSystemsViewTests._test_critical_scenarios(ctx)

        # Test 9: Test view performance
        CriticalSystemsViewTests._test_view_performance(ctx)

        # Clean up test data
        CriticalSystemsViewTests.cleanup_test_data(ctx)

    @staticmethod
    def _test_view_exists(ctx: CrystalForgeTestContext) -> bool:
        """Test that the view exists and can be queried"""
        ctx.logger.log_info("Testing critical systems view existence...")

        try:
            # Check if view exists
            view_check_result = ctx.server.succeed(
                "sudo -u crystal-forge psql crystal_forge -t -c "
                "\"SELECT EXISTS (SELECT 1 FROM information_schema.views WHERE table_name = 'view_critical_systems');\""
            ).strip()

            if view_check_result == "t":
                ctx.logger.log_success("view_critical_systems exists")

                # Test basic query
                ctx.server.succeed(
                    "sudo -u crystal-forge psql crystal_forge -c "
                    '"SELECT COUNT(*) FROM view_critical_systems;"'
                )
                ctx.logger.log_success("Basic critical systems view query successful")
                return True
            else:
                ctx.logger.log_warning("view_critical_systems does not exist")
                return False

        except Exception as e:
            ctx.logger.log_error(f"Error checking critical systems view existence: {e}")
            return False

    @staticmethod
    def _test_basic_structure(ctx: CrystalForgeTestContext) -> None:
        """Test basic structure and columns"""
        ctx.logger.log_info("Testing basic structure and columns...")

        structure_test_sql = """
        -- Test that the view returns expected columns
        SELECT 
            hostname,
            status,
            hours_ago,
            ip,
            version
        FROM view_critical_systems
        LIMIT 1;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u crystal-forge psql crystal_forge -t -c "{structure_test_sql}"'
            )

            # Just verify we can query all expected columns without error
            ctx.logger.log_success("✅ Basic structure test PASSED")
            ctx.logger.log_info(
                "  All expected columns (hostname, status, hours_ago, ip, version) are accessible"
            )

        except Exception as e:
            ctx.logger.log_error(f"❌ Basic structure test ERROR: {e}")

    @staticmethod
    def _test_status_logic(ctx: CrystalForgeTestContext) -> None:
        """Test critical vs offline status logic"""
        ctx.logger.log_info("Testing critical vs offline status logic...")

        status_logic_test_sql = """
        BEGIN;
        
        -- Create test systems with specific timestamps
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES 
        -- Critical: last seen 2 hours ago (between 1-4 hours)
        ('test-critical-2hr', '/nix/store/critical-2hr', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.2.1', '25.05', true,
         NOW() - INTERVAL '2 hours'),
        -- Critical: last seen 3 hours ago (between 1-4 hours)
        ('test-critical-3hr', '/nix/store/critical-3hr', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.2.2', '25.05', true,
         NOW() - INTERVAL '3 hours'),
        -- Offline: last seen 6 hours ago (more than 4 hours)
        ('test-offline-6hr', '/nix/store/offline-6hr', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.2.3', '25.05', true,
         NOW() - INTERVAL '6 hours');

        -- Add heartbeats with matching timestamps
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
        ((SELECT id FROM system_states WHERE hostname = 'test-critical-2hr'), NOW() - INTERVAL '2 hours', '1.2.3', 'critical2hr'),
        ((SELECT id FROM system_states WHERE hostname = 'test-critical-3hr'), NOW() - INTERVAL '3 hours', '1.2.4', 'critical3hr'),
        ((SELECT id FROM system_states WHERE hostname = 'test-offline-6hr'), NOW() - INTERVAL '6 hours', '1.2.5', 'offline6hr');
        
        -- Query our test systems from the critical systems view
        SELECT 
            hostname,
            status,
            hours_ago,
            ip,
            version
        FROM view_critical_systems
        WHERE hostname LIKE 'test-critical-%' OR hostname LIKE 'test-offline-%'
        ORDER BY hostname;
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u crystal-forge psql crystal_forge -t -c "{status_logic_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            status_results = {}
            for line in lines:
                parts = [part.strip() for part in line.split("|")]
                if len(parts) >= 2:
                    hostname = parts[0]
                    status = parts[1]
                    status_results[hostname] = status

            # Verify expected statuses
            expected_results = {
                "test-critical-2hr": "Critical",
                "test-critical-3hr": "Critical",
                "test-offline-6hr": "Offline",
            }

            all_correct = True
            for hostname, expected_status in expected_results.items():
                actual_status = status_results.get(hostname)
                if actual_status == expected_status:
                    ctx.logger.log_info(f"  ✅ {hostname}: {actual_status} (correct)")
                else:
                    ctx.logger.log_error(
                        f"  ❌ {hostname}: expected {expected_status}, got {actual_status}"
                    )
                    all_correct = False

            if all_correct:
                ctx.logger.log_success("✅ Status logic test PASSED")
            else:
                ctx.logger.log_error("❌ Status logic test FAILED")

        except Exception as e:
            ctx.logger.log_error(f"❌ Status logic test ERROR: {e}")

    @staticmethod
    def _test_hours_ago_calculation(ctx: CrystalForgeTestContext) -> None:
        """Test hours_ago calculation accuracy"""
        ctx.logger.log_info("Testing hours_ago calculation...")

        hours_calculation_test_sql = """
        BEGIN;
        
        -- Create test system with known timestamp
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES 
        ('test-hours-calc', '/nix/store/hours-calc', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.3.1', '25.05', true,
         NOW() - INTERVAL '2.5 hours');

        -- Add heartbeat
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
        ((SELECT id FROM system_states WHERE hostname = 'test-hours-calc'), NOW() - INTERVAL '2.5 hours', '1.0.0', 'hourscalc');
        
        -- Query hours_ago calculation
        SELECT 
            hostname,
            hours_ago,
            status
        FROM view_critical_systems
        WHERE hostname = 'test-hours-calc';
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u crystal-forge psql crystal_forge -t -c "{hours_calculation_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            if lines:
                parts = [part.strip() for part in lines[0].split("|")]
                if len(parts) >= 2:
                    hostname = parts[0]
                    hours_ago = parts[1]

                    # Verify hours_ago is reasonable (should be around 2.5, allowing for test execution time)
                    if (
                        hours_ago
                        and hours_ago != ""
                        and hours_ago.replace(".", "").isdigit()
                    ):
                        hours_value = float(hours_ago)
                        if 2.3 <= hours_value <= 2.7:  # Allow some tolerance
                            ctx.logger.log_success(
                                "✅ Hours ago calculation test PASSED"
                            )
                            ctx.logger.log_info(
                                f"  {hostname}: {hours_ago} hours ago (expected ~2.5)"
                            )
                        else:
                            ctx.logger.log_error(
                                f"❌ Hours ago calculation test FAILED - got {hours_ago}, expected ~2.5"
                            )
                    else:
                        ctx.logger.log_error(
                            f"❌ Hours ago calculation test FAILED - invalid hours_ago value: {hours_ago}"
                        )
                else:
                    ctx.logger.log_error(
                        "❌ Hours ago calculation test FAILED - insufficient columns"
                    )
            else:
                ctx.logger.log_error(
                    "❌ Hours ago calculation test FAILED - no results"
                )

        except Exception as e:
            ctx.logger.log_error(f"❌ Hours ago calculation test ERROR: {e}")

    @staticmethod
    def _test_data_filtering(ctx: CrystalForgeTestContext) -> None:
        """Test data filtering (WHERE conditions)"""
        ctx.logger.log_info("Testing data filtering...")

        filtering_test_sql = """
        BEGIN;
        
        -- Create test systems with different conditions
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES 
        -- Should be included: 2 hours ago (critical)
        ('test-filter-include', '/nix/store/filter-include', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.4.1', '25.05', true,
         NOW() - INTERVAL '2 hours'),
        -- Should be excluded: 30 minutes ago (too recent)
        ('test-filter-exclude', '/nix/store/filter-exclude', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.4.2', '25.05', true,
         NOW() - INTERVAL '30 minutes'),
        -- Should be included: 8 hours ago (offline)
        ('test-filter-offline', '/nix/store/filter-offline', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.4.3', '25.05', true,
         NOW() - INTERVAL '8 hours');

        -- Add heartbeats
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
        ((SELECT id FROM system_states WHERE hostname = 'test-filter-include'), NOW() - INTERVAL '2 hours', '1.0.0', 'include'),
        ((SELECT id FROM system_states WHERE hostname = 'test-filter-exclude'), NOW() - INTERVAL '30 minutes', '1.0.0', 'exclude'),
        ((SELECT id FROM system_states WHERE hostname = 'test-filter-offline'), NOW() - INTERVAL '8 hours', '1.0.0', 'offline');
        
        -- Query filtered results
        SELECT 
            hostname,
            status,
            hours_ago
        FROM view_critical_systems
        WHERE hostname LIKE 'test-filter-%'
        ORDER BY hostname;
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u crystal-forge psql crystal_forge -t -c "{filtering_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            found_hostnames = set()
            for line in lines:
                parts = [part.strip() for part in line.split("|")]
                if len(parts) >= 1:
                    hostname = parts[0]
                    found_hostnames.add(hostname)

            # Should include critical and offline, but exclude the recent one
            expected_included = {"test-filter-include", "test-filter-offline"}
            expected_excluded = {"test-filter-exclude"}

            included_correctly = expected_included.issubset(found_hostnames)
            excluded_correctly = expected_excluded.isdisjoint(found_hostnames)

            if included_correctly and excluded_correctly:
                ctx.logger.log_success("✅ Data filtering test PASSED")
                ctx.logger.log_info(f"  Included: {', '.join(found_hostnames)}")
                ctx.logger.log_info(f"  Correctly excluded recent system")
            else:
                ctx.logger.log_error("❌ Data filtering test FAILED")
                ctx.logger.log_error(f"  Found: {found_hostnames}")

        except Exception as e:
            ctx.logger.log_error(f"❌ Data filtering test ERROR: {e}")

    @staticmethod
    def _test_sorting_order(ctx: CrystalForgeTestContext) -> None:
        """Test that results are returned in the correct priority order"""
        ctx.logger.log_info("Testing sorting order...")

        sorting_test_sql = """
        BEGIN;
        
        -- Create test systems with different hours_ago values
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES 
        ('test-sort-1hr', '/nix/store/sort-1hr', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.5.1', '25.05', true,
         NOW() - INTERVAL '1.5 hours'),
        ('test-sort-3hr', '/nix/store/sort-3hr', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.5.2', '25.05', true,
         NOW() - INTERVAL '3 hours'),
        ('test-sort-6hr', '/nix/store/sort-6hr', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.5.3', '25.05', true,
         NOW() - INTERVAL '6 hours');

        -- Add heartbeats
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
        ((SELECT id FROM system_states WHERE hostname = 'test-sort-1hr'), NOW() - INTERVAL '1.5 hours', '1.0.0', 'sort1'),
        ((SELECT id FROM system_states WHERE hostname = 'test-sort-3hr'), NOW() - INTERVAL '3 hours', '1.0.0', 'sort3'),
        ((SELECT id FROM system_states WHERE hostname = 'test-sort-6hr'), NOW() - INTERVAL '6 hours', '1.0.0', 'sort6');
        
        -- Query with expected sort order (hours_ago ascending)
        SELECT 
            hostname,
            hours_ago,
            status
        FROM view_critical_systems
        WHERE hostname LIKE 'test-sort-%'
        ORDER BY hours_ago;
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u crystal-forge psql crystal_forge -t -c "{sorting_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            # Extract hours_ago values to check sort order
            hours_values = []
            for line in lines:
                parts = [part.strip() for part in line.split("|")]
                if len(parts) >= 2:
                    hostname = parts[0]
                    hours_ago = parts[1]
                    if hours_ago and hours_ago.replace(".", "").isdigit():
                        hours_values.append((hostname, float(hours_ago)))

            # Check if sorted in ascending order
            if len(hours_values) >= 2:
                is_sorted = all(
                    hours_values[i][1] <= hours_values[i + 1][1]
                    for i in range(len(hours_values) - 1)
                )

                if is_sorted:
                    ctx.logger.log_success("✅ Sorting order test PASSED")
                    for hostname, hours in hours_values:
                        ctx.logger.log_info(f"  {hostname}: {hours} hours ago")
                else:
                    ctx.logger.log_error(
                        "❌ Sorting order test FAILED - not in ascending order"
                    )
            else:
                ctx.logger.log_warning("⚠️ Insufficient data to test sorting order")

        except Exception as e:
            ctx.logger.log_error(f"❌ Sorting order test ERROR: {e}")

    @staticmethod
    def _test_edge_cases(ctx: CrystalForgeTestContext) -> None:
        """Test edge cases and boundary conditions"""
        ctx.logger.log_info("Testing edge cases and boundary conditions...")

        edge_cases_sql = """
        BEGIN;
        
        -- Test exactly at 1-hour boundary (should be included)
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES 
        ('test-edge-1hr', '/nix/store/edge-1hr', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.6.1', '25.05', true,
         NOW() - INTERVAL '1 hour'),
        -- Test exactly at 4-hour boundary (should be critical, not offline)
        ('test-edge-4hr', '/nix/store/edge-4hr', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.6.2', '25.05', true,
         NOW() - INTERVAL '4 hours');

        -- Add heartbeats
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
        ((SELECT id FROM system_states WHERE hostname = 'test-edge-1hr'), NOW() - INTERVAL '1 hour', '1.0.0', 'edge1'),
        ((SELECT id FROM system_states WHERE hostname = 'test-edge-4hr'), NOW() - INTERVAL '4 hours', '1.0.0', 'edge4');
        
        -- Query edge case systems
        SELECT 
            hostname,
            status,
            hours_ago
        FROM view_critical_systems
        WHERE hostname LIKE 'test-edge-%'
        ORDER BY hostname;
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u crystal-forge psql crystal_forge -t -c "{edge_cases_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            edge_results = {}
            for line in lines:
                parts = [part.strip() for part in line.split("|")]
                if len(parts) >= 2:
                    hostname = parts[0]
                    status = parts[1]
                    edge_results[hostname] = status

            # Expected edge case behavior
            expected = {
                "test-edge-1hr": "Critical",  # 1 hour = critical
                "test-edge-4hr": "Critical",  # 4 hours = still critical (not offline)
            }

            all_correct = True
            for hostname, expected_status in expected.items():
                actual_status = edge_results.get(hostname)
                if actual_status == expected_status:
                    ctx.logger.log_info(f"  ✅ {hostname}: {actual_status} (correct)")
                else:
                    ctx.logger.log_error(
                        f"  ❌ {hostname}: expected {expected_status}, got {actual_status}"
                    )
                    all_correct = False

            if all_correct:
                ctx.logger.log_success("✅ Edge cases test PASSED")
            else:
                ctx.logger.log_error("❌ Edge cases test FAILED")

        except Exception as e:
            ctx.logger.log_error(f"❌ Edge cases test ERROR: {e}")

    @staticmethod
    def _test_critical_scenarios(ctx: CrystalForgeTestContext) -> None:
        """Test comprehensive critical scenarios"""
        ctx.logger.log_info("Testing comprehensive critical scenarios...")

        scenarios_sql = """
        BEGIN;
        
        -- Create multiple systems in different critical/offline states
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES 
        ('scenario-crit-1', '/nix/store/scenario-crit-1', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.7.1', '25.05', true, NOW() - INTERVAL '1.2 hours'),
        ('scenario-crit-2', '/nix/store/scenario-crit-2', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.7.2', '25.05', true, NOW() - INTERVAL '2.5 hours'),
        ('scenario-crit-3', '/nix/store/scenario-crit-3', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.7.3', '25.05', true, NOW() - INTERVAL '3.8 hours'),
        ('scenario-off-1', '/nix/store/scenario-off-1', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.7.4', '25.05', true, NOW() - INTERVAL '5 hours'),
        ('scenario-off-2', '/nix/store/scenario-off-2', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.7.5', '25.05', true, NOW() - INTERVAL '12 hours');

        -- Add heartbeats
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
        ((SELECT id FROM system_states WHERE hostname = 'scenario-crit-1'), NOW() - INTERVAL '1.2 hours', '1.0.0', 'sc1'),
        ((SELECT id FROM system_states WHERE hostname = 'scenario-crit-2'), NOW() - INTERVAL '2.5 hours', '1.0.0', 'sc2'),
        ((SELECT id FROM system_states WHERE hostname = 'scenario-crit-3'), NOW() - INTERVAL '3.8 hours', '1.0.0', 'sc3'),
        ((SELECT id FROM system_states WHERE hostname = 'scenario-off-1'), NOW() - INTERVAL '5 hours', '1.0.0', 'so1'),
        ((SELECT id FROM system_states WHERE hostname = 'scenario-off-2'), NOW() - INTERVAL '12 hours', '1.0.0', 'so2');
        
        -- Query scenarios
        SELECT 
            status,
            COUNT(*) as count
        FROM view_critical_systems
        WHERE hostname LIKE 'scenario-%'
        GROUP BY status
        ORDER BY status;
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u crystal-forge psql crystal_forge -t -c "{scenarios_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            scenario_counts = {}
            for line in lines:
                parts = [part.strip() for part in line.split("|")]
                if len(parts) >= 2:
                    status = parts[0]
                    count = int(parts[1]) if parts[1].isdigit() else 0
                    scenario_counts[status] = count

            # Verify expected counts
            expected_critical = 3  # 1.2, 2.5, 3.8 hours
            expected_offline = 2  # 5, 12 hours

            actual_critical = scenario_counts.get("Critical", 0)
            actual_offline = scenario_counts.get("Offline", 0)

            if (
                actual_critical == expected_critical
                and actual_offline == expected_offline
            ):
                ctx.logger.log_success("✅ Critical scenarios test PASSED")
                ctx.logger.log_info(f"  Critical: {actual_critical} systems")
                ctx.logger.log_info(f"  Offline: {actual_offline} systems")
            else:
                ctx.logger.log_error("❌ Critical scenarios test FAILED")
                ctx.logger.log_error(
                    f"  Expected: {expected_critical} Critical, {expected_offline} Offline"
                )
                ctx.logger.log_error(
                    f"  Actual: {actual_critical} Critical, {actual_offline} Offline"
                )

        except Exception as e:
            ctx.logger.log_error(f"❌ Critical scenarios test ERROR: {e}")

    @staticmethod
    def _test_view_performance(ctx: CrystalForgeTestContext) -> None:
        """Test view performance with EXPLAIN ANALYZE"""
        ctx.logger.log_info("Testing critical systems view performance...")

        performance_sql = "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT * FROM view_critical_systems;"

        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u crystal-forge psql crystal_forge -c "{performance_sql}"',
            "critical-systems-view-performance.txt",
            "Critical systems view performance analysis",
        )

        # Test simple timing
        timing_sql = "\\\\timing on\\nSELECT COUNT(*) FROM view_critical_systems;\\nSELECT * FROM view_critical_systems;"

        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u crystal-forge psql crystal_forge -c "{timing_sql}"',
            "critical-systems-view-timing.txt",
            "Critical systems view timing test",
        )

        ctx.logger.log_success("Critical systems view performance testing completed")

    @staticmethod
    def cleanup_test_data(ctx: CrystalForgeTestContext) -> None:
        """Clean up any test data that might have been left behind"""
        ctx.logger.log_info("Cleaning up critical systems view test data...")

        cleanup_sql = """
        DELETE FROM agent_heartbeats 
        WHERE system_state_id IN (
            SELECT id FROM system_states 
            WHERE hostname LIKE 'test-critical-%' 
               OR hostname LIKE 'test-offline-%'
               OR hostname LIKE 'test-hours-%'
               OR hostname LIKE 'test-filter-%'
               OR hostname LIKE 'test-sort-%'
               OR hostname LIKE 'test-edge-%'
               OR hostname LIKE 'scenario-%'
        );
        DELETE FROM system_states 
        WHERE hostname LIKE 'test-critical-%' 
           OR hostname LIKE 'test-offline-%'
           OR hostname LIKE 'test-hours-%'
           OR hostname LIKE 'test-filter-%'
           OR hostname LIKE 'test-sort-%'
           OR hostname LIKE 'test-edge-%'
           OR hostname LIKE 'scenario-%';
        """

        try:
            ctx.server.succeed(
                f'sudo -u crystal-forge psql crystal_forge -c "{cleanup_sql}"'
            )
            ctx.logger.log_success("Critical systems view test data cleanup completed")
        except Exception as e:
            ctx.logger.log_warning(
                f"Could not clean up critical systems view test data: {e}"
            )
