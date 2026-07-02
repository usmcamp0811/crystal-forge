// Scanning view — CVE scan pipeline status + schedule config

function ScanningView({ onNavigate }) {
  const [tab, setTab] = React.useState("queue");
  const [configOpen, setConfigOpen] = React.useState(false);
  const [showActivity, setShowActivity] = React.useState(() => {
    try { return localStorage.getItem("cf-scan-activity") !== "0"; } catch { return true; }
  });
  const scanSel = useMultiSelect(tab);
  const toggleActivity = () => setShowActivity(v => {
    const n = !v;
    try { localStorage.setItem("cf-scan-activity", n ? "1" : "0"); } catch {}
    return n;
  });

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
          <div className="stat-meta">{SCAN_STATS.queued} queued</div>
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
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color":"#f87171" }}/>
          <div className="stat-label">Failed</div>
          <div className="stat-value" style={{ color: SCAN_STATS.failed>0?"#f87171":"#34d399" }}>{SCAN_STATS.failed}</div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color":"#34d399" }}/>
          <div className="stat-label">Coverage</div>
          <div className="stat-value" style={{ color:"#34d399" }}>{SCAN_STATS.coverage}%</div>
          <div className="stat-meta">configs with results</div>
        </div>
      </div>

      <div style={{ display:"grid", gridTemplateColumns: showActivity ? "1fr 320px" : "1fr", gap:14, alignItems:"start" }}>
        {/* Scan queue */}
        <div className="card" style={{ overflow:"hidden" }}>
          <div className="sd-tabs" style={{ padding:"0 16px", borderBottom:"1px solid var(--cf-card-border)", display:"flex", alignItems:"center" }}>
            {[
              { k:"queue", l:"Active & Recent" },
              { k:"all",   l:"All configs" },
            ].map(t => (
              <button key={t.k} className={`sd-tab focus-ring${tab===t.k?" active":""}`} onClick={()=>setTab(t.k)}>{t.l}</button>
            ))}
            {tab==="queue" && SCAN_CONFIGS.some(s=>s.status==="scanning"||s.status==="queued") && <MultiSelectHint />}
            {!showActivity && (
              <button className="btn btn-ghost focus-ring xs" style={{ marginLeft:"auto" }} onClick={toggleActivity} title="Show scan activity">
                <Icon name="history" size={11}/> Activity
              </button>
            )}
          </div>
          {tab === "queue"
            ? <ScanTable rows={SCAN_CONFIGS.filter(s=>s.status!=="unscanned" || s.freshness!=="archived")} onNavigate={onNavigate} sel={scanSel}/>
            : <ScanAllConfigs onNavigate={onNavigate}/>}
        </div>

        {/* Activity feed */}
        {showActivity && (
        <div className="card" style={{ padding:16 }}>
          <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", marginBottom:12 }}>
            <h3 style={{ margin:0, fontSize:13, fontWeight:600 }}>Scan activity</h3>
            <button className="btn-icon focus-ring" onClick={toggleActivity} title="Hide panel"><Icon name="x" size={14}/></button>
          </div>
          <div className="dash-w-body" style={{ gap:0 }}>
            {SCAN_ACTIVITY.map((a, i) => (
              <div key={i} style={{ display:"flex", gap:10, paddingLeft:2 }}>
                <div style={{ display:"flex", flexDirection:"column", alignItems:"center", paddingTop:4, flexShrink:0 }}>
                  <div style={{ width:22, height:22, borderRadius:6, background:`color-mix(in oklab, ${a.color} 18%, transparent)`, color:a.color, display:"grid", placeItems:"center" }}>
                    <Icon name={a.icon} size={11}/>
                  </div>
                  {i < SCAN_ACTIVITY.length-1 && <div style={{ width:2, flex:1, background:"var(--cf-divider)", minHeight:16 }}/>}
                </div>
                <div style={{ paddingTop:3, paddingBottom:i===SCAN_ACTIVITY.length-1?0:14, minWidth:0 }}>
                  <div style={{ fontSize:12, color:"var(--cf-text-primary)", display:"flex", gap:6, justifyContent:"space-between" }}>
                    <span style={{ fontWeight:600 }}>{a.event}</span>
                    <span style={{ fontSize:11, color:"var(--cf-text-muted)", whiteSpace:"nowrap" }}>{a.at}</span>
                  </div>
                  <div className="mono" style={{ fontSize:11, color:"var(--cf-brand-purple)" }}>{a.name}</div>
                  <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>{a.detail}</div>
                </div>
              </div>
            ))}
          </div>
        </div>
        )}
      </div>

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

function ScanTable({ rows, onNavigate, sel }) {
  const freshChip = (f) => {
    const map = { deployed:["chip-healthy","deployed"], recent:["chip-info","recent"], archived:["chip-unknown","archived"] };
    const [cls,label] = map[f] || ["chip-unknown",f];
    return <span className={`chip ${cls}`} style={{ fontSize:10 }}>{label}</span>;
  };
  const isCancellable = (s) => s.status === "scanning" || s.status === "queued";
  const cancellableIds = sel ? rows.filter(isCancellable).map(s => s.id) : [];
  return (
    <table className="sys-table">
      <thead>
        <tr>
          <th>Config</th>
          <th>Freshness</th>
          <th>Status</th>
          <th>Findings</th>
          <th>Last scan</th>
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
              className={`${sel && isCancellable(s) ? "selectable " : ""}${checked ? "row-checked" : ""}`}
              onMouseDown={sel ? (e)=>{ if(e.shiftKey) e.preventDefault(); } : undefined}
              onClick={sel ? (e)=>{ sel.handleClick(e, s.id, cancellableIds); } : undefined}>
              <td>
                <div style={{ fontWeight:600, fontSize:13 }}>{s.name}</div>
                <div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{s.flake} · {s.commit}</div>
              </td>
              <td>{freshChip(s.freshness)}</td>
              <td>
                <span className={`chip ${meta.cls}`}><span className="chip-dot" style={{ background:meta.color }}/>{meta.label}</span>
                {s.status==="scanning" && s.progress != null && (
                  <ProgressBar value={s.progress} height={3} color="#60a5fa" style={{ marginTop:5, maxWidth:80 }} />
                )}
                {s.error && <div style={{ fontSize:10, color:"#fca5a5", marginTop:3 }}>{s.error}</div>}
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
                  <button className="btn-icon focus-ring" title="Rescan now"><Icon name="sync" size={14}/></button>
                  {s.found && (s.found.crit>0||s.found.high>0) && (
                    <button className="btn-icon focus-ring" title="View CVEs" onClick={()=>onNavigate("cves")}><Icon name="arrow-right" size={14}/></button>
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

function ScanAllConfigs({ onNavigate }) {
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
      <div style={{ padding:"10px 16px", borderBottom:"1px solid var(--cf-divider)", display:"flex", gap:10, alignItems:"center", flexWrap:"wrap" }}>
        <div className="filter-search" style={{ maxWidth:240 }}>
          <Icon name="search"/>
          <input className="input focus-ring" placeholder="Search systems…" value={query} onChange={e=>setQuery(e.target.value)}/>
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
                  <tr>
                    <td colSpan={6} style={{ padding:0, background:"color-mix(in oklab, var(--cf-brand-purple) 4%, var(--cf-page-bg))" }}>
                      <div style={{ padding:"6px 16px 10px 40px" }}>
                        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"center", padding:"4px 8px" }}>
                          <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>
                            {s.commits.length} config{s.commits.length===1?"":"s"} for this system{s.commits.length>8 ? " · newest first" : ""}
                          </span>
                          <button className="btn btn-ghost focus-ring xs"><Icon name="sync" size={10}/> Rescan all</button>
                        </div>
                        <div style={{ maxHeight: s.commits.length > 8 ? 300 : "none", overflowY: s.commits.length > 8 ? "auto" : "visible", border:"1px solid var(--cf-divider)", borderRadius:8 }}>
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
                              return (
                                <tr key={i} style={{ borderTop:"1px solid var(--cf-divider)" }}>
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
                                      ? <button className="btn btn-ghost focus-ring xs" title="Not in cache — build first, then scan"><Icon name="build" size={11}/> Build & scan</button>
                                      : <button className="btn-icon focus-ring" title="Rescan this config"><Icon name="sync" size={13}/></button>}
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
