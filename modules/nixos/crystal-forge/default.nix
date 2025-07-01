{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.crystal-forge;

  tomlFormat = pkgs.formats.toml {};

  rawConfigFile = tomlFormat.generate "crystal-forge-config.toml" (
    lib.mkMerge [
      {
        database = {
          inherit (cfg.database) host user name;
          password =
            if cfg.database.passwordFile != null
            then "__USE_EXTERNAL_PASSWORD__"
            else cfg.database.password;
        };
      }

      # Add server section only if enabled
      (lib.optionalAttrs cfg.server.enable {
        server = {
          inherit (cfg.server) host port;
        };
      })

      # Add client section only if enabled
      (lib.optionalAttrs cfg.client.enable {
        client = {
          inherit (cfg.client) server_host server_port private_key;
        };
      })

      # Always include flakes when present
      (lib.optionalAttrs (cfg.flakes.watched != []) {
        flakes = {
          watched = cfg.flakes.watched;
        };
      })

      # Always include systems when present
      (lib.optionalAttrs (cfg.systems != []) {
        systems = cfg.systems;
      })
    ]
  );

  server = pkgs.writeShellApplication {
    name = "server";
    runtimeInputs = with pkgs; [nix git];
    text = ''
      ${pkgs.crystal-forge.server}/bin/server
    '';
  };

  generatedConfigPath = "/var/lib/crystal_forge/config.toml";

  configScript = pkgs.writeShellApplication {
    name = "generate-crystal-forge-config";
    runtimeInputs = with pkgs; [coreutils gnused];
    text = ''
      set -euo pipefail

      echo "Starting config generation..."
      echo "Source config: ${rawConfigFile}"
      echo "Target config: ${generatedConfigPath}"

      # Ensure target directory exists
      mkdir -p "$(dirname "${generatedConfigPath}")"

      # Copy the base config
      if [ -f "${rawConfigFile}" ]; then
        echo "Copying base config..."
        cp "${rawConfigFile}" "${generatedConfigPath}"
        echo "Base config copied successfully"
      else
        echo "ERROR: Source config file not found: ${rawConfigFile}"
        exit 1
      fi

      # Handle password substitution if needed
      ${lib.optionalString (cfg.database.passwordFile != null) ''
        if [ -f "${cfg.database.passwordFile}" ]; then
          echo "Substituting password from file..."
          PASSWORD=$(cat "${cfg.database.passwordFile}" | sed 's/[&/\]/\\&/g')
          sed -i "s|__USE_EXTERNAL_PASSWORD__|$PASSWORD|" "${generatedConfigPath}"
          echo "Password substitution completed"
        else
          echo "ERROR: Password file not found: ${cfg.database.passwordFile}"
          exit 1
        fi
      ''}

      echo "Config generation completed successfully"
      echo "Final config file:"
      cat "${generatedConfigPath}"
    '';
  };
in {
  options.services.crystal-forge = {
    enable = lib.mkEnableOption "Enable the Crystal Forge service(s)";
    log_level = lib.mkOption {
      type = lib.types.enum ["off" "error" "warn" "info" "debug" "trace"];
      default = "info";
    };
    configPath = lib.mkOption {
      type = lib.types.path;
      default = generatedConfigPath;
      description = "Path to the final config.toml file.";
    };

    local-database = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Enable PostgreSQL setup for Crystal Forge";
    };
    database = {
      host = lib.mkOption {
        type = lib.types.str;
        default = "/run/postgresql";
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
      name = lib.mkOption {
        type = lib.types.str;
        default = "crystal_forge";
      };
    };
    flakes = {
      watched = lib.mkOption {
        type = lib.types.listOf (lib.types.submodule {
          options = {
            name = lib.mkOption {
              type = lib.types.str;
              description = "Name of the flake.";
            };
            repo_url = lib.mkOption {
              type = lib.types.str;
              description = "Repository URL of the flake.";
            };
          };
        });
        default = [];
        description = "List of watched flakes as array of name/repo_url entries.";
      };
    };

    systems = lib.mkOption {
      type = lib.types.listOf (lib.types.submodule {
        options = {
          hostname = lib.mkOption {
            type = lib.types.str;
            description = "Hostname of the system.";
          };
          public_key = lib.mkOption {
            type = lib.types.str;
            description = "Base64-encoded Ed25519 public key.";
          };
          environment = lib.mkOption {
            type = lib.types.str;
            description = "Name of the environment this system belongs to.";
          };
          flake_name = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "Optional flake name referenced from flakes.watched.";
          };
        };
      });
      default = [];
      description = "Systems to register with Crystal Forge.";
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
    services.postgresql = lib.mkIf (cfg.local-database && cfg.server.enable) {
      enable = true;
      ensureDatabases = [cfg.database.name];
      ensureUsers = [
        {
          name = cfg.database.user;
          ensureDBOwnership = true;
          ensureClauses = {login = true;};
        }
      ];
      authentication = lib.concatStringsSep "\n" [
        "host  crystal_forge  crystal_forge  127.0.0.1/32  trust"
        "local  crystal_forge  crystal_forge  trust"
      ];
    };

    systemd.services.crystal-forge-server = lib.mkIf cfg.server.enable {
      description = "Crystal Forge Server";
      wantedBy = ["multi-user.target"];
      after = ["postgresql.service"];
      wants = ["postgresql.service"];
      environment = {
        RUST_LOG = cfg.log_level;
        CRYSTAL_FORGE_CONFIG = generatedConfigPath;
      };
      preStart = ''
        echo "Crystal Forge Server preStart beginning..."
        echo "Config script: ${configScript}/bin/generate-crystal-forge-config"
        ${configScript}/bin/generate-crystal-forge-config
        echo "Crystal Forge Server preStart completed"
      '';
      serviceConfig = {
        ExecStart = "${server}/bin/server";
        User = "root";
        Group = "root";
        RuntimeDirectory = "crystal-forge";
        Restart = "always";
        RestartSec = 5;
      };
    };

    systemd.services.crystal-forge-agent = lib.mkIf cfg.client.enable {
      description = "Crystal Forge Agent";
      wantedBy = ["multi-user.target"];
      after = lib.optional cfg.server.enable "crystal-forge-server.service";
      environment = {
        RUST_LOG = cfg.log_level;
        CRYSTAL_FORGE__CLIENT__SERVER_HOST = cfg.client.server_host;
        CRYSTAL_FORGE__CLIENT__SERVER_PORT = toString cfg.client.server_port;
        CRYSTAL_FORGE__CLIENT__PRIVATE_KEY = cfg.client.private_key;
      };
      serviceConfig = {
        ExecStart = "${pkgs.crystal-forge.agent}/bin/agent";
        User = "root";
        Group = "root";
        RuntimeDirectory = "crystal-forge";
        Restart = "always";
        RestartSec = 5;
      };
    };
  };
}
