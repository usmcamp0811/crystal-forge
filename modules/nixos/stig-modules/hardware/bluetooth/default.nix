{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "bluetooth";
    srgList = [
      "SRG-OS-000300-GPOS-00118"
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268147
      hardware.bluetooth.enable = false;
    };
  }
