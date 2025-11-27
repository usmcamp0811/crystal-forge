{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "wireless";
    srgList = [
      "SRG-OS-000299-GPOS-00117"
      "SRG-OS-000481-GPOS-00481"
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268146
      networking.wireless.enable = false;
    };
  }
