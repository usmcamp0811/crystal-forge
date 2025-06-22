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
  db_port = 3042;
  db_password = "password";
  cf_port = 3445;

  envExports = ''
    export RUST_LOG=debug
    export CRYSTAL_FORGE__DATABASE__HOST=127.0.0.1
    export CRYSTAL_FORGE__DATABASE__PORT=${toString db_port}
    export CRYSTAL_FORGE__DATABASE__USER=crystal_forge
    export CRYSTAL_FORGE__DATABASE__PASSWORD=${db_password}
    export CRYSTAL_FORGE__DATABASE__NAME=crystal_forge
    export DATABASE_URL=postgres://crystal_forge:${db_password}@127.0.0.1:${toString db_port}/crystal_forge
    export CRYSTAL_FORGE__FLAKES__WATCHED__dotfiles=https://gitlab.com/usmcamp0811/dotfiles
    export CRYSTAL_FORGE__SERVER__HOST=0.0.0.0
    export CRYSTAL_FORGE__SERVER__PORT=${toString cf_port}
    export CRYSTAL_FORGE__CLIENT__SERVER_HOST=127.0.0.1
    export CRYSTAL_FORGE__CLIENT__SERVER_PORT=${toString cf_port}
  '';

  runAgent = pkgs.writeShellApplication {
    name = "run-agent";
    runtimeInputs = [pkgs.nix];
    text = ''
      ${envExports}
      if [[ "''${1:-}" == "--dev" ]]; then
        exec sudo -E nix run .#agent
      else
        exec sudo -E ${pkgs.crystal-forge.agent}/bin/agent
      fi
    '';
  };

  runServer = pkgs.writeShellApplication {
    name = "run-server";
    runtimeInputs = [pkgs.nix];
    text = ''
      ${envExports}
      if [[ "''${1:-}" == "--dev" ]]; then
        exec nix run .#server
      else
        exec ${pkgs.crystal-forge.server}/bin/server
      fi
    '';
  };

  cf-dev = pkgs.process-compose-flake.evalModules {
    modules = [
      inputs.services-flake.processComposeModules.default
      {
        settings.processes.server = {
          command = runServer;
          depends_on."crystal-forge-db".condition = "process_healthy";
          readiness_probe = {
            exec.command = "${pkgs.postgresql}/bin/pg_isready -h 127.0.0.1 -p ${toString db_port} -U crystal_forge -d crystal_forge";
            initial_delay_seconds = 2;
            period_seconds = 5;
            timeout_seconds = 3;
            success_threshold = 1;
            failure_threshold = 5;
          };
        };
        services.postgres."crystal-forge-db" = {
          enable = true;
          listen_addresses = "0.0.0.0";
          port = db_port;
          initialScript.before = ''
            CREATE USER crystal_forge LOGIN;
            CREATE DATABASE crystal_forge OWNER crystal_forge;
            GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
          '';
          initialDatabases = [];
        };
      }
    ];
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
      runServer
      runAgent
      cf-dev.config.outputs.package
    ];

    shellHook = ''
      echo 🔮 Welcome to the Crystal Forge

      export CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      mkdir -p "$CF_KEY_DIR"

      if [ ! -f "$CF_KEY_DIR/agent.key" ]; then
        echo "🔑 Generating dev agent keypair..."
        ${pkgs.crystal-forge.agent.cf-keygen}/bin/cf-keygen -f "$CF_KEY_DIR/agent.key"
      fi

      export CRYSTAL_FORGE__CLIENT__PRIVATE_KEY="$CF_KEY_DIR/agent.key"
      hostname="$(hostname)"
      pubkey="$(cat "$CF_KEY_DIR/agent.pub")"
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__"''${hostname}"="$pubkey"


      ${envExports}

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
