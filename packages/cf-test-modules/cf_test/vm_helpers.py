"""
VM test helpers - Common patterns for integration tests
"""

import os
from typing import Any, Dict

# Constants for smoke tests
API_PORT = 3000
DB_NAME = "crystal_forge"
DB_USER = "crystal_forge"
DEFAULT_WEBHOOK_COMMIT = "2abc071042b61202f824e7f50b655d00dfd07765"


def get_webhook_commit() -> str:
    """Get webhook commit from environment or default"""
    return os.environ.get("CF_TEST_WEBHOOK_COMMIT", DEFAULT_WEBHOOK_COMMIT)


def build_webhook_payload(project_url: str, commit_sha: str) -> Dict[str, Any]:
    """Build standard webhook payload"""
    return {"project": {"web_url": project_url}, "checkout_sha": commit_sha}


def format_curl_webhook_data(payload: Dict[str, Any]) -> str:
    """Format webhook payload for curl command (shell-escaped)"""
    import json

    return f"'{json.dumps(payload)}'"


def check_service_active(machine, service_name: str) -> bool:
    """Check if a systemd service is active"""
    try:
        machine.succeed(f"systemctl is-active {service_name}")
        return True
    except:
        return False


def get_system_hash(machine) -> str:
    """Get the system hash from /run/current-system"""
    return machine.succeed("readlink /run/current-system").strip().split("-")[-1]


def check_keys_exist(machine, *key_paths: str) -> None:
    """Assert that SSH keys exist and are readable"""
    for path in key_paths:
        machine.succeed(f"test -r {path}")


def wait_for_agent_acceptance(cf_client, machine, timeout: int = 120) -> None:
    """Wait for agent to be accepted by server"""
    cf_client.wait_for_service_log(
        machine, "crystal-forge-server.service", "✅ accepted agent", timeout=timeout
    )


def verify_db_state(
    cf_client,
    machine,
    expected_hostname: str,
    expected_hash: str,
    expected_reason: str = "startup",
) -> None:
    """Verify database contains expected system state"""
    # Use direct DB connection instead of VM shell commands
    rows = cf_client.execute_sql(
        "SELECT hostname, derivation_path, change_reason FROM system_states"
    )

    machine.log(f"Final DB state: {len(rows)} rows found")
    for row in rows:
        machine.log(f"  {row['hostname']}: {row['change_reason']}")

    hostnames = [row["hostname"] for row in rows]
    reasons = [row["change_reason"] for row in rows]
    derivation_paths = [row["derivation_path"] for row in rows]

    assert (
        expected_hostname in hostnames
    ), f"Hostname {expected_hostname} not found in DB. Found: {hostnames}"
    assert (
        expected_reason in reasons
    ), f"Change reason {expected_reason} not found in DB. Found: {reasons}"
    assert any(
        expected_hash in path for path in derivation_paths
    ), f"System hash {expected_hash} not found in any derivation path"


def verify_flake_in_db(cf_client, machine, repo_url: str) -> None:
    """Verify flake was inserted into database"""
    rows = cf_client.execute_sql("SELECT repo_url FROM flakes")
    repo_urls = [row["repo_url"] for row in rows]
    assert (
        repo_url in repo_urls
    ), f"Flake {repo_url} not found in database. Found: {repo_urls}"


def verify_commits_exist(cf_client, machine) -> None:
    """Verify commits table is not empty"""
    rows = cf_client.execute_sql("SELECT COUNT(*) as count FROM commits")
    count = rows[0]["count"]
    machine.log(f"commits count: {count}")
    assert count > 0, "No commits found in database"


def check_timer_active(machine, timer_name: str) -> None:
    """Check that a systemd timer is active"""
    machine.succeed(f"systemctl list-timers | grep {timer_name}")


def run_service_and_verify_success(
    cf_client, machine, service_name: str, success_pattern: str
) -> None:
    """Run a service and verify it completed successfully"""
    machine.succeed(f"systemctl start {service_name}")
    cf_client.wait_for_service_log(machine, service_name, success_pattern)


class SmokeTestConstants:
    """Common constants for smoke tests"""

    API_PORT = API_PORT
    DB_NAME = DB_NAME
    DB_USER = DB_USER

    # Service names
    POSTGRES_SERVICE = "postgresql"
    SERVER_SERVICE = "crystal-forge-server.service"
    AGENT_SERVICE = "crystal-forge-agent.service"
    JOBS_SERVICE = "crystal-forge-postgres-jobs.service"
    JOBS_TIMER = "crystal-forge-postgres-jobs"

    # Common paths
    AGENT_KEY_PATH = "/etc/agent.key"
    AGENT_PUB_PATH = "/etc/agent.pub"
    SERVER_PUB_PATH = "/etc/agent.pub"

    # Timeouts
    BOOT_TIMEOUT = 180
    NETWORK_TIMEOUT = 60
    AGENT_ACCEPTANCE_TIMEOUT = 120
    WEBHOOK_TIMEOUT = 120
    JOBS_TIMEOUT = 120


class SmokeTestData:
    """Container for test data that needs to be shared across test functions"""

    def __init__(self):
        self.webhook_commit = get_webhook_commit()
        self.git_server_url = "http://gitserver/crystal-forge"
        self.webhook_payload = build_webhook_payload(
            self.git_server_url, self.webhook_commit
        )
        self.curl_data = format_curl_webhook_data(self.webhook_payload)
