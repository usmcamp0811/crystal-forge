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
  grafana_port = 3446;
  pgweb_port = 12084;
  # Create the dashboard JSON file
  crystalForgeDashboard = pkgs.writeTextFile {
    name = "crystal-forge-dashboard.json";
    text = builtins.toJSON (builtins.fromJSON (builtins.readFile ./dashboards/crystal-forge-dashboard.json));
  };
  tomlFormat = pkgs.formats.toml {};
  # gray = pkgs.writeShellApplication {
  #   name = "test-gray";
  #   text = ''
  #     nix run "$PROJECT_ROOT#testAgents.test-gray.agent"
  #   '';
  # };
  # lucas = pkgs.writeShellApplication {
  #   name = "test-lucas";
  #   text = ''
  #     nix run "$PROJECT_ROOT#testAgents.test-lucas.agent"
  #   '';
  # };
  agent-sim = pkgs.writeShellApplication {
    name = "agent-sim";
    text = ''
      nix run "$PROJECT_ROOT#testAgents.weekly-simulation"
    '';
  };
  generateConfig = pkgs.writeShellApplication {
    name = "generate-config";
    runtimeInputs = with pkgs; [hostname coreutils];
    text = ''
      set -euo pipefail

      CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      ACTUAL_HOSTNAME="$(hostname -s)"
      ACTUAL_PUBKEY="$(cat "$CF_KEY_DIR/agent.pub")"

      CONFIG_DIR="''${XDG_RUNTIME_DIR:-/tmp}/crystal-forge"
      mkdir -p "$CONFIG_DIR"
      CONFIG_FILE="$CONFIG_DIR/crystal-forge-config.toml"

      sed \
        -e "s/HOSTNAME_PLACEHOLDER/$ACTUAL_HOSTNAME/g" \
        -e "s|PUBLIC_KEY_PLACEHOLDER|$ACTUAL_PUBKEY|g" \
        ${configTemplate} > "$CONFIG_FILE"

      echo "$CONFIG_FILE"
    '';
  };
  # Create a template config that will be filled at runtime
  configTemplate = tomlFormat.generate "crystal-forge-config-template.toml" {
    database = {
      host = "127.0.0.1";
      port = db_port;
      user = "crystal_forge";
      password = db_password;
      name = "crystal_forge";
    };
    server = {
      host = "0.0.0.0";
      port = cf_port;
    };
    build = {
      cores = 7;
      max_jobs = 1;
      poll_interval = "1m";
    };
    client = {
      server_host = "127.0.0.1";
      server_port = cf_port;
      private_key = "$CF_KEY_DIR/agent.key";
    };
    environments = [
      # {
      #   name = "devshell";
      #   description = "Development environment for Crystal Forge agents and evaluation";
      #   is_active = true;
      #   risk_profile = "LOW";
      #   compliance_level = "NONE";
      # }
      {
        name = "mockenv";
        description = "An environment full of agents created from shell scripts for testing purposes";
        is_active = true;
        risk_profile = "LOW";
        compliance_level = "NONE";
      }
    ];
    systems = [
      # {
      #   hostname = "HOSTNAME_PLACEHOLDER";
      #   public_key = "PUBLIC_KEY_PLACEHOLDER";
      #   environment = "devshell";
      #   flake_name = "dotfiles";
      # }
      {
        hostname = "test.gray";
        public_key = pkgs.crystal-forge.testAgents.test-gray.publicKey;
        environment = "mockenv";
        flake_name = "dotfiles";
      }
      # {
      #   hostname = "test.lucas";
      #   public_key = pkgs.crystal-forge.testAgents.test-lucas.publicKey;
      #   environment = "mockenv";
      #   flake_name = "dotfiles";
      # }
    ];
    flakes = {
      flake_polling_interval = "10m";
      commit_evaluation_interval = "10m";
      build_processing_interval = "10m";
      watched = [
        {
          name = "dotfiles";
          repo_url = "git+https://gitlab.com/usmcamp0811/dotfiles";
          auto_poll = false;
          initial_commit_depth = 10;
        }
      ];
    };
  };

  envExports = ''
    export CRYSTAL_FORGE_CONFIG="$(${generateConfig}/bin/generate-config)"
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

      PAYLOAD="$(jq -n \
        --arg url "$REPO_URL" \
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
      CRYSTAL_FORGE_CONFIG="$(${generateConfig}/bin/generate-config)"
      export CRYSTAL_FORGE_CONFIG
      if [[ "''${1:-}" == "--dev" ]]; then
        exec sudo -E nix run .#agent
      else
        exec sudo -E ${pkgs.crystal-forge.default.agent}/bin/agent
      fi
    '';
  };

  runServer = pkgs.writeShellApplication {
    name = "run-server";
    runtimeInputs = [pkgs.nix pkgs.git pkgs.vulnix];
    text = ''
      CRYSTAL_FORGE_CONFIG="$(${generateConfig}/bin/generate-config)"
      export CRYSTAL_FORGE_CONFIG
      if [[ "''${1:-}" == "--dev" ]]; then
        exec nix run .#server
      else
        exec ${pkgs.crystal-forge.default.server}/bin/server
      fi
    '';
  };
  db-module = {
    settings.processes.pgweb = {
      inherit namespace;
      command = "${pkgs.pgweb}/bin/pgweb --listen=${toString pgweb_port} --bind=0.0.0.0";
      depends_on."db".condition = "process_healthy";
      environment.PGWEB_DATABASE_URL = "postgres://crystal_forge:${db_password}@127.0.0.1:${toString db_port}/crystal_forge";
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

        CREATE USER root WITH SUPERUSER LOGIN;
        CREATE USER grafana_user LOGIN;
        CREATE DATABASE grafana_db OWNER grafana_user;
        GRANT ALL PRIVILEGES ON DATABASE grafana_db TO grafana_user;
      '';
      initialDatabases = [];
    };
    settings.processes.postgres-jobs = {
      inherit namespace;
      command = ''
        nix run "$PROJECT_ROOT#run-postgres-jobs"
      '';
      depends_on."db".condition = "process_healthy";
      environment = {
        DB_HOST = "127.0.0.1";
        DB_PORT = toString db_port;
        DB_NAME = "crystal_forge";
        DB_USER = "crystal_forge";
        DB_PASSWORD = db_password;
      };
    };
    services.grafana.grafana = {
      enable = true;
      http_port = grafana_port;
      domain = "localhost";
      declarativePlugins = with pkgs.grafanaPlugins; [grafana-piechart-panel];
      providers = [
        {
          name = "default";
          type = "file";
          disableDeletion = false;
          updateIntervalSeconds = 10;
          options = {
            path = pkgs.linkFarm "grafana-dashboards" [
              {
                name = "crystal-forge-dashboard.json";
                path = crystalForgeDashboard;
              }
            ];
          };
        }
      ];
      datasources = [
        {
          name = "CrystalForge DB";
          uid = "crystal-forge-ds";
          type = "postgres";
          access = "proxy";
          url = "localhost:${toString db_port}";
          database = "crystal_forge";
          user = "crystal_forge";
          secureJsonData = {
            password = db_password;
          };
          jsonData = {
            sslmode = "disable";
            maxOpenConns = 100;
            maxIdleConns = 100;
            maxIdleConnsAuto = true;
          };
        }
        {
          name = "Grafana DB";
          uid = "grafana-db";
          type = "postgres";
          access = "proxy";
          url = "localhost:${toString db_port}";
          database = "grafana_db";
          user = "grafana_user";
          secureJsonData = {
            password = db_password;
          };
          jsonData = {
            sslmode = "disable";
            maxOpenConns = 100;
            maxIdleConns = 100;
            maxIdleConnsAuto = true;
          };
        }
      ];
      extraConf."auth.anonymous" = {
        enabled = true;
        org_role = "Editor";
      };
      extraConf.database = with config.services.postgres.grafana-db; {
        type = "postgres";
        host = "localhost:${toString db_port}";
        name = "grafana_db";
      };
    };
    settings.processes."grafana".depends_on."db".condition = "process_healthy";
  };
  agent-module = {
    settings.processes.agent = {
      inherit namespace;
      command = runAgent;
      depends_on."server".condition = "process_healthy";
      disabled = false;
    };
  };
  server-module = {
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
  };

  full-stack = pkgs.process-compose-flake.evalModules {
    modules = [
      inputs.services-flake.processComposeModules.default
      db-module
      server-module
      agent-module
    ];
  };
  server-only = pkgs.process-compose-flake.evalModules {
    modules = [
      inputs.services-flake.processComposeModules.default
      server-module
      db-module
    ];
  };

  dbOnly = pkgs.process-compose-flake.evalModules {
    modules = [
      inputs.services-flake.processComposeModules.default
      db-module
    ];
  };
  # Simple agent with default actions
in
  full-stack.config.outputs.package
  // {
    inherit runServer runAgent simulatePush envExports;
    db-only = dbOnly.config.outputs.package;
    server-only = server-only.config.outputs.package;
  }
