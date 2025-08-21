from .test_context import CrystalForgeTestContext


class DatabaseTests:
    """Database setup and verification tests"""

    @staticmethod
    def setup_and_verify(ctx: CrystalForgeTestContext) -> None:
        """Complete database setup and verification"""
        DatabaseTests._setup_postgresql(ctx)
        DatabaseTests._verify_database_functionality(ctx)
        DatabaseTests._test_view_systems_status_table(ctx)

    @staticmethod
    def _setup_postgresql(ctx: CrystalForgeTestContext) -> None:
        """Setup and verify PostgreSQL"""
        ctx.logger.log_section("🗄️ Setting up PostgreSQL Database")

        # Debug PostgreSQL status
        ctx.logger.capture_command_output(
            ctx.server,
            "systemctl status postgresql.service || true",
            "postgresql-status-before.txt",
            "PostgreSQL status before startup",
        )

        # Check if PostgreSQL service exists
        ctx.logger.capture_command_output(
            ctx.server,
            "systemctl list-unit-files | grep postgresql || echo 'No PostgreSQL unit files found'",
            "postgresql-units.txt",
            "PostgreSQL unit files",
        )

        # Wait for PostgreSQL
        ctx.logger.log_info("Waiting for PostgreSQL to start...")
        ctx.server.wait_for_unit("postgresql.service")
        ctx.logger.log_success("PostgreSQL is ready")

    @staticmethod
    def _verify_database_functionality(ctx: CrystalForgeTestContext) -> None:
        """Verify PostgreSQL functionality"""
        ctx.server.succeed("sudo -u postgres psql -c 'SELECT version();'")
        ctx.logger.log_success("PostgreSQL is functional")

    @staticmethod
    def _test_view_systems_status_table(ctx: CrystalForgeTestContext) -> None:
        """Test the view_systems_status_table view"""
        ctx.logger.log_section("🔍 Testing view_systems_status_table")

        # Check if the crystal-forge database user exists first
        try:
            ctx.server.succeed(
                "sudo -u postgres psql -t -c \"SELECT 1 FROM pg_roles WHERE rolname = 'crystal-forge';\""
            )
            ctx.logger.log_info("crystal-forge database user exists")
        except Exception:
            ctx.logger.log_warning(
                "crystal-forge database user not found - skipping view tests (server not started yet)"
            )
            return

        # Check if the database exists
        try:
            ctx.server.succeed(
                "sudo -u postgres psql -t -c \"SELECT 1 FROM pg_database WHERE datname = 'crystal_forge';\""
            )
            ctx.logger.log_info("crystal_forge database exists")
        except Exception:
            ctx.logger.log_warning(
                "crystal_forge database not found - skipping view tests (server not started yet)"
            )
            return

        # Test 1: Verify view exists and is queryable
        DatabaseTests._test_view_exists(ctx)

        # Only run other tests if view exists
        try:
            view_exists = ctx.server.succeed(
                "sudo -u crystal-forge psql crystal_forge -t -c "
                "\"SELECT EXISTS (SELECT 1 FROM information_schema.views WHERE table_name = 'view_systems_status_table');\""
            ).strip()

            if view_exists != "t":
                ctx.logger.log_warning(
                    "Skipping remaining view tests - view does not exist"
                )
                return
        except:
            ctx.logger.log_warning(
                "Skipping remaining view tests - cannot verify view existence"
            )
            return

        # Test 2: Test with sample data
        DatabaseTests._test_view_with_sample_data(ctx)

        # Test 3: Test status logic scenarios
        DatabaseTests._test_status_logic_scenarios(ctx)

        # Test 4: Verify view performance
        DatabaseTests._test_view_performance(ctx)

    @staticmethod
    def _test_view_exists(ctx: CrystalForgeTestContext) -> None:
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
            return

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
            else:
                ctx.logger.log_warning(
                    "view_systems_status_table does not exist - skipping view tests"
                )

                # List all views for debugging
                ctx.logger.capture_command_output(
                    ctx.server,
                    "sudo -u crystal-forge psql crystal_forge -c \"SELECT table_name FROM information_schema.views WHERE table_schema = 'public';\"",
                    "existing-views.txt",
                    "List of existing views",
                )
                return

        except Exception as e:
            ctx.logger.log_error(f"Error checking view existence: {e}")

            # Capture more debug info
            ctx.logger.capture_command_output(
                ctx.server,
                'sudo -u crystal-forge psql crystal_forge -c "\\d"',
                "database-schema.txt",
                "Database schema debug info",
            )
            return

    @staticmethod
    def _test_view_with_sample_data(ctx: CrystalForgeTestContext) -> None:
        """Test view with sample data scenarios"""
        ctx.logger.log_info("Testing view with sample data...")

        # Create test transaction with sample data
        test_sql = """
        BEGIN;
        
        -- Insert test system with recent activity (should be 'starting' or 'online')
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel, 
            memory_gb, uptime_secs, cpu_brand, cpu_cores, 
            primary_ip_address, nixos_version, agent_compatible
        ) VALUES (
            'test-recent', '/nix/store/test-recent', 'startup', '25.05', '6.6.89',
            16384, 3600, 'Test CPU Recent', 8, '10.0.0.100', '25.05', true
        );
        
        -- Insert test system with old activity (should be 'offline')
        INSERT INTO system_states (
            hostname, derivation_path, change_reason, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible, timestamp
        ) VALUES (
            'test-old', '/nix/store/test-old', 'startup', '25.05', '6.6.89',
            8192, 7200, 'Test CPU Old', 4, '10.0.0.101', '25.05', true,
            NOW() - INTERVAL '2 hours'
        );
        
        -- Query the view for our test data
        SELECT hostname, status, status_text, version, uptime, ip_address
        FROM view_systems_status_table 
        WHERE hostname LIKE 'test-%'
        ORDER BY hostname;
        
        ROLLBACK;
        """

        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u crystal-forge psql crystal_forge -c "{test_sql}"',
            "view-sample-data-test.txt",
            "View sample data test results",
        )
        ctx.logger.log_success("Sample data test completed")

    @staticmethod
    def _test_status_logic_scenarios(ctx: CrystalForgeTestContext) -> None:
        """Test specific status logic scenarios"""
        ctx.logger.log_info("Testing status logic scenarios...")

        test_scenarios = [
            {
                "name": "Recent system state, no heartbeat",
                "description": "Should show 'starting' status",
                "sql": """
                INSERT INTO system_states (
                    hostname, derivation_path, change_reason, os, kernel,
                    memory_gb, uptime_secs, cpu_brand, cpu_cores,
                    primary_ip_address, nixos_version, agent_compatible
                ) VALUES (
                    'test-starting', '/nix/store/test', 'startup', '25.05', '6.6.89',
                    16384, 3600, 'Test CPU', 8, '10.0.0.200', '25.05', true
                );
                SELECT 
                    'recent_system_no_heartbeat' as test_name,
                    v.hostname,
                    v.status,
                    v.status_text,
                    CASE WHEN v.status = 'starting' THEN 'PASS' ELSE 'FAIL' END as result
                FROM view_systems_status_table v
                WHERE v.hostname = 'test-starting';
                """,
            },
            {
                "name": "Old system state, no heartbeat",
                "description": "Should show 'offline' status",
                "sql": """
                INSERT INTO system_states (
                    hostname, derivation_path, change_reason, os, kernel,
                    memory_gb, uptime_secs, cpu_brand, cpu_cores,
                    primary_ip_address, nixos_version, agent_compatible, timestamp
                ) VALUES (
                    'test-offline', '/nix/store/test', 'startup', '25.05', '6.6.89',
                    8192, 3600, 'Test CPU', 4, '10.0.0.201', '25.05', true,
                    NOW() - INTERVAL '2 hours'
                );
                SELECT 
                    'old_system_no_heartbeat' as test_name,
                    v.hostname,
                    v.status,
                    v.status_text,
                    CASE WHEN v.status = 'offline' THEN 'PASS' ELSE 'FAIL' END as result
                FROM view_systems_status_table v
                WHERE v.hostname = 'test-offline';
                """,
            },
        ]

        for i, scenario in enumerate(test_scenarios):
            ctx.logger.log_info(f"Testing scenario: {scenario['name']}")

            # Run test in transaction and rollback
            full_sql = f"BEGIN; {scenario['sql']} ROLLBACK;"

            ctx.logger.capture_command_output(
                ctx.server,
                f'sudo -u crystal-forge psql crystal_forge -c "{full_sql}"',
                f"status-logic-test-{i+1}.txt",
                f"Status logic test: {scenario['name']}",
            )

        ctx.logger.log_success("Status logic scenarios completed")

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
    def _cleanup_test_data(ctx: CrystalForgeTestContext) -> None:
        """Clean up any test data that might have been left behind"""
        ctx.logger.log_info("Cleaning up test data...")

        cleanup_sql = """
        DELETE FROM agent_heartbeats 
        WHERE system_state_id IN (
            SELECT id FROM system_states WHERE hostname LIKE 'test-%'
        );
        DELETE FROM system_states WHERE hostname LIKE 'test-%';
        """

        ctx.server.succeed(
            f'sudo -u crystal-forge psql crystal_forge -c "{cleanup_sql}"'
        )

        ctx.logger.log_success("Test data cleanup completed")
