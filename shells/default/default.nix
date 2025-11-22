{
  mkShell,
  system,
  inputs,
  pkgs,
  lib,
  ...
}:
with lib;
with lib.crystal-forge; let
  # TODO: do something to configure these from here.. for now they are in `packages/devScripts/default.nix`
  # namespace = "crystal-forge";
  # db_port = 3042;
  # db_password = "password";
  # cf_port = 3445;
  # pgweb_port = 12084;
  scenarioRunnerPy = pkgs.writeTextFile {
    name = "scenario-runner.py";
    text = ''
      import sys
      import os
      sys.path.insert(0, os.environ.get("PYTHONPATH", ""))
      from cf_test import CFTestClient, CFTestConfig
      from cf_test.scenarios.cli import _discover_scenarios, _filter_kwargs, _coerce_arg

      cfg = CFTestConfig()
      cfg.db_host = os.getenv("CF_TEST_DB_HOST", "127.0.0.1")
      cfg.db_port = int(os.getenv("CF_TEST_DB_PORT", "5432"))
      cfg.db_name = os.getenv("CF_TEST_DB_NAME", "crystal_forge")
      cfg.db_user = os.getenv("CF_TEST_DB_USER", "postgres")
      cfg.db_password = os.getenv("CF_TEST_DB_PASSWORD", "")

      client = CFTestClient(config=cfg)
      scenarios = _discover_scenarios()
      scenario_name = sys.argv[1]

      if scenario_name not in scenarios:
          print(f"""❌ Unknown scenario: {scenario_name}""")
          print(f"""Available: {", ".join(sorted(scenarios.keys()))}""")
          sys.exit(1)

      kwargs = {}
      for arg in sys.argv[2:]:
          if '=' in arg:
              k, v = arg.split('=', 1)
              kwargs[k.replace('-', '_')] = _coerce_arg(v)

      call_kwargs = _filter_kwargs(scenarios[scenario_name], kwargs)
      print(f"\n📋 Running scenario: {scenario_name}")
      if call_kwargs:
          print(f"   Parameters: {call_kwargs}\n")

      try:
          result = scenarios[scenario_name](client, **call_kwargs)
          print(f"\n✅ Success! Created {len(result.get('hostnames', []))} systems")
          for hn in result.get('hostnames', []):
              print(f"   • {hn}")
      except Exception as e:
          print(f"\n❌ Failed: {e}")
          import traceback
          traceback.print_exc()
          sys.exit(1)
    '';
  };

  run-scenario = pkgs.writeShellApplication {
    name = "run-scenario";
    runtimeInputs = [
      (pkgs.python3.withPackages (
        ps:
          with ps;
            [
              psycopg2
            ]
            ++ [pkgs.crystal-forge.cf-test-suite]
      ))

      pkgs.postgresql
    ];
    text = ''
      set -euo pipefail

      SCENARIO_NAME="''${1:-mixed_commit_lag}"
      shift || true

      # Ensure required environment variables are set with your devshell defaults
      : "''${CF_TEST_DB_HOST:=$DB_HOST}"
      : "''${CF_TEST_DB_PORT:=$DB_PORT}"
      : "''${CF_TEST_DB_USER:=$DB_USER}"
      : "''${CF_TEST_DB_PASSWORD:=$DB_PASSWORD}"
      : "''${CF_TEST_DB_NAME:=$DB_NAME}"

      export CF_TEST_DB_HOST CF_TEST_DB_PORT CF_TEST_DB_USER CF_TEST_DB_PASSWORD CF_TEST_DB_NAME

      # Check database connection
      check_db() {
          local psql_cmd=(psql -h "$CF_TEST_DB_HOST" -p "$CF_TEST_DB_PORT" -U "$CF_TEST_DB_USER" -d "$CF_TEST_DB_NAME" -c "SELECT 1")
          if [[ -n "$CF_TEST_DB_PASSWORD" ]]; then
              PGPASSWORD="$CF_TEST_DB_PASSWORD" "''${psql_cmd[@]}" &>/dev/null
          else
              "''${psql_cmd[@]}" &>/dev/null
          fi
      }

      echo "📍 Connecting to database at $CF_TEST_DB_HOST:$CF_TEST_DB_PORT..."
      for i in {1..30}; do
          if check_db; then
              echo "✓ Database ready"
              break
          fi
          if [[ $i -eq 30 ]]; then
              echo "❌ Could not connect to database after 30 attempts"
              exit 1
          fi
          sleep 1
      done

      python3 ${scenarioRunnerPy} "$SCENARIO_NAME" "$@"
    '';
  };
in
  mkShell {
    buildInputs = with pkgs; [
      rustc
      cargo
      pkg-config
      openssl
      fzf
      postgresql
      sqlx-cli
      vulnix
      python3
      python3Packages.pytest
      run-scenario
      # Add the test modules to the shell
    ];
    shellHook = ''
      export CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      export PROJECT_ROOT="$PWD"
      export DB_HOST="''${DB_HOST:-localhost}"
      export DB_PORT="''${DB_PORT:-3042}"
      export DB_NAME="''${DB_NAME:-crystal_forge}"
      export DB_USER="''${DB_USER:-crystal_forge}"
      export DB_PASSWORD="''${DB_PASSWORD:-password}"
      export DATABASE_URL="postgres://$DB_USER:password@$DB_HOST:$DB_PORT/$DB_NAME"
      export CF_TEST_MODE=devshell

      # Add test modules to Python path so you can import them
      export PYTHONPATH="${pkgs.crystal-forge.cf-test-suite}/lib/python3.12/site-packages:''${PYTHONPATH:-}"

      alias full-stack='sudo echo && nix run $PROJECT_ROOT#devScripts --'
      alias server-stack='nix run $PROJECT_ROOT#devScripts.server-only --'
      alias db-only='nix run $PROJECT_ROOT#devScripts.db-only --'
      alias run-server='nix run $PROJECT_ROOT#devScripts.runServer --'
      alias run-agent='nix run $PROJECT_ROOT#devScripts.runAgent --'
      alias simulate-push='nix run $PROJECT_ROOT#devScripts.simulatePush --'
      alias test-agent='nix run $PROJECT_ROOT#test-agent --'
      alias run-db-test='nix run .#cf-test-suite.runTests --'

      echo "🔮 Welcome to the Crystal Forge Dev Environment"
      echo ""
      echo "🧰 Dev Workflow:"
      echo ""
      echo "  1️⃣  Start core services:"
      echo "      full-stack up"
      echo "      - Launches PostgreSQL, the Crystal Forge server and agent in process-compose"
      echo "      server-stack up"
      echo "      - Launches PostgreSQL and the Crystal Forge server in process-compose"
      echo "      db-only up"
      echo "      - Launches PostgreSQL in process-compose"
      echo ""
      echo "  2️⃣  Run the agent:"
      echo "      run-agent"
      echo "      - Automatically runs with sudo"
      echo "      - Requires the server to be running first"
      echo ""
      echo "  3️⃣  Run agent with local code (for development):"
      echo "      run-agent --dev"
      echo ""
      echo "🛠  Helpful Commands:"
      echo ""
      echo "  run-server         → Run server directly (uses packaged binary unless --dev)"
      echo "  simulate-push      → Simulate a webook push event"
      echo "  sqlx-refresh       → Drop DB and re-run sqlx prepare"
      echo "  sqlx-prepare       → Just re-run sqlx prepare"
      echo "  run-db-test        → Run database tests against dev database (must run `server-stak up`)"
      echo ""
      echo "🧪 Test Suite Available:"
      echo ""
      echo "  python3            → Regular Python with test modules in path"
      echo ""
      echo "🔑 Dev keys in: \$CF_KEY_DIR ($CF_KEY_DIR)"
      echo ""
      echo "💡 Tip: View all env vars with: env | grep CRYSTAL_FORGE"
      mkdir -p "$CF_KEY_DIR"
      if [ ! -f "$CF_KEY_DIR/agent.key" ]; then
        echo "🔑 Generating dev agent keypair..."
        nix run .#agent.cf-keygen -- -f "$CF_KEY_DIR/agent.key"
      fi
      export RUST_LOG=info
      export CRYSTAL_FORGE__CLIENT__PRIVATE_KEY="$CF_KEY_DIR/agent.key"
      hostname="$(hostname)"
      pubkey="$(cat "$CF_KEY_DIR/agent.pub")"
      export "CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__''${hostname}"="$pubkey"
      ${pkgs.crystal-forge.devScripts.envExports}
      sqlx-refresh() {
        echo "🔄 Resetting and preparing sqlx..."
        sqlx database reset -y
        cargo sqlx prepare
      }
      sqlx-prepare() {
        echo "🛠  Running cargo sqlx prepare..."
        cargo sqlx prepare
      }
      if [ -n "$BASH_VERSION" ]; then
        . ${pkgs.fzf}/share/fzf/key-bindings.bash
        . ${pkgs.fzf}/share/fzf/completion.bash
      elif [ -n "$ZSH_VERSION" ]; then
        source ${pkgs.fzf}/share/fzf/key-bindings.zsh
        source ${pkgs.fzf}/share/fzf/completion.zsh
      fi
    '';
  }
