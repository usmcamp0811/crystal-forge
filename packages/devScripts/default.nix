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

  # Internal (local) issuer for health checks
  oidc_issuer_internal =
    "http://127.0.0.1:${toString oidc_port}/realms/${oidc_realm}";

  oidc_realm_import = ./oidc/realm-crystal-forge.json;
  tomlFormat = pkgs.formats.toml { };

  agent-sim = pkgs.writeShellApplication {
    name = "agent-sim";
    text = ''
      nix run "$PROJECT_ROOT#testAgents.weekly-simulation"
    '';
  };

  # Helper function to create config generators with different templates
  makeGenerateConfig = template:
    pkgs.writeShellApplication {
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

        # Generate a local cache encryption key if it doesn't exist
        if [[ ! -f "$CF_KEY_DIR/cache-encryption.key" ]]; then
          echo "Generating cache encryption key..."
          head -c 48 /dev/urandom | base64 > "$CF_KEY_DIR/cache-encryption.key"
          chmod 600 "$CF_KEY_DIR/cache-encryption.key"
        fi

        # Prefer an address other machines can resolve (FQDN), fall back to short host.
        # If your LAN DNS doesn't resolve either, set CF_PUBLIC_HOST yourself before running.
        ACTUAL_HOST="''${CF_PUBLIC_HOST:-$(hostname -f 2>/dev/null || hostname -s)}"
        ACTUAL_PUBKEY="$(cat "$CF_KEY_DIR/agent.pub")"

        CONFIG_DIR="''${XDG_RUNTIME_DIR:-/tmp}/crystal-forge"
        mkdir -p "$CONFIG_DIR"
        CONFIG_FILE="$CONFIG_DIR/crystal-forge-config.toml"

        sed \
          -e "s/HOSTNAME_PLACEHOLDER/$ACTUAL_HOST/g" \
          -e "s|PUBLIC_KEY_PLACEHOLDER|$ACTUAL_PUBKEY|g" \
          -e "s|BUILDER_KEY_PATH_PLACEHOLDER|$CF_KEY_DIR/builder.key|g" \
          ${template} > "$CONFIG_FILE"

        echo "$CONFIG_FILE"
      '';
    };

  # Clean config generator (no pre-populated data)
  generateConfig = makeGenerateConfig configTemplate;

  # Mock config generator (includes pre-populated mock data)
  generateConfigMock = makeGenerateConfig configTemplateMock;

  envExports = ''
    export CRYSTAL_FORGE_CONFIG="$(${generateConfig}/bin/generate-config)"
    CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
    CACHE_ENCRYPTION_KEY="$(cat "$CF_KEY_DIR/cache-encryption.key")"
    export CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY="$CACHE_ENCRYPTION_KEY"
  '';

  # Clean config template for production-like deployments (no pre-populated data)
  configTemplateClean = tomlFormat.generate "crystal-forge-config-clean.toml" {
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

      # 👇 external-friendly (not localhost)
      server_url = "http://HOSTNAME_PLACEHOLDER:${toString cf_port}";

      poll_interval = 5;
      heartbeat_interval = 30;
      max_concurrent_jobs = 1;
    };
    client = {
      # 👇 external-friendly (not localhost)
      server_host = "HOSTNAME_PLACEHOLDER";
      server_port = cf_port;
      private_key = "$CF_KEY_DIR/agent.key";
    };
    # NO pre-populated environments, systems, or flakes
    flakes = {
      flake_polling_interval = "10m";
      commit_evaluation_interval = "10m";
      build_processing_interval = "10m";
      watched = [ ];
    };
  };

  # Mock config template for development/demo (pre-populated with test data)
  configTemplateMock = tomlFormat.generate "crystal-forge-config-mock.toml" {
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

      # 👇 external-friendly (not localhost)
      server_url = "http://HOSTNAME_PLACEHOLDER:${toString cf_port}";

      poll_interval = 5;
      heartbeat_interval = 30;
      max_concurrent_jobs = 1;
    };
    client = {
      # 👇 external-friendly (not localhost)
      server_host = "HOSTNAME_PLACEHOLDER";
      server_port = cf_port;
      private_key = "$CF_KEY_DIR/agent.key";
    };
    # Pre-populated mock data for development/demo
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

  # Default to clean template (backward compatibility alias)
  configTemplate = configTemplateClean;

  simulatePush = pkgs.writeShellApplication {
    name = "simulate-push";
    runtimeInputs = with pkgs; [ git curl jq hostname ];
    text = ''
      set -euo pipefail

      REPO_URL="''${1:-https://gitlab.com/usmcamp0811/dotfiles}"

      HOST="''${CF_PUBLIC_HOST:-$(hostname -f 2>/dev/null || hostname -s)}"
      DEFAULT_SERVER_URL="http://$HOST:${toString cf_port}/webhook"
      SERVER_URL="''${2:-$DEFAULT_SERVER_URL}"

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
    runtimeInputs = [ pkgs.nix pkgs.git pkgs.vulnix pkgs.coreutils ];
    text = ''
      CRYSTAL_FORGE_CONFIG="$(${generateConfig}/bin/generate-config)"
      export CRYSTAL_FORGE_CONFIG
      CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      CACHE_ENCRYPTION_KEY="$(cat "$CF_KEY_DIR/cache-encryption.key")"
      export CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY="$CACHE_ENCRYPTION_KEY"
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
      coreutils
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

      echo "Waiting for server to be ready (migrations to run)..."

      # Wait up to 60 seconds for the builders table to exist
      for i in {1..60}; do
        if psql -h 127.0.0.1 -p ${
          toString db_port
        } -U crystal_forge -d crystal_forge \
               -c "SELECT 1 FROM builders LIMIT 1;" >/dev/null 2>&1; then
          break
        fi
        if [ "$i" -eq 60 ]; then
          echo "ERROR: Timed out waiting for builders table to exist"
          echo "Make sure the server has started and run migrations"
          exit 1
        fi
        sleep 1
      done

      echo "Bootstrapping dev builder..."
      echo "  Builder ID: $BUILDER_UUID"
      echo "  Public Key: $BUILDER_PUBKEY"

      # Insert or update the dev builder
      psql -h 127.0.0.1 -p ${
        toString db_port
      } -U crystal_forge -d crystal_forge <<SQL
        -- Delete any existing dev builder to ensure clean state
        DELETE FROM builders WHERE id = '$BUILDER_UUID'::uuid;

        -- Insert dev builder with active status
        INSERT INTO builders (id, name, public_key, status, max_concurrent_jobs)
        VALUES (
          '$BUILDER_UUID'::uuid,
          'dev-builder',
          '$BUILDER_PUBKEY',
          'active',
          1
        );
      SQL

      echo "✅ Dev builder registered successfully"
    '';
  };

  runBuilder = pkgs.writeShellApplication {
    name = "run-builder";
    runtimeInputs = [ pkgs.nix pkgs.coreutils ];
    text = ''
      CRYSTAL_FORGE_CONFIG="$(${generateConfig}/bin/generate-config)"
      export CRYSTAL_FORGE_CONFIG
      CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      CACHE_ENCRYPTION_KEY="$(cat "$CF_KEY_DIR/cache-encryption.key")"
      export CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY="$CACHE_ENCRYPTION_KEY"

      # Bootstrap the dev builder in the database before starting
      ${bootstrapDevBuilder}/bin/bootstrap-dev-builder

      if [[ "''${1:-}" == "--dev" ]]; then
        exec nix run .#builder
      else
        exec ${pkgs.crystal-forge.default.builder}/bin/builder
      fi
    '';
  };

  # Mock variants that use pre-populated config template
  runServerMock = pkgs.writeShellApplication {
    name = "run-server-mock";
    runtimeInputs = [ pkgs.nix pkgs.git pkgs.vulnix pkgs.coreutils ];
    text = ''
      CRYSTAL_FORGE_CONFIG="$(${generateConfigMock}/bin/generate-config)"
      export CRYSTAL_FORGE_CONFIG
      CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      CACHE_ENCRYPTION_KEY="$(cat "$CF_KEY_DIR/cache-encryption.key")"
      export CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY="$CACHE_ENCRYPTION_KEY"
      if [[ "''${1:-}" == "--dev" ]]; then
        exec nix run .#server
      else
        exec ${pkgs.crystal-forge.default.server}/bin/server
      fi
    '';
  };

  runBuilderMock = pkgs.writeShellApplication {
    name = "run-builder-mock";
    runtimeInputs = [ pkgs.nix pkgs.coreutils ];
    text = ''
      CRYSTAL_FORGE_CONFIG="$(${generateConfigMock}/bin/generate-config)"
      export CRYSTAL_FORGE_CONFIG
      CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      CACHE_ENCRYPTION_KEY="$(cat "$CF_KEY_DIR/cache-encryption.key")"
      export CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY="$CACHE_ENCRYPTION_KEY"

      # Bootstrap the dev builder in the database before starting
      ${bootstrapDevBuilder}/bin/bootstrap-dev-builder

      if [[ "''${1:-}" == "--dev" ]]; then
        exec nix run .#builder
      else
        exec ${pkgs.crystal-forge.default.builder}/bin/builder
      fi
    '';
  };

  seedCveMock = pkgs.writeShellApplication {
    name = "seed-cve-mock";
    runtimeInputs = with pkgs; [ postgresql coreutils ];
    text = ''
      set -euo pipefail

      DB_URL="postgresql://crystal_forge@127.0.0.1:${
        toString db_port
      }/crystal_forge"

      # Wait for schema and mock system bootstrapping to be ready.
      for i in {1..60}; do
        if psql "$DB_URL" -c "SELECT 1 FROM systems WHERE hostname = 'test.gray' LIMIT 1;" >/dev/null 2>&1; then
          break
        fi
        if [ "$i" -eq 60 ]; then
          echo "ERROR: timed out waiting for systems seed (test.gray)"
          exit 1
        fi
        sleep 1
      done

      psql "$DB_URL" <<'SQL'
      DO $$
      DECLARE
        v_status_id integer;
        v_flake_id integer;
        v_commit_id integer;
        v_host_deriv_id integer;
        v_scan_id uuid;
        v_pkg_go_1 integer;
        v_pkg_go_2 integer;
        v_pkg_ssl_1 integer;
        v_pkg_ssl_2 integer;
      BEGIN
        SELECT id INTO v_status_id
        FROM derivation_statuses
        WHERE name = 'complete'
        LIMIT 1;

        IF v_status_id IS NULL THEN
          RAISE EXCEPTION 'missing derivation_statuses.complete';
        END IF;

        -- Idempotent flake/commit records used for mock CVE provenance.
        INSERT INTO flakes (name, repo_url, branch, build_scope)
        VALUES ('mock-cve-flake', 'https://example.com/mock-cve-flake', 'main', 'full')
        ON CONFLICT (name) DO UPDATE SET
          repo_url = EXCLUDED.repo_url,
          branch = EXCLUDED.branch,
          build_scope = EXCLUDED.build_scope
        RETURNING id INTO v_flake_id;

        SELECT id INTO v_commit_id
        FROM commits
        WHERE flake_id = v_flake_id
          AND git_commit_hash = 'mockcve0001'
        ORDER BY id DESC
        LIMIT 1;

        IF v_commit_id IS NULL THEN
          INSERT INTO commits (
            flake_id,
            git_commit_hash,
            commit_timestamp,
            attempt_count,
            evaluation_status,
            evaluation_attempt_count
          ) VALUES (
            v_flake_id,
            'mockcve0001',
            NOW(),
            0,
            'completed',
            0
          )
          RETURNING id INTO v_commit_id;
        END IF;

        -- Reuse existing host derivation if present; otherwise create one.
        SELECT id INTO v_host_deriv_id
        FROM derivations
        WHERE derivation_name = 'test.gray'
          AND derivation_type = 'nixos'
        ORDER BY completed_at DESC NULLS LAST, id DESC
        LIMIT 1;

        IF v_host_deriv_id IS NULL THEN
          INSERT INTO derivations (
            commit_id,
            derivation_type,
            derivation_name,
            derivation_path,
            attempt_count,
            status_id,
            completed_at
          ) VALUES (
            v_commit_id,
            'nixos',
            'test.gray',
            '/nix/store/mock-cve-test-gray.drv',
            0,
            v_status_id,
            NOW()
          )
          RETURNING id INTO v_host_deriv_id;
        END IF;

        -- Idempotency guard: if the mock scan already exists for this host derivation, stop.
        IF EXISTS (
          SELECT 1
          FROM cve_scans
          WHERE derivation_id = v_host_deriv_id
            AND scanner_name = 'mock-seed-cve'
        ) THEN
          RAISE NOTICE 'mock CVE data already seeded for test.gray';
          RETURN;
        END IF;

        -- Seed four representative CVEs (critical/high/medium/low).
        INSERT INTO cves (id, description, cvss_v3_score, published_date) VALUES
          ('CVE-2024-7001', 'Mock critical CVE for server-stack-mock', 9.8, DATE '2024-01-01'),
          ('CVE-2024-7002', 'Mock high CVE for server-stack-mock',     8.0, DATE '2024-01-02'),
          ('CVE-2024-7003', 'Mock medium CVE for server-stack-mock',   6.5, DATE '2024-01-03'),
          ('CVE-2024-7004', 'Mock low CVE for server-stack-mock',      3.5, DATE '2024-01-04')
        ON CONFLICT (id) DO NOTHING;

        INSERT INTO cve_scans (
          id,
          derivation_id,
          scanner_name,
          completed_at,
          status,
          attempts,
          total_packages,
          total_vulnerabilities,
          critical_count,
          high_count,
          medium_count,
          low_count,
          scan_metadata
        ) VALUES (
          gen_random_uuid(),
          v_host_deriv_id,
          'mock-seed-cve',
          NOW(),
          'completed',
          1,
          5,
          8,
          2,
          2,
          2,
          2,
          '{"source":"server-stack-mock","seed":"task-272"}'::jsonb
        )
        RETURNING id INTO v_scan_id;

        -- Insert package derivations used by the mock CVEs (idempotent by derivation_path).
        SELECT id INTO v_pkg_go_1 FROM derivations WHERE derivation_path = '/nix/store/mock-go-1.drv' LIMIT 1;
        IF v_pkg_go_1 IS NULL THEN
          INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            pname, version, attempt_count, status_id, completed_at
          ) VALUES (
            v_commit_id, 'package', 'mock-go-1', '/nix/store/mock-go-1.drv',
            'go', '1.22.0', 0, v_status_id, NOW()
          ) RETURNING id INTO v_pkg_go_1;
        END IF;

        SELECT id INTO v_pkg_go_2 FROM derivations WHERE derivation_path = '/nix/store/mock-go-2.drv' LIMIT 1;
        IF v_pkg_go_2 IS NULL THEN
          INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            pname, version, attempt_count, status_id, completed_at
          ) VALUES (
            v_commit_id, 'package', 'mock-go-2', '/nix/store/mock-go-2.drv',
            'go', '1.23.0', 0, v_status_id, NOW()
          ) RETURNING id INTO v_pkg_go_2;
        END IF;

        SELECT id INTO v_pkg_ssl_1 FROM derivations WHERE derivation_path = '/nix/store/mock-openssl-1.drv' LIMIT 1;
        IF v_pkg_ssl_1 IS NULL THEN
          INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            pname, version, attempt_count, status_id, completed_at
          ) VALUES (
            v_commit_id, 'package', 'mock-openssl-1', '/nix/store/mock-openssl-1.drv',
            'openssl', '3.1.0', 0, v_status_id, NOW()
          ) RETURNING id INTO v_pkg_ssl_1;
        END IF;

        SELECT id INTO v_pkg_ssl_2 FROM derivations WHERE derivation_path = '/nix/store/mock-openssl-2.drv' LIMIT 1;
        IF v_pkg_ssl_2 IS NULL THEN
          INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            pname, version, attempt_count, status_id, completed_at
          ) VALUES (
            v_commit_id, 'package', 'mock-openssl-2', '/nix/store/mock-openssl-2.drv',
            'openssl', '3.2.0', 0, v_status_id, NOW()
          ) RETURNING id INTO v_pkg_ssl_2;
        END IF;

        INSERT INTO scan_packages (id, scan_id, derivation_id)
        VALUES
          (gen_random_uuid(), v_scan_id, v_pkg_go_1),
          (gen_random_uuid(), v_scan_id, v_pkg_go_2),
          (gen_random_uuid(), v_scan_id, v_pkg_ssl_1),
          (gen_random_uuid(), v_scan_id, v_pkg_ssl_2)
        ON CONFLICT DO NOTHING;

        -- Explicit vulnerability mapping per package.
        INSERT INTO package_vulnerabilities (id, derivation_id, cve_id, is_whitelisted, detection_method)
        SELECT gen_random_uuid(), d.id, cve_id, FALSE, 'mock-seed'
        FROM (
          VALUES
            ('mock-go-1', 'CVE-2024-7001'),
            ('mock-go-2', 'CVE-2024-7001'),
            ('mock-go-1', 'CVE-2024-7002'),
            ('mock-go-2', 'CVE-2024-7002'),
            ('mock-openssl-1', 'CVE-2024-7003'),
            ('mock-openssl-2', 'CVE-2024-7003'),
            ('mock-openssl-1', 'CVE-2024-7004'),
            ('mock-openssl-2', 'CVE-2024-7004')
        ) AS mapping(derivation_name, cve_id)
        JOIN derivations d
          ON d.derivation_name = mapping.derivation_name
         AND d.derivation_type = 'package'
        ON CONFLICT DO NOTHING;

        RAISE NOTICE 'mock CVE data seeded for test.gray (task-272)';
      END $$;
      SQL

      echo "✅ mock CVE data seeded"
    '';
  };

  startBuilderApi = pkgs.writeShellApplication {
    name = "start-builder-api";
    runtimeInputs = with pkgs; [ nix python3 coreutils hostname ];
    text = ''
      set -euo pipefail

      CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      mkdir -p "$CF_KEY_DIR"
      if [[ ! -f "$CF_KEY_DIR/cache-encryption.key" ]]; then
        head -c 48 /dev/urandom | base64 > "$CF_KEY_DIR/cache-encryption.key"
        chmod 600 "$CF_KEY_DIR/cache-encryption.key"
      fi
      CACHE_ENCRYPTION_KEY="$(cat "$CF_KEY_DIR/cache-encryption.key")"
      export CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY="$CACHE_ENCRYPTION_KEY"

      REPO_ROOT="''${PROJECT_ROOT:-$PWD}"
      HOST="''${CF_PUBLIC_HOST:-$(hostname -f 2>/dev/null || hostname -s)}"
      DEFAULT_SERVER_URL="http://$HOST:${toString cf_port}"

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

      # NOTE: grafana's "domain" affects generated links; keep it reachable remotely.
      domain = "0.0.0.0";

      datasources = [{
        name = "Crystal Forge PostgreSQL";
        uid = "crystal-forge-postgres";
        type = "postgres";
        access = "proxy";

        # Postgres is local to the same machine running grafana here.
        url = "127.0.0.1:${toString db_port}";

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
        # Probe locally so it works regardless of LAN DNS
        exec.command =
          "${pkgs.curl}/bin/curl -fsS ${oidc_issuer_internal}/.well-known/openid-configuration >/dev/null";
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

      # 👇 issuer that remote browsers/clients can resolve
      CRYSTAL_FORGE_OIDC_ISSUER_URL = "http://HOSTNAME_PLACEHOLDER:${
          toString oidc_port
        }/realms/${oidc_realm}";

      CRYSTAL_FORGE_OIDC_CLIENT_ID = oidc_client_id;
      CRYSTAL_FORGE_OIDC_CLIENT_SECRET = oidc_client_secret;

      # 👇 callback that matches the host you're visiting from your laptop
      CRYSTAL_FORGE_OIDC_REDIRECT_URI = "http://HOSTNAME_PLACEHOLDER:${
          toString cf_port
        }/api/auth/oidc/callback";

      CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP = "admin";
    };
    settings.processes.server.depends_on."oidc".condition = "process_healthy";
  };

  mock-execution-module = {
    settings.processes.server.command =
      mkForce "${runServerMock}/bin/run-server-mock --dev";
    settings.processes.server.environment = {
      AUTH_MODE = mkForce "local";
      CRYSTAL_FORGE__SERVER__EXECUTION_MODE = "mock";
      CRYSTAL_FORGE_LOCAL_BOOTSTRAP_USERNAME = "admin";
      CRYSTAL_FORGE_LOCAL_BOOTSTRAP_PASSWORD = "password";
      CRYSTAL_FORGE_LOCAL_BOOTSTRAP_EMAIL = "admin@crystal-forge.local";
    };

    settings.processes.builder.command =
      mkForce "${runBuilderMock}/bin/run-builder-mock --dev";
    settings.processes.builder.environment = {
      AUTH_MODE = mkForce "local";
      CRYSTAL_FORGE__SERVER__EXECUTION_MODE = "mock";
      CRYSTAL_FORGE__BUILDER__ENABLE_API_MODE = "true";
      CRYSTAL_FORGE__BUILDER__BUILDER_ID =
        "00000000-0000-0000-0000-000000000001";
      CRYSTAL_FORGE__BUILDER__SERVER_URL =
        "http://127.0.0.1:${toString cf_port}";
    };

    settings.processes.seed-cve-mock = {
      inherit namespace;
      command = seedCveMock;
      depends_on."db".condition = "process_healthy";
      depends_on."server".condition = "process_healthy";
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
      builder-module
      db-module
    ];
  };

  server-stack-mock = pkgs.process-compose-flake.evalModules {
    modules = [
      inputs.services-flake.processComposeModules.default
      server-module
      builder-module
      db-module
      mock-execution-module
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
  server-stack-mock = server-stack-mock.config.outputs.package;
  oidc-stack = oidc-stack.config.outputs.package;
}
