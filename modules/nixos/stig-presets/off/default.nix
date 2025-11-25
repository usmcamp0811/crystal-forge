{
  lib,
  config,
  ...
}:
with lib; {
  options.crystal-forge.stig-presets.off = {
    enable = mkEnableOption "Disable all STIG controls";
    justification = mkOption {
      type = types.listOf types.str;
      default = ["Disabled via stig-presets.off"];
      description = "Justification for disabling all STIG controls";
    };
  };
  config = let
    cfg = config.crystal-forge.stig-presets.off;
    disabledControl = {
      enable = false;
      justification = cfg.justification;
    };
  in
    mkIf cfg.enable {
      crystal-forge.stig = {
        # Critical security controls
        ssh = disabledControl;
        firewall = disabledControl;
        audit = disabledControl;
        apparmor = disabledControl;
        pam = disabledControl;
        pwquality = disabledControl;
        sudo = disabledControl;
        kernel = disabledControl;

        # Access & authentication
        getty = disabledControl;
        displaymanager = disabledControl;
        sssd = disabledControl;

        # Monitoring & logging
        syslog-ng = disabledControl;
        cron = disabledControl;
        usbguard = disabledControl;
        timesyncd = disabledControl;

        # Additional hardening
        aide = disabledControl;
        wireless = disabledControl;
        bluetooth = disabledControl;

        # Environment
        account = disabledControl;
        login = disabledControl;
        packages = disabledControl;

        # Nix configuration
        overlays = disabledControl;
        settings = disabledControl;
        dconf = disabledControl;
        timeservers = disabledControl;
      };
    };
}
