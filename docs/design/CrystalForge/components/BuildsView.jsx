// Builds view — workers + queue + active/history tabs + build detail pane

// Shared live indicator — shows the view streams + a relative "updated Ns ago"
function LiveIndicator({ label = "Live" }) {
  const [secs, setSecs] = React.useState(0);
  React.useEffect(() => {
    const id = setInterval(() => setSecs(s => (s + 1) % 6), 1000);
    return () => clearInterval(id);
  }, []);
  return (
    <div style={{ display:"flex", alignItems:"center", gap:8, fontSize:12, color:"var(--cf-text-muted)" }}>
      <span style={{ display:"inline-flex", alignItems:"center", gap:6 }}>
        <Pulse style={{ position:"static", margin:0 }} />
        <span style={{ color:"#34d399", fontWeight:600 }}>{label}</span>
      </span>
      <span>· updated {secs === 0 ? "just now" : `${secs}s ago`}</span>
    </div>
  );
}
window.LiveIndicator = LiveIndicator;

function BuildsView() {
  const [tab, setTab] = React.useState("active");
  const hasFailed = (typeof HISTORY_BUILDS !== "undefined" ? HISTORY_BUILDS : []).some(b => b.status === "failed");
  // Tab pulses continuously while there are failures the user hasn't looked at yet.
  const [ackedHist, setAckedHist] = React.useState(false);
  const flashTab = hasFailed && !ackedHist;
  // On first opening Completed, acknowledge + pulse the failed rows once.
  const [flashHistRows, setFlashHistRows] = React.useState(false);
  React.useEffect(() => {
    if (tab === "history" && !ackedHist && hasFailed) {
      setAckedHist(true);
      acknowledgeView("builds");
      setFlashHistRows(true);
      const t = setTimeout(() => setFlashHistRows(false), 3200);
      return () => clearTimeout(t);
    }
  }, [tab]);
  const [selected, setSelected] = React.useState(null);
  const [logOpen, setLogOpen] = React.useState(false);
  const sel = useMultiSelect(tab);

  const [activeList, setActiveList] = React.useState(ACTIVE_BUILDS);
  const historyList = HISTORY_BUILDS;

  const moveBuild = (id, dir) => {
    setActiveList(prev => {
      const idx = prev.findIndex(b => b.id === id);
      if ((dir === -1 && idx === 0) || (dir === 1 && idx === prev.length - 1)) return prev;
      const next = [...prev];
      [next[idx], next[idx + dir]] = [next[idx + dir], next[idx]];
      return next;
    });
  };
  const reorderBuild = (id, toIdx) => {
    setActiveList(prev => {
      const from = prev.findIndex(b => b.id === id);
      if (from === -1 || toIdx < 0 || toIdx >= prev.length || from === toIdx) return prev;
      const next = [...prev];
      const [moved] = next.splice(from, 1);
      next.splice(toIdx, 0, moved);
      return next;
    });
  };

  const isCancellable = (b) => b.status === "building" || b.status === "queued" || b.status === "cache-pushing" || b.status === "stopping";
  const cancellable = activeList.filter(isCancellable);

  // Search filter — matches system, flake, commit, worker, arch, status label.
  const [query, setQuery] = React.useState("");
  React.useEffect(() => { setQuery(""); }, [tab]);
  const q = query.trim().toLowerCase();
  const matchBuild = (b) => !q ||
    [b.system, b.flake, b.commit, b.worker, b.arch, b.meta?.label, b.currentPkg, b.failedPkg]
      .filter(Boolean).some(v => String(v).toLowerCase().includes(q));
  const baseList = tab === "active" ? activeList : historyList;
  const filteredList = baseList.filter(matchBuild);
  // Selection eligibility: active = cancellable builds; completed = every row.
  const selectableIds = tab === "active" ? cancellable.map(b => b.id) : filteredList.map(b => b.id);

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      {/* Page head */}
      <div className="page-head">
        <div>
          <h1 className="page-title">Builds</h1>
          <p className="page-subtitle">{BUILD_STATS.building} building · {BUILD_STATS.queued} queued · {BUILD_STATS.workers}/{BUILD_STATS.totalWorkers} workers active</p>
        </div>
        <LiveIndicator />
      </div>

      {/* Stat strip */}
      <div className="stat-strip">
        {[
          { label:"Building",    val:BUILD_STATS.building,    color:"#60a5fa" },
          { label:"Queued",      val:BUILD_STATS.queued,      color:"#a78bfa" },
          { label:"Failed 24h",  val:BUILD_STATS.failed24h,   color:"#f87171" },
          { label:"Workers",     val:`${BUILD_STATS.workers}/${BUILD_STATS.totalWorkers}`, color:"#34d399" },
          { label:"Slot usage",  val:Math.round(BUILD_WORKERS.filter(w=>w.status==="running").reduce((a,w)=>a+w.slots.used,0)/Math.max(1,BUILD_WORKERS.filter(w=>w.status==="running").reduce((a,w)=>a+w.slots.total,0))*100)+"%", color:"#22d3ee" },
        ].map(s => (
          <div key={s.label} className="stat">
            <span className="stat-accent" style={{ "--stat-color": s.color }} />
            <div className="stat-label">{s.label}</div>
            <div className="stat-value" style={{ color:s.color }}>{s.val}</div>
          </div>
        ))}
      </div>

      {/* Workers */}
      <section>
        <div style={{ fontSize:12, fontWeight:600, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", marginBottom:10 }}>Build Workers</div>
        <div style={{ display:"grid", gridTemplateColumns:"repeat(auto-fill,minmax(240px,1fr))", gap:10 }}>
          {BUILD_WORKERS.map(w => <WorkerCard key={w.id} w={w} />)}
        </div>
      </section>

      {/* Queue tabs */}
      <div className="card" style={{ overflow:"hidden" }}>
        <div className="sd-tabs q-tabbar" style={{ padding:"0 16px", borderBottom:"1px solid var(--cf-card-border)" }}>
          {[{k:"active",l:"Active",n:activeList.length},{k:"history",l:"Completed",n:historyList.length}].map(t => (
            <button key={t.k} className={`sd-tab focus-ring${tab===t.k?" active":""}${flashTab && t.k==="history"?" attention-flash-tab":""}`} onClick={()=>{setTab(t.k);setSelected(null);}}>
              {t.l} <span className="sd-tab-badge">{t.n}</span>
            </button>
          ))}
          {selectableIds.length > 0 && <MultiSelectHint />}
          <div className="q-search">
            <Icon name="search" size={13} />
            <input className="q-search-input" placeholder={`Search ${tab==="active"?"active":"completed"} builds…`}
              value={query} onChange={e=>setQuery(e.target.value)} />
            {q && <span className="q-search-count">{filteredList.length} of {baseList.length}</span>}
            {q && <button className="btn-icon xs focus-ring" title="Clear search" onClick={()=>setQuery("")}><Icon name="x" size={13}/></button>}
          </div>
        </div>
        {filteredList.length === 0 ? (
          <div className="q-empty">
            <Icon name="search" size={20} />
            <div>No builds match “{query}”.</div>
            <button className="btn btn-ghost xs focus-ring" onClick={()=>setQuery("")}>Clear search</button>
          </div>
        ) : (
          <BuildQueueTable
            entries={filteredList}
            selected={selected}
            onSelect={setSelected}
            onLog={(b) => { setSelected(b); setLogOpen(true); }}
            sel={sel}
            isCancellable={isCancellable}
            cancellable={cancellable}
            selectableIds={selectableIds}
            reorderable={tab==="active" && !q}
            flashFailed={tab==="history" && flashHistRows}
            onMove={moveBuild}
            onReorder={reorderBuild}
          />
        )}
      </div>

      <BulkBar count={sel.size} onClear={sel.clear}>
        {tab === "active" ? (
          <button className="btn btn-danger xs focus-ring"
            onClick={() => { alert(`Cancelling ${sel.size} build${sel.size===1?"":"s"}…`); sel.clear(); }}>
            <Icon name="x" size={12} /> Cancel {sel.size} build{sel.size===1?"":"s"}
          </button>
        ) : (
          <>
            <button className="btn btn-ghost xs focus-ring"
              onClick={() => { alert(`Re-running ${sel.size} build${sel.size===1?"":"s"}…`); sel.clear(); }}>
              <Icon name="rollback" size={12} /> Re-run {sel.size}
            </button>
            <button className="btn btn-ghost xs focus-ring"
              onClick={() => { alert(`Downloading logs for ${sel.size} build${sel.size===1?"":"s"}…`); }}>
              <Icon name="download" size={12} /> Download logs
            </button>
            <button className="btn btn-danger xs focus-ring"
              onClick={() => { alert(`Deleting ${sel.size} build${sel.size===1?"":"s"} from history…`); sel.clear(); }}>
              <Icon name="x" size={12} /> Delete {sel.size}
            </button>
          </>
        )}
      </BulkBar>

      {/* Build detail drawer (tabbed: Log + Details, like the eval drawer) */}
      {selected && (
        <BuildDetailPanel build={selected} initialTab={logOpen ? "log" : "details"} onClose={() => { setSelected(null); setLogOpen(false); }} />
      )}
    </div>
  );
}

function WorkerCard({ w }) {
  const statusColor = { running:"#34d399", paused:"#fbbf24", draining:"#60a5fa", offline:"#6b7280" }[w.status] || "#6b7280";
  const pct = w.slots.total ? Math.round(w.slots.used/w.slots.total*100) : 0;
  return (
    <div className="card" style={{ padding:"14px 16px", display:"flex", flexDirection:"column", gap:10 }}>
      <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between" }}>
        <div>
          <div style={{ fontSize:13, fontWeight:600 }}>{w.name}</div>
          <div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{w.host}</div>
        </div>
        <span className="chip" style={{ color:statusColor, background:statusColor+"22", fontSize:10 }}>{w.status}</span>
      </div>
      <div style={{ fontSize:11, color:"var(--cf-text-secondary)", display:"flex", gap:12 }}>
        <span>{w.arch}</span>
        <span>{w.cores}c · {w.mem}GB</span>
      </div>
      <div>
        <div style={{ display:"flex", justifyContent:"space-between", fontSize:11, color:"var(--cf-text-muted)", marginBottom:4 }}>
          <span>Slots</span><span>{w.slots.used}/{w.slots.total}</span>
        </div>
        <div style={{ height:4, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
          <div style={{ width:`${pct}%`, height:"100%", background:statusColor }} />
        </div>
      </div>
    </div>
  );
}

function BuildQueueTable({ entries, selected, onSelect, onLog, sel, isCancellable, cancellable, selectableIds, reorderable, onMove, onReorder, flashFailed }) {
  const cancellableIds = sel ? (selectableIds || cancellable.map(b => b.id)) : [];
  const [dragId, setDragId] = React.useState(null);
  const [overIdx, setOverIdx] = React.useState(null);
  const dragIdx = dragId ? entries.findIndex(e => e.id === dragId) : -1;
  return (
    <table className="sys-table q-queue-table">
      <thead>
        <tr>
          {reorderable && <th style={{ width:48 }}>#</th>}
          <th>System configuration</th>
          <th>Status</th>
          <th>Worker</th>
          <th>Derivations</th>
          <th>Queued</th>
          <th>Duration</th>
          <th style={{ textAlign:"right" }}>{reorderable ? "Reorder · actions" : " "}</th>
        </tr>
      </thead>
      <tbody>
        {entries.map((b, i) => {
          const checked = sel && sel.has(b.id);
          const isDragging = dragId === b.id;
          const showDropBefore = reorderable && dragId && overIdx === i && dragIdx > i;
          const showDropAfter  = reorderable && dragId && overIdx === i && dragIdx < i;
          return (
          <tr key={b.id}
            draggable={reorderable || undefined}
            onDragStart={reorderable ? (e)=>{ setDragId(b.id); e.dataTransfer.effectAllowed="move"; try{ e.dataTransfer.setData("text/plain", b.id); }catch{} } : undefined}
            onDragOver={reorderable ? (e)=>{ e.preventDefault(); e.dataTransfer.dropEffect="move"; if (overIdx!==i) setOverIdx(i); } : undefined}
            onDrop={reorderable ? (e)=>{ e.preventDefault(); if (dragId) onReorder(dragId, i); setDragId(null); setOverIdx(null); } : undefined}
            onDragEnd={reorderable ? ()=>{ setDragId(null); setOverIdx(null); } : undefined}
            className={`${sel?"selectable ":""}q-row ${selected?.id===b.id?"selected":""}${checked?" row-checked":""}${isDragging?" q-dragging":""}${showDropBefore?" q-drop-before":""}${showDropAfter?" q-drop-after":""}${flashFailed && b.status==="failed"?" attention-flash":""}`}
            onMouseDown={sel ? (e)=>{ if(e.shiftKey) e.preventDefault(); } : undefined}
            onClick={(e)=>{ if (sel && sel.handleClick(e, b.id, cancellableIds)) return; if (sel) sel.setAnchor(b.id); onSelect(b); }}>
            {reorderable && (
              <td onClick={e=>e.stopPropagation()}>
                <div style={{ display:"flex", alignItems:"center", gap:6 }}>
                  <span className="q-drag-handle" title="Drag to reorder"><Icon name="grip" size={15}/></span>
                  <span style={{ color:"var(--cf-text-muted)", fontSize:12, fontVariantNumeric:"tabular-nums" }}>{i+1}</span>
                </div>
              </td>
            )}
            <td>
              <div style={{ fontWeight:600, fontSize:13, display:"flex", alignItems:"center", gap:6 }}>
                <Icon name="server" size={12} style={{ color:"var(--cf-text-muted)" }}/>{b.system}
              </div>
              <div style={{ fontSize:10, color:"var(--cf-text-muted)" }}>{b.flake} · <span className="mono">{b.commit}</span> · {b.arch}</div>
              {b.currentPkg && <div className="mono" style={{ fontSize:10, color:"#60a5fa", marginTop:2 }}>building {b.currentPkg}…</div>}
              {b.failedPkg && <div className="mono" style={{ fontSize:10, color:"#f87171", marginTop:2 }}>failed on {b.failedPkg}</div>}
            </td>
            <td><span className={`chip ${b.meta.cls}`}><span className="chip-dot" style={{ background:b.meta.color }} />{b.meta.label}</span></td>
            <td><span className="mono" style={{ fontSize:12, color:"var(--cf-text-secondary)" }}>{b.worker || "—"}</span></td>
            <td style={{ width:140 }}>
              <div style={{ display:"flex", alignItems:"center", gap:8 }}>
                <div style={{ flex:1, height:5, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden", display:"flex" }}>
                  <div style={{ width:`${(b.cachedDerivs/b.totalDerivs)*100}%`, background:"#34d399" }} title={`${b.cachedDerivs} from cache`}/>
                  <div style={{ width:`${((b.builtDerivs-b.cachedDerivs)/b.totalDerivs)*100}%`, background:b.meta.color, transition:"width 1s" }} title={`${b.builtDerivs-b.cachedDerivs} built`}/>
                </div>
                <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)", whiteSpace:"nowrap" }}>{b.builtDerivs}/{b.totalDerivs}</span>
              </div>
            </td>
            <td style={{ fontSize:12, color:"var(--cf-text-muted)" }}>{b.queuedAt}</td>
            <td>{b.dur ? <LiveDuration dur={b.dur} live={b.status==="building"||b.status==="cache-pushing"} style={{ fontSize:12, color:"var(--cf-text-secondary)" }}/> : <span className="mono" style={{ fontSize:12, color:"var(--cf-text-muted)" }}>—</span>}</td>
            <td onClick={reorderable ? (e=>e.stopPropagation()) : undefined}>
              <div className="row-actions" style={reorderable ? { opacity:1, gap:6, justifyContent:"flex-end" } : undefined}>
                {reorderable && (
                  <div className="q-move-group">
                    <button className="q-move-btn focus-ring" title="Move up" disabled={i===0} onClick={()=>onMove(b.id,-1)}><Icon name="chevron-up" size={15}/></button>
                    <button className="q-move-btn focus-ring" title="Move down" disabled={i===entries.length-1} onClick={()=>onMove(b.id,1)}><Icon name="chevron-down" size={15}/></button>
                  </div>
                )}
                <button className="btn-icon focus-ring" title="Logs" onClick={e=>{e.stopPropagation();onLog(b);}}><Icon name="terminal" size={14} /></button>
                {(b.status === "building" || b.status === "queued" || b.status === "cache-pushing") && (
                  <button className="btn-icon focus-ring" title="Cancel build" onClick={e=>e.stopPropagation()}><Icon name="x" size={14} /></button>
                )}
                {b.status === "stopping" && (
                  <button className="btn-icon focus-ring" title="Force kill" style={{ color:"var(--cf-red)" }} onClick={e=>e.stopPropagation()}><Icon name="x" size={14} /></button>
                )}
                {b.status === "failed" && (
                  <button className="btn-icon focus-ring" title="Retry build" onClick={e=>e.stopPropagation()}><Icon name="rollback" size={14} /></button>
                )}
              </div>
            </td>
          </tr>
          );
        })}
      </tbody>
    </table>
  );
}

// Build log lines (mock) — shared by the Log tab.
function buildLogLines(b) {
  const pkgs = ["glibc-2.40","openssl-3.3.2","zlib-1.3.1","gcc-13.3.0","systemd-256.7","linux-6.12.4",
    "python3-3.12.7","perl-5.40.0","coreutils-9.5","bash-5.2","curl-8.11.0","nginx-1.27.4",
    "postgresql-16.4","redis-7.4.1","prometheus-2.55.1","grafana-11.3.0","node-22.11.0","go-1.23.4"];
  const l = [
    { t:"12:04:01", lvl:"info",  m:`builder @ ${b.worker}: building ${b.system} (${b.totalDerivs} derivations)` },
    { t:"12:04:02", lvl:"info",  m:`${b.cachedDerivs} substitutes available from cache` },
    { t:"12:04:02", lvl:"info",  m:`evaluating ${b.system}#nixosConfigurations — 1 flake input locked` },
  ];
  let sec = 4;
  pkgs.forEach((p, i) => {
    l.push({ t:`12:04:${String(sec).padStart(2,"0")}`, lvl:"info", m:`building '/nix/store/${(Math.random().toString(36).slice(2,10))}-${p}.drv'` });
    sec += 1 + (i % 3);
    if (i === 5) l.push({ t:`12:04:${String(sec).padStart(2,"0")}`, lvl:"warn", m:`warning: dumping very large path (> 256 MiB) to binary cache` });
    if (i === 9) l.push({ t:`12:04:${String(sec).padStart(2,"0")}`, lvl:"info", m:`fetched ${b.cachedDerivs} paths from s3://crystal-forge-cache (${(Math.random()*40+5).toFixed(1)} MiB/s)` });
    if (i === 12) l.push({ t:`12:04:${String(sec).padStart(2,"0")}`, lvl:"warn", m:`warning: deprecated attribute 'lib.mdDoc' used in ${p}` });
    sec += 1;
  });
  l.push({ t:`12:05:00`, lvl:"info", m:`built ${b.builtDerivs}/${b.totalDerivs} derivations` });
  l.push({ t:`12:05:02`, lvl:"info", m:`running post-build hook: sign + push to cache` });
  if (b.status === "failed") {
    l.push({ t:"12:05:08", lvl:"error", m:`error: builder for '/nix/store/…-${b.failedPkg}.drv' failed with exit code 1` });
    l.push({ t:"12:05:08", lvl:"error", m:`       last 10 log lines: see ${b.failedPkg} build output above` });
  }
  if (b.status === "complete" || b.status === "cache-pushed") {
    l.push({ t:"12:05:11", lvl:"info", m:`signed 38 paths with key cache-key-1` });
    l.push({ t:"12:05:12", lvl:"info", m:`build of ${b.system} succeeded (${b.totalDerivs} derivations)` });
  }
  return l;
}

// Tabbed build drawer — mirrors the eval drawer (header actions, stats grid, Log/Details tabs).
function BuildDetailPanel({ build: b, initialTab = "details", onClose }) {
  const [tab, setTab] = React.useState(initialTab);
  const [maximized, setMaximized] = React.useState(false);
  const live = b.status === "building" || b.status === "cache-pushing";
  const active = live || b.status === "queued" || b.status === "stopping";

  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <>
      <div className="fl-tray-backdrop" onClick={onClose} />
      <aside className={`fl-tray build-log-tray${maximized ? " build-log-tray-max" : ""}`} role="dialog" aria-label="Build detail">
        <header className="fl-tray-head">
          <div style={{ display:"flex", alignItems:"center", gap:12, minWidth:0, flex:1 }}>
            <Icon name="build" size={18} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }} />
            <div style={{ minWidth:0 }}>
              <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                <span style={{ fontWeight:700, fontSize:15 }}>{b.system}</span>
                <span className={`chip ${b.meta.cls}`} style={{ fontSize:10 }}>
                  <span className="chip-dot" style={{ background:b.meta.color }} />{b.meta.label}
                  {live && <Pulse style={{ marginLeft:6 }} />}
                </span>
              </div>
              <div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>
                {b.commit} · {b.drv.slice(0,40)}…
              </div>
            </div>
          </div>
          <div style={{ display:"flex", gap:6, alignItems:"center", flexShrink:0 }}>
            {active && <button className="btn btn-ghost focus-ring xs" style={b.status==="stopping"?{ color:"var(--cf-red)" }:null}>{b.status==="stopping" ? "Force kill" : "Cancel"}</button>}
            {b.status === "failed" && <button className="btn btn-ghost focus-ring xs"><Icon name="rollback" size={12} /> Retry</button>}
            <button className="btn-icon focus-ring" title={maximized ? "Restore" : "Expand"} onClick={()=>setMaximized(m=>!m)}>
              <Icon name={maximized ? "minimize" : "maximize"} size={15} />
            </button>
            <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={16} /></button>
          </div>
        </header>

        {/* Stats grid */}
        <div className="ed-stats">
          <div className="ed-stat">
            <div className="ed-stat-label">Queued</div>
            <div className="ed-stat-val" style={{ fontSize:12.5, fontWeight:600 }}><DTG at={b.queuedAt} relative={b.queuedAt}/></div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Duration</div>
            <div className="ed-stat-val" style={{ fontFamily:"var(--font-mono)" }}>{b.dur ? <LiveDuration dur={b.dur} live={live}/> : "—"}</div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Derivations</div>
            <div className="ed-stat-val">{b.builtDerivs}<span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>/{b.totalDerivs}</span></div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Worker</div>
            <div className="ed-stat-val mono" style={{ fontSize:12.5 }}>{b.worker || "—"}</div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Arch</div>
            <div className="ed-stat-val mono" style={{ fontSize:12.5 }}>{b.arch}</div>
          </div>
        </div>

        {/* Tabs */}
        <div className="sd-tabs" style={{ padding:"0 16px", borderBottom:"1px solid var(--cf-card-border)", flexShrink:0 }}>
          <button className={`sd-tab focus-ring${tab==="log"?" active":""}`} onClick={()=>setTab("log")}>
            <Icon name="terminal" size={12}/> Log {live && <Pulse style={{ marginLeft:4 }} />}
          </button>
          <button className={`sd-tab focus-ring${tab==="details"?" active":""}`} onClick={()=>setTab("details")}>
            <Icon name="info" size={12}/> Details
          </button>
        </div>

        {/* Body */}
        {tab === "log"     && <BuildLogTab b={b} live={live} />}
        {tab === "details" && <BuildDetailsTab b={b} live={live} />}
      </aside>
    </>
  );
}

/* ── Details tab ── */
function BuildDetailsTab({ b, live }) {
  return (
    <div className="ed-body" style={{ padding:"14px 16px" }}>
      <dl className="kv-grid">
        <dt>System</dt><dd className="mono">{b.system}</dd>
        <dt>Flake</dt><dd>{b.flake}</dd>
        <dt>Commit</dt><dd className="mono">{b.commit}</dd>
        <dt>Worker</dt><dd className="mono">{b.worker || "unassigned"}</dd>
        <dt>Arch</dt><dd className="mono">{b.arch}</dd>
        <dt>Derivations</dt><dd>{b.builtDerivs}/{b.totalDerivs} built · {b.cachedDerivs} cached</dd>
        <dt>Queued</dt><dd><DTG at={b.queuedAt} relative={b.queuedAt}/></dd>
        {["complete","failed","cache-pushed"].includes(b.status) && b.dur && (
          <>
            <dt>{b.status==="failed" ? "Failed" : "Completed"}</dt>
            <dd><DTG at={new Date(relToDate(b.queuedAt).getTime() + parseDur(b.dur)*1000)}/></dd>
          </>
        )}
        <dt>Duration</dt><dd className="mono">{b.dur ? <LiveDuration dur={b.dur} live={live}/> : "—"}</dd>
        <dt>Attempts</dt><dd>{b.attempts}</dd>
      </dl>
      {b.progress > 0 && b.progress < 1 && (
        <section style={{ marginTop:18 }}>
          <h3 style={{ fontSize:12, fontWeight:600, margin:"0 0 8px", color:"var(--cf-text-secondary)" }}>Derivation progress</h3>
          <div style={{ height:6, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden", display:"flex" }}>
            <div style={{ width:`${(b.cachedDerivs/b.totalDerivs)*100}%`, background:"#34d399" }} />
            <div style={{ width:`${((b.builtDerivs-b.cachedDerivs)/b.totalDerivs)*100}%`, background:b.meta.color }} />
          </div>
          <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:4 }}>
            {b.builtDerivs} of {b.totalDerivs} derivations · {b.currentPkg && <span className="mono" style={{ color:"#60a5fa" }}>building {b.currentPkg}</span>}
          </div>
        </section>
      )}
    </div>
  );
}

/* ── Log tab (search + tail, matching the eval log tab) ── */
function BuildLogTab({ b, live }) {
  const lines = React.useMemo(() => buildLogLines(b), [b.id]);
  const [query, setQuery] = React.useState("");
  const [matchIdx, setMatchIdx] = React.useState(0);
  const ref = React.useRef(null);
  const searchRef = React.useRef(null);

  const q = query.trim().toLowerCase();
  const matches = React.useMemo(() =>
    q ? lines.map((l, i) => l.m.toLowerCase().includes(q) || l.t.includes(q) ? i : -1).filter(i => i >= 0) : [],
    [q, lines]);

  React.useEffect(() => { if (ref.current && !q) ref.current.scrollTop = ref.current.scrollHeight; }, []);
  React.useEffect(() => { setMatchIdx(0); }, [q]);
  React.useEffect(() => {
    if (!matches.length || !ref.current) return;
    const el = ref.current.querySelector(`[data-li="${matches[Math.min(matchIdx, matches.length-1)]}"]`);
    if (el) el.scrollIntoView({ block:"center" });
  }, [matchIdx, matches]);
  React.useEffect(() => {
    const onKey = (e) => {
      if (e.key === "Enter" && matches.length && document.activeElement === searchRef.current) {
        e.preventDefault(); setMatchIdx(i => (i + (e.shiftKey ? -1 : 1) + matches.length) % matches.length);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [matches]);

  const renderMsg = (m) => {
    if (!q) return m;
    const lo = m.toLowerCase();
    const out = []; let from = 0, idx;
    while ((idx = lo.indexOf(q, from)) !== -1) {
      if (idx > from) out.push(m.slice(from, idx));
      out.push(<mark key={idx} className="log-hit">{m.slice(idx, idx + q.length)}</mark>);
      from = idx + q.length;
    }
    out.push(m.slice(from));
    return out;
  };
  const activeLine = matches.length ? matches[Math.min(matchIdx, matches.length-1)] : -1;

  return (
    <div style={{ display:"flex", flexDirection:"column", flex:1, minHeight:0 }}>
      <div style={{ padding:"8px 16px", borderBottom:"1px solid var(--cf-divider)", display:"flex", gap:10, alignItems:"center", flexShrink:0 }}>
        <span style={{ fontSize:11, color:"var(--cf-text-muted)", whiteSpace:"nowrap" }}>
          {q ? `${matches.length} ${matches.length===1?"match":"matches"}` : `${lines.length} lines`}
        </span>
        <div style={{ flex:1 }}/>
        <div className="log-search">
          <Icon name="search" size={13} />
          <input ref={searchRef} className="log-search-input" placeholder="Search log…" value={query}
            onChange={e=>setQuery(e.target.value)} />
          {q && <span className="log-search-count">{matches.length ? `${Math.min(matchIdx,matches.length-1)+1}/${matches.length}` : "0"}</span>}
          {q && (
            <>
              <button className="btn-icon xs focus-ring" title="Previous match (Shift+Enter)" disabled={!matches.length}
                onClick={()=>setMatchIdx(i=>(i-1+matches.length)%matches.length)}><Icon name="chevron-up" size={13}/></button>
              <button className="btn-icon xs focus-ring" title="Next match (Enter)" disabled={!matches.length}
                onClick={()=>setMatchIdx(i=>(i+1)%matches.length)}><Icon name="chevron-down" size={13}/></button>
            </>
          )}
        </div>
        <button className="btn-icon focus-ring" title="Download"><Icon name="download" size={13}/></button>
      </div>
      <pre ref={ref} className="sd-log-stream build-log-stream">
        {lines.map((l,i) => {
          const isHit = matches.includes(i);
          const isActive = i === activeLine;
          return (
            <div key={i} data-li={i} className={`sd-log-line sd-log-${l.lvl}${isHit ? " log-line-hit" : ""}${isActive ? " log-line-active" : ""}`}>
              <span className="sd-log-t">{l.t}</span>
              <span className="sd-log-lvl">{l.lvl.toUpperCase()}</span>
              <span className="sd-log-m">{renderMsg(l.m)}</span>
            </div>
          );
        })}
        {!q && live && <div className="sd-log-caret">▍</div>}
      </pre>
    </div>
  );
}

Object.assign(window, { BuildsView });
