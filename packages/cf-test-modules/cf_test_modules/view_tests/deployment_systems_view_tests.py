"""
Tests for the view_deployment_status view
"""

import os
from pathlib import Path

from ..test_context import CrystalForgeTestContext


class DeploymentStatusViewTests:
    """Test suite for view_deployment_status"""

    @staticmethod
    def _get_sql_path(filename: str) -> Path:
        """Get the path to a SQL file in the sql directory"""
        current_dir = Path(__file__).parent
        return current_dir / f"sql/{filename}.sql"

    @staticmethod
    def _load_sql(filename: str) -> str:
        """Load SQL content from a file"""
        sql_path = DeploymentStatusViewTests._get_sql_path(filename)
        try:
            with open(sql_path, "r", encoding="utf-8") as f:
                return f.read().strip()
        except FileNotFoundError:
            raise FileNotFoundError(f"SQL file not found: {sql_path}")
        except Exception as e:
            raise RuntimeError(f"Error loading SQL file {sql_path}: {e}")

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

    @staticmethod
    def _log_sql_on_failure(
        ctx: CrystalForgeTestContext, sql: str, test_name: str, reason: str
    ) -> None:
        """Log SQL when test fails due to unexpected results"""
        ctx.logger.log_error(f"❌ {test_name} - {reason}")
        ctx.logger.log_error(f"SQL that produced unexpected results:")
        ctx.logger.log_error("-" * 50)
        for i, line in enumerate(sql.split("\n"), 1):
            ctx.logger.log_error(f"{i:3}: {line}")
        ctx.logger.log_error("-" * 50)

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
            view_exists_sql = DeploymentStatusViewTests._load_sql(
                "deployment_status_view_exists"
            )
            view_check_result = DeploymentStatusViewTests._execute_sql_with_logging(
                ctx, view_exists_sql, "View existence check"
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

        try:
            basic_aggregation_sql = DeploymentStatusViewTests._load_sql(
                "deployment_status_basic_aggregation"
            )
            result = DeploymentStatusViewTests._execute_sql_with_logging(
                ctx, basic_aggregation_sql, "Basic aggregation test"
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
                        DeploymentStatusViewTests._log_sql_on_failure(
                            ctx,
                            basic_aggregation_sql,
                            "Basic aggregation test",
                            f"Invalid data structure - count_numeric: {count_is_numeric}, has_display: {has_display_text}, length_ok: {length_reasonable}",
                        )
                        ctx.logger.log_error(
                            "❌ Basic aggregation test FAILED - Invalid data structure"
                        )
                else:
                    DeploymentStatusViewTests._log_sql_on_failure(
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

        try:
            status_mappings_sql = DeploymentStatusViewTests._load_sql(
                "deployment_status_status_mappings"
            )
            result = DeploymentStatusViewTests._execute_sql_with_logging(
                ctx, status_mappings_sql, "Status display mappings test"
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

        try:
            sorting_order_sql = DeploymentStatusViewTests._load_sql(
                "deployment_status_sorting_order"
            )
            result = DeploymentStatusViewTests._execute_sql_with_logging(
                ctx, sorting_order_sql, "Sorting order test"
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
                    DeploymentStatusViewTests._log_sql_on_failure(
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
    def _test_deployment_scenarios(ctx: CrystalForgeTestContext) -> None:
        """Test various deployment scenarios and their aggregation"""
        ctx.logger.log_info("Testing deployment scenarios...")

        try:
            deployment_scenarios_sql = DeploymentStatusViewTests._load_sql(
                "deployment_status_deployment_scenarios"
            )
            result = DeploymentStatusViewTests._execute_sql_with_logging(
                ctx, deployment_scenarios_sql, "Deployment scenarios test"
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

        try:
            # Test performance analysis
            view_performance_sql = DeploymentStatusViewTests._load_sql(
                "deployment_status_view_performance"
            )
            ctx.logger.capture_command_output(
                ctx.server,
                f'sudo -u postgres psql crystal_forge -c "{view_performance_sql}"',
                "deployment-status-view-performance.txt",
                "Deployment status view performance analysis",
            )

            # Test simple timing
            view_timing_sql = DeploymentStatusViewTests._load_sql(
                "deployment_status_view_timing"
            )
            ctx.logger.capture_command_output(
                ctx.server,
                f'sudo -u postgres psql crystal_forge -c "{view_timing_sql}"',
                "deployment-status-view-timing.txt",
                "Deployment status view timing test",
            )

            ctx.logger.log_success(
                "Deployment status view performance testing completed"
            )

        except Exception as e:
            ctx.logger.log_error(f"❌ View performance test ERROR: {e}")

    @staticmethod
    def cleanup_test_data(ctx: CrystalForgeTestContext) -> None:
        """Clean up any test data that might have been left behind"""
        ctx.logger.log_info("Cleaning up deployment status view test data...")

        try:
            cleanup_sql = DeploymentStatusViewTests._load_sql(
                "deployment_status_cleanup"
            )
            DeploymentStatusViewTests._execute_sql_with_logging(
                ctx, cleanup_sql, "Cleanup test data"
            )
            ctx.logger.log_success("Deployment status view test data cleanup completed")
        except Exception as e:
            ctx.logger.log_warning(
                f"Could not clean up deployment status view test data: {e}"
            )
