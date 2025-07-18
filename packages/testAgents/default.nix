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

  # Agent with custom action plan
  test-lucas = mkAgent {
    inherit pkgs;
    hostname = "test.lucas";
    serverHost = "localhost";
    serverPort = cf_port;
    privateKeyString = "uK2wOnCBjF8hOo3Ep8uy3UNpfM7aDHm/3K05tmbRt2o=";
    publicKeyString = "pwByU3iXjxGB/WP5hVEoR4eL/xsYWv1QmOdBHkIchnM=";
    actions = mkWeeklyActions {
      timeScale = 0.01; # 100x faster - adjust as needed
      startDerivation = "/nix/store/7jpnf4zpa92qhzi0qbvgapq15xs6bvj8-nixos-system-lucas-25.05.20250619.005b89d";
      updateDerivations = [
        "/nix/store/w30p4cmca85rzglsr2q33vn2m50l6yqy-nixos-system-lucas-25.11.20250708.9807714"
      ];
      dailyHeartbeats = 96; # Every 15 minutes = 96 per day
      weeklyUpdates = 2;
      emergencyRestarts = 1;
    };
  };
in
  pkgs.writeShellApplication {
    name = "crystal-forge-agents";
    runtimeInputs = with pkgs; [bat];
    text = ''
          cat << 'EOF' | bat --language=markdown --style=plain
      # Crystal Forge Test Agents

      This package provides two test agents for Crystal Forge:

      ## test.gray
      Agent that simulates a NixOS system upgrade from 25.11 to 25.05

      ## test.lucas
      Agent that simulates multiple configuration changes and heartbeats

      ## Usage

      Run the agents using:
      - `nix run .#test-gray.agent`
      - `nix run .#test-lucas.agent`

      Both agents connect to localhost:${toString cf_port} by default.
      EOF
    '';
  }
  // {
    # Convenience access to both agents and their keys
    test-gray = {
      agent = test-gray.agent;
      publicKey = test-gray.publicKey;
    };
    test-lucas = {
      agent = test-lucas.agent;
      publicKey = test-lucas.publicKey;
    };
  }
