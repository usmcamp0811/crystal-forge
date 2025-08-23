# View Tests Module

Database view tests for Crystal Forge. Each view has its own test file with SQL-driven scenarios, and all suites inherit common helpers from `BaseViewTests`.

## Layout

```

view\_tests/
├── **init**.py                      # Exports BaseViewTests and all suites
├── base.py                          # Shared helpers (SQL load/exec/log/perf/cleanup)
├── sql/                             # External SQL used by tests
│   ├── critical\_systems\_*.sql
│   ├── deployment\_status\_*.sql
│   ├── fleet\_health\_*.sql
│   ├── systems\_status\_*.sql
│   └── {view\_prefix}\_{test\_name}.sql
├── critical\_systems\_view\_tests.py   # view\_critical\_systems suite
├── deployment\_systems\_view\_tests.py # view\_deployment\_status suite
├── fleet\_health\_status\_tests.py     # view\_fleet\_health\_status suite
└── systems\_status\_table\_tests.py    # view\_systems\_status\_table suite

```

## Base Class

All suites extend `BaseViewTests` for:

- `_get_sql_path(filename)`
- `_load_sql(filename)`
- `_execute_sql_with_logging(ctx, sql, test_name, *, db="crystal_forge", tuple_only=True, field_sep="|")`
- `_log_sql_on_failure(ctx, sql, test_name, reason)`
- `_view_exists(ctx, sql_exists_file, *, db="crystal_forge", truthy="t", test_name=None)`
- `_run_performance_suite(ctx, sql_perf_file, sql_timing_file, *, db="crystal_forge", perf_out="view-performance-analysis.txt", timing_out="view-timing-test.txt", perf_desc="View performance analysis", timing_desc="View timing test")`
- `_cleanup(ctx, sql_cleanup_file, *, db="crystal_forge")`

> Per-suite copies of these methods should be removed; only call the base methods.

## Writing a New Suite

1. Create `my_view_tests.py`:

```python
"""
Tests for the view_my_view view
"""
from ..test_context import CrystalForgeTestContext
from .base import BaseViewTests

class MyViewTests(BaseViewTests):
    @staticmethod
    def run_all_tests(ctx: CrystalForgeTestContext) -> None:
        ctx.logger.log_section("🧪 Testing view_my_view")

        # 1) Existence check (short-circuit on failure)
        if not MyViewTests._view_exists(ctx, "my_view_view_exists"):
            ctx.logger.log_warning("view_my_view not found; skipping")
            return

        # 2) Specific scenario tests (examples)
        MyViewTests._test_basic(ctx)
        MyViewTests._test_edge_cases(ctx)

        # 3) Performance
        MyViewTests._run_performance_suite(
            ctx, "my_view_view_performance", "my_view_view_timing"
        )

        # 4) Cleanup
        MyViewTests._cleanup(ctx, "my_view_cleanup")

    @staticmethod
    def _test_basic(ctx: CrystalForgeTestContext) -> None:
        sql = MyViewTests._load_sql("my_view_basic_functionality")
        out = MyViewTests._execute_sql_with_logging(ctx, sql, "Basic functionality")
        # parse/validate `out`; on mismatch:
        # MyViewTests._log_sql_on_failure(ctx, sql, "Basic functionality", "reason")

    @staticmethod
    def _test_edge_cases(ctx: CrystalForgeTestContext) -> None:
        sql = MyViewTests._load_sql("my_view_edge_cases")
        out = MyViewTests._execute_sql_with_logging(ctx, sql, "Edge cases")
        # parse/validate
```

2. Add SQL files under `sql/`:

Required:

- `my_view_view_exists.sql`
- `my_view_view_performance.sql`
- `my_view_view_timing.sql`
- `my_view_cleanup.sql`

Optional (by your tests):

- `my_view_basic_functionality.sql`
- `my_view_edge_cases.sql`
- etc.

3. Register in `view_tests/__init__.py` and call from your top-level test runner.

## Conventions

- **Isolation:** Use `BEGIN; … ROLLBACK;` in SQL that mutates data.
- **Test data:** Use `test-*` hostnames and descriptive cases.
- **Parsing:** Tests assume `-t -A -F '|'` output; split by pipe and trim.
- **Performance:** Always capture `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` and a timing query.

## Existing Suites

- `CriticalSystemsViewTests` — status logic, hours_ago, ordering, edge cases, perf.
- `DeploymentStatusViewTests` — aggregation, display mappings, ordering, scenarios, perf.
- `FleetHealthStatusViewTests` — aggregation, interval buckets, ordering, filtering, scenarios, perf.
- `SystemsStatusTableTests` — connectivity/update logic scenarios, heartbeat/state interactions, edges, perf.
