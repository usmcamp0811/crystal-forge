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
    name = "sssd";
    srgList = [
      "SRG-OS-000066-GPOS-00034" # V-268124: validate certificates using PKI
      "SRG-OS-000403-GPOS-00182" # V-268124: certification path validation
      "SRG-OS-000775-GPOS-00230" # V-268124: trusted root certificate
      "SRG-OS-000383-GPOS-00166" # V-268178: prohibit cached authenticators after one day
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268124
      services.sssd.enable = true;
      environment.etc."sssd/pki/sssd_auth_ca_db.pem".source = let
        certzip = pkgs.fetchzip {
          url = "https://dl.dod.cyber.mil/wp-content/uploads/pki-pke/zip/unclass-certificates_pkcs7_v5-6_dod.zip";
          sha256 = "sha256-iwwJRXCnONk/LFddQlwy8KX9e9kVXW/QWDnX5qZFZJc=";
        };
      in "${certzip}/DoD_PKE_CA_chain.pem";
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268178
      services.sssd.config = ''
        [pam]
        offline_credentials_expiration = 1
      '';
    };
  }
