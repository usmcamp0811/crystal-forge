{lib, ...}:
with lib; rec {
  /**
  * Creates a NixOS module for declarative STIG compliance configuration.
  *
  * This function generates a NixOS module that allows enabling/disabling individual
  * STIG controls with mandatory justification when disabled. When enabled, the control's
  * configuration is forcefully applied to prevent accidental gaps in compliance coverage.
  *
  * Tracks both active and inactive controls with their associated SRG, CCI, and
  * configuration metadata for audit and reporting purposes.
  *
  * @param name        Unique identifier for this STIG control (e.g., "account_expiry").
  * @param srgList     List of Security Requirements Guide (SRG) identifiers (default: []).
  * @param cciList     List of CCI (Control Correlation Identifier) mappings (default: []).
  * @param config      The global NixOS module `config` object for accessing settings.
  * @param stigConfig  NixOS configuration to apply when this control is enabled.
  *
  * @return A NixOS module with:
  *         - options under `crystal-forge.stig.${name}`:
  *           - `enable`: toggle for this control (defaults to global crystal-forge.stig.enable)
  *           - `justification`: list of strings explaining why disabled
  *         - config that enforces stigConfig when enabled and tracks state in active/inactive
  *         - assertion requiring justification if disabled while stig is globally enabled
  */
  mkStigModule = {
    name,
    srgList ? [],
    cciList ? [],
    config,
    stigConfig,
  }: let
    cfg = config.stig.${name};
    forceAttrs = attrs: mapAttrsRecursive (_: v: mkForce v) attrs;
  in {
    options.stig.${name} = with types; {
      enable =
        lib.crystal-forge.mkBoolOpt config.stig.enable
        "Enable/Disable ${name}";
      justification =
        lib.crystal-forge.mkOpt (listOf str) [] "Reasons why this is disabled.";
    };
    config = mkMerge [
      (mkIf cfg.enable (forceAttrs stigConfig))
      {
        stig = {
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
        assertions = [
          {
            assertion =
              (!cfg.enable && (config.stig.enable or false))
              -> (cfg.justification != []);
            message = "You must provide at least one justification if config.crystal-forge.stig.${name} is disabled.";
          }
        ];
      }
    ];
  };
}
