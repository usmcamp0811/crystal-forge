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
  * Supports version-gated configuration for cross-version NixOS compatibility,
  * allowing different configuration to be applied based on the target NixOS release.
  *
  * @param name                Unique identifier for this STIG control (e.g., "banner", "ssh").
  *                            Used to namespace the control under `crystal-forge.stig.${name}`.
  *
  * @param srgList             List of Security Requirements Guide (SRG) identifiers mapped to this control.
  *                            Example: ["SRG-OS-000023-GPOS-00006"] (default: []).
  *
  * @param cciList             List of CCI (Control Correlation Identifier) mappings for this control.
  *                            Used for compliance tracking and reporting (default: []).
  *
  * @param config              The global NixOS module `config` object for accessing control settings
  *                            and other system configuration. Required for accessing cfg values and
  *                            system.nixos.release for version checking.
  *
  * @param stigConfig          Base NixOS configuration attrset to apply when this control is enabled.
  *                            This can include any valid NixOS configuration options
  *                            (services, security, environment, etc.).
  *                            Version-specific overrides from versionedStigConfig are merged on top.
  *
  * @param extraOptions        Additional NixOS module options to define for this control.
  *                            Use this to declare custom configuration options that downstream
  *                            modules need to set. These appear at the top-level module scope
  *                            (e.g., services.syslog-ng.remote_hosts).
  *                            Default: {} (empty attrset).
  *
  * @param versionedStigConfig Version-specific configuration overrides for cross-NixOS-version compatibility.
  *                            An attrset where keys are version strings (e.g., "25.05", "24.11")
  *                            and values are configuration attrsets to merge when that version or later is detected.
  *                            This is useful when NixOS option namespaces change between releases.
  *                            Example: { "25.05" = { services.displayManager.gdm.banner = "..."; }; }
  *                            Default: {} (empty attrset, no version-specific overrides).
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
  *           - Applies merged stigConfig + versionedStigConfig with mkForce when enabled
  *           - Populates crystal-forge.stig.active.${name} with srg, cci, and final merged config when enabled
  *           - Populates crystal-forge.stig.inactive.${name} with srg, cci, justification, and config when disabled
  *           - Enforces assertion: disabled controls must have justification provided
  *
  * @example Basic usage:
  *   mkStigModule {
  *     inherit config;
  *     name = "ssh";
  *     srgList = ["SRG-OS-000423-GPOS-00187" "SRG-OS-000033-GPOS-00014"];
  *     cciList = [];
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
  *       };
  *     };
  *     stigConfig = {
  *       services.syslog-ng.enable = true;
  *       services.syslog-ng.extraConfig = "...";
  *     };
  *   }
  *
  * @example With version-gated configuration:
  *   mkStigModule {
  *     inherit config;
  *     name = "displaymanager";
  *     srgList = ["SRG-OS-000480-GPOS-00229" "SRG-OS-000023-GPOS-00006"];
  *     cciList = [];
  *     stigConfig = {
  *       services.displayManager.autoLogin.user = null;
  *     };
  *     versionedStigConfig = {
  *       # Applied to NixOS 25.05 unstable (services.displayManager.gdm namespace)
  *       "25.05" = {
  *         services.displayManager.gdm.banner = "...";
  *       };
  *       # Applied to NixOS 24.11 and earlier (services.xserver.displayManager.gdm namespace)
  *       "24.11" = {
  *         services.xserver.displayManager.gdm.banner = "...";
  *       };
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
    versionedStigConfig ? {},
  }: let
    cfg = config.crystal-forge.stig.${name};
    forceAttrs = attrs: mapAttrsRecursive (_: v: mkForce v) attrs;

    # Apply versioned configs based on release version
    # Iterates through all version keys and applies configs for versions >= current release
    appliedVersionedConfig = lib.foldl (
      acc: version:
        if lib.versionAtLeast config.system.nixos.release version
        then lib.recursiveUpdate acc versionedStigConfig.${version}
        else acc
    ) {} (builtins.attrNames versionedStigConfig);

    # Final config is base stigConfig merged with version-specific overrides
    finalStigConfig = lib.recursiveUpdate stigConfig appliedVersionedConfig;
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
      (mkIf cfg.enable (forceAttrs finalStigConfig))
      {
        crystal-forge.stig = {
          active.${name} = mkIf cfg.enable {
            srg = srgList;
            cci = cciList;
            config = finalStigConfig;
          };
          inactive.${name} = mkIf (!cfg.enable) {
            srg = srgList;
            cci = cciList;
            justification = cfg.justification;
            config = finalStigConfig;
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
