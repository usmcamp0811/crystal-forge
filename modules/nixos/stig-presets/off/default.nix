{
  lib,
  config,
  ...
}:
with lib; {
  options.crystal-forge.stig-presets.high = {
    enable = mkEnableOption "STIG High Security preset - enables all controls for maximum compliance";
  };

  config = mkIf config.crystal-forge.stig-presets.high.enable {
    crystal-forge.stig = {
      # Critical security controls
      ssh = {enable = false;};
      firewall = {enable = false;};
      audit = {enable = false;};
      apparmor = {enable = false;};
      pam = {enable = false;};
      pwquality = {enable = false;};
      sudo = {enable = false;};
      kernel = {enable = false;};

      # Access & authentication
      getty = {enable = false;};
      displaymanager = {enable = false;};
      sssd = {enable = false;};

      # Monitoring & logging
      syslog-ng = {enable = false;};
      cron = {enable = false;};
      usbguard = {enable = false;};
      timesyncd = {enable = false;};

      # Additional hardening
      aide = {enable = false;};
      wireless = {enable = false;};
      bluetooth = {enable = false;};

      # Environment
      account = {enable = false;};
      login = {enable = false;};
      packages = {enable = false;};

      # Nix configuration
      overlays = {enable = false;};
      settings = {enable = false;};
      dconf = {enable = false;};
      timeservers = {enable = false;};
    };
  };
}
