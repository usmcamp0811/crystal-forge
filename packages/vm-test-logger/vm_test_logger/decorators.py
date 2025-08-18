# pkgs/vm-test-logger/vm_test_logger/decorators.py
from functools import wraps
from typing import Any, Callable

from .logger import TestLogger


def with_logging(test_name: str, primary_vm_name: str = "server"):
    """Decorator to automatically set up logging for a test function"""

    def decorator(test_func: Callable) -> Callable:
        @wraps(test_func)
        def wrapper(*args, **kwargs):
            # Extract VMs from globals (they're injected by NixOS test framework)
            import sys

            frame = sys._getframe(1)
            primary_vm = frame.f_globals.get(primary_vm_name)

            if not primary_vm:
                raise ValueError(
                    f"Primary VM '{primary_vm_name}' not found in test globals"
                )

            # Create logger and set it up
            logger = TestLogger(test_name, primary_vm)
            logger.setup_logging()

            # Inject logger into test function kwargs
            kwargs["logger"] = logger

            try:
                result = test_func(*args, **kwargs)
                logger.finalize_test()
                return result
            except Exception as e:
                logger.log_error(f"Test failed with exception: {str(e)}")
                logger.finalize_test()
                raise

        return wrapper

    return decorator
