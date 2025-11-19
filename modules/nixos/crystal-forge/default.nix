{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.crystal-forge;
  tomlFormat = pkgs.formats.toml {};
  postgres_pkg = config.services.postgresql.package;

  # Recursively remove any null values so TOML generation won’t choke.
  stripNulls = v:
    if builtins.isAttrs v
    then lib.filterAttrs (_: vv: vv != null) (lib.mapAttrs (_: stripNulls) v)
    else if builtins.isList v
    then lib.filter (x: x != null) (map stripNulls v)
    else v;

  # Build the raw config first (your existing baseConfig logic, unchanged)
  baseConfigRaw =
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
        eval_workers = cfg.server.eval_workers;
        eval_max_memory_mb = cfg.server.eval_max_memory_mb;
        eval_check_cache = cfg.server.eval_check_cache;
      };
    }
    // lib.optionalAttrs cfg.client.enable {
      client = {
        server_host = cfg.client.server_host;
        server_port = cfg.client.server_port;
        private_key = toString cfg.client.private_key;
      };
    }
    // lib.optionalAttrs (cfg.deployment.cache_url != null || cfg.deployment.max_deployment_age_minutes != 30 || !cfg.deployment.dry_run_first || cfg.deployment.fallback_to_local_build || cfg.deployment.deployment_timeout_minutes != 60 || cfg.deployment.deployment_poll_interval != "15m") {
      deployment =
        {
          max_deployment_age_minutes = cfg.deployment.max_deployment_age_minutes;
          dry_run_first = cfg.deployment.dry_run_first;
          fallback_to_local_build = cfg.deployment.fallback_to_local_build;
          deployment_timeout_minutes = cfg.deployment.deployment_timeout_minutes;
          deployment_poll_interval = cfg.deployment.deployment_poll_interval;
          require_sigs = cfg.deployment.require_sigs;
        }
        // lib.optionalAttrs (cfg.deployment.cache_url != null) {
          cache_url = cfg.deployment.cache_url;
          cache_public_key = cfg.deployment.cache_public_key;
        };
    }
    // lib.optionalAttrs (cfg.systems != []) {
      # NOTE: systems’ items can include null fields by default (e.g., flake_name, desired_target, server_public_key)
      # We’ll strip them globally via stripNulls below.
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
          # New concurrency control
          max_concurrent_derivations = cfg.build.max_concurrent_derivations;
          max_jobs = cfg.build.max_jobs;
          cores_per_job = cfg.build.cores_per_job;

          # Binary cache and network
          use_substitutes = cfg.build.use_substitutes;
          offline = cfg.build.offline;

          # Timing
          poll_interval = cfg.build.poll_interval;
          max_silent_time = cfg.build.max_silent_time;
          timeout = cfg.build.timeout;

          # Security
          sandbox = cfg.build.sandbox;

          # Systemd isolation
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
          force_repush = cfg.cache.force_repush;
          require_sigs = cfg.deployment.require_sigs;
          attic_ignore_upstream_cache_filter = cfg.cache.attic_ignore_upstream_cache_filter;
          attic_jobs = cfg.cache.attic_jobs;
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

  # Now sanitize away any nulls before TOML generation
  baseConfig = stripNulls baseConfigRaw;

  rawConfigFile = tomlFormat.generate "crystal-forge-config.toml" baseConfig;

  serverConfigPath = "/var/lib/crystal-forge/config.toml";
  agentConfigPath = "/var/lib/crystal-forge-agent/config.toml";

  makeConfigScript = destPath:
    pkgs.writeShellScript "generate-crystal-forge-config-${lib.replaceStrings ["/" "."] ["-" "-"] destPath}" ''
      set -euo pipefail
      generatedConfigPath="${destPath}"

      mkdir -p "$(dirname "$generatedConfigPath")"
      cp "${rawConfigFile}" "$generatedConfigPath"

      ${lib.optionalString (cfg.database.passwordFile != null) ''
        if [ -f "${cfg.database.passwordFile}" ]; then
          PASSWORD=$(cat "${cfg.database.passwordFile}")
          ${pkgs.gnused}/bin/sed -i "s|__PLACEHOLDER_PASSWORD__|$PASSWORD|" "$generatedConfigPath"
        else
          echo "ERROR: Password file not found: ${cfg.database.passwordFile}" >&2
          exit 1
        fi
      ''}

      ${lib.optionalString (cfg.cache.cache_type == "Attic") ''
        if [ -f "${cfg.env-file}" ]; then
          echo "Loading Attic token from ${cfg.env-file}..."
          # shellcheck disable=SC1090
          source ${cfg.env-file}
          if [ -n "$ATTIC_TOKEN" ]; then
            echo "Injecting dynamic ATTIC_TOKEN into config..."
            if grep -q "attic_token" "$generatedConfigPath"; then
              ${pkgs.gnused}/bin/sed -i 's|attic_token = .*|attic_token = "'"$ATTIC_TOKEN"'"|' "$generatedConfigPath"
            else
              ${pkgs.gnused}/bin/sed -i '/^\[cache\]/a attic_token = "'"$ATTIC_TOKEN"'"' "$generatedConfigPath"
            fi
            echo "✅ Attic token injected successfully"
          else
            echo "⚠️  ATTIC_TOKEN not found in ${cfg.env-file}"
          fi
        else
          echo "⚠️  ${cfg.env-file} not found - using static attic_token from config"
        fi
      ''}

      ${lib.optionalString (cfg.auth.ssh_key_path == null && (cfg.build.enable || cfg.server.enable)) ''
        SSH_KEY_PATH="/var/lib/crystal-forge/.ssh/id_ed25519"
        if [ ! -f "$SSH_KEY_PATH" ]; then
          echo "Generating SSH key for Crystal Forge Git authentication..."
          ${pkgs.openssh}/bin/ssh-keygen -t ed25519 -f "$SSH_KEY_PATH" -N "" -C "crystal-forge@$(${pkgs.nettools}/bin/hostname)"
          chown crystal-forge:crystal-forge "$SSH_KEY_PATH" "$SSH_KEY_PATH.pub"
          chmod 600 "$SSH_KEY_PATH"
          chmod 644 "$SSH_KEY_PATH.pub"
          echo "SSH key generated at $SSH_KEY_PATH"
          echo "Public key for Git repository setup:"
          cat "$SSH_KEY_PATH.pub"
        fi

        ${pkgs.gnused}/bin/sed -i '/\[auth\]/a ssh_key_path = "/var/lib/crystal-forge/.ssh/id_ed25519"' "$generatedConfigPath"
      ''}

      chmod 600 "$generatedConfigPath"
    '';

  configScriptServer = makeConfigScript serverConfigPath;
  configScriptAgent = makeConfigScript agentConfigPath;

  serverScript = pkgs.writeShellScript "crystal-forge-server" ''
    export CRYSTAL_FORGE_CONFIG="${serverConfigPath}"
    exec ${pkgs.crystal-forge.default.server}/bin/server "$@"
  '';

  builderScript = pkgs.writeShellScript "crystal-forge-builder" ''
    set -euo pipefail
    export CRYSTAL_FORGE_CONFIG="${serverConfigPath}"
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
    exec ${pkgs.crystal-forge.default.server}/bin/builder "$@"
  '';

  agentScript = pkgs.writeShellScript "crystal-forge-agent" ''
    export CRYSTAL_FORGE_CONFIG="${agentConfigPath}"
    exec ${pkgs.crystal-forge.default.agent}/bin/agent "$@"
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
      default = serverConfigPath;
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

    dashboards = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = lib.mdDoc ''
          Enable Crystal Forge Grafana dashboards.

          This will:
          - Enable Grafana if not already enabled
          - Configure a PostgreSQL datasource for Crystal Forge
          - Provision the Crystal Forge monitoring dashboard

          Can be enabled on any host (with or without server/builder).
        '';
      };

      datasource = {
        name = lib.mkOption {
          type = lib.types.str;
          default = "Crystal Forge PostgreSQL";
          description = "Name for the Grafana datasource";
        };

        host = lib.mkOption {
          type = lib.types.str;
          default = cfg.database.host;
          defaultText = lib.literalExpression "config.services.crystal-forge.database.host";
          description = "PostgreSQL host for Grafana to connect to";
        };

        port = lib.mkOption {
          type = lib.types.port;
          default = cfg.database.port;
          defaultText = lib.literalExpression "config.services.crystal-forge.database.port";
          description = "PostgreSQL port for Grafana to connect to";
        };

        database = lib.mkOption {
          type = lib.types.str;
          default = cfg.database.name;
          defaultText = lib.literalExpression "config.services.crystal-forge.database.name";
          description = "Database name for Grafana to connect to";
        };

        user = lib.mkOption {
          type = lib.types.str;
          default = "grafana";
          description = "Database user for Grafana datasource";
        };

        passwordFile = lib.mkOption {
          type = lib.types.nullOr lib.types.path;
          default = null;
          description = lib.mdDoc ''
            Path to file containing the database password for Grafana.

            If null, Grafana will attempt to connect without a password
            (works for socket connections with peer auth).
          '';
        };

        sslMode = lib.mkOption {
          type = lib.types.enum ["disable" "require" "verify-ca" "verify-full"];
          default = "disable";
          description = "SSL mode for PostgreSQL connection";
        };
      };

      grafana = {
        provision = lib.mkOption {
          type = lib.types.bool;
          default = true;
          description = lib.mdDoc ''
            Whether to use Grafana provisioning for dashboards.

            When true, dashboards are managed declaratively by NixOS.
            When false, you must manually configure the datasource and import dashboards.
          '';
        };

        disableDeletion = lib.mkOption {
          type = lib.types.bool;
          default = true;
          description = "Prevent deletion of provisioned dashboards from Grafana UI";
        };
      };
    };
    build = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = cfg.server.enable;
        description = "Crystal Forge Builder";
      };

      # === BUILD CONCURRENCY SETTINGS ===

      max_concurrent_derivations = lib.mkOption {
        type = lib.types.ints.positive;
        default = 1;
        description = lib.mdDoc ''
          Maximum number of concurrent nix-store --realise processes.

          This controls how many builds Crystal Forge runs in parallel across
          the entire system.

          **Formula for CPU usage:**
          ```
          Max CPU = max_concurrent_derivations × max_jobs × cores_per_job
          ```

          **Default**: 1 (very conservative - one build at a time)

          **Recommended values by system:**
          - 4-8 cores: 1-2
          - 16 cores: 2-3
          - 32 cores: 3-4
          - 64+ cores: 4-8

          ⚠️  Too high = system overload and slowdown
        '';
        example = 3;
      };

      max_jobs = lib.mkOption {
        type = lib.types.ints.positive;
        default = 1;
        description = lib.mdDoc ''
          Number of parallel derivations within each nix-store process.

          Passed as `--max-jobs` to Nix. This is NOT cores per build - it's
          how many different derivations can build simultaneously within a
          single build process.

          **Default**: 1 (sequential derivations within each build)

          **Example**: If max_jobs=2, a single build process can compile
          two different packages at the same time.

          **Recommended values:**
          - Conservative: 1 (one derivation at a time)
          - Moderate: 2-3 (some parallelism)
          - Aggressive: 4-6 (high parallelism, needs many cores)

          ⚠️  Total parallelism = max_concurrent_derivations × max_jobs
        '';
        example = 2;
      };

      cores_per_job = lib.mkOption {
        type = lib.types.ints.unsigned;
        default = 0;
        description = lib.mdDoc ''
          Number of CPU cores each derivation build can use.

          Passed as `--cores` to Nix. Controls how many cores a single
          derivation (e.g., compiling a package) can utilize.

          **Special value 0**: Unrestricted - each derivation can use all
          available cores. This is the Nix default and works well when
          max_concurrent_derivations = 1.

          **Default**: 0 (unrestricted for single builds)

          **Recommended values:**
          - If max_concurrent_derivations = 1: 0 (let it use all cores)
          - If max_concurrent_derivations > 1: Set to avoid oversubscription

          **Formula to avoid oversubscription:**
          ```
          cores_per_job ≤ total_cores / (max_concurrent_derivations × max_jobs)
          ```

          **Example on 32-core system:**
          - max_concurrent_derivations=3, max_jobs=2 → cores_per_job=4
          - (3 × 2 × 4 = 24 cores, leaves 8 for system)

          ⚠️  If 0 with max_concurrent_derivations > 1, you'll oversubscribe CPUs!
        '';
        example = 4;
      };

      # === BINARY CACHE SETTINGS ===

      use_substitutes = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = lib.mdDoc ''
          Whether to use binary substitutes/caches.

          When true, Nix will download pre-built packages from caches
          instead of building them locally.

          **Recommended**: true (much faster builds)
          **Disable if**: Testing local builds or working offline
        '';
      };

      offline = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = lib.mdDoc ''
          Build in offline mode (no network access).

          When true, Nix will not attempt to download anything. Useful
          for air-gapped environments or testing.

          **Note**: Requires all sources to be pre-fetched or in store.
        '';
      };

      # === TIMING SETTINGS ===

      poll_interval = lib.mkOption {
        type = lib.types.str;
        default = "5m";
        description = lib.mdDoc ''
          Interval between checking for new build jobs.

          How often the build coordinator checks the database for new
          derivations to build.

          **Default**: "5m" (5 minutes)
          **For active development**: "5s" (5 seconds)
          **For production**: "1m" - "5m"

          Format: duration string (e.g., "30s", "5m", "1h")
        '';
        example = "30s";
      };

      max_silent_time = lib.mkOption {
        type = lib.types.str;
        default = "1h";
        description = lib.mdDoc ''
          Maximum time a build can be silent before timing out.

          If a build produces no output for this duration, it will be
          killed. Prevents hung builds from consuming resources.

          **Default**: "1h" (1 hour)

          **Adjust for:**
          - Large builds (Firefox, Chromium): "2h" or more
          - Small packages: "30m"

          Format: duration string (e.g., "30m", "2h")
        '';
        example = "2h";
      };

      timeout = lib.mkOption {
        type = lib.types.str;
        default = "2h";
        description = lib.mdDoc ''
          Maximum total time for a build before timing out.

          The absolute maximum time any build can run, regardless of
          whether it's producing output.

          **Default**: "2h" (2 hours)

          **Adjust for:**
          - Very large builds (LLVM, WebKit): "6h" or more
          - Typical packages: "1h" - "3h"

          Format: duration string (e.g., "1h", "6h")
        '';
        example = "6h";
      };

      # === SECURITY SETTINGS ===

      sandbox = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = lib.mdDoc ''
          Enable sandbox for builds.

          When true, builds run in an isolated environment with restricted
          network and filesystem access. This is a security best practice.

          **Recommended**: true (always)
          **Disable only if**: Build requires network access (rare, usually wrong)
        '';
      };

      # === SYSTEMD RESOURCE ISOLATION ===

      use_systemd_scope = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = lib.mdDoc ''
          Whether to use systemd-run for resource isolation.

          When enabled, each build runs in a systemd scope with enforced
          resource limits (memory, CPU). This prevents runaway builds from
          taking down the entire system.

          **Benefits:**
          - Memory limits prevent OOM kills of main process
          - CPU quotas prevent one build from starving others
          - Automatic cleanup of build processes

          **Requires**: systemd (works on NixOS, most Linux distros)

          **Fallback**: If systemd-run fails, builds run directly

          **Recommended**: true (critical for production)
        '';
      };

      systemd_memory_max = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = "32G";
        description = lib.mdDoc ''
          Memory limit for each build scope.

          Maximum amount of RAM a single build scope can use. When
          exceeded, the scope's processes will be OOM-killed, protecting
          the main Crystal Forge process.

          **Formula:**
          ```
          Total memory usage ≤ max_concurrent_derivations × systemd_memory_max
          ```

          **Default**: "32G" (32 GB per build)

          **Recommended values:**
          - 16GB system: "4G" per build
          - 32GB system: "8G" - "16G" per build
          - 64GB system: "16G" - "32G" per build
          - 128GB+ system: "32G" - "64G" per build

          **Large builds (LLVM, Chromium)**: May need 16GB+

          Format: suffixed size (e.g., "4G", "2048M", "8192M")

          Set to `null` to disable memory limits (not recommended).
        '';
        example = "16G";
      };

      systemd_cpu_quota = lib.mkOption {
        type = lib.types.nullOr lib.types.ints.positive;
        default = 800;
        description = lib.mdDoc ''
          CPU quota for each build scope as percentage.

          Limits the total CPU time available to an entire build scope
          (which may run multiple derivations via max_jobs).

          **Value**: Percentage × 100 (e.g., 400 = 4 cores, 800 = 8 cores)

          **Default**: 800 (8 cores per build scope)

          **Formula for setting:**
          ```
          systemd_cpu_quota ≥ (max_jobs × cores_per_job) × 100
          ```

          **Example:**
          - max_jobs=2, cores_per_job=4 → systemd_cpu_quota should be ≥ 800

          **Recommended values:**
          - Small systems: 200-400 (2-4 cores per build)
          - Medium systems: 400-800 (4-8 cores per build)
          - Large systems: 800-1200 (8-12 cores per build)

          ⚠️  If too low, builds will be throttled even if cores are free

          Set to `null` to disable CPU quotas (not recommended).
        '';
        example = 600;
      };

      systemd_timeout_stop_sec = lib.mkOption {
        type = lib.types.nullOr lib.types.ints.positive;
        default = 600;
        description = lib.mdDoc ''
          Timeout for systemd scope stop operation in seconds.

          How long systemd will wait for a build scope to stop gracefully
          before force-killing it.

          **Default**: 600 (10 minutes)

          **Recommended values:**
          - Quick builds: 300 (5 minutes)
          - Normal builds: 600 (10 minutes)
          - Large builds: 900 (15 minutes)

          ⚠️  Too short = premature kills during cleanup
          ⚠️  Too long = delays in canceling stuck builds
        '';
        example = 900;
      };

      systemd_properties = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [
          "MemorySwapMax=2G"
          "TasksMax=3000"
        ];
        description = lib.mdDoc ''
          Additional systemd properties to set for build scopes.

          These are passed as `--property` arguments to systemd-run.
          Only certain properties are valid for scopes.

          **Default properties:**
          - `MemorySwapMax=2G`: Limit swap usage
          - `TasksMax=3000`: Limit number of processes/threads

          **Valid property prefixes for scopes:**
          - Memory* (MemoryMax, MemorySwapMax, MemoryHigh, etc.)
          - CPU* (CPUQuota, CPUWeight, etc.)
          - Tasks* (TasksMax)
          - IO* (IOWeight, IOReadBandwidthMax, etc.)
          - Kill* (KillMode, KillSignal)
          - OOM* (OOMPolicy, OOMScoreAdjust)
          - Device* (DevicePolicy, DeviceAllow)
          - IPAccounting* (IPAccounting, IPAddressAllow, etc.)

          **Note**: Service-only properties (Environment, Restart,
          WorkingDirectory) are ignored for scopes.
        '';
        example = [
          "MemorySwapMax=4G"
          "TasksMax=5000"
          "IOWeight=100"
          "CPUWeight=100"
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
      attic_ignore_upstream_cache_filter = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Push full derivation to attic";
      };
      attic_jobs = lib.mkOption {
        type = lib.types.ints.unsigned;
        default = 5;
        description = "Parallel Attic cache uploads";
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
      poll_interval = lib.mkOption {
        type = lib.types.ints.unsigned;
        default = 5;
        description = "Delay between cache push attempts in seconds";
      };
      force_repush = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Force re-push to cache even if it thinks it's already there.";
      };
    };
    deployment = {
      max_deployment_age_minutes = lib.mkOption {
        type = lib.types.ints.unsigned;
        default = 30;
        description = "Maximum age in minutes for deployments to be considered valid";
      };
      dry_run_first = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Perform a dry run before actual deployment";
      };
      fallback_to_local_build = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Fallback to local build if remote build fails";
      };
      deployment_timeout_minutes = lib.mkOption {
        type = lib.types.ints.unsigned;
        default = 60;
        description = "Timeout for deployment operations in minutes";
      };
      cache_url = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Cache URL for deployment artifacts";
      };
      cache_public_key = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Cache Public Key for deployment artifacts";
      };
      deployment_poll_interval = lib.mkOption {
        type = lib.types.str;
        default = "15m";
        description = "Interval between deployment polling checks";
      };
      require_sigs = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Check sigs before deployment";
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
          desired_target = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "Desired derivation hash for system";
          };
          deployment_policy = lib.mkOption {
            type = lib.types.enum ["manual" "auto_latest" "pinned"];
            default = "manual";
            description = "Deployment policy for the system";
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
          desired_target = null;
          deployment_policy = "manual";
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

      eval_workers = lib.mkOption {
        type = lib.types.int;
        default = 4;
        description = lib.mdDoc ''
          Number of worker threads for nix-eval-jobs parallel evaluation.
          Set to 0 to automatically use the number of CPU cores available.

          This controls how many systems can be evaluated concurrently
          when processing flake commits.
        '';
      };

      eval_max_memory_mb = lib.mkOption {
        type = lib.types.int;
        default = 4096;
        description = lib.mdDoc ''
          Maximum memory size per worker in MB for nix-eval-jobs.

          Each evaluation worker will be limited to this amount of memory.
          Default is 4096 MB (4 GB) per worker.

          Adjust based on available system memory and the number of workers.
        '';
      };

      eval_check_cache = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = lib.mdDoc ''
          Whether to check cache status during evaluation.

          When enabled, nix-eval-jobs will report which derivations are
          already built (in local store or binary cache) vs need building.

          Disable if cache checking is slow or causing issues.
        '';
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
    env-file = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/crystal-forge/.config/crystal-forge-attic.env";
      description = "Path to env file";
    };
  };

  config = lib.mkIf cfg.enable {
    nix.settings = lib.mkIf (cfg.server.enable || cfg.build.enable || cfg.client.enable) {
      experimental-features = ["nix-command" "flakes"];
      allowed-users = ["root" "crystal-forge"];
      trusted-users = ["root" "crystal-forge"];

      # Add substituters based on cache configuration
      substituters = lib.mkIf (cfg.cache.push_to != null) [
        cfg.cache.push_to
      ];
      trusted-public-keys = lib.mkIf (cfg.deployment.cache_public_key != null) [
        cfg.deployment.cache_public_key
      ];
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
      "d /var/lib/crystal-forge/.config/nix 0755 crystal-forge crystal-forge -"
      "d /var/lib/crystal-forge/.local 0755 crystal-forge crystal-forge -"
      "d /var/lib/crystal-forge/.local/share 0755 crystal-forge crystal-forge -"
      "d /var/cache/crystal-forge/gc-roots 0755 crystal-forge crystal-forge -" # <-- ADD THIS
      "Z /var/lib/crystal-forge/ 0755 crystal-forge crystal-forge -"
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
        TasksMax = "infinity"; # Keep this as a reasonable default
      };
    };

    services.postgresql = lib.mkIf cfg.local-database {
      # Enable PostgreSQL if server or dashboards need it
      enable = lib.mkIf (cfg.server.enable || cfg.dashboards.enable) true;

      # Ensure database exists (only needed for server)
      ensureDatabases = lib.mkIf cfg.server.enable [cfg.database.name];

      # Ensure users exist - combine both user types
      ensureUsers =
        lib.optional cfg.server.enable {
          name = cfg.database.user;
          ensureDBOwnership = true;
          ensureClauses.login = true;
        }
        ++ lib.optional cfg.dashboards.enable {
          name = cfg.dashboards.datasource.user;
          ensureDBOwnership = false;
        };

      # Identity map (only for server)
      identMap = lib.mkIf cfg.server.enable ''
        crystal-forge-map crystal-forge ${cfg.database.user}
      '';

      initialScript = lib.mkIf cfg.dashboards.enable (
        pkgs.writeText "init-crystal-forge-grafana.sql" ''
          -- Create users if they don't exist
          DO $$
          BEGIN
            IF NOT EXISTS (SELECT FROM pg_user WHERE usename = 'crystal_forge') THEN
              CREATE USER crystal_forge LOGIN;
            END IF;
            IF NOT EXISTS (SELECT FROM pg_user WHERE usename = 'grafana') THEN
              CREATE USER grafana LOGIN;
            END IF;
          END
          $$;

          -- Create database
          SELECT 'CREATE DATABASE crystal_forge OWNER crystal_forge'
          WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'crystal_forge')\gexec

          -- Grant database-level privileges
          GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
          GRANT CONNECT ON DATABASE crystal_forge TO grafana;

          -- Connect to database and set schema permissions
          \c crystal_forge

          -- PostgreSQL 15+ requires explicit schema permissions
          ALTER SCHEMA public OWNER TO crystal_forge;

          -- Grant comprehensive permissions to grafana
          GRANT USAGE ON SCHEMA public TO grafana;
          GRANT CREATE ON SCHEMA public TO grafana;
          GRANT ALL PRIVILEGES ON SCHEMA public TO grafana;

          -- Grant permissions on all current tables and sequences
          GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO grafana;
          GRANT SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO grafana;

          -- Set default privileges for future objects created by crystal_forge
          ALTER DEFAULT PRIVILEGES FOR USER crystal_forge IN SCHEMA public
            GRANT SELECT ON TABLES TO grafana;

          -- Set default privileges for objects created by grafana
          ALTER DEFAULT PRIVILEGES FOR USER grafana IN SCHEMA public
            GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO grafana;
          ALTER DEFAULT PRIVILEGES FOR USER grafana IN SCHEMA public
            GRANT SELECT, UPDATE ON SEQUENCES TO grafana;
        ''
      );

      # Authentication - combine rules for both users
      authentication = lib.mkAfter (
        lib.optionalString cfg.server.enable ''
          local  ${cfg.database.name}  ${cfg.database.user}  peer map=crystal-forge-map
          local  ${cfg.database.name}  ${cfg.database.user}  trust
          host   ${cfg.database.name}  ${cfg.database.user}  127.0.0.1/32  trust
          host   ${cfg.database.name}  ${cfg.database.user}  ::1/128       trust
        ''
        + lib.optionalString cfg.dashboards.enable ''
          local  ${cfg.database.name}  ${cfg.dashboards.datasource.user}  peer
          host   ${cfg.database.name}  ${cfg.dashboards.datasource.user}  127.0.0.1/32  trust
          host   ${cfg.database.name}  ${cfg.dashboards.datasource.user}  ::1/128       trust
        ''
      );
    };

    # Grafana dashboard configuration
    services.grafana = lib.mkIf cfg.dashboards.enable {
      enable = true;
      settings = {
        "plugin.grafana-postgresql-datasource" = {
          enabled = true;
        };
      };

      provision = lib.mkIf cfg.dashboards.grafana.provision {
        enable = true;

        datasources.settings = {
          apiVersion = 1;
          datasources = [
            ({
                uid = "crystal-forge-postgres";
                name = cfg.dashboards.datasource.name;
                type = "postgres";
                url = "${cfg.dashboards.datasource.host}:${toString cfg.dashboards.datasource.port}";
                database = cfg.dashboards.datasource.database;
                user = cfg.dashboards.datasource.user;
                jsonData = {
                  sslmode = cfg.dashboards.datasource.sslMode;
                  postgresVersion = 1400;
                  timescaledb = false;
                };
                isDefault = false;
                editable = true;
              }
              // lib.optionalAttrs (cfg.dashboards.datasource.passwordFile != null) {
                secureJsonData = {
                  password = "$__file{${cfg.dashboards.datasource.passwordFile}}";
                };
              })
          ];
        };

        dashboards.settings = {
          apiVersion = 1;
          providers = [
            {
              name = "Crystal Forge";
              type = "file";
              options.path = "${pkgs.crystal-forge.dashboards}/dashboards";
              disableDeletion = cfg.dashboards.grafana.disableDeletion;
              updateIntervalSeconds = 60;
            }
          ];
        };
      };
    };

    systemd.services.crystal-forge-grafana-db-init = lib.mkIf cfg.dashboards.enable {
      description = "Initialize Crystal Forge database for Grafana";
      after = lib.optional cfg.local-database "postgresql.service";
      before = ["grafana.service"];
      wantedBy = ["multi-user.target"];

      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };

      environment = {
        PGHOST = cfg.dashboards.datasource.host;
        PGPORT = toString cfg.dashboards.datasource.port;
        PGDATABASE = cfg.dashboards.datasource.database;
        PGUSER = cfg.dashboards.datasource.user; # Use the main CF user to grant permissions
      };

      script = let
        psqlCmd =
          if cfg.local-database
          then "${postgres_pkg}/bin/psql"
          else "${pkgs.postgresql}/bin/psql";
      in ''
        # Wait for database to be available
        max_attempts=30
        attempt=0
        while ! ${psqlCmd} -c "SELECT 1" >/dev/null 2>&1; do
          attempt=$((attempt + 1))
          if [ $attempt -ge $max_attempts ]; then
            echo "Failed to connect to database after $max_attempts attempts"
            exit 1
          fi
          echo "Waiting for database... (attempt $attempt/$max_attempts)"
          sleep 2
        done

        # Create grafana user if it doesn't exist (works for both local and remote)
        ${psqlCmd} <<'EOF'
        DO $$
        BEGIN
          IF NOT EXISTS (SELECT FROM pg_user WHERE usename = '${cfg.dashboards.datasource.user}') THEN
            CREATE USER ${cfg.dashboards.datasource.user} LOGIN;
          END IF;
        END
        $$;

        -- Grant permissions
        GRANT USAGE ON SCHEMA public TO ${cfg.dashboards.datasource.user};
        GRANT SELECT ON ALL TABLES IN SCHEMA public TO ${cfg.dashboards.datasource.user};
        GRANT SELECT ON ALL SEQUENCES IN SCHEMA public TO ${cfg.dashboards.datasource.user};

        -- Set default privileges
        ALTER DEFAULT PRIVILEGES FOR USER ${cfg.database.user} IN SCHEMA public
          GRANT SELECT ON TABLES TO ${cfg.dashboards.datasource.user};
        EOF
      '';
    };

    systemd.services.grafana = lib.mkIf cfg.dashboards.enable {
      after =
        lib.optionals cfg.local-database ["postgresql.service"]
        ++ ["crystal-forge-grafana-db-init.service"];
      wants = ["crystal-forge-grafana-db-init.service"];
      requires = ["crystal-forge-grafana-db-init.service"];
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
        # disable registry and per-user nix.conf for deterministic evals
        NIX_REGISTRY = "/dev/null";
        NIX_CONFIG_DIR = "/dev/null";
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
        [nix git vulnix systemd nix-fast-build nix-eval-jobs]
        ++ lib.optional (cfg.cache.cache_type == "Attic") attic-client;

      # Merge existing env with any Environment=… pairs from systemd_properties
      environment = lib.mkMerge [
        {
          RUST_LOG = cfg.log_level;
          NIX_REMOTE = "daemon";
          # NIX_USER_CACHE_DIR = "/var/cache/crystal-forge-nix";
          TMPDIR = "/var/lib/crystal-forge/tmp";
          XDG_RUNTIME_DIR = "/run/crystal-forge";
          XDG_CONFIG_HOME = "/var/lib/crystal-forge/.config";
          HOME = "/var/lib/crystal-forge";
          # disable registry and per-user nix.conf for deterministic evals
          NIX_REGISTRY = "/dev/null";
          NIX_CONFIG_DIR = "/dev/null";
          GC_MARKERS = "1";
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
        mkdir -p /run/crystal-forge
        ${configScriptServer}
        mkdir -p /var/lib/crystal-forge/.config/attic

        # Ensure proper ownership - do this AFTER creating all directories
        chown -R crystal-forge:crystal-forge /var/lib/crystal-forge/.cache
        chown -R crystal-forge:crystal-forge /var/lib/crystal-forge/.config
        # Ensure proper permissions
        chmod -R 755 /var/lib/crystal-forge/.cache
        chmod -R 755 /var/lib/crystal-forge/.config

        # Source the attic environment if it exists
        if [ -f ${cfg.env-file} ]; then
          echo "Loading Attic environment variables from ${cfg.env-file}"
          set -a
          source ${cfg.env-file}
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
        (
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

            # When this service stops, kill all children
            KillMode = "control-group";

            # Make sure we load the environment file
            EnvironmentFile = [
              "-${cfg.env-file}"
              "-/var/lib/crystal-forge/.config/crystal-forge-attic.env"
            ];

            NoNewPrivileges = true;
            ProtectSystem = "no";
            ProtectHome = false;
            PrivateTmp = true;
            ProtectKernelTunables = true;
            ProtectKernelModules = true;
            ProtectControlGroups = true;

            TasksMax = "infinity";
            LimitNPROC = "infinity";
            LimitNOFILE = 1048576;
            OOMPolicy = "continue";

            ReadWritePaths = [
              "/var/lib/crystal-forge"
              "/tmp"
              "/run/crystal-forge"
              "/var/cache/crystal-forge-nix"
              "/var/cache/crystal-forge"
              "/var/lib/crystal-forge/.cache"
              "/nix/var/nix/daemon-socket"
            ];
            ReadOnlyPaths = ["/etc/nix" "/etc/ssl/certs"];

            Restart = "always";
            RestartSec = 5;
          }
          // parsed.svc
        )
        // {
          # Ensure ExecStart is never removed by parsed.svc merge
          ExecStart = lib.mkForce builderScript;
        };
    });

    systemd.services.crystal-forge-server = lib.mkIf cfg.server.enable {
      description = "Crystal Forge Server";
      wantedBy = ["multi-user.target"];
      after = lib.optional cfg.local-database "postgresql.service";
      wants = lib.optional cfg.local-database "postgresql.service";

      path = with pkgs; [
        nix
        git
        nix-fast-build
        nix-eval-jobs
        coreutils
        findutils
        gnused
        gnugrep
      ];

      environment = {
        # Core runtime
        RUST_LOG = cfg.log_level;
        TZDIR = "${pkgs.tzdata}/share/zoneinfo";
        LOCALE_ARCHIVE = "${pkgs.glibcLocales}/lib/locale/locale-archive";

        # --- Critical Nix environment for deterministic evaluation ---
        # Use the daemon socket for evaluation
        NIX_REMOTE = "daemon";

        # Completely isolate from user registries/config
        HOME = "/var/lib/crystal-forge";
        XDG_CONFIG_HOME = "/var/lib/crystal-forge/.config";
        NIX_REGISTRY = "/dev/null";
        NIX_CONFIG_DIR = "/dev/null";
        NIX_USER_CONF_FILES = "/dev/null";

        # Enable flakes and nix-command — exactly as in your manual test
        NIX_CONFIG = ''
          experimental-features = nix-command flakes
          flake-registry =
        '';

        # Required to allow git+ssh or https fetches
        GIT_SSH_COMMAND = "ssh -i /var/lib/crystal-forge/.ssh/id_ed25519 -o UserKnownHostsFile=/var/lib/crystal-forge/.ssh/known_hosts -o StrictHostKeyChecking=yes";

        # Optional: specify cache location for nix-eval-jobs
        NIX_USER_CACHE_DIR = "/var/cache/crystal-forge-nix";
      };

      preStart = ''
        mkdir -p /run/crystal-forge
        ${configScriptServer}
        mkdir -p /var/lib/crystal-forge/.config/attic
        chown -R crystal-forge:crystal-forge /var/lib/crystal-forge/.cache
        chown -R crystal-forge:crystal-forge /var/lib/crystal-forge/.config
        chmod -R 755 /var/lib/crystal-forge/.cache
        chmod -R 755 /var/lib/crystal-forge/.config
      '';

      serviceConfig = {
        Type = "exec";
        ExecStart = serverScript;
        User = "crystal-forge";
        Group = "crystal-forge";
        WorkingDirectory = "/var/lib/crystal-forge";

        # Filesystem permissions
        StateDirectory = "crystal-forge";
        StateDirectoryMode = "0750";
        RuntimeDirectory = "crystal-forge";
        RuntimeDirectoryMode = "0700";
        CacheDirectory = "crystal-forge-nix";
        CacheDirectoryMode = "0750";

        # Read/write permissions
        ReadWritePaths = [
          "/var/lib/crystal-forge"
          "/var/lib/crystal-forge/.cache"
          "/tmp"
          "/run/crystal-forge"
          "/var/cache/crystal-forge"
          "/var/cache/crystal-forge-nix"
          "/nix/var/nix/daemon-socket"
        ];

        # Security isolation
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectSystem = "no";
        ProtectHome = false;
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
        nix-eval-jobs
        nix-fast-build
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
        nixos-rebuild
        git
      ];
      environment = {
        RUST_LOG = cfg.log_level;
        # CRYSTAL_FORGE__CLIENT__SERVER_HOST = cfg.client.server_host;
        # CRYSTAL_FORGE__CLIENT__SERVER_PORT = toString cfg.client.server_port;
        # CRYSTAL_FORGE__CLIENT__PRIVATE_KEY = cfg.client.private_key;

        # make nix/git caches writable for the agent
        HOME = "/var/lib/crystal-forge-agent";
        XDG_CACHE_HOME = "/var/lib/crystal-forge-agent/.cache";
        NIX_USER_CACHE_DIR = "/var/cache/crystal-forge-agent";

        NIX_CONFIG = ''
          experimental-features = nix-command flakes
          ${lib.optionalString (cfg.deployment.cache_url != null) ''
            substituters = ${cfg.deployment.cache_url}
          ''}
          ${lib.optionalString (cfg.deployment.cache_public_key != null) ''
            trusted-public-keys = ${cfg.deployment.cache_public_key}
          ''}
        '';
      };
      preStart = ''
        mkdir -p /var/lib/crystal-forge-agent
        ${configScriptAgent}
      '';

      serviceConfig = {
        Type = "exec";
        ExecStart = agentScript;
        User = "root";
        Group = "root";

        # Keep state + runtime under /var/lib and /run
        StateDirectory = "crystal-forge-agent";
        StateDirectoryMode = "0750";
        RuntimeDirectory = "crystal-forge-agent";
        RuntimeDirectoryMode = "0700";
        WorkingDirectory = "/var/lib/crystal-forge-agent";

        CacheDirectory = "crystal-forge-agent";

        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = false;

        # Allow writes where we actually need them
        ReadWritePaths = [
          "/nix/var/nix/profiles"
          "/nix/var/nix/gcroots"
          # "/boot"
          "/var/lib/crystal-forge-agent"
          "/var/cache/crystal-forge-agent"
          "/tmp"
          "/run/crystal-forge-agent"
        ];
        # Also ensure read-only access to CA bundle (good practice):
        ReadOnlyPaths = ["/etc/ssl/certs"];
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
        assertion = cfg.server.enable || cfg.client.enable || cfg.build.enable || cfg.dashboards.enable;
        message = "At least one of server, client, build, or dashboards must be enabled";
      }
      {
        assertion = cfg.dashboards.enable -> (cfg.dashboards.datasource.host != null);
        message = "Crystal Forge dashboards require database.host or dashboards.datasource.host to be set";
      }
      {
        assertion = cfg.dashboards.enable && !cfg.local-database -> (cfg.dashboards.datasource.host != "/run/postgresql");
        message = "When using remote database for dashboards, dashboards.datasource.host must be a network address, not a socket path";
      }
    ];
  };
}
