{
  lib,
  config,
  ...
}:
with lib;
with lib.crystal-forge;
  mkStigModule {
    inherit config;
    name = "audit";
    srgList = [
      # V-268080 - audit daemon enable
      "SRG-OS-000004-GPOS-00004"
      "SRG-OS-000254-GPOS-00095"
      "SRG-OS-000344-GPOS-00135"
      "SRG-OS-000348-GPOS-00136"
      "SRG-OS-000349-GPOS-00137"
      "SRG-OS-000350-GPOS-00138"
      "SRG-OS-000351-GPOS-00139"
      "SRG-OS-000352-GPOS-00140"
      "SRG-OS-000353-GPOS-00141"
      "SRG-OS-000354-GPOS-00142"
      "SRG-OS-000122-GPOS-00063"
      "SRG-OS-000358-GPOS-00145"
      # V-268091 - execve privilege changes
      "SRG-OS-000042-GPOS-00020"
      "SRG-OS-000062-GPOS-00031"
      "SRG-OS-000064-GPOS-00033"
      "SRG-OS-000365-GPOS-00152"
      "SRG-OS-000392-GPOS-00172"
      "SRG-OS-000471-GPOS-00215"
      "SRG-OS-000755-GPOS-00220"
      # V-268148 - prevent software execution at higher privilege
      "SRG-OS-000326-GPOS-00126"
      # V-268096 - module changes
      "SRG-OS-000471-GPOS-00216"
      # V-268098 - file access
      "SRG-OS-000461-GPOS-00205"
      # V-268100 - chmod
      "SRG-OS-000462-GPOS-00206"
      # V-268119 - loginuid immutable
      "SRG-OS-000058-GPOS-00028"
      "SRG-OS-000059-GPOS-00029"
      # V-268163 - setxattr
      "SRG-OS-000463-GPOS-00207"
      "SRG-OS-000458-GPOS-00203"
      "SRG-OS-000474-GPOS-00219"
      # V-268164 - usermod
      "SRG-OS-000466-GPOS-00210"
      # V-268165 - chage/chcon
      "SRG-OS-000468-GPOS-00212"
      # V-268166 - lastlog
      "SRG-OS-000473-GPOS-00218"
      "SRG-OS-000475-GPOS-00220"
      # V-268167 - identity
      "SRG-OS-000476-GPOS-00221"
      "SRG-OS-000274-GPOS-00104"
      "SRG-OS-000275-GPOS-00105"
      "SRG-OS-000276-GPOS-00106"
      "SRG-OS-000277-GPOS-00107"
      "SRG-OS-000477-GPOS-00222"
      "SRG-OS-000304-GPOS-00121"
      # V-268101-V-268104 - disk space notifications
      "SRG-OS-000046-GPOS-00022"
      "SRG-OS-000343-GPOS-00134"
      # V-268105, V-268106 - disk full/error actions
      "SRG-OS-000047-GPOS-00023"
      # V-268110 - log_group
      "SRG-OS-000057-GPOS-00027"
      "SRG-OS-000206-GPOS-00084"
    ];
    cciList = [];
    stigConfig = {
      # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268080
      security.auditd.enable = true;
      security.audit.enable = true;

      security.audit.rules = [
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268091
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268148
        "-a always,exit -F arch=b64 -S execve -C uid!=euid -F euid=0 -k execpriv"
        "-a always,exit -F arch=b32 -S execve -C uid!=euid -F euid=0 -k execpriv"
        "-a always,exit -F arch=b32 -S execve -C gid!=egid -F egid=0 -k execpriv"
        "-a always,exit -F arch=b64 -S execve -C gid!=egid -F egid=0 -k execpriv"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268094
        "-a always,exit -F arch=b32 -S mount -F auid>=1000 -F auid!=unset -k privileged-mount"
        "-a always,exit -F arch=b64 -S mount -F auid>=1000 -F auid!=unset -k privileged-mount"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268095
        "-a always,exit -F arch=b32 -S rename,unlink,rmdir,renameat,unlinkat -F auid>=1000 -F auid!=unset -k delete"
        "-a always,exit -F arch=b64 -S rename,unlink,rmdir,renameat,unlinkat -F auid>=1000 -F auid!=unset -k delete"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268096
        "-a always,exit -F arch=b32 -S init_module,finit_module,delete_module -F auid>=1000 -F auid!=unset -k module_chng"
        "-a always,exit -F arch=b64 -S init_module,finit_module,delete_module -F auid>=1000 -F auid!=unset -k module_chng"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268097
        "-w /var/cron/tabs/ -p wa -k services"
        "-w /var/cron/cron.allow -p wa -k services"
        "-w /var/cron/cron.deny -p wa -k services"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268098
        "-a always,exit -F arch=b32 -S open,creat,truncate,ftruncate,openat,open_by_handle_at -F exit=-EACCES -F auid>=1000 -F auid!=unset -F key=access"
        "-a always,exit -F arch=b32 -S open,creat,truncate,ftruncate,openat,open_by_handle_at -F exit=-EPERM -F auid>=1000 -F auid!=unset -F key=access"
        "-a always,exit -F arch=b64 -S open,creat,truncate,ftruncate,openat,open_by_handle_at -F exit=-EACCES -F auid>=1000 -F auid!=unset -F key=access"
        "-a always,exit -F arch=b64 -S open,creat,truncate,ftruncate,openat,open_by_handle_at -F exit=-EPERM -F auid>=1000 -F auid!=unset -F key=access"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268099
        "-a always,exit -F arch=b32 -S lchown,fchown,chown,fchownat -F auid>=1000 -F auid!=unset -F key=perm_mod"
        "-a always,exit -F arch=b64 -S chown,fchown,lchown,fchownat -F auid>=1000 -F auid!=unset -F key=perm_mod"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268100
        "-a always,exit -F arch=b32 -S chmod,fchmod,fchmodat -F auid>=1000 -F auid!=unset -k perm_mod"
        "-a always,exit -F arch=b64 -S chmod,fchmod,fchmodat -F auid>=1000 -F auid!=unset -k perm_mod"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268119
        "--loginuid-immutable"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268163
        "-a always,exit -F arch=b32 -S setxattr,fsetxattr,lsetxattr,removexattr,fremovexattr,lremovexattr -F auid>=1000 -F auid!=-1 -k perm_mod"
        "-a always,exit -F arch=b32 -S setxattr,fsetxattr,lsetxattr,removexattr,fremovexattr,lremovexattr -F auid=0 -k perm_mod"
        "-a always,exit -F arch=b64 -S setxattr,fsetxattr,lsetxattr,removexattr,fremovexattr,lremovexattr -F auid>=1000 -F auid!=-1 -k perm_mod"
        "-a always,exit -F arch=b64 -S setxattr,fsetxattr,lsetxattr,removexattr,fremovexattr,lremovexattr -F auid=0 -k perm_mod"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268164
        "-a always,exit -F path=/run/current-system/sw/bin/usermod -F perm=x -F auid>=1000 -F auid!=unset -k privileged-usermod"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268165
        "-a always,exit -F path=/run/current-system/sw/bin/chage -F perm=x -F auid>=1000 -F auid!=unset -k privileged-chage"
        "-a always,exit -F path=/run/current-system/sw/bin/chcon -F perm=x -F auid>=1000 -F auid!=unset -k perm_mod"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268166
        "-w /var/log/lastlog -p wa -k logins"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268167
        "-w /etc/sudoers -p wa -k identity"
        "-w /etc/passwd -p wa -k identity"
        "-w /etc/shadow -p wa -k identity"
        "-w /etc/gshadow -p wa -k identity"
        "-w /etc/group -p wa -k identity"
        "-w /etc/security/opasswd -p wa -k identity"
      ];

      environment.etc."audit/auditd.conf".text = lib.strings.concatLines [
        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268101
        "space_left_action = syslog"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268102
        "admin_space_left_action = syslog"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268103
        "space_left = 25%"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268104
        "admin_space_left = 10%"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268105
        "disk_full_action = HALT"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268106
        "disk_error_action = HALT"

        # https://stigui.com/stigs/Anduril_NixOS_STIG/groups/V-268110
        "log_group = root"
      ];
    };
  }
