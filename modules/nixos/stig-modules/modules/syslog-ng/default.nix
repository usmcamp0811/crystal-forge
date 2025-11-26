{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge; let
  cfg = config.services.syslog-ng;
in
  mkStigModule {
    inherit config;
    name = "syslog-ng";
    srgList = [
      "SRG-OS-000051-GPOS-00024" # V-268107: packages for offloading audit logs
      "SRG-OS-000269-GPOS-00103" # V-268107: offload audit logs
      "SRG-OS-000342-GPOS-00133" # V-268109: authenticate remote logging server
      "SRG-OS-000479-GPOS-00224" # V-268109: TLS encryption for audit logs
      "SRG-OS-000057-GPOS-00027" # V-268115: syslog owned by root
      "SRG-OS-000206-GPOS-00084" # V-268115: protect syslog from unauthorized access
      "SRG-OS-000205-GPOS-00083" # V-268118: syslog file permissions
    ];
    cciList = [];
    extraOptions = {
      services.syslog-ng = {
        remote_hosts = mkOption {
          type = types.listOf types.str;
          description = "Remote syslog-ng hosts for log forwarding";
        };
        remote_tls = mkOption {
          type = types.bool;
          default = true;
          description = "Use TLS for remote log forwarding";
        };
        certfile = mkOption {
          type = types.str;
          default = "/var/syslog-ng/certs.d/certificate.crt";
          description = "Path to client certificate for TLS authentication";
        };
        keyfile = mkOption {
          type = types.str;
          default = "/var/syslog-ng/certs.d/certificate.key";
          description = "Path to client certificate key for TLS authentication";
        };
        cafile = mkOption {
          type = types.str;
          default = "/var/syslog-ng/certs.d/cert-bundle.crt";
          description = "Path to CA certificate bundle for TLS verification";
        };
      };
    };
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268107
      services.syslog-ng.enable = true;
      services.syslog-ng.extraConfig = strings.concatLines [
        ''
          source s_local { system(); internal(); };
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268108
        (strings.optionalString (!cfg.remote_tls) ''
          destination d_network {
            ${strings.concatMapStrings (host: ''
              syslog(
                ${host}
              );
            '')
            cfg.remote_hosts}
          };
          log {
            source(s_local);
            destination(d_network);
          };
        '')
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268109
        (strings.optionalString (cfg.remote_tls) ''
          destination d_network {
            ${strings.concatMapStrings (host: ''
              syslog(
                ${host}
                transport(tls)
                tls(
                  cert-file("${cfg.certfile}")
                  key-file("${cfg.keyfile}")
                  ca-file("${cfg.cafile}")
                  peer-verify(yes)
                )
              );
            '')
            cfg.remote_hosts}
          };
          log { source(s_local); destination(d_local); destination(d_network); };
        '')
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268115
        ''
          options {
            owner(root);
            dir_owner(root);
          };
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268116
        ''
          options {
            group(root);
            dir_group(root);
          };
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268117
        ''
          options {
            dir_perm(0750);
          };
        ''
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268118
        ''
          options {
            perm(0640);
          };
        ''
      ];
    };
  }
