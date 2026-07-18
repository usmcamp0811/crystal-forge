// Evaluations view — active queue + history with bulk-select + drawer + toast + keyboard nav

function EvalsView({ focus, onClearFocus, onOpenSystem, onOpenPolicy }) {
  const [tab, setTab] = React.useState("active");
  const hasFailed = (typeof HISTORY_EVALS !== "undefined" ? HISTORY_EVALS : []).some(e => e.status === "failed");
  // History tab pulses continuously while there are failures not yet looked at.
  const [ackedHist, setAckedHist] = React.useState(false);
  const flashTab = hasFailed && !ackedHist;
  const [flashHistRows, setFlashHistRows] = React.useState(false);
  React.useEffect(() => {
    if (tab === "history" && !ackedHist && hasFailed) {
      setAckedHist(true);
      acknowledgeView("evals");
      setFlashHistRows(true);
      const t = setTimeout(() => setFlashHistRows(false), 3200);
      return () => clearTimeout(t);
    }
  }, [tab]);
  const [filterStatus, setFilterStatus] = React.useState("all");
  const [filterFlake, setFilterFlake]   = React.useState("all");
  const [drawerEv, setDrawerEv]   = React.useState(null);
  const [evals, setEvals]         = React.useState(ACTIVE_EVALS);
  React.useEffect(() => {
    if (!focus) return;
    const bySha = (e) => e.commit === focus.sha || e.commit?.startsWith(focus.sha) || focus.sha?.startsWith(e.commit);
    const byFlakeStatus = (e) => e.flake === focus.flake && (!focus.status || e.status === focus.status);
    const byFlake = (e) => e.flake === focus.flake;
    const find = (list) => list.find(bySha) || list.find(byFlakeStatus) || list.find(byFlake);
    const inHist = find(HISTORY_EVALS);
    const inActive = !inHist && find(ACTIVE_EVALS);
    if (inHist) { setTab("history"); setQuery(""); setDrawerEv(inHist); }
    else if (inActive) { setTab("active"); setQuery(""); setDrawerEv(inActive); }
    else { setQuery(focus.flake || focus.sha || ""); }
    onClearFocus?.();
  }, [focus]);
  const [toast, setToast]         = React.useState(null);
  const [activeIdx, setActiveIdx] = React.useState(0);
  const undoTimer = React.useRef(null);
  const activeSel = useMultiSelect(tab);
  // Search filter (same UX as Builds) — applies to whichever tab is showing.
  const [query, setQuery] = React.useState("");
  React.useEffect(() => { setQuery(""); }, [tab]);
  const q = query.trim().toLowerCase();
  const matchEval = React.useCallback((e) => !q ||
    [e.flake, e.commit, e.branch, e.meta?.label, e.status].filter(Boolean)
      .some(v => String(v).toLowerCase().includes(q)), [q]);
  const bulkCancel = () => {
    const ids = [...activeSel.ids];
    ids.forEach(id => cancelEvalSilent(id));
    setToast({ msg: `Cancelled ${ids.length} evaluation${ids.length===1?"":"s"}`, action:null });
    setTimeout(()=>setToast(null), 3500);
    activeSel.clear();
  };
  const cancelEvalSilent = (id) => {
    setEvals(prev => prev.map(e => e.id !== id ? e : {
      ...e,
      status: e.status === "in_progress" ? "cancelling" : "cancelled",
      meta: EVAL_STATUS_META[e.status === "in_progress" ? "cancelling" : "cancelled"],
      canCancel: false,
      canForceCancel: e.status === "in_progress",
    }));
  };

  // Soft-cancel with undo via toast
  const cancelEval = (id, force) => {
    if (force) {
      setEvals(prev => prev.map(e => e.id !== id ? e : { ...e, status:"cancelled", meta:EVAL_STATUS_META.cancelled, canCancel:false, canForceCancel:false }));
      setToast({ msg:"Force-cancelled eval", action:null });
      setTimeout(()=>setToast(null), 4000);
      setDrawerEv(null);
      return;
    }
    const original = evals.find(e => e.id === id);
    if (!original) return;
    setEvals(prev => prev.map(e => e.id !== id ? e : {
      ...e,
      status: e.status === "in_progress" ? "cancelling" : "cancelled",
      meta: EVAL_STATUS_META[e.status === "in_progress" ? "cancelling" : "cancelled"],
      canCancel: false,
      canForceCancel: e.status === "in_progress",
    }));
    if (undoTimer.current) clearTimeout(undoTimer.current);
    setToast({
      msg: `Cancelled ${original.flake} eval`,
      action: () => {
        setEvals(prev => prev.map(e => e.id === id ? original : e));
        setToast(null);
      },
    });
    undoTimer.current = setTimeout(() => setToast(null), 6000);
  };

  const moveEval = (id, dir) => {
    setEvals(prev => {
      const idx = prev.findIndex(e => e.id === id);
      if ((dir === -1 && idx === 0) || (dir === 1 && idx === prev.length - 1)) return prev;
      const next = [...prev];
      [next[idx], next[idx + dir]] = [next[idx + dir], next[idx]];
      return next.map((e, i) => ({ ...e, queuePos: i + 1 }));
    });
  };

  // Drag-reorder: move `id` to occupy `toIdx`.
  const reorderEval = (id, toIdx) => {
    setEvals(prev => {
      const from = prev.findIndex(e => e.id === id);
      if (from === -1 || toIdx < 0 || toIdx >= prev.length || from === toIdx) return prev;
      const next = [...prev];
      const [moved] = next.splice(from, 1);
      next.splice(toIdx, 0, moved);
      return next.map((e, i) => ({ ...e, queuePos: i + 1 }));
    });
  };

  const historyFiltered = HISTORY_EVALS.filter(e => {
    if (filterStatus !== "all" && e.status !== filterStatus) return false;
    if (filterFlake  !== "all" && e.flake  !== filterFlake)  return false;
    if (!matchEval(e)) return false;
    return true;
  });
  const historySel = useMultiSelect("hist|" + filterStatus + "|" + filterFlake + "|" + q);
  const evalsShown = evals.filter(matchEval);
  const activePaging = useInfiniteScroll("active|" + q, 20);
  const evalsPaged = evalsShown.slice(0, activePaging.count);
  const activeHasMore = activePaging.count < evalsShown.length;
  const histPaging = useInfiniteScroll("hist|" + filterStatus + "|" + filterFlake + "|" + q, 20);
  const historyPaged = historyFiltered.slice(0, histPaging.count);
  const histHasMore = histPaging.count < historyFiltered.length;

  // Keyboard nav (when drawer closed)
  React.useEffect(() => {
    if (drawerEv) return;
    const list = tab === "active" ? evalsShown : historyFiltered;
    const onKey = (e) => {
      if (e.target.tagName === "INPUT" || e.target.tagName === "SELECT" || e.target.tagName === "TEXTAREA") return;
      if (e.key === "j" || e.key === "ArrowDown") { e.preventDefault(); setActiveIdx(i => Math.min(list.length-1, i+1)); }
      else if (e.key === "k" || e.key === "ArrowUp") { e.preventDefault(); setActiveIdx(i => Math.max(0, i-1)); }
      else if (e.key === "Enter") {
        const ev = list[activeIdx];
        if (ev) setDrawerEv(ev);
      } else if (e.key === "c" && tab === "active") {
        const ev = list[activeIdx];
        if (ev?.canCancel) cancelEval(ev.id, false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [drawerEv, tab, evals, evalsShown, historyFiltered, activeIdx]);

  React.useEffect(() => { setActiveIdx(0); }, [tab, filterStatus, filterFlake, query]);

  // History bulk selection (range-aware, mirrors Builds).
  const historyIds = historyFiltered.map(e => e.id);
  const selectAll = () => {
    if (historyIds.every(id => historySel.has(id))) historySel.clear();
    else historySel.set(historyIds);
  };
  const bulkAction = (label) => {
    setToast({ msg: `${label} ${historySel.size} evaluations`, action: null });
    setTimeout(()=>setToast(null), 3000);
    historySel.clear();
  };

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">Evaluations</h1>
          <p className="page-subtitle">{EVAL_STATS.active} active · {EVAL_STATS.completed} completed · {EVAL_STATS.failed} failed</p>
        </div>
        <div style={{ display:"flex", gap:12, alignItems:"center" }}>
          <LiveIndicator />
        </div>
      </div>

      <div className="stat-strip">
        {[
          { label:"Active",    val:EVAL_STATS.active,    color:"#60a5fa" },
          { label:"Completed", val:EVAL_STATS.completed, color:"#34d399" },
          { label:"Failed",    val:EVAL_STATS.failed,    color:"#f87171" },
          { label:"Total",     val:EVAL_STATS.total,     color:"var(--cf-text-secondary)" },
        ].map(s => (
          <div key={s.label} className="stat">
            <span className="stat-accent" style={{ "--stat-color": s.color }} />
            <div className="stat-label">{s.label}</div>
            <div className="stat-value" style={{ color:s.color }}>{s.val}</div>
          </div>
        ))}
      </div>

      <div className="card" style={{ overflow:"hidden" }}>
        <div className="sd-tabs q-tabbar" style={{ padding:"0 16px", borderBottom:"1px solid var(--cf-card-border)" }}>
          <button className={`sd-tab focus-ring${tab==="active"?" active":""}`} onClick={()=>setTab("active")}>
            Active Queue <span className="sd-tab-badge">{evals.length}</span>
          </button>
          <button className={`sd-tab focus-ring${tab==="history"?" active":""}${flashTab?" attention-flash-tab":""}`} onClick={()=>setTab("history")}>
            History <span className="sd-tab-badge">{HISTORY_EVALS.length}</span>
          </button>
          {((tab==="active" && evals.some(e=>e.canCancel)) || tab==="history") && <MultiSelectHint />}
          <div className="q-search">
            <Icon name="search" size={13} />
            <input className="q-search-input" placeholder={`Search ${tab==="active"?"queue":"history"}…`}
              value={query} onChange={e=>setQuery(e.target.value)} />
            {q && <span className="q-search-count">{(tab==="active"?evalsShown.length:historyFiltered.length)} of {tab==="active"?evals.length:HISTORY_EVALS.length}</span>}
            {q && <button className="btn-icon xs focus-ring" title="Clear search" onClick={()=>setQuery("")}><Icon name="x" size={13}/></button>}
          </div>
        </div>

        {tab === "active" && (
          evalsShown.length === 0 ? (
            <div className="q-empty"><Icon name="search" size={20} /><div>No active evaluations match “{query}”.</div><button className="btn btn-ghost xs focus-ring" onClick={()=>setQuery("")}>Clear search</button></div>
          ) : (
            <>
              <EvalActiveQueue evals={evalsPaged} activeIdx={activeIdx} onCancel={cancelEval} onMove={moveEval} onReorder={reorderEval} onOpen={setDrawerEv} sel={activeSel} reorderable={!q}/>
              {activeHasMore && <div ref={activePaging.sentinelRef} className="infinite-sentinel">Loading more…</div>}
            </>
          )
        )}

        {tab === "history" && (
          <>
            {historySel.size > 0 && (() => {
              const sel = HISTORY_EVALS.filter(e => historySel.has(e.id));
              const sameFlake = sel.length === 2 && sel[0].flake === sel[1].flake;
              const compareDisabled = !sameFlake;
              const compareTitle = sel.length !== 2
                ? "Select exactly 2 evaluations to compare"
                : !sameFlake
                  ? "Compare only works for two evaluations of the same flake"
                  : `Compare ${sel[0].commit.slice(0,7)} vs ${sel[1].commit.slice(0,7)}`;
              return (
                <div className="ed-bulkbar">
                  <span style={{ fontSize:13, fontWeight:600 }}>{historySel.size} selected</span>
                  <div style={{ flex:1 }}/>
                  <button className="btn btn-ghost focus-ring xs" onClick={()=>bulkAction("Re-evaluate")}><Icon name="sync" size={11}/> Re-evaluate</button>
                  <button
                    className="btn btn-ghost focus-ring xs"
                    onClick={()=>bulkAction("Compare")}
                    disabled={compareDisabled}
                    title={compareTitle}
                    style={compareDisabled ? { opacity:0.4, cursor:"not-allowed" } : null}
                  >
                    Compare
                  </button>
                  <button className="btn btn-ghost focus-ring xs" onClick={()=>bulkAction("Download logs for")}><Icon name="download" size={11}/> Download logs</button>
                  <button className="btn-icon focus-ring" onClick={historySel.clear} title="Clear"><Icon name="x" size={14}/></button>
                </div>
              );
            })()}
            <EvalHistory
              entries={historyPaged}
              activeIdx={activeIdx}
              filterStatus={filterStatus} setFilterStatus={setFilterStatus}
              filterFlake={filterFlake}   setFilterFlake={setFilterFlake}
              sel={historySel} onSelectAll={selectAll}
              onOpen={setDrawerEv}
              flashFailed={flashHistRows}
              onRowAction={(label, ev)=>{ setToast({ msg:`${label} ${ev.flake} · ${ev.commit.slice(0,7)}`, action:null }); setTimeout(()=>setToast(null), 3000); }}
              hasMore={histHasMore}
              sentinelRef={histPaging.sentinelRef}
              totalCount={historyFiltered.length}
            />
          </>
        )}
      </div>

      {drawerEv && <EvalDrawer ev={drawerEv} onClose={()=>setDrawerEv(null)} onCancel={cancelEval} onOpenSystem={onOpenSystem} onOpenPolicy={onOpenPolicy}/>}

      <BulkBar count={activeSel.size} onClear={activeSel.clear}>
        <button className="btn btn-danger xs focus-ring" onClick={bulkCancel}>
          <Icon name="x" size={12} /> Cancel {activeSel.size} eval{activeSel.size===1?"":"s"}
        </button>
      </BulkBar>

      {toast && (
        <div className="ed-toast">
          <Icon name="check" size={14} style={{ color:"#34d399" }}/>
          <span>{toast.msg}</span>
          {toast.action && <button onClick={toast.action}>Undo</button>}
        </div>
      )}

      {!drawerEv && (
        <div className="ed-kbd-hint">
          <span><kbd>j</kbd><kbd>k</kbd> navigate</span>
          <span><kbd>↵</kbd> open</span>
          {tab === "active" && <span><kbd>c</kbd> cancel</span>}
        </div>
      )}
    </div>
  );
}

/* ── Active queue ───────────────────────────────────── */
function EvalActiveQueue({ evals, activeIdx, onCancel, onMove, onReorder, onOpen, sel, reorderable = true }) {
  const [dragId, setDragId] = React.useState(null);
  const [overIdx, setOverIdx] = React.useState(null);
  if (evals.length === 0) {
    return <div className="empty" style={{ margin:24 }}><h3>No active evaluations</h3><div>All flake evaluations are complete.</div></div>;
  }
  const cancellableIds = sel ? evals.filter(e => e.canCancel).map(e => e.id) : [];
  const dragIdx = dragId ? evals.findIndex(e => e.id === dragId) : -1;
  return (
    <table className="sys-table q-queue-table">
      <thead>
        <tr>
          <th style={{ width:64 }}>#</th>
          <th>Flake · commit</th>
          <th>Branch</th>
          <th>Status</th>
          <th>Systems</th>
          <th>Policy</th>
          <th>Started</th>
          <th style={{ textAlign:"right" }}>Reorder · actions</th>
        </tr>
      </thead>
      <tbody>
        {evals.map((ev, i) => {
          const checked = sel && sel.has(ev.id);
          const isDragging = dragId === ev.id;
          const showDropBefore = dragId && overIdx === i && dragIdx > i;
          const showDropAfter  = dragId && overIdx === i && dragIdx < i;
          return (
          <tr key={ev.id}
            draggable={reorderable}
            onDragStart={reorderable ? (e)=>{ setDragId(ev.id); e.dataTransfer.effectAllowed="move"; try{ e.dataTransfer.setData("text/plain", ev.id); }catch{} } : undefined}
            onDragOver={reorderable ? (e)=>{ e.preventDefault(); e.dataTransfer.dropEffect="move"; if (overIdx!==i) setOverIdx(i); } : undefined}
            onDrop={reorderable ? (e)=>{ e.preventDefault(); if (dragId) onReorder(dragId, i); setDragId(null); setOverIdx(null); } : undefined}
            onDragEnd={reorderable ? ()=>{ setDragId(null); setOverIdx(null); } : undefined}
            className={`selectable q-row ${i===activeIdx?"selected":""}${checked?" row-checked":""}${isDragging?" q-dragging":""}${showDropBefore?" q-drop-before":""}${showDropAfter?" q-drop-after":""}`}
            onMouseDown={sel ? (e)=>{ if(e.shiftKey) e.preventDefault(); } : undefined}
            onClick={(e)=>{ if (sel && sel.handleClick(e, ev.id, cancellableIds)) return; if (sel) sel.setAnchor(ev.id); onOpen(ev); }}
            style={{ cursor:"pointer" }}>
            <td onClick={e=>e.stopPropagation()}>
              <div style={{ display:"flex", alignItems:"center", gap:6 }}>
                <span className="q-drag-handle" title="Drag to reorder"><Icon name="grip" size={15}/></span>
                <span style={{ color:"var(--cf-text-muted)", fontSize:12, fontVariantNumeric:"tabular-nums" }}>{ev.queuePos}</span>
              </div>
            </td>
            <td>
              <div style={{ fontWeight:600, fontSize:13, display:"flex", alignItems:"center", gap:6 }}><Icon name="git" size={12} style={{ color:"var(--cf-text-muted)" }}/>{ev.flake}</div>
              <div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{ev.commit}</div>
            </td>
            <td><span className="chip chip-unknown">{ev.branch}</span></td>
            <td><span className={`chip ${ev.meta.cls}`}><span className="chip-dot" style={{ background:ev.meta.color }} />{ev.meta.label}</span></td>
            <td style={{ fontSize:12, color:"var(--cf-text-secondary)" }}>{ev.systemCount} hosts</td>
            <td>
              <div style={{ display:"flex", gap:6 }}>
                <span className="chip chip-healthy">{ev.policyPass} ✓</span>
                {ev.policyFail > 0 && <span className="chip chip-critical">{ev.policyFail} ✗</span>}
              </div>
            </td>
            <td style={{ fontSize:12, color:"var(--cf-text-muted)" }}>{ev.startedAt}</td>
            <td onClick={e=>e.stopPropagation()}>
              <div className="row-actions" style={{ opacity:1, gap:6, justifyContent:"flex-end" }}>
                <div className="q-move-group">
                  <button className="q-move-btn focus-ring" title="Move up" disabled={i===0} onClick={()=>onMove(ev.id,-1)}><Icon name="chevron-up" size={15}/></button>
                  <button className="q-move-btn focus-ring" title="Move down" disabled={i===evals.length-1} onClick={()=>onMove(ev.id,1)}><Icon name="chevron-down" size={15}/></button>
                </div>
                {ev.canCancel && <button className="btn btn-ghost focus-ring" style={{ padding:"3px 8px", fontSize:11 }} onClick={()=>onCancel(ev.id, false)}>Cancel</button>}
              </div>
            </td>
          </tr>
          );
        })}
      </tbody>
    </table>
  );
}

/* ── History ────────────────────────────────────────── */
function EvalHistory({ entries, activeIdx, filterStatus, setFilterStatus, filterFlake, setFilterFlake, sel, onSelectAll, onOpen, onRowAction, flashFailed, hasMore, sentinelRef, totalCount }) {
  const ids = entries.map(e => e.id);
  const allChecked = entries.length > 0 && entries.every(e => sel.has(e.id));
  return (
    <>
      <div style={{ padding:"12px 16px", borderBottom:"1px solid var(--cf-divider)", display:"flex", gap:10, flexWrap:"wrap", alignItems:"center" }}>
        <div className="seg">
          {["all","complete","failed","cancelled"].map(k => (
            <button key={k} className={filterStatus===k?"active":""} onClick={()=>setFilterStatus(k)}>{k}</button>
          ))}
        </div>
        <select className="input filter-select focus-ring" style={{ width:"auto" }} value={filterFlake} onChange={e=>setFilterFlake(e.target.value)}>
          <option value="all">All flakes</option>
          {EVAL_FLAKES.map(f => <option key={f} value={f}>{f}</option>)}
        </select>
        <span className="filter-count">{typeof totalCount === "number" ? totalCount : entries.length} entries</span>
      </div>
      {entries.length === 0 ? (
        <div className="q-empty"><Icon name="search" size={20} /><div>No evaluations match these filters.</div></div>
      ) : (
      <>
      <table className="sys-table">
        <thead>
          <tr>
            <th>Flake · commit</th>
            <th>Branch</th>
            <th>Status</th>
            <th>Systems</th>
            <th>Policy</th>
            <th>Duration</th>
            <th>Completed</th>
            <th style={{ textAlign:"right" }}> </th>
          </tr>
        </thead>
        <tbody>
          {entries.map((ev, i) => {
            const checked = sel.has(ev.id);
            return (
            <tr key={ev.id} className={`selectable${i===activeIdx?" selected":""}${checked?" row-checked":""}${flashFailed && ev.status==="failed"?" attention-flash":""}`}
              onMouseDown={(e)=>{ if(e.shiftKey) e.preventDefault(); }}
              onClick={(e)=>{ if (sel.handleClick(e, ev.id, ids)) return; sel.setAnchor(ev.id); onOpen(ev); }}
              style={{ cursor:"pointer" }}>
              <td><div style={{ fontWeight:600, fontSize:13, display:"flex", alignItems:"center", gap:6 }}><Icon name="git" size={12} style={{ color:"var(--cf-text-muted)" }}/>{ev.flake}</div><div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{ev.commit}</div></td>
              <td><span className="chip chip-unknown">{ev.branch}</span></td>
              <td><span className={`chip ${ev.meta.cls}`}><span className="chip-dot" style={{ background:ev.meta.color }} />{ev.meta.label}</span></td>
              <td style={{ fontSize:12 }}>{ev.systemCount}</td>
              <td>
                <div style={{ display:"flex", gap:6 }}>
                  <span className="chip chip-healthy" style={{ fontSize:10 }}>{ev.policyPass} ✓</span>
                  {ev.policyFail > 0 && <span className="chip chip-critical" style={{ fontSize:10 }}>{ev.policyFail} ✗</span>}
                </div>
              </td>
              <td className="mono" style={{ fontSize:12, color:"var(--cf-text-secondary)" }}>{ev.dur || "—"}</td>
              <td style={{ fontSize:12, color:"var(--cf-text-muted)" }}>{ev.completedAt}</td>
              <td onClick={e=>e.stopPropagation()}>
                <div className="row-actions">
                  <button className="btn-icon focus-ring" title="View logs" onClick={e=>{e.stopPropagation();onOpen(ev);}}><Icon name="terminal" size={14} /></button>
                  <button className="btn-icon focus-ring" title="Re-evaluate" onClick={e=>{e.stopPropagation();onRowAction("Re-evaluating",ev);}}><Icon name="sync" size={14} /></button>
                  {ev.status === "failed" && (
                    <button className="btn-icon focus-ring" title="Retry evaluation" onClick={e=>{e.stopPropagation();onRowAction("Retrying",ev);}}><Icon name="rollback" size={14} /></button>
                  )}
                  <button className="btn-icon focus-ring" title="Download logs" onClick={e=>{e.stopPropagation();onRowAction("Downloading logs for",ev);}}><Icon name="download" size={14} /></button>
                </div>
              </td>
            </tr>
            );
          })}
        </tbody>
      </table>
      {hasMore && <div ref={sentinelRef} className="infinite-sentinel">Loading more…</div>}
      </>
      )}
    </>
  );
}

Object.assign(window, { EvalsView });
