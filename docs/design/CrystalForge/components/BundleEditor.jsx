// Bundle editor — sectioned shell matching the policy editor (pe-*), with
// rule-based control inclusion so a 100+ control bundle stays authorable:
// each rule is a match expression, individual controls are pinned or excluded
// only as exceptions, and the resolved set is materialized into policyIds on save.

const BE_SECTIONS = [
  { id:"basics",   label:"Basics",   icon:"file" },
  { id:"controls", label:"Controls", icon:"shield" },
];

function bundleControlHay(p) {
  return [p.name, p.description, p.controlFamily, (p.srgIds||[]).join(" "), (p.cciIds||[]).join(" "), p.severity]
    .filter(Boolean).join(" ").toLowerCase();
}

// Severity carries the same CAT palette the Policies view uses.
function beSev(p) {
  const c = p.severity === "high" ? "#f87171" : p.severity === "medium" ? "#fbbf24" : p.severity === "low" ? "#60a5fa" : null;
  if (!c) return null;
  return { color:c, label: p.severity === "high" ? "CAT I" : p.severity === "medium" ? "CAT II" : "CAT III" };
}
function BESevChip({ p }) {
  const s = beSev(p);
  if (!s) return null;
  return <span className="chip" style={{ fontSize:9, flexShrink:0, color:s.color, background:`color-mix(in oklab, ${s.color} 14%, transparent)` }}>{s.label}</span>;
}

// The bundle's control set: ids that exist in the catalog, in selection order.
function resolveBundleControls(catalog, selectedIds) {
  const known = new Set(catalog.map(p => p.id));
  return new Set((selectedIds || []).filter(id => known.has(id)));
}

function BundleEditor({ bundle: editBundle, onClose, onDelete }) {
  const isEdit = !!editBundle;
  const [section, setSection] = React.useState("basics");
  // Authoring a brand-new policy without losing this editor's state: the bundle
  // editor stays mounted underneath, and any policy created is pinned on return.
  const [newPolicyOpen, setNewPolicyOpen] = React.useState(false);
  const [catalogNonce, setCatalogNonce] = React.useState(0);
  const knownIds = React.useRef(null);
  const catalog = React.useMemo(
    () => (typeof POLICIES !== "undefined" ? POLICIES : []).filter(p => p.publicationState !== "deprecated"), [catalogNonce]);
  const [form, setForm] = React.useState(() => ({
    name: editBundle?.name || "",
    framework: editBundle?.framework || "DISA STIG",
    version: editBundle?.version || "",
    description: editBundle?.description || "",
    requiredEnvs: editBundle?.requiredEnvs ? [...editBundle.requiredEnvs] : ["production"],
    pinIds: [...(editBundle?.policyIds || [])],
  }));
  const set = (k, v) => setForm(p => ({ ...p, [k]: v }));

  const [customFrameworks, setCustomFrameworks] = React.useState(() => loadCustomFrameworks());
  const [confirmDel, setConfirmDel] = React.useState(false);

  const resolved = React.useMemo(() => resolveBundleControls(catalog, form.pinIds), [catalog, form.pinIds]);
  const resolvedIds = React.useMemo(() => [...resolved], [resolved]);

  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape" && !confirmDel && !newPolicyOpen) onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, confirmDel, newPolicyOpen]);

  const canSave = form.name.trim() && resolvedIds.length > 0;
  const save = () => {
    if (isEdit) {
      Object.assign(editBundle, {
        name: form.name.trim(), framework: form.framework, version: form.version,
        description: form.description, requiredEnvs: form.requiredEnvs,
        policyIds: resolvedIds,
        lastReview: "just now",
      });
    } else {
      window.__cfCoach?.complete("compliance");
    }
    onClose();
  };

  if (confirmDel) {
    return (
      <div className="modal-backdrop" onClick={onClose}>
        <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(520px,96vw)" }}>
          <DeleteBundleConfirm bundle={editBundle} onCancel={()=>setConfirmDel(false)} onConfirm={()=>{ onDelete?.(); onClose(); }}/>
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
              <span className="pe-head-title">{isEdit ? (form.name || editBundle.name) : "New compliance bundle"}</span>
              <span className="chip chip-info">{form.framework}</span>
              {form.version && <span className="chip chip-unknown mono" style={{ fontSize:10 }}>{form.version}</span>}
              {resolvedIds.length === 0 && <span className="chip" title="A bundle needs at least one control.">No controls</span>}
            </div>
            <span className="pe-head-sub">{form.description || "A bundle represents a standard — assembled from policies that each assert one thing."}</span>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={16}/></button>
        </header>

        <nav className="pe-rail">
          {BE_SECTIONS.map(s => (
            <button key={s.id} className={`pe-rail-item focus-ring${section===s.id?" active":""}`} onClick={()=>setSection(s.id)}>
              <Icon name={s.icon} size={13}/>
              <span className="pe-rail-label">{s.label}</span>
              {s.id === "controls" && <span className="pe-rail-badge">{resolvedIds.length}</span>}
            </button>
          ))}
        </nav>

        <div className="pe-body">
          {section === "basics" && (
            <BEBasics form={form} set={set} isEdit={isEdit} customFrameworks={customFrameworks}
              onAddFramework={(name)=>{ const next=[...customFrameworks,{ id:`fw-${Date.now()}`, name }]; setCustomFrameworks(next); saveCustomFrameworks(next); set("framework", name); }}
              onDelete={()=>setConfirmDel(true)}/>
          )}
          {section === "controls" && (
            <BEControls form={form} setForm={setForm} catalog={catalog} resolved={resolved}
              onNewPolicy={() => { knownIds.current = new Set((typeof POLICIES !== "undefined" ? POLICIES : []).map(p => p.id)); setNewPolicyOpen(true); }}/>
          )}
        </div>

        <footer className="pe-foot">
          <span className="pe-foot-state">
            {resolvedIds.length} control{resolvedIds.length===1?"":"s"} in bundle
            <span className="pe-foot-dot">·</span>
            {form.framework}{form.version ? ` ${form.version}` : ""}
          </span>
          <div style={{ display:"flex", gap:8 }}>
            <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
            <button className="btn btn-primary focus-ring" disabled={!canSave} onClick={save}>
              <Icon name="check" size={13}/> {isEdit ? "Save changes" : "Create bundle"}
            </button>
          </div>
        </footer>
      </div>
      {newPolicyOpen && typeof PolicyEditor !== "undefined" && (
        <div onClick={e=>e.stopPropagation()}>
        <PolicyEditor mode="add" onClose={() => {
          setNewPolicyOpen(false);
          const before = knownIds.current || new Set();
          const created = (typeof POLICIES !== "undefined" ? POLICIES : []).filter(p => !before.has(p.id)).map(p => p.id);
          if (created.length) setForm(p => ({ ...p, pinIds:[...new Set([...p.pinIds, ...created])] }));
          setCatalogNonce(n => n + 1);
        }}/>
        </div>
      )}
    </div>
  );
}

function BEBasics({ form, set, isEdit, customFrameworks, onAddFramework, onDelete }) {
  const [newFwOpen, setNewFwOpen] = React.useState(false);
  const [newFwName, setNewFwName] = React.useState("");
  const commitFw = () => { const n = newFwName.trim(); if (!n) return; onAddFramework(n); setNewFwOpen(false); setNewFwName(""); };
  const toggleEnv = (env) => set("requiredEnvs", form.requiredEnvs.includes(env)
    ? form.requiredEnvs.filter(x => x !== env) : [...form.requiredEnvs, env]);

  return (
    <>
      <div className="pe-sec-head">
        <h3>Basics</h3>
        <p style={{ margin:0, fontSize:12, color:"var(--cf-text-muted)" }}>What standard this bundle represents, and where it applies.</p>
      </div>
      <div style={{ display:"grid", gridTemplateColumns:"2fr 1fr", gap:14 }}>
        <div className="field" style={{ marginTop:0 }}>
          <label>Bundle name</label>
          <input className="input focus-ring" value={form.name} onChange={e=>set("name",e.target.value)} placeholder="e.g. Anduril NixOS STIG (v1r2)"/>
        </div>
        <div className="field" style={{ marginTop:0 }}>
          <label>Version / revision</label>
          <input className="input focus-ring mono" value={form.version} onChange={e=>set("version",e.target.value)} placeholder="v1r5" style={{ fontSize:12 }}/>
        </div>
      </div>
      <div style={{ display:"grid", gridTemplateColumns:"1fr 2fr", gap:14, marginTop:14 }}>
        <div className="field" style={{ marginTop:0 }}>
          <label>Framework</label>
          {newFwOpen ? (
            <div style={{ display:"flex", gap:6 }}>
              <input className="input focus-ring" autoFocus value={newFwName} onChange={e=>setNewFwName(e.target.value)}
                placeholder="e.g. Acme Internal Baseline"
                onKeyDown={e=>{ if(e.key==="Enter") commitFw(); if(e.key==="Escape") setNewFwOpen(false); }}/>
              <button className="btn btn-ghost focus-ring xs" onClick={commitFw} disabled={!newFwName.trim()}>Add</button>
              <button className="btn btn-ghost focus-ring xs" onClick={()=>setNewFwOpen(false)}>Cancel</button>
            </div>
          ) : (
            <select className="input focus-ring" value={form.framework}
              onChange={e=>{ if (e.target.value === "__new__") setNewFwOpen(true); else set("framework", e.target.value); }}>
              <optgroup label="Standard">{BUILTIN_FRAMEWORKS.map(f => <option key={f}>{f}</option>)}</optgroup>
              {customFrameworks.length > 0 && (
                <optgroup label="Custom">{customFrameworks.map(f => <option key={f.name}>{f.name}</option>)}</optgroup>
              )}
              <option value="__new__">+ Define new framework…</option>
            </select>
          )}
        </div>
        <div className="field" style={{ marginTop:0 }}>
          <label>Description</label>
          <input className="input focus-ring" value={form.description} onChange={e=>set("description",e.target.value)} placeholder="What this bundle verifies"/>
        </div>
      </div>

      <div className="field">
        <label>Applies to environments</label>
        <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
          {ENVIRONMENTS.map(env => {
            const on = form.requiredEnvs.includes(env.name);
            return (
              <button key={env.name} className="focus-ring" onClick={()=>toggleEnv(env.name)}
                style={{ padding:"4px 10px", borderRadius:99, fontSize:11, cursor:"pointer",
                  border:`1px solid ${on ? env.color : "var(--cf-card-border)"}`,
                  background: on ? `color-mix(in oklab, ${env.color} 14%, var(--cf-card-bg))` : "transparent",
                  color: on ? env.color : "var(--cf-text-secondary)",
                  display:"inline-flex", alignItems:"center", gap:6, fontFamily:"inherit" }}>
                <span style={{ width:6, height:6, borderRadius:"50%", background: env.color }}/>
                {env.name}
              </button>
            );
          })}
        </div>
        <div className="help">Bundles apply automatically to systems in these environments unless a system names an explicit revision.</div>
      </div>

      {isEdit && (
        <div style={{ marginTop:22, paddingTop:14, borderTop:"1px solid var(--cf-divider)" }}>
          <div className="pe-sec-label">Danger zone</div>
          <button className="btn btn-ghost focus-ring" onClick={onDelete} style={{ color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }}>
            <Icon name="trash" size={12}/> Delete bundle
          </button>
        </div>
      )}
    </>
  );
}

function BEControls({ form, setForm, catalog, resolved, onNewPolicy }) {
  const [membership, setMembership] = React.useState("all"); // all | in | out
  const [schemeId, setSchemeId] = React.useState("control-family");
  const [browseQuery, setBrowseQuery] = React.useState("");
  const [openGroups, setOpenGroups] = React.useState(() => new Set());

  const pin = (id) => setForm(p => ({ ...p, pinIds:[...new Set([...p.pinIds, id])] }));
  const pinAll = (ids) => setForm(p => ({ ...p, pinIds:[...new Set([...p.pinIds, ...ids])] }));
  const dropAll = (ids) => setForm(p => ({ ...p, pinIds:p.pinIds.filter(x => !ids.includes(x)) }));

  const scheme = React.useMemo(
    () => (typeof GROUPING_SCHEMES !== "undefined" ? GROUPING_SCHEMES : []).find(s => s.id === schemeId) || null,
    [schemeId]);
  const browseGroups = React.useMemo(() => {
    if (!scheme) return [];
    const t = browseQuery.trim().toLowerCase();
    let pool = t ? catalog.filter(p => bundleControlHay(p).includes(t)) : catalog;
    if (membership === "in")  pool = pool.filter(p => resolved.has(p.id));
    if (membership === "out") pool = pool.filter(p => !resolved.has(p.id));
    const byKey = new Map();
    pool.forEach(p => {
      const key = String(scheme.groupKeyOf(p));
      const label = scheme.groupOf(p) || "All controls";
      if (!byKey.has(key)) byKey.set(key, { key, label, items: [] });
      byKey.get(key).items.push(p);
    });
    return [...byKey.values()].sort((a,b) => b.items.length - a.items.length);
  }, [scheme, catalog, browseQuery, membership, resolved]);

  const [peek, setPeek] = React.useState(null); // control id whose detail is open

  return (
    <>
      <div className="pe-sec-head">
        <h3>Controls</h3>
        <p style={{ margin:0, fontSize:12, color:"var(--cf-text-muted)", lineHeight:1.55 }}>
          Controls come from the policy catalog. Browse by the grouping that matches how you audit, add a whole group or single
          controls, and expand any row to see what it actually asserts.
        </p>
      </div>

      {/* Browse the catalog by the same grouping schemes the Policies view uses */}
      <div style={{ marginTop:22, paddingTop:14, borderTop:"1px solid var(--cf-divider)" }}>
        <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:8 }}>
          <div className="pe-sec-label">Browse the catalog</div>
          <button className="btn btn-ghost focus-ring xs" onClick={onNewPolicy} title="Author a new policy and add it to this bundle — you'll come back here">
            <Icon name="plus" size={11}/> New policy…
          </button>
        </div>
        <div style={{ display:"flex", alignItems:"center", gap:8, marginBottom:10, flexWrap:"wrap" }}>
          <select className="input focus-ring" value={schemeId} onChange={e=>{ setSchemeId(e.target.value); setOpenGroups(new Set()); }} style={{ width:"auto", fontSize:12 }}>
            {(typeof GROUPING_SCHEMES !== "undefined" ? GROUPING_SCHEMES : []).map(s => (
              <option key={s.id} value={s.id}>Group by {s.label}</option>
            ))}
          </select>
          <div className="filter-search" style={{ margin:0, maxWidth:240, flex:1 }}>
            <Icon name="search" size={12}/>
            <input className="input focus-ring" placeholder="Search the catalog…" value={browseQuery} onChange={e=>setBrowseQuery(e.target.value)}/>
          </div>
          <div style={{ display:"flex", gap:4 }}>
            {[["all","All"],["in",`In bundle · ${resolved.size}`],["out","Not added"]].map(([k,l]) => (
              <button key={k} className={`btn btn-ghost xs focus-ring${membership===k?" active-filter":""}`} onClick={()=>{ setMembership(k); setOpenGroups(new Set()); }}>{l}</button>
            ))}
          </div>
        </div>
        <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
          {browseGroups.map(g => {
            const open = openGroups.has(g.key);
            const ids = g.items.map(p => p.id);
            const inBundle = ids.filter(id => resolved.has(id));
            const missing = ids.filter(id => !resolved.has(id));
            const full = inBundle.length === ids.length;
            const partial = inBundle.length > 0 && !full;
            const accent = full ? "#34d399" : partial ? "var(--cf-brand-purple)" : null;
            return (
              <div key={g.key} style={{ border:"1px solid var(--cf-divider)", borderLeft:`3px solid ${accent || "var(--cf-divider)"}`, borderRadius:9, overflow:"hidden" }}>
                <div style={{ display:"flex", alignItems:"center", gap:8, padding:"7px 10px",
                  background: accent ? `color-mix(in oklab, ${full ? "#34d399" : "var(--cf-brand-purple)"} 8%, var(--cf-card-bg))` : "color-mix(in oklab,var(--cf-page-bg) 45%,var(--cf-card-bg))" }}>
                  <button className="focus-ring" onClick={()=>setOpenGroups(prev => { const n = new Set(prev); open ? n.delete(g.key) : n.add(g.key); return n; })}
                    style={{ all:"unset", cursor:"pointer", display:"flex", alignItems:"center", gap:7, flex:1, minWidth:0 }}>
                    <Icon name={open ? "chevron-down" : "chevron-right"} size={11} style={{ color:"var(--cf-text-muted)", flexShrink:0 }}/>
                    <span style={{ fontSize:12.5, fontWeight:600, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{g.label}</span>
                    <span className="mono" style={{ fontSize:10.5, fontWeight: accent ? 600 : 400, color: full ? "#34d399" : partial ? "var(--cf-brand-purple)" : "var(--cf-text-muted)", flexShrink:0 }}>
                      {inBundle.length}/{ids.length}
                    </span>
                  </button>
                  {missing.length > 0 && (
                    <button className="btn btn-ghost focus-ring xs" onClick={()=>pinAll(missing)}>
                      <Icon name="plus" size={10}/> Add {missing.length}
                    </button>
                  )}
                  {inBundle.length > 0 && (
                    <button className="btn btn-ghost focus-ring xs" onClick={()=>dropAll(inBundle)}>Remove</button>
                  )}
                </div>
                {open && (
                  <div>
                    {g.items.slice(0, 40).map(p => (
                      <BEControlRow key={p.id} p={p} inBundle={resolved.has(p.id)}
                        open={peek === p.id} onPeek={()=>setPeek(peek === p.id ? null : p.id)}
                        onAdd={()=>pin(p.id)} onRemove={()=>dropAll([p.id])}/>
                    ))}
                    {g.items.length > 40 && (
                      <div style={{ padding:"6px 10px 6px 26px", fontSize:10.5, color:"var(--cf-text-muted)", borderTop:"1px solid var(--cf-divider)" }}>
                        +{g.items.length - 40} more in this group — narrow with search, or use “Add {missing.length || ids.length}”.
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
          {browseGroups.length === 0 && (
            <div style={{ fontSize:11.5, color:"var(--cf-text-muted)", padding:"8px 0" }}>No controls match “{browseQuery}”.</div>
          )}
        </div>
      </div>

    </>
  );
}

// One control line — expands in place to show what the policy actually asserts,
// so a bundle can be assembled without leaving the editor to go read the catalog.
function BEControlRow({ p, inBundle, open, onPeek, onAdd, onRemove }) {
  const sev = beSev(p);
  return (
    <div style={{ borderTop:"1px solid var(--cf-divider)", background: inBundle ? "color-mix(in oklab, #34d399 4%, transparent)" : "transparent" }}>
      <div style={{ display:"flex", alignItems:"center", gap:8, padding:"5px 8px 5px 10px", fontSize:11.5 }}>
        <button className="focus-ring" onClick={onPeek} title="Inspect this control"
          style={{ all:"unset", cursor:"pointer", display:"flex", alignItems:"center", gap:7, flex:1, minWidth:0 }}>
          <Icon name={open ? "chevron-down" : "chevron-right"} size={10} style={{ color:"var(--cf-text-muted)", flexShrink:0 }}/>
          <span className="mono truncate" style={{ minWidth:0, color: inBundle ? "var(--cf-text-primary)" : "var(--cf-text-secondary)" }}>{p.name}</span>
          {sev && <span style={{ width:6, height:6, borderRadius:"50%", background:sev.color, flexShrink:0 }} title={sev.label}/>}
        </button>
        {inBundle
          ? <button className="btn btn-ghost focus-ring xs" onClick={onRemove}>Remove</button>
          : <button className="btn btn-ghost focus-ring xs" onClick={onAdd}><Icon name="plus" size={10}/> Add</button>}
      </div>
      {open && <BEControlDetail p={p}/>}
    </div>
  );
}

function BEControlDetail({ p }) {
  const sev = beSev(p);
  const family = p.controlFamily && typeof CONTROL_FAMILIES !== "undefined" ? CONTROL_FAMILIES[p.controlFamily] : null;
  const phrase = typeof enforcementPhrase === "function" ? enforcementPhrase : null;
  const meta = [
    ["Family", family ? `${p.controlFamily} — ${family.label}` : p.controlFamily],
    ["Severity", sev ? `${sev.label} (${p.severity})` : null],
    ["SRG", (p.srgIds || []).join(", ")],
    ["CCI", (p.cciIds || []).join(", ")],
    ["Vuln ID", p.vulnId],
    ["Published", p.publishedDate],
  ].filter(([, v]) => v);
  return (
    <div style={{ padding:"10px 12px 12px 27px", background:"color-mix(in oklab, var(--cf-page-bg) 55%, var(--cf-card-bg))", borderTop:"1px solid var(--cf-divider)" }}>
      {p.description && <div style={{ fontSize:11.5, color:"var(--cf-text-secondary)", lineHeight:1.55, marginBottom:8, textWrap:"pretty" }}>{p.description}</div>}
      <div style={{ display:"grid", gridTemplateColumns:"repeat(auto-fill, minmax(190px, 1fr))", gap:"4px 14px", marginBottom:(p.rules||[]).length?10:0 }}>
        {meta.map(([k, v]) => (
          <div key={k} style={{ display:"flex", gap:6, fontSize:10.5, minWidth:0 }}>
            <span style={{ color:"var(--cf-text-muted)", flexShrink:0 }}>{k}</span>
            <span className="mono truncate" style={{ color:"var(--cf-text-secondary)" }}>{v}</span>
          </div>
        ))}
      </div>
      {(p.rules || []).length > 0 && (
        <>
          <div style={{ fontSize:10, textTransform:"uppercase", letterSpacing:"0.06em", color:"var(--cf-text-muted)", marginBottom:4 }}>Asserts</div>
          <div style={{ display:"flex", flexDirection:"column", gap:3 }}>
            {p.rules.map((r, i) => {
              // enforcementPhrase returns {subject, verb, value} — compose, never render raw.
              const ph = phrase ? phrase(r) : null;
              const text = ph
                ? [ph.subject, ph.verb, ph.value].filter(Boolean).map(String).join(" ")
                : (r.path ? `config.${r.path} = ${String(r.value)}` : String(r.kind || ""));
              return (
                <div key={i} className="mono" style={{ fontSize:10.5, color:"var(--cf-text-secondary)", lineHeight:1.5 }}>· {text}</div>
              );
            })}
          </div>
        </>
      )}
      {(p.rules || []).length === 0 && (
        <div style={{ fontSize:10.5, color:"var(--cf-text-muted)", fontStyle:"italic" }}>No enforcement defined — this control is verified by attestation.</div>
      )}
    </div>
  );
}

Object.assign(window, { BundleEditor, BEBasics, BEControls, BEControlRow, BEControlDetail, BESevChip, resolveBundleControls, bundleControlHay, beSev });
