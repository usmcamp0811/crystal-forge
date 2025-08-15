import time

import pytest

start_all()

server.succeed("systemctl status crystal-forge-server.service || true")
server.log("=== crystal-forge-server service logs ===")
server.succeed("journalctl -u crystal-forge-server.service --no-pager || true")

server.wait_for_unit("postgresql")
server.wait_for_unit("crystal-forge-server.service")
agent.wait_for_unit("crystal-forge-agent.service")
server.wait_for_unit("multi-user.target")

# keys present
agent.succeed("test -r /etc/agent.key")
agent.succeed("test -r /etc/agent.pub")
server.succeed("test -r /etc/agent.pub")

# server listening
server.succeed("ss -ltn | grep ':3000'")

# basic network
agent.succeed("ping -c1 server")

agent_hostname = agent.succeed("hostname -s").strip()
system_hash = agent.succeed("readlink /run/current-system").strip().split("-")[-1]
change_reason = "startup"

# accepted agent
server.wait_until_succeeds(
    "journalctl -u crystal-forge-server.service | grep '✅ accepted agent'"
)
agent.log("=== agent logs ===")
agent.log(agent.succeed("journalctl -u crystal-forge-agent.service || true"))

# DB initial state
output = server.succeed(
    "psql -U crystal_forge -d crystal_forge -c 'SELECT hostname, derivation_path, change_reason FROM system_states;'"
)
server.log("Final DB state:\n" + output)
assert agent_hostname in output, "hostname not found in DB"
assert change_reason in output, "change_reason not found in DB"
assert system_hash in output, "derivation_path hash not found in DB"
