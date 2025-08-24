from ..runtime.test_context import CrystalForgeTestContext


class AgentTests:
    """Crystal Forge agent tests"""

    @staticmethod
    def setup_and_verify(ctx: CrystalForgeTestContext) -> None:
        """Complete agent setup and verification"""
        AgentTests._start_agent(ctx)
        AgentTests._verify_agent_connection(ctx)
        AgentTests._verify_key_files(ctx)

    @staticmethod
    def _start_agent(ctx: CrystalForgeTestContext) -> None:
        """Start Crystal Forge agent"""
        ctx.logger.log_section("🤖 Starting Crystal Forge Agent")

        from ..utils.test_patterns import TestPatterns

        TestPatterns.standard_service_startup(
            ctx.logger,
            ctx.agent,
            [
                "crystal-forge-agent.service",
            ],
        )

    @staticmethod
    def _verify_agent_connection(ctx: CrystalForgeTestContext) -> None:
        """Verify agent connection to server"""
        ctx.logger.log_section("🤝 Waiting for agent to connect to server...")
        ctx.server.wait_until_succeeds(
            "journalctl -u crystal-forge-server.service | grep -E 'accepted.*agent'"
        )
        ctx.logger.log_success("Agent successfully connected to server")

    @staticmethod
    def _verify_key_files(ctx: CrystalForgeTestContext) -> None:
        """Verify key file accessibility"""
        from ..utils.test_patterns import TestPatterns

        TestPatterns.key_file_verification(
            ctx.logger,
            ctx.agent,
            {
                f"{ctx.cf_key_dir}/agent.key": "Agent private key accessible",
                f"{ctx.cf_key_dir}/agent.pub": "Agent public key accessible on agent",
            },
        )

        ctx.server.succeed(f"test -r {ctx.cf_key_dir}/agent.pub")
        ctx.logger.log_success("Agent public key accessible on server")
