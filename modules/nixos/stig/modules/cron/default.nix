{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "cron";
    srgList = [
      "SRG-OS-000363-GPOS-00150" # V-268153: notify of unauthorized configuration changes
      "SRG-OS-000445-GPOS-00199" # V-268153: unauthorized modification notification
      "SRG-OS-000446-GPOS-00200" # V-268153: unauthorized modification notification
      "SRG-OS-000447-GPOS-00201" # V-268153: unauthorized modification notification
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268153
      services.cron = {
        enable = true;
        systemCronJobs = [
          "00 0 * * 0\troot\taide -c /etc/aide/aide.conf --check | /bin/mail -s \"aide integrity check run for ${config.networking.hostName}\" root@notareal.email"
        ];
      };
    };
  }
