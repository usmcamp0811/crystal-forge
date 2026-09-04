{ flakeRef, policyCheckers, requestedRevision }:

let
  flake = builtins.getFlake flakeRef;
  cfAgentEnabled = config:
    (config.systemd.services.crystal-forge-agent.enable or false)
    || ((config.services.crystal-forge.enable or false)
      && (config.services.crystal-forge.client.enable or false));
in
builtins.mapAttrs
  (name: configuration:
    let
      system = configuration.config.system.build.toplevel;
      checkPolicies = policyCheckers.${name} or (_: { });
    in
    system // {
      meta = {
        policies = (checkPolicies configuration.config) // {
          cfAgentEnabled = cfAgentEnabled configuration.config;
          requestedSourceRevision = requestedRevision;
          resolvedSourceRevision = flake.sourceInfo.rev or null;
        };
      };
    })
  flake.nixosConfigurations
