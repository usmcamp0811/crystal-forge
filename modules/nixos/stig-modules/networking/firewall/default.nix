{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "firewall";
    srgList = [
      # Enable firewall - https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268078
      "SRG-OS-000298-GPOS-00116"
      "SRG-OS-000096-GPOS-00050"
      "SRG-OS-000297-GPOS-00115"
      "SRG-OS-000480-GPOS-00232"
      # Rate limiting - https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268158
      "SRG-OS-000420-GPOS-00186"
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268078
      networking.firewall.enable = true;
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268158
      networking.firewall.extraCommands = ''
        ip46tables --append INPUT --protocol tcp --dport 22 --match hashlimit --hashlimit-name stig_byte_limit --hashlimit-mode srcip --hashlimit-above 1000000b/second --jump nixos-fw-refuse
        ip46tables --append INPUT --protocol tcp --dport 80 --match hashlimit --hashlimit-name stig_conn_limit --hashlimit-mode srcip --hashlimit-above 1000/minute --jump nixos-fw-refuse
        ip46tables --append INPUT --protocol tcp --dport 443 --match hashlimit --hashlimit-name stig_conn_limit --hashlimit-mode srcip --hashlimit-above 1000/minute --jump nixos-fw-refuse
      '';
    };
  }
