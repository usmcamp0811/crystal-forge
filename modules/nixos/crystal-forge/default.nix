{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.crystal-forge;
  tomlFormat = pkgs.formats.toml {};
  postgres_pkg = config.services.postgresql.package;

  baseConfig =
    {
      database = {
        host = cfg.database.host;
        port = cfg.database.port;
        user = cfg.database.user;
        password =
          if cfg.database.passwordFile != null
          then "__PLACEHOLDER_PASSWORD__"
          else cfg.database.password;
        name = cfg.database.name;
      };
    }
    // lib.optionalAttrs cfg.server.enable {
      server = {
        host = cfg.server.host;
        port = cfg.server.port;
      };
    }
    // lib.optionalAttrs cfg.client.enable {
      client = {
        server_host = cfg.client.server_host;
        server_port = cfg.client.server_port;
        private_key = toString cfg.client.private_key;
      };
    }
    // lib.optionalAttrs (cfg.systems != []) {
      systems = cfg.systems;
    }
    // lib.optionalAttrs (cfg.flakes.watched != []) {
      flakes = {
        watched = cfg.flakes.watched;
        flake_polling_interval = cfg.flakes.flake_polling_interval;
        commit_evaluation_interval = cfg.flakes.commit_evaluation_interval;
        build_processing_interval = cfg.flakes.build_processing_interval;
      };
    }
    // lib.optionalAttrs (cfg.environments != []) {
      environments = cfg.environments;
    }
    // lib.optionalAttrs cfg.build.enable {
      build =
        {
          cores = cfg.build.cores;
          max_jobs = cfg.build.max_jobs;
          use_substitutes = cfg.build.use_substitutes;
          offline = cfg.build.offline;
          poll_interval = cfg.build.poll_interval;
          max_silent_time = cfg.build.max_silent_time;
          timeout = cfg.build.timeout;
          sandbox = cfg.build.sandbox;
          use_systemd_scope = cfg.build.use_systemd_scope;
        }
        // lib.optionalAttrs (cfg.build.systemd_memory_max != null) {
          systemd_memory_max = cfg.build.systemd_memory_max;
        }
        // lib.optionalAttrs (cfg.build.systemd_cpu_quota != null) {
          systemd_cpu_quota = cfg.build.systemd_cpu_quota;
        }
        // lib.optionalAttrs (cfg.build.systemd_timeout_stop_sec != null) {
          systemd_timeout_stop_sec = cfg.build.systemd_timeout_stop_sec;
        }
        // lib.optionalAttrs (cfg.build.systemd_properties != []) {
          systemd_properties = cfg.build.systemd_properties;
        };
    }
    // {
      vulnix =
        {
          timeout = cfg.vulnix.timeout;
          max_retries = cfg.vulnix.max_retries;
          enable_whitelist = cfg.vulnix.enable_whitelist;
          extra_args = cfg.vulnix.extra_args;
          poll_interval = cfg.vulnix.poll_interval;
        }
        // lib.optionalAttrs (cfg.vulnix.whitelist_path != null) {
          whitelist_path = toString cfg.vulnix.whitelist_path;
        };
    }
    // lib.optionalAttrs (cfg.cache.push_to != null) {
      cache =
        {
          push_to = cfg.cache.push_to;
          push_after_build = cfg.cache.push_after_build;
          compression = cfg.cache.compression;
          push_filter = cfg.cache.push_filter;
          parallel_uploads = cfg.cache.parallel_uploads;
        }
        // lib.optionalAttrs (cfg.cache.signing_key != null) {
          signing_key = toString cfg.cache.signing_key;
        };
    };

  rawConfigFile = tomlFormat.generate "crystal-forge-config.toml" baseConfig;
  generatedConfigPath = "/var/lib/crystal-forge/config.toml";

  configScript = pkgs.writeShellScript "generate-crystal-forge-config" ''
    set -euo pipefail
    mkdir -p "$(dirname "${generatedConfigPath}")"
    cp "${rawConfigFile}" "${generatedConfigPath}"

    ${lib.optionalString (cfg.database.passwordFile != null) ''
      if [ -f "${cfg.database.passwordFile}" ]; then
        PASSWORD=$(cat "${cfg.database.passwordFile}")
        ${pkgs.gnused}/bin/sed -i "s|__PLACEHOLDER_PASSWORD__|''${PASSWORD}|" "${generatedConfigPath}"
      else
        echo "ERROR: Password file not found: ${cfg.database.passwordFile}" >&2
        exit 1
      fi
    ''}

    chmod 600 "${generatedConfigPath}"
  '';

  serverScript = pkgs.writeShellScript "crystal-forge-server" ''
    export CRYSTAL_FORGE_CONFIG="${generatedConfigPath}"
    exec ${pkgs.crystal-forge.server}/bin/server "$@"
  '';

  builderScript = pkgs.writeShellScript "crystal-forge-builder" ''
    set -euo pipefail
    export CRYSTAL_FORGE_CONFIG="${generatedConfigPath}"
    export TMPDIR="/var/lib/crystal-forge/tmp"
    export HOME="/var/lib/crystal-forge"

    cleanup_old_builds() {
      find /var/lib/crystal-forge/workdir -name "result*" -type l -mtime +1 -delete 2>/dev/null || true
      find /var/lib/crystal-forge/tmp -type f -mtime +1 -delete 2>/dev/null || true
    }
    trap cleanup_old_builds EXIT INT TERM

    mkdir -p /var/lib/crystal-forge/workdir
    cd /var/lib/crystal-forge/workdir
    cleanup_old_builds

    export NIX_BUILD_CORES="${toString cfg.build.cores}"
    export NIX_MAX_JOBS="${toString cfg.build.max_jobs}"
    ulimit -v $((4 * 1024 * 1024)) # 4GiB

    exec ${pkgs.crystal-forge.server}/bin/builder "$@"
  '';

  agentScript = pkgs.writeShellScript "crystal-forge-agent" ''
    export CRYSTAL_FORGE_CONFIG="${generatedConfigPath}"
    exec ${pkgs.crystal-forge.agent}/bin/agent "$@"
  '';
in {
  options.services.crystal-forge = {
    enable = lib.mkEnableOption "Crystal Forge service(s)";

    log_level = lib.mkOption {
      type = lib.types.enum ["off" "error" "warn" "info" "debug" "trace"];
      default = "info";
      description = "Log level for Crystal Forge services";
    };

    configPath = lib.mkOption {
      type = lib.types.path;
      default = generatedConfigPath;
      readOnly = true;
      description = "Path to the generated config.toml file";
    };

    local-database = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Enable local PostgreSQL setup for Crystal Forge";
    };

    database = {
      host = lib.mkOption {
        type = lib.types.str;
        default = "/run/postgresql";
        description = "Database host (use socket path for local connections)";
      };
      port = lib.mkOption {
        type = lib.types.port;
        default = 5432;
        description = "Database port";
      };
      user = lib.mkOption {
        type = lib.types.str;
        default = "crystal_forge";
        description = "Database user";
      };
      password = lib.mkOption {
        type = lib.types.str;
        default = "password";
        description = "Database password (only used if passwordFile is null)";
      };
      passwordFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Path to file containing database password";
      };
      name = lib.mkOption {
        type = lib.types.str;
        default = "crystal_forge";
        description = "Database name";
      };
    };

    flakes = {
      watched = lib.mkOption {
        type = lib.types.listOf (lib.types.submodule {
          options = {
            name = lib.mkOption {
              type = lib.types.str;
              description = "Name identifier for the flake";
            };
            repo_url = lib.mkOption {
              type = lib.types.str;
              description = "Repository URL of the flake";
            };
            auto_poll = lib.mkOption {
              type = lib.types.bool;
              description = "Automatically poll for new commits";
            };
          };
        });
        default = [];
        description = "List of flakes to watch for changes";
        example = [
          {
            name = "dotfiles";
            repo_url = "git+https://gitlab.com/usmcamp0811/dotfiles";
            auto_poll = false;
          }
        ];
      };
      flake_polling_interval = lib.mkOption {
        type = lib.types.str;
        default = "10m";
        description = "Interval between flake polling checks";
      };
      commit_evaluation_interval = lib.mkOption {
        type = lib.types.str;
        default = "1m";
        description = "Interval between commit evaluation checks";
      };
      build_processing_interval = lib.mkOption {
        type = lib.types.str;
        default = "1m";
        description = "Interval between build processing checks";
      };
    };

    build = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = cfg.server.enable;
        description = "Crystal Forge Builder";
      };
      cores = lib.mkOption {
        type = lib.types.ints.positive;
        default = 1;
        description = "Maximum CPU cores to use per build job";
      };
      max_jobs = lib.mkOption {
        type = lib.types.ints.positive;
        default = 1;
        description = "Maximum number of concurrent build jobs";
      };
      use_substitutes = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to use binary substitutes/caches";
      };
      offline = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Build in offline mode (no network access)";
      };
      poll_interval = lib.mkOption {
        type = lib.types.str;
        default = "5m";
        description = "Interval between checking for new build jobs";
      };
      max_silent_time = lib.mkOption {
        type = lib.types.str;
        default = "1h";
        description = "Maximum time a build can be silent before timing out";
      };
      timeout = lib.mkOption {
        type = lib.types.str;
        default = "2h";
        description = "Maximum total time for a build before timing out";
      };
      sandbox = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Enable sandbox for builds";
      };

      # Systemd resource controls
      use_systemd_scope = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to use systemd-run for resource isolation";
      };
      systemd_memory_max = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = "32G";
        description = "Memory limit for systemd scope (e.g., '4G', '2048M')";
      };
      systemd_cpu_quota = lib.mkOption {
        type = lib.types.nullOr lib.types.ints.positive;
        default = 800;
        description = "CPU quota as percentage (e.g., 300 for 3 cores worth)";
      };
      systemd_timeout_stop_sec = lib.mkOption {
        type = lib.types.nullOr lib.types.ints.positive;
        default = 600;
        description = "Timeout for systemd scope stop operation in seconds";
      };
      systemd_properties = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [
          "MemorySwapMax=2G"
          "TasksMax=3000"
        ];
        description = "Additional systemd properties to set";
        example = [
          "MemorySwapMax=2G"
          "TasksMax=3000"
          "IOWeight=100"
        ];
      };
    };

    vulnix = {
      timeout = lib.mkOption {
        type = lib.types.str;
        default = "5m";
        description = "Timeout for vulnix scans";
      };
      max_retries = lib.mkOption {
        type = lib.types.ints.unsigned;
        default = 5;
        description = "Max retries";
      };
      enable_whitelist = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Enable whitelist";
      };
      extra_args = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [];
        description = "Extra args";
      };
      whitelist_path = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Whitelist path";
      };
      poll_interval = lib.mkOption {
        type = lib.types.str;
        default = "1m";
        description = "Polling interval for CVE jobs";
      };
    };

    cache = {
      push_to = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Cache URI to push to";
      };
      push_after_build = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Push after build";
      };
      signing_key = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Signing key path";
      };
      compression = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Compression method";
      };
      push_filter = lib.mkOption {
        type = lib.types.nullOr (lib.types.listOf lib.types.str);
        default = null;
        description = "Push filter";
      };
      parallel_uploads = lib.mkOption {
        type = lib.types.ints.positive;
        default = 4;
        description = "Parallel uploads";
      };
    };

    systems = lib.mkOption {
      type = lib.types.listOf (lib.types.submodule {
        options = {
          hostname = lib.mkOption {
            type = lib.types.str;
            description = "System hostname";
          };
          public_key = lib.mkOption {
            type = lib.types.str;
            description = "Base64-encoded Ed25519 public key";
          };
          environment = lib.mkOption {
            type = lib.types.str;
            description = "Environment name";
          };
          flake_name = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "Flake ref name";
          };
        };
      });
      default = [];
      description = "Systems to register with Crystal Forge";
      example = [
        {
          hostname = "myhost";
          public_key = "base64encodedkey";
          environment = "production";
          flake_name = "dotfiles";
        }
      ];
    };

    environments = lib.mkOption {
      type = lib.types.listOf (lib.types.submodule {
        options = {
          name = lib.mkOption {
            type = lib.types.str;
            description = "Environment name";
          };
          description = lib.mkOption {
            type = lib.types.str;
            description = "Description";
          };
          is_active = lib.mkOption {
            type = lib.types.bool;
            description = "Active flag";
          };
          risk_profile = lib.mkOption {
            type = lib.types.str;
            description = "Risk profile";
          };
          compliance_level = lib.mkOption {
            type = lib.types.str;
            description = "Compliance level";
          };
        };
      });
      default = [];
      description = "List of environments";
      example = [
        {
          name = "dev";
          description = "Development environment for Crystal Forge agents and evaluation";
          is_active = true;
          risk_profile = "LOW";
          compliance_level = "NONE";
        }
      ];
    };

    server = {
      enable = lib.mkEnableOption "Crystal Forge Server";
      host = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1";
        description = "Server bind address";
      };
      port = lib.mkOption {
        type = lib.types.port;
        default = 3000;
        description = "Server port";
      };
    };

    client = {
      enable = lib.mkEnableOption "Crystal Forge Agent";
      server_host = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1";
        description = "Server hostname";
      };
      server_port = lib.mkOption {
        type = lib.types.port;
        default = 3000;
        description = "Server port";
      };
      private_key = lib.mkOption {
        type = lib.types.path;
        description = "Path to Ed25519 private key file";
      };
    };
  };

  config = lib.mkIf cfg.enable {
    nix.settings = lib.mkIf (cfg.server.enable || cfg.build.enable) {
      experimental-features = ["nix-command" "flakes"];
      allowed-users = ["root" "crystal-forge"];
      trusted-users = ["root" "crystal-forge"];
    };

    users.users.crystal-forge = lib.mkIf (cfg.server.enable || cfg.build.enable) {
      description = "Crystal Forge service user";
      isSystemUser = true;
      group = "crystal-forge";
      home = "/var/lib/crystal-forge";
      createHome = true;
      # extraGroups is optional for daemon-style nix; keep empty unless needed.
      # extraGroups = [ "nixbld" ];
    };

    users.groups.crystal-forge = {};

    systemd.tmpfiles.rules = [
      "d /var/lib/crystal-forge 0755 crystal-forge crystal-forge -"
      "d /var/lib/crystal-forge/.cache 0755 crystal-forge crystal-forge -"
      "d /var/lib/crystal-forge/.cache/nix 0755 crystal-forge crystal-forge -"
      "d /var/lib/crystal-forge/tmp 0755 crystal-forge crystal-forge -"
      "d /var/lib/crystal-forge/builds 0755 crystal-forge crystal-forge -"
      "d /var/lib/crystal-forge/workdir 0755 crystal-forge crystal-forge -"
      "f /var/lib/crystal-forge/config.toml 0600 crystal-forge crystal-forge - -"
    ];

    systemd.slices.crystal-forge-builds = lib.mkIf cfg.build.enable {
      description = "Crystal Forge Build Operations";
      sliceConfig = {
        # Use build config values or sensible defaults for the slice
        MemoryMax =
          lib.mkIf (cfg.build.systemd_memory_max != null)
          (cfg.build.systemd_memory_max + ""); # Ensure string conversion
        MemoryHigh =
          lib.mkIf (cfg.build.systemd_memory_max != null)
          (let
            # Calculate 75% of max memory for "high" threshold
            memStr = cfg.build.systemd_memory_max;
            memVal =
              if lib.hasSuffix "G" memStr
              then toString (lib.toInt (lib.removeSuffix "G" memStr) * 3 / 4) + "G"
              else if lib.hasSuffix "M" memStr
              then toString (lib.toInt (lib.removeSuffix "M" memStr) * 3 / 4) + "M"
              else memStr;
          in
            memVal);
        CPUQuota =
          lib.mkIf (cfg.build.systemd_cpu_quota != null)
          (toString cfg.build.systemd_cpu_quota + "%");
        TasksMax = "200"; # Keep this as a reasonable default
      };
    };

    services.postgresql = lib.mkIf (cfg.local-database && cfg.server.enable) {
      enable = true;
      ensureDatabases = [cfg.database.name];
      ensureUsers = [
        {
          name = cfg.database.user;
          ensureDBOwnership = true;
          ensureClauses.login = true;
        }
      ];
      authentication = lib.mkAfter ''
        local  ${cfg.database.name}  ${cfg.database.user}  trust
        host   ${cfg.database.name}  ${cfg.database.user}  127.0.0.1/32  trust
        host   ${cfg.database.name}  ${cfg.database.user}  ::1/128       trust
      '';
    };

    systemd.services."crystal-forge-postgres-jobs" = lib.mkIf cfg.server.enable {
      description = "Crystal Forge Postgres Jobs";
      after = ["postgresql.service"];
      wantedBy = ["multi-user.target"];

      serviceConfig = {
        Type = "oneshot";
        User = "crystal-forge";
        Group = "crystal-forge";
        StateDirectory = "crystal-forge";
        RuntimeDirectory = "crystal-forge";
        CacheDirectory = "crystal-forge-nix";
      };

      environment = {
        DB_HOST = cfg.database.host;
        DB_PORT = toString cfg.database.port;
        DB_NAME = cfg.database.name;
        DB_USER = cfg.database.user;
        DB_PASSWORD = lib.mkIf (cfg.database.passwordFile == null) cfg.database.password;
        JOB_DIR = "${pkgs.crystal-forge.run-postgres-jobs}/jobs";
      };

      script =
        lib.optionalString (cfg.database.passwordFile != null) ''
          export DB_PASSWORD="$(cat ${cfg.database.passwordFile})"
          exec ${pkgs.crystal-forge.run-postgres-jobs}/bin/run-postgres-jobs
        ''
        + lib.optionalString (cfg.database.passwordFile == null) ''
          exec ${pkgs.crystal-forge.run-postgres-jobs}/bin/run-postgres-jobs
        '';
    };

    systemd.timers."crystal-forge-postgres-jobs" = lib.mkIf cfg.server.enable {
      description = "Run Crystal Forge Postgres Jobs daily at midnight";
      wantedBy = ["timers.target"];
      timerConfig = {
        OnCalendar = "*-*-* 00:00:00";
        Persistent = true;
      };
    };

    security.polkit.enable = true;
    security.polkit.extraConfig = ''
      // Allow only crystal-forge to start/stop *its* transient scopes.
      // Use: systemd-run --unit=cf-build-%j --scope ...
      polkit.addRule(function(action, subject) {
        if (action.id == "org.freedesktop.systemd1.manage-units"
            && subject.user == "crystal-forge") {
          var unit = action.lookup("unit");
          var verb = action.lookup("verb");
          if (unit && unit.indexOf("cf-build-") === 0 && unit.slice(-6) === ".scope"
              && (verb == "start" || verb == "stop")) {
            return polkit.Result.YES;
          }
        }
      });
    '';

    systemd.services.crystal-forge-builder = lib.mkIf cfg.build.enable {
      description = "Crystal Forge Builder";
      wantedBy = ["multi-user.target"];
      after = lib.optional cfg.local-database "postgresql.service";
      wants = lib.optional cfg.local-database "postgresql.service";

      path = with pkgs; [nix git vulnix systemd];
      environment = {
        RUST_LOG = cfg.log_level;
        NIX_USER_CACHE_DIR = "/var/lib/crystal-forge/.cache/nix";
        TMPDIR = "/var/lib/crystal-forge/tmp";
        XDG_RUNTIME_DIR = "/run/crystal-forge";
      };

      preStart = ''
        ${configScript}
        mkdir -p /var/lib/crystal-forge/.cache/nix
      '';

      serviceConfig = {
        Type = "exec";
        ExecStart = builderScript;
        User = "crystal-forge";
        Group = "crystal-forge";

        Slice = "crystal-forge-builds.slice";

        # Use individual service limits that are more conservative than slice limits
        MemoryMax =
          lib.mkIf (cfg.build.systemd_memory_max != null)
          (let
            # Use half of the systemd memory max for individual service
            memStr = cfg.build.systemd_memory_max;
            memVal =
              if lib.hasSuffix "G" memStr
              then toString (lib.toInt (lib.removeSuffix "G" memStr) / 2) + "G"
              else if lib.hasSuffix "M" memStr
              then toString (lib.toInt (lib.removeSuffix "M" memStr) / 2) + "M"
              else "2G";
          in
            memVal);
        MemoryHigh =
          lib.mkIf (cfg.build.systemd_memory_max != null)
          (let
            # Use 37.5% of systemd memory max for "high" threshold
            memStr = cfg.build.systemd_memory_max;
            memVal =
              if lib.hasSuffix "G" memStr
              then toString (lib.toInt (lib.removeSuffix "G" memStr) * 3 / 8) + "G"
              else if lib.hasSuffix "M" memStr
              then toString (lib.toInt (lib.removeSuffix "M" memStr) * 3 / 8) + "M"
              else "1.5G";
          in
            memVal);
        MemorySwapMax = "1G";
        CPUQuota =
          lib.mkIf (cfg.build.systemd_cpu_quota != null)
          (toString (cfg.build.systemd_cpu_quota / 2) + "%"); # Half of the slice quota

        # Use systemd-managed dirs (no hardcoded IDs)
        StateDirectory = "crystal-forge";
        StateDirectoryMode = "0750";
        RuntimeDirectory = "crystal-forge";
        RuntimeDirectoryMode = "0700";
        CacheDirectory = "crystal-forge-nix";
        CacheDirectoryMode = "0750";
        WorkingDirectory = "/var/lib/crystal-forge/workdir";

        NoNewPrivileges = true;
        ProtectSystem = "no";
        ProtectHome = true;
        PrivateTmp = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;

        # Only the required write paths
        ReadWritePaths = [
          "/var/lib/crystal-forge"
          "/tmp"
          "/run/crystal-forge"
        ];

        ReadOnlyPaths = [
          "/etc/nix"
          "/etc/ssl/certs"
        ];

        Restart = "always";
        RestartSec = 5;
      };
    };

    systemd.services.crystal-forge-server = lib.mkIf cfg.server.enable {
      description = "Crystal Forge Server";
      wantedBy = ["multi-user.target"];
      after = lib.optional cfg.local-database "postgresql.service";
      wants = lib.optional cfg.local-database "postgresql.service";

      path = with pkgs; [nix git];
      environment = {
        RUST_LOG = cfg.log_level;
        NIX_USER_CACHE_DIR = "/var/lib/crystal-forge/.cache/nix";
      };

      preStart = ''
        ${configScript}
        mkdir -p /var/lib/crystal-forge/.cache/nix
      '';

      serviceConfig = {
        Type = "exec";
        ExecStart = serverScript;
        User = "crystal-forge";
        Group = "crystal-forge";

        StateDirectory = "crystal-forge";
        StateDirectoryMode = "0750";
        RuntimeDirectory = "crystal-forge";
        RuntimeDirectoryMode = "0700";
        CacheDirectory = "crystal-forge-nix";
        CacheDirectoryMode = "0750";
        WorkingDirectory = "/var/lib/crystal-forge";

        NoNewPrivileges = true;
        ProtectSystem = "no";
        ProtectHome = true;
        ReadWritePaths = ["/var/lib/crystal-forge"];
        PrivateTmp = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;

        Restart = "always";
        RestartSec = 5;
      };
    };

    systemd.services.crystal-forge-agent = lib.mkIf cfg.client.enable {
      description = "Crystal Forge Agent";
      wantedBy = ["multi-user.target"];
      after = lib.optional cfg.server.enable "crystal-forge-server.service";

      path = with pkgs; [
        coreutils
        zfs
        util-linux
        iproute2
        nettools
        pciutils
        usbutils
        dmidecode
        procps
        parted
        systemd
        gawk
        gnused
        gnugrep
        findutils
        vulnix
      ];
      environment = {
        RUST_LOG = cfg.log_level;
        CRYSTAL_FORGE__CLIENT__SERVER_HOST = cfg.client.server_host;
        CRYSTAL_FORGE__CLIENT__SERVER_PORT = toString cfg.client.server_port;
        CRYSTAL_FORGE__CLIENT__PRIVATE_KEY = cfg.client.private_key;
      };

      serviceConfig = {
        Type = "exec";
        ExecStart = agentScript;
        User = "root";
        Group = "root";

        StateDirectory = "crystal-forge";
        StateDirectoryMode = "0750";
        RuntimeDirectory = "crystal-forge";
        RuntimeDirectoryMode = "0700";
        WorkingDirectory = "/var/lib/crystal-forge";

        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ReadWritePaths = ["/var/lib/crystal-forge"];
        PrivateTmp = true;

        Restart = "always";
        RestartSec = 5;
      };
    };

    assertions = [
      {
        assertion = cfg.client.enable -> (cfg.client.private_key != null);
        message = "Crystal Forge client requires a private key file";
      }
      {
        assertion = cfg.database.passwordFile != null -> cfg.database.password == "";
        message = "Cannot specify both database.password and database.passwordFile";
      }
      {
        assertion = cfg.server.enable || cfg.client.enable;
        message = "At least one of server or client must be enabled";
      }
    ];
  };
}
