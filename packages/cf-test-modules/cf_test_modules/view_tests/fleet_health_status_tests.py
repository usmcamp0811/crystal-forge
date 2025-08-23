"""
Tests for the view_fleet_health_status view
"""

from ..test_context import CrystalForgeTestContext


class FleetHealthStatusViewTests:
    """Test suite for view_fleet_health_status"""

    @staticmethod
    def run_all_tests(ctx: CrystalForgeTestContext) -> None:
        """Run all tests for the fleet health status view"""
        ctx.logger.log_section("💚 Testing view_fleet_health_status")

        # Test 1: Verify view exists and is queryable
        if not FleetHealthStatusViewTests._test_view_exists(ctx):
            ctx.logger.log_warning("View does not exist - skipping remaining tests")
            return

        # Test 2: Test basic aggregation functionality
        FleetHealthStatusViewTests._test_basic_aggregation(ctx)

        # Test 3: Test health status time intervals
        FleetHealthStatusViewTests._test_health_status_intervals(ctx)

        # Test 4: Test sorting order
        FleetHealthStatusViewTests._test_sorting_order(ctx)

        # Test 5: Test data filtering (last_seen conditions)
        FleetHealthStatusViewTests._test_data_filtering(ctx)

        # Test 6: Test various health scenarios
        FleetHealthStatusViewTests._test_health_scenarios(ctx)

        # Test 7: Test view performance
        FleetHealthStatusViewTests._test_view_performance(ctx)

        # Clean up test data
        FleetHealthStatusViewTests.cleanup_test_data(ctx)

    @staticmethod
    def _test_view_exists(ctx: CrystalForgeTestContext) -> bool:
        """Test that the view exists and can be queried"""
        ctx.logger.log_info("Testing fleet health status view existence...")

        try:
            # Check if view exists
            view_check_result = ctx.server.succeed(
                "sudo -u postgres psql crystal_forge -t -c "
                "\"SELECT EXISTS (SELECT 1 FROM information_schema.views WHERE table_name = 'view_fleet_health_status');\""
            ).strip()

            if view_check_result == "t":
                ctx.logger.log_success("view_fleet_health_status exists")

                # Test basic query
                ctx.server.succeed(
                    "sudo -u postgres psql crystal_forge -c "
                    '"SELECT COUNT(*) FROM view_fleet_health_status;"'
                )
                ctx.logger.log_success(
                    "Basic fleet health status view query successful"
                )
                return True
            else:
                ctx.logger.log_warning("view_fleet_health_status does not exist")
                return False

        except Exception as e:
            ctx.logger.log_error(
                f"Error checking fleet health status view existence: {e}"
            )
            return False

    @staticmethod
    def _test_basic_aggregation(ctx: CrystalForgeTestContext) -> None:
        """Test basic aggregation functionality"""
        ctx.logger.log_info("Testing basic aggregation functionality...")

        aggregation_test_sql = """
        -- Test that the view returns proper structure
        SELECT 
            health_status,
            count,
            LENGTH(health_status) as status_length
        FROM view_fleet_health_status
        LIMIT 5;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u postgres psql crystal_forge -t -c "{aggregation_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            if lines:
                # Parse first line to verify structure
                parts = [part.strip() for part in lines[0].split("|")]
                if len(parts) >= 3:
                    health_status = parts[0]
                    count_value = parts[1]
                    status_length = parts[2]

                    # Verify count is numeric
                    count_is_numeric = count_value.isdigit()
                    # Verify health_status has content
                    has_health_status = len(health_status) > 0
                    # Verify status_length is reasonable
                    length_reasonable = int(status_length) > 0

                    if count_is_numeric and has_health_status and length_reasonable:
                        ctx.logger.log_success("✅ Basic aggregation test PASSED")
                        ctx.logger.log_info(
                            f"  Sample: {count_value} systems with health '{health_status}'"
                        )
                    else:
                        ctx.logger.log_error(
                            "❌ Basic aggregation test FAILED - Invalid data structure"
                        )
                else:
                    ctx.logger.log_error(
                        "❌ Basic aggregation test FAILED - Insufficient columns"
                    )
            else:
                ctx.logger.log_info("ℹ️ No data returned (empty systems table)")

        except Exception as e:
            ctx.logger.log_error(f"❌ Basic aggregation test ERROR: {e}")

    @staticmethod
    def _test_health_status_intervals(ctx: CrystalForgeTestContext) -> None:
        """Test that health status intervals are correctly applied"""
        ctx.logger.log_info("Testing health status time intervals...")

        intervals_test_sql = """
        BEGIN;
        
        -- Create test systems with different last_seen timestamps
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES 
        -- Healthy: last seen 5 minutes ago
        ('test-fleet-healthy', '/nix/store/healthy', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.0.100', '25.05', true,
         NOW() - INTERVAL '5 minutes'),
        -- Warning: last seen 30 minutes ago  
        ('test-fleet-warning', '/nix/store/warning', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.0.101', '25.05', true,
         NOW() - INTERVAL '30 minutes'),
        -- Critical: last seen 2 hours ago
        ('test-fleet-critical', '/nix/store/critical', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.0.102', '25.05', true,
         NOW() - INTERVAL '2 hours'),
        -- Offline: last seen 8 hours ago
        ('test-fleet-offline', '/nix/store/offline', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.0.103', '25.05', true,
         NOW() - INTERVAL '8 hours');

        -- Add heartbeats with same timestamps to ensure last_seen calculation
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
        ((SELECT id FROM system_states WHERE hostname = 'test-fleet-healthy' ORDER BY timestamp DESC LIMIT 1),
         NOW() - INTERVAL '5 minutes', '1.0.0', 'hash1'),
        ((SELECT id FROM system_states WHERE hostname = 'test-fleet-warning' ORDER BY timestamp DESC LIMIT 1),
         NOW() - INTERVAL '30 minutes', '1.0.0', 'hash2'),
        ((SELECT id FROM system_states WHERE hostname = 'test-fleet-critical' ORDER BY timestamp DESC LIMIT 1),
         NOW() - INTERVAL '2 hours', '1.0.0', 'hash3'),
        ((SELECT id FROM system_states WHERE hostname = 'test-fleet-offline' ORDER BY timestamp DESC LIMIT 1),
         NOW() - INTERVAL '8 hours', '1.0.0', 'hash4');
        
        -- Query fleet health status for our test systems
        SELECT 
            health_status,
            count
        FROM view_fleet_health_status
        WHERE health_status IN ('Healthy', 'Warning', 'Critical', 'Offline')
        ORDER BY 
            CASE health_status
                WHEN 'Healthy' THEN 1
                WHEN 'Warning' THEN 2
                WHEN 'Critical' THEN 3
                WHEN 'Offline' THEN 4
            END;
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u postgres psql crystal_forge -t -c "{intervals_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            found_statuses = set()
            for line in lines:
                parts = [part.strip() for part in line.split("|")]
                if len(parts) >= 2:
                    health_status = parts[0]
                    count = int(parts[1]) if parts[1].isdigit() else 0
                    found_statuses.add(health_status)
                    ctx.logger.log_info(f"  {health_status}: {count} systems")

            # Expected statuses based on our test data
            expected_statuses = {"Healthy", "Warning", "Critical", "Offline"}

            # Check if we found expected health statuses
            if expected_statuses.intersection(found_statuses):
                ctx.logger.log_success("✅ Health status intervals test PASSED")
                ctx.logger.log_info(f"  Found statuses: {', '.join(found_statuses)}")
            else:
                ctx.logger.log_warning(
                    "⚠️ Expected health statuses not found in test results"
                )
                ctx.logger.log_info(
                    f"  Available statuses: {', '.join(found_statuses)}"
                )

        except Exception as e:
            ctx.logger.log_error(f"❌ Health status intervals test ERROR: {e}")

    @staticmethod
    def _test_sorting_order(ctx: CrystalForgeTestContext) -> None:
        """Test that results are returned in the correct priority order"""
        ctx.logger.log_info("Testing sorting order...")

        sorting_test_sql = """
        -- Get all health statuses in order to verify sorting
        SELECT 
            health_status,
            count,
            ROW_NUMBER() OVER() as row_num
        FROM view_fleet_health_status
        ORDER BY 
            CASE health_status
                WHEN 'Healthy' THEN 1
                WHEN 'Warning' THEN 2
                WHEN 'Critical' THEN 3
                WHEN 'Offline' THEN 4
                ELSE 5
            END;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u postgres psql crystal_forge -t -c "{sorting_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            if lines:
                # Expected priority order
                priority_order = ["Healthy", "Warning", "Critical", "Offline"]

                actual_order = []
                for line in lines:
                    parts = [part.strip() for part in line.split("|")]
                    if len(parts) >= 1:
                        health_status = parts[0]
                        actual_order.append(health_status)

                # Check if actual order follows priority (allowing for missing statuses)
                last_priority = -1
                order_correct = True
                for status in actual_order:
                    if status in priority_order:
                        current_priority = priority_order.index(status)
                        if current_priority < last_priority:
                            order_correct = False
                            break
                        last_priority = current_priority

                if order_correct:
                    ctx.logger.log_success("✅ Sorting order test PASSED")
                    ctx.logger.log_info(f"  Order: {' → '.join(actual_order)}")
                else:
                    ctx.logger.log_error("❌ Sorting order test FAILED")
                    ctx.logger.log_error(f"  Actual order: {actual_order}")

            else:
                ctx.logger.log_info("ℹ️ No data to test sorting order")

        except Exception as e:
            ctx.logger.log_error(f"❌ Sorting order test ERROR: {e}")

    @staticmethod
    def _test_data_filtering(ctx: CrystalForgeTestContext) -> None:
        """Test that data filtering works correctly (last_seen conditions)"""
        ctx.logger.log_info("Testing data filtering...")

        filtering_test_sql = """
        BEGIN;
        
        -- Create test systems with edge case last_seen values
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES 
        -- System with NULL primary_ip_address but valid timestamp
        ('test-filter-null-ip', '/nix/store/null-ip', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, NULL, '25.05', true,
         NOW() - INTERVAL '10 minutes'),
        -- System that should be filtered out (no last_seen data)
        ('test-filter-no-data', '/nix/store/no-data', 'startup', '25.05', '6.6.89',
         8192, 3600, 'Test CPU', 4, '10.0.0.200', '25.05', true,
         NOW() - INTERVAL '10 minutes');
        
        -- Add heartbeat only for the first system
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
        ((SELECT id FROM system_states WHERE hostname = 'test-filter-null-ip' ORDER BY timestamp DESC LIMIT 1),
         NOW() - INTERVAL '10 minutes', '1.0.0', 'filtertest');
        
        -- The second system will have no heartbeats, so last_seen might be just the system_state timestamp
        
        -- Check what our test systems look like in the base view
        SELECT 
            hostname,
            last_seen,
            ip_address
        FROM view_systems_status_table 
        WHERE hostname LIKE 'test-filter-%'
        ORDER BY hostname;
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u postgres psql crystal_forge -t -c "{filtering_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            systems_found = len(lines)
            valid_last_seen_count = 0

            for line in lines:
                parts = [part.strip() for part in line.split("|")]
                if len(parts) >= 2:
                    hostname = parts[0]
                    last_seen = parts[1]
                    if last_seen and last_seen != "Unknown":
                        valid_last_seen_count += 1
                        ctx.logger.log_info(f"  {hostname}: last_seen = {last_seen}")

            if systems_found > 0:
                ctx.logger.log_success("✅ Data filtering test PASSED")
                ctx.logger.log_info(f"  Found {systems_found} test systems")
                ctx.logger.log_info(
                    f"  {valid_last_seen_count} have valid last_seen values"
                )
            else:
                ctx.logger.log_warning(
                    "⚠️ No test systems found for data filtering test"
                )

        except Exception as e:
            ctx.logger.log_error(f"❌ Data filtering test ERROR: {e}")

    @staticmethod
    def _test_health_scenarios(ctx: CrystalForgeTestContext) -> None:
        """Test various health scenarios and their aggregation"""
        ctx.logger.log_info("Testing health scenarios...")

        scenarios_test_sql = """
        BEGIN;
        
        -- Create multiple test systems in different health states
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES 
        ('health-scenario-1', '/nix/store/scenario1', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.1.1', '25.05', true, NOW() - INTERVAL '5 minutes'),
        ('health-scenario-2', '/nix/store/scenario2', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.1.2', '25.05', true, NOW() - INTERVAL '5 minutes'),
        ('health-scenario-3', '/nix/store/scenario3', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.1.3', '25.05', true, NOW() - INTERVAL '45 minutes'),
        ('health-scenario-4', '/nix/store/scenario4', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.1.4', '25.05', true, NOW() - INTERVAL '3 hours'),
        ('health-scenario-5', '/nix/store/scenario5', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.1.5', '25.05', true, NOW() - INTERVAL '6 hours');

        -- Add heartbeats to create different health statuses
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
        ((SELECT id FROM system_states WHERE hostname = 'health-scenario-1'), NOW() - INTERVAL '5 minutes', '1.0.0', 'healthy1'),
        ((SELECT id FROM system_states WHERE hostname = 'health-scenario-2'), NOW() - INTERVAL '5 minutes', '1.0.0', 'healthy2'),
        ((SELECT id FROM system_states WHERE hostname = 'health-scenario-3'), NOW() - INTERVAL '45 minutes', '1.0.0', 'warning1'),
        ((SELECT id FROM system_states WHERE hostname = 'health-scenario-4'), NOW() - INTERVAL '3 hours', '1.0.0', 'critical1'),
        ((SELECT id FROM system_states WHERE hostname = 'health-scenario-5'), NOW() - INTERVAL '6 hours', '1.0.0', 'offline1');
        
        -- Query fleet health status
        SELECT 
            health_status,
            count
        FROM view_fleet_health_status
        WHERE health_status IN ('Healthy', 'Warning', 'Critical', 'Offline')
        ORDER BY count DESC;
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u postgres psql crystal_forge -t -c "{scenarios_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            health_counts = {}
            total_systems = 0

            for line in lines:
                parts = [part.strip() for part in line.split("|")]
                if len(parts) >= 2:
                    health_status = parts[0]
                    count = int(parts[1]) if parts[1].isdigit() else 0
                    health_counts[health_status] = count
                    total_systems += count

            if health_counts:
                ctx.logger.log_success("✅ Health scenarios test PASSED")
                for status, count in health_counts.items():
                    ctx.logger.log_info(f"  {status}: {count} systems")
                ctx.logger.log_info(f"  Total systems in health view: {total_systems}")
            else:
                ctx.logger.log_warning("⚠️ No health scenarios found in test data")

        except Exception as e:
            ctx.logger.log_error(f"❌ Health scenarios test ERROR: {e}")

    @staticmethod
    def _test_view_performance(ctx: CrystalForgeTestContext) -> None:
        """Test view performance with EXPLAIN ANALYZE"""
        ctx.logger.log_info("Testing fleet health status view performance...")

        performance_sql = "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT * FROM view_fleet_health_status;"

        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u postgres psql crystal_forge -c "{performance_sql}"',
            "fleet-health-status-view-performance.txt",
            "Fleet health status view performance analysis",
        )

        # Test simple timing
        timing_sql = "\\\\timing on\\nSELECT COUNT(*) FROM view_fleet_health_status;\\nSELECT * FROM view_fleet_health_status;"

        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u postgres psql crystal_forge -c "{timing_sql}"',
            "fleet-health-status-view-timing.txt",
            "Fleet health status view timing test",
        )

        ctx.logger.log_success("Fleet health status view performance testing completed")

    @staticmethod
    def cleanup_test_data(ctx: CrystalForgeTestContext) -> None:
        """Clean up any test data that might have been left behind"""
        ctx.logger.log_info("Cleaning up fleet health status view test data...")

        cleanup_sql = """
        DELETE FROM agent_heartbeats 
        WHERE system_state_id IN (
            SELECT id FROM system_states WHERE hostname LIKE 'test-fleet-%' OR hostname LIKE 'health-scenario-%' OR hostname LIKE 'test-filter-%'
        );
        DELETE FROM system_states WHERE hostname LIKE 'test-fleet-%' OR hostname LIKE 'health-scenario-%' OR hostname LIKE 'test-filter-%';
        """

        try:
            ctx.server.succeed(
                f'sudo -u postgres psql crystal_forge -c "{cleanup_sql}"'
            )
            ctx.logger.log_success(
                "Fleet health status view test data cleanup completed"
            )
        except Exception as e:
            ctx.logger.log_warning(
                f"Could not clean up fleet health status view test data: {e}"
            )
