{ lib, inputs, ... }:
with lib; rec {
  mkStigModule = { name, srgList ? [ ], cciList ? [ ], config, stigConfig }:
    let
      cfg = config.campground.stig.${name};
      forceAttrs = attrs: mapAttrsRecursive (_: v: mkForce v) attrs;

    in
    {
      options.campground.stig.${name} = with types; {
        enable = lib.campground.mkBoolOpt config.campground.stig.enable
          "Enable/Disable ${name}";
        justification =
          lib.campground.mkOpt (listOf str) [ ] "Reasons why this is disabled.";
      };

      config = mkMerge [
        (mkIf cfg.enable (forceAttrs stigConfig))

        {
          campground.stig = {
            active.${name} = mkIf cfg.enable {
              srg = srgList;
              cci = cciList;
              config = stigConfig;
            };

            inactive.${name} = mkIf (!cfg.enable) {
              srg = srgList;
              cci = cciList;
              justification = cfg.justification;
              config = stigConfig;
            };
          };

          assertions = [{
            assertion = (!cfg.enable && config.campground.stig.enable == true)
              -> (cfg.justification != [ ]);
            message =
              "You must provide at least one justification if config.campground.stig.${name} is disabled.";
          }];
        }
      ];
    };
}
