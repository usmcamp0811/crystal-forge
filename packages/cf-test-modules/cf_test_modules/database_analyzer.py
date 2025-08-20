from .test_context import CrystalForgeTestContext


class DatabaseAnalyzer:
    """Database analysis and reporting"""

    @staticmethod
    def generate_comprehensive_report(ctx: CrystalForgeTestContext) -> None:
        """Generate comprehensive database analysis report"""
        ctx.logger.log_section("📈 Database Statistics and Analysis")

        DatabaseAnalyzer._analyze_flakes(ctx)
        DatabaseAnalyzer._analyze_commits(ctx)
        DatabaseAnalyzer._analyze_derivations(ctx)
        DatabaseAnalyzer._analyze_system_states(ctx)
        DatabaseAnalyzer._generate_summary_report(ctx)

    @staticmethod
    def _generate_summary_report(ctx: CrystalForgeTestContext) -> None:
        """Generate an executive summary report"""
        ctx.logger.log_section("📊 Executive Summary Report")

        # Generate a comprehensive summary query
        summary_query = """
        WITH flake_stats AS (
            SELECT 
                f.name as flake_name,
                COUNT(DISTINCT c.id) as total_commits,
                COUNT(DISTINCT d.id) as total_derivations,
                COUNT(DISTINCT CASE WHEN d.derivation_type = 'nixos' THEN d.id END) as nixos_systems,
                COUNT(DISTINCT CASE WHEN d.derivation_type = 'package' THEN d.id END) as packages,
                COUNT(DISTINCT CASE WHEN ds.name = 'dry-run-complete' THEN d.id END) as dry_run_complete,
                COUNT(DISTINCT CASE WHEN ds.name = 'build-complete' THEN d.id END) as build_complete,
                COUNT(DISTINCT CASE WHEN ds.name LIKE '%failed%' THEN d.id END) as failed
            FROM flakes f
            LEFT JOIN commits c ON f.id = c.flake_id
            LEFT JOIN derivations d ON c.id = d.commit_id  
            LEFT JOIN derivation_statuses ds ON d.status_id = ds.id
            GROUP BY f.name
        ),
        system_stats AS (
            SELECT 
                COUNT(DISTINCT hostname) as unique_agents,
                COUNT(*) as total_system_states,
                COUNT(DISTINCT CASE WHEN change_reason = 'startup' THEN hostname END) as agents_started,
                COUNT(DISTINCT CASE WHEN change_reason = 'heartbeat' THEN hostname END) as agents_heartbeat
            FROM system_states
        )
        SELECT 
            fs.*,
            ss.*
        FROM flake_stats fs, system_stats ss
        WHERE fs.flake_name = 'crystal-forge';
        """

        summary_output = ctx.logger.database_query(
            ctx.server, "crystal_forge", summary_query, "executive-summary.txt"
        )

        # Parse key metrics for logging
        try:
            lines = summary_output.strip().split("\n")
            if len(lines) >= 3:  # Header + separator + data
                data_line = lines[2].strip()
                ctx.logger.log_success(
                    "📊 Test Summary Generated - see executive-summary.txt for details"
                )
        except Exception as e:
            ctx.logger.log_warning(f"Could not parse summary metrics: {e}")

        # Generate status distribution
        status_distribution = ctx.logger.database_query(
            ctx.server,
            "crystal_forge",
            """
            SELECT 
                ds.name as status,
                ds.description,
                COUNT(d.id) as count,
                ROUND(COUNT(d.id) * 100.0 / SUM(COUNT(d.id)) OVER(), 2) as percentage
            FROM derivations d
            JOIN derivation_statuses ds ON d.status_id = ds.id
            JOIN commits c ON d.commit_id = c.id
            JOIN flakes f ON c.flake_id = f.id
            WHERE f.name = 'crystal-forge'
            GROUP BY ds.name, ds.description, ds.display_order
            ORDER BY ds.display_order;
            """,
            "status-distribution.txt",
        )

        ctx.logger.log_success("📈 Status distribution analysis completed")

    @staticmethod
    def _analyze_flakes(ctx: CrystalForgeTestContext) -> None:
        """Analyze flakes table"""
        flakes_output = ctx.logger.database_query(
            ctx.server,
            "crystal_forge",
            "SELECT id, name, repo_url FROM flakes;",
            "flakes-table.txt",
        )
        ctx.logger.assert_in_output(
            "crystal-forge", flakes_output, "Crystal Forge flake in flakes table"
        )
        ctx.logger.assert_in_output(
            "http://gitserver", flakes_output, "Git server URL in flakes table"
        )

        flakes_count = ctx.server.succeed(
            """psql -U crystal_forge -d crystal_forge -c 'SELECT COUNT(*) FROM flakes;' -t"""
        ).strip()
        ctx.logger.log_info(f"Total flakes: {flakes_count}")

    @staticmethod
    def _analyze_commits(ctx: CrystalForgeTestContext) -> None:
        """Analyze commits table"""
        commits_output = ctx.logger.database_query(
            ctx.server,
            "crystal_forge",
            "SELECT c.id, f.name as flake_name, c.git_commit_hash, c.commit_timestamp FROM commits c JOIN flakes f ON c.flake_id = f.id WHERE f.name = 'crystal-forge' LIMIT 5;",
            "commits-table.txt",
        )
        ctx.logger.assert_in_output(
            "crystal-forge", commits_output, "Commits linked to crystal-forge flake"
        )

        commits_count = ctx.server.succeed(
            """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM commits c JOIN flakes f ON c.flake_id = f.id WHERE f.name = 'crystal-forge';" -t"""
        ).strip()
        ctx.logger.log_info(f"Commits for crystal-forge flake: {commits_count}")

    @staticmethod
    def _analyze_derivations(ctx: CrystalForgeTestContext) -> None:
        """Analyze derivations table"""
        derivations_output = ctx.logger.database_query(
            ctx.server,
            "crystal_forge",
            "SELECT d.derivation_name, d.derivation_type, d.derivation_target, f.name as flake_name FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = 'crystal-forge' AND d.derivation_type = 'nixos' LIMIT 10;",
            "nixos-derivations.txt",
        )

        # Count various types of derivations
        total_derivations = ctx.server.succeed(
            """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = 'crystal-forge';" -t"""
        ).strip()
        ctx.logger.log_info(
            f"Total derivations for crystal-forge flake: {total_derivations}"
        )

        nixos_derivations = ctx.server.succeed(
            """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = 'crystal-forge' AND d.derivation_type = 'nixos';" -t"""
        ).strip()
        ctx.logger.log_info(
            f"NixOS derivations for crystal-forge flake: {nixos_derivations}"
        )

        package_derivations = ctx.server.succeed(
            """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = 'crystal-forge' AND d.derivation_type = 'package';" -t"""
        ).strip()
        ctx.logger.log_info(
            f"Package derivations for crystal-forge flake: {package_derivations}"
        )

    @staticmethod
    def _analyze_system_states(ctx: CrystalForgeTestContext) -> None:
        """Analyze system states"""
        from .test_patterns import TestPatterns

        TestPatterns.database_verification(
            ctx.logger,
            ctx.server,
            "crystal_forge",
            {
                "hostname": ctx.system_info["hostname"],
                "change_reason": "startup",
            },
        )
