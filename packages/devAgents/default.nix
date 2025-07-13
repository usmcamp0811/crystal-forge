{
  pkgs,
  lib,
  ...
}:
with lib;
with lib.crystal-forge; let
  agent1 = mkAgent {
    inherit pkgs;
    hostname = "agent1";
    serverHost = "localhost";
    serverPort = cf_port;
  };

  # Agent with custom action plan
  agent2 = mkAgent {
    inherit pkgs;
    hostname = "agent2";
    serverHost = "localhost";
    serverPort = cf_port;
    actions = [
      # Start with initial state
      {
        type = "startup";
        derivationPath = "/nix/store/nixos-system-v1.2.3";
      }

      # Heartbeat after 30 seconds
      {
        type = "heartbeat";
        delay = 30;
      }

      # Another heartbeat after 30 more seconds
      {
        type = "heartbeat";
        delay = 30;
      }

      # State change after 60 seconds (system update)
      {
        type = "config_change";
        derivationPath = "/nix/store/nixos-system-v1.2.4";
        delay = 60;
      }

      # Final heartbeat
      {
        type = "heartbeat";
        delay = 30;
      }
    ];
  };
  agents = {inherit agent1 agent2;};
in
  agents
