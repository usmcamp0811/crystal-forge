# modules/nixos/stig/presets/medium.nix
{
  lib,
  config,
  ...
}:
with lib; {
  options.crystal-forge.stig-presets.medium = {
    enable = mkEnableOption "STIG Medium Security preset - balanced security and usability";
  };

  config = mkIf config.crystal-forge.stig-presets.medium.enable {
    crystal-forge.stig = {
      # Essential security controls
      ssh = {enable = true;};
      firewall = {enable = true;};
      audit = {enable = true;};
      pam = {enable = true;};
      pwquality = {enable = true;};
      sudo = {enable = true;};
      kernel = {enable = true;};

      # Access & authentication
      banner = {enable = true;};
      getty = {enable = true;};
      displaymanager = {enable = true;};

      # Monitoring & logging
      syslog-ng = {enable = true;};
      timesyncd = {enable = true;};
      usbguard = {enable = true;};

      # Additional security
      cron = {
        enable = true;
        justification = ["File integrity monitoring"];
      };
      apparmor = {
        enable = false;
        justification = ["AppArmor overhead in development"];
      };

      # PKI authentication (optional)
      sssd = {
        enable = false;
        justification = ["PKI auth not required in dev"];
      };

      # File integrity (optional)
      aide = {
        enable = false;
        justification = ["Integrity checking not critical in dev"];
      };

      # Hardware
      wireless = {
        enable = false;
        justification = ["Not applicable"];
      };
      bluetooth = {
        enable = false;
        justification = ["Not applicable"];
      };

      # Environment (disabled)
      account = {
        enable = false;
        justification = ["Account policies not critical"];
      };
      login = {
        enable = false;
        justification = ["Login policies not critical"];
      };
      packages = {
        enable = false;
        justification = ["Custom package controls not required"];
      };

      # Nix configuration (disabled)
      overlays = {
        enable = false;
        justification = ["Overlay controls not critical"];
      };
      settings = {
        enable = false;
        justification = ["Nix settings not critical"];
      };
      dconf = {
        enable = false;
        justification = ["GNOME-specific controls not needed"];
      };
      timeservers = {
        enable = false;
        justification = ["Default timeservers acceptable"];
      };
    };
  };
}
