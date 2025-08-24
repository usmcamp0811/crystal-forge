from ..runtime.test_context import CrystalForgeTestContext


class FlakeProcessingTests:
    """Tests for flake processing workflow"""

    @staticmethod
    def verify_complete_workflow(ctx: CrystalForgeTestContext) -> None:
        """Verify the complete flake processing workflow"""
        FlakeProcessingTests._wait_for_flake_processing(ctx)
        FlakeProcessingTests._wait_for_commit_processing(ctx)
        FlakeProcessingTests._wait_for_derivation_evaluation(ctx)
        FlakeProcessingTests._verify_cf_test_sys_processing(ctx)

    @staticmethod
    def _wait_for_flake_processing(ctx: CrystalForgeTestContext) -> None:
        """Wait for flake to be processed"""
        ctx.logger.log_section("⏳ Waiting for flake processing...")
        ctx.server.wait_until_succeeds(
            """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM flakes WHERE name = 'crystal-forge'" -t | grep -E '^\\s*1\\s*$'""",
            timeout=120,
        )
        ctx.logger.log_success(
            "Flake 'crystal-forge' has been processed and stored in database"
        )

    @staticmethod
    def _wait_for_commit_processing(ctx: CrystalForgeTestContext) -> None:
        """Wait for commits to be processed"""
        ctx.logger.log_section("📝 Waiting for commit processing...")
        ctx.server.wait_until_succeeds(
            """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM commits c JOIN flakes f ON c.flake_id = f.id WHERE f.name = 'crystal-forge'" -t | grep -v '^\\s*0\\s*$'""",
            timeout=180,
        )
        ctx.logger.log_success("Commits have been processed for crystal-forge flake")

    @staticmethod
    def _wait_for_derivation_evaluation(ctx: CrystalForgeTestContext) -> None:
        """Wait for system evaluation (derivations)"""
        ctx.logger.log_section("🔍 Waiting for system evaluation...")
        ctx.server.wait_until_succeeds(
            """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = 'crystal-forge'" -t | grep -v '^\\s*0\\s*$'""",
            timeout=300,
        )
        ctx.logger.log_success("System derivations have been evaluated")

    @staticmethod
    def _verify_cf_test_sys_processing(ctx: CrystalForgeTestContext) -> None:
        """Verify cf-test-sys specific processing"""
        ctx.logger.log_section("🎯 Verifying cf-test-sys Processing")

        # Check if cf-test-sys derivation exists and has been dry-run evaluated
        cf_test_sys_count = ctx.server.succeed(
            """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = 'crystal-forge' AND d.derivation_name LIKE '%cf-test-sys%'" -t"""
        ).strip()

        ctx.logger.log_info(f"cf-test-sys derivations found: {cf_test_sys_count}")

        if int(cf_test_sys_count) > 0:
            ctx.logger.log_success(
                "cf-test-sys derivation successfully stored in database"
            )

            # Get detailed information about cf-test-sys processing
            cf_test_sys_details = ctx.logger.database_query(
                ctx.server,
                "crystal_forge",
                "SELECT d.derivation_name, d.derivation_type, d.derivation_target, d.derivation_path, ds.name as status FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id JOIN derivation_statuses ds ON d.status_id = ds.id WHERE f.name = 'crystal-forge' AND d.derivation_name LIKE '%cf-test-sys%' LIMIT 3;",
                "cf-test-sys-details.txt",
            )

            # Verify that dry-run has been completed
            dry_run_complete_count = ctx.server.succeed(
                """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id JOIN derivation_statuses ds ON d.status_id = ds.id WHERE f.name = 'crystal-forge' AND d.derivation_name LIKE '%cf-test-sys%' AND ds.name IN ('dry-run-complete', 'build-pending', 'build-complete')" -t"""
            ).strip()

            if int(dry_run_complete_count) > 0:
                ctx.logger.log_success(
                    "cf-test-sys has been successfully dry-run evaluated"
                )
            else:
                ctx.logger.log_warning(
                    "cf-test-sys dry-run evaluation may still be in progress"
                )

        else:
            ctx.logger.log_warning(
                "cf-test-sys derivation not found - may need longer evaluation time"
            )

            # Debug: show all available derivation names
            all_derivations = ctx.logger.database_query(
                ctx.server,
                "crystal_forge",
                "SELECT DISTINCT d.derivation_name FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = 'crystal-forge' ORDER BY d.derivation_name LIMIT 20;",
                "all-derivation-names.txt",
            )
            ctx.logger.log_info("Captured all available derivation names for debugging")
