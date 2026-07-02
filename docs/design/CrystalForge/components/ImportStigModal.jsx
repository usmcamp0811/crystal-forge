// Import a DISA STIG (XCCDF .xml) → generate a compliance bundle + its policies

/* ---------- XCCDF parsing ---------- */
// Walk the XML tree collecting elements whose localName matches (namespace-agnostic).
function _byLocal(root, name) {
  const out = [];
  const walk = (el) => {
    for (const child of el.children) {
      if (child.localName === name) out.push(child);
      walk(child);
    }
  };
  walk(root);
  return out;
}
function _firstLocal(el, name) {
  return _byLocal(el, name)[0] || null;
}
function _text(el) {
  if (!el) return "";
  // strip XHTML tags that STIGs embed in description/fixtext
  return (el.textContent || "").replace(/\s+/g, " ").trim();
}

const CAT = {
  high:   { cat:"CAT I",   label:"High",   color:"#f87171" },
  medium: { cat:"CAT II",  label:"Medium", color:"#fbbf24" },
  low:    { cat:"CAT III", label:"Low",    color:"#60a5fa" },
};

function parseXccdf(text) {
  const doc = new DOMParser().parseFromString(text, "application/xml");
  if (doc.querySelector("parsererror")) throw new Error("Not valid XML");
  const bench = _firstLocal(doc.documentElement, "Benchmark")
             || (doc.documentElement.localName === "Benchmark" ? doc.documentElement : null);
  if (!bench) throw new Error("No <Benchmark> element — is this an XCCDF STIG?");

  const title = _text(_firstLocal(bench, "title")) || "Imported STIG";
  const plain = _byLocal(bench, "plain-text").map(_text).join(" ");
  const verEl = _byLocal(bench, "version").find(v => v.parentElement === bench);
  const relMatch = plain.match(/Release:\s*([\d.]+)/i);
  const benchMatch = plain.match(/Benchmark Date:\s*([^\s]+(?:\s+\d{4})?)/i);
  const version = (verEl ? _text(verEl) : "") + (relMatch ? ` (r${relMatch[1]})` : "");

  const ruleEls = _byLocal(bench, "Rule");
  const rules = ruleEls.map(r => {
    const sev = (r.getAttribute("severity") || "medium").toLowerCase();
    const vEl = _byLocal(r, "version")[0];
    const stigId = _text(vEl) || r.getAttribute("id") || "";
    const checkEl = _firstLocal(r, "check-content");
    return {
      ruleId: r.getAttribute("id") || stigId,
      stigId,
      severity: ["high","medium","low"].includes(sev) ? sev : "medium",
      title: _text(_firstLocal(r, "title")),
      fixtext: _text(_firstLocal(r, "fixtext")),
      check: _text(checkEl),
      srg: (_text(_firstLocal(r, "ident")) || "").trim(),
      selected: true,
    };
  }).filter(r => r.title);

  return { title, version: version.trim() || "v1", ruleCount: rules.length, rules };
}

/* ---------- Sample STIG (used when no file is provided) ---------- */
const SAMPLE_STIG = {
  title: "Anduril NixOS Security Technical Implementation Guide",
  version: "v1 (r2)",
  sample: true,
  rules: [
    { stigId:"V-268089", severity:"high",   title:"NixOS must implement DOD-approved encryption to protect the confidentiality of remote access sessions.",                                   srg:"SRG-OS-000033", fixtext:"Configure SSH to use only FIPS-validated algorithms; set services.openssh.settings.Ciphers to approved values in configuration.nix.", check:"Verify FIPS-approved ciphers: sshd -T | grep -i ciphers", assert:[{ kind:"custom_eval", expr:"config.services.openssh.settings.Ciphers != null && builtins.all (c: builtins.elem c FIPS_APPROVED_CIPHERS) config.services.openssh.settings.Ciphers", message:"SSH must use only FIPS-validated ciphers" }] },
    { stigId:"V-268130", severity:"high",   title:"NixOS must store only encrypted representations of passwords.",                                                                            srg:"SRG-OS-000112", fixtext:"Ensure PAM uses yescrypt or sha512 hashing for all local accounts.", check:"Verify /etc/shadow contains only hashed passwords: awk -F: '($2!~/^\\$/) {print $1}' /etc/shadow", assert:[{ kind:"custom_eval", expr:"builtins.elem config.security.pam.hashAlgorithm [ \"yescrypt\" \"sha512\" ]", message:"Password hash must be yescrypt or sha512" }] },
    { stigId:"V-268131", severity:"high",   title:"NixOS must not have the telnet package installed.",                                                                                        srg:"SRG-OS-000095", fixtext:"Remove telnet from environment.systemPackages and rebuild: nixos-rebuild switch.", check:"Verify telnet is not installed: which telnet && echo FAIL || echo PASS", assert:[{ kind:"custom_eval", expr:"!builtins.elem pkgs.telnet config.environment.systemPackages", message:"telnet must not be installed" }] },
    { stigId:"V-268144", severity:"high",   title:"NixOS must protect the confidentiality and integrity of all information at rest.",                                                         srg:"SRG-OS-000185", fixtext:"Enable full-disk encryption via boot.initrd.luks.devices in configuration.nix.", check:"Verify LUKS encryption on data partitions: lsblk -o NAME,TYPE,MOUNTPOINT | grep crypt", assert:[{ kind:"custom_eval", expr:"config.boot.initrd.luks.devices != {}", message:"Data partitions must be LUKS-encrypted" }] },
    { stigId:"V-268168", severity:"high",   title:"NixOS must implement NIST FIPS-validated cryptography for digital signatures, cryptographic hashes, and confidentiality protection.",     srg:"SRG-OS-000478", fixtext:"Enable FIPS mode: set security.enableFIPSMode = true in configuration.nix and rebuild.", check:"Verify FIPS mode is enabled: cat /proc/sys/crypto/fips_enabled", assert:[{ kind:"nixos_option", path:"security.enableFIPSMode", op:"==", value:"true" }] },
    { stigId:"V-268172", severity:"high",   title:"NixOS must not allow an unattended or automatic logon to the system via the console.",                                                     srg:"SRG-OS-000480", fixtext:"Ensure services.getty.autologinUser is not set in configuration.nix.", check:"Verify no autologin is configured: grep -r autologinUser /etc/nixos/", assert:[{ kind:"nixos_option", path:"services.getty.autologinUser", op:"==", value:"null" }] },
    { stigId:"V-268078", severity:"medium", title:"NixOS must enable the built-in firewall.",                                                                                                 srg:"SRG-OS-000298", fixtext:"Set networking.firewall.enable = true in configuration.nix and define allowedTCPPorts.", check:"Verify firewall is active: nixos-option networking.firewall.enable", assert:[{ kind:"nixos_option", path:"networking.firewall.enable", op:"==", value:"true" }] },
    { stigId:"V-268080", severity:"medium", title:"NixOS must enable the audit daemon.",                                                                                                      srg:"SRG-OS-000004", fixtext:"Set security.audit.enable = true and configure audit rules in configuration.nix.", check:"Verify auditd is running: systemctl is-active auditd", assert:[{ kind:"nixos_option", path:"security.audit.enable", op:"==", value:"true" }, { kind:"custom_eval", expr:"config.security.audit.rules != []", message:"Audit rules must be configured" }] },
    { stigId:"V-268081", severity:"medium", title:"NixOS must enforce the limit of three consecutive invalid logon attempts by a user during a 15-minute time period.",                      srg:"SRG-OS-000021", fixtext:"Configure pam_faillock via custom PAM text with deny=3 unlock_time=900.", check:"Verify faillock configuration: faillock --user root", assert:[{ kind:"custom_eval", expr:"builtins.match \".*deny=3.*\" config.security.pam.services.login.text != null", message:"faillock must deny after 3 attempts" }] },
    { stigId:"V-268082", severity:"medium", title:"NixOS must display the Standard Mandatory DOD Notice and Consent Banner before granting local or remote access via command line logon.",  srg:"SRG-OS-000023", fixtext:"Set services.openssh.banner and /etc/issue to the DoD consent banner text in configuration.nix.", check:"Verify banner is configured: cat /etc/issue", assert:[{ kind:"nixos_option", path:"services.openssh.banner", op:"!=", value:"null" }] },
    { stigId:"V-268134", severity:"medium", title:"NixOS must enforce a minimum 15-character password length.",                                                                               srg:"SRG-OS-000078", fixtext:"Configure pam_pwquality with minlen = 15 via security.pam.services in configuration.nix.", check:"Verify minimum password length: grep minlen /etc/security/pwquality.conf", assert:[{ kind:"nixos_option", path:"security.pam.pwquality.minlen", op:">=", value:"15" }] },
    { stigId:"V-268137", severity:"medium", title:"NixOS must not allow direct login to the root account via SSH.",                                                                           srg:"SRG-OS-000109", fixtext:"Set services.openssh.settings.PermitRootLogin = \"no\" in configuration.nix.", check:"Verify root SSH login is disabled: sshd -T | grep permitrootlogin", assert:[{ kind:"nixos_option", path:"services.openssh.settings.PermitRootLogin", op:"==", value:"\"no\"" }] },
    { stigId:"V-268139", severity:"medium", title:"NixOS must enable USBguard.",                                                                                                              srg:"SRG-OS-000114", fixtext:"Set services.usbguard.enable = true and configure an allow-list policy in configuration.nix.", check:"Verify USBguard is active: systemctl is-active usbguard", assert:[{ kind:"nixos_option", path:"services.usbguard.enable", op:"==", value:"true" }, { kind:"packages_installed", packages:["usbguard"] }] },
    { stigId:"V-268142", severity:"medium", title:"NixOS must terminate all SSH connections after 10 minutes of becoming unresponsive.",                                                      srg:"SRG-OS-000163", fixtext:"Set services.openssh.settings.ClientAliveInterval = 600 and ClientAliveCountMax = 0 in configuration.nix.", check:"Verify SSH idle timeout: sshd -T | grep clientaliveinterval", assert:[{ kind:"nixos_option", path:"services.openssh.settings.ClientAliveInterval", op:"==", value:"600" }, { kind:"nixos_option", path:"services.openssh.settings.ClientAliveCountMax", op:"==", value:"0" }] },
    { stigId:"V-268149", severity:"medium", title:"NixOS must compare internal clocks at least every 24 hours with an authoritative time server.",                                            srg:"SRG-OS-000355", fixtext:"Enable services.chrony.enable = true and set servers to approved USNO/DOD NTP sources in configuration.nix.", check:"Verify NTP is synchronized: chronyc tracking", assert:[{ kind:"nixos_option", path:"services.chrony.enable", op:"==", value:"true" }] },
    { stigId:"V-268086", severity:"low",    title:"NixOS must initiate a session lock after a 10-minute period of inactivity for graphical user logon.",                                     srg:"SRG-OS-000029", fixtext:"Configure GNOME screen lock timeout via services.xserver.desktopManager.gnome.extraGSettingsOverrides with lock-delay=600.", check:"Verify screen lock timeout: gsettings get org.gnome.desktop.session idle-delay", assert:[{ kind:"custom_eval", expr:"config.services.xserver.desktopManager.gnome.extraGSettingsOverrides != null", message:"GNOME idle lock must be configured" }] },
  ].map(r => ({ ...r, ruleId:r.stigId, selected:true })),
};

/* ---------- Generation ---------- */
function _slug(s) { return s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 48); }

// Normalize a parsed/sample rule so every control carries the same editable shape:
//  · assertions[] — eval-time NixOS config assertions (the CF "rules"), each a typed
//    CF rule (nixos_option / packages_installed / custom_eval). Inferred from the STIG
//    when we can map it to an option/package; otherwise left EMPTY for the admin.
//  · evidence[]   — ATO proof. Seeded from the STIG check command + an attestation.
function normalizeStigRule(r) {
  const assertions = r.assertions
    ? r.assertions
    : (r.assert ? r.assert.map(a => ({ ...a, inferred: true })) : []);
  const evidence = r.evidence
    ? r.evidence
    : [
        ...(r.check ? [{ kind:"command", cmd: r.check, expect:"compliant" }] : []),
        { kind:"attestation", note:`Host booted the config generation whose eval-time assertion satisfies ${r.stigId}` },
      ];
  return { ...r, ruleId: r.ruleId || r.stigId, name: r.name || (r.stigId ? `${r.stigId} · ${r.title}` : r.title), selected: r.selected !== false, assertions, evidence };
}

// Is an assertion fully specified (vs. an empty scaffold the user hasn't filled)?
function assertFilled(a) {
  if (a.kind === "nixos_option")       return (a.path || "").trim().length > 0;
  if (a.kind === "packages_installed") return (a.packages || []).length > 0;
  if (a.kind === "custom_eval")        return (a.expr || "").trim().length > 0;
  return true;
}

function stigRuleToPolicy(rule, framework) {
  const id = "stig-" + _slug(rule.stigId || rule.title);
  const assertions = (rule.assertions || []).filter(assertFilled);
  return {
    id,
    name: rule.name || rule.stigId || _slug(rule.title),
    category: "security",
    description: rule.title,
    type: "custom",
    severity: rule.severity,
    enabled: true,
    imported: true,
    source: { framework, stigId: rule.stigId, srg: rule.srg },
    // CF rules = eval-time config assertions, already typed. Empty when none authored.
    rules: assertions.map(a => { const { inferred, ...rest } = a; return { ...rest, phase:"eval" }; }),
    rationale: rule.rationale || `${rule.srg ? rule.srg + ". " : ""}${rule.fixtext || "Imported from STIG benchmark."}`,
    evidence: rule.evidence && rule.evidence.length ? rule.evidence : [
      { kind:"command", cmd:(rule.check || "").slice(0, 120) || "manual verification", expect:"compliant" },
      { kind:"attestation", note:`Host booted the config generation whose eval-time assertion satisfies ${rule.stigId}` },
    ],
    createdBy: "stig-import",
    createdAt: "just now",
    lastModified: "just now",
  };
}

const EVIDENCE_KINDS = [
  { kind:"command",     label:"Command output" },
  { kind:"file",        label:"File contents" },
  { kind:"unit_state",  label:"systemd unit state" },
  { kind:"log",         label:"Log excerpt" },
  { kind:"attestation", label:"Store-path attestation" },
];

const EV_DEFAULTS = {
  command:     { cmd:"", expect:"compliant" },
  file:        { path:"", note:"" },
  unit_state:  { unit:"", state:"active" },
  log:         { source:"journald", unit:"", match:"" },
  attestation: { note:"Agent reports the booted config generation (store-path hash) for this host" },
};

// Scaffolds for a freshly-added assertion, mirroring the create-policy modal.
const ASSERT_DEFAULTS = {
  nixos_option:       { kind:"nixos_option", path:"", op:"==", value:"true" },
  packages_installed: { kind:"packages_installed", packages:[] },
  custom_eval:        { kind:"custom_eval", expr:"", message:"" },
};

function ImportStigModal({ onClose, onComplete }) {
  const [step, setStep] = React.useState("upload"); // upload | review | done
  const [parsed, setParsed] = React.useState(null);
  const [error, setError] = React.useState("");
  const [dragOver, setDragOver] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [fileName, setFileName] = React.useState("");

  // review-step state
  const [bundleName, setBundleName] = React.useState("");
  const [envs, setEnvs] = React.useState(["production"]);
  const [created, setCreated] = React.useState(null);
  const [cursor, setCursor] = React.useState(0);
  const fileRef = React.useRef(null);

  const allEnvs = (typeof ENVIRONMENTS !== "undefined" ? ENVIRONMENTS : []);

  const loadParsed = (p, name) => {
    const rules = (p.rules || []).map(normalizeStigRule);
    setParsed({ ...p, rules });
    setBundleName(p.title.length > 60 ? p.title.slice(0, 57) + "…" : p.title);
    setFileName(name || (p.sample ? "Anduril_NixOS_STIG_V1R2.xml (sample)" : ""));
    setStep("review");
  };

  const handleFile = (file) => {
    if (!file) return;
    setBusy(true); setError("");
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const p = parseXccdf(reader.result);
        if (!p.rules.length) throw new Error("No <Rule> elements found in this benchmark.");
        loadParsed(p, file.name);
      } catch (e) {
        setError(e.message || "Could not parse this file.");
      } finally { setBusy(false); }
    };
    reader.onerror = () => { setError("Could not read the file."); setBusy(false); };
    reader.readAsText(file);
  };

  const toggleRule = (i) => setParsed(p => ({ ...p, rules: p.rules.map((r, j) => j === i ? { ...r, selected: !r.selected } : r) }));
  const setSevSelected = (sev, val) => setParsed(p => ({ ...p, rules: p.rules.map(r => r.severity === sev ? { ...r, selected: val } : r) }));
  const toggleEnv = (n) => setEnvs(prev => prev.includes(n) ? prev.filter(x => x !== n) : [...prev, n]);

  // Edit a rule by its index in the full parsed.rules array
  const editRule = (ruleId, patch) => setParsed(p => ({
    ...p, rules: p.rules.map(r => (r.ruleId === ruleId ? { ...r, ...patch } : r)),
  }));

  const selectedRules = parsed ? parsed.rules.filter(r => r.selected) : [];
  const counts = parsed ? ["high","medium","low"].map(s => ({ s, n: parsed.rules.filter(r => r.severity === s).length, sel: parsed.rules.filter(r => r.severity === s && r.selected).length })) : [];

  const doImport = () => {
    const framework = "DISA STIG";
    // generate policies, de-duping against existing POLICIES
    const existing = new Set((typeof POLICIES !== "undefined" ? POLICIES : []).map(p => p.id));
    const newPolicies = [];
    const policyIds = [];
    selectedRules.forEach(rule => {
      const pol = stigRuleToPolicy(rule, framework);
      policyIds.push(pol.id);
      if (!existing.has(pol.id)) { newPolicies.push(pol); existing.add(pol.id); if (typeof POLICIES !== "undefined") POLICIES.push(pol); }
    });
    const bundleId = _slug(bundleName || parsed.title) || ("stig-" + Date.now());
    const bundle = {
      id: bundleId,
      name: bundleName || parsed.title,
      framework,
      version: parsed.version,
      description: `Imported from ${fileName || "STIG benchmark"} — ${policyIds.length} controls.`,
      layer: "system",
      owner: "security-team",
      lastReview: "just now",
      policyIds,
      requiredEnvs: envs.length ? envs : ["production"],
      imported: true,
    };
    if (typeof COMPLIANCE_BUNDLES !== "undefined") {
      const dup = COMPLIANCE_BUNDLES.findIndex(b => b.id === bundleId);
      if (dup >= 0) COMPLIANCE_BUNDLES.splice(dup, 1, bundle); else COMPLIANCE_BUNDLES.push(bundle);
    }
    setCreated({ bundle, newPolicyCount: newPolicies.length, reusedCount: policyIds.length - newPolicies.length });
    setStep("done");
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(720px,97vw)", maxHeight:"92vh", display:"flex", flexDirection:"column" }}>

        {/* ---------- Upload ---------- */}
        {step === "upload" && (
          <>
            <div className="modal-head">
              <h2><Icon name="download" size={14} style={{ marginRight:6, verticalAlign:"text-bottom", transform:"rotate(180deg)" }}/>Import STIG</h2>
              <p>Upload a DISA XCCDF benchmark (<span className="mono">.xml</span>). Crystal Forge parses each rule into a policy and assembles them into a compliance bundle.</p>
            </div>
            <div className="modal-body">
              <div
                onDragOver={e=>{e.preventDefault();setDragOver(true);}}
                onDragLeave={()=>setDragOver(false)}
                onDrop={e=>{e.preventDefault();setDragOver(false);handleFile(e.dataTransfer.files[0]);}}
                onClick={()=>fileRef.current?.click()}
                className="focus-ring"
                style={{
                  border:`2px dashed ${dragOver ? "var(--cf-brand-purple)" : "var(--cf-divider)"}`,
                  background: dragOver ? "color-mix(in oklab, var(--cf-brand-purple) 7%, var(--cf-card-bg))" : "var(--cf-card-bg)",
                  borderRadius:12, padding:"38px 20px", textAlign:"center", cursor:"pointer",
                }}>
                <input ref={fileRef} type="file" accept=".xml,.xccdf,text/xml,application/xml" style={{ display:"none" }}
                  onChange={e=>handleFile(e.target.files[0])}/>
                <div style={{ fontSize:30, marginBottom:8 }}>{busy ? "⏳" : "📄"}</div>
                <div style={{ fontSize:14, fontWeight:600 }}>{busy ? "Parsing benchmark…" : "Drop an XCCDF .xml here, or click to browse"}</div>
                <div style={{ fontSize:12, color:"var(--cf-text-muted)", marginTop:4 }}>DISA STIG / SCAP benchmark · parsed entirely in your browser</div>
              </div>
              {error && (
                <div className="sd-callout sd-callout-danger" style={{ marginTop:12 }}>
                  <Icon name="warn" size={13}/><div style={{ fontSize:12 }}>{error}</div>
                </div>
              )}
              <div style={{ display:"flex", alignItems:"center", gap:10, margin:"16px 0 4px" }}>
                <div style={{ flex:1, height:1, background:"var(--cf-divider)" }}/>
                <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>or</span>
                <div style={{ flex:1, height:1, background:"var(--cf-divider)" }}/>
              </div>
              <button className="btn btn-ghost focus-ring" style={{ width:"100%" }} onClick={()=>loadParsed(SAMPLE_STIG)}>
                <Icon name="shield" size={13}/> Try with a sample Anduril NixOS STIG
              </button>
            </div>
            <div className="modal-foot">
              <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
            </div>
          </>
        )}

        {/* ---------- Review ---------- */}
        {step === "review" && parsed && (
          <>
            <div className="modal-head">
              <h2><Icon name="shield" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>Review imported controls</h2>
              <p><span className="mono">{fileName}</span> · {parsed.title} · <strong>{parsed.version}</strong></p>
            </div>
            <div className="modal-body" style={{ overflowY:"auto" }}>
              <div className="field">
                <label>Bundle name</label>
                <input className="input focus-ring" value={bundleName} onChange={e=>setBundleName(e.target.value)}/>
              </div>

              <div className="field">
                <label>Applies to environments</label>
                <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
                  {allEnvs.map(env => {
                    const on = envs.includes(env.name);
                    return (
                      <button key={env.name} className="focus-ring" onClick={()=>toggleEnv(env.name)}
                        style={{ all:"unset", cursor:"pointer", padding:"6px 12px", borderRadius:99,
                          border:`1px solid ${on ? env.dot : "var(--cf-divider)"}`,
                          background: on ? `color-mix(in oklab, ${env.dot} 14%, var(--cf-card-bg))` : "var(--cf-card-bg)",
                          display:"flex", alignItems:"center", gap:7, fontSize:12, fontWeight:600,
                          color: on ? "var(--cf-text-primary)" : "var(--cf-text-muted)" }}>
                        <span style={{ width:8, height:8, borderRadius:99, background:env.dot }}/>{env.name}{on && <Icon name="check" size={11}/>}
                      </button>
                    );
                  })}
                </div>
              </div>

              <div className="field">
                <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", flexWrap:"wrap", gap:8 }}>
                  <label style={{ margin:0 }}>Controls <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· {selectedRules.length} of {parsed.rules.length} selected</span></label>
                  <div style={{ display:"flex", gap:6 }}>
                    {counts.map(({ s, n, sel }) => (
                      <button key={s} className="focus-ring" onClick={()=>setSevSelected(s, sel < n)}
                        title={`Toggle all ${CAT[s].cat}`}
                        style={{ all:"unset", cursor:"pointer", fontSize:11, fontWeight:600, padding:"3px 8px", borderRadius:99,
                          border:`1px solid ${CAT[s].color}55`, color:CAT[s].color,
                          background:`color-mix(in oklab, ${CAT[s].color} 10%, transparent)` }}>
                        {CAT[s].cat} · {sel}/{n}
                      </button>
                    ))}
                  </div>
                </div>
                <div style={{ display:"flex", flexDirection:"column", gap:5, maxHeight:280, overflowY:"auto", marginTop:8, paddingRight:2 }}>
                  {parsed.rules.map((r, i) => (
                    <button key={r.ruleId + i} className="focus-ring" onClick={()=>toggleRule(i)}
                      style={{ all:"unset", cursor:"pointer", display:"flex", gap:10, alignItems:"flex-start",
                        padding:"8px 10px", borderRadius:8,
                        border:`1px solid ${r.selected ? "var(--cf-brand-purple)55" : "var(--cf-divider)"}`,
                        background: r.selected ? "color-mix(in oklab, var(--cf-brand-purple) 6%, var(--cf-card-bg))" : "var(--cf-card-bg)" }}>
                      <span style={{ width:15, height:15, borderRadius:4, flexShrink:0, marginTop:1,
                        border:`1.5px solid ${r.selected ? "var(--cf-brand-purple)" : "var(--cf-text-muted)"}`,
                        background: r.selected ? "var(--cf-brand-purple)" : "transparent",
                        display:"flex", alignItems:"center", justifyContent:"center" }}>
                        {r.selected && <Icon name="check" size={10} style={{ color:"white" }}/>}
                      </span>
                      <span style={{ flexShrink:0, fontSize:10, fontWeight:700, padding:"2px 6px", borderRadius:4, marginTop:1,
                        color:CAT[r.severity].color, background:`color-mix(in oklab, ${CAT[r.severity].color} 14%, transparent)` }}>
                        {CAT[r.severity].cat}
                      </span>
                      <span style={{ minWidth:0 }}>
                        <span style={{ fontSize:12.5, fontWeight:600, display:"block", lineHeight:1.4 }}>{r.title}</span>
                        <span className="mono" style={{ fontSize:10.5, color:"var(--cf-text-muted)" }}>{r.stigId}{r.srg ? " · " + r.srg : ""}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </div>

              <div className="sd-callout sd-callout-info">
                <Icon name="check" size={13}/>
                <div style={{ fontSize:12 }}>
                  Creates <strong>{selectedRules.length}</strong> security {selectedRules.length === 1 ? "policy" : "policies"} and one bundle. Each becomes a standard CF policy — you'll set the <strong>config assertions</strong> (eval-time) and <strong>ATO evidence</strong> per control next. Existing policies with the same ID are reused, not duplicated.
                </div>
              </div>
            </div>
            <div className="modal-foot" style={{ justifyContent:"space-between" }}>
              <button className="btn btn-ghost focus-ring" onClick={()=>{ setStep("upload"); setParsed(null); setError(""); }}>← Back</button>
              <div style={{ display:"flex", gap:8 }}>
                <button className="btn btn-ghost focus-ring" disabled={!selectedRules.length || !bundleName.trim() || !envs.length}
                  style={(!selectedRules.length || !bundleName.trim() || !envs.length) ? { opacity:0.5, cursor:"not-allowed" } : null}
                  onClick={doImport} title="Create all policies as-is without per-control review">
                  Skip &amp; create all
                </button>
                <button className="btn btn-primary focus-ring" disabled={!selectedRules.length || !bundleName.trim() || !envs.length}
                  style={(!selectedRules.length || !bundleName.trim() || !envs.length) ? { opacity:0.5, cursor:"not-allowed" } : null}
                  onClick={()=>{ setCursor(0); setStep("refine"); }}>
                  Refine {selectedRules.length} {selectedRules.length === 1 ? "policy" : "policies"} <Icon name="chevron-right" size={13}/>
                </button>
              </div>
            </div>
          </>
        )}

        {/* ---------- Refine (per-control walkthrough) ---------- */}
        {step === "refine" && parsed && selectedRules.length > 0 && (() => {
          const rule = selectedRules[Math.min(cursor, selectedRules.length - 1)];
          const total = selectedRules.length;
          const isLast = cursor >= total - 1;
          const asserts = rule.assertions || [];
          const evlist = rule.evidence || [];
          const patchAssert = (i, patch) => editRule(rule.ruleId, { assertions: asserts.map((a,j)=> j===i ? { ...a, ...patch } : a) });
          const addAssert   = (kind) => editRule(rule.ruleId, { assertions: [...asserts, { ...ASSERT_DEFAULTS[kind] }] });
          const rmAssert    = (i) => editRule(rule.ruleId, { assertions: asserts.filter((_,j)=> j!==i) });
          const patchEv     = (i, patch) => editRule(rule.ruleId, { evidence: evlist.map((e,j)=> j===i ? { ...e, ...patch } : e) });
          const addEv       = (kind) => editRule(rule.ruleId, { evidence: [...evlist, { kind, ...EV_DEFAULTS[kind] }] });
          const rmEv        = (i) => editRule(rule.ruleId, { evidence: evlist.filter((_,j)=> j!==i) });
          const evInput = (ev, i, key, ph) => (
            <input className="input focus-ring mono" style={{ fontSize:11.5 }} value={ev[key] || ""} placeholder={ph}
              onChange={e=>patchEv(i, { [key]: e.target.value })}/>
          );
          const evFields = (ev, i) => {
            switch (ev.kind) {
              case "command":     return <><div style={{ fontSize:10, color:"var(--cf-text-muted)", marginBottom:3 }}>command</div>{evInput(ev,i,"cmd","sshd -T | grep …")}<div style={{ fontSize:10, color:"var(--cf-text-muted)", margin:"6px 0 3px" }}>expected output</div>{evInput(ev,i,"expect","compliant")}</>;
              case "file":        return <><div style={{ fontSize:10, color:"var(--cf-text-muted)", marginBottom:3 }}>path</div>{evInput(ev,i,"path","/etc/issue")}<div style={{ fontSize:10, color:"var(--cf-text-muted)", margin:"6px 0 3px" }}>must contain</div>{evInput(ev,i,"note","banner text")}</>;
              case "unit_state":  return <><div style={{ fontSize:10, color:"var(--cf-text-muted)", marginBottom:3 }}>unit</div>{evInput(ev,i,"unit","auditd.service")}<div style={{ fontSize:10, color:"var(--cf-text-muted)", margin:"6px 0 3px" }}>state</div>{evInput(ev,i,"state","active")}</>;
              case "log":         return <><div style={{ fontSize:10, color:"var(--cf-text-muted)", marginBottom:3 }}>unit</div>{evInput(ev,i,"unit","auditd.service")}<div style={{ fontSize:10, color:"var(--cf-text-muted)", margin:"6px 0 3px" }}>log line matches</div>{evInput(ev,i,"match","audit: rules loaded")}</>;
              default:            return <><div style={{ fontSize:10, color:"var(--cf-text-muted)", marginBottom:3 }}>agent reports</div>{evInput(ev,i,"note","booted generation / store-path hash")}</>;
            }
          };
          return (
            <>
              <div className="modal-head">
                <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:10 }}>
                  <h2><Icon name="shield" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>Refine policy {cursor + 1} of {total}</h2>
                  <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{rule.stigId}</span>
                </div>
                <div style={{ height:4, borderRadius:99, background:"var(--cf-divider)", marginTop:8, overflow:"hidden" }}>
                  <div style={{ height:"100%", width:`${((cursor + 1) / total) * 100}%`, background:"var(--cf-brand-purple)", transition:"width .2s" }}/>
                </div>
              </div>
              <div className="modal-body" style={{ overflowY:"auto" }}>
                <div style={{ display:"flex", gap:8, alignItems:"center", marginBottom:12 }}>
                  <span className="chip chip-info">Security &amp; hardening</span>
                  {rule.srg && <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{rule.srg}</span>}
                  <span style={{ flex:1 }}/>
                  <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>policy id: <span className="mono">stig-{_slug(rule.stigId || rule.name)}</span></span>
                </div>

                {/* ---- From the STIG: the ubiquitous, read-only control text ---- */}
                <div style={{ border:"1px solid var(--cf-divider)", borderRadius:10, overflow:"hidden", marginBottom:18 }}>
                  <div style={{ display:"flex", alignItems:"center", gap:8, padding:"8px 12px", background:"var(--cf-subtle-bg)", borderBottom:"1px solid var(--cf-divider)" }}>
                    <Icon name="shield" size={12} style={{ color:"var(--cf-text-muted)" }}/>
                    <span style={{ fontSize:10, textTransform:"uppercase", letterSpacing:"0.08em", fontWeight:700, color:"var(--cf-text-muted)" }}>From the STIG</span>
                    <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{rule.stigId}</span>
                    <span style={{ flex:1 }}/>
                    <span style={{ fontSize:10, fontWeight:700, padding:"2px 6px", borderRadius:4, color:CAT[rule.severity].color, background:`color-mix(in oklab, ${CAT[rule.severity].color} 14%, transparent)` }}>{CAT[rule.severity].cat}</span>
                  </div>
                  <div style={{ padding:"11px 12px", display:"flex", flexDirection:"column", gap:10 }}>
                    <div style={{ fontSize:12.5, fontWeight:600, lineHeight:1.45 }}>{rule.title}</div>
                    {rule.fixtext && (
                      <div>
                        <div style={{ fontSize:9.5, textTransform:"uppercase", letterSpacing:"0.07em", fontWeight:700, color:"var(--cf-text-muted)", marginBottom:3 }}>Official fix</div>
                        <div style={{ fontSize:12, color:"var(--cf-text-secondary)", lineHeight:1.5 }}>{rule.fixtext}</div>
                      </div>
                    )}
                    {rule.check && (
                      <div>
                        <div style={{ fontSize:9.5, textTransform:"uppercase", letterSpacing:"0.07em", fontWeight:700, color:"var(--cf-text-muted)", marginBottom:3 }}>Official check</div>
                        <div className="mono" style={{ fontSize:11, color:"var(--cf-text-secondary)", lineHeight:1.5 }}>{rule.check}</div>
                      </div>
                    )}
                  </div>
                </div>

                {/* ---- Policy basics ---- */}
                <div style={{ display:"flex", gap:12, marginBottom:4 }}>
                  <div className="field" style={{ flex:1 }}>
                    <label>Policy name</label>
                    <input className="input focus-ring mono" value={rule.name || `${rule.stigId} · ${rule.title}`}
                      onChange={e=>editRule(rule.ruleId, { name: e.target.value })}/>
                  </div>
                  <div className="field">
                    <label>Severity</label>
                    <div className="seg" style={{ width:"fit-content" }}>
                      {["high","medium","low"].map(s => (
                        <button key={s} className={rule.severity === s ? "active" : ""}
                          onClick={()=>editRule(rule.ruleId, { severity: s })}
                          style={rule.severity === s ? { color:CAT[s].color } : null}>
                          {CAT[s].cat}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>

                {/* ---- Config assertions (eval-time rules), mirroring the policy editor ---- */}
                <div className="field">
                  <label style={{ display:"flex", alignItems:"center", gap:8 }}>
                    NixOS config assertions
                    <span className="chip" style={{ fontSize:9.5, fontWeight:700, padding:"1px 6px", borderRadius:4,
                      color:"var(--cf-brand-purple)", background:"color-mix(in oklab, var(--cf-brand-purple) 14%, transparent)",
                      border:"1px solid color-mix(in oklab, var(--cf-brand-purple) 40%, transparent)" }}>EVAL-TIME</span>
                  </label>
                  <div style={{ fontSize:11, color:"var(--cf-text-muted)", margin:"0 0 9px", lineHeight:1.5 }}>
                    Asserted against the rendered config during <span className="mono">nix flake check</span> — fails the build before it deploys. These become the policy's rules. {asserts.length === 0 ? "Nothing was inferred for this control — add the assertion that proves it." : "Inferred from the STIG; review before importing."}
                  </div>

                  {asserts.length === 0 && (
                    <div style={{ display:"flex", gap:10, alignItems:"flex-start", padding:"11px 12px", borderRadius:8,
                      border:"1px dashed color-mix(in oklab, #fbbf24 45%, var(--cf-divider))",
                      background:"color-mix(in oklab, #fbbf24 7%, var(--cf-card-bg))" }}>
                      <Icon name="warn" size={13} style={{ color:"#fbbf24", marginTop:1, flexShrink:0 }}/>
                      <div style={{ fontSize:12, color:"var(--cf-text-secondary)", lineHeight:1.5 }}>
                        No assertion could be inferred from this STIG control. Add one — assert a NixOS option value, assert a package is installed, or write a custom nix expression — or leave empty to rely on runtime evidence alone.
                      </div>
                    </div>
                  )}

                  {asserts.length > 0 && (
                    <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
                      {asserts.map((a, i) => (
                        <div key={i} style={{ padding:"8px 10px", borderRadius:8, background:"var(--cf-subtle-bg)",
                          border: a.inferred ? "1px solid color-mix(in oklab, var(--cf-brand-purple) 28%, transparent)" : "1px solid transparent" }}>
                          {a.inferred && (
                            <div style={{ marginBottom:6 }}>
                              <span style={{ fontSize:9, fontWeight:700, padding:"1px 6px", borderRadius:4, color:"var(--cf-brand-purple)", background:"color-mix(in oklab, var(--cf-brand-purple) 13%, transparent)" }}>inferred · review</span>
                            </div>
                          )}
                          <div style={{ display:"grid", gridTemplateColumns:"1fr auto", gap:8, alignItems:"center" }}>
                            <RuleEditor rule={a} onChange={patch=>patchAssert(i, { ...patch, inferred:false })}/>
                            <button className="btn-icon focus-ring" onClick={()=>rmAssert(i)} aria-label="Remove assertion" title="Remove"><Icon name="x" size={13}/></button>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}

                  <div style={{ marginTop:8 }}>
                    <select className="input focus-ring" defaultValue=""
                      onChange={e=>{ if (e.target.value) { addAssert(e.target.value); e.target.value = ""; } }}
                      style={{ maxWidth:280, fontSize:12 }}>
                      <option value="" disabled>+ Add assertion…</option>
                      <option value="nixos_option">Assert a NixOS option value</option>
                      <option value="packages_installed">Assert packages installed</option>
                      <option value="custom_eval">Custom nix expression</option>
                    </select>
                  </div>
                </div>

                {/* ---- Evidence for ATO ---- */}
                <div className="field">
                  <label style={{ display:"flex", alignItems:"center", gap:8 }}>
                    Evidence for ATO <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· {evlist.length}</span>
                  </label>
                  <div style={{ fontSize:11, color:"var(--cf-text-muted)", margin:"0 0 9px", lineHeight:1.5 }}>
                    Artifacts collected at deploy and runtime to prove the control to an assessor. Seeded from the STIG check.
                  </div>
                  {evlist.map((ev, i) => (
                    <div key={i} style={{ border:"1px solid var(--cf-divider)", borderRadius:8, padding:"10px 11px", marginBottom:8, background:"var(--cf-card-bg)" }}>
                      <div style={{ display:"flex", alignItems:"center", gap:8, marginBottom:8 }}>
                        <span className="chip chip-unknown" style={{ fontSize:9 }}>{ev.kind.replace("_"," ")}</span>
                        <span style={{ flex:1 }}/>
                        <button className="btn-icon focus-ring" onClick={()=>rmEv(i)} aria-label="Remove evidence" title="Remove"><Icon name="x" size={13}/></button>
                      </div>
                      {evFields(ev, i)}
                    </div>
                  ))}
                  <div style={{ display:"flex", flexWrap:"wrap", gap:6, marginTop:2 }}>
                    {EVIDENCE_KINDS.map(k => (
                      <button key={k.kind} className="btn btn-ghost focus-ring" style={{ fontSize:11, padding:"3px 9px" }} onClick={()=>addEv(k.kind)}>
                        <Icon name="plus" size={10}/> {k.label}
                      </button>
                    ))}
                  </div>
                </div>

                <div className="sd-callout sd-callout-info">
                  <Icon name="check" size={13}/>
                  <div style={{ fontSize:12 }}>
                    On import this becomes a standard CF security policy — <strong>{asserts.filter(assertFilled).length} config {asserts.filter(assertFilled).length === 1 ? "assertion" : "assertions"}</strong> (eval-time rules) and <strong>{evlist.length} evidence {evlist.length === 1 ? "item" : "items"}</strong> for ATO. Editable later from the Policies view.
                  </div>
                </div>
              </div>
              <div className="modal-foot" style={{ justifyContent:"space-between" }}>
                <div style={{ display:"flex", gap:8 }}>
                  <button className="btn btn-ghost focus-ring" onClick={()=> cursor === 0 ? setStep("review") : setCursor(c => c - 1)}>
                    <Icon name="chevron-left" size={13}/> {cursor === 0 ? "Back to list" : "Previous"}
                  </button>
                  <button className="btn btn-ghost focus-ring" style={{ color:"#f87171" }}
                    title="Exclude this control from the bundle"
                    onClick={()=>{
                      editRule(rule.ruleId, { selected:false });
                      if (total <= 1) { setStep("review"); }
                      else if (isLast) { setCursor(c => Math.max(0, c - 1)); }
                    }}>
                    Exclude
                  </button>
                </div>
                <div style={{ display:"flex", gap:8, alignItems:"center" }}>
                  <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{cursor + 1} / {total}</span>
                  {isLast ? (
                    <button className="btn btn-primary focus-ring" onClick={doImport}>
                      <Icon name="check" size={13}/> Create bundle + {total} {total === 1 ? "policy" : "policies"}
                    </button>
                  ) : (
                    <button className="btn btn-primary focus-ring" onClick={()=>setCursor(c => Math.min(total - 1, c + 1))}>
                      Next <Icon name="chevron-right" size={13}/>
                    </button>
                  )}
                </div>
              </div>
            </>
          );
        })()}

        {/* ---------- Done ---------- */}
        {step === "done" && created && (
          <>
            <div className="modal-head">
              <h2 style={{ display:"flex", alignItems:"center", gap:8 }}><Icon name="check" size={16} style={{ color:"#34d399" }}/>Bundle created</h2>
              <p><span className="mono" style={{ fontWeight:600 }}>{created.bundle.name}</span> is ready.</p>
            </div>
            <div className="modal-body">
              <div style={{ display:"grid", gridTemplateColumns:"repeat(3,1fr)", gap:10 }}>
                {[
                  { n: created.bundle.policyIds.length, l:"controls" },
                  { n: created.newPolicyCount, l:"new policies" },
                  { n: created.reusedCount, l:"reused" },
                ].map((s,i)=>(
                  <div key={i} className="card" style={{ padding:"14px 12px", textAlign:"center" }}>
                    <div style={{ fontSize:24, fontWeight:700 }}>{s.n}</div>
                    <div style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{s.l}</div>
                  </div>
                ))}
              </div>
              <div className="sd-callout sd-callout-info" style={{ marginTop:12 }}>
                <Icon name="shield" size={13}/>
                <div style={{ fontSize:12 }}>
                  New policies appear in the <strong>Policies</strong> view under <strong>Security &amp; hardening</strong>. The bundle now gates the environments you selected: {created.bundle.requiredEnvs.join(", ")}.
                </div>
              </div>
            </div>
            <div className="modal-foot">
              <button className="btn btn-primary focus-ring" onClick={()=>onComplete?.(created.bundle.id)}>
                View bundle
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

Object.assign(window, { ImportStigModal });
