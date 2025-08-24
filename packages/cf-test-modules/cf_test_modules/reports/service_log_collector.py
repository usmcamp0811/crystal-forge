from ..runtime.test_context import CrystalForgeTestContext


class ServiceLogCollector:
    """Centralized service log collection"""

    @staticmethod
    def collect_all_logs(ctx: CrystalForgeTestContext) -> None:
        """Collect logs from all services across all VMs"""
        ctx.logger.log_section("📋 Collecting Service Logs from All VMs")

        ServiceLogCollector._collect_gitserver_logs(ctx)
        ServiceLogCollector._collect_server_logs(ctx)
        ServiceLogCollector._collect_agent_logs(ctx)
        ServiceLogCollector._collect_system_info(ctx)

    @staticmethod
    def _collect_gitserver_logs(ctx: CrystalForgeTestContext) -> None:
        """Collect git server logs"""
        ctx.logger.log_info("Collecting Git Server logs...")
        ctx.logger.capture_service_logs(ctx.gitserver, "git-http-server.service")

        # Also capture git repository state
        ctx.logger.capture_command_output(
            ctx.gitserver,
            "ls -la /srv/git/crystal-forge.git/",
            "git-repo-structure.txt",
            "Git repository structure",
        )

    @staticmethod
    def _collect_server_logs(ctx: CrystalForgeTestContext) -> None:
        """Collect Crystal Forge server logs"""
        ctx.logger.log_info("Collecting Crystal Forge Server logs...")
        ctx.logger.capture_service_logs(ctx.server, "crystal-forge-server.service")
        ctx.logger.capture_service_logs(ctx.server, "crystal-forge-builder.service")
        ctx.logger.capture_service_logs(ctx.server, "postgresql.service")

        # Capture additional server state
        ctx.logger.capture_command_output(
            ctx.server,
            "systemctl status crystal-forge-server.service crystal-forge-builder.service",
            "server-service-status.txt",
            "Server service status",
        )

    @staticmethod
    def _collect_agent_logs(ctx: CrystalForgeTestContext) -> None:
        """Collect Crystal Forge agent logs"""
        ctx.logger.log_info("Collecting Crystal Forge Agent logs...")
        ctx.logger.capture_service_logs(ctx.agent, "crystal-forge-agent.service")

    @staticmethod
    def _collect_system_info(ctx: CrystalForgeTestContext) -> None:
        """Collect system information from all VMs"""
        ctx.logger.log_info("Collecting system information...")

        # Network connectivity information
        ctx.logger.capture_command_output(
            ctx.server,
            f"ss -tlnp | grep -E ':({ctx.cf_server_port}|{ctx.db_port})'",
            "server-network-ports.txt",
            "Server network ports",
        )
