// Flakes view — registry table/cards + side-tray commit explorer

function FlakesView({ defaultView, focus, onClearFocus, onOpenEval, onOpenBuild, onOpenSystems }) {
  const [viewMode, setViewMode] = React.useState(defaultView || "table");
  React.useEffect(() => { if (defaultView) setViewMode(defaultView); }, [defaultView]);
  const [query, setQuery]       = React.useState("");
  const [trayFlake, setTrayFlake] = React.useState(null);
  const [focusSha, setFocusSha] = React.useState(null);
  const [addOpen, setAddOpen]   = React.useState(false);
  const [editFlake, setEditFlake] = React.useState(null);
  const flashError = useAttentionFlash("flakes", FLAKE_REGISTRY.some(f => f.status === "error"));

  const flakes = FLAKE_REGISTRY.filter(f =>
    !query ||
    f.name.toLowerCase().includes(query.toLowerCase()) ||
    f.description.toLowerCase().includes(query.toLowerCase())
  );

  // Deep-link from a system's deployment history: open the matching flake's commit
  // explorer focused on the deployed commit.
  React.useEffect(() => {
    if (!focus) return;
    const f = FLAKE_REGISTRY.find(x => x.name === focus.flake)
           || FLAKE_REGISTRY.find(x => (FLAKE_COMMITS[x.id] || []).some(c => c.sha === focus.sha))
           || FLAKE_REGISTRY[0];
    setTrayFlake(f);
    setFocusSha(focus.capture ? null : (focus.sha || null));
    onClearFocus?.();
  }, [focus]);

  React.useEffect(() => {
    if (!trayFlake) return;
    const onKey = (e) => { if (e.key === "Escape") setTrayFlake(null); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [trayFlake]);

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      {/* Page head */}
      <div className="page-head">
        <div>
          <h1 className="page-title">Flakes</h1>
          <p className="page-subtitle">
            {FLAKE_REGISTRY.length} tracked · {FLAKE_REGISTRY.reduce((a,f)=>a+f.systemCount,0)} systems ·{" "}
            {FLAKE_REGISTRY.filter(f=>f.status==="synced").length} synced
          </p>
        </div>
        <div style={{ display:"flex", gap:8 }}>
          <button className="btn btn-ghost focus-ring"><Icon name="sync" size={14}/> Sync all</button>
          <button className="btn btn-primary focus-ring" data-coach-target="flake" onClick={()=>setAddOpen(true)}>
            <Icon name="plus" size={14}/> Add flake
          </button>
        </div>
      </div>

      {/* Filter bar */}
      <div className="filterbar">
        <div className="filter-search">
          <Icon name="search"/>
          <input className="input focus-ring" placeholder="Search flakes…" value={query} onChange={e=>setQuery(e.target.value)} />
        </div>
        <div className="seg">
          <button className={viewMode==="table"?"active":""} onClick={()=>setViewMode("table")}><Icon name="rows" size={12}/> Table</button>
          <button className={viewMode==="cards"?"active":""} onClick={()=>setViewMode("cards")}><Icon name="grid" size={12}/> Cards</button>
        </div>
        <span className="filter-count">{flakes.length} flakes</span>
      </div>

      {viewMode === "table"
        ? <FlakeTable flakes={flakes} selected={trayFlake} onSelect={setTrayFlake} onEdit={setEditFlake} flashError={flashError}/>
        : <FlakeCards flakes={flakes} selected={trayFlake} onSelect={setTrayFlake} onEdit={setEditFlake} flashError={flashError}/>
      }

      {/* Side tray */}
      {trayFlake && <FlakeTray flake={trayFlake} focusSha={focusSha} onClose={() => { setTrayFlake(null); setFocusSha(null); }} onEdit={() => { setEditFlake(trayFlake); }} onOpenEval={onOpenEval} onOpenBuild={onOpenBuild} onOpenSystems={onOpenSystems} />}

      {addOpen && <FlakeFormModal mode="add" onClose={()=>setAddOpen(false)}/>}
      {editFlake && <FlakeFormModal mode="edit" flake={editFlake} onClose={()=>setEditFlake(null)}/>}
    </div>
  );
}

/* ── Side tray: history + diff ─────────────────────────────────────── */
function FlakeTray({ flake, focusSha, focusMeta, onClose, onEdit, onOpenEval, onOpenBuild, onOpenSystems }) {
  const commits = FLAKE_COMMITS[flake.id] || [];
  // If the deep-linked commit isn't in the tracked list, synthesize a stub so the
  // tray can still focus it (e.g. a short sha referenced from a deployment) — using
  // whatever real message/author/time the caller already knew about that commit.
  const allCommits = React.useMemo(() => {
    if (focusSha && !commits.some(c => c.sha === focusSha)) {
      return [{ sha: focusSha, msg: focusMeta?.msg || "(deployed commit)", author: focusMeta?.author || "—", at: focusMeta?.at || "deployed", files: 0, add: 0, del: 0, synthetic: true }, ...commits];
    }
    return commits;
  }, [flake.id, focusSha]);
  const [selCommit, setSelCommit] = React.useState(
    (focusSha && allCommits.find(c => c.sha === focusSha)) || allCommits[0] || null
  );
  React.useEffect(() => {
    if (focusSha) { const c = allCommits.find(x => x.sha === focusSha); if (c) setSelCommit(c); }
  }, [focusSha]);
  const [selFile, setSelFile]     = React.useState(null);
  const [commitQuery, setCommitQuery] = React.useState("");

  const filteredCommits = React.useMemo(() => {
    if (!commitQuery) return allCommits;
    const q = commitQuery.toLowerCase();
    return allCommits.filter(c =>
      c.msg.toLowerCase().includes(q) ||
      c.sha.toLowerCase().includes(q) ||
      c.author.toLowerCase().includes(q)
    );
  }, [commitQuery, allCommits]);

  const commitGroups = React.useMemo(() => {
    // Bucket by relative time string parsed loosely
    const groups = { "Deployed": [], "Today": [], "This week": [], "Earlier": [] };
    filteredCommits.forEach(c => {
      const t = c.at.toLowerCase();
      if (c.synthetic) groups["Deployed"].push(c);
      else if (t.includes("h ago") || t.includes("now") || t.includes("min ago")) groups["Today"].push(c);
      else if (/^([1-6])d ago/.test(t)) groups["This week"].push(c);
      else groups["Earlier"].push(c);
    });
    // Drop empty groups
    return Object.fromEntries(Object.entries(groups).filter(([_, v]) => v.length > 0));
  }, [filteredCommits]);

  const commitFiles = React.useMemo(
    () => selCommit ? flakeCommitFiles(selCommit.sha, selCommit.files) : [],
    [selCommit?.sha]
  );

  React.useEffect(() => {
    setSelFile(null); // close any open diff when switching commits
  }, [selCommit?.sha]);

  const idx = allCommits.findIndex(c => c.sha === selCommit?.sha);
  const pipe = COMMIT_PIPELINE_STATUS[idx >= 0 ? idx % COMMIT_PIPELINE_STATUS.length : 0];

  // Rollout fraction: pretend system count tied to flake's systemCount, vary by index
  const rolloutTotal = flake.systemCount;
  const rolloutOn = idx === 0 ? rolloutTotal : Math.max(0, rolloutTotal - (idx * 2));

  return (
    <>
      <div className="fl-tray-backdrop" onClick={onClose}/>
      <aside className="fl-tray" role="dialog" aria-label={`${flake.name} commits`}>
        {/* Header */}
        <header className="fl-tray-head">
          <div style={{ display:"flex", alignItems:"center", gap:10, minWidth:0, flex:1 }}>
            <Icon name="git" size={18} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
            <div style={{ minWidth:0 }}>
              <div style={{ display:"flex", alignItems:"center", gap:8 }}>
                <span style={{ fontWeight:700, fontSize:15 }}>{flake.name}</span>
                <span className="chip chip-unknown" style={{ fontSize:10 }}>{flake.branch}</span>
                <FlakeSyncChip f={flake}/>
              </div>
              <div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)", overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{flake.url}</div>
            </div>
          </div>
          <div style={{ display:"flex", gap:6, alignItems:"center" }}>
            <button className="btn btn-ghost focus-ring xs"><Icon name="sync" size={11}/> Sync</button>
            <button className="btn btn-ghost focus-ring xs" onClick={onEdit}><Icon name="gear" size={11}/> Edit</button>
            <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={16}/></button>
          </div>
        </header>

        {/* Sync error banner — surfaces WHY the flake failed to sync */}
        {flake.status === "error" && (
          <div className="fl-sync-error">
            <div className="fl-sync-error-head">
              <Icon name="warn" size={14}/>
              <span>Sync failed</span>
              <span className="fl-sync-error-when">{flake.lastSyncAt}</span>
              <span style={{ flex:1 }}/>
              <button className="btn btn-ghost focus-ring xs"><Icon name="sync" size={11}/> Retry sync</button>
            </div>
            <pre className="fl-sync-error-msg mono">$ nix flake metadata {flake.url}{"\n"}error: {flake.errorMsg || "unknown error"}</pre>
            <div className="fl-sync-error-meta">
              <span><span style={{ color:"var(--cf-text-muted)" }}>last good commit</span> <span className="mono">{flake.latestCommit}</span></span>
              <span><span style={{ color:"var(--cf-text-muted)" }}>remote</span> <span className="mono">{flake.url}</span></span>
            </div>
          </div>
        )}

        {/* Body: 2-pane — commit list (left) / detail (right) */}
        <div className="fl-tray-body">
          {/* Commit list */}
          <nav className="fl-tray-commits">
            <div className="fl-tray-commits-search">
              <Icon name="search" size={12} style={{ color:"var(--cf-text-muted)", flexShrink:0 }}/>
              <input
                className="input focus-ring"
                placeholder="Filter commits…"
                value={commitQuery}
                onChange={e => setCommitQuery(e.target.value)}
                style={{ background:"transparent", border:"none", padding:"4px 0", fontSize:12, flex:1 }}
              />
              <span style={{ fontSize:10, color:"var(--cf-text-muted)" }}>{filteredCommits.length}/{allCommits.length}</span>
            </div>
            {Object.entries(commitGroups).map(([bucket, list], gi) => (
              <div key={bucket}>
                <div className="fl-commits-bucket">{bucket}</div>
                {list.map((c, i) => {
                  const isSel = selCommit?.sha === c.sha;
                  const ci = allCommits.findIndex(x => x.sha === c.sha);
                  const p = COMMIT_PIPELINE_STATUS[ci % COMMIT_PIPELINE_STATUS.length];
                  const isLastInBucket = i === list.length - 1;
                  const isLastBucket = gi === Object.keys(commitGroups).length - 1;
                  return (
                    <div key={c.sha}
                      className={`fl-commit-item${isSel?" active":""}`}
                      onClick={()=>setSelCommit(c)}
                    >
                      <div className="fl-rail">
                        <div className={`fl-dot${isSel?" sel":""}`}/>
                        {!(isLastInBucket && isLastBucket) && <div className="fl-stem"/>}
                      </div>
                      <div style={{ minWidth:0, flex:1 }}>
                        <div style={{ display:"flex", alignItems:"baseline", gap:6 }}>
                          <span className="mono" style={{ fontSize:11, fontWeight:700, color:isSel?"var(--cf-brand-purple)":"var(--cf-text-primary)" }}>{c.sha}</span>
                          <span style={{ fontSize:11, color:"var(--cf-text-muted)", marginLeft:"auto" }}>{c.at}</span>
                        </div>
                        <div className="truncate" style={{ fontSize:12, marginTop:3, color:"var(--cf-text-primary)" }}>{c.msg}</div>
                        <div style={{ display:"flex", gap:5, marginTop:6, flexWrap:"wrap" }}>
                          <PipelineDot kind="eval"  val={p.eval}/>
                          <PipelineDot kind="build" val={p.build}/>
                          <span className="mono" style={{ fontSize:10, color:"var(--cf-text-muted)", marginLeft:"auto" }}>{c.author}</span>
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
            ))}
            {filteredCommits.length === 0 && (
              <div className="empty" style={{ margin:24 }}>No commits match.</div>
            )}
          </nav>

          {/* Detail pane */}
          <section className="fl-tray-detail">
            {selCommit ? (
              <>
                {/* Commit header */}
                <div className="fl-tray-commit-h">
                  <div style={{ display:"flex", alignItems:"baseline", gap:10, flexWrap:"wrap" }}>
                    <span className="mono" style={{ fontSize:14, fontWeight:700, color:"var(--cf-brand-purple)" }}>{selCommit.sha}</span>
                    <span style={{ fontSize:14, fontWeight:600 }}>{selCommit.msg}</span>
                  </div>
                  <div style={{ display:"flex", gap:12, marginTop:6, fontSize:11, color:"var(--cf-text-muted)", flexWrap:"wrap" }}>
                    <span><Icon name="user" size={11}/> <span className="mono">{selCommit.author}</span></span>
                    <span>{selCommit.at}</span>
                    <span style={{ color:"#34d399" }}>+{selCommit.add}</span>
                    <span style={{ color:"#f87171" }}>-{selCommit.del}</span>
                    <span>{selCommit.files} files</span>
                  </div>

                  {/* Pipeline strip — eval / build / rollout */}
                  <div className="fl-pipeline">
                    <PipelinePill stage="eval"  val={pipe.eval}  onClick={() => onOpenEval?.({ sha: selCommit.sha, msg: selCommit.msg, flake: flake.name, author: selCommit.author, at: selCommit.at, status: pipe.eval==="failed"?"failed":pipe.eval==="complete"?"complete":"in_progress" })}/>
                    <PipelineArrow/>
                    <PipelinePill stage="build" val={pipe.build} onClick={() => onOpenBuild?.({ sha: selCommit.sha, msg: selCommit.msg, flake: flake.name, author: selCommit.author, at: selCommit.at, status: pipe.build==="failed"?"failed":pipe.build })}/>
                    <PipelineArrow/>
                    <RolloutPill on={rolloutOn} total={rolloutTotal} failed={pipe.eval==="failed" || pipe.build==="failed" ? 0 : 0} onClick={() => onOpenSystems?.(flake.name)}/>
                  </div>
                </div>

                {/* Files changed — full-width grid, click opens DiffModal */}
                <div className="fl-files-section">
                  <div className="fl-tray-section-h">
                    <span>{commitFiles.length} files changed · click to view diff</span>
                    <span style={{ color:"var(--cf-text-muted)", fontWeight:400, fontSize:10 }}>
                      <span style={{ color:"#34d399" }}>+{selCommit.add}</span>{" / "}
                      <span style={{ color:"#f87171" }}>-{selCommit.del}</span>
                    </span>
                  </div>
                  <div className="fl-files-grid">
                    {commitFiles.map((f, i) => {
                      const total = f.add + f.del + 0.001;
                      return (
                        <button key={f.name}
                          className="fl-file-card focus-ring"
                          onClick={()=>setSelFile(f)}
                        >
                          <div className="fl-file-card-head">
                            <Icon name="file" size={13} style={{ opacity:0.55, flexShrink:0 }}/>
                            <div style={{ minWidth:0, flex:1 }}>
                              <div className="fl-file-name truncate" title={f.name}>{f.name.split("/").pop()}</div>
                              <div className="fl-file-path truncate" title={f.name}>{f.name.split("/").slice(0,-1).join("/") || "."}</div>
                            </div>
                          </div>
                          <div className="fl-file-stats">
                            <span className="mono" style={{ fontSize:11, color:"#34d399" }}>+{f.add}</span>
                            <span className="mono" style={{ fontSize:11, color:"#f87171" }}>-{f.del}</span>
                            <div className="fl-file-bar">
                              <div style={{ width:`${Math.round(f.add/total*100)}%`, height:"100%", background:"#34d399", display:"inline-block", verticalAlign:"top" }}/>
                              <div style={{ width:`${Math.round(f.del/total*100)}%`, height:"100%", background:"#f87171", display:"inline-block", verticalAlign:"top" }}/>
                            </div>
                          </div>
                        </button>
                      );
                    })}
                  </div>
                </div>
              </>
            ) : (
              <div className="empty" style={{ margin:32 }}>No commits yet for this flake.</div>
            )}
          </section>
        </div>
      </aside>
      {selFile && selCommit && (
        <DiffModal file={selFile} commit={selCommit} flake={flake} onClose={()=>setSelFile(null)} />
      )}
    </>
  );
}

/* Diff modal — gitlab/github-style file diff viewer */
function DiffModal({ file, commit, flake, onClose }) {
  const bodyRef = React.useRef(null);
  const hunkRefs = React.useRef([]);
  const [activeHunk, setActiveHunk] = React.useState(0);
  const [wrap, setWrap] = React.useState(false);

  React.useEffect(() => {
    const onKey = (e) => {
      if (e.key === "Escape") onClose();
      if (e.key === "j" || e.key === "n") { e.preventDefault(); jumpHunk(1); }
      if (e.key === "k" || e.key === "p") { e.preventDefault(); jumpHunk(-1); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const lines = flakeFileDiff(file).split("\n");

  // Compute line numbers + hunk index per row
  let oldNo = 0, newNo = 0, hIdx = -1;
  const annotated = lines.map((line) => {
    if (line.startsWith("@@")) {
      const m = line.match(/-(\d+)(?:,\d+)?\s+\+(\d+)/);
      if (m) { oldNo = parseInt(m[1]) - 1; newNo = parseInt(m[2]) - 1; }
      hIdx++;
      return { type: "hunk", text: line, oldNo: null, newNo: null, h: hIdx };
    }
    if (line.startsWith("+++") || line.startsWith("---")) return { type: "meta", text: line };
    if (line.startsWith("+")) { newNo++; return { type: "add", text: line, oldNo: null, newNo, h: hIdx }; }
    if (line.startsWith("-")) { oldNo++; return { type: "del", text: line, oldNo, newNo: null, h: hIdx }; }
    oldNo++; newNo++; return { type: "ctx", text: line, oldNo, newNo, h: hIdx };
  });

  const hunks = annotated.filter(r => r.type === "hunk");
  const totalAdd = annotated.filter(r => r.type === "add").length;
  const totalDel = annotated.filter(r => r.type === "del").length;

  const jumpHunk = (dir) => {
    const next = Math.max(0, Math.min(hunks.length - 1, activeHunk + dir));
    setActiveHunk(next);
    hunkRefs.current[next]?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  // Track which hunk is currently in view
  React.useEffect(() => {
    const body = bodyRef.current;
    if (!body) return;
    const onScroll = () => {
      const top = body.scrollTop + 8;
      let cur = 0;
      hunkRefs.current.forEach((el, i) => {
        if (el && el.offsetTop <= top) cur = i;
      });
      setActiveHunk(cur);
    };
    body.addEventListener("scroll", onScroll, { passive: true });
    return () => body.removeEventListener("scroll", onScroll);
  }, []);

  return (
    <div className="modal-backdrop" onClick={onClose} style={{ zIndex: 90 }}>
      <div className="diff-modal" onClick={e=>e.stopPropagation()}>
        <header className="diff-modal-head">
          <div style={{ minWidth:0, flex:1 }}>
            <div style={{ display:"flex", alignItems:"center", gap:8, fontSize:11, color:"var(--cf-text-muted)" }}>
              <Icon name="git" size={11}/>
              <span className="mono">{flake.name}</span>
              <span>·</span>
              <span className="mono">{commit.sha}</span>
              <span style={{ overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{commit.msg}</span>
            </div>
            <div style={{ display:"flex", alignItems:"center", gap:10, marginTop:4, flexWrap:"wrap" }}>
              <Icon name="file" size={13} style={{ opacity:0.6 }}/>
              <span className="mono" style={{ fontSize:13, fontWeight:600 }}>{file.name}</span>
              <span className="chip chip-healthy" style={{ fontSize:10 }}>+{totalAdd}</span>
              <span className="chip chip-critical" style={{ fontSize:10 }}>-{totalDel}</span>
              <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>· {hunks.length} hunk{hunks.length===1?"":"s"} · {annotated.filter(r=>r.type!=="meta").length} lines</span>
            </div>
          </div>
          <div style={{ display:"flex", gap:6, alignItems:"center" }}>
            {hunks.length > 1 && (
              <div className="diff-hunk-nav">
                <button className="btn-icon focus-ring" title="Previous hunk (k)" onClick={()=>jumpHunk(-1)} disabled={activeHunk===0}>
                  <Icon name="chevron-up" size={13}/>
                </button>
                <span className="mono" style={{ fontSize:11, color:"var(--cf-text-secondary)", padding:"0 6px" }}>{activeHunk+1}/{hunks.length}</span>
                <button className="btn-icon focus-ring" title="Next hunk (j)" onClick={()=>jumpHunk(1)} disabled={activeHunk===hunks.length-1}>
                  <Icon name="chevron-down" size={13}/>
                </button>
              </div>
            )}
            <button className={`btn-icon focus-ring${wrap?" active":""}`} title={wrap?"Disable line wrap":"Wrap long lines"} onClick={()=>setWrap(w=>!w)}>
              <Icon name="rows" size={14}/>
            </button>
            <button className="btn-icon focus-ring" title="Copy path"><Icon name="link" size={14}/></button>
            <button className="btn-icon focus-ring" title="Close (Esc)" onClick={onClose}><Icon name="x" size={16}/></button>
          </div>
        </header>
        <div className="diff-modal-body" ref={bodyRef}>
          <table className={`diff-table${wrap?" wrap":""}`}>
            <tbody>
              {annotated.map((row, i) => {
                if (row.type === "meta") return null;
                if (row.type === "hunk") return (
                  <tr key={i} className="diff-hunk" ref={el => hunkRefs.current[row.h] = el}>
                    <td colSpan={3}>{row.text}</td>
                  </tr>
                );
                return (
                  <tr key={i} className={`diff-row diff-${row.type}`}>
                    <td className="diff-gutter mono">{row.oldNo ?? ""}</td>
                    <td className="diff-gutter mono">{row.newNo ?? ""}</td>
                    <td className="diff-code mono">{row.text}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

/* Tiny status dot — used in commit list */
function PipelineDot({ kind, val }) {
  if (!val) return null;
  const palette = {
    "complete":"#34d399", "cache-pushed":"#34d399", "up-to-date":"#34d399",
    "building":"#60a5fa", "pending":"#60a5fa", "in_progress":"#60a5fa",
    "failed":"#f87171",
    "behind":"#f59e0b",
  };
  const color = palette[val] || "#6b7280";
  const label = { eval:"E", build:"B" }[kind] || kind[0].toUpperCase();
  return (
    <span title={`${kind}: ${val}`} style={{
      display:"inline-flex", alignItems:"center", justifyContent:"center",
      width:14, height:14, borderRadius:4, fontSize:9, fontWeight:700,
      color, background:`color-mix(in oklab, ${color} 15%, transparent)`,
      fontFamily:"var(--font-mono)",
    }}>{label}</span>
  );
}

/* Bigger pill — used in commit detail header */
function PipelinePill({ stage, val, onClick }) {
  const map = {
    eval:    { complete:["chip-healthy","Eval ✓"], pending:["chip-info","Eval…"], failed:["chip-critical","Eval ✗"] },
    build:   { "cache-pushed":["chip-healthy","Cached"], complete:["chip-healthy","Built"], building:["chip-info","Building"], failed:["chip-critical","Build ✗"], pending:["chip-unknown","Queued"] },
  };
  const [cls, label] = map[stage]?.[val] || ["chip-unknown", String(val)];
  return <span className={`chip ${cls} focus-ring`} style={{ fontWeight:600, cursor: onClick ? "pointer" : undefined }} onClick={onClick} title={onClick ? `Open ${stage}` : undefined}>{label}</span>;
}

function PipelineArrow() {
  return <span style={{ color:"var(--cf-text-muted)", fontSize:11 }}>→</span>;
}

/* Rollout pill — replaces "deployed" with N/M systems on this commit */
function RolloutPill({ on, total, failed, onClick }) {
  const pct = total > 0 ? on / total : 0;
  const cls = failed > 0 ? "chip-critical" : pct === 1 ? "chip-healthy" : pct === 0 ? "chip-unknown" : "chip-warning";
  return (
    <span className={`chip ${cls} focus-ring`} style={{ display:"inline-flex", alignItems:"center", gap:6, fontWeight:600, cursor: onClick ? "pointer" : undefined }} onClick={onClick} title={onClick ? "Open in Systems" : undefined}>
      <Icon name="server" size={10}/>
      Rollout {on}/{total}
      <div style={{ width:32, height:3, background:"rgba(255,255,255,0.2)", borderRadius:99, overflow:"hidden" }}>
        <div style={{ width:`${pct*100}%`, height:"100%", background:"currentColor" }}/>
      </div>
    </span>
  );
}

function FlakeSyncChip({ f }) {
  const cfg = { synced:["chip-healthy","#34d399","synced"], syncing:["chip-info","#60a5fa","syncing"], error:["chip-critical","#f87171","error"] }[f.status] || ["chip-unknown","#6b7280",f.status];
  return <span className={`chip ${cfg[0]}`} title={f.errorMsg}><span className="chip-dot" style={{ background:cfg[1] }}/>{cfg[2]}</span>;
}

/* ── Flake table ──────────────────────────────────────────────────── */
function FlakeTable({ flakes, selected, onSelect, onEdit, flashError }) {
  return (
    <div className="card" style={{ overflow:"hidden" }}>
      <table className="sys-table">
        <thead>
          <tr>
            <th>Flake</th>
            <th>Status</th>
            <th>Branch</th>
            <th>Systems</th>
            <th>Environments</th>
            <th>Latest commit</th>
            <th>Author</th>
            <th>Synced</th>
            <th style={{ textAlign:"right" }}> </th>
          </tr>
        </thead>
        <tbody>
          {flakes.map(f => (
            <tr key={f.id} className={`${selected?.id===f.id?"selected":""}${flashError && f.status==="error"?" attention-flash":""}`} onClick={()=>onSelect(f)} style={{ cursor:"pointer" }}>
              <td>
                <div style={{ fontWeight:600, fontSize:13 }}>{f.name}</div>
                <div style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{f.description}</div>
              </td>
              <td><FlakeSyncChip f={f}/></td>
              <td><span className="chip chip-unknown">{f.branch}</span></td>
              <td style={{ fontSize:13 }}>{f.systemCount}</td>
              <td><FlakeEnvBadges flake={f} align="flex-start" max={3}/></td>
              <td>
                <span className="mono" style={{ fontSize:12, fontWeight:600 }}>{f.latestCommit}</span>
                <div style={{ fontSize:11, color:"var(--cf-text-muted)", maxWidth:260, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{f.latestMessage}</div>
              </td>
              <td className="mono" style={{ fontSize:12, color:"var(--cf-text-secondary)" }}>{f.latestAuthor}</td>
              <td style={{ fontSize:12, color:"var(--cf-text-muted)" }}>{f.lastSyncAt}</td>
              <td>
                <div className="row-actions">
                  <button className="btn-icon focus-ring" title="Sync" onClick={e=>e.stopPropagation()}><Icon name="sync" size={14}/></button>
                  <button className="btn-icon focus-ring" title="Edit flake" onClick={e=>{e.stopPropagation(); onEdit(f);}}><Icon name="gear" size={14}/></button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/* Environments a flake spans — derived from its systems, shown as a stack of badges. */
function FlakeEnvBadges({ flake, max = 4, align = "flex-end" }) {
  const envs = flakeEnvironments(flake);
  if (envs.length === 0) {
    return <span className="chip chip-unknown" style={{ fontSize:10 }}>no systems yet</span>;
  }
  const shown = envs.slice(0, max);
  const extra = envs.length - shown.length;
  return (
    <div style={{ display:"flex", alignItems:"center", gap:4, flexWrap:"wrap", justifyContent:align }}>
      {shown.map(e => <EnvBadge key={e} env={e}/>)}
      {extra > 0 && <span className="chip chip-unknown" style={{ fontSize:10 }} title={envs.slice(max).join(", ")}>+{extra}</span>}
    </div>
  );
}

/* ── Flake cards ──────────────────────────────────────────────────── */
function FlakeCards({ flakes, selected, onSelect, onEdit, flashError }) {
  return (
    <div className="cards-grid">
      {flakes.map(f => {
        const statusColor = { synced:"#34d399", syncing:"#60a5fa", error:"#f87171" }[f.status] || "#6b7280";
        return (
          <div key={f.id} className={`sys-card${flashError && f.status==="error"?" attention-flash":""}`} style={{ borderColor: selected?.id===f.id?"var(--cf-brand-purple)":undefined }} onClick={()=>onSelect(f)}>
            <div className="status-rail" style={{ "--status-color": statusColor }}/>
            <div className="sys-card-head">
              <div className="sys-title">
                <div className="sys-hostname"><Icon name="git" size={13}/>&nbsp;{f.name}</div>
                <div className="sys-fqdn">{f.url}</div>
              </div>
            </div>
            <div style={{ fontSize:12, color:"var(--cf-text-secondary)" }}>{f.description}</div>
            <div style={{ display:"flex", alignItems:"baseline", gap:8, flexWrap:"wrap" }}>
              <span style={{ fontSize:10, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", fontWeight:600, flexShrink:0 }}>Environments</span>
              <FlakeEnvBadges flake={f} max={6} align="flex-start"/>
            </div>
            <div className="sys-card-body">
              <div><div className="sys-kv-key">Branch</div><div className="sys-kv-val">{f.branch}</div></div>
              <div><div className="sys-kv-key">Systems</div><div className="sys-kv-val" style={{fontFamily:"inherit"}}>{f.systemCount}</div></div>
              <div><div className="sys-kv-key">Commit</div><div className="sys-kv-val">{f.latestCommit}</div></div>
              <div><div className="sys-kv-key">Synced</div><div className="sys-kv-val" style={{fontFamily:"inherit"}}>{f.lastSyncAt}</div></div>
            </div>
            {f.errorMsg && (
              <div className="sd-callout sd-callout-danger" style={{ padding:"8px 10px" }}>
                <Icon name="warn" size={12}/>
                <div style={{ fontSize:11 }}>{f.errorMsg}</div>
              </div>
            )}
            <div className="sys-card-foot">
              <div className="chips-row">
                <FlakeSyncChip f={f}/>
                <span className="chip chip-unknown">{f.totalCommits} commits</span>
              </div>
              <button className="btn btn-subtle focus-ring" style={{ padding:"4px 10px", fontSize:12 }} onClick={e=>{e.stopPropagation(); onEdit(f);}}>
                <Icon name="gear" size={12}/> Edit
              </button>
            </div>
          </div>
        );
      })}
    </div>
  );
}

/* ── Add / Edit modal ─────────────────────────────────────────────── */
function FlakeFormModal({ mode, flake, onClose }) {
  const isEdit = mode === "edit";
  const [form, setForm] = React.useState(() => isEdit ? {
    name: flake.name,
    url: flake.url,
    branch: flake.branch,
    description: flake.description || "",
    autoSync: true,
    syncInterval: "5m",
    credType: flake.url?.startsWith("https") ? "https" : "ssh",
    credId: "cred-default",
  } : {
    name: "",
    url: "",
    branch: "main",
    description: "",
    autoSync: true,
    syncInterval: "5m",
    credType: "ssh",
    credId: "cred-default",
  });
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const [testing, setTesting] = React.useState(null); // null | "running" | "ok" | "fail"

  const set = (k, v) => setForm(p => ({ ...p, [k]: v }));
  const test = () => {
    setTesting("running");
    setTimeout(()=>setTesting(Math.random() > 0.25 ? "ok" : "fail"), 900);
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(620px,96vw)", maxHeight:"92vh" }}>
        {confirmDelete ? (
          <DeleteFlakeConfirm flake={flake} onCancel={()=>setConfirmDelete(false)} onConfirm={onClose}/>
        ) : (
        <>
        <div className="modal-head">
          <h2>
            <Icon name={isEdit ? "gear" : "plus"} size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>
            {isEdit ? `Edit ${flake.name}` : "Add flake"}
          </h2>
          <p>{isEdit ? "Update flake registration. URL changes will trigger a re-clone." : "Register a new NixOS flake repository."}</p>
        </div>
        <div className="modal-body" style={{ overflowY:"auto" }}>
          <div className="field">
            <label>Name</label>
            <input className="input focus-ring" value={form.name} onChange={e=>set("name",e.target.value)} placeholder="e.g. infrastructure"/>
          </div>
          <div className="field">
            <label>Repository URL</label>
            <input className="input focus-ring mono" value={form.url} onChange={e=>set("url",e.target.value)} placeholder="git+ssh://git@gitlab.example.com/…" style={{ fontSize:12 }}/>
          </div>
          <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
            <div className="field">
              <label>Branch</label>
              <input className="input focus-ring" value={form.branch} onChange={e=>set("branch",e.target.value)}/>
            </div>
            <div className="field">
              <label>Environments</label>
              <div style={{ display:"flex", alignItems:"center", minHeight:34, gap:6, flexWrap:"wrap" }}>
                {isEdit
                  ? <FlakeEnvBadges flake={flake} align="flex-start" max={6}/>
                  : <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>Populated from its systems</span>}
              </div>
              <div className="help">Derived from the systems built off this flake — not assigned here.</div>
            </div>
          </div>
          <div className="field">
            <label>Description</label>
            <input className="input focus-ring" value={form.description} onChange={e=>set("description",e.target.value)} placeholder="Short description shown in the registry"/>
          </div>

          {/* Credentials section */}
          <div style={{ marginTop:8, padding:14, border:"1px solid var(--cf-divider)", borderRadius:10, background:"color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg))" }}>
            <div style={{ display:"flex", justifyContent:"space-between", alignItems:"center", marginBottom:10 }}>
              <div style={{ fontSize:13, fontWeight:600, display:"flex", alignItems:"center", gap:6 }}>
                <Icon name="key" size={13}/> Repository credentials
              </div>
              <button className="btn btn-ghost focus-ring xs" onClick={test} disabled={testing==="running" || form.credType==="none"}>
                {testing==="running" ? <><Spinner size={11}/> Testing…</>
                : testing==="ok"     ? <><Icon name="check" size={11} style={{color:"#34d399"}}/> Connected</>
                : testing==="fail"   ? <><Icon name="warn" size={11} style={{color:"#f87171"}}/> Failed</>
                : <>Test connection</>}
              </button>
            </div>

            <div className="seg" style={{ marginBottom:12 }}>
              {[
                { v:"none",  l:"None (public)" },
                { v:"ssh",   l:"SSH key" },
                { v:"https", l:"HTTPS token" },
              ].map(o => (
                <button key={o.v} className={form.credType===o.v?"active":""} onClick={()=>{ set("credType", o.v); setTesting(null); }}>{o.l}</button>
              ))}
            </div>

            {form.credType === "ssh"   && <SshCredPicker form={form} set={set}/>}
            {form.credType === "https" && <HttpsCredPicker form={form} set={set}/>}
            {form.credType === "none"  && (
              <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>
                No auth — works for anonymous HTTPS clones and read-only public repos.
              </div>
            )}
          </div>

          {/* Sync section */}
          <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
            <label style={{ display:"flex", gap:8, alignItems:"center", fontSize:13, cursor:"pointer" }}>
              <input type="checkbox" checked={form.autoSync} onChange={e=>set("autoSync",e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
              <span>Auto-sync</span>
            </label>
            <div className="field">
              <label>Sync interval</label>
              <select className="input focus-ring" value={form.syncInterval} onChange={e=>set("syncInterval",e.target.value)} disabled={!form.autoSync}>
                <option value="1m">Every 1 min</option>
                <option value="5m">Every 5 min</option>
                <option value="15m">Every 15 min</option>
                <option value="1h">Every hour</option>
              </select>
            </div>
          </div>

          {isEdit && (
            <div style={{ marginTop:10, paddingTop:14, borderTop:"1px solid var(--cf-divider)" }}>
              <div style={{ fontSize:11, fontWeight:600, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", marginBottom:8 }}>Danger zone</div>
              <button className="btn btn-ghost focus-ring" onClick={()=>setConfirmDelete(true)} style={{ color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }}>
                <Icon name="x" size={12}/> Remove flake from registry
              </button>
            </div>
          )}
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" onClick={onClose}>
            <Icon name="check" size={13}/> {isEdit ? "Save changes" : "Add flake"}
          </button>
        </div>
        </>
        )}
      </div>
    </div>
  );
}

/* ── Delete confirm — replaces the form when triggered ────── */
function DeleteFlakeConfirm({ flake, onCancel, onConfirm }) {
  const [typed, setTyped] = React.useState("");
  const matches = typed === flake.name;
  return (
    <>
      <div className="modal-head" style={{ background:"rgba(248,113,113,0.06)" }}>
        <h2 style={{ color:"#fecaca", display:"flex", alignItems:"center", gap:8 }}>
          <Icon name="warn" size={16} style={{ color:"#f87171" }}/>
          Remove flake from registry
        </h2>
        <p>This stops auto-sync for <span className="mono" style={{ fontWeight:600 }}>{flake.name}</span> and removes it from the registry.</p>
      </div>
      <div className="modal-body">
        <div className="sd-callout sd-callout-danger" style={{ flexDirection:"column", alignItems:"stretch" }}>
          <div style={{ display:"flex", gap:10, alignItems:"flex-start" }}>
            <Icon name="warn" size={14}/>
            <div style={{ fontSize:12 }}>
              <div style={{ fontWeight:600, color:"#fecaca", marginBottom:4 }}>What happens</div>
              <ul style={{ margin:0, paddingLeft:18, color:"var(--cf-text-secondary)", lineHeight:1.6 }}>
                <li>Auto-sync polling stops immediately</li>
                <li>{flake.systemCount} system{flake.systemCount === 1 ? "" : "s"} on this flake will need to be retargeted</li>
                <li>Tracked commits are retained for audit; build/eval history stays</li>
                <li>Repository credentials are <em>not</em> deleted</li>
              </ul>
            </div>
          </div>
        </div>
        <div className="field">
          <label>Type <span className="mono" style={{ color:"#fecaca", fontWeight:700 }}>{flake.name}</span> to confirm</label>
          <input
            className="input focus-ring mono"
            placeholder={flake.name}
            value={typed}
            onChange={e=>setTyped(e.target.value)}
            autoFocus
            style={{ borderColor: typed && !matches ? "rgba(248,113,113,0.5)" : undefined }}
          />
        </div>
      </div>
      <div className="modal-foot">
        <button className="btn btn-ghost focus-ring" onClick={onCancel}>Cancel</button>
        <button
          className="btn focus-ring"
          disabled={!matches}
          onClick={onConfirm}
          style={{ background: matches ? "#dc2626" : "var(--cf-subtle-bg)", color: matches ? "white" : "var(--cf-text-muted)" }}
        >
          <Icon name="x" size={13}/> Remove flake
        </button>
      </div>
    </>
  );
}

/* ── Credential pickers ───────────────────────────────────────────── */
const SAVED_SSH_KEYS = [
  { id: "cred-default",   name: "id_ed25519_cf",      fingerprint: "SHA256:Hxk2…JdmA", added: "3 mo ago", lastUsed: "2m ago" },
  { id: "cred-gitlab",    name: "id_ed25519_gitlab",  fingerprint: "SHA256:9Lp1…7vQz", added: "1 mo ago", lastUsed: "1d ago" },
  { id: "cred-rsa-legacy",name: "id_rsa_legacy",      fingerprint: "SHA256:4Tn8…aBcd", added: "8 mo ago", lastUsed: "never", deprecated: true },
];
const SAVED_HTTPS_TOKENS = [
  { id: "tok-gitlab",     name: "gitlab-ops",         user: "ops-bot", scope: "read_repository", masked: "glpat-•••••••••••a3F2", added: "2 mo ago" },
  { id: "tok-github",     name: "github-readonly",    user: "deploy",  scope: "repo:read",        masked: "ghp_•••••••••••mK91",  added: "5 mo ago" },
];

function SshCredPicker({ form, set }) {
  const [adding, setAdding] = React.useState(false);
  const [newKey, setNewKey] = React.useState({ name:"", body:"" });
  const sel = SAVED_SSH_KEYS.find(k => k.id === form.credId);

  if (adding) return (
    <div style={{ display:"flex", flexDirection:"column", gap:10 }}>
      <div className="field">
        <label>Key name</label>
        <input className="input focus-ring" placeholder="e.g. id_ed25519_prod" value={newKey.name} onChange={e=>setNewKey({...newKey, name:e.target.value})}/>
      </div>
      <div className="field">
        <label>Private key</label>
        <textarea className="input focus-ring mono" rows={5} placeholder="-----BEGIN OPENSSH PRIVATE KEY-----&#10;…&#10;-----END OPENSSH PRIVATE KEY-----"
          value={newKey.body} onChange={e=>setNewKey({...newKey, body:e.target.value})}
          style={{ fontSize:11, fontFamily:"var(--font-mono)", resize:"vertical", padding:10 }}/>
        <div className="help">Encrypted at rest. Crystal Forge never logs key material.</div>
      </div>
      <div style={{ display:"flex", gap:8, justifyContent:"flex-end" }}>
        <button className="btn btn-ghost focus-ring xs" onClick={()=>setAdding(false)}>Cancel</button>
        <button className="btn btn-primary focus-ring xs" onClick={()=>{ setAdding(false); set("credId", "cred-new"); }} disabled={!newKey.name || !newKey.body}>
          <Icon name="plus" size={11}/> Save key
        </button>
      </div>
    </div>
  );

  return (
    <div>
      <select className="input focus-ring" value={form.credId} onChange={e=>{
        if (e.target.value === "__new__") setAdding(true);
        else set("credId", e.target.value);
      }}>
        {SAVED_SSH_KEYS.map(k => (
          <option key={k.id} value={k.id}>{k.name}{k.deprecated ? " (deprecated)" : ""} — last used {k.lastUsed}</option>
        ))}
        <option value="__new__">+ Add new SSH key…</option>
      </select>
      {sel && (
        <div style={{ marginTop:10, padding:"10px 12px", border:"1px solid var(--cf-divider)", borderRadius:8, fontSize:11 }}>
          <div style={{ display:"flex", justifyContent:"space-between", alignItems:"center", marginBottom:4 }}>
            <span className="mono" style={{ fontWeight:600 }}>{sel.fingerprint}</span>
            <span style={{ color:"var(--cf-text-muted)" }}>added {sel.added}</span>
          </div>
          <div style={{ display:"flex", gap:10, color:"var(--cf-text-muted)" }}>
            <span>Last used: {sel.lastUsed}</span>
            {sel.deprecated && <span className="chip chip-warning" style={{ fontSize:9 }}>deprecated</span>}
          </div>
        </div>
      )}
    </div>
  );
}

function HttpsCredPicker({ form, set }) {
  const [adding, setAdding] = React.useState(false);
  const [newTok, setNewTok] = React.useState({ name:"", user:"", token:"" });
  const sel = SAVED_HTTPS_TOKENS.find(t => t.id === form.credId) ||
              (form.credId.startsWith("tok") ? null : SAVED_HTTPS_TOKENS[0]);

  if (adding) return (
    <div style={{ display:"flex", flexDirection:"column", gap:10 }}>
      <div className="field">
        <label>Token name</label>
        <input className="input focus-ring" placeholder="e.g. gitlab-ops" value={newTok.name} onChange={e=>setNewTok({...newTok, name:e.target.value})}/>
      </div>
      <div style={{ display:"grid", gridTemplateColumns:"1fr 2fr", gap:10 }}>
        <div className="field">
          <label>Username</label>
          <input className="input focus-ring" value={newTok.user} onChange={e=>setNewTok({...newTok, user:e.target.value})} placeholder="ops-bot"/>
        </div>
        <div className="field">
          <label>Token / password</label>
          <input className="input focus-ring mono" type="password" value={newTok.token} onChange={e=>setNewTok({...newTok, token:e.target.value})} placeholder="glpat-… or ghp_…" style={{ fontSize:12 }}/>
        </div>
      </div>
      <div style={{ display:"flex", gap:8, justifyContent:"flex-end" }}>
        <button className="btn btn-ghost focus-ring xs" onClick={()=>setAdding(false)}>Cancel</button>
        <button className="btn btn-primary focus-ring xs" onClick={()=>{ setAdding(false); set("credId", "tok-new"); }} disabled={!newTok.name || !newTok.token}>
          <Icon name="plus" size={11}/> Save token
        </button>
      </div>
    </div>
  );

  return (
    <div>
      <select className="input focus-ring" value={form.credId} onChange={e=>{
        if (e.target.value === "__new__") setAdding(true);
        else set("credId", e.target.value);
      }}>
        {SAVED_HTTPS_TOKENS.map(t => (
          <option key={t.id} value={t.id}>{t.name} — {t.user} ({t.scope})</option>
        ))}
        <option value="__new__">+ Add new token…</option>
      </select>
      {sel && (
        <div style={{ marginTop:10, padding:"10px 12px", border:"1px solid var(--cf-divider)", borderRadius:8, fontSize:11 }}>
          <div style={{ display:"flex", justifyContent:"space-between", alignItems:"center" }}>
            <span className="mono" style={{ fontWeight:600 }}>{sel.masked}</span>
            <span style={{ color:"var(--cf-text-muted)" }}>added {sel.added}</span>
          </div>
          <div style={{ marginTop:4, color:"var(--cf-text-muted)" }}>User: <span className="mono">{sel.user}</span> · Scope: {sel.scope}</div>
        </div>
      )}
    </div>
  );
}

Object.assign(window, { FlakesView, FlakeTray });
