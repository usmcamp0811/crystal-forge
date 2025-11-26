{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "sudo";
    srgList = [
      "SRG-OS-000480-GPOS-00227" # V-268155: reauthenticate for privilege escalation
      "SRG-OS-000480-GPOS-00227" # V-268156: reauthenticate when changing roles
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268155
      security.sudo.extraConfig = ''
        Defaults timestamp_timeout=0
      '';
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268156
      security.sudo.wheelNeedsPassword = true;
    };
  }
