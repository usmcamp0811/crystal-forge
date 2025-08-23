"""
Database view tests module

This module contains tests for all database views in Crystal Forge.
Each view test suite is in its own file for better organization.
"""

from .deployment_systems_view_tests import DeploymentStatusViewTests
from .systems_status_table_tests import SystemsStatusTableTests

# Add future view test classes here as you create them
# from .other_view_tests import OtherViewTests

__all__ = [
    "SystemsStatusTableTests",
    "DeploymentStatusViewTests",
    # Add other test classes here
]
