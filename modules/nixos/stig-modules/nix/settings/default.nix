{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "settings";
    srgList = [
      # require-sigs - https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268154
      "SRG-OS-000366-GPOS-00153"
      # allowed-users - https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268152
      "SRG-OS-000362-GPOS-00149"
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268154
      nix.settings.require-sigs = true;
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268152
      nix.settings.allowed-users = [
        "root"
        "@wheel"
      ];
    };
  }
