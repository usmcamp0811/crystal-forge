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

      # Add test modules to Python path so you can import them
      export PYTHONPATH="${pkgs.crystal-forge.cf-test-modules}/lib/python3.12/site-packages:''${PYTHONPATH:-}"

      alias process-compose='sudo echo && nix run $PROJECT_ROOT#devScripts --'
      alias run-server='nix run $PROJECT_ROOT#devScripts.runServer --'
      alias run-agent='nix run $PROJECT_ROOT#devScripts.runAgent --'
      alias simulate-push='nix run $PROJECT_ROOT#devScripts.simulatePush --'
      alias dbOnly='nix run $PROJECT_ROOT#devScripts.dbOnly --'
      alias test-agent='nix run $PROJECT_ROOT#agent.test-agent --'

      echo "🔮 Welcome to the Crystal Forge Dev Environment"
      echo ""
      echo "🧰 Dev Workflow:"
      echo ""
      echo "  1️⃣  Start core services:"
      echo "      process-compose up"
      echo "      - Launches PostgreSQL and the Crystal Forge server"
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
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__"''${hostname}"="$pubkey"
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
