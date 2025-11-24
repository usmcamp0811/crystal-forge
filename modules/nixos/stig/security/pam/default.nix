{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "pam";
    srgList = [
      # V-268081 - faillock
      "SRG-OS-000021-GPOS-00005"
      "SRG-OS-000329-GPOS-00128"
      "SRG-OS-000470-GPOS-00214"
      # V-268177 - pam.p11
      "SRG-OS-000375-GPOS-00160"
      "SRG-OS-000068-GPOS-00036"
      "SRG-OS-000376-GPOS-00161"
      "SRG-OS-000377-GPOS-00162"
      "SRG-OS-000705-GPOS-00150"
      # V-268179 - pam_pkcs11
      "SRG-OS-000384-GPOS-00167"
      # V-268085 - loginLimits
      "SRG-OS-000027-GPOS-00008"
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268081
      security.pam.services = let
        pamfile = ''
          auth required pam_faillock.so preauth silent audit deny=3 fail_interval=900 unlock_time=0
          auth sufficient pam_unix.so nullok try_first_pass
          auth [default=die] pam_faillock.so authfail audit deny=3 fail_interval=900 unlock_time=0
          auth sufficient pam_faillock.so authsucc
          account required pam_faillock.so
        '';
      in {
        login.text = mkDefault pamfile;
        sshd.text = mkDefault pamfile;
      };
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268177
      security.pam.p11.enable = true;
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268179
      environment.etc."pam_pkcs11/pam_pkcs11.conf".text = mkDefault ''
        cert_policy = ca,signature,ocsp_on,crl_auto;
      '';
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268085
      security.pam.loginLimits = [
        {
          domain = "*";
          item = "maxlogins";
          type = "hard";
          value = "10";
        }
      ];
    };
  }
