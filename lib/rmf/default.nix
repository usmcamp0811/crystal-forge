{ lib, ... }: rec {
  /**
  * Wraps a Nix package with RMF (Risk Management Framework) metadata and compliance logic.
  *
  * This function is intended to be used by software vendors to annotate their packages
  * with RMF control metadata, define system-level install modules, and expose
  * configuration options to downstream consumers (e.g., government agencies).
  *
  * Validates that all controls are either "met" or "waived", and ensures all non-"met"
  * controls are justified.
  *
  * @param pkg             The Nix package to wrap.
  * @param rmfMeta         RMF metadata including control mappings, status, and justifications.
  * @param installModule   Optional NixOS module function that configures the system when enabled.
  * @param moduleOptions   Optional NixOS module options to expose under `campground.rmf.<name>.settings`.
  *
  * @return A package with extended `meta.rmf`, `passthru.enforcedConfig`, and optionally
  *         `passthru.installModule` and `passthru.moduleOptions`.
  */
  wrapWithRMF =
    { pkg
    , rmfMeta
    , installModule ? null
    , moduleOptions ? { }
    ,
    }:
    let
      controlSet = rmfMeta.controls or { };

      # All controls must be either met or waived
      allResolved =
        builtins.all
          (c: builtins.elem controlSet.${c}.status [ "met" "waived" ])
          (builtins.attrNames controlSet);

      _0 =
        lib.asserts.assertMsg allResolved
          "All RMF controls must be either 'met' or 'waived'.";

      justificationRequired =
        !(rmfMeta.approved or false)
        || builtins.any (c: controlSet.${c}.status != "met")
          (builtins.attrNames controlSet);

      _1 =
        lib.asserts.assertMsg
          (!justificationRequired
            || builtins.all (c: controlSet.${c} ? justification)
            (builtins.attrNames controlSet))
          "Each non-met RMF control must include a justification.";

      enforcedConfig =
        lib.foldlAttrs
          (
            name: acc: val:
              if builtins.elem val.status [ "met" "waived" ] && val ? config
              then lib.recursiveUpdate acc val.config
              else acc
          )
          { }
          controlSet;
    in
    pkg.overrideAttrs (old: {
      meta =
        (old.meta or { })
        // {
          rmf = rmfMeta;
        };

      passthru =
        (old.passthru or { })
        // {
          enforcedConfig = enforcedConfig;
          inherit installModule moduleOptions;
        };
    });

  /**
  * Generates a NixOS module for enabling and configuring a wrapped RMF-compliant package.
  *
  * This function is intended to be used by system integrators (e.g., government consumers)
  * to declare whether to enforce a vendor’s wrapped package, configure its controls,
  * and optionally apply the vendor-defined install configuration and settings.
  *
  * Enables structured declaration of RMF control status and justification, enforces assertions,
  * and integrates the vendor's install logic conditionally based on `enable = true`.
  *
  * @param name    Unique identifier used to namespace the module (e.g., "example-flask-app").
  * @param pkg     A package previously wrapped by `wrapWithRMF`, which must include RMF metadata.
  * @param pkgs    The `pkgs` scope for injecting dependencies into the install module.
  * @param config  The global NixOS module `config` object for accessing user-defined settings.
  *
  * @return A NixOS module with:
  *         - options under `campground.rmf.${name}`:
  *           - `enable`: global toggle
  *           - `controls.<CONTROL>`: status + justification
  *           - `settings`: vendor-defined knobs
  *         - config that enforces controls and installs vendor system config if enabled
  */
  mkRmfModuleFromPackage =
    { name
    , pkg
    , pkgs
    , config
    ,
    }:
    let
      controls = pkg.meta.rmf.controls or { };
      forceAll = attrs: lib.mapAttrsRecursive (_: v: lib.mkForce v) attrs;

      buildControlModule = controlName: control:
        let
          ctrlCfg = config.campground.rmf.${name}.controls.${controlName} or { };
          pkgEnabled = config.campground.rmf.${name}.enable or false;
          enabled = (ctrlCfg ? enabled && ctrlCfg.enabled) || pkgEnabled;

          controlConfig = control.config or { };
          srg = control.srg or [ ];
          cci = control.cci or [ ];
        in
        {
          options = {
            controls.${controlName} = with lib.types; {
              enabled = lib.campground.mkBoolOpt true "Enable/Disable control ${controlName}";
              justification = lib.campground.mkOpt (listOf str) [ ] "Justification if disabled.";
            };
          };

          config = lib.mkMerge [
            (lib.mkIf enabled (forceAll controlConfig))

            {
              campground.controls.active.${name}.${controlName} = lib.mkIf enabled {
                inherit srg cci config;
              };

              campground.controls.inactive.${name}.${controlName} = lib.mkIf (!enabled) {
                inherit srg cci;
                justification = ctrlCfg.justification;
                config = controlConfig;
              };
            }

            {
              assertions = [
                {
                  assertion = (!enabled) -> (ctrlCfg.justification != [ ]);
                  message = "You must justify disabling ${controlName} for ${name}.";
                }
              ];
            }
          ];
        };

      entries = lib.mapAttrsToList buildControlModule controls;
      controlOptions = lib.mergeAttrsList (map (e: e.options) entries);
      controlConfigs = map (e: e.config) entries;

      installModule = pkg.passthru.installModule or (_: { config = { }; });
      moduleOptions = pkg.passthru.moduleOptions or { };
      enabled = config.campground.rmf.${name}.enable or false;
    in
    {
      options = {
        campground.rmf.${name} =
          {
            enable = lib.campground.mkBoolOpt true "Enable all controls for ${name}";
            settings = moduleOptions;
          }
          // controlOptions;
      };

      config = lib.mkMerge (
        controlConfigs
        ++ [
          (lib.mkIf enabled (
            (installModule {
              inherit config lib pkgs;
            }).config
          ))
        ]
      );
    };
}
