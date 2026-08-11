// Policies view — deployment policies + rule builder

function PoliciesView({ onOpenSystem, focus, onClearFocus }) {
  const [query, setQuery] = React.useState("");
  const [domain, setDomain] = React.useState("platform"); // platform | security
  const [catFilter, setCatFilter] = React.useState("all");
  const [groupingScheme, setGroupingScheme] = React.useState("control-family");
  const [customSchemes, setCustomSchemes] = React.useState(() => loadCustomGroupingSchemes());
  const [adminOpen, setAdminOpen] = React.useState(false);
  const [editPolicy, setEditPolicy] = React.useState(null);
  const [addOpen, setAddOpen] = React.useState(false);
  const [drawerPolicy, setDrawerPolicy] = React.useState(null);
  const [drawerTab, setDrawerTab] = React.useState(null);
  const [importOpen, setImportOpen] = React.useState(false);
  const [selectMode, setSelectMode] = React.useState(false);
  const [selectedIds, setSelectedIds] = React.useState(() => new Set());
  const toggleSelected = (id) => setSelectedIds(prev => { const n = new Set(prev); n.has(id) ? n.delete(id) : n.add(id); return n; });
  React.useEffect(() => {
    if (!focus) return;
    const p = POLICIES.find(x => x.id === focus || x.name === focus);
    if (p) setDrawerPolicy(p);
    onClearFocus?.();
  }, [focus]);

  const searchMatch = (p) => {
    if (!query) return true;
    const q = query.toLowerCase();
    return p.name.toLowerCase().includes(q) || p.description.toLowerCase().includes(q)
      || (p.srgIds||[]).some(s => s.toLowerCase().includes(q)) || (p.cciIds||[]).some(s => s.toLowerCase().includes(q));
  };

  const lineages = (typeof groupPoliciesByLineage === "function" ? groupPoliciesByLineage(POLICIES) : POLICIES.map(p => ({ lineageId:p.id, current:p, revisions:[p] })));
  const groupListAll = lineages.filter(g => g.revisions.some(searchMatch));
  const groupListDomain = groupListAll.filter(g => (typeof policyDomain === "function" ? policyDomain(g.current) : "platform") === domain);
  const [frameworkFilter, setFrameworkFilter] = React.useState("all");
  const availableFrameworks = domain === "security" ? [...new Set(groupListDomain.map(g => g.current.framework).filter(Boolean))] : [];
  const groupListFramework = domain === "security" && frameworkFilter !== "all"
    ? groupListDomain.filter(g => g.current.framework === frameworkFilter)
    : groupListDomain;
  const groupList = domain === "platform" ? groupListFramework.filter(g => catFilter === "all" || (g.current.category || "deployment") === catFilter) : groupListFramework;

  const platformCats = POLICY_CATEGORIES.filter(c => c.domain === "platform");
  const allSchemes = [...GROUPING_SCHEMES, ...customSchemes];
  const activeScheme = allSchemes.find(s => s.id === groupingScheme) || GROUPING_SCHEMES[0];
  const matchesGroupRule = (p, grp) => {
    if ((grp.pinIds || []).includes(p.id)) return !(grp.excludeIds || []).includes(p.id);
    const q = (grp.query || "").trim().toLowerCase();
    if (!q) return false;
    const hay = [p.name, p.description, p.controlFamily, (p.srgIds||[]).join(" "), (p.cciIds||[]).join(" "), p.severity].filter(Boolean).join(" ").toLowerCase();
    return hay.includes(q) && !(grp.excludeIds || []).includes(p.id);
  };

  let groups = [];
  if (domain === "platform") {
    groups = platformCats
      .map(cat => ({ key:cat.id, label:cat.label, icon:cat.icon, color:cat.color, blurb:cat.blurb, items: groupList.filter(g => (g.current.category || "deployment") === cat.id) }))
      .filter(g => g.items.length > 0);
  } else if (activeScheme.id === "flat") {
    groups = groupListFramework.length ? [{ key:"all", label:"All security controls", icon:"shield", color:"#f87171", blurb:"Every control in this domain, ungrouped.", items: groupListFramework }] : [];
  } else if (activeScheme.id === "control-family") {
    const order = [...Object.keys(CONTROL_FAMILIES), "ungrouped"];
    groups = order.map(fid => {
      const fam = CONTROL_FAMILIES[fid];
      return { key:fid, label: fam ? `${fam.id} — ${fam.label}` : "Ungrouped", icon:"shield", color:"#f87171",
        blurb: fam ? fam.blurb : "Controls with no NIST family tag set yet.",
        items: groupListFramework.filter(g => (g.current.controlFamily || "ungrouped") === fid) };
    }).filter(g => g.items.length > 0);
  } else if (activeScheme.id === "severity") {
    const order = [["high","CAT I — High","#f87171","Findings that could result in loss of confidentiality, availability, or integrity — highest priority to remediate."],
      ["medium","CAT II — Medium","#fbbf24","Findings that could result in degraded protection but not immediate compromise."],
      ["low","CAT III — Low","#60a5fa","Findings that reduce the ability to detect or recover, but don't directly weaken protection."],
      ["unrated","Unrated","#6b7280","No STIG severity assigned."]];
    groups = order.map(([sid,label,color,blurb]) => ({ key:sid, label, icon:"shield", color, blurb, items: groupListFramework.filter(g => (g.current.severity || "unrated") === sid) })).filter(g => g.items.length > 0);
  } else if (activeScheme.id === "cci" || activeScheme.id === "srg-category" || activeScheme.id === "cmmc-level" || activeScheme.id === "remediation") {
    // Generic path: every one of these schemes is a pure pivot over a groupKeyOf/groupOf
    // pair on GROUPING_SCHEMES — no bespoke per-scheme code needed beyond an order + blurb.
    const blurbs = {
      cci: {}, // labels are the CCI ids themselves, self-explanatory
      "srg-category": {}, // labels are the SRG categories themselves
      "cmmc-level": {
        l3: "Findings backing Level 3 (Expert) practices.", l2: "Findings backing Level 2 (Advanced) practices.",
        l1: "Findings backing Level 1 (Foundational) practices.", unrated: "No severity to derive a level from.",
      },
      remediation: {
        auto: "Declarative NixOS options — corrected automatically by the next build/deploy.",
        semi: "Needs a custom eval assertion — flagged, but not self-healing.",
        manual: "No automatable rule yet — verified by attestation or manual check.",
      },
    };
    const byKey = new Map();
    groupListFramework.forEach(g => {
      const key = activeScheme.groupKeyOf(g.current);
      const label = activeScheme.groupOf(g.current);
      if (!byKey.has(key)) byKey.set(key, { key, label, items: [] });
      byKey.get(key).items.push(g);
    });
    groups = Array.from(byKey.values()).map(grp => ({
      ...grp, icon:"shield", color:"#f87171", blurb: blurbs[activeScheme.id]?.[grp.key] || "",
    })).sort((a,b) => b.items.length - a.items.length);
  } else {
    // Custom admin-defined scheme — each group is a match rule (+ pins/excludes), so
    // adding a new policy later joins matching groups automatically. No manual re-triage.
    const assigned = new Set();
    groups = (activeScheme.groups || []).map(grp => {
      const items = groupListFramework.filter(g => matchesGroupRule(g.current, grp));
      items.forEach(it => assigned.add(it.current.id));
      return { key:grp.id, label:grp.name, icon:"shield", color:"#f87171", blurb: grp.desc || "", items };
    }).filter(g => g.items.length > 0);
    const ungrouped = groupListFramework.filter(g => !assigned.has(g.current.id));
    if (ungrouped.length) groups.push({ key:"ungrouped", label:"Ungrouped", icon:"shield", color:"#6b7280", blurb:"Controls that don't match any group's rule in this scheme.", items: ungrouped });
  }

  const catCount = (id) => lineages.filter(g => (g.current.category || "deployment") === id).length;
  const domainCount = (id) => lineages.filter(g => (typeof policyDomain === "function" ? policyDomain(g.current) : "platform") === id).length;
  const currentPolicies = lineages.map(g => g.current);
  const refreshCustomSchemes = () => setCustomSchemes(loadCustomGroupingSchemes());

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:14 }}>
      <div className="page-head" style={{ marginBottom:0 }}>
        <h1 className="page-title" style={{ marginBottom:0 }}>Policies</h1>
        <div style={{ display:"flex", gap:8 }}>
          {selectMode ? (
            <>
              <span style={{ fontSize:12, color:"var(--cf-text-muted)", alignSelf:"center" }}>{selectedIds.size} selected</span>
              <button className="btn btn-primary focus-ring" disabled={selectedIds.size===0}
                onClick={() => { exportPolicies(POLICIES.filter(p => selectedIds.has(p.id))); setSelectMode(false); setSelectedIds(new Set()); }}>
                <Icon name="download" size={14}/> Export selected
              </button>
              <button className="btn btn-ghost focus-ring" onClick={() => { setSelectMode(false); setSelectedIds(new Set()); }}>Cancel</button>
            </>
          ) : (
            <>
              <IOMenu items={[
                { label:"Import policies…", icon:"upload", onClick:() => setImportOpen(true) },
                { label:"Select policies to export…", icon:"download", onClick:() => { setSelectMode(true); setSelectedIds(new Set()); } },
                { label:"Export all custom policies", icon:"download", onClick:() => exportPolicies(POLICY_CUSTOM) },
              ]}/>
              <button className="btn btn-primary focus-ring" onClick={() => setAddOpen(true)}>
                <Icon name="plus" size={14}/> New custom policy
              </button>
            </>
          )}
        </div>
      </div>

      {/* Domain split — Platform (pipeline mechanics) vs Security controls (framework-owned).
          Sits as the page's sub-header, directly under the title. Real tabs: underline
          indicator + hover state — no stat-card look-alike, no extra copy line. */}
      <div className="pol-domain-tabs" role="tablist" style={{ marginTop:-4 }}>
        {POLICY_DOMAINS.map(d => {
          const active = domain === d.id;
          return (
            <button key={d.id} role="tab" aria-selected={active} className={`pol-domain-tab${active?" active":""}`}
              onClick={()=>setDomain(d.id)} title={d.blurb}
              style={active ? { "--dc": d.color } : undefined}>
              <Icon name={d.icon} size={14}/>
              <span>{d.label}</span>
              <span className="pol-domain-tab-count">{domainCount(d.id)}</span>
            </button>
          );
        })}
      </div>

      {/* Grouping toolbar — identical row/position/height in both domains so nothing
          shifts when switching tabs; only the control inside changes. */}
      <div className="pol-group-toolbar">
        <span className="pol-group-toolbar-label">{domain === "platform" ? "Category" : "Framework"}</span>
        {domain === "platform" ? (
          <div className="seg">
            <button className={catFilter==="all"?"active":""} onClick={()=>setCatFilter("all")}>all</button>
            {platformCats.map(c => (
              <button key={c.id} className={catFilter===c.id?"active":""} onClick={()=>setCatFilter(c.id)} title={c.blurb}>
                <span style={{ display:"inline-flex", alignItems:"center", gap:5 }}>
                  <span style={{ width:6, height:6, borderRadius:"50%", background:c.color, flexShrink:0 }}/>
                  {c.short} <span className="mono" style={{ opacity:0.6, fontSize:10.5 }}>{catCount(c.id)}</span>
                </span>
              </button>
            ))}
          </div>
        ) : (
          <>
            <select className="input focus-ring" value={groupingScheme} onChange={e=>setGroupingScheme(e.target.value)} style={{ width:"auto", fontSize:12, padding:"6px 10px" }}>
              <optgroup label="Predefined">
                {GROUPING_SCHEMES.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}
              </optgroup>
              {customSchemes.length > 0 && (
                <optgroup label="Custom">
                  {customSchemes.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}
                </optgroup>
              )}
            </select>
          </>
        )}
        <button className="btn btn-ghost focus-ring xs" style={{ marginLeft:"auto", visibility: domain === "security" ? "visible" : "hidden" }} onClick={()=>setAdminOpen(true)}>
          <Icon name="gear" size={12}/> Manage groupings
        </button>
      </div>

      {/* Filterbar — fixed position/contents across both domains. */}
      <div className="filterbar">
        <div className="filter-search" style={{ maxWidth:280 }}>
          <Icon name="search"/>
          <input className="input focus-ring" placeholder="Search policies…" value={query} onChange={e=>setQuery(e.target.value)}/>
        </div>
        {(catFilter !== "all" || query) && (
          <button className="btn btn-ghost focus-ring xs" onClick={()=>{ setCatFilter("all"); setQuery(""); }}>
            <Icon name="x" size={11}/> Clear
          </button>
        )}
        <span className="filter-count">{(() => {
          const shown = groups.reduce((a,g) => a + g.items.length, 0);
          return shown < groupList.length
            ? `Showing ${shown} of ${groupList.length} ${groupList.length === 1 ? "policy" : "policies"}`
            : `${groupList.length} ${groupList.length === 1 ? "policy" : "policies"}`;
        })()}</span>      </div>

      {groups.length === 0 ? (
        <div className="card" style={{ padding:"40px 20px", textAlign:"center", color:"var(--cf-text-muted)" }}>
          <Icon name="search" size={20} style={{ opacity:0.5 }}/>
          <div style={{ marginTop:8, fontSize:13 }}>No policies match these filters.</div>
        </div>
      ) : groups.map((g) => (
        <section key={g.key} className="pol-group">
          <div className="pol-group-head">
            <span className="pol-group-icon" style={{ background:`color-mix(in oklab, ${g.color} 16%, transparent)`, color:g.color }}>
              <Icon name={g.icon} size={13}/>
            </span>
            <div style={{ minWidth:0 }}>
              <h2 className="pol-group-title">{g.label} <span className="pol-group-count">{g.items.length}</span></h2>
              {g.blurb && <div className="pol-group-blurb">{g.blurb}</div>}
            </div>
          </div>
          <div className="cards-grid">
            {g.items.map(grp => (
              <PolicyCard key={grp.lineageId} group={grp}
                onOpen={(p, tab) => selectMode ? toggleSelected(p.id) : (setDrawerPolicy(p), setDrawerTab(tab || null))}
                onEdit={!selectMode ? (p) => setEditPolicy(p) : null}
                selectMode={selectMode}
                selected={selectedIds}
              />
            ))}
          </div>
        </section>
      ))}

      {drawerPolicy && (
        <PolicyDrawer
          policy={drawerPolicy}
          initialTab={drawerTab}
          onClose={() => setDrawerPolicy(null)}
          onEdit={drawerPolicy.type === "custom" ? () => { setEditPolicy(drawerPolicy); setDrawerPolicy(null); } : null}
          onOpenSystem={onOpenSystem}
          onSwitchPolicy={setDrawerPolicy}
        />
      )}
      {(editPolicy || addOpen) && (
        <PolicyFormModal
          mode={addOpen ? "add" : "edit"}
          policy={editPolicy}
          onClose={() => { setEditPolicy(null); setAddOpen(false); }}
        />
      )}
      {importOpen && <ImportPoliciesModal onClose={() => setImportOpen(false)}/>}
      {adminOpen && (
        <AdminGroupingsModal
          schemes={customSchemes}
          onClose={() => setAdminOpen(false)}
          onChange={(next) => { saveCustomGroupingSchemes(next); refreshCustomSchemes(); }}
          onSelectScheme={(id) => setGroupingScheme(id)}
        />
      )}
    </div>
  );
}

// ── Community policy interchange — matches the CVE-style single-check schema
// shared as JSON/TOML: { config: { description, expression, strict }, enabled, policy_type }.
function policyToExternal(p) {
  const rule = p.rules.find(r => r.kind === "custom_eval");
  return {
    config: {
      description: p.description,
      expression: rule ? rule.expr : (p.rules.map(ruleDescription).join(" && ") || ""),
      strict: rule ? !!rule.strict : false,
    },
    enabled: p.enabled !== false,
    policy_type: "custom_check",
  };
}

function slugify(s) {
  return (s || "").toLowerCase().replace(/[^a-z0-9]+/g,"-").replace(/(^-|-$)/g,"");
}

function externalToPolicy(ext, idx) {
  const cfg = ext.config || ext || {};
  const description = cfg.description || "Imported policy";
  const slug = slugify(description) || `imported-${idx}`;
  return {
    id: `custom-import-${slug}-${idx}`,
    name: slug,
    category: "security",
    description,
    type: "custom",
    enabled: ext.enabled !== false,
    severity: "medium",
    rationale: "Imported from an external policy file.",
    rules: [{ kind:"custom_eval", expr: cfg.expression || "", message: description, strict: !!cfg.strict }],
    evidence: [],
  };
}

function downloadFile(filename, content, mime) {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url; a.download = filename;
  document.body.appendChild(a); a.click(); a.remove();
  URL.revokeObjectURL(url);
}

function exportPolicies(list) {
  if (list.length === 1) {
    downloadFile(`${slugify(list[0].name)||"policy"}.json`, JSON.stringify(policyToExternal(list[0]), null, 2), "application/json");
  } else {
    downloadFile("crystal-forge-policies.json", JSON.stringify(list.map(policyToExternal), null, 2), "application/json");
  }
}

// Minimal TOML reader for this one schema shape — repeated [[policy]] tables with a
// nested [policy.config] (or flat config.* keys). Not a general TOML parser.
function parseSimpleToml(text) {
  const lines = text.split(/\r?\n/);
  const items = [];
  let current = null, inConfig = false;
  const coerce = (raw) => {
    let v = raw.trim();
    if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) return v.slice(1,-1);
    if (v === "true") return true;
    if (v === "false") return false;
    if (!isNaN(Number(v)) && v !== "") return Number(v);
    return v;
  };
  for (const raw of lines) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    if (/^\[\[(policy|policies)\]\]$/i.test(line)) { current = { config:{}, enabled:true, policy_type:"custom_check" }; items.push(current); inConfig = false; continue; }
    if (/^\[(policy|policies)?\.?config\]$/i.test(line)) { if (!current) { current = { config:{}, enabled:true, policy_type:"custom_check" }; items.push(current); } inConfig = true; continue; }
    if (/^\[(policy|policies)\]$/i.test(line)) { if (!current) { current = { config:{}, enabled:true, policy_type:"custom_check" }; items.push(current); } inConfig = false; continue; }
    const m = line.match(/^([\w.]+)\s*=\s*(.+)$/);
    if (m && current) {
      const key = m[1], val = coerce(m[2]);
      if (inConfig || key.startsWith("config.")) current.config[key.replace(/^config\./,"")] = val;
      else current[key] = val;
    }
  }
  return items;
}

function parsePolicyFile(text, filename) {
  const isToml = /\.toml$/i.test(filename) || (!/\.json$/i.test(filename) && !text.trim().startsWith("{") && !text.trim().startsWith("["));
  if (isToml) {
    const items = parseSimpleToml(text);
    if (items.length === 0) throw new Error("No [[policy]] entries found");
    return items;
  }
  const parsed = JSON.parse(text);
  if (Array.isArray(parsed)) return parsed;
  if (Array.isArray(parsed.policies)) return parsed.policies;
  return [parsed];
}

function ImportPoliciesModal({ onClose }) {
  const [entries, setEntries] = React.useState([]); // { key, source, external, policy, error, checked }
  const [dragOver, setDragOver] = React.useState(false);
  const [imported, setImported] = React.useState(false);
  const fileRef = React.useRef(null);

  const handleFiles = async (fileList) => {
    const files = Array.from(fileList);
    const next = [];
    for (const f of files) {
      try {
        const text = await f.text();
        const externals = parsePolicyFile(text, f.name);
        externals.forEach((ext, i) => {
          try {
            const policy = externalToPolicy(ext, `${f.name}-${i}`);
            next.push({ key:`${f.name}-${i}`, source:f.name, external:ext, policy, error:null, checked:true });
          } catch (e) {
            next.push({ key:`${f.name}-${i}`, source:f.name, external:ext, policy:null, error:e.message, checked:false });
          }
        });
      } catch (e) {
        next.push({ key:f.name, source:f.name, external:null, policy:null, error:`Could not parse ${f.name}: ${e.message}`, checked:false });
      }
    }
    setEntries(prev => [...prev, ...next]);
  };

  const toggle = (key) => setEntries(prev => prev.map(e => e.key===key ? { ...e, checked: !e.checked } : e));
  const remove = (key) => setEntries(prev => prev.filter(e => e.key !== key));

  const existingSlugs = new Set(POLICIES.map(p => slugify(p.name)));
  const importable = entries.filter(e => e.policy && e.checked);

  if (imported) {
    return (
      <div className="modal-backdrop" onClick={onClose}>
        <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(480px,94vw)" }}>
          <div className="modal-body" style={{ textAlign:"center", padding:"32px 20px" }}>
            <Icon name="check" size={28} style={{ color:"#34d399" }}/>
            <h2 style={{ marginTop:12 }}>{importable.length} polic{importable.length===1?"y":"ies"} imported</h2>
            <p style={{ color:"var(--cf-text-muted)", fontSize:12 }}>They're now available to assign from a system's edit dialog.</p>
          </div>
          <div className="modal-foot"><button className="btn btn-primary focus-ring" onClick={onClose}>Done</button></div>
        </div>
      </div>
    );
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(680px,96vw)", maxHeight:"92vh" }}>
        <div className="modal-head">
          <h2><Icon name="upload" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/> Import policies</h2>
          <p>Drop one or more JSON/TOML files shared by the community — each holds a single check (<span className="mono">config.description</span> / <span className="mono">config.expression</span>), same shape you'd export from another Crystal Forge instance.</p>
        </div>
        <div className="modal-body" style={{ overflowY:"auto" }}>
          <div
            onDragOver={e=>{ e.preventDefault(); setDragOver(true); }}
            onDragLeave={()=>setDragOver(false)}
            onDrop={e=>{ e.preventDefault(); setDragOver(false); handleFiles(e.dataTransfer.files); }}
            onClick={()=>fileRef.current?.click()}
            style={{
              border:`2px dashed ${dragOver ? "var(--cf-brand-purple)" : "var(--cf-divider)"}`,
              borderRadius:10, padding:"28px 16px", textAlign:"center", cursor:"pointer",
              background: dragOver ? "color-mix(in oklab, var(--cf-brand-purple) 8%, transparent)" : "var(--cf-subtle-bg)",
            }}>
            <Icon name="upload" size={20} style={{ color:"var(--cf-text-muted)" }}/>
            <div style={{ marginTop:8, fontSize:13, fontWeight:500 }}>Drop policy files here, or click to browse</div>
            <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>.json or .toml · multiple files OK</div>
            <input ref={fileRef} type="file" accept=".json,.toml,application/json" multiple hidden
              onChange={e=>{ handleFiles(e.target.files); e.target.value=""; }}/>
          </div>

          {entries.length > 0 && (
            <div style={{ marginTop:16, display:"flex", flexDirection:"column", gap:8 }}>
              <div style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.06em", color:"var(--cf-text-muted)", fontWeight:600 }}>
                Preview — {importable.length} of {entries.length} selected
              </div>
              {entries.map(e => {
                const dupe = e.policy && existingSlugs.has(slugify(e.policy.name));
                return (
                  <div key={e.key} className="card" style={{ padding:"10px 12px", display:"flex", gap:10, alignItems:"flex-start", opacity: e.error ? 0.7 : 1 }}>
                    {e.error ? (
                      <Icon name="warn" size={15} style={{ color:"#f87171", flexShrink:0, marginTop:1 }}/>
                    ) : (
                      <input type="checkbox" checked={e.checked} onChange={()=>toggle(e.key)} style={{ marginTop:3, flexShrink:0 }}/>
                    )}
                    <div style={{ minWidth:0, flex:1 }}>
                      {e.error ? (
                        <div style={{ fontSize:12, color:"#f87171" }}>{e.error}</div>
                      ) : (
                        <>
                          <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                            <span className="mono" style={{ fontWeight:600, fontSize:13 }}>{e.policy.name}</span>
                            {dupe && <span className="chip chip-unknown" style={{ fontSize:9 }}>name collision — will suffix</span>}
                            {!e.policy.enabled && <span className="chip" style={{ fontSize:9 }}>disabled</span>}
                          </div>
                          <div style={{ fontSize:11, color:"var(--cf-text-secondary)", marginTop:2 }}>{e.policy.description}</div>
                          <div className="mono" style={{ fontSize:10.5, color:"var(--cf-text-muted)", marginTop:3, wordBreak:"break-all" }}>{e.policy.rules[0].expr}</div>
                        </>
                      )}
                      <div style={{ fontSize:10, color:"var(--cf-text-muted)", marginTop:4 }}>{e.source}</div>
                    </div>
                    <button className="btn-icon focus-ring" onClick={()=>remove(e.key)} title="Remove"><Icon name="x" size={13}/></button>
                  </div>
                );
              })}
            </div>
          )}
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" disabled={importable.length===0}
            onClick={()=>setImported(true)}>
            <Icon name="upload" size={13}/> Import {importable.length || ""} polic{importable.length===1?"y":"ies"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Admin: manage custom grouping schemes for the Security controls domain ──
// A scheme is a named set of groups; each group holds an explicit list of security
// policy ids. Lets security teams mirror their own control catalog (an internal
// baseline, a customer's ATO package, etc.) without touching the underlying policies.
function AdminGroupingsModal({ schemes, onClose, onChange, onSelectScheme }) {
  const [local, setLocal] = React.useState(() => schemes.map(s => ({ ...s, groups: s.groups.map(g => ({ ...g, pinIds: [...(g.pinIds||g.policyIds||[])], excludeIds: [...(g.excludeIds||[])] })) })));
  const [activeId, setActiveId] = React.useState(local[0]?.id || null);
  const active = local.find(s => s.id === activeId);
  const securityLineages = React.useMemo(() => {
    const lineages = groupPoliciesByLineage(POLICIES);
    return lineages.filter(g => (typeof policyDomain === "function" ? policyDomain(g.current) : "platform") === "security").map(g => g.current);
  }, []);
  const matchesRule = (p, grp) => {
    if ((grp.pinIds || []).includes(p.id)) return !(grp.excludeIds || []).includes(p.id);
    const q = (grp.query || "").trim().toLowerCase();
    if (!q) return false;
    const hay = [p.name, p.description, p.controlFamily, (p.srgIds||[]).join(" "), (p.cciIds||[]).join(" "), p.severity].filter(Boolean).join(" ").toLowerCase();
    return hay.includes(q) && !(grp.excludeIds || []).includes(p.id);
  };

  const commit = (next) => { setLocal(next); onChange(next); };

  const addScheme = () => {
    const id = `custom-${Date.now()}`;
    const next = [...local, { id, label: "New grouping", groups: [] }];
    commit(next);
    setActiveId(id);
  };
  const renameScheme = (id, label) => commit(local.map(s => s.id===id ? { ...s, label } : s));
  const removeScheme = (id) => { commit(local.filter(s => s.id !== id)); if (activeId === id) setActiveId(local.find(s=>s.id!==id)?.id || null); };

  const addGroup = () => {
    if (!active) return;
    const g = { id: `grp-${Date.now()}`, name: "New group", desc: "", query: "", pinIds: [], excludeIds: [] };
    commit(local.map(s => s.id===active.id ? { ...s, groups: [...s.groups, g] } : s));
  };
  const patchGroup = (gid, patch) => commit(local.map(s => s.id===active.id ? { ...s, groups: s.groups.map(g => g.id===gid ? { ...g, ...patch } : g) } : s));
  const removeGroup = (gid) => commit(local.map(s => s.id===active.id ? { ...s, groups: s.groups.filter(g => g.id!==gid) } : s));

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(860px,96vw)", maxHeight:"90vh", display:"flex", flexDirection:"column" }}>
        <div className="modal-head">
          <h2><Icon name="gear" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/> Manage security groupings</h2>
          <p>Each group is a match rule — controls whose name, description, SRG/CCI id, control family, or severity contain the text join it automatically. New controls that match join without you revisiting this screen; pin or exclude individual controls only for exceptions.</p>
        </div>
        <div className="modal-body" style={{ overflow:"hidden", display:"flex", flexDirection:"row", gap:0, padding:0, flex:1 }}>
          <div style={{ width:220, flexShrink:0, boxSizing:"border-box", borderRight:"1px solid var(--cf-divider)", overflowY:"auto", padding:12, display:"flex", flexDirection:"column", gap:6, minWidth:0 }}>
            {local.map(s => (
              <button key={s.id} className="focus-ring" onClick={()=>setActiveId(s.id)}
                style={{ all:"unset", cursor:"pointer", display:"flex", alignItems:"center", justifyContent:"space-between", gap:6,
                  padding:"8px 10px", borderRadius:8, fontSize:12.5,
                  background: activeId===s.id ? "color-mix(in oklab,var(--cf-brand-purple) 12%, transparent)" : "transparent",
                  border: `1px solid ${activeId===s.id ? "var(--cf-brand-purple)" : "transparent"}` }}>
                <span style={{ overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap", minWidth:0, flex:1 }}>{s.label}</span>
                <span className="mono" style={{ fontSize:10, color:"var(--cf-text-muted)", flexShrink:0 }}>{s.groups.length}</span>
              </button>
            ))}
            <button className="btn btn-ghost focus-ring" onClick={addScheme} style={{ marginTop:4, fontSize:12, width:"100%", minWidth:0 }}>
              <Icon name="plus" size={12}/> New scheme
            </button>
          </div>
          <div style={{ flex:1, overflowY:"auto", padding:18 }}>
            {!active ? (
              <div style={{ fontSize:12, color:"var(--cf-text-muted)", textAlign:"center", padding:"40px 0" }}>Create a scheme to define custom groups.</div>
            ) : (
              <>
                <div style={{ display:"flex", alignItems:"center", gap:8, marginBottom:16 }}>
                  <input className="input focus-ring" value={active.label} onChange={e=>renameScheme(active.id, e.target.value)} style={{ fontSize:14, fontWeight:600, flex:1 }}/>
                  <button className="btn btn-ghost focus-ring xs" onClick={()=>{ onSelectScheme(active.id); onClose(); }}><Icon name="check" size={11}/> Use this scheme</button>
                  <button className="btn-icon focus-ring" onClick={()=>removeScheme(active.id)} title="Delete scheme"><Icon name="x" size={13}/></button>
                </div>
                <div style={{ display:"flex", flexDirection:"column", gap:14 }}>
                  {active.groups.map(g => {
                    const matched = securityLineages.filter(p => matchesRule(p, g));
                    return (
                    <div key={g.id} style={{ border:"1px solid var(--cf-divider)", borderRadius:10, padding:12 }}>
                      <div style={{ display:"flex", alignItems:"center", gap:8, marginBottom:8 }}>
                        <input className="input focus-ring" value={g.name} onChange={e=>patchGroup(g.id, { name:e.target.value })} style={{ fontSize:13, fontWeight:600, flex:1 }}/>
                        <button className="btn-icon focus-ring" onClick={()=>removeGroup(g.id)} title="Delete group"><Icon name="x" size={13}/></button>
                      </div>
                      <input className="input focus-ring" value={g.desc||""} onChange={e=>patchGroup(g.id, { desc:e.target.value })}
                        placeholder="One-line description shown as this section's subtitle in the list…" style={{ fontSize:11.5, marginBottom:8 }}/>
                      <div style={{ display:"flex", alignItems:"center", gap:8 }}>
                        <Icon name="search" size={12} style={{ color:"var(--cf-text-muted)", flexShrink:0 }}/>
                        <input className="input focus-ring mono" value={g.query} onChange={e=>patchGroup(g.id, { query:e.target.value })}
                          placeholder="e.g. ssh, AC, CAT I, password…" style={{ fontSize:12, flex:1 }}/>
                        <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)", flexShrink:0, whiteSpace:"nowrap" }}>{matched.length} match{matched.length===1?"":"es"}</span>
                      </div>
                      <div style={{ display:"flex", flexWrap:"wrap", gap:5, marginTop:8 }}>
                        {matched.slice(0, 10).map(p => {
                          const isPinned = (g.pinIds||[]).includes(p.id);
                          return (
                            <span key={p.id} className="chip chip-unknown mono" style={{ fontSize:10, display:"inline-flex", alignItems:"center", gap:4 }}>
                              {isPinned && <Icon name="key" size={8} title="Pinned"/>}
                              {p.name}
                              <button className="focus-ring" title="Exclude from this group" onClick={()=>patchGroup(g.id, { excludeIds:[...(g.excludeIds||[]), p.id], pinIds:(g.pinIds||[]).filter(x=>x!==p.id) })}
                                style={{ all:"unset", cursor:"pointer", display:"inline-flex", opacity:0.6 }}>
                                <Icon name="x" size={9}/>
                              </button>
                            </span>
                          );
                        })}
                        {matched.length > 10 && <span style={{ fontSize:10.5, color:"var(--cf-text-muted)", alignSelf:"center" }}>+{matched.length - 10} more</span>}
                        {matched.length === 0 && <span style={{ fontSize:11, color:"var(--cf-text-muted)", fontStyle:"italic" }}>No controls match yet.</span>}
                      </div>
                      {(g.excludeIds||[]).length > 0 && (
                        <div style={{ display:"flex", flexWrap:"wrap", gap:5, marginTop:6 }}>
                          <span style={{ fontSize:10, color:"var(--cf-text-muted)" }}>Excluded:</span>
                          {g.excludeIds.map(id => {
                            const p = securityLineages.find(x=>x.id===id);
                            return (
                              <button key={id} className="chip focus-ring" style={{ fontSize:10, cursor:"pointer", opacity:0.7 }}
                                title="Un-exclude" onClick={()=>patchGroup(g.id, { excludeIds:g.excludeIds.filter(x=>x!==id) })}>
                                {p?.name || id} <Icon name="x" size={8}/>
                              </button>
                            );
                          })}
                        </div>
                      )}
                      <PinPolicyPicker options={securityLineages.filter(p=>!matchesRule(p,g))} onPick={(id)=>patchGroup(g.id, { pinIds:[...(g.pinIds||[]), id], excludeIds:(g.excludeIds||[]).filter(x=>x!==id) })}/>
                    </div>
                    );
                  })}
                  <button className="btn btn-ghost focus-ring" onClick={addGroup} style={{ width:"fit-content" }}>
                    <Icon name="plus" size={12}/> New group
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn btn-primary focus-ring" onClick={onClose}>Done</button>
        </div>
      </div>
    </div>
  );
}

// Small typeahead for pinning the rare exception that a text rule won't catch —
// not a full checkbox wall, so it stays usable at any policy-catalog size.
function PinPolicyPicker({ options, onPick }) {
  const [q, setQ] = React.useState("");
  const [open, setOpen] = React.useState(false);
  const matches = q.trim() ? options.filter(p => p.name.toLowerCase().includes(q.trim().toLowerCase())).slice(0, 6) : [];
  return (
    <div style={{ position:"relative", marginTop:8 }}>
      <input className="input focus-ring mono" value={q} onChange={e=>{ setQ(e.target.value); setOpen(true); }} onFocus={()=>setOpen(true)}
        placeholder="+ pin a specific control not caught by the rule…" style={{ fontSize:11.5 }}/>
      {open && matches.length > 0 && (
        <div style={{ position:"absolute", top:"100%", left:0, right:0, marginTop:2, background:"var(--cf-card-bg)", border:"1px solid var(--cf-divider)", borderRadius:8, zIndex:5, boxShadow:"0 8px 20px rgba(0,0,0,0.25)", overflow:"hidden" }}>
          {matches.map(p => (
            <button key={p.id} className="focus-ring" onClick={()=>{ onPick(p.id); setQ(""); setOpen(false); }}
              style={{ all:"unset", cursor:"pointer", display:"block", width:"100%", boxSizing:"border-box", padding:"7px 10px", fontSize:11.5, fontFamily:"var(--font-mono)" }}
              onMouseDown={e=>e.preventDefault()}>
              {p.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// Chip-based id picker for SRG/CCI — these aren't small closed sets like control family or
// CMMC level (DISA publishes hundreds of SRGs and thousands of CCIs), so we can't offer a
// full dropdown. Instead: type-ahead suggestions from ids already used elsewhere in this
// registry, rendered as removable chips, with free entry for ids not yet known here.
// Shared "comma-separated string -> trimmed id array" parser, used by both the SRG/CCI
// save logic and the chip picker's own add/remove.
function parseIdList(v) { return (v || "").split(",").map(s => s.trim()).filter(Boolean); }
function IdChipPicker({ value, onChange, allKnownIds, placeholder }) {
  const ids = parseIdList(value);
  const [draft, setDraft] = React.useState("");
  const [open, setOpen] = React.useState(false);
  const suggestions = draft.trim()
    ? allKnownIds.filter(id => id.toLowerCase().includes(draft.trim().toLowerCase()) && !ids.includes(id)).slice(0, 6)
    : [];
  const commit = (id) => {
    const clean = id.trim();
    if (!clean || ids.includes(clean)) { setDraft(""); setOpen(false); return; }
    onChange([...ids, clean].join(", "));
    setDraft(""); setOpen(false);
  };
  const remove = (id) => onChange(ids.filter(x => x !== id).join(", "));
  return (
    <div style={{ position:"relative" }}>
      <div className="input focus-ring" style={{ height:"auto", minHeight:36, display:"flex", flexWrap:"wrap", gap:6, alignItems:"center", padding:"6px 8px" }}>
        {ids.map(id => (
          <span key={id} className="chip chip-unknown mono" style={{ fontSize:10.5, display:"inline-flex", alignItems:"center", gap:4 }}>
            {id}
            <button className="focus-ring" onClick={()=>remove(id)} style={{ all:"unset", cursor:"pointer", display:"inline-flex", opacity:0.6 }}><Icon name="x" size={9}/></button>
          </span>
        ))}
        <input value={draft} onChange={e=>{ setDraft(e.target.value); setOpen(true); }} onFocus={()=>setOpen(true)}
          onKeyDown={e=>{ if (e.key==="Enter"||e.key===",") { e.preventDefault(); commit(draft); } if (e.key==="Backspace" && !draft && ids.length) remove(ids[ids.length-1]); }}
          placeholder={ids.length ? "Add another…" : placeholder}
          style={{ all:"unset", flex:1, minWidth:120, fontFamily:"var(--font-mono)", fontSize:11.5 }}/>
      </div>
      {open && suggestions.length > 0 && (
        <div style={{ position:"absolute", top:"100%", left:0, right:0, marginTop:2, background:"var(--cf-card-bg)", border:"1px solid var(--cf-divider)", borderRadius:8, zIndex:5, boxShadow:"0 8px 20px rgba(0,0,0,0.25)", overflow:"hidden" }}>
          {suggestions.map(id => (
            <button key={id} className="focus-ring" onMouseDown={e=>e.preventDefault()} onClick={()=>commit(id)}
              style={{ all:"unset", cursor:"pointer", display:"block", width:"100%", boxSizing:"border-box", padding:"7px 10px", fontSize:11, fontFamily:"var(--font-mono)" }}>
              {id}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function PolicyCard({ group, onOpen, onEdit, selectMode, selected }) {
  const [shownId, setShownId] = React.useState(group.current.id);
  const policy = group.revisions.find(r => r.id === shownId) || group.current;
  const multi = group.revisions.length > 1;
  const isSelected = selectMode && selected.has(policy.id);
  const usage = policyUsage(policy.id);
  const cat = policyCategoryMeta(policy.category || "deployment");
  const disabled = policy.type === "custom" && policy.enabled === false;
  const railColor = disabled ? "#6b7280" : cat.color;

  return (
    <div className="sys-card" onClick={() => onOpen(policy)} style={{ cursor:"pointer", opacity: disabled ? 0.72 : 1, outline: isSelected ? "2px solid var(--cf-brand-purple)" : "none", outlineOffset: -1 }}>
      <div className="status-rail" style={{ "--status-color": railColor }}/>
      {selectMode && (
        <span className={`pol-select-box${isSelected?" checked":""}`}>
          {isSelected && <Icon name="check" size={11} style={{ color:"#fff" }}/>}
        </span>
      )}
      <div className="sys-card-head">
        <div className="sys-title">
          <div className="sys-hostname"><Icon name="file" size={13}/>&nbsp;{policy.name}</div>
          <div style={{ fontSize:11, color:"var(--cf-text-secondary)" }}>{policy.description}</div>
        </div>
        <div style={{ display:"flex", flexDirection:"column", alignItems:"flex-end", gap:5 }}>
          {policy.type === "builtin"
            ? <span className="chip chip-info">built-in</span>
            : <span className="chip chip-healthy">custom</span>}
          {multi && <PubStateChip state={policy.publicationState}/>}
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

      {(typeof mappingsForPolicy === "function" && (mappingsForPolicy(policy.id).length || (typeof bundlesUsingPolicy === "function" && bundlesUsingPolicy(policy.id).count))) ? (
        <div style={{ display:"flex", flexWrap:"wrap", gap:5 }}>
          {mappingsForPolicy(policy.id).length > 0 && <span className="chip chip-info" style={{ fontSize:9.5 }}>{mappingsForPolicy(policy.id).length} mapped requirement{mappingsForPolicy(policy.id).length===1?"":"s"}</span>}
          {bundlesUsingPolicy(policy.id).count > 0 && <span className="chip chip-unknown" style={{ fontSize:9.5 }}>used by {bundlesUsingPolicy(policy.id).count} bundle{bundlesUsingPolicy(policy.id).count===1?"":"s"}</span>}
        </div>
      ) : null}

      <div style={{ paddingTop:10, borderTop:"1px solid var(--cf-divider)", display:"flex", justifyContent:"space-between", alignItems:"center" }}>
        <div style={{ display:"flex", alignItems:"center", gap:8 }}>
          <Icon name="server" size={11} style={{ color:"var(--cf-text-muted)" }}/>
          <span className="mono" style={{ fontSize:12, fontWeight:600 }}>{usage.count}</span>
          <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>systems use this</span>
        </div>
        {onEdit && policy.type === "custom" && (
          <button className="btn btn-subtle focus-ring" style={{ padding:"4px 10px", fontSize:12 }} onClick={e=>{ e.stopPropagation(); onEdit(policy); }}>
            <Icon name="gear" size={12}/> Edit
          </button>
        )}
      </div>

      {multi && (
        <div style={{ margin:"2px -16px -16px", borderTop:"1px solid var(--cf-divider)", background:"var(--cf-subtle-bg)", borderRadius:"0 0 var(--radius-xl) var(--radius-xl)" }}>
          <button className="focus-ring" onClick={e => { e.stopPropagation(); onOpen(policy, "revisions"); }}
            style={{ all:"unset", cursor:"pointer", display:"flex", alignItems:"center", justifyContent:"space-between", width:"100%", boxSizing:"border-box", padding:"9px 14px", fontSize:11.5, color:"var(--cf-text-secondary)" }}>
            <span>{group.revisions.length} revisions</span>
            <Icon name="chevron-right" size={12}/>
          </button>
        </div>
      )}
    </div>
  );
}

/* Drawer/modal for browsing many revisions at once — used when a lineage has more than a few. */
function RevisionPickerModal({ title, revisions, currentId, selectedId, onSelect, onClose }) {
  const [query, setQuery] = React.useState("");
  const q = query.toLowerCase();
  const filtered = revisions.filter(r => !q
    || String(r.revision).includes(q) || (r.version||"").toLowerCase().includes(q)
    || (r.publicationState||"").toLowerCase().includes(q) || (r.publishedDate||"").includes(q)
    || (r.digest||"").toLowerCase().includes(q));
  return ReactDOM.createPortal((
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(520px,94vw)", maxHeight:"84vh", display:"flex", flexDirection:"column" }}>
        <div className="modal-head">
          <h2><Icon name="sync" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>{title} — {revisions.length} revisions</h2>
          <p>Pick an exact revision. The selection is preserved as-is — it won't silently move to “latest” later.</p>
        </div>
        <div className="modal-body" style={{ overflowY:"auto", flex:1 }}>
          <div className="filter-search" style={{ marginBottom:10 }}>
            <Icon name="search"/>
            <input className="input focus-ring" placeholder="Search revision, state, date, digest…" value={query} onChange={e=>setQuery(e.target.value)} autoFocus/>
          </div>
          <div style={{ display:"flex", flexDirection:"column", gap:5 }}>
            {filtered.map(r => {
              const isSel = r.id === selectedId;
              return (
                <button key={r.id} onClick={() => onSelect(r.id)} className="focus-ring"
                  style={{ all:"unset", cursor:"pointer", display:"flex", alignItems:"center", justifyContent:"space-between", gap:8,
                    padding:"9px 12px", borderRadius:8, background: isSel ? "color-mix(in oklab,var(--cf-brand-purple) 12%, transparent)" : "var(--cf-subtle-bg)",
                    border: `1px solid ${isSel ? "var(--cf-brand-purple)" : "var(--cf-divider)"}` }}>
                  <span style={{ display:"flex", alignItems:"center", gap:8, minWidth:0 }}>
                    <span className="mono" style={{ fontSize:12, fontWeight:600, flexShrink:0 }}>Rev {r.revision}</span>
                    {r.version && <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{r.version}</span>}
                    {r.digest && <span className="mono" style={{ fontSize:10, color:"var(--cf-text-muted)", overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{r.digest}</span>}
                  </span>
                  <span style={{ display:"flex", alignItems:"center", gap:6, flexShrink:0 }}>
                    <span style={{ fontSize:10.5, color:"var(--cf-text-muted)" }}>{r.publishedDate}</span>
                    <PubStateChip state={r.publicationState}/>
                    {r.id === currentId && <span className="chip" style={{ fontSize:8.5, color:"#34d399", background:"color-mix(in oklab, #34d399 16%, transparent)" }}>Current</span>}
                  </span>
                </button>
              );
            })}
            {filtered.length === 0 && (
              <div style={{ fontSize:12, color:"var(--cf-text-muted)", padding:"16px 0", textAlign:"center" }}>No revisions match “{query}”.</div>
            )}
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  ), document.body);
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

function PolicyDrawer({ policy, onClose, onEdit, onOpenSystem, onSwitchPolicy, initialTab }) {
  const [tab, setTab] = React.useState(initialTab || "details");
  React.useEffect(() => { setTab(initialTab || "details"); }, [policy.lineageId || policy.id]);
  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const usage = policyUsage(policy.id);
  const group = React.useMemo(() => {
    const groups = (typeof groupPoliciesByLineage === "function") ? groupPoliciesByLineage(POLICIES) : [];
    return groups.find(g => g.lineageId === (policy.lineageId || policy.id));
  }, [policy]);
  const multi = group && group.revisions.length > 1;

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
            {policy.type === "custom" && <button className="btn btn-ghost focus-ring xs" onClick={()=>exportPolicies([policy])}><Icon name="download" size={11}/> Export</button>}
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

        {multi && (
          <div className="seg" style={{ margin:"12px 22px 0", width:"fit-content" }}>
            <button className={tab==="details"?"active":""} onClick={()=>setTab("details")}>Details</button>
            <button className={tab==="revisions"?"active":""} onClick={()=>setTab("revisions")}>Revisions · {group.revisions.length}</button>
          </div>
        )}

        {tab === "revisions" && multi ? (
          <div className="ed-body" style={{ padding:"18px 22px", overflow:"auto", display:"flex", flexDirection:"column", gap:8 }}>
            <div style={{ fontSize:12, color:"var(--cf-text-muted)", marginBottom:2 }}>Revision history for this policy lineage — selecting a revision does not change which policy other bundles reference.</div>
            {group.revisions.map(r => {
              const isSel = r.id === policy.id;
              return (
                <button key={r.id} onClick={() => onSwitchPolicy?.(r)} className="focus-ring"
                  style={{ all:"unset", cursor:"pointer", display:"flex", alignItems:"center", justifyContent:"space-between", gap:10,
                    padding:"10px 12px", borderRadius:9, background: isSel ? "color-mix(in oklab,var(--cf-brand-purple) 10%, transparent)" : "var(--cf-subtle-bg)",
                    border: `1px solid ${isSel ? "var(--cf-brand-purple)" : "var(--cf-divider)"}` }}>
                  <span style={{ display:"flex", alignItems:"center", gap:10, minWidth:0 }}>
                    <span className="mono" style={{ fontSize:12.5, fontWeight:600, flexShrink:0 }}>Revision {r.revision}</span>
                    <span style={{ fontSize:11.5, color:"var(--cf-text-secondary)", overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{r.description}</span>
                  </span>
                  <span style={{ display:"flex", alignItems:"center", gap:8, flexShrink:0 }}>
                    <span style={{ fontSize:10.5, color:"var(--cf-text-muted)" }}>{r.publishedDate}</span>
                    <PubStateChip state={r.publicationState}/>
                    {r.id === group.current.id && <span className="chip" style={{ fontSize:8.5, color:"#34d399", background:"color-mix(in oklab, #34d399 16%, transparent)" }}>Current</span>}
                  </span>
                </button>
              );
            })}
          </div>
        ) : (
        <div className="ed-body" style={{ padding:"18px 22px", overflow:"auto", display:"flex", flexDirection:"column", gap:18 }}>
          {policy.rationale && (
            <section>
              <h3 style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", margin:"0 0 8px", fontWeight:600 }}>Rationale</h3>
              <div style={{ fontSize:13, color:"var(--cf-text-primary)", lineHeight:1.5 }}>{policy.rationale}</div>
            </section>
          )}
          <section>
            <h3 style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", margin:"0 0 8px", fontWeight:600 }}>
              Mapped Requirements · {mappingsForPolicy(policy.id).length}
            </h3>
            {mappingsForPolicy(policy.id).length === 0 ? (
              <div className="sd-callout sd-callout-info">
                <Icon name="check" size={13}/>
                <div style={{ fontSize:12 }}>This policy is not currently mapped to an external compliance requirement. It can still be used as an operational or custom policy.</div>
              </div>
            ) : (
              <div style={{ display:"flex", flexDirection:"column", gap:12 }}>
                {mappingsGroupedByFramework(policy.id).map(grp => (
                  <div key={grp.framework?.id || "unknown"}>
                    <div style={{ fontSize:11.5, fontWeight:700, color:"var(--cf-text-primary)", marginBottom:6 }}>{grp.framework?.name} <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>{grp.framework?.version}</span></div>
                    <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
                      {grp.rows.map(({ mapping, requirement }) => (
                        <div key={mapping.id} style={{ padding:"9px 11px", background:"var(--cf-subtle-bg)", borderRadius:8, border:"1px solid var(--cf-divider)" }}>
                          <div style={{ display:"flex", justifyContent:"space-between", gap:8 }}>
                            <span className="mono" style={{ fontSize:12, fontWeight:600 }}>{requirement.externalId}</span>
                            <span style={{ fontSize:9.5, color:"var(--cf-text-muted)" }}>{mapping.provenance === "imported" ? `Imported from ${mapping.importedFrom||"benchmark"}` : "Manual mapping"}</span>
                          </div>
                          <div style={{ fontSize:11.5, color:"var(--cf-text-secondary)", margin:"2px 0 5px" }}>{requirement.title}</div>
                          <div style={{ fontSize:11, display:"flex", gap:6, alignItems:"center" }}>
                            <span style={{ fontWeight:600, color:"var(--cf-text-primary)" }}>{relationshipMeta(mapping.relationship).label}</span>
                            <span style={{ color:"var(--cf-text-muted)" }}>· {mapping.coverage === "full" ? "Full" : "Partial"} coverage</span>
                          </div>
                          {mapping.rationale && <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:5, lineHeight:1.4 }}>{mapping.rationale}</div>}
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            )}
            {typeof suggestedForPolicy === "function" && suggestedForPolicy(policy.id).length > 0 && (
              <div style={{ marginTop:12 }}>
                <div style={{ fontSize:10.5, fontWeight:600, color:"var(--cf-text-muted)", marginBottom:6 }}>Suggested mappings</div>
                {suggestedForPolicy(policy.id).map(s => { const req = reqById(s.requirementId); const fw = frameworkById(req?.frameworkId); return (
                  <div key={s.id} style={{ padding:"8px 11px", background:"color-mix(in oklab, #a78bfa 8%, transparent)", borderRadius:8, border:"1px dashed color-mix(in oklab, #a78bfa 40%, transparent)", fontSize:11.5 }}>
                    <span className="mono" style={{ fontWeight:600 }}>{fw?.name} · {req?.externalId}</span>
                    <span style={{ color:"var(--cf-text-muted)", marginLeft:6 }}>Derived from {s.derivedFrom}</span>
                  </div>
                ); })}
              </div>
            )}
          </section>
          <section>
            <h3 style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", margin:"0 0 8px", fontWeight:600 }}>
              Used by bundles · {bundlesUsingPolicy(policy.id).count}
            </h3>
            {bundlesUsingPolicy(policy.id).count === 0 ? (
              <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>Not part of any compliance bundle yet.</div>
            ) : (
              <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
                {[...new Set(bundlesUsingPolicy(policy.id).bundles.map(b=>b.lineageId||b.id))].map(lid => {
                  const b = bundlesUsingPolicy(policy.id).bundles.find(x => (x.lineageId||x.id)===lid);
                  return <span key={lid} className="chip chip-unknown" style={{ fontSize:10.5 }}>{b.name}</span>;
                })}
              </div>
            )}
          </section>
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
        )}
      </aside>
    </>
  );
}

function PolicyFormModal({ mode, policy, onClose }) {
  const isEdit = mode === "edit";
  const knownSrgIds = React.useMemo(()=>[...new Set(POLICIES.flatMap(p=>p.srgIds||[]))].sort(), []);
  const knownCciIds = React.useMemo(()=>[...new Set(POLICIES.flatMap(p=>p.cciIds||[]))].sort(), []);
  const [form, setForm] = React.useState(() => isEdit && policy ? {
    name: policy.name,
    description: policy.description,
    category: policy.category || "deployment",
    controlFamily: policy.controlFamily || "",
    cmmcLevel: policy.cmmcLevel || "",
    framework: policy.framework || "",
    cisSection: policy.cisSection || "",
    rationale: policy.rationale || "",
    severity: policy.severity || "medium",
    srgIds: (policy.srgIds || []).join(", "),
    cciIds: (policy.cciIds || []).join(", "),
    enabled: policy.enabled !== false,
    rules: [...policy.rules],
    evidence: policy.evidence ? policy.evidence.map(e => ({ ...e })) : [],
    mappings: (typeof mappingsForPolicy === "function" ? mappingsForPolicy(policy.id) : []).map(m => ({ ...m })),
  } : {
    name: "",
    description: "",
    category: "deployment",
    controlFamily: "",
    cmmcLevel: "",
    framework: "",
    cisSection: "",
    rationale: "",
    severity: "medium",
    srgIds: "",
    cciIds: "",
    enabled: true,
    rules: [{ kind:"eval_passed" }, { kind:"build_succeeded" }],
    evidence: [],
    mappings: [],
  });
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const [tab, setTab] = React.useState("details");
  const [mappingEditor, setMappingEditor] = React.useState(null); // { mapping } | { } for new
  const set = (k,v) => setForm(p => ({ ...p, [k]: v }));
  const saveMapping = (m) => set("mappings", form.mappings.some(x=>x.id===m.id) ? form.mappings.map(x=>x.id===m.id?m:x) : [...form.mappings, m]);
  const removeMapping = (id) => set("mappings", form.mappings.filter(x=>x.id!==id));

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

  const parseIdList = (s) => s.split(",").map(x => x.trim()).filter(Boolean);

  const doSave = () => {
    const srgIds = parseIdList(form.srgIds);
    const cciIds = parseIdList(form.cciIds);
    const policyId = isEdit ? policy.id : `custom-${slugify(form.name) || Date.now()}`;
    if (isEdit) {
      Object.assign(policy, {
        name: form.name, description: form.description, category: form.category,
        controlFamily: form.category === "security" ? (form.controlFamily || null) : undefined,
        cmmcLevel: form.category === "security" ? (form.cmmcLevel || null) : undefined,
        framework: form.category === "security" ? (form.framework || null) : undefined,
        cisSection: form.category === "security" ? (form.cisSection || null) : undefined,
        rationale: form.rationale, severity: form.severity, srgIds, cciIds,
        enabled: form.enabled, rules: form.rules, evidence: form.evidence,
        lastModified: "just now",
      });
    } else {
      POLICIES.push({
        id: policyId, lineageId: policyId, revision: 1, publicationState: "current", publishedDate: new Date().toISOString().slice(0,10),
        name: form.name, description: form.description, category: form.category,
        controlFamily: form.category === "security" ? (form.controlFamily || null) : null,
        cmmcLevel: form.category === "security" ? (form.cmmcLevel || null) : null,
        framework: form.category === "security" ? (form.framework || null) : null,
        cisSection: form.category === "security" ? (form.cisSection || null) : null,
        rationale: form.rationale, severity: form.severity, srgIds, cciIds,
        type: "custom", enabled: form.enabled, rules: form.rules, evidence: form.evidence,
        createdBy: "you", createdAt: "just now", lastModified: "just now",
      });
    }
    if (typeof POLICY_REQUIREMENT_MAPPINGS !== "undefined") {
      for (let i = POLICY_REQUIREMENT_MAPPINGS.length - 1; i >= 0; i--) { if (POLICY_REQUIREMENT_MAPPINGS[i].policyId === policyId) POLICY_REQUIREMENT_MAPPINGS.splice(i,1); }
      form.mappings.forEach(m => POLICY_REQUIREMENT_MAPPINGS.push({ ...m, policyId }));
    }
  };
  const doDelete = () => {
    const idx = POLICIES.findIndex(p => p.id === policy.id);
    if (idx >= 0) POLICIES.splice(idx, 1);
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(680px,96vw)", maxHeight:"92vh" }}>
        {confirmDelete ? (
          <DeletePolicyConfirm policy={policy} onCancel={()=>setConfirmDelete(false)} onConfirm={doDelete}/>
        ) : (
          <>
            <div className="modal-head">
              <h2><Icon name={isEdit ? "gear" : "plus"} size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>
                {isEdit ? `Edit ${policy.name}` : "New custom policy"}
              </h2>
              <p>{isEdit ? "Update the rules and rationale." : "Compose a policy from gate rules. Systems can be assigned this policy from their edit dialog."}</p>
            </div>
            <div className="modal-body" style={{ overflowY:"auto", display:"flex", flexDirection:"column" }}>
              <div style={{ border:"1px solid var(--cf-divider)", borderRadius:10, overflow:"hidden", flexShrink:0 }}>
                <div style={{ display:"flex", borderBottom:"1px solid var(--cf-divider)", background:"var(--cf-subtle-bg)" }}>
                  {[
                    { id:"details",    label:"Details", color:"var(--cf-text-primary)" },
                    { id:"mappings",   label:`Mappings · ${form.mappings.length}`, color:"#34d399" },
                    { id:"rule",       label:`Enforcement · ${form.rules.length}`, color:"var(--cf-brand-purple)" },
                    { id:"evidence",   label:`Evidence · ${form.evidence.length}`, color:"#60a5fa" },
                  ].map(t => (
                    <button key={t.id} type="button" onClick={()=>setTab(t.id)} className="focus-ring"
                      style={{ all:"unset", cursor:"pointer", flex:1, textAlign:"center", padding:"10px 8px", fontSize:12, fontWeight:600,
                        color: tab===t.id ? t.color : "var(--cf-text-muted)",
                        background: tab===t.id ? "var(--cf-card-bg)" : "transparent",
                        borderBottom: tab===t.id ? `2px solid ${t.color}` : "2px solid transparent",
                        marginBottom:-1 }}>
                      {t.label}
                    </button>
                  ))}
                </div>
                <div style={{ padding:16 }}>

              {tab === "details" && (
              <>
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
                <label>Domain</label>
                <div className="seg" style={{ width:"fit-content" }}>
                  {POLICY_DOMAINS.map(dom => {
                    const active = (policyCategoryMeta(form.category).domain || "platform") === dom.id;
                    return (
                      <button key={dom.id} type="button" className={active ? "active" : ""}
                        onClick={()=>{ if (dom.id === "security") set("category", "security"); else if (form.category === "security") set("category", "deployment"); }}
                        style={active ? { color: dom.color } : undefined}>
                        <span style={{ display:"inline-flex", alignItems:"center", gap:6 }}>
                          <Icon name={dom.icon} size={12}/> {dom.label}
                        </span>
                      </button>
                    );
                  })}
                </div>
                <div className="help">{POLICY_DOMAINS.find(d=>d.id===(policyCategoryMeta(form.category).domain||"platform"))?.blurb}</div>
              </div>
              {policyCategoryMeta(form.category).domain !== "security" && (
                <div className="field">
                  <label>Category</label>
                  <div style={{ display:"grid", gridTemplateColumns:"repeat(auto-fit, minmax(150px, 1fr))", gap:8 }}>
                    {POLICY_CATEGORIES.filter(c => c.domain === "platform").map(c => {
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
              )}
              {form.category === "security" && (
                <details className="pol-adv-meta" style={{ margin:"4px 0 10px" }}>
                  <summary style={{ cursor:"pointer", fontSize:11, fontWeight:600, color:"var(--cf-text-muted)", textTransform:"uppercase", letterSpacing:"0.06em" }}>Source metadata (advanced)</summary>
                  <div style={{ fontSize:11.5, color:"var(--cf-text-muted)", margin:"8px 0 12px", lineHeight:1.5 }}>
                    Retained for import fidelity and legacy grouping views — this policy's compliance meaning now lives in <strong>Mappings</strong>, not this framework/family tagging.
                  </div>
                <div className="field">
                  <label>Framework</label>
                  <select className="input focus-ring" value={form.framework||""} onChange={e=>set("framework", e.target.value)}>
                    <option value="">Choose a framework…</option>
                    <optgroup label="Standard">
                      {BUILTIN_FRAMEWORKS.map(f => <option key={f}>{f}</option>)}
                    </optgroup>
                    {loadCustomFrameworks().length > 0 && (
                      <optgroup label="Custom">
                        {loadCustomFrameworks().map(f => <option key={f.name}>{f.name}</option>)}
                      </optgroup>
                    )}
                  </select>
                  <div className="help">Determines which grouping field applies below — e.g. NIST 800-53 asks for a control family, CMMC asks for a level.</div>
                </div>
              {form.framework === "NIST 800-53" && (
                <div className="field">
                  <label>NIST 800-53 control family <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· drives the "Group by → NIST family" view</span></label>
                  <select className="input focus-ring" value={form.controlFamily} onChange={e=>set("controlFamily", e.target.value)}>
                    <option value="">Unassigned (shows as "Ungrouped")</option>
                    {Object.values(CONTROL_FAMILIES).map(f => <option key={f.id} value={f.id}>{f.id} — {f.label}</option>)}
                  </select>
                  <div className="help">At scale, most bulk-imported controls won't have this set — an admin can also route them into a group with a text-match rule from Security controls → Manage groupings, without editing each one.</div>
                </div>
              )}
              {form.category === "security" && form.framework === "CMMC 2.0" && (
                <div className="field">
                  <label>CMMC 2.0 level <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· drives the "Group by → CMMC level" view</span></label>
                  <select className="input focus-ring" value={form.cmmcLevel||""} onChange={e=>set("cmmcLevel", e.target.value ? parseInt(e.target.value,10) : "")}>
                    <option value="">Derive from severity (default)</option>
                    <option value={1}>Level 1 — Foundational</option>
                    <option value={2}>Level 2 — Advanced</option>
                    <option value={3}>Level 3 — Expert</option>
                  </select>
                  <div className="help">Left unset, the level is inferred from STIG severity. Set it explicitly if this control maps to a different CMMC practice than its severity implies.</div>
                </div>
              )}
              {form.category === "security" && form.framework === "CIS Benchmark" && (
                <div className="field">
                  <label>CIS section <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· drives the "Group by → CIS Benchmark section" view</span></label>
                  <input className="input focus-ring mono" value={form.cisSection||""} onChange={e=>set("cisSection", e.target.value)} placeholder="e.g. 5.2.3"/>
                  <div className="help">CIS Benchmark section number this control maps to. SRG/CCI don't apply to CIS — those are DISA-specific identifiers.</div>
                </div>
              )}
              {form.category === "security" && form.framework === "DISA STIG" && (
              <>
              <div className="field">
                <label>SRG IDs</label>
                <IdChipPicker value={form.srgIds} onChange={v=>set("srgIds",v)}
                  allKnownIds={knownSrgIds}
                  placeholder="Type to search or add an SRG id…"/>
                <div className="help">Security Requirements Guide IDs this control satisfies — searchable from the policy list.</div>
              </div>
              <div className="field">
                <label>CCI IDs</label>
                <IdChipPicker value={form.cciIds} onChange={v=>set("cciIds",v)}
                  allKnownIds={knownCciIds}
                  placeholder="Type to search or add a CCI id…"/>
                <div className="help">CCI mappings, if applicable.</div>
              </div>
              </>
              )}
              </details>
              )}
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
              </>
              )}

              {tab === "mappings" && (
              <>
              <div style={{ fontSize:12, color:"var(--cf-text-secondary)", marginBottom:10, lineHeight:1.5 }}>
                Map this policy to the compliance requirements it implements, supports, or provides evidence for. Policies can map to requirements from multiple frameworks.
              </div>
              {form.mappings.length === 0 ? (
                <div className="sd-callout sd-callout-info" style={{ marginBottom:10 }}>
                  <Icon name="check" size={13}/>
                  <div style={{ fontSize:12 }}>No compliance mappings yet. This policy can still be used as an operational/custom policy with zero mappings.</div>
                </div>
              ) : (
                <div style={{ display:"flex", flexDirection:"column", gap:8, marginBottom:10 }}>
                  {form.mappings.map(m => { const req = reqById(m.requirementId); const fw = frameworkById(req?.frameworkId); const readOnly = m.provenance === "imported"; return (
                    <div key={m.id} style={{ padding:"9px 11px", background:"var(--cf-subtle-bg)", borderRadius:8, border:"1px solid var(--cf-divider)" }}>
                      <div style={{ display:"flex", justifyContent:"space-between", gap:8, alignItems:"flex-start" }}>
                        <div style={{ minWidth:0 }}>
                          <div style={{ fontSize:11, fontWeight:700, color:"var(--cf-text-muted)", textTransform:"uppercase", letterSpacing:"0.04em" }}>{fw?.name} {fw?.version}</div>
                          <div className="mono" style={{ fontSize:12.5, fontWeight:600, marginTop:2 }}>{req?.externalId} <span className="mono" style={{ fontWeight:400, color:"var(--cf-text-secondary)" }}>· {req?.title}</span></div>
                          <div style={{ fontSize:11, marginTop:4 }}><strong>{relationshipMeta(m.relationship).label}</strong> <span style={{ color:"var(--cf-text-muted)" }}>· {m.coverage === "full" ? "Full" : "Partial"} coverage</span></div>
                          {m.rationale && <div style={{ fontSize:10.5, color:"var(--cf-text-muted)", marginTop:3 }}>{m.rationale}</div>}
                          <div style={{ fontSize:9.5, color:"var(--cf-text-muted)", marginTop:4 }}>{readOnly ? `Imported from ${m.importedFrom||"benchmark"} · read-only` : "Manual mapping"}</div>
                        </div>
                        {!readOnly && (
                          <div style={{ display:"flex", gap:4, flexShrink:0 }}>
                            <button className="btn-icon focus-ring" title="Edit mapping" onClick={()=>setMappingEditor(mappingEditor?.mapping?.id===m.id ? null : { mapping:m })}><Icon name="gear" size={12}/></button>
                            <button className="btn-icon focus-ring" title="Remove mapping" onClick={()=>removeMapping(m.id)}><Icon name="x" size={12}/></button>
                          </div>
                        )}
                      </div>
                      {mappingEditor?.mapping?.id === m.id && (
                        <InlineMappingEditor
                          initial={mappingEditor.mapping}
                          existingMappings={form.mappings}
                          onCancel={()=>setMappingEditor(null)}
                          onSave={(mm)=>{ saveMapping(mm); setMappingEditor(null); }}
                        />
                      )}
                    </div>
                  ); })}
                </div>
              )}
              {mappingEditor && !mappingEditor.mapping && (
                <InlineMappingEditor
                  existingMappings={form.mappings}
                  onCancel={()=>setMappingEditor(null)}
                  onSave={(mm)=>{ saveMapping(mm); setMappingEditor(null); }}
                />
              )}
              {!mappingEditor && (
                <button className="btn btn-ghost focus-ring" type="button" onClick={()=>setMappingEditor({})}>
                  <Icon name="plus" size={12}/> Add mapping
                </button>
              )}
              </>
              )}

              {tab === "rule" && (
              <>
              {/* Rules */}
              <div>
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
              </>
              )}

              {tab === "evidence" && (
              <>
              {/* Evidence */}
              <div>
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
              </>
              )}

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
              <button className="btn btn-primary focus-ring" onClick={()=>{ doSave(); onClose(); }} disabled={!form.name}>
                <Icon name="check" size={13}/> {isEdit ? "Save changes" : "Create policy"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function InlineMappingEditor({ initial, onCancel, onSave, existingMappings }) {
  const initReq = initial ? reqById(initial.requirementId) : null;
  const [frameworkId, setFrameworkId] = React.useState(initReq?.frameworkId || "");
  const [query, setQuery] = React.useState("");
  const [requirementId, setRequirementId] = React.useState(initial?.requirementId || "");
  const [relationship, setRelationship] = React.useState(initial?.relationship || "implements");
  const [coverage, setCoverage] = React.useState(initial?.coverage || "full");
  const [rationale, setRationale] = React.useState(initial?.rationale || "");
  const results = frameworkId ? reqSearch(frameworkId, query).filter(r => reqChildren(r.id).length === 0) : [];
  const dup = requirementId && !initial && existingMappings.some(m => m.requirementId === requirementId);
  const canSave = frameworkId && requirementId && relationship && coverage && !dup;
  const req = requirementId ? reqById(requirementId) : null;
  const doSave = () => {
    onSave({ id: initial?.id || mapId(), requirementId, relationship, coverage, rationale: rationale.trim() || undefined, provenance:"manual" });
  };
  return (
    <div style={{ border:"1px solid var(--cf-brand-purple)", borderRadius:10, padding:14, background:"color-mix(in oklab, var(--cf-brand-purple) 5%, var(--cf-card-bg))", display:"flex", flexDirection:"column", gap:14, marginTop:8 }}>
        <div>
          <div style={{ fontSize:12.5, fontWeight:600 }}>{initial ? "Edit mapping" : "Add mapping"}</div>
          <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>Map this policy to a compliance requirement it implements, supports, or provides evidence for.</div>
        </div>
          <div className="field">
            <label>Framework</label>
            <select className="input focus-ring" value={frameworkId} onChange={e=>{ setFrameworkId(e.target.value); setRequirementId(""); setQuery(""); }}>
              <option value="">Choose a framework…</option>
              {COMPLIANCE_FRAMEWORKS.map(f => <option key={f.id} value={f.id}>{f.name} · {f.version}</option>)}
            </select>
          </div>
          {frameworkId && (
            <div className="field">
              <label>Requirement</label>
              {req && !query ? (
                <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", padding:"8px 10px", background:"var(--cf-subtle-bg)", borderRadius:8, border:"1px solid var(--cf-divider)" }}>
                  <div>
                    <div className="mono" style={{ fontSize:12.5, fontWeight:600 }}>{req.externalId} <span style={{ fontWeight:400, color:"var(--cf-text-secondary)" }}>· {req.title}</span></div>
                    <div style={{ fontSize:10.5, color:"var(--cf-text-muted)", marginTop:2 }}>{reqBreadcrumb(req.id).slice(0,-1).map(r=>r.externalId).join(" › ") || frameworkById(req.frameworkId)?.name}</div>
                  </div>
                  <button className="btn btn-ghost focus-ring xs" onClick={()=>{ setRequirementId(""); setQuery(""); }}>Change</button>
                </div>
              ) : (
                <>
                  <input className="input focus-ring" autoFocus value={query} onChange={e=>setQuery(e.target.value)} placeholder="Search by ID, title, or CCI…"/>
                  <div style={{ maxHeight:220, overflowY:"auto", display:"flex", flexDirection:"column", gap:4, marginTop:6 }}>
                    {results.slice(0,40).map(r => (
                      <button key={r.id} className="focus-ring" onClick={()=>{ setRequirementId(r.id); setQuery(""); }}
                        style={{ all:"unset", cursor:"pointer", textAlign:"left", padding:"7px 9px", borderRadius:7, background:"var(--cf-subtle-bg)" }}>
                        <div className="mono" style={{ fontSize:12, fontWeight:600 }}>{r.externalId}</div>
                        <div style={{ fontSize:11, color:"var(--cf-text-secondary)" }}>{r.title}</div>
                        <div style={{ fontSize:9.5, color:"var(--cf-text-muted)", marginTop:1 }}>{reqBreadcrumb(r.id).slice(0,-1).map(x=>x.externalId).join(" › ") || r.kind}</div>
                      </button>
                    ))}
                    {results.length === 0 && <div style={{ fontSize:11.5, color:"var(--cf-text-muted)", padding:"6px 2px" }}>No requirements match.</div>}
                  </div>
                </>
              )}
              {dup && <div className="help" style={{ color:"#fbbf24" }}><Icon name="warn" size={10} style={{ verticalAlign:"middle" }}/> Already mapped to this requirement.</div>}
            </div>
          )}
          <div className="field">
            <label>Relationship</label>
            <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
              {RELATIONSHIPS.map(r => (
                <button key={r.id} type="button" className="focus-ring" onClick={()=>setRelationship(r.id)}
                  style={{ all:"unset", cursor:"pointer", display:"flex", flexDirection:"column", gap:2, padding:"8px 10px", borderRadius:8,
                    background: relationship===r.id ? "color-mix(in oklab, var(--cf-brand-purple) 12%, transparent)" : "var(--cf-subtle-bg)",
                    border: `1px solid ${relationship===r.id ? "var(--cf-brand-purple)" : "var(--cf-divider)"}` }}>
                  <span style={{ fontSize:12, fontWeight:600 }}>{r.label}</span>
                  <span style={{ fontSize:10.5, color:"var(--cf-text-muted)" }}>{r.blurb}</span>
                </button>
              ))}
            </div>
          </div>
          <div className="field">
            <label>Coverage</label>
            <div className="seg" style={{ width:"fit-content" }}>
              <button className={coverage==="full"?"active":""} onClick={()=>setCoverage("full")}>Full</button>
              <button className={coverage==="partial"?"active":""} onClick={()=>setCoverage("partial")}>Partial</button>
            </div>
          </div>
          <div className="field">
            <label>Mapping rationale <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· optional</span></label>
            <textarea className="input focus-ring" rows={2} value={rationale} onChange={e=>setRationale(e.target.value)} placeholder="Why this policy satisfies the requirement" style={{ resize:"vertical" }}/>
          </div>
        <div style={{ display:"flex", justifyContent:"flex-end", gap:8 }}>
          <button className="btn btn-ghost focus-ring" type="button" onClick={onCancel}>Cancel</button>
          <button className="btn btn-primary focus-ring" type="button" disabled={!canSave} onClick={()=>{ doSave(); }}><Icon name="check" size={13}/> Save mapping</button>
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
          <textarea className="input focus-ring mono code-editor" rows={3} value={rule.expr} onChange={e=>onChange({ expr:e.target.value })}
            placeholder="config.networking.firewall.enable == true" style={{ fontSize:12, resize:"vertical" }}/>
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

Object.assign(window, { PoliciesView, RuleEditor, policyToExternal, externalToPolicy, slugify, downloadFile, exportPolicies, parsePolicyFile, ruleDescription, ImportPoliciesModal, RevisionPickerModal, AdminGroupingsModal, InlineMappingEditor });
