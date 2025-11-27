{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "kernel";
    srgList = [
      "SRG-OS-000478-GPOS-00223"
      "SRG-OS-000396-GPOS-00176"
      "SRG-OS-000042-GPOS-00020"
      "SRG-OS-000341-GPOS-00132"
      "SRG-OS-000142-GPOS-00071"
      "SRG-OS-000433-GPOS-00192"
      "SRG-OS-000132-GPOS-00067"
      "SRG-OS-000433-GPOS-00193"
    ];
    cciList = []; # TODO: CCI values not available on stigui.com; check the STIG document directly
    stigConfig = {
      boot.kernelParams = [
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268168
        "fips=1"
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268092
        "audit=1"
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268093
        "audit_backlog_limit=8192"
      ];
      boot.kernel.sysctl = {
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268141
        "net.ipv4.tcp_syncookies" = 1;
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268160
        "kernel.kptr_restrict" = 1;
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268161
        "kernel.randomize_va_space" = 2;
      };
    };
  }
