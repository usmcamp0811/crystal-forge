{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "timesyncd";
    srgList = [
      "SRG-OS-000356-GPOS-00144" # V-268151: time synchronization enabled
      "SRG-OS-000356-GPOS-00144" # V-268150: synchronize with authoritative time source
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268151
      services.timesyncd.enable = mkForce true;
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268150
      services.timesyncd.extraConfig = ''
        PollIntervalMaxSec=60
      '';
    };
  }
