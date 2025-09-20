{
  lib,
  inputs,
  system ? null,
  ...
}: rec {
  makeS3CacheNode = {
    pkgs,
    bucketName ? "nix-cache",
    accessKey ? "minioadmin",
    secretKey ? "minioadmin",
    port ? 9000,
    consolePort ? 9001,
    enableFirewall ? false,
    extraConfig ? {},
    ...
  }:
    {
      virtualisation.writableStore = true;
      virtualisation.memorySize = 1024;

      networking.useDHCP = true;
      networking.firewall.enable = enableFirewall;

      # BUGFIX: you only want to open ports when the firewall is enabled
      networking.firewall.allowedTCPPorts = lib.mkIf enableFirewall [port consolePort];

      services.minio = {
        enable = true;
        listenAddress = "0.0.0.0:${toString port}";
        consoleAddress = "0.0.0.0:${toString consolePort}";
        rootCredentialsFile = pkgs.writeText "minio-credentials" ''
          MINIO_ROOT_USER=${accessKey}
          MINIO_ROOT_PASSWORD=${secretKey}
        '';
        # Optional but nice to be explicit
        dataDir = ["/var/lib/minio"];
      };

      # Create bucket and set policy
      systemd.services.minio-setup = {
        # Ensure network is actually up and MinIO is running before we poke it
        after = ["network-online.target" "minio.service"];
        wants = ["network-online.target" "minio.service"];
        requires = ["minio.service"];
        wantedBy = ["multi-user.target"];

        environment = {
          # BUGFIX: you call `ip` but didn’t have iproute2 in PATH
          PATH =
            lib.mkForce
            "${pkgs.minio-client}/bin:${pkgs.iproute2}/bin:${pkgs.coreutils}/bin:${pkgs.curl}/bin:${pkgs.gnugrep}/bin";
          # keep mc state ephemeral
          HOME = "/tmp";
        };

        script = ''
          set -euo pipefail
          echo "Starting MinIO setup for bucket: ${bucketName}"

          # Prefer localhost (avoids interface name assumptions like eth0/ens3)
          MINIO_URL="http://127.0.0.1:${toString port}"
          echo "Using MinIO URL: $MINIO_URL"

          # Wait for MinIO readiness
          for i in {1..60}; do
            if curl -fsS "$MINIO_URL/minio/health/live" >/dev/null; then
              echo "MinIO is ready after $i attempts"
              break
            fi
            if [ "$i" -eq 60 ]; then
              echo "ERROR: MinIO failed to start after 60 attempts"
              exit 1
            fi
            echo "Waiting for MinIO... attempt $i/60"
            sleep 2
          done

          echo "Configuring MinIO client..."
          mc alias set local "$MINIO_URL" "${accessKey}" "${secretKey}"

          echo "Creating bucket: ${bucketName}"
          if mc mb "local/${bucketName}" 2>/dev/null; then
            echo "Created bucket"
          elif mc ls local/ | grep -q "^${bucketName}/\?$"; then
            echo "Bucket already exists"
          else
            echo "ERROR: Failed to create or find bucket"
            exit 1
          fi

          echo "Setting bucket policy for public read (anon download)..."
          # For Nix substituters without AWS creds; remove if you’ll auth from Nix
          mc anonymous set download "local/${bucketName}" || \
            echo "WARNING: failed to set anonymous download policy"

          echo "Verifying access via mc..."
          mc ls "local/${bucketName}" >/dev/null

          echo "Testing S3 path-style via curl (anonymous)…"
          # This will only succeed if anonymous was enabled above
          if curl -fsS "$MINIO_URL/${bucketName}/" >/dev/null; then
            echo "S3 API test passed"
          else
            echo "WARNING: S3 API test failed (expected if bucket is not public)"
          fi

          echo "MinIO setup completed successfully"
        '';

        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          User = "root";
          Group = "root";
          ExitType = "main";
        };
      };

      systemd.services.minio-verify = {
        after = ["minio-setup.service"];
        wants = ["minio-setup.service"];
        wantedBy = ["multi-user.target"];

        environment = {
          PATH = lib.mkForce "${pkgs.minio-client}/bin:${pkgs.coreutils}/bin:${pkgs.curl}/bin";
          HOME = "/tmp"; # keep mc state consistent with setup
        };

        script = ''
          set -euo pipefail
          echo "Verifying MinIO setup..."
          MINIO_URL="http://127.0.0.1:${toString port}"
          mc alias set verify "$MINIO_URL" "${accessKey}" "${secretKey}"
          mc ls "verify/${bucketName}" >/dev/null
          echo "Bucket verification successful"
        '';

        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          User = "root";
          Group = "root";
        };
      };
    }
    // extraConfig;

  makeAtticCacheNode = {
    pkgs,
    lib,
    port ? 8080,
    enableFirewall ? false,
    extraConfig ? {},
    ...
  }: let
    # Find the Attic *client* package across nixpkgs variants
    atticClient =
      pkgs.attic or pkgs.attic-client or (throw ''
        Attic client package not found in pkgs.
        Tried: pkgs.attic and pkgs.attic-client.
        Fix by:
          • Updating nixpkgs to a revision that includes Attic, or
          • Adding an overlay/input that provides the Attic client.
      '');
  in
    {
      virtualisation.writableStore = true;
      virtualisation.memorySize = 1024;

      networking.useDHCP = true;
      networking.firewall.enable = enableFirewall;
      networking.firewall.allowedTCPPorts = lib.mkIf enableFirewall [port];

      # Server is usually pkgs.attic-server; client is detected above
      environment.systemPackages = [
        atticClient
        pkgs.attic-server
        pkgs.curl
        pkgs.coreutils
      ];

      users.users.attic = {
        description = "Attic service user";
        isSystemUser = true;
        group = "attic";
        home = "/var/lib/attic";
        createHome = true;
      };
      users.groups.attic = {};

      # PostgreSQL setup for Attic
      services.postgresql = {
        enable = true;
        ensureDatabases = ["attic"];
        ensureUsers = [
          {
            name = "attic";
            ensureDBOwnership = true;
          }
        ];
        authentication = ''
          local all all peer
          host all all 127.0.0.1/32 trust
          host all all ::1/128 trust
        '';
      };

      environment.etc."atticd.toml".text = ''
        listen = "0.0.0.0:${toString port}"

        [database]
        url = "postgresql://attic@localhost/attic"

        [storage]
        type = "local"
        path = "/var/lib/attic/storage"

        [chunking]
        nar-size-threshold = 65536
        min-size = 16384
        avg-size = 65536
        max-size = 262144

        [compression]
        type = "zstd"
        level = 8

        [jwt.signing]
        token-hs256-secret-base64 = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA=="
      '';

      systemd.services.atticd = {
        description = "Attic Cache Daemon";
        wantedBy = ["multi-user.target"];
        after = ["network-online.target" "postgresql.service"];
        wants = ["network-online.target"];
        requires = ["postgresql.service"];

        environment = {
          ATTICD_SERVER_TOKEN_HS256_SECRET_BASE64 = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==";
        };

        serviceConfig = {
          ExecStart = "${pkgs.attic-server}/bin/atticd --config /etc/atticd.toml";
          Restart = "always";
          RestartSec = 10;
          User = "attic";
          Group = "attic";
          StateDirectory = "attic";
          StateDirectoryMode = "0755";
          WorkingDirectory = "/var/lib/attic";
          ReadWritePaths = "/var/lib/attic";
        };
      };

      systemd.services.attic-setup = {
        description = "Attic Cache Setup";
        after = ["atticd.service" "postgresql.service"];
        requires = ["atticd.service" "postgresql.service"];
        wantedBy = ["multi-user.target"];

        environment = {
          PATH = lib.mkForce "${pkgs.systemd}/bin:${pkgs.attic-server}/bin:${atticClient}/bin:${pkgs.curl}/bin:${pkgs.coreutils}/bin:${pkgs.gnugrep}/bin";
        };

        script = ''
          set -euo pipefail
          echo "Starting Attic cache setup..."

          BASE_URL="http://127.0.0.1:${toString port}"

          # Wait for API to be reachable (any 2xx/3xx/4xx means listener is up)
          for i in {1..60}; do
            if curl -sf -o /dev/null -w "%{http_code}" "$BASE_URL/" | grep -qE '^(2|3|4)'; then
              echo "atticd is up after $i attempts"
              break
            fi
            if [ "$i" -eq 60 ]; then
              echo "ERROR: atticd did not become ready"
              systemctl status atticd.service || true
              journalctl -u atticd.service --no-pager -n 100 || true
              exit 1
            fi
            sleep 2
          done

          # Mint a token with wide perms using the SAME config/secret as atticd
          TOKEN="$(${pkgs.attic-server}/bin/atticadm --config /etc/atticd.toml \
            make-token --sub setup --validity 1d \
            --pull '*' --push '*' --create-cache '*' --configure-cache '*')"

          # Login alias "local"
          ${atticClient}/bin/attic login local "$BASE_URL" "$TOKEN"

          # Create cache "test" if missing (idempotent)
          if ! ${atticClient}/bin/attic cache info local:test >/dev/null 2>&1; then
            ${atticClient}/bin/attic cache create local:test
          fi

          # Make it public (idempotent)
          ${atticClient}/bin/attic cache configure local:test --public || true

          echo "Attic setup completed successfully"
        '';

        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          User = "root";
          Group = "root";
        };
      };

      systemd.services.attic-debug = {
        description = "Attic Debug Info";
        after = ["attic-setup.service"];
        wantedBy = ["multi-user.target"];

        environment = {
          PATH = lib.mkForce "${pkgs.attic-server}/bin:${atticClient}/bin:${pkgs.curl}/bin:${pkgs.coreutils}/bin:${pkgs.gnugrep}/bin";
        };

        # in makeAtticCacheNode -> systemd.services.attic-debug.script
        script = ''
          echo "=== Attic Debug Info ==="
          ss -tlnp | grep ":${toString port}" || echo "Nothing listening on ${toString port}"
          curl -sv "http://127.0.0.1:${toString port}/" || true

          TOKEN="$(${pkgs.attic-server}/bin/atticadm --config /etc/atticd.toml \
              make-token --sub debug --validity 5m \
              --pull '*' --push '*' --create-cache '*' --configure-cache '*')"

          ${atticClient}/bin/attic login debug "http://127.0.0.1:${toString port}" "$TOKEN" || true
          ${atticClient}/bin/attic cache info debug:test || true

          systemctl status atticd.service || true
          ls -la /var/lib/attic/ || true
          echo "=== End Debug Info ==="
        '';

        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
        };
      };
    }
    // extraConfig;
}
