"""
Tests for the view_systems_status_table view
"""

import os
from pathlib import Path

from ..test_context import CrystalForgeTestContext
from ..test_exceptions import AssertionFailedException
from .base import BaseViewTests


class SystemsStatusTableTests(BaseViewTests):
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

        # Test 4: Test update status logic
        SystemsStatusTableTests._test_update_status_logic(ctx)

        # Test 5: Test edge cases and boundary conditions
        SystemsStatusTableTests._test_edge_cases(ctx)

        # Test 6: Verify view performance
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
                'sudo -u postgres psql crystal_forge -c "SELECT 1;" > /dev/null'
            )
            ctx.logger.log_success("Database connection verified")
        except Exception as e:
            ctx.logger.log_error(f"Cannot connect to database: {e}")
            return False

        # Check if view exists - capture output for debugging
        try:
            view_exists_sql = SystemsStatusTableTests._load_sql(
                "systems_status_view_exists"
            )
            view_check_result = SystemsStatusTableTests._execute_sql_with_logging(
                ctx, view_exists_sql, "View existence check"
            ).strip()

            if view_check_result == "t":
                ctx.logger.log_success("view_systems_status_table exists")

                # Test basic query if view exists
                ctx.server.succeed(
                    "sudo -u postgres psql crystal_forge -c "
                    '"SELECT COUNT(*) FROM view_systems_status_table;"'
                )
                ctx.logger.log_success("Basic view query successful")
                return True
            else:
                ctx.logger.log_warning("view_systems_status_table does not exist")

                # List all views for debugging
                ctx.logger.capture_command_output(
                    ctx.server,
                    "sudo -u postgres psql crystal_forge -c \"SELECT table_name FROM information_schema.views WHERE table_schema = 'public';\"",
                    "existing-views.txt",
                    "List of existing views",
                )
                return False

        except Exception as e:
            ctx.logger.log_error(f"Error checking view existence: {e}")

            # Capture more debug info
            ctx.logger.capture_command_output(
                ctx.server,
                'sudo -u postgres psql crystal_forge -c "\\d"',
                "database-schema.txt",
                "Database schema debug info",
            )
            return False

    @staticmethod
    def _test_status_logic_scenarios(ctx: CrystalForgeTestContext) -> None:
        """Test specific status logic scenarios with assertions"""
        ctx.logger.log_info("Testing connectivity status logic scenarios...")

        test_scenarios = [
            {
                "name": "Recent system state, no heartbeat (should be 'starting')",
                "sql_file": "systems_status_scenario_starting",
                "expected_connectivity_status": "starting",
                "expected_connectivity_status_text": "System starting up",
            },
            {
                "name": "Old system state, no heartbeat (should be 'offline')",
                "sql_file": "systems_status_scenario_offline",
                "expected_connectivity_status": "offline",
                "expected_connectivity_status_text": "No heartbeats",
            },
            {
                "name": "Recent heartbeat, recent system state (should be 'online')",
                "sql_file": "systems_status_scenario_online",
                "expected_connectivity_status": "online",
                "expected_connectivity_status_text": "Active",
            },
            {
                "name": "Old heartbeat, recent system state (should be 'starting')",
                "sql_file": "systems_status_scenario_restarted",
                "expected_connectivity_status": "starting",
                "expected_connectivity_status_text": "System restarted",
            },
            {
                "name": "Old heartbeat, old system state (should be 'stale')",
                "sql_file": "systems_status_scenario_stale",
                "expected_connectivity_status": "stale",
                "expected_connectivity_status_text": "Heartbeat overdue",
            },
        ]

        for scenario in test_scenarios:
            ctx.logger.log_info(f"Testing scenario: {scenario['name']}")

            try:
                scenario_sql = SystemsStatusTableTests._load_sql(scenario["sql_file"])
                result = SystemsStatusTableTests._execute_sql_with_logging(
                    ctx, scenario_sql, f"Scenario: {scenario['name']}"
                )

                # Parse and validate results
                lines = [
                    line.strip() for line in result.strip().split("\n") if line.strip()
                ]

                if not lines:
                    SystemsStatusTableTests._log_sql_on_failure(
                        ctx,
                        scenario_sql,
                        f"Scenario: {scenario['name']}",
                        f"No results returned for scenario: {scenario['name']}",
                    )
                    ctx.logger.log_error(
                        f"No results returned for scenario: {scenario['name']}"
                    )
                    continue

                # Parse the result line (pipe-separated values)
                parts = [part.strip() for part in lines[0].split("|")]

                if len(parts) < 4:
                    SystemsStatusTableTests._log_sql_on_failure(
                        ctx,
                        scenario_sql,
                        f"Scenario: {scenario['name']}",
                        f"Invalid result format: {lines[0]}",
                    )
                    ctx.logger.log_error(f"Invalid result format: {lines[0]}")
                    continue

                actual_hostname = parts[0]
                actual_connectivity_status = parts[1]
                actual_connectivity_status_text = parts[2]

                # Determine expected hostname from SQL file content
                expected_hostname = scenario["sql_file"].split("_")[
                    -1
                ]  # e.g., "starting" from "scenario_starting"
                expected_hostname = f"test-{expected_hostname}"

                # Assertions
                hostname_match = actual_hostname == expected_hostname
                connectivity_status_match = (
                    actual_connectivity_status
                    == scenario["expected_connectivity_status"]
                )
                connectivity_status_text_match = (
                    actual_connectivity_status_text
                    == scenario["expected_connectivity_status_text"]
                )

                if (
                    hostname_match
                    and connectivity_status_match
                    and connectivity_status_text_match
                ):
                    ctx.logger.log_success(f"✅ Scenario '{scenario['name']}' PASSED")
                else:
                    SystemsStatusTableTests._log_sql_on_failure(
                        ctx,
                        scenario_sql,
                        f"Scenario: {scenario['name']}",
                        f"Expected: hostname={expected_hostname}, connectivity_status={scenario['expected_connectivity_status']}, connectivity_status_text={scenario['expected_connectivity_status_text']}",
                    )
                    ctx.logger.log_error(f"❌ Scenario '{scenario['name']}' FAILED")
                    ctx.logger.log_error(
                        f"  Expected: hostname={expected_hostname}, connectivity_status={scenario['expected_connectivity_status']}, connectivity_status_text={scenario['expected_connectivity_status_text']}"
                    )
                    ctx.logger.log_error(
                        f"  Actual:   hostname={actual_hostname}, connectivity_status={actual_connectivity_status}, connectivity_status_text={actual_connectivity_status_text}"
                    )

            except Exception as e:
                ctx.logger.log_error(f"❌ Scenario '{scenario['name']}' ERROR: {e}")

        ctx.logger.log_success("Connectivity status logic scenarios testing completed")

    @staticmethod
    def _test_update_status_logic(ctx: CrystalForgeTestContext) -> None:
        """Test update status logic scenarios"""
        ctx.logger.log_info("Testing update status logic scenarios...")

        try:
            update_logic_sql = SystemsStatusTableTests._load_sql(
                "systems_status_update_status_logic"
            )
            result = SystemsStatusTableTests._execute_sql_with_logging(
                ctx, update_logic_sql, "Update status logic test"
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            if lines:
                parts = [part.strip() for part in lines[0].split("|")]
                if len(parts) >= 3:
                    hostname = parts[0]
                    connectivity_status = parts[1]
                    update_status = parts[2]

                    # Verify update status logic
                    hostname_correct = hostname == "test-update-sys"
                    update_behind = (
                        update_status == "behind"
                    )  # Should be behind since running old derivation

                    if hostname_correct and update_behind:
                        ctx.logger.log_success("✅ Update status logic test PASSED")
                        ctx.logger.log_info(
                            f"  System correctly identified as: {update_status}"
                        )
                    else:
                        SystemsStatusTableTests._log_sql_on_failure(
                            ctx,
                            update_logic_sql,
                            "Update status logic test",
                            f"Expected: behind, Got: {update_status}",
                        )
                        ctx.logger.log_error("❌ Update status logic test FAILED")
                        ctx.logger.log_error(
                            f"  Expected: behind, Got: {update_status}"
                        )
                else:
                    SystemsStatusTableTests._log_sql_on_failure(
                        ctx,
                        update_logic_sql,
                        "Update status logic test",
                        "Insufficient columns in update status result",
                    )
                    ctx.logger.log_error(
                        "❌ Insufficient columns in update status result"
                    )
            else:
                SystemsStatusTableTests._log_sql_on_failure(
                    ctx,
                    update_logic_sql,
                    "Update status logic test",
                    "No results returned for update status test",
                )
                ctx.logger.log_error("❌ No results returned for update status test")

        except Exception as e:
            ctx.logger.log_error(f"❌ Update status logic test ERROR: {e}")

    @staticmethod
    def _test_heartbeat_system_state_interactions(ctx: CrystalForgeTestContext) -> None:
        """Test the interaction between heartbeats and system states"""
        ctx.logger.log_info("Testing heartbeat and system state interactions...")

        try:
            interactions_sql = SystemsStatusTableTests._load_sql(
                "systems_status_heartbeat_interactions"
            )
            result = SystemsStatusTableTests._execute_sql_with_logging(
                ctx, interactions_sql, "Heartbeat/System State interaction test"
            )

            lines = [
                line.strip() for line in result.strip().split("\n") if line.strip()
            ]

            if lines:
                parts = [part.strip() for part in lines[0].split("|")]
                if len(parts) >= 5:
                    hostname = parts[0]
                    connectivity_status = parts[1]
                    connectivity_status_text = parts[2]
                    agent_version = parts[3]
                    ip_address = parts[4]

                    # Assertions for latest state/heartbeat usage
                    hostname_correct = hostname == "test-multi"
                    status_should_be_online = connectivity_status == "online"
                    version_is_latest = agent_version == "1.2.0"
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
                            f"  Uses latest heartbeat version: {agent_version}"
                        )
                    else:
                        SystemsStatusTableTests._log_sql_on_failure(
                            ctx,
                            interactions_sql,
                            "Heartbeat/System State interaction test",
                            f"hostname_correct: {hostname_correct}, status_should_be_online: {status_should_be_online}, version_is_latest: {version_is_latest} (got {agent_version}), ip_is_latest: {ip_is_latest} (got {ip_address})",
                        )
                        ctx.logger.log_error(
                            "❌ Heartbeat/System State interaction test FAILED"
                        )
                        ctx.logger.log_error(f"  hostname_correct: {hostname_correct}")
                        ctx.logger.log_error(
                            f"  status_should_be_online: {status_should_be_online}"
                        )
                        ctx.logger.log_error(
                            f"  version_is_latest: {version_is_latest} (got {agent_version})"
                        )
                        ctx.logger.log_error(
                            f"  ip_is_latest: {ip_is_latest} (got {ip_address})"
                        )
                else:
                    SystemsStatusTableTests._log_sql_on_failure(
                        ctx,
                        interactions_sql,
                        "Heartbeat/System State interaction test",
                        "Insufficient columns in result",
                    )
                    ctx.logger.log_error("❌ Insufficient columns in result")
            else:
                SystemsStatusTableTests._log_sql_on_failure(
                    ctx,
                    interactions_sql,
                    "Heartbeat/System State interaction test",
                    "No results returned for interaction test",
                )
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
                "sql_file": "systems_status_edge_boundary",
                "expected_connectivity_status": "offline",
            },
            {
                "name": "System with NULL optional fields",
                "sql_file": "systems_status_edge_nulls",
                "expected_connectivity_status": "starting",
            },
        ]

        for test_case in edge_case_tests:
            try:
                edge_case_sql = SystemsStatusTableTests._load_sql(test_case["sql_file"])
                result = SystemsStatusTableTests._execute_sql_with_logging(
                    ctx, edge_case_sql, f"Edge case: {test_case['name']}"
                ).strip()

                if result == test_case["expected_connectivity_status"]:
                    ctx.logger.log_success(f"✅ Edge case '{test_case['name']}' PASSED")
                else:
                    SystemsStatusTableTests._log_sql_on_failure(
                        ctx,
                        edge_case_sql,
                        f"Edge case: {test_case['name']}",
                        f"Expected: {test_case['expected_connectivity_status']}, Got: {result}",
                    )
                    ctx.logger.log_error(f"❌ Edge case '{test_case['name']}' FAILED")
                    ctx.logger.log_error(
                        f"  Expected: {test_case['expected_connectivity_status']}, Got: {result}"
                    )

            except Exception as e:
                ctx.logger.log_error(f"❌ Edge case '{test_case['name']}' ERROR: {e}")

    @staticmethod
    def _test_view_performance(ctx: CrystalForgeTestContext) -> None:
        """Test view performance with EXPLAIN ANALYZE"""
        ctx.logger.log_info("Testing view performance...")

        try:
            # Test performance analysis
            view_performance_sql = SystemsStatusTableTests._load_sql(
                "systems_status_view_performance"
            )
            ctx.logger.capture_command_output(
                ctx.server,
                f'sudo -u postgres psql crystal_forge -c "{view_performance_sql}"',
                "view-performance-analysis.txt",
                "View performance analysis",
            )

            # Test simple timing
            view_timing_sql = SystemsStatusTableTests._load_sql(
                "systems_status_view_timing"
            )
            ctx.logger.capture_command_output(
                ctx.server,
                f'sudo -u postgres psql crystal_forge -c "{view_timing_sql}"',
                "view-timing-test.txt",
                "View timing test",
            )

            ctx.logger.log_success("Performance testing completed")

        except Exception as e:
            ctx.logger.log_error(f"❌ View performance test ERROR: {e}")

    @staticmethod
    def cleanup_test_data(ctx: CrystalForgeTestContext) -> None:
        """Clean up any test data that might have been left behind"""
        ctx.logger.log_info("Cleaning up view test data...")

        try:
            cleanup_sql = SystemsStatusTableTests._load_sql("systems_status_cleanup")
            SystemsStatusTableTests._execute_sql_with_logging(
                ctx, cleanup_sql, "Cleanup test data"
            )
            ctx.logger.log_success("View test data cleanup completed")
        except Exception as e:
            ctx.logger.log_warning(f"Could not clean up test data: {e}")
