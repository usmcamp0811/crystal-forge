# View Tests Module

Database view tests for Crystal Forge. Each view gets its own test file.

## Structure

```
view_tests/
├── __init__.py                      # Imports all view test classes
├── systems_status_table_tests.py   # Tests for view_systems_status_table
└── (add more view test files here)
```

## Adding New View Tests

1. **Create new test file**: `my_view_tests.py`
2. **Follow the pattern**:
   ```python
   class MyViewTests:
       @staticmethod
       def run_all_tests(ctx: CrystalForgeTestContext) -> None:
           # Your tests here
   ```
3. **Add to `__init__.py`**:
   ```python
   from .my_view_tests import MyViewTests
   __all__ = [..., "MyViewTests"]
   ```
4. **Add to main `database_tests.py`**:
   ```python
   MyViewTests.run_all_tests(ctx)
   ```

## Test Pattern

Each test class should:

- Have a `run_all_tests(ctx)` static method
- Check if prerequisites exist (database/view)
- Gracefully skip if not ready
- Clean up test data
- Use transactions with ROLLBACK for data tests

## Notes

- Tests run during database setup phase
- May be skipped if Crystal Forge server hasn't started yet
- All test data should use `test-*` hostnames for easy cleanup
