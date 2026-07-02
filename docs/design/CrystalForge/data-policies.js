// Deployment policies — built-in + custom rules

// Category taxonomy — every policy is a criterion that must be met to deploy a system,
// grouped by the KIND of criterion so the registry reads clearly.
const POLICY_CATEGORIES = [
  { id:"deployment", label:"Deployment",         short:"Deploy",    color:"#60a5fa", icon:"deploy",
    blurb:"Base strategy — how and when a system picks up a new configuration." },
  { id:"pipeline",   label:"Pipeline gates",     short:"Pipeline",  color:"#a78bfa", icon:"build",
    blurb:"Gates on pipeline output — eval, build, and CVE results must pass before promotion." },
  { id:"rollout",    label:"Rollout control",    short:"Rollout",   color:"#fbbf24", icon:"sync",
    blurb:"Govern the timing, approvals, and staging of a rollout." },
  { id:"security",   label:"Security & hardening", short:"Security", color:"#f87171", icon:"shield",
    blurb:"Config-level assertions — STIG / hardening controls a system must satisfy." },
];
function policyCategoryMeta(id) {
  return POLICY_CATEGORIES.find(c => c.id === id) || POLICY_CATEGORIES[0];
}

const POLICY_BUILTIN = [
  {
    id: "manual",
    name: "manual",
    category: "deployment",
    description: "Operator must explicitly approve every deploy.",
    type: "builtin",
    rules: [],
    rationale: "Safest default for production-critical hosts. Every promotion is a human decision.",
  },
  {
    id: "auto_latest",
    name: "auto_latest",
    category: "deployment",
    description: "Auto-deploy the newest passing commit on the assigned flake/branch.",
    type: "builtin",
    rules: [{ kind:"eval_passed" }, { kind:"build_succeeded" }],
    rationale: "Best for dev and edge nodes that should always track HEAD.",
  },
  {
    id: "pinned",
    name: "pinned",
    category: "deployment",
    description: "Stay on a specific commit until manually changed.",
    type: "builtin",
    rules: [{ kind:"pin_required" }],
    rationale: "Holds a system at a known-good revision. Use for compliance baselines.",
  },
];

const POLICY_CUSTOM = [
  {
    id: "cve-gated",
    name: "cve-gated",
    category: "pipeline",
    description: "Block deploys that introduce any critical CVE.",
    type: "custom",
    enabled: true,
    rules: [
      { kind:"cve_block", severity:"critical", maxAllowed:0 },
      { kind:"cve_block", severity:"high",     maxAllowed:2 },
      { kind:"eval_passed" },
      { kind:"build_succeeded" },
    ],
    rationale: "Catches regressions surfaced by vulnix during eval. Critical = hard block.",
    createdBy: "mreyes",
    createdAt: "3mo ago",
    lastModified: "2w ago",
  },
  {
    id: "business-hours",
    name: "business-hours",
    category: "rollout",
    description: "Auto-deploy permitted only between 09:00–17:00 weekdays, US-East.",
    type: "custom",
    enabled: true,
    rules: [
      { kind:"time_window", days:["mon","tue","wed","thu","fri"], from:"09:00", to:"17:00", tz:"America/New_York" },
      { kind:"eval_passed" },
      { kind:"build_succeeded" },
    ],
    rationale: "Operator-coverage window. Outside hours, defer to manual.",
    createdBy: "jpark",
    createdAt: "5mo ago",
    lastModified: "1mo ago",
  },
  {
    id: "two-approver",
    name: "two-approver",
    category: "rollout",
    description: "Requires sign-off from 2 distinct operators with admin role.",
    type: "custom",
    enabled: true,
    rules: [
      { kind:"approval_required", count:2, role:"admin" },
      { kind:"eval_passed" },
      { kind:"build_succeeded" },
    ],
    rationale: "For tier-0 systems (auth providers, secrets brokers). 4-eyes principle.",
    createdBy: "security-team",
    createdAt: "1mo ago",
    lastModified: "3d ago",
  },
  {
    id: "canary-25",
    name: "canary-25",
    category: "rollout",
    description: "Roll out to 25% of matching systems at a time, watch for 30 min, then continue.",
    type: "custom",
    enabled: false,
    rules: [
      { kind:"rollout_percent", percent:25, observeMin:30 },
      { kind:"eval_passed" },
      { kind:"build_succeeded" },
    ],
    rationale: "Staged rollout for the web tier. Disabled — pending observability integration.",
    createdBy: "dchen",
    createdAt: "2w ago",
    lastModified: "yesterday",
  },
  {
    id: "stig-sshd",
    name: "stig-ssh-hardening",
    category: "security",
    description: "Anduril NixOS STIG: SSH daemon hardening — no root login, FIPS ciphers, 10-min idle timeout.",
    type: "custom",
    severity: "high",
    enabled: true,
    rules: [
      { kind:"nixos_option", path:"services.openssh.settings.PermitRootLogin", op:"==", value:"\"no\"" },
      { kind:"nixos_option", path:"services.openssh.settings.ClientAliveInterval", op:"==", value:"600" },
      { kind:"nixos_option", path:"services.openssh.settings.ClientAliveCountMax", op:"==", value:"0" },
      { kind:"custom_eval", expr:"builtins.all (c: builtins.elem c FIPS_APPROVED_CIPHERS) config.services.openssh.settings.Ciphers", message:"SSH must use only FIPS-validated ciphers" },
    ],
    rationale: "V-268137 (no root SSH login), V-268142 (10-min idle timeout), V-268089 (FIPS-approved remote-access encryption). SRG-OS-000109 / 000163 / 000033.",
    evidence: [
      { kind:"command", cmd:"sshd -T | grep -i permitrootlogin", expect:"permitrootlogin no" },
      { kind:"command", cmd:"sshd -T | grep -i clientaliveinterval", expect:"clientaliveinterval 600" },
      { kind:"command", cmd:"sshd -T | grep -i ciphers", expect:"FIPS-approved ciphers only" },
      { kind:"unit_state", unit:"sshd.service", state:"active" },
    ],
    createdBy: "security-team",
    createdAt: "2mo ago",
    lastModified: "1w ago",
  },
  {
    id: "stig-auditd",
    name: "stig-audit-daemon",
    category: "security",
    description: "Anduril NixOS STIG: audit daemon enabled with the firewall to enforce host logging and ingress control.",
    type: "custom",
    severity: "medium",
    enabled: true,
    rules: [
      { kind:"nixos_option", path:"security.audit.enable", op:"==", value:"true" },
      { kind:"nixos_option", path:"networking.firewall.enable", op:"==", value:"true" },
      { kind:"custom_eval", expr:"builtins.length config.security.audit.rules > 0", message:"Audit rules must be configured in configuration.nix" },
    ],
    rationale: "V-268080 (enable the audit daemon), V-268078 (enable the built-in firewall). SRG-OS-000004 / 000298.",
    evidence: [
      { kind:"unit_state", unit:"auditd.service", state:"active" },
      { kind:"command", cmd:"systemctl is-active auditd", expect:"active" },
      { kind:"command", cmd:"nixos-option networking.firewall.enable", expect:"true" },
    ],
    createdBy: "security-team",
    createdAt: "2mo ago",
    lastModified: "5d ago",
  },
  {
    id: "stig-banner",
    name: "stig-consent-banner",
    category: "security",
    description: "Anduril NixOS STIG: DoD Notice and Consent banner on all command-line logon paths.",
    type: "custom",
    severity: "medium",
    enabled: true,
    rules: [
      { kind:"nixos_option", path:"services.openssh.banner", op:"!=", value:"null" },
      { kind:"custom_eval", expr:"(builtins.match \".*USG.*\" (builtins.readFile config.environment.etc.\"issue\".source)) != null", message:"/etc/issue must contain the DoD/USG consent banner" },
    ],
    rationale: "V-268082 (display the Standard Mandatory DOD Notice and Consent Banner). SRG-OS-000023-GPOS-00006.",
    evidence: [
      { kind:"file", path:"/etc/issue", note:"Must contain the DoD/USG consent banner verbatim" },
      { kind:"command", cmd:"cat /etc/issue", expect:"DoD consent banner" },
    ],
    createdBy: "security-team",
    createdAt: "2mo ago",
    lastModified: "3w ago",
  },
  {
    id: "stig-fips",
    name: "stig-fips-crypto",
    category: "security",
    description: "Anduril NixOS STIG: FIPS-validated cryptography enabled and data-at-rest encrypted.",
    type: "custom",
    severity: "high",
    enabled: true,
    rules: [
      { kind:"nixos_option", path:"security.enableFIPSMode", op:"==", value:"true" },
      { kind:"custom_eval", expr:"config.boot.initrd.luks.devices != {}", message:"Data partitions must be LUKS-encrypted via boot.initrd.luks.devices" },
    ],
    rationale: "V-268168 (FIPS-validated cryptography), V-268144 (protect information at rest). SRG-OS-000478 / 000185.",
    evidence: [
      { kind:"command", cmd:"cat /proc/sys/crypto/fips_enabled", expect:"1" },
      { kind:"command", cmd:"lsblk -o NAME,TYPE,MOUNTPOINT | grep crypt", expect:"LUKS devices present" },
      { kind:"attestation", note:"Agent attests security.enableFIPSMode = true at activation" },
    ],
    createdBy: "security-team",
    createdAt: "6w ago",
    lastModified: "4d ago",
  },
  {
    id: "stig-usbguard",
    name: "stig-usbguard",
    category: "security",
    description: "Anduril NixOS STIG: USBguard enabled with an allow-list policy to control peripheral access.",
    type: "custom",
    severity: "medium",
    enabled: true,
    rules: [
      { kind:"nixos_option", path:"services.usbguard.enable", op:"==", value:"true" },
      { kind:"custom_eval", expr:"config.services.usbguard.rules != \"\"", message:"USBguard must define an allow-list policy" },
    ],
    rationale: "V-268139 (enable USBguard). SRG-OS-000114 \u2014 controls unauthorized peripheral connections.",
    evidence: [
      { kind:"unit_state", unit:"usbguard.service", state:"active" },
      { kind:"command", cmd:"systemctl is-active usbguard", expect:"active" },
    ],
    createdBy: "security-team",
    createdAt: "5w ago",
    lastModified: "6d ago",
  },
  {
    id: "stig-pwquality",
    name: "stig-password-policy",
    category: "security",
    description: "Anduril NixOS STIG: enforce 15-character minimum password length and encrypted password storage.",
    type: "custom",
    severity: "medium",
    enabled: false,
    rules: [
      { kind:"custom_eval", expr:"config.security.pam.services ? pwquality && config.security.pam.pwquality.minlen >= 15", message:"Minimum password length must be >= 15" },
      { kind:"custom_eval", expr:"builtins.elem config.security.pam.hashAlgorithm [\"yescrypt\" \"sha512\"]", message:"Passwords must be stored using yescrypt or sha512" },
    ],
    rationale: "V-268134 (15-char minimum length), V-268130 (encrypted password storage). SRG-OS-000078 / 000112.",
    evidence: [
      { kind:"command", cmd:"grep minlen /etc/security/pwquality.conf", expect:"minlen = 15" },
      { kind:"command", cmd:"awk -F: '($2!~/^\\$/){print $1}' /etc/shadow", expect:"no unhashed passwords" },
    ],
    createdBy: "security-team",
    createdAt: "4w ago",
    lastModified: "1w ago",
  },
];

const POLICIES = [...POLICY_BUILTIN, ...POLICY_CUSTOM];

// Per-policy usage rollup
function policyUsage(policyId) {
  const systems = (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).filter(s => s.deploymentPolicy === policyId);
  const byEnv = {};
  systems.forEach(s => { byEnv[s.environment] = (byEnv[s.environment] || 0) + 1; });
  return { systems, count: systems.length, byEnv };
}

Object.assign(window, { POLICIES, POLICY_BUILTIN, POLICY_CUSTOM, POLICY_CATEGORIES, policyCategoryMeta, policyUsage });
