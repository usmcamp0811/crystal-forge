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
  { id:"quality",    label:"Quality management",  short:"Quality",  color:"#38bdf8", icon:"check",     domain:"security",
    blurb:"Process controls audited against a quality standard — change authorization, records, audit cadence." },
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

// Anduril NixOS STIG rule set (OS controls) — completes the 110-control bundle alongside the hand-authored policies above.
const POLICY_STIG_MOCK = [
  { id:"stig-sshd-root-login-disabled", lineageId:"stig-sshd-root-login-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268140", srgIds:["SRG-OS-000021","SRG-OS-000100"], cciIds:["CCI-000044","CCI-001000"], name:"stig-sshd-root-login-disabled", category:"security", controlFamily:"AC", description:"SSH daemon must not permit direct root login.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.PermitRootLogin", op:"==", value:"\"no\"" }], rationale:"V-268140 — SSH daemon must not permit direct root login. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i permitrootlogin", expect:"permitrootlogin no" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-sshd-password-auth-disabled", lineageId:"stig-sshd-password-auth-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268142", srgIds:["SRG-OS-000069","SRG-OS-000107"], cciIds:["CCI-000192","CCI-001013"], name:"stig-sshd-password-auth-disabled", category:"security", controlFamily:"IA", description:"SSH daemon must not permit password authentication.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.PasswordAuthentication", op:"==", value:"false" }], rationale:"V-268142 — SSH daemon must not permit password authentication. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i passwordauthentication", expect:"passwordauthentication no" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-sshd-empty-passwords-denied", lineageId:"stig-sshd-empty-passwords-denied", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268144", srgIds:["SRG-OS-000069","SRG-OS-000114"], cciIds:["CCI-000192","CCI-001026"], name:"stig-sshd-empty-passwords-denied", category:"security", controlFamily:"IA", description:"SSH daemon must not permit authentication with empty passwords.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.PermitEmptyPasswords", op:"==", value:"false" }], rationale:"V-268144 — SSH daemon must not permit authentication with empty passwords. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i permitemptypasswords", expect:"permitemptypasswords no" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-sshd-x11-forwarding-disabled", lineageId:"stig-sshd-x11-forwarding-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268146", srgIds:["SRG-OS-000095","SRG-OS-000121"], cciIds:["CCI-000381","CCI-001039"], name:"stig-sshd-x11-forwarding-disabled", category:"security", controlFamily:"CM", description:"SSH daemon must not permit X11 forwarding.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.X11Forwarding", op:"==", value:"false" }], rationale:"V-268146 — SSH daemon must not permit X11 forwarding. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i x11forwarding", expect:"x11forwarding no" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-sshd-idle-timeout", lineageId:"stig-sshd-idle-timeout", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268148", srgIds:["SRG-OS-000021","SRG-OS-000128"], cciIds:["CCI-000044","CCI-001052"], name:"stig-sshd-idle-timeout", category:"security", controlFamily:"AC", description:"SSH daemon must terminate idle sessions after 10 minutes.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.ClientAliveInterval", op:"==", value:"600" }], rationale:"V-268148 — SSH daemon must terminate idle sessions after 10 minutes. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i clientaliveinterval", expect:"clientaliveinterval 600" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-sshd-login-grace-time", lineageId:"stig-sshd-login-grace-time", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268150", srgIds:["SRG-OS-000021","SRG-OS-000135"], cciIds:["CCI-000044","CCI-001065"], name:"stig-sshd-login-grace-time", category:"security", controlFamily:"AC", description:"SSH daemon must limit the authentication grace period to 60 seconds.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.LoginGraceTime", op:"==", value:"60" }], rationale:"V-268150 — SSH daemon must limit the authentication grace period to 60 seconds. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i logingracetime", expect:"logingracetime 60" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-sshd-max-auth-tries", lineageId:"stig-sshd-max-auth-tries", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268152", srgIds:["SRG-OS-000021","SRG-OS-000142"], cciIds:["CCI-000044","CCI-001078"], name:"stig-sshd-max-auth-tries", category:"security", controlFamily:"AC", description:"SSH daemon must limit authentication attempts per connection.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.MaxAuthTries", op:"==", value:"3" }], rationale:"V-268152 — SSH daemon must limit authentication attempts per connection. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i maxauthtries", expect:"maxauthtries 3" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-sshd-fips-macs", lineageId:"stig-sshd-fips-macs", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268154", srgIds:["SRG-OS-000033","SRG-OS-000149"], cciIds:["CCI-000068","CCI-001091"], name:"stig-sshd-fips-macs", category:"security", controlFamily:"SC", description:"SSH daemon must use only FIPS-validated message authentication codes.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.Macs", op:"==", value:"FIPS_APPROVED_MACS" }], rationale:"V-268154 — SSH daemon must use only FIPS-validated message authentication codes. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i macs", expect:"hmac-sha2-512,hmac-sha2-256" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-sshd-fips-kex-algorithms", lineageId:"stig-sshd-fips-kex-algorithms", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268156", srgIds:["SRG-OS-000033","SRG-OS-000156"], cciIds:["CCI-000068","CCI-001104"], name:"stig-sshd-fips-kex-algorithms", category:"security", controlFamily:"SC", description:"SSH daemon must use only FIPS-validated key exchange algorithms.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.KexAlgorithms", op:"==", value:"FIPS_APPROVED_KEX" }], rationale:"V-268156 — SSH daemon must use only FIPS-validated key exchange algorithms. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i kexalgorithms", expect:"ecdh-sha2-nistp384" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-sshd-strict-modes", lineageId:"stig-sshd-strict-modes", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268158", srgIds:["SRG-OS-000095","SRG-OS-000163"], cciIds:["CCI-000381","CCI-001117"], name:"stig-sshd-strict-modes", category:"security", controlFamily:"CM", description:"SSH daemon must verify permissions on user key files before accepting a login.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.StrictModes", op:"==", value:"true" }], rationale:"V-268158 — SSH daemon must verify permissions on user key files before accepting a login. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i strictmodes", expect:"strictmodes yes" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-sshd-host-key-algorithms", lineageId:"stig-sshd-host-key-algorithms", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268160", srgIds:["SRG-OS-000033","SRG-OS-000170"], cciIds:["CCI-000068","CCI-001130"], name:"stig-sshd-host-key-algorithms", category:"security", controlFamily:"SC", description:"SSH daemon must offer only approved host key algorithms.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.settings.HostKeyAlgorithms", op:"==", value:"APPROVED_HOST_KEY_ALGS" }], rationale:"V-268160 — SSH daemon must offer only approved host key algorithms. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i hostkeyalgorithms", expect:"rsa-sha2-512,ecdsa-sha2-nistp384" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-auditd-enabled", lineageId:"stig-auditd-enabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268162", srgIds:["SRG-OS-000062","SRG-OS-000177"], cciIds:["CCI-000169","CCI-001143"], name:"stig-auditd-enabled", category:"security", controlFamily:"AU", description:"The audit daemon must be enabled at boot.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"security.audit.enable", op:"==", value:"true" }], rationale:"V-268162 — The audit daemon must be enabled at boot. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"systemctl is-active auditd", expect:"active" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-auditd-audit-backlog-limit", lineageId:"stig-auditd-audit-backlog-limit", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268164", srgIds:["SRG-OS-000062","SRG-OS-000184"], cciIds:["CCI-000169","CCI-001156"], name:"stig-auditd-audit-backlog-limit", category:"security", controlFamily:"AU", description:"The kernel audit backlog limit must be at least 8192.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernelParams", op:"==", value:"[ \"audit_backlog_limit=8192\" ]" }], rationale:"V-268164 — The kernel audit backlog limit must be at least 8192. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"cat /proc/cmdline | tr ' ' '\n' | grep audit_backlog", expect:"audit_backlog_limit=8192" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-auditd-failure-action", lineageId:"stig-auditd-failure-action", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268166", srgIds:["SRG-OS-000062","SRG-OS-000191"], cciIds:["CCI-000169","CCI-001169"], name:"stig-auditd-failure-action", category:"security", controlFamily:"AU", description:"The audit system must take a defined action when audit storage fails.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.auditd.failureMode", op:"==", value:"2" }], rationale:"V-268166 — The audit system must take a defined action when audit storage fails. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"auditctl -s | grep failure", expect:"failure 2" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-auditd-log-retention", lineageId:"stig-auditd-log-retention", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268168", srgIds:["SRG-OS-000062","SRG-OS-000198"], cciIds:["CCI-000169","CCI-001182"], name:"stig-auditd-log-retention", category:"security", controlFamily:"AU", description:"Audit records must be retained for at least 90 days.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.journald.extraConfig", op:"==", value:"\"MaxRetentionSec=90day\"" }], rationale:"V-268168 — Audit records must be retained for at least 90 days. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"grep MaxRetentionSec /etc/systemd/journald.conf", expect:"MaxRetentionSec=90day" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-auditd-execve-rules", lineageId:"stig-auditd-execve-rules", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268170", srgIds:["SRG-OS-000062","SRG-OS-000205"], cciIds:["CCI-000169","CCI-001195"], name:"stig-auditd-execve-rules", category:"security", controlFamily:"AU", description:"The audit system must record all process executions.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.audit.rules", op:"==", value:"AUDIT_EXECVE_RULES" }], rationale:"V-268170 — The audit system must record all process executions. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"auditctl -l | grep execve", expect:"-a always,exit -F arch=b64 -S execve" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-auditd-privileged-command-rules", lineageId:"stig-auditd-privileged-command-rules", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268172", srgIds:["SRG-OS-000062","SRG-OS-000212"], cciIds:["CCI-000169","CCI-001208"], name:"stig-auditd-privileged-command-rules", category:"security", controlFamily:"AU", description:"The audit system must record execution of privileged commands.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.audit.rules", op:"==", value:"AUDIT_PRIVILEGED_RULES" }], rationale:"V-268172 — The audit system must record execution of privileged commands. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"auditctl -l | grep priv_cmd", expect:"-F perm=x -F auid>=1000" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-auditd-account-mod-rules", lineageId:"stig-auditd-account-mod-rules", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268174", srgIds:["SRG-OS-000062","SRG-OS-000219"], cciIds:["CCI-000169","CCI-001221"], name:"stig-auditd-account-mod-rules", category:"security", controlFamily:"AU", description:"The audit system must record modifications to account files.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.audit.rules", op:"==", value:"AUDIT_ACCOUNT_RULES" }], rationale:"V-268174 — The audit system must record modifications to account files. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"auditctl -l | grep identity", expect:"-w /etc/passwd -p wa -k identity" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-auditd-sudo-log-rules", lineageId:"stig-auditd-sudo-log-rules", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268176", srgIds:["SRG-OS-000062","SRG-OS-000226"], cciIds:["CCI-000169","CCI-001234"], name:"stig-auditd-sudo-log-rules", category:"security", controlFamily:"AU", description:"The audit system must record all uses of sudo.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.audit.rules", op:"==", value:"AUDIT_SUDO_RULES" }], rationale:"V-268176 — The audit system must record all uses of sudo. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"auditctl -l | grep sudo", expect:"-w /etc/sudoers -p wa" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-auditd-time-change-rules", lineageId:"stig-auditd-time-change-rules", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268178", srgIds:["SRG-OS-000062","SRG-OS-000233"], cciIds:["CCI-000169","CCI-001247"], name:"stig-auditd-time-change-rules", category:"security", controlFamily:"AU", description:"The audit system must record changes to the system clock.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.audit.rules", op:"==", value:"AUDIT_TIME_RULES" }], rationale:"V-268178 — The audit system must record changes to the system clock. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"auditctl -l | grep time-change", expect:"-S adjtimex -k time-change" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-auditd-perm-mod-rules", lineageId:"stig-auditd-perm-mod-rules", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268180", srgIds:["SRG-OS-000062","SRG-OS-000240"], cciIds:["CCI-000169","CCI-001260"], name:"stig-auditd-perm-mod-rules", category:"security", controlFamily:"AU", description:"The audit system must record permission and ownership changes.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.audit.rules", op:"==", value:"AUDIT_PERM_RULES" }], rationale:"V-268180 — The audit system must record permission and ownership changes. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"auditctl -l | grep perm_mod", expect:"-S chmod -S chown -k perm_mod" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-auditd-module-load-rules", lineageId:"stig-auditd-module-load-rules", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268182", srgIds:["SRG-OS-000062","SRG-OS-000247"], cciIds:["CCI-000169","CCI-001273"], name:"stig-auditd-module-load-rules", category:"security", controlFamily:"AU", description:"The audit system must record kernel module load and unload events.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.audit.rules", op:"==", value:"AUDIT_MODULE_RULES" }], rationale:"V-268182 — The audit system must record kernel module load and unload events. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"auditctl -l | grep modules", expect:"-w /sbin/insmod -p x -k modules" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-auditd-immutable-ruleset", lineageId:"stig-auditd-immutable-ruleset", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268184", srgIds:["SRG-OS-000062","SRG-OS-000254"], cciIds:["CCI-000169","CCI-001286"], name:"stig-auditd-immutable-ruleset", category:"security", controlFamily:"AU", description:"The audit ruleset must be immutable until reboot.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.audit.rules", op:"==", value:"[ \"-e 2\" ]" }], rationale:"V-268184 — The audit ruleset must be immutable until reboot. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"auditctl -s | grep enabled", expect:"enabled 2" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-auditd-remote-offload", lineageId:"stig-auditd-remote-offload", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268186", srgIds:["SRG-OS-000062","SRG-OS-000261"], cciIds:["CCI-000169","CCI-001299"], name:"stig-auditd-remote-offload", category:"security", controlFamily:"AU", description:"Audit records must be offloaded to a central log host.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.rsyslog.extraConfig", op:"==", value:"REMOTE_AUDIT_TARGET" }], rationale:"V-268186 — Audit records must be offloaded to a central log host. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"logger -t stig-check test && grep stig-check /var/log/forwarded", expect:"forwarded" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-pam-faillock-deny", lineageId:"stig-pam-faillock-deny", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268188", srgIds:["SRG-OS-000021","SRG-OS-000268"], cciIds:["CCI-000044","CCI-001312"], name:"stig-pam-faillock-deny", category:"security", controlFamily:"AC", description:"Accounts must lock after three consecutive failed logon attempts.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.pam.faillock.deny", op:"==", value:"3" }], rationale:"V-268188 — Accounts must lock after three consecutive failed logon attempts. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"grep deny= /etc/security/faillock.conf", expect:"deny = 3" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-pam-faillock-unlock-time", lineageId:"stig-pam-faillock-unlock-time", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268190", srgIds:["SRG-OS-000021","SRG-OS-000275"], cciIds:["CCI-000044","CCI-001325"], name:"stig-pam-faillock-unlock-time", category:"security", controlFamily:"AC", description:"Locked accounts must remain locked until released by an administrator.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.pam.faillock.unlockTime", op:"==", value:"0" }], rationale:"V-268190 — Locked accounts must remain locked until released by an administrator. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"grep unlock_time /etc/security/faillock.conf", expect:"unlock_time = 0" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-pam-faillock-fail-interval", lineageId:"stig-pam-faillock-fail-interval", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268192", srgIds:["SRG-OS-000021","SRG-OS-000282"], cciIds:["CCI-000044","CCI-001338"], name:"stig-pam-faillock-fail-interval", category:"security", controlFamily:"AC", description:"Failed logon attempts must be counted within a 15-minute interval.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"security.pam.faillock.failInterval", op:"==", value:"900" }], rationale:"V-268192 — Failed logon attempts must be counted within a 15-minute interval. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"grep fail_interval /etc/security/faillock.conf", expect:"fail_interval = 900" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-pwquality-minlen", lineageId:"stig-pwquality-minlen", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268194", srgIds:["SRG-OS-000069","SRG-OS-000289"], cciIds:["CCI-000192","CCI-001351"], name:"stig-pwquality-minlen", category:"security", controlFamily:"IA", description:"Passwords must be at least 15 characters long.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.pam.pwquality.settings.minlen", op:"==", value:"15" }], rationale:"V-268194 — Passwords must be at least 15 characters long. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"grep minlen /etc/security/pwquality.conf", expect:"minlen = 15" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-pwquality-complexity-classes", lineageId:"stig-pwquality-complexity-classes", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268196", srgIds:["SRG-OS-000069","SRG-OS-000296"], cciIds:["CCI-000192","CCI-001364"], name:"stig-pwquality-complexity-classes", category:"security", controlFamily:"IA", description:"Passwords must contain at least one character from each class.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.pam.pwquality.settings.minclass", op:"==", value:"4" }], rationale:"V-268196 — Passwords must contain at least one character from each class. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"grep minclass /etc/security/pwquality.conf", expect:"minclass = 4" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-pwquality-max-repeat", lineageId:"stig-pwquality-max-repeat", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268198", srgIds:["SRG-OS-000069","SRG-OS-000303"], cciIds:["CCI-000192","CCI-001377"], name:"stig-pwquality-max-repeat", category:"security", controlFamily:"IA", description:"Passwords must not contain more than three consecutive repeating characters.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"security.pam.pwquality.settings.maxrepeat", op:"==", value:"3" }], rationale:"V-268198 — Passwords must not contain more than three consecutive repeating characters. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"grep maxrepeat /etc/security/pwquality.conf", expect:"maxrepeat = 3" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-pwquality-dictcheck", lineageId:"stig-pwquality-dictcheck", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268200", srgIds:["SRG-OS-000069","SRG-OS-000310"], cciIds:["CCI-000192","CCI-001390"], name:"stig-pwquality-dictcheck", category:"security", controlFamily:"IA", description:"Passwords must be screened against a dictionary of common words.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.pam.pwquality.settings.dictcheck", op:"==", value:"1" }], rationale:"V-268200 — Passwords must be screened against a dictionary of common words. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"grep dictcheck /etc/security/pwquality.conf", expect:"dictcheck = 1" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-password-hash-sha512", lineageId:"stig-password-hash-sha512", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268202", srgIds:["SRG-OS-000069","SRG-OS-000317"], cciIds:["CCI-000192","CCI-001403"], name:"stig-password-hash-sha512", category:"security", controlFamily:"IA", description:"Passwords must be stored using a SHA-512 hash.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"security.loginDefs.settings.ENCRYPT_METHOD", op:"==", value:"\"SHA512\"" }], rationale:"V-268202 — Passwords must be stored using a SHA-512 hash. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"grep ENCRYPT_METHOD /etc/login.defs", expect:"ENCRYPT_METHOD SHA512" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-password-max-age", lineageId:"stig-password-max-age", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268204", srgIds:["SRG-OS-000069","SRG-OS-000324"], cciIds:["CCI-000192","CCI-001416"], name:"stig-password-max-age", category:"security", controlFamily:"IA", description:"Passwords must expire after 60 days.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.loginDefs.settings.PASS_MAX_DAYS", op:"==", value:"60" }], rationale:"V-268204 — Passwords must expire after 60 days. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"grep PASS_MAX_DAYS /etc/login.defs", expect:"PASS_MAX_DAYS 60" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-password-min-age", lineageId:"stig-password-min-age", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268206", srgIds:["SRG-OS-000069","SRG-OS-000331"], cciIds:["CCI-000192","CCI-001429"], name:"stig-password-min-age", category:"security", controlFamily:"IA", description:"Passwords must not be changeable more than once per 24 hours.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"security.loginDefs.settings.PASS_MIN_DAYS", op:"==", value:"1" }], rationale:"V-268206 — Passwords must not be changeable more than once per 24 hours. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"grep PASS_MIN_DAYS /etc/login.defs", expect:"PASS_MIN_DAYS 1" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-password-reuse-limit", lineageId:"stig-password-reuse-limit", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268208", srgIds:["SRG-OS-000069","SRG-OS-000338"], cciIds:["CCI-000192","CCI-001442"], name:"stig-password-reuse-limit", category:"security", controlFamily:"IA", description:"The last five passwords must not be reused.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.pam.services.passwd.rules.password.unix.settings.remember", op:"==", value:"5" }], rationale:"V-268208 — The last five passwords must not be reused. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"grep remember /etc/pam.d/passwd", expect:"remember=5" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-accounts-no-shared-ids", lineageId:"stig-accounts-no-shared-ids", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268210", srgIds:["SRG-OS-000069","SRG-OS-000345"], cciIds:["CCI-000192","CCI-001455"], name:"stig-accounts-no-shared-ids", category:"security", controlFamily:"IA", description:"Accounts must not share a UID.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"users.enforceIdUniqueness", op:"==", value:"true" }], rationale:"V-268210 — Accounts must not share a UID. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"awk -F: '{print $3}' /etc/passwd | sort | uniq -d", expect:"(empty)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-accounts-inactive-disable", lineageId:"stig-accounts-inactive-disable", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268212", srgIds:["SRG-OS-000021","SRG-OS-000352"], cciIds:["CCI-000044","CCI-001468"], name:"stig-accounts-inactive-disable", category:"security", controlFamily:"AC", description:"Accounts inactive for 35 days must be disabled.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.loginDefs.settings.INACTIVE", op:"==", value:"35" }], rationale:"V-268212 — Accounts inactive for 35 days must be disabled. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"useradd -D | grep INACTIVE", expect:"INACTIVE=35" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-sudo-no-nopasswd", lineageId:"stig-sudo-no-nopasswd", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268214", srgIds:["SRG-OS-000021","SRG-OS-000359"], cciIds:["CCI-000044","CCI-001481"], name:"stig-sudo-no-nopasswd", category:"security", controlFamily:"AC", description:"sudo must require re-authentication and must not use NOPASSWD.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"security.sudo.wheelNeedsPassword", op:"==", value:"true" }], rationale:"V-268214 — sudo must require re-authentication and must not use NOPASSWD. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"grep -r NOPASSWD /etc/sudoers /etc/sudoers.d", expect:"(no matches)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-sudo-timestamp-timeout", lineageId:"stig-sudo-timestamp-timeout", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268216", srgIds:["SRG-OS-000021","SRG-OS-000366"], cciIds:["CCI-000044","CCI-001494"], name:"stig-sudo-timestamp-timeout", category:"security", controlFamily:"AC", description:"The sudo credential cache must time out after 5 minutes.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.sudo.extraConfig", op:"==", value:"\"Defaults timestamp_timeout=5\"" }], rationale:"V-268216 — The sudo credential cache must time out after 5 minutes. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"sudo -l | grep timestamp_timeout", expect:"timestamp_timeout=5" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-sudo-use-pty", lineageId:"stig-sudo-use-pty", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268218", srgIds:["SRG-OS-000021","SRG-OS-000373"], cciIds:["CCI-000044","CCI-001507"], name:"stig-sudo-use-pty", category:"security", controlFamily:"AC", description:"sudo must be configured to run commands in a pseudo-terminal.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"security.sudo.extraConfig", op:"==", value:"\"Defaults use_pty\"" }], rationale:"V-268218 — sudo must be configured to run commands in a pseudo-terminal. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"grep use_pty /etc/sudoers", expect:"Defaults use_pty" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-login-banner-dod-notice", lineageId:"stig-login-banner-dod-notice", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268220", srgIds:["SRG-OS-000021","SRG-OS-000380"], cciIds:["CCI-000044","CCI-001520"], name:"stig-login-banner-dod-notice", category:"security", controlFamily:"AC", description:"The DoD notice and consent banner must be displayed before logon.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"environment.etc.\"issue\".text", op:"==", value:"DOD_LOGIN_BANNER" }], rationale:"V-268220 — The DoD notice and consent banner must be displayed before logon. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"head -1 /etc/issue", expect:"You are accessing a U.S. Government" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-ssh-banner-configured", lineageId:"stig-ssh-banner-configured", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268222", srgIds:["SRG-OS-000021","SRG-OS-000387"], cciIds:["CCI-000044","CCI-001533"], name:"stig-ssh-banner-configured", category:"security", controlFamily:"AC", description:"SSH must display the standard notice and consent banner.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.openssh.banner", op:"==", value:"\"/etc/issue\"" }], rationale:"V-268222 — SSH must display the standard notice and consent banner. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"sshd -T | grep -i banner", expect:"banner /etc/issue" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-session-idle-lock", lineageId:"stig-session-idle-lock", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268224", srgIds:["SRG-OS-000021","SRG-OS-000394"], cciIds:["CCI-000044","CCI-001546"], name:"stig-session-idle-lock", category:"security", controlFamily:"AC", description:"Sessions must lock after 15 minutes of inactivity.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"environment.variables.TMOUT", op:"==", value:"\"900\"" }], rationale:"V-268224 — Sessions must lock after 15 minutes of inactivity. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"grep TMOUT /etc/profile.d/tmout.sh", expect:"TMOUT=900" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-ctrl-alt-del-disabled", lineageId:"stig-ctrl-alt-del-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268226", srgIds:["SRG-OS-000095","SRG-OS-000401"], cciIds:["CCI-000381","CCI-001559"], name:"stig-ctrl-alt-del-disabled", category:"security", controlFamily:"CM", description:"The Ctrl-Alt-Del key sequence must not reboot the system.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"systemd.extraConfig", op:"==", value:"\"CtrlAltDelBurstAction=none\"" }], rationale:"V-268226 — The Ctrl-Alt-Del key sequence must not reboot the system. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"systemctl status ctrl-alt-del.target", expect:"masked" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-kernel-aslr-enabled", lineageId:"stig-kernel-aslr-enabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268228", srgIds:["SRG-OS-000033","SRG-OS-000408"], cciIds:["CCI-000068","CCI-001572"], name:"stig-kernel-aslr-enabled", category:"security", controlFamily:"SC", description:"Address space layout randomization must be fully enabled.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernel.sysctl.\"kernel.randomize_va_space\"", op:"==", value:"2" }], rationale:"V-268228 — Address space layout randomization must be fully enabled. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sysctl kernel.randomize_va_space", expect:"= 2" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-kernel-ptrace-scope", lineageId:"stig-kernel-ptrace-scope", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268230", srgIds:["SRG-OS-000033","SRG-OS-000415"], cciIds:["CCI-000068","CCI-001585"], name:"stig-kernel-ptrace-scope", category:"security", controlFamily:"SC", description:"Kernel ptrace scope must be restricted.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernel.sysctl.\"kernel.yama.ptrace_scope\"", op:"==", value:"1" }], rationale:"V-268230 — Kernel ptrace scope must be restricted. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sysctl kernel.yama.ptrace_scope", expect:"= 1" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-kernel-dmesg-restrict", lineageId:"stig-kernel-dmesg-restrict", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268232", srgIds:["SRG-OS-000191","SRG-OS-000422"], cciIds:["CCI-001240","CCI-001598"], name:"stig-kernel-dmesg-restrict", category:"security", controlFamily:"SI", description:"Access to the kernel ring buffer must be restricted.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernel.sysctl.\"kernel.dmesg_restrict\"", op:"==", value:"1" }], rationale:"V-268232 — Access to the kernel ring buffer must be restricted. SRG-OS-000191 / CCI-001240.", evidence:[{ kind:"command", cmd:"sysctl kernel.dmesg_restrict", expect:"= 1" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-kernel-core-dumps-disabled", lineageId:"stig-kernel-core-dumps-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268234", srgIds:["SRG-OS-000191","SRG-OS-000429"], cciIds:["CCI-001240","CCI-001611"], name:"stig-kernel-core-dumps-disabled", category:"security", controlFamily:"SI", description:"Core dumps must be disabled for all users.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"systemd.coredump.enable", op:"==", value:"false" }], rationale:"V-268234 — Core dumps must be disabled for all users. SRG-OS-000191 / CCI-001240.", evidence:[{ kind:"command", cmd:"sysctl fs.suid_dumpable", expect:"= 0" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-kernel-unprivileged-bpf-disabled", lineageId:"stig-kernel-unprivileged-bpf-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268236", srgIds:["SRG-OS-000033","SRG-OS-000436"], cciIds:["CCI-000068","CCI-001624"], name:"stig-kernel-unprivileged-bpf-disabled", category:"security", controlFamily:"SC", description:"Unprivileged access to BPF must be disabled.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernel.sysctl.\"kernel.unprivileged_bpf_disabled\"", op:"==", value:"1" }], rationale:"V-268236 — Unprivileged access to BPF must be disabled. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sysctl kernel.unprivileged_bpf_disabled", expect:"= 1" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-kernel-module-loading-locked", lineageId:"stig-kernel-module-loading-locked", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268238", srgIds:["SRG-OS-000095","SRG-OS-000443"], cciIds:["CCI-000381","CCI-001637"], name:"stig-kernel-module-loading-locked", category:"security", controlFamily:"CM", description:"Kernel module loading must be disabled after boot.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernel.sysctl.\"kernel.modules_disabled\"", op:"==", value:"1" }], rationale:"V-268238 — Kernel module loading must be disabled after boot. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"sysctl kernel.modules_disabled", expect:"= 1" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-kernel-lockdown-integrity", lineageId:"stig-kernel-lockdown-integrity", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268240", srgIds:["SRG-OS-000191","SRG-OS-000450"], cciIds:["CCI-001240","CCI-001650"], name:"stig-kernel-lockdown-integrity", category:"security", controlFamily:"SI", description:"Kernel lockdown must be enabled in integrity mode.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernelParams", op:"==", value:"[ \"lockdown=integrity\" ]" }], rationale:"V-268240 — Kernel lockdown must be enabled in integrity mode. SRG-OS-000191 / CCI-001240.", evidence:[{ kind:"command", cmd:"cat /sys/kernel/security/lockdown", expect:"[integrity]" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-net-ip-forward-disabled", lineageId:"stig-net-ip-forward-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268242", srgIds:["SRG-OS-000033","SRG-OS-000457"], cciIds:["CCI-000068","CCI-001663"], name:"stig-net-ip-forward-disabled", category:"security", controlFamily:"SC", description:"IP forwarding must be disabled on non-router hosts.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernel.sysctl.\"net.ipv4.ip_forward\"", op:"==", value:"0" }], rationale:"V-268242 — IP forwarding must be disabled on non-router hosts. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sysctl net.ipv4.ip_forward", expect:"= 0" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-net-source-route-disabled", lineageId:"stig-net-source-route-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268244", srgIds:["SRG-OS-000033","SRG-OS-000464"], cciIds:["CCI-000068","CCI-001676"], name:"stig-net-source-route-disabled", category:"security", controlFamily:"SC", description:"Source-routed packets must not be accepted.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernel.sysctl.\"net.ipv4.conf.all.accept_source_route\"", op:"==", value:"0" }], rationale:"V-268244 — Source-routed packets must not be accepted. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sysctl net.ipv4.conf.all.accept_source_route", expect:"= 0" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-net-icmp-redirects-disabled", lineageId:"stig-net-icmp-redirects-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268246", srgIds:["SRG-OS-000033","SRG-OS-000471"], cciIds:["CCI-000068","CCI-001689"], name:"stig-net-icmp-redirects-disabled", category:"security", controlFamily:"SC", description:"ICMP redirects must not be accepted.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernel.sysctl.\"net.ipv4.conf.all.accept_redirects\"", op:"==", value:"0" }], rationale:"V-268246 — ICMP redirects must not be accepted. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sysctl net.ipv4.conf.all.accept_redirects", expect:"= 0" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-net-reverse-path-filter", lineageId:"stig-net-reverse-path-filter", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268248", srgIds:["SRG-OS-000033","SRG-OS-000478"], cciIds:["CCI-000068","CCI-001702"], name:"stig-net-reverse-path-filter", category:"security", controlFamily:"SC", description:"Reverse path filtering must be enabled on all interfaces.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernel.sysctl.\"net.ipv4.conf.all.rp_filter\"", op:"==", value:"1" }], rationale:"V-268248 — Reverse path filtering must be enabled on all interfaces. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sysctl net.ipv4.conf.all.rp_filter", expect:"= 1" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-net-ipv6-ra-disabled", lineageId:"stig-net-ipv6-ra-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268250", srgIds:["SRG-OS-000033","SRG-OS-000105"], cciIds:["CCI-000068","CCI-001715"], name:"stig-net-ipv6-ra-disabled", category:"security", controlFamily:"SC", description:"IPv6 router advertisements must not be accepted.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernel.sysctl.\"net.ipv6.conf.all.accept_ra\"", op:"==", value:"0" }], rationale:"V-268250 — IPv6 router advertisements must not be accepted. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"sysctl net.ipv6.conf.all.accept_ra", expect:"= 0" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-fs-tmp-noexec", lineageId:"stig-fs-tmp-noexec", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268252", srgIds:["SRG-OS-000095","SRG-OS-000112"], cciIds:["CCI-000381","CCI-001728"], name:"stig-fs-tmp-noexec", category:"security", controlFamily:"CM", description:"The /tmp filesystem must be mounted with noexec.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"fileSystems.\"/tmp\".options", op:"==", value:"[ \"noexec\" \"nosuid\" \"nodev\" ]" }], rationale:"V-268252 — The /tmp filesystem must be mounted with noexec. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"findmnt /tmp -o OPTIONS", expect:"noexec,nosuid,nodev" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-fs-vartmp-nosuid", lineageId:"stig-fs-vartmp-nosuid", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268254", srgIds:["SRG-OS-000095","SRG-OS-000119"], cciIds:["CCI-000381","CCI-001741"], name:"stig-fs-vartmp-nosuid", category:"security", controlFamily:"CM", description:"The /var/tmp filesystem must be mounted with nosuid.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"fileSystems.\"/var/tmp\".options", op:"==", value:"[ \"nosuid\" \"nodev\" ]" }], rationale:"V-268254 — The /var/tmp filesystem must be mounted with nosuid. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"findmnt /var/tmp -o OPTIONS", expect:"nosuid,nodev" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-fs-home-nosuid", lineageId:"stig-fs-home-nosuid", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268256", srgIds:["SRG-OS-000095","SRG-OS-000126"], cciIds:["CCI-000381","CCI-001754"], name:"stig-fs-home-nosuid", category:"security", controlFamily:"CM", description:"The /home filesystem must be mounted with nosuid.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"fileSystems.\"/home\".options", op:"==", value:"[ \"nosuid\" \"nodev\" ]" }], rationale:"V-268256 — The /home filesystem must be mounted with nosuid. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"findmnt /home -o OPTIONS", expect:"nosuid,nodev" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-fs-removable-nosuid", lineageId:"stig-fs-removable-nosuid", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268258", srgIds:["SRG-OS-000480","SRG-OS-000133"], cciIds:["CCI-000366","CCI-001767"], name:"stig-fs-removable-nosuid", category:"security", controlFamily:"MP", description:"Removable media must be mounted with nosuid and noexec.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.udisks2.settings", op:"==", value:"REMOVABLE_MOUNT_OPTS" }], rationale:"V-268258 — Removable media must be mounted with nosuid and noexec. SRG-OS-000480 / CCI-000366.", evidence:[{ kind:"command", cmd:"findmnt -t vfat -o OPTIONS", expect:"nosuid,noexec" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-fs-home-dir-permissions", lineageId:"stig-fs-home-dir-permissions", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268260", srgIds:["SRG-OS-000021","SRG-OS-000140"], cciIds:["CCI-000044","CCI-001780"], name:"stig-fs-home-dir-permissions", category:"security", controlFamily:"AC", description:"Home directories must be mode 0750 or more restrictive.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"users.users.defaults.homeMode", op:"==", value:"\"0750\"" }], rationale:"V-268260 — Home directories must be mode 0750 or more restrictive. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"stat -c %a /home/*", expect:"750" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-fs-world-writable-dirs-sticky", lineageId:"stig-fs-world-writable-dirs-sticky", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268262", srgIds:["SRG-OS-000021","SRG-OS-000147"], cciIds:["CCI-000044","CCI-001793"], name:"stig-fs-world-writable-dirs-sticky", category:"security", controlFamily:"AC", description:"World-writable directories must have the sticky bit set.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"system.activationScripts.stigStickyBits", op:"==", value:"STICKY_BIT_SCRIPT" }], rationale:"V-268262 — World-writable directories must have the sticky bit set. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"find / -xdev -type d -perm -0002 ! -perm -1000", expect:"(empty)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-fs-no-unowned-files", lineageId:"stig-fs-no-unowned-files", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268264", srgIds:["SRG-OS-000021","SRG-OS-000154"], cciIds:["CCI-000044","CCI-001806"], name:"stig-fs-no-unowned-files", category:"security", controlFamily:"AC", description:"Files must not be owned by an unassigned user or group.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"system.activationScripts.stigOwnershipAudit", op:"==", value:"OWNERSHIP_AUDIT_SCRIPT" }], rationale:"V-268264 — Files must not be owned by an unassigned user or group. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"find / -xdev -nouser -o -nogroup", expect:"(empty)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-fs-library-permissions", lineageId:"stig-fs-library-permissions", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268266", srgIds:["SRG-OS-000095","SRG-OS-000161"], cciIds:["CCI-000381","CCI-001819"], name:"stig-fs-library-permissions", category:"security", controlFamily:"CM", description:"System library directories must not be group- or world-writable.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"system.activationScripts.stigLibPerms", op:"==", value:"LIB_PERMS_SCRIPT" }], rationale:"V-268266 — System library directories must not be group- or world-writable. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"find /nix/store -maxdepth 1 -perm -0022", expect:"(empty)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-fs-audit-log-permissions", lineageId:"stig-fs-audit-log-permissions", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268268", srgIds:["SRG-OS-000062","SRG-OS-000168"], cciIds:["CCI-000169","CCI-001832"], name:"stig-fs-audit-log-permissions", category:"security", controlFamily:"AU", description:"Audit log files must be mode 0600 and owned by root.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.auditd.logPermissions", op:"==", value:"\"0600\"" }], rationale:"V-268268 — Audit log files must be mode 0600 and owned by root. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"stat -c '%a %U' /var/log/audit/audit.log", expect:"600 root" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-disk-encryption-luks2", lineageId:"stig-disk-encryption-luks2", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268270", srgIds:["SRG-OS-000033","SRG-OS-000175"], cciIds:["CCI-000068","CCI-001845"], name:"stig-disk-encryption-luks2", category:"security", controlFamily:"SC", description:"Data at rest must be encrypted with LUKS2 and an approved cipher.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"boot.initrd.luks.devices.\"cryptroot\".device", op:"==", value:"\"/dev/disk/by-partlabel/luks\"" }], rationale:"V-268270 — Data at rest must be encrypted with LUKS2 and an approved cipher. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"cryptsetup luksDump /dev/disk/by-partlabel/luks | grep Version", expect:"Version: 2" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-swap-encrypted", lineageId:"stig-swap-encrypted", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268272", srgIds:["SRG-OS-000033","SRG-OS-000182"], cciIds:["CCI-000068","CCI-001858"], name:"stig-swap-encrypted", category:"security", controlFamily:"SC", description:"Swap devices must be encrypted.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"swapDevices", op:"==", value:"ENCRYPTED_SWAP_DEVICES" }], rationale:"V-268272 — Swap devices must be encrypted. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"swapon --show && cryptsetup status swap", expect:"active" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-fips-mode-enabled", lineageId:"stig-fips-mode-enabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268274", srgIds:["SRG-OS-000033","SRG-OS-000189"], cciIds:["CCI-000068","CCI-001871"], name:"stig-fips-mode-enabled", category:"security", controlFamily:"SC", description:"The system must operate in FIPS 140-3 mode.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"boot.kernelParams", op:"==", value:"[ \"fips=1\" ]" }], rationale:"V-268274 — The system must operate in FIPS 140-3 mode. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"cat /proc/sys/crypto/fips_enabled", expect:"1" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-fips-openssl-provider", lineageId:"stig-fips-openssl-provider", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268276", srgIds:["SRG-OS-000033","SRG-OS-000196"], cciIds:["CCI-000068","CCI-001884"], name:"stig-fips-openssl-provider", category:"security", controlFamily:"SC", description:"OpenSSL must load the FIPS provider.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"environment.etc.\"ssl/openssl.cnf\".text", op:"==", value:"OPENSSL_FIPS_CONF" }], rationale:"V-268276 — OpenSSL must load the FIPS provider. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"openssl list -providers | grep -A1 fips", expect:"status: active" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-tls-min-version-1-2", lineageId:"stig-tls-min-version-1-2", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268278", srgIds:["SRG-OS-000033","SRG-OS-000203"], cciIds:["CCI-000068","CCI-001897"], name:"stig-tls-min-version-1-2", category:"security", controlFamily:"SC", description:"TLS endpoints must not negotiate below TLS 1.2.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.nginx.sslProtocols", op:"==", value:"\"TLSv1.2 TLSv1.3\"" }], rationale:"V-268278 — TLS endpoints must not negotiate below TLS 1.2. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"openssl s_client -tls1_1 -connect localhost:443", expect:"handshake failure" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-tls-approved-cipher-suites", lineageId:"stig-tls-approved-cipher-suites", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268280", srgIds:["SRG-OS-000033","SRG-OS-000210"], cciIds:["CCI-000068","CCI-001910"], name:"stig-tls-approved-cipher-suites", category:"security", controlFamily:"SC", description:"TLS endpoints must offer only approved cipher suites.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.nginx.sslCiphers", op:"==", value:"APPROVED_TLS_CIPHERS" }], rationale:"V-268280 — TLS endpoints must offer only approved cipher suites. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"nmap --script ssl-enum-ciphers -p 443 localhost", expect:"approved suites only" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-cert-validation-enforced", lineageId:"stig-cert-validation-enforced", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268282", srgIds:["SRG-OS-000069","SRG-OS-000217"], cciIds:["CCI-000192","CCI-001923"], name:"stig-cert-validation-enforced", category:"security", controlFamily:"IA", description:"Certificate chains must be validated against the DoD PKI trust store.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.pki.certificateFiles", op:"==", value:"[ DOD_ROOT_CA_BUNDLE ]" }], rationale:"V-268282 — Certificate chains must be validated against the DoD PKI trust store. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"openssl verify -CAfile /etc/ssl/certs/dod-root.pem host.pem", expect:"OK" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-cert-revocation-checking", lineageId:"stig-cert-revocation-checking", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268284", srgIds:["SRG-OS-000069","SRG-OS-000224"], cciIds:["CCI-000192","CCI-001936"], name:"stig-cert-revocation-checking", category:"security", controlFamily:"IA", description:"Certificate revocation must be checked via OCSP.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.nginx.sslOcspStapling", op:"==", value:"true" }], rationale:"V-268284 — Certificate revocation must be checked via OCSP. SRG-OS-000069 / CCI-000192.", evidence:[{ kind:"command", cmd:"openssl s_client -status -connect localhost:443", expect:"OCSP Response Status: successful" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-key-rotation-90-days", lineageId:"stig-key-rotation-90-days", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268286", srgIds:["SRG-OS-000033","SRG-OS-000231"], cciIds:["CCI-000068","CCI-001949"], name:"stig-key-rotation-90-days", category:"security", controlFamily:"SC", description:"Service signing keys must be rotated at least every 90 days.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.crystalForge.keyRotationDays", op:"==", value:"90" }], rationale:"V-268286 — Service signing keys must be rotated at least every 90 days. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"cf-keys age --max 90d", expect:"within policy" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-secrets-not-in-nix-store", lineageId:"stig-secrets-not-in-nix-store", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268288", srgIds:["SRG-OS-000033","SRG-OS-000238"], cciIds:["CCI-000068","CCI-001962"], name:"stig-secrets-not-in-nix-store", category:"security", controlFamily:"SC", description:"Secrets must not be written to the world-readable Nix store.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"sops.age.keyFile", op:"==", value:"\"/var/lib/sops/age.key\"" }], rationale:"V-268288 — Secrets must not be written to the world-readable Nix store. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"grep -rl 'BEGIN PRIVATE KEY' /nix/store", expect:"(empty)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-usbguard-enabled", lineageId:"stig-usbguard-enabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268290", srgIds:["SRG-OS-000480","SRG-OS-000245"], cciIds:["CCI-000366","CCI-001975"], name:"stig-usbguard-enabled", category:"security", controlFamily:"MP", description:"USB device authorization must be enforced by USBGuard.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.usbguard.enable", op:"==", value:"true" }], rationale:"V-268290 — USB device authorization must be enforced by USBGuard. SRG-OS-000480 / CCI-000366.", evidence:[{ kind:"command", cmd:"systemctl is-active usbguard", expect:"active" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-usbguard-default-block", lineageId:"stig-usbguard-default-block", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268292", srgIds:["SRG-OS-000480","SRG-OS-000252"], cciIds:["CCI-000366","CCI-001988"], name:"stig-usbguard-default-block", category:"security", controlFamily:"MP", description:"USBGuard must block devices absent an allow rule.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.usbguard.implicitPolicyTarget", op:"==", value:"\"block\"" }], rationale:"V-268292 — USBGuard must block devices absent an allow rule. SRG-OS-000480 / CCI-000366.", evidence:[{ kind:"command", cmd:"usbguard get-parameter ImplicitPolicyTarget", expect:"block" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-usb-storage-module-blacklisted", lineageId:"stig-usb-storage-module-blacklisted", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268294", srgIds:["SRG-OS-000480","SRG-OS-000259"], cciIds:["CCI-000366","CCI-002001"], name:"stig-usb-storage-module-blacklisted", category:"security", controlFamily:"MP", description:"The usb-storage kernel module must be disabled.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.blacklistedKernelModules", op:"==", value:"[ \"usb-storage\" ]" }], rationale:"V-268294 — The usb-storage kernel module must be disabled. SRG-OS-000480 / CCI-000366.", evidence:[{ kind:"command", cmd:"lsmod | grep usb_storage", expect:"(empty)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-bluetooth-disabled", lineageId:"stig-bluetooth-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268296", srgIds:["SRG-OS-000095","SRG-OS-000266"], cciIds:["CCI-000381","CCI-002014"], name:"stig-bluetooth-disabled", category:"security", controlFamily:"CM", description:"The Bluetooth radio must be disabled.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"hardware.bluetooth.enable", op:"==", value:"false" }], rationale:"V-268296 — The Bluetooth radio must be disabled. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"systemctl is-enabled bluetooth", expect:"masked" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-wireless-interfaces-disabled", lineageId:"stig-wireless-interfaces-disabled", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268298", srgIds:["SRG-OS-000095","SRG-OS-000273"], cciIds:["CCI-000381","CCI-002027"], name:"stig-wireless-interfaces-disabled", category:"security", controlFamily:"CM", description:"Wireless interfaces must be disabled on wired-only hosts.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"networking.wireless.enable", op:"==", value:"false" }], rationale:"V-268298 — Wireless interfaces must be disabled on wired-only hosts. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"ip link show type wlan", expect:"(empty)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-firewire-module-blacklisted", lineageId:"stig-firewire-module-blacklisted", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268300", srgIds:["SRG-OS-000480","SRG-OS-000280"], cciIds:["CCI-000366","CCI-002040"], name:"stig-firewire-module-blacklisted", category:"security", controlFamily:"MP", description:"FireWire and Thunderbolt DMA modules must be disabled.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"boot.blacklistedKernelModules", op:"==", value:"[ \"firewire-core\" \"thunderbolt\" ]" }], rationale:"V-268300 — FireWire and Thunderbolt DMA modules must be disabled. SRG-OS-000480 / CCI-000366.", evidence:[{ kind:"command", cmd:"lsmod | grep -E 'firewire|thunderbolt'", expect:"(empty)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-firewall-default-deny", lineageId:"stig-firewall-default-deny", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268302", srgIds:["SRG-OS-000033","SRG-OS-000287"], cciIds:["CCI-000068","CCI-002053"], name:"stig-firewall-default-deny", category:"security", controlFamily:"SC", description:"The host firewall must default to deny for inbound traffic.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"networking.firewall.enable", op:"==", value:"true" }], rationale:"V-268302 — The host firewall must default to deny for inbound traffic. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"nft list ruleset | grep policy", expect:"policy drop" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-firewall-allowed-ports-explicit", lineageId:"stig-firewall-allowed-ports-explicit", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268304", srgIds:["SRG-OS-000095","SRG-OS-000294"], cciIds:["CCI-000381","CCI-002066"], name:"stig-firewall-allowed-ports-explicit", category:"security", controlFamily:"CM", description:"Only explicitly approved inbound ports may be open.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"networking.firewall.allowedTCPPorts", op:"==", value:"[ 22 443 ]" }], rationale:"V-268304 — Only explicitly approved inbound ports may be open. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"ss -tlnp | awk '{print $4}'", expect:"22, 443 only" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-telnet-service-absent", lineageId:"stig-telnet-service-absent", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268306", srgIds:["SRG-OS-000095","SRG-OS-000301"], cciIds:["CCI-000381","CCI-002079"], name:"stig-telnet-service-absent", category:"security", controlFamily:"CM", description:"Telnet server packages must not be installed.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"services.telnet.enable", op:"==", value:"false" }], rationale:"V-268306 — Telnet server packages must not be installed. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"which telnetd", expect:"not found" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-ftp-service-absent", lineageId:"stig-ftp-service-absent", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268308", srgIds:["SRG-OS-000095","SRG-OS-000308"], cciIds:["CCI-000381","CCI-002092"], name:"stig-ftp-service-absent", category:"security", controlFamily:"CM", description:"FTP server packages must not be installed.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.vsftpd.enable", op:"==", value:"false" }], rationale:"V-268308 — FTP server packages must not be installed. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"systemctl is-enabled vsftpd", expect:"not-found" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-nfs-exports-restricted", lineageId:"stig-nfs-exports-restricted", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268310", srgIds:["SRG-OS-000021","SRG-OS-000315"], cciIds:["CCI-000044","CCI-002105"], name:"stig-nfs-exports-restricted", category:"security", controlFamily:"AC", description:"NFS exports must not use the no_root_squash option.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.nfs.server.exports", op:"==", value:"NFS_EXPORT_RULES" }], rationale:"V-268310 — NFS exports must not use the no_root_squash option. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"grep no_root_squash /etc/exports", expect:"(no matches)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-snmp-v3-only", lineageId:"stig-snmp-v3-only", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268312", srgIds:["SRG-OS-000033","SRG-OS-000322"], cciIds:["CCI-000068","CCI-002118"], name:"stig-snmp-v3-only", category:"security", controlFamily:"SC", description:"SNMP must not use community strings v1 or v2c.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.snmpd.extraConfig", op:"==", value:"SNMPV3_ONLY_CONF" }], rationale:"V-268312 — SNMP must not use community strings v1 or v2c. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"grep -E '^com2sec' /etc/snmp/snmpd.conf", expect:"(no matches)" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-mail-relay-local-only", lineageId:"stig-mail-relay-local-only", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268314", srgIds:["SRG-OS-000033","SRG-OS-000329"], cciIds:["CCI-000068","CCI-002131"], name:"stig-mail-relay-local-only", category:"security", controlFamily:"SC", description:"The mail transfer agent must accept connections only from localhost.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.postfix.config.inet_interfaces", op:"==", value:"\"loopback-only\"" }], rationale:"V-268314 — The mail transfer agent must accept connections only from localhost. SRG-OS-000033 / CCI-000068.", evidence:[{ kind:"command", cmd:"postconf inet_interfaces", expect:"loopback-only" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-vnc-service-absent", lineageId:"stig-vnc-service-absent", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268316", srgIds:["SRG-OS-000095","SRG-OS-000336"], cciIds:["CCI-000381","CCI-002144"], name:"stig-vnc-service-absent", category:"security", controlFamily:"CM", description:"Remote desktop services must not be enabled.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.xrdp.enable", op:"==", value:"false" }], rationale:"V-268316 — Remote desktop services must not be enabled. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"systemctl is-enabled xrdp", expect:"not-found" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-cron-restricted-to-root", lineageId:"stig-cron-restricted-to-root", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268318", srgIds:["SRG-OS-000021","SRG-OS-000343"], cciIds:["CCI-000044","CCI-002157"], name:"stig-cron-restricted-to-root", category:"security", controlFamily:"AC", description:"Scheduled job creation must be restricted to authorized users.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.cron.extraConfig", op:"==", value:"CRON_ALLOW_ROOT" }], rationale:"V-268318 — Scheduled job creation must be restricted to authorized users. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"cat /etc/cron.allow", expect:"root" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-container-runtime-rootless", lineageId:"stig-container-runtime-rootless", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268320", srgIds:["SRG-OS-000095","SRG-OS-000350"], cciIds:["CCI-000381","CCI-002170"], name:"stig-container-runtime-rootless", category:"security", controlFamily:"CM", description:"The container runtime must run without root privileges.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"virtualisation.podman.enable", op:"==", value:"true" }], rationale:"V-268320 — The container runtime must run without root privileges. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"podman info | grep rootless", expect:"rootless: true" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-container-images-signed", lineageId:"stig-container-images-signed", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268322", srgIds:["SRG-OS-000191","SRG-OS-000357"], cciIds:["CCI-001240","CCI-002183"], name:"stig-container-images-signed", category:"security", controlFamily:"SI", description:"Container images must carry a verified signature.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"virtualisation.containers.policy", op:"==", value:"SIGSTORE_POLICY" }], rationale:"V-268322 — Container images must carry a verified signature. SRG-OS-000191 / CCI-001240.", evidence:[{ kind:"command", cmd:"cosign verify --key cf.pub $IMAGE", expect:"verified" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-journald-forward-to-syslog", lineageId:"stig-journald-forward-to-syslog", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268324", srgIds:["SRG-OS-000062","SRG-OS-000364"], cciIds:["CCI-000169","CCI-002196"], name:"stig-journald-forward-to-syslog", category:"security", controlFamily:"AU", description:"Journal records must be forwarded to the central syslog host.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.journald.extraConfig", op:"==", value:"\"ForwardToSyslog=yes\"" }], rationale:"V-268324 — Journal records must be forwarded to the central syslog host. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"grep ForwardToSyslog /etc/systemd/journald.conf", expect:"ForwardToSyslog=yes" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-journald-storage-persistent", lineageId:"stig-journald-storage-persistent", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268326", srgIds:["SRG-OS-000062","SRG-OS-000371"], cciIds:["CCI-000169","CCI-002209"], name:"stig-journald-storage-persistent", category:"security", controlFamily:"AU", description:"Journal storage must be persistent across reboots.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.journald.storage", op:"==", value:"\"persistent\"" }], rationale:"V-268326 — Journal storage must be persistent across reboots. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"grep Storage /etc/systemd/journald.conf", expect:"Storage=persistent" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-logrotate-configured", lineageId:"stig-logrotate-configured", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268328", srgIds:["SRG-OS-000062","SRG-OS-000378"], cciIds:["CCI-000169","CCI-002222"], name:"stig-logrotate-configured", category:"security", controlFamily:"AU", description:"Log rotation must be configured to prevent disk exhaustion.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"nixos_option", path:"services.logrotate.enable", op:"==", value:"true" }], rationale:"V-268328 — Log rotation must be configured to prevent disk exhaustion. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"systemctl is-enabled logrotate.timer", expect:"enabled" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-time-sync-authorized-source", lineageId:"stig-time-sync-authorized-source", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268330", srgIds:["SRG-OS-000062","SRG-OS-000385"], cciIds:["CCI-000169","CCI-002235"], name:"stig-time-sync-authorized-source", category:"security", controlFamily:"AU", description:"System time must be synchronized to an authorized time source.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"services.chrony.servers", op:"==", value:"AUTHORIZED_NTP_SERVERS" }], rationale:"V-268330 — System time must be synchronized to an authorized time source. SRG-OS-000062 / CCI-000169.", evidence:[{ kind:"command", cmd:"chronyc sources | head -3", expect:"^* tick.usno.navy.mil" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-package-signature-verification", lineageId:"stig-package-signature-verification", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268332", srgIds:["SRG-OS-000191","SRG-OS-000392"], cciIds:["CCI-001240","CCI-002248"], name:"stig-package-signature-verification", category:"security", controlFamily:"SI", description:"Packages and binary caches must be signature-verified before use.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"nix.settings.require-sigs", op:"==", value:"true" }], rationale:"V-268332 — Packages and binary caches must be signature-verified before use. SRG-OS-000191 / CCI-001240.", evidence:[{ kind:"command", cmd:"nix show-config | grep require-sigs", expect:"require-sigs = true" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-update-cadence-30-days", lineageId:"stig-update-cadence-30-days", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268334", srgIds:["SRG-OS-000191","SRG-OS-000399"], cciIds:["CCI-001240","CCI-002261"], name:"stig-update-cadence-30-days", category:"security", controlFamily:"SI", description:"Security updates must be applied within 30 days of release.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"system.autoUpgrade.enable", op:"==", value:"true" }], rationale:"V-268334 — Security updates must be applied within 30 days of release. SRG-OS-000191 / CCI-001240.", evidence:[{ kind:"command", cmd:"cf-fleet drift --max-age 30d", expect:"within policy" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-boot-loader-password", lineageId:"stig-boot-loader-password", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268336", srgIds:["SRG-OS-000095","SRG-OS-000406"], cciIds:["CCI-000381","CCI-002274"], name:"stig-boot-loader-password", category:"security", controlFamily:"CM", description:"The boot loader configuration must require authentication to modify.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.loader.grub.users.admin.hashedPasswordFile", op:"==", value:"\"/etc/grub-pw\"" }], rationale:"V-268336 — The boot loader configuration must require authentication to modify. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"grep -c password_pbkdf2 /boot/grub/grub.cfg", expect:"1" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
  { id:"stig-secure-boot-enforced", lineageId:"stig-secure-boot-enforced", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268338", srgIds:["SRG-OS-000191","SRG-OS-000413"], cciIds:["CCI-001240","CCI-002287"], name:"stig-secure-boot-enforced", category:"security", controlFamily:"SI", description:"UEFI Secure Boot must be enabled and enforcing.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"nixos_option", path:"boot.lanzaboote.enable", op:"==", value:"true" }], rationale:"V-268338 — UEFI Secure Boot must be enabled and enforcing. SRG-OS-000191 / CCI-001240.", evidence:[{ kind:"command", cmd:"bootctl status | grep 'Secure Boot'", expect:"enabled" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"6d ago", framework:"DISA STIG" },
  { id:"stig-tpm-measured-boot", lineageId:"stig-tpm-measured-boot", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268340", srgIds:["SRG-OS-000191","SRG-OS-000420"], cciIds:["CCI-001240","CCI-002300"], name:"stig-tpm-measured-boot", category:"security", controlFamily:"SI", description:"Boot measurements must be sealed to the TPM.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"boot.initrd.systemd.tpm2.enable", op:"==", value:"true" }], rationale:"V-268340 — Boot measurements must be sealed to the TPM. SRG-OS-000191 / CCI-001240.", evidence:[{ kind:"command", cmd:"tpm2_pcrread sha256:7", expect:"PCR 7 sealed" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"3w ago", framework:"DISA STIG" },
  { id:"stig-selinux-or-apparmor-enforcing", lineageId:"stig-selinux-or-apparmor-enforcing", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268342", srgIds:["SRG-OS-000021","SRG-OS-000427"], cciIds:["CCI-000044","CCI-002313"], name:"stig-selinux-or-apparmor-enforcing", category:"security", controlFamily:"AC", description:"A mandatory access control framework must be enforcing.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"security.apparmor.enable", op:"==", value:"true" }], rationale:"V-268342 — A mandatory access control framework must be enforcing. SRG-OS-000021 / CCI-000044.", evidence:[{ kind:"command", cmd:"aa-status | grep 'profiles are in enforce'", expect:"enforce mode" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"1mo ago", framework:"DISA STIG" },
  { id:"stig-systemd-service-sandboxing", lineageId:"stig-systemd-service-sandboxing", revision:1, publicationState:"current", publishedDate:"2026-04-12", stigId:"V-268344", srgIds:["SRG-OS-000095","SRG-OS-000434"], cciIds:["CCI-000381","CCI-002326"], name:"stig-systemd-service-sandboxing", category:"security", controlFamily:"CM", description:"Network-facing services must run with systemd sandboxing directives.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"nixos_option", path:"systemd.services.nginx.serviceConfig", op:"==", value:"SANDBOX_DIRECTIVES" }], rationale:"V-268344 — Network-facing services must run with systemd sandboxing directives. SRG-OS-000095 / CCI-000381.", evidence:[{ kind:"command", cmd:"systemd-analyze security nginx.service", expect:"exposure < 3.0" }], createdBy:"security-team", createdAt:"5mo ago", lastModified:"2mo ago", framework:"DISA STIG" },
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

// ISO 9001:2015 quality-management controls — the process half of a bundle assignment.
// Same shape as the STIG controls, different framework, so a system can carry both.
const POLICY_ISO_QMS = [
  { id:"iso-documented-information", lineageId:"iso-documented-information", revision:1, publicationState:"current", publishedDate:"2026-03-02", framework:"ISO 9001", clause:"7.5.3", clauseName:"Documented information", name:"iso-documented-information", category:"quality", description:"Every deployed configuration must be traceable to a controlled document revision in the flake.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.crystalForge.qms.\"7.5.3\".satisfied == true", message:"flake revision recorded for each generation" }], rationale:"ISO 9001:2015 clause 7.5.3 (Documented information).", evidence:[{ kind:"command", cmd:"cf-fleet generations --with-commit", expect:"every generation maps to a signed commit" }], createdBy:"quality-team", createdAt:"6mo ago", lastModified:"2w ago" },
  { id:"iso-change-authorization", lineageId:"iso-change-authorization", revision:1, publicationState:"current", publishedDate:"2026-03-02", framework:"ISO 9001", clause:"8.5.6", clauseLabel:"8.5.6a", clauseName:"Control of changes", name:"iso-change-authorization", category:"quality", description:"Changes to a production configuration must carry documented authorization before release.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"custom_eval", expr:"config.crystalForge.qms.\"8.5.6\".satisfied == true", message:"two distinct approvers recorded on the deploy" }], rationale:"ISO 9001:2015 clause 8.5.6 (Control of changes).", evidence:[{ kind:"command", cmd:"cf-audit deploys --require-approvals 2", expect:"all deploys carry 2 approvals" }], createdBy:"quality-team", createdAt:"6mo ago", lastModified:"2w ago" },
  { id:"iso-change-records", lineageId:"iso-change-records", revision:1, publicationState:"current", publishedDate:"2026-03-02", framework:"ISO 9001", clause:"8.5.6", clauseLabel:"8.5.6b", clauseName:"Control of changes", name:"iso-change-records", category:"quality", description:"Records of each change — what changed, who authorized it, and the result — must be retained.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.crystalForge.qms.\"8.5.6\".satisfied == true", message:"deploy record retained with diff and outcome" }], rationale:"ISO 9001:2015 clause 8.5.6 (Control of changes).", evidence:[{ kind:"command", cmd:"cf-audit export --since 12mo | jq 'length'", expect:"complete record set" }], createdBy:"quality-team", createdAt:"6mo ago", lastModified:"2w ago" },
  { id:"iso-internal-audit-cadence", lineageId:"iso-internal-audit-cadence", revision:1, publicationState:"current", publishedDate:"2026-03-02", framework:"ISO 9001", clause:"9.2", clauseName:"Internal audit", name:"iso-internal-audit-cadence", category:"quality", description:"Configuration compliance must be audited at planned intervals not exceeding 90 days.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.crystalForge.qms.\"9.2\".satisfied == true", message:"last full evaluation within 90 days" }], rationale:"ISO 9001:2015 clause 9.2 (Internal audit).", evidence:[{ kind:"command", cmd:"cf-fleet audit-age --max 90d", expect:"within interval" }], createdBy:"quality-team", createdAt:"6mo ago", lastModified:"2w ago" },
  { id:"iso-corrective-action", lineageId:"iso-corrective-action", revision:1, publicationState:"current", publishedDate:"2026-03-02", framework:"ISO 9001", clause:"10.2", clauseName:"Nonconformity and corrective action", name:"iso-corrective-action", category:"quality", description:"Each nonconformity must have a corrective action with an owner and a target date.", type:"custom", severity:"high", enabled:true, rules:[{ kind:"custom_eval", expr:"config.crystalForge.qms.\"10.2\".satisfied == true", message:"every failing control has an owned remediation plan" }], rationale:"ISO 9001:2015 clause 10.2 (Nonconformity and corrective action).", evidence:[{ kind:"command", cmd:"cf-poam list --unowned", expect:"(empty)" }], createdBy:"quality-team", createdAt:"6mo ago", lastModified:"2w ago" },
  { id:"iso-external-provider-control", lineageId:"iso-external-provider-control", revision:1, publicationState:"current", publishedDate:"2026-03-02", framework:"ISO 9001", clause:"8.4", clauseName:"Control of externally provided processes", name:"iso-external-provider-control", category:"quality", description:"Externally provided packages must be identified and verified before use.", type:"custom", severity:"medium", enabled:true, rules:[{ kind:"custom_eval", expr:"config.crystalForge.qms.\"8.4\".satisfied == true", message:"SBOM generated and inputs pinned in flake.lock" }], rationale:"ISO 9001:2015 clause 8.4 (Control of externally provided processes).", evidence:[{ kind:"command", cmd:"cf-sbom verify --closure /run/current-system", expect:"all inputs attested" }], createdBy:"quality-team", createdAt:"6mo ago", lastModified:"2w ago" },
  { id:"iso-measurement-traceability", lineageId:"iso-measurement-traceability", revision:1, publicationState:"current", publishedDate:"2026-03-02", framework:"ISO 9001", clause:"7.1.5", clauseName:"Monitoring and measuring resources", name:"iso-measurement-traceability", category:"quality", description:"Measurement records must carry a traceable timestamp from an authorized source.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.crystalForge.qms.\"7.1.5\".satisfied == true", message:"host clock synchronized to an authorized source" }], rationale:"ISO 9001:2015 clause 7.1.5 (Monitoring and measuring resources).", evidence:[{ kind:"command", cmd:"chronyc tracking | grep 'Leap status'", expect:"Normal" }], createdBy:"quality-team", createdAt:"6mo ago", lastModified:"2w ago" },
  { id:"iso-competence-records", lineageId:"iso-competence-records", revision:1, publicationState:"current", publishedDate:"2026-03-02", framework:"ISO 9001", clause:"7.2", clauseName:"Competence", name:"iso-competence-records", category:"quality", description:"Operators authorized to deploy must have current competence records on file.", type:"custom", severity:"low", enabled:true, rules:[{ kind:"custom_eval", expr:"config.crystalForge.qms.\"7.2\".satisfied == true", message:"deploy role membership reviewed within 12 months" }], rationale:"ISO 9001:2015 clause 7.2 (Competence).", evidence:[{ kind:"command", cmd:"cf-admin roles --review-age", expect:"reviewed within 12mo" }], createdBy:"quality-team", createdAt:"6mo ago", lastModified:"2w ago" },
];

const POLICIES = (typeof __fx === "function" && __fx("policies")) || [...POLICY_BUILTIN, ...POLICY_CUSTOM, ...POLICY_EDITOR_DEMO, ...POLICY_STIG_MOCK, ...POLICY_STIG_BULK, ...POLICY_ISO_QMS];

// Per-policy usage rollup
function policyUsage(policyId) {
  const systems = (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).filter(s => s.deploymentPolicy === policyId);
  const byEnv = {};
  systems.forEach(s => { byEnv[s.environment] = (byEnv[s.environment] || 0) + 1; });
  return { systems, count: systems.length, byEnv };
}

Object.assign(window, { POLICIES, POLICY_BUILTIN, POLICY_CUSTOM, POLICY_CATEGORIES, POLICY_DOMAINS, CONTROL_FAMILIES, GROUPING_SCHEMES, policyCategoryMeta, policyDomain, policyUsage, groupPoliciesByLineage, loadCustomGroupingSchemes, saveCustomGroupingSchemes, srgCategoryOf, cmmcLevelOf, remediationStatusOf, BUILTIN_FRAMEWORKS, loadCustomFrameworks, saveCustomFrameworks, allFrameworkOptions, FRAMEWORK_ID_FIELDS });
