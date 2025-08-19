from .test_context import CrystalForgeTestContext


class GitServerTests:
    """Git server setup and verification tests"""

    @staticmethod
    def setup_and_verify(ctx: CrystalForgeTestContext) -> None:
        """Complete git server setup and verification"""
        GitServerTests._start_git_server(ctx)
        GitServerTests._verify_git_accessibility(ctx)
        GitServerTests._test_flake_operations(ctx)

    @staticmethod
    def _start_git_server(ctx: CrystalForgeTestContext) -> None:
        """Start and verify git server"""
        ctx.logger.log_section("🚀 Starting Git Server")

        from .test_patterns import TestPatterns

        TestPatterns.standard_service_startup(
            ctx.logger,
            ctx.gitserver,
            [
                "git-http-server.service",
                "multi-user.target",
            ],
        )

        ctx.gitserver.wait_for_open_port(8080)
        ctx.logger.log_success("Git server is listening on port 8080")

    @staticmethod
    def _verify_git_accessibility(ctx: CrystalForgeTestContext) -> None:
        """Verify git repository accessibility"""
        ctx.logger.log_section("🔍 Verifying Git Server Setup")

        ctx.gitserver.succeed("ls -la /srv/git/crystal-forge.git/")
        ctx.logger.log_success("Git repository is accessible")

        ctx.gitserver.succeed(
            "cd /tmp && git clone /srv/git/crystal-forge.git crystal-forge-checkout"
        )
        ctx.gitserver.succeed("ls -la /tmp/crystal-forge-checkout/")
        ctx.logger.log_success("Git repository can be cloned locally")

    @staticmethod
    def _test_flake_operations(ctx: CrystalForgeTestContext) -> None:
        """Test flake operations from server"""
        ctx.logger.log_section("📦 Testing Flake Operations from Server")

        # Verify server can access git server
        ctx.server.succeed("ping -c1 gitserver")
        ctx.logger.log_success("Server can reach git server")

        # Test git access from server
        ctx.server.succeed("git ls-remote git://gitserver:8080/crystal-forge.git")
        ctx.logger.log_success("Server can access git repository remotely")

        # Test flake operations
        ctx.server.succeed(
            "nix flake show git://gitserver:8080/crystal-forge.git --no-write-lock-file"
        )
        ctx.logger.log_success("Server can show flake metadata")
