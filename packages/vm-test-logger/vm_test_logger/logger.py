import time
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

import pytest


@dataclass
class TestLogger:
    """Main logger class for VM tests"""

    test_name: str
    primary_vm: Any
    start_time: float = field(default_factory=time.time)
    log_files: List[str] = field(default_factory=list)

    def setup_logging(self) -> None:
        """Initialize logging directories and main log file"""
        self.primary_vm.succeed("mkdir -p /tmp/xchg")

        header = f"""🚀 Starting {self.test_name}
{'=' * 60}
Test started at: {time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}
✅ All VMs started successfully
"""
        self._write_to_main_log(header, overwrite=True)
        self.log_files.append("test-results.log")

    def _write_to_main_log(self, message: str, overwrite: bool = False) -> None:
        """Write message to main log file"""
        escaped_msg = message.replace("'", "'\"'\"'")
        operator = ">" if overwrite else ">>"
        self.primary_vm.succeed(
            f"echo '{escaped_msg}' {operator} /tmp/xchg/test-results.log"
        )

    def log(self, message: str, level: str = "INFO") -> None:
        """Log message to both console and VM file"""
        timestamp = time.strftime("%H:%M:%S", time.gmtime())
        formatted_msg = f"[{timestamp}] {level}: {message}"
        print(formatted_msg)
        self._write_to_main_log(formatted_msg)

    def log_section(self, title: str) -> None:
        """Log a section header"""
        section = f"\n{title}"
        self.log(section)

    def log_success(self, message: str) -> None:
        """Log a success message with checkmark"""
        self.log(f"✅ {message}", "SUCCESS")

    def log_info(self, message: str) -> None:
        """Log an info message with bullet"""
        self.log(f"  • {message}", "INFO")

    def log_error(self, message: str) -> None:
        """Log an error message"""
        self.log(f"❌ FAIL: {message}", "ERROR")

    def log_warning(self, message: str) -> None:
        """Log a warning message"""
        self.log(f"⚠️  {message}", "WARNING")

    def capture_service_logs(
        self, vm: Any, service_name: str, filename: Optional[str] = None
    ) -> str:
        """Capture systemd service logs"""
        if not filename:
            filename = f"{service_name.replace('.service', '')}-logs.txt"

        self.log_section(f"📝 Capturing {service_name} logs...")
        vm.succeed(f"echo '{service_name} Service Logs:' > /tmp/xchg/{filename}")
        vm.succeed(
            f"journalctl -u {service_name} --no-pager >> /tmp/xchg/{filename} || true"
        )

        self.log_files.append(filename)
        self.log_success(f"Service logs captured: {filename}")
        return filename

    def capture_command_output(
        self, vm: Any, command: str, filename: str, description: Optional[str] = None
    ) -> str:
        """Capture command output to file"""
        if not description:
            description = f"Command: {command}"

        self.log_section(f"📊 Capturing {description}...")
        vm.succeed(f"echo '{description}' > /tmp/xchg/{filename}")
        vm.succeed(f"echo 'Command: {command}' >> /tmp/xchg/{filename}")
        vm.succeed(f"echo '--- Output ---' >> /tmp/xchg/{filename}")
        vm.succeed(f"{command} >> /tmp/xchg/{filename} || true")

        self.log_files.append(filename)
        self.log_success(f"Command output captured: {filename}")
        return filename

    def wait_for_services(self, vm: Any, services: List[str]) -> None:
        """Wait for multiple services with logging"""
        self.log_section("⏳ Waiting for essential services to start...")

        for service in services:
            self.log_info(f"{service}...")
            vm.wait_for_unit(service)
            self.log_success(f"{service} is ready")

    def verify_files(self, vm: Any, files: Dict[str, str]) -> None:
        """Verify multiple files exist with logging"""
        self.log_section("🔍 Verifying file accessibility...")

        for file_path, description in files.items():
            vm.succeed(f"test -r {file_path}")
            self.log_success(description)

    def test_network_connectivity(
        self, source_vm: Any, target_host: str, port: Optional[int] = None
    ) -> None:
        """Test network connectivity between VMs"""
        self.log_section("🌐 Testing network connectivity...")

        if port:
            source_vm.succeed(f"ss -ltn | grep ':{port}'")
            self.log_success(f"Server listening on port {port}")

        source_vm.succeed(f"ping -c1 {target_host}")
        self.log_success(f"Can reach {target_host}")

    def assert_in_output(self, needle: str, haystack: str, description: str) -> None:
        """Assert that needle is in haystack with proper logging"""
        if needle not in haystack:
            self.log_error(f"{description}: '{needle}' not found")
            pytest.fail(f"{description}: '{needle}' not found in output")
        else:
            self.log_success(f"{description}: '{needle}' found")

    def gather_system_info(self, vm: Any) -> Dict[str, str]:
        """Gather common system information"""
        self.log_section("📋 Gathering system information...")

        info = {}
        info["hostname"] = vm.succeed("hostname -s").strip()
        info["system_hash"] = (
            vm.succeed("readlink /run/current-system").strip().split("-")[-1]
        )
        info["uptime"] = vm.succeed("uptime").strip()

        for key, value in info.items():
            self.log_info(f"{key}: {value}")

        return info

    def database_query(
        self, vm: Any, database: str, query: str, filename: Optional[str] = None
    ) -> str:
        """Execute database query and capture results"""
        if not filename:
            filename = "database-query.txt"

        self.log_section("🗄️  Executing database query...")
        self.capture_command_output(
            vm,
            f"psql -U {database} -d {database} -c '{query}'",
            filename,
            f"Database Query: {query[:50]}...",
        )

        # Also return the output for immediate use
        return vm.succeed(f"psql -U {database} -d {database} -c '{query}'")

    def finalize_test(self) -> None:
        """Finalize test with summary and file copying"""
        duration = time.time() - self.start_time

        summary = f"""
🎉 Test completed successfully at: {time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}
⏱️  Test duration: {duration:.2f} seconds
{'=' * 60}
✅ {self.test_name} PASSED

📁 Generated log files:"""

        self.log(summary)

        for log_file in self.log_files:
            self.log_info(log_file)

        # Copy all log files to host
        for log_file in self.log_files:
            self.primary_vm.copy_from_vm(f"/tmp/xchg/{log_file}")


class TestPatterns:
    """Common test patterns as static methods"""

    @staticmethod
    def standard_service_startup(
        logger: TestLogger, vm: Any, services: List[str]
    ) -> None:
        """Standard service startup pattern"""
        logger.wait_for_services(vm, services)

    @staticmethod
    def key_file_verification(
        logger: TestLogger, vm: Any, files: Dict[str, str]
    ) -> None:
        """Key file verification pattern"""
        logger.verify_files(vm, files)

    @staticmethod
    def network_test(
        logger: TestLogger, source_vm: Any, target_host: str, port: int
    ) -> None:
        """Network connectivity test pattern"""
        logger.test_network_connectivity(source_vm, target_host, port)

    @staticmethod
    def database_verification(
        logger: TestLogger, vm: Any, database: str, expected_data: Dict[str, str]
    ) -> None:
        """Database verification pattern"""
        logger.log_section("✅ Validating expected data in database...")

        output = logger.database_query(vm, database, "SELECT * FROM system_states;")

        for key, value in expected_data.items():
            logger.assert_in_output(value, output, f"{key} verification")
