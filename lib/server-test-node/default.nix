{
  lib,
  system ? null,
  ...
}: rec {
  mkServerNode = {
    pkgs,
    inputs,
    systemBuildClosure,
    keyPath ? null,
    pubPath ? null,
    cfFlakePath ? null,
    port ? 3000,
    agents ? [],
    # Crystal-forge-specific config (deep merged with defaults)
    crystalForgeConfig ? {},
    # General NixOS config merged on top
    extraConfig ? {},
    ...
  }: let
    # Generate keypairs for agents if not provided
    agentKeyPairs =
      map (agentName: rec {
        name = agentName;
        keyPair = lib.crystal-forge.mkKeyPair {
          inherit pkgs name;
        };
      })
      agents;

    # Generate systems configuration from agents
    agentSystems =
      map (agent: {
        hostname = agent.name;
        public_key = lib.crystal-forge.mkPublicKey {
          inherit pkgs;
          name = agent.name;
          keyPair = agent.keyPair;
        };
        environment = "test";
        flake_name = "test-flake";
      })
      agentKeyPairs;

    # Default crystal-forge configuration - use mkDefault for everything
    defaultCrystalForgeConfig = {
      enable = lib.mkDefault true;
      "local-database" = lib.mkDefault true;
      log_level = lib.mkDefault "debug";

      cache = {
        cache_type = "S3";
        push_to = "s3://crystal-forge-cache";
        push_after_build = true;
        s3_region = "us-east-1";
        parallel_uploads = 2;
        max_retries = 2;
        retry_delay_seconds = 1;
      };
      build = {
        enable = true;
        offline = lib.mkDefault false;
        systemd_properties = [
          "Environment=AWS_ENDPOINT_URL=http://s3Cache:9000"
          "Environment=AWS_ACCESS_KEY_ID=minioadmin"
          "Environment=AWS_SECRET_ACCESS_KEY=minioadmin"
          "Environment=NIX_LOG=trace"
          "Environment=NIX_SHOW_STATS=1"
        ];
      };

      database = {
        user = lib.mkDefault "crystal_forge";
        host = lib.mkDefault "localhost";
        name = lib.mkDefault "crystal_forge";
        port = lib.mkDefault 5432;
      };

      flakes = {
        flake_polling_interval = lib.mkDefault "1m";
        watched = lib.mkDefault [
          {
            name = "crystal-forge";
            repo_url = "http://gitserver/crystal-forge";
            auto_poll = true;
            initial_commit_depth = 5;
          }
          {
            name = "crystal-forge-development";
            repo_url = "http://gitserver/crystal-forge?ref=development";
            auto_poll = true;
            initial_commit_depth = 7;
          }
          {
            name = "crystal-forge-feature";
            repo_url = "http://gitserver/crystal-forge?ref=feature/experimental";
            auto_poll = true;
            initial_commit_depth = 3;
          }
        ];
      };

      environments = lib.mkDefault [
        {
          name = "test";
          description = "Test environment for Crystal Forge agents and evaluation";
          is_active = true;
          risk_profile = "LOW";
          compliance_level = "NONE";
        }
      ];

      server = {
        port = lib.mkDefault port;
        enable = lib.mkDefault true;
        host = lib.mkDefault "0.0.0.0";
      };

      # Default to empty systems - will be overridden by agent logic or user config
      systems = lib.mkDefault [];
    };

    # Agent systems override (higher priority than defaults)
    agentSystemsOverride =
      if agents != []
      then {
        systems = agentSystems;
      }
      else if pubPath != null
      then {
        systems = [
          {
            hostname = "agent";
            public_key = lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
            environment = "test";
            flake_name = "test-flake";
          }
        ];
      }
      else {};

    # Final crystal-forge config: defaults < agent systems < user overrides
    # Using recursive update for deep merging instead of mkMerge
    finalCrystalForgeConfig =
      lib.recursiveUpdate
      (lib.recursiveUpdate defaultCrystalForgeConfig agentSystemsOverride)
      crystalForgeConfig;

    # Base system configuration
    baseConfig = {
      imports = [inputs.self.nixosModules.crystal-forge];

      networking.useDHCP = true;
      networking.firewall.allowedTCPPorts = [port 5432];

      virtualisation.writableStore = true;
      virtualisation.memorySize = 8096;
      virtualisation.cores = 8;
      virtualisation.additionalPaths = [systemBuildClosure];

      environment.systemPackages = with pkgs; [
        git
        jq
        crystal-forge.default
        crystal-forge.cf-test-modules.runTests
        crystal-forge.cf-test-modules.testRunner
      ];

      environment.etc = lib.mkMerge [
        (lib.mkIf (keyPath != null) {"agent.key".source = "${keyPath}/agent.key";})
        (lib.mkIf (pubPath != null) {"agent.pub".source = "${pubPath}/agent.pub";})
        (lib.mkIf (cfFlakePath != null) {"cf_flake".source = cfFlakePath;})
      ];

      services.crystal-forge = finalCrystalForgeConfig;
    };
  in
    # Merge base config with extra config
    lib.recursiveUpdate baseConfig extraConfig;
}
