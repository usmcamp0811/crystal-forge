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
    name = "ssh";
    srgList = [
      "SRG-OS-000423-GPOS-00187" # V-268159: protect transmitted information confidentiality and integrity
      "SRG-OS-000112-GPOS-00057" # V-268159: network encryption
      "SRG-OS-000113-GPOS-00058" # V-268159: network encryption
      "SRG-OS-000424-GPOS-00188" # V-268159: network encryption
      "SRG-OS-000425-GPOS-00189" # V-268159: network encryption
      "SRG-OS-000426-GPOS-00190" # V-268159: network encryption
      "SRG-OS-000033-GPOS-00014" # V-268089: DOD-approved encryption for remote access
      "SRG-OS-000250-GPOS-00093" # V-268089: encryption algorithms
      "SRG-OS-000394-GPOS-00174" # V-268089: encryption algorithms
      "SRG-OS-000393-GPOS-00173" # V-268157: cryptographic mechanisms for integrity of nonlocal maintenance
      "SRG-OS-000125-GPOS-00065" # V-268176: strong authenticators for nonlocal maintenance
      "SRG-OS-000023-GPOS-00006" # V-268083: display DOD Notice and Consent Banner via SSH
      "SRG-OS-000032-GPOS-00013" # V-268088: monitor remote access methods
      "SRG-OS-000109-GPOS-00056" # V-268137: prevent direct root login via SSH
      "SRG-OS-000163-GPOS-00072" # V-268142: terminate unresponsive SSH connections
      "SRG-OS-000279-GPOS-00109" # V-268142: terminate idle SSH sessions
      "SRG-OS-000395-GPOS-00175" # V-268142: terminate idle SSH sessions
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268159
      services.openssh.enable = true;
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268089
      services.openssh.settings.Ciphers = [
        "aes256-ctr"
        "aes192-ctr"
        "aes128-ctr"
      ];
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268157
      services.openssh.settings.Macs = [
        "hmac-sha2-512"
        "hmac-sha2-256"
      ];
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268176
      services.openssh.settings.UsePAM = true;
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268083
      services.openssh.banner = lib.mkDefault ''
        You are accessing a U.S. Government (USG) Information System (IS) that is provided for USG-authorized use only.
        By using this IS (which includes any device attached to this IS), you consent to the following conditions:
        -The USG routinely intercepts and monitors communications on this IS for purposes including, but not limited to, penetration testing, COMSEC monitoring, network operations and defense, personnel misconduct (PM), law enforcement (LE), and counterintelligence (CI) investigations.
        -At any time, the USG may inspect and seize data stored on this IS.
        -Communications using, or data stored on, this IS are not private, are subject to routine monitoring, interception, and search, and may be disclosed or used for any USG-authorized purpose.
        -This IS includes security measures (e.g., authentication and access controls) to protect USG interests--not for your personal benefit or privacy.
        -Notwithstanding the above, using this IS does not constitute consent to PM, LE or CI investigative searching or monitoring of the content of privileged communications, or work product, related to personal representation or services by attorneys, psychotherapists, or clergy, and their assistants. Such communications and work product are private and confidential. See User Agreement for details.
      '';
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268088
      services.openssh.settings.LogLevel = mkForce "VERBOSE";
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268137
      services.openssh.settings.PermitRootLogin = mkForce "no";
      services.openssh.extraConfig = lib.strings.concatLines [
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268142
        ''
          ClientAliveInterval 600
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268143
        ''
          ClientAliveCountMax 1
        ''
      ];
    };
  }
