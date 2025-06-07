{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.crystal-forge;

  rawConfigFile = pkgs.writeText "crystal-forge-config.toml" (lib.generators.toTOML {} {
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
  });

  generatedConfigPath = "/run/crystal-forge/config.toml";

  configScript = pkgs.writeShellScript "generate-crystal-forge-config" ''
    install -d -m 0750 -o crystal_forge /run/crystal-forge
    cp ${rawConfigFile} ${generatedConfigPath}
    ${lib.optionalString (cfg.database.passwordFile != null) ''
      sed -i "s|__USE_EXTERNAL_PASSWORD__|$(<${cfg.database.passwordFile})|" ${generatedConfigPath}
    ''}
  '';

  mkService = role: {
    description = "Crystal Forge ${role}";
    wantedBy = ["multi-user.target"];
    after = lib.optional (role == "server") "postgresql.service";
    wants = lib.optional (role == "server") "postgresql.service";
    serviceConfig = {
      ExecStartPre = [configScript];
      ExecStart =
        if role == "server"
        then "${pkgs.crystal-forge.server}/bin/server"
        else "${pkgs.crystal-forge.agent}/bin/agent";
      Environment = ["CRYSTAL_FORGE_CONFIG=${generatedConfigPath}"];
      User =
        if role == "server"
        then "crystal_forge"
        else "root";
      Group =
        if role == "server"
        then "crystal_forge"
        else "root";
      RuntimeDirectory = "crystal-forge";
    };
  };
in {
  options.services.crystal-forge = with lib; {
    enable = mkEnableOption "Enable the Crystal Forge service(s)";
    roles = mkOption {
      type = types.listOf (types.enum ["agent" "server"]);
      default = ["agent"];
      description = "Which roles to run on this system.";
    };
    configPath = mkOption {
      type = types.path;
      default = generatedConfigPath;
      description = "Path to the final config.toml file.";
    };
    database = {
      host = mkOption {
        type = types.str;
        default = "localhost";
      };
      user = mkOption {
        type = types.str;
        default = "crystal_forge";
      };
      password = mkOption {
        type = types.str;
        default = "password";
      };
      passwordFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Optional path to a file containing the database password. Overrides 'password'.";
      };
      dbname = mkOption {
        type = types.str;
        default = "crystal_forge";
      };
    };
    server = {
      host = mkOption {
        type = types.str;
        default = "0.0.0.0";
      };
      port = mkOption {
        type = types.port;
        default = 3000;
      };
      authorized_keys = mkOption {
        type = types.attrsOf types.str;
        default = {};
      };
    };
    client = {
      server_host = mkOption {
        type = types.str;
        default = "reckless";
      };
      server_port = mkOption {
        type = types.port;
        default = 3000;
      };
      private_key = mkOption {type = types.path;};
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services = lib.mkMerge (map (
        role: {"crystal-forge-${role}" = mkService role;}
      )
      cfg.roles);
  };
}
