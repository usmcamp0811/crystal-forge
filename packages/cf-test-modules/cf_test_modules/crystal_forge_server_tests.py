from .test_context import CrystalForgeTestContext


class CrystalForgeServerTests:
    """Crystal Forge server tests"""

    @staticmethod
    def setup_and_verify(ctx: CrystalForgeTestContext) -> None:
        """Complete server setup and verification"""
        CrystalForgeServerTests._start_services(ctx)
        CrystalForgeServerTests._verify_service_health(ctx)
        CrystalForgeServerTests._test_dry_run_capability(ctx)

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
        from .test_patterns import TestPatterns

        TestPatterns.network_test(ctx.logger, ctx.server, "server", 3000)

    @staticmethod
    def _test_dry_run_capability(ctx: CrystalForgeTestContext) -> None:
        """Test that server can perform dry-run builds"""
        ctx.logger.log_section("🏗️ Testing Dry Run Build Capability")

        # Test that the server can do a dry run build of cf-test-sys
        ctx.logger.capture_command_output(
            ctx.server,
            "nix build git://gitserver:8080/crystal-forge.git#nixosConfigurations.cf-test-sys.config.system.build.toplevel --dry-run --no-write-lock-file",
            "dry-run-output.txt",
            "Dry run build of cf-test-sys",
        )
        ctx.logger.log_success("Server can perform dry run build of cf-test-sys")
