"""
Database view tests module

This module contains tests for all database views in Crystal Forge.
Each view test suite is in its own file for better organization.
"""

from .critical_systems_view_tests import CriticalSystemsViewTests
from .deployment_systems_view_tests import DeploymentStatusViewTests
from .fleet_health_status_tests import FleetHealthStatusViewTests
from .systems_status_table_tests import SystemsStatusTableTests

# Add future view test classes here as you create them
# from .other_view_tests import OtherViewTests

__all__ = [
    "SystemsStatusTableTests",
    "DeploymentStatusViewTests",
    "FleetHealthStatusViewTests",
    "CriticalSystemsViewTests",
    # Add other test classes here
]
