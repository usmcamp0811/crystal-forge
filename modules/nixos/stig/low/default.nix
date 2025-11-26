{
  lib,
  config,
  ...
}:
with lib; {
  options.crystal-forge.stig-presets.low = {
    enable = mkEnableOption "STIG Low Security preset - minimal compliance controls";
  };

  config = mkIf config.crystal-forge.stig-presets.low.enable {
    crystal-forge.stig = {
      # Essential only
      ssh = {enable = true;};
      firewall = {enable = true;};
      pam = {enable = true;};
      sudo = {enable = true;};

      # Legal compliance
      timesyncd = {enable = true;};

      # Everything else disabled
      audit = {
        enable = false;
        justification = ["Audit logging disabled for low security"];
      };
      apparmor = {
        enable = false;
        justification = ["AppArmor not required"];
      };
      pwquality = {
        enable = false;
        justification = ["Password quality not enforced"];
      };
      boot.kernel = {
        enable = false;
        justification = ["Kernel hardening disabled"];
      };
      getty = {
        enable = false;
        justification = ["Console login controls not required"];
      };
      displaymanager = {
        enable = false;
        justification = ["Display manager controls not required"];
      };
      sssd = {
        enable = false;
        justification = ["PKI auth not required"];
      };
      syslog-ng = {
        enable = false;
        justification = ["Log forwarding not required"];
      };
      cron = {
        enable = false;
        justification = ["Integrity monitoring disabled"];
      };
      usbguard = {
        enable = false;
        justification = ["USB controls not required"];
      };
      aide = {
        enable = false;
        justification = ["File integrity disabled"];
      };
      wireless = {
        enable = false;
        justification = ["Not applicable"];
      };
      bluetooth = {
        enable = false;
        justification = ["Not applicable"];
      };
      account = {
        enable = false;
        justification = ["Account controls disabled"];
      };
      login = {
        enable = false;
        justification = ["Login controls disabled"];
      };
      packages = {
        enable = false;
        justification = ["Package controls disabled"];
      };
      overlays = {
        enable = false;
        justification = ["Overlay controls disabled"];
      };
      settings = {
        enable = false;
        justification = ["Nix settings controls disabled"];
      };
      dconf = {
        enable = false;
        justification = ["GNOME controls disabled"];
      };
      timeservers = {
        enable = false;
        justification = ["Timeserver controls disabled"];
      };
    };
  };
}
