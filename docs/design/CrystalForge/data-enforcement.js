// Enforcement vocabulary + NixOS option metadata.
//
// Two ideas live here, and they're deliberately separate from compliance:
//   ENFORCEMENT_TYPES — what Crystal Forge can assert, require, prohibit, or gate.
//   NIXOS_OPTIONS     — type metadata for known NixOS options, so the value editor can
//                       pick itself instead of asking the user what kind of value they mean.

const ENFORCEMENT_GROUPS = [
  { id:"config",   label:"System & configuration", blurb:"Assert what the evaluated NixOS config must contain." },
  { id:"supply",   label:"Vulnerability & supply chain", blurb:"Gate on scan results and package provenance." },
  { id:"pipeline", label:"Pipeline & build", blurb:"Require pipeline outcomes before a config can be promoted." },
  { id:"rollout",  label:"Rollout & approval", blurb:"Govern who approves a rollout and when it may run." },
];

const ENFORCEMENT_TYPES = [
  { kind:"nixos_option",       group:"config",   label:"NixOS option value",     blurb:"Assert a config option equals (or doesn't equal) an exact value.", icon:"file" },
  { kind:"packages_installed", group:"config",   label:"Package required",       blurb:"Assert packages are present in the system closure.", icon:"cube" },
  { kind:"packages_absent",    group:"config",   label:"Package prohibited",     blurb:"Assert packages are NOT present in the system closure.", icon:"cube" },
  { kind:"custom_eval",        group:"config",   label:"Custom nix assertion",   blurb:"Any nix expression that must evaluate to true.", icon:"terminal" },
  { kind:"cve_block",          group:"supply",   label:"CVE threshold",          blurb:"Cap how many CVEs of a severity may be present.", icon:"warn" },
  { kind:"eval_passed",        group:"pipeline", label:"Evaluation must pass",   blurb:"The flake must evaluate cleanly.", icon:"check" },
  { kind:"pin_required",       group:"pipeline", label:"Pinned commit required", blurb:"Only a pinned flake revision may deploy.", icon:"key" },
  { kind:"time_window",        group:"rollout",  label:"Deploy window",          blurb:"Restrict deploys to given days and hours.", icon:"history" },
  { kind:"approval_required",  group:"rollout",  label:"Approval required",      blurb:"Require named approvers before rollout.", icon:"check" },
  { kind:"rollout_percent",    group:"rollout",  label:"Canary rollout",         blurb:"Stage the rollout in batches with an observation window.", icon:"sync" },
];

// Category → the enforcement types most likely to be right. Guidance only: every type
// stays reachable from "More enforcement types", and nothing is ever blocked.
const ENFORCEMENT_RECOMMENDED = {
  security:   ["nixos_option", "packages_absent", "cve_block", "custom_eval"],
  deployment: ["eval_passed", "approval_required", "time_window"],
  pipeline:   ["eval_passed", "pin_required", "cve_block"],
  rollout:    ["rollout_percent", "time_window", "approval_required"],
};

function enforcementMeta(kind) {
  return ENFORCEMENT_TYPES.find(t => t.kind === kind) || { kind, group:"config", label:kind, blurb:"", icon:"file" };
}
function recommendedEnforcement(category) {
  return ENFORCEMENT_RECOMMENDED[category] || ENFORCEMENT_RECOMMENDED.deployment;
}

const DOD_CONSENT_BANNER = `You are accessing a U.S. Government (USG) Information System (IS) that is provided for USG-authorized use only.

By using this IS (which includes any device attached to this IS), you consent to the following conditions:

-The USG routinely intercepts and monitors communications on this IS for purposes including, but not limited to, penetration testing, COMSEC monitoring, network operations and defense, personnel misconduct (PM), law enforcement (LE), and counterintelligence (CI) investigations.

-At any time, the USG may inspect and seize data stored on this IS.

-Communications using, or data stored on, this IS are not private, are subject to routine monitoring, interception, and search, and may be disclosed or used for any USG-authorized purpose.

-This IS includes security measures (e.g., authentication and access controls) to protect USG interests--not for your personal benefit or privacy.

-Notwithstanding the above, using this IS does not constitute consent to PM, LE or CI investigative searching or monitoring of the content of privileged communications, or work product, related to personal representation or services by attorneys, psychotherapists, or clergy, and their assistants. Such communications and work product are private and confidential. See User Agreement for details.`;

// Known NixOS option types. `type` drives which value editor appears; `values` supplies
// the real allowed values for enums (note PermitRootLogin's yes/no are enum members, not
// booleans — they are not interchangeable with true/false).
const NIXOS_OPTIONS = {
  "services.openssh.enable":                        { type:"boolean", desc:"Whether to enable the OpenSSH secure shell daemon." },
  "networking.firewall.enable":                      { type:"boolean", desc:"Whether to enable the nftables-based firewall." },
  "security.auditd.enable":                          { type:"boolean", desc:"Whether to enable the audit daemon." },
  "services.usbguard.enable":                        { type:"boolean", desc:"Whether to enable USBGuard peripheral control." },
  "security.sudo.execWheelOnly":                     { type:"boolean", desc:"Restrict sudo execution to members of the wheel group." },
  "services.openssh.settings.PermitRootLogin":       { type:"enum", values:["yes","without-password","prohibit-password","forced-commands-only","no"], desc:"Whether the root user may log in over SSH." },
  "services.openssh.settings.PasswordAuthentication":{ type:"enum", values:["yes","no"], desc:"Whether password authentication is offered." },
  "services.openssh.settings.ClientAliveInterval":   { type:"int", unit:"seconds", desc:"Idle seconds before the server probes the client." },
  "services.openssh.settings.ClientAliveCountMax":   { type:"int", desc:"Unanswered probes tolerated before disconnect." },
  "boot.kernel.sysctl.\"kernel.randomize_va_space\"":{ type:"int", desc:"Address-space layout randomization level." },
  "security.pam.services.login.failDelay":           { type:"int", unit:"microseconds", desc:"Delay after a failed login attempt." },
  "users.users.root.hashedPassword":                 { type:"str", desc:"Hashed root password, or ! to disable." },
  "system.nixos.label":                              { type:"str", desc:"Label shown in the boot menu." },
  "environment.etc.\"issue\".text":                  { type:"lines", desc:"Contents of /etc/issue — the pre-login banner shown on every console and SSH session." },
  "users.motd":                                      { type:"lines", desc:"Message of the day, shown after login." },
  "security.pam.services.sshd.text":                 { type:"lines", desc:"Raw PAM stack for sshd." },
  "services.openssh.extraConfig":                    { type:"lines", desc:"Verbatim sshd_config fragment appended to the generated file." },
};
const NIXOS_OPTION_PATHS = Object.keys(NIXOS_OPTIONS);

function nixosOptionMeta(path) {
  return NIXOS_OPTIONS[path] || { type: "unknown", desc: "" };
}

// Semantic value <-> nix literal. The editor always holds the semantic value; users never
// type quotes, escapes, or ''…'' blocks.
function nixLiteral(value, type) {
  if (type === "boolean") return value === true || value === "true" ? "true" : "false";
  if (type === "int") return String(value ?? 0);
  if (type === "lines" || (typeof value === "string" && value.includes("\n"))) return "''\n" + String(value ?? "") + "\n''";
  return JSON.stringify(String(value ?? ""));
}
// Older stored rules kept nix literals in `value` — normalize once on load.
function semanticValue(raw, type) {
  if (typeof raw !== "string") return raw;
  let v = raw.trim();
  if (type === "boolean") return v === "true";
  if (v.startsWith("''") && v.endsWith("''")) return v.slice(2, -2).replace(/^\n/, "").replace(/\n$/, "");
  if (v.length > 1 && v.startsWith('"') && v.endsWith('"')) { try { return JSON.parse(v); } catch { return v.slice(1,-1); } }
  return v;
}
// Compact one-line summary of a value, whatever its size.
function valueSummary(value, type, max = 46) {
  if (type === "boolean") return value === true || value === "true" ? "True" : "False";
  const s = String(value ?? "");
  if (s.length <= max && !s.includes("\n")) return s;
  const head = s.replace(/\s+/g, " ").slice(0, max).trim();
  return `“${head}…” · ${s.length.toLocaleString()} characters`;
}

Object.assign(window, {
  ENFORCEMENT_GROUPS, ENFORCEMENT_TYPES, ENFORCEMENT_RECOMMENDED, enforcementMeta, recommendedEnforcement,
  NIXOS_OPTIONS, NIXOS_OPTION_PATHS, nixosOptionMeta, nixLiteral, semanticValue, valueSummary, DOD_CONSENT_BANNER,
});
