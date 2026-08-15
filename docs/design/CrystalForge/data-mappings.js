// Compliance mappings — the reusable-policy abstraction.
// Policy <-> Requirement is an explicit, inspectable relationship. Frameworks own
// requirements; requirements have hierarchy; policies map to zero, one, or many
// requirements across zero, one, or many frameworks. A policy is never "a NIST policy".

const COMPLIANCE_FRAMEWORKS = [
  { id:"nist-800-53", name:"NIST 800-53", version:"Rev 5", hierarchyLabels:["Family","Control","Enhancement"] },
  { id:"disa-stig",   name:"DISA STIG",   version:"Anduril NixOS v1r2", hierarchyLabels:["Group","Rule"] },
  { id:"cis",         name:"CIS Benchmark", version:"NixOS Benchmark 1.0", hierarchyLabels:["Section","Subsection","Recommendation"] },
  { id:"cmmc",        name:"CMMC 2.0",    version:"Level-based practices", hierarchyLabels:["Domain","Practice"] },
];
function frameworkById(id) { return COMPLIANCE_FRAMEWORKS.find(f => f.id === id); }

// Generic requirement node: { id, frameworkId, externalId, title, kind, parentId }
// kind is framework-defined (family/control/enhancement, group/rule, section/subsection/recommendation, domain/practice).
const COMPLIANCE_REQUIREMENTS = [
  // NIST 800-53 — families, then controls/enhancements under them
  { id:"nist-AC", frameworkId:"nist-800-53", externalId:"AC", title:"Access Control", kind:"family", parentId:null },
  { id:"nist-AC-8", frameworkId:"nist-800-53", externalId:"AC-8", title:"System Use Notification", kind:"control", parentId:"nist-AC" },
  { id:"nist-AC-17", frameworkId:"nist-800-53", externalId:"AC-17", title:"Remote Access", kind:"control", parentId:"nist-AC" },
  { id:"nist-AC-17-1", frameworkId:"nist-800-53", externalId:"AC-17(1)", title:"Monitoring and Control", kind:"enhancement", parentId:"nist-AC-17" },
  { id:"nist-AU", frameworkId:"nist-800-53", externalId:"AU", title:"Audit & Accountability", kind:"family", parentId:null },
  { id:"nist-AU-12", frameworkId:"nist-800-53", externalId:"AU-12", title:"Audit Record Generation", kind:"control", parentId:"nist-AU" },
  { id:"nist-SC", frameworkId:"nist-800-53", externalId:"SC", title:"System & Communications Protection", kind:"family", parentId:null },
  { id:"nist-SC-7", frameworkId:"nist-800-53", externalId:"SC-7", title:"Boundary Protection", kind:"control", parentId:"nist-SC" },
  { id:"nist-SC-8", frameworkId:"nist-800-53", externalId:"SC-8", title:"Transmission Confidentiality and Integrity", kind:"control", parentId:"nist-SC" },
  { id:"nist-SC-13", frameworkId:"nist-800-53", externalId:"SC-13", title:"Cryptographic Protection", kind:"control", parentId:"nist-SC" },
  { id:"nist-SC-28", frameworkId:"nist-800-53", externalId:"SC-28", title:"Protection of Information at Rest", kind:"control", parentId:"nist-SC" },
  { id:"nist-CM", frameworkId:"nist-800-53", externalId:"CM", title:"Configuration Management", kind:"family", parentId:null },
  { id:"nist-CM-6", frameworkId:"nist-800-53", externalId:"CM-6", title:"Configuration Settings", kind:"control", parentId:"nist-CM" },
  { id:"nist-IA", frameworkId:"nist-800-53", externalId:"IA", title:"Identification & Authentication", kind:"family", parentId:null },
  { id:"nist-IA-5", frameworkId:"nist-800-53", externalId:"IA-5", title:"Authenticator Management", kind:"control", parentId:"nist-IA" },
  { id:"nist-MP", frameworkId:"nist-800-53", externalId:"MP", title:"Media Protection", kind:"family", parentId:null },
  { id:"nist-MP-7", frameworkId:"nist-800-53", externalId:"MP-7", title:"Media Use", kind:"control", parentId:"nist-MP" },

  // DISA STIG — rules (flat under an implicit benchmark; group omitted where source didn't carry one)
  { id:"stig-V-268137", frameworkId:"disa-stig", externalId:"V-268137", title:"The operating system must not permit direct root logon via SSH.", kind:"rule", parentId:null, cci:"CCI-000770" },
  { id:"stig-V-268142", frameworkId:"disa-stig", externalId:"V-268142", title:"The operating system must terminate idle SSH sessions after 10 minutes.", kind:"rule", parentId:null, cci:"CCI-001133" },
  { id:"stig-V-268089", frameworkId:"disa-stig", externalId:"V-268089", title:"The operating system must use FIPS-validated ciphers for remote access.", kind:"rule", parentId:null, cci:"CCI-000068" },
  { id:"stig-V-268080", frameworkId:"disa-stig", externalId:"V-268080", title:"The operating system must enable the audit daemon.", kind:"rule", parentId:null, cci:"CCI-000018" },
  { id:"stig-V-268078", frameworkId:"disa-stig", externalId:"V-268078", title:"The operating system must enable the built-in firewall.", kind:"rule", parentId:null, cci:"CCI-000366" },
  { id:"stig-V-268082", frameworkId:"disa-stig", externalId:"V-268082", title:"The operating system must display the DoD/USG consent banner.", kind:"rule", parentId:null, cci:"CCI-000048" },
  { id:"stig-V-268168", frameworkId:"disa-stig", externalId:"V-268168", title:"The operating system must use FIPS-validated cryptography.", kind:"rule", parentId:null, cci:"CCI-002450" },
  { id:"stig-V-268144", frameworkId:"disa-stig", externalId:"V-268144", title:"The operating system must protect data at rest with encryption.", kind:"rule", parentId:null, cci:"CCI-001199" },
  { id:"stig-V-268139", frameworkId:"disa-stig", externalId:"V-268139", title:"The operating system must control peripheral access with USBGuard.", kind:"rule", parentId:null, cci:"CCI-001958" },
  { id:"stig-V-268134", frameworkId:"disa-stig", externalId:"V-268134", title:"The operating system must enforce a 15-character minimum password length.", kind:"rule", parentId:null, cci:"CCI-000205" },
  { id:"stig-V-268130", frameworkId:"disa-stig", externalId:"V-268130", title:"The operating system must store only encrypted passwords.", kind:"rule", parentId:null, cci:"CCI-000196" },
  { id:"stig-V-268200", frameworkId:"disa-stig", externalId:"V-268200", title:"The operating system must ensure enforce SSH daemon baseline configuration.", kind:"rule", parentId:null, cci:"CCI-005673" },
  { id:"stig-V-268201", frameworkId:"disa-stig", externalId:"V-268201", title:"The operating system must ensure enforce firewall rules baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008757" },
  { id:"stig-V-268202", frameworkId:"disa-stig", externalId:"V-268202", title:"The operating system must ensure enforce audit logging baseline configuration.", kind:"rule", parentId:null, cci:"CCI-009798" },
  { id:"stig-V-268203", frameworkId:"disa-stig", externalId:"V-268203", title:"The operating system must ensure enforce account lockout baseline configuration.", kind:"rule", parentId:null, cci:"CCI-006779" },
  { id:"stig-V-268204", frameworkId:"disa-stig", externalId:"V-268204", title:"The operating system must ensure enforce password complexity baseline configuration.", kind:"rule", parentId:null, cci:"CCI-002454" },
  { id:"stig-V-268205", frameworkId:"disa-stig", externalId:"V-268205", title:"The operating system must ensure enforce kernel hardening baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008838" },
  { id:"stig-V-268206", frameworkId:"disa-stig", externalId:"V-268206", title:"The operating system must ensure enforce filesystem permissions baseline configuration.", kind:"rule", parentId:null, cci:"CCI-007329" },
  { id:"stig-V-268207", frameworkId:"disa-stig", externalId:"V-268207", title:"The operating system must ensure enforce USB device control baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008186" },
  { id:"stig-V-268208", frameworkId:"disa-stig", externalId:"V-268208", title:"The operating system must ensure enforce TLS cipher suite baseline configuration.", kind:"rule", parentId:null, cci:"CCI-001993" },
  { id:"stig-V-268209", frameworkId:"disa-stig", externalId:"V-268209", title:"The operating system must ensure enforce DNS resolver baseline configuration.", kind:"rule", parentId:null, cci:"CCI-001529" },
  { id:"stig-V-268210", frameworkId:"disa-stig", externalId:"V-268210", title:"The operating system must ensure enforce NTP sync baseline configuration.", kind:"rule", parentId:null, cci:"CCI-006851" },
  { id:"stig-V-268211", frameworkId:"disa-stig", externalId:"V-268211", title:"The operating system must ensure enforce syslog forwarding baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008622" },
  { id:"stig-V-268212", frameworkId:"disa-stig", externalId:"V-268212", title:"The operating system must ensure enforce sudo policy baseline configuration.", kind:"rule", parentId:null, cci:"CCI-005328" },
  { id:"stig-V-268213", frameworkId:"disa-stig", externalId:"V-268213", title:"The operating system must ensure enforce PAM stack baseline configuration.", kind:"rule", parentId:null, cci:"CCI-004434" },
  { id:"stig-V-268214", frameworkId:"disa-stig", externalId:"V-268214", title:"The operating system must ensure enforce SELinux/AppArmor profile baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008133" },
  { id:"stig-V-268215", frameworkId:"disa-stig", externalId:"V-268215", title:"The operating system must ensure enforce boot loader integrity baseline configuration.", kind:"rule", parentId:null, cci:"CCI-003624" },
  { id:"stig-V-268216", frameworkId:"disa-stig", externalId:"V-268216", title:"The operating system must ensure enforce disk encryption baseline configuration.", kind:"rule", parentId:null, cci:"CCI-005270" },
  { id:"stig-V-268217", frameworkId:"disa-stig", externalId:"V-268217", title:"The operating system must ensure enforce service isolation baseline configuration.", kind:"rule", parentId:null, cci:"CCI-003861" },
  { id:"stig-V-268218", frameworkId:"disa-stig", externalId:"V-268218", title:"The operating system must ensure enforce network segmentation baseline configuration.", kind:"rule", parentId:null, cci:"CCI-006172" },
  { id:"stig-V-268219", frameworkId:"disa-stig", externalId:"V-268219", title:"The operating system must ensure enforce container runtime baseline configuration.", kind:"rule", parentId:null, cci:"CCI-005250" },
  { id:"stig-V-268220", frameworkId:"disa-stig", externalId:"V-268220", title:"The operating system must ensure enforce package signing baseline configuration.", kind:"rule", parentId:null, cci:"CCI-006108" },
  { id:"stig-V-268221", frameworkId:"disa-stig", externalId:"V-268221", title:"The operating system must ensure enforce update cadence baseline configuration.", kind:"rule", parentId:null, cci:"CCI-005877" },
  { id:"stig-V-268222", frameworkId:"disa-stig", externalId:"V-268222", title:"The operating system must ensure enforce session timeout baseline configuration.", kind:"rule", parentId:null, cci:"CCI-002744" },
  { id:"stig-V-268223", frameworkId:"disa-stig", externalId:"V-268223", title:"The operating system must ensure enforce banner text baseline configuration.", kind:"rule", parentId:null, cci:"CCI-006238" },
  { id:"stig-V-268224", frameworkId:"disa-stig", externalId:"V-268224", title:"The operating system must ensure enforce log rotation baseline configuration.", kind:"rule", parentId:null, cci:"CCI-004507" },
  { id:"stig-V-268225", frameworkId:"disa-stig", externalId:"V-268225", title:"The operating system must ensure enforce core dump handling baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008070" },
  { id:"stig-V-268226", frameworkId:"disa-stig", externalId:"V-268226", title:"The operating system must ensure enforce IPv6 stack baseline configuration.", kind:"rule", parentId:null, cci:"CCI-006139" },
  { id:"stig-V-268227", frameworkId:"disa-stig", externalId:"V-268227", title:"The operating system must ensure enforce USB storage baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008850" },
  { id:"stig-V-268228", frameworkId:"disa-stig", externalId:"V-268228", title:"The operating system must ensure enforce Bluetooth radio baseline configuration.", kind:"rule", parentId:null, cci:"CCI-003583" },
  { id:"stig-V-268229", frameworkId:"disa-stig", externalId:"V-268229", title:"The operating system must ensure enforce wireless interface baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008978" },
  { id:"stig-V-268230", frameworkId:"disa-stig", externalId:"V-268230", title:"The operating system must ensure enforce SNMP daemon baseline configuration.", kind:"rule", parentId:null, cci:"CCI-001086" },
  { id:"stig-V-268231", frameworkId:"disa-stig", externalId:"V-268231", title:"The operating system must ensure enforce NFS export baseline configuration.", kind:"rule", parentId:null, cci:"CCI-001405" },
  { id:"stig-V-268232", frameworkId:"disa-stig", externalId:"V-268232", title:"The operating system must ensure enforce Samba share baseline configuration.", kind:"rule", parentId:null, cci:"CCI-006172" },
  { id:"stig-V-268233", frameworkId:"disa-stig", externalId:"V-268233", title:"The operating system must ensure enforce cron daemon baseline configuration.", kind:"rule", parentId:null, cci:"CCI-001226" },
  { id:"stig-V-268234", frameworkId:"disa-stig", externalId:"V-268234", title:"The operating system must ensure enforce mail relay baseline configuration.", kind:"rule", parentId:null, cci:"CCI-005862" },
  { id:"stig-V-268235", frameworkId:"disa-stig", externalId:"V-268235", title:"The operating system must ensure enforce X11 forwarding baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008501" },
  { id:"stig-V-268236", frameworkId:"disa-stig", externalId:"V-268236", title:"The operating system must ensure enforce VNC service baseline configuration.", kind:"rule", parentId:null, cci:"CCI-005288" },
  { id:"stig-V-268237", frameworkId:"disa-stig", externalId:"V-268237", title:"The operating system must ensure enforce container image scanning baseline configuration.", kind:"rule", parentId:null, cci:"CCI-002331" },
  { id:"stig-V-268238", frameworkId:"disa-stig", externalId:"V-268238", title:"The operating system must ensure enforce secrets storage baseline configuration.", kind:"rule", parentId:null, cci:"CCI-009594" },
  { id:"stig-V-268239", frameworkId:"disa-stig", externalId:"V-268239", title:"The operating system must ensure enforce key rotation baseline configuration.", kind:"rule", parentId:null, cci:"CCI-007923" },
  { id:"stig-V-268240", frameworkId:"disa-stig", externalId:"V-268240", title:"The operating system must ensure enforce certificate validation baseline configuration.", kind:"rule", parentId:null, cci:"CCI-007760" },
  { id:"stig-V-268241", frameworkId:"disa-stig", externalId:"V-268241", title:"The operating system must ensure enforce kernel module loading baseline configuration.", kind:"rule", parentId:null, cci:"CCI-006211" },
  { id:"stig-V-268242", frameworkId:"disa-stig", externalId:"V-268242", title:"The operating system must ensure enforce ASLR enforcement baseline configuration.", kind:"rule", parentId:null, cci:"CCI-005743" },
  { id:"stig-V-268243", frameworkId:"disa-stig", externalId:"V-268243", title:"The operating system must ensure enforce stack protector baseline configuration.", kind:"rule", parentId:null, cci:"CCI-002240" },
  { id:"stig-V-268244", frameworkId:"disa-stig", externalId:"V-268244", title:"The operating system must ensure enforce ptrace scope baseline configuration.", kind:"rule", parentId:null, cci:"CCI-001069" },
  { id:"stig-V-268245", frameworkId:"disa-stig", externalId:"V-268245", title:"The operating system must ensure enforce coredump storage baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008374" },
  { id:"stig-V-268246", frameworkId:"disa-stig", externalId:"V-268246", title:"The operating system must ensure enforce swap encryption baseline configuration.", kind:"rule", parentId:null, cci:"CCI-004777" },
  { id:"stig-V-268247", frameworkId:"disa-stig", externalId:"V-268247", title:"The operating system must ensure enforce tmp mount options baseline configuration.", kind:"rule", parentId:null, cci:"CCI-002082" },
  { id:"stig-V-268248", frameworkId:"disa-stig", externalId:"V-268248", title:"The operating system must ensure enforce home directory perms baseline configuration.", kind:"rule", parentId:null, cci:"CCI-007354" },
  { id:"stig-V-268249", frameworkId:"disa-stig", externalId:"V-268249", title:"The operating system must ensure enforce shell history baseline configuration.", kind:"rule", parentId:null, cci:"CCI-005791" },
  { id:"stig-V-268250", frameworkId:"disa-stig", externalId:"V-268250", title:"The operating system must ensure enforce login banner baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008036" },
  { id:"stig-V-268251", frameworkId:"disa-stig", externalId:"V-268251", title:"The operating system must ensure enforce MOTD content baseline configuration.", kind:"rule", parentId:null, cci:"CCI-002120" },
  { id:"stig-V-268252", frameworkId:"disa-stig", externalId:"V-268252", title:"The operating system must ensure enforce idle session lock baseline configuration.", kind:"rule", parentId:null, cci:"CCI-008635" },
  { id:"stig-V-268253", frameworkId:"disa-stig", externalId:"V-268253", title:"The operating system must ensure enforce screen lock baseline configuration.", kind:"rule", parentId:null, cci:"CCI-007461" },
  { id:"stig-V-268254", frameworkId:"disa-stig", externalId:"V-268254", title:"The operating system must ensure sSH daemon configuration hardened per DISA baseline (key exchange, MACs, ciphers restricted to FIPS-approved sets).", kind:"rule", parentId:null, cci:"CCI-900054" },
  { id:"stig-V-268255", frameworkId:"disa-stig", externalId:"V-268255", title:"The operating system must ensure host firewall enforces default-deny inbound with explicit allow rules for required services.", kind:"rule", parentId:null, cci:"CCI-900055" },
  { id:"stig-V-268256", frameworkId:"disa-stig", externalId:"V-268256", title:"The operating system must ensure audit subsystem captures privileged command execution, auth events, and file-access denials.", kind:"rule", parentId:null, cci:"CCI-900056" },
  { id:"stig-V-268257", frameworkId:"disa-stig", externalId:"V-268257", title:"The operating system must ensure account lockout enforced after repeated failed authentication attempts.", kind:"rule", parentId:null, cci:"CCI-900057" },
  { id:"stig-V-268258", frameworkId:"disa-stig", externalId:"V-268258", title:"The operating system must ensure password complexity policy requires mixed case, digits, and special characters.", kind:"rule", parentId:null, cci:"CCI-900058" },
  { id:"stig-V-268259", frameworkId:"disa-stig", externalId:"V-268259", title:"The operating system must ensure kernel sysctl parameters hardened against common memory-corruption and network attack classes.", kind:"rule", parentId:null, cci:"CCI-900059" },
  { id:"stig-V-268260", frameworkId:"disa-stig", externalId:"V-268260", title:"The operating system must ensure world-writable and unowned files are disallowed; sensitive paths use restrictive permissions.", kind:"rule", parentId:null, cci:"CCI-900060" },
  { id:"stig-V-268261", frameworkId:"disa-stig", externalId:"V-268261", title:"The operating system must ensure uSB mass-storage and peripheral devices are blocked unless explicitly allow-listed.", kind:"rule", parentId:null, cci:"CCI-900061" },
  { id:"stig-V-268262", frameworkId:"disa-stig", externalId:"V-268262", title:"The operating system must ensure tLS services restricted to FIPS-validated cipher suites and TLS 1.2+.", kind:"rule", parentId:null, cci:"CCI-900062" },
  { id:"stig-V-268263", frameworkId:"disa-stig", externalId:"V-268263", title:"The operating system must ensure dNS resolution is pinned to approved resolvers with DNSSEC validation enabled.", kind:"rule", parentId:null, cci:"CCI-900063" },
  { id:"stig-V-268264", frameworkId:"disa-stig", externalId:"V-268264", title:"The operating system must ensure system clock is synchronized to an approved time source for reliable audit timestamps.", kind:"rule", parentId:null, cci:"CCI-900064" },
  { id:"stig-V-268265", frameworkId:"disa-stig", externalId:"V-268265", title:"The operating system must ensure audit and system logs are forwarded to a central log collector over an encrypted channel.", kind:"rule", parentId:null, cci:"CCI-900065" },
  { id:"stig-V-268266", frameworkId:"disa-stig", externalId:"V-268266", title:"The operating system must ensure sudo access is restricted to named users/groups with command logging enabled.", kind:"rule", parentId:null, cci:"CCI-900066" },
  { id:"stig-V-268267", frameworkId:"disa-stig", externalId:"V-268267", title:"The operating system must ensure pAM stack enforces password quality, lockout, and session controls consistently across services.", kind:"rule", parentId:null, cci:"CCI-900067" },
  { id:"stig-V-268268", frameworkId:"disa-stig", externalId:"V-268268", title:"The operating system must ensure mandatory access control profile is enforcing (not permissive) for system services.", kind:"rule", parentId:null, cci:"CCI-900068" },
  { id:"stig-V-268269", frameworkId:"disa-stig", externalId:"V-268269", title:"The operating system must ensure boot loader requires a password for interactive edits and verifies kernel signatures.", kind:"rule", parentId:null, cci:"CCI-900069" },
  { id:"stig-V-268270", frameworkId:"disa-stig", externalId:"V-268270", title:"The operating system must ensure data-at-rest is protected with full-disk encryption using an approved cipher.", kind:"rule", parentId:null, cci:"CCI-900070" },
  { id:"stig-V-268271", frameworkId:"disa-stig", externalId:"V-268271", title:"The operating system must ensure system services run under dedicated unprivileged accounts with restricted capabilities.", kind:"rule", parentId:null, cci:"CCI-900071" },
  { id:"stig-V-268272", frameworkId:"disa-stig", externalId:"V-268272", title:"The operating system must ensure host network interfaces are segmented per zone; inter-zone routing is explicitly denied by default.", kind:"rule", parentId:null, cci:"CCI-900072" },
  { id:"stig-V-268273", frameworkId:"disa-stig", externalId:"V-268273", title:"The operating system must ensure container runtime is configured to drop unnecessary capabilities and run rootless where supported.", kind:"rule", parentId:null, cci:"CCI-900073" },
  { id:"stig-V-268274", frameworkId:"disa-stig", externalId:"V-268274", title:"The operating system must ensure package manager only installs packages signed by a trusted, pinned key set.", kind:"rule", parentId:null, cci:"CCI-900074" },
  { id:"stig-V-268275", frameworkId:"disa-stig", externalId:"V-268275", title:"The operating system must ensure security updates are applied within the required patch window and tracked.", kind:"rule", parentId:null, cci:"CCI-900075" },
  { id:"stig-V-268276", frameworkId:"disa-stig", externalId:"V-268276", title:"The operating system must ensure interactive sessions are terminated automatically after a defined idle period.", kind:"rule", parentId:null, cci:"CCI-900076" },
  { id:"stig-V-268277", frameworkId:"disa-stig", externalId:"V-268277", title:"The operating system must ensure login banners display the required consent-to-monitoring notice before authentication.", kind:"rule", parentId:null, cci:"CCI-900077" },
  { id:"stig-V-268278", frameworkId:"disa-stig", externalId:"V-268278", title:"The operating system must ensure audit logs are rotated and retained to prevent loss of accountability data from disk exhaustion.", kind:"rule", parentId:null, cci:"CCI-900078" },
  { id:"stig-V-268279", frameworkId:"disa-stig", externalId:"V-268279", title:"The operating system must ensure core dumps are disabled or restricted to prevent leakage of sensitive process memory.", kind:"rule", parentId:null, cci:"CCI-900079" },
  { id:"stig-V-268280", frameworkId:"disa-stig", externalId:"V-268280", title:"The operating system must ensure unused IPv6 stack is disabled where not required, reducing network attack surface.", kind:"rule", parentId:null, cci:"CCI-900080" },
  { id:"stig-V-268281", frameworkId:"disa-stig", externalId:"V-268281", title:"The operating system must ensure uSB mass storage class drivers are blocked at the kernel level.", kind:"rule", parentId:null, cci:"CCI-900081" },
  { id:"stig-V-268282", frameworkId:"disa-stig", externalId:"V-268282", title:"The operating system must ensure bluetooth radio is disabled on systems with no approved use case.", kind:"rule", parentId:null, cci:"CCI-900082" },
  { id:"stig-V-268283", frameworkId:"disa-stig", externalId:"V-268283", title:"The operating system must ensure wireless network interfaces are disabled unless explicitly required and approved.", kind:"rule", parentId:null, cci:"CCI-900083" },
  { id:"stig-V-268284", frameworkId:"disa-stig", externalId:"V-268284", title:"The operating system must ensure sNMP service is disabled, or restricted to v3 with authentication and encryption.", kind:"rule", parentId:null, cci:"CCI-900084" },
  { id:"stig-V-268285", frameworkId:"disa-stig", externalId:"V-268285", title:"The operating system must ensure nFS exports restrict access to approved subnets and disallow root squash bypass.", kind:"rule", parentId:null, cci:"CCI-900085" },
  { id:"stig-V-268286", frameworkId:"disa-stig", externalId:"V-268286", title:"The operating system must ensure sMB/Samba shares require authentication and disallow guest access.", kind:"rule", parentId:null, cci:"CCI-900086" },
  { id:"stig-V-268287", frameworkId:"disa-stig", externalId:"V-268287", title:"The operating system must ensure cron job definitions are restricted to authorized users and reviewed for integrity.", kind:"rule", parentId:null, cci:"CCI-900087" },
  { id:"stig-V-268288", frameworkId:"disa-stig", externalId:"V-268288", title:"The operating system must ensure local mail transfer agent does not relay mail for untrusted networks.", kind:"rule", parentId:null, cci:"CCI-900088" },
  { id:"stig-V-268289", frameworkId:"disa-stig", externalId:"V-268289", title:"The operating system must ensure x11 forwarding over SSH is disabled unless explicitly required.", kind:"rule", parentId:null, cci:"CCI-900089" },
  { id:"stig-V-268290", frameworkId:"disa-stig", externalId:"V-268290", title:"The operating system must ensure vNC remote-desktop service is disabled or tunneled over an authenticated, encrypted channel.", kind:"rule", parentId:null, cci:"CCI-900090" },
  { id:"stig-V-268291", frameworkId:"disa-stig", externalId:"V-268291", title:"The operating system must ensure container images are scanned for known vulnerabilities before deployment.", kind:"rule", parentId:null, cci:"CCI-900091" },
  { id:"stig-V-268292", frameworkId:"disa-stig", externalId:"V-268292", title:"The operating system must ensure application secrets are stored in an encrypted secrets manager, not plaintext config.", kind:"rule", parentId:null, cci:"CCI-900092" },
  { id:"stig-V-268293", frameworkId:"disa-stig", externalId:"V-268293", title:"The operating system must ensure cryptographic keys are rotated on a defined schedule and revoked keys are removed from trust stores.", kind:"rule", parentId:null, cci:"CCI-900093" },
  { id:"stig-V-268294", frameworkId:"disa-stig", externalId:"V-268294", title:"The operating system must ensure tLS clients validate certificate chains and reject expired or self-signed certificates in production.", kind:"rule", parentId:null, cci:"CCI-900094" },
  { id:"stig-V-268295", frameworkId:"disa-stig", externalId:"V-268295", title:"The operating system must ensure loading of unused or unsigned kernel modules is disabled.", kind:"rule", parentId:null, cci:"CCI-900095" },
  { id:"stig-V-268296", frameworkId:"disa-stig", externalId:"V-268296", title:"The operating system must ensure address space layout randomization is enabled fleet-wide to mitigate memory-corruption exploits.", kind:"rule", parentId:null, cci:"CCI-900096" },
  { id:"stig-V-268297", frameworkId:"disa-stig", externalId:"V-268297", title:"The operating system must ensure binaries are compiled with stack-protector and related exploit-mitigation flags.", kind:"rule", parentId:null, cci:"CCI-900097" },
  { id:"stig-V-268298", frameworkId:"disa-stig", externalId:"V-268298", title:"The operating system must ensure kernel ptrace scope is restricted to prevent unprivileged process inspection.", kind:"rule", parentId:null, cci:"CCI-900098" },
  { id:"stig-V-268299", frameworkId:"disa-stig", externalId:"V-268299", title:"The operating system must ensure core dump storage location is restricted and cleared on a schedule.", kind:"rule", parentId:null, cci:"CCI-900099" },
  { id:"stig-V-268300", frameworkId:"disa-stig", externalId:"V-268300", title:"The operating system must ensure swap space is encrypted to prevent recovery of sensitive data written to disk.", kind:"rule", parentId:null, cci:"CCI-900100" },
  { id:"stig-V-268301", frameworkId:"disa-stig", externalId:"V-268301", title:"The operating system must ensure temporary filesystems are mounted with noexec, nosuid, and nodev options.", kind:"rule", parentId:null, cci:"CCI-900101" },
  { id:"stig-V-268302", frameworkId:"disa-stig", externalId:"V-268302", title:"The operating system must ensure user home directories default to owner-only permissions.", kind:"rule", parentId:null, cci:"CCI-900102" },
  { id:"stig-V-268303", frameworkId:"disa-stig", externalId:"V-268303", title:"The operating system must ensure block deploys that introduce any critical CVE.", kind:"rule", parentId:null, cci:"CCI-000366" },

  // CIS Benchmark — section -> subsection -> recommendation
  { id:"cis-4", frameworkId:"cis", externalId:"4", title:"Logging and Auditing", kind:"section", parentId:null },
  { id:"cis-4.1", frameworkId:"cis", externalId:"4.1", title:"Configure System Accounting (auditd)", kind:"subsection", parentId:"cis-4" },
  { id:"cis-4.1.1", frameworkId:"cis", externalId:"4.1.1", title:"Ensure auditd is installed and enabled", kind:"recommendation", parentId:"cis-4.1" },
  { id:"cis-5", frameworkId:"cis", externalId:"5", title:"Access, Authentication and Authorization", kind:"section", parentId:null },
  { id:"cis-5.1", frameworkId:"cis", externalId:"5.1", title:"Configure SSH Server", kind:"subsection", parentId:"cis-5" },
  { id:"cis-5.1.8", frameworkId:"cis", externalId:"5.1.8", title:"Ensure SSH root login is disabled", kind:"recommendation", parentId:"cis-5.1" },
  { id:"cis-5.1.10", frameworkId:"cis", externalId:"5.1.10", title:"Ensure SSH warning banner is configured", kind:"recommendation", parentId:"cis-5.1" },
  { id:"cis-5.4", frameworkId:"cis", externalId:"5.4", title:"User Accounts and Environment", kind:"subsection", parentId:"cis-5" },
  { id:"cis-5.4.1", frameworkId:"cis", externalId:"5.4.1", title:"Ensure password quality is configured", kind:"recommendation", parentId:"cis-5.4" },

  // CMMC 2.0 — domain -> practice
  { id:"cmmc-AC", frameworkId:"cmmc", externalId:"AC", title:"Access Control", kind:"domain", parentId:null },
  { id:"cmmc-AC-3.1.12", frameworkId:"cmmc", externalId:"AC.L2-3.1.12", title:"Remote access monitoring and control", kind:"practice", parentId:"cmmc-AC" },
  { id:"cmmc-AU", frameworkId:"cmmc", externalId:"AU", title:"Audit & Accountability", kind:"domain", parentId:null },
  { id:"cmmc-AU-3.3.1", frameworkId:"cmmc", externalId:"AU.L2-3.3.1", title:"System audit records", kind:"practice", parentId:"cmmc-AU" },
  { id:"cmmc-IA", frameworkId:"cmmc", externalId:"IA", title:"Identification & Authentication", kind:"domain", parentId:null },
  { id:"cmmc-IA-3.5.7", frameworkId:"cmmc", externalId:"IA.L2-3.5.7", title:"Enforce minimum password complexity", kind:"practice", parentId:"cmmc-IA" },
  { id:"cmmc-MP", frameworkId:"cmmc", externalId:"MP", title:"Media Protection", kind:"domain", parentId:null },
  { id:"cmmc-MP-3.8.7", frameworkId:"cmmc", externalId:"MP.L2-3.8.7", title:"Control use of removable media", kind:"practice", parentId:"cmmc-MP" },
  { id:"cmmc-SC", frameworkId:"cmmc", externalId:"SC", title:"System & Communications Protection", kind:"domain", parentId:null },
  { id:"cmmc-SC-3.13.11", frameworkId:"cmmc", externalId:"SC.L2-3.13.11", title:"FIPS-validated cryptography", kind:"practice", parentId:"cmmc-SC" },
];

function reqById(id) { return COMPLIANCE_REQUIREMENTS.find(r => r.id === id); }
function reqsOfFramework(frameworkId) { return COMPLIANCE_REQUIREMENTS.filter(r => r.frameworkId === frameworkId); }
function reqChildren(id) { return COMPLIANCE_REQUIREMENTS.filter(r => r.parentId === id); }
function reqBreadcrumb(id) {
  const chain = [];
  let cur = reqById(id);
  while (cur) { chain.unshift(cur); cur = cur.parentId ? reqById(cur.parentId) : null; }
  return chain;
}
function reqTree(frameworkId) {
  const roots = reqsOfFramework(frameworkId).filter(r => !r.parentId);
  const build = (node) => ({ ...node, children: reqChildren(node.id).map(build) });
  return roots.map(build);
}
function reqSearch(frameworkId, query) {
  const q = (query || "").trim().toLowerCase();
  const pool = reqsOfFramework(frameworkId);
  if (!q) return pool;
  return pool.filter(r => r.externalId.toLowerCase().includes(q) || r.title.toLowerCase().includes(q) || (r.cci||"").toLowerCase().includes(q));
}

const RELATIONSHIPS = [
  { id:"implements", label:"Implements", blurb:"The policy directly satisfies this requirement." },
  { id:"supports", label:"Supports", blurb:"The policy contributes to satisfying the requirement but does not satisfy it alone." },
  { id:"provides_evidence", label:"Provides evidence for", blurb:"The policy gathers or produces evidence relevant to determining compliance with the requirement." },
];
function relationshipMeta(id) { return RELATIONSHIPS.find(r => r.id === id) || RELATIONSHIPS[0]; }

let _mapSeq = 1;
function mapId() { return `map-${_mapSeq++}`; }

// Policy <-> Requirement mappings. provenance: "manual" | "imported" | "suggested".
const POLICY_REQUIREMENT_MAPPINGS = [
  // stig-ssh-hardening
  { id:mapId(), policyId:"stig-sshd", requirementId:"stig-V-268137", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-sshd", requirementId:"stig-V-268142", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-sshd", requirementId:"stig-V-268089", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-sshd", requirementId:"nist-AC-17", relationship:"implements", coverage:"full", provenance:"manual",
    rationale:"Disabling root SSH login and enforcing idle timeouts directly satisfies remote access control." },
  { id:mapId(), policyId:"stig-sshd", requirementId:"nist-SC-8", relationship:"supports", coverage:"partial", provenance:"manual",
    rationale:"FIPS-approved ciphers protect the SSH transport, but full SC-8 coverage needs org-wide key management too." },
  { id:mapId(), policyId:"stig-sshd", requirementId:"cis-5.1.8", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-sshd", requirementId:"cmmc-AC-3.1.12", relationship:"supports", coverage:"partial", provenance:"manual" },

  // stig-audit-daemon
  { id:mapId(), policyId:"stig-auditd", requirementId:"stig-V-268080", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-auditd", requirementId:"stig-V-268078", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-auditd", requirementId:"nist-AU-12", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-auditd", requirementId:"nist-SC-7", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-auditd", requirementId:"cis-4.1.1", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-auditd", requirementId:"cmmc-AU-3.3.1", relationship:"implements", coverage:"full", provenance:"manual" },

  // stig-consent-banner
  { id:mapId(), policyId:"stig-banner", requirementId:"stig-V-268082", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-banner", requirementId:"nist-AC-8", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-banner", requirementId:"cis-5.1.10", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-banner", requirementId:"cis-5.1.8", relationship:"supports", coverage:"partial", provenance:"manual",
    rationale:"The consent banner and root-login lockout are checked together on most SSH-access audits, so the banner policy also partially evidences this recommendation." },

  // stig-fips-crypto
  { id:mapId(), policyId:"stig-fips", requirementId:"stig-V-268168", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-fips", requirementId:"stig-V-268144", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-fips", requirementId:"nist-SC-13", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-fips", requirementId:"nist-SC-28", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-fips", requirementId:"nist-SC-8", relationship:"implements", coverage:"full", provenance:"manual",
    rationale:"FIPS-validated ciphers also directly satisfy SC-8's transmission confidentiality requirement — a second, independent path to the same control." },
  { id:mapId(), policyId:"stig-fips", requirementId:"cmmc-SC-3.13.11", relationship:"implements", coverage:"full", provenance:"manual" },

  // stig-usbguard
  { id:mapId(), policyId:"stig-usbguard", requirementId:"stig-V-268139", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-usbguard", requirementId:"nist-MP-7", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-usbguard", requirementId:"cmmc-MP-3.8.7", relationship:"implements", coverage:"full", provenance:"manual" },

  // stig-password-policy (current revision, id "stig-pwquality")
  { id:mapId(), policyId:"stig-pwquality", requirementId:"stig-V-268134", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-pwquality", requirementId:"stig-V-268130", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-pwquality", requirementId:"nist-IA-5", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-pwquality", requirementId:"cis-5.4.1", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-pwquality", requirementId:"cmmc-IA-3.5.7", relationship:"implements", coverage:"full", provenance:"manual" },

  // Additional DISA STIG rules — 1:1 policy-to-requirement (bulk-imported from XCCDF)
  { id:mapId(), policyId:"stig-mock-ssh-daemon-0", requirementId:"stig-V-268200", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-firewall-rules-1", requirementId:"stig-V-268201", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-audit-logging-2", requirementId:"stig-V-268202", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-account-lockout-3", requirementId:"stig-V-268203", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-password-complexity-4", requirementId:"stig-V-268204", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-kernel-hardening-5", requirementId:"stig-V-268205", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-filesystem-permissions-6", requirementId:"stig-V-268206", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-usb-device-control-7", requirementId:"stig-V-268207", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-tls-cipher-suite-8", requirementId:"stig-V-268208", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-dns-resolver-9", requirementId:"stig-V-268209", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-ntp-sync-10", requirementId:"stig-V-268210", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-syslog-forwarding-11", requirementId:"stig-V-268211", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-sudo-policy-12", requirementId:"stig-V-268212", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-pam-stack-13", requirementId:"stig-V-268213", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-selinux-apparmor-profile-14", requirementId:"stig-V-268214", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-boot-loader-integrity-15", requirementId:"stig-V-268215", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-disk-encryption-16", requirementId:"stig-V-268216", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-service-isolation-17", requirementId:"stig-V-268217", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-network-segmentation-18", requirementId:"stig-V-268218", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-container-runtime-19", requirementId:"stig-V-268219", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-package-signing-20", requirementId:"stig-V-268220", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-update-cadence-21", requirementId:"stig-V-268221", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-session-timeout-22", requirementId:"stig-V-268222", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-banner-text-23", requirementId:"stig-V-268223", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-log-rotation-24", requirementId:"stig-V-268224", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-core-dump-handling-25", requirementId:"stig-V-268225", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-ipv6-stack-26", requirementId:"stig-V-268226", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-usb-storage-27", requirementId:"stig-V-268227", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-bluetooth-radio-28", requirementId:"stig-V-268228", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-wireless-interface-29", requirementId:"stig-V-268229", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-snmp-daemon-30", requirementId:"stig-V-268230", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-nfs-export-31", requirementId:"stig-V-268231", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-samba-share-32", requirementId:"stig-V-268232", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-cron-daemon-33", requirementId:"stig-V-268233", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-mail-relay-34", requirementId:"stig-V-268234", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-x11-forwarding-35", requirementId:"stig-V-268235", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-vnc-service-36", requirementId:"stig-V-268236", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-container-image-scanning-37", requirementId:"stig-V-268237", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-secrets-storage-38", requirementId:"stig-V-268238", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-key-rotation-39", requirementId:"stig-V-268239", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-certificate-validation-40", requirementId:"stig-V-268240", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-kernel-module-loading-41", requirementId:"stig-V-268241", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-aslr-enforcement-42", requirementId:"stig-V-268242", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-stack-protector-43", requirementId:"stig-V-268243", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-ptrace-scope-44", requirementId:"stig-V-268244", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-coredump-storage-45", requirementId:"stig-V-268245", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-swap-encryption-46", requirementId:"stig-V-268246", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-tmp-mount-options-47", requirementId:"stig-V-268247", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-home-directory-perms-48", requirementId:"stig-V-268248", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-shell-history-49", requirementId:"stig-V-268249", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-login-banner-50", requirementId:"stig-V-268250", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-motd-content-51", requirementId:"stig-V-268251", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-idle-session-lock-52", requirementId:"stig-V-268252", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-screen-lock-53", requirementId:"stig-V-268253", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-ssh-daemon-54", requirementId:"stig-V-268254", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-firewall-rules-55", requirementId:"stig-V-268255", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-audit-logging-56", requirementId:"stig-V-268256", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-account-lockout-57", requirementId:"stig-V-268257", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-password-complexity-58", requirementId:"stig-V-268258", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-kernel-hardening-59", requirementId:"stig-V-268259", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-filesystem-permissions-60", requirementId:"stig-V-268260", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-usb-device-control-61", requirementId:"stig-V-268261", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-tls-cipher-suite-62", requirementId:"stig-V-268262", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-dns-resolver-63", requirementId:"stig-V-268263", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-ntp-sync-64", requirementId:"stig-V-268264", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-syslog-forwarding-65", requirementId:"stig-V-268265", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-sudo-policy-66", requirementId:"stig-V-268266", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-pam-stack-67", requirementId:"stig-V-268267", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-selinux-apparmor-profile-68", requirementId:"stig-V-268268", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-boot-loader-integrity-69", requirementId:"stig-V-268269", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-disk-encryption-70", requirementId:"stig-V-268270", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-service-isolation-71", requirementId:"stig-V-268271", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-network-segmentation-72", requirementId:"stig-V-268272", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-container-runtime-73", requirementId:"stig-V-268273", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-package-signing-74", requirementId:"stig-V-268274", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-update-cadence-75", requirementId:"stig-V-268275", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-session-timeout-76", requirementId:"stig-V-268276", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-banner-text-77", requirementId:"stig-V-268277", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-log-rotation-78", requirementId:"stig-V-268278", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-core-dump-handling-79", requirementId:"stig-V-268279", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-ipv6-stack-80", requirementId:"stig-V-268280", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-usb-storage-81", requirementId:"stig-V-268281", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-bluetooth-radio-82", requirementId:"stig-V-268282", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-wireless-interface-83", requirementId:"stig-V-268283", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-snmp-daemon-84", requirementId:"stig-V-268284", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-nfs-export-85", requirementId:"stig-V-268285", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-samba-share-86", requirementId:"stig-V-268286", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-cron-daemon-87", requirementId:"stig-V-268287", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-mail-relay-88", requirementId:"stig-V-268288", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-x11-forwarding-89", requirementId:"stig-V-268289", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-vnc-service-90", requirementId:"stig-V-268290", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-container-image-scanning-91", requirementId:"stig-V-268291", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-secrets-storage-92", requirementId:"stig-V-268292", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-key-rotation-93", requirementId:"stig-V-268293", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-certificate-validation-94", requirementId:"stig-V-268294", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-kernel-module-loading-95", requirementId:"stig-V-268295", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-aslr-enforcement-96", requirementId:"stig-V-268296", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-stack-protector-97", requirementId:"stig-V-268297", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-ptrace-scope-98", requirementId:"stig-V-268298", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-coredump-storage-99", requirementId:"stig-V-268299", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-swap-encryption-100", requirementId:"stig-V-268300", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-tmp-mount-options-101", requirementId:"stig-V-268301", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-mock-home-directory-perms-102", requirementId:"stig-V-268302", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"cve-gated", requirementId:"stig-V-268303", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
];

// Suggested (not-yet-accepted) mappings derived from a framework crosswalk — kept
// separate from POLICY_REQUIREMENT_MAPPINGS on purpose; a crosswalk between two
// requirements never silently becomes a policy mapping.
const SUGGESTED_MAPPINGS = [
  { id:"sug-1", policyId:"stig-sshd", requirementId:"nist-CM-6", derivedFrom:"DISA crosswalk (V-268137 → CM-6)" },
];

function mappingsForPolicy(policyId) { return POLICY_REQUIREMENT_MAPPINGS.filter(m => m.policyId === policyId); }
function suggestedForPolicy(policyId) { return SUGGESTED_MAPPINGS.filter(m => m.policyId === policyId); }
function mappingsForRequirement(reqId) { return POLICY_REQUIREMENT_MAPPINGS.filter(m => m.requirementId === reqId); }

// Group a policy's mappings by framework, each entry joined with its requirement + framework record.
function mappingsGroupedByFramework(policyId) {
  const rows = mappingsForPolicy(policyId).map(m => ({ mapping:m, requirement: reqById(m.requirementId), framework: frameworkById(reqById(m.requirementId)?.frameworkId) }));
  const byFw = new Map();
  rows.forEach(r => {
    const key = r.framework?.id || "unknown";
    if (!byFw.has(key)) byFw.set(key, { framework:r.framework, rows:[] });
    byFw.get(key).rows.push(r);
  });
  return Array.from(byFw.values());
}

// Which bundles (by lineage) reference this policy id — "used by N bundles", distinct from "mapped to N requirements".
function bundlesUsingPolicy(policyId) {
  const bundles = (typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : []).filter(b => (b.policyIds||[]).includes(policyId));
  const lineages = new Set(bundles.map(b => b.lineageId || b.id));
  return { bundles, count: lineages.size };
}

function isDuplicateMapping(policyId, requirementId, excludeId) {
  return POLICY_REQUIREMENT_MAPPINGS.some(m => m.policyId === policyId && m.requirementId === requirementId && m.id !== excludeId);
}

// Requirement coverage for a bundle: every requirement under the bundle's framework,
// derived purely from mappings of the policies actually selected into the bundle —
// never from a `family`/`framework` property living on the policy itself.
function bundleRequirementCoverage(bundle) {
  const fw = COMPLIANCE_FRAMEWORKS.find(f => f.name === bundle.framework);
  if (!fw) return null;
  const policyIds = new Set(bundle.policyIds || []);
  const allReqs = reqsOfFramework(fw.id).filter(r => reqChildren(r.id).length === 0); // leaf requirements only
  const rows = allReqs.map(req => {
    const maps = mappingsForRequirement(req.id).filter(m => policyIds.has(m.policyId));
    let status = "unmapped";
    if (maps.some(m => m.relationship === "implements" && m.coverage === "full")) status = "full";
    else if (maps.length) status = "partial";
    return { requirement:req, mappings:maps, status };
  });
  return {
    framework: fw,
    total: rows.length,
    full: rows.filter(r=>r.status==="full").length,
    partial: rows.filter(r=>r.status==="partial").length,
    unmapped: rows.filter(r=>r.status==="unmapped").length,
    rows,
  };
}

// Split candidate policies for "Add Policies to Bundle": mapped-to-this-framework vs custom additions.
function splitPoliciesForBundleFramework(policies, bundleFrameworkName) {
  const fw = COMPLIANCE_FRAMEWORKS.find(f => f.name === bundleFrameworkName);
  if (!fw) return { mapped: [], other: policies };
  const mapped = [], other = [];
  policies.forEach(p => {
    const hasMapping = mappingsForPolicy(p.id).some(m => reqById(m.requirementId)?.frameworkId === fw.id);
    (hasMapping ? mapped : other).push(p);
  });
  return { mapped, other };
}

Object.assign(window, {
  COMPLIANCE_FRAMEWORKS, COMPLIANCE_REQUIREMENTS, POLICY_REQUIREMENT_MAPPINGS, SUGGESTED_MAPPINGS, RELATIONSHIPS,
  frameworkById, reqById, reqsOfFramework, reqChildren, reqBreadcrumb, reqTree, reqSearch, relationshipMeta,
  mappingsForPolicy, suggestedForPolicy, mappingsForRequirement, mappingsGroupedByFramework, bundlesUsingPolicy,
  isDuplicateMapping, bundleRequirementCoverage, splitPoliciesForBundleFramework, mapId,
});
