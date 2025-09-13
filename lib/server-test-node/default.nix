{
  lib,
  system ? null,
  ...
}: rec {
  makeServerNode = {
    pkgs,
    inputs,
    systemBuildClosure,
    keyPath ? null,
    pubPath ? null,
    cfFlakePath ? null,
    port ? 3000,
    agents ? [],
    extraConfig ? {},
    ...
  }: let
    # Generate keypairs for agents if not provided
    agentKeyPairs =
      map (agentName: {
        name = agentName;
        keyPair = lib.crystal-forge.mkKeyPair {
          inherit pkgs;
          name = agentName;
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

    # Extract crystal-forge config from extraConfig
    extraCrystalForgeConfig = extraConfig.services.crystal-forge or {};

    # Remove services from extraConfig to avoid conflicts
    cleanedExtraConfig = removeAttrs extraConfig ["services"];
  in
    {
      imports = [inputs.self.nixosModules.crystal-forge];
      networking.useDHCP = true;
      networking.firewall.allowedTCPPorts = [port 5432];
      virtualisation.writableStore = true;
      virtualisation.memorySize = 8096;
      virtualisation.cores = 8;
      virtualisation.additionalPaths = [systemBuildClosure];

      environment.systemPackages = [pkgs.git pkgs.jq pkgs.crystal-forge.default pkgs.crystal-forge.cf-test-modules.runTests pkgs.crystal-forge.cf-test-modules.testRunner];
      environment.etc = lib.mkMerge [
        (lib.mkIf (keyPath != null) {"agent.key".source = "${keyPath}/agent.key";})
        (lib.mkIf (pubPath != null) {"agent.pub".source = "${pubPath}/agent.pub";})
        (lib.mkIf (cfFlakePath != null) {"cf_flake".source = cfFlakePath;})
      ];
      # environment.variables = {
      #   PGHOST = "/run/postgresql";
      #   PGUSER = "postgres";
      # };

      services.postgresql = {
        #   enable = true;
        #   settings."listen_addresses" = lib.mkForce "*";
        #   authentication = lib.concatStringsSep "\n" [
        #     "local   all   postgres   trust"
        #     "local   all   all        peer"
        #     "host    all   all 127.0.0.1/32 trust"
        #     "host    all   all ::1/128      trust"
        #     "host    all   all 10.0.2.2/32  trust"
        #   ];
        #   initialScript = pkgs.writeText "init-crystal-forge.sql" ''
        #     CREATE USER crystal_forge LOGIN;
        #     CREATE DATABASE crystal_forge OWNER crystal_forge;
        #     GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
        #   '';
      };

      services.crystal-forge = lib.mkMerge [
        {
          enable = true;
          local-database = true;
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
        }
        # Use generated agent systems if agents provided
        (lib.mkIf (agents != []) {
          systems = agentSystems;
        })
        # Fallback to single agent if pubPath provided and no agents list
        (lib.mkIf (pubPath != null && agents == []) {
          systems = [
            {
              hostname = "agent";
              public_key = lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
              environment = "test";
              flake_name = "crystal-forge";
            }
          ];
        })
        # Merge in extra crystal-forge config from extraConfig
        extraCrystalForgeConfig
      ];
    }
    // cleanedExtraConfig;
}
