# Evaluate the real agent module and read its generated TOML, without running
# the config-copy script, agent, or any service. The nondefault delay forces
# emission of the deployment table, including the module's default "15m".
# deployment-config.toml is the resulting normal component-test fixture;
# deployment-config.sh regenerates and compares it before testing fresh output.
{root}: let
  flake = builtins.getFlake ("git+file://" + root);
  lib = flake.inputs.nixpkgs.lib;
  host = lib.nixosSystem {
    system = builtins.currentSystem;
    modules = [
      flake.nixosModules.crystal-forge
      {
        system.stateVersion = "26.05";
        services.crystal-forge = {
          enable = true;
          client.enable = true;
          client.private_key = "/unused-deployment-regression-key";
          server.enable = false;
          deployment.post_agent_start_deployment_delay = 0;
        };
      }
    ];
  };
  generators = builtins.filter
    (line: builtins.match "/nix/store/.*-generate-crystal-forge-config-.*" line != null)
    (lib.splitString "\n" host.config.systemd.services.crystal-forge-agent.preStart);
  script = builtins.readFile (assert builtins.length generators == 1; builtins.head generators);
  source = builtins.head (builtins.match ''.*cp "([^"]+)" "\$generatedConfigPath".*'' script);
in
  builtins.readFile source
