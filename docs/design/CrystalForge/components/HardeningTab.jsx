// Systemd service hardening tab — mirrors systemd-analyze security style

function HardeningTab({ sys }) {
  const [query, setQuery] = React.useState("");
  const [filterRisk, setFilterRisk] = React.useState("all");
  const [selected, setSelected] = React.useState(null);

  const services = React.useMemo(() => window.buildSystemHardening(sys), [sys.id]);

  const filtered = services.filter(svc => {
    if (query && !svc.name.toLowerCase().includes(query.toLowerCase())) return false;
    if (filterRisk !== "all" && svc.risk !== filterRisk) return false;
    return true;
  });

  const stats = {
    ok:   services.filter(s => s.risk === "OK").length,
    med:  services.filter(s => s.risk === "MED").length,
    high: services.filter(s => s.risk === "HIGH").length,
    vuln: services.filter(s => s.risk === "VULN").length,
    avg:  Math.round(services.reduce((a,s) => a+s.score, 0) / services.length),
  };

  return (
    <>
      {/* Summary */}
      <div className="hd-stat-row">
        {[
          { label: "Avg score", val: stats.avg + "%", color: stats.avg < 30 ? "#f87171" : "#34d399" },
          { label: "VULN", val: stats.vuln, color: "#f87171" },
          { label: "HIGH", val: stats.high, color: "#f97316" },
          { label: "MED",  val: stats.med,  color: "#fbbf24" },
          { label: "OK",   val: stats.ok,   color: "#34d399" },
          { label: "Total", val: services.length, color: "var(--cf-text-secondary)" },
        ].map(m => (
          <div key={m.label} className="hd-stat">
            <div className="hd-stat-val" style={{ color: m.color }}>{m.val}</div>
            <div className="hd-stat-label">{m.label}</div>
          </div>
        ))}
        <div className="sd-callout sd-callout-info" style={{ flex:1, marginLeft:8, padding:"8px 12px" }}>
          <Icon name="warn" size={13} />
          <div style={{ fontSize:12 }}>
            Mirrors <code className="mono">systemd-analyze security</code>. Higher score = more directives enforced. Set directives in NixOS via <code className="mono">systemd.services.&lt;name&gt;.serviceConfig</code>.
          </div>
        </div>
      </div>

      {/* Filters */}
      <div className="filterbar" style={{ marginBottom:10 }}>
        <div className="filter-search" style={{ maxWidth:280 }}>
          <Icon name="search" />
          <input className="input focus-ring" placeholder="Filter service…" value={query} onChange={e=>setQuery(e.target.value)} />
        </div>
        <div className="seg">
          {["all","VULN","HIGH","MED","OK"].map(k => (
            <button key={k} className={filterRisk===k?"active":""} onClick={() => setFilterRisk(k)}>{k}</button>
          ))}
        </div>
        <span className="filter-count">{filtered.length} services</span>
      </div>

      {/* Table */}
      <div className="card" style={{ overflow:"hidden" }}>
        <table className="sys-table">
          <thead>
            <tr>
              <th style={{ width:"22%" }}>Service</th>
              <th>Risk</th>
              <th>Score</th>
              <th>User</th>
              {SD_COLS.slice(0,8).map(c => (
                <th key={c} style={{ fontSize:9, letterSpacing:"0.04em", textAlign:"center", padding:"8px 4px" }} title={c}>
                  {c.replace(/([A-Z])/g," $1").trim().slice(0,4)}
                </th>
              ))}
              <th>Missing</th>
              <th style={{ textAlign:"right" }}> </th>
            </tr>
          </thead>
          <tbody>
            {filtered.map(svc => (
              <tr key={svc.id} onClick={() => setSelected(svc)} style={{ cursor:"pointer" }}>
                <td>
                  <span className="mono" style={{ fontSize:12, color:"var(--cf-text-primary)", fontWeight:600 }}>{svc.name}</span>
                  {svc.waivers && Object.keys(svc.waivers).length > 0 && <div style={{ fontSize:10, color:"#fbbf24", marginTop:2 }}>⚠ {Object.keys(svc.waivers).length} waived</div>}
                </td>
                <td>
                  <span className="chip" style={{ color:svc.riskColor, background:svc.riskColor+"22", border:`1px solid ${svc.riskColor}44`, fontSize:10, fontWeight:700 }}>
                    {svc.risk}
                  </span>
                </td>
                <td>
                  <div style={{ display:"flex", alignItems:"center", gap:8 }}>
                    <div style={{ width:60, height:6, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
                      <div style={{ width:`${svc.score}%`, height:"100%", background:svc.riskColor }} />
                    </div>
                    <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{svc.score}%</span>
                  </div>
                </td>
                <td><span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{svc.user}</span></td>
                {SD_COLS.slice(0,8).map((c, ci) => (
                  <td key={c} style={{ textAlign:"center", padding:"4px" }}>
                    {svc.enabled[ci]
                      ? <span style={{ color:"#34d399", fontSize:11 }}>✓</span>
                      : <span style={{ color:"var(--cf-text-disabled)", fontSize:11 }}>–</span>
                    }
                  </td>
                ))}
                <td>
                  <span style={{ fontSize:11, color: svc.missing > 15 ? "#f87171" : "var(--cf-text-muted)" }}>
                    {svc.missing}/{SD_COLS.length}
                  </span>
                </td>
                <td>
                  <button className="btn-icon focus-ring" title="View details" onClick={e=>{e.stopPropagation();setSelected(svc);}}>
                    <Icon name="arrow-right" size={14} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Detail modal */}
      {selected && <HardeningDetailModal svc={selected} onClose={() => setSelected(null)} />}
    </>
  );
}

function HardeningDetailModal({ svc, onClose }) {
  const [tab, setTab] = React.useState("overview");
  const [waivers, setWaivers] = React.useState(() => ({ ...(svc.waivers || {}) }));
  const [editing, setEditing] = React.useState(null);
  const [draft, setDraft] = React.useState("");

  const openWaiver = (col) => { setEditing(col); setDraft(waivers[col]?.text || ""); };
  const saveWaiver = () => {
    const next = { ...waivers, [editing]: { text: draft.trim(), by: "mreyes", at: "just now" } };
    setWaivers(next); svc.waivers = next; setEditing(null); setDraft("");
  };
  const removeWaiver = (col) => {
    const next = { ...waivers }; delete next[col];
    setWaivers(next); svc.waivers = next;
  };
  const waiverCount = Object.keys(waivers).length;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" style={{ width:"min(720px,98vw)" }} onClick={e=>e.stopPropagation()}>
        <div className="modal-head" style={{ display:"flex", justifyContent:"space-between", alignItems:"flex-start" }}>
          <div>
            <h2 style={{ margin:0, fontSize:16, display:"flex", alignItems:"center", gap:10 }}>
              <span className="chip" style={{ color:svc.riskColor, background:svc.riskColor+"22", fontSize:10 }}>{svc.risk}</span>
              <span className="mono">{svc.name}</span>
            </h2>
            <div style={{ fontSize:12, color:"var(--cf-text-muted)", marginTop:4 }}>
              Score: <strong style={{ color:svc.riskColor }}>{svc.score}%</strong>
              &nbsp;·&nbsp;{svc.missing} missing directives&nbsp;·&nbsp;user: <span className="mono">{svc.user}</span>
              {waiverCount > 0 && <>&nbsp;·&nbsp;<span style={{ color:"#fbbf24" }}>{waiverCount} waived</span></>}
            </div>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16} /></button>
        </div>

        <div className="sd-tabs" style={{ padding:"0 22px", marginTop:0 }}>
          {[{k:"overview",l:"Directives"},{k:"nix",l:"NixOS config"},{k:"all",l:"All checks"}].map(t => (
            <button key={t.k} className={`sd-tab focus-ring${tab===t.k?" active":""}`} onClick={()=>setTab(t.k)}>{t.l}</button>
          ))}
        </div>

        <div style={{ padding:"16px 22px", maxHeight:"60vh", overflowY:"auto" }}>
          {tab === "overview" && (
            <div>
              <div className="sd-callout sd-callout-info" style={{ marginBottom:12 }}>
                <Icon name="file" size={13} />
                <div style={{ fontSize:12 }}>
                  Directives that aren’t enforced can be <strong>justified with a waiver</strong> (e.g. compensating control, not applicable). Waivers flow into the compliance evidence export.
                </div>
              </div>
              <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:8 }}>
                {SD_COLS.map((col, ci) => {
                  const on = svc.enabled[ci];
                  const waiver = waivers[col];
                  const isEditing = editing === col;
                  const accent = on ? "#34d399" : waiver ? "#fbbf24" : "#f87171";
                  const bg = on ? "rgba(52,211,153,0.06)" : waiver ? "rgba(251,191,36,0.07)" : "rgba(248,113,113,0.05)";
                  return (
                    <div key={col} style={{
                      gridColumn: isEditing ? "1 / -1" : "auto",
                      padding:"8px 10px", background:bg,
                      border:`1px solid ${accent}33`, borderRadius:8,
                    }}>
                      <div style={{ display:"flex", alignItems:"center", gap:10 }}>
                        <span style={{ fontSize:16 }}>{on ? "✅" : waiver ? "⚠️" : "❌"}</span>
                        <div style={{ minWidth:0, flex:1 }}>
                          <div className="mono" style={{ fontSize:12, fontWeight:600 }}>{col}</div>
                          <div style={{ fontSize:10, color: waiver ? "#fbbf24" : "var(--cf-text-muted)" }}>
                            {on ? "enforced" : waiver ? "not set · waived" : "not set"}
                          </div>
                        </div>
                        {!on && !isEditing && (
                          <button className="btn btn-ghost focus-ring xs" onClick={() => openWaiver(col)} style={{ flexShrink:0 }}>
                            <Icon name={waiver ? "file" : "plus"} size={11} /> {waiver ? "Edit" : "Justify"}
                          </button>
                        )}
                      </div>
                      {waiver && !isEditing && (
                        <div style={{ marginTop:8, paddingLeft:26 }}>
                          <div style={{ fontSize:12, color:"var(--cf-text-primary)", lineHeight:1.5 }}>{waiver.text}</div>
                          <div style={{ display:"flex", alignItems:"center", gap:8, marginTop:6 }}>
                            <span style={{ fontSize:10, color:"var(--cf-text-muted)", display:"flex", alignItems:"center", gap:4 }}>
                              <Icon name="user" size={10} /> {waiver.by} · {waiver.at}
                            </span>
                            <button className="focus-ring" onClick={() => removeWaiver(col)}
                              style={{ all:"unset", cursor:"pointer", fontSize:10, color:"#f87171" }}>Remove</button>
                          </div>
                        </div>
                      )}
                      {isEditing && (
                        <div style={{ marginTop:8, paddingLeft:26 }}>
                          <textarea className="input focus-ring" rows={2} autoFocus value={draft}
                            onChange={e=>setDraft(e.target.value)}
                            placeholder="Why is leaving this unset acceptable? (compensating control, N/A…)"
                            style={{ resize:"vertical", width:"100%", fontSize:12 }} />
                          <div style={{ display:"flex", gap:6, flexWrap:"wrap", marginTop:6 }}>
                            {WAIVER_PRESETS.map(p => (
                              <button key={p} className="focus-ring" onClick={() => setDraft(p)}
                                style={{ all:"unset", cursor:"pointer", fontSize:10, padding:"3px 8px", borderRadius:99, background:"var(--cf-subtle-bg)", color:"var(--cf-text-secondary)", border:"1px solid var(--cf-card-border)" }}>
                                {p.length > 46 ? p.slice(0,44)+"…" : p}
                              </button>
                            ))}
                          </div>
                          <div style={{ display:"flex", gap:8, marginTop:8 }}>
                            <button className="btn btn-primary focus-ring xs" disabled={draft.trim().length < 8}
                              style={draft.trim().length < 8 ? { opacity:0.5, cursor:"not-allowed" } : null}
                              onClick={saveWaiver}><Icon name="check" size={11} /> Save waiver</button>
                            <button className="btn btn-ghost focus-ring xs" onClick={() => { setEditing(null); setDraft(""); }}>Cancel</button>
                          </div>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {tab === "nix" && (
            <div>
              <div className="sd-callout sd-callout-info" style={{ marginBottom:12 }}>
                <Icon name="file" size={13} />
                <div style={{ fontSize:12 }}>
                  Add these options to your NixOS module to harden <span className="mono">{svc.name}</span>.
                </div>
              </div>
              <pre className="sd-nix" style={{ maxHeight:"45vh" }}>{svc.nixSnippet}</pre>
            </div>
          )}

          {tab === "all" && (
            <table className="sys-table">
              <thead><tr>
                <th>Directive</th><th>Category</th><th>Points</th><th>Status</th>
              </tr></thead>
              <tbody>
                {SD_COLS.map((col, ci) => {
                  const def = SD_COLS_DEFS[ci] || {};
                  const waived = !svc.enabled[ci] && waivers[col];
                  return (
                    <tr key={col}>
                      <td><span className="mono" style={{ fontSize:12, fontWeight:600 }}>{col}</span></td>
                      <td style={{ fontSize:12, color:"var(--cf-text-muted)" }}>security</td>
                      <td className="mono" style={{ fontSize:12 }}>—</td>
                      <td>
                        {svc.enabled[ci]
                          ? <span className="chip chip-healthy">enforced</span>
                          : waived
                            ? <span className="chip chip-info" title={waivers[col].text}>waived</span>
                            : <span className="chip chip-critical">missing</span>}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>

        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring xs"><Icon name="download" size={12} /> Export report</button>
          <button className="btn btn-primary focus-ring" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}

// Placeholder — used in tab="all"
const SD_COLS_DEFS = [];

const WAIVER_PRESETS = [
  "Not applicable — service runs in an isolated container.",
  "Compensating control in place (AppArmor/SELinux confinement).",
  "Enforcing breaks required functionality; risk accepted.",
  "Upstream unit limitation — cannot be enforced here.",
];

Object.assign(window, { HardeningTab, HardeningDetailModal, SD_COLS });
