"""
Crystal Forge VM Test Modules
Modular test components for Crystal Forge integration testing

This package provides a comprehensive testing framework for Crystal Forge
with modular components that can be used independently or together.
"""

from .agent_tests import AgentTests
from .crystal_forge_server_tests import CrystalForgeServerTests
from .database_analyzer import DatabaseAnalyzer
from .database_tests import DatabaseTests
from .flake_processing_tests import FlakeProcessingTests
from .git_server_tests import GitServerTests
from .service_log_collector import ServiceLogCollector
from .system_state_tests import SystemStateTests
from .test_context import CrystalForgeTestContext
from .test_patterns import TestPatterns

# Import the utility functions directly, not a class
from .test_utilities import format_duration, sanitize_hostname

__all__ = [
    "CrystalForgeTestContext",
    "GitServerTests",
    "DatabaseTests",
    "CrystalForgeServerTests",
    "AgentTests",
    "FlakeProcessingTests",
    "SystemStateTests",
    "ServiceLogCollector",
    "DatabaseAnalyzer",
    "TestPatterns",
    # Export the utility functions
    "format_duration",
    "sanitize_hostname",
]

__version__ = "1.0.0"
__author__ = "Matt Camp"
__description__ = "Modular test components for Crystal Forge integration testing"
