# packages/cf-test-modules/cf_test/conftest.py
"""
Pytest integration layer for Crystal Forge tests that works in BOTH:
  1) A normal/devshell environment (talks to a locally running DB/API), and
  2) A NixOS VM test (pytest runs in the driver while VMs run services).

## How it works

- In NixOS VM tests, your test driver sets:
    - `NIXOS_TEST_DRIVER=1`
    - For DB/API access, it *forwards* server ports to the driver and exports:
        CF_TEST_DB_HOST, CF_TEST_DB_PORT, CF_TEST_DB_NAME,
        CF_TEST_DB_USER, CF_TEST_DB_PASSWORD,
        CF_TEST_SERVER_HOST, CF_TEST_SERVER_PORT
    - It injects the VM Machine objects so tests can drive them:
        `import cf_test; cf_test._driver_machines = { "server": server, "builder": builder, "agent1": agent1, ... }`

- This file reads those environment variables to build a `CFTestConfig`, and
  exposes driver Machine objects through fixtures (`machines`, `server`, `builder`,
  `agent`, `agents`) so tests can call:
    machine.wait_for_unit("…"), machine.succeed("…"), machine.wait_until_succeeds("…")

- In a devshell (non-VM) run, DB/API values default to your local setup, and all
  VM-only fixtures return `None` / empty mappings. Mark such tests with `@pytest.mark.vm_only`.

## Key Fixtures

- `cf_config`  : Resolved Crystal Forge config (based on env).
- `cf_client`  : Thin DB client; session-scoped sanity-checked connection.
- `clean_test_data` : Function-scoped teardown that removes common test artifacts.
- `machines`   : Mapping[str, Machine] in VM runs; `{}` elsewhere.
- `server`/`builder`/`agent`/`agents` : Convenience selectors over `machines`.

## Markers

- `vm_only` : Tests that require the NixOS VM driver (skipped in devshell runs).

This file is intentionally small: the heavy lifting (port-forwarding, machine
injection, artifact export) is handled by your NixOS test driver code.
"""

from __future__ import annotations

import os
from typing import Any, Dict, List

import pytest

from cf_test import CFTestClient, CFTestConfig


def _cfg() -> CFTestConfig:
    """
    Build a `CFTestConfig` from environment variables.

    The following environment variables are recognized (all optional; sensible
    defaults are taken from `CFTestConfig()` when unset):

      - CF_TEST_DB_HOST        : Database host or UNIX socket dir (e.g., "/run/postgresql")
      - CF_TEST_DB_PORT        : Database TCP port (int)
      - CF_TEST_DB_NAME        : Database name
      - CF_TEST_DB_USER        : Database user
      - CF_TEST_DB_PASSWORD    : Database password
      - CF_TEST_SERVER_HOST    : Crystal Forge server host
      - CF_TEST_SERVER_PORT    : Crystal Forge server port (int)

    In NixOS VM tests, the driver typically forwards VM ports to the host and
    exports these to point at 127.0.0.1:<forwarded-port>.
    """
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
    """
    Session-scoped resolved configuration for Crystal Forge tests.

    Returns
    -------
    CFTestConfig
        Config pre-populated from environment variables (see `_cfg()`).
    """
    return _cfg()


@pytest.fixture(scope="session")
def cf_client(cf_config: CFTestConfig) -> CFTestClient:
    """
    Session-scoped DB client with a quick readiness probe.

    Behavior
    --------
    - Executes `SELECT 1` up-front to validate connectivity.
    - In NixOS VM runs (`NIXOS_TEST_DRIVER=1`), a failed probe is *fatal*
      (pytest session exits).
    - In devshell runs, a failed probe *skips* the session (useful while iterating).

    Returns
    -------
    CFTestClient
        Thin wrapper used by tests to execute SQL.
    """
    c = CFTestClient(cf_config)
    try:
        c.execute_sql("SELECT 1")
    except Exception as e:  # pragma: no cover
        if os.getenv("NIXOS_TEST_DRIVER") == "1":
            pytest.exit(f"DB not reachable in VM: {e}", returncode=1)
        pytest.skip(f"DB not available: {e}")
    return c


@pytest.fixture(scope="session")
def machines() -> Dict[str, Any]:
    """
    Mapping of machine name -> NixOS test Machine (VM driver object).

    In NixOS VM tests, the test driver injects the mapping on the `cf_test` module:
        cf_test._driver_machines = { "server": server, "builder": builder, "agent1": agent1, ... }

    This fixture retrieves that mapping to enable multi-node tests.

    Returns
    -------
    dict[str, Machine] | {}
        A dict of driver Machine objects when running under the VM driver, else `{}`.
    """
    try:
        import cf_test  # provided by your package in the driver env

        m = getattr(cf_test, "_driver_machines", None)
        if isinstance(m, dict):
            return m
    except Exception:
        pass
    return {}


@pytest.fixture(scope="session")
def server(machines):
    """
    Convenience selector for the primary server node.

    Returns
    -------
    Machine | None
        The NixOS test Machine named "server" when available, else `None`.
    """
    return machines.get("server")


@pytest.fixture(scope="session")
def builder(machines):
    """
    Convenience selector for the builder node.

    Returns
    -------
    Machine | None
        The NixOS test Machine named "builder" when available, else `None`.
    """
    return machines.get("builder")


@pytest.fixture(scope="session")
def agent(machines):
    """
    Convenience selector for a single agent node.

    Heuristic
    ---------
    - Prefer "agent1" (if present) to keep deterministic across multi-agent setups.
    - Fallback to "agent" if only a single agent is defined.

    Returns
    -------
    Machine | None
        A representative agent Machine, else `None` in non-VM runs.
    """
    return machines.get("agent1") or machines.get("agent")


@pytest.fixture(scope="session")
def agents(machines) -> List[Any]:
    """
    Ordered list of all agent Machines (agent1, agent2, …).

    Sorting by name keeps the order stable for parametrized tests.

    Returns
    -------
    list[Machine]
        May be empty in non-VM runs.
    """
    return [m for name, m in sorted(machines.items()) if name.startswith("agent")]


def pytest_configure(config: pytest.Config) -> None:
    """
    Register custom markers used by this test suite.
    """
    for mark, desc in [
        ("vm_only", "requires NixOS test driver"),
        ("s3_cache", "requires NixOS test driver"),
        ("attic_cache", "requires NixOS test driver"),
        ("commits", "requires NixOS test driver"),
        ("vm_internal", "internal VM-mode checks"),
        ("driver", "requires NixOS driver machine fixture(s)"),
        ("harness", "scenario harness validation"),
        ("slow", "Tests that take a long time"),
        ("smoke", "Quick smoke tests"),
        ("database", "Database-related tests"),
        ("views", "Database view tests"),
        ("integration", "Integration tests"),
        ("agent", "Agent-related tests"),
        ("timeout", "Tests with timeout constraints"),
    ]:
        config.addinivalue_line("markers", f"{mark}: {desc}")


def pytest_collection_modifyitems(
    config: pytest.Config, items: List[pytest.Item]
) -> None:
    """
    Auto-skip `@pytest.mark.vm_only` tests when not running under the VM driver.

    Detection
    ---------
    - Consider we are under the VM driver when `NIXOS_TEST_DRIVER=1`.

    Parameters
    ----------
    config : pytest.Config
        Pytest configuration object.
    items : list[pytest.Item]
        Collected tests to possibly re-mark/skip.
    """
    in_driver = os.getenv("NIXOS_TEST_DRIVER") == "1"
    for it in items:
        if it.get_closest_marker("vm_only") and not in_driver:
            it.add_marker(pytest.mark.skip(reason="vm_only (needs NixOS driver)"))


@pytest.fixture(scope="function")
def clean_test_data(cf_client: CFTestClient):
    """
    Broader cleanup to avoid cross-test UNIQUE violations on commits.
    Removes test data in correct foreign key order.
    """
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
    """
    Returns:
      {
        'db_vm':   <int>,   # DB port inside the VM
        'api_vm':  <int>,   # API port inside the VM
        'db_host': <int>,   # forwarded DB port on the driver/host
        'api_host':<int>,   # forwarded API port on the driver/host
      }
    """

    def _to_int(v, default):
        try:
            return int(v)
        except Exception:
            return default

    host_db = _to_int(os.getenv("CF_TEST_DB_PORT"), 5432)
    host_api = _to_int(os.getenv("CF_TEST_SERVER_PORT"), 3000)

    vm_db = _to_int(os.getenv("CF_TEST_VM_DB_PORT", "0"), 0)
    vm_api = _to_int(os.getenv("CF_TEST_VM_SERVER_PORT", "0"), 0)

    # If not provided by the driver, try to read from the VM’s env
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
