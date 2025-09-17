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
    port ? 8080,
    enableFirewall ? false,
    extraConfig ? {},
    ...
  }:
    {
      virtualisation.writableStore = true;
      virtualisation.memorySize = 512;
      networking.useDHCP = true;
      networking.firewall.enable = enableFirewall;
      networking.firewall.allowedTCPPorts = lib.mkIf (!enableFirewall) [port];

      # Install attic packages
      environment.systemPackages = with pkgs; [attic-server attic-client];

      # Create a simple systemd service for atticd
      systemd.services.atticd = {
        description = "Attic Cache Daemon";
        wantedBy = ["multi-user.target"];
        after = ["network.target"];

        environment = {
          ATTICD_SERVER_TOKEN_HS256_SECRET_BASE64 = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==";
        };

        serviceConfig = {
          Type = "exec";
          ExecStart = "${pkgs.attic-server}/bin/atticd --listen [::]:${toString port}";
          Restart = "always";
          RestartSec = 5;
          User = "attic";
          Group = "attic";
          StateDirectory = "attic";
          WorkingDirectory = "/var/lib/attic";
        };
      };

      # Create attic user
      users.users.attic = {
        description = "Attic service user";
        isSystemUser = true;
        group = "attic";
        home = "/var/lib/attic";
        createHome = true;
      };
      users.groups.attic = {};

      # Create initial cache setup
      systemd.services.attic-setup = {
        description = "Attic Cache Setup";
        after = ["atticd.service"];
        wantedBy = ["multi-user.target"];

        environment = {
          ATTICD_SERVER_TOKEN_HS256_SECRET_BASE64 = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==";
        };

        script = ''
          sleep 5  # Wait for atticd to start
          ${pkgs.attic-client}/bin/attic login local http://localhost:${toString port} $ATTICD_SERVER_TOKEN_HS256_SECRET_BASE64 || true
          ${pkgs.attic-client}/bin/attic cache create test || true
        '';

        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          User = "attic";
          Group = "attic";
        };
      };
    }
    // extraConfig;
}
