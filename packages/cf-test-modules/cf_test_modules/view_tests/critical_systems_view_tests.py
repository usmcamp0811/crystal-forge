"""
Tests for the view_critical_systems view
"""

import os
from pathlib import Path

from ..test_context import CrystalForgeTestContext


class CriticalSystemsViewTests:
    """Test suite for view_critical_systems"""

    @staticmethod
    def _get_sql_path(filename: str) -> Path:
        """Get the path to a SQL file in the same directory as this Python file"""
        current_dir = Path(__file__).parent
        return current_dir / f"sql/{filename}.sql"

    @staticmethod
    def _execute_sql_with_logging(
        ctx: CrystalForgeTestContext, sql: str, test_name: str
    ) -> str:
        """Execute SQL and log it if there's a failure"""
        try:
            return ctx.server.succeed(
                f'sudo -u postgres psql crystal_forge -t -c "{sql}"'
            )
        except Exception as e:
            ctx.logger.log_error(f"❌ {test_name} - SQL execution failed")
            ctx.logger.log_error(f"SQL that failed:")
            ctx.logger.log_error("-" * 50)
            for i, line in enumerate(sql.split("\n"), 1):
                ctx.logger.log_error(f"{i:3}: {line}")
            ctx.logger.log_error("-" * 50)
            raise e
        """Load SQL content from a file"""
        sql_path = CriticalSystemsViewTests._get_sql_path(filename)
        try:
            with open(sql_path, "r", encoding="utf-8") as f:
                return f.read().strip()
        except FileNotFoundError:
            raise FileNotFoundError(f"SQL file not found: {sql_path}")
        except Exception as e:
            raise RuntimeError(f"Error loading SQL file {sql_path}: {e}")

    @staticmethod
    def _load_sql(filename: str) -> str:
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
            view_exists_sql = CriticalSystemsViewTests._load_sql(
                "critical_systems_view_exists"
            )
            view_check_result = CriticalSystemsViewTests._execute_sql_with_logging(
                ctx, view_exists_sql, "View existence check"
            ).strip()

            if view_check_result == "t":
                ctx.logger.log_success("view_critical_systems exists")

                # Test basic query
                ctx.server.succeed(
                    "sudo -u postgres psql crystal_forge -c "
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

        try:
            basic_structure_sql = CriticalSystemsViewTests._load_sql(
                "critical_systems_basic_structure"
            )
            result = CriticalSystemsViewTests._execute_sql_with_logging(
                ctx, basic_structure_sql, "Basic structure test"
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

        try:
            status_logic_sql = CriticalSystemsViewTests._load_sql(
                "critical_systems_status_logic"
            )
            result = CriticalSystemsViewTests._execute_sql_with_logging(
                ctx, status_logic_sql, "Status logic test"
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

        try:
            hours_calculation_sql = CriticalSystemsViewTests._load_sql(
                "critical_systems_hours_calculation"
            )
            result = CriticalSystemsViewTests._execute_sql_with_logging(
                ctx, hours_calculation_sql, "Hours calculation test"
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

        try:
            data_filtering_sql = CriticalSystemsViewTests._load_sql(
                "critical_systems_data_filtering"
            )
            result = CriticalSystemsViewTests._execute_sql_with_logging(
                ctx, data_filtering_sql, "Data filtering test"
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

        try:
            sorting_order_sql = CriticalSystemsViewTests._load_sql(
                "critical_systems_sorting_order"
            )
            result = CriticalSystemsViewTests._execute_sql_with_logging(
                ctx, sorting_order_sql, "Sorting order test"
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

        try:
            edge_cases_sql = CriticalSystemsViewTests._load_sql(
                "critical_systems_edge_cases"
            )
            result = CriticalSystemsViewTests._execute_sql_with_logging(
                ctx, edge_cases_sql, "Edge cases test"
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

        try:
            critical_scenarios_sql = CriticalSystemsViewTests._load_sql(
                "critical_systems_critical_scenarios"
            )
            result = CriticalSystemsViewTests._execute_sql_with_logging(
                ctx, critical_scenarios_sql, "Critical scenarios test"
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

        try:
            # Test performance analysis
            view_performance_sql = CriticalSystemsViewTests._load_sql(
                "critical_systems_view_performance"
            )
            ctx.logger.capture_command_output(
                ctx.server,
                f'sudo -u postgres psql crystal_forge -c "{view_performance_sql}"',
                "critical-systems-view-performance.txt",
                "Critical systems view performance analysis",
            )

            # Test simple timing
            view_timing_sql = CriticalSystemsViewTests._load_sql(
                "critical_systems_view_timing"
            )
            ctx.logger.capture_command_output(
                ctx.server,
                f'sudo -u postgres psql crystal_forge -c "{view_timing_sql}"',
                "critical-systems-view-timing.txt",
                "Critical systems view timing test",
            )

            ctx.logger.log_success(
                "Critical systems view performance testing completed"
            )

        except Exception as e:
            ctx.logger.log_error(f"❌ View performance test ERROR: {e}")

    @staticmethod
    def cleanup_test_data(ctx: CrystalForgeTestContext) -> None:
        """Clean up any test data that might have been left behind"""
        ctx.logger.log_info("Cleaning up critical systems view test data...")

        try:
            cleanup_sql = CriticalSystemsViewTests._load_sql("critical_systems_cleanup")
            CriticalSystemsViewTests._execute_sql_with_logging(
                ctx, cleanup_sql, "Cleanup test data"
            )
            ctx.logger.log_success("Critical systems view test data cleanup completed")
        except Exception as e:
            ctx.logger.log_warning(
                f"Could not clean up critical systems view test data: {e}"
            )
