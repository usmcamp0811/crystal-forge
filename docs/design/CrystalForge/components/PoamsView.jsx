// POA&M — one register for remediation plans and risk acceptances.
// They answer the same question about a deficiency (what are we doing about it, and by when?)
// with different commitments: a plan fixes it by a target date; an acceptance lets it stand until
// a review date. Both are scoped to the same findings, so they are browsed together and kept
// visually distinct. Queues surface what needs action; the scope bar drills environment → host
// (or bundle / owner) inline, so the list keeps the full width.

const RR_KINDS = [["all","Everything"],["poam","Remediation plans"],["ra","Risk acceptances"]];
const RR_SORTS = [["urgency","Urgency"],["due","Due / review date"],["severity","Risk"],["updated","Last activity"],["id","ID"]];

function PoamsView({ focus, onClearFocus, onOpenSystem }) {
  usePoamStore();
  const [prefs, setPrefs] = React.useState(() => {
    const d = { groupBy:"env", sort:"urgency", kind:"all", cols:{ ms:true, owner:true } };
    try { return { ...d, ...JSON.parse(localStorage.getItem("cf.poams.prefs") || "{}"), cols:{ ...d.cols, ...(JSON.parse(localStorage.getItem("cf.poams.prefs") || "{}").cols || {}) } }; } catch { return d; }
  });
  const setPref = (k, v) => setPrefs(p => { const n = { ...p, [k]:v }; try { localStorage.setItem("cf.poams.prefs", JSON.stringify(n)); } catch {} return n; });
  const setCol = (k, v) => setPrefs(p => { const n = { ...p, cols:{ ...p.cols, [k]:v } }; try { localStorage.setItem("cf.poams.prefs", JSON.stringify(n)); } catch {} return n; });
  const [colsOpen, setColsOpen] = React.useState(false);
  const colsRef = React.useRef(null);
  React.useEffect(() => {
    if (!colsOpen) return;
    const off = (e) => { if (colsRef.current && !colsRef.current.contains(e.target)) setColsOpen(false); };
    document.addEventListener("mousedown", off);
    return () => document.removeEventListener("mousedown", off);
  }, [colsOpen]);
  const kind = prefs.kind;
  const [queue, setQueue] = React.useState(null);
  const [scope, setScopeState] = React.useState(null);
  const [dim, setDimState] = React.useState("env");
  const [status, setStatus] = React.useState("open");
  const [sev, setSev] = React.useState("all");
  const [mine, setMine] = React.useState(false);
  const [q, setQ] = React.useState("");
  const sel = useMultiSelect();
  const [collapsed, setCollapsed] = React.useState({});
  const [flatLimit, setFlatLimit] = React.useState(50);
  const [raOpen, setRaOpen] = React.useState(null);
  const resetPaging = () => { sel.clear(); setFlatLimit(50); };
  const setScope = (s) => { setScopeState(s || null); resetPaging(); };
  const setDim = (d) => { setDimState(d); setScopeState(null); resetPaging(); };
  const setKind = (k) => { setPref("kind", k); setQueue(null); if (k !== "all" && prefs.groupBy === "type") setPref("groupBy", "env"); resetPaging(); };

  React.useEffect(() => {
    if (!focus) return;
    if (focus.kind) setPref("kind", focus.kind);
    if (focus.raId) setRaOpen(focus.raId);
    if (focus.queue) { setQueue(focus.queue); setStatus("open"); if (!focus.kind && !RR_QUEUES[kind].includes(focus.queue)) setPref("kind", "all"); }
    if (focus.scope) { setDimState(["bundle","owner"].includes(focus.scope.type) ? focus.scope.type : "env"); setScopeState(focus.scope); }
    onClearFocus?.();
  }, [focus]);

  const index = rrIndex();
  const ql = q.trim().toLowerCase();
  const inKind = x => kind === "all" || x.kind === kind;
  const passFilters = x => inKind(x) && (sev === "all" || x.severity === sev) && (!mine || x.owner === RR_ME) && (!ql || x.text.includes(ql));
  const passStatus = x => status === "all" ? true : status === "closed" ? !x.open : x.open;
  const qDef = queue ? RR_Q[queue] : null;
  const filtered = index.filter(passFilters);
  // Queue counts follow the scope; scope pills follow the queue — each shows where the other's work sits.
  const queueBase = filtered.filter(x => x.open && rrInScope(x, scope));
  const scopeBase = filtered.filter(x => passStatus(x) && (!qDef || qDef.test(x)));
  const list = scopeBase.filter(x => rrInScope(x, scope)).sort(rrCompare(prefs.sort));

  const openPlans = index.filter(x => x.kind === "poam" && x.open);
  const openRa = index.filter(x => x.kind === "ra" && x.open);
  const openAll = [...openPlans, ...openRa];
  const envCount = new Set(openAll.flatMap(x => x.envs)).size;
  const sysCount = new Set(openAll.flatMap(x => x.systems.map(s => s.id))).size;
  const kindCount = { all:openAll.length, poam:openPlans.length, ra:openRa.length };

  const selectableIds = list.slice(0, flatLimit).map(x => x.id);
  const selItems = index.filter(x => sel.has(x.id));
  const selPlans = selItems.filter(x => x.kind === "poam"), selRa = selItems.filter(x => x.kind === "ra" && x.open);
  const openItem = (x) => x.kind === "poam" ? openPoamDetail(x.id) : setRaOpen(x.id);

  const groupOptions = [["none","No grouping"], ...(kind === "all" ? [["type","Type"]] : []), ["env","Environment"], ["system","System"], ["bundle","Bundle"], ["owner", kind === "ra" ? "Approver" : "Owner"], ["due","Due / review date"]];
  const groupBy = kind !== "all" && prefs.groupBy === "type" ? "env" : prefs.groupBy;
  let groups = null;
  if (groupBy !== "none") {
    const m = new Map();
    list.forEach(x => rrGroupKeys(x, groupBy).forEach(k => { if (!m.has(k)) m.set(k, []); m.get(k).push(x); }));
    groups = [...m.entries()].map(([name, items]) => ({ name, items, ...rrTally(items) }));
    if (groupBy === "due") groups.sort((a, b) => RR_DUE_ORDER.indexOf(a.name) - RR_DUE_ORDER.indexOf(b.name));
    else if (groupBy === "type") groups.sort((a, b) => a.name < b.name ? 1 : -1);
    else groups.sort((a, b) => b.late - a.late || b.n - a.n);
  }
  const multiMember = groups && ["env", "system", "bundle"].includes(groupBy) && groups.reduce((a, g) => a + g.n, 0) > list.length;
  const gKey = (name) => `${groupBy}:${name}`;
  const isOpen = (name, i) => collapsed[gKey(name)] != null ? !collapsed[gKey(name)] : (i < 3 || groups.length <= 4);
  const groupFocus = (name) => {
    if (groupBy === "env" && name !== "No host scope") return () => { setDimState("env"); setScope({ type:"env", id:name, label:name }); };
    if (groupBy === "system") { const s = SYSTEMS.find(y => y.hostname === name); if (s) return () => { setDimState("env"); setScope({ type:"system", id:s.id, label:s.hostname }); }; }
    if (groupBy === "bundle") { const b = list.flatMap(x => x.bundles).find(v => v.name === name); if (b) return () => { setDimState("bundle"); setScope({ type:"bundle", id:b.id, label:b.name }); }; }
    if (groupBy === "owner") return () => { setDimState("owner"); setScope({ type:"owner", id:name, label:name === "unassigned" ? "Unassigned" : name }); };
    if (groupBy === "type") return () => setKind(name === "Risk acceptances" ? "ra" : "poam");
    return null;
  };

  const clearAll = () => { setQueue(null); setScope(null); setSev("all"); setMine(false); setQ(""); setStatus("open"); };
  const scopeSys = scope?.type === "system" ? SYSTEMS.find(s => s.id === scope.id) : null;
  const raItem = raOpen ? raById(raOpen) : null;

  const activePills = (
    <>
      {qDef && <span className="pv-pill" style={{ "--q":qDef.color }}>{qDef.label}<button type="button" className="focus-ring" aria-label={`Clear ${qDef.label}`} onClick={() => setQueue(null)}><Icon name="x" size={10}/></button></span>}
      {mine && <span className="pv-pill">Mine<button type="button" className="focus-ring" aria-label="Clear mine" onClick={() => setMine(false)}><Icon name="x" size={10}/></button></span>}
    </>
  );

  return (
    <div className="pv" style={{ display:"flex", flexDirection:"column", gap:16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">POA&M</h1>
          <p className="page-subtitle">{openPlans.length} open remediation plans · {openRa.length} active risk acceptances · across {envCount} environments and {sysCount} systems</p>
        </div>
        <div style={{ display:"flex", gap:8 }}>
          <window.ExportMenuButton list={list} scope={scope}/>
        </div>
      </div>

      <div className="rr-kinds" role="tablist" aria-label="Record type">
        {RR_KINDS.map(([k, l]) => (
          <button key={k} type="button" role="tab" aria-selected={kind === k} className={`rr-kind focus-ring${kind === k ? " active" : ""}`} onClick={() => setKind(k)}>
            {k === "poam" && <span className="rr-kind-mark" style={{ background:"#60a5fa" }}/>}
            {k === "ra" && <span className="rr-kind-mark" style={{ background:"#a78bfa" }}/>}
            {l}<span className="rr-kind-n">{kindCount[k]}</span>
          </button>
        ))}
        <span className="rr-kinds-note">{kind === "ra" ? "Decisions to let a deficiency stand, with a justification, an approver and a review date." : kind === "poam" ? "Plans to fix a deficiency by a target date, tracked through milestones to a passing evaluation." : "Plans fix a deficiency by a target date; acceptances let it stand until a review date."}</span>
      </div>

      <div className="pv-queues" role="group" aria-label="Work queues">
        {RR_QUEUES[kind].map(k => {
          const d = RR_Q[k];
          const items = queueBase.filter(d.test);
          const cat1 = items.filter(x => x.severity === "high").length;
          const on = queue === k;
          return (
            <button key={k} type="button" className={`pv-q focus-ring${on ? " active" : ""}`} style={{ "--q":d.color }} aria-pressed={on}
              onClick={() => { setQueue(on ? null : k); if (!on) setStatus("open"); resetPaging(); }}>
              <span className="pv-q-count" style={{ color: items.length ? d.color : "var(--cf-text-muted)" }}>{items.length}</span>
              <span className="pv-q-label">{d.label}</span>
              <span className="pv-q-sub">{items.length ? (cat1 && k !== "cat1" ? `${cat1} CAT I` : d.blurb) : "None"}</span>
            </button>
          );
        })}
      </div>

      <div className={`pv-main card${!prefs.cols.ms ? " hide-ms" : ""}${!prefs.cols.owner ? " hide-owner" : ""}`}>
        <RegisterScopeBar items={scopeBase} scope={scope} onScope={setScope} dim={dim} onDim={setDim}
          count={`${list.length} item${list.length === 1 ? "" : "s"}${multiMember ? " · items spanning several groups appear in each" : ""}`}
          pills={activePills}
          right={scopeSys && onOpenSystem ? <button type="button" className="btn btn-ghost xs focus-ring" onClick={() => onOpenSystem(scopeSys)}><Icon name="server" size={11}/> Open host</button> : null}/>

        <div className="pv-toolbar">
          <div className="filter-search" style={{ flex:"1 1 220px", maxWidth:300 }}>
            <Icon name="search"/>
            <input className="input focus-ring" placeholder="Search ID, title, host, requirement, CVE…" value={q} onChange={e => { setQ(e.target.value); setFlatLimit(50); }}/>
          </div>
          <div className="seg">
            {[["open","Active"],["closed","Closed"],["all","All"]].map(([k, l]) => (
              <button key={k} className={status === k ? "active" : ""} onClick={() => { setStatus(k); if (k === "closed") setQueue(null); }}>{l}</button>
            ))}
          </div>
          <div className="seg">
            {[["all","Any risk"],["high","CAT I"],["medium","CAT II"],["low","CAT III"]].map(([k, l]) => (
              <button key={k} className={sev === k ? "active" : ""} onClick={() => setSev(k)}>{l}</button>
            ))}
          </div>
          <button type="button" className={`btn btn-ghost xs focus-ring${mine ? " pv-on" : ""}`} aria-pressed={mine} onClick={() => setMine(m => !m)}>
            <Icon name="user" size={12}/> Mine
          </button>
          <div className="pv-toolbar-r">
            <MultiSelectHint/>
            <label className="pv-tool-label" htmlFor="pv-group">Group</label>
            <select id="pv-group" className="cfgx-select focus-ring" value={groupBy} onChange={e => setPref("groupBy", e.target.value)}>
              {groupOptions.map(([k, l]) => <option key={k} value={k}>{l}</option>)}
            </select>
            <label className="pv-tool-label" htmlFor="pv-sort">Sort</label>
            <select id="pv-sort" className="cfgx-select focus-ring" value={prefs.sort} onChange={e => setPref("sort", e.target.value)}>
              {RR_SORTS.map(([k, l]) => <option key={k} value={k}>{l}</option>)}
            </select>
            <div className="rr-cols" ref={colsRef}>
              <button type="button" className="btn btn-ghost xs focus-ring" aria-expanded={colsOpen} onClick={() => setColsOpen(o => !o)}><Icon name="grid" size={12}/> Columns</button>
              {colsOpen && (
                <div className="rr-cols-pop card" role="menu" aria-label="Toggle columns">
                  <label className="rr-cols-item"><input type="checkbox" checked={prefs.cols.ms} onChange={e => setCol("ms", e.target.checked)}/> Progress</label>
                  <label className="rr-cols-item"><input type="checkbox" checked={prefs.cols.owner} onChange={e => setCol("owner", e.target.checked)}/> Owner</label>
                </div>
              )}
            </div>
          </div>
        </div>

        {list.length === 0 ? (
          <div className="empty" style={{ margin:0, border:"none" }}>
            <h3>{qDef ? `Nothing in ${qDef.label.toLowerCase()}` : "No items match"}</h3>
            <div>{scope ? `Nothing in ${scope.label} for the current filters.` : "Try widening the filters."}</div>
            <button type="button" className="btn btn-ghost xs focus-ring" style={{ marginTop:12 }} onClick={clearAll}>Reset filters</button>
          </div>
        ) : groups ? (
          <div className="pv-groups">
            {groups.map((g, i) => (
              <RegisterGroup key={gKey(g.name)} name={g.name} by={groupBy} items={g.items}
                open={isOpen(g.name, i)} onToggleOpen={() => setCollapsed(c => ({ ...c, [gKey(g.name)]: isOpen(g.name, i) }))}
                onFocus={groupFocus(g.name)} sel={sel} selectableIds={selectableIds} onOpen={openItem}/>
            ))}
          </div>
        ) : (
          <>
            <RegisterRowsTable rows={list.slice(0, flatLimit)} sel={sel} selectableIds={selectableIds} onOpen={openItem}/>
            {list.length > flatLimit && (
              <button type="button" className="pv-more pv-more-row focus-ring" onClick={() => setFlatLimit(l => l + 50)}>
                Show {Math.min(50, list.length - flatLimit)} more · {list.length - flatLimit} hidden
              </button>
            )}
          </>
        )}
      </div>

      <BulkBar count={selItems.length} onClear={sel.clear}>
        {selPlans.length > 0 && (
          <>
            <select className="cfgx-select focus-ring" value="" aria-label="Reassign owner" onChange={e => e.target.value && selPlans.forEach(x => poamSetField(x.id, "owner", e.target.value))}>
              <option value="" disabled>Reassign owner…</option>
              <PoamOwnerOptions/>
            </select>
            <select className="cfgx-select focus-ring" value="" aria-label="Set plan status" onChange={e => e.target.value && selPlans.forEach(x => poamSetStatus(x.id, e.target.value, `Bulk update to ${POAM_STATUS[e.target.value].label}.`))}>
              <option value="" disabled>Set status…</option>
              {POAM_STATUS_ORDER.filter(s => s !== "completed").map(s => <option key={s} value={s}>{POAM_STATUS[s].label}</option>)}
            </select>
            <button type="button" className="btn btn-ghost xs focus-ring" onClick={() => selPlans.forEach(x => x.item.due && poamSetField(x.id, "due", rrAddDays(x.item.due, 30)))}><Icon name="clock" size={11}/> Extend due 30d</button>
          </>
        )}
        {selRa.length > 0 && <button type="button" className="btn btn-ghost xs focus-ring" onClick={() => selRa.forEach(x => raRenew(x.id, 90))}><Icon name="clock" size={11}/> Renew {selRa.length} acceptance{selRa.length === 1 ? "" : "s"} 90d</button>}
      </BulkBar>

      {raItem && <RiskAcceptanceTray r={raItem} onClose={() => setRaOpen(null)} onOpenSystem={onOpenSystem}/>}
    </div>
  );
}

Object.assign(window, { PoamsView });
