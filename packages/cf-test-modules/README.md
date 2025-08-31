# cf-scenarios (Crystal Forge test data)

Run preset scenarios to populate your database via Nix.

## Help

```bash
nix run .#cf-test-modules.scenarioRunner -- -h
```

## Quick start

```bash
# minimal run (defaults)
nix run .#cf-test-modules.scenarioRunner -- -s up_to_date
```

## Examples

```bash
# specify counts
nix run .#cf-test-modules.scenarioRunner -- -s up_to_date \
  --num-systems 10 --agent-version 1.2.3

# DB overrides (NixOS env) + overdue hosts
DB_HOST=127.0.0.1 DB_PORT=3042 DB_USER=crystal_forge DB_PASSWORD=pass DB_NAME=crystal_forge \
  nix run .#cf-test-modules.scenarioRunner -- -s mixed_commit_lag --num-overdue 2

# generic --param for scenario-specific kwargs
nix run .#cf-test-modules.scenarioRunner -- -s flake_time_series \
  --param days=7 --param repo_url=https://example.com/repo.git --param flake_name=my/flake

# heartbeat lag examples
nix run .#cf-test-modules.scenarioRunner -- -s behind \
  --ok-heartbeat-minutes 10 --overdue-minutes 60 --num-systems 8

nix run .#cf-test-modules.scenarioRunner -- -s flaky_agent \
  --heartbeat-interval-minutes 5 --heartbeat-hours 2

# other common scenarios
nix run .#cf-test-modules.scenarioRunner -- -s latest_with_two_overdue --num-overdue 2
nix run .#cf-test-modules.scenarioRunner -- -s never_seen --base-hostname testhost
nix run .#cf-test-modules.scenarioRunner -- -s agent_restart --stagger-window-minutes 30
nix run .#cf-test-modules.scenarioRunner -- -s rollback --hostname gray
nix run .#cf-test-modules.scenarioRunner -- -s offline --num-systems 5
```
