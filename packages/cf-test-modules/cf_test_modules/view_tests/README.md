# View Tests Module

Database view tests for Crystal Forge. Each view gets its own test file with comprehensive SQL testing and error logging.

## Structure

```
view_tests/
├── __init__.py                         # Imports all view test classes
├── sql/                               # External SQL files for all tests
│   ├── critical_systems_*.sql        # SQL files for critical systems view tests
│   ├── deployment_status_*.sql       # SQL files for deployment status view tests
│   ├── fleet_health_*.sql            # SQL files for fleet health view tests
│   ├── systems_status_*.sql          # SQL files for systems status table tests
│   └── (view_name_test_name.sql)     # Pattern: {view_prefix}_{test_name}.sql
├── critical_systems_view_tests.py    # Tests for view_critical_systems
├── deployment_status_view_tests.py   # Tests for view_deployment_status
├── fleet_health_status_view_tests.py # Tests for view_fleet_health_status
├── systems_status_table_tests.py     # Tests for view_systems_status_table
└── (add more view test files here)
```

## Test Architecture

### Core Principles

1. **External SQL Files**: All SQL logic is stored in separate `.sql` files in the `sql/` directory
2. **Comprehensive Error Logging**: SQL is logged on both execution failures and assertion failures
3. **Transactional Testing**: All data manipulation tests use `BEGIN...ROLLBACK` for isolation
4. **Consistent Patterns**: All test classes follow the same structure and helper methods

### Required Helper Methods

Every test class must implement these helper methods:

```python
@staticmethod
def _get_sql_path(filename: str) -> Path:
    """Get the path to a SQL file in the sql directory"""
    current_dir = Path(__file__).parent
    return current_dir / f"sql/{filename}.sql"

@staticmethod
def _load_sql(filename: str) -> str:
    """Load SQL content from a file"""
    # Implementation with error handling

@staticmethod
def _execute_sql_with_logging(ctx: CrystalForgeTestContext, sql: str, test_name: str) -> str:
    """Execute SQL and log it if there's a failure"""
    # Logs SQL on execution failures (database errors, syntax issues)

@staticmethod
def _log_sql_on_failure(ctx: CrystalForgeTestContext, sql: str, test_name: str, reason: str) -> None:
    """Log SQL when test fails due to unexpected results"""
    # Logs SQL on assertion failures (wrong results, missing data)
```

## Adding New View Tests

### Step 1: Create Test Class

Create a new test file: `my_view_tests.py`

```python
"""
Tests for the view_my_view view
"""

import os
from pathlib import Path
from ..test_context import CrystalForgeTestContext

class MyViewTests:
    """Test suite for view_my_view"""

    @staticmethod
    def _get_sql_path(filename: str) -> Path:
        """Get the path to a SQL file in the sql directory"""
        current_dir = Path(__file__).parent
        return current_dir / f"sql/{filename}.sql"

    @staticmethod
    def _load_sql(filename: str) -> str:
        """Load SQL content from a file"""
        sql_path = MyViewTests._get_sql_path(filename)
        try:
            with open(sql_path, 'r', encoding='utf-8') as f:
                return f.read().strip()
        except FileNotFoundError:
            raise FileNotFoundError(f"SQL file not found: {sql_path}")
        except Exception as e:
            raise RuntimeError(f"Error loading SQL file {sql_path}: {e}")

    @staticmethod
    def _execute_sql_with_logging(ctx: CrystalForgeTestContext, sql: str, test_name: str) -> str:
        """Execute SQL and log it if there's a failure"""
        try:
            return ctx.server.succeed(
                f'sudo -u postgres psql crystal_forge -t -c "{sql}"'
            )
        except Exception as e:
            ctx.logger.log_error(f"❌ {test_name} - SQL execution failed")
            ctx.logger.log_error(f"SQL that failed:")
            ctx.logger.log_error("-" * 50)
            for i, line in enumerate(sql.split('\n'), 1):
                ctx.logger.log_error(f"{i:3}: {line}")
            ctx.logger.log_error("-" * 50)
            raise e

    @staticmethod
    def _log_sql_on_failure(ctx: CrystalForgeTestContext, sql: str, test_name: str, reason: str) -> None:
        """Log SQL when test fails due to unexpected results"""
        ctx.logger.log_error(f"❌ {test_name} - {reason}")
        ctx.logger.log_error(f"SQL that produced unexpected results:")
        ctx.logger.log_error("-" * 50)
        for i, line in enumerate(sql.split('\n'), 1):
            ctx.logger.log_error(f"{i:3}: {line}")
        ctx.logger.log_error("-" * 50)

    @staticmethod
    def run_all_tests(ctx: CrystalForgeTestContext) -> None:
        """Run all tests for my view"""
        ctx.logger.log_section("📋 Testing view_my_view")

        # Test 1: Verify view exists and is queryable
        if not MyViewTests._test_view_exists(ctx):
            ctx.logger.log_warning("View does not exist - skipping remaining tests")
            return

        # Test 2-N: Add your specific tests here
        MyViewTests._test_basic_functionality(ctx)
        MyViewTests._test_edge_cases(ctx)
        MyViewTests._test_view_performance(ctx)

        # Cleanup
        MyViewTests.cleanup_test_data(ctx)

    @staticmethod
    def _test_view_exists(ctx: CrystalForgeTestContext) -> bool:
        """Test that the view exists and can be queried"""
        ctx.logger.log_info("Testing view existence...")

        try:
            view_exists_sql = MyViewTests._load_sql("my_view_view_exists")
            view_check_result = MyViewTests._execute_sql_with_logging(
                ctx, view_exists_sql, "View existence check"
            ).strip()

            if view_check_result == "t":
                ctx.logger.log_success("view_my_view exists")
                return True
            else:
                ctx.logger.log_warning("view_my_view does not exist")
                return False
        except Exception as e:
            ctx.logger.log_error(f"Error checking view existence: {e}")
            return False

    # Add your other test methods here...

    @staticmethod
    def cleanup_test_data(ctx: CrystalForgeTestContext) -> None:
        """Clean up any test data that might have been left behind"""
        ctx.logger.log_info("Cleaning up view test data...")

        try:
            cleanup_sql = MyViewTests._load_sql("my_view_cleanup")
            MyViewTests._execute_sql_with_logging(
                ctx, cleanup_sql, "Cleanup test data"
            )
            ctx.logger.log_success("View test data cleanup completed")
        except Exception as e:
            ctx.logger.log_warning(f"Could not clean up test data: {e}")
```

### Step 2: Create SQL Files

Create SQL files in the `sql/` directory using the naming pattern:
`{view_prefix}_{test_name}.sql`

**Required SQL files:**

- `my_view_view_exists.sql` - View existence check
- `my_view_cleanup.sql` - Test data cleanup

**Example files:**

```bash
touch sql/my_view_view_exists.sql
touch sql/my_view_basic_functionality.sql
touch sql/my_view_edge_cases.sql
touch sql/my_view_view_performance.sql
touch sql/my_view_cleanup.sql
```

### Step 3: Register Test Class

Add to `__init__.py`:

```python
from .my_view_tests import MyViewTests
__all__ = [..., "MyViewTests"]
```

Add to main `database_tests.py`:

```python
MyViewTests.run_all_tests(ctx)
```

## Test Requirements

### Mandatory Tests

Every view test class **must** include:

1. **View Existence Test** (`_test_view_exists`)

   - Check if view exists in information_schema
   - Perform basic COUNT(\*) query
   - Return boolean to control test execution flow

2. **Performance Test** (`_test_view_performance`)

   - Run EXPLAIN ANALYZE on the view
   - Capture timing information
   - Save results to log files

3. **Cleanup Method** (`cleanup_test_data`)
   - Remove all test data using test-\* hostname patterns
   - Use proper DELETE order (heartbeats → derivations → commits → systems → flakes → system_states)
   - Handle cleanup failures gracefully

### Recommended Tests

**Basic Functionality:**

- View structure validation (expected columns)
- Basic aggregation/calculation correctness
- Data type validation

**Business Logic:**

- Status calculation logic
- Time-based calculations
- Sorting/ordering requirements

**Edge Cases:**

- Boundary conditions (exact time thresholds)
- NULL value handling
- Empty result sets

**Data Integration:**

- Multi-table join correctness
- Latest record selection logic
- Cross-reference validation

## SQL File Standards

### Naming Convention

`{view_prefix}_{test_name}.sql`

**View Prefixes:**

- `critical_systems_` for view_critical_systems
- `deployment_status_` for view_deployment_status
- `fleet_health_` for view_fleet_health_status
- `systems_status_` for view_systems_status_table
- `{your_view}_` for new views (use underscores, keep concise)

### SQL File Content Requirements

1. **Transactional Structure** (for data manipulation):

```sql
BEGIN;

-- Test data setup
INSERT INTO ...;

-- Query under test
SELECT ... FROM view_name WHERE ...;

ROLLBACK;
```

2. **Test Data Conventions**:

   - Use `test-*` hostnames for easy cleanup
   - Use descriptive test data that clearly shows what's being tested
   - Include comments explaining the test scenario

3. **View Existence Check**:

```sql
SELECT EXISTS (
    SELECT 1
    FROM information_schema.views
    WHERE table_name = 'view_name'
);
```

4. **Performance Testing**:

```sql
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT * FROM view_name;
```

### Error Handling

SQL files should be designed to:

- Work with empty databases (use INSERT ... ON CONFLICT DO NOTHING where appropriate)
- Handle missing reference data gracefully
- Produce predictable test results

## Testing Best Practices

### Data Isolation

- Always use `BEGIN...ROLLBACK` for data manipulation tests
- Use unique test identifiers to avoid conflicts
- Clean up any data that escapes transaction boundaries

### Assertion Patterns

```python
# Good: Log SQL on assertion failure
if actual_result != expected_result:
    TestClass._log_sql_on_failure(
        ctx, sql_query, "Test name",
        f"Expected {expected_result}, got {actual_result}"
    )
    ctx.logger.log_error("❌ Test FAILED")
else:
    ctx.logger.log_success("✅ Test PASSED")
```

### Error Recovery

- Tests should be defensive and handle missing dependencies
- Skip gracefully when prerequisites aren't met
- Provide helpful error messages for debugging

### Performance Considerations

- Keep test data minimal but realistic
- Use transactions to avoid impacting production data
- Capture performance metrics for regression detection

## SQL Logging

The framework automatically logs SQL in two scenarios:

### Execution Failures

When SQL fails to execute (syntax errors, database errors):

```
❌ Test name - SQL execution failed
SQL that failed:
--------------------------------------------------
  1: BEGIN;
  2:
  3: INSERT INTO system_states (hostname, ...) VALUES
  4: ('test-system', ...);
  ...
--------------------------------------------------
```

### Assertion Failures

When SQL executes but returns unexpected results:

```
❌ Test name - Expected 5 systems, got 3
SQL that produced unexpected results:
--------------------------------------------------
  1: SELECT COUNT(*) FROM view_critical_systems
  2: WHERE status = 'Critical';
--------------------------------------------------
```

This dual logging approach makes debugging much faster since you can see exactly what SQL was executed when problems occur.

## Notes

- Tests run during database setup phase
- May be skipped if Crystal Forge server hasn't started yet
- All test data should use `test-*` hostnames for easy cleanup
- SQL files make tests more maintainable and easier to debug
- Comprehensive logging helps with production issue diagnosis
