from .test_context import CrystalForgeTestContext
from .view_tests import DeploymentStatusViewTests, SystemsStatusTableTests


class DatabaseTests:
    """Database setup and verification tests"""

    @staticmethod
    def setup_and_verify(ctx: CrystalForgeTestContext) -> None:
        """Complete database setup and verification"""
        DatabaseTests._setup_postgresql(ctx)
        DatabaseTests._verify_database_functionality(ctx)
        DatabaseTests._run_view_tests(ctx)

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

    @staticmethod
    def _run_view_tests(ctx: CrystalForgeTestContext) -> None:
        """Run all view tests"""
        ctx.logger.log_section("🔍 Running Database View Tests")

        # Check if database is ready for testing
        if not DatabaseTests._check_database_ready(ctx):
            ctx.logger.log_warning("Database not ready for view testing - skipping")
            return

        # Run view test suites
        SystemsStatusTableTests.run_all_tests(ctx)
        DeploymentStatusViewTests.run_all_tests(ctx)

    @staticmethod
    def _check_database_ready(ctx: CrystalForgeTestContext) -> bool:
        """Check if the database is ready for testing"""
        # Check if the crystal-forge database user exists
        try:
            ctx.server.succeed(
                "sudo -u postgres psql -t -c \"SELECT 1 FROM pg_roles WHERE rolname = 'crystal-forge';\""
            )
            ctx.logger.log_info("crystal-forge database user exists")
        except Exception:
            ctx.logger.log_warning(
                "crystal-forge database user not found (server not started yet)"
            )
            return False

        # Check if the database exists
        try:
            ctx.server.succeed(
                "sudo -u postgres psql -t -c \"SELECT 1 FROM pg_database WHERE datname = 'crystal_forge';\""
            )
            ctx.logger.log_info("crystal_forge database exists")
        except Exception:
            ctx.logger.log_warning(
                "crystal_forge database not found (server not started yet)"
            )
            return False

        return True
