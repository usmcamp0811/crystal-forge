import os
import time

import pytest

pytestmark = pytest.mark.vm_only

API_PORT = 3000
DB_NAME = "crystal_forge"
DB_USER = "crystal_forge"
WEBHOOK_COMMIT = os.environ.get(
    "CF_TEST_WEBHOOK_COMMIT", "2abc071042b61202f824e7f50b655d00dfd07765"
)


@pytest.fixture(scope="session")
def server():
    import cf_test

    return cf_test._driver_machines["server"]


@pytest.fixture(scope="session")
def agent():
    import cf_test

    return cf_test._driver_machines["agent"]


def _wait_until_succeeds(machine, cmd: str, timeout: int = 120, interval: float = 1.0):
    end = time.time() + timeout
    last = ""
    while time.time() < end:
        code, out = machine.execute(cmd)
        last = out
        if code == 0:
            return out
        time.sleep(interval)
    raise AssertionError(f"Timed out after {timeout}s: {cmd}\nLast output:\n{last}")


def _db(server, sql: str, timeout: int = 60) -> str:
    sql_escaped = sql.replace("'", "''")
    return _wait_until_succeeds(
        server,
        f"sudo -u {DB_USER} psql -d {DB_NAME} -At -c $'{sql_escaped}'",
        timeout=timeout,
    )


@pytest.mark.timeout(180)
def test_boot_and_units(server, agent):
    server.succeed("systemctl status crystal-forge-server.service || true")
    server.log("=== crystal-forge-server service logs ===")
    server.succeed("journalctl -u crystal-forge-server.service --no-pager || true")

    server.wait_for_unit("postgresql")
    server.wait_for_unit("crystal-forge-server.service")
    agent.wait_for_unit("crystal-forge-agent.service")
    server.wait_for_unit("multi-user.target")


@pytest.mark.timeout(60)
def test_keys_and_network(server, agent):
    agent.succeed("test -r /etc/agent.key")
    agent.succeed("test -r /etc/agent.pub")
    server.succeed("test -r /etc/agent.pub")

    # server.wait_for_open_port(API_PORT)
    # server.succeed(f"ss -ltn | grep ':${API_PORT}\\b'")

    agent.succeed("ping -c1 server")


@pytest.mark.timeout(120)
def test_agent_accept_and_db_state(server, agent):
    agent_hostname = agent.succeed("hostname -s").strip()
    system_hash = agent.succeed("readlink /run/current-system").strip().split("-")[-1]
    change_reason = "startup"

    _wait_until_succeeds(
        server,
        "journalctl -u crystal-forge-server.service | grep '✅ accepted agent'",
        timeout=120,
    )

    agent.log("=== agent logs ===")
    agent.log(agent.succeed("journalctl -u crystal-forge-agent.service || true"))

    out = server.succeed(
        "sudo -u postgres psql -d crystal_forge -At "
        "-c 'SELECT hostname, derivation_path, change_reason FROM system_states;'"
    )
    server.log("Final DB state:\n" + out)

    assert agent_hostname in out
    assert change_reason in out
    assert system_hash in out


@pytest.mark.timeout(120)
def test_webhook_and_commit_ingest(server):
    curl_data = (
        "'{"
        '"project":{"web_url":"http://gitserver/crystal-forge"},'
        f'"checkout_sha":"{WEBHOOK_COMMIT}"'
        "}'"
    )
    server.succeed(
        f"curl -s -X POST http://localhost:{API_PORT}/webhook "
        f"-H 'Content-Type: application/json' -d {curl_data}"
    )
    _wait_until_succeeds(
        server,
        f"journalctl -u crystal-forge-server.service | grep {WEBHOOK_COMMIT}",
        timeout=90,
    )

    flake_row = server.succeed(
        "sudo -u postgres psql -d crystal_forge -At -c "
        "\"SELECT repo_url FROM flakes WHERE repo_url = 'http://gitserver/crystal-forge';\""
    )
    assert "http://gitserver/crystal-forge" in flake_row

    commits = server.succeed(
        "sudo -u postgres psql -d crystal_forge -At -c 'SELECT COUNT(*) FROM commits;'"
    ).strip()
    server.log("commits contents:\n" + commits)
    assert "0 rows" not in commits and "0 rows" not in commits.lower()


@pytest.mark.timeout(120)
def test_postgres_jobs_timer_and_idempotency(server, agent):
    active_services = agent.succeed(
        "systemctl list-units --type=service --state=active"
    )
    assert "postgresql" not in active_services

    server.succeed("systemctl list-timers | grep crystal-forge-postgres-jobs")

    server.succeed("systemctl start crystal-forge-postgres-jobs.service")
    server.succeed(
        "journalctl -u crystal-forge-postgres-jobs.service | "
        "grep 'All jobs completed successfully'"
    )

    server.succeed("systemctl start crystal-forge-postgres-jobs.service")
    server.succeed(
        "journalctl -u crystal-forge-postgres-jobs.service | tail -20 | "
        "grep 'All jobs completed successfully'"
    )
