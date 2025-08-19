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
                "cgit-gitserver.service",
                "git-daemon.service",  # Also updated from git-http-server
                "nginx.service",  # nginx for cgit web interface
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
    def test_flake_operations(ctx: CrystalForgeTestContext) -> None:
        """Test flake operations from server using both git:// and HTTP protocols"""
        ctx.logger.log_section("📦 Testing Flake Operations from Server")

        # Verify server can access git server
        ctx.server.succeed("ping -c1 gitserver")
        ctx.logger.log_success("Server can reach git server")

        # Test git:// protocol access
        ctx.server.succeed("git ls-remote git://gitserver:8080/crystal-forge.git")
        ctx.logger.log_success("Server can access git repository via git:// protocol")

        # Test HTTP protocol access via cgit
        ctx.server.succeed("git ls-remote http://gitserver/crystal-forge.git")
        ctx.logger.log_success("Server can access git repository via HTTP protocol")

        # Test cgit web interface is accessible
        ctx.server.succeed("curl -f http://gitserver/")
        ctx.logger.log_success("cgit web interface is accessible")

        # Test flake operations with git:// protocol
        ctx.server.succeed(
            "nix flake show git://gitserver:8080/crystal-forge.git --no-write-lock-file"
        )
        ctx.logger.log_success("Server can show flake metadata via git:// protocol")

        # Test flake operations with HTTP protocol
        ctx.server.succeed(
            "nix flake show http://gitserver/crystal-forge.git --no-write-lock-file"
        )
        ctx.logger.log_success("Server can show flake metadata via HTTP protocol")

        # Test cloning via HTTP (more comprehensive test)
        ctx.server.succeed(
            "cd /tmp && git clone http://gitserver/crystal-forge.git test-clone && ls test-clone/flake.nix"
        )
        ctx.logger.log_success(
            "Server can clone repository via HTTP and verify contents"
        )
