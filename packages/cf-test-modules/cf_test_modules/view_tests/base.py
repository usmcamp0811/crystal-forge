from __future__ import annotations

from pathlib import Path
from typing import Optional

from ..test_context import CrystalForgeTestContext
from ..test_exceptions import AssertionFailedException


class BaseViewTests:
    @staticmethod
    def _get_sql_path(filename: str) -> Path:
        return Path(__file__).parent / f"sql/{filename}.sql"

    @staticmethod
    def _load_sql(filename: str) -> str:
        p = BaseViewTests._get_sql_path(filename)
        return p.read_text(encoding="utf-8").strip()

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
        flags = "-t -A" if tuple_only else ""
        try:
            return ctx.server.succeed(
                f"sudo -u postgres psql {db} {flags} -F '{field_sep}' -c \"{sql}\""
            )
        except Exception as e:
            BaseViewTests._log_sql_on_failure(
                ctx, sql, test_name, "SQL execution failed"
            )
            raise e

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
        out = BaseViewTests._execute_sql_with_logging(
            ctx, sql, test_name, db=db
        ).strip()
        return out == truthy

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
        BaseViewTests._execute_sql_with_logging(ctx, sql, "Cleanup", db=db)
