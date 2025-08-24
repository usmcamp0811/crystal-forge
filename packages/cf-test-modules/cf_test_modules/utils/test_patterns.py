from typing import Any, Dict, List

from ..runtime.test_context import CrystalForgeTestContext


class TestPatterns:
    """Common test patterns as static methods"""

    @staticmethod
    def standard_service_startup(logger: Any, vm: Any, services: List[str]) -> None:
        """Standard service startup pattern"""
        logger.wait_for_services(vm, services)

    @staticmethod
    def key_file_verification(logger: Any, vm: Any, files: Dict[str, str]) -> None:
        """Key file verification pattern"""
        logger.verify_files(vm, files)

    @staticmethod
    def network_test(logger: Any, source_vm: Any, target_host: str, port: int) -> None:
        """Network connectivity test pattern"""
        logger.test_network_connectivity(source_vm, target_host, port)

    @staticmethod
    def database_verification(
        logger: Any, vm: Any, database: str, expected_data: Dict[str, str]
    ) -> None:
        """Database verification pattern"""
        logger.log_section("✅ Validating expected data in database...")
        output = logger.database_query(vm, database, "SELECT * FROM system_states;")
        for key, value in expected_data.items():
            logger.assert_in_output(value, output, f"{key} verification")
