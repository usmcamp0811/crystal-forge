{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "timeservers";
    srgList = [
      "SRG-OS-000355-GPOS-00143"
      "SRG-OS-000359-GPOS-00146"
      "SRG-OS-000785-GPOS-00250"
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268149
      networking.timeServers = [
        "tick.usnogps.navy.mil"
        "tock.usnogps.navy.mil"
      ];
    };
  }
