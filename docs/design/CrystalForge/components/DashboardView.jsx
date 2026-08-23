// Dashboard view — customizable widget grid with drag-and-drop

function DashboardView({ onNavigate }) {
  // Layout persists in localStorage for now (in real app -> server-side per-user prefs)
  const [layout, setLayout] = React.useState(() => {
    try {
      const saved = localStorage.getItem("cf-dashboard-layout");
      if (saved) {
        const parsed = JSON.parse(saved);
        if (!parsed.find(w => w.id === "attestationTrust")) parsed.splice(1, 0, { id:"attestationTrust", cols:1 }, { id:"deployApprovals", cols:1 });
        if (!parsed.find(w => w.id === "poamSummary")) parsed.splice(1, 0, { id:"poamSummary", cols:1 });
        if (!parsed.find(w => w.id === "poamWatchlist")) parsed.push({ id:"poamWatchlist", cols:2 });
        return parsed;
      }
    } catch {}
    return DEFAULT_DASHBOARD_LAYOUT;
  });
  const [editMode, setEditMode] = React.useState(false);
  const [pickerOpen, setPickerOpen] = React.useState(false);
  const [dragIdx, setDragIdx] = React.useState(null);
  const [overIdx, setOverIdx] = React.useState(null);

  const persist = (next) => {
    setLayout(next);
    try { localStorage.setItem("cf-dashboard-layout", JSON.stringify(next)); } catch {}
  };

  const addWidget = (id) => {
    if (layout.find(w => w.id === id)) return;
    const def = DASHBOARD_WIDGETS[id];
    persist([...layout, { id, cols: def.defaultCols, rows: def.defaultRows || 1 }]);
  };
  const removeWidget = (id) => persist(layout.filter(w => w.id !== id));
  const setCols = (id, cols) => persist(layout.map(w => w.id === id ? { ...w, cols } : w));
  const setRows = (id, rows) => persist(layout.map(w => w.id === id ? { ...w, rows } : w));
  const reorder = (from, to) => {
    if (from === to || from == null || to == null) return;
    const next = [...layout];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    persist(next);
  };
  const resetLayout = () => {
    localStorage.removeItem("cf-dashboard-layout");
    setLayout(DEFAULT_DASHBOARD_LAYOUT);
  };

  const available = Object.values(DASHBOARD_WIDGETS).filter(w => !layout.find(l => l.id === w.id));

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">Dashboard</h1>
          <p className="page-subtitle">{layout.length} widgets · drag to rearrange in edit mode</p>
        </div>
        <div style={{ display:"flex", gap:8 }}>
          {editMode && (
            <>
              <button className="btn btn-ghost focus-ring" onClick={resetLayout} title="Reset to default layout">
                <Icon name="sync" size={14}/> Reset
              </button>
              <button className="btn btn-ghost focus-ring" onClick={() => setPickerOpen(true)}>
                <Icon name="plus" size={14}/> Add widget
              </button>
            </>
          )}
          <button className={`btn focus-ring ${editMode ? "btn-primary" : "btn-ghost"}`} onClick={() => setEditMode(!editMode)}>
            <Icon name={editMode ? "check" : "tweaks"} size={14}/> {editMode ? "Done" : "Customize"}
          </button>
        </div>
      </div>

      {editMode && (
        <div className="sd-callout sd-callout-info">
          <Icon name="tweaks" size={13}/>
          <div style={{ fontSize:12 }}>
            <strong>Edit mode.</strong> Drag widgets to reorder, set <strong>Width</strong> and (on list widgets) <strong>Height</strong>, or remove with the × button. Click "Add widget" to browse the widget library.
          </div>
        </div>
      )}

      {/* Grid */}
      <div className="dash-grid">
        {layout.map((w, idx) => {
          const def = DASHBOARD_WIDGETS[w.id];
          if (!def) return null;
          return (
            <div key={w.id}
              className={`dash-widget dash-cols-${w.cols} dash-rows-${w.rows || 1}${editMode ? " edit" : ""}${dragIdx === idx ? " dragging" : ""}${overIdx === idx && dragIdx !== idx ? " over" : ""}`}
              draggable={editMode}
              onDragStart={(e) => {
                if (!editMode) return;
                setDragIdx(idx);
                e.dataTransfer.effectAllowed = "move";
                e.dataTransfer.setData("text/plain", String(idx));
              }}
              onDragOver={(e) => {
                if (!editMode || dragIdx === null) return;
                e.preventDefault();
                e.dataTransfer.dropEffect = "move";
                if (overIdx !== idx) setOverIdx(idx);
              }}
              onDragLeave={() => { if (overIdx === idx) setOverIdx(null); }}
              onDrop={(e) => {
                if (!editMode) return;
                e.preventDefault();
                reorder(dragIdx, idx);
                setDragIdx(null); setOverIdx(null);
              }}
              onDragEnd={() => { setDragIdx(null); setOverIdx(null); }}
            >
              {editMode && (
                <div className="dash-widget-edit">
                  <span className="dash-widget-grip" title="Drag to move"><Icon name="more" size={12}/></span>
                  <span className="dash-size-group">
                    <span className="dash-col-label">Width</span>
                    <div className="seg dash-col-seg">
                      {[1, 2, 3].map(c => (
                        <button key={c} className={w.cols === c ? "active" : ""} onClick={() => setCols(w.id, c)} title={`Span ${c} of 3 columns`} aria-label={`Span ${c} of 3 columns`}>
                          <span className="dash-wglyph">
                            {[0, 1, 2].map(i => <span key={i} className={`dash-wcell${i < c ? " on" : ""}`}/>)}
                          </span>
                        </button>
                      ))}
                    </div>
                  </span>
                  {HEIGHT_RESIZABLE.has(w.id) ? (
                    <span className="dash-size-group">
                      <span className="dash-col-label">Height</span>
                      <div className="seg dash-col-seg">
                        {[1, 2, 3].map(r => (
                          <button key={r} className={(w.rows || 1) === r ? "active" : ""} onClick={() => setRows(w.id, r)} title={r === 1 ? "Show fewer items" : `Show more items (${HEIGHT_COUNTS[r]})`} aria-label={`Height level ${r}`}>
                            <span className="dash-hglyph">
                              {[0, 1, 2].map(i => <span key={i} className={`dash-hcell${i >= 3 - r ? " on" : ""}`}/>)}
                            </span>
                          </button>
                        ))}
                      </div>
                    </span>
                  ) : (
                    <span className="dash-col-label dash-fixed-h" title="This widget sizes to its content">Fixed height</span>
                  )}
                  <button className="btn-icon focus-ring dash-widget-remove" onClick={() => removeWidget(w.id)} title="Remove">
                    <Icon name="x" size={13}/>
                  </button>
                </div>
              )}
              <Widget id={w.id} editMode={editMode} onNavigate={onNavigate} rows={w.rows || 1}/>
            </div>
          );
        })}
        {layout.length === 0 && (
          <div className="empty" style={{ gridColumn:"1 / -1" }}>
            <h3>Empty dashboard</h3>
            <div>Click <strong>Customize</strong> then <strong>Add widget</strong> to get started.</div>
          </div>
        )}
      </div>

      {pickerOpen && (
        <WidgetPicker
          addedIds={new Set(layout.map(l => l.id))}
          onAdd={addWidget}
          onClose={() => setPickerOpen(false)}/>
      )}
    </div>
  );
}

/* ── Widget switchboard ── */
// Widgets whose content is a list/feed — extra height shows more rows.
// Everything else (stat rollups, the self-sizing git graph) stays fit-to-content.
const HEIGHT_RESIZABLE = new Set([
  "deploymentTimeline", "recentCommits", "topAffected", "poamWatchlist",
]);
// rows setting → how many list items the widget shows.
const HEIGHT_COUNTS = { 1: 4, 2: 8, 3: 13 };
function Widget({ id, editMode, onNavigate, rows }) {
  switch (id) {
    case "fleetHealth":     return <WFleetHealth onNavigate={onNavigate}/>;
    case "heartbeatStatus": return <WHeartbeat onNavigate={onNavigate}/>;
    case "buildQueue":      return <WBuildQueue onNavigate={onNavigate}/>;
    case "evalQueue":       return <WEvalQueue onNavigate={onNavigate}/>;
    case "cveSummary":      return <WCveSummary onNavigate={onNavigate}/>;
    case "topAffected":     return <WTopAffected onNavigate={onNavigate} rows={rows}/>;
    case "recentCommits":   return <WRecentCommits onNavigate={onNavigate} rows={rows}/>;
    case "deploymentTimeline": return <WDeploymentTimeline onNavigate={onNavigate} rows={rows}/>;
    case "gitGraph":        return <WGitGraph onNavigate={onNavigate}/>;
    case "cacheHealth":     return <WCacheHealth onNavigate={onNavigate}/>;
    case "envBreakdown":    return <WEnvBreakdown onNavigate={onNavigate}/>;
    case "quickActions":    return <WQuickActions onNavigate={onNavigate}/>;
    case "deployApprovals": return <WDeployApprovals onNavigate={onNavigate}/>;
    case "attestationTrust": return <WAttestationTrust onNavigate={onNavigate}/>;
    case "poamSummary":      return <WPoamSummary onNavigate={onNavigate}/>;
    case "poamWatchlist":    return <WPoamWatchlist onNavigate={onNavigate} rows={rows}/>;
    default: return <div style={{ padding:14 }}>Unknown widget</div>;
  }
}

function WidgetHeader({ icon, title, action, onAction }) {
  return (
    <div className="dash-w-head">
      <div style={{ display:"flex", alignItems:"center", gap:8, minWidth:0 }}>
        <Icon name={icon} size={13} style={{ color:"var(--cf-text-muted)", flexShrink:0 }}/>
        <h3 className="dash-w-title">{title}</h3>
      </div>
      {action && (
        <button className="btn btn-ghost focus-ring xs" onClick={onAction}>{action}</button>
      )}
    </div>
  );
}

/* ── Individual widgets ── */
function WDeployApprovals({ onNavigate }) {
  const queue = typeof APPROVAL_QUEUE !== "undefined" ? APPROVAL_QUEUE : [];
  const pending = queue.filter(a => a.status === "pending");
  const waitingLong = pending.filter(a => Date.now() - new Date(a.requestedAt).getTime() > 3600_000).length;
  return (
    <>
      <WidgetHeader icon="deploy" title="Deploy Approvals" action="Review →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, color: pending.length > 0 ? "#fbbf24" : "#34d399", lineHeight:1, fontVariantNumeric:"tabular-nums" }}>{pending.length}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>awaiting approval</span>
        </div>
        {waitingLong > 0 && (
          <div style={{ padding:"8px 10px", borderRadius:6, background:"rgba(251,191,36,0.08)", border:"1px solid rgba(251,191,36,0.25)", fontSize:11, color:"#fcd34d" }}>
            {waitingLong} waiting over 1h
          </div>
        )}
        <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:6, fontSize:11 }}>
          <div className="dash-w-mini"><span>Two-approver</span><strong>{pending.filter(a=>a.neededApprovals>1).length}</strong></div>
          <div className="dash-w-mini"><span>Partially signed</span><strong>{pending.filter(a=>a.approvals.length>0).length}</strong></div>
        </div>
      </div>
    </>
  );
}

function WAttestationTrust({ onNavigate }) {
  const records = typeof ATTESTATION_RECORDS !== "undefined" ? ATTESTATION_RECORDS : [];
  const flagged = records.filter(r => ["unauthorized_artifact","unknown_artifact","agent_identity_invalid"].includes(r.classification) && !r.resolution);
  const staleEvidence = records.filter(r => r.classification === "authorized_but_evidence_stale" || r.classification === "agent_attestation_stale").length;
  const authorizedCurrent = records.filter(r => r.classification === "authorized_current").length;
  return (
    <>
      <WidgetHeader icon="key" title="Attestation Trust" action="Review →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, color: flagged.length > 0 ? "#ef4444" : "#34d399", lineHeight:1, fontVariantNumeric:"tabular-nums" }}>{flagged.length}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>flagged artifacts</span>
        </div>
        {flagged.length > 0 && (
          <div style={{ padding:"8px 10px", borderRadius:6, background:"rgba(239,68,68,0.08)", border:"1px solid rgba(239,68,68,0.25)", fontSize:11, color:"#fca5a5" }}>
            Unauthorized or unidentified — needs a decision
          </div>
        )}
        <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:6, fontSize:11 }}>
          <div className="dash-w-mini"><span>Authorized</span><strong style={{ color:"#34d399" }}>{authorizedCurrent}</strong></div>
          <div className="dash-w-mini"><span>Stale evidence</span><strong style={{ color: staleEvidence > 0 ? "#60a5fa" : undefined }}>{staleEvidence}</strong></div>
        </div>
      </div>
    </>
  );
}

function WPoamSummary({ onNavigate }) {
  usePoamStore();
  const list = typeof POAMS !== "undefined" ? POAMS : [];
  const open = list.filter(p => p.status !== "completed");
  const overdue = open.filter(poamIsOverdue).length;
  const awaiting = open.filter(p => p.status === "awaiting_verification").length;
  const closed = list.filter(p => p.status === "completed").length;
  return (
    <>
      <WidgetHeader icon="activity" title="POA&M Summary" action="Review →" onAction={() => onNavigate("compliance")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, color: open.length > 0 ? "#60a5fa" : "#34d399", lineHeight:1, fontVariantNumeric:"tabular-nums" }}>{open.length}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>open remediation plans</span>
        </div>
        {overdue > 0 && (
          <div style={{ padding:"8px 10px", borderRadius:6, background:"rgba(248,113,113,0.08)", border:"1px solid rgba(248,113,113,0.25)", fontSize:11, color:"#fca5a5" }}>
            {overdue} overdue
          </div>
        )}
        <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:6, fontSize:11 }}>
          <div className="dash-w-mini"><span>Awaiting verification</span><strong style={{ color: awaiting > 0 ? "#a78bfa" : undefined }}>{awaiting}</strong></div>
          <div className="dash-w-mini"><span>Closed</span><strong style={{ color:"#34d399" }}>{closed}</strong></div>
        </div>
      </div>
    </>
  );
}

function WPoamWatchlist({ onNavigate, rows }) {
  usePoamStore();
  const list = typeof POAMS !== "undefined" ? POAMS : [];
  const ranked = list
    .filter(p => p.status !== "completed")
    .map(p => ({ p, overdue: poamIsOverdue(p) }))
    .filter(x => x.overdue || x.p.status === "awaiting_verification")
    .sort((a, b) => (b.overdue - a.overdue) || (a.p.due || "").localeCompare(b.p.due || ""))
    .slice(0, HEIGHT_COUNTS[rows || 1] || 5);
  return (
    <>
      <WidgetHeader icon="activity" title="POA&M Watchlist" action="Review →" onAction={() => onNavigate("compliance")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:6 }}>
        {ranked.map(({ p, overdue }) => (
          <div key={p.id} style={{ display:"flex", alignItems:"center", gap:10, padding:"7px 10px", background:"var(--cf-subtle-bg)", borderRadius:6, fontSize:12, cursor:"pointer" }}
            onClick={() => { onNavigate("compliance"); if (typeof openPoamDetail === "function") setTimeout(() => openPoamDetail(p.id), 60); }}>
            <span className="mono" style={{ fontWeight:700, fontSize:11, color:"var(--cf-brand-purple)", flexShrink:0 }}>{p.id}</span>
            <span style={{ flex:1, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{p.title}</span>
            <span className="chip" style={{ fontSize:9.5, flexShrink:0, color: overdue ? "#f87171" : "#a78bfa", background: overdue ? "rgba(248,113,113,0.14)" : "rgba(167,139,250,0.16)" }}>
              {overdue ? "Overdue" : "Awaiting verification"}
            </span>
            <span style={{ fontSize:10, color:"var(--cf-text-muted)", flexShrink:0 }}>{p.owner}</span>
          </div>
        ))}
        {ranked.length === 0 && <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>Nothing overdue or awaiting verification.</div>}
      </div>
    </>
  );
}

function WFleetHealth({ onNavigate }) {
  const counts = {
    healthy: SYSTEMS.filter(s => s.health === "healthy").length,
    warning: SYSTEMS.filter(s => s.health === "warning" || s.health === "drifted").length,
    critical: SYSTEMS.filter(s => s.health === "critical").length,
    offline: SYSTEMS.filter(s => s.health === "offline").length,
    unknown: SYSTEMS.filter(s => s.health === "unknown").length,
  };
  const total = SYSTEMS.length;
  return (
    <>
      <WidgetHeader icon="server" title="Fleet Health" action="View →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:14 }}>
        <div style={{ display:"flex", alignItems:"baseline", gap:10 }}>
          <span style={{ fontSize:32, fontWeight:700, color:"var(--cf-text-primary)", lineHeight:1, fontVariantNumeric:"tabular-nums" }}>{counts.healthy}</span>
          <span style={{ fontSize:14, color:"var(--cf-text-muted)" }}>of {total} healthy</span>
        </div>
        <div style={{ display:"flex", height:8, borderRadius:99, overflow:"hidden", background:"var(--cf-subtle-bg)" }}>
          {counts.healthy  > 0 && <div style={{ width:`${(counts.healthy/total)*100}%`,  background:"#34d399" }}/>}
          {counts.warning  > 0 && <div style={{ width:`${(counts.warning/total)*100}%`,  background:"#fbbf24" }}/>}
          {counts.critical > 0 && <div style={{ width:`${(counts.critical/total)*100}%`, background:"#f87171" }}/>}
          {counts.offline  > 0 && <div style={{ width:`${(counts.offline/total)*100}%`,  background:"#6b7280" }}/>}
        </div>
        <div style={{ display:"grid", gridTemplateColumns:"repeat(4, 1fr)", gap:8, fontSize:11 }}>
          {[
            { label:"Healthy",  color:"#34d399", n:counts.healthy },
            { label:"Warning",  color:"#fbbf24", n:counts.warning },
            { label:"Critical", color:"#f87171", n:counts.critical },
            { label:"Offline",  color:"#6b7280", n:counts.offline },
          ].map(s => (
            <div key={s.label} style={{ padding:"8px 10px", borderRadius:6, background:"var(--cf-subtle-bg)" }}>
              <div style={{ display:"flex", alignItems:"center", gap:5 }}>
                <span style={{ width:6, height:6, borderRadius:"50%", background: s.color }}/>
                <span style={{ color:"var(--cf-text-muted)" }}>{s.label}</span>
              </div>
              <div style={{ fontSize:18, fontWeight:700, color: s.color, marginTop:2, fontVariantNumeric:"tabular-nums" }}>{s.n}</div>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}

function WHeartbeat({ onNavigate }) {
  const overdue = SYSTEMS.filter(s => s.heartbeatNextInSec < 0).length;
  const stale = SYSTEMS.filter(s => s.heartbeatNextInSec < -s.heartbeatIntervalSec).length;
  const healthy = SYSTEMS.length - overdue;
  return (
    <>
      <WidgetHeader icon="warn" title="Heartbeats" action="View →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", alignItems:"baseline", gap:10 }}>
          <span style={{ fontSize:32, fontWeight:700, color: overdue > 0 ? "#fbbf24" : "#34d399", lineHeight:1, fontVariantNumeric:"tabular-nums" }}>{overdue}</span>
          <span style={{ fontSize:13, color:"var(--cf-text-muted)" }}>overdue</span>
        </div>
        {stale > 0 && (
          <div style={{ padding:"8px 10px", borderRadius:6, background:"rgba(248,113,113,0.08)", border:"1px solid rgba(248,113,113,0.25)", fontSize:12, color:"#fca5a5" }}>
            {stale} system{stale === 1 ? "" : "s"} past 2× heartbeat interval
          </div>
        )}
        <div style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{healthy} of {SYSTEMS.length} reporting on schedule</div>
      </div>
    </>
  );
}

function WBuildQueue({ onNavigate }) {
  return (
    <>
      <WidgetHeader icon="build" title="Build Queue" action="View →" onAction={() => onNavigate("builds")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, color:"#60a5fa", lineHeight:1, fontVariantNumeric:"tabular-nums" }}>{BUILD_STATS.building}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>building</span>
        </div>
        <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:6, fontSize:11 }}>
          <div className="dash-w-mini"><span>Queued</span><strong>{BUILD_STATS.queued}</strong></div>
          <div className="dash-w-mini"><span>Failed 24h</span><strong style={{ color: BUILD_STATS.failed24h > 0 ? "#f87171" : undefined }}>{BUILD_STATS.failed24h}</strong></div>
          <div className="dash-w-mini"><span>Workers</span><strong>{BUILD_STATS.workers}/{BUILD_STATS.totalWorkers}</strong></div>
          <div className="dash-w-mini"><span>Slot use</span><strong>{Math.round(BUILD_WORKERS.filter(w=>w.status==="running").reduce((a,w)=>a+w.slots.used,0)/Math.max(1,BUILD_WORKERS.filter(w=>w.status==="running").reduce((a,w)=>a+w.slots.total,0))*100)}%</strong></div>
        </div>
      </div>
    </>
  );
}

function WEvalQueue({ onNavigate }) {
  return (
    <>
      <WidgetHeader icon="eval" title="Eval Queue" action="View →" onAction={() => onNavigate("evals")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, color:"#a78bfa", lineHeight:1, fontVariantNumeric:"tabular-nums" }}>{EVAL_STATS.active}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>active</span>
        </div>
        <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:6, fontSize:11 }}>
          <div className="dash-w-mini"><span>Completed</span><strong style={{ color:"#34d399" }}>{EVAL_STATS.completed}</strong></div>
          <div className="dash-w-mini"><span>Failed</span><strong style={{ color: EVAL_STATS.failed > 0 ? "#f87171" : undefined }}>{EVAL_STATS.failed}</strong></div>
        </div>
      </div>
    </>
  );
}

function WCveSummary({ onNavigate }) {
  return (
    <>
      <WidgetHeader icon="shield" title="CVE Summary" action="View →" onAction={() => onNavigate("cves")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, color:"#f87171", lineHeight:1, fontVariantNumeric:"tabular-nums" }}>{CVE_STATS.critical}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>critical CVEs</span>
        </div>
        {CVE_STATS.exploited > 0 && (
          <div style={{ padding:"8px 10px", borderRadius:6, background:"rgba(248,113,113,0.08)", border:"1px solid rgba(248,113,113,0.25)", fontSize:11, color:"#fca5a5" }}>
            {CVE_STATS.exploited} actively exploited
          </div>
        )}
        <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:6, fontSize:11 }}>
          <div className="dash-w-mini"><span>High</span><strong style={{ color:"#fbbf24" }}>{CVE_STATS.high}</strong></div>
          <div className="dash-w-mini"><span>Patchable</span><strong style={{ color:"#34d399" }}>{CVE_STATS.fixable}</strong></div>
        </div>
      </div>
    </>
  );
}

function WTopAffected({ onNavigate, rows }) {
  const top = (CVE_INSIGHTS?.sysScores || []).slice(0, HEIGHT_COUNTS[rows || 1] || 5);
  return (
    <>
      <WidgetHeader icon="shield" title="Top CVE-affected systems" action="View →" onAction={() => onNavigate("cves")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:6 }}>
        {top.map(({ sys, counts }) => (
          <div key={sys.id} style={{ display:"flex", alignItems:"center", gap:10, padding:"7px 10px", background:"var(--cf-subtle-bg)", borderRadius:6, fontSize:12 }}>
            <span className="status-dot" style={{ "--status-color": sys.statusColor }}/>
            <span className="mono" style={{ fontWeight:600, flex:1, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{sys.hostname}</span>
            <EnvBadge env={sys.environment}/>
            <span className="chip chip-critical" style={{ fontSize:10 }}>{counts.critical}</span>
            <span className="chip chip-warning" style={{ fontSize:10 }}>{counts.high}</span>
          </div>
        ))}
        {top.length === 0 && <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>No affected systems.</div>}
      </div>
    </>
  );
}

function WGitGraph({ onNavigate }) {
  const FLAKE_COLORS = ["#a78bfa", "#60a5fa", "#34d399", "#f59e0b", "#f472b6"];
  const flakes = FLAKE_REGISTRY.slice(0, 5);
  const [collapsed, setCollapsed] = React.useState(false);

  const merged = React.useMemo(() => {
    const grouped = flakes.map((f, idx) => (FLAKE_COMMITS[f.id] || []).slice(0, 5).map((c, i) => ({
      ...c, flakeIdx: idx, flakeName: f.name, ord: i,
    })));
    const stream = [];
    const maxRows = Math.max(...grouped.map(g => g.length));
    for (let i = 0; i < maxRows; i++) {
      grouped.forEach(g => { if (g[i]) stream.push(g[i]); });
    }
    return stream.slice(0, 18);
  }, []);

  // Per-commit rollup — how many systems are on this commit, evaluating, building
  const rollup = React.useMemo(() => {
    const m = {};
    merged.forEach((c, i) => {
      // Deterministic-ish counts from the sha so each commit has a consistent profile
      const seed = (c.sha || "").split("").reduce((a, ch) => a + ch.charCodeAt(0), 0) + i;
      const onCommit = i === 0 ? 4 + (seed % 5) : Math.max(0, 3 - i + (seed % 3));
      const evaluating = i < 2 ? (seed % 3) : 0;
      const building = i < 3 ? (seed % 4) : 0;
      const failed = (seed % 7 === 0) ? 1 : 0;
      const failedKind = (seed % 2 === 0) ? "eval" : "build";
      m[c.sha] = { onCommit, evaluating, building, failed, failedKind };
    });
    return m;
  }, [merged]);

  const ROW_H = 36, LANE_W = 22, LEFT_PAD = 16;
  const lanes = collapsed ? 1 : flakes.length;
  const graphWidth = LEFT_PAD + lanes * LANE_W + 8;
  const height = merged.length * ROW_H + 16;

  return (
    <>
      <WidgetHeader icon="git" title="Flake Git Graph"
        action={collapsed ? "Expand lanes" : "Collapse to one line"}
        onAction={() => setCollapsed(c => !c)}/>
      <div className="dash-w-body" style={{ gap:10 }}>
        <div style={{ display:"flex", flexWrap:"wrap", gap:10, paddingLeft:4, alignItems:"center" }}>
          {flakes.map((f, idx) => (
            <span key={f.id} style={{ display:"inline-flex", alignItems:"center", gap:6, fontSize:11 }}>
              <span style={{ width:9, height:9, borderRadius:"50%", background: FLAKE_COLORS[idx], boxShadow:`0 0 0 2px color-mix(in oklab, ${FLAKE_COLORS[idx]} 30%, transparent)` }}/>
              <span className="mono" style={{ color:"var(--cf-text-secondary)" }}>{f.name}</span>
            </span>
          ))}
          <button className="btn btn-ghost focus-ring xs" style={{ marginLeft:"auto" }}
            onClick={() => onNavigate("flakes")}>Open flakes →</button>
        </div>

        <div style={{ display:"grid", gridTemplateColumns:`${graphWidth}px 1fr`, alignItems:"start", gap:10, overflow:"hidden" }}>
          <svg width={graphWidth} height={height} style={{ flexShrink:0 }}>
            {collapsed ? (
              <>
                {/* Single rail */}
                <line x1={LEFT_PAD} y1={ROW_H/2} x2={LEFT_PAD} y2={(merged.length - 1) * ROW_H + ROW_H/2}
                  stroke="var(--cf-text-muted)" strokeWidth="2" strokeOpacity="0.4"/>
                {merged.map((c, i) => {
                  const y = i * ROW_H + ROW_H/2;
                  const color = FLAKE_COLORS[c.flakeIdx];
                  return (
                    <g key={i}>
                      <circle cx={LEFT_PAD} cy={y} r="5" fill={color} stroke="var(--cf-card-bg)" strokeWidth="2"/>
                    </g>
                  );
                })}
              </>
            ) : (
              <>
                {flakes.map((_, idx) => {
                  const x = LEFT_PAD + idx * LANE_W;
                  const rows = merged.map((c, i) => c.flakeIdx === idx ? i : -1).filter(i => i >= 0);
                  if (rows.length === 0) return null;
                  const yStart = rows[0] * ROW_H + ROW_H/2;
                  const yEnd = rows[rows.length-1] * ROW_H + ROW_H/2;
                  return (
                    <line key={idx} x1={x} y1={yStart} x2={x} y2={yEnd}
                      stroke={FLAKE_COLORS[idx]} strokeWidth="2" strokeOpacity="0.4"/>
                  );
                })}
                {merged.map((c, i) => {
                  const x = LEFT_PAD + c.flakeIdx * LANE_W;
                  const y = i * ROW_H + ROW_H/2;
                  const color = FLAKE_COLORS[c.flakeIdx];
                  return (
                    <g key={i}>
                      <circle cx={x} cy={y} r="5" fill={color} stroke="var(--cf-card-bg)" strokeWidth="2"/>
                      <circle cx={x} cy={y} r="2" fill="var(--cf-card-bg)" opacity="0.4"/>
                    </g>
                  );
                })}
              </>
            )}
          </svg>

          <div style={{ display:"flex", flexDirection:"column", gap:0, minWidth:0 }}>
            {merged.map((c, i) => {
              const r = rollup[c.sha] || { onCommit:0, evaluating:0, building:0, failed:0 };
              return (
              <div key={i} style={{ height:ROW_H, display:"flex", alignItems:"center", gap:10, minWidth:0, padding:"0 2px", borderRadius:6, cursor:"pointer" }}
                onMouseEnter={e=>e.currentTarget.style.background="var(--cf-subtle-bg)"}
                onMouseLeave={e=>e.currentTarget.style.background="transparent"}
                onClick={() => onNavigate("flakes")}
                title={`${c.flakeName} · ${c.author}`}
              >
                <span className="mono" style={{ fontSize:11, color:"var(--cf-brand-purple)", fontWeight:700, flexShrink:0, width:60 }}>{c.sha}</span>
                {collapsed && (
                  <span className="chip mono" style={{ fontSize:10, padding:"1px 6px", background:`color-mix(in oklab, ${FLAKE_COLORS[c.flakeIdx]} 18%, transparent)`, color: FLAKE_COLORS[c.flakeIdx], flexShrink:0 }}>{c.flakeName}</span>
                )}
                <span style={{ fontSize:12, color:"var(--cf-text-primary)", overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap", flex:1, minWidth:0 }}>{c.msg}</span>
                <div style={{ display:"flex", gap:4, flexShrink:0 }}>
                  {r.onCommit > 0 && (
                    <span title={`${r.onCommit} systems on this commit`} style={{ display:"inline-flex", alignItems:"center", gap:3, fontSize:10, padding:"1px 6px", borderRadius:99, background:"rgba(52,211,153,0.14)", color:"#34d399", fontWeight:600 }}>
                      <Icon name="server" size={9}/>{r.onCommit}
                    </span>
                  )}
                  {r.building > 0 && (
                    <span title={`${r.building} systems building — view builds`} onClick={ev=>{ev.stopPropagation();onNavigate("builds");}} style={{ display:"inline-flex", alignItems:"center", gap:3, fontSize:10, padding:"1px 6px", borderRadius:99, background:"rgba(96,165,250,0.14)", color:"#60a5fa", fontWeight:600, cursor:"pointer" }}>
                      <Icon name="build" size={9}/>{r.building}
                    </span>
                  )}
                  {r.evaluating > 0 && (
                    <span title={`${r.evaluating} systems evaluating — view evals`} onClick={ev=>{ev.stopPropagation();onNavigate("evals");}} style={{ display:"inline-flex", alignItems:"center", gap:3, fontSize:10, padding:"1px 6px", borderRadius:99, background:"rgba(167,139,250,0.16)", color:"#a78bfa", fontWeight:600, cursor:"pointer" }}>
                      <Icon name="eval" size={9}/>{r.evaluating}
                    </span>
                  )}
                  {r.failed > 0 && (
                    <span title={`${r.failed} failed — view the failure`} onClick={ev=>{ev.stopPropagation();onNavigate(r.failedKind === "build" ? "builds" : "evals", { sha: c.sha, msg: c.msg, flake: c.flakeName, author: c.author, at: c.at });}} style={{ display:"inline-flex", alignItems:"center", gap:3, fontSize:10, padding:"1px 6px", borderRadius:99, background:"rgba(248,113,113,0.14)", color:"#f87171", fontWeight:600, cursor:"pointer" }}>
                      <Icon name="warn" size={9}/>{r.failed}
                    </span>
                  )}
                </div>
                <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)", flexShrink:0, width:54, textAlign:"right" }}>{c.author}</span>
                <span style={{ fontSize:11, color:"var(--cf-text-muted)", flexShrink:0, width:54, textAlign:"right" }}>{c.at}</span>
              </div>
              );
            })}
          </div>
        </div>
      </div>
    </>
  );
}

function WDeploymentTimeline({ onNavigate, rows }) {
  // Build a fake recent-activity feed from systems + builds + evals
  const items = React.useMemo(() => {
    const out = [];
    // Recent successful deploys
    SYSTEMS.slice(0, 8).forEach((sys, i) => {
      out.push({
        kind: "deploy",
        at: ["2m ago", "12m ago", "28m ago", "1h ago"][i] || `${i}h ago`,
        title: <>Deployed <span className="mono" style={{ fontWeight:600 }}>{sys.commit}</span> to <span className="mono">{sys.hostname}</span></>,
        sub: sys.commitMessage,
        env: sys.environment,
        color: sys.health === "critical" ? "#f87171" : "#34d399",
        icon: "deploy",
      });
    });
    // Active builds
    ACTIVE_BUILDS.slice(0, 3).forEach((b, i) => {
      out.push({
        kind: "build",
        at: b.queuedAt,
        title: <>Building <span className="mono" style={{ fontWeight:600 }}>{b.pkg}</span> on <span className="mono">{b.worker}</span></>,
        sub: `Worker progress · ${Math.round(b.progress*100)}%`,
        env: null,
        color: "#60a5fa",
        icon: "build",
      });
    });
    // Active evals
    ACTIVE_EVALS.slice(0, 3).forEach((e, i) => {
      out.push({
        kind: "eval",
        at: e.startedAt,
        title: <>Evaluating <span className="mono" style={{ fontWeight:600 }}>{e.flake}@{e.commit}</span></>,
        sub: `${e.systemCount} systems · policy ${e.policyPass}✓ / ${e.policyFail}✗`,
        env: null,
        color: "#a78bfa",
        icon: "eval",
      });
    });
    // Failed history
    HISTORY_BUILDS.filter(b => b.status === "failed").slice(0, 3).forEach((b, i) => {
      out.push({
        kind: "build_failed",
        at: b.queuedAt,
        title: <>Build failed for <span className="mono" style={{ fontWeight:600 }}>{b.pkg}</span></>,
        sub: `Worker ${b.worker} · attempt ${b.attempts}`,
        env: null,
        color: "#f87171",
        icon: "warn",
      });
    });
    return out;
  }, []);
  const shown = items.slice(0, HEIGHT_COUNTS[rows || 1] || 5);

  return (
    <>
      <WidgetHeader icon="history" title="Deployment Timeline" action="History →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:0 }}>
        {shown.map((item, i) => (
          <div key={i} style={{ display:"flex", gap:12, paddingLeft:4 }}>
            <div style={{ display:"flex", flexDirection:"column", alignItems:"center", paddingTop:6, flexShrink:0 }}>
              <div style={{
                width:24, height:24, borderRadius:6,
                background: `color-mix(in oklab, ${item.color} 18%, transparent)`,
                color: item.color,
                display:"grid", placeItems:"center",
                flexShrink:0,
              }}>
                <Icon name={item.icon} size={12}/>
              </div>
              {i < shown.length - 1 && <div style={{ width:2, flex:1, background:"var(--cf-divider)", minHeight:18 }}/>}
            </div>
            <div style={{ paddingTop:5, paddingBottom:i === shown.length - 1 ? 0 : 14, minWidth:0, flex:1 }}>
              <div style={{ display:"flex", justifyContent:"space-between", gap:8, alignItems:"baseline" }}>
                <div style={{ fontSize:12, color:"var(--cf-text-primary)", overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>
                  {item.title}
                </div>
                <div style={{ fontSize:11, color:"var(--cf-text-muted)", whiteSpace:"nowrap", flexShrink:0 }}>{item.at}</div>
              </div>
              <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2, display:"flex", gap:6, alignItems:"center", flexWrap:"wrap" }}>
                {item.env && <EnvBadge env={item.env}/>}
                <span style={{ overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap", minWidth:0 }}>{item.sub}</span>
              </div>
            </div>
          </div>
        ))}
      </div>
    </>
  );
}

function WRecentCommits({ onNavigate, rows }) {
  // Flatten all flake commits, most recent first
  const allCommits = Object.entries(FLAKE_COMMITS).flatMap(([flakeId, list]) =>
    list.slice(0, 5).map(c => ({ ...c, flakeId, flakeName: FLAKE_REGISTRY.find(f => f.id === flakeId)?.name || flakeId }))
  ).slice(0, HEIGHT_COUNTS[rows || 1] || 5);
  return (
    <>
      <WidgetHeader icon="git" title="Recent Commits" action="View →" onAction={() => onNavigate("flakes")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:6 }}>
        {allCommits.map(c => (
          <div key={c.flakeId+c.sha} style={{ display:"flex", alignItems:"center", gap:10, padding:"7px 10px", background:"var(--cf-subtle-bg)", borderRadius:6, fontSize:12 }}>
            <span className="mono" style={{ fontWeight:600, fontSize:11, color:"var(--cf-brand-purple)" }}>{c.sha}</span>
            <span style={{ flex:1, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{c.msg}</span>
            <span style={{ fontSize:10, color:"var(--cf-text-muted)" }}>{c.flakeName}</span>
            <span style={{ fontSize:10, color:"var(--cf-text-muted)" }}>{c.at}</span>
          </div>
        ))}
      </div>
    </>
  );
}

function WCacheHealth({ onNavigate }) {
  const list = (window.CACHE_DESTINATIONS || []);
  const issues = list.filter(c => c.status !== "healthy");
  return (
    <>
      <WidgetHeader icon="download" title="Cache Health" action="View →" onAction={() => onNavigate("caches")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:8 }}>
        {list.slice(0,4).map(c => {
          const pct = c.storage ? Math.round((c.storage.used / c.storage.total) * 100) : 0;
          const color = c.status === "healthy" ? "#34d399" : c.status === "warning" ? "#fbbf24" : "#f87171";
          return (
            <div key={c.id} style={{ display:"flex", flexDirection:"column", gap:3 }}>
              <div style={{ display:"flex", justifyContent:"space-between", fontSize:11 }}>
                <span className="mono truncate" style={{ maxWidth:"60%" }}>{c.name}</span>
                <span style={{ color: color }}>{c.storage ? `${pct}%` : c.status}</span>
              </div>
              <div style={{ height:4, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
                <div style={{ width:`${pct}%`, height:"100%", background: color }}/>
              </div>
            </div>
          );
        })}
        {issues.length > 0 && (
          <div style={{ fontSize:11, color:"#fca5a5", marginTop:4 }}>{issues.length} cache{issues.length === 1 ? "" : "s"} with issues</div>
        )}
      </div>
    </>
  );
}

function WEnvBreakdown({ onNavigate }) {
  const byEnv = ENVIRONMENTS.map(e => ({ env: e, count: SYSTEMS.filter(s => s.environment === e.name).length })).sort((a,b) => b.count - a.count);
  const total = SYSTEMS.length;
  return (
    <>
      <WidgetHeader icon="env" title="Environments" action="View →" onAction={() => onNavigate("environments")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:6 }}>
        {byEnv.map(({ env, count }) => (
          <div key={env.name} style={{ display:"flex", flexDirection:"column", gap:3 }}>
            <div style={{ display:"flex", justifyContent:"space-between", fontSize:12 }}>
              <span style={{ display:"flex", alignItems:"center", gap:6 }}>
                <span style={{ width:6, height:6, borderRadius:"50%", background: env.color }}/>
                <span className="mono">{env.name}</span>
              </span>
              <span className="mono" style={{ color:"var(--cf-text-muted)" }}>{count}</span>
            </div>
            <div style={{ height:4, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
              <div style={{ width:`${(count/total)*100}%`, height:"100%", background: env.color }}/>
            </div>
          </div>
        ))}
      </div>
    </>
  );
}

function WQuickActions({ onNavigate }) {
  const actions = [
    { label:"Systems",     icon:"server",   route:"systems" },
    { label:"Builds",      icon:"build",    route:"builds" },
    { label:"Evaluations", icon:"eval",     route:"evals" },
    { label:"Flakes",      icon:"git",      route:"flakes" },
    { label:"CVEs",        icon:"shield",   route:"cves" },
    { label:"Caches",      icon:"download", route:"caches" },
  ];
  return (
    <>
      <WidgetHeader icon="tweaks" title="Quick Actions"/>
      <div className="dash-w-body" style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:6 }}>
        {actions.map(a => (
          <button key={a.label} className="btn btn-ghost focus-ring" onClick={() => onNavigate(a.route)}
            style={{ justifyContent:"flex-start", padding:"8px 10px", fontSize:12, minWidth:0, overflow:"hidden", textOverflow:"ellipsis" }}>
            <Icon name={a.icon} size={13}/> <span style={{ overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{a.label}</span>
          </button>
        ))}
      </div>
    </>
  );
}

/* ── Widget picker modal ── */
/* ── Widget library modal ── */
const WIDGET_CATEGORIES = {
  fleetHealth:        "Fleet",
  heartbeatStatus:    "Fleet",
  envBreakdown:       "Fleet",
  buildQueue:         "Pipeline",
  evalQueue:          "Pipeline",
  cacheHealth:        "Infrastructure",
  cveSummary:         "Security",
  topAffected:        "Security",
  recentCommits:      "Activity",
  deploymentTimeline: "Activity",
  gitGraph:           "Activity",
  attestationTrust:   "Security",
  poamSummary:        "Security",
  poamWatchlist:      "Security",
  quickActions:       "Actions",
};
const CATEGORY_ORDER = ["Fleet", "Pipeline", "Security", "Activity", "Infrastructure", "Actions"];

function WidgetPicker({ addedIds, onAdd, onClose }) {
  const all = Object.values(DASHBOARD_WIDGETS);
  const [query, setQuery] = React.useState("");
  const [cat, setCat] = React.useState("All");
  const [selId, setSelId] = React.useState(null);

  const cats = ["All", ...CATEGORY_ORDER.filter(c => all.some(w => WIDGET_CATEGORIES[w.id] === c))];
  const q = query.trim().toLowerCase();
  const filtered = all.filter(w => {
    if (cat !== "All" && WIDGET_CATEGORIES[w.id] !== cat) return false;
    if (q && !(`${w.title} ${w.description}`.toLowerCase().includes(q))) return false;
    return true;
  });
  const sel = all.find(w => w.id === selId) || filtered[0] || all[0];
  const selAdded = sel && addedIds.has(sel.id);
  const widthLabel = (n) => n === 1 ? "⅓ width" : n === 2 ? "⅔ width" : "Full width";

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()} style={{ width: "min(820px,96vw)", maxHeight: "88vh", display: "flex", flexDirection: "column" }}>
        <div className="modal-head">
          <h2><Icon name="plus" size={14} style={{ marginRight: 6, verticalAlign: "text-bottom" }} />Widget library</h2>
          <p>Add widgets from the library to your dashboard. {addedIds.size} of {all.length} added.</p>
        </div>

        <div style={{ display: "flex", minHeight: 0, flex: 1 }}>
          {/* Catalog */}
          <div style={{ flex: "1 1 0", minWidth: 0, display: "flex", flexDirection: "column", borderRight: "1px solid var(--cf-divider)" }}>
            <div style={{ padding: "12px 16px", display: "flex", flexDirection: "column", gap: 10, borderBottom: "1px solid var(--cf-divider)" }}>
              <div className="filter-search" style={{ width: "100%" }}>
                <Icon name="search" />
                <input className="input focus-ring" placeholder="Search widgets…" value={query} onChange={e => setQuery(e.target.value)} />
              </div>
              <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                {cats.map(c => (
                  <button key={c} className={`chip focus-ring${cat === c ? " chip-info" : ""}`}
                    onClick={() => setCat(c)}
                    style={{ cursor: "pointer", border: cat === c ? undefined : "1px solid var(--cf-divider)", background: cat === c ? undefined : "transparent", color: cat === c ? undefined : "var(--cf-text-secondary)" }}>
                    {c}
                  </button>
                ))}
              </div>
            </div>
            <div style={{ overflowY: "auto", padding: 8 }}>
              {filtered.length === 0 ? (
                <div className="empty" style={{ margin: 16 }}><h3>No widgets match</h3><div>Try a different search or category.</div></div>
              ) : filtered.map(w => {
                const added = addedIds.has(w.id);
                const isSel = sel && sel.id === w.id;
                return (
                  <button key={w.id} className="focus-ring widget-lib-item"
                    onClick={() => setSelId(w.id)}
                    style={{ outline: isSel ? "1px solid var(--cf-brand-purple)" : undefined, background: isSel ? "color-mix(in oklab, var(--cf-brand-purple) 8%, transparent)" : undefined }}>
                    <span className="widget-lib-icon"><Icon name={w.icon} size={15} /></span>
                    <span style={{ minWidth: 0, flex: 1 }}>
                      <span className="widget-lib-title">{w.title}</span>
                      <span className="widget-lib-desc">{w.description}</span>
                    </span>
                    {added
                      ? <span className="chip chip-healthy" style={{ fontSize: 10, flexShrink: 0 }}><Icon name="check" size={9} /> Added</span>
                      : <span className="widget-lib-add"><Icon name="plus" size={13} /></span>}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Detail */}
          {sel && (
            <div style={{ flex: "0 0 300px", maxWidth: 300, padding: 18, display: "flex", flexDirection: "column", gap: 14, overflowY: "auto" }}>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <span className="widget-lib-icon" style={{ width: 38, height: 38 }}><Icon name={sel.icon} size={18} /></span>
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontSize: 15, fontWeight: 650 }}>{sel.title}</div>
                  <div style={{ fontSize: 11, color: "var(--cf-brand-purple)", fontWeight: 600 }}>{WIDGET_CATEGORIES[sel.id] || "Widget"}</div>
                </div>
              </div>
              <p style={{ margin: 0, fontSize: 13, color: "var(--cf-text-secondary)", lineHeight: 1.55 }}>{sel.description}</p>

              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <div style={{ fontSize: 10, fontWeight: 700, letterSpacing: "0.07em", textTransform: "uppercase", color: "var(--cf-text-muted)" }}>Defaults</div>
                <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                  <span className="chip chip-unknown" style={{ fontSize: 11 }}><Icon name="grid" size={10} /> {widthLabel(sel.defaultCols)}</span>
                  {HEIGHT_RESIZABLE.has(sel.id)
                    ? <span className="chip chip-unknown" style={{ fontSize: 11 }}><Icon name="rows" size={10} /> Adjustable height</span>
                    : <span className="chip chip-unknown" style={{ fontSize: 11 }}>Fixed height</span>}
                </div>
              </div>

              <div style={{ marginTop: "auto" }}>
                {selAdded ? (
                  <button className="btn btn-ghost focus-ring" disabled style={{ width: "100%", justifyContent: "center", opacity: 0.7 }}>
                    <Icon name="check" size={13} /> Already on dashboard
                  </button>
                ) : (
                  <button className="btn btn-primary focus-ring" style={{ width: "100%", justifyContent: "center" }} onClick={() => onAdd(sel.id)}>
                    <Icon name="plus" size={13} /> Add to dashboard
                  </button>
                )}
                <div className="help" style={{ marginTop: 8, textAlign: "center" }}>Reorder & resize it after adding.</div>
              </div>
            </div>
          )}
        </div>

        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Done</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { DashboardView });
