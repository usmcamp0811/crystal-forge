import os

import pytest


def pytest_configure(config):
    """Register custom pytest marks to avoid warnings"""
    config.addinivalue_line("markers", "s3cache: S3 cache integration tests")
    config.addinivalue_line("markers", "vm_only: Tests that only run in VM environment")
    config.addinivalue_line("markers", "slow: Slow running tests")
    config.addinivalue_line("markers", "harness: Test harness tests")
    config.addinivalue_line("markers", "commits: Commit tracking tests")
    config.addinivalue_line("markers", "dry_run: Dry run build tests")
    config.addinivalue_line("markers", "driver: Driver tests")
    config.addinivalue_line("markers", "vm_internal: VM internal tests")
    config.addinivalue_line("markers", "attic_cache: Attic cache tests")
    config.addinivalue_line("markers", "builder: Builder tests")
    config.addinivalue_line("markers", "integration: Integration tests")
    config.addinivalue_line("markers", "build_pipeline: Build pipeline tests")


@pytest.fixture(scope="session", autouse=True)
def vm_test_setup():
    """Automatically set up VM test environment for all tests"""

    # Mark as NixOS test driver environment
    os.environ["NIXOS_TEST_DRIVER"] = "1"

    # Get machine references
    import cf_test

    machines = cf_test._driver_machines

    if not machines:
        pytest.skip("No VM machines available - not running in NixOS test environment")

    # Start all machines
    for machine_name, machine in machines.items():
        machine.start()

    # Wait for core services based on available machines
    if "s3Cache" in machines:
        machines["s3Cache"].wait_for_unit("minio.service")
        machines["s3Cache"].wait_for_unit("minio-setup.service")
        machines["s3Cache"].wait_for_open_port(9000)

    if "cfServer" in machines:
        machines["cfServer"].wait_for_unit("postgresql.service")
        if (
            machines["cfServer"]
            .succeed("systemctl list-unit-files | grep crystal-forge-builder || true")
            .strip()
        ):
            machines["cfServer"].wait_for_unit("crystal-forge-builder.service")
        machines["cfServer"].wait_for_open_port(5432)
        machines["cfServer"].forward_port(5433, 5432)

    if "gitserver" in machines:
        from cf_test.vm_helpers import wait_for_git_server_ready

        wait_for_git_server_ready(machines["gitserver"], timeout=60)

    # Set up standard test environment variables
    test_env = {
        "CF_TEST_GIT_SERVER_URL": "http://gitserver/crystal-forge",
        "CF_TEST_DB_HOST": "127.0.0.1",
        "CF_TEST_DB_PORT": "5433",
        "CF_TEST_DB_USER": "postgres",
        "CF_TEST_DB_PASSWORD": "",
        "CF_TEST_SERVER_HOST": "127.0.0.1",
    }

    # Add environment-specific variables if they exist
    env_vars = [
        "CF_TEST_PACKAGE_DRV",
        "CF_TEST_PACKAGE_NAME",
        "CF_TEST_PACKAGE_VERSION",
        "CF_TEST_SERVER_PORT",
        "CF_TEST_DRV",
    ]

    for var in env_vars:
        if var in os.environ:
            test_env[var] = os.environ[var]

    os.environ.update(test_env)

    # Basic connectivity verification
    if "cfServer" in machines and "s3Cache" in machines:
        machines["cfServer"].succeed("ping -c 1 s3Cache")
        machines["cfServer"].succeed("curl -f http://s3Cache:9000/minio/health/live")

    yield machines

    # Cleanup handled by NixOS test framework


@pytest.fixture(scope="session")
def cfServer():
    """Get Crystal Forge server machine"""
    import cf_test

    return cf_test._driver_machines["cfServer"]


@pytest.fixture(scope="session")
def s3Cache():
    """Get S3 cache machine"""
    import cf_test

    return cf_test._driver_machines["s3Cache"]


@pytest.fixture(scope="session")
def gitserver():
    """Get git server machine"""
    import cf_test

    return cf_test._driver_machines["gitserver"]


@pytest.fixture(scope="session")
def cf_client():
    """Get Crystal Forge test client"""
    from cf_test import CFTestClient

    # Create a simple config object with the environment variables
    class CFConfig:
        def __init__(self):
            self.db_host = os.environ.get("CF_TEST_DB_HOST", "127.0.0.1")
            self.db_port = int(os.environ.get("CF_TEST_DB_PORT", "5433"))
            self.db_user = os.environ.get("CF_TEST_DB_USER", "postgres")
            self.db_password = os.environ.get("CF_TEST_DB_PASSWORD", "")
            self.db_name = os.environ.get("CF_TEST_DB_NAME", "crystal_forge")
            self.server_host = os.environ.get("CF_TEST_SERVER_HOST", "127.0.0.1")
            self.server_port = int(os.environ.get("CF_TEST_SERVER_PORT", "3000"))

    return CFTestClient(CFConfig())
