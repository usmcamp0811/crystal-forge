// Policies view — deployment policies + rule builder

function PoliciesView({ onOpenSystem }) {
  const [query, setQuery] = React.useState("");
  const [catFilter, setCatFilter] = React.useState("all");
  const [typeFilter, setTypeFilter] = React.useState("all");
  const [editPolicy, setEditPolicy] = React.useState(null);
  const [addOpen, setAddOpen] = React.useState(false);
  const [drawerPolicy, setDrawerPolicy] = React.useState(null);

  const list = POLICIES.filter(p => {
    if (catFilter !== "all" && (p.category || "deployment") !== catFilter) return false;
    if (typeFilter !== "all" && p.type !== typeFilter) return false;
    if (query) {
      const q = query.toLowerCase();
      if (!p.name.toLowerCase().includes(q) && !p.description.toLowerCase().includes(q)) return false;
    }
    return true;
  });

  // Group the filtered list by category, preserving POLICY_CATEGORIES order.
  const groups = POLICY_CATEGORIES
    .map(cat => ({ cat, items: list.filter(p => (p.category || "deployment") === cat.id) }))
    .filter(g => g.items.length > 0);

  const catCount = (id) => POLICIES.filter(p => (p.category || "deployment") === id).length;

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">Policies</h1>
          <p className="page-subtitle">
            Criteria a system must satisfy to deploy · {POLICY_BUILTIN.length} built-in · {POLICY_CUSTOM.length} custom · governing {SYSTEMS.length} systems
          </p>
        </div>
        <button className="btn btn-primary focus-ring" onClick={() => setAddOpen(true)}>
          <Icon name="plus" size={14}/> New custom policy
        </button>
      </div>

      {/* Category stat strip — doubles as a filter. Click to scope, click again to clear. */}
      <div className="stat-strip pol-cat-strip">
        {POLICY_CATEGORIES.map(cat => {
          const active = catFilter === cat.id;
          return (
            <button key={cat.id}
              className="stat pol-cat-stat"
              onClick={() => setCatFilter(active ? "all" : cat.id)}
              title={cat.blurb}
              style={active ? {
                background:`color-mix(in oklab, ${cat.color} 14%, transparent)`,
                boxShadow:`inset 0 0 0 1px color-mix(in oklab, ${cat.color} 45%, transparent)`,
              } : undefined}>
              <span className="stat-accent" style={{ "--stat-color": cat.color }}/>
              <div className="stat-label" style={{ display:"flex", alignItems:"center", gap:6 }}>
                <Icon name={cat.icon} size={12} style={{ color:cat.color }}/> {cat.label}
              </div>
              <div className="stat-value" style={{ color: cat.color }}>{catCount(cat.id)}</div>
            </button>
          );
        })}
      </div>

      <div className="filterbar">
        <div className="filter-search" style={{ maxWidth:280 }}>
          <Icon name="search"/>
          <input className="input focus-ring" placeholder="Search policies…" value={query} onChange={e=>setQuery(e.target.value)}/>
        </div>
        <div className="seg">
          <button className={catFilter==="all"?"active":""} onClick={()=>setCatFilter("all")}>all</button>
          {POLICY_CATEGORIES.map(c => (
            <button key={c.id} className={catFilter===c.id?"active":""} onClick={()=>setCatFilter(c.id)} title={c.blurb}>
              <span style={{ display:"inline-flex", alignItems:"center", gap:5 }}>
                <span style={{ width:6, height:6, borderRadius:"50%", background:c.color, flexShrink:0 }}/>
                {c.short}
              </span>
            </button>
          ))}
        </div>
        <select className="input focus-ring" value={typeFilter} onChange={e=>setTypeFilter(e.target.value)}
          style={{ width:"auto", fontSize:12, padding:"6px 10px" }} title="Filter by policy type">
          <option value="all">All types</option>
          <option value="builtin">Built-in</option>
          <option value="custom">Custom</option>
        </select>
        {(catFilter !== "all" || typeFilter !== "all" || query) && (
          <button className="btn btn-ghost focus-ring xs" onClick={()=>{ setCatFilter("all"); setTypeFilter("all"); setQuery(""); }}>
            <Icon name="x" size={11}/> Clear
          </button>
        )}
        <span className="filter-count">{list.length} {list.length === 1 ? "policy" : "policies"}</span>
      </div>

      {groups.length === 0 ? (
        <div className="card" style={{ padding:"40px 20px", textAlign:"center", color:"var(--cf-text-muted)" }}>
          <Icon name="search" size={20} style={{ opacity:0.5 }}/>
          <div style={{ marginTop:8, fontSize:13 }}>No policies match these filters.</div>
        </div>
      ) : groups.map(({ cat, items }) => (
        <section key={cat.id} className="pol-group">
          <div className="pol-group-head">
            <span className="pol-group-icon" style={{ background:`color-mix(in oklab, ${cat.color} 16%, transparent)`, color:cat.color }}>
              <Icon name={cat.icon} size={13}/>
            </span>
            <div style={{ minWidth:0 }}>
              <h2 className="pol-group-title">{cat.label} <span className="pol-group-count">{items.length}</span></h2>
              <div className="pol-group-blurb">{cat.blurb}</div>
            </div>
          </div>
          <div className="cards-grid">
            {items.map(p => (
              <PolicyCard key={p.id} policy={p}
                onOpen={() => setDrawerPolicy(p)}
                onEdit={p.type === "custom" ? () => setEditPolicy(p) : null}
              />
            ))}
          </div>
        </section>
      ))}

      {drawerPolicy && (
        <PolicyDrawer
          policy={drawerPolicy}
          onClose={() => setDrawerPolicy(null)}
          onEdit={drawerPolicy.type === "custom" ? () => { setEditPolicy(drawerPolicy); setDrawerPolicy(null); } : null}
          onOpenSystem={onOpenSystem}
        />
      )}
      {(editPolicy || addOpen) && (
        <PolicyFormModal
          mode={addOpen ? "add" : "edit"}
          policy={editPolicy}
          onClose={() => { setEditPolicy(null); setAddOpen(false); }}
        />
      )}
    </div>
  );
}

function PolicyCard({ policy, onOpen, onEdit }) {
  const usage = policyUsage(policy.id);
  const cat = policyCategoryMeta(policy.category || "deployment");
  const disabled = policy.type === "custom" && policy.enabled === false;
  const railColor = disabled ? "#6b7280" : cat.color;

  return (
    <div className="sys-card" onClick={onOpen} style={{ cursor:"pointer", opacity: disabled ? 0.72 : 1 }}>
      <div className="status-rail" style={{ "--status-color": railColor }}/>
      <div className="sys-card-head">
        <div className="sys-title">
          <div className="sys-hostname"><Icon name="file" size={13}/>&nbsp;{policy.name}</div>
          <div style={{ fontSize:11, color:"var(--cf-text-secondary)" }}>{policy.description}</div>
        </div>
        <div style={{ display:"flex", flexDirection:"column", alignItems:"flex-end", gap:5 }}>
          {policy.type === "builtin"
            ? <span className="chip chip-info">built-in</span>
            : <span className="chip chip-healthy">custom</span>}
          {disabled && <span className="chip chip-unknown" style={{ fontSize:9 }}>disabled</span>}
          {policy.severity && (
            <span className="chip" style={{ fontSize:9,
              color: policy.severity === "high" ? "#f87171" : policy.severity === "medium" ? "#fbbf24" : "#60a5fa",
              background: `color-mix(in oklab, ${policy.severity === "high" ? "#f87171" : policy.severity === "medium" ? "#fbbf24" : "#60a5fa"} 14%, transparent)` }}>
              {policy.severity === "high" ? "CAT I" : policy.severity === "medium" ? "CAT II" : "CAT III"}
            </span>
          )}
        </div>
      </div>
      {/* card chip end */}

      <div>
        <div style={{ fontSize:10, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", fontWeight:600, marginBottom:6 }}>Rules</div>
        <div style={{ display:"flex", flexDirection:"column", gap:4 }}>
          {policy.rules.length === 0 ? (
            <div style={{ fontSize:11, color:"var(--cf-text-muted)", fontStyle:"italic" }}>No rules — operator approves directly.</div>
          ) : policy.rules.map((r, i) => (
            <div key={i} style={{ display:"flex", alignItems:"center", gap:6, fontSize:11 }}>
              <Icon name="check" size={10} style={{ color:"#34d399", flexShrink:0 }}/>
              <span style={{ color:"var(--cf-text-primary)" }}>{ruleDescription(r)}</span>
            </div>
          ))}
        </div>
      </div>

      <div style={{ paddingTop:10, borderTop:"1px solid var(--cf-divider)", display:"flex", justifyContent:"space-between", alignItems:"center" }}>
        <div style={{ display:"flex", alignItems:"center", gap:8 }}>
          <Icon name="server" size={11} style={{ color:"var(--cf-text-muted)" }}/>
          <span className="mono" style={{ fontSize:12, fontWeight:600 }}>{usage.count}</span>
          <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>systems use this</span>
        </div>
        {onEdit && (
          <button className="btn btn-subtle focus-ring" style={{ padding:"4px 10px", fontSize:12 }} onClick={e=>{ e.stopPropagation(); onEdit(); }}>
            <Icon name="gear" size={12}/> Edit
          </button>
        )}
      </div>
    </div>
  );
}

function ruleDescription(r) {
  switch (r.kind) {
    case "eval_passed":       return "Evaluation must pass";
    case "build_succeeded":   return "Build must succeed (and be cached)";
    case "pin_required":      return "Pinned to a specific commit";
    case "cve_block":         return `Block deploy: max ${r.maxAllowed} ${r.severity} CVE${r.maxAllowed === 1 ? "" : "s"}`;
    case "time_window":       return `Deploy window: ${r.days.join(",")} ${r.from}-${r.to} ${r.tz}`;
    case "approval_required": return `${r.count} approver${r.count === 1 ? "" : "s"} required (${r.role})`;
    case "rollout_percent":   return `Canary: ${r.percent}% at a time, observe ${r.observeMin}min`;
    case "packages_installed":return `Packages present: ${(r.packages||[]).join(", ")}`;
    case "nixos_option":      return `config.${r.path} ${r.op} ${r.value}`;
    case "custom_eval":       return r.message || "Custom nix assertion";
    default: return r.kind;
  }
}

function evidenceSummary(ev) {
  switch (ev.kind) {
    case "command":     return <span className="mono" style={{ fontSize:11, fontWeight:400 }}>{ev.cmd}</span>;
    case "log":         return <span style={{ fontSize:12, fontWeight:400 }}>{ev.source}: {ev.unit} matches <span className="mono">{ev.match}</span></span>;
    case "file":        return <span className="mono" style={{ fontSize:11, fontWeight:400 }}>{ev.path}</span>;
    case "unit_state":  return <span style={{ fontSize:12, fontWeight:400 }}><span className="mono">{ev.unit}</span> is {ev.state}</span>;
    case "eval_attr":   return <span className="mono" style={{ fontSize:11, fontWeight:400 }}>{ev.attr}</span>;
    case "attestation": return <span style={{ fontSize:12, fontWeight:400 }}>{ev.note}</span>;
    default: return ev.kind;
  }
}

function PolicyDrawer({ policy, onClose, onEdit, onOpenSystem }) {
  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const usage = policyUsage(policy.id);

  return (
    <>
      <div className="fl-tray-backdrop" onClick={onClose}/>
      <aside className="fl-tray">
        <header className="fl-tray-head">
          <div style={{ display:"flex", alignItems:"center", gap:12, minWidth:0, flex:1 }}>
            <Icon name="file" size={18} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
            <div style={{ minWidth:0 }}>
              <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                <span className="mono" style={{ fontWeight:700, fontSize:15 }}>{policy.name}</span>
                {(() => { const c = policyCategoryMeta(policy.category || "deployment"); return (
                  <span className="chip" style={{ color:c.color, background:`color-mix(in oklab, ${c.color} 14%, transparent)`, display:"inline-flex", alignItems:"center", gap:4 }}>
                    <Icon name={c.icon} size={10}/> {c.label}
                  </span>
                ); })()}
                {policy.type === "builtin"
                  ? <span className="chip chip-info">built-in</span>
                  : <span className="chip chip-healthy">custom</span>}
              </div>
              {/* detail chip end */}
              <div style={{ fontSize:12, color:"var(--cf-text-secondary)", marginTop:3 }}>{policy.description}</div>
            </div>
          </div>
          <div style={{ display:"flex", gap:6 }}>
            {onEdit && <button className="btn btn-ghost focus-ring xs" onClick={onEdit}><Icon name="gear" size={11}/> Edit</button>}
            <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16}/></button>
          </div>
        </header>

        <div className="ed-stats">
          <div className="ed-stat"><div className="ed-stat-label">Systems</div><div className="ed-stat-val">{usage.count}</div></div>
          <div className="ed-stat"><div className="ed-stat-label">Rules</div><div className="ed-stat-val">{policy.rules.length}</div></div>
          <div className="ed-stat"><div className="ed-stat-label">Type</div><div className="ed-stat-val" style={{ fontSize:14 }}>{policy.type}</div></div>
          {policy.lastModified && <div className="ed-stat"><div className="ed-stat-label">Modified</div><div className="ed-stat-val" style={{ fontSize:13 }}>{policy.lastModified}</div></div>}
          {policy.createdBy && <div className="ed-stat"><div className="ed-stat-label">Owner</div><div className="ed-stat-val mono" style={{ fontSize:13 }}>{policy.createdBy}</div></div>}
        </div>

        <div className="ed-body" style={{ padding:"18px 22px", overflow:"auto", display:"flex", flexDirection:"column", gap:18 }}>
          {policy.rationale && (
            <section>
              <h3 style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", margin:"0 0 8px", fontWeight:600 }}>Rationale</h3>
              <div style={{ fontSize:13, color:"var(--cf-text-primary)", lineHeight:1.5 }}>{policy.rationale}</div>
            </section>
          )}
          <section>
            <h3 style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", margin:"0 0 8px", fontWeight:600 }}>Rules</h3>
            {policy.rules.length === 0 ? (
              <div className="sd-callout sd-callout-info">
                <Icon name="check" size={13}/>
                <div style={{ fontSize:12 }}>No automated rules — every deploy is operator-approved.</div>
              </div>
            ) : (
              <div style={{ display:"flex", flexDirection:"column", gap:8 }}>
                {policy.rules.map((r, i) => (
                  <div key={i} style={{ padding:"10px 12px", background:"var(--cf-subtle-bg)", borderRadius:8, border:"1px solid var(--cf-divider)" }}>
                    <div style={{ fontSize:12, fontWeight:600, color:"var(--cf-text-primary)" }}>{ruleDescription(r)}</div>
                    <div className="mono" style={{ fontSize:10, color:"var(--cf-text-muted)", marginTop:4 }}>kind: {r.kind}</div>
                  </div>
                ))}
              </div>
            )}
          </section>
          {policy.evidence && policy.evidence.length > 0 && (
            <section>
              <h3 style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", margin:"0 0 8px", fontWeight:600 }}>
                Evidence for ATO · {policy.evidence.length}
              </h3>
              <div style={{ display:"flex", flexDirection:"column", gap:8 }}>
                {policy.evidence.map((ev, i) => (
                  <div key={i} style={{ padding:"10px 12px", background:"var(--cf-subtle-bg)", borderRadius:8, border:"1px solid var(--cf-divider)" }}>
                    <div style={{ fontSize:12, fontWeight:600, color:"var(--cf-text-primary)", display:"flex", alignItems:"center", gap:6 }}>
                      <span className="chip chip-unknown" style={{ fontSize:9 }}>{ev.kind.replace("_"," ")}</span>
                      {evidenceSummary(ev)}
                    </div>
                  </div>
                ))}
              </div>
            </section>
          )}
          <section>
            <h3 style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", margin:"0 0 8px", fontWeight:600 }}>
              Systems using this policy · {usage.count}
            </h3>
            {usage.count === 0 ? (
              <div style={{ fontSize:12, color:"var(--cf-text-muted)", padding:"8px 0" }}>No systems are assigned to this policy.</div>
            ) : (
              <>
                <div style={{ display:"flex", gap:6, flexWrap:"wrap", marginBottom:10 }}>
                  {Object.entries(usage.byEnv).map(([env, n]) => (
                    <span key={env} style={{ display:"inline-flex", alignItems:"center", gap:6 }}>
                      <EnvBadge env={env}/>
                      <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{n}</span>
                    </span>
                  ))}
                </div>
                <div className="card" style={{ overflow:"hidden", border:"1px solid var(--cf-divider)" }}>
                  <table className="sys-table" style={{ fontSize:12 }}>
                    <tbody>
                      {usage.systems.slice(0, 8).map(sys => (
                        <tr key={sys.id} style={{ cursor:"pointer" }} onClick={() => { onClose(); onOpenSystem?.(sys); }}>
                          <td>
                            <div style={{ display:"flex", alignItems:"center", gap:8 }}>
                              <span className="status-dot" style={{ "--status-color": sys.statusColor }}/>
                              <span className="mono" style={{ fontWeight:600 }}>{sys.hostname}</span>
                            </div>
                          </td>
                          <td><EnvBadge env={sys.environment}/></td>
                          <td className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{sys.flake}</td>
                          <td style={{ textAlign:"right" }}><Icon name="arrow-right" size={13} style={{ color:"var(--cf-text-muted)" }}/></td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                {usage.systems.length > 8 && (
                  <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:6 }}>+ {usage.systems.length - 8} more</div>
                )}
              </>
            )}
          </section>
        </div>
      </aside>
    </>
  );
}

function PolicyFormModal({ mode, policy, onClose }) {
  const isEdit = mode === "edit";
  const [form, setForm] = React.useState(() => isEdit && policy ? {
    name: policy.name,
    description: policy.description,
    category: policy.category || "deployment",
    rationale: policy.rationale || "",
    severity: policy.severity || "medium",
    enabled: policy.enabled !== false,
    rules: [...policy.rules],
    evidence: policy.evidence ? policy.evidence.map(e => ({ ...e })) : [],
  } : {
    name: "",
    description: "",
    category: "deployment",
    rationale: "",
    severity: "medium",
    enabled: true,
    rules: [{ kind:"eval_passed" }, { kind:"build_succeeded" }],
    evidence: [],
  });
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const set = (k,v) => setForm(p => ({ ...p, [k]: v }));

  const addRule = (kind) => {
    const defaults = {
      eval_passed: {},
      build_succeeded: {},
      cve_block: { severity:"critical", maxAllowed:0 },
      time_window: { days:["mon","tue","wed","thu","fri"], from:"09:00", to:"17:00", tz:"America/New_York" },
      approval_required: { count:2, role:"admin" },
      rollout_percent: { percent:25, observeMin:30 },
      packages_installed: { packages:["openssh","auditd"] },
      nixos_option: { path:"services.openssh.settings.PermitRootLogin", op:"==", value:"\"no\"" },
      custom_eval: { expr:"config.services.openssh.enable == true", message:"SSH must be enabled" },
    };
    set("rules", [...form.rules, { kind, ...defaults[kind] }]);
  };
  const removeRule = (idx) => set("rules", form.rules.filter((_, i) => i !== idx));
  const updateRule = (idx, patch) => set("rules", form.rules.map((r, i) => i === idx ? { ...r, ...patch } : r));

  const addEvidence = (kind) => {
    const defaults = {
      command:    { kind:"command",    cmd:"sshd -T | grep permitrootlogin", expect:"permitrootlogin no" },
      log:        { kind:"log",        source:"journald", unit:"auditd.service", match:"audit: rules loaded" },
      file:       { kind:"file",       path:"/etc/issue", note:"Must contain USG banner text" },
      unit_state: { kind:"unit_state", unit:"auditd.service", state:"active" },
      eval_attr:  { kind:"eval_attr",  attr:"config.services.openssh.settings.PermitRootLogin" },
      attestation:{ kind:"attestation",note:"Ed25519-signed agent fingerprint snapshot at deploy time" },
    };
    set("evidence", [...form.evidence, defaults[kind]]);
  };
  const removeEvidence = (idx) => set("evidence", form.evidence.filter((_, i) => i !== idx));
  const updateEvidence = (idx, patch) => set("evidence", form.evidence.map((e, i) => i === idx ? { ...e, ...patch } : e));

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(680px,96vw)", maxHeight:"92vh" }}>
        {confirmDelete ? (
          <DeletePolicyConfirm policy={policy} onCancel={()=>setConfirmDelete(false)} onConfirm={onClose}/>
        ) : (
          <>
            <div className="modal-head">
              <h2><Icon name={isEdit ? "gear" : "plus"} size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>
                {isEdit ? `Edit ${policy.name}` : "New custom policy"}
              </h2>
              <p>{isEdit ? "Update the rules and rationale." : "Compose a policy from gate rules. Systems can be assigned this policy from their edit dialog."}</p>
            </div>
            <div className="modal-body" style={{ overflowY:"auto" }}>
              <div style={{ display:"grid", gridTemplateColumns:"1fr", gap:14 }}>
                <div className="field">
                  <label>Name</label>
                  <input className="input focus-ring mono" value={form.name} onChange={e=>set("name",e.target.value)} placeholder="e.g. canary-25"/>
                </div>
              </div>
              <div className="field">
                <label>Description</label>
                <input className="input focus-ring" value={form.description} onChange={e=>set("description",e.target.value)} placeholder="One-line summary shown in the registry"/>
              </div>
              <div className="field">
                <label>Category</label>
                <div style={{ display:"grid", gridTemplateColumns:"repeat(auto-fit, minmax(150px, 1fr))", gap:8 }}>
                  {POLICY_CATEGORIES.map(c => {
                    const active = form.category === c.id;
                    return (
                      <button key={c.id} type="button" onClick={()=>set("category", c.id)}
                        className="focus-ring"
                        style={{
                          display:"flex", alignItems:"flex-start", gap:9, textAlign:"left",
                          padding:"9px 11px", borderRadius:9, cursor:"pointer",
                          background: active ? `color-mix(in oklab, ${c.color} 12%, transparent)` : "var(--cf-subtle-bg)",
                          border: `1px solid ${active ? `color-mix(in oklab, ${c.color} 55%, transparent)` : "var(--cf-divider)"}`,
                        }}>
                        <span style={{ flexShrink:0, width:24, height:24, borderRadius:6, display:"grid", placeItems:"center",
                          background:`color-mix(in oklab, ${c.color} 16%, transparent)`, color:c.color }}>
                          <Icon name={c.icon} size={13}/>
                        </span>
                        <span style={{ minWidth:0 }}>
                          <span style={{ display:"block", fontSize:12, fontWeight:600, color: active ? c.color : "var(--cf-text-primary)" }}>{c.label}</span>
                          <span style={{ display:"block", fontSize:10.5, color:"var(--cf-text-muted)", lineHeight:1.35, marginTop:2 }}>{c.blurb}</span>
                        </span>
                      </button>
                    );
                  })}
                </div>
              </div>
              <div className="field">
                <label>Severity</label>
                <div className="seg seg-sev" style={{ width:"fit-content" }}>
                  {[
                    { v:"high",   l:"High (CAT I)",    c:"#f87171" },
                    { v:"medium", l:"Medium (CAT II)", c:"#fbbf24" },
                    { v:"low",    l:"Low (CAT III)",   c:"#60a5fa" },
                  ].map(o => (
                    <button key={o.v}
                      className={form.severity === o.v ? "active" : ""}
                      onClick={()=>set("severity", o.v)}
                      style={form.severity === o.v ? {
                        color: o.c,
                        background: `color-mix(in oklab, ${o.c} 16%, transparent)`,
                        boxShadow: `inset 0 0 0 1px color-mix(in oklab, ${o.c} 45%, transparent)`,
                      } : { color: "var(--cf-text-secondary)" }}>
                      <span style={{ display:"inline-flex", alignItems:"center", gap:6 }}>
                        <span style={{ width:7, height:7, borderRadius:"50%", background:o.c }}/>
                        {o.l}
                      </span>
                    </button>
                  ))}
                </div>
                <div className="help">Drives how failures of this control are weighted in compliance scoring and evidence reports.</div>
              </div>
              <div className="field">
                <label>Rationale</label>
                <textarea className="input focus-ring" rows={2} value={form.rationale} onChange={e=>set("rationale",e.target.value)}
                  placeholder="Why this policy exists — shown in detail view" style={{ resize:"vertical" }}/>
              </div>

              {/* Rules */}
              <div style={{ marginTop:6 }}>
                <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline", marginBottom:8 }}>
                  <label style={{ fontSize:12, fontWeight:600 }}>Assertions &amp; gate rules ({form.rules.length})</label>
                  <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>All must hold — each compiles to a nix-eval-job check.</span>
                </div>
                <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
                  {form.rules.map((r, i) => (
                    <div key={i} style={{ display:"grid", gridTemplateColumns:"1fr auto", gap:8, alignItems:"center", padding:"8px 10px", background:"var(--cf-subtle-bg)", borderRadius:8 }}>
                      <RuleEditor rule={r} onChange={patch => updateRule(i, patch)}/>
                      <button className="btn-icon focus-ring" onClick={()=>removeRule(i)} title="Remove rule">
                        <Icon name="x" size={13}/>
                      </button>
                    </div>
                  ))}
                </div>
                <div style={{ marginTop:8, display:"flex", gap:8, flexWrap:"wrap" }}>
                  <select className="input focus-ring" defaultValue=""
                    onChange={e => { if (e.target.value) { addRule(e.target.value); e.target.value = ""; } }}
                    style={{ maxWidth:260, fontSize:12 }}>
                    <option value="" disabled>+ Add assertion / rule…</option>
                    <optgroup label="NixOS config assertions">
                      <option value="packages_installed">Packages installed</option>
                      <option value="nixos_option">NixOS option equals</option>
                      <option value="custom_eval">Custom nix expression</option>
                    </optgroup>
                    <optgroup label="Pipeline gates">
                      <option value="eval_passed">Eval must pass</option>
                      <option value="build_succeeded">Build must succeed</option>
                      <option value="cve_block">CVE gate</option>
                    </optgroup>
                    <optgroup label="Rollout gates">
                      <option value="time_window">Time window</option>
                      <option value="approval_required">Approval required</option>
                      <option value="rollout_percent">Canary rollout</option>
                    </optgroup>
                  </select>
                </div>
              </div>

              {/* Evidence */}
              <div style={{ marginTop:6 }}>
                <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline", marginBottom:8 }}>
                  <label style={{ fontSize:12, fontWeight:600 }}>Evidence for ATO ({form.evidence.length})</label>
                  <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>Artifacts collected to prove compliance to an assessor.</span>
                </div>
                {form.evidence.length === 0 && (
                  <div className="sd-callout sd-callout-info" style={{ marginBottom:8 }}>
                    <Icon name="file" size={13}/>
                    <div style={{ fontSize:12 }}>No evidence defined. Without it, this policy gates deploys but produces nothing for an audit package. Add command output, logs, or attestations.</div>
                  </div>
                )}
                <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
                  {form.evidence.map((ev, i) => (
                    <div key={i} style={{ display:"grid", gridTemplateColumns:"1fr auto", gap:8, alignItems:"flex-start", padding:"8px 10px", background:"var(--cf-subtle-bg)", borderRadius:8 }}>
                      <EvidenceEditor ev={ev} onChange={patch => updateEvidence(i, patch)}/>
                      <button className="btn-icon focus-ring" onClick={()=>removeEvidence(i)} title="Remove evidence">
                        <Icon name="x" size={13}/>
                      </button>
                    </div>
                  ))}
                </div>
                <div style={{ marginTop:8 }}>
                  <select className="input focus-ring" defaultValue=""
                    onChange={e => { if (e.target.value) { addEvidence(e.target.value); e.target.value = ""; } }}
                    style={{ maxWidth:260, fontSize:12 }}>
                    <option value="" disabled>+ Add evidence source…</option>
                    <option value="command">Command output</option>
                    <option value="log">Log line match</option>
                    <option value="file">File contents</option>
                    <option value="unit_state">systemd unit state</option>
                    <option value="eval_attr">Nix eval attribute</option>
                    <option value="attestation">Signed attestation</option>
                  </select>
                </div>
              </div>

              {isEdit && (
                <div style={{ marginTop:10, paddingTop:14, borderTop:"1px solid var(--cf-divider)" }}>
                  <div style={{ fontSize:11, fontWeight:600, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", marginBottom:8 }}>Danger zone</div>
                  <button className="btn btn-ghost focus-ring" onClick={()=>setConfirmDelete(true)} style={{ color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }}>
                    <Icon name="x" size={12}/> Remove policy
                  </button>
                </div>
              )}
            </div>
            <div className="modal-foot">
              <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
              <button className="btn btn-primary focus-ring" onClick={onClose} disabled={!form.name}>
                <Icon name="check" size={13}/> {isEdit ? "Save changes" : "Create policy"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function EvidenceEditor({ ev, onChange }) {
  const Wrap = ({ icon, label, children }) => (
    <div style={{ display:"flex", flexDirection:"column", gap:4, fontSize:12, width:"100%" }}>
      <span style={{ display:"flex", alignItems:"center", gap:6, fontWeight:600 }}>
        <Icon name={icon} size={11} style={{ color:"var(--cf-brand-purple)", verticalAlign:"text-bottom" }}/> {label}
      </span>
      {children}
    </div>
  );
  const inp = { fontSize:11, padding:"5px 8px" };
  switch (ev.kind) {
    case "command":
      return (
        <Wrap icon="terminal" label="Command output">
          <input className="input focus-ring mono" value={ev.cmd} onChange={e=>onChange({ cmd:e.target.value })} placeholder="sshd -T | grep permitrootlogin" style={inp}/>
          <div style={{ display:"flex", alignItems:"center", gap:6 }}>
            <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>expect output contains</span>
            <input className="input focus-ring mono" value={ev.expect} onChange={e=>onChange({ expect:e.target.value })} placeholder="permitrootlogin no" style={{ ...inp, flex:1 }}/>
          </div>
        </Wrap>
      );
    case "log":
      return (
        <Wrap icon="terminal" label="Log line match">
          <div style={{ display:"flex", gap:6, flexWrap:"wrap" }}>
            <select className="input focus-ring" value={ev.source} onChange={e=>onChange({ source:e.target.value })} style={{ ...inp, width:"auto" }}>
              <option value="journald">journald</option><option value="auditd">auditd</option><option value="file">file</option>
            </select>
            <input className="input focus-ring mono" value={ev.unit} onChange={e=>onChange({ unit:e.target.value })} placeholder="auditd.service" style={{ ...inp, flex:1, minWidth:140 }}/>
          </div>
          <input className="input focus-ring mono" value={ev.match} onChange={e=>onChange({ match:e.target.value })} placeholder="regex / substring to match" style={inp}/>
        </Wrap>
      );
    case "file":
      return (
        <Wrap icon="file" label="File contents">
          <input className="input focus-ring mono" value={ev.path} onChange={e=>onChange({ path:e.target.value })} placeholder="/etc/issue" style={inp}/>
          <input className="input focus-ring" value={ev.note} onChange={e=>onChange({ note:e.target.value })} placeholder="What to look for / why it proves compliance" style={inp}/>
        </Wrap>
      );
    case "unit_state":
      return (
        <Wrap icon="server" label="systemd unit state">
          <div style={{ display:"flex", gap:6, alignItems:"center", flexWrap:"wrap" }}>
            <input className="input focus-ring mono" value={ev.unit} onChange={e=>onChange({ unit:e.target.value })} placeholder="auditd.service" style={{ ...inp, flex:1, minWidth:140 }}/>
            <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>is</span>
            <select className="input focus-ring" value={ev.state} onChange={e=>onChange({ state:e.target.value })} style={{ ...inp, width:"auto" }}>
              <option value="active">active</option><option value="enabled">enabled</option><option value="masked">masked</option>
            </select>
          </div>
        </Wrap>
      );
    case "eval_attr":
      return (
        <Wrap icon="cube" label="Nix eval attribute">
          <input className="input focus-ring mono" value={ev.attr} onChange={e=>onChange({ attr:e.target.value })} placeholder="config.services.openssh.settings.PermitRootLogin" style={inp}/>
          <span className="mono" style={{ fontSize:10, color:"var(--cf-text-muted)" }}>Captured from the evaluated config — no host access needed.</span>
        </Wrap>
      );
    case "attestation":
      return (
        <Wrap icon="key" label="Signed attestation">
          <input className="input focus-ring" value={ev.note} onChange={e=>onChange({ note:e.target.value })} placeholder="What the agent attests to (signed snapshot)" style={inp}/>
          <span className="mono" style={{ fontSize:10, color:"var(--cf-text-muted)" }}>Ed25519-signed by the agent at collection time.</span>
        </Wrap>
      );
    default:
      return <span style={{ fontSize:12, fontStyle:"italic" }}>{ev.kind}</span>;
  }
}

function RuleEditor({ rule, onChange }) {
  switch (rule.kind) {
    case "eval_passed":
      return <span style={{ fontSize:12 }}><Icon name="check" size={11} style={{ color:"#34d399", verticalAlign:"text-bottom" }}/> Evaluation must pass</span>;
    case "build_succeeded":
      return <span style={{ fontSize:12 }}><Icon name="check" size={11} style={{ color:"#34d399", verticalAlign:"text-bottom" }}/> Build must succeed</span>;
    case "cve_block":
      return (
        <div style={{ display:"flex", alignItems:"center", gap:8, fontSize:12, flexWrap:"wrap" }}>
          <span>Block deploy when</span>
          <select className="input focus-ring" value={rule.severity} onChange={e=>onChange({ severity:e.target.value })} style={{ width:"auto", fontSize:12, padding:"4px 8px" }}>
            <option value="critical">critical</option><option value="high">high</option><option value="medium">medium</option>
          </select>
          <span>CVEs exceed</span>
          <input type="number" className="input focus-ring mono" min={0} value={rule.maxAllowed} onChange={e=>onChange({ maxAllowed:parseInt(e.target.value,10) || 0 })} style={{ width:60, fontSize:12, padding:"4px 8px" }}/>
        </div>
      );
    case "time_window":
      return (
        <div style={{ display:"flex", alignItems:"center", gap:8, fontSize:12, flexWrap:"wrap" }}>
          <span>Only between</span>
          <input className="input focus-ring mono" value={rule.from} onChange={e=>onChange({ from:e.target.value })} style={{ width:70, fontSize:12, padding:"4px 8px" }}/>
          <span>–</span>
          <input className="input focus-ring mono" value={rule.to} onChange={e=>onChange({ to:e.target.value })} style={{ width:70, fontSize:12, padding:"4px 8px" }}/>
          <span>on</span>
          <input className="input focus-ring mono" value={rule.days.join(",")} onChange={e=>onChange({ days:e.target.value.split(",").map(s=>s.trim()) })} style={{ width:140, fontSize:12, padding:"4px 8px" }}/>
          <span className="mono" style={{ color:"var(--cf-text-muted)", fontSize:11 }}>{rule.tz}</span>
        </div>
      );
    case "approval_required":
      return (
        <div style={{ display:"flex", alignItems:"center", gap:8, fontSize:12, flexWrap:"wrap" }}>
          <span>Require</span>
          <input type="number" className="input focus-ring mono" min={1} value={rule.count} onChange={e=>onChange({ count:parseInt(e.target.value,10) || 1 })} style={{ width:50, fontSize:12, padding:"4px 8px" }}/>
          <span>approver(s) with role</span>
          <select className="input focus-ring" value={rule.role} onChange={e=>onChange({ role:e.target.value })} style={{ width:"auto", fontSize:12, padding:"4px 8px" }}>
            <option value="admin">admin</option><option value="operator">operator</option><option value="any">any</option>
          </select>
        </div>
      );
    case "rollout_percent":
      return (
        <div style={{ display:"flex", alignItems:"center", gap:8, fontSize:12, flexWrap:"wrap" }}>
          <span>Roll out</span>
          <input type="number" className="input focus-ring mono" min={1} max={100} value={rule.percent} onChange={e=>onChange({ percent:parseInt(e.target.value,10) || 25 })} style={{ width:55, fontSize:12, padding:"4px 8px" }}/>
          <span>% at a time, observe</span>
          <input type="number" className="input focus-ring mono" min={1} value={rule.observeMin} onChange={e=>onChange({ observeMin:parseInt(e.target.value,10) || 30 })} style={{ width:55, fontSize:12, padding:"4px 8px" }}/>
          <span>min</span>
        </div>
      );
    case "packages_installed":
      return (
        <div style={{ display:"flex", flexDirection:"column", gap:4, fontSize:12, width:"100%" }}>
          <span style={{ display:"flex", alignItems:"center", gap:6 }}>
            <Icon name="cube" size={11} style={{ color:"var(--cf-brand-purple)", verticalAlign:"text-bottom" }}/>
            Assert these packages are in the system closure
          </span>
          <input className="input focus-ring mono" value={(rule.packages||[]).join(", ")}
            onChange={e=>onChange({ packages: e.target.value.split(",").map(s=>s.trim()).filter(Boolean) })}
            placeholder="openssh, auditd, aide" style={{ fontSize:12, padding:"5px 8px" }}/>
          <span className="mono" style={{ fontSize:10, color:"var(--cf-text-muted)" }}>
            → builtins.any (p: p.pname == "…") config.environment.systemPackages
          </span>
        </div>
      );
    case "nixos_option":
      return (
        <div style={{ display:"flex", flexDirection:"column", gap:4, fontSize:12, width:"100%" }}>
          <span style={{ display:"flex", alignItems:"center", gap:6 }}>
            <Icon name="file" size={11} style={{ color:"var(--cf-brand-purple)", verticalAlign:"text-bottom" }}/>
            Assert a NixOS option value
          </span>
          <div style={{ display:"flex", gap:6, alignItems:"center", flexWrap:"wrap" }}>
            <input className="input focus-ring mono" value={rule.path} onChange={e=>onChange({ path:e.target.value })}
              placeholder="services.openssh.settings.PermitRootLogin" style={{ fontSize:11, padding:"5px 8px", flex:1, minWidth:200 }}/>
            <select className="input focus-ring mono" value={rule.op} onChange={e=>onChange({ op:e.target.value })} style={{ width:"auto", fontSize:12, padding:"5px 6px" }}>
              <option value="==">==</option><option value="!=">!=</option><option value=">=">≥</option><option value="<=">≤</option>
            </select>
            <input className="input focus-ring mono" value={rule.value} onChange={e=>onChange({ value:e.target.value })}
              placeholder={'"no"'} style={{ width:90, fontSize:11, padding:"5px 8px" }}/>
          </div>
          <span className="mono" style={{ fontSize:10, color:"var(--cf-text-muted)" }}>→ config.{rule.path} {rule.op} {rule.value}</span>
        </div>
      );
    case "custom_eval":
      return (
        <div style={{ display:"flex", flexDirection:"column", gap:4, fontSize:12, width:"100%" }}>
          <span style={{ display:"flex", alignItems:"center", gap:6 }}>
            <Icon name="terminal" size={11} style={{ color:"var(--cf-brand-purple)", verticalAlign:"text-bottom" }}/>
            Custom nix expression (must evaluate to <span className="mono">true</span>)
          </span>
          <textarea className="input focus-ring mono" rows={2} value={rule.expr} onChange={e=>onChange({ expr:e.target.value })}
            placeholder="config.networking.firewall.enable == true" style={{ fontSize:11, padding:"6px 8px", resize:"vertical" }}/>
          <input className="input focus-ring" value={rule.message} onChange={e=>onChange({ message:e.target.value })}
            placeholder="Failure message shown when assertion fails" style={{ fontSize:11, padding:"5px 8px" }}/>
        </div>
      );
    default:
      return <span style={{ fontSize:12, fontStyle:"italic" }}>{rule.kind}</span>;
  }
}

function DeletePolicyConfirm({ policy, onCancel, onConfirm }) {
  const usage = policyUsage(policy.id);
  const [typed, setTyped] = React.useState("");
  const matches = typed === policy.name;
  const hasUsage = usage.count > 0;
  return (
    <>
      <div className="modal-head" style={{ background:"rgba(248,113,113,0.06)" }}>
        <h2 style={{ color:"#fecaca", display:"flex", alignItems:"center", gap:8 }}>
          <Icon name="warn" size={16} style={{ color:"#f87171" }}/>
          Remove policy
        </h2>
        <p>This deletes the <span className="mono" style={{ fontWeight:600 }}>{policy.name}</span> policy.</p>
      </div>
      <div className="modal-body">
        {hasUsage && (
          <div className="sd-callout sd-callout-danger" style={{ marginBottom:12 }}>
            <Icon name="warn" size={14}/>
            <div style={{ fontSize:12, color:"#fecaca" }}>
              <strong>{usage.count} system{usage.count === 1 ? "" : "s"} still use this policy.</strong> Reassign them first.
            </div>
          </div>
        )}
        <div className="field">
          <label>Type <span className="mono" style={{ color:"#fecaca", fontWeight:700 }}>{policy.name}</span> to confirm</label>
          <input className="input focus-ring mono" placeholder={policy.name} value={typed} onChange={e=>setTyped(e.target.value)} autoFocus disabled={hasUsage}/>
        </div>
      </div>
      <div className="modal-foot">
        <button className="btn btn-ghost focus-ring" onClick={onCancel}>Cancel</button>
        <button className="btn focus-ring" disabled={!matches || hasUsage} onClick={onConfirm}
          style={{ background: matches && !hasUsage ? "#dc2626" : "var(--cf-subtle-bg)", color: matches && !hasUsage ? "white" : "var(--cf-text-muted)" }}>
          <Icon name="x" size={13}/> Remove policy
        </button>
      </div>
    </>
  );
}

Object.assign(window, { PoliciesView, RuleEditor });
