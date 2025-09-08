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
        script = ''
          ${pkgs.minio-client}/bin/mc alias set local http://localhost:${toString port} ${accessKey} ${secretKey}
          ${pkgs.minio-client}/bin/mc mb local/${bucketName} || true
        '';
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
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

      services.atticd = {
        enable = true;
        credentialsFile = pkgs.writeText "atticd-credentials" ''
          ATTICD_SERVER_TOKEN_HS256_SECRET_BASE64="dGVzdCBzZWNyZXQgZm9yIGF0dGljZA=="
        '';
        settings = {
          listen = "[::]:${toString port}";
          chunking = {
            nar-size-threshold = 64 * 1024;
            min-size = 16 * 1024;
            avg-size = 64 * 1024;
            max-size = 256 * 1024;
          };
        };
      };

      # Create initial cache setup
      systemd.services.attic-setup = {
        after = ["atticd.service"];
        wantedBy = ["multi-user.target"];
        script = ''
          ${pkgs.attic-client}/bin/attic login local http://localhost:${toString port} $ATTICD_SERVER_TOKEN_HS256_SECRET_BASE64
          ${pkgs.attic-client}/bin/attic cache create test || true
        '';
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          EnvironmentFile = pkgs.writeText "attic-env" ''
            ATTICD_SERVER_TOKEN_HS256_SECRET_BASE64=dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==
          '';
        };
      };
    }
    // extraConfig;
}
