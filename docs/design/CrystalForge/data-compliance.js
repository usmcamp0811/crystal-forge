// Compliance bundles — policies of policies for STIG / NIST / CMMC / custom

const COMPLIANCE_BUNDLES = (typeof __fx === "function" && __fx("compliance")) || [
  {
    id: "disa-rhel9-stig",
    name: "Anduril NixOS STIG (v1r2)",
    framework: "DISA STIG",
    version: "NixOS v1r2",
    description: "Anduril NixOS Security Technical Implementation Guide — operating system controls.",
    layer: "system",
    owner: "security-team",
    lastReview: "2026-04-12",
    policyIds: ["stig-ssh", "stig-auditd", "stig-banner", "stig-usbguard", "stig-pwquality", "stig-fips", "cve-gated"],
    requiredEnvs: ["production","staging"],
  },
  {
    id: "disa-app-stig",
    name: "DISA Application Security STIG",
    framework: "DISA STIG",
    version: "v6r1",
    description: "Application-layer STIG: TLS, auth, logging, secrets rotation.",
    layer: "application",
    owner: "security-team",
    lastReview: "2026-03-18",
    policyIds: ["tls_min_version","two-approver","cve-gated","log_remote_forward"],
    requiredEnvs: ["production"],
  },
  {
    id: "nist-800-53-mod",
    name: "NIST 800-53 Moderate",
    framework: "NIST 800-53",
    version: "Rev 5",
    description: "FedRAMP Moderate baseline subset — access control, audit, configuration management.",
    layer: "system",
    owner: "compliance-team",
    lastReview: "2026-02-04",
    policyIds: ["sshd_hardening","audit_rules","banner","pam_faillock","session_lockout","time_sync","cve-gated"],
    requiredEnvs: ["production","staging","edge"],
  },
  {
    id: "internal-prod-baseline",
    name: "Internal Production Baseline",
    framework: "Internal",
    version: "v2.4",
    description: "Crystal Forge organization minimum bar for production hosts.",
    layer: "system",
    owner: "ops-team",
    lastReview: "2026-05-01",
    policyIds: ["sshd_hardening","firewall_default_deny","audit_rules","cve-gated","two-approver","business-hours"],
    requiredEnvs: ["production"],
  },
];

// Evidence types Crystal Forge can collect for a (system, policy) pair
const EVIDENCE_TYPES = {
  config:        { label:"Rendered config",      icon:"file",     desc:"NixOS module output applied to the host" },
  systemd_unit:  { label:"systemd unit state",   icon:"cpu",      desc:"Active unit settings + analyze score" },
  audit_log:     { label:"Audit log excerpt",    icon:"terminal", desc:"auditd events matching control window" },
  cve_scan:      { label:"CVE scan output",      icon:"shield",   desc:"vulnix scan result at time of eval" },
  build_artifact:{ label:"Build artifact",       icon:"build",    desc:"Hash of derivation + signed manifest" },
  policy_eval:   { label:"Policy evaluation",    icon:"check",    desc:"Gate decision + per-rule outcomes" },
};

// Per-system compliance rollup vs a bundle
function bundleStatusForSystem(bundle, sys) {
  if (!bundle.requiredEnvs.includes(sys.environment) && !sys.compliance?.bundles?.includes(bundle.id)) {
    return { applies: false };
  }
  const seed = (sys.id + bundle.id).split("").reduce((a,c) => a + c.charCodeAt(0), 0);
  const rand = (k) => ((seed * (k+1) * 9301) % 233280) / 233280;
  const total = bundle.policyIds.length;
  let pass = 0, warn = 0, fail = 0, waiver = 0;
  bundle.policyIds.forEach((_, i) => {
    const r = rand(i);
    if (r < 0.74) pass++;
    else if (r < 0.85) warn++;
    else if (r < 0.94) fail++;
    else waiver++;
  });
  return { applies:true, total, pass, warn, fail, waiver, score: Math.round((pass + waiver) / total * 100) };
}

// Generate the actual artifact body for an evidence item — the proof an auditor reads.
const _NIX_SNIPPETS = {
  sshd_hardening: `services.openssh.settings = {\n  PermitRootLogin = "no";\n  PasswordAuthentication = false;\n  KbdInteractiveAuthentication = false;\n  X11Forwarding = false;\n  Ciphers = [ "aes256-gcm@openssh.com" "chacha20-poly1305@openssh.com" ];\n  MACs = [ "hmac-sha2-512-etm@openssh.com" ];\n  ClientAliveInterval = 600;\n  ClientAliveCountMax = 1;\n};`,
  audit_rules: `security.auditd.enable = true;\nsecurity.audit.rules = [\n  "-w /etc/shadow -p wa -k identity"\n  "-w /etc/sudoers -p wa -k privilege"\n  "-a always,exit -F arch=b64 -S execve -k exec"\n];`,
  banner: `environment.etc."issue".text = ''\n  *** U.S. GOVERNMENT INFORMATION SYSTEM ***\n  Use of this system constitutes consent to monitoring.\n'';\nservices.openssh.banner = config.environment.etc."issue".text;`,
  firewall_default_deny: `networking.firewall = {\n  enable = true;\n  rejectPackets = false;            # drop, don't reject\n  allowedTCPPorts = [ 22 443 ];\n  extraCommands = "iptables -P INPUT DROP";\n};`,
  fips_crypto: `boot.kernelParams = [ "fips=1" ];\nenvironment.systemPackages = [ pkgs.openssl_fips ];\n# /proc/sys/crypto/fips_enabled => 1`,
  tls_min_version: `services.nginx.sslProtocols = "TLSv1.3";\nservices.nginx.sslCiphers = "TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256";`,
  kernel_hardening: `boot.kernel.sysctl = {\n  "kernel.kptr_restrict" = 2;\n  "kernel.dmesg_restrict" = 1;\n  "net.ipv4.conf.all.rp_filter" = 1;\n};`,
};
function _artifactFor(type, policyId, policyName, sys, status, seed) {
  switch (type) {
    case "config":
      return { kind:"code", lang:"nix", title:`modules/security/${policyId}.nix · rendered for ${sys.hostname}`,
        content: (_NIX_SNIPPETS[policyId] || `# ${policyName}\nsecurity.${policyId}.enable = true;`) +
          `\n\n# drv: /nix/store/${(seed%1e9).toString(36)}-${policyId}.drv\n# applied in generation #${sys.generation ?? "—"} @ ${sys.commit}` };
    case "systemd_unit": {
      const unit = `${policyId.replace(/_/g,"-")}.service`;
      return { kind:"terminal", title:`# systemctl + systemd-analyze on ${sys.hostname}`,
        content: `$ systemctl show ${unit} -p ActiveState -p SubState -p UnitFileState\nActiveState=active\nSubState=running\nUnitFileState=enabled\n\n$ systemd-analyze security ${unit}\n  NoNewPrivileges=yes                         ✓\n  ProtectSystem=strict                        ✓\n  ProtectKernelModules=yes                    ✓\n  → Overall exposure level for ${unit}: 1.9 OK 🙂` };
    }
    case "audit_log":
      return { kind:"terminal", title:`# auditd events · key=${policyId}`,
        content: `$ ausearch -k ${policyId} -ts recent\n----\ntype=CONFIG_CHANGE msg=audit(1716480042.118:457): auid=0 ses=12\n  op=add_rule key="${policyId}" list=4 res=1\ntype=SYSCALL msg=audit(1716480042.118:458): arch=c000003e syscall=59\n  success=yes exit=0 comm="sshd" key="${policyId}"\n----\n${status === "warn" ? "2 events matched · 0 violations (1 deprecation notice)" : "events matched · 0 policy violations"}` };
    case "cve_scan":
      return { kind:"terminal", title:`# vulnix scan of /run/current-system`,
        content: `$ vulnix --system /run/current-system --json | jq .summary\nscanned 1,247 derivations in 3.2s\n\nCRITICAL  ${sys.cves?.critical ?? 0}\nHIGH      ${sys.cves?.high ?? 0}\nMEDIUM    ${sys.cves?.medium ?? 0}\n${(sys.cves?.critical ?? 0) === 0 ? "→ gate: PASS (no critical advisories)" : "→ gate: BLOCK (critical advisories present)"}` };
    case "policy_eval":
      return { kind:"json", title:`crystal-forge gate decision`,
        content: `{\n  "control": "${policyId}",\n  "system": "${sys.hostname}",\n  "decision": "${status === "pass" ? "allow" : status}",\n  "evaluated_at": "2026-05-23T14:02:11Z",\n  "rules": [\n    { "id": "${policyId}/present",  "result": "${status === "fail" ? "fail" : "pass"}" },\n    { "id": "${policyId}/enforced", "result": "${status === "fail" ? "fail" : "pass"}" }\n  ],\n  "signed_by": "crystal-forge-server",\n  "signature": "ed25519:${(seed % 1e12).toString(16)}…"\n}` };
    case "banner":
      return { kind:"screenshot", title:`console login captured on ${sys.hostname}`,
        content: `*** U.S. GOVERNMENT INFORMATION SYSTEM ***\n\nThis system is for authorized use only. By using this\nsystem you consent to monitoring and recording.\nUnauthorized use is subject to criminal prosecution.\n\n${sys.hostname} login: _` };
    default:
      return { kind:"terminal", title:type, content:"(no artifact body)" };
  }
}

// Per-policy evidence for a (system, policy) pair
function evidenceForControl(bundle, policyId, sys) {
  const policy = (typeof POLICIES !== "undefined" ? POLICIES : []).find(p => p.id === policyId) || { id: policyId, name: policyId };
  const seed = (sys.id + policyId).split("").reduce((a,c) => a + c.charCodeAt(0), 0);
  const rand = (k) => ((seed * (k+1) * 9301) % 233280) / 233280;
  const r = rand(0);
  const status = r < 0.72 ? "pass" : r < 0.84 ? "warn" : r < 0.93 ? "fail" : "waiver";

  // Curated evidence list per control
  const items = [];
  if (status !== "fail") {
    items.push({ type:"config",       at:"2h ago",  source:"eval@" + sys.commit, ref:"modules/security/" + policyId + ".nix", hash:"sha256-" + (seed % 1e6).toString(16).padStart(6,"0") });
    items.push({ type:"systemd_unit", at:"2h ago",  source:"agent", ref:policy.name + ".service", value:"score 94/100" });
  }
  if (policyId === "banner" && status !== "fail") {
    items.push({ type:"banner", at:"2h ago", source:"agent console capture", ref:"/etc/issue", _label:"Login banner (captured)" });
  }
  if (status === "pass" || status === "warn") {
    items.push({ type:"audit_log",    at:"5m ago",  source:"auditd", ref:"key=" + policyId, value:rand(3) > 0.5 ? "3 events matched" : "no policy violations" });
  }
  items.push({ type:"policy_eval",   at:"just now", source:"crystal-forge", ref:"gate evaluation", value: status === "pass" ? "allow" : status });
  if (policyId === "cve-gated") items.push({ type:"cve_scan", at:"1h ago", source:"vulnix", ref:"scan-2026-05-23", value:`${sys.cves?.critical ?? 0} crit / ${sys.cves?.high ?? 0} high` });
  if (status === "waiver") items.unshift({ type:"policy_eval", at:"7d ago", source:"security-team", ref:"WAIVER-2026-" + (seed % 999), value:"Risk accepted until 2026-08-30 (compensating control: network isolation)", _waiver:true });

  // Attach the actual artifact body to each item
  items.forEach(it => {
    if (it._waiver) {
      it.artifact = { kind:"doc", title:it.ref + " · approved waiver", content:
        `WAIVER ${it.ref}\nStatus:       APPROVED\nApprover:     security-team (M. Reyes)\nApproved:     2026-05-16\nExpires:      2026-08-30\nControl:      ${policy.name}\nSystem:       ${sys.hostname} (${sys.environment})\n\nCompensating control:\n  Host is network-isolated on VLAN 220 with no inbound\n  routes; access via bastion only. Reviewed quarterly.\n\nJustification:\n  Vendor patch pending upstream; risk accepted by AO.` };
    } else {
      it.artifact = _artifactFor(it.type === "banner" ? "banner" : it.type, policyId, policy.name, sys, status, seed);
    }
  });

  return {
    policyId, policyName: policy.name,
    status,
    items,
    summary: status === "pass" ? "Enforced; evidence collected on last eval."
           : status === "warn" ? "Enforced with deprecation warnings — review next maintenance window."
           : status === "fail" ? "Non-compliant — control not applied on this host."
           : "Waiver in effect — risk accepted per documented compensating control.",
    severity: policy.severity
            ? policy.severity
            : ["banner","session_lockout","umask_077"].includes(policyId) ? "low"
            : ["fips_crypto","sshd_hardening","audit_rules","cve-gated","two-approver"].includes(policyId) ? "high"
            : "medium",
    // NOTE: severity comes from the policy definition (policy.severity) when present;
    // the list above is a fallback for built-ins that predate the field.
  };
}

Object.assign(window, { COMPLIANCE_BUNDLES, EVIDENCE_TYPES, bundleStatusForSystem, evidenceForControl });
