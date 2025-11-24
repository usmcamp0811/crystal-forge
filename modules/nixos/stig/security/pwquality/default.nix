{
  lib,
  config,
  pkgs,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "pwquality";
    srgList = [
      "SRG-OS-000069-GPOS-00037" # V-268126: uppercase character
      "SRG-OS-000070-GPOS-00038" # V-268127: lowercase character
      "SRG-OS-000071-GPOS-00039" # V-268128: numeric character
      "SRG-OS-000072-GPOS-00040" # V-268129: character change requirement
      "SRG-OS-000078-GPOS-00046" # V-268134: minimum password length
      "SRG-OS-000266-GPOS-00101" # V-268145: special character
      "SRG-OS-000480-GPOS-00225" # V-268169: dictionary word prevention
      "SRG-OS-000480-GPOS-00227" # V-268170: pwquality enablement
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268170
      security.pam.services.passwd.text = lib.mkDefault (
        lib.mkBefore "password requisite ${pkgs.libpwquality.lib}/lib/security/pam_pwquality.so"
      );
      security.pam.services.chpasswd.text = lib.mkDefault (
        lib.mkBefore "password requisite ${pkgs.libpwquality.lib}/lib/security/pam_pwquality.so"
      );
      security.pam.services.sudo.text = lib.mkDefault (
        lib.mkBefore "password requisite ${pkgs.libpwquality.lib}/lib/security/pam_pwquality.so"
      );

      environment.etc."/security/pwquality.conf".text = lib.strings.concatLines [
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268126
        ''
          ucredit=-1
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268127
        ''
          lcredit=-1
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268128
        ''
          dcredit=-1
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268129
        ''
          difok=8
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268134
        ''
          minlen=15
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268145
        ''
          ocredit=-1
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268169
        ''
          dictcheck=1
        ''
      ];
    };
  }
