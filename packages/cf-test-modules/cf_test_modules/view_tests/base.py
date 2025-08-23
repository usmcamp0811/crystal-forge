from __future__ import annotations

from pathlib import Path
from typing import Iterable, List, Optional, Sequence

from ..test_context import CrystalForgeTestContext
from ..test_exceptions import AssertionFailedException


class BaseViewTests:
    # ---------- SQL file ----------
    @staticmethod
    def _get_sql_path(filename: str) -> Path:
        return Path(__file__).parent / f"sql/{filename}.sql"

    @staticmethod
    def _load_sql(filename: str) -> str:
        p = BaseViewTests._get_sql_path(filename)
        return p.read_text(encoding="utf-8").strip()

    # ---------- Exec (new) ----------
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

    # ---------- Back-compat shim ----------
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
    ) -> None:
        if actual != expected:
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
    ) -> None:
        fset = set(found)
        subset = set(expected_subset)
        if not subset.issubset(fset):
            BaseViewTests._fail(
                ctx,
                sql,
                test_name,
                f"Expected set to include {sorted(subset)}, got {sorted(fset)}",
            )

    # ---------- High-level ----------
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
