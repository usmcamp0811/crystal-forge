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
  namespace = "crystal-forge";
  db_port = 3042;
  db_password = "password";
  cf_port = 3445;
  pgweb_port = 12084;

  envExports = ''
    export CRYSTAL_FORGE__DATABASE__HOST=127.0.0.1
    export CRYSTAL_FORGE__DATABASE__PORT=${toString db_port}
    export CRYSTAL_FORGE__DATABASE__USER=crystal_forge
    export CRYSTAL_FORGE__DATABASE__PASSWORD=${db_password}
    export CRYSTAL_FORGE__DATABASE__NAME=crystal_forge
    export DATABASE_URL=postgres://crystal_forge:${db_password}@127.0.0.1:${toString db_port}/crystal_forge
    export CRYSTAL_FORGE__FLAKES__WATCHED__dotfiles=git+https://gitlab.com/usmcamp0811/dotfiles
    export CRYSTAL_FORGE__SERVER__HOST=0.0.0.0
    export CRYSTAL_FORGE__SERVER__PORT=${toString cf_port}
    export CRYSTAL_FORGE__CLIENT__SERVER_HOST=127.0.0.1
    export CRYSTAL_FORGE__CLIENT__SERVER_PORT=${toString cf_port}
  '';

  simulatePush = pkgs.writeShellApplication {
    name = "simulate-push";
    runtimeInputs = with pkgs; [git curl jq];
    text = ''
      set -euo pipefail

      REPO_URL="''${1:-https://gitlab.com/usmcamp0811/dotfiles}"
      SERVER_URL="''${2:-http://localhost:${toString cf_port}/webhook}"

      if [[ -z "$REPO_URL" ]]; then
        echo "Usage: simulate-push <repo-url> [server-url]"
        exit 1
      fi

      TMP_DIR="$(mktemp -d)"
      trap 'rm -rf "$TMP_DIR"' EXIT

      git clone --quiet --depth=1 "$REPO_URL" "$TMP_DIR"
      cd "$TMP_DIR"
      COMMIT_HASH="$(git rev-parse HEAD)"
      REPO_URL_WITH_PREFIX="git+''${REPO_URL}"

      PAYLOAD="$(jq -n \
        --arg url "$REPO_URL_WITH_PREFIX" \
        --arg sha "$COMMIT_HASH" \
        '{ project: { web_url: $url }, checkout_sha: $sha }')"

      echo "=== PAYLOAD ==="
      echo "$PAYLOAD" | jq

      curl -v -X POST "$SERVER_URL" \
        -H "Content-Type: application/json" \
        -d "$PAYLOAD"
    '';
  };

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
          inherit namespace;
          command = runServer;
          depends_on."db".condition = "process_healthy";
          readiness_probe = {
            exec.command = "${pkgs.postgresql}/bin/pg_isready -h 127.0.0.1 -p ${toString db_port} -U crystal_forge -d crystal_forge";
            initial_delay_seconds = 2;
            period_seconds = 5;
            timeout_seconds = 3;
            success_threshold = 1;
            failure_threshold = 5;
          };
        };
        settings.processes.pgweb = {
          inherit namespace;
          command = "${pkgs.pgweb}/bin/pgweb --listen=${toString pgweb_port} --bind=0.0.0.0";
          depends_on."db".condition = "process_healthy";
          environment.PGWEB_DATABASE_URL = "postgres://crystal_forge:${db_password}@127.0.0.1:${toString db_port}/crystal_forge";
        };
        settings.processes.agent = {
          inherit namespace;
          command = runAgent;
          depends_on."server".condition = "process_healthy";
        };
        services.postgres."db" = {
          inherit namespace;
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
  cf-dev.config.outputs.package
  // {
    inherit runServer runAgent simulatePush envExports;
  }
