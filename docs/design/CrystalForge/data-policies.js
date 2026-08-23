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
  CP: { id:"CP", label:"Contingency Planning", blurb:"Backup, restore, and continuity of operations after a failure." },
  IR: { id:"IR", label:"Incident Response", blurb:"Detecting, reporting, and handling security incidents." },
  MA: { id:"MA", label:"Maintenance", blurb:"Controlled local and nonlocal system maintenance and diagnostics." },
  PE: { id:"PE", label:"Physical & Environmental Protection", blurb:"Physical access, boot integrity, and environmental safeguards." },
  PL: { id:"PL", label:"Planning", blurb:"Security plans, architecture records, and rules of behavior." },
  PS: { id:"PS", label:"Personnel Security", blurb:"Role assignment, least privilege, and access on personnel change." },
  RA: { id:"RA", label:"Risk Assessment", blurb:"Vulnerability scanning cadence and risk categorization." },
  SA: { id:"SA", label:"System & Services Acquisition", blurb:"Supply-chain provenance, SBOMs, and developer requirements." },
  SR: { id:"SR", label:"Supply Chain Risk Management", blurb:"Provenance, tamper detection, and component traceability." },
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
    rules: [{ kind:"eval_passed" }],
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
    vulnId: "V-268144",
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
  {
    id: "stig-ssh-pam",
    lineageId: "stig-ssh-pam",
    revision: 1,
    publicationState: "draft",
    publishedDate: "2026-08-16",
    srgIds: [],
    cciIds: ["CCI-000877"],
    name: "NixOS must employ strong authenticators in the establishment of nonlocal maintenance and diagnostic sessions.",
    category: "security",
    controlFamily: "IA",
    description: "If maintenance tools are used by unauthorized personnel, they may accidentally or intentionally damage or compromise the system. The act of managing systems and applications includes the ability to access sensitive application information, such as system configuration details, diagnostic information, user information, and potentially sensitive application data.\n\nSome maintenance and test tools are either standalone devices with their own operating systems or are applications bundled with an operating system.\n\nNonlocal maintenance and diagnostic activities are those activities conducted by individuals communicating through a network, either an external network (e.g., the internet) or an internal network. Local maintenance and diagnostic activities are those activities carried out by individuals physically present at the information system or information system component and not communicating across a network connection. Typically, strong authentication requires authenticators that are resistant to replay attacks and employ multifactor authentication. Strong authenticators include, for example, PKI where certificates are stored on a token protected by a password, passphrase, or biometric.",
    type: "custom",
    severity: "high",
    enabled: false,
    rules: [
      { kind:"nixos_option", path:"services.openssh.settings.UsePAM", op:"==", value:"\"yes\"" },
    ],
    rationale: "Configure the NixOS operating system to use strong authentication when establishing nonlocal maintenance and diagnostic sessions. Add or modify the following line to /etc/nixos/configuration.nix: openssh.settings.UsePAM = \"yes\"; then rebuild with `sudo nixos-rebuild switch`.",
    evidence: [
      { kind:"command", cmd:"sudo /run/current-system/sw/bin/sshd -G | grep pam", expect:"usepam yes" },
    ],
    createdBy: "imported",
    createdAt: "just now",
    lastModified: "just now",
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

// Additional STIG rules bulk-generated to realistically fill out the Anduril NixOS STIG bundle (~110 controls total)
const POLICY_STIG_MOCK = [
  { id:"stig-mock-ssh-daemon-0", lineageId:"stig-mock-ssh-daemon-0", revision:1, publicationState:"current", publishedDate:"2026-01-01", srgIds:["SRG-OS-184669"], cciIds:["CCI-005673"], name:"stig-ssh-daemon", category:"security", controlFamily:"AU", description:"Anduril NixOS STIG: enforce SSH daemon baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.ssh_daemon.enabled == true", message:"SSH daemon must be configured per STIG V-268202" }], rationale:"V-268202 (SSH daemon). SRG-OS-0184669.", evidence:[{ kind:"command", cmd:"check-ssh-daemon --verify", expect:"pass" }], createdBy:"security-team", createdAt:"1w ago", lastModified:"1d ago", framework:"DISA STIG" },
  { id:"stig-mock-firewall-rules-1", lineageId:"stig-mock-firewall-rules-1", revision:1, publicationState:"current", publishedDate:"2026-02-02", srgIds:["SRG-OS-971192"], cciIds:["CCI-008757"], name:"stig-firewall-rules", category:"security", controlFamily:"IA", description:"Anduril NixOS STIG: enforce firewall rules baseline configuration.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.firewall_rules.enabled == true", message:"firewall rules must be configured per STIG V-268205" }], rationale:"V-268205 (firewall rules). SRG-OS-0971192.", evidence:[{ kind:"command", cmd:"check-firewall-rules --verify", expect:"pass" }], createdBy:"security-team", createdAt:"2w ago", lastModified:"2d ago", framework:"DISA STIG" },
  { id:"stig-mock-audit-logging-2", lineageId:"stig-mock-audit-logging-2", revision:1, publicationState:"current", publishedDate:"2026-03-03", srgIds:["SRG-OS-318831"], cciIds:["CCI-009798"], name:"stig-audit-logging", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce audit logging baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.audit_logging.enabled == true", message:"audit logging must be configured per STIG V-268208" }], rationale:"V-268208 (audit logging). SRG-OS-0318831.", evidence:[{ kind:"command", cmd:"check-audit-logging --verify", expect:"pass" }], createdBy:"security-team", createdAt:"3w ago", lastModified:"3d ago", framework:"DISA STIG" },
  { id:"stig-mock-account-lockout-3", lineageId:"stig-mock-account-lockout-3", revision:1, publicationState:"current", publishedDate:"2026-04-04", srgIds:["SRG-OS-431189"], cciIds:["CCI-006779"], name:"stig-account-lockout", category:"security", controlFamily:"AU", description:"Anduril NixOS STIG: enforce account lockout baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.account_lockout.enabled == true", message:"account lockout must be configured per STIG V-268211" }], rationale:"V-268211 (account lockout). SRG-OS-0431189.", evidence:[{ kind:"command", cmd:"check-account-lockout --verify", expect:"pass" }], createdBy:"security-team", createdAt:"4w ago", lastModified:"4d ago", framework:"DISA STIG" },
  { id:"stig-mock-password-complexity-4", lineageId:"stig-mock-password-complexity-4", revision:1, publicationState:"current", publishedDate:"2026-05-05", srgIds:["SRG-OS-591259"], cciIds:["CCI-002454"], name:"stig-password-complexity", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce password complexity baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.password_complexity.enabled == true", message:"password complexity must be configured per STIG V-268212" }], rationale:"V-268212 (password complexity). SRG-OS-0591259.", evidence:[{ kind:"command", cmd:"check-password-complexity --verify", expect:"pass" }], createdBy:"security-team", createdAt:"5w ago", lastModified:"5d ago", framework:"DISA STIG" },
  { id:"stig-mock-kernel-hardening-5", lineageId:"stig-mock-kernel-hardening-5", revision:1, publicationState:"current", publishedDate:"2026-06-06", srgIds:["SRG-OS-173048"], cciIds:["CCI-008838"], name:"stig-kernel-hardening", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce kernel hardening baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.kernel_hardening.enabled == true", message:"kernel hardening must be configured per STIG V-268215" }], rationale:"V-268215 (kernel hardening). SRG-OS-0173048.", evidence:[{ kind:"command", cmd:"check-kernel-hardening --verify", expect:"pass" }], createdBy:"security-team", createdAt:"6w ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-mock-filesystem-permissions-6", lineageId:"stig-mock-filesystem-permissions-6", revision:1, publicationState:"current", publishedDate:"2026-07-07", srgIds:["SRG-OS-886350"], cciIds:["CCI-007329"], name:"stig-filesystem-permissions", category:"security", controlFamily:"MP", description:"Anduril NixOS STIG: enforce filesystem permissions baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.filesystem_permissions.enabled == true", message:"filesystem permissions must be configured per STIG V-268219" }], rationale:"V-268219 (filesystem permissions). SRG-OS-0886350.", evidence:[{ kind:"command", cmd:"check-filesystem-permissions --verify", expect:"pass" }], createdBy:"security-team", createdAt:"7w ago", lastModified:"7d ago", framework:"DISA STIG" },
  { id:"stig-mock-usb-device-control-7", lineageId:"stig-mock-usb-device-control-7", revision:1, publicationState:"current", publishedDate:"2026-08-08", srgIds:["SRG-OS-580155"], cciIds:["CCI-008186"], name:"stig-usb-device-control", category:"security", controlFamily:"AC", description:"Anduril NixOS STIG: enforce USB device control baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.usb_device_control.enabled == true", message:"USB device control must be configured per STIG V-268222" }], rationale:"V-268222 (USB device control). SRG-OS-0580155.", evidence:[{ kind:"command", cmd:"check-usb-device-control --verify", expect:"pass" }], createdBy:"security-team", createdAt:"8w ago", lastModified:"8d ago", framework:"DISA STIG" },
  { id:"stig-mock-tls-cipher-suite-8", lineageId:"stig-mock-tls-cipher-suite-8", revision:1, publicationState:"current", publishedDate:"2026-09-09", srgIds:["SRG-OS-601200"], cciIds:["CCI-001993"], name:"stig-tls-cipher-suite", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce TLS cipher suite baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.tls_cipher_suite.enabled == true", message:"TLS cipher suite must be configured per STIG V-268226" }], rationale:"V-268226 (TLS cipher suite). SRG-OS-0601200.", evidence:[{ kind:"command", cmd:"check-tls-cipher-suite --verify", expect:"pass" }], createdBy:"security-team", createdAt:"9w ago", lastModified:"9d ago", framework:"DISA STIG" },
  { id:"stig-mock-dns-resolver-9", lineageId:"stig-mock-dns-resolver-9", revision:1, publicationState:"current", publishedDate:"2026-01-10", srgIds:["SRG-OS-307171"], cciIds:["CCI-001529"], name:"stig-dns-resolver", category:"security", controlFamily:"SI", description:"Anduril NixOS STIG: enforce DNS resolver baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.dns_resolver.enabled == true", message:"DNS resolver must be configured per STIG V-268228" }], rationale:"V-268228 (DNS resolver). SRG-OS-0307171.", evidence:[{ kind:"command", cmd:"check-dns-resolver --verify", expect:"pass" }], createdBy:"security-team", createdAt:"10w ago", lastModified:"10d ago", framework:"DISA STIG" },
  { id:"stig-mock-ntp-sync-10", lineageId:"stig-mock-ntp-sync-10", revision:1, publicationState:"current", publishedDate:"2026-02-11", srgIds:["SRG-OS-412917"], cciIds:["CCI-006851"], name:"stig-ntp-sync", category:"security", controlFamily:"IA", description:"Anduril NixOS STIG: enforce NTP sync baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.ntp_sync.enabled == true", message:"NTP sync must be configured per STIG V-268230" }], rationale:"V-268230 (NTP sync). SRG-OS-0412917.", evidence:[{ kind:"command", cmd:"check-ntp-sync --verify", expect:"pass" }], createdBy:"security-team", createdAt:"11w ago", lastModified:"11d ago", framework:"DISA STIG" },
  { id:"stig-mock-syslog-forwarding-11", lineageId:"stig-mock-syslog-forwarding-11", revision:1, publicationState:"current", publishedDate:"2026-03-12", srgIds:["SRG-OS-896313"], cciIds:["CCI-008622"], name:"stig-syslog-forwarding", category:"security", controlFamily:"MP", description:"Anduril NixOS STIG: enforce syslog forwarding baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.syslog_forwarding.enabled == true", message:"syslog forwarding must be configured per STIG V-268235" }], rationale:"V-268235 (syslog forwarding). SRG-OS-0896313.", evidence:[{ kind:"command", cmd:"check-syslog-forwarding --verify", expect:"pass" }], createdBy:"security-team", createdAt:"12w ago", lastModified:"12d ago", framework:"DISA STIG" },
  { id:"stig-mock-sudo-policy-12", lineageId:"stig-mock-sudo-policy-12", revision:1, publicationState:"current", publishedDate:"2026-04-13", srgIds:["SRG-OS-452093"], cciIds:["CCI-005328"], name:"stig-sudo-policy", category:"security", controlFamily:"IA", description:"Anduril NixOS STIG: enforce sudo policy baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.sudo_policy.enabled == true", message:"sudo policy must be configured per STIG V-268236" }], rationale:"V-268236 (sudo policy). SRG-OS-0452093.", evidence:[{ kind:"command", cmd:"check-sudo-policy --verify", expect:"pass" }], createdBy:"security-team", createdAt:"1w ago", lastModified:"13d ago", framework:"DISA STIG" },
  { id:"stig-mock-pam-stack-13", lineageId:"stig-mock-pam-stack-13", revision:1, publicationState:"current", publishedDate:"2026-05-14", srgIds:["SRG-OS-550585"], cciIds:["CCI-004434"], name:"stig-pam-stack", category:"security", controlFamily:"CM", description:"Anduril NixOS STIG: enforce PAM stack baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.pam_stack.enabled == true", message:"PAM stack must be configured per STIG V-268241" }], rationale:"V-268241 (PAM stack). SRG-OS-0550585.", evidence:[{ kind:"command", cmd:"check-pam-stack --verify", expect:"pass" }], createdBy:"security-team", createdAt:"2w ago", lastModified:"14d ago", framework:"DISA STIG" },
  { id:"stig-mock-selinux-apparmor-profile-14", lineageId:"stig-mock-selinux-apparmor-profile-14", revision:1, publicationState:"current", publishedDate:"2026-06-15", srgIds:["SRG-OS-577733"], cciIds:["CCI-008133"], name:"stig-selinux-apparmor-profile", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce SELinux/AppArmor profile baseline configuration.", type:"custom", severity:"low", enabled:false, rules:[{ kind:"custom_eval", expr:"config.mock.selinux_apparmor_profile.enabled == true", message:"SELinux/AppArmor profile must be configured per STIG V-268243" }], rationale:"V-268243 (SELinux/AppArmor profile). SRG-OS-0577733.", evidence:[{ kind:"command", cmd:"check-selinux-apparmor-profile --verify", expect:"pass" }], createdBy:"security-team", createdAt:"3w ago", lastModified:"15d ago", framework:"DISA STIG" },
  { id:"stig-mock-boot-loader-integrity-15", lineageId:"stig-mock-boot-loader-integrity-15", revision:1, publicationState:"current", publishedDate:"2026-07-16", srgIds:["SRG-OS-196745"], cciIds:["CCI-003624"], name:"stig-boot-loader-integrity", category:"security", controlFamily:"AC", description:"Anduril NixOS STIG: enforce boot loader integrity baseline configuration.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.boot_loader_integrity.enabled == true", message:"boot loader integrity must be configured per STIG V-268247" }], rationale:"V-268247 (boot loader integrity). SRG-OS-0196745.", evidence:[{ kind:"command", cmd:"check-boot-loader-integrity --verify", expect:"pass" }], createdBy:"security-team", createdAt:"4w ago", lastModified:"16d ago", framework:"DISA STIG" },
  { id:"stig-mock-disk-encryption-16", lineageId:"stig-mock-disk-encryption-16", revision:1, publicationState:"current", publishedDate:"2026-08-17", srgIds:["SRG-OS-123967"], cciIds:["CCI-005270"], name:"stig-disk-encryption", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce disk encryption baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.disk_encryption.enabled == true", message:"disk encryption must be configured per STIG V-268250" }], rationale:"V-268250 (disk encryption). SRG-OS-0123967.", evidence:[{ kind:"command", cmd:"check-disk-encryption --verify", expect:"pass" }], createdBy:"security-team", createdAt:"5w ago", lastModified:"17d ago", framework:"DISA STIG" },
  { id:"stig-mock-service-isolation-17", lineageId:"stig-mock-service-isolation-17", revision:1, publicationState:"current", publishedDate:"2026-09-18", srgIds:["SRG-OS-645834"], cciIds:["CCI-003861"], name:"stig-service-isolation", category:"security", controlFamily:"MP", description:"Anduril NixOS STIG: enforce service isolation baseline configuration.", type:"custom", severity:"medium", enabled:false, rules:[{ kind:"custom_eval", expr:"config.mock.service_isolation.enabled == true", message:"service isolation must be configured per STIG V-268253" }], rationale:"V-268253 (service isolation). SRG-OS-0645834.", evidence:[{ kind:"command", cmd:"check-service-isolation --verify", expect:"pass" }], createdBy:"security-team", createdAt:"6w ago", lastModified:"18d ago", framework:"DISA STIG" },
  { id:"stig-mock-network-segmentation-18", lineageId:"stig-mock-network-segmentation-18", revision:1, publicationState:"current", publishedDate:"2026-01-19", srgIds:["SRG-OS-563849"], cciIds:["CCI-006172"], name:"stig-network-segmentation", category:"security", controlFamily:"MP", description:"Anduril NixOS STIG: enforce network segmentation baseline configuration.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.network_segmentation.enabled == true", message:"network segmentation must be configured per STIG V-268254" }], rationale:"V-268254 (network segmentation). SRG-OS-0563849.", evidence:[{ kind:"command", cmd:"check-network-segmentation --verify", expect:"pass" }], createdBy:"security-team", createdAt:"7w ago", lastModified:"19d ago", framework:"DISA STIG" },
  { id:"stig-mock-container-runtime-19", lineageId:"stig-mock-container-runtime-19", revision:1, publicationState:"current", publishedDate:"2026-02-20", srgIds:["SRG-OS-934076"], cciIds:["CCI-005250"], name:"stig-container-runtime", category:"security", controlFamily:"SC", description:"Anduril NixOS STIG: enforce container runtime baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.container_runtime.enabled == true", message:"container runtime must be configured per STIG V-268258" }], rationale:"V-268258 (container runtime). SRG-OS-0934076.", evidence:[{ kind:"command", cmd:"check-container-runtime --verify", expect:"pass" }], createdBy:"security-team", createdAt:"8w ago", lastModified:"20d ago", framework:"DISA STIG" },
  { id:"stig-mock-package-signing-20", lineageId:"stig-mock-package-signing-20", revision:1, publicationState:"current", publishedDate:"2026-03-21", srgIds:["SRG-OS-810562"], cciIds:["CCI-006108"], name:"stig-package-signing", category:"security", controlFamily:"AC", description:"Anduril NixOS STIG: enforce package signing baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.package_signing.enabled == true", message:"package signing must be configured per STIG V-268262" }], rationale:"V-268262 (package signing). SRG-OS-0810562.", evidence:[{ kind:"command", cmd:"check-package-signing --verify", expect:"pass" }], createdBy:"security-team", createdAt:"9w ago", lastModified:"21d ago", framework:"DISA STIG" },
  { id:"stig-mock-update-cadence-21", lineageId:"stig-mock-update-cadence-21", revision:1, publicationState:"current", publishedDate:"2026-04-22", srgIds:["SRG-OS-501960"], cciIds:["CCI-005877"], name:"stig-update-cadence", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce update cadence baseline configuration.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.update_cadence.enabled == true", message:"update cadence must be configured per STIG V-268265" }], rationale:"V-268265 (update cadence). SRG-OS-0501960.", evidence:[{ kind:"command", cmd:"check-update-cadence --verify", expect:"pass" }], createdBy:"security-team", createdAt:"10w ago", lastModified:"22d ago", framework:"DISA STIG" },
  { id:"stig-mock-session-timeout-22", lineageId:"stig-mock-session-timeout-22", revision:1, publicationState:"current", publishedDate:"2026-05-23", srgIds:["SRG-OS-751332"], cciIds:["CCI-002744"], name:"stig-session-timeout", category:"security", controlFamily:"SC", description:"Anduril NixOS STIG: enforce session timeout baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.session_timeout.enabled == true", message:"session timeout must be configured per STIG V-268268" }], rationale:"V-268268 (session timeout). SRG-OS-0751332.", evidence:[{ kind:"command", cmd:"check-session-timeout --verify", expect:"pass" }], createdBy:"security-team", createdAt:"11w ago", lastModified:"23d ago", framework:"DISA STIG" },
  { id:"stig-mock-banner-text-23", lineageId:"stig-mock-banner-text-23", revision:1, publicationState:"current", publishedDate:"2026-06-24", srgIds:["SRG-OS-444321"], cciIds:["CCI-006238"], name:"stig-banner-text", category:"security", controlFamily:"AC", description:"Anduril NixOS STIG: enforce banner text baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.banner_text.enabled == true", message:"banner text must be configured per STIG V-268270" }], rationale:"V-268270 (banner text). SRG-OS-0444321.", evidence:[{ kind:"command", cmd:"check-banner-text --verify", expect:"pass" }], createdBy:"security-team", createdAt:"12w ago", lastModified:"24d ago", framework:"DISA STIG" },
  { id:"stig-mock-log-rotation-24", lineageId:"stig-mock-log-rotation-24", revision:1, publicationState:"current", publishedDate:"2026-07-25", srgIds:["SRG-OS-484374"], cciIds:["CCI-004507"], name:"stig-log-rotation", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce log rotation baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.log_rotation.enabled == true", message:"log rotation must be configured per STIG V-268273" }], rationale:"V-268273 (log rotation). SRG-OS-0484374.", evidence:[{ kind:"command", cmd:"check-log-rotation --verify", expect:"pass" }], createdBy:"security-team", createdAt:"1w ago", lastModified:"25d ago", framework:"DISA STIG" },
  { id:"stig-mock-core-dump-handling-25", lineageId:"stig-mock-core-dump-handling-25", revision:1, publicationState:"current", publishedDate:"2026-08-26", srgIds:["SRG-OS-639228"], cciIds:["CCI-008070"], name:"stig-core-dump-handling", category:"security", controlFamily:"AU", description:"Anduril NixOS STIG: enforce core dump handling baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.core_dump_handling.enabled == true", message:"core dump handling must be configured per STIG V-268277" }], rationale:"V-268277 (core dump handling). SRG-OS-0639228.", evidence:[{ kind:"command", cmd:"check-core-dump-handling --verify", expect:"pass" }], createdBy:"security-team", createdAt:"2w ago", lastModified:"26d ago", framework:"DISA STIG", cisSection:"1.1" },
  { id:"stig-mock-ipv6-stack-26", lineageId:"stig-mock-ipv6-stack-26", revision:1, publicationState:"current", publishedDate:"2026-09-27", srgIds:["SRG-OS-504178"], cciIds:["CCI-006139"], name:"stig-ipv6-stack", category:"security", controlFamily:"IA", description:"Anduril NixOS STIG: enforce IPv6 stack baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.ipv6_stack.enabled == true", message:"IPv6 stack must be configured per STIG V-268279" }], rationale:"V-268279 (IPv6 stack). SRG-OS-0504178.", evidence:[{ kind:"command", cmd:"check-ipv6-stack --verify", expect:"pass" }], createdBy:"security-team", createdAt:"3w ago", lastModified:"27d ago", framework:"DISA STIG", cisSection:"1.2" },
  { id:"stig-mock-usb-storage-27", lineageId:"stig-mock-usb-storage-27", revision:1, publicationState:"current", publishedDate:"2026-01-01", srgIds:["SRG-OS-138491"], cciIds:["CCI-008850"], name:"stig-usb-storage", category:"security", controlFamily:"AU", description:"Anduril NixOS STIG: enforce USB storage baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.usb_storage.enabled == true", message:"USB storage must be configured per STIG V-268283" }], rationale:"V-268283 (USB storage). SRG-OS-0138491.", evidence:[{ kind:"command", cmd:"check-usb-storage --verify", expect:"pass" }], createdBy:"security-team", createdAt:"4w ago", lastModified:"28d ago", framework:"DISA STIG", cisSection:"1.3" },
  { id:"stig-mock-bluetooth-radio-28", lineageId:"stig-mock-bluetooth-radio-28", revision:1, publicationState:"current", publishedDate:"2026-02-02", srgIds:["SRG-OS-655442"], cciIds:["CCI-003583"], name:"stig-bluetooth-radio", category:"security", controlFamily:"AU", description:"Anduril NixOS STIG: enforce Bluetooth radio baseline configuration.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.bluetooth_radio.enabled == true", message:"Bluetooth radio must be configured per STIG V-268284" }], rationale:"V-268284 (Bluetooth radio). SRG-OS-0655442.", evidence:[{ kind:"command", cmd:"check-bluetooth-radio --verify", expect:"pass" }], createdBy:"security-team", createdAt:"5w ago", lastModified:"29d ago", framework:"DISA STIG", cisSection:"2.1" },
  { id:"stig-mock-wireless-interface-29", lineageId:"stig-mock-wireless-interface-29", revision:1, publicationState:"current", publishedDate:"2026-03-03", srgIds:["SRG-OS-777606"], cciIds:["CCI-008978"], name:"stig-wireless-interface", category:"security", controlFamily:"AC", description:"Anduril NixOS STIG: enforce wireless interface baseline configuration.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.wireless_interface.enabled == true", message:"wireless interface must be configured per STIG V-268289" }], rationale:"V-268289 (wireless interface). SRG-OS-0777606.", evidence:[{ kind:"command", cmd:"check-wireless-interface --verify", expect:"pass" }], createdBy:"security-team", createdAt:"6w ago", lastModified:"30d ago", framework:"DISA STIG", cisSection:"2.2" },
  { id:"stig-mock-snmp-daemon-30", lineageId:"stig-mock-snmp-daemon-30", revision:1, publicationState:"current", publishedDate:"2026-04-04", srgIds:["SRG-OS-850977"], cciIds:["CCI-001086"], name:"stig-snmp-daemon", category:"security", controlFamily:"IA", description:"Anduril NixOS STIG: enforce SNMP daemon baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.snmp_daemon.enabled == true", message:"SNMP daemon must be configured per STIG V-268291" }], rationale:"V-268291 (SNMP daemon). SRG-OS-0850977.", evidence:[{ kind:"command", cmd:"check-snmp-daemon --verify", expect:"pass" }], createdBy:"security-team", createdAt:"7w ago", lastModified:"1d ago", framework:"DISA STIG", cisSection:"3.1" },
  { id:"stig-mock-nfs-export-31", lineageId:"stig-mock-nfs-export-31", revision:1, publicationState:"current", publishedDate:"2026-05-05", srgIds:["SRG-OS-237942"], cciIds:["CCI-001405"], name:"stig-nfs-export", category:"security", controlFamily:"IA", description:"Anduril NixOS STIG: enforce NFS export baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.nfs_export.enabled == true", message:"NFS export must be configured per STIG V-268293" }], rationale:"V-268293 (NFS export). SRG-OS-0237942.", evidence:[{ kind:"command", cmd:"check-nfs-export --verify", expect:"pass" }], createdBy:"security-team", createdAt:"8w ago", lastModified:"2d ago", framework:"DISA STIG", cisSection:"3.2" },
  { id:"stig-mock-samba-share-32", lineageId:"stig-mock-samba-share-32", revision:1, publicationState:"current", publishedDate:"2026-06-06", srgIds:["SRG-OS-923885"], cciIds:["CCI-006172"], name:"stig-samba-share", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce Samba share baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.samba_share.enabled == true", message:"Samba share must be configured per STIG V-268296" }], rationale:"V-268296 (Samba share). SRG-OS-0923885.", evidence:[{ kind:"command", cmd:"check-samba-share --verify", expect:"pass" }], createdBy:"security-team", createdAt:"9w ago", lastModified:"3d ago", framework:"DISA STIG", cisSection:"4.1" },
  { id:"stig-mock-cron-daemon-33", lineageId:"stig-mock-cron-daemon-33", revision:1, publicationState:"current", publishedDate:"2026-07-07", srgIds:["SRG-OS-168988"], cciIds:["CCI-001226"], name:"stig-cron-daemon", category:"security", controlFamily:"IA", description:"Anduril NixOS STIG: enforce cron daemon baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.cron_daemon.enabled == true", message:"cron daemon must be configured per STIG V-268300" }], rationale:"V-268300 (cron daemon). SRG-OS-0168988.", evidence:[{ kind:"command", cmd:"check-cron-daemon --verify", expect:"pass" }], createdBy:"security-team", createdAt:"10w ago", lastModified:"4d ago", framework:"DISA STIG", cisSection:"4.2" },
  { id:"stig-mock-mail-relay-34", lineageId:"stig-mock-mail-relay-34", revision:1, publicationState:"current", publishedDate:"2026-08-08", srgIds:["SRG-OS-359327"], cciIds:["CCI-005862"], name:"stig-mail-relay", category:"security", controlFamily:"AC", description:"Anduril NixOS STIG: enforce mail relay baseline configuration.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.mail_relay.enabled == true", message:"mail relay must be configured per STIG V-268304" }], rationale:"V-268304 (mail relay). SRG-OS-0359327.", evidence:[{ kind:"command", cmd:"check-mail-relay --verify", expect:"pass" }], createdBy:"security-team", createdAt:"11w ago", lastModified:"5d ago", framework:"DISA STIG", cisSection:"5.1" },
  { id:"stig-mock-x11-forwarding-35", lineageId:"stig-mock-x11-forwarding-35", revision:1, publicationState:"current", publishedDate:"2026-09-09", srgIds:["SRG-OS-753475"], cciIds:["CCI-008501"], name:"stig-x11-forwarding", category:"security", controlFamily:"AU", description:"Anduril NixOS STIG: enforce X11 forwarding baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.x11_forwarding.enabled == true", message:"X11 forwarding must be configured per STIG V-268307" }], rationale:"V-268307 (X11 forwarding). SRG-OS-0753475.", evidence:[{ kind:"command", cmd:"check-x11-forwarding --verify", expect:"pass" }], createdBy:"security-team", createdAt:"12w ago", lastModified:"6d ago", framework:"DISA STIG", cisSection:"5.2" },
  { id:"stig-mock-vnc-service-36", lineageId:"stig-mock-vnc-service-36", revision:1, publicationState:"current", publishedDate:"2026-01-10", srgIds:["SRG-OS-226194"], cciIds:["CCI-005288"], name:"stig-vnc-service", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce VNC service baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.vnc_service.enabled == true", message:"VNC service must be configured per STIG V-268308" }], rationale:"V-268308 (VNC service). SRG-OS-0226194.", evidence:[{ kind:"command", cmd:"check-vnc-service --verify", expect:"pass" }], createdBy:"security-team", createdAt:"1w ago", lastModified:"7d ago", framework:"DISA STIG", cisSection:"5.3" },
  { id:"stig-mock-container-image-scanning-37", lineageId:"stig-mock-container-image-scanning-37", revision:1, publicationState:"current", publishedDate:"2026-02-11", srgIds:["SRG-OS-573173"], cciIds:["CCI-002331"], name:"stig-container-image-scanning", category:"security", controlFamily:"SC", description:"Anduril NixOS STIG: enforce container image scanning baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.container_image_scanning.enabled == true", message:"container image scanning must be configured per STIG V-268311" }], rationale:"V-268311 (container image scanning). SRG-OS-0573173.", evidence:[{ kind:"command", cmd:"check-container-image-scanning --verify", expect:"pass" }], createdBy:"security-team", createdAt:"2w ago", lastModified:"8d ago", framework:"DISA STIG", cisSection:"6.1" },
  { id:"stig-mock-secrets-storage-38", lineageId:"stig-mock-secrets-storage-38", revision:1, publicationState:"current", publishedDate:"2026-03-12", srgIds:["SRG-OS-403996"], cciIds:["CCI-009594"], name:"stig-secrets-storage", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce secrets storage baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.secrets_storage.enabled == true", message:"secrets storage must be configured per STIG V-268316" }], rationale:"V-268316 (secrets storage). SRG-OS-0403996.", evidence:[{ kind:"command", cmd:"check-secrets-storage --verify", expect:"pass" }], createdBy:"security-team", createdAt:"3w ago", lastModified:"9d ago", framework:"DISA STIG", cisSection:"1.1" },
  { id:"stig-mock-key-rotation-39", lineageId:"stig-mock-key-rotation-39", revision:1, publicationState:"current", publishedDate:"2026-04-13", srgIds:["SRG-OS-608193"], cciIds:["CCI-007923"], name:"stig-key-rotation", category:"security", controlFamily:"IA", description:"Anduril NixOS STIG: enforce key rotation baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.key_rotation.enabled == true", message:"key rotation must be configured per STIG V-268318" }], rationale:"V-268318 (key rotation). SRG-OS-0608193.", evidence:[{ kind:"command", cmd:"check-key-rotation --verify", expect:"pass" }], createdBy:"security-team", createdAt:"4w ago", lastModified:"10d ago", framework:"DISA STIG", cisSection:"1.2" },
  { id:"stig-mock-certificate-validation-40", lineageId:"stig-mock-certificate-validation-40", revision:1, publicationState:"current", publishedDate:"2026-05-14", srgIds:["SRG-OS-275270"], cciIds:["CCI-007760"], name:"stig-certificate-validation", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce certificate validation baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.certificate_validation.enabled == true", message:"certificate validation must be configured per STIG V-268321" }], rationale:"V-268321 (certificate validation). SRG-OS-0275270.", evidence:[{ kind:"command", cmd:"check-certificate-validation --verify", expect:"pass" }], createdBy:"security-team", createdAt:"5w ago", lastModified:"11d ago", framework:"DISA STIG", cisSection:"1.3" },
  { id:"stig-mock-kernel-module-loading-41", lineageId:"stig-mock-kernel-module-loading-41", revision:1, publicationState:"current", publishedDate:"2026-06-15", srgIds:["SRG-OS-177591"], cciIds:["CCI-006211"], name:"stig-kernel-module-loading", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce kernel module loading baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.kernel_module_loading.enabled == true", message:"kernel module loading must be configured per STIG V-268324" }], rationale:"V-268324 (kernel module loading). SRG-OS-0177591.", evidence:[{ kind:"command", cmd:"check-kernel-module-loading --verify", expect:"pass" }], createdBy:"security-team", createdAt:"6w ago", lastModified:"12d ago", framework:"DISA STIG", cisSection:"2.1" },
  { id:"stig-mock-aslr-enforcement-42", lineageId:"stig-mock-aslr-enforcement-42", revision:1, publicationState:"current", publishedDate:"2026-07-16", srgIds:["SRG-OS-928839"], cciIds:["CCI-005743"], name:"stig-aslr-enforcement", category:"security", controlFamily:"SI", description:"Anduril NixOS STIG: enforce ASLR enforcement baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.aslr_enforcement.enabled == true", message:"ASLR enforcement must be configured per STIG V-268326" }], rationale:"V-268326 (ASLR enforcement). SRG-OS-0928839.", evidence:[{ kind:"command", cmd:"check-aslr-enforcement --verify", expect:"pass" }], createdBy:"security-team", createdAt:"7w ago", lastModified:"13d ago", framework:"DISA STIG", cisSection:"2.2" },
  { id:"stig-mock-stack-protector-43", lineageId:"stig-mock-stack-protector-43", revision:1, publicationState:"current", publishedDate:"2026-08-17", srgIds:["SRG-OS-304097"], cciIds:["CCI-002240"], name:"stig-stack-protector", category:"security", controlFamily:"SI", description:"Anduril NixOS STIG: enforce stack protector baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.stack_protector.enabled == true", message:"stack protector must be configured per STIG V-268329" }], rationale:"V-268329 (stack protector). SRG-OS-0304097.", evidence:[{ kind:"command", cmd:"check-stack-protector --verify", expect:"pass" }], createdBy:"security-team", createdAt:"8w ago", lastModified:"14d ago", framework:"DISA STIG", cisSection:"3.1" },
  { id:"stig-mock-ptrace-scope-44", lineageId:"stig-mock-ptrace-scope-44", revision:1, publicationState:"current", publishedDate:"2026-09-18", srgIds:["SRG-OS-394213"], cciIds:["CCI-001069"], name:"stig-ptrace-scope", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce ptrace scope baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.ptrace_scope.enabled == true", message:"ptrace scope must be configured per STIG V-268332" }], rationale:"V-268332 (ptrace scope). SRG-OS-0394213.", evidence:[{ kind:"command", cmd:"check-ptrace-scope --verify", expect:"pass" }], createdBy:"security-team", createdAt:"9w ago", lastModified:"15d ago", framework:"DISA STIG", cisSection:"3.2" },
  { id:"stig-mock-coredump-storage-45", lineageId:"stig-mock-coredump-storage-45", revision:1, publicationState:"current", publishedDate:"2026-01-19", srgIds:["SRG-OS-960777"], cciIds:["CCI-008374"], name:"stig-coredump-storage", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce coredump storage baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.coredump_storage.enabled == true", message:"coredump storage must be configured per STIG V-268335" }], rationale:"V-268335 (coredump storage). SRG-OS-0960777.", evidence:[{ kind:"command", cmd:"check-coredump-storage --verify", expect:"pass" }], createdBy:"security-team", createdAt:"10w ago", lastModified:"16d ago", framework:"DISA STIG", cisSection:"4.1" },
  { id:"stig-mock-swap-encryption-46", lineageId:"stig-mock-swap-encryption-46", revision:1, publicationState:"current", publishedDate:"2026-02-20", srgIds:["SRG-OS-202461"], cciIds:["CCI-004777"], name:"stig-swap-encryption", category:"security", controlFamily:"AC", description:"Anduril NixOS STIG: enforce swap encryption baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.swap_encryption.enabled == true", message:"swap encryption must be configured per STIG V-268339" }], rationale:"V-268339 (swap encryption). SRG-OS-0202461.", evidence:[{ kind:"command", cmd:"check-swap-encryption --verify", expect:"pass" }], createdBy:"security-team", createdAt:"11w ago", lastModified:"17d ago", framework:"DISA STIG", cisSection:"4.2" },
  { id:"stig-mock-tmp-mount-options-47", lineageId:"stig-mock-tmp-mount-options-47", revision:1, publicationState:"current", publishedDate:"2026-03-21", srgIds:["SRG-OS-932439"], cciIds:["CCI-002082"], name:"stig-tmp-mount-options", category:"security", controlFamily:"AU", description:"Anduril NixOS STIG: enforce tmp mount options baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.tmp_mount_options.enabled == true", message:"tmp mount options must be configured per STIG V-268341" }], rationale:"V-268341 (tmp mount options). SRG-OS-0932439.", evidence:[{ kind:"command", cmd:"check-tmp-mount-options --verify", expect:"pass" }], createdBy:"security-team", createdAt:"12w ago", lastModified:"18d ago", framework:"DISA STIG", cisSection:"5.1" },
  { id:"stig-mock-home-directory-perms-48", lineageId:"stig-mock-home-directory-perms-48", revision:1, publicationState:"current", publishedDate:"2026-04-22", srgIds:["SRG-OS-843859"], cciIds:["CCI-007354"], name:"stig-home-directory-perms", category:"security", controlFamily:"SI", description:"Anduril NixOS STIG: enforce home directory perms baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.home_directory_perms.enabled == true", message:"home directory perms must be configured per STIG V-268344" }], rationale:"V-268344 (home directory perms). SRG-OS-0843859.", evidence:[{ kind:"command", cmd:"check-home-directory-perms --verify", expect:"pass" }], createdBy:"security-team", createdAt:"1w ago", lastModified:"19d ago", framework:"DISA STIG", cisSection:"5.2" },
  { id:"stig-mock-shell-history-49", lineageId:"stig-mock-shell-history-49", revision:1, publicationState:"current", publishedDate:"2026-05-23", srgIds:["SRG-OS-699601"], cciIds:["CCI-005791"], name:"stig-shell-history", category:"security", controlFamily:"SI", description:"Anduril NixOS STIG: enforce shell history baseline configuration.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.shell_history.enabled == true", message:"shell history must be configured per STIG V-268349" }], rationale:"V-268349 (shell history). SRG-OS-0699601.", evidence:[{ kind:"command", cmd:"check-shell-history --verify", expect:"pass" }], createdBy:"security-team", createdAt:"2w ago", lastModified:"20d ago", framework:"DISA STIG", cisSection:"5.3" },
  { id:"stig-mock-login-banner-50", lineageId:"stig-mock-login-banner-50", revision:1, publicationState:"current", publishedDate:"2026-06-24", srgIds:["SRG-OS-367629"], cciIds:["CCI-008036"], name:"stig-login-banner", category:"security", controlFamily:"MP", description:"Anduril NixOS STIG: enforce login banner baseline configuration.", type:"custom", severity:"medium", enabled:false, rules:[{ kind:"custom_eval", expr:"config.mock.login_banner.enabled == true", message:"login banner must be configured per STIG V-268351" }], rationale:"V-268351 (login banner). SRG-OS-0367629.", evidence:[{ kind:"command", cmd:"check-login-banner --verify", expect:"pass" }], createdBy:"security-team", createdAt:"3w ago", lastModified:"21d ago", framework:"DISA STIG" },
  { id:"stig-mock-motd-content-51", lineageId:"stig-mock-motd-content-51", revision:1, publicationState:"current", publishedDate:"2026-07-25", srgIds:["SRG-OS-427827"], cciIds:["CCI-002120"], name:"stig-motd-content", category:"security", controlFamily:"AC", description:"Anduril NixOS STIG: enforce MOTD content baseline configuration.", type:"custom", severity:"high", enabled:false, rules:[{ kind:"custom_eval", expr:"config.mock.motd_content.enabled == true", message:"MOTD content must be configured per STIG V-268354" }], rationale:"V-268354 (MOTD content). SRG-OS-0427827.", evidence:[{ kind:"command", cmd:"check-motd-content --verify", expect:"pass" }], createdBy:"security-team", createdAt:"4w ago", lastModified:"22d ago", framework:"DISA STIG" },
  { id:"stig-mock-idle-session-lock-52", lineageId:"stig-mock-idle-session-lock-52", revision:1, publicationState:"current", publishedDate:"2026-08-26", srgIds:["SRG-OS-244075"], cciIds:["CCI-008635"], name:"stig-idle-session-lock", category:"security", controlFamily:null, description:"Anduril NixOS STIG: enforce idle session lock baseline configuration.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.idle_session_lock.enabled == true", message:"idle session lock must be configured per STIG V-268356" }], rationale:"V-268356 (idle session lock). SRG-OS-0244075.", evidence:[{ kind:"command", cmd:"check-idle-session-lock --verify", expect:"pass" }], createdBy:"security-team", createdAt:"5w ago", lastModified:"23d ago", framework:"DISA STIG" },
  { id:"stig-mock-screen-lock-53", lineageId:"stig-mock-screen-lock-53", revision:1, publicationState:"current", publishedDate:"2026-09-27", srgIds:["SRG-OS-220245"], cciIds:["CCI-007461"], name:"stig-screen-lock", category:"security", controlFamily:"AU", description:"Anduril NixOS STIG: enforce screen lock baseline configuration.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.mock.screen_lock.enabled == true", message:"screen lock must be configured per STIG V-268361" }], rationale:"V-268361 (screen lock). SRG-OS-0220245.", evidence:[{ kind:"command", cmd:"check-screen-lock --verify", expect:"pass" }], createdBy:"security-team", createdAt:"6w ago", lastModified:"24d ago", framework:"DISA STIG" },
  { id:"stig-mock-ssh-daemon-54", name:"stig-ssh-daemon", category:"security", description:"Anduril NixOS STIG: SSH daemon configuration hardened per DISA baseline (key exchange, MACs, ciphers restricted to FIPS-approved sets).", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.ssh_daemon.enable", op:"==", value:"true" }], rationale:"SRG-OS-680054 — SSH daemon configuration hardened per DISA baseline (key exchange, MACs, ciphers restricted to FIPS-approved sets).", evidence:[{ kind:"command", cmd:"systemctl show ssh_daemon 2>/dev/null || nixos-option services.ssh_daemon.enable", expect:"true" }], createdBy:"security-team", createdAt:"12w ago", lastModified:"3w ago", lineageId:"stig-mock-ssh-daemon-54", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680054"], cciIds:["CCI-900054"], controlFamily:"AC", framework:"DISA STIG" },
  { id:"stig-mock-firewall-rules-55", name:"stig-firewall-rules", category:"security", description:"Anduril NixOS STIG: Host firewall enforces default-deny inbound with explicit allow rules for required services.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.firewall_rules.enable", op:"==", value:"true" }], rationale:"SRG-OS-680055 — Host firewall enforces default-deny inbound with explicit allow rules for required services.", evidence:[{ kind:"command", cmd:"systemctl show firewall_rules 2>/dev/null || nixos-option services.firewall_rules.enable", expect:"true" }], createdBy:"security-team", createdAt:"13w ago", lastModified:"4w ago", lineageId:"stig-mock-firewall-rules-55", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680055"], cciIds:["CCI-900055"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-audit-logging-56", name:"stig-audit-logging", category:"security", description:"Anduril NixOS STIG: Audit subsystem captures privileged command execution, auth events, and file-access denials.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.audit_logging.enable", op:"==", value:"true" }], rationale:"SRG-OS-680056 — Audit subsystem captures privileged command execution, auth events, and file-access denials.", evidence:[{ kind:"command", cmd:"systemctl show audit_logging 2>/dev/null || nixos-option services.audit_logging.enable", expect:"true" }], createdBy:"security-team", createdAt:"14w ago", lastModified:"1w ago", lineageId:"stig-mock-audit-logging-56", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680056"], cciIds:["CCI-900056"], controlFamily:"AU", framework:"DISA STIG" },
  { id:"stig-mock-account-lockout-57", name:"stig-account-lockout", category:"security", description:"Anduril NixOS STIG: Account lockout enforced after repeated failed authentication attempts.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.account_lockout.enable", op:"==", value:"true" }], rationale:"SRG-OS-680057 — Account lockout enforced after repeated failed authentication attempts.", evidence:[{ kind:"command", cmd:"systemctl show account_lockout 2>/dev/null || nixos-option services.account_lockout.enable", expect:"true" }], createdBy:"security-team", createdAt:"15w ago", lastModified:"2w ago", lineageId:"stig-mock-account-lockout-57", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680057"], cciIds:["CCI-900057"], controlFamily:"IA", framework:"DISA STIG" },
  { id:"stig-mock-password-complexity-58", name:"stig-password-complexity", category:"security", description:"Anduril NixOS STIG: Password complexity policy requires mixed case, digits, and special characters.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.password_complexity.enable", op:"==", value:"true" }], rationale:"SRG-OS-680058 — Password complexity policy requires mixed case, digits, and special characters.", evidence:[{ kind:"command", cmd:"systemctl show password_complexity 2>/dev/null || nixos-option services.password_complexity.enable", expect:"true" }], createdBy:"security-team", createdAt:"16w ago", lastModified:"3w ago", lineageId:"stig-mock-password-complexity-58", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680058"], cciIds:["CCI-900058"], controlFamily:"IA", framework:"DISA STIG" },
  { id:"stig-mock-kernel-hardening-59", name:"stig-kernel-hardening", category:"security", description:"Anduril NixOS STIG: Kernel sysctl parameters hardened against common memory-corruption and network attack classes.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.kernel_hardening.enable", op:"==", value:"true" }], rationale:"SRG-OS-680059 — Kernel sysctl parameters hardened against common memory-corruption and network attack classes.", evidence:[{ kind:"command", cmd:"systemctl show kernel_hardening 2>/dev/null || nixos-option services.kernel_hardening.enable", expect:"true" }], createdBy:"security-team", createdAt:"17w ago", lastModified:"4w ago", lineageId:"stig-mock-kernel-hardening-59", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680059"], cciIds:["CCI-900059"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-filesystem-permissions-60", name:"stig-filesystem-permissions", category:"security", description:"Anduril NixOS STIG: World-writable and unowned files are disallowed; sensitive paths use restrictive permissions.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.filesystem_permissions.enable", op:"==", value:"true" }], rationale:"SRG-OS-680060 — World-writable and unowned files are disallowed; sensitive paths use restrictive permissions.", evidence:[{ kind:"command", cmd:"systemctl show filesystem_permissions 2>/dev/null || nixos-option services.filesystem_permissions.enable", expect:"true" }], createdBy:"security-team", createdAt:"8w ago", lastModified:"1w ago", lineageId:"stig-mock-filesystem-permissions-60", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680060"], cciIds:["CCI-900060"], controlFamily:"CM", framework:"DISA STIG" },
  { id:"stig-mock-usb-device-control-61", name:"stig-usb-device-control", category:"security", description:"Anduril NixOS STIG: USB mass-storage and peripheral devices are blocked unless explicitly allow-listed.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.usb_device_control.enable", op:"==", value:"true" }], rationale:"SRG-OS-680061 — USB mass-storage and peripheral devices are blocked unless explicitly allow-listed.", evidence:[{ kind:"command", cmd:"systemctl show usb_device_control 2>/dev/null || nixos-option services.usb_device_control.enable", expect:"true" }], createdBy:"security-team", createdAt:"9w ago", lastModified:"2w ago", lineageId:"stig-mock-usb-device-control-61", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680061"], cciIds:["CCI-900061"], controlFamily:"MP", framework:"DISA STIG" },
  { id:"stig-mock-tls-cipher-suite-62", name:"stig-tls-cipher-suite", category:"security", description:"Anduril NixOS STIG: TLS services restricted to FIPS-validated cipher suites and TLS 1.2+.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.tls_cipher_suite.enable", op:"==", value:"true" }], rationale:"SRG-OS-680062 — TLS services restricted to FIPS-validated cipher suites and TLS 1.2+.", evidence:[{ kind:"command", cmd:"systemctl show tls_cipher_suite 2>/dev/null || nixos-option services.tls_cipher_suite.enable", expect:"true" }], createdBy:"security-team", createdAt:"10w ago", lastModified:"3w ago", lineageId:"stig-mock-tls-cipher-suite-62", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680062"], cciIds:["CCI-900062"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-dns-resolver-63", name:"stig-dns-resolver", category:"security", description:"Anduril NixOS STIG: DNS resolution is pinned to approved resolvers with DNSSEC validation enabled.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.dns_resolver.enable", op:"==", value:"true" }], rationale:"SRG-OS-680063 — DNS resolution is pinned to approved resolvers with DNSSEC validation enabled.", evidence:[{ kind:"command", cmd:"systemctl show dns_resolver 2>/dev/null || nixos-option services.dns_resolver.enable", expect:"true" }], createdBy:"security-team", createdAt:"11w ago", lastModified:"4w ago", lineageId:"stig-mock-dns-resolver-63", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680063"], cciIds:["CCI-900063"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-ntp-sync-64", name:"stig-ntp-sync", category:"security", description:"Anduril NixOS STIG: System clock is synchronized to an approved time source for reliable audit timestamps.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.ntp_sync.enable", op:"==", value:"true" }], rationale:"SRG-OS-680064 — System clock is synchronized to an approved time source for reliable audit timestamps.", evidence:[{ kind:"command", cmd:"systemctl show ntp_sync 2>/dev/null || nixos-option services.ntp_sync.enable", expect:"true" }], createdBy:"security-team", createdAt:"12w ago", lastModified:"1w ago", lineageId:"stig-mock-ntp-sync-64", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680064"], cciIds:["CCI-900064"], controlFamily:"AU", framework:"DISA STIG" },
  { id:"stig-mock-syslog-forwarding-65", name:"stig-syslog-forwarding", category:"security", description:"Anduril NixOS STIG: Audit and system logs are forwarded to a central log collector over an encrypted channel.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.syslog_forwarding.enable", op:"==", value:"true" }], rationale:"SRG-OS-680065 — Audit and system logs are forwarded to a central log collector over an encrypted channel.", evidence:[{ kind:"command", cmd:"systemctl show syslog_forwarding 2>/dev/null || nixos-option services.syslog_forwarding.enable", expect:"true" }], createdBy:"security-team", createdAt:"13w ago", lastModified:"2w ago", lineageId:"stig-mock-syslog-forwarding-65", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680065"], cciIds:["CCI-900065"], controlFamily:"AU", framework:"DISA STIG" },
  { id:"stig-mock-sudo-policy-66", name:"stig-sudo-policy", category:"security", description:"Anduril NixOS STIG: Sudo access is restricted to named users/groups with command logging enabled.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.sudo_policy.enable", op:"==", value:"true" }], rationale:"SRG-OS-680066 — Sudo access is restricted to named users/groups with command logging enabled.", evidence:[{ kind:"command", cmd:"systemctl show sudo_policy 2>/dev/null || nixos-option services.sudo_policy.enable", expect:"true" }], createdBy:"security-team", createdAt:"14w ago", lastModified:"3w ago", lineageId:"stig-mock-sudo-policy-66", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680066"], cciIds:["CCI-900066"], controlFamily:"AC", framework:"DISA STIG" },
  { id:"stig-mock-pam-stack-67", name:"stig-pam-stack", category:"security", description:"Anduril NixOS STIG: PAM stack enforces password quality, lockout, and session controls consistently across services.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.pam_stack.enable", op:"==", value:"true" }], rationale:"SRG-OS-680067 — PAM stack enforces password quality, lockout, and session controls consistently across services.", evidence:[{ kind:"command", cmd:"systemctl show pam_stack 2>/dev/null || nixos-option services.pam_stack.enable", expect:"true" }], createdBy:"security-team", createdAt:"15w ago", lastModified:"4w ago", lineageId:"stig-mock-pam-stack-67", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680067"], cciIds:["CCI-900067"], controlFamily:"IA", framework:"DISA STIG" },
  { id:"stig-mock-selinux-apparmor-profile-68", name:"stig-selinux-apparmor-profile", category:"security", description:"Anduril NixOS STIG: Mandatory access control profile is enforcing (not permissive) for system services.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.selinux_apparmor_profile.enable", op:"==", value:"true" }], rationale:"SRG-OS-680068 — Mandatory access control profile is enforcing (not permissive) for system services.", evidence:[{ kind:"command", cmd:"systemctl show selinux_apparmor_profile 2>/dev/null || nixos-option services.selinux_apparmor_profile.enable", expect:"true" }], createdBy:"security-team", createdAt:"16w ago", lastModified:"1w ago", lineageId:"stig-mock-selinux-apparmor-profile-68", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680068"], cciIds:["CCI-900068"], controlFamily:"CM", framework:"DISA STIG" },
  { id:"stig-mock-boot-loader-integrity-69", name:"stig-boot-loader-integrity", category:"security", description:"Anduril NixOS STIG: Boot loader requires a password for interactive edits and verifies kernel signatures.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.boot_loader_integrity.enable", op:"==", value:"true" }], rationale:"SRG-OS-680069 — Boot loader requires a password for interactive edits and verifies kernel signatures.", evidence:[{ kind:"command", cmd:"systemctl show boot_loader_integrity 2>/dev/null || nixos-option services.boot_loader_integrity.enable", expect:"true" }], createdBy:"security-team", createdAt:"17w ago", lastModified:"2w ago", lineageId:"stig-mock-boot-loader-integrity-69", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680069"], cciIds:["CCI-900069"], controlFamily:"CM", framework:"DISA STIG" },
  { id:"stig-mock-disk-encryption-70", name:"stig-disk-encryption", category:"security", description:"Anduril NixOS STIG: Data-at-rest is protected with full-disk encryption using an approved cipher.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.disk_encryption.enable", op:"==", value:"true" }], rationale:"SRG-OS-680070 — Data-at-rest is protected with full-disk encryption using an approved cipher.", evidence:[{ kind:"command", cmd:"systemctl show disk_encryption 2>/dev/null || nixos-option services.disk_encryption.enable", expect:"true" }], createdBy:"security-team", createdAt:"8w ago", lastModified:"3w ago", lineageId:"stig-mock-disk-encryption-70", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680070"], cciIds:["CCI-900070"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-service-isolation-71", name:"stig-service-isolation", category:"security", description:"Anduril NixOS STIG: System services run under dedicated unprivileged accounts with restricted capabilities.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.service_isolation.enable", op:"==", value:"true" }], rationale:"SRG-OS-680071 — System services run under dedicated unprivileged accounts with restricted capabilities.", evidence:[{ kind:"command", cmd:"systemctl show service_isolation 2>/dev/null || nixos-option services.service_isolation.enable", expect:"true" }], createdBy:"security-team", createdAt:"9w ago", lastModified:"4w ago", lineageId:"stig-mock-service-isolation-71", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680071"], cciIds:["CCI-900071"], controlFamily:"CM", framework:"DISA STIG" },
  { id:"stig-mock-network-segmentation-72", name:"stig-network-segmentation", category:"security", description:"Anduril NixOS STIG: Host network interfaces are segmented per zone; inter-zone routing is explicitly denied by default.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.network_segmentation.enable", op:"==", value:"true" }], rationale:"SRG-OS-680072 — Host network interfaces are segmented per zone; inter-zone routing is explicitly denied by default.", evidence:[{ kind:"command", cmd:"systemctl show network_segmentation 2>/dev/null || nixos-option services.network_segmentation.enable", expect:"true" }], createdBy:"security-team", createdAt:"10w ago", lastModified:"1w ago", lineageId:"stig-mock-network-segmentation-72", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680072"], cciIds:["CCI-900072"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-container-runtime-73", name:"stig-container-runtime", category:"security", description:"Anduril NixOS STIG: Container runtime is configured to drop unnecessary capabilities and run rootless where supported.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.container_runtime.enable", op:"==", value:"true" }], rationale:"SRG-OS-680073 — Container runtime is configured to drop unnecessary capabilities and run rootless where supported.", evidence:[{ kind:"command", cmd:"systemctl show container_runtime 2>/dev/null || nixos-option services.container_runtime.enable", expect:"true" }], createdBy:"security-team", createdAt:"11w ago", lastModified:"2w ago", lineageId:"stig-mock-container-runtime-73", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680073"], cciIds:["CCI-900073"], controlFamily:"CM", framework:"DISA STIG" },
  { id:"stig-mock-package-signing-74", name:"stig-package-signing", category:"security", description:"Anduril NixOS STIG: Package manager only installs packages signed by a trusted, pinned key set.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.package_signing.enable", op:"==", value:"true" }], rationale:"SRG-OS-680074 — Package manager only installs packages signed by a trusted, pinned key set.", evidence:[{ kind:"command", cmd:"systemctl show package_signing 2>/dev/null || nixos-option services.package_signing.enable", expect:"true" }], createdBy:"security-team", createdAt:"12w ago", lastModified:"3w ago", lineageId:"stig-mock-package-signing-74", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680074"], cciIds:["CCI-900074"], controlFamily:"CM", framework:"DISA STIG" },
  { id:"stig-mock-update-cadence-75", name:"stig-update-cadence", category:"security", description:"Anduril NixOS STIG: Security updates are applied within the required patch window and tracked.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.update_cadence.enable", op:"==", value:"true" }], rationale:"SRG-OS-680075 — Security updates are applied within the required patch window and tracked.", evidence:[{ kind:"command", cmd:"systemctl show update_cadence 2>/dev/null || nixos-option services.update_cadence.enable", expect:"true" }], createdBy:"security-team", createdAt:"13w ago", lastModified:"4w ago", lineageId:"stig-mock-update-cadence-75", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680075"], cciIds:["CCI-900075"], controlFamily:"CM", framework:"DISA STIG" },
  { id:"stig-mock-session-timeout-76", name:"stig-session-timeout", category:"security", description:"Anduril NixOS STIG: Interactive sessions are terminated automatically after a defined idle period.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.session_timeout.enable", op:"==", value:"true" }], rationale:"SRG-OS-680076 — Interactive sessions are terminated automatically after a defined idle period.", evidence:[{ kind:"command", cmd:"systemctl show session_timeout 2>/dev/null || nixos-option services.session_timeout.enable", expect:"true" }], createdBy:"security-team", createdAt:"14w ago", lastModified:"1w ago", lineageId:"stig-mock-session-timeout-76", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680076"], cciIds:["CCI-900076"], controlFamily:"AC", framework:"DISA STIG" },
  { id:"stig-mock-banner-text-77", name:"stig-banner-text", category:"security", description:"Anduril NixOS STIG: Login banners display the required consent-to-monitoring notice before authentication.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.banner_text.enable", op:"==", value:"true" }], rationale:"SRG-OS-680077 — Login banners display the required consent-to-monitoring notice before authentication.", evidence:[{ kind:"command", cmd:"systemctl show banner_text 2>/dev/null || nixos-option services.banner_text.enable", expect:"true" }], createdBy:"security-team", createdAt:"15w ago", lastModified:"2w ago", lineageId:"stig-mock-banner-text-77", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680077"], cciIds:["CCI-900077"], controlFamily:"AC", framework:"DISA STIG" },
  { id:"stig-mock-log-rotation-78", name:"stig-log-rotation", category:"security", description:"Anduril NixOS STIG: Audit logs are rotated and retained to prevent loss of accountability data from disk exhaustion.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.log_rotation.enable", op:"==", value:"true" }], rationale:"SRG-OS-680078 — Audit logs are rotated and retained to prevent loss of accountability data from disk exhaustion.", evidence:[{ kind:"command", cmd:"systemctl show log_rotation 2>/dev/null || nixos-option services.log_rotation.enable", expect:"true" }], createdBy:"security-team", createdAt:"16w ago", lastModified:"3w ago", lineageId:"stig-mock-log-rotation-78", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680078"], cciIds:["CCI-900078"], controlFamily:"AU", framework:"DISA STIG" },
  { id:"stig-mock-core-dump-handling-79", name:"stig-core-dump-handling", category:"security", description:"Anduril NixOS STIG: Core dumps are disabled or restricted to prevent leakage of sensitive process memory.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.core_dump_handling.enable", op:"==", value:"true" }], rationale:"SRG-OS-680079 — Core dumps are disabled or restricted to prevent leakage of sensitive process memory.", evidence:[{ kind:"command", cmd:"systemctl show core_dump_handling 2>/dev/null || nixos-option services.core_dump_handling.enable", expect:"true" }], createdBy:"security-team", createdAt:"17w ago", lastModified:"4w ago", lineageId:"stig-mock-core-dump-handling-79", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680079"], cciIds:["CCI-900079"], controlFamily:"SI", framework:"DISA STIG" },
  { id:"stig-mock-ipv6-stack-80", name:"stig-ipv6-stack", category:"security", description:"Anduril NixOS STIG: Unused IPv6 stack is disabled where not required, reducing network attack surface.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.ipv6_stack.enable", op:"==", value:"true" }], rationale:"SRG-OS-680080 — Unused IPv6 stack is disabled where not required, reducing network attack surface.", evidence:[{ kind:"command", cmd:"systemctl show ipv6_stack 2>/dev/null || nixos-option services.ipv6_stack.enable", expect:"true" }], createdBy:"security-team", createdAt:"8w ago", lastModified:"1w ago", lineageId:"stig-mock-ipv6-stack-80", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680080"], cciIds:["CCI-900080"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-usb-storage-81", name:"stig-usb-storage", category:"security", description:"Anduril NixOS STIG: USB mass storage class drivers are blocked at the kernel level.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.usb_storage.enable", op:"==", value:"true" }], rationale:"SRG-OS-680081 — USB mass storage class drivers are blocked at the kernel level.", evidence:[{ kind:"command", cmd:"systemctl show usb_storage 2>/dev/null || nixos-option services.usb_storage.enable", expect:"true" }], createdBy:"security-team", createdAt:"9w ago", lastModified:"2w ago", lineageId:"stig-mock-usb-storage-81", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680081"], cciIds:["CCI-900081"], controlFamily:"MP", framework:"DISA STIG" },
  { id:"stig-mock-bluetooth-radio-82", name:"stig-bluetooth-radio", category:"security", description:"Anduril NixOS STIG: Bluetooth radio is disabled on systems with no approved use case.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.bluetooth_radio.enable", op:"==", value:"true" }], rationale:"SRG-OS-680082 — Bluetooth radio is disabled on systems with no approved use case.", evidence:[{ kind:"command", cmd:"systemctl show bluetooth_radio 2>/dev/null || nixos-option services.bluetooth_radio.enable", expect:"true" }], createdBy:"security-team", createdAt:"10w ago", lastModified:"3w ago", lineageId:"stig-mock-bluetooth-radio-82", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680082"], cciIds:["CCI-900082"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-wireless-interface-83", name:"stig-wireless-interface", category:"security", description:"Anduril NixOS STIG: Wireless network interfaces are disabled unless explicitly required and approved.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.wireless_interface.enable", op:"==", value:"true" }], rationale:"SRG-OS-680083 — Wireless network interfaces are disabled unless explicitly required and approved.", evidence:[{ kind:"command", cmd:"systemctl show wireless_interface 2>/dev/null || nixos-option services.wireless_interface.enable", expect:"true" }], createdBy:"security-team", createdAt:"11w ago", lastModified:"4w ago", lineageId:"stig-mock-wireless-interface-83", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680083"], cciIds:["CCI-900083"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-snmp-daemon-84", name:"stig-snmp-daemon", category:"security", description:"Anduril NixOS STIG: SNMP service is disabled, or restricted to v3 with authentication and encryption.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.snmp_daemon.enable", op:"==", value:"true" }], rationale:"SRG-OS-680084 — SNMP service is disabled, or restricted to v3 with authentication and encryption.", evidence:[{ kind:"command", cmd:"systemctl show snmp_daemon 2>/dev/null || nixos-option services.snmp_daemon.enable", expect:"true" }], createdBy:"security-team", createdAt:"12w ago", lastModified:"1w ago", lineageId:"stig-mock-snmp-daemon-84", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680084"], cciIds:["CCI-900084"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-nfs-export-85", name:"stig-nfs-export", category:"security", description:"Anduril NixOS STIG: NFS exports restrict access to approved subnets and disallow root squash bypass.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.nfs_export.enable", op:"==", value:"true" }], rationale:"SRG-OS-680085 — NFS exports restrict access to approved subnets and disallow root squash bypass.", evidence:[{ kind:"command", cmd:"systemctl show nfs_export 2>/dev/null || nixos-option services.nfs_export.enable", expect:"true" }], createdBy:"security-team", createdAt:"13w ago", lastModified:"2w ago", lineageId:"stig-mock-nfs-export-85", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680085"], cciIds:["CCI-900085"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-samba-share-86", name:"stig-samba-share", category:"security", description:"Anduril NixOS STIG: SMB/Samba shares require authentication and disallow guest access.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.samba_share.enable", op:"==", value:"true" }], rationale:"SRG-OS-680086 — SMB/Samba shares require authentication and disallow guest access.", evidence:[{ kind:"command", cmd:"systemctl show samba_share 2>/dev/null || nixos-option services.samba_share.enable", expect:"true" }], createdBy:"security-team", createdAt:"14w ago", lastModified:"3w ago", lineageId:"stig-mock-samba-share-86", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680086"], cciIds:["CCI-900086"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-cron-daemon-87", name:"stig-cron-daemon", category:"security", description:"Anduril NixOS STIG: Cron job definitions are restricted to authorized users and reviewed for integrity.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.cron_daemon.enable", op:"==", value:"true" }], rationale:"SRG-OS-680087 — Cron job definitions are restricted to authorized users and reviewed for integrity.", evidence:[{ kind:"command", cmd:"systemctl show cron_daemon 2>/dev/null || nixos-option services.cron_daemon.enable", expect:"true" }], createdBy:"security-team", createdAt:"15w ago", lastModified:"4w ago", lineageId:"stig-mock-cron-daemon-87", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680087"], cciIds:["CCI-900087"], controlFamily:"CM", framework:"DISA STIG" },
  { id:"stig-mock-mail-relay-88", name:"stig-mail-relay", category:"security", description:"Anduril NixOS STIG: Local mail transfer agent does not relay mail for untrusted networks.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.mail_relay.enable", op:"==", value:"true" }], rationale:"SRG-OS-680088 — Local mail transfer agent does not relay mail for untrusted networks.", evidence:[{ kind:"command", cmd:"systemctl show mail_relay 2>/dev/null || nixos-option services.mail_relay.enable", expect:"true" }], createdBy:"security-team", createdAt:"16w ago", lastModified:"1w ago", lineageId:"stig-mock-mail-relay-88", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680088"], cciIds:["CCI-900088"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-x11-forwarding-89", name:"stig-x11-forwarding", category:"security", description:"Anduril NixOS STIG: X11 forwarding over SSH is disabled unless explicitly required.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.x11_forwarding.enable", op:"==", value:"true" }], rationale:"SRG-OS-680089 — X11 forwarding over SSH is disabled unless explicitly required.", evidence:[{ kind:"command", cmd:"systemctl show x11_forwarding 2>/dev/null || nixos-option services.x11_forwarding.enable", expect:"true" }], createdBy:"security-team", createdAt:"17w ago", lastModified:"2w ago", lineageId:"stig-mock-x11-forwarding-89", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680089"], cciIds:["CCI-900089"], controlFamily:"AC", framework:"DISA STIG" },
  { id:"stig-mock-vnc-service-90", name:"stig-vnc-service", category:"security", description:"Anduril NixOS STIG: VNC remote-desktop service is disabled or tunneled over an authenticated, encrypted channel.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.vnc_service.enable", op:"==", value:"true" }], rationale:"SRG-OS-680090 — VNC remote-desktop service is disabled or tunneled over an authenticated, encrypted channel.", evidence:[{ kind:"command", cmd:"systemctl show vnc_service 2>/dev/null || nixos-option services.vnc_service.enable", expect:"true" }], createdBy:"security-team", createdAt:"8w ago", lastModified:"3w ago", lineageId:"stig-mock-vnc-service-90", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680090"], cciIds:["CCI-900090"], controlFamily:"AC", framework:"DISA STIG" },
  { id:"stig-mock-container-image-scanning-91", name:"stig-container-image-scanning", category:"security", description:"Anduril NixOS STIG: Container images are scanned for known vulnerabilities before deployment.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.container_image_scanning.enable", op:"==", value:"true" }], rationale:"SRG-OS-680091 — Container images are scanned for known vulnerabilities before deployment.", evidence:[{ kind:"command", cmd:"systemctl show container_image_scanning 2>/dev/null || nixos-option services.container_image_scanning.enable", expect:"true" }], createdBy:"security-team", createdAt:"9w ago", lastModified:"4w ago", lineageId:"stig-mock-container-image-scanning-91", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680091"], cciIds:["CCI-900091"], controlFamily:"SI", framework:"DISA STIG" },
  { id:"stig-mock-secrets-storage-92", name:"stig-secrets-storage", category:"security", description:"Anduril NixOS STIG: Application secrets are stored in an encrypted secrets manager, not plaintext config.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.secrets_storage.enable", op:"==", value:"true" }], rationale:"SRG-OS-680092 — Application secrets are stored in an encrypted secrets manager, not plaintext config.", evidence:[{ kind:"command", cmd:"systemctl show secrets_storage 2>/dev/null || nixos-option services.secrets_storage.enable", expect:"true" }], createdBy:"security-team", createdAt:"10w ago", lastModified:"1w ago", lineageId:"stig-mock-secrets-storage-92", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680092"], cciIds:["CCI-900092"], controlFamily:"IA", framework:"DISA STIG" },
  { id:"stig-mock-key-rotation-93", name:"stig-key-rotation", category:"security", description:"Anduril NixOS STIG: Cryptographic keys are rotated on a defined schedule and revoked keys are removed from trust stores.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.key_rotation.enable", op:"==", value:"true" }], rationale:"SRG-OS-680093 — Cryptographic keys are rotated on a defined schedule and revoked keys are removed from trust stores.", evidence:[{ kind:"command", cmd:"systemctl show key_rotation 2>/dev/null || nixos-option services.key_rotation.enable", expect:"true" }], createdBy:"security-team", createdAt:"11w ago", lastModified:"2w ago", lineageId:"stig-mock-key-rotation-93", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680093"], cciIds:["CCI-900093"], controlFamily:"IA", framework:"DISA STIG" },
  { id:"stig-mock-certificate-validation-94", name:"stig-certificate-validation", category:"security", description:"Anduril NixOS STIG: TLS clients validate certificate chains and reject expired or self-signed certificates in production.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.certificate_validation.enable", op:"==", value:"true" }], rationale:"SRG-OS-680094 — TLS clients validate certificate chains and reject expired or self-signed certificates in production.", evidence:[{ kind:"command", cmd:"systemctl show certificate_validation 2>/dev/null || nixos-option services.certificate_validation.enable", expect:"true" }], createdBy:"security-team", createdAt:"12w ago", lastModified:"3w ago", lineageId:"stig-mock-certificate-validation-94", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680094"], cciIds:["CCI-900094"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-kernel-module-loading-95", name:"stig-kernel-module-loading", category:"security", description:"Anduril NixOS STIG: Loading of unused or unsigned kernel modules is disabled.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.kernel_module_loading.enable", op:"==", value:"true" }], rationale:"SRG-OS-680095 — Loading of unused or unsigned kernel modules is disabled.", evidence:[{ kind:"command", cmd:"systemctl show kernel_module_loading 2>/dev/null || nixos-option services.kernel_module_loading.enable", expect:"true" }], createdBy:"security-team", createdAt:"13w ago", lastModified:"4w ago", lineageId:"stig-mock-kernel-module-loading-95", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680095"], cciIds:["CCI-900095"], controlFamily:"CM", framework:"DISA STIG" },
  { id:"stig-mock-aslr-enforcement-96", name:"stig-aslr-enforcement", category:"security", description:"Anduril NixOS STIG: Address space layout randomization is enabled fleet-wide to mitigate memory-corruption exploits.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.aslr_enforcement.enable", op:"==", value:"true" }], rationale:"SRG-OS-680096 — Address space layout randomization is enabled fleet-wide to mitigate memory-corruption exploits.", evidence:[{ kind:"command", cmd:"systemctl show aslr_enforcement 2>/dev/null || nixos-option services.aslr_enforcement.enable", expect:"true" }], createdBy:"security-team", createdAt:"14w ago", lastModified:"1w ago", lineageId:"stig-mock-aslr-enforcement-96", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680096"], cciIds:["CCI-900096"], controlFamily:"SI", framework:"DISA STIG" },
  { id:"stig-mock-stack-protector-97", name:"stig-stack-protector", category:"security", description:"Anduril NixOS STIG: Binaries are compiled with stack-protector and related exploit-mitigation flags.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.stack_protector.enable", op:"==", value:"true" }], rationale:"SRG-OS-680097 — Binaries are compiled with stack-protector and related exploit-mitigation flags.", evidence:[{ kind:"command", cmd:"systemctl show stack_protector 2>/dev/null || nixos-option services.stack_protector.enable", expect:"true" }], createdBy:"security-team", createdAt:"15w ago", lastModified:"2w ago", lineageId:"stig-mock-stack-protector-97", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680097"], cciIds:["CCI-900097"], controlFamily:"SI", framework:"DISA STIG" },
  { id:"stig-mock-ptrace-scope-98", name:"stig-ptrace-scope", category:"security", description:"Anduril NixOS STIG: Kernel ptrace scope is restricted to prevent unprivileged process inspection.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.ptrace_scope.enable", op:"==", value:"true" }], rationale:"SRG-OS-680098 — Kernel ptrace scope is restricted to prevent unprivileged process inspection.", evidence:[{ kind:"command", cmd:"systemctl show ptrace_scope 2>/dev/null || nixos-option services.ptrace_scope.enable", expect:"true" }], createdBy:"security-team", createdAt:"16w ago", lastModified:"3w ago", lineageId:"stig-mock-ptrace-scope-98", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680098"], cciIds:["CCI-900098"], controlFamily:"SI", framework:"DISA STIG" },
  { id:"stig-mock-coredump-storage-99", name:"stig-coredump-storage", category:"security", description:"Anduril NixOS STIG: Core dump storage location is restricted and cleared on a schedule.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.coredump_storage.enable", op:"==", value:"true" }], rationale:"SRG-OS-680099 — Core dump storage location is restricted and cleared on a schedule.", evidence:[{ kind:"command", cmd:"systemctl show coredump_storage 2>/dev/null || nixos-option services.coredump_storage.enable", expect:"true" }], createdBy:"security-team", createdAt:"17w ago", lastModified:"4w ago", lineageId:"stig-mock-coredump-storage-99", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680099"], cciIds:["CCI-900099"], controlFamily:"SI", framework:"DISA STIG" },
  { id:"stig-mock-swap-encryption-100", name:"stig-swap-encryption", category:"security", description:"Anduril NixOS STIG: Swap space is encrypted to prevent recovery of sensitive data written to disk.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.swap_encryption.enable", op:"==", value:"true" }], rationale:"SRG-OS-680100 — Swap space is encrypted to prevent recovery of sensitive data written to disk.", evidence:[{ kind:"command", cmd:"systemctl show swap_encryption 2>/dev/null || nixos-option services.swap_encryption.enable", expect:"true" }], createdBy:"security-team", createdAt:"8w ago", lastModified:"1w ago", lineageId:"stig-mock-swap-encryption-100", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680100"], cciIds:["CCI-900100"], controlFamily:"SC", framework:"DISA STIG" },
  { id:"stig-mock-tmp-mount-options-101", name:"stig-tmp-mount-options", category:"security", description:"Anduril NixOS STIG: Temporary filesystems are mounted with noexec, nosuid, and nodev options.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.tmp_mount_options.enable", op:"==", value:"true" }], rationale:"SRG-OS-680101 — Temporary filesystems are mounted with noexec, nosuid, and nodev options.", evidence:[{ kind:"command", cmd:"systemctl show tmp_mount_options 2>/dev/null || nixos-option services.tmp_mount_options.enable", expect:"true" }], createdBy:"security-team", createdAt:"9w ago", lastModified:"2w ago", lineageId:"stig-mock-tmp-mount-options-101", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680101"], cciIds:["CCI-900101"], controlFamily:"CM", framework:"DISA STIG" },
  { id:"stig-mock-home-directory-perms-102", name:"stig-home-directory-perms", category:"security", description:"Anduril NixOS STIG: User home directories default to owner-only permissions.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.home_directory_perms.enable", op:"==", value:"true" }], rationale:"SRG-OS-680102 — User home directories default to owner-only permissions.", evidence:[{ kind:"command", cmd:"systemctl show home_directory_perms 2>/dev/null || nixos-option services.home_directory_perms.enable", expect:"true" }], createdBy:"security-team", createdAt:"10w ago", lastModified:"3w ago", lineageId:"stig-mock-home-directory-perms-102", revision:1, publicationState:"current", publishedDate:"2026-04-12", srgIds:["SRG-OS-680102"], cciIds:["CCI-900102"], controlFamily:"MP", framework:"DISA STIG" },
];

// Scale set — a full DISA STIG bundle as deployed sites actually receive it (~715 controls),
// so the grouped-list navigation can be exercised at real size: CAT I 45 / CAT II 500 / CAT III 170,
// spread across 18 NIST families. Deterministic (no Math.random) so ids stay stable across reloads.
const POLICY_STIG_BULK = (() => {
  const subjects = [
    ["SSH daemon","AC","sshd"],["account lockout","AC","pam_faillock"],["session timeout","AC","logind"],
    ["audit rule set","AU","auditd"],["audit log retention","AU","auditd"],["log forwarding","AU","rsyslog"],
    ["configuration baseline","CM","nix-module"],["package allow-list","CM","nixpkgs"],
    ["contingency snapshot","CP","zfs"],["authenticator strength","IA","pam"],["certificate trust store","IA","p11-kit"],
    ["incident alerting","IR","alertmanager"],["maintenance session control","MA","cockpit"],
    ["removable media control","MP","usbguard"],["boot integrity","PE","tpm2"],["baseline planning record","PL","docs"],
    ["least-privilege role","PS","sudoers"],["risk scan cadence","RA","openscap"],
    ["acquisition provenance","SA","sbom"],["transport encryption","SC","openssl"],["kernel hardening","SC","sysctl"],
    ["file integrity monitoring","SI","aide"],["malicious code protection","SI","clamav"],["telemetry redaction","SR","otel"],
  ];
  const verbs = ["must be configured to","must enforce","must be capable of","must not permit","must automatically","must continuously"];
  const objects = [
    "reject connections that fail the approved policy check","record the outcome of each attempt for audit review",
    "terminate the session after the organization-defined period of inactivity","apply the approved cryptographic module",
    "prevent unauthorized modification of the enforcing configuration","alert designated personnel on enforcement failure",
    "restrict the action to accounts holding an explicit authorization","retain the resulting record for the required period",
  ];
  const sev = [...Array(45).fill("high"), ...Array(500).fill("medium"), ...Array(170).fill("low")];
  return sev.map((severity, i) => {
    const [subject, family, module] = subjects[i % subjects.length];
    const verb = verbs[(i * 7) % verbs.length];
    const object = objects[(i * 5) % objects.length];
    const vid = `V-2${70000 + i * 3}`;
    const slug = subject.replace(/[^a-z]+/gi, "-").toLowerCase();
    const cat = severity === "high" ? "CAT I" : severity === "medium" ? "CAT II" : "CAT III";
    return {
      id:`stig-bulk-${i}`, lineageId:`stig-bulk-${i}`, revision:1, publicationState:"current",
      publishedDate:`2026-0${(i % 9) + 1}-${String((i % 27) + 1).padStart(2,"0")}`,
      srgIds:[`SRG-OS-${String(100000 + i * 37).slice(0,6)}`], cciIds:[`CCI-00${String(1000 + (i * 13) % 8999).slice(0,4)}`],
      name:`NixOS ${subject} ${verb} ${object}.`,
      category:"security", controlFamily:family, framework:"DISA STIG", type:"custom", severity,
      description:`${cat} finding ${vid}. The ${subject} configuration is evaluated at build time against the ${module} module; a deviation fails the eval before the image is signed, so the control cannot drift into a deployed system.`,
      enabled: i % 11 !== 0,
      rules:[{ kind:"custom_eval", expr:`config.${module.replace(/-/g,"_")}.${slug.replace(/-/g,"_")}.compliant == true`, message:`${subject} must satisfy ${vid}` }],
      rationale:`${vid} (${subject}). Mapped to the ${family} family.`,
      evidence:[{ kind:"command", cmd:`check-${slug} --verify`, expect:"pass" }],
      createdBy:"security-team", createdAt:"3w ago", lastModified:`${(i % 28) + 1}d ago`,
    };
  });
})();

// Editor showcase policies — the states the policy editor has to handle cleanly:
// unmapped-but-enforced, mixed enforcement, imported-and-mapped-but-not-yet-enforced,
// and both ends of the NixOS value spectrum (a boolean and an exact multiline banner).
const POLICY_EDITOR_DEMO = [
  {
    id:"required-applications", lineageId:"required-applications", revision:1, publicationState:"current", publishedDate:"2026-06-02",
    name:"Required applications", category:"deployment", type:"custom", severity:"low", enabled:true,
    description:"Every machine in the fleet must have the in-house toolchain installed. No framework involved — this is a house rule.",
    rules:[{ kind:"packages_installed", packages:["homelab-agent","tailscale","restic"] }],
    evidence:[], rationale:"Operational baseline so remote support and backups always work.",
    createdBy:"you", createdAt:"2mo ago", lastModified:"3w ago",
  },
  {
    id:"critical-vuln-protection", lineageId:"critical-vuln-protection", revision:1, publicationState:"current", publishedDate:"2026-05-20",
    name:"Critical vulnerability protection", category:"security", framework:"NIST 800-53", controlFamily:"RA", type:"custom", severity:"high", enabled:true,
    description:"No critical CVEs may reach production, and the known-vulnerable log4j-shim package must never be in the closure.",
    rules:[
      { kind:"cve_block", severity:"critical", maxAllowed:0 },
      { kind:"packages_absent", packages:["log4j-shim","openssl-1.0"] },
    ],
    evidence:[{ kind:"eval_attr", attr:"config.environment.systemPackages" }],
    rationale:"Two different enforcement mechanisms, one policy: scan results and closure contents.",
    createdBy:"security-team", createdAt:"3mo ago", lastModified:"1w ago",
  },
  {
    id:"stig-consent-banner-exact", lineageId:"stig-consent-banner-exact", revision:1, publicationState:"current", publishedDate:"2026-04-11",
    name:"DoD consent banner text", category:"security", framework:"DISA STIG", controlFamily:"AC", type:"custom", severity:"medium", enabled:true,
    description:"/etc/issue must contain the DoD Notice and Consent banner verbatim, and sshd must be the daemon that displays it.",
    srgIds:["SRG-OS-000023-GPOS-00006"], cciIds:["CCI-000048"],
    rules:[
      { kind:"nixos_option", path:"services.openssh.enable", op:"==", value:true },
      { kind:"nixos_option", path:"environment.etc.\"issue\".text", op:"==", value: DOD_CONSENT_BANNER },
    ],
    evidence:[{ kind:"file", path:"/etc/issue", note:"Byte-for-byte match against the published banner text" }],
    rationale:"V-268082 requires the exact approved wording — a paraphrase is a finding.",
    source:{ kind:"XCCDF import", framework:"DISA STIG", artifact:"U_NixOS_V1R2_STIG.zip", ruleId:"SV-268082r1_rule", groupId:"V-268082", version:"1", release:"2", published:"2026-03-14", importedAt:"2026-04-11", importedBy:"security-team" },
    createdBy:"security-team", createdAt:"4mo ago", lastModified:"2w ago",
  },
  {
    id:"stig-fips-mode-unimplemented", lineageId:"stig-fips-mode-unimplemented", revision:1, publicationState:"draft", publishedDate:"2026-06-18",
    name:"FIPS 140-3 module must be the only crypto provider", category:"security", framework:"DISA STIG", controlFamily:"SC", type:"custom", severity:"high", enabled:false,
    description:"Imported from the benchmark with its compliance mappings intact. Nobody has written the enforcement yet, so it asserts nothing today.",
    srgIds:["SRG-OS-000033-GPOS-00014"], cciIds:["CCI-002450"],
    rules:[], evidence:[],
    rationale:"Held as a draft until the crypto-policy module lands in the fleet flake.",
    source:{ kind:"XCCDF import", framework:"DISA STIG", artifact:"U_NixOS_V1R2_STIG.zip", ruleId:"SV-268168r1_rule", groupId:"V-268168", version:"1", release:"2", published:"2026-03-14", importedAt:"2026-06-18", importedBy:"security-team" },
    createdBy:"security-team", createdAt:"2mo ago", lastModified:"2mo ago",
  },
];

const POLICIES = (typeof __fx === "function" && __fx("policies")) || [...POLICY_BUILTIN, ...POLICY_CUSTOM, ...POLICY_EDITOR_DEMO, ...POLICY_STIG_MOCK, ...POLICY_STIG_BULK];

// Per-policy usage rollup
function policyUsage(policyId) {
  const systems = (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).filter(s => s.deploymentPolicy === policyId);
  const byEnv = {};
  systems.forEach(s => { byEnv[s.environment] = (byEnv[s.environment] || 0) + 1; });
  return { systems, count: systems.length, byEnv };
}

Object.assign(window, { POLICIES, POLICY_BUILTIN, POLICY_CUSTOM, POLICY_CATEGORIES, POLICY_DOMAINS, CONTROL_FAMILIES, GROUPING_SCHEMES, policyCategoryMeta, policyDomain, policyUsage, groupPoliciesByLineage, loadCustomGroupingSchemes, saveCustomGroupingSchemes, srgCategoryOf, cmmcLevelOf, remediationStatusOf, BUILTIN_FRAMEWORKS, loadCustomFrameworks, saveCustomFrameworks, allFrameworkOptions, FRAMEWORK_ID_FIELDS });
