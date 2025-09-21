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
          max_concurrent_derivations = cfg.build.max_concurrent_derivations;
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
    // lib.optionalAttrs (cfg.auth.ssh_key_path != null || cfg.auth.netrc_path != null || cfg.auth.ssh_known_hosts_path != null || cfg.auth.ssh_disable_strict_host_checking) {
      auth =
        lib.optionalAttrs (cfg.auth.ssh_key_path != null) {
          ssh_key_path = toString cfg.auth.ssh_key_path;
        }
        // lib.optionalAttrs (cfg.auth.ssh_known_hosts_path != null) {
          ssh_known_hosts_path = toString cfg.auth.ssh_known_hosts_path;
        }
        // lib.optionalAttrs (cfg.auth.netrc_path != null) {
          netrc_path = toString cfg.auth.netrc_path;
        }
        // lib.optionalAttrs cfg.auth.ssh_disable_strict_host_checking {
          ssh_disable_strict_host_checking = cfg.auth.ssh_disable_strict_host_checking;
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
    // lib.optionalAttrs (cfg.cache.push_to != null || cfg.cache.cache_type != "Nix") {
      cache =
        {
          cache_type = cfg.cache.cache_type;
          push_after_build = cfg.cache.push_after_build;
          parallel_uploads = cfg.cache.parallel_uploads;
          max_retries = cfg.cache.max_retries;
          retry_delay_seconds = cfg.cache.retry_delay_seconds;
        }
        // lib.optionalAttrs (cfg.cache.push_to != null) {
          push_to = cfg.cache.push_to;
        }
        // lib.optionalAttrs (cfg.cache.signing_key != null) {
          signing_key = toString cfg.cache.signing_key;
        }
        // lib.optionalAttrs (cfg.cache.compression != null) {
          compression = cfg.cache.compression;
        }
        // lib.optionalAttrs (cfg.cache.push_filter != null) {
          push_filter = cfg.cache.push_filter;
        }
        // lib.optionalAttrs (cfg.cache.s3_region != null) {
          s3_region = cfg.cache.s3_region;
        }
        // lib.optionalAttrs (cfg.cache.s3_profile != null) {
          s3_profile = cfg.cache.s3_profile;
        }
        // lib.optionalAttrs (cfg.cache.attic_token != null) {
          attic_token = cfg.cache.attic_token;
        }
        // lib.optionalAttrs (cfg.cache.attic_cache_name != null) {
          attic_cache_name = cfg.cache.attic_cache_name;
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

    # Inject dynamic attic token from environment file if available and cache_type is Attic
    ${lib.optionalString (cfg.cache.cache_type == "Attic") ''
      if [ -f "/etc/attic-env" ]; then
        echo "Loading Attic token from /etc/attic-env..."
        source /etc/attic-env
        if [ -n "''${ATTIC_TOKEN:-}" ]; then
          echo "Injecting dynamic ATTIC_TOKEN into config..."
          # Use sed to add or update the attic_token field in the [cache] section
          if grep -q "attic_token" "${generatedConfigPath}"; then
            ${pkgs.gnused}/bin/sed -i "s|attic_token = .*|attic_token = \"''${ATTIC_TOKEN}\"|" "${generatedConfigPath}"
          else
            # Find the [cache] section and add attic_token after it
            ${pkgs.gnused}/bin/sed -i '/^\[cache\]/a attic_token = "'"''${ATTIC_TOKEN}"'"' "${generatedConfigPath}"
          fi
          echo "✅ Attic token injected successfully"
        else
          echo "⚠️  ATTIC_TOKEN not found in /etc/attic-env"
        fi
      else
        echo "⚠️  /etc/attic-env not found - using static attic_token from config"
      fi
    ''}

    # Generate SSH keys if ssh_key_path is null and we need SSH auth
    ${lib.optionalString (cfg.auth.ssh_key_path == null && (cfg.build.enable || cfg.server.enable)) ''
      SSH_KEY_PATH="/var/lib/crystal-forge/.ssh/id_ed25519"
      if [ ! -f "$SSH_KEY_PATH" ]; then
        echo "Generating SSH key for Crystal Forge Git authentication..."
        ${pkgs.openssh}/bin/ssh-keygen -t ed25519 -f "$SSH_KEY_PATH" -N "" -C "crystal-forge@$(hostname)"
        chown crystal-forge:crystal-forge "$SSH_KEY_PATH" "$SSH_KEY_PATH.pub"
        chmod 600 "$SSH_KEY_PATH"
        chmod 644 "$SSH_KEY_PATH.pub"
        echo "SSH key generated at $SSH_KEY_PATH"
        echo "Public key for Git repository setup:"
        cat "$SSH_KEY_PATH.pub"
      fi

      # Update config to use generated key path
      ${pkgs.gnused}/bin/sed -i '/\[auth\]/a ssh_key_path = "/var/lib/crystal-forge/.ssh/id_ed25519"' "${generatedConfigPath}"
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
            initial_commit_depth = lib.mkOption {
              type = lib.types.int;
              default = 5;
              description = "How many commits in the past to monitor when initializing the flake monitor";
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

    auth = {
      ssh_key_path = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Path to SSH private key for Git authentication. If null, SSH keys will be generated automatically.";
      };
      ssh_known_hosts_path = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Path to SSH known_hosts file. If null, defaults to /var/lib/crystal-forge/.ssh/known_hosts";
      };
      netrc_path = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Path to .netrc file for HTTPS Git authentication. If null, defaults to /var/lib/crystal-forge/.netrc";
      };
      ssh_disable_strict_host_checking = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Whether to disable strict host key checking for SSH";
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

      max_concurrent_derivations = lib.mkOption {
        type = lib.types.int;
        default = 8;
        description = "Maximum concurrent dry run derivations to process";
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
      cache_type = lib.mkOption {
        type = lib.types.enum ["S3" "Attic" "Http" "Nix"];
        default = "Nix";
        description = "Type of cache to use";
      };
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
      # S3-specific options
      s3_region = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "S3 region for cache";
      };
      s3_profile = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "AWS profile to use for S3 cache";
      };
      # Attic-specific options
      attic_token = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Attic authentication token";
      };
      attic_cache_name = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Attic cache name";
      };
      # Retry configuration
      max_retries = lib.mkOption {
        type = lib.types.ints.unsigned;
        default = 3;
        description = "Maximum retry attempts for cache operations";
      };
      retry_delay_seconds = lib.mkOption {
        type = lib.types.ints.unsigned;
        default = 5;
        description = "Delay between retry attempts in seconds";
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
      "d /var/lib/crystal-forge/.ssh 0700 crystal-forge crystal-forge -"
      "f /var/lib/crystal-forge/config.toml 0600 crystal-forge crystal-forge - -"
      "d /var/lib/crystal-forge/.config 0755 crystal-forge crystal-forge -"
      "d /var/lib/crystal-forge/.config/attic 0755 crystal-forge crystal-forge -"
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
      identMap = ''
        crystal-forge-map crystal-forge ${cfg.database.user}
      '';
      authentication = lib.mkAfter ''
        local  ${cfg.database.name}  ${cfg.database.user}  peer map=crystal-forge-map
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
      polkit.addRule(function(action, subject) {
        if (subject.user === "crystal-forge") {
          if (action.id === "org.freedesktop.systemd1.manage-units" ||
              action.id === "org.freedesktop.systemd1.set-property") {
            return polkit.Result.YES;
          }
        }
      });
    '';

    systemd.services.crystal-forge-builder = lib.mkIf cfg.build.enable (let
      # Parse cfg.build.systemd_properties (["Environment=FOO=bar" "IOWeight=100" …])
      parsed =
        lib.foldl' (
          acc: prop: let
            kv = lib.splitString "=" prop;
            key = lib.elemAt kv 0;
            val = lib.concatStringsSep "=" (lib.drop 1 kv);
          in
            if key == "Environment"
            then acc // {env = (acc.env or []) ++ [val];}
            else acc // {svc = (acc.svc or {}) // {${key} = val;};}
        ) {
          env = [];
          svc = {};
        }
        cfg.build.systemd_properties;

      envFromProps = builtins.listToAttrs (map (
          s: let
            p = lib.splitString "=" s;
          in {
            name = lib.elemAt p 0;
            value = lib.concatStringsSep "=" (lib.drop 1 p);
          }
        )
        parsed.env);
    in {
      description = "Crystal Forge Builder";
      wantedBy = ["multi-user.target"];
      after = lib.optional cfg.local-database "postgresql.service";
      wants = lib.optional cfg.local-database "postgresql.service";

      path = with pkgs;
        [nix git vulnix systemd]
        ++ lib.optional (cfg.cache.cache_type == "Attic") attic-client;

      # Merge existing env with any Environment=… pairs from systemd_properties
      environment = lib.mkMerge [
        {
          RUST_LOG = cfg.log_level;
          NIX_USER_CACHE_DIR = "/var/cache/crystal-forge-nix";
          TMPDIR = "/var/lib/crystal-forge/tmp";
          XDG_RUNTIME_DIR = "/run/crystal-forge";
          XDG_CONFIG_HOME = "/var/lib/crystal-forge/.config";
          HOME = "/var/lib/crystal-forge";
        }
        # Add Attic-specific environment variables if using Attic cache
        (lib.mkIf (cfg.cache.cache_type == "Attic") {
            # Force these to be available even if not in envFromProps
            ATTIC_SERVER_URL = envFromProps.ATTIC_SERVER_URL or "http://atticCache:8080";
            ATTIC_REMOTE_NAME = envFromProps.ATTIC_REMOTE_NAME or "local";
            # Only set ATTIC_TOKEN if it's provided
          }
          // lib.optionalAttrs (envFromProps ? ATTIC_TOKEN) {
            ATTIC_TOKEN = envFromProps.ATTIC_TOKEN;
          })
        envFromProps
      ];

      preStart = ''
        ${configScript}
        mkdir -p /var/lib/crystal-forge/.cache/nix
        mkdir -p /run/crystal-forge
        mkdir -p /var/lib/crystal-forge/.config/attic

        # Ensure attic config directory has proper ownership
        chown -R crystal-forge:crystal-forge /var/lib/crystal-forge/.config

        # Source the attic environment if it exists
        if [ -f /etc/attic-env ]; then
          echo "Loading Attic environment variables from /etc/attic-env"
          set -a
          source /etc/attic-env
          set +a

          # Verify the environment was loaded
          echo "ATTIC_SERVER_URL: ''${ATTIC_SERVER_URL:-NOT_SET}"
          echo "ATTIC_REMOTE_NAME: ''${ATTIC_REMOTE_NAME:-NOT_SET}"
          echo "ATTIC_TOKEN: ''${ATTIC_TOKEN:+SET}"
        fi

        # Test attic configuration as the crystal-forge user
        echo "Testing Attic configuration..."
        runuser -u crystal-forge -- env \
          HOME="/var/lib/crystal-forge" \
          XDG_CONFIG_HOME="/var/lib/crystal-forge/.config" \
          ATTIC_SERVER_URL="''${ATTIC_SERVER_URL:-}" \
          ATTIC_TOKEN="''${ATTIC_TOKEN:-}" \
          ATTIC_REMOTE_NAME="''${ATTIC_REMOTE_NAME:-}" \
          attic login list || echo "Attic configuration test failed"
      '';

      # Splice arbitrary unit properties (e.g., IOWeight=100, TasksMax=3000) parsed above
      serviceConfig =
        {
          Type = "exec";
          ExecStart = builderScript;
          User = "crystal-forge";
          Group = "crystal-forge";
          Slice = "crystal-forge-builds.slice";

          StateDirectory = "crystal-forge";
          StateDirectoryMode = "0750";
          RuntimeDirectory = "crystal-forge";
          RuntimeDirectoryMode = "0700";
          CacheDirectory = "crystal-forge-nix";
          CacheDirectoryMode = "0750";
          WorkingDirectory = "/var/lib/crystal-forge/workdir";

          # Make sure we load the environment file
          EnvironmentFile = [
            "-/etc/attic-env"
            "-/var/lib/crystal-forge/.config/crystal-forge-attic.env"
          ];

          NoNewPrivileges = true;
          ProtectSystem = "no";
          ProtectHome = true;
          PrivateTmp = true;
          ProtectKernelTunables = true;
          ProtectKernelModules = true;
          ProtectControlGroups = true;

          ReadWritePaths = ["/var/lib/crystal-forge" "/tmp" "/run/crystal-forge"];
          ReadOnlyPaths = ["/etc/nix" "/etc/ssl/certs"];

          Restart = "always";
          RestartSec = 5;
        }
        // parsed.svc;
    });

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
        mkdir -p /run/crystal-forge
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
        nix
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
        assertion = cfg.server.enable || cfg.client.enable || cfg.build.enable;
        message = "At least one of server or client must be enabled";
      }
    ];
  };
}
