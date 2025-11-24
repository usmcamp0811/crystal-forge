{lib, ...}: {
  options.crystal-forge.stig = with lib.types; {
    active = lib.mkOption {
      type = attrsOf (attrsOf anything);
      default = {};
      description = "Tracking of active STIG controls with their SRG, CCI, and config";
    };
    inactive = lib.mkOption {
      type = attrsOf (attrsOf anything);
      default = {};
      description = "Tracking of inactive STIG controls with justifications";
    };
  };

  imports = [
    ./banner
  ];
}
