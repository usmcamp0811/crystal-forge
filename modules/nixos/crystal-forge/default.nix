{
  config,
  lib,
  pkgs,
  ...
}:
with lib; let
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
in {
  options.services.crystal-forge = with lib; {
    enable = mkEnableOption "Enable the Crystal Forge service(s)";
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
      enable = mkEnableOption "Enable the Crystal Forge Server";
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
      enable = mkEnableOption "Enable the Crystal Forge Agent";
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
    # TODO: Add postgres
    systemd.services.crystal-forge-agent = mkIf cfg.client.enable {
      description = "Crystal Forge Agent";
      wantedBy = ["multi-user.target"];
      serviceConfig = {
        ExecStartPre = [configScript];
        ExecStart = "${pkgs.crystal-forge.agent}/bin/agent";
        Environment = ["CRYSTAL_FORGE_CONFIG=${generatedConfigPath}"];
        User = "root";
        Group = "root";
        RuntimeDirectory = "crystal-forge";
      };
    };
    systemd.services.crystal-forge-server = mkIf cfg.server.enable {
      description = "Crystal Forge Server";
      wantedBy = ["multi-user.target"];
      after = "postgresql.service";
      wants = "postgresql.service";
      serviceConfig = {
        ExecStartPre = [configScript];
        ExecStart = "${pkgs.crystal-forge.server}/bin/server";
        Environment = ["CRYSTAL_FORGE_CONFIG=${generatedConfigPath}"];
        User = "crystal_forge";
        Group = "crystal_forge";
        RuntimeDirectory = "crystal-forge";
      };
    };
  };
}
