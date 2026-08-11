// Deployment policies — built-in + custom rules

// Category taxonomy — every policy is a criterion that must be met to deploy a system,
// grouped by the KIND of criterion so the registry reads clearly.
const POLICY_CATEGORIES = [
  { id:"deployment", label:"Deployment",         short:"Deploy",    color:"#60a5fa", icon:"deploy",    domain:"platform",
    blurb:"Base strategy — how and when a system picks up a new configuration." },
  { id:"pipeline",   label:"Pipeline gates",     short:"Pipeline",  color:"#a78bfa", icon:"build",     domain:"platform",
    blurb:"Gates on pipeline output — eval, build, and CVE results must pass before promotion." },
  { id:"rollout",    label:"Rollout control",    short:"Rollout",   color:"#fbbf24", icon:"sync",      domain:"platform",
    blurb:"Govern the timing, approvals, and staging of a rollout." },
  { id:"security",   label:"Security & hardening", short:"Security", color:"#f87171", icon:"shield",   domain:"security",
    blurb:"Config-level assertions — STIG / hardening controls a system must satisfy." },
];
function policyCategoryMeta(id) {
  return POLICY_CATEGORIES.find(c => c.id === id) || POLICY_CATEGORIES[0];
}

// Two audiences, two top-level domains. Platform = how devops/admins run the pipeline.
// Security controls = what security/compliance people are accountable for against a
// framework — this domain supports pluggable grouping schemes (below) instead of one
// fixed taxonomy, since different orgs audit against different standards.
const POLICY_DOMAINS = [
  { id:"platform", label:"Platform", icon:"deploy", color:"#60a5fa",
    blurb:"Deployment modes, pipeline gates, and rollout control — configured by whoever runs the pipeline." },
  { id:"security",  label:"Security controls", icon:"shield", color:"#f87171",
    blurb:"Controls security/compliance own against a framework — grouped however they audit, not by CF's internal categories." },
];
function policyDomain(p) {
  return policyCategoryMeta(p.category || "deployment").domain || "platform";
}

// NIST 800-53 rev5 control families relevant to the STIG controls we model.
const CONTROL_FAMILIES = {
  AC: { id:"AC", label:"Access Control", blurb:"Who and what can authenticate, and what they're authorized to do once in." },
  AU: { id:"AU", label:"Audit & Accountability", blurb:"Logging, review, and non-repudiation of system activity." },
  CM: { id:"CM", label:"Configuration Management", blurb:"Baseline configs, change control, and inventory of what's running." },
  IA: { id:"IA", label:"Identification & Authentication", blurb:"Verifying the identity of users, devices, and processes." },
  SC: { id:"SC", label:"System & Communications Protection", blurb:"Protecting data in transit and isolating system boundaries." },
  SI: { id:"SI", label:"System & Information Integrity", blurb:"Detecting and correcting flaws, malicious code, and unauthorized change." },
  MP: { id:"MP", label:"Media Protection", blurb:"Controlling access to and sanitization of removable/physical media." },
};

// Predefined grouping schemes for the Security controls domain — a pivot over tags
// already on each policy, so switching schemes never touches the underlying policy data.
const GROUPING_SCHEMES = [
  { id:"control-family", label:"NIST 800-53 family", builtin:true,
    groupOf: (p) => p.controlFamily ? (CONTROL_FAMILIES[p.controlFamily]?.label || p.controlFamily) : "Ungrouped",
    groupKeyOf: (p) => p.controlFamily || "ungrouped" },
  { id:"severity", label:"STIG severity (CAT)", builtin:true,
    groupOf: (p) => p.severity === "high" ? "CAT I — High" : p.severity === "medium" ? "CAT II — Medium" : p.severity === "low" ? "CAT III — Low" : "Unrated",
    groupKeyOf: (p) => p.severity || "unrated" },
  { id:"cci", label:"CCI (Control Correlation Identifier)", builtin:true,
    groupOf: (p) => (p.cciIds && p.cciIds[0]) || "Unmapped",
    groupKeyOf: (p) => (p.cciIds && p.cciIds[0]) || "unmapped" },
  { id:"srg-category", label:"SRG category", builtin:true,
    groupOf: (p) => srgCategoryOf(p), groupKeyOf: (p) => srgCategoryOf(p) },
  { id:"cmmc-level", label:"CMMC 2.0 level", builtin:true,
    groupOf: (p) => cmmcLevelOf(p).label, groupKeyOf: (p) => cmmcLevelOf(p).id },
  { id:"cis-section", label:"CIS Benchmark section", builtin:true,
    groupOf: (p) => p.cisSection ? `Section ${p.cisSection.split(".")[0]}` : "Unmapped",
    groupKeyOf: (p) => p.cisSection ? p.cisSection.split(".")[0] : "unmapped" },
  { id:"remediation", label:"Remediation status", builtin:true,
    groupOf: (p) => remediationStatusOf(p).label, groupKeyOf: (p) => remediationStatusOf(p).id },
  { id:"flat", label:"Flat list (no grouping)", builtin:true,
    groupOf: () => null, groupKeyOf: () => "all" },
];

// SRG (Security Requirement Guide) category — the token right after "SRG-" in a
// DISA SRG id (e.g. SRG-OS-000109 -> "OS"). Standard DISA taxonomy, no local mapping needed.
const SRG_CATEGORY_LABELS = {
  OS: "Operating System", APP: "Application", NET: "Network", DB: "Database",
  ENCLAVE: "Enclave", MOB: "Mobile", VIRT: "Virtualization",
};
function srgCategoryOf(p) {
  const first = (p.srgIds || [])[0];
  if (!first) return "Unmapped";
  const m = first.match(/^SRG-([A-Z]+)-/);
  const tok = m ? m[1] : null;
  return tok ? `SRG: ${SRG_CATEGORY_LABELS[tok] || tok}` : "Unmapped";
}

// CMMC 2.0 level — no official STIG-to-CMMC crosswalk is modeled here; this derives a
// plausible level from STIG severity as a stand-in (higher-severity findings tend to back
// higher-maturity practices) unless a policy carries an explicit cmmcLevel override.
function cmmcLevelOf(p) {
  if (p.cmmcLevel) return { id:`l${p.cmmcLevel}`, label:`Level ${p.cmmcLevel}` };
  const lvl = p.severity === "high" ? 3 : p.severity === "medium" ? 2 : p.severity === "low" ? 1 : null;
  return lvl ? { id:`l${lvl}`, label:`Level ${lvl}` } : { id:"unrated", label:"Unrated" };
}

// Remediation status — derived from what kind of rules a policy already carries, not a
// separate data field: purely-declarative NixOS options are auto-remediated by the next
// build; custom_eval assertions still need someone to write the fix; anything else is
// manual/attestation-based.
function remediationStatusOf(p) {
  const kinds = new Set((p.rules || []).map(r => r.kind));
  if (kinds.size === 0) return { id:"manual", label:"Manual verification only" };
  if ([...kinds].every(k => k === "nixos_option")) return { id:"auto", label:"Automated (declarative)" };
  if (kinds.has("nixos_option") || kinds.has("custom_eval")) return { id:"semi", label:"Semi-automated (custom eval)" };
  return { id:"manual", label:"Manual verification only" };
}

// Custom/internal compliance frameworks — an org can define its own framework name
// (e.g. "Acme Internal Baseline") to use on New Bundle instead of only DISA STIG/NIST/CMMC.
// Persisted client-side like custom grouping schemes; each entry is just a label + id.
const BUILTIN_FRAMEWORKS = ["DISA STIG", "NIST 800-53", "CMMC 2.0", "CIS Benchmark"];
function loadCustomFrameworks() {
  try { const raw = localStorage.getItem("cf.customFrameworks"); if (raw) return JSON.parse(raw); } catch {}
  return [];
}
function saveCustomFrameworks(list) {
  try { localStorage.setItem("cf.customFrameworks", JSON.stringify(list)); } catch {}
}
function allFrameworkOptions() {
  return [...BUILTIN_FRAMEWORKS, ...loadCustomFrameworks().map(f => f.name)];
}
// Which id scheme(s) actually apply to each standard framework — SRG/CCI are DISA
// identifiers and don't exist under CIS or CMMC; CIS has its own section numbering.
const FRAMEWORK_ID_FIELDS = {
  "DISA STIG": ["srgIds", "cciIds"],
  "CIS Benchmark": ["cisSection"],
  "NIST 800-53": [],
  "CMMC 2.0": [],
};

// Custom grouping schemes an admin defines — e.g. an org-specific control catalog.
// Persisted client-side; each scheme owns a list of named groups, each holding an
// explicit list of policy ids (a manual pivot, since custom groups aren't tag-derived).
function loadCustomGroupingSchemes() {
  try { const raw = localStorage.getItem("cf.customGroupingSchemes"); if (raw) return JSON.parse(raw); } catch {}
  return [];
}
function saveCustomGroupingSchemes(list) {
  try { localStorage.setItem("cf.customGroupingSchemes", JSON.stringify(list)); } catch {}
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
    lineageId: "cve-gated",
    revision: 1,
    publicationState: "current",
    publishedDate: "2026-02-15",
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
    lineageId: "business-hours",
    revision: 1,
    publicationState: "current",
    publishedDate: "2026-03-08",
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
    lineageId: "two-approver",
    revision: 1,
    publicationState: "current",
    publishedDate: "2026-07-05",
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
    lineageId: "canary-25",
    revision: 1,
    publicationState: "draft",
    publishedDate: "2026-07-25",
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
    lineageId: "stig-sshd",
    revision: 1,
    publicationState: "current",
    publishedDate: "2026-05-28",
    srgIds: ["SRG-OS-000109","SRG-OS-000163","SRG-OS-000033"],
    cciIds: ["CCI-000770","CCI-001133","CCI-000068"],
    name: "stig-ssh-hardening",
    category: "security",
    controlFamily: "AC",
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
    lineageId: "stig-auditd",
    revision: 1,
    publicationState: "current",
    publishedDate: "2026-06-02",
    srgIds: ["SRG-OS-000004","SRG-OS-000298"],
    cciIds: ["CCI-000018","CCI-000366"],
    name: "stig-audit-daemon",
    category: "security",
    controlFamily: "AU",
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
    lineageId: "stig-banner",
    revision: 1,
    publicationState: "current",
    publishedDate: "2026-06-18",
    srgIds: ["SRG-OS-000023-GPOS-00006"],
    cciIds: ["CCI-000048"],
    name: "stig-consent-banner",
    category: "security",
    controlFamily: "AC",
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
    lineageId: "stig-fips",
    revision: 1,
    publicationState: "current",
    publishedDate: "2026-06-25",
    srgIds: ["SRG-OS-000478","SRG-OS-000185"],
    cciIds: ["CCI-002450","CCI-001199"],
    name: "stig-fips-crypto",
    category: "security",
    controlFamily: "SC",
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
    lineageId: "stig-usbguard",
    revision: 1,
    publicationState: "current",
    publishedDate: "2026-07-01",
    srgIds: ["SRG-OS-000114"],
    cciIds: ["CCI-001958"],
    name: "stig-usbguard",
    category: "security",
    controlFamily: "MP",
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
    lineageId: "stig-pwquality",
    revision: 2,
    publicationState: "current",
    publishedDate: "2026-07-10",
    digest: "sha256:2b7e91",
    srgIds: ["SRG-OS-000078","SRG-OS-000112"],
    cciIds: ["CCI-000205","CCI-000196"],
    name: "stig-password-policy",
    category: "security",
    controlFamily: "IA",
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
  {
    id: "stig-pwquality-r2",
    lineageId: "stig-pwquality",
    revision: 2,
    publicationState: "deprecated",
    publishedDate: "2026-04-01",
    digest: "sha256:5c9d02",
    srgIds: ["SRG-OS-000078"],
    cciIds: ["CCI-000205"],
    name: "stig-password-policy",
    category: "security",
    controlFamily: "IA",
    description: "Anduril NixOS STIG: enforce 12-character minimum password length.",
    type: "custom",
    severity: "medium",
    enabled: false,
    rules: [
      { kind:"custom_eval", expr:"config.security.pam.services ? pwquality && config.security.pam.pwquality.minlen >= 12", message:"Minimum password length must be >= 12" },
    ],
    rationale: "V-268134 (12-char minimum length, interim revision). SRG-OS-000078.",
    evidence: [
      { kind:"command", cmd:"grep minlen /etc/security/pwquality.conf", expect:"minlen = 12" },
    ],
    createdBy: "security-team",
    createdAt: "4mo ago",
    lastModified: "3mo ago",
  },
  {
    id: "stig-pwquality-r3",
    lineageId: "stig-pwquality",
    revision: 3,
    publicationState: "deprecated",
    publishedDate: "2026-05-05",
    digest: "sha256:6da813",
    srgIds: ["SRG-OS-000078"],
    cciIds: ["CCI-000205"],
    name: "stig-password-policy",
    category: "security",
    controlFamily: "IA",
    description: "Anduril NixOS STIG: enforce 13-character minimum password length.",
    type: "custom",
    severity: "medium",
    enabled: false,
    rules: [
      { kind:"custom_eval", expr:"config.security.pam.services ? pwquality && config.security.pam.pwquality.minlen >= 13", message:"Minimum password length must be >= 13" },
    ],
    rationale: "V-268134 (13-char minimum length, interim revision). SRG-OS-000078.",
    evidence: [
      { kind:"command", cmd:"grep minlen /etc/security/pwquality.conf", expect:"minlen = 13" },
    ],
    createdBy: "security-team",
    createdAt: "3mo ago",
    lastModified: "2mo ago",
  },
  {
    id: "stig-pwquality-r4",
    lineageId: "stig-pwquality",
    revision: 4,
    publicationState: "deprecated",
    publishedDate: "2026-06-02",
    digest: "sha256:7eb924",
    srgIds: ["SRG-OS-000078","SRG-OS-000112"],
    cciIds: ["CCI-000205","CCI-000196"],
    name: "stig-password-policy",
    category: "security",
    controlFamily: "IA",
    description: "Anduril NixOS STIG: enforce 14-character minimum password length and encrypted password storage.",
    type: "custom",
    severity: "medium",
    enabled: false,
    rules: [
      { kind:"custom_eval", expr:"config.security.pam.services ? pwquality && config.security.pam.pwquality.minlen >= 14", message:"Minimum password length must be >= 14" },
      { kind:"custom_eval", expr:"builtins.elem config.security.pam.hashAlgorithm [\"yescrypt\" \"sha512\"]", message:"Passwords must be stored using yescrypt or sha512" },
    ],
    rationale: "V-268134 (14-char minimum length), V-268130 (encrypted password storage). SRG-OS-000078 / 000112.",
    evidence: [
      { kind:"command", cmd:"grep minlen /etc/security/pwquality.conf", expect:"minlen = 14" },
    ],
    createdBy: "security-team",
    createdAt: "2mo ago",
    lastModified: "5w ago",
  },
  {
    id: "stig-pwquality-r1",
    lineageId: "stig-pwquality",
    revision: 1,
    publicationState: "deprecated",
    publishedDate: "2026-03-02",
    digest: "sha256:9a10f4",
    srgIds: ["SRG-OS-000078"],
    cciIds: ["CCI-000205"],
    name: "stig-password-policy",
    category: "security",
    controlFamily: "IA",
    description: "Anduril NixOS STIG: enforce 10-character minimum password length (superseded by 15-char revision).",
    type: "custom",
    severity: "medium",
    enabled: false,
    rules: [
      { kind:"custom_eval", expr:"config.security.pam.services ? pwquality && config.security.pam.pwquality.minlen >= 10", message:"Minimum password length must be >= 10" },
    ],
    rationale: "V-268134 (10-char minimum length, prior revision). SRG-OS-000078.",
    evidence: [
      { kind:"command", cmd:"grep minlen /etc/security/pwquality.conf", expect:"minlen = 10" },
    ],
    createdBy: "security-team",
    createdAt: "5mo ago",
    lastModified: "4mo ago",
  },
];

// Group policies by lineage (bundle-independent revision history) — newest revision first.
function groupPoliciesByLineage(policies) {
  const byLineage = new Map();
  policies.forEach(p => {
    const key = p.lineageId || p.id;
    if (!byLineage.has(key)) byLineage.set(key, []);
    byLineage.get(key).push(p);
  });
  return Array.from(byLineage.entries()).map(([lineageId, revisions]) => {
    const sorted = [...revisions].sort((a,b) => (b.revision||0) - (a.revision||0));
    const current = sorted.find(r => r.publicationState === "current") || sorted[0];
    return { lineageId, current, revisions: sorted };
  });
}

const POLICIES = (typeof __fx === "function" && __fx("policies")) || [...POLICY_BUILTIN, ...POLICY_CUSTOM];

// Per-policy usage rollup
function policyUsage(policyId) {
  const systems = (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).filter(s => s.deploymentPolicy === policyId);
  const byEnv = {};
  systems.forEach(s => { byEnv[s.environment] = (byEnv[s.environment] || 0) + 1; });
  return { systems, count: systems.length, byEnv };
}

Object.assign(window, { POLICIES, POLICY_BUILTIN, POLICY_CUSTOM, POLICY_CATEGORIES, POLICY_DOMAINS, CONTROL_FAMILIES, GROUPING_SCHEMES, policyCategoryMeta, policyDomain, policyUsage, groupPoliciesByLineage, loadCustomGroupingSchemes, saveCustomGroupingSchemes, srgCategoryOf, cmmcLevelOf, remediationStatusOf, BUILTIN_FRAMEWORKS, loadCustomFrameworks, saveCustomFrameworks, allFrameworkOptions, FRAMEWORK_ID_FIELDS });
