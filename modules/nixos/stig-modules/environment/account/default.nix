{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "account";
    srgList = [
      "SRG-OS-000118-GPOS-00060"
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268174
      environment.etc."/default/useradd".text = mkForce ''
        INACTIVE=35
      '';
    };
  }
