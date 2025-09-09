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
      networking.firewall.allowedTCPPorts = lib.mkIf (!enableFirewall) [port consolePort];

      services.minio = {
        enable = true;
        listenAddress = "0.0.0.0:${toString port}";
        consoleAddress = "0.0.0.0:${toString consolePort}";
        rootCredentialsFile = pkgs.writeText "minio-credentials" ''
          MINIO_ROOT_USER=${accessKey}
          MINIO_ROOT_PASSWORD=${secretKey}
        '';
      };

      # Create bucket and setup for nix cache usage
      systemd.services.minio-setup = {
        after = ["minio.service"];
        wantedBy = ["multi-user.target"];

        # Force override the PATH to avoid conflicts
        environment = {
          PATH = lib.mkForce "${pkgs.minio-client}/bin:${pkgs.glibc.bin}/bin:${pkgs.coreutils}/bin:${pkgs.curl}/bin:${pkgs.gnugrep}/bin";
          HOME = "/tmp";
        };

        script = ''
          sleep 3  # Give MinIO time to start

          # Test MinIO is responding
          for i in {1..30}; do
            if curl -s http://localhost:${toString port}/minio/health/live > /dev/null; then
              echo "MinIO is ready"
              break
            fi
            echo "Waiting for MinIO... attempt $i"
            sleep 1
          done

          # Configure MinIO client
          mc alias set local http://localhost:${toString port} ${accessKey} ${secretKey}

          # Create bucket if it doesn't exist
          mc mb local/${bucketName} || echo "Bucket already exists or creation failed"

          # Verify bucket exists
          mc ls local/ | grep ${bucketName} || echo "Bucket verification failed"
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
