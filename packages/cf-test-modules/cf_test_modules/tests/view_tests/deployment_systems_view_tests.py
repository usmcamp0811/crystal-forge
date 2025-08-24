from __future__ import annotations

from ..test_context import CrystalForgeTestContext
from .base import BaseViewTests


class DeploymentStatusViewTests(BaseViewTests):
    """Test suite for view_deployment_status using hybrid approach"""

    @staticmethod
    def run_all_tests(ctx: CrystalForgeTestContext) -> None:
        ctx.logger.log_section("🚀 Testing view_deployment_status")

        if not BaseViewTests.view_exists(ctx, "view_deployment_status"):
            ctx.logger.log_warning("View does not exist - skipping remaining tests")
            return

        # Test using pure SQL approach for complex edge cases
        DeploymentStatusViewTests._test_complex_deployment_states(ctx)

        # Test using hybrid approach for realistic scenarios
        DeploymentStatusViewTests._test_successful_deployment_scenario(ctx)
        DeploymentStatusViewTests._test_failed_build_scenario(ctx)
        DeploymentStatusViewTests._test_system_deployment_mismatch(ctx)

        # Performance and cleanup
        BaseViewTests.capture_view_performance(ctx, "view_deployment_status")
        BaseViewTests.cleanup_test_data(
            ctx, hostname_patterns=["test-deploy-%"], flake_patterns=["test-deploy-%"]
        )

    @staticmethod
    def _test_complex_deployment_states(ctx: CrystalForgeTestContext) -> None:
        """Test complex deployment state logic using SQL"""
        sql = BaseViewTests._load_sql("deployment_status_complex_states")

        rows = BaseViewTests._query_rows(ctx, sql, "Complex deployment states test")

        # Assert we have expected test scenarios
        BaseViewTests._assert_rows_count(
            ctx, rows, 4, sql=sql, test_name="Complex deployment states"
        )

        # Check specific deployment states
        states = [row[2] for row in rows]  # assuming deployment_status is 3rd column
        expected_states = ["deployed", "build-failed", "pending", "unknown"]
        BaseViewTests._assert_in_set(
            ctx,
            states,
            expected_states,
            sql=sql,
            test_name="Complex deployment states",
            description="deployment states",
        )

        ctx.logger.log_success("✅ Complex deployment states test PASSED")

    @staticmethod
    def _test_successful_deployment_scenario(ctx: CrystalForgeTestContext) -> None:
        """Test successful deployment scenario using hybrid approach"""

        def setup_server_data():
            # Create a successful build scenario
            build_data = BaseViewTests._create_complete_build_scenario(
                ctx,
                flake_name="test-deploy-app",
                repo_url="https://github.com/test/deploy-app.git",
                commit_hash="abc123def456",
                system_name="web-server",
                build_status="build-complete",
                test_name="Setup successful deployment",
            )

            # The derivation path that agents should report
            derivation_path = f"/nix/store/abc123de-web-server.drv"

            return {
                **build_data,
                "derivation_path": derivation_path,
                "expected_hostname": "test-deploy-web01",
            }

        agent_actions = [
            {
                "hostname": "test-deploy-web01",
                "change_reason": "deployment",
                "os_version": "NixOS 25.05",
                "kernel": "6.6.89",
                "memory_gb": 32.0,
                "cpu_brand": "Intel Xeon",
                "cpu_cores": 16,
                # derivation will be auto-injected from server_data
            }
        ]

        query_sql = """
            SELECT 
                hostname,
                flake_name,
                deployment_status,
                deployment_status_text,
                commit_hash,
                build_status,
                last_seen
            FROM view_deployment_status 
            WHERE hostname = 'test-deploy-web01'
            ORDER BY last_seen DESC;
        """

        def assert_successful_deployment(rows):
            if not rows:
                return "No deployment status found for test-deploy-web01"

            row = rows[0]
            if len(row) < 7:
                return f"Expected ≥7 columns, got {len(row)}: {row}"

            (
                hostname,
                flake_name,
                deploy_status,
                status_text,
                commit_hash,
                build_status,
                last_seen,
            ) = row

            if hostname != "test-deploy-web01":
                return f"hostname mismatch: expected test-deploy-web01, got {hostname}"
            if flake_name != "test-deploy-app":
                return (
                    f"flake_name mismatch: expected test-deploy-app, got {flake_name}"
                )
            if deploy_status != "deployed":
                return f"deployment_status mismatch: expected deployed, got {deploy_status}"
            if commit_hash != "abc123def456":
                return f"commit_hash mismatch: expected abc123def456, got {commit_hash}"
            if build_status != "build-complete":
                return f"build_status mismatch: expected build-complete, got {build_status}"
            if not last_seen:
                return "last_seen should not be empty"

            return None  # Success

        BaseViewTests.run_hybrid_scenario_test(
            ctx,
            test_name="Successful deployment scenario",
            server_setup=setup_server_data,
            agent_actions=agent_actions,
            query_sql=query_sql,
            assertion_func=assert_successful_deployment,
        )

    @staticmethod
    def _test_failed_build_scenario(ctx: CrystalForgeTestContext) -> None:
        """Test failed build scenario using hybrid approach"""

        def setup_failed_build():
            build_data = BaseViewTests._create_complete_build_scenario(
                ctx,
                flake_name="test-deploy-broken",
                repo_url="https://github.com/test/broken-app.git",
                commit_hash="deadbeef1234",
                system_name="broken-service",
                build_status="build-failed",
                error_message="Compilation failed: missing dependency libfoo",
                test_name="Setup failed build",
            )

            return {**build_data, "expected_hostname": "test-deploy-broken01"}

        # Agent reports old derivation since new build failed
        agent_actions = [
            {
                "hostname": "test-deploy-broken01",
                "change_reason": "startup",
                "derivation": "/nix/store/oldversion-broken-service.drv",  # Old working version
                "os_version": "NixOS 25.05",
            }
        ]

        query_sql = """
            SELECT 
                hostname,
                deployment_status,
                build_status,
                error_message
            FROM view_deployment_status 
            WHERE hostname = 'test-deploy-broken01';
        """

        def assert_failed_build(rows):
            if not rows:
                return "No deployment status found for test-deploy-broken01"

            row = rows[0]
            if len(row) < 4:
                return f"Expected ≥4 columns, got {len(row)}: {row}"

            hostname, deploy_status, build_status, error_msg = row

            if (
                deploy_status != "outdated"
            ):  # System running old version due to build failure
                return f"Expected deployment_status=outdated, got {deploy_status}"
            if build_status != "build-failed":
                return f"Expected build_status=build-failed, got {build_status}"
            if not error_msg or "libfoo" not in error_msg:
                return f"Expected error message about libfoo, got: {error_msg}"

            return None

        BaseViewTests.run_hybrid_scenario_test(
            ctx,
            test_name="Failed build scenario",
            server_setup=setup_failed_build,
            agent_actions=agent_actions,
            query_sql=query_sql,
            assertion_func=assert_failed_build,
        )

    @staticmethod
    def _test_system_deployment_mismatch(ctx: CrystalForgeTestContext) -> None:
        """Test scenario where system is running different version than latest build"""

        def setup_version_mismatch():
            # Create two builds: one old (complete), one new (complete)
            old_build = BaseViewTests._create_complete_build_scenario(
                ctx,
                flake_name="test-deploy-multi",
                repo_url="https://github.com/test/multi-version.git",
                commit_hash="old123abc456",
                system_name="multi-service",
                build_status="build-complete",
                test_name="Setup old build",
            )

            # Create newer commit and build
            new_commit_id = BaseViewTests._create_test_commit(
                ctx,
                flake_id=old_build["flake_id"],
                commit_hash="new456def789",
                test_name="Setup new commit",
            )

            new_derivation_id = BaseViewTests._create_test_derivation(
                ctx,
                commit_id=new_commit_id,
                derivation_type="nixos",
                derivation_name="multi-service",
                derivation_path="/nix/store/new456de-multi-service.drv",
                status_id=10,  # build-complete
                test_name="Setup new derivation",
            )

            BaseViewTests._update_derivation_status(
                ctx,
                derivation_id=new_derivation_id,
                status_id=10,
                derivation_path="/nix/store/new456de-multi-service.drv",
                completed_at=True,
                test_name="Complete new build",
            )

            return {
                "old_derivation_path": "/nix/store/old123ab-multi-service.drv",
                "new_derivation_path": "/nix/store/new456de-multi-service.drv",
                "expected_hostname": "test-deploy-multi01",
            }

        # System is still running old version
        agent_actions = [
            {
                "hostname": "test-deploy-multi01",
                "change_reason": "heartbeat",
                "derivation": "/nix/store/old123ab-multi-service.drv",  # Old version
                "os_version": "NixOS 25.05",
            }
        ]

        query_sql = """
            SELECT 
                hostname,
                deployment_status,
                deployment_status_text,
                commit_hash_current,
                commit_hash_latest,
                version_behind
            FROM view_deployment_status 
            WHERE hostname = 'test-deploy-multi01';
        """

        def assert_version_mismatch(rows):
            if not rows:
                return "No deployment status found for test-deploy-multi01"

            row = rows[0]
            if len(row) < 6:
                return f"Expected ≥6 columns, got {len(row)}: {row}"

            hostname, deploy_status, status_text, current_hash, latest_hash, behind = (
                row
            )

            if deploy_status != "outdated":
                return f"Expected deployment_status=outdated, got {deploy_status}"
            if current_hash == latest_hash:
                return f"Current and latest commit should be different: {current_hash}"
            if (
                behind != "1" and behind != "1 commit"
            ):  # Depending on view implementation
                return f"Expected to be 1 commit behind, got: {behind}"
            if "newer version available" not in status_text.lower():
                return f"Expected status text about newer version, got: {status_text}"

            return None

        BaseViewTests.run_hybrid_scenario_test(
            ctx,
            test_name="System deployment mismatch",
            server_setup=setup_version_mismatch,
            agent_actions=agent_actions,
            query_sql=query_sql,
            assertion_func=assert_version_mismatch,
        )
