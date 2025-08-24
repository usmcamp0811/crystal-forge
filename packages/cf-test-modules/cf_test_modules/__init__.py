"""
Crystal Forge VM Test Modules
Modular test components for Crystal Forge integration testing

This package provides a comprehensive testing framework for Crystal Forge
with modular components that can be used independently or together.
"""

# Reporting
from .reports.database_analyzer import DatabaseAnalyzer
from .reports.service_log_collector import ServiceLogCollector

# Runtime / Support
from .runtime.test_context import CrystalForgeTestContext

# Tests
from .tests.agent_tests import AgentTests
from .tests.crystal_forge_server_tests import CrystalForgeServerTests
from .tests.database_tests import DatabaseTests
from .tests.flake_processing_tests import FlakeProcessingTests
from .tests.system_state_tests import SystemStateTests

# Utils
from .utils.test_patterns import TestPatterns
from .utils.test_utilities import format_duration, sanitize_hostname

__all__ = [
    "CrystalForgeTestContext",
    "DatabaseTests",
    "CrystalForgeServerTests",
    "AgentTests",
    "FlakeProcessingTests",
    "SystemStateTests",
    "ServiceLogCollector",
    "DatabaseAnalyzer",
    "TestPatterns",
    "format_duration",
    "sanitize_hostname",
]

__version__ = "1.0.0"
__author__ = "Matt Camp"
__description__ = "Modular test components for Crystal Forge integration testing"
