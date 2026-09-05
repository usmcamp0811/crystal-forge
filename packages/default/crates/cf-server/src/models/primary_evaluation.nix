{ flakeRef, policyCheckers, requestedRevision }:

let
  flake = builtins.getFlake flakeRef;
  cfAgentEnabled = config:
    (config.systemd.services.crystal-forge-agent.enable or false)
    || ((config.services.crystal-forge.enable or false)
      && (config.services.crystal-forge.client.enable or false));
in
builtins.mapAttrs
  (name: cfg:
    let
      system = cfg.config.system.build.toplevel;
      checker = policyCheckers.${name} or (_: { });
    in
    system // {
      meta = {
        policies = (checker cfg.config) // {
          cfAgentEnabled = cfAgentEnabled cfg.config;
          requestedSourceRevision = requestedRevision;
          resolvedSourceRevision = flake.sourceInfo.rev or null;
        };
      };
    })
  flake.nixosConfigurations
