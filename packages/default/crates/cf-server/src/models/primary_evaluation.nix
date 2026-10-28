{ flakeRef
, policyCheckers
, requestedRevision
, resolvedRevisionOverride ? null
, configurationNames ? null
, shallowObserver ? null
}:

let
  flake = builtins.getFlake flakeRef;
  cfAgentEnabled = config:
    (config.systemd.services.crystal-forge-agent.enable or false)
    || ((config.services.crystal-forge.enable or false)
      && (config.services.crystal-forge.client.enable or false));
  selectedConfigurations =
    if configurationNames == null then
      flake.nixosConfigurations
    else
      builtins.intersectAttrs
        (builtins.listToAttrs (builtins.map
          (name: { inherit name; value = null; })
          configurationNames))
        flake.nixosConfigurations;
in
builtins.mapAttrs
  (name: cfg:
    let
      system = cfg.config.system.build.toplevel;
      checker = policyCheckers.${name} or (_: { });
      rootAttempt =
        if shallowObserver == null then { success = false; }
        else builtins.tryEval (
          let root = shallowObserver {
            configuration = cfg;
            operation = "root";
            path = [ ];
            childOffset = 0;
          };
          in builtins.deepSeq root root
        );
    in
    system // {
      meta = {
        policies = (checker cfg.config) // {
          cfAgentEnabled = cfAgentEnabled cfg.config;
          requestedSourceRevision = requestedRevision;
          resolvedSourceRevision =
            if resolvedRevisionOverride != null
            then resolvedRevisionOverride
            else flake.sourceInfo.rev or null;
        };
        crystalForgeConfigRoot = if rootAttempt.success then {
          status = "available";
          payload = rootAttempt.value;
        } else {
          status = "failed";
          code = "root_capture_failed";
        };
      };
    })
  selectedConfigurations
