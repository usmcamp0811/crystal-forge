"""
Enhanced base class for database view tests with both SQL and agent-driven approaches
Includes server-side simulation for flakes, commits, derivations, and builds
"""

import json
import shlex
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Sequence, Union
from uuid import uuid4

from ..test_context import CrystalForgeTestContext
from ..test_exceptions import AssertionFailedException

JSONLike = Union[Dict[str, Any], List[Any]]


class BaseViewTests:
    """Base class for view tests providing SQL execution and full system simulation"""

    # =============================================================================
    #                            SQL FILE OPERATIONS
    # =============================================================================

    @staticmethod
    def _get_sql_path(filename: str) -> Path:
        """Get path to SQL file in sql/ directory"""
        return Path(__file__).parent / f"sql/{filename}.sql"

    @staticmethod
    def _load_sql(filename: str) -> str:
        """Load SQL content from file"""
        path = BaseViewTests._get_sql_path(filename)
        if not path.exists():
            raise FileNotFoundError(f"SQL file not found: {path}")
        return path.read_text(encoding="utf-8").strip()

    # =============================================================================
    #                            SQL EXECUTION
    # =============================================================================

    @staticmethod
    def _execute_sql(
        ctx: CrystalForgeTestContext,
        sql: str,
        test_name: str,
        *,
        db: str = "crystal_forge",
        tuples_only: bool = True,
        field_separator: str = "|",
    ) -> str:
        """Execute SQL query and return raw output"""
        flags = "-t -A" if tuples_only else "-A"
        cmd = f"sudo -u postgres psql {db} {flags} -F '{field_separator}' -c \"{sql}\""

        try:
            return ctx.server.succeed(cmd)
        except Exception as e:
            BaseViewTests._log_sql_failure(
                ctx, sql, test_name, f"SQL execution failed: {e}"
            )
            raise

    @staticmethod
    def _query_rows(
        ctx: CrystalForgeTestContext,
        sql: str,
        test_name: str,
        *,
        db: str = "crystal_forge",
        separator: str = "|",
    ) -> List[List[str]]:
        """Execute SQL and return parsed rows"""
        output = BaseViewTests._execute_sql(
            ctx, sql, test_name, db=db, field_separator=separator
        )
        lines = [line.strip() for line in output.strip().split("\n") if line.strip()]
        return [[cell.strip() for cell in line.split(separator)] for line in lines]

    @staticmethod
    def _query_scalar(
        ctx: CrystalForgeTestContext,
        sql: str,
        test_name: str,
        *,
        db: str = "crystal_forge",
    ) -> str:
        """Execute SQL and return single scalar value"""
        rows = BaseViewTests._query_rows(ctx, sql, test_name, db=db)
        if not rows or not rows[0]:
            BaseViewTests._fail(
                ctx, sql, test_name, "Expected scalar result but got no rows"
            )
        return rows[0][0]

    # =============================================================================
    #                        SERVER-SIDE DATA SIMULATION
    # =============================================================================

    @staticmethod
    def _create_test_flake(
        ctx: CrystalForgeTestContext,
        *,
        name: str,
        repo_url: str,
        test_name: str = "create test flake",
        db: str = "crystal_forge",
    ) -> int:
        """Create a test flake and return its ID"""
        sql = f"""
            INSERT INTO flakes (name, repo_url, created_at, updated_at)
            VALUES ('{name}', '{repo_url}', NOW(), NOW())
            ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
            RETURNING id;
        """
        return int(BaseViewTests._query_scalar(ctx, sql, test_name, db=db))

    @staticmethod
    def _create_test_commit(
        ctx: CrystalForgeTestContext,
        *,
        flake_id: int,
        commit_hash: str,
        test_name: str = "create test commit",
        db: str = "crystal_forge",
    ) -> int:
        """Create a test commit and return its ID"""
        sql = f"""
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES ({flake_id}, '{commit_hash}', NOW(), 0)
            ON CONFLICT (git_commit_hash) DO UPDATE SET commit_timestamp = NOW()
            RETURNING id;
        """
        return int(BaseViewTests._query_scalar(ctx, sql, test_name, db=db))

    @staticmethod
    def _create_test_derivation(
        ctx: CrystalForgeTestContext,
        *,
        derivation_name: str,
        commit_id: Optional[int] = None,
        derivation_type: str = "nixos",
        derivation_path: Optional[str] = None,
        status_id: int = 3,  # dry-run-pending by default
        pname: Optional[str] = None,
        version: Optional[str] = None,
        test_name: str = "create test derivation",
        db: str = "crystal_forge",
    ) -> int:
        """Create a test derivation and return its ID"""

        # Handle NULL values properly in SQL
        commit_id_sql = str(commit_id) if commit_id is not None else "NULL"
        derivation_path_sql = f"'{derivation_path}'" if derivation_path else "NULL"
        pname_sql = f"'{pname}'" if pname else "NULL"
        version_sql = f"'{version}'" if version else "NULL"

        sql = f"""
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, attempt_count, pname, version, scheduled_at
            )
            VALUES (
                {commit_id_sql}, '{derivation_type}', '{derivation_name}', {derivation_path_sql},
                {status_id}, 0, {pname_sql}, {version_sql}, NOW()
            )
            RETURNING id;
        """
        return int(BaseViewTests._query_scalar(ctx, sql, test_name, db=db))

    @staticmethod
    def _update_derivation_status(
        ctx: CrystalForgeTestContext,
        *,
        derivation_id: int,
        status_id: int,
        derivation_path: Optional[str] = None,
        started_at: bool = False,
        completed_at: bool = False,
        error_message: Optional[str] = None,
        test_name: str = "update derivation status",
        db: str = "crystal_forge",
    ) -> None:
        """Update derivation status with optional fields"""

        updates = [f"status_id = {status_id}"]

        if derivation_path:
            updates.append(f"derivation_path = '{derivation_path}'")
        if started_at:
            updates.append("started_at = NOW()")
        if completed_at:
            updates.append("completed_at = NOW()")
        if error_message:
            escaped = error_message.replace("'", "''")
            updates.append(f"error_message = '{escaped}'")

        sql = f"""
            UPDATE derivations 
            SET {', '.join(updates)}
            WHERE id = {derivation_id};
        """
        BaseViewTests._execute_sql(ctx, sql, test_name, db=db)

    @staticmethod
    def _create_complete_build_scenario(
        ctx: CrystalForgeTestContext,
        *,
        flake_name: str,
        repo_url: str,
        commit_hash: str,
        system_name: str,
        build_status: str = "build-complete",  # or "build-failed", "dry-run-complete", etc.
        error_message: Optional[str] = None,
        test_name: str = "create build scenario",
        db: str = "crystal_forge",
    ) -> Dict[str, int]:
        """Create a complete flake->commit->derivation build scenario"""

        # Map status names to IDs (based on your EvaluationStatus enum)
        status_map = {
            "dry-run-pending": 3,
            "dry-run-complete": 5,
            "dry-run-failed": 6,
            "build-pending": 7,
            "build-complete": 10,
            "build-failed": 12,
            "complete": 11,
        }

        status_id = status_map.get(build_status, 5)  # default to dry-run-complete

        # Create flake
        flake_id = BaseViewTests._create_test_flake(
            ctx, name=flake_name, repo_url=repo_url, test_name=f"{test_name} - flake"
        )

        # Create commit
        commit_id = BaseViewTests._create_test_commit(
            ctx,
            flake_id=flake_id,
            commit_hash=commit_hash,
            test_name=f"{test_name} - commit",
        )

        # Create derivation
        derivation_path = (
            f"/nix/store/{commit_hash[:8]}-{system_name}.drv"
            if status_id >= 5
            else None
        )
        derivation_id = BaseViewTests._create_test_derivation(
            ctx,
            commit_id=commit_id,
            derivation_type="nixos",
            derivation_name=system_name,
            derivation_path=derivation_path,
            status_id=status_id,
            test_name=f"{test_name} - derivation",
        )

        # Update with completion details if needed
        if status_id in [5, 6, 10, 11, 12]:  # terminal states
            BaseViewTests._update_derivation_status(
                ctx,
                derivation_id=derivation_id,
                status_id=status_id,
                derivation_path=derivation_path,
                started_at=True,
                completed_at=True,
                error_message=error_message,
                test_name=f"{test_name} - completion",
            )

        return {
            "flake_id": flake_id,
            "commit_id": commit_id,
            "derivation_id": derivation_id,
        }

    # =============================================================================
    #                            TEST AGENT OPERATIONS
    # =============================================================================

    @staticmethod
    def _write_remote_config(
        ctx: CrystalForgeTestContext,
        config: Union[str, JSONLike],
        suffix: str = ".json",
    ) -> str:
        """Write config to remote temp file and return path"""
        if isinstance(config, (dict, list)):
            content = json.dumps(config, indent=2)
        else:
            content = str(config)

        temp_path = ctx.server.succeed(
            f"mktemp --suffix={shlex.quote(suffix)} /tmp/cf-test-XXXXXXXX"
        ).strip()

        # Write content using heredoc to avoid shell escaping issues
        ctx.server.succeed(f"cat > {shlex.quote(temp_path)} << 'EOF'\n{content}\nEOF")
        return temp_path

    @staticmethod
    def _run_test_agent(
        ctx: CrystalForgeTestContext,
        *,
        hostname: str,
        change_reason: str = "startup",
        derivation: str = "/nix/store/test-derivation",
        timestamp: Optional[str] = None,
        server_host: str = "localhost",
        server_port: int = 3000,
        private_key: Optional[str] = None,
        os_version: Optional[str] = None,
        kernel: Optional[str] = None,
        memory_gb: Optional[float] = None,
        cpu_brand: Optional[str] = None,
        cpu_cores: Optional[int] = None,
        test_name: str = "test agent run",
    ) -> None:
        """Run the Crystal Forge test agent with specified parameters"""

        # Build command arguments
        cmd_args = [
            "/run/current-system/sw/bin/test-agent",
            "--hostname",
            hostname,
            "--change-reason",
            change_reason,
            "--derivation",
            derivation,
            "--server-host",
            server_host,
            "--server-port",
            str(server_port),
        ]

        # Add optional parameters
        if timestamp:
            cmd_args.extend(["--timestamp", timestamp])
        if private_key:
            cmd_args.extend(["--private-key", private_key])
        if os_version:
            cmd_args.extend(["--os", os_version])
        if kernel:
            cmd_args.extend(["--kernel", kernel])
        if memory_gb is not None:
            cmd_args.extend(["--memory-gb", str(memory_gb)])
        if cpu_brand:
            cmd_args.extend(["--cpu-brand", cpu_brand])
        if cpu_cores is not None:
            cmd_args.extend(["--cpu-cores", str(cpu_cores)])

        cmd = " ".join(shlex.quote(arg) for arg in cmd_args)

        try:
            ctx.logger.log_info(f"Running test agent: {hostname} ({change_reason})")
            ctx.server.succeed(cmd)
        except Exception as e:
            BaseViewTests._fail(ctx, cmd, test_name, f"Test agent failed: {e}")

    # =============================================================================
    #                            ASSERTION HELPERS
    # =============================================================================

    @staticmethod
    def _assert_rows_count(
        ctx: CrystalForgeTestContext,
        rows: Sequence[Sequence[str]],
        expected_count: int,
        *,
        sql: str,
        test_name: str,
    ) -> None:
        """Assert that we have the expected number of rows"""
        if len(rows) != expected_count:
            BaseViewTests._fail(
                ctx,
                sql,
                test_name,
                f"Expected {expected_count} row(s), got {len(rows)}",
            )

    @staticmethod
    def _assert_column_count(
        ctx: CrystalForgeTestContext,
        row: Sequence[str],
        min_columns: int,
        *,
        sql: str,
        test_name: str,
    ) -> None:
        """Assert that row has at least the expected number of columns"""
        if len(row) < min_columns:
            BaseViewTests._fail(
                ctx,
                sql,
                test_name,
                f"Row must have ≥{min_columns} columns, got {len(row)}: {row}",
            )

    @staticmethod
    def _assert_equal(
        ctx: CrystalForgeTestContext,
        actual: str,
        expected: str,
        *,
        sql: str,
        test_name: str,
        field_name: str = "value",
    ) -> None:
        """Assert that actual equals expected value"""
        if actual != expected:
            BaseViewTests._fail(
                ctx,
                sql,
                test_name,
                f"Expected {field_name} '{expected}', got '{actual}'",
            )

    @staticmethod
    def _assert_in_set(
        ctx: CrystalForgeTestContext,
        found_items: Sequence[str],
        expected_subset: Sequence[str],
        *,
        sql: str,
        test_name: str,
        description: str = "items",
    ) -> None:
        """Assert that all expected items are found in the results"""
        found_set = set(found_items)
        expected_set = set(expected_subset)

        if not expected_set.issubset(found_set):
            missing = expected_set - found_set
            BaseViewTests._fail(
                ctx,
                sql,
                test_name,
                f"Expected {description} missing: {sorted(missing)}. Found: {sorted(found_set)}",
            )

    # =============================================================================
    #                            ERROR HANDLING
    # =============================================================================

    @staticmethod
    def _log_sql_failure(
        ctx: CrystalForgeTestContext, sql: str, test_name: str, reason: str
    ) -> None:
        """Log SQL failure with formatted output"""
        ctx.logger.log_error(f"❌ {test_name} - {reason}")
        ctx.logger.log_error("SQL that failed:")
        ctx.logger.log_error("-" * 60)
        for i, line in enumerate(sql.splitlines(), 1):
            ctx.logger.log_error(f"{i:3}: {line}")
        ctx.logger.log_error("-" * 60)

        if getattr(ctx, "exit_on_failure", False):
            raise AssertionFailedException(test_name, reason, sql)

    @staticmethod
    def _fail(
        ctx: CrystalForgeTestContext,
        sql_or_cmd: str,
        test_name: str,
        reason: str,
    ) -> None:
        """Fail test with detailed error information"""
        BaseViewTests._log_sql_failure(ctx, sql_or_cmd, test_name, reason)

        if getattr(ctx, "exit_on_failure", False):
            raise AssertionFailedException(test_name, reason, sql_or_cmd)

    # =============================================================================
    #                            COMMON VIEW OPERATIONS
    # =============================================================================

    @staticmethod
    def view_exists(
        ctx: CrystalForgeTestContext,
        view_name: str,
        *,
        db: str = "crystal_forge",
    ) -> bool:
        """Check if a database view exists"""
        sql = f"""
            SELECT EXISTS (
                SELECT 1 FROM information_schema.views 
                WHERE table_name = '{view_name}'
            );
        """

        try:
            result = BaseViewTests._query_scalar(
                ctx, sql, f"Check {view_name} exists", db=db
            )
            return result.strip().lower() == "t"
        except Exception:
            return False

    @staticmethod
    def cleanup_test_data(
        ctx: CrystalForgeTestContext,
        hostname_patterns: List[str],
        *,
        flake_patterns: Optional[List[str]] = None,
        commit_patterns: Optional[List[str]] = None,
        db: str = "crystal_forge",
    ) -> None:
        """Clean up test data by hostname and optionally flake/commit patterns"""

        # Clean up agent-side data
        if hostname_patterns:
            hostname_where = " OR ".join(
                f"hostname LIKE '{pattern}'" for pattern in hostname_patterns
            )

            cleanup_sql = f"""
                -- Clean up heartbeats first (foreign key dependency)
                DELETE FROM agent_heartbeats 
                WHERE system_state_id IN (
                    SELECT id FROM system_states WHERE {hostname_where}
                );
                
                -- Clean up system states
                DELETE FROM system_states WHERE {hostname_where};
            """

            try:
                BaseViewTests._execute_sql(
                    ctx, cleanup_sql, "Cleanup agent data", db=db
                )
                ctx.logger.log_info("Agent data cleanup completed")
            except Exception as e:
                ctx.logger.log_warning(f"Could not clean up agent data: {e}")

        # Clean up server-side data
        if flake_patterns:
            flake_where = " OR ".join(
                f"name LIKE '{pattern}' OR repo_url LIKE '{pattern}'"
                for pattern in flake_patterns
            )

            server_cleanup_sql = f"""
                -- Clean up in dependency order: derivations -> commits -> flakes
                DELETE FROM derivations 
                WHERE commit_id IN (
                    SELECT c.id FROM commits c 
                    JOIN flakes f ON c.flake_id = f.id 
                    WHERE {flake_where}
                );
                
                DELETE FROM commits 
                WHERE flake_id IN (
                    SELECT id FROM flakes WHERE {flake_where}
                );
                
                DELETE FROM flakes WHERE {flake_where};
            """

            try:
                BaseViewTests._execute_sql(
                    ctx, server_cleanup_sql, "Cleanup server data", db=db
                )
                ctx.logger.log_info("Server data cleanup completed")
            except Exception as e:
                ctx.logger.log_warning(f"Could not clean up server data: {e}")

    @staticmethod
    def capture_view_performance(
        ctx: CrystalForgeTestContext,
        view_name: str,
        *,
        db: str = "crystal_forge",
    ) -> None:
        """Capture view performance metrics"""
        perf_sql = f"EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT * FROM {view_name};"
        timing_sql = f"\\timing on\nSELECT COUNT(*) FROM {view_name};\nSELECT * FROM {view_name} LIMIT 5;"

        # Save performance analysis
        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u postgres psql {db} -c "{perf_sql}"',
            f"{view_name.replace('view_', '')}-performance.txt",
            f"{view_name} performance analysis",
        )

        # Save timing test
        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u postgres psql {db} -c "{timing_sql}"',
            f"{view_name.replace('view_', '')}-timing.txt",
            f"{view_name} timing test",
        )

    # =============================================================================
    #                            HIGH-LEVEL TEST PATTERNS
    # =============================================================================

    @staticmethod
    def run_agent_scenario_test(
        ctx: CrystalForgeTestContext,
        *,
        test_name: str,
        agent_actions: List[Dict[str, Any]],
        query_sql: str,
        assertion_func: Callable[[List[List[str]]], Optional[str]],
        wait_after_agent: float = 1.0,
        db: str = "crystal_forge",
    ) -> List[List[str]]:
        """
        Run a complete agent scenario test:
        1. Execute agent actions to create data
        2. Query the view
        3. Assert the results

        agent_actions: List of dicts with test-agent parameters
        assertion_func: Function that takes rows and returns None (success) or error string
        """
        ctx.logger.log_info(f"Running agent scenario: {test_name}")

        # Execute agent actions
        for i, action in enumerate(agent_actions):
            action_name = f"{test_name} - action {i+1}"
            BaseViewTests._run_test_agent(ctx, test_name=action_name, **action)

        # Wait for data to be processed
        if wait_after_agent > 0:
            ctx.server.succeed(f"sleep {wait_after_agent}")

        # Query the view
        rows = BaseViewTests._query_rows(ctx, query_sql, f"{test_name} - query", db=db)

        # Run assertion
        error = assertion_func(rows)
        if error:
            BaseViewTests._fail(ctx, query_sql, test_name, error)

        ctx.logger.log_success(f"✅ {test_name} PASSED")
        return rows

    @staticmethod
    def run_hybrid_scenario_test(
        ctx: CrystalForgeTestContext,
        *,
        test_name: str,
        agent_actions: List[Dict[str, Any]],
        query_sql: str,
        assertion_func: Callable[[List[List[str]]], Optional[str]],
        server_setup: Optional[Callable[[], Dict[str, Any]]] = None,
        wait_after_agent: float = 1.0,
        db: str = "crystal_forge",
    ) -> List[List[str]]:
        """
        Run a hybrid scenario test:
        1. Set up server-side data (flakes, commits, derivations)
        2. Execute agent actions (system states, heartbeats)
        3. Query the view and assert results
        """
        ctx.logger.log_info(f"Running hybrid scenario: {test_name}")

        # Set up server-side data if provided
        server_data = {}
        if server_setup:
            try:
                server_data = server_setup()
                ctx.logger.log_info(
                    f"Server setup completed: {list(server_data.keys())}"
                )
            except Exception as e:
                BaseViewTests._fail(
                    ctx, "server setup", test_name, f"Server setup failed: {e}"
                )

        # Execute agent actions (potentially using server_data for derivation paths etc.)
        for i, action in enumerate(agent_actions):
            action_name = f"{test_name} - action {i+1}"
            # Allow actions to reference server_data
            if server_data and "derivation" not in action:
                if "derivation_path" in server_data:
                    action["derivation"] = server_data["derivation_path"]
            BaseViewTests._run_test_agent(ctx, test_name=action_name, **action)

        # Wait for data to be processed
        if wait_after_agent > 0:
            ctx.server.succeed(f"sleep {wait_after_agent}")

        # Query the view
        rows = BaseViewTests._query_rows(ctx, query_sql, f"{test_name} - query", db=db)

        # Run assertion
        error = assertion_func(rows)
        if error:
            BaseViewTests._fail(ctx, query_sql, test_name, error)

        ctx.logger.log_success(f"✅ {test_name} PASSED")
        return rows
