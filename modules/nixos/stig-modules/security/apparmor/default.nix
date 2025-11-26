{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "apparmor";
    srgList = [
      "SRG-OS-000480-GPOS-00230"
      "SRG-OS-000368-GPOS-00154"
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268173
      security.apparmor.enable = true;
    };
  }
