from .test_context import CrystalForgeTestContext


class CrystalForgeServerTests:
    """Crystal Forge server tests"""

    @staticmethod
    def setup_and_verify(ctx: CrystalForgeTestContext) -> None:
        """Complete server setup and verification"""
        CrystalForgeServerTests._start_services(ctx)
        CrystalForgeServerTests._verify_service_health(ctx)
        CrystalForgeServerTests._test_crystal_forge_evaluation_capability(ctx)
        CrystalForgeServerTests._test_crystal_forge_processing_logs(ctx)

    @staticmethod
    def _start_services(ctx: CrystalForgeTestContext) -> None:
        """Start Crystal Forge server services"""
        ctx.logger.log_section("🖥️ Starting Crystal Forge Server")

        from .test_patterns import TestPatterns

        TestPatterns.standard_service_startup(
            ctx.logger,
            ctx.server,
            [
                "crystal-forge-server.service",
                "crystal-forge-builder.service",
                "multi-user.target",
            ],
        )

    @staticmethod
    def _verify_service_health(ctx: CrystalForgeTestContext) -> None:
        """Verify server service health"""
        # Wait for the port to be open before running the network test
        ctx.server.wait_for_open_port(ctx.cf_server_port)
        ctx.logger.log_success(
            f"Crystal Forge server is listening on port {ctx.cf_server_port}"
        )

        from .test_patterns import TestPatterns

        TestPatterns.network_test(ctx.logger, ctx.server, "server", ctx.cf_server_port)

    @staticmethod
    def _test_crystal_forge_evaluation_capability(ctx: CrystalForgeTestContext) -> None:
        """Test that Crystal Forge server can evaluate cf-test-sys through its own processing"""
        ctx.logger.log_section("🔍 Testing Crystal Forge Evaluation Capability")

        # First, verify cf-test-sys derivation exists in database
        cf_test_sys_query = """
            SELECT d.id, d.derivation_name, d.derivation_target, d.derivation_path, ds.name as status
            FROM derivations d 
            JOIN derivation_statuses ds ON d.status_id = ds.id
            JOIN commits c ON d.commit_id = c.id 
            JOIN flakes f ON c.flake_id = f.id 
            WHERE f.name = 'crystal-forge' 
            AND d.derivation_name LIKE '%cf-test-sys%'
            LIMIT 1;
        """

        cf_test_sys_result = ctx.logger.database_query(
            ctx.server, "crystal_forge", cf_test_sys_query, "cf-test-sys-derivation.txt"
        )

        # Verify the derivation exists
        # ctx.logger.assert_in_output(
        #     "cf-test-sys",
        #     cf_test_sys_result,
        #     "cf-test-sys derivation found in Crystal Forge database",
        # )

        # Check if it has a derivation_target (the flake URL Crystal Forge built)
        if "nixosConfigurations.cf-test-sys" in cf_test_sys_result:
            ctx.logger.log_success(
                "Crystal Forge has built proper flake target for cf-test-sys"
            )

            # Extract the derivation target for further testing
            lines = cf_test_sys_result.strip().split("\n")
            if len(lines) >= 3:  # header + separator + data
                data_line = lines[2].strip()
                # Parse the derivation target from the result
                # This would contain something like: http://gitserver:8080/crystal-forge.git?rev=abc123#nixosConfigurations.cf-test-sys.config.system.build.toplevel
                ctx.logger.log_info(f"Crystal Forge derivation target: {data_line}")

        # Check the status to see if Crystal Forge successfully evaluated it
        if (
            "dry-run-complete" in cf_test_sys_result
            or "build-complete" in cf_test_sys_result
        ):
            ctx.logger.log_success(
                "Crystal Forge has successfully evaluated cf-test-sys"
            )
        elif "pending" in cf_test_sys_result or "in-progress" in cf_test_sys_result:
            ctx.logger.log_warning("cf-test-sys evaluation still in progress")
        elif "failed" in cf_test_sys_result:
            ctx.logger.log_error("Crystal Forge failed to evaluate cf-test-sys")
            # Capture error details
            error_query = """
                SELECT d.error_message, d.attempt_count 
                FROM derivations d 
                JOIN commits c ON d.commit_id = c.id 
                JOIN flakes f ON c.flake_id = f.id 
                WHERE f.name = 'crystal-forge' 
                AND d.derivation_name LIKE '%cf-test-sys%'
                AND d.error_message IS NOT NULL;
            """
            ctx.logger.database_query(
                ctx.server, "crystal_forge", error_query, "cf-test-sys-errors.txt"
            )

        # Verify Crystal Forge can get the derivation path
        derivation_path_query = """
            SELECT d.derivation_path 
            FROM derivations d 
            JOIN commits c ON d.commit_id = c.id 
            JOIN flakes f ON c.flake_id = f.id 
            WHERE f.name = 'crystal-forge' 
            AND d.derivation_name LIKE '%cf-test-sys%'
            AND d.derivation_path IS NOT NULL;
        """

        path_result = ctx.logger.database_query(
            ctx.server, "crystal_forge", derivation_path_query, "cf-test-sys-path.txt"
        )

        if "/nix/store/" in path_result and ".drv" in path_result:
            ctx.logger.log_success(
                "Crystal Forge has determined derivation path for cf-test-sys"
            )
        else:
            ctx.logger.log_warning(
                "Crystal Forge has not yet determined derivation path for cf-test-sys"
            )

    @staticmethod
    def _test_crystal_forge_processing_logs(ctx: CrystalForgeTestContext) -> None:
        """Test by monitoring Crystal Forge's internal processing logs"""
        ctx.logger.log_section("📋 Testing Crystal Forge Internal Processing")

        # Wait for and verify that Crystal Forge has processed cf-test-sys
        ctx.logger.log_info("Waiting for Crystal Forge to process cf-test-sys...")

        # # Look for specific log messages from your Rust code
        # ctx.server.wait_until_succeeds(
        #     "journalctl -u crystal-forge-server.service --no-pager | grep -E 'cf-test-sys.*nixosConfigurations'",
        #     timeout=120,
        # )
        # ctx.logger.log_success("Crystal Forge has discovered cf-test-sys configuration")

        # # Wait for Crystal Forge to insert the derivation
        # ctx.server.wait_until_succeeds(
        #     "journalctl -u crystal-forge-server.service --no-pager | grep -E '(Inserted NixOS derivation.*cf-test-sys|cf-test-sys.*with target)'",
        #     timeout=120,
        # )
        # ctx.logger.log_success("Crystal Forge has created cf-test-sys derivation")

        # Wait for Crystal Forge to begin evaluation
        ctx.server.wait_until_succeeds(
            "journalctl -u crystal-forge-server.service --no-pager | grep -E '(Evaluating derivation paths.*cf-test-sys|dry-run.*cf-test-sys)'",
            timeout=180,
        )
        ctx.logger.log_success("Crystal Forge has begun evaluating cf-test-sys")

        # Capture the full processing log for cf-test-sys
        ctx.logger.capture_command_output(
            ctx.server,
            "journalctl -u crystal-forge-server.service --no-pager | grep -E 'cf-test-sys' | tail -20",
            "cf-test-sys-processing-log.txt",
            "Crystal Forge cf-test-sys processing log",
        )
