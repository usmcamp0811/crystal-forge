{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "dconf";
    srgList = [
      "SRG-OS-000029-GPOS-00010"
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268086
      programs.dconf.profiles.user.databases = with lib.gvariant; [
        {
          settings."org/gnome/desktop/session".idle-delay = mkUint32 600;
          locks = ["org/gnome/desktop/session/idle-delay"];
        }
      ];
    };
  }
