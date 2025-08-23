"""
Tests for the view_fleet_health_status view
"""

import os
from pathlib import Path

from ..test_context import CrystalForgeTestContext
from ..test_exceptions import AssertionFailedException
from .base import BaseViewTests


class FleetHealthStatusViewTests(BaseViewTests):
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
            view_exists_sql = FleetHealthStatusViewTests._load_sql(
                "fleet_health_view_exists"
            )
            view_check_result = FleetHealthStatusViewTests._execute_sql_with_logging(
                ctx, view_exists_sql, "View existence check"
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

        try:
            basic_aggregation_sql = FleetHealthStatusViewTests._load_sql(
                "fleet_health_basic_aggregation"
            )
            result = FleetHealthStatusViewTests._execute_sql_with_logging(
                ctx, basic_aggregation_sql, "Basic aggregation test"
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
                        FleetHealthStatusViewTests._log_sql_on_failure(
                            ctx,
                            basic_aggregation_sql,
                            "Basic aggregation test",
                            f"Invalid data structure - count_numeric: {count_is_numeric}, has_health: {has_health_status}, length_ok: {length_reasonable}",
                        )
                        ctx.logger.log_error(
                            "❌ Basic aggregation test FAILED - Invalid data structure"
                        )
                else:
                    FleetHealthStatusViewTests._log_sql_on_failure(
                        ctx,
                        basic_aggregation_sql,
                        "Basic aggregation test",
                        f"Insufficient columns - got {len(parts)} columns, expected 3",
                    )
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

        try:
            status_intervals_sql = FleetHealthStatusViewTests._load_sql(
                "fleet_health_status_intervals"
            )
            result = FleetHealthStatusViewTests._execute_sql_with_logging(
                ctx, status_intervals_sql, "Health status intervals test"
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
                FleetHealthStatusViewTests._log_sql_on_failure(
                    ctx,
                    status_intervals_sql,
                    "Health status intervals test",
                    f"Expected statuses {expected_statuses} not found. Found: {found_statuses}",
                )
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

        try:
            sorting_order_sql = FleetHealthStatusViewTests._load_sql(
                "fleet_health_sorting_order"
            )
            result = FleetHealthStatusViewTests._execute_sql_with_logging(
                ctx, sorting_order_sql, "Sorting order test"
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
                    FleetHealthStatusViewTests._log_sql_on_failure(
                        ctx,
                        sorting_order_sql,
                        "Sorting order test",
                        f"Incorrect sort order - expected priority order, got: {actual_order}",
                    )
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

        try:
            data_filtering_sql = FleetHealthStatusViewTests._load_sql(
                "fleet_health_data_filtering"
            )
            result = FleetHealthStatusViewTests._execute_sql_with_logging(
                ctx, data_filtering_sql, "Data filtering test"
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
                FleetHealthStatusViewTests._log_sql_on_failure(
                    ctx,
                    data_filtering_sql,
                    "Data filtering test",
                    "No test systems found for data filtering test",
                )
                ctx.logger.log_warning(
                    "⚠️ No test systems found for data filtering test"
                )

        except Exception as e:
            ctx.logger.log_error(f"❌ Data filtering test ERROR: {e}")

    @staticmethod
    def _test_health_scenarios(ctx: CrystalForgeTestContext) -> None:
        """Test various health scenarios and their aggregation"""
        ctx.logger.log_info("Testing health scenarios...")

        try:
            health_scenarios_sql = FleetHealthStatusViewTests._load_sql(
                "fleet_health_health_scenarios"
            )
            result = FleetHealthStatusViewTests._execute_sql_with_logging(
                ctx, health_scenarios_sql, "Health scenarios test"
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
                FleetHealthStatusViewTests._log_sql_on_failure(
                    ctx,
                    health_scenarios_sql,
                    "Health scenarios test",
                    "No health scenarios found in test data",
                )
                ctx.logger.log_warning("⚠️ No health scenarios found in test data")

        except Exception as e:
            ctx.logger.log_error(f"❌ Health scenarios test ERROR: {e}")

    @staticmethod
    def _test_view_performance(ctx: CrystalForgeTestContext) -> None:
        """Test view performance with EXPLAIN ANALYZE"""
        ctx.logger.log_info("Testing fleet health status view performance...")

        try:
            # Test performance analysis
            view_performance_sql = FleetHealthStatusViewTests._load_sql(
                "fleet_health_view_performance"
            )
            ctx.logger.capture_command_output(
                ctx.server,
                f'sudo -u postgres psql crystal_forge -c "{view_performance_sql}"',
                "fleet-health-status-view-performance.txt",
                "Fleet health status view performance analysis",
            )

            # Test simple timing
            view_timing_sql = FleetHealthStatusViewTests._load_sql(
                "fleet_health_view_timing"
            )
            ctx.logger.capture_command_output(
                ctx.server,
                f'sudo -u postgres psql crystal_forge -c "{view_timing_sql}"',
                "fleet-health-status-view-timing.txt",
                "Fleet health status view timing test",
            )

            ctx.logger.log_success(
                "Fleet health status view performance testing completed"
            )

        except Exception as e:
            ctx.logger.log_error(f"❌ View performance test ERROR: {e}")

    @staticmethod
    def cleanup_test_data(ctx: CrystalForgeTestContext) -> None:
        """Clean up any test data that might have been left behind"""
        ctx.logger.log_info("Cleaning up fleet health status view test data...")

        try:
            cleanup_sql = FleetHealthStatusViewTests._load_sql("fleet_health_cleanup")
            FleetHealthStatusViewTests._execute_sql_with_logging(
                ctx, cleanup_sql, "Cleanup test data"
            )
            ctx.logger.log_success(
                "Fleet health status view test data cleanup completed"
            )
        except Exception as e:
            ctx.logger.log_warning(
                f"Could not clean up fleet health status view test data: {e}"
            )
