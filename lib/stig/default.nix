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
   *                      All values are automatically overridden with priority 1 to ensure
   *                      STIG compliance takes precedence over other module definitions.
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
   *           - Applies stigConfig with mkOverride priority 1 when enabled to override all other definitions
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
    # Apply mkOverride 1 (lowest numeric priority = highest precedence) to
    # every leaf value in stigConfig, ensuring active STIG controls beat all
    # user config including mkForce (priority 50) and ordinary definitions
    # (priority 100).
    #
    # NOTE on Nix priority semantics: LOWER number = HIGHER precedence.
    #   mkVMOverride    = mkOverride 10   (highest after ours)
    #   mkForce         = mkOverride 50
    #   normal defs     = priority 100
    #   mkDefault       = mkOverride 1000 (weak default)
    #   mkOptionDefault = mkOverride 1500 (lowest)
    #
    # Since stigConfig may contain any valid NixOS option-definition value,
    # including property wrappers such as mkOverride, mkForce, mkDefault,
    # mkBefore, mkAfter, mkIf, and mkMerge, we cannot use plain mapAttrsRecursive
    # (which would recurse into their internal _type/priority/content fields).
    #
    # Instead, overrideDefinition recurses INTO merge.contents and if.content
    # (transforming their children at STIG priority), unwraps and replaces
    # override priorities, and wraps whole order properties. The
    # mapAttrsRecursiveCond predicate stops before any known property wrapper
    # and hands control to the definition transformer.
    #
    # - merge:   recurse into each element of contents via overrideAttrs
    # - if:      recurse into content via overrideAttrs
    # - override: unwrap and re-apply at STIG priority
    # - order:   wrap the whole wrapper at STIG priority
    # - definition: preserve metadata, recurse into value
    overrideDefinition = value:
      if value ? _type then
        if value._type == "merge" then
          value // {
            contents = map overrideAttrs value.contents;
          }
        else if value._type == "if" then
          value // {
            content = overrideAttrs value.content;
          }
        else if value._type == "override" then
          mkOverride 1 value.content
        else if value._type == "order" then
          mkOverride 1 value
        else if value._type == "definition" then
          value // {
            value = overrideAttrs value.value;
          }
        else
          mkOverride 1 value
      else
        mkOverride 1 value;

    propertyTypes = [ "merge" "if" "override" "order" "definition" ];

    # Transform every value in a config attrset to wrap leaf values in
    # mkOverride 1 (STIG priority). Property wrappers (merge, if, override,
    # order, definition) are handled specially by overrideDefinition to
    # avoid corrupting their internal structure.
    #
    # Must check for property wrappers BEFORE calling mapAttrsRecursiveCond,
    # because mapAttrsRecursiveCond iterates all top-level keys of its input
    # and would pass _type, contents, etc. through overrideDefinition,
    # corrupting the wrapper's structure.
    overrideAttrs = value:
      if builtins.isAttrs value && builtins.elem (value._type or null) propertyTypes then
        overrideDefinition value
      else if builtins.isAttrs value then
        mapAttrsRecursiveCond
          (v: !(v ? _type && builtins.elem v._type propertyTypes))
          (_: overrideDefinition)
          value
      else
        mkOverride 1 value;
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
      (mkIf cfg.enable (overrideAttrs stigConfig))
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
