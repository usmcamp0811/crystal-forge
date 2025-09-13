# lib/server-test-node/default.nix
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
    # crystal-forge-specific config (overrides defaults)
    crystalForgeConfig ? {},
    # General NixOS config merged on top
    extraConfig ? {},
    ...
  }: let
    # Generate keypairs for agents if not provided
    agentKeyPairs = map (
      agentName: rec {
        name = agentName;
        keyPair = lib.crystal-forge.mkKeyPair {
          inherit pkgs;
          name = agentName; # or: inherit name;
        };
      }
    )
    agents;

    # Generate systems configuration from agents
    agentSystems = map (agent: {
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

    # Default crystal-forge configuration
    defaultCrystalForgeConfig = {
      enable = true;
      "local-database" = true; # dashed keys must be quoted
      log_level = "debug";
      build.offline = true;
      database = {
        user = "crystal_forge";
        host = "localhost";
        name = "crystal_forge";
      };
      flakes.flake_polling_interval = "1m";
      flakes.watched = [
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
      environments = [
        {
          name = "test";
          description = "Test environment for Crystal Forge agents and evaluation";
          is_active = true;
          risk_profile = "LOW";
          compliance_level = "NONE";
        }
      ];
      server = {
        inherit port;
        enable = true;
        host = "0.0.0.0";
      };
    };

    # Agent systems configuration (only when provided)
    agentSystemsConfig =
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
            flake_name = "crystal-forge";
          }
        ];
      }
      else {};

    # When using mkMerge, lists are concatenated. Force-replace lists we want to override.
    forceListReplacements = cfg:
      cfg
      // {
        flakes =
          (cfg.flakes or {})
          // {
            watched = lib.mkForce (cfg.flakes.watched or []);
          };
        environments = lib.mkForce (cfg.environments or []);
        systems = lib.mkForce (cfg.systems or []);
      };

    # Merge with precedence: defaults < agent systems < caller overrides
    finalCrystalForgeConfig = lib.mkMerge [
      defaultCrystalForgeConfig
      agentSystemsConfig
      (forceListReplacements crystalForgeConfig)
    ];

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
    lib.mkMerge [baseConfig extraConfig];
}
