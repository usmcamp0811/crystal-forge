from __future__ import annotations

import json
import shlex
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Optional, Sequence, Union

from ..test_context import CrystalForgeTestContext
from ..test_exceptions import AssertionFailedException

JSONLike = Union[Dict[str, Any], List[Any]]


class BaseViewTests:
    # ---------- SQL file ----------
    @staticmethod
    def _get_sql_path(filename: str) -> Path:
        return Path(__file__).parent / f"sql/{filename}.sql"

    @staticmethod
    def _load_sql(filename: str) -> str:
        p = BaseViewTests._get_sql_path(filename)
        return p.read_text(encoding="utf-8").strip()

    # ---------- Exec ----------
    @staticmethod
    def _execute_sql(
        ctx: CrystalForgeTestContext,
        sql: str,
        test_name: str,
        *,
        db: str = "crystal_forge",
        tuple_only: bool = True,
        field_sep: str = "|",
    ) -> str:
        flags = "-t -A" if tuple_only else ""
        try:
            return ctx.server.succeed(
                f"sudo -u postgres psql {db} {flags} -F '{field_sep}' -c \"{sql}\""
            )
        except Exception:
            BaseViewTests._log_sql_on_failure(
                ctx, sql, test_name, "SQL execution failed"
            )
            raise

    # Back-compat shim (legacy tests call this)
    @staticmethod
    def _execute_sql_with_logging(
        ctx: CrystalForgeTestContext,
        sql: str,
        test_name: str,
        *,
        db: str = "crystal_forge",
        tuple_only: bool = True,
        field_sep: str = "|",
    ) -> str:
        return BaseViewTests._execute_sql(
            ctx, sql, test_name, db=db, tuple_only=tuple_only, field_sep=field_sep
        )

    # ---------- Parsing helpers ----------
    @staticmethod
    def _query_rows(
        ctx: CrystalForgeTestContext,
        sql: str,
        test_name: str,
        *,
        db: str = "crystal_forge",
        sep: str = "|",
    ) -> List[List[str]]:
        out = BaseViewTests._execute_sql(ctx, sql, test_name, db=db, field_sep=sep)
        lines = [ln.strip() for ln in out.strip().split("\n") if ln.strip()]
        return [[part.strip() for part in ln.split(sep)] for ln in lines]

    @staticmethod
    def _query_scalar(
        ctx: CrystalForgeTestContext,
        sql: str,
        test_name: str,
        *,
        db: str = "crystal_forge",
    ) -> str:
        rows = BaseViewTests._query_rows(ctx, sql, test_name, db=db)
        if not rows or not rows[0]:
            BaseViewTests._fail(
                ctx, sql, test_name, "Expected a scalar result but got no rows"
            )
        return rows[0][0]

    # ---------- Logging / fail ----------
    @staticmethod
    def _log_sql_on_failure(
        ctx: CrystalForgeTestContext, sql: str, test_name: str, reason: str
    ) -> None:
        ctx.logger.log_error(f"❌ {test_name} - {reason}")
        for i, line in enumerate(sql.splitlines(), 1):
            ctx.logger.log_error(f"{i:3}: {line}")
        if getattr(ctx, "exit_on_failure", False):
            raise AssertionFailedException(test_name, reason, sql)

    @staticmethod
    def _fail(
        ctx: CrystalForgeTestContext,
        sql: str,
        test_name: str,
        reason: str,
        *,
        details: Optional[str] = None,
    ) -> None:
        BaseViewTests._log_sql_on_failure(ctx, sql, test_name, reason)
        if details:
            ctx.logger.log_error(details)
        if getattr(ctx, "exit_on_failure", False):
            raise AssertionFailedException(test_name, reason, sql)

    # ---------- Assertions ----------
    @staticmethod
    def _assert_true(
        ctx: CrystalForgeTestContext, cond: bool, *, sql: str, test_name: str, msg: str
    ) -> None:
        if not cond:
            BaseViewTests._fail(ctx, sql, test_name, msg)

    @staticmethod
    def _assert_equal(
        ctx: CrystalForgeTestContext,
        actual: str,
        expected: str,
        *,
        sql: str,
        test_name: str,
        label: str = "value",
        case_sensitive: bool = True,
    ) -> None:
        a = actual if case_sensitive else actual.lower()
        e = expected if case_sensitive else expected.lower()
        if a != e:
            BaseViewTests._fail(
                ctx, sql, test_name, f"Expected {label} '{expected}', got '{actual}'"
            )

    @staticmethod
    def _assert_rows_count(
        ctx: CrystalForgeTestContext,
        rows: Sequence[Sequence[str]],
        n: int,
        *,
        sql: str,
        test_name: str,
    ) -> None:
        if len(rows) != n:
            BaseViewTests._fail(
                ctx, sql, test_name, f"Expected {n} row(s), got {len(rows)}"
            )

    @staticmethod
    def _assert_rowlen_at_least(
        ctx: CrystalForgeTestContext,
        row: Sequence[str],
        n: int,
        *,
        sql: str,
        test_name: str,
    ) -> None:
        if len(row) < n:
            BaseViewTests._fail(
                ctx,
                sql,
                test_name,
                f"Row must have ≥{n} columns, got {len(row)}: {row}",
            )

    @staticmethod
    def _assert_in_set(
        ctx: CrystalForgeTestContext,
        found: Iterable[str],
        expected_subset: Iterable[str],
        *,
        sql: str,
        test_name: str,
        label: str = "set",
    ) -> None:
        fset = set(found)
        subset = set(expected_subset)
        if not subset.issubset(fset):
            BaseViewTests._fail(
                ctx,
                sql,
                test_name,
                f"Expected {label} to include {sorted(subset)}, got {sorted(fset)}",
            )

    # ---------- High-level (SQL path) ----------
    @staticmethod
    def _view_exists(
        ctx: CrystalForgeTestContext,
        sql_exists_file: str,
        *,
        db: str = "crystal_forge",
        truthy: str = "t",
        test_name: Optional[str] = None,
    ) -> bool:
        test_name = test_name or f"{sql_exists_file} existence check"
        sql = BaseViewTests._load_sql(sql_exists_file)
        out = BaseViewTests._query_scalar(ctx, sql, test_name, db=db)
        return out.strip() == truthy

    @staticmethod
    def _run_performance_suite(
        ctx: CrystalForgeTestContext,
        sql_perf_file: str,
        sql_timing_file: str,
        *,
        db: str = "crystal_forge",
        perf_out: str = "view-performance-analysis.txt",
        timing_out: str = "view-timing-test.txt",
        perf_desc: str = "View performance analysis",
        timing_desc: str = "View timing test",
    ) -> None:
        perf_sql = BaseViewTests._load_sql(sql_perf_file)
        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u postgres psql {db} -c "{perf_sql}"',
            perf_out,
            perf_desc,
        )
        timing_sql = BaseViewTests._load_sql(sql_timing_file)
        ctx.logger.capture_command_output(
            ctx.server,
            f'sudo -u postgres psql {db} -c "{timing_sql}"',
            timing_out,
            timing_desc,
        )

    @staticmethod
    def _cleanup(
        ctx: CrystalForgeTestContext,
        sql_cleanup_file: str,
        *,
        db: str = "crystal_forge",
    ) -> None:
        sql = BaseViewTests._load_sql(sql_cleanup_file)
        BaseViewTests._execute_sql(ctx, sql, "Cleanup", db=db)

    # ========================================================================
    #                       Agent-driven test helpers
    # ========================================================================
    @staticmethod
    def _remote_write_tmp(
        ctx: CrystalForgeTestContext, content: str, *, suffix: str = ".json"
    ) -> str:
        """Write content to a remote temp file, return path."""
        path = ctx.server.succeed(
            f"mktemp --suffix={shlex.quote(suffix)} /tmp/cf-test-XXXXXXXX"
        ).strip()
        # use single-quoted EOF to avoid interpolation on remote
        ctx.server.succeed(
            f"cat > {shlex.quote(path)} <<'__CF_EOF__'\n{content}\n__CF_EOF__"
        )
        return path

    @staticmethod
    def _run_test_agent(
        ctx: CrystalForgeTestContext,
        *,
        runner_cmd: str,
        config: Union[str, JSONLike, Path],
        sudo_user: Optional[str] = None,
        extra_args: Optional[List[str]] = None,
        config_arg: Optional[str] = None,
        wait_after_secs: int = 0,
        test_name: str = "Run test agent",
    ) -> None:
        """
        Run the Crystal Forge test agent with a supplied config.
        - runner_cmd: executable, e.g. '/run/current-system/sw/bin/cf-test-agent'
        - config: path|json-serializable|raw string (YAML/JSON) written to /tmp and passed to agent
        - config_arg: if provided, pass like '--config <path>'; if None, appended as last arg
        - sudo_user: run as that user (e.g. 'crystalforge')
        """
        if isinstance(config, (dict, list)):
            config_str = json.dumps(config, ensure_ascii=False, indent=2)
        elif isinstance(config, Path):
            # Read local file and copy to remote tmp
            config_str = Path(config).read_text(encoding="utf-8")
        else:
            config_str = str(config)

        cfg_path = BaseViewTests._remote_write_tmp(ctx, config_str, suffix=".json")
        su = f"sudo -u {sudo_user} " if sudo_user else ""
        args = extra_args or []
        if config_arg:
            args = [*args, config_arg, cfg_path]
        else:
            args = [*args, cfg_path]

        cmd = f"{su}{runner_cmd} {' '.join(map(shlex.quote, args))}"
        try:
            ctx.logger.log_info(f"▶︎ {test_name}: {runner_cmd}")
            ctx.server.succeed(cmd)
            if wait_after_secs > 0:
                ctx.server.succeed(f"sleep {int(wait_after_secs)}")
        except Exception as e:
            BaseViewTests._fail(ctx, config_str, test_name, f"Agent run failed: {e}")

    @staticmethod
    def _agent_scenario(
        ctx: CrystalForgeTestContext,
        *,
        runner_cmd: str,
        agent_config: Union[str, JSONLike, Path],
        query_sql: str,
        test_name: str,
        assert_fn: Optional[Callable[[List[List[str]]], Optional[str]]] = None,
        db: str = "crystal_forge",
        sudo_user: Optional[str] = None,
        config_arg: Optional[str] = "--config",
        extra_args: Optional[List[str]] = None,
        wait_after_secs: int = 0,
    ) -> List[List[str]]:
        """
        End-to-end scenario:
          1) run test agent with config
          2) query the DB view
          3) (optional) custom assertion on rows
        assert_fn: receives parsed rows; return None if OK, else return error string
        """
        BaseViewTests._run_test_agent(
            ctx,
            runner_cmd=runner_cmd,
            config=agent_config,
            sudo_user=sudo_user,
            extra_args=extra_args,
            config_arg=config_arg,
            wait_after_secs=wait_after_secs,
            test_name=f"{test_name} (agent)",
        )
        rows = BaseViewTests._query_rows(ctx, query_sql, f"{test_name} (query)", db=db)
        if assert_fn:
            err = assert_fn(rows)
            if err:
                BaseViewTests._fail(ctx, query_sql, test_name, err)
        return rows
