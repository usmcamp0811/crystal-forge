{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "overlays";
    srgList = [
      # aide overlay - https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268153
      "SRG-OS-000363-GPOS-00150"
      "SRG-OS-000445-GPOS-00199"
      "SRG-OS-000446-GPOS-00200"
      "SRG-OS-000447-GPOS-00201"
    ];
    cciList = [];
    stigConfig = {
      nixpkgs.overlays = [
        (final: prev: {
          # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268153
          aide = prev.aide.overrideAttrs (old: {
            configureFlags = (old.configureFlags or []) ++ ["--sysconfdir=/etc/aide"];
          });
        })
      ];
    };
  }
