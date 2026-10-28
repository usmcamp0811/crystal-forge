{config, lib, pkgs, ...}: let
  cfg = config.services.crystal-forge.attic-metrics;
in {
  options.services.crystal-forge.attic-metrics = {
    enable = lib.mkEnableOption "optional provider-side Attic logical inventory";
    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.crystal-forge.attic-metrics;
      description = "Package that provides cf-attic-metrics.";
    };
    databaseUriFile = lib.mkOption {
      type = lib.types.str;
      description = "Absolute runtime path to a protected restricted-role PostgreSQL URI file. Never use a Nix store path.";
    };
    tokensFile = lib.mkOption {
      type = lib.types.str;
      description = "Absolute runtime path to a protected JSON cache-to-token mapping. Restart after changes.";
    };
    port = lib.mkOption {
      type = lib.types.port;
      default = 9121;
      description = "Loopback inventory listener port. The module does not open the firewall.";
    };
  };
  config = lib.mkIf cfg.enable {
    assertions = map (path: {
      assertion = lib.hasPrefix "/" path
        && path != "/nix/store"
        && !lib.hasPrefix "/nix/store/" path
        && !(builtins.elem ".." (lib.splitString "/" path));
      message = "Attic metrics credentials require absolute runtime paths outside the Nix store without parent traversal.";
    }) [cfg.databaseUriFile cfg.tokensFile];
    systemd.services.cf-attic-metrics = {
      description = "Optional Attic logical inventory provider";
      wantedBy = ["multi-user.target"];
      after = ["network.target"];
      serviceConfig = {
        # LoadCredential permissions vary by systemd version. Copy as the
        # service user so the adapter can enforce its owner-only file contract.
        RuntimeDirectory = "cf-attic-metrics";
        RuntimeDirectoryMode = "0700";
        ExecStartPre = [
          "${pkgs.coreutils}/bin/install -m0600 %d/database-uri /run/cf-attic-metrics/database-uri"
          "${pkgs.coreutils}/bin/install -m0600 %d/tokens /run/cf-attic-metrics/tokens"
        ];
        ExecStart = "${lib.getExe cfg.package} --database-uri-file /run/cf-attic-metrics/database-uri --tokens-file /run/cf-attic-metrics/tokens --listen 127.0.0.1 --port ${toString cfg.port}";
        LoadCredential = ["database-uri:${cfg.databaseUriFile}" "tokens:${cfg.tokensFile}"];
        DynamicUser = true;
        Restart = "on-failure";
        RestartSec = 5;
        UMask = "0077";
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        CapabilityBoundingSet = "";
        RestrictAddressFamilies = ["AF_INET" "AF_INET6" "AF_UNIX"];
        IPAddressDeny = "any";
        IPAddressAllow = "localhost";
        MemoryMax = "128M";
        TasksMax = 16;
        LimitNOFILE = 64;
      };
    };
  };
}
