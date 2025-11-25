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
  * @param name          Unique identifier for this STIG control (e.g., "banner", "ssh").
  *                      Used to namespace the control under `crystal-forge.stig.${name}`.
  *
  * @param srgList       List of Security Requirements Guide (SRG) identifiers mapped to this control.
  *                      Example: ["SRG-OS-000023-GPOS-00006"] (default: []).
  *
  * @param cciList       List of CCI (Control Correlation Identifier) mappings for this control.
  *                      Used for compliance tracking and reporting (default: []).
  *
  * @param config        The global NixOS module `config` object for accessing control settings
  *                      and other system configuration. Required for accessing cfg values.
  *
  * @param stigConfig    NixOS configuration attrset to apply when this control is enabled.
  *                      This can include any valid NixOS configuration options
  *                      (services, security, environment, etc.).
  *
  * @param extraOptions  Additional NixOS module options to define for this control.
  *                      Use this to declare custom configuration options that downstream
  *                      modules need to set. These appear at the top-level module scope
  *                      (e.g., services.syslog-ng.remote_hosts).
  *                      Default: {} (empty attrset).
  *
  * @return A NixOS module with:
  *         - options:
  *           - All extraOptions (if provided)
  *           - crystal-forge.stig.active: attrset tracking enabled controls
  *           - crystal-forge.stig.inactive: attrset tracking disabled controls with justifications
  *           - crystal-forge.stig.${name}.enable: boolean toggle (defaults to true)
  *           - crystal-forge.stig.${name}.justification: list of strings (required if disabled)
  *
  *         - config:
  *           - Applies stigConfig with mkForce when enabled to prevent accidental overrides
  *           - Populates crystal-forge.stig.active.${name} with srg, cci, and config when enabled
  *           - Populates crystal-forge.stig.inactive.${name} with srg, cci, justification, and config when disabled
  *           - Enforces assertion: disabled controls must have justification provided
  *
  * @example
  *   mkStigModule {
  *     inherit config;
  *     name = "ssh";
  *     srgList = ["SRG-OS-000423-GPOS-00187" "SRG-OS-000033-GPOS-00014"];
  *     cciList = [];
  *     extraOptions = {};
  *     stigConfig = {
  *       services.openssh.enable = true;
  *       services.openssh.settings.PermitRootLogin = "no";
  *     };
  *   }
  *
  * @example With extraOptions:
  *   mkStigModule {
  *     inherit config;
  *     name = "syslog-ng";
  *     srgList = ["SRG-OS-000051-GPOS-00024"];
  *     cciList = [];
  *     extraOptions = {
  *       services.syslog-ng = {
  *         remote_hosts = mkOption {
  *           type = types.listOf types.str;
  *           description = "Remote hosts for log forwarding";
  *         };
  *         remote_tls = mkOption {
  *           type = types.bool;
  *           default = true;
  *           description = "Use TLS for remote log forwarding";
  *         };
  *       };
  *     };
  *     stigConfig = {
  *       services.syslog-ng.enable = true;
  *       services.syslog-ng.extraConfig = "...";
  *     };
  *   }
  */
  mkStigModule = {
    name,
    srgList ? [],
    cciList ? [],
    config,
    stigConfig,
    extraOptions ? {},
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
            description = "Tracking of active STIG controls with their SRG, CCI, and applied configuration";
          };
          inactive = mkOption {
            type = attrsOf (attrsOf anything);
            default = {};
            description = "Tracking of inactive STIG controls with justifications and unapplied configuration";
          };
          ${name} = {
            enable = mkOption {
              type = bool;
              default = true;
              description = "Enable STIG control '${name}'. Defaults to true for secure-by-default behavior.";
            };
            justification = mkOption {
              type = listOf str;
              default = [];
              description = "Mandatory justification for why this control is disabled. Required if enable = false.";
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
