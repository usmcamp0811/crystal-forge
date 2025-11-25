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
      ssh = {enable = true;};
      firewall = {enable = true;};
      audit = {enable = true;};
      apparmor = {enable = true;};
      pam = {enable = true;};
      pwquality = {enable = true;};
      sudo = {enable = true;};
      kernel = {enable = true;};

      # Access & authentication
      getty = {enable = true;};
      displaymanager = {enable = true;};
      sssd = {enable = true;};

      # Monitoring & logging
      syslog-ng = {enable = true;};
      cron = {enable = true;};
      usbguard = {enable = true;};
      timesyncd = {enable = true;};

      # Additional hardening
      aide = {enable = true;};
      wireless = {enable = true;};
      bluetooth = {enable = true;};

      # Environment
      account = {enable = true;};
      login = {enable = true;};
      packages = {enable = true;};

      # Nix configuration
      overlays = {enable = true;};
      settings = {enable = true;};
      dconf = {enable = true;};
      timeservers = {enable = true;};
    };
  };
}
