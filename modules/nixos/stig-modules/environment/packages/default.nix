{
  lib,
  config,
  pkgs,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "packages";
    srgList = [
      # vlock - https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268087
      "SRG-OS-000030-GPOS-00011"
      "SRG-OS-000028-GPOS-00009"
      "SRG-OS-000031-GPOS-00012"
      # audit - https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268090
      "SRG-OS-000037-GPOS-00015"
      "SRG-OS-000038-GPOS-00016"
      "SRG-OS-000039-GPOS-00017"
      "SRG-OS-000040-GPOS-00018"
      "SRG-OS-000041-GPOS-00019"
      "SRG-OS-000042-GPOS-00021"
      "SRG-OS-000054-GPOS-00025"
      "SRG-OS-000055-GPOS-00026"
      "SRG-OS-000058-GPOS-00028"
      "SRG-OS-000059-GPOS-00029"
      "SRG-OS-000239-GPOS-00089"
      "SRG-OS-000240-GPOS-00090"
      "SRG-OS-000241-GPOS-00091"
      "SRG-OS-000255-GPOS-00096"
      "SRG-OS-000303-GPOS-00120"
      "SRG-OS-000327-GPOS-00127"
      # opencryptoki - https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268136
      "SRG-OS-000105-GPOS-00052"
      "SRG-OS-000106-GPOS-00053"
      "SRG-OS-000107-GPOS-00054"
      "SRG-OS-000108-GPOS-00055"
      # aide - https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268153
      "SRG-OS-000363-GPOS-00150"
      "SRG-OS-000445-GPOS-00199"
      "SRG-OS-000446-GPOS-00200"
      "SRG-OS-000447-GPOS-00201"
    ];
    cciList = [];
    stigConfig = {
      environment.systemPackages = with pkgs; [
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268087
        vlock
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268090
        audit
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268136
        opencryptoki
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268153
        aide
      ];
    };
  }
