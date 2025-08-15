commit_hash = "2abc071042b61202f824e7f50b655d00dfd07765"
curl_data = f"""'{{
  "project": {{
    "web_url": "https://gitlab.com/usmcamp0811/dotfiles"
  }},
  "checkout_sha": "{commit_hash}"
}}'"""

server.succeed(
    f"curl -s -X POST http://localhost:3000/webhook -H 'Content-Type: application/json' -d {curl_data}"
)
server.wait_until_succeeds(
    f"journalctl -u crystal-forge-server.service | grep {commit_hash}"
)

flake_check = server.succeed(
    "psql -U crystal_forge -d crystal_forge -c \"SELECT repo_url FROM flakes WHERE repo_url = 'https://gitlab.com/usmcamp0811/dotfiles';\""
)
assert "https://gitlab.com/usmcamp0811/dotfiles" in flake_check, "flake not found in DB"

commit_list = server.succeed(
    "psql -U crystal_forge -d crystal_forge -c 'SELECT * FROM commits;'"
)
server.log("commits contents:\n" + commit_list)
assert "0 rows" not in commit_list.lower(), "commits is empty"

active_services = agent.succeed("systemctl list-units --type=service --state=active")
assert (
    "postgresql" not in active_services
), "PostgreSQL unexpectedly running on the agent"
