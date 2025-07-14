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
    actions = [
      # Start with initial state
      {
        type = "startup";
        derivationPath = "/nix/store/rjl1jl1s2s1b76fjqibh9llxrfij6b0s-nixos-system-gray-25.11.20250708.9807714";
      }

      # Heartbeat after 30 seconds
      {
        type = "heartbeat";
        delay = heartbeat_delay;
        derivationPath = "/nix/store/rjl1jl1s2s1b76fjqibh9llxrfij6b0s-nixos-system-gray-25.11.20250708.9807714";
      }

      # Another heartbeat after 30 more seconds
      {
        type = "heartbeat";
        delay = heartbeat_delay;
        derivationPath = "/nix/store/rjl1jl1s2s1b76fjqibh9llxrfij6b0s-nixos-system-gray-25.11.20250708.9807714";
      }

      # State change after 60 seconds (system update)
      {
        type = "config_change";
        derivationPath = "/nix/store/wfz1hffcar6anakhl0wlz90dn2gngryp-nixos-system-gray-25.05.20250619.005b89d";
        delay = config_delay;
      }

      # Final heartbeat
      {
        type = "heartbeat";
        derivationPath = "/nix/store/wfz1hffcar6anakhl0wlz90dn2gngryp-nixos-system-gray-25.05.20250619.005b89d";
        delay = heartbeat_delay;
      }
    ];
  };

  # Agent with custom action plan
  test-lucas = mkAgent {
    inherit pkgs;
    hostname = "test.lucas";
    serverHost = "localhost";
    serverPort = cf_port;

    privateKeyString = "uK2wOnCBjF8hOo3Ep8uy3UNpfM7aDHm/3K05tmbRt2o=";
    publicKeyString = "pwByU3iXjxGB/WP5hVEoR4eL/xsYWv1QmOdBHkIchnM=";
    actions = [
      # Start with initial state
      {
        type = "startup";
        derivationPath = "/nix/store/7jpnf4zpa92qhzi0qbvgapq15xs6bvj8-nixos-system-lucas-25.05.20250619.005b89d";
      }

      # Heartbeat after 30 seconds
      {
        type = "heartbeat";
        delay = heartbeat_delay;
        derivationPath = "/nix/store/7jpnf4zpa92qhzi0qbvgapq15xs6bvj8-nixos-system-lucas-25.05.20250619.005b89d";
      }

      {
        type = "startup";
        derivationPath = "/nix/store/7jpnf4zpa92qhzi0qbvgapq15xs6bvj8-nixos-system-lucas-25.05.20250619.005b89d";
      }

      {
        type = "config_change";
        derivationPath = "/nix/store/w30p4cmca85rzglsr2q33vn2m50l6yqy-nixos-system-lucas-25.11.20250708.9807714";
        delay = 60;
      }
      # Another heartbeat after 30 more seconds
      {
        type = "heartbeat";
        delay = heartbeat_delay;
        derivationPath = "/nix/store/w30p4cmca85rzglsr2q33vn2m50l6yqy-nixos-system-lucas-25.11.20250708.9807714";
      }

      # State change after 60 seconds (system update)
      {
        type = "config_change";
        derivationPath = "/nix/store/w30p4cmca85rzglsr2q33vn2m50l6yqy-nixos-system-lucas-25.11.20250708.9807714";
        delay = 60;
      }

      # Final heartbeat
      {
        type = "heartbeat";
        delay = heartbeat_delay;
        derivationPath = "/nix/store/w30p4cmca85rzglsr2q33vn2m50l6yqy-nixos-system-lucas-25.11.20250708.9807714";
      }
    ];
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
