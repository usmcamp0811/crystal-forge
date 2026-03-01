{ mkShell, system, inputs, pkgs, lib, ... }:
with lib;
with lib.crystal-forge;
let
  namespace = "crystal-forge";
  db_port = 3042;
  db_password = "password";
  cf_port = 3445;
  oidc_port = 38080;
  oidc_realm = "crystal-forge";
  oidc_client_id = "crystal-forge-web";
  oidc_client_secret = "dev-only-secret";
  grafana_port = 3446;
  pgweb_port = 12084;
  oidc_issuer = "http://127.0.0.1:${toString oidc_port}/realms/${oidc_realm}";
  oidc_realm_import = ./oidc/realm-crystal-forge.json;
  tomlFormat = pkgs.formats.toml { };

  agent-sim = pkgs.writeShellApplication {
    name = "agent-sim";
    text = ''
      nix run "$PROJECT_ROOT#testAgents.weekly-simulation"
    '';
  };

  generateConfig = pkgs.writeShellApplication {
    name = "generate-config";
    runtimeInputs = with pkgs; [
      hostname
      coreutils
      pkgs.crystal-forge.default.cf-keygen
    ];
    text = ''
      set -euo pipefail

      CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      mkdir -p "$CF_KEY_DIR"

      # Generate agent keys if they don't exist
      if [[ ! -f "$CF_KEY_DIR/agent.key" ]]; then
        echo "Generating agent keys..."
        cf-keygen -f "$CF_KEY_DIR/agent.key"
      fi

      # Generate builder keys if they don't exist
      if [[ ! -f "$CF_KEY_DIR/builder.key" ]]; then
        echo "Generating builder keys..."
        cf-keygen -f "$CF_KEY_DIR/builder.key"
      fi

      ACTUAL_HOSTNAME="$(hostname -s)"
      ACTUAL_PUBKEY="$(cat "$CF_KEY_DIR/agent.pub")"

      CONFIG_DIR="''${XDG_RUNTIME_DIR:-/tmp}/crystal-forge"
      mkdir -p "$CONFIG_DIR"
      CONFIG_FILE="$CONFIG_DIR/crystal-forge-config.toml"

      sed \
        -e "s/HOSTNAME_PLACEHOLDER/$ACTUAL_HOSTNAME/g" \
        -e "s|PUBLIC_KEY_PLACEHOLDER|$ACTUAL_PUBKEY|g" \
        -e "s|BUILDER_KEY_PATH_PLACEHOLDER|$CF_KEY_DIR/builder.key|g" \
        ${configTemplate} > "$CONFIG_FILE"

      echo "$CONFIG_FILE"
    '';
  };

  envExports = ''
    export CRYSTAL_FORGE_CONFIG="$(${generateConfig}/bin/generate-config)"
  '';
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
    builder = {
      enable_api_mode = true;
      builder_id = "00000000-0000-0000-0000-000000000001";
      private_key_path = "BUILDER_KEY_PATH_PLACEHOLDER";
      server_url = "http://127.0.0.1:${toString cf_port}";
      poll_interval = "5s";
      heartbeat_interval = "30s";
      max_concurrent_jobs = 1;
    };
    client = {
      server_host = "127.0.0.1";
      server_port = cf_port;
      private_key = "$CF_KEY_DIR/agent.key";
    };
    environments = [{
      name = "mockenv";
      description =
        "An environment full of agents created from shell scripts for testing purposes";
      is_active = true;
      risk_profile = "LOW";
      compliance_level = "NONE";
    }];
    systems = [{
      hostname = "test.gray";
      public_key = pkgs.crystal-forge.testAgents.test-gray.publicKey;
      environment = "mockenv";
      flake_name = "dotfiles";
    }];
    flakes = {
      flake_polling_interval = "10m";
      commit_evaluation_interval = "10m";
      build_processing_interval = "10m";
      watched = [{
        name = "dotfiles";
        repo_url = "https://gitlab.com/usmcamp0811/dotfiles";
        auto_poll = false;
        initial_commit_depth = 10;
      }];
    };
  };

  simulatePush = pkgs.writeShellApplication {
    name = "simulate-push";
    runtimeInputs = with pkgs; [ git curl jq ];
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
    runtimeInputs = [ pkgs.nix ];
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
    runtimeInputs = [ pkgs.nix pkgs.git pkgs.vulnix ];
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

  bootstrapDevBuilder = pkgs.writeShellApplication {
    name = "bootstrap-dev-builder";
    runtimeInputs = with pkgs; [
      postgresql
      pkgs.crystal-forge.default.cf-keygen
    ];
    text = ''
      set -euo pipefail

      CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"

      # Ensure builder keys exist
      if [[ ! -f "$CF_KEY_DIR/builder.pub" ]]; then
        echo "Error: Builder keys not found. Run 'generate-config' first."
        exit 1
      fi

      BUILDER_PUBKEY="$(cat "$CF_KEY_DIR/builder.pub")"
      BUILDER_UUID="00000000-0000-0000-0000-000000000001"

      echo "Bootstrapping dev builder..."
      echo "  Builder ID: $BUILDER_UUID"
      echo "  Public Key: $BUILDER_PUBKEY"

      # Insert or update the dev builder
      psql -h 127.0.0.1 -p ${
        toString db_port
      } -U crystal_forge -d crystal_forge <<SQL
        INSERT INTO builders (id, name, public_key, status, max_concurrent_jobs)
        VALUES (
          '$BUILDER_UUID'::uuid,
          'dev-builder',
          '$BUILDER_PUBKEY',
          'active',
          1
        )
        ON CONFLICT (id) DO UPDATE SET
          public_key = EXCLUDED.public_key,
          status = 'active',
          updated_at = now();
      SQL

      echo "✅ Dev builder registered successfully"
    '';
  };

  runBuilder = pkgs.writeShellApplication {
    name = "run-builder";
    runtimeInputs = [ pkgs.nix ];
    text = ''
      CRYSTAL_FORGE_CONFIG="$(${generateConfig}/bin/generate-config)"
      export CRYSTAL_FORGE_CONFIG

      # Bootstrap the dev builder in the database before starting
      ${bootstrapDevBuilder}/bin/bootstrap-dev-builder

      if [[ "''${1:-}" == "--dev" ]]; then
        exec nix run .#builder
      else
        exec ${pkgs.crystal-forge.default.builder}/bin/builder
      fi
    '';
  };

  startBuilderApi = pkgs.writeShellApplication {
    name = "start-builder-api";
    runtimeInputs = with pkgs; [ nix python3 coreutils ];
    text = ''
            set -euo pipefail

            REPO_ROOT="''${PROJECT_ROOT:-$PWD}"
            DEFAULT_SERVER_URL="http://127.0.0.1:${toString cf_port}"

            echo "🔧 Crystal Forge API Builder Launcher"
            echo ""

            read -r -p "Builder UUID: " BUILDER_ID
            if [[ -z "$BUILDER_ID" ]]; then
              echo "Builder UUID is required."
              exit 1
            fi

            read -r -p "Server URL [$DEFAULT_SERVER_URL]: " SERVER_URL
            SERVER_URL="''${SERVER_URL:-$DEFAULT_SERVER_URL}"

            read -r -p "Poll interval seconds [5]: " POLL_INTERVAL
            POLL_INTERVAL="''${POLL_INTERVAL:-5}"

            read -r -p "Heartbeat interval seconds [30]: " HEARTBEAT_INTERVAL
            HEARTBEAT_INTERVAL="''${HEARTBEAT_INTERVAL:-30}"

            read -r -p "Max concurrent jobs [1]: " MAX_CONCURRENT_JOBS
            MAX_CONCURRENT_JOBS="''${MAX_CONCURRENT_JOBS:-1}"

            read -r -s -p "Builder private key hex (64 chars): " PRIVATE_KEY_HEX
            echo ""

            if [[ -z "$PRIVATE_KEY_HEX" ]]; then
              echo "Private key hex is required."
              exit 1
            fi

            KEY_FILE="$(mktemp "''${TMPDIR:-/tmp}/cf-builder-key.XXXXXX")"
            trap 'rm -f "$KEY_FILE"' EXIT

            python3 - "$PRIVATE_KEY_HEX" "$KEY_FILE" <<'PY'
      import pathlib
      import string
      import sys

      hex_key = sys.argv[1].strip()
      out = pathlib.Path(sys.argv[2])

      if len(hex_key) != 64:
          print(f"Expected 64 hex chars, got {len(hex_key)}", file=sys.stderr)
          sys.exit(1)

      if any(c not in string.hexdigits for c in hex_key):
          print("Private key must be valid hex", file=sys.stderr)
          sys.exit(1)

      out.write_bytes(bytes.fromhex(hex_key))
      PY

            chmod 600 "$KEY_FILE"

            export CRYSTAL_FORGE__BUILDER__ENABLE_API_MODE=true
            export CRYSTAL_FORGE__BUILDER__BUILDER_ID="$BUILDER_ID"
            export CRYSTAL_FORGE__BUILDER__PRIVATE_KEY_PATH="$KEY_FILE"
            export CRYSTAL_FORGE__BUILDER__SERVER_URL="$SERVER_URL"
            export CRYSTAL_FORGE__BUILDER__POLL_INTERVAL="$POLL_INTERVAL"
            export CRYSTAL_FORGE__BUILDER__HEARTBEAT_INTERVAL="$HEARTBEAT_INTERVAL"
            export CRYSTAL_FORGE__BUILDER__MAX_CONCURRENT_JOBS="$MAX_CONCURRENT_JOBS"

            echo "🚀 Starting builder in API mode..."
            echo "   Builder ID: $BUILDER_ID"
            echo "   Server URL: $SERVER_URL"

            nix run "$REPO_ROOT#builder"
    '';
  };

  db-module = {
    settings.processes.pgweb = {
      inherit namespace;
      command = "${pkgs.pgweb}/bin/pgweb --listen=${
          toString pgweb_port
        } --bind=0.0.0.0";
      depends_on."db".condition = "process_healthy";
      environment.PGWEB_DATABASE_URL =
        "postgres://crystal_forge:${db_password}@127.0.0.1:${
          toString db_port
        }/crystal_forge";
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
        CREATE USER grafana LOGIN;
        CREATE DATABASE grafana_db OWNER grafana;
        GRANT ALL PRIVILEGES ON DATABASE grafana_db TO grafana;
      '';
      initialDatabases = [ ];
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
      datasources = [{
        name = "Crystal Forge PostgreSQL";
        uid = "crystal-forge-postgres";
        type = "postgres";
        access = "proxy";
        url = "localhost:${toString db_port}";
        database = "crystal_forge";
        user = "crystal_forge";
        secureJsonData = { password = db_password; };
        jsonData = { sslmode = "disable"; };
        isDefault = false;
        editable = true;
      }];
      providers = [{
        name = "Crystal Forge";
        type = "file";
        disableDeletion = true;
        updateIntervalSeconds = 60;
        options = { path = "${pkgs.crystal-forge.dashboards}/dashboards"; };
      }];
    };
    settings.processes."grafana".depends_on."db".condition = "process_healthy";
  };

  db-core-module = {
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
      initialDatabases = [ ];
    };
  };

  agent-module = {
    settings.processes.agent = {
      inherit namespace;
      command = runAgent;
      depends_on."server".condition = "process_healthy";
      disabled = false;
    };
  };

  builder-module = {
    settings.processes.builder = {
      inherit namespace;
      command = runBuilder;
      depends_on."db".condition = "process_healthy";
      disabled = false;
    };
  };

  server-module = {
    settings.processes.server = {
      inherit namespace;
      command = runServer;
      depends_on."db".condition = "process_healthy";
      environment = { AUTH_MODE = "local"; };
      readiness_probe = {
        exec.command = "${pkgs.postgresql}/bin/pg_isready -h 127.0.0.1 -p ${
            toString db_port
          } -U crystal_forge -d crystal_forge";
        initial_delay_seconds = 2;
        period_seconds = 5;
        timeout_seconds = 3;
        success_threshold = 1;
        failure_threshold = 5;
      };
    };
  };

  oidc-module = {
    settings.processes.oidc = {
      inherit namespace;
      command = ''
        set -euo pipefail
        ${pkgs.docker}/bin/docker rm -f cf-oidc >/dev/null 2>&1 || true
        exec ${pkgs.docker}/bin/docker run --rm --name cf-oidc -p ${
          toString oidc_port
        }:8080 -e KEYCLOAK_ADMIN=admin -e KEYCLOAK_ADMIN_PASSWORD=admin -v ${oidc_realm_import}:/opt/keycloak/data/import/realm-crystal-forge.json:ro quay.io/keycloak/keycloak:26.0 start-dev --import-realm --http-port=8080 --hostname-strict=false
      '';
      readiness_probe = {
        exec.command =
          "${pkgs.curl}/bin/curl -fsS ${oidc_issuer}/.well-known/openid-configuration >/dev/null";
        initial_delay_seconds = 5;
        period_seconds = 5;
        timeout_seconds = 3;
        success_threshold = 1;
        failure_threshold = 24;
      };
    };
  };

  server-oidc-module = {
    settings.processes.server.environment = {
      AUTH_MODE = "oidc";
      CRYSTAL_FORGE_OIDC_ISSUER_URL = oidc_issuer;
      CRYSTAL_FORGE_OIDC_CLIENT_ID = oidc_client_id;
      CRYSTAL_FORGE_OIDC_CLIENT_SECRET = oidc_client_secret;
      CRYSTAL_FORGE_OIDC_REDIRECT_URI =
        "http://127.0.0.1:${toString cf_port}/api/auth/oidc/callback";
      CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP = "admin";
    };
    settings.processes.server.depends_on."oidc".condition = "process_healthy";
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
      builder-module
      db-module
    ];
  };

  dbOnly = pkgs.process-compose-flake.evalModules {
    modules = [ inputs.services-flake.processComposeModules.default db-module ];
  };

  oidc-stack = pkgs.process-compose-flake.evalModules {
    modules = [
      inputs.services-flake.processComposeModules.default
      db-core-module
      oidc-module
      server-module
      server-oidc-module
    ];
  };
in full-stack.config.outputs.package // {
  inherit runServer runAgent runBuilder simulatePush startBuilderApi
    bootstrapDevBuilder envExports;
  db-only = dbOnly.config.outputs.package;
  server-only = server-only.config.outputs.package;
  oidc-stack = oidc-stack.config.outputs.package;
}
