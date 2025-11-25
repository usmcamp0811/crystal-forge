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
  *           - `enable`: toggle for this control (defaults to true)
  *           - `justification`: list of strings explaining why disabled
  *         - config that enforces stigConfig when enabled and tracks state in active/inactive
  *         - assertion requiring justification if disabled
  */
  mkStigModule = {
    name,
    srgList ? [],
    cciList ? [],
    config,
    stigConfig,
    extraOptions ? {}, # NEW: additional options to define
  }: let
    cfg = config.crystal-forge.stig.${name};
    forceAttrs = attrs: mapAttrsRecursive (_: v: mkForce v) attrs;
  in {
    options =
      extraOptions
      // {
        crystal-forge.stig = with types; {
          active = mkOption {
            type = attrsOf (attrsOf anything);
            default = {};
            description = "Tracking of active STIG controls with their SRG, CCI, and config";
          };
          inactive = mkOption {
            type = attrsOf (attrsOf anything);
            default = {};
            description = "Tracking of inactive STIG controls with justifications";
          };
          ${name} = {
            enable = mkOption {
              type = bool;
              default = true;
              description = "Enable STIG control ${name}";
            };
            justification = mkOption {
              type = listOf str;
              default = [];
              description = "Reasons why this control is disabled";
            };
          };
        };
      };
    config = mkMerge [
      (mkIf cfg.enable (forceAttrs stigConfig))
      {
        crystal-forge.stig = {
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
            assertion = (!cfg.enable) -> (cfg.justification != []);
            message = "You must provide justification if config.crystal-forge.stig.${name} is disabled.";
          }
        ];
      }
    ];
  };
}
