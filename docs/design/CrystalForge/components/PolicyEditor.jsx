// Policy editor — one editor for every policy, whatever its origin.
//
// Five concepts, kept apart on purpose: Intent (Basics), Enforcement, Compliance,
// Evidence, and Provenance. Provenance is read-only: editing where information came
// from would rewrite history. Imported STIG controls use this exact editor — they just
// arrive with more of it already filled in.

const PE_SECTIONS = [
  { id:"basics",      label:"Basics",      icon:"file",   blurb:"What this policy is" },
  { id:"enforcement", label:"Enforcement", icon:"shield", blurb:"What Crystal Forge requires" },
  { id:"compliance",  label:"Compliance",  icon:"check",  blurb:"External requirements it implements" },
  { id:"evidence",    label:"Evidence",    icon:"key",    blurb:"What gets collected for an assessor" },
];

function policyEditorInitialForm(policy) {
  const p = policy || {};
  return {
    name: p.name || "",
    description: p.description || "",
    category: p.category || "deployment",
    rationale: p.rationale || "",
    severity: p.severity || "medium",
    rules: (p.rules || []).map(r => r.kind === "nixos_option"
      ? { ...r, value: semanticValue(r.value, nixosOptionMeta(r.path).type) }
      : { ...r }),
    evidence: (p.evidence || []).map(e => ({ ...e })),
    mappings: (policy && typeof mappingsForPolicy === "function" ? mappingsForPolicy(policy.id) : []).map(m => ({ ...m })),
  };
}

function PolicyEditor({ mode, policy, onClose }) {
  const isEdit = mode === "edit";
  const [form, setForm] = React.useState(() => policyEditorInitialForm(isEdit ? policy : null));
  const [section, setSection] = React.useState("basics");
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const [openRule, setOpenRule] = React.useState(null);
  const [adding, setAdding] = React.useState(false);
  const [mappingEditor, setMappingEditor] = React.useState(null);
  const [catChanged, setCatChanged] = React.useState(null);
  const set = (k, v) => setForm(p => ({ ...p, [k]: v }));
  const source = isEdit ? policy?.source : null;

  const cat = policyCategoryMeta(form.category);
  const recommended = recommendedEnforcement(form.category);
  const offCategoryRules = form.rules.filter(r => !recommended.includes(r.kind));

  const addRule = (kind) => {
    const defaults = {
      eval_passed: {}, pin_required: {},
      cve_block: { severity:"critical", maxAllowed:0 },
      time_window: { days:["mon","tue","wed","thu","fri"], from:"09:00", to:"17:00", tz:"America/New_York" },
      approval_required: { count:2, role:"admin" },
      rollout_percent: { percent:25, observeMin:30 },
      packages_installed: { packages:["openssh"] },
      packages_absent: { packages:["telnet"] },
      nixos_option: { path:"services.openssh.enable", op:"==", value:true },
      custom_eval: { expr:"config.networking.firewall.enable == true", message:"Firewall must be enabled" },
    };
    setForm(p => ({ ...p, rules: [...p.rules, { kind, ...defaults[kind] }] }));
    setOpenRule(form.rules.length);
    setAdding(false);
  };
  const updateRule = (i, patch) => setForm(p => ({ ...p, rules: p.rules.map((r, ix) => ix === i ? { ...r, ...patch } : r) }));
  const removeRule = (i) => { setForm(p => ({ ...p, rules: p.rules.filter((_, ix) => ix !== i) })); setOpenRule(null); };

  const addEvidence = (kind) => {
    const defaults = {
      command:    { kind:"command",    cmd:"sshd -T | grep permitrootlogin", expect:"permitrootlogin no" },
      log:        { kind:"log",        source:"journald", unit:"auditd.service", match:"audit: rules loaded" },
      file:       { kind:"file",       path:"/etc/issue", note:"Must contain the USG banner text" },
      unit_state: { kind:"unit_state", unit:"auditd.service", state:"active" },
      eval_attr:  { kind:"eval_attr",  attr:"config.services.openssh.settings.PermitRootLogin" },
      attestation:{ kind:"attestation",note:"Ed25519-signed agent fingerprint snapshot at deploy time" },
    };
    set("evidence", [...form.evidence, defaults[kind]]);
  };

  const counts = {
    enforcement: form.rules.length,
    compliance: form.mappings.length,
    evidence: form.evidence.length,
  };
  const badge = (id) => {
    if (id === "enforcement") return counts.enforcement ? String(counts.enforcement) : (source ? "Needs refinement" : "None");
    if (id === "compliance") return counts.compliance ? String(counts.compliance) : "Unmapped";
    if (id === "evidence") return counts.evidence ? String(counts.evidence) : "None";
    return null;
  };
  const mappedNotEnforced = counts.compliance > 0 && counts.enforcement === 0;

  const doSave = () => {
    const serializedRules = form.rules.map(r => r.kind === "nixos_option" ? { ...r } : r);
    const policyId = isEdit ? policy.id : `custom-${slugify(form.name) || Date.now()}`;
    const base = {
      name: form.name, description: form.description, category: form.category,
      rationale: form.rationale, severity: form.severity,
      rules: serializedRules, evidence: form.evidence, lastModified: "just now",
    };
    if (isEdit) Object.assign(policy, base);
    else {
      window.__cfCoach?.complete("policy");
      POLICIES.push({
        id: policyId, lineageId: policyId, revision: 1, publicationState: "current",
        publishedDate: new Date().toISOString().slice(0,10), type: "custom",
        createdBy: "you", createdAt: "just now", ...base,
      });
    }
    if (typeof POLICY_REQUIREMENT_MAPPINGS !== "undefined") {
      for (let i = POLICY_REQUIREMENT_MAPPINGS.length - 1; i >= 0; i--) if (POLICY_REQUIREMENT_MAPPINGS[i].policyId === policyId) POLICY_REQUIREMENT_MAPPINGS.splice(i,1);
      form.mappings.forEach(m => POLICY_REQUIREMENT_MAPPINGS.push({ ...m, policyId }));
    }
  };
  const doDelete = () => {
    const idx = POLICIES.findIndex(p => p.id === policy.id);
    if (idx >= 0) POLICIES.splice(idx, 1);
    onClose();
  };

  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape" && !confirmDelete) onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, confirmDelete]);

  if (confirmDelete) {
    return (
      <div className="modal-backdrop" onClick={onClose}>
        <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(520px,96vw)" }}>
          <DeletePolicyConfirm policy={policy} onCancel={()=>setConfirmDelete(false)} onConfirm={doDelete}/>
        </div>
      </div>
    );
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="pe-shell" onClick={e=>e.stopPropagation()}>
        <header className="pe-head">
          <div style={{ minWidth:0, display:"flex", flexDirection:"column", gap:3 }}>
            <div style={{ display:"flex", alignItems:"center", gap:9, minWidth:0 }}>
              <Icon name={isEdit ? "gear" : "plus"} size={15} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
              <span className="pe-head-title">{isEdit ? (form.name || policy.name) : "New policy"}</span>
              <span className="chip" style={{ color:cat.color, background:`color-mix(in oklab, ${cat.color} 14%, transparent)`, display:"inline-flex", alignItems:"center", gap:4 }}>
                <Icon name={cat.icon} size={10}/> {cat.label}
              </span>
              {source && <span className="chip chip-info"><Icon name="upload" size={9}/> Imported</span>}
              {counts.compliance === 0 && <span className="chip" title="Valid — this policy implements no external framework requirement.">Unmapped</span>}
            </div>
            <span className="pe-head-sub">{form.description || "Describe what this policy is for in Basics."}</span>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16}/></button>
        </header>

        <nav className="pe-rail">
          {PE_SECTIONS.map(s => {
            const b = badge(s.id);
            const warn = s.id === "enforcement" && mappedNotEnforced;
            return (
              <button key={s.id} className={`pe-rail-item focus-ring${section===s.id?" active":""}`} onClick={()=>setSection(s.id)}>
                <Icon name={s.icon} size={13}/>
                <span className="pe-rail-label">{s.label}</span>
                {b && <span className={`pe-rail-badge${warn?" warn":""}`}>{b}</span>}
              </button>
            );
          })}
          {source && (
            <div className="pe-prov">
              <div className="pe-prov-head"><Icon name="upload" size={10}/> Provenance <span className="pe-prov-ro">read-only</span></div>
              {[["Source", source.kind], ["Framework", source.framework], ["Artifact", source.artifact],
                ["Rule ID", source.ruleId], ["Group ID", source.groupId],
                ["Version", source.version && source.release ? `V${source.version}R${source.release}` : source.version],
                ["Published", source.published], ["Imported", `${source.importedAt} · ${source.importedBy}`]]
                .filter(([,v]) => v).map(([k,v]) => (
                  <div key={k} className="pe-prov-row"><span>{k}</span><span className="mono">{v}</span></div>
                ))}
              <div className="pe-prov-note">Recorded at import. Compliance relationships live in Compliance — they are not re-entered here.</div>
            </div>
          )}
        </nav>

        <div className="pe-body">
          {mappedNotEnforced && (
            <div className="sd-callout sd-callout-warn" style={{ marginBottom:14 }}>
              <Icon name="warn" size={14}/>
              <div style={{ fontSize:12 }}><strong>Mapped, not enforced.</strong> This policy claims {counts.compliance} compliance {counts.compliance===1?"requirement":"requirements"} but asserts nothing yet, so it cannot pass or fail. Add enforcement to make it real.</div>
            </div>
          )}

          {section === "basics" && (
            <PEBasics form={form} set={set} isEdit={isEdit} catChanged={catChanged} setCatChanged={setCatChanged}
              onCategory={(id)=>{ if (id !== form.category && form.rules.length) setCatChanged({ from:form.category, to:id }); set("category", id); }}
              offCategoryRules={offCategoryRules} onGoEnforcement={()=>setSection("enforcement")}/>
          )}

          {section === "enforcement" && (
            <PEEnforcement form={form} cat={cat} recommended={recommended} source={source}
              openRule={openRule} setOpenRule={setOpenRule} adding={adding} setAdding={setAdding}
              addRule={addRule} updateRule={updateRule} removeRule={removeRule}/>
          )}

          {section === "compliance" && (
            <PECompliance form={form} setForm={setForm} mappingEditor={mappingEditor} setMappingEditor={setMappingEditor} source={source}/>
          )}

          {section === "evidence" && (
            <PEEvidence form={form} set={set} addEvidence={addEvidence}/>
          )}

          {isEdit && section === "basics" && (
            <div style={{ marginTop:22, paddingTop:14, borderTop:"1px solid var(--cf-divider)" }}>
              <div className="pe-sec-label">Danger zone</div>
              <button className="btn btn-ghost focus-ring" onClick={()=>setConfirmDelete(true)} style={{ color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }}>
                <Icon name="x" size={12}/> Remove policy
              </button>
            </div>
          )}
        </div>

        <footer className="pe-foot">
          <span className="pe-foot-state">
            {counts.enforcement === 0 ? (source ? "Enforcement needs refinement" : "No enforcement defined") : `${counts.enforcement} enforcement ${counts.enforcement===1?"requirement":"requirements"}`}
            <span className="pe-foot-dot">·</span>
            {counts.compliance === 0 ? "Unmapped" : `${counts.compliance} compliance ${counts.compliance===1?"mapping":"mappings"}`}
            <span className="pe-foot-dot">·</span>
            {counts.evidence === 0 ? "No evidence" : `${counts.evidence} evidence ${counts.evidence===1?"source":"sources"}`}
          </span>
          <div style={{ display:"flex", gap:8 }}>
            <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
            <button className="btn btn-primary focus-ring" disabled={!form.name} onClick={()=>{ doSave(); onClose(); }}>
              <Icon name="check" size={13}/> {isEdit ? "Save changes" : "Create policy"}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}

// ── Basics / Intent ────────────────────────────────────────────────────────────
function PEBasics({ form, set, onCategory, catChanged, setCatChanged, offCategoryRules, onGoEnforcement }) {
  return (
    <>
      <div className="pe-sec-head">
        <h3>Basics</h3>
        <p>What this policy is and what it's primarily trying to accomplish. Implementation lives in Enforcement.</p>
      </div>
      <div className="field">
        <label>Name</label>
        <input className="input focus-ring" value={form.name} onChange={e=>set("name", e.target.value)} placeholder="e.g. Required applications"/>
      </div>
      <div className="field">
        <label>Description</label>
        <textarea className="input focus-ring" rows={2} value={form.description} onChange={e=>set("description", e.target.value)}
          placeholder="One or two lines shown in the registry" style={{ resize:"vertical" }}/>
      </div>
      <div className="field">
        <label>Category</label>
        <div className="pe-cat-grid">
          {POLICY_CATEGORIES.map(c => {
            const active = form.category === c.id;
            return (
              <button key={c.id} type="button" className="focus-ring pe-cat"
                onClick={()=>onCategory(c.id)}
                style={{ background: active ? `color-mix(in oklab, ${c.color} 12%, transparent)` : "var(--cf-subtle-bg)",
                  borderColor: active ? `color-mix(in oklab, ${c.color} 55%, transparent)` : "var(--cf-divider)" }}>
                <span className="pe-cat-icon" style={{ background:`color-mix(in oklab, ${c.color} 16%, transparent)`, color:c.color }}>
                  <Icon name={c.icon} size={13}/>
                </span>
                <span style={{ minWidth:0 }}>
                  <span className="pe-cat-label" style={{ color: active ? c.color : "var(--cf-text-primary)" }}>{c.label}</span>
                  <span className="pe-cat-blurb">{c.blurb}</span>
                </span>
              </button>
            );
          })}
        </div>
        <div className="help">Category guides which enforcement mechanisms are suggested. It never restricts them.</div>
      </div>
      {catChanged && offCategoryRules.length > 0 && (
        <div className="sd-callout sd-callout-info" style={{ margin:"2px 0 14px" }}>
          <Icon name="info" size={13}/>
          <div style={{ fontSize:12 }}>
            {offCategoryRules.length} existing {offCategoryRules.length===1?"requirement":"requirements"} ({offCategoryRules.map(r=>enforcementMeta(r.kind).label).join(", ")}) {offCategoryRules.length===1?"is":"are"} unusual for <strong>{policyCategoryMeta(form.category).label}</strong>. Nothing was changed or removed.{" "}
            <button className="link-action" onClick={onGoEnforcement} style={{ background:"none", border:"none", padding:0, font:"inherit", color:"inherit", cursor:"pointer" }}>Review enforcement</button>
            <button className="link-action" onClick={()=>setCatChanged(null)} style={{ background:"none", border:"none", padding:0, font:"inherit", color:"var(--cf-text-muted)", cursor:"pointer", marginLeft:10 }}>Dismiss</button>
          </div>
        </div>
      )}
      {form.category === "security" && (
        <div className="field">
          <label>Severity</label>
          <div className="seg seg-sev" style={{ width:"fit-content" }}>
            {[{ v:"high", l:"High (CAT I)", c:"#f87171" }, { v:"medium", l:"Medium (CAT II)", c:"#fbbf24" }, { v:"low", l:"Low (CAT III)", c:"#60a5fa" }].map(o => (
              <button key={o.v} className={form.severity===o.v?"active":""} onClick={()=>set("severity", o.v)}
                style={form.severity===o.v ? { color:o.c, background:`color-mix(in oklab, ${o.c} 16%, transparent)`, boxShadow:`inset 0 0 0 1px color-mix(in oklab, ${o.c} 45%, transparent)` } : { color:"var(--cf-text-secondary)" }}>
                <span style={{ display:"inline-flex", alignItems:"center", gap:6 }}>
                  <span style={{ width:7, height:7, borderRadius:"50%", background:o.c }}/>{o.l}
                </span>
              </button>
            ))}
          </div>
          <div className="help">Weights how failures of this control score in compliance reporting.</div>
        </div>
      )}
      <div className="field">
        <label>Rationale <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· optional</span></label>
        <textarea className="input focus-ring" rows={2} value={form.rationale} onChange={e=>set("rationale", e.target.value)}
          placeholder="Why this policy exists" style={{ resize:"vertical" }}/>
      </div>
    </>
  );
}

// ── Enforcement ───────────────────────────────────────────────────────────────
function enforcementPhrase(r) {
  if (r.kind === "nixos_option") {
    const meta = nixosOptionMeta(r.path);
    const opWord = { "==":"equals", "!=":"does not equal", ">=":"is at least", "<=":"is at most" }[r.op] || r.op;
    return { subject: `config.${r.path}`, verb: opWord, value: valueSummary(r.value, meta.type) };
  }
  if (r.kind === "packages_installed") return { subject:"System closure", verb:"must contain", value:(r.packages||[]).join(", ") };
  if (r.kind === "packages_absent") return { subject:"System closure", verb:"must not contain", value:(r.packages||[]).join(", ") };
  if (r.kind === "cve_block") return { subject:`${r.severity} CVEs`, verb:"at most", value:String(r.maxAllowed) };
  if (r.kind === "custom_eval") return { subject:"Nix assertion", verb:"must hold", value:r.message || r.expr };
  return { subject: ruleDescription(r), verb:"", value:"" };
}

function PEEnforcement({ form, cat, recommended, source, openRule, setOpenRule, adding, setAdding, addRule, updateRule, removeRule }) {
  const recTypes = recommended.map(k => enforcementMeta(k));
  const otherGroups = ENFORCEMENT_GROUPS.map(g => ({
    ...g, types: ENFORCEMENT_TYPES.filter(t => t.group === g.id && !recommended.includes(t.kind)),
  })).filter(g => g.types.length);
  return (
    <>
      <div className="pe-sec-head">
        <h3>Enforcement</h3>
        <p>What Crystal Forge asserts, requires, prohibits, or gates. All requirements must hold. Compliance mappings are separate — a policy can enforce a great deal and map to nothing.</p>
      </div>
      {form.rules.length === 0 ? (
        <div className={`sd-callout ${source ? "sd-callout-warn" : "sd-callout-info"}`} style={{ marginBottom:12 }}>
          <Icon name={source ? "warn" : "info"} size={13}/>
          <div style={{ fontSize:12 }}>
            {source
              ? <><strong>Enforcement needs refinement.</strong> This control was imported with its compliance mappings and provenance, but no assertion was inferred. Until one exists it asserts nothing.</>
              : <><strong>No enforcement defined.</strong> Add at least one requirement for this policy to have an effect.</>}
          </div>
        </div>
      ) : (
        <div className="pe-rules">
          {form.rules.map((r, i) => {
            const meta = enforcementMeta(r.kind);
            const ph = enforcementPhrase(r);
            const open = openRule === i;
            return (
              <div key={i} className={`pe-rule${open?" open":""}`}>
                <div className="pe-rule-row">
                  <span className="pe-rule-icon"><Icon name={meta.icon} size={12}/></span>
                  <button className="pe-rule-main focus-ring" onClick={()=>setOpenRule(open ? null : i)}>
                    <span className="pe-rule-phrase">
                      <span className="mono">{ph.subject}</span>
                      {ph.verb && <span className="pe-rule-verb">{ph.verb}</span>}
                      {ph.value && <span className="pe-rule-value mono">{ph.value}</span>}
                    </span>
                    <span className="pe-rule-kind">{meta.label}</span>
                  </button>
                  <button className="btn-icon focus-ring" title={open?"Done":"Edit"} onClick={()=>setOpenRule(open ? null : i)}><Icon name={open?"chevron-down":"gear"} size={12}/></button>
                  <button className="btn-icon focus-ring" title="Remove" onClick={()=>removeRule(i)}><Icon name="x" size={12}/></button>
                </div>
                {open && (
                  <div className="pe-rule-edit">
                    {r.kind === "nixos_option" ? <NixOptionRule rule={r} onChange={p=>updateRule(i,p)}/>
                      : r.kind === "packages_absent" ? <PackageListRule rule={r} onChange={p=>updateRule(i,p)} prohibited/>
                      : r.kind === "packages_installed" ? <PackageListRule rule={r} onChange={p=>updateRule(i,p)}/>
                      : r.kind === "pin_required" ? <span style={{ fontSize:12, color:"var(--cf-text-secondary)" }}>Only a pinned flake revision may deploy. No further configuration.</span>
                      : <RuleEditor rule={r} onChange={p=>updateRule(i,p)}/>}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {!adding ? (
        <button className="btn btn-subtle focus-ring" style={{ marginTop:10 }} onClick={()=>setAdding(true)}>
          <Icon name="plus" size={12}/> Add enforcement
        </button>
      ) : (
        <div className="pe-chooser">
          <div className="pe-chooser-head">
            <span>Suggested for <strong style={{ color:cat.color }}>{cat.label}</strong></span>
            <button className="btn btn-ghost focus-ring xs" onClick={()=>setAdding(false)}>Cancel</button>
          </div>
          <div className="pe-type-grid">
            {recTypes.map(t => (
              <button key={t.kind} className="pe-type focus-ring" onClick={()=>addRule(t.kind)}>
                <span className="pe-type-icon"><Icon name={t.icon} size={12}/></span>
                <span style={{ minWidth:0 }}>
                  <span className="pe-type-label">{t.label}</span>
                  <span className="pe-type-blurb">{t.blurb}</span>
                </span>
              </button>
            ))}
          </div>
          <div className="pe-chooser-note">Suggestions follow the category — they are not restrictions. Any policy can combine mechanisms from any group.</div>
          <details className="pe-more">
            <summary>More enforcement types</summary>
            {otherGroups.map(g => (
              <div key={g.id} className="pe-more-group">
                <div className="pe-more-label">{g.label}</div>
                <div className="pe-type-grid">
                  {g.types.map(t => (
                    <button key={t.kind} className="pe-type focus-ring" onClick={()=>addRule(t.kind)}>
                      <span className="pe-type-icon"><Icon name={t.icon} size={12}/></span>
                      <span style={{ minWidth:0 }}>
                        <span className="pe-type-label">{t.label}</span>
                        <span className="pe-type-blurb">{t.blurb}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            ))}
          </details>
        </div>
      )}
    </>
  );
}

function PackageListRule({ rule, onChange, prohibited }) {
  return (
    <div style={{ display:"flex", flexDirection:"column", gap:5 }}>
      <span style={{ fontSize:11.5, color:"var(--cf-text-secondary)" }}>
        Packages that must {prohibited ? "not " : ""}be present in the evaluated system closure — comma separated.
      </span>
      <input className="input focus-ring mono" value={(rule.packages||[]).join(", ")}
        onChange={e=>onChange({ packages: e.target.value.split(",").map(s=>s.trim()).filter(Boolean) })}
        placeholder="openssh, auditd" style={{ fontSize:12, padding:"6px 9px" }}/>
      <span className="mono pe-serial">→ {prohibited ? "!" : ""}builtins.any (p: p.pname == "…") config.environment.systemPackages</span>
    </div>
  );
}

// Type-aware NixOS value editor. The user edits the semantic value; Crystal Forge owns
// quoting and escaping. A boolean stays a two-button control; a banner becomes a real
// multiline editor with an expand-to-focus option.
function NixOptionRule({ rule, onChange }) {
  const meta = nixosOptionMeta(rule.path);
  const [expanded, setExpanded] = React.useState(false);
  const isLong = meta.type === "lines" || (typeof rule.value === "string" && (rule.value.length > 120 || rule.value.includes("\n")));
  const ops = meta.type === "int" ? ["==","!=",">=","<="] : ["==","!="];
  const opWords = { "==":"equals", "!=":"does not equal", ">=":"is at least", "<=":"is at most" };
  const setPath = (path) => {
    const next = nixosOptionMeta(path);
    let value = rule.value;
    if (next.type !== meta.type) value = next.type === "boolean" ? true : next.type === "int" ? 0 : next.type === "enum" ? next.values[0] : "";
    onChange({ path, value, op: next.type === "int" ? rule.op : "==" });
  };
  return (
    <div style={{ display:"flex", flexDirection:"column", gap:8 }}>
      <div className="pe-nix-row">
        <div className="field" style={{ margin:0, flex:1, minWidth:220 }}>
          <label>Option</label>
          <input className="input focus-ring mono" list="pe-nix-opts" value={rule.path} onChange={e=>setPath(e.target.value)}
            placeholder="services.openssh.enable" style={{ fontSize:11.5, padding:"6px 9px" }}/>
          <datalist id="pe-nix-opts">{NIXOS_OPTION_PATHS.map(p => <option key={p} value={p}/>)}</datalist>
        </div>
        <div className="field" style={{ margin:0, width:150 }}>
          <label>Comparison</label>
          <select className="input focus-ring" value={rule.op} onChange={e=>onChange({ op:e.target.value })} style={{ fontSize:12, padding:"6px 8px" }}>
            {ops.map(o => <option key={o} value={o}>{opWords[o]}</option>)}
          </select>
        </div>
      </div>
      <div className="pe-nix-meta">
        {meta.type === "unknown"
          ? <><Icon name="info" size={10}/> Unknown option — Crystal Forge has no type metadata, so the value is treated as text.</>
          : <><span className="pe-nix-type">{meta.type}{meta.unit ? ` · ${meta.unit}` : ""}</span> {meta.desc}</>}
      </div>

      <div className="field" style={{ margin:0 }}>
        <label>Value</label>
        {meta.type === "boolean" ? (
          <div className="seg" style={{ width:"fit-content" }}>
            <button className={rule.value === true ? "active" : ""} onClick={()=>onChange({ value:true })}>True</button>
            <button className={rule.value === false ? "active" : ""} onClick={()=>onChange({ value:false })}>False</button>
          </div>
        ) : meta.type === "enum" ? (
          <select className="input focus-ring mono" value={String(rule.value)} onChange={e=>onChange({ value:e.target.value })} style={{ width:"auto", fontSize:12, padding:"6px 9px" }}>
            {meta.values.map(v => <option key={v} value={v}>{v}</option>)}
          </select>
        ) : meta.type === "int" ? (
          <input type="number" className="input focus-ring mono" value={rule.value ?? 0} onChange={e=>onChange({ value: parseInt(e.target.value,10) || 0 })}
            style={{ width:130, fontSize:12, padding:"6px 9px" }}/>
        ) : isLong ? (
          <>
            <div className="pe-exact-note"><Icon name="info" size={10}/> Exact value comparison — whitespace and line breaks are compared byte for byte.</div>
            <textarea className="input focus-ring mono pe-lines" value={String(rule.value ?? "")} onChange={e=>onChange({ value:e.target.value })}
              placeholder="Exact text the option must equal" spellCheck={false}/>
            <div className="pe-lines-foot">
              <span>{String(rule.value ?? "").length.toLocaleString()} characters · {String(rule.value ?? "").split("\n").length} lines</span>
              <button className="btn btn-ghost focus-ring xs" onClick={()=>setExpanded(true)}><Icon name="grid" size={11}/> Expand</button>
            </div>
          </>
        ) : (
          <input className="input focus-ring mono" value={String(rule.value ?? "")} onChange={e=>onChange({ value:e.target.value })}
            style={{ fontSize:12, padding:"6px 9px" }} placeholder="Exact value"/>
        )}
      </div>
      <span className="mono pe-serial">→ config.{rule.path} {rule.op} {nixLiteral(rule.value, meta.type).replace(/\n[\s\S]*\n/, "\n  …\n")}</span>

      {expanded && (
        <div className="modal-backdrop" style={{ zIndex:80 }} onClick={()=>setExpanded(false)}>
          <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(860px,94vw)" }}>
            <div className="modal-head">
              <h2><Icon name="file" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/> Exact value</h2>
              <p className="mono" style={{ fontSize:11.5 }}>config.{rule.path}</p>
            </div>
            <div className="modal-body">
              <textarea className="input focus-ring mono" value={String(rule.value ?? "")} onChange={e=>onChange({ value:e.target.value })}
                spellCheck={false} style={{ width:"100%", height:"52vh", fontSize:12, lineHeight:1.55, resize:"vertical", whiteSpace:"pre" }}/>
              <div className="help">Crystal Forge serializes this into a nix string block on save — no quoting or escaping by hand.</div>
            </div>
            <div className="modal-foot">
              <button className="btn btn-primary focus-ring" onClick={()=>setExpanded(false)}><Icon name="check" size={13}/> Done</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Compliance ────────────────────────────────────────────────────────────────
function PECompliance({ form, setForm, mappingEditor, setMappingEditor, source }) {
  const saveMapping = (m) => setForm(p => ({ ...p, mappings: p.mappings.some(x=>x.id===m.id) ? p.mappings.map(x=>x.id===m.id?m:x) : [...p.mappings, m] }));
  const removeMapping = (id) => setForm(p => ({ ...p, mappings: p.mappings.filter(x=>x.id!==id) }));
  return (
    <>
      <div className="pe-sec-head">
        <h3>Compliance</h3>
        <p>External framework requirements this policy implements, supports, or evidences. Zero mappings is a valid, complete state — plenty of good policies answer to nobody but you.</p>
      </div>
      {form.mappings.length === 0 ? (
        <div className="pe-unmapped">
          <div className="pe-unmapped-title">Unmapped</div>
          <div className="pe-unmapped-body">
            This policy implements no external requirement. It still enforces, still gates deploys, and still reports.
            {source && " This control was imported, so mappings would normally arrive with it — check the source artifact if you expected some."}
          </div>
        </div>
      ) : (
        <div style={{ display:"flex", flexDirection:"column", gap:8 }}>
          {form.mappings.map(m => {
            const req = reqById(m.requirementId), fw = frameworkById(req?.frameworkId);
            const readOnly = m.provenance === "imported";
            return (
              <div key={m.id} className="pe-map">
                <div style={{ display:"flex", justifyContent:"space-between", gap:8, alignItems:"flex-start" }}>
                  <div style={{ minWidth:0 }}>
                    <div className="pe-map-fw">{fw?.name} {fw?.version}</div>
                    <div className="mono pe-map-req">{req?.externalId} <span style={{ fontWeight:400, color:"var(--cf-text-secondary)" }}>· {req?.title}</span></div>
                    <div className="pe-map-rel"><strong>{relationshipMeta(m.relationship).label}</strong> · {m.coverage === "full" ? "Full" : "Partial"} coverage
                      {readOnly && <span className="pe-map-ro">from import · read-only</span>}
                    </div>
                    {m.rationale && <div className="pe-map-why">{m.rationale}</div>}
                  </div>
                  {!readOnly && (
                    <div style={{ display:"flex", gap:4, flexShrink:0 }}>
                      <button className="btn-icon focus-ring" title="Edit mapping" onClick={()=>setMappingEditor(mappingEditor?.mapping?.id===m.id ? null : { mapping:m })}><Icon name="gear" size={12}/></button>
                      <button className="btn-icon focus-ring" title="Remove mapping" onClick={()=>removeMapping(m.id)}><Icon name="x" size={12}/></button>
                    </div>
                  )}
                </div>
                {mappingEditor?.mapping?.id === m.id && (
                  <InlineMappingEditor initial={mappingEditor.mapping} existingMappings={form.mappings}
                    onCancel={()=>setMappingEditor(null)} onSave={(mm)=>{ saveMapping(mm); setMappingEditor(null); }}/>
                )}
              </div>
            );
          })}
        </div>
      )}
      {mappingEditor && !mappingEditor.mapping ? (
        <InlineMappingEditor existingMappings={form.mappings}
          onCancel={()=>setMappingEditor(null)} onSave={(mm)=>{ saveMapping(mm); setMappingEditor(null); }}/>
      ) : !mappingEditor && (
        <button className="btn btn-subtle focus-ring" style={{ marginTop:10 }} onClick={()=>setMappingEditor({})}>
          <Icon name="plus" size={12}/> Add compliance mapping
        </button>
      )}
    </>
  );
}

// ── Evidence ──────────────────────────────────────────────────────────────────
function PEEvidence({ form, set, addEvidence }) {
  return (
    <>
      <div className="pe-sec-head">
        <h3>Evidence</h3>
        <p>What Crystal Forge collects or retains to show an assessor what happened. Optional — enforcement works without it.</p>
      </div>
      {form.evidence.length === 0 && (
        <div className="sd-callout sd-callout-info" style={{ marginBottom:10 }}>
          <Icon name="info" size={13}/>
          <div style={{ fontSize:12 }}>No evidence configured. The policy still gates deploys; it just contributes nothing to an audit package.</div>
        </div>
      )}
      <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
        {form.evidence.map((ev, i) => (
          <div key={i} className="pe-ev">
            <EvidenceEditor ev={ev} onChange={patch => set("evidence", form.evidence.map((e, ix) => ix === i ? { ...e, ...patch } : e))}/>
            <button className="btn-icon focus-ring" title="Remove evidence" onClick={()=>set("evidence", form.evidence.filter((_, ix) => ix !== i))}><Icon name="x" size={12}/></button>
          </div>
        ))}
      </div>
      <select className="input focus-ring" defaultValue="" style={{ maxWidth:250, fontSize:12, marginTop:10 }}
        onChange={e => { if (e.target.value) { addEvidence(e.target.value); e.target.value = ""; } }}>
        <option value="" disabled>+ Add evidence source…</option>
        <option value="command">Command output</option>
        <option value="log">Log line match</option>
        <option value="file">File contents</option>
        <option value="unit_state">systemd unit state</option>
        <option value="eval_attr">Nix eval attribute</option>
        <option value="attestation">Signed attestation</option>
      </select>
    </>
  );
}

Object.assign(window, { PolicyEditor, PEBasics, PEEnforcement, PECompliance, PEEvidence, NixOptionRule, PackageListRule, enforcementPhrase });
