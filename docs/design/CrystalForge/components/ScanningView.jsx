// Scanning view — CVE scan pipeline status + schedule config

function ScanningView({ onNavigate }) {
  const [tab, setTab] = React.useState("deployed");
  const [configOpen, setConfigOpen] = React.useState(false);
  const [logCfg, setLogCfg] = React.useState(null);
  const scanSel = useMultiSelect(tab);

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">Scanning</h1>
          <p className="page-subtitle">
            CVE scanning · vulnix {SCAN_POLICY.vulnixVersion} · DB updated {SCAN_POLICY.dbAge}
          </p>
        </div>
        <div style={{ display:"flex", gap:8 }}>
          <button className="btn btn-ghost focus-ring" onClick={()=>setConfigOpen(true)}>
            <Icon name="gear" size={14}/> Schedule
          </button>
          <button className="btn btn-primary focus-ring"><Icon name="sync" size={14}/> Rescan all</button>
        </div>
      </div>

      <div className="stat-strip">
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color":"#60a5fa" }}/>
          <div className="stat-label">Scanning now</div>
          <div className="stat-value" style={{ color:"#60a5fa" }}>{SCAN_STATS.scanning}</div>
          <div className="stat-meta">{SCAN_STATS.queued} queued · {SCAN_STATS.awaiting} awaiting closure</div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color":"#fbbf24" }}/>
          <div className="stat-label">Stale</div>
          <div className="stat-value" style={{ color:"#fbbf24" }}>{SCAN_STATS.stale}</div>
          <div className="stat-meta">past rescan interval</div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color":"#9ca3af" }}/>
          <div className="stat-label">Never scanned</div>
          <div className="stat-value" style={{ color:"#9ca3af" }}>{SCAN_STATS.unscanned}</div>
        </div>
        <div className="stat" style={{ cursor: SCAN_STATS.failed>0 ? "pointer" : undefined }}
          onClick={SCAN_STATS.failed>0 ? () => { setTab("all"); const f = SCAN_CONFIGS.find(s=>s.status==="failed"); if (f) setLogCfg(f); } : undefined}
          title={SCAN_STATS.failed>0 ? "Open the failing scan's log" : undefined}>
          <span className="stat-accent" style={{ "--stat-color":"#f87171" }}/>
          <div className="stat-label">Failed</div>
          <div className="stat-value" style={{ color: SCAN_STATS.failed>0?"#f87171":"#34d399" }}>{SCAN_STATS.failed}</div>
          {SCAN_STATS.failed>0 && <div className="stat-meta">view log →</div>}
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color":"#34d399" }}/>
          <div className="stat-label">Coverage</div>
          <div className="stat-value" style={{ color:"#34d399" }}>{SCAN_STATS.coverage}%</div>
          <div className="stat-meta">configs with results</div>
        </div>
      </div>

      <div className="card" style={{ overflow:"hidden" }}>
        <div className="sd-tabs" style={{ padding:"0 16px", borderBottom:"1px solid var(--cf-card-border)", display:"flex", alignItems:"center" }}>
          {[
            { k:"deployed", l:"Deployed",  n:SCAN_CONFIGS.filter(x=>x.freshness==="deployed").length },
            { k:"all",      l:"All scans", n:SCAN_CONFIGS.length },
            { k:"systems",  l:"By system", n:(typeof SCAN_HISTORY!=="undefined"?SCAN_HISTORY.length:0) },
          ].map(t => (
            <button key={t.k} className={`sd-tab focus-ring${tab===t.k?" active":""}`} onClick={()=>setTab(t.k)}>
              {t.l} <span className="sd-tab-badge">{t.n}</span>
            </button>
          ))}
        </div>
        {tab === "systems"
          ? <ScanAllConfigs onNavigate={onNavigate} onOpenLog={setLogCfg}/>
          : <ScanQueue
              scope={tab}
              rows={tab === "deployed" ? SCAN_CONFIGS.filter(x=>x.freshness==="deployed") : SCAN_CONFIGS}
              onNavigate={onNavigate} sel={scanSel} onOpenLog={setLogCfg}/>}
      </div>

      {logCfg && <ScanLogDrawer cfg={logCfg} onClose={()=>setLogCfg(null)} onNavigate={onNavigate}/>}

      {configOpen && <ScanScheduleModal onClose={()=>setConfigOpen(false)}/>}

      <BulkBar count={scanSel.size} onClear={scanSel.clear}>
        <button className="btn btn-danger xs focus-ring"
          onClick={() => { alert(`Cancelling ${scanSel.size} scan${scanSel.size===1?"":"s"}…`); scanSel.clear(); }}>
          <Icon name="x" size={12} /> Cancel {scanSel.size} scan{scanSel.size===1?"":"s"}
        </button>
      </BulkBar>
    </div>
  );
}

// Relative timestamps sort by age, so "12m ago" ranks ahead of "3d ago".
function scanAgeMins(v) {
  if (!v) return Infinity;
  if (/scanning|pending|waiting/i.test(v)) return -1;
  if (/never/i.test(v)) return Infinity;
  const n = parseFloat(v);
  if (isNaN(n)) return Infinity;
  if (/mo/.test(v)) return n * 43200;
  if (/w/.test(v)) return n * 10080;
  if (/d/.test(v)) return n * 1440;
  if (/h/.test(v)) return n * 60;
  return n;
}
const SCAN_SEV = (f) => f ? f.crit * 10000 + f.high * 100 + f.med : -1;
const SCAN_STATUS_ORDER = ["failed","awaiting","scanning","queued","stale","complete","unscanned"];

function ScanQueue({ rows, scope, onNavigate, sel, onOpenLog }) {
  const [query, setQuery] = React.useState("");
  const [status, setStatus] = React.useState("all");
  const [fresh, setFresh] = React.useState("all");
  const [latestOnly, setLatestOnly] = React.useState(false);
  const [sort, setSort] = React.useState({ key:"status", dir:"asc" });
  React.useEffect(() => { setQuery(""); setStatus("all"); setFresh("all"); }, [scope]);

  const q = query.trim().toLowerCase();
  const latestIds = React.useMemo(
    () => (typeof latestPerFlake === "function" ? latestPerFlake(rows) : new Set()), [rows]);

  const filtered = rows.filter(r =>
    (status === "all" || r.status === status) &&
    (fresh === "all" || r.freshness === fresh) &&
    (!latestOnly || latestIds.has(r.id)) &&
    (!q || r.name.toLowerCase().includes(q) || r.flake.toLowerCase().includes(q) || (r.commit||"").toLowerCase().includes(q))
  );

  const sorted = React.useMemo(() => {
    const dir = sort.dir === "asc" ? 1 : -1;
    const val = (r) => {
      switch (sort.key) {
        case "name":     return r.name;
        case "freshness":return ["deployed","recent","archived"].indexOf(r.freshness);
        case "status":   return SCAN_STATUS_ORDER.indexOf(r.status);
        case "findings": return -SCAN_SEV(r.found);
        case "lastScan": return scanAgeMins(r.lastScan);
        default:         return 0;
      }
    };
    return [...filtered].sort((a,b) => {
      const x = val(a), y = val(b);
      if (x < y) return -dir;
      if (x > y) return dir;
      return a.name.localeCompare(b.name);
    });
  }, [filtered, sort]);

  const statuses = SCAN_STATUS_ORDER.filter(k => rows.some(r => r.status === k));
  const selectable = sorted.filter(r => r.status === "scanning" || r.status === "queued");

  return (
    <>
      <div className="scan-toolbar">
        <div className="q-search" style={{ maxWidth:250 }}>
          <Icon name="search" size={13}/>
          <input className="q-search-input" placeholder={scope==="deployed"?"Search deployed configs…":"Search all scans…"}
            value={query} onChange={e=>setQuery(e.target.value)}/>
          {q && <button className="btn-icon xs focus-ring" title="Clear search" onClick={()=>setQuery("")}><Icon name="x" size={13}/></button>}
        </div>
        <select className="input filter-select focus-ring" style={{ width:"auto" }} value={status} onChange={e=>setStatus(e.target.value)}>
          <option value="all">All statuses</option>
          {statuses.map(k => <option key={k} value={k}>{SCAN_STATUS_META[k].label}</option>)}
        </select>
        {scope !== "deployed" && (
          <select className="input filter-select focus-ring" style={{ width:"auto" }} value={fresh} onChange={e=>setFresh(e.target.value)}>
            <option value="all">All revisions</option>
            <option value="deployed">Deployed</option>
            <option value="recent">Recent</option>
            <option value="archived">Archived</option>
          </select>
        )}
        <button className={`btn btn-ghost xs focus-ring${latestOnly?" active-filter":""}`} onClick={()=>setLatestOnly(v=>!v)}
          title="Show only the most recent revision per flake">
          <Icon name="star" size={12}/> Latest per flake
        </button>
        <span className="filter-count" style={{ marginLeft:"auto" }}>{sorted.length} of {rows.length}</span>
        {selectable.length > 0 && <MultiSelectHint />}
      </div>
      {sorted.length === 0 ? (
        <div className="q-empty">
          <Icon name="search" size={20}/>
          <div>No scans match these filters.</div>
          <button className="btn btn-ghost xs focus-ring" onClick={()=>{ setQuery(""); setStatus("all"); setFresh("all"); setLatestOnly(false); }}>Reset filters</button>
        </div>
      ) : (
        <ScanTable rows={sorted} onNavigate={onNavigate} sel={sel} onOpenLog={onOpenLog} sort={sort} onSort={setSort} showFreshness={scope!=="deployed"}/>
      )}
    </>
  );
}

function ScanTable({ rows, onNavigate, sel, onOpenLog, sort, onSort, showFreshness = true }) {
  const latestIds = React.useMemo(() => (typeof latestPerFlake === "function" ? latestPerFlake(rows) : new Set()), [rows]);
  const freshChip = (f) => {
    const map = { deployed:["chip-healthy","deployed"], recent:["chip-info","recent"], archived:["chip-unknown","archived"] };
    const [cls,label] = map[f] || ["chip-unknown",f];
    return <span className={`chip ${cls}`} style={{ fontSize:10 }}>{label}</span>;
  };
  const isCancellable = (s) => s.status === "scanning" || s.status === "queued";
  const cancellableIds = sel ? rows.filter(isCancellable).map(s => s.id) : [];
  const SortTh = ({ k, children, align }) => {
    if (!onSort) return <th style={align?{textAlign:align}:undefined}>{children}</th>;
    const on = sort && sort.key === k;
    return (
      <th style={align?{textAlign:align}:undefined}>
        <button className={`th-sort focus-ring${on?" on":""}`}
          onClick={()=>onSort({ key:k, dir: on && sort.dir === "asc" ? "desc" : "asc" })}>
          {children}
          <Icon name={on && sort.dir === "desc" ? "chevron-down" : "chevron-up"} size={10}
            style={{ opacity: on ? 0.9 : 0.25 }}/>
        </button>
      </th>
    );
  };
  return (
    <table className="sys-table">
      <thead>
        <tr>
          <SortTh k="name">Config</SortTh>
          {showFreshness && <SortTh k="freshness">Revision</SortTh>}
          <SortTh k="status">Status</SortTh>
          <SortTh k="findings">Findings</SortTh>
          <SortTh k="lastScan">Last scan</SortTh>
          <th>Trigger</th>
          <th style={{ textAlign:"right" }}> </th>
        </tr>
      </thead>
      <tbody>
        {rows.map(s => {
          const meta = SCAN_STATUS_META[s.status];
          const checked = sel && sel.has(s.id);
          return (
            <tr key={s.id}
              className={`row-clickable ${sel && isCancellable(s) ? "selectable " : ""}${checked ? "row-checked" : ""}`}
              onMouseDown={sel ? (e)=>{ if(e.shiftKey) e.preventDefault(); } : undefined}
              onClick={sel ? (e)=>{ if (e.shiftKey || e.metaKey || e.ctrlKey) { sel.handleClick(e, s.id, cancellableIds); } else { onOpenLog?.(s); } } : ()=>onOpenLog?.(s)}>
              <td>
                <div style={{ fontWeight:600, fontSize:13 }}>{s.name}</div>
                <div className={`mono${latestIds.has(s.id)?" commit-latest":""}`} style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{s.flake} · {latestIds.has(s.id) && <Icon name="star" size={9} className="latest-star" style={{ marginRight:2, verticalAlign:"-1px" }}/>}{s.commit}</div>
              </td>
              {showFreshness && <td>{freshChip(s.freshness)}</td>}
              <td>
                <span className={`chip ${meta.cls}`}><span className="chip-dot" style={{ background:meta.color }}/>{meta.label}</span>
                {/* vulnix reports no progress — only that it is running. Show elapsed instead of a fake bar. */}
                {s.status==="scanning" && s.startedAgo && (
                  <div style={{ fontSize:10, color:"var(--cf-text-muted)", marginTop:3, display:"flex", alignItems:"center", gap:5 }}>
                    <span className="scan-pulse"/> running {s.startedAgo}
                  </div>
                )}
                {s.status==="awaiting" && s.awaitingDetail && (
                  <div style={{ fontSize:10, color:"var(--cf-text-muted)", marginTop:3, maxWidth:190 }}>{s.awaitingDetail}</div>
                )}
                {s.error && (
                  <button className="scan-err-link focus-ring" onClick={(e)=>{ e.stopPropagation(); onOpenLog?.(s); }} title="Open full scan log">
                    {s.error} <Icon name="arrow-right" size={9}/>
                  </button>
                )}
              </td>
              <td>
                {s.found ? (
                  <div style={{ display:"flex", gap:4 }}>
                    {s.found.crit>0 && <span className="chip chip-critical" style={{ fontSize:10 }}>{s.found.crit}C</span>}
                    {s.found.high>0 && <span className="chip chip-warning" style={{ fontSize:10 }}>{s.found.high}H</span>}
                    {s.found.med>0  && <span className="chip chip-info" style={{ fontSize:10 }}>{s.found.med}M</span>}
                    {s.found.crit===0 && s.found.high===0 && s.found.med===0 && <span className="chip chip-healthy" style={{ fontSize:10 }}><Icon name="check" size={9}/> clean</span>}
                  </div>
                ) : <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>—</span>}
              </td>
              <td style={{ fontSize:12, color:"var(--cf-text-muted)" }}>{s.lastScan}</td>
              <td>{s.trigger ? <span className="chip chip-unknown" style={{ fontSize:10 }}>{s.trigger}</span> : <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>—</span>}</td>
              <td>
                <div className="row-actions">
                  <button className="btn-icon focus-ring" title="View scan log" onClick={(e)=>{ e.stopPropagation(); onOpenLog?.(s); }}><Icon name="terminal" size={14}/></button>
                  <button className="btn-icon focus-ring" title="Rescan now" onClick={(e)=>e.stopPropagation()}><Icon name="sync" size={14}/></button>
                  {s.found && (s.found.crit>0||s.found.high>0) && (
                    <button className="btn-icon focus-ring" title="View CVEs" onClick={(e)=>{ e.stopPropagation(); onNavigate("cves"); }}><Icon name="arrow-right" size={14}/></button>
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

// Scan log drawer — full vulnix output for one config, with search. Same tray shell as
// the build/eval log drawers so "open the log" behaves identically across the app.
function ScanLogDrawer({ cfg, onClose, onNavigate }) {
  const [tab, setTab] = React.useState("log");
  const lines = React.useMemo(() => scanLogLines(cfg), [cfg.id]);
  const [query, setQuery] = React.useState("");
  const [matchIdx, setMatchIdx] = React.useState(0);
  const ref = React.useRef(null);
  const searchRef = React.useRef(null);
  const meta = SCAN_STATUS_META[cfg.status];
  const live = cfg.status === "scanning";

  const q = query.trim().toLowerCase();
  const matches = React.useMemo(() =>
    q ? lines.map((l, i) => l.m.toLowerCase().includes(q) || l.t.includes(q) ? i : -1).filter(i => i >= 0) : [],
    [q, lines]);

  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  React.useEffect(() => { if (ref.current && !q) ref.current.scrollTop = ref.current.scrollHeight; }, [tab]);
  React.useEffect(() => { setMatchIdx(0); }, [q]);

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
  const failed = cfg.status === "failed";

  return (
    <>
      <div className="fl-tray-backdrop" onClick={onClose}/>
      <aside className="fl-tray" style={{ width:"min(900px,100vw)", display:"flex", flexDirection:"column" }}>
        <div className="panel-head">
          <div className="panel-title">
            <h2><Icon name="shield" size={14} style={{ opacity:0.7 }}/> {cfg.name}</h2>
            <span className="fqdn mono">{cfg.flake} · {cfg.commit}</span>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={16}/></button>
        </div>

        <div style={{ padding:"10px 16px", borderBottom:"1px solid var(--cf-divider)", display:"flex", gap:8, alignItems:"center", flexWrap:"wrap", flexShrink:0 }}>
          <span className={`chip ${meta.cls}`}><span className="chip-dot" style={{ background:meta.color }}/>{meta.label}</span>
          {cfg.trigger && <span className="chip chip-unknown" style={{ fontSize:10 }}>{cfg.trigger}</span>}
          <span style={{ fontSize:11.5, color:"var(--cf-text-muted)" }}>{cfg.lastScan}</span>
          {cfg.found && (
            <div style={{ display:"flex", gap:4, marginLeft:"auto" }}>
              {cfg.found.crit>0 && <span className="chip chip-critical" style={{ fontSize:10 }}>{cfg.found.crit}C</span>}
              {cfg.found.high>0 && <span className="chip chip-warning" style={{ fontSize:10 }}>{cfg.found.high}H</span>}
              {cfg.found.med>0  && <span className="chip chip-info" style={{ fontSize:10 }}>{cfg.found.med}M</span>}
            </div>
          )}
        </div>

        {cfg.status === "awaiting" && (
          <div style={{ margin:"12px 16px 0", padding:"10px 12px", borderRadius:8, border:"1px solid var(--cf-card-border)", background:"var(--cf-subtle-bg)", display:"flex", gap:9, alignItems:"flex-start", flexShrink:0 }}>
            <Icon name="clock" size={14} style={{ color:"#94a3b8", flexShrink:0, marginTop:1 }}/>
            <div style={{ minWidth:0 }}>
              <div style={{ fontSize:12.5, fontWeight:600, color:"var(--cf-text-primary)" }}>
                {cfg.awaiting === "building" ? "Waiting on the build" : "Closure not available to any scanner"}
              </div>
              <div style={{ fontSize:11.5, color:"var(--cf-text-muted)", marginTop:3 }}>
                {cfg.awaitingDetail}. Scanning is async from building — vulnix needs the realised closure in the store or on a reachable substituter. This scan starts on its own once the closure lands; no action needed unless it stays here.
              </div>
              <div style={{ display:"flex", gap:6, marginTop:8 }}>
                <button className="btn btn-ghost focus-ring xs" onClick={()=>onNavigate?.(cfg.awaiting === "building" ? "builds" : "caches", cfg.awaiting === "building" ? { sha: cfg.commit, flake: cfg.flake } : undefined)}>
                  <Icon name={cfg.awaiting === "building" ? "build" : "cache"} size={11}/> {cfg.awaiting === "building" ? "Go to build" : "Check caches"}
                </button>
                <button className="btn btn-ghost focus-ring xs"><Icon name="sync" size={11}/> Check now</button>
              </div>
            </div>
          </div>
        )}
        {failed && (
          <div style={{ margin:"12px 16px 0", padding:"10px 12px", borderRadius:8, border:"1px solid rgba(248,113,113,0.35)", background:"rgba(248,113,113,0.07)", display:"flex", gap:9, alignItems:"flex-start", flexShrink:0 }}>
            <Icon name="warn" size={14} style={{ color:"#f87171", flexShrink:0, marginTop:1 }}/>
            <div style={{ minWidth:0 }}>
              <div style={{ fontSize:12.5, fontWeight:600, color:"#fca5a5" }}>{cfg.error}</div>
              <div style={{ fontSize:11.5, color:"var(--cf-text-muted)", marginTop:3 }}>
                vulnix needs the config's derivation closure in the store. Build this config (or fetch it from a cache) and rescan.
              </div>
              <div style={{ display:"flex", gap:6, marginTop:8 }}>
                <button className="btn btn-ghost focus-ring xs" onClick={()=>onNavigate?.("builds", { sha: cfg.commit, flake: cfg.flake })}><Icon name="build" size={11}/> Go to build</button>
                <button className="btn btn-ghost focus-ring xs"><Icon name="sync" size={11}/> Retry scan</button>
              </div>
            </div>
          </div>
        )}

        <div className="sd-tabs" style={{ padding:"0 16px", borderBottom:"1px solid var(--cf-card-border)", flexShrink:0, marginTop:12 }}>
          {[{ k:"log", l:"Log" }, { k:"details", l:"Details" }].map(t => (
            <button key={t.k} className={`sd-tab focus-ring${tab===t.k?" active":""}`} onClick={()=>setTab(t.k)}>{t.l}</button>
          ))}
        </div>

        {tab === "log" ? (
          <div style={{ display:"flex", flexDirection:"column", flex:1, minHeight:0 }}>
            <div style={{ padding:"8px 16px", borderBottom:"1px solid var(--cf-divider)", display:"flex", gap:10, alignItems:"center", flexShrink:0 }}>
              <span style={{ fontSize:11, color:"var(--cf-text-muted)", whiteSpace:"nowrap" }}>
                {q ? `${matches.length} ${matches.length===1?"match":"matches"}` : `${lines.length} lines`}
              </span>
              <div style={{ flex:1 }}/>
              <div className="log-search">
                <Icon name="search" size={13}/>
                <input ref={searchRef} className="log-search-input" placeholder="Search log…" value={query} onChange={e=>setQuery(e.target.value)}/>
                {q && <span className="log-search-count">{matches.length ? `${Math.min(matchIdx,matches.length-1)+1}/${matches.length}` : "0"}</span>}
                {q && (
                  <>
                    <button className="btn-icon xs focus-ring" title="Previous match" disabled={!matches.length}
                      onClick={()=>setMatchIdx(i=>(i-1+matches.length)%matches.length)}><Icon name="chevron-up" size={13}/></button>
                    <button className="btn-icon xs focus-ring" title="Next match" disabled={!matches.length}
                      onClick={()=>setMatchIdx(i=>(i+1)%matches.length)}><Icon name="chevron-down" size={13}/></button>
                  </>
                )}
              </div>
              <button className="btn-icon focus-ring" title="Download log"
                onClick={()=>downloadFile(`${cfg.name}-${cfg.commit}-scan.log`, lines.map(l=>`${l.t} ${l.lvl.toUpperCase()} ${l.m}`).join("\n"), "text/plain")}>
                <Icon name="download" size={13}/>
              </button>
            </div>
            <pre ref={ref} className="sd-log-stream build-log-stream">
              {lines.map((l,i) => (
                <div key={i} data-li={i} className={`sd-log-line sd-log-${l.lvl}${matches.includes(i) ? " log-line-hit" : ""}${i===activeLine ? " log-line-active" : ""}`}>
                  <span className="sd-log-t">{l.t}</span>
                  <span className="sd-log-lvl">{l.lvl.toUpperCase()}</span>
                  <span className="sd-log-m">{renderMsg(l.m)}</span>
                </div>
              ))}
              {!q && live && <div className="sd-log-caret">▍</div>}
            </pre>
          </div>
        ) : (
          <div style={{ padding:"14px 16px", overflowY:"auto", flex:1 }}>
            <dl className="kv-grid">
              <dt>Config</dt><dd className="mono">{cfg.name}</dd>
              <dt>Flake</dt><dd className="mono">{cfg.flake}</dd>
              <dt>Commit</dt><dd className="mono">{cfg.commit}</dd>
              <dt>Freshness</dt><dd>{cfg.freshness}</dd>
              <dt>Status</dt><dd>{meta.label}</dd>
              <dt>Trigger</dt><dd>{cfg.trigger || "—"}</dd>
              <dt>Last scan</dt><dd>{cfg.lastScan}</dd>
              <dt>Scanner</dt><dd className="mono">vulnix {SCAN_POLICY.vulnixVersion}</dd>
              <dt>CVE database</dt><dd>updated {SCAN_POLICY.dbAge}</dd>
            </dl>
            {cfg.found && (
              <div style={{ marginTop:18 }}>
                <h3 style={{ fontSize:12, fontWeight:600, margin:"0 0 8px", color:"var(--cf-text-secondary)" }}>Findings</h3>
                <div style={{ display:"flex", gap:6 }}>
                  <span className="chip chip-critical">{cfg.found.crit} critical</span>
                  <span className="chip chip-warning">{cfg.found.high} high</span>
                  <span className="chip chip-info">{cfg.found.med} medium</span>
                </div>
                {(cfg.found.crit>0 || cfg.found.high>0) && (
                  <button className="btn btn-ghost focus-ring xs" style={{ marginTop:10 }} onClick={()=>onNavigate?.("cves")}>
                    <Icon name="arrow-right" size={11}/> View these CVEs
                  </button>
                )}
              </div>
            )}
          </div>
        )}
      </aside>
    </>
  );
}

function ScanAllConfigs({ onNavigate, onOpenLog }) {
  const [query, setQuery] = React.useState("");
  const [envFilter, setEnvFilter] = React.useState("all");
  const [expanded, setExpanded] = React.useState(null);

  const rows = SCAN_HISTORY.filter(s =>
    (envFilter === "all" || s.environment === envFilter) &&
    (!query || s.hostname.toLowerCase().includes(query.toLowerCase()) || s.flake.toLowerCase().includes(query.toLowerCase()))
  ).sort((a,b) => b.totalConfigs - a.totalConfigs);

  const freshChip = (f) => {
    const map = { deployed:["chip-healthy","deployed"], recent:["chip-info","recent"], archived:["chip-unknown","archived"] };
    const [cls,label] = map[f] || ["chip-unknown",f];
    return <span className={`chip ${cls}`} style={{ fontSize:10 }}>{label}</span>;
  };

  return (
    <>
      <div className="scan-toolbar">
        <div className="q-search" style={{ maxWidth:250 }}>
          <Icon name="search" size={13}/>
          <input className="q-search-input" placeholder="Search systems…" value={query} onChange={e=>setQuery(e.target.value)}/>
          {query && <button className="btn-icon xs focus-ring" title="Clear search" onClick={()=>setQuery("")}><Icon name="x" size={13}/></button>}
        </div>
        <select className="input filter-select focus-ring" style={{ width:"auto" }} value={envFilter} onChange={e=>setEnvFilter(e.target.value)}>
          <option value="all">All environments</option>
          {ENVIRONMENTS.map(e => <option key={e.name} value={e.name}>{e.name}</option>)}
        </select>
        <span className="filter-count">{rows.length} systems · {rows.reduce((a,s)=>a+s.totalConfigs,0)} configs</span>
      </div>
      <table className="sys-table">
        <thead>
          <tr>
            <th>System</th>
            <th>Env</th>
            <th>Configs</th>
            <th title="Share of this system's configs that have a fresh scan (green), a stale scan past the rescan interval (amber), or were never scanned (gray)">Scan freshness</th>
            <th>Current findings</th>
            <th style={{ textAlign:"right" }}> </th>
          </tr>
        </thead>
        <tbody>
          {rows.map(s => {
            const isOpen = expanded === s.id;
            const covPct = Math.round(s.scanned / s.totalConfigs * 100);
            return (
              <React.Fragment key={s.id}>
                <tr style={{ cursor:"pointer" }} onClick={()=>setExpanded(isOpen?null:s.id)}>
                  <td>
                    <div style={{ display:"flex", alignItems:"center", gap:8 }}>
                      <Icon name={isOpen?"chevron-down":"chevron-right"} size={12} style={{ color:"var(--cf-text-muted)", flexShrink:0 }}/>
                      <span className="status-dot" style={{ "--status-color": s.statusColor }}/>
                      <div>
                        <div style={{ fontWeight:600, fontSize:13 }}>{s.hostname}</div>
                        <div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{s.flake}</div>
                      </div>
                    </div>
                  </td>
                  <td><EnvBadge env={s.environment}/></td>
                  <td className="mono" style={{ fontSize:12 }}>{s.totalConfigs}</td>
                  <td>
                    <div style={{ display:"flex", alignItems:"center", gap:8, minWidth:120 }} title={`${s.scanned} fresh · ${s.stale} stale · ${s.needsBuild} need build · ${s.unscanned} never scanned`}>
                      <div style={{ flex:1, height:5, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden", display:"flex" }}>
                        <div style={{ width:`${(s.scanned/s.totalConfigs)*100}%`, background:"#34d399" }}/>
                        <div style={{ width:`${(s.stale/s.totalConfigs)*100}%`, background:"#fbbf24" }}/>
                        <div style={{ width:`${(s.needsBuild/s.totalConfigs)*100}%`, background:"#f59e0b" }}/>
                        <div style={{ width:`${(s.unscanned/s.totalConfigs)*100}%`, background:"#4b5563" }}/>
                      </div>
                      <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{s.scanned}/{s.totalConfigs}</span>
                    </div>
                    <div style={{ fontSize:10, color:"var(--cf-text-muted)", marginTop:3, display:"flex", gap:8, flexWrap:"wrap" }}>
                      <span style={{ color:"#34d399" }}>{s.scanned} fresh</span>
                      {s.stale>0 && <span style={{ color:"#fbbf24" }}>{s.stale} stale</span>}
                      {s.needsBuild>0 && <span style={{ color:"#f59e0b" }}>{s.needsBuild} need build</span>}
                      {s.unscanned>0 && <span>{s.unscanned} never</span>}
                    </div>
                  </td>
                  <td>
                    {s.currentCrit>0 || s.currentHigh>0 ? (
                      <div style={{ display:"flex", gap:4 }}>
                        {s.currentCrit>0 && <span className="chip chip-critical" style={{ fontSize:10 }}>{s.currentCrit}C</span>}
                        {s.currentHigh>0 && <span className="chip chip-warning" style={{ fontSize:10 }}>{s.currentHigh}H</span>}
                      </div>
                    ) : <span className="chip chip-healthy" style={{ fontSize:10 }}><Icon name="check" size={9}/> clean</span>}
                  </td>
                  <td>
                    <div className="row-actions">
                      <button className="btn-icon focus-ring" title="Rescan current" onClick={e=>e.stopPropagation()}><Icon name="sync" size={14}/></button>
                    </div>
                  </td>
                </tr>
                {isOpen && (
                  <tr className="scan-sys-expand-row">
                    <td colSpan={6} style={{ padding:0 }}>
                      <div className="scan-sys-expand">
                        <div className="scan-sys-expand-head">
                          <span>
                            {s.commits.length} config{s.commits.length===1?"":"s"} for this system{s.commits.length>8 ? " · newest first" : ""}
                          </span>
                          <button className="btn btn-ghost focus-ring xs"><Icon name="sync" size={10}/> Rescan all</button>
                        </div>
                        <div className="scan-sys-expand-table-wrap" style={{ maxHeight: s.commits.length > 8 ? 300 : "none", overflowY: s.commits.length > 8 ? "auto" : "visible" }}>
                        <table style={{ width:"100%", borderCollapse:"collapse", fontSize:12 }}>
                          <thead style={{ position:"sticky", top:0, zIndex:1 }}>
                            <tr style={{ color:"var(--cf-text-muted)", fontSize:10, textTransform:"uppercase", letterSpacing:"0.06em", background:"var(--cf-card-bg)" }}>
                              <th style={{ textAlign:"left", padding:"6px 8px", fontWeight:600 }}>Commit</th>
                              <th style={{ textAlign:"left", padding:"6px 8px", fontWeight:600 }}>Freshness</th>
                              <th style={{ textAlign:"left", padding:"6px 8px", fontWeight:600 }}>Status</th>
                              <th style={{ textAlign:"left", padding:"6px 8px", fontWeight:600 }}>Findings</th>
                              <th style={{ textAlign:"left", padding:"6px 8px", fontWeight:600 }}>Last scan</th>
                              <th style={{ textAlign:"right", padding:"6px 8px" }}></th>
                            </tr>
                          </thead>
                          <tbody>
                            {s.commits.map((c, i) => {
                              const meta = SCAN_STATUS_META[c.status];
                              const openable = c.status !== "needs-build" && c.status !== "unscanned";
                              const openLog = () => onOpenLog && onOpenLog({
                                id: `${s.id}-${c.commit}-${i}`, name: s.hostname, flake: s.flake, commit: c.commit,
                                status: c.status === "complete" ? "complete" : c.status, found: c.found,
                                lastScan: c.lastScan, trigger: c.trigger, freshness: c.freshness,
                                error: c.status === "failed" ? "vulnix: derivation not available" : undefined,
                              });
                              return (
                                <tr key={i} className={`scan-sys-commit-row${openable?"":" no-log"}`}
                                  style={{ borderTop:"1px solid var(--cf-divider)" }}
                                  onClick={openable ? openLog : undefined} title={openable ? "Open scan log" : undefined}>
                                  <td style={{ padding:"7px 8px" }}>
                                    <span className="mono" style={{ fontWeight:600 }}>{c.commit}</span>
                                    {c.current && <span className="chip chip-info" style={{ fontSize:9, marginLeft:6 }}>current</span>}
                                    <div style={{ fontSize:10, color:"var(--cf-text-muted)" }}>{c.msg}</div>
                                  </td>
                                  <td style={{ padding:"7px 8px" }}>{freshChip(c.freshness)}</td>
                                  <td style={{ padding:"7px 8px" }}>
                                    <span className={`chip ${meta.cls}`} style={{ fontSize:10 }}><span className="chip-dot" style={{ background:meta.color }}/>{meta.label}</span>
                                  </td>
                                  <td style={{ padding:"7px 8px" }}>
                                    {c.found ? (
                                      <div style={{ display:"flex", gap:4 }}>
                                        {c.found.crit>0 && <span className="chip chip-critical" style={{ fontSize:10 }}>{c.found.crit}C</span>}
                                        {c.found.high>0 && <span className="chip chip-warning" style={{ fontSize:10 }}>{c.found.high}H</span>}
                                        {c.found.med>0  && <span className="chip chip-info" style={{ fontSize:10 }}>{c.found.med}M</span>}
                                        {c.found.crit===0 && c.found.high===0 && c.found.med===0 && <span className="chip chip-healthy" style={{ fontSize:10 }}>clean</span>}
                                      </div>
                                    ) : <span style={{ color:"var(--cf-text-muted)" }}>—</span>}
                                  </td>
                                  <td style={{ padding:"7px 8px", color:"var(--cf-text-muted)" }}>{c.lastScan}</td>
                                  <td style={{ padding:"7px 8px", textAlign:"right" }}>
                                    {c.status === "needs-build"
                                      ? <button className="btn btn-ghost focus-ring xs" title="Not in cache — build first, then scan" onClick={e=>e.stopPropagation()}><Icon name="build" size={11}/> Build & scan</button>
                                      : openable
                                      ? <button className="btn-icon focus-ring" title="Open scan log" onClick={e=>{ e.stopPropagation(); openLog(); }}><Icon name="terminal" size={13}/></button>
                                      : <button className="btn-icon focus-ring" title="Rescan this config" onClick={e=>e.stopPropagation()}><Icon name="sync" size={13}/></button>}
                                  </td>
                                </tr>
                              );
                            })}
                          </tbody>
                        </table>
                        </div>
                      </div>
                    </td>
                  </tr>
                )}
              </React.Fragment>
            );
          })}
        </tbody>
      </table>
    </>
  );
}

function ScanScheduleModal({ onClose }) {
  const [form, setForm] = React.useState({ ...SCAN_POLICY });
  const set = (k,v) => setForm(p => ({ ...p, [k]: v }));
  const IntervalSelect = ({ value, onChange, disabled }) => (
    <select className="input focus-ring" value={value} onChange={e=>onChange(e.target.value)} disabled={disabled} style={{ width:120 }}>
      {SCAN_INTERVALS.map(i => <option key={i} value={i}>{i === "never" ? "Never" : `Every ${i}`}</option>)}
    </select>
  );
  const Row = ({ title, desc, children }) => (
    <div style={{ display:"flex", alignItems:"flex-start", justifyContent:"space-between", gap:16, padding:"12px 0", borderBottom:"1px solid var(--cf-divider)" }}>
      <div style={{ minWidth:0 }}>
        <div style={{ fontSize:13, fontWeight:600 }}>{title}</div>
        <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2, lineHeight:1.5 }}>{desc}</div>
      </div>
      <div style={{ flexShrink:0 }}>{children}</div>
    </div>
  );
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(620px,96vw)" }}>
        <div className="modal-head">
          <h2><Icon name="gear" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/> Scan schedule</h2>
          <p>Control how often vulnix rescans configurations. New & deployed configs scan most often; old ones least.</p>
        </div>
        <div className="modal-body">
          <Row title="Scan on build" desc="Scan a freshly-built config before it can be deployed. Strongly recommended — the derivation is already in the store, so no extra build is needed.">
            <label style={{ display:"flex", gap:8, alignItems:"center", fontSize:13, cursor:"pointer" }}>
              <input type="checkbox" checked={form.onBuild} onChange={e=>set("onBuild",e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
              <span>{form.onBuild ? "On" : "Off"}</span>
            </label>
          </Row>
          <Row title="Deployed configs" desc="Currently running on at least one system. Rescanned to catch newly-published advisories.">
            <IntervalSelect value={form.deployedInterval} onChange={v=>set("deployedInterval",v)}/>
          </Row>
          <Row title="Recent configs" desc="Built in the last 30 days but not currently deployed.">
            <IntervalSelect value={form.recentInterval} onChange={v=>set("recentInterval",v)}/>
          </Row>
          <Row title="Archived configs" desc="Old / superseded configs no longer in rotation. Scan rarely (or never) to save builder time.">
            <div style={{ display:"flex", alignItems:"center", gap:8 }}>
              <input type="checkbox" checked={form.archivedEnabled} onChange={e=>set("archivedEnabled",e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
              <IntervalSelect value={form.archivedInterval} onChange={v=>set("archivedInterval",v)} disabled={!form.archivedEnabled}/>
            </div>
          </Row>
          <Row title="Rebuild to scan old configs" desc="vulnix needs a realised derivation. Archived configs evicted from cache must be rebuilt before they can be scanned — this can be expensive. Off = skip uncached configs instead of building them.">
            <label style={{ display:"flex", gap:8, alignItems:"center", fontSize:13, cursor:"pointer" }}>
              <input type="checkbox" checked={form.rebuildToScan ?? false} onChange={e=>set("rebuildToScan",e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
              <span>{form.rebuildToScan ? "On" : "Off"}</span>
            </label>
          </Row>
          <div className="sd-callout sd-callout-info" style={{ fontSize:11, marginTop:12 }}>
            <Icon name="shield" size={12}/>
            <div>Estimated load: ~{form.onBuild ? "every build" : "no"} build scans + periodic rescans. Deployed configs at <strong>{form.deployedInterval}</strong> dominate builder cost.</div>
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" onClick={onClose}><Icon name="check" size={13}/> Save schedule</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { ScanningView });
