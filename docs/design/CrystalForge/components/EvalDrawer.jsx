// Eval drawer — live log + policy matrix + dependency graph

function EvalDrawer({ ev, onClose, onCancel, onOpenSystem, onOpenPolicy }) {
  const [tab, setTab] = React.useState("log");
  const [confirmForce, setConfirmForce] = React.useState(false);

  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const isLive = ev.status === "in_progress" || ev.status === "queued" || ev.status === "cancelling";

  return (
    <>
      <div className="fl-tray-backdrop" onClick={onClose}/>
      <aside className="fl-tray" role="dialog" aria-label="Evaluation detail">
        {/* Header */}
        <header className="fl-tray-head">
          <div style={{ display:"flex", alignItems:"center", gap:12, minWidth:0, flex:1 }}>
            <Icon name="eval" size={18} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
            <div style={{ minWidth:0 }}>
              <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                <span style={{ fontWeight:700, fontSize:15 }}>{ev.flake}</span>
                <span className="chip chip-unknown" style={{ fontSize:10 }}>{ev.branch}</span>
                <span className={`chip ${ev.meta.cls}`}>
                  <span className="chip-dot" style={{ background:ev.meta.color }} />
                  {ev.meta.label}
                  {isLive && <Pulse style={{ marginLeft:6 }} />}
                </span>
              </div>
              <div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>
                {ev.commit} · {ev.id}
              </div>
            </div>
          </div>
          <div style={{ display:"flex", gap:6, alignItems:"center" }}>
            {ev.canCancel && <button className="btn btn-ghost focus-ring xs" onClick={()=>onCancel(ev.id, false)}>Cancel</button>}
            {ev.canForceCancel && <button className="btn btn-ghost focus-ring xs" style={{ color:"#f87171" }} onClick={()=>setConfirmForce(true)}>Force-cancel</button>}
            <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={16}/></button>
          </div>
        </header>

        {/* Stats grid */}
        <div className="ed-stats">
          <div className="ed-stat">
            <div className="ed-stat-label">{ev.completedAt ? "Completed" : "Started"}</div>
            <div className="ed-stat-val" style={{ fontSize:12.5, fontWeight:600 }}><DTG at={ev.completedAt || ev.startedAt} relative={ev.completedAt || ev.startedAt}/></div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Duration</div>
            <div className="ed-stat-val" style={{ fontFamily:"var(--font-mono)" }}>{ev.dur || "—"}</div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Systems</div>
            <div className="ed-stat-val">{ev.systemCount}</div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Policy</div>
            <div className="ed-stat-val" style={{ display:"flex", gap:6, alignItems:"baseline" }}>
              <span style={{ color:"#34d399" }}>{ev.policyPass}</span>
              <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>/</span>
              <span style={{ color: ev.policyFail > 0 ? "#f87171" : "var(--cf-text-muted)" }}>{ev.policyFail}</span>
            </div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Derivations</div>
            <div className="ed-stat-val">{ev.derivCount || ev.systemCount * 18}</div>
          </div>
        </div>

        {/* Tabs */}
        <div className="sd-tabs" style={{ padding:"0 16px", borderBottom:"1px solid var(--cf-card-border)", flexShrink:0 }}>
          <button className={`sd-tab focus-ring${tab==="log"?" active":""}`} onClick={()=>setTab("log")}>
            <Icon name="terminal" size={12}/> Log {isLive && <Pulse style={{ marginLeft:4 }} />}
          </button>
          <button className={`sd-tab focus-ring${tab==="policy"?" active":""}`} onClick={()=>setTab("policy")}>
            <Icon name="shield" size={12}/> Policy matrix
          </button>
          <button className={`sd-tab focus-ring${tab==="graph"?" active":""}`} onClick={()=>setTab("graph")}>
            <Icon name="git" size={12}/> Dependency graph
          </button>
        </div>

        {/* Body */}
        <div className="ed-body">
          {tab === "log"    && <EvalLogTab ev={ev} live={isLive}/>}
          {tab === "policy" && <EvalPolicyTab ev={ev} onOpenSystem={onOpenSystem} onOpenPolicy={onOpenPolicy}/>}
          {tab === "graph"  && <EvalGraphTab ev={ev}/>}
        </div>

        {confirmForce && (
          <ConfirmForceCancel
            ev={ev}
            onConfirm={() => { setConfirmForce(false); onCancel(ev.id, true); }}
            onCancel={() => setConfirmForce(false)}
          />
        )}
      </aside>
    </>
  );
}

/* ── Log tab ─────────────────────────────────────────── */
function EvalLogTab({ ev, live }) {
  const [autoscroll, setAutoscroll] = React.useState(true);
  const preRef = React.useRef(null);
  const lines = (ev.logLines || EVAL_DEFAULT_LOG)(ev);

  React.useEffect(() => {
    if (autoscroll && preRef.current) {
      preRef.current.scrollTop = preRef.current.scrollHeight;
    }
  }, [lines, autoscroll]);

  return (
    <div style={{ display:"flex", flexDirection:"column", flex:1, minHeight:0 }}>
      <div style={{ padding:"8px 16px", borderBottom:"1px solid var(--cf-divider)", display:"flex", gap:10, alignItems:"center", flexShrink:0 }}>
        <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{lines.length} lines</span>
        <div style={{ flex:1 }}/>
        <label style={{ display:"flex", gap:6, alignItems:"center", fontSize:11 }}>
          <input type="checkbox" className="ed-checkbox" checked={autoscroll} onChange={e=>setAutoscroll(e.target.checked)}/>
          Auto-scroll
        </label>
        <button className="btn-icon focus-ring" title="Download"><Icon name="download" size={13}/></button>
      </div>
      <pre ref={preRef} className="fl-diff" style={{ flex:1, fontSize:11, lineHeight:1.55, padding:"10px 16px" }}>
        {lines.map((line, i) => {
          const isErr = /error|fail|✗/i.test(line);
          const isWarn = /warn|skip/i.test(line);
          const isOk  = /ok|pass|✓|complete/i.test(line);
          const color = isErr ? "#f87171" : isWarn ? "#f59e0b" : isOk ? "#34d399" : "inherit";
          return <div key={i} style={{ color }}><span style={{ color:"var(--cf-text-muted)", userSelect:"none", display:"inline-block", width:36 }}>{String(i+1).padStart(3," ")}</span> {line}</div>;
        })}
        {live && <div style={{ color:"#60a5fa" }}><span style={{ color:"var(--cf-text-muted)", display:"inline-block", width:36 }}>{String(lines.length+1).padStart(3," ")}</span> <Pulse style={{ marginLeft:0 }}/></div>}
      </pre>
    </div>
  );
}

/* ── Policy matrix tab ────────────────────────────────── */
// Short codes used by the mock eval matrix, mapped to a human cause + (when one
// really exists in the Policies registry) a deep-link target.
const EVAL_CHECK_INFO = {
  "stig.audit": { label:"STIG · audit daemon", policyId:"stig-auditd",
    attr:"config.security.audit", assertion:"auditd rule set does not cover required syscalls (execve, ptrace) per STIG V-230351" },
  "stig.fw":    { label:"STIG · firewall",
    attr:"config.networking.firewall", assertion:"host-based firewall is disabled (networking.firewall.enable = false)" },
  "stig.sshd":  { label:"STIG · sshd hardening", policyId:"stig-sshd",
    attr:"config.services.openssh.settings", assertion:"PermitRootLogin is not \"no\" as required by STIG V-230501" },
  "stig.tls":   { label:"STIG · TLS/FIPS", policyId:"stig-fips",
    attr:"config.boot.kernelParams", assertion:"FIPS-validated crypto is not enabled (missing fips=1 kernel param)" },
  "cf.hb":      { label:"Heartbeat cadence",
    attr:"config.services.cf-agent.heartbeatIntervalSec", assertion:"heartbeat interval (900s) exceeds the fleet policy maximum (300s)" },
  "cf.cve":     { label:"CVE gate", policyId:"cve-gated",
    attr:"flake.lock » nixpkgs", assertion:"locked nixpkgs input pulls openssl 3.0.11, affected by CVE-2024-6119 (critical)" },
  "cf.cache":   { label:"Cache push",
    attr:"config.nix.settings.substituters", assertion:"no cache destination configured for this environment" },
};
function EvalPolicyTab({ ev, onOpenSystem, onOpenPolicy }) {
  const matrix = ev.policyMatrix || EVAL_DEFAULT_POLICY(ev);
  const policies = matrix.policies;
  const baseRows = matrix.rows;

  const [filter, setFilter] = React.useState("all"); // all | fail | warn | clean
  const [sort, setSort]     = React.useState("health"); // health | name
  const [expanded, setExpanded] = React.useState(null); // host name
  const [openCause, setOpenCause] = React.useState(null); // `${host}::${policyIdx}` when a config detail is expanded
  const [policyFilter, setPolicyFilter] = React.useState(null); // policy name when clicked from summary

  // Annotate rows with counts
  const annotated = baseRows.map(r => {
    const fail = r.results.filter(x => x === "fail").length;
    const warn = r.results.filter(x => x === "warn").length;
    const pass = r.results.filter(x => x === "pass").length;
    return { ...r, fail, warn, pass };
  });

  // Apply filter
  let filtered = annotated;
  if (filter === "fail")  filtered = filtered.filter(r => r.fail > 0);
  if (filter === "warn")  filtered = filtered.filter(r => r.warn > 0 && r.fail === 0);
  if (filter === "clean") filtered = filtered.filter(r => r.fail === 0 && r.warn === 0);
  if (policyFilter) {
    const idx = policies.indexOf(policyFilter);
    filtered = filtered.filter(r => r.results[idx] !== "pass");
  }

  // Sort
  if (sort === "health") {
    filtered = [...filtered].sort((a,b) => (b.fail*10+b.warn) - (a.fail*10+a.warn));
  } else {
    filtered = [...filtered].sort((a,b) => a.host.localeCompare(b.host));
  }

  // Per-policy summary
  const policyStats = policies.map((p, i) => {
    const fail = annotated.filter(r => r.results[i] === "fail").length;
    const warn = annotated.filter(r => r.results[i] === "warn").length;
    const pass = annotated.filter(r => r.results[i] === "pass").length;
    return { name: p, fail, warn, pass, total: annotated.length };
  });

  // Failure highlights — surface top issues
  const topIssues = policyStats.filter(s => s.fail > 0).sort((a,b) => b.fail - a.fail).slice(0, 3);

  const counts = {
    fail:  annotated.filter(r => r.fail > 0).length,
    warn:  annotated.filter(r => r.fail === 0 && r.warn > 0).length,
    clean: annotated.filter(r => r.fail === 0 && r.warn === 0).length,
  };

  const cellGlyph = (res) => res === "pass" ? "✓" : res === "warn" ? "!" : "✗";

  return (
    <div style={{ flex:1, overflow:"hidden", display:"flex", flexDirection:"column" }}>

      {/* Top issues callout (if any) */}
      {topIssues.length > 0 && (
        <div className="pm-issues">
          <div className="pm-issues-label">Top issues</div>
          {topIssues.map(iss => (
            <button key={iss.name}
              className={`pm-issue-chip${policyFilter === iss.name ? " active" : ""}`}
              onClick={() => setPolicyFilter(policyFilter === iss.name ? null : iss.name)}
            >
              <span className="pm-issue-dot"/>
              <span className="mono">{iss.name}</span>
              <span style={{ color:"#f87171", fontWeight:700 }}>{iss.fail}</span>
              <span style={{ color:"var(--cf-text-muted)" }}>/{iss.total} fail</span>
            </button>
          ))}
          {policyFilter && (
            <button className="btn-icon focus-ring" style={{ marginLeft:"auto" }} title="Clear policy filter" onClick={()=>setPolicyFilter(null)}>
              <Icon name="x" size={12}/>
            </button>
          )}
        </div>
      )}

      {/* Controls */}
      <div className="pm-controls">
        <div className="seg">
          <button className={filter==="all"  ?"active":""} onClick={()=>setFilter("all")}>All <span className="pm-count">{annotated.length}</span></button>
          <button className={filter==="fail" ?"active":""} onClick={()=>setFilter("fail")}>Failing <span className="pm-count pm-count-fail">{counts.fail}</span></button>
          <button className={filter==="warn" ?"active":""} onClick={()=>setFilter("warn")}>Warning <span className="pm-count pm-count-warn">{counts.warn}</span></button>
          <button className={filter==="clean"?"active":""} onClick={()=>setFilter("clean")}>Clean <span className="pm-count pm-count-pass">{counts.clean}</span></button>
        </div>
        <div style={{ flex:1 }}/>
        <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>Sort</span>
        <div className="seg">
          <button className={sort==="health"?"active":""} onClick={()=>setSort("health")}>Worst first</button>
          <button className={sort==="name"  ?"active":""} onClick={()=>setSort("name")}>Name</button>
        </div>
      </div>

      {/* Matrix */}
      <div className="pm-scroll">
        <table className="pm-table">
          <thead>
            <tr>
              <th className="pm-th-host">System</th>
              <th className="pm-th-health">Health</th>
              {policies.map(p => {
                const s = policyStats.find(x=>x.name===p);
                const isFiltered = policyFilter === p;
                return (
                  <th key={p}
                    className={`pm-th-policy${isFiltered?" filtered":""}`}
                    title={`${p} — ${s.fail} fail / ${s.warn} warn / ${s.pass} pass`}
                    onClick={()=>setPolicyFilter(policyFilter === p ? null : p)}
                  >
                    <div className="pm-th-policy-inner">
                      <span className="pm-th-policy-label">{p}</span>
                    </div>
                    <div className="pm-th-policy-bar">
                      <div style={{ width: `${(s.fail/s.total)*100}%`, background:"#f87171" }}/>
                      <div style={{ width: `${(s.warn/s.total)*100}%`, background:"#f59e0b" }}/>
                      <div style={{ width: `${(s.pass/s.total)*100}%`, background:"#34d399" }}/>
                    </div>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {filtered.map(r => {
              const isExp = expanded === r.host;
              const healthColor = r.fail > 0 ? "#f87171" : r.warn > 0 ? "#f59e0b" : "#34d399";
              return (
                <React.Fragment key={r.host}>
                  <tr className={`pm-row${isExp?" expanded":""}`} onClick={()=>setExpanded(isExp ? null : r.host)}>
                    <td className="pm-td-host">
                      <div className="pm-host-cell">
                        <Icon name={isExp ? "chevron-down" : "chevron-right"} size={11} style={{ color:"var(--cf-text-muted)", flexShrink:0 }}/>
                        <span className="mono pm-host-name">{r.host}</span>
                      </div>
                    </td>
                    <td className="pm-td-health">
                      <div className="pm-health">
                        <div className="pm-health-bar">
                          {r.fail > 0 && <div style={{ width:`${(r.fail/policies.length)*100}%`, background:"#f87171" }}/>}
                          {r.warn > 0 && <div style={{ width:`${(r.warn/policies.length)*100}%`, background:"#f59e0b" }}/>}
                          {r.pass > 0 && <div style={{ width:`${(r.pass/policies.length)*100}%`, background:"#34d399" }}/>}
                        </div>
                        <span className="mono pm-health-num" style={{ color: healthColor }}>
                          {r.pass}/{policies.length}
                        </span>
                      </div>
                    </td>
                    {r.results.map((res, i) => {
                      const policyIsFiltered = policyFilter === policies[i];
                      return (
                        <td key={i}
                          className={`pm-td-cell pm-${res}${policyIsFiltered?" col-filtered":""}`}
                          title={`${policies[i]}: ${res}`}
                          onClick={e => { e.stopPropagation(); setPolicyFilter(policyFilter === policies[i] ? null : policies[i]); }}
                        >
                          <span className="pm-glyph">{cellGlyph(res)}</span>
                        </td>
                      );
                    })}
                  </tr>
                  {isExp && (
                    <tr className="pm-expand-row">
                      <td colSpan={policies.length + 2}>
                        <div className="pm-expand">
                          <div style={{ display:"flex", flexDirection:"column", gap:14 }}>
                            {r.results.map((res, i) => res === "pass" ? null : (() => {
                              const info = EVAL_CHECK_INFO[policies[i]];
                              const key = `${r.host}::${i}`;
                              const isOpen = openCause === key;
                              return (
                              <div key={i} style={{ border:"1px solid var(--cf-divider)", borderRadius:8, overflow:"hidden" }}>
                                <div className={`pm-failcard pm-failcard-${res}`} style={{ cursor:"pointer", border:"none", borderRadius:0 }}
                                  title="Show the config line that failed this check"
                                  onClick={e => { e.stopPropagation(); setOpenCause(isOpen ? null : key); }}
                                >
                                  <span className={`pm-failcard-glyph pm-${res}`}>{cellGlyph(res)}</span>
                                  <div style={{ minWidth:0, textAlign:"left" }}>
                                    <div className="mono" style={{ fontWeight:600, fontSize:12 }}>{info?.label || policies[i]}</div>
                                    <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>
                                      {info?.assertion || (res === "fail" ? "Blocks deployment until resolved" : "Soft warning — deploy will proceed")}
                                    </div>
                                  </div>
                                  <Icon name={isOpen ? "chevron-down" : "chevron-right"} size={12} style={{ color:"var(--cf-text-muted)", marginLeft:8, flexShrink:0 }}/>
                                </div>
                                {isOpen && info?.assertion && (
                                  <div style={{ padding:"10px 12px", background:"var(--cf-canvas)", borderTop:"1px solid var(--cf-divider)" }}>
                                    <div className="mono" style={{ fontSize:10.5, color:"var(--cf-text-muted)", marginBottom:6 }}>
                                      nixosConfigurations.{r.host}.{info.attr}
                                    </div>
                                    <div style={{ fontSize:12, color:"#f87171", lineHeight:1.5 }}>
                                      <span className="mono" style={{ fontWeight:600 }}>assertion failed:</span> {info.assertion}
                                    </div>
                                    <div style={{ fontSize:10.5, color:"var(--cf-text-muted)", marginTop:8 }}>
                                      From nix-eval-jobs — attribute path + assertion message only; eval doesn't report a source line for module assertions.
                                    </div>
                                    {info.policyId && (
                                      <button className="btn btn-ghost focus-ring xs" style={{ marginTop:8 }} onClick={e => { e.stopPropagation(); onOpenPolicy?.(info.policyId); }}>
                                        <Icon name="file" size={11}/> View policy definition
                                      </button>
                                    )}
                                  </div>
                                )}
                              </div>
                              );
                            })())}
                            {r.fail === 0 && r.warn === 0 && (
                              <div style={{ fontSize:12, color:"#34d399", display:"flex", alignItems:"center", gap:8 }}>
                                <Icon name="check" size={14}/> All policies pass for this system.
                              </div>
                            )}
                          </div>
                          <div style={{ display:"flex", gap:6, marginTop:12 }}>
                            <button className="btn btn-ghost focus-ring xs" onClick={() => { const s = SYSTEMS.find(x => x.hostname === r.host); if (s) onOpenSystem?.(s); }}><Icon name="arrow-right" size={11}/> Open system</button>
                          </div>
                        </div>
                      </td>
                    </tr>
                  )}
                </React.Fragment>
              );
            })}
            {filtered.length === 0 && (
              <tr><td colSpan={policies.length + 2} style={{ padding:24, textAlign:"center", color:"var(--cf-text-muted)", fontSize:13 }}>
                No systems match this filter.
              </td></tr>
            )}
          </tbody>
        </table>
      </div>

      {/* Legend */}
      <div className="pm-legend">
        <span><span className="pm-legend-sw pm-pass">✓</span> Pass</span>
        <span><span className="pm-legend-sw pm-warn">!</span> Warning</span>
        <span><span className="pm-legend-sw pm-fail">✗</span> Fail — blocks deploy</span>
        <span style={{ marginLeft:"auto", fontSize:11, color:"var(--cf-text-muted)" }}>Click any policy header to filter · Click a row to expand</span>
      </div>
    </div>
  );
}

/* ── Dependency graph tab ────────────────────────────── */
function EvalGraphTab({ ev }) {
  const graph = ev.graph || EVAL_DEFAULT_GRAPH(ev);

  return (
    <div style={{ flex:1, overflow:"auto", padding:"18px" }}>
      {/* Top: source → eval → fanout */}
      <div className="ed-graph-summary">
        <div className="ed-graph-node ed-graph-source">
          <Icon name="git" size={12}/>
          <span className="mono">{ev.commit}</span>
        </div>
        <span style={{ color:"var(--cf-text-muted)" }}>→</span>
        <div className="ed-graph-node ed-graph-eval">
          <Icon name="eval" size={12}/>
          eval
        </div>
        <span style={{ color:"var(--cf-text-muted)" }}>→</span>
        <div className="ed-graph-node ed-graph-fan">
          <span style={{ fontWeight:700 }}>{graph.totalDerivs}</span>
          <span style={{ fontSize:10, color:"var(--cf-text-muted)" }}>derivations</span>
        </div>
        <span style={{ color:"var(--cf-text-muted)" }}>→</span>
        <div className="ed-graph-node ed-graph-fan">
          <span style={{ fontWeight:700, color:"#34d399" }}>{graph.cached}</span>
          <span style={{ fontSize:10, color:"var(--cf-text-muted)" }}>cached</span>
        </div>
        <div className="ed-graph-node ed-graph-fan">
          <span style={{ fontWeight:700, color:"#60a5fa" }}>{graph.toBuild}</span>
          <span style={{ fontSize:10, color:"var(--cf-text-muted)" }}>to build</span>
        </div>
      </div>

      {/* List of derivations */}
      <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline", marginBottom:8 }}>
        <h3 style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.06em", color:"var(--cf-text-muted)", fontWeight:700, margin:0 }}>Derivations by package</h3>
        <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{graph.packages.length} packages</span>
      </div>
      <div className="ed-graph-list">
        {graph.packages.map(p => {
          const total = p.cached + p.toBuild;
          return (
            <div key={p.name} className="ed-graph-row">
              <div className="ed-graph-pkg">
                <span style={{ fontSize:12, fontWeight:600 }} className="mono truncate">{p.name}</span>
                <span style={{ fontSize:10, color:"var(--cf-text-muted)" }}>{total} derivs</span>
              </div>
              <div className="ed-graph-bar">
                <div className="ed-graph-bar-cached" style={{ width:`${(p.cached/total)*100}%` }}/>
                <div className="ed-graph-bar-build"  style={{ width:`${(p.toBuild/total)*100}%` }}/>
              </div>
              <div style={{ display:"flex", gap:6, justifyContent:"flex-end", fontSize:11 }}>
                <span style={{ color:"#34d399", fontWeight:600 }}>{p.cached}</span>
                <span style={{ color:"var(--cf-text-muted)" }}>·</span>
                <span style={{ color:"#60a5fa", fontWeight:600 }}>{p.toBuild}</span>
              </div>
            </div>
          );
        })}
      </div>

      <div className="ed-graph-legend">
        <span><span className="ed-graph-sw" style={{ background:"#34d399" }}/>Cached (already in binary cache)</span>
        <span><span className="ed-graph-sw" style={{ background:"#60a5fa" }}/>To build (will fan out to builders)</span>
      </div>
    </div>
  );
}

/* ── Force-cancel confirmation ───────────────────────── */
function ConfirmForceCancel({ ev, onConfirm, onCancel }) {
  return (
    <div className="modal-backdrop" onClick={onCancel} style={{ zIndex:90 }}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ maxWidth:440 }}>
        <div className="modal-head">
          <h2 style={{ display:"flex", gap:8, alignItems:"center" }}>
            <Icon name="warn" size={18} style={{ color:"#f87171" }}/>
            Force-cancel evaluation?
          </h2>
          <p>This terminates <span className="mono">{ev.id}</span> immediately. In-flight builds may leave partial state in the Nix store. This action cannot be undone.</p>
        </div>
        <div className="modal-body" style={{ paddingTop:0 }}>
          <div className="sd-callout sd-callout-danger">
            <Icon name="warn" size={14}/>
            <div style={{ fontSize:12 }}>Prefer normal cancel — it lets in-flight derivations finish cleanly.</div>
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onCancel}>Keep running</button>
          <button className="btn focus-ring" onClick={onConfirm} style={{ background:"#dc2626", color:"white" }}>Force-cancel</button>
        </div>
      </div>
    </div>
  );
}

/* ── Default mock generators (when entry doesn't define) ── */
function EVAL_DEFAULT_LOG(ev) {
  const seed = ev.id.split("").reduce((a,c)=>a+c.charCodeAt(0),0);
  const sysHosts = SYSTEMS.filter(s => s.flake === ev.flake).map(s => s.hostname);
  const hosts0 = sysHosts.length ? sysHosts : SYSTEMS.map(s => s.hostname);
  const lines = [
    `evaluating flake ${ev.flake}@${ev.commit}`,
    `loading flake.lock`,
    `resolving inputs… nixpkgs (locked at 24.11.20260401)`,
    `building eval config for ${ev.systemCount} systems`,
    ...hosts0.slice(0, Math.min(ev.systemCount, 5)).flatMap(h => [
      `  ► evaluating ${h}.nix`,
      `    policy: stig.audit_rules ✓`,
      `    policy: stig.firewall ✓`,
      `    policy: cf.heartbeat_interval ✓`,
      `  ✓ ${h} evaluated (${(seed%23+18)} derivations)`,
    ]),
  ];
  if (ev.status === "complete") lines.push(`evaluation complete in ${ev.dur}`);
  if (ev.status === "failed")   lines.push(`✗ error: attribute 'foo' missing at hosts/atlas-01/services.nix:42:14`);
  if (ev.status === "in_progress") lines.push(`evaluating package overrides…`);
  return lines;
}

function EVAL_DEFAULT_POLICY(ev) {
  const seed = ev.id.split("").reduce((a,c)=>a+c.charCodeAt(0),0);
  const policies = ["stig.audit","stig.fw","stig.sshd","stig.tls","cf.hb","cf.cve","cf.cache"];
  // Use real fleet hostnames (not invented ones) so "Open system" always resolves.
  const hosts = SYSTEMS.filter(s => s.flake === ev.flake).map(s => s.hostname).slice(0, ev.systemCount);
  const usedHostnames = new Set(hosts);
  const fallbackPool = SYSTEMS.map(s => s.hostname).filter(h => !usedHostnames.has(h));
  let fbIdx = 0;
  while (hosts.length < ev.systemCount && fbIdx < fallbackPool.length) { hosts.push(fallbackPool[fbIdx]); usedHostnames.add(fallbackPool[fbIdx]); fbIdx++; }
  const rows = hosts.map((h, i) => {
    const results = policies.map((_, j) => {
      const r = (seed + i*13 + j*7) % 100;
      if (ev.status === "failed" && j === 3 && i === 0) return "fail";
      return r > 92 ? "fail" : r > 80 ? "warn" : "pass";
    });
    return { host:h, results };
  });
  return { policies, rows };
}

function EVAL_DEFAULT_GRAPH(ev) {
  const seed = ev.id.split("").reduce((a,c)=>a+c.charCodeAt(0),0);
  const packages = [
    { name:"systemd",     cached:48, toBuild:2 },
    { name:"openssl",     cached:0,  toBuild:8 },
    { name:"nginx",       cached:6,  toBuild:0 },
    { name:"linux-kernel",cached:1,  toBuild:0 },
    { name:"python311",   cached:34, toBuild:1 },
    { name:"glibc",       cached:12, toBuild:0 },
    { name:"audit",       cached:0,  toBuild:4 },
    { name:"sops-nix",    cached:3,  toBuild:0 },
  ].map(p => ({ ...p, cached: p.cached + (seed%5), toBuild: p.toBuild + (seed%3) }));
  const cached = packages.reduce((a,p)=>a+p.cached, 0);
  const toBuild = packages.reduce((a,p)=>a+p.toBuild, 0);
  return { packages, cached, toBuild, totalDerivs: cached + toBuild };
}

Object.assign(window, { EvalDrawer });
