{
  pkgs,
  lib,
  ...
}:
with lib;
with lib.crystal-forge; let
  cf_port = 3445;
  heartbeat_delay = 10;
  config_delay = 15;

  test-gray = mkAgent {
    inherit pkgs;
    hostname = "test.gray";
    serverHost = "localhost";
    serverPort = cf_port;
    privateKeyString = "PjCQGMmzXHpPqGXjSPZ4sdHu7+stRX0AOuhZAvKwuKg=";
    publicKeyString = "49+maHYdvvn/qUx1CMzg0TLu1BbLS64c1K4E0/2ORO4=";
    actions = mkWeeklyActions {
      endTimeNow = true;
      timeScale = 0.01; # 100x faster - adjust as needed
      startDerivation = "/nix/store/rjl1jl1s2s1b76fjqibh9llxrfij6b0s-nixos-system-gray-25.11.20250708.9807714";
      updateDerivations = [
        "/nix/store/wfz1hffcar6anakhl0wlz90dn2gngryp-nixos-system-gray-25.05.20250619.005b89d"
      ];
      dailyHeartbeats = 96; # Every 15 minutes = 96 per day
      weeklyUpdates = 2;
      emergencyRestarts = 1;
    };
  };

  test-lucas = mkAgent {
    inherit pkgs;
    hostname = "test.lucas";
    serverHost = "localhost";
    serverPort = cf_port;
    privateKeyString = "uK2wOnCBjF8hOo3Ep8uy3UNpfM7aDHm/3K05tmbRt2o=";
    publicKeyString = "pwByU3iXjxGB/WP5hVEoR4eL/xsYWv1QmOdBHkIchnM=";
    actions = mkWeeklyActions {
      timeScale = 0.01; # 100x faster - adjust as needed
      endTimeNow = true;
      startDerivation = "/nix/store/7jpnf4zpa92qhzi0qbvgapq15xs6bvj8-nixos-system-lucas-25.05.20250619.005b89d";
      updateDerivations = [
        "/nix/store/w30p4cmca85rzglsr2q33vn2m50l6yqy-nixos-system-lucas-25.11.20250708.9807714"
      ];
      dailyHeartbeats = 96; # Every 15 minutes = 96 per day
      weeklyUpdates = 2;
      emergencyRestarts = 1;
    };
  };

  # Weekly orchestrator that runs both agents with midnight SQL jobs
  weekly-simulation = mkWeeklyOrchestrator {
    inherit pkgs;
    timeScale = 0.001;
    agents = [
      {
        agent = test-gray.agent;
        hostname = "test.gray";
        actions = mkWeeklyActions {
          endTimeNow = true;
          timeScale = 0.001;
          startDerivation = "/nix/store/rjl1jl1s2s1b76fjqibh9llxrfij6b0s-nixos-system-gray-25.11.20250708.9807714";
          updateDerivations = [
            "/nix/store/wfz1hffcar6anakhl0wlz90dn2gngryp-nixos-system-gray-25.05.20250619.005b89d"
          ];
          dailyHeartbeats = 96;
          weeklyUpdates = 2;
          emergencyRestarts = 1;
        };
      }
      {
        agent = test-lucas.agent;
        hostname = "test.lucas";
        actions = mkWeeklyActions {
          timeScale = 0.001;
          endTimeNow = true;
          startDerivation = "/nix/store/7jpnf4zpa92qhzi0qbvgapq15xs6bvj8-nixos-system-lucas-25.05.20250619.005b89d";
          updateDerivations = [
            "/nix/store/w30p4cmca85rzglsr2q33vn2m50l6yqy-nixos-system-lucas-25.11.20250708.9807714"
          ];
          dailyHeartbeats = 96;
          weeklyUpdates = 2;
          emergencyRestarts = 1;
        };
      }
    ];
    sqlJobsPackage = pkgs.crystal-forge.run-postgres-jobs;
  };
in
  pkgs.writeShellApplication {
    name = "crystal-forge-agents";
    runtimeInputs = with pkgs; [bat];
    text = ''
           cat << 'EOF' | bat --language=markdown --style=plain
      # Crystal Forge Test Agents

      This package provides test agents for Crystal Forge:

      ## Individual Agents
      - **test.gray**: Simulates NixOS system upgrade from 25.11 to 25.05
      - **test.lucas**: Simulates multiple configuration changes and heartbeats

      ## Weekly Simulation
      - **weekly-simulation**: Runs both agents with midnight SQL jobs

      ## Usage

      Run individual agents:
      - `nix run .#test-gray.agent`
      - `nix run .#test-lucas.agent`

      Run full weekly simulation with SQL jobs:
      - `nix run .#weekly-simulation`

      All agents connect to localhost:${toString cf_port} by default.
      Time scale: 0.01 (100x faster - full week in ~1.7 hours)
      EOF
    '';
  }
  // {
    # Individual agents
    test-gray = {
      agent = test-gray.agent;
      publicKey = test-gray.publicKey;
    };
    test-lucas = {
      agent = test-lucas.agent;
      publicKey = test-lucas.publicKey;
    };

    # Weekly orchestrator
    inherit weekly-simulation;
  }
