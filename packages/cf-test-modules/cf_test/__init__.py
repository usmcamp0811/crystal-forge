#!/usr/bin/env python3
"""
Crystal Forge Test Package - Simple pytest-based testing
"""
import json
import os
import shlex
import subprocess
import tempfile
import time
from contextlib import contextmanager
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional, Union

import psycopg2
import pytest
from psycopg2.extras import RealDictCursor


@dataclass
class CFTestConfig:
    """Configuration for Crystal Forge tests"""

    # Database connection
    db_host: str = field(default_factory=lambda: os.getenv("DB_HOST", "127.0.0.1"))
    db_port: int = field(default_factory=lambda: int(os.getenv("DB_PORT", "5432")))
    db_name: str = field(default_factory=lambda: os.getenv("DB_NAME", "crystal_forge"))
    db_user: str = field(default_factory=lambda: os.getenv("DB_USER", "crystal_forge"))
    db_password: str = field(
        default_factory=lambda: os.getenv("DB_PASSWORD", "password")
    )

    # Server connection
    server_host: str = field(
        default_factory=lambda: os.getenv("CF_SERVER_HOST", "127.0.0.1")
    )
    server_port: int = field(
        default_factory=lambda: int(os.getenv("CF_SERVER_PORT", "3000"))
    )

    # Test environment
    is_nixos_test: bool = field(
        default_factory=lambda: os.getenv("NIX_BUILD_TOP") is not None
    )
    output_dir: Path = field(default_factory=lambda: Path("/tmp/cf-test-outputs"))

    def __post_init__(self):
        self.output_dir = Path(self.output_dir)
        self.output_dir.mkdir(parents=True, exist_ok=True)


class CFTestClient:
    """Simple Crystal Forge test client"""

    def __init__(self, config: Optional[CFTestConfig] = None):
        self.config = config or CFTestConfig()
        self._conn = None

    @contextmanager
    def db_connection(self):
        if self._conn is None:
            conn_params = {
                "host": self.config.db_host,
                "port": self.config.db_port,
                "database": self.config.db_name,
                "user": self.config.db_user,
                "password": self.config.db_password,
                "cursor_factory": RealDictCursor,
            }

            # In NixOS test mode, connect via forwarded port to VM
            if os.getenv("NIXOS_TEST_DRIVER") == "1":
                # Use forwarded connection to VM postgres
                conn_params["host"] = "127.0.0.1"  # driver host
                conn_params["port"] = int(
                    os.getenv("CF_TEST_DB_PORT", "5432")
                )  # forwarded port
                conn_params["user"] = "postgres"
                conn_params["password"] = ""  # VM postgres has no password

            self._conn = psycopg2.connect(**conn_params)
        try:
            yield self._conn
        finally:
            if self._conn:
                self._conn.close()
                self._conn = None

    def execute_sql(
        self, sql: str, params: Optional[tuple] = None
    ) -> List[Dict[str, Any]]:
        """Execute SQL and return results as list of dicts; always commit the statement."""
        with self.db_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(sql, params)
                rows = [dict(row) for row in cur.fetchall()] if cur.description else []
                conn.commit()
                return rows

    # VM Testing Helpers
    def wait_until_succeeds(
        self, machine, cmd: str, timeout: int = 120, interval: float = 1.0
    ) -> str:
        """Wait for a command to succeed on a VM machine"""
        end = time.time() + timeout
        last = ""
        while time.time() < end:
            code, out = machine.execute(cmd)
            last = out
            if code == 0:
                return out
            time.sleep(interval)
        raise AssertionError(f"Timed out after {timeout}s: {cmd}\nLast output:\n{last}")

    def db_query_on_vm(
        self,
        machine,
        sql: str,
        timeout: int = 60,
        db_name: str = "crystal_forge",
        db_user: str = "crystal_forge",
    ) -> str:
        """Execute SQL query on a VM via psql command using temporary file"""
        temp_sql_path = f"/tmp/query_{os.getpid()}_{int(time.time())}.sql"

        # Write the SQL to a file on the VM
        machine.succeed(f"cat > {temp_sql_path} << 'EOF'\n{sql}\nEOF")

        try:
            # Execute the SQL file
            result = self.wait_until_succeeds(
                machine,
                f"sudo -u {db_user} psql -d {db_name} -At -f {temp_sql_path}",
                timeout=timeout,
            )
            return result
        finally:
            # Clean up the temporary file
            machine.succeed(f"rm -f {temp_sql_path}")

    def db_query_on_vm_simple(
        self,
        machine,
        sql: str,
        timeout: int = 60,
        db_name: str = "crystal_forge",
        db_user: str = "crystal_forge",
    ) -> str:
        """Execute simple SQL query on a VM via psql command (for basic queries without special characters)"""
        sql_escaped = sql.replace("'", "''")
        cmd = f"sudo -u {db_user} psql -d {db_name} -At -c $'{sql_escaped}'"
        return self.wait_until_succeeds(machine, cmd, timeout=timeout)

    def wait_for_service_log(
        self, machine, service_name: str, log_pattern: str, timeout: int = 120
    ) -> None:
        """Wait for a specific pattern to appear in service logs"""
        self.wait_until_succeeds(
            machine,
            f"journalctl -u {service_name} | grep '{log_pattern}'",
            timeout=timeout,
        )

    def send_webhook(self, machine, port: int, payload: dict) -> str:
        """Send webhook payload to server"""
        import json

        payload_str = json.dumps(payload).replace('"', '\\"')
        return machine.succeed(
            f"curl -s -X POST http://localhost:{port}/webhook "
            f"-H 'Content-Type: application/json' -d \"{payload_str}\""
        )

    def execute_sql_file(self, sql_file: Union[str, Path]) -> List[Dict[str, Any]]:
        """Execute SQL from file"""
        sql_path = Path(sql_file)
        if not sql_path.exists():
            # Try relative to test file
            import inspect

            caller_frame = inspect.currentframe().f_back
            caller_file = Path(caller_frame.f_globals["__file__"])
            sql_path = caller_file.parent / sql_file

        if not sql_path.exists():
            raise FileNotFoundError(f"SQL file not found: {sql_file}")

        sql = sql_path.read_text()
        return self.execute_sql(sql)

    def setup_test_data(
        self, data: Dict[str, List[Dict[str, Any]]]
    ) -> Dict[str, List[int]]:
        """Setup test data in database tables"""
        inserted_ids = {}

        with self.db_connection() as conn:
            with conn.cursor() as cur:
                for table, rows in data.items():
                    table_ids = []
                    for row in rows:
                        columns = ", ".join(row.keys())
                        placeholders = ", ".join(["%s"] * len(row))
                        sql = f"INSERT INTO {table} ({columns}) VALUES ({placeholders}) RETURNING id"

                        cur.execute(sql, list(row.values()))
                        table_ids.append(cur.fetchone()["id"])

                    inserted_ids[table] = table_ids

                conn.commit()

        return inserted_ids

    def cleanup_test_data(self, patterns: Dict[str, List[str]]):
        """Cleanup test data by patterns in correct order for foreign keys"""
        with self.db_connection() as conn:
            with conn.cursor() as cur:
                # Define the correct deletion order based on foreign key dependencies
                deletion_order = [
                    "agent_heartbeats",
                    "system_states",
                    "derivations",
                    "systems",
                    "commits",
                    "flakes",
                ]

                for table in deletion_order:
                    if table in patterns:
                        for pattern in patterns[table]:
                            try:
                                if "WHERE" in pattern.upper():
                                    sql = f"DELETE FROM {table} {pattern}"
                                else:
                                    sql = f"DELETE FROM {table} WHERE {pattern}"
                                cur.execute(sql)
                            except Exception as e:
                                print(
                                    f"Warning: Failed to delete from {table} with pattern '{pattern}': {e}"
                                )
                conn.commit()

    def run_agent_command(self, hostname: str, **kwargs) -> subprocess.CompletedProcess:
        """Run Crystal Forge test agent"""
        cmd = [
            (
                "/run/current-system/sw/bin/test-agent"
                if self.config.is_nixos_test
                else "cf-test-agent"
            ),
            "--hostname",
            hostname,
            "--server-host",
            self.config.server_host,
            "--server-port",
            str(self.config.server_port),
        ]

        for key, value in kwargs.items():
            cmd.extend([f"--{key.replace('_', '-')}", str(value)])

        return subprocess.run(cmd, capture_output=True, text=True, check=True)

    def wait_for_service(self, port: int, timeout: int = 30):
        """Wait for service to be available"""
        import socket

        for _ in range(timeout):
            try:
                sock = socket.create_connection(
                    (self.config.server_host, port), timeout=1
                )
                sock.close()
                return
            except:
                time.sleep(1)

        raise TimeoutError(f"Service on port {port} not available after {timeout}s")

    def save_artifact(self, content: str, filename: str, description: str = ""):
        """Save test artifact"""
        artifact_path = self.config.output_dir / filename
        artifact_path.write_text(content)

        if description:
            print(f"📄 Saved {description}: {artifact_path}")

        return artifact_path


# Pytest fixtures
@pytest.fixture(scope="session")
def cf_config():
    """Crystal Forge test configuration"""
    return CFTestConfig()


@pytest.fixture(scope="session")
def cf_client(cf_config):
    """Crystal Forge test client"""
    return CFTestClient(cf_config)


@pytest.fixture
def db_transaction(cf_client):
    """Database transaction fixture - rolls back after test"""
    with cf_client.db_connection() as conn:
        with conn.cursor() as cur:
            cur.execute("BEGIN")
            yield cf_client
            cur.execute("ROLLBACK")


# Test markers
pytest.mark.database = pytest.mark.database
pytest.mark.integration = pytest.mark.integration
pytest.mark.agent = pytest.mark.agent
pytest.mark.views = pytest.mark.views
pytest.mark.smoke = pytest.mark.smoke


# Helper functions for common assertions
def assert_view_has_data(
    cf_client: CFTestClient, view_name: str, expected_count: int = None
):
    """Assert view has data"""
    rows = cf_client.execute_sql(f"SELECT COUNT(*) as count FROM {view_name}")
    count = rows[0]["count"]

    if expected_count is not None:
        assert (
            count == expected_count
        ), f"Expected {expected_count} rows in {view_name}, got {count}"
    else:
        assert count > 0, f"Expected data in {view_name}, but it's empty"


def assert_view_columns(
    cf_client: CFTestClient, view_name: str, expected_columns: List[str]
):
    """Assert view has expected columns"""
    sql = """
        SELECT column_name 
        FROM information_schema.columns 
        WHERE table_name = %s
        ORDER BY ordinal_position
    """
    rows = cf_client.execute_sql(sql, (view_name,))
    actual_columns = [row["column_name"] for row in rows]

    missing = set(expected_columns) - set(actual_columns)
    assert not missing, f"View {view_name} missing columns: {missing}"


def assert_deployment_status(
    cf_client: CFTestClient, hostname: str, expected_status: str
):
    """Assert specific deployment status"""
    sql = """
        SELECT deployment_status 
        FROM view_deployment_status 
        WHERE hostname = %s
    """
    rows = cf_client.execute_sql(sql, (hostname,))
    assert rows, f"No deployment status found for {hostname}"

    actual_status = rows[0]["deployment_status"]
    assert (
        actual_status == expected_status
    ), f"Expected {expected_status}, got {actual_status}"


# Test result collection for NixOS
def pytest_sessionfinish(session, exitstatus):
    """Copy test results to NixOS test output"""
    config = CFTestConfig()

    if config.is_nixos_test and config.output_dir.exists():
        nix_out = os.getenv("out", "/tmp/test-results")
        nix_out_path = Path(nix_out)
        nix_out_path.mkdir(parents=True, exist_ok=True)

        import shutil

        shutil.copytree(
            config.output_dir, nix_out_path / "cf-test-results", dirs_exist_ok=True
        )
        print(f"📦 Test results copied to {nix_out_path}")


def main(argv=None):
    """Console entrypoint for `cf-test`."""
    import sys

    import pytest

    cfg = CFTestConfig()
    args = [
        "--tb=short",
        "--maxfail=5",
        "-v",
        f"--junit-xml={cfg.output_dir}/junit.xml",
    ]
    if argv is None:
        argv = sys.argv[1:]
    args.extend(argv)
    return pytest.main(args)


if __name__ == "__main__":
    raise SystemExit(main())
