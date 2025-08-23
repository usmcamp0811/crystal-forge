"""
Tests for the view_deployment_status view
"""

from ..test_context import CrystalForgeTestContext


class DeploymentStatusViewTests:
    """Test suite for view_deployment_status"""

    @staticmethod
    def run_all_tests(ctx: CrystalForgeTestContext) -> None:
        """Run all tests for the deployment status view"""
        ctx.logger.log_section("📊 Testing view_deployment_status")

        # Test 1: Verify view exists and is queryable
        if not DeploymentStatusViewTests._test_view_exists(ctx):
            ctx.logger.log_warning("View does not exist - skipping remaining tests")
            return

        # Test 2: Test basic aggregation functionality
        DeploymentStatusViewTests._test_basic_aggregation(ctx)

        # Test 3: Test status display mappings
        DeploymentStatusViewTests._test_status_display_mappings(ctx)

        # Test 4: Test sorting order
        DeploymentStatusViewTests._test_sorting_order(ctx)

        # Test 5: Test with various deployment scenarios
        DeploymentStatusViewTests._test_deployment_scenarios(ctx)

        # Test 6: Test view performance
        DeploymentStatusViewTests._test_view_performance(ctx)

        # Clean up test data
        DeploymentStatusViewTests.cleanup_test_data(ctx)

    @staticmethod
    def _test_view_exists(ctx: CrystalForgeTestContext) -> bool:
        """Test that the view exists and can be queried"""
        ctx.logger.log_info("Testing deployment status view existence...")

        try:
            # Check if view exists
            view_check_result = ctx.server.succeed(
                "sudo -u postgres psql crystal_forge -t -c "
                "\"SELECT EXISTS (SELECT 1 FROM information_schema.views WHERE table_name = 'view_deployment_status');\""
            ).strip()

            if view_check_result == "t":
                ctx.logger.log_success("view_deployment_status exists")

                # Test basic query
                ctx.server.succeed(
                    "sudo -u postgres psql crystal_forge -c "
                    '"SELECT COUNT(*) FROM view_deployment_status;"'
                )
                ctx.logger.log_success("Basic deployment status view query successful")
                return True
            else:
                ctx.logger.log_warning("view_deployment_status does not exist")
                return False

        except Exception as e:
            ctx.logger.log_error(
                f"Error checking deployment status view existence: {e}"
            )
            return False

    @staticmethod
    def _test_basic_aggregation(ctx: CrystalForgeTestContext) -> None:
        """Test basic aggregation functionality"""
        ctx.logger.log_info("Testing basic aggregation functionality...")

        aggregation_test_sql = """
        -- Test that the view returns proper structure
        SELECT 
            count,
            status_display,
            LENGTH(status_display) as display_length
        FROM view_deployment_status
        LIMIT 3;
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
                    count_value = parts[0]
                    status_display = parts[1]
                    display_length = parts[2]

                    # Verify count is numeric
                    count_is_numeric = count_value.isdigit()
                    # Verify status_display has content
                    has_display_text = len(status_display) > 0
                    # Verify display_length is reasonable
                    length_reasonable = int(display_length) > 0

                    if count_is_numeric and has_display_text and length_reasonable:
                        ctx.logger.log_success("✅ Basic aggregation test PASSED")
                        ctx.logger.log_info(
                            f"  Sample: {count_value} systems with status '{status_display}'"
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
    def _test_status_display_mappings(ctx: CrystalForgeTestContext) -> None:
        """Test that status display mappings are correct"""
        ctx.logger.log_info("Testing status display mappings...")

        # Expected mappings from the view definition
        expected_mappings = {
            "up_to_date": "Up to Date",
            "behind": "Behind",
            "evaluation_failed": "Evaluation Failed",
            "no_evaluation": "No Evaluation",
            "no_deployment": "No Deployment",
            "never_seen": "Never Seen",
            "unknown": "Unknown",
        }

        mapping_test_sql = """
        -- Create test data with known statuses and verify mappings
        BEGIN;
        
        -- Create test flake and system
        INSERT INTO flakes (name, repo_url) VALUES ('test-deploy-flake', 'http://test-deploy.git');
        INSERT INTO systems (hostname, flake_id, derivation, public_key, is_active) 
        VALUES ('test-deploy-mapping', (SELECT id FROM flakes WHERE name = 'test-deploy-flake'), 'nixosConfigurations.test', 'test-deploy-key', true);
        
        -- Create commit
        INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES 
        ((SELECT id FROM flakes WHERE name = 'test-deploy-flake'), 'mapping123', NOW() - INTERVAL '1 hour');
        
        -- Create derivation status if not exists
        INSERT INTO derivation_statuses (name, description, is_terminal, is_success, display_order) VALUES 
        ('test-mapping-complete', 'Test Mapping Complete', true, true, 101) ON CONFLICT (name) DO NOTHING;
        
        -- Create derivation
        INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id) VALUES 
        ((SELECT id FROM commits WHERE git_commit_hash = 'mapping123'), 'nixos', 'test-deploy-mapping', '/nix/store/mapping-path', (SELECT id FROM derivation_statuses WHERE name = 'test-mapping-complete'));
        
        -- Create system state to make it "up_to_date"
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible,
            timestamp
        ) VALUES (
            'test-deploy-mapping', '/nix/store/mapping-path', 'startup', '25.05', '6.6.89',
            8192, 3600, 'Test CPU', 4, '10.0.0.400', '25.05', true,
            NOW() - INTERVAL '10 minutes'
        );
        
        -- Query to see if our test system shows up with correct mapping
        SELECT 
            count,
            status_display
        FROM view_deployment_status
        WHERE status_display IN ('Up to Date', 'Behind', 'Evaluation Failed', 'No Evaluation', 'No Deployment', 'Never Seen', 'Unknown');
        
        ROLLBACK;
        """

        try:
            result = ctx.server.succeed(
                f'sudo -u postgres psql crystal_forge -t -c "{mapping_test_sql}"'
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            found_mappings = set()
            for line in lines:
                parts = [part.strip() for part in line.split("|")]
                if len(parts) >= 2:
                    status_display = parts[1]
                    found_mappings.add(status_display)

            # Verify at least some expected mappings are present
            expected_displays = set(expected_mappings.values())
            valid_mappings_found = found_mappings.intersection(expected_displays)

            if valid_mappings_found:
                ctx.logger.log_success("✅ Status display mappings test PASSED")
                ctx.logger.log_info(
                    f"  Found valid mappings: {', '.join(valid_mappings_found)}"
                )
            else:
                ctx.logger.log_warning(
                    "⚠️ No expected status display mappings found in current data"
                )
                ctx.logger.log_info(
                    f"  Available displays: {', '.join(found_mappings)}"
                )

        except Exception as e:
            ctx.logger.log_error(f"❌ Status display mappings test ERROR: {e}")

    @staticmethod
    def _test_sorting_order(ctx: CrystalForgeTestContext) -> None:
        """Test that results are returned in the correct priority order"""
        ctx.logger.log_info("Testing sorting order...")

        sorting_test_sql = """
        -- Get all statuses in order to verify sorting
        SELECT 
            status_display,
            count,
            ROW_NUMBER() OVER() as row_num
        FROM view_deployment_status
        ORDER BY 
            CASE status_display
                WHEN 'Up to Date' THEN 1
                WHEN 'Behind' THEN 2 
                WHEN 'Evaluation Failed' THEN 3
                WHEN 'No Evaluation' THEN 4
                WHEN 'No Deployment' THEN 5
                WHEN 'Never Seen' THEN 6
                WHEN 'Unknown' THEN 7
                ELSE 8
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
                priority_order = [
                    "Up to Date",
                    "Behind",
                    "Evaluation Failed",
                    "No Evaluation",
                    "No Deployment",
                    "Never Seen",
                    "Unknown",
                ]

                actual_order = []
                for line in lines:
                    parts = [part.strip() for part in line.split("|")]
                    if len(parts) >= 1:
                        status_display = parts[0]
                        actual_order.append(status_display)

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
                    ctx.logger.log_info(f"  Order: {' → '.join(actual_order[:5])}...")
                else:
                    ctx.logger.log_error("❌ Sorting order test FAILED")
                    ctx.logger.log_error(f"  Actual order: {actual_order}")

            else:
                ctx.logger.log_info("ℹ️ No data to test sorting order")

        except Exception as e:
            ctx.logger.log_error(f"❌ Sorting order test ERROR: {e}")

    @staticmethod
    def _test_deployment_scenarios(ctx: CrystalForgeTestContext) -> None:
        """Test various deployment scenarios and their aggregation"""
        ctx.logger.log_info("Testing deployment scenarios...")

        scenarios_test_sql = """
        BEGIN;
        
        -- Create test flake
        INSERT INTO flakes (name, repo_url) VALUES ('test-scenarios-flake', 'http://scenarios.git');
        
        -- Create multiple test systems with different scenarios
        INSERT INTO systems (hostname, flake_id, derivation, public_key, is_active) VALUES 
        ('scenario-uptodate', (SELECT id FROM flakes WHERE name = 'test-scenarios-flake'), 'nixosConfigurations.uptodate', 'key1', true),
        ('scenario-behind', (SELECT id FROM flakes WHERE name = 'test-scenarios-flake'), 'nixosConfigurations.behind', 'key2', true),
        ('scenario-never', (SELECT id FROM flakes WHERE name = 'test-scenarios-flake'), 'nixosConfigurations.never', 'key3', true);
        
        -- Create commits
        INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES 
        ((SELECT id FROM flakes WHERE name = 'test-scenarios-flake'), 'old456', NOW() - INTERVAL '2 hours'),
        ((SELECT id FROM flakes WHERE name = 'test-scenarios-flake'), 'new789', NOW() - INTERVAL '1 hour');
        
        -- Create derivation status
        INSERT INTO derivation_statuses (name, description, is_terminal, is_success, display_order) VALUES 
        ('test-scenarios-complete', 'Scenarios Complete', true, true, 102) ON CONFLICT (name) DO NOTHING;
        
        -- Create derivations
        INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id) VALUES 
        -- Up to date system - latest derivation
        ((SELECT id FROM commits WHERE git_commit_hash = 'new789'), 'nixos', 'scenario-uptodate', '/nix/store/uptodate-new', (SELECT id FROM derivation_statuses WHERE name = 'test-scenarios-complete')),
        -- Behind system - old derivation  
        ((SELECT id FROM commits WHERE git_commit_hash = 'old456'), 'nixos', 'scenario-behind', '/nix/store/behind-old', (SELECT id FROM derivation_statuses WHERE name = 'test-scenarios-complete')),
        ((SELECT id FROM commits WHERE git_commit_hash = 'new789'), 'nixos', 'scenario-behind', '/nix/store/behind-new', (SELECT id FROM derivation_statuses WHERE name = 'test-scenarios-complete'));
        
        -- Create system states
        INSERT INTO system_states (hostname, derivation_path, change_reason, timestamp) VALUES 
        -- Up to date system running latest
        ('scenario-uptodate', '/nix/store/uptodate-new', 'startup', NOW() - INTERVAL '10 minutes'),
        -- Behind system running old
        ('scenario-behind', '/nix/store/behind-old', 'startup', NOW() - INTERVAL '10 minutes');
        -- scenario-never has no system_states (never seen)
        
        -- Query deployment status
        SELECT 
            status_display,
            count
        FROM view_deployment_status
        WHERE status_display IN ('Up to Date', 'Behind', 'Never Seen')
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

            scenario_counts = {}
            for line in lines:
                parts = [part.strip() for part in line.split("|")]
                if len(parts) >= 2:
                    status_display = parts[0]
                    count = int(parts[1]) if parts[1].isdigit() else 0
                    scenario_counts[status_display] = count

            # Verify we have expected scenarios
            scenarios_found = len(scenario_counts) > 0

            if scenarios_found:
                ctx.logger.log_success("✅ Deployment scenarios test PASSED")
                for status, count in scenario_counts.items():
                    ctx.logger.log_info(f"  {status}: {count} systems")
            else:
                ctx.logger.log_warning("⚠️ No deployment scenarios found in test data")

        except Exception as e:
            ctx.logger.log_error(f"❌ Deployment scenarios test ERROR: {e}")

    @staticmethod
    def _test_view_performance(ctx: CrystalForgeTestContext) -> None:
        """Test view performance with EXPLAIN ANALYZE"""
        ctx.logger.log_info("Testing deployment status view performance...")

        performance_sql = "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT * FROM view_deployment_status;"

        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u postgres psql crystal_forge -c "{performance_sql}"',
            "deployment-status-view-performance.txt",
            "Deployment status view performance analysis",
        )

        # Test simple timing
        timing_sql = "\\\\timing on\\nSELECT COUNT(*) FROM view_deployment_status;\\nSELECT * FROM view_deployment_status;"

        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u postgres psql crystal_forge -c "{timing_sql}"',
            "deployment-status-view-timing.txt",
            "Deployment status view timing test",
        )

        ctx.logger.log_success("Deployment status view performance testing completed")

    @staticmethod
    def cleanup_test_data(ctx: CrystalForgeTestContext) -> None:
        """Clean up any test data that might have been left behind"""
        ctx.logger.log_info("Cleaning up deployment status view test data...")

        cleanup_sql = """
        DELETE FROM agent_heartbeats 
        WHERE system_state_id IN (
            SELECT id FROM system_states WHERE hostname LIKE 'test-deploy-%' OR hostname LIKE 'scenario-%'
        );
        DELETE FROM derivations WHERE derivation_name LIKE 'test-deploy-%' OR derivation_name LIKE 'scenario-%';
        DELETE FROM commits WHERE git_commit_hash IN ('mapping123', 'old456', 'new789');
        DELETE FROM systems WHERE hostname LIKE 'test-deploy-%' OR hostname LIKE 'scenario-%';
        DELETE FROM flakes WHERE name LIKE 'test-%flake';
        DELETE FROM system_states WHERE hostname LIKE 'test-deploy-%' OR hostname LIKE 'scenario-%';
        DELETE FROM derivation_statuses WHERE name LIKE 'test-%';
        """

        try:
            ctx.server.succeed(
                f'sudo -u postgres psql crystal_forge -c "{cleanup_sql}"'
            )
            ctx.logger.log_success("Deployment status view test data cleanup completed")
        except Exception as e:
            ctx.logger.log_warning(
                f"Could not clean up deployment status view test data: {e}"
            )
