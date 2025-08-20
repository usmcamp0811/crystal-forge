"""
VM Test Logger - Standardized logging for NixOS VM tests
"""

from .decorators import with_logging
from .logger import TestLogger, TestPatterns

__version__ = "1.0.0"
__all__ = ["TestLogger", "TestPatterns", "with_logging"]
