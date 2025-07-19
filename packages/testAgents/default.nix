{
  pkgs,
  lib,
  ...
}:
with lib;
with lib.crystal-forge; let
  cf_port = 3445;

  # Define agent configurations - DRY principle
  agentConfigs = {
    gray = {
      hostname = "test.gray";
      privateKeyString = "PjCQGMmzXHpPqGXjSPZ4sdHu7+stRX0AOuhZAvKwuKg=";
      publicKeyString = "49+maHYdvvn/qUx1CMzg0TLu1BbLS64c1K4E0/2ORO4=";
      startDerivation = "/nix/store/rjl1jl1s2s1b76fjqibh9llxrfij6b0s-nixos-system-gray-25.11.20250708.9807714";
      updateDerivations = [
        "/nix/store/wfz1hffcar6anakhl0wlz90dn2gngryp-nixos-system-gray-25.05.20250619.005b89d"
      ];
    };
    lucas = {
      hostname = "test.lucas";
      privateKeyString = "uK2wOnCBjF8hOo3Ep8uy3UNpfM7aDHm/3K05tmbRt2o=";
      publicKeyString = "pwByU3iXjxGB/WP5hVEoR4eL/xsYWv1QmOdBHkIchnM=";
      startDerivation = "/nix/store/7jpnf4zpa92qhzi0qbvgapq15xs6bvj8-nixos-system-lucas-25.05.20250619.005b89d";
      updateDerivations = [
        "/nix/store/w30p4cmca85rzglsr2q33vn2m50l6yqy-nixos-system-lucas-25.11.20250708.9807714"
      ];
    };
  };

  # Common action settings
  commonActionSettings = {
    dailyHeartbeats = 96; # Every 15 minutes = 96 per day
    weeklyUpdates = 2;
    emergencyRestarts = 1;
    endTimeNow = true;
  };

  # Helper function to create an agent with actions
  mkTestAgent = name: config: timeScale:
    mkAgent {
      inherit pkgs;
      inherit (config) hostname privateKeyString publicKeyString;
      serverHost = "localhost";
      serverPort = cf_port;
      actions = mkWeeklyActions (commonActionSettings
        // {
          inherit timeScale;
          inherit (config) startDerivation updateDerivations;
        });
    };

  # Create individual agents for standalone use (100x faster)
  test-gray = mkTestAgent "gray" agentConfigs.gray 0.01;
  test-lucas = mkTestAgent "lucas" agentConfigs.lucas 0.01;

  # Create agents for orchestrator (1000x faster for full simulation)
  orchestrator-agents =
    mapAttrsToList (
      name: config: let
        agent = mkTestAgent name config 0.001;
      in {
        inherit (agent) agent hostname actions privateKeyString serverHost serverPort;
      }
    )
    agentConfigs;

  # Weekly orchestrator with SQL jobs
  weekly-simulation = mkWeeklyOrchestrator {
    inherit pkgs;
    timeScale = 0.001;
    agents = orchestrator-agents;
    sqlJobsPackage = pkgs.crystal-forge.run-postgres-jobs;
  };
in
  pkgs.writeShellApplication {
    name = "crystal-forge-agents";
    runtimeInputs = with pkgs; [bat];
    text = ''
      cat << 'EOF' | bat --language=markdown --style=plain
      # Crystal Forge Test Agents

      This package provides test agents for Crystal Forge with multiple time scales:

      ## Individual Agents (100x faster - ~1.7 hours for full week)
      - **test-gray**: Simulates NixOS system management for gray host
      - **test-lucas**: Simulates NixOS system management for lucas host

      ## Full Weekly Simulation (1000x faster - ~10 minutes for full week)
      - **weekly-simulation**: Runs both agents with coordinated timeline and midnight SQL jobs

      ## Usage

      Run individual agents for testing:
      ```bash
      nix run .#test-gray.agent
      nix run .#test-lucas.agent
      ```

      Run complete weekly simulation with SQL jobs:
      ```bash
      nix run .#weekly-simulation
      ```

      ## Configuration
      - Server: localhost:${toString cf_port}
      - Daily heartbeats: ${toString commonActionSettings.dailyHeartbeats} (every 15 simulated minutes)
      - Weekly updates: ${toString commonActionSettings.weeklyUpdates}
      - Emergency restarts: ${toString commonActionSettings.emergencyRestarts}

      ## Agent Details
      ${concatStringsSep "\n" (mapAttrsToList (name: config: ''
          ### ${config.hostname}
          - Start derivation: ${config.startDerivation}
          - Update derivations: ${toString (length config.updateDerivations)} configured
          - Public key: ${config.publicKeyString}
        '')
        agentConfigs)}
      EOF
    '';
  }
  // {
    # Individual agents for standalone testing
    test-gray = {
      agent = test-gray.agent;
      publicKey = test-gray.publicKey;
    };
    test-lucas = {
      agent = test-lucas.agent;
      publicKey = test-lucas.publicKey;
    };

    # Weekly orchestrator for full simulation
    inherit weekly-simulation;
  }
