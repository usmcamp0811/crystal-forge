{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.crystal-forge;

  tomlFormat = pkgs.formats.toml {};

  rawConfigFile = tomlFormat.generate "crystal-forge-config.toml" {
    database = {
      inherit (cfg.database) host user dbname;
      password =
        if cfg.database.passwordFile != null
        then "__USE_EXTERNAL_PASSWORD__"
        else cfg.database.password;
    };
    server = {
      inherit (cfg.server) host port authorized_keys;
    };
    client = {
      inherit (cfg.client) server_host server_port private_key;
    };
  };

  generatedConfigPath = "/run/crystal-forge/config.toml";

  configScript = pkgs.writeShellScript "generate-crystal-forge-config" ''
    mkdir -p /run/crsystal-forge
    cp ${rawConfigFile} ${generatedConfigPath}
    ${lib.optionalString (cfg.database.passwordFile != null) ''
      sed -i "s|__USE_EXTERNAL_PASSWORD__|$(<${cfg.database.passwordFile})|" ${generatedConfigPath}
    ''}
  '';
in {
  options.services.crystal-forge = {
    enable = lib.mkEnableOption "Enable the Crystal Forge service(s)";
    configPath = lib.mkOption {
      type = lib.types.path;
      default = generatedConfigPath;
      description = "Path to the final config.toml file.";
    };

    local-database = lib.mkOption {
      type = lib.types.bool;
      default = cfg.server.enable;
      description = "Enable PostgreSQL setup for Crystal Forge";
    };
    database = {
      host = lib.mkOption {
        type = lib.types.str;
        default = "localhost";
      };
      user = lib.mkOption {
        type = lib.types.str;
        default = "crystal_forge";
      };
      password = lib.mkOption {
        type = lib.types.str;
        default = "password";
      };
      passwordFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Optional path to a file containing the DB password.";
      };
      dbname = lib.mkOption {
        type = lib.types.str;
        default = "crystal_forge";
      };
    };

    server = {
      enable = lib.mkEnableOption "Enable the Crystal Forge Server";
      host = lib.mkOption {
        type = lib.types.str;
        default = "0.0.0.0";
      };
      port = lib.mkOption {
        type = lib.types.port;
        default = 3000;
      };
      authorized_keys = lib.mkOption {
        type = lib.types.attrsOf lib.types.str;
        default = {};
      };
    };

    client = {
      enable = lib.mkEnableOption "Enable the Crystal Forge Agent";
      server_host = lib.mkOption {
        type = lib.types.str;
        default = "reckless";
      };
      server_port = lib.mkOption {
        type = lib.types.port;
        default = 3000;
      };
      private_key = lib.mkOption {type = lib.types.path;};
    };
  };

  config = lib.mkIf cfg.enable {
    services.postgresql = {
      enable = cfg.local-database;
      ensureDatabases = [cfg.database.dbname];
      ensureUsers = [
        {
          name = cfg.database.user;
          ensureDBOwnership = true;
          ensureClauses = {
            login = true;
          };
        }
      ];
    };
    systemd.services.crystal-forge-agent = lib.mkIf cfg.client.enable {
      description = "Crystal Forge Agent";
      wantedBy = ["multi-user.target"];
      environment = {
        CRYSTAL_FORGE_CONFIG = "${generatedConfigPath}";
      };
      serviceConfig = {
        ExecStartPre = [configScript];
        ExecStart = "${pkgs.crystal-forge.agent}/bin/agent";
        User = "root";
        Group = "root";
        RuntimeDirectory = "crystal-forge";
      };
    };

    systemd.services.crystal-forge-server = lib.mkIf cfg.server.enable {
      description = "Crystal Forge Server";
      wantedBy = ["multi-user.target"];
      after = ["postgresql.service"];
      wants = ["postgresql.service"];
      environment = {
        CRYSTAL_FORGE_CONFIG = "${generatedConfigPath}";
      };
      serviceConfig = {
        ExecStartPre = [configScript];
        ExecStart = "${pkgs.crystal-forge.server}/bin/server";
        User = "root";
        Group = "root";
        RuntimeDirectory = "crystal-forge";
      };
    };
  };
}
