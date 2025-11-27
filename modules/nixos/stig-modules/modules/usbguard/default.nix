{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "usbguard";
    srgList = [
      "SRG-OS-000114-GPOS-00059" # V-268139: identify devices
      "SRG-OS-000378-GPOS-00163" # V-268139: control USB devices
      "SRG-OS-000690-GPOS-00140" # V-268139: prevent unauthorized peripheral access
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268139
      services.usbguard.enable = true;
    };
  }
