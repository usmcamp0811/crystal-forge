#!/usr/bin/env python3
"""
Crystal Forge DevShell Test Runner
Mirrors the NixOS testScript structure for running database view tests in devshell
"""

import os
import sys
import time
from pathlib import Path

# Add the cf_test_modules to Python path (for direct execution)
if __name__ == "__main__":
    # When run directly, add parent directory to path
    sys.path.insert(0, str(Path(__file__).parent.parent))

from cf_test_modules.devshell_adapter import create_devshell_server_vm, DevShellLogger
from cf_test_modules import (
    CrystalForgeTestContext,
    DatabaseTests,
)
from cf_test_modules.test_exceptions import AssertionFailedException


def run_database_tests():
    """
    Main test function - mirrors the NixOS testScript structure
    This matches what your actual NixOS test does
    """
    # Create devshell logger (equivalent to TestLogger in NixOS test)
    logger = DevShellLogger()
    
    # Setup logging equivalent
    logger.log_section("🚀 Crystal Forge Database Tests - DevShell Mode")
    
    # Get hostname (equivalent to server.succeed("hostname -s").strip())
    hostname = os.getenv('HOSTNAME', 'devshell-test')
    system_info = {"hostname": hostname}
    
    # Create test context using devshell VMs (equivalent to NixOS VM nodes)
    server_vm = create_devshell_server_vm()  # Get the adapted server VM
    
    ctx = CrystalForgeTestContext(
        gitserver=None,  # Not needed for database tests
        server=server_vm,
        agent=None,      # Not needed for database tests  
        logger=logger,
        system_info=system_info,
        exit_on_failure=True,  # Match NixOS test behavior
    )
    
    def run_phase(name, func, *args, **kwargs):
        """Run a test phase with error handling - matches NixOS testScript pattern"""
        logger.log_section(f"🚀 STARTING: {name}")
        try:
            func(*args, **kwargs)
            logger.log_success(f"✅ COMPLETED: {name}")
        except AssertionFailedException as e:
            logger.log_error(f"❌ ASSERTION FAILED: {name}")
            logger.log_error(f"🔍 {e}")
            raise
        except Exception as e:
            import traceback
            tb = traceback.extract_tb(e.__traceback__)
            if tb:
                last = tb[-1]
                loc = f"{last.filename.split('/')[-1]}::{last.name}() line {last.lineno}"
            else:
                loc = "unknown"
            logger.log_error(f"❌ FAILED: {name}")
            logger.log_error(f"🔍 {loc}")
            logger.log_error(f"🔍 {e}")
            raise
    
    try:
        # These phases match exactly what the NixOS test does
        run_phase("Phase DB.1: Database Setup", DatabaseTests.setup_and_verify, ctx)
        run_phase("Phase DB.2: Database View Tests", DatabaseTests.run_view_tests, ctx)
        logger.log_success("🎉 Database-related tests passed!")
    finally:
        # Equivalent to logger.finalize_test() in NixOS test
        logger.log_section("📋 Test execution completed")


def check_prerequisites():
    """Check that the devshell environment is ready"""
    logger = DevShellLogger()
    
    # Check database connectivity
    db_host = os.getenv('DB_HOST', '127.0.0.1')
    db_port = os.getenv('DB_PORT', '3042')
    
    try:
        import subprocess
        result = subprocess.run(
            ['pg_isready', '-h', db_host, '-p', db_port], 
            capture_output=True, timeout=5
        )
        if result.returncode != 0:
            logger.log_error(f"PostgreSQL not ready on {db_host}:{db_port}")
            logger.log_error("Make sure process-compose is running: nix run .#cf-dev")
            return False
        else:
            logger.log_success(f"PostgreSQL ready on {db_host}:{db_port}")
            
    except (FileNotFoundError, subprocess.TimeoutExpired):
        logger.log_warning("Could not check PostgreSQL status (pg_isready not found)")
        logger.log_warning("Assuming database is running...")
    
    # Check server connectivity (optional)
    server_port = int(os.getenv('CF_SERVER_PORT', '3445'))
    try:
        import socket
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(2)
        result = sock.connect_ex(('127.0.0.1', server_port))
        sock.close()
        if result == 0:
            logger.log_success(f"Crystal Forge server ready on port {server_port}")
        else:
            logger.log_warning(f"Crystal Forge server not ready on port {server_port}")
            logger.log_warning("Some tests may fail")
    except:
        logger.log_warning("Could not check server status")
    
    return True


def main():
    """Main entry point"""
    print("Crystal Forge DevShell Test Runner")
    print("Equivalent to NixOS testScript for database tests")
    print("=" * 60)
    
    # Check prerequisites
    if not check_prerequisites():
        sys.exit(1)
    
    # Run the main test function (equivalent to the NixOS testScript)
    try:
        run_database_tests()
        print("\n🎉 All tests completed successfully!")
        sys.exit(0)
    except Exception as e:
        print(f"\n❌ Tests failed: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
