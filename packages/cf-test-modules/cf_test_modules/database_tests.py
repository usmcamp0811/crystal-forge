
from .test_context import CrystalForgeTestContext
class DatabaseTests:
    """Database setup and verification tests"""

    @staticmethod
    def setup_and_verify(ctx: CrystalForgeTestContext) -> None:
        """Complete database setup and verification"""
        DatabaseTests._setup_postgresql(ctx)
        DatabaseTests._verify_database_functionality(ctx)

    @staticmethod
    def _setup_postgresql(ctx: CrystalForgeTestContext) -> None:
        """Setup and verify PostgreSQL"""
        ctx.logger.log_section("🗄️ Setting up PostgreSQL Database")

        # Debug PostgreSQL status
        ctx.logger.capture_command_output(
            ctx.server,
            "systemctl status postgresql.service || true",
            "postgresql-status-before.txt",
            "PostgreSQL status before startup",
        )

        # Check if PostgreSQL service exists
        ctx.logger.capture_command_output(
            ctx.server,
            "systemctl list-unit-files | grep postgresql || echo 'No PostgreSQL unit files found'",
            "postgresql-units.txt",
            "PostgreSQL unit files",
        )

        # Wait for PostgreSQL
        ctx.logger.log_info("Waiting for PostgreSQL to start...")
        ctx.server.wait_for_unit("postgresql.service")
        ctx.logger.log_success("PostgreSQL is ready")

    @staticmethod
    def _verify_database_functionality(ctx: CrystalForgeTestContext) -> None:
        """Verify PostgreSQL functionality"""
        ctx.server.succeed("sudo -u postgres psql -c 'SELECT version();'")
        ctx.logger.log_success("PostgreSQL is functional")
