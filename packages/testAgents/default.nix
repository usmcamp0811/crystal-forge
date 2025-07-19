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
    emergencyRestarts = 3;
    endTimeNow = true;
  };

  # Helper function to create an agent with actions (for legacy standalone agents)
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

  # Helper to create agents for orchestrators (without pre-generated actions)
  mkOrchestratorAgent = {
    hostname,
    privateKeyString,
    publicKeyString ? null,
    serverHost ? "localhost",
    serverPort ? 3445,
    currentDerivation ? null,
    startDerivation ? null,
  }: {
    inherit hostname privateKeyString serverHost serverPort;
    inherit currentDerivation startDerivation;
    publicKey = publicKeyString;
  };

  # Create individual agents for standalone use (100x faster)
  test-gray = mkTestAgent "gray" agentConfigs.gray 0.0001;
  test-lucas = mkTestAgent "lucas" agentConfigs.lucas 0.0001;

  # Create agents for orchestrator (1000x faster for full simulation)
  legacy-orchestrator-agents =
    mapAttrsToList (
      name: config: let
        agent = mkTestAgent name config 0.001;
      in {
        inherit (agent) agent hostname actions privateKeyString serverHost serverPort;
      }
    )
    agentConfigs;

  # Create simplified agents for new orchestrators
  orchestrator-agents =
    mapAttrsToList (
      name: config:
        mkOrchestratorAgent {
          inherit (config) hostname privateKeyString publicKeyString startDerivation;
          serverHost = "localhost";
          serverPort = cf_port;
          currentDerivation = config.startDerivation; # Use start derivation as current
        }
    )
    agentConfigs;

  # Weekly orchestrator with SQL jobs (using new approach)
  weekly-simulation = mkWeeklyOrchestrator {
    inherit pkgs;
    timeScale = 0.00001;
    agents = orchestrator-agents;
    sqlJobsPackage = pkgs.crystal-forge.run-postgres-jobs;
    simulationDays = 7;
    dailyHeartbeats = 96;
    agentConfigChanges = {
      "test.gray" = [
        {
          derivationPath = elemAt agentConfigs.gray.updateDerivations 0;
          hour = 10;
        }
      ];
      "test.lucas" = [
        {
          derivationPath = elemAt agentConfigs.lucas.updateDerivations 0;
          hour = 14;
        }
      ];
    };
    agentRestarts = {
      "test.gray" = [
        {
          derivationPath = agentConfigs.gray.startDerivation;
          hour = 8;
        }
      ];
    };
  };

  # Daily simulation example (simulate 3 days ago)
  daily-sim = mkDailyOrchestrator {
    inherit pkgs;
    agents = orchestrator-agents;
    daysBack = 3;
    timeScale = 0.0000001;
    # dailyHeartbeats = 96;
    dailyHeartbeats = 96;
    sqlJobsPackage = pkgs.crystal-forge.run-postgres-jobs;
    agentConfigChanges = {
      "test.gray" = [
        {
          derivationPath = elemAt agentConfigs.gray.updateDerivations 0;
          hour = 10;
        }
        {
          derivationPath = agentConfigs.gray.startDerivation;
          hour = 16;
        }
      ];
      "test.lucas" = [
        {
          derivationPath = elemAt agentConfigs.lucas.updateDerivations 0;
          hour = 14;
        }
      ];
    };
    agentRestarts = {
      "test.gray" = [
        {
          derivationPath = agentConfigs.gray.startDerivation;
          hour = 23;
        }
      ];
      # test.lucas has no restarts this day
    };
  };

  # Multiple daily simulations for different days
  daily-sim-yesterday = mkDailyOrchestrator {
    inherit pkgs;
    agents = orchestrator-agents;
    daysBack = 1;
    timeScale = 0.00001;
    dailyHeartbeats = 96;
    sqlJobsPackage = pkgs.crystal-forge.run-postgres-jobs;
    agentConfigChanges = {
      "test.lucas" = [
        {
          derivationPath = elemAt agentConfigs.lucas.updateDerivations 0;
          hour = 9;
        }
      ];
    };
    agentRestarts = {};
  };
in
  pkgs.writeShellApplication {
    name = "crystal-forge-agents";
    runtimeInputs = with pkgs; [bat];
    text = ''
      cat << 'EOF' | bat --language=markdown --style=plain
      # Crystal Forge Test Agents

      This package provides test agents for Crystal Forge with multiple approaches:

      ## Individual Agents (100x faster - ~1.7 hours for full week)
      - **test-gray**: Legacy standalone agent for gray host
      - **test-lucas**: Legacy standalone agent for lucas host

      ## New Orchestrator Simulations
      - **weekly-simulation**: New approach with per-agent config changes and restarts
      - **daily-sim**: Single day simulation (3 days ago) with specific events
      - **daily-sim-yesterday**: Yesterday's simulation with different events

      ## Usage

      Run new orchestrator simulations:
      ```bash
      nix run .#weekly-simulation
      nix run .#daily-sim
      nix run .#daily-sim-yesterday
      ```

      Run legacy weekly simulation:
      ```bash
      nix run .#legacy-weekly-simulation
      ```

      ## Configuration
      - Server: localhost:${toString cf_port}
      - Daily heartbeats: ${toString commonActionSettings.dailyHeartbeats} (every 15 simulated minutes)
      - Time scales: 0.01 for daily sims, 0.001 for weekly sims

      ## Agent Details
      ${concatStringsSep "\n" (mapAttrsToList (name: config: ''
          ### ${config.hostname}
          - Start derivation: ${config.startDerivation}
          - Update derivations: ${toString (length config.updateDerivations)} configured
          - Public key: ${config.publicKeyString}
        '')
        agentConfigs)}

      ## New Orchestrator Features
      - Per-agent configuration changes at specific hours
      - Per-agent restart schedules
      - SQL jobs run at end of each simulated day
      - Flexible daily or weekly simulation periods
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

    # New orchestrator simulations
    inherit weekly-simulation daily-sim daily-sim-yesterday;

    # Expose orchestrator agents for external use
    inherit orchestrator-agents;
  }
