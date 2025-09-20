from __future__ import annotations

import os
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig


def pytest_configure(config: pytest.Config) -> None:
    """Register custom pytest marks to avoid warnings"""
    for mark, desc in [
        ("s3cache", "S3 cache integration tests"),
        ("driver", "requires NixOS driver machine fixture(s)"),
        ("harness", "scenario harness validation"),
        ("slow", "Tests that take a long time"),
        ("smoke", "Quick smoke tests"),
        ("database", "Database-related tests"),
        ("views", "Database view tests"),
        ("integration", "Integration tests"),
        ("agent", "Agent-related tests"),
        ("timeout", "Tests with timeout constraints"),
        ("commits", "Commit tracking tests"),
        ("dry_run", "Dry run build tests"),
        ("commits", "Test Commit tracking and polling"),
        ("server", "Test the Server"),
        ("attic_cache", "Attic cache tests"),
        ("builder", "Builder tests"),
        ("build_pipeline", "Build pipeline tests"),
    ]:
        config.addinivalue_line("markers", f"{mark}: {desc}")


def pytest_collection_modifyitems(
    config: pytest.Config, items: List[pytest.Item]
) -> None:
    """Auto-skip `@pytest.mark.vm_only` tests when not running under the VM driver."""
    in_driver = os.getenv("NIXOS_TEST_DRIVER") == "1"
    for it in items:
        if it.get_closest_marker("vm_only") and not in_driver:
            it.add_marker(pytest.mark.skip(reason="vm_only (needs NixOS driver)"))


@pytest.fixture(scope="session", autouse=True)
def vm_test_setup():
    """Automatically set up VM test environment for all tests"""
    # Mark as NixOS test driver environment
    os.environ["NIXOS_TEST_DRIVER"] = "1"

    # Get machine references
    import cf_test

    machines = cf_test._driver_machines if hasattr(cf_test, "_driver_machines") else {}

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

    if "atticCache" in machines:
        machines["atticCache"].wait_for_unit("atticd.service")
        machines["atticCache"].wait_for_open_port(8080)

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

    if "server" in machines:
        machines["server"].wait_for_unit("postgresql.service")
        machines["server"].wait_for_open_port(5432)
        machines["server"].forward_port(5433, 5432)

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
    if ("cfServer" in machines or "server" in machines) and "s3Cache" in machines:
        server_machine = machines.get("cfServer") or machines.get("server")
        server_machine.succeed("ping -c 1 s3Cache")
        server_machine.succeed("curl -f http://s3Cache:9000/minio/health/live")

    yield machines

    # Cleanup handled by NixOS test framework


def _cfg() -> CFTestConfig:
    """Build a `CFTestConfig` from environment variables."""
    c = CFTestConfig()
    c.db_host = os.getenv("CF_TEST_DB_HOST", c.db_host)
    c.db_port = int(os.getenv("CF_TEST_DB_PORT", str(c.db_port)))
    c.db_name = os.getenv("CF_TEST_DB_NAME", c.db_name)
    c.db_user = os.getenv("CF_TEST_DB_USER", c.db_user)
    c.db_password = os.getenv("CF_TEST_DB_PASSWORD", c.db_password)
    c.server_host = os.getenv("CF_TEST_SERVER_HOST", c.server_host)
    c.server_port = int(os.getenv("CF_TEST_SERVER_PORT", str(c.server_port)))
    return c


@pytest.fixture(scope="session")
def cf_config() -> CFTestConfig:
    """Session-scoped resolved configuration for Crystal Forge tests."""
    return _cfg()


@pytest.fixture(scope="session")
def cf_client(cf_config: CFTestConfig) -> CFTestClient:
    """Session-scoped DB client with a quick readiness probe."""
    c = CFTestClient(cf_config)
    try:
        c.execute_sql("SELECT 1")
    except Exception as e:
        if os.getenv("NIXOS_TEST_DRIVER") == "1":
            pytest.exit(f"DB not reachable in VM: {e}", returncode=1)
        pytest.skip(f"DB not available: {e}")
    return c


@pytest.fixture(scope="session")
def machines() -> Dict[str, Any]:
    """Mapping of machine name -> NixOS test Machine (VM driver object)."""
    try:
        import cf_test

        m = getattr(cf_test, "_driver_machines", None)
        if isinstance(m, dict):
            return m
    except Exception:
        pass
    return {}


# Machine convenience fixtures
@pytest.fixture(scope="session")
def cfServer():
    """Get Crystal Forge server machine"""
    import cf_test

    return cf_test._driver_machines.get("cfServer")


@pytest.fixture(scope="session")
def s3Cache():
    """Get S3 cache machine"""
    import cf_test

    return cf_test._driver_machines.get("s3Cache")


@pytest.fixture(scope="session")
def gitserver():
    """Get git server machine"""
    import cf_test

    return cf_test._driver_machines.get("gitserver")


@pytest.fixture(scope="session")
def server(machines):
    """Convenience selector for the primary server node."""
    return machines.get("server")


@pytest.fixture(scope="session")
def builder(machines):
    """Convenience selector for the builder node."""
    return machines.get("builder")


@pytest.fixture(scope="session")
def agent(machines):
    """Convenience selector for a single agent node."""
    return machines.get("agent1") or machines.get("agent")


@pytest.fixture(scope="session")
def agents(machines) -> List[Any]:
    """Ordered list of all agent Machines (agent1, agent2, …)."""
    return [m for name, m in sorted(machines.items()) if name.startswith("agent")]


@pytest.fixture(scope="function")
def clean_test_data(cf_client: CFTestClient):
    """Broader cleanup to avoid cross-test UNIQUE violations on commits."""
    yield  # Run the test first, then cleanup

    # Clean up in proper foreign key dependency order
    try:
        # Step 1: Delete agent_heartbeats (references system_states)
        cf_client.execute_sql(
            """
            DELETE FROM agent_heartbeats
            WHERE system_state_id IN (
                SELECT id FROM system_states
                WHERE hostname LIKE 'test-%' OR hostname LIKE 'vm-test-%' OR hostname LIKE 'validate-%'
            )
            """
        )

        # Step 2: Delete system_states (references systems via hostname)
        cf_client.execute_sql(
            """
            DELETE FROM system_states 
            WHERE hostname LIKE 'test-%' OR hostname LIKE 'vm-test-%' OR hostname LIKE 'validate-%'
            """
        )

        # Step 3: Delete systems (references flakes)
        cf_client.execute_sql(
            """
            DELETE FROM systems 
            WHERE hostname LIKE 'test-%' OR hostname LIKE 'vm-test-%' OR hostname LIKE 'validate-%'
            """
        )

        # Step 4: Delete derivations (references commits)
        cf_client.execute_sql(
            """
            DELETE FROM derivations 
            WHERE derivation_name LIKE 'test-%' 
               OR derivation_name LIKE 'vm-test-%' 
               OR derivation_name LIKE 'validate-%'
               OR commit_id IN (
                   SELECT c.id FROM commits c
                   JOIN flakes f ON c.flake_id = f.id
                   WHERE f.repo_url LIKE 'https://example.com/%'
                      OR f.repo_url LIKE '%/test.git'
                      OR f.name ILIKE '%test%'
               )
            """
        )

        # Step 5: Delete commits (references flakes)
        cf_client.execute_sql(
            """
            DELETE FROM commits
            WHERE flake_id IN (
                    SELECT id FROM flakes
                    WHERE repo_url LIKE 'https://example.com/%'
                       OR repo_url LIKE '%/test.git'
                       OR name ILIKE '%test%'
                  )
               OR git_commit_hash LIKE 'working123-%'
               OR git_commit_hash LIKE 'broken456-%'
               OR git_commit_hash LIKE 'old-%'
               OR git_commit_hash LIKE 'newer-%'
               OR git_commit_hash LIKE 'timing%'
            """
        )

        # Step 6: Finally delete flakes (no dependencies)
        cf_client.execute_sql(
            """
            DELETE FROM flakes
            WHERE repo_url LIKE 'https://example.com/%'
               OR repo_url LIKE '%/test.git'
               OR name ILIKE '%test%'
            """
        )

    except Exception as e:
        print(f"Warning: Cleanup failed: {e}")
        # Don't fail the test due to cleanup issues


@pytest.fixture(scope="session")
def cf_ports(server):
    """Returns port configuration for CF services."""

    def _to_int(v, default):
        try:
            return int(v)
        except Exception:
            return default

    host_db = _to_int(os.getenv("CF_TEST_DB_PORT"), 5432)
    host_api = _to_int(os.getenv("CF_TEST_SERVER_PORT"), 3000)

    vm_db = _to_int(os.getenv("CF_TEST_VM_DB_PORT", "0"), 0)
    vm_api = _to_int(os.getenv("CF_TEST_VM_SERVER_PORT", "0"), 0)

    # If not provided by the driver, try to read from the VM's env
    if (vm_db == 0 or vm_api == 0) and server is not None:
        out = server.succeed("printenv CF_TEST_DB_PORT || true").strip()
        if out.isdigit():
            vm_db = int(out)
        out = server.succeed("printenv CF_TEST_SERVER_PORT || true").strip()
        if out.isdigit():
            vm_api = int(out)

    # Final fallbacks
    if vm_db == 0:
        vm_db = 5432
    if vm_api == 0:
        vm_api = 3000

    return {"db_vm": vm_db, "api_vm": vm_api, "db_host": host_db, "api_host": host_api}


@pytest.fixture(scope="session")
def wait_listening():
    """wait_listening(machine, port) -> blocks until TCP port is in LISTEN state"""

    def _wait(machine, port: int):
        machine.wait_until_succeeds(
            f"ss -ltn | awk 'BEGIN{{rc=1}} /:{port}\\b/ {{rc=0}} END{{exit rc}}'"
        )

    return _wait


@pytest.fixture(scope="session")
def atticCache():
    """Get Attic cache machine"""
    import cf_test

    return cf_test._driver_machines.get("atticCache")


@pytest.fixture
def completed_derivation_data(cf_client):
    """
    Creates a completed derivation for cache push testing.
    """
    package_drv_path = os.environ.get("CF_TEST_PACKAGE_DRV")
    package_name = os.environ.get("CF_TEST_PACKAGE_NAME", "hello")
    package_version = os.environ.get("CF_TEST_PACKAGE_VERSION", "2.12.1")

    if not package_drv_path:
        pytest.skip("CF_TEST_PACKAGE_DRV environment variable not set")

    # Insert test flake
    flake_result = cf_client.execute_sql(
        """INSERT INTO flakes (name, repo_url)
           VALUES ('test-completed-flake', 'http://test-completed')
           RETURNING id"""
    )
    flake_id = flake_result[0]["id"]

    # Insert test commit
    commit_result = cf_client.execute_sql(
        """INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
           VALUES (%s, 'completed123abc456', NOW())
           RETURNING id""",
        (flake_id,),
    )
    commit_id = commit_result[0]["id"]

    # Insert completed derivation (status_id = 10 for build-complete)
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, derivation_path,
               scheduled_at, completed_at, attempt_count, started_at,
               evaluation_duration_ms, pname, version, status_id
           ) VALUES (
               %s, 'package', %s, %s,
               NOW() - INTERVAL '1 hour', NOW() - INTERVAL '30 minutes', 0,
               NOW() - INTERVAL '35 minutes', 1500,
               %s, %s, 10
           ) RETURNING id""",
        (
            commit_id,
            f"{package_name}-{package_version}",
            package_drv_path,
            package_name,
            package_version,
        ),
    )
    derivation_id = derivation_result[0]["id"]

    # Create cache push job
    hello_store_path = os.environ.get("CF_TEST_PACKAGE_STORE_PATH")
    if not hello_store_path:
        job_row = cf_client.execute_sql(
            """
            INSERT INTO cache_push_jobs (derivation_id, status, cache_destination)
            VALUES (%s, 'pending', 's3://crystal-forge-cache')
            ON CONFLICT (derivation_id) WHERE (status = ANY (ARRAY['pending', 'in_progress'])) DO NOTHING
            RETURNING id
            """,
            (derivation_id,),
        )
    else:
        job_row = cf_client.execute_sql(
            """
            INSERT INTO cache_push_jobs (derivation_id, status, cache_destination, store_path)
            VALUES (%s, 'pending', 's3://crystal-forge-cache', %s)
            ON CONFLICT (derivation_id) WHERE (status = ANY (ARRAY['pending', 'in_progress'])) DO NOTHING
            RETURNING id
            """,
            (derivation_id, hello_store_path),
        )

    cache_push_job_id = job_row[0]["id"] if job_row else None

    test_data = {
        "flake_id": flake_id,
        "commit_id": commit_id,
        "derivation_id": derivation_id,
        "cache_push_job_id": cache_push_job_id,
        "derivation_path": package_drv_path,
        "derivation_name": f"{package_name}-{package_version}",
        "pname": package_name,
        "version": package_version,
        "status_id": 10,
    }

    yield test_data

    # Cleanup
    cf_client.execute_sql(
        "DELETE FROM cache_push_jobs WHERE derivation_id = %s", (derivation_id,)
    )
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))


@pytest.fixture
def failed_derivation_data(cf_client):
    """
    Creates a failed derivation scenario for testing cache push error handling.
    """
    # Insert test flake
    flake_result = cf_client.execute_sql(
        """INSERT INTO flakes (name, repo_url) 
           VALUES ('test-failed-flake', 'http://test-failed') 
           RETURNING id""",
    )
    flake_id = flake_result[0]["id"]

    # Insert test commit
    commit_result = cf_client.execute_sql(
        """INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) 
           VALUES (%s, 'failed123abc456', NOW()) 
           RETURNING id""",
        (flake_id,),
    )
    commit_id = commit_result[0]["id"]

    # Insert failed derivation (status_id = 12 for failed build)
    derivation_result = cf_client.execute_sql(
        """INSERT INTO derivations (
               commit_id, derivation_type, derivation_name, derivation_path,
               scheduled_at, completed_at, attempt_count, started_at,
               evaluation_duration_ms, error_message, pname, version, status_id
           ) VALUES (
               %s, 'package', '/nix/store/l46k596qypwijbp4qnbzz93gn86rbxbf-dbus-1.drv',
               '/nix/store/l46k596qypwijbp4qnbzz93gn86rbxbf-dbus-1.drv',
               NOW() - INTERVAL '1 hour', NOW() - INTERVAL '30 minutes', 0,
               NOW() - INTERVAL '35 minutes', 1795,
               'nix-store --realise failed with exit code: 1',
               'dbus', '1', 12
           ) RETURNING id""",
        (commit_id,),
    )
    derivation_id = derivation_result[0]["id"]

    test_data = {
        "flake_id": flake_id,
        "commit_id": commit_id,
        "derivation_id": derivation_id,
        "derivation_path": "/nix/store/l46k596qypwijbp4qnbzz93gn86rbxbf-dbus-1.drv",
        "error_message": "nix-store --realise failed with exit code: 1",
        "pname": "dbus",
        "version": "1",
        "status_id": 12,
    }

    yield test_data

    # Cleanup
    cf_client.execute_sql("DELETE FROM derivations WHERE id = %s", (derivation_id,))
    cf_client.execute_sql("DELETE FROM commits WHERE id = %s", (commit_id,))
    cf_client.execute_sql("DELETE FROM flakes WHERE id = %s", (flake_id,))
