# minimal plugin to make your pytest work in BOTH devshell and NixOS VM tests.
# - Gives you a `machine` fixture (driver Machine in VM tests, None in devshell).
# - Reads DB/API endpoints from env (set by the Nix test driver with port-forwarding).
# - Provides a cleanup fixture.

from __future__ import annotations

import os
from typing import Any, Dict, List, Optional

import pytest

# your lib
from cf_test import CFTestClient, CFTestConfig


def _mk_config_from_env() -> CFTestConfig:
    cfg = CFTestConfig()
    # honor forwarded ports from the driver if present
    cfg.db_host = os.getenv("CF_TEST_DB_HOST", cfg.db_host)
    cfg.db_port = int(os.getenv("CF_TEST_DB_PORT", str(cfg.db_port)))
    cfg.db_name = os.getenv("CF_TEST_DB_NAME", cfg.db_name)
    cfg.db_user = os.getenv("CF_TEST_DB_USER", cfg.db_user)
    cfg.db_password = os.getenv("CF_TEST_DB_PASSWORD", cfg.db_password)
    cfg.server_host = os.getenv("CF_TEST_SERVER_HOST", cfg.server_host)
    cfg.server_port = int(os.getenv("CF_TEST_SERVER_PORT", str(cfg.server_port)))
    return cfg


@pytest.fixture(scope="session")
def cf_config() -> CFTestConfig:
    return _mk_config_from_env()


@pytest.fixture(scope="session")
def cf_client(cf_config: CFTestConfig) -> CFTestClient:
    c = CFTestClient(cf_config)
    # sanity ping; skip entire session if DB isn’t reachable in interactive runs
    try:
        c.execute_sql("SELECT 1")
    except Exception as e:  # pragma: no cover
        if os.getenv("NIXOS_TEST_DRIVER", "") == "1":
            pytest.exit(f"Database connection failed in VM test: {e}", returncode=1)
        pytest.skip(f"DB not available: {e}")
    return c


@pytest.fixture(scope="function")
def clean_test_data(cf_client: CFTestClient):
    yield
    # idempotent cleanup for your test hostnames/commits; extend as needed
    cf_client.execute_sql(
        """
        DELETE FROM agent_heartbeats
        WHERE system_state_id IN (SELECT id FROM system_states WHERE hostname LIKE 'test-%' OR hostname LIKE 'vm-test-%');

        DELETE FROM system_states WHERE hostname LIKE 'test-%' OR hostname LIKE 'vm-test-%';
        DELETE FROM systems       WHERE hostname LIKE 'test-%' OR hostname LIKE 'vm-test-%';
        DELETE FROM derivations   WHERE derivation_name LIKE 'test-%' OR derivation_name LIKE 'vm-test-%';
        DELETE FROM commits       WHERE git_commit_hash LIKE 'working123-%' OR git_commit_hash LIKE 'broken456-%';
        DELETE FROM flakes        WHERE repo_url LIKE '%/failed.git' OR repo_url LIKE '%/test.git';
        """
    )


@pytest.fixture(scope="session")
def machine():
    """
    In a NixOS VM test, the driver injects the Machine object here so tests can call:
      machine.wait_for_unit(...), machine.wait_until_succeeds(...), machine.succeed(...), etc.
    In interactive runs, returns None (mark such tests with @pytest.mark.vm_only).
    """
    try:
        import cf_test  # type: ignore

        m = getattr(cf_test, "_driver_machine", None)
    except Exception:
        m = None
    return m


def pytest_configure(config: pytest.Config):
    # mark to segregate tests that truly need the VM driver
    config.addinivalue_line(
        "markers", "vm_only: requires NixOS test driver machine fixture"
    )


def pytest_collection_modifyitems(config: pytest.Config, items: List[pytest.Item]):
    # auto-skip vm_only tests if we aren’t under the NixOS test driver
    in_driver = os.getenv("NIXOS_TEST_DRIVER", "") == "1"
    for item in items:
        if item.get_closest_marker("vm_only") and not in_driver:
            item.add_marker(
                pytest.mark.skip(reason="vm_only (needs NixOS test driver)")
            )
