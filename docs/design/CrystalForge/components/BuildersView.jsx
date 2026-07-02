// Builders view — fleet of build workers

// Click-to-copy key fingerprint — the identifier the server knows a builder by.
function BuilderFpChip({ fp, size = "sm" }) {
  const [copied, setCopied] = React.useState(false);
  const copy = (e) => {
    e.stopPropagation();
    if (navigator.clipboard) navigator.clipboard.writeText(fp).catch(()=>{});
    setCopied(true);
    setTimeout(()=>setCopied(false), 1500);
  };
  return (
    <button className="builder-id-chip focus-ring" onClick={copy} title="Copy key fingerprint">
      <Icon name="key" size={size === "lg" ? 12 : 10}/>
      <span className="mono">{fp}</span>
      <Icon name={copied ? "check" : "file"} size={size === "lg" ? 12 : 10} style={{ opacity: copied ? 1 : 0.55, color: copied ? "#34d399" : "inherit" }}/>
    </button>
  );
}

function BuildersView(props) {
  const [query, setQuery] = React.useState("");
  const [statusFilter, setStatusFilter] = React.useState("all");
  const [archFilter, setArchFilter] = React.useState("all");
  const [viewMode, setViewMode] = React.useState(props.defaultView || "cards");
  React.useEffect(() => { if (props.defaultView) setViewMode(props.defaultView); }, [props.defaultView]);
  const [edit, setEdit] = React.useState(null);
  const [addOpen, setAddOpen] = React.useState(false);

  const arches = [...new Set(BUILD_WORKERS.map(w => w.arch))];

  const filtered = BUILD_WORKERS.filter(w => {
    if (statusFilter !== "all" && w.status !== statusFilter) return false;
    if (archFilter !== "all" && w.arch !== archFilter) return false;
    if (query) {
      const q = query.toLowerCase();
      if (!w.name.toLowerCase().includes(q) && !w.host.toLowerCase().includes(q) && !w.arch.toLowerCase().includes(q)) return false;
    }
    return true;
  });

  const totals = {
    total: BUILD_WORKERS.length,
    running: BUILD_WORKERS.filter(w => w.status === "running").length,
    slotsUsed: BUILD_WORKERS.reduce((a,w) => a + w.slots.used, 0),
    slotsTotal: BUILD_WORKERS.reduce((a,w) => a + w.slots.total, 0),
    completed: BUILD_WORKERS.reduce((a,w) => a + w.completed24h, 0),
    failed: BUILD_WORKERS.reduce((a,w) => a + w.failed24h, 0),
  };
  const slotPct = totals.slotsTotal > 0 ? Math.round((totals.slotsUsed / totals.slotsTotal) * 100) : 0;

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">Builders</h1>
          <p className="page-subtitle">
            {totals.running} of {totals.total} running · {totals.slotsUsed}/{totals.slotsTotal} slots used · {totals.completed.toLocaleString()} builds in last 24h
          </p>
        </div>
        <button className="btn btn-primary focus-ring" data-coach-target="builder" onClick={() => setAddOpen(true)}>
          <Icon name="plus" size={14}/> Register builder
        </button>
      </div>

      <div className="stat-strip">
        {[
          { label:"Total",       val:totals.total,     color:"#a78bfa" },
          { label:"Running",     val:totals.running,   color:"#34d399" },
          { label:"Slot use",    val:`${slotPct}%`,    color:slotPct > 85 ? "#fbbf24" : "#60a5fa" },
          { label:"Built 24h",   val:totals.completed.toLocaleString(), color:"#34d399" },
          { label:"Failed 24h",  val:totals.failed,    color:totals.failed > 0 ? "#f87171" : "#34d399" },
        ].map(s => (
          <div key={s.label} className="stat">
            <span className="stat-accent" style={{ "--stat-color": s.color }}/>
            <div className="stat-label">{s.label}</div>
            <div className="stat-value" style={{ color: s.color }}>{s.val}</div>
          </div>
        ))}
      </div>

      <div className="filterbar">
        <div className="filter-search" style={{ maxWidth:320 }}>
          <Icon name="search"/>
          <input className="input focus-ring" placeholder="Search builders…" value={query} onChange={e=>setQuery(e.target.value)}/>
        </div>
        <div className="seg">
          {["all","running","paused","offline"].map(k => (
            <button key={k} className={statusFilter===k?"active":""} onClick={()=>setStatusFilter(k)}>{k}</button>
          ))}
        </div>
        <select className="input filter-select focus-ring" style={{ width:"auto" }} value={archFilter} onChange={e=>setArchFilter(e.target.value)}>
          <option value="all">All architectures</option>
          {arches.map(a => <option key={a} value={a}>{a}</option>)}
        </select>
        <div className="seg">
          <button className={viewMode==="cards"?"active":""} onClick={()=>setViewMode("cards")}><Icon name="grid" size={12}/> Cards</button>
          <button className={viewMode==="table"?"active":""} onClick={()=>setViewMode("table")}><Icon name="rows" size={12}/> Table</button>
        </div>
        <span className="filter-count">{filtered.length} builders</span>
      </div>

      {viewMode === "cards" ? (
        <div className="cards-grid">
          {filtered.map(w => <BuilderCard key={w.id} w={w} onEdit={()=>setEdit(w)}/>)}
        </div>
      ) : (
        <div className="card" style={{ overflow:"hidden" }}>
          <table className="sys-table">
            <thead>
              <tr>
                <th>Builder</th>
                <th>Status</th>
                <th>Arch · envs</th>
                <th>Resources</th>
                <th>Slot use</th>
                <th>Built 24h</th>
                <th>Last seen</th>
                <th style={{ textAlign:"right" }}> </th>
              </tr>
            </thead>
            <tbody>
              {filtered.map(w => <BuilderRow key={w.id} w={w} onEdit={()=>setEdit(w)}/>)}
            </tbody>
          </table>
        </div>
      )}

      {(edit || addOpen) && (
        <BuilderFormModal
          mode={addOpen ? "add" : "edit"}
          builder={edit}
          onClose={() => { setEdit(null); setAddOpen(false); }}
        />
      )}
    </div>
  );
}

function builderStatusChip(w) {
  const cfg = {
    running: { cls:"chip-healthy",  dot:"#34d399", label:"running" },
    paused:  { cls:"chip-warning",  dot:"#fbbf24", label:"paused"  },
    offline: { cls:"chip-critical", dot:"#f87171", label:"offline" },
    draining:{ cls:"chip-info",     dot:"#60a5fa", label:"draining"},
  }[w.status] || { cls:"chip-unknown", dot:"#6b7280", label:w.status };
  return <span className={`chip ${cfg.cls}`}><span className="chip-dot" style={{ background:cfg.dot }}/>{cfg.label}</span>;
}

function BuilderCard({ w, onEdit }) {
  const slotPct = w.slots.total ? Math.round((w.slots.used/w.slots.total)*100) : 0;
  const rail = w.status === "running" ? "#34d399" : w.status === "paused" ? "#fbbf24" : "#f87171";
  return (
    <div className="sys-card">
      <div className="status-rail" style={{ "--status-color": rail }}/>
      <div className="sys-card-head">
        <div className="sys-title">
          <div className="sys-hostname"><Icon name="cpu" size={13}/>&nbsp;{w.name}</div>
          <div className="sys-fqdn">{w.host}</div>
        </div>
        {builderStatusChip(w)}
      </div>
      {w.registered === false && (
        <div className="builder-pending-banner" style={{ flexDirection:"column", alignItems:"stretch", gap:6 }}>
          <span style={{ display:"flex", alignItems:"center", gap:7 }}>
            <Icon name="warn" size={12}/>
            <span>Connected but <strong>not registered</strong> — match this key to recognize it.</span>
          </span>
          <BuilderFpChip fp={w.fingerprint}/>
        </div>
      )}
      <div className="sys-card-body">
        <div><div className="sys-kv-key">Arch</div><div className="sys-kv-val">{w.arch}</div></div>
        <div><div className="sys-kv-key">Cores · mem</div><div className="sys-kv-val" style={{fontFamily:"inherit"}}>{w.cores}c · {w.mem} GiB</div></div>
        <div><div className="sys-kv-key">Environments</div>
          <div className="sys-kv-val" style={{ fontFamily:"inherit", display:"flex", gap:4, flexWrap:"wrap" }}>
            {(w.environments || []).map(e => <EnvBadge key={e} env={e}/>)}
            {(!w.environments || w.environments.length === 0) && <span style={{ color:"var(--cf-text-muted)", fontStyle:"italic", fontSize:11 }}>none</span>}
          </div>
        </div>
        <div><div className="sys-kv-key">Last seen</div><div className="sys-kv-val" style={{fontFamily:"inherit"}}>{w.lastSeen}</div></div>
      </div>
      <div>
        <div style={{ display:"flex", justifyContent:"space-between", fontSize:11, color:"var(--cf-text-muted)", marginBottom:4 }}>
          <span>Slot use</span><span className="mono">{w.slots.used}/{w.slots.total} · {slotPct}%</span>
        </div>
        <div style={{ height:5, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
          <div style={{ width:`${slotPct}%`, height:"100%", background: slotPct > 85 ? "#fbbf24" : "#34d399" }}/>
        </div>
      </div>
      <div>
        <div style={{ display:"flex", justifyContent:"space-between", fontSize:11, color:"var(--cf-text-muted)", marginBottom:4 }}>
          <span>Load</span><span className="mono">{Math.round(w.load*100)}%</span>
        </div>
        <div style={{ height:5, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
          <div style={{ width:`${w.load*100}%`, height:"100%", background: w.load > 0.85 ? "#f87171" : w.load > 0.6 ? "#fbbf24" : "#60a5fa" }}/>
        </div>
      </div>
      <div className="sys-card-foot">
        <div className="chips-row">
          <span className="chip chip-healthy">{w.completed24h.toLocaleString()} built</span>
          {w.failed24h > 0 && <span className="chip chip-critical">{w.failed24h} failed</span>}
        </div>
        <button className="btn btn-subtle focus-ring" style={{ padding:"4px 10px", fontSize:12 }} onClick={e=>{ e.stopPropagation(); onEdit(); }}>
          <Icon name={w.registered === false ? "key" : "gear"} size={12}/> {w.registered === false ? "Register" : "Edit"}
        </button>
      </div>
    </div>
  );
}

function BuilderRow({ w, onEdit }) {
  const slotPct = w.slots.total ? Math.round((w.slots.used/w.slots.total)*100) : 0;
  return (
    <tr style={{ cursor:"pointer" }} onClick={onEdit}>
      <td>
        <div style={{ fontWeight:600, fontSize:13 }}>{w.name}</div>
        <div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{w.host}</div>
      </td>
      <td>
        <div style={{ display:"flex", flexDirection:"column", gap:3 }}>
          {builderStatusChip(w)}
          {w.registered === false && <span className="chip chip-warning" style={{ fontSize:10 }}>unregistered</span>}
        </div>
      </td>
      <td>
        <div className="mono" style={{ fontSize:12 }}>{w.arch}</div>
        <div style={{ fontSize:11, display:"flex", gap:4, flexWrap:"wrap", marginTop:2 }}>
          {(w.environments || []).map(e => <EnvBadge key={e} env={e}/>)}
        </div>
      </td>
      <td className="mono" style={{ fontSize:12 }}>{w.cores}c · {w.mem} GiB</td>
      <td>
        <div style={{ display:"flex", alignItems:"center", gap:8, minWidth:130 }}>
          <div style={{ height:4, flex:1, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
            <div style={{ width:`${slotPct}%`, height:"100%", background: slotPct > 85 ? "#fbbf24" : "#34d399" }}/>
          </div>
          <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{w.slots.used}/{w.slots.total}</span>
        </div>
      </td>
      <td>
        <div style={{ display:"flex", flexDirection:"column", gap:1 }}>
          <span className="mono" style={{ fontSize:12 }}>{w.completed24h.toLocaleString()}</span>
          {w.failed24h > 0 && <span style={{ fontSize:11, color:"#f87171" }}>{w.failed24h} failed</span>}
        </div>
      </td>
      <td style={{ fontSize:12, color:"var(--cf-text-muted)" }}>{w.lastSeen}</td>
      <td>
        <div className="row-actions">
          <button className="btn-icon focus-ring" title={w.registered === false ? "Register" : "Edit"} onClick={e=>{ e.stopPropagation(); onEdit(); }}>
            <Icon name={w.registered === false ? "key" : "gear"} size={14}/>
          </button>
        </div>
      </td>
    </tr>
  );
}

function BuilderFormModal({ mode, builder, onClose }) {
  const isEdit = mode === "edit";
  const [form, setForm] = React.useState(() => isEdit && builder ? {
    name: builder.name,
    host: builder.host,
    arch: builder.arch,
    environments: builder.environments || [],
    cores: builder.cores,
    mem: builder.mem,
    maxSlots: builder.slots.total,
    publicKey: builder.publicKey,
    enabled: builder.status !== "offline",
  } : {
    name: "",
    host: "",
    arch: "x86_64-linux",
    environments: ["production"],
    cores: 16,
    mem: 64,
    maxSlots: 4,
    publicKey: "",
    enabled: true,
  });
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const set = (k,v) => setForm(p => ({ ...p, [k]: v }));

  // Derive a display fingerprint from the pasted public key (mock of `ssh-keygen -lf`).
  const fingerprint = React.useMemo(() => {
    const key = (form.publicKey || "").trim();
    if (!key || key.length < 20) return null;
    const b64chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let seed = 0x811c9dc5; // FNV-style seed for better diffusion
    for (let i = 0; i < key.length; i++) { seed ^= key.charCodeAt(i); seed = Math.imul(seed, 0x01000193) >>> 0; }
    let s = "";
    for (let i = 0; i < 43; i++) {
      seed = (Math.imul(seed, 1103515245) + 12345) >>> 0;
      s += b64chars[(seed >>> 24) % 64]; // high byte — avoids the weak low bits of an LCG
    }
    return `SHA256:${s}`;
  }, [form.publicKey]);

  const keyLooksValid = /^ssh-(ed25519|rsa|ecdsa)/.test((form.publicKey || "").trim());

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(620px,96vw)", maxHeight:"92vh" }}>
        {confirmDelete ? (
          <DeleteBuilderConfirm builder={builder} onCancel={()=>setConfirmDelete(false)} onConfirm={onClose}/>
        ) : (
          <>
            <div className="modal-head">
              <h2>
                <Icon name={isEdit ? "gear" : "plus"} size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>
                {isEdit ? `Edit ${builder.name}` : "Register builder"}
              </h2>
              <p>{isEdit ? "Update builder registration." : "Recognize a build worker by its public key."}</p>
            </div>
            <div className="modal-body" style={{ overflowY:"auto" }}>
              {!isEdit && (
                <div className="sd-callout sd-callout-info" style={{ fontSize:11.5, display:"block", marginBottom:16 }}>
                  <div style={{ display:"flex", alignItems:"center", gap:6, marginBottom:6, fontWeight:600, color:"var(--cf-text-secondary)" }}>
                    <Icon name="server" size={12}/> How registration works
                  </div>
                  <ol style={{ margin:0, paddingLeft:18, lineHeight:1.7, color:"var(--cf-text-secondary)" }}>
                    <li>Deploy the host with <span className="mono">services.crystal-forge.build.api_mode = true</span>. On first start it generates its own keypair and runs — it just won't be recognized yet.</li>
                    <li>Grab the builder's <strong>public</strong> key from that host:<br/><span className="mono" style={{ fontSize:10.5 }}>cat /var/lib/crystal-forge/builder-api.key.pub</span> (also printed in <span className="mono" style={{ fontSize:10.5 }}>journalctl -u crystal-forge-builder</span>).</li>
                    <li>Paste it below to register. The builder is recognized on its next check-in — no redeploy needed.</li>
                  </ol>
                </div>
              )}
              <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
                <div className="field">
                  <label>Name</label>
                  <input className="input focus-ring mono" value={form.name} onChange={e=>set("name",e.target.value)} placeholder="e.g. hydra-03"/>
                </div>
                <div className="field">
                  <label>Environments served</label>
                  <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
                    {ENVIRONMENTS.map(env => {
                      const on = form.environments.includes(env.name);
                      return (
                        <button key={env.name}
                          className="focus-ring"
                          onClick={() => set("environments",
                            on ? form.environments.filter(e => e !== env.name)
                               : [...form.environments, env.name]
                          )}
                          style={{
                            padding: "4px 10px",
                            borderRadius: 99,
                            fontSize: 11,
                            border: `1px solid ${on ? env.color : "var(--cf-card-border)"}`,
                            background: on ? `color-mix(in oklab, ${env.color} 14%, var(--cf-card-bg))` : "transparent",
                            color: on ? env.color : "var(--cf-text-secondary)",
                            cursor: "pointer",
                            display: "inline-flex",
                            alignItems: "center",
                            gap: 6,
                            fontFamily: "inherit",
                          }}>
                          <span style={{ width:6, height:6, borderRadius:"50%", background: env.color }}/>
                          {env.name}
                        </button>
                      );
                    })}
                  </div>
                  <div className="help">Builds for systems in any of these environments will be routed to this worker.</div>
                </div>
              </div>
              <div className="field">
                <label>Host (SSH endpoint)</label>
                <input className="input focus-ring mono" value={form.host} onChange={e=>set("host",e.target.value)} placeholder="hydra-03.production.cf.internal" style={{ fontSize:12 }}/>
              </div>
              <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr 1fr", gap:14 }}>
                <div className="field">
                  <label>Architecture</label>
                  <select className="input focus-ring" value={form.arch} onChange={e=>set("arch",e.target.value)}>
                    {["x86_64-linux","aarch64-linux","aarch64-darwin","x86_64-darwin"].map(a => <option key={a}>{a}</option>)}
                  </select>
                </div>
                <div className="field">
                  <label>Cores</label>
                  <input type="number" className="input focus-ring" min={1} value={form.cores} onChange={e=>set("cores",parseInt(e.target.value,10) || 1)}/>
                </div>
                <div className="field">
                  <label>Memory (GiB)</label>
                  <input type="number" className="input focus-ring" min={1} value={form.mem} onChange={e=>set("mem",parseInt(e.target.value,10) || 1)}/>
                </div>
              </div>
              <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
                <div className="field">
                  <label>Max concurrent slots</label>
                  <input type="number" className="input focus-ring" min={1} value={form.maxSlots} onChange={e=>set("maxSlots",parseInt(e.target.value,10) || 1)}/>
                  <div className="help">How many builds this worker may run in parallel.</div>
                </div>
                <div className="field">
                  <label>Status</label>
                  <label style={{ display:"flex", gap:8, alignItems:"center", fontSize:13, padding:"6px 0" }}>
                    <input type="checkbox" checked={form.enabled} onChange={e=>set("enabled",e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
                    <span>Enabled (accepts jobs)</span>
                  </label>
                </div>
              </div>
              <div className="field">
                <label>Builder public key {!isEdit && <span style={{ color:"#f87171" }}>*</span>}</label>
                <textarea className="input focus-ring mono" rows={3} value={form.publicKey} onChange={e=>set("publicKey",e.target.value)}
                  placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5… crystal-forge@hostname"
                  style={{ fontSize:11, resize:"vertical", padding:10, marginTop:2 }}/>
                <div className="help">
                  The public half of the keypair the builder generated on first start. Crystal Forge uses it to authenticate the builder and verify build signatures — the private key never leaves the builder host.
                </div>
                {form.publicKey.trim() && (
                  <div style={{ marginTop:10, padding:"9px 12px", borderRadius:8,
                    border:`1px solid ${keyLooksValid ? "rgba(52,211,153,0.3)" : "rgba(248,113,113,0.35)"}`,
                    background: keyLooksValid ? "rgba(52,211,153,0.06)" : "rgba(248,113,113,0.06)",
                    display:"flex", alignItems:"center", gap:8 }}>
                    <Icon name={keyLooksValid ? "key" : "warn"} size={13} style={{ color: keyLooksValid ? "#34d399" : "#f87171", flexShrink:0 }}/>
                    {keyLooksValid ? (
                      <div style={{ minWidth:0 }}>
                        <div style={{ fontSize:10, textTransform:"uppercase", letterSpacing:"0.06em", color:"var(--cf-text-muted)", fontWeight:600 }}>Fingerprint</div>
                        <div className="mono" style={{ fontSize:11.5, color:"var(--cf-text-primary)", wordBreak:"break-all" }}>{fingerprint}</div>
                      </div>
                    ) : (
                      <span style={{ fontSize:11.5, color:"#fca5a5" }}>Doesn't look like an SSH public key — expected it to start with <span className="mono">ssh-ed25519</span>.</span>
                    )}
                  </div>
                )}
              </div>
              {isEdit && (
                <div style={{ marginTop:10, paddingTop:14, borderTop:"1px solid var(--cf-divider)" }}>
                  <div style={{ fontSize:11, fontWeight:600, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", marginBottom:8 }}>Danger zone</div>
                  <button className="btn btn-ghost focus-ring" onClick={()=>setConfirmDelete(true)} style={{ color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }}>
                    <Icon name="x" size={12}/> Remove builder
                  </button>
                </div>
              )}
            </div>
            <div className="modal-foot">
              <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
              <button className="btn btn-primary focus-ring" onClick={onClose} disabled={!isEdit && !keyLooksValid}>
                <Icon name="check" size={13}/> {isEdit ? "Save changes" : "Register builder"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function DeleteBuilderConfirm({ builder, onCancel, onConfirm }) {
  const [typed, setTyped] = React.useState("");
  const matches = typed === builder.name;
  const active = builder.slots.used > 0;
  return (
    <>
      <div className="modal-head" style={{ background:"rgba(248,113,113,0.06)" }}>
        <h2 style={{ color:"#fecaca", display:"flex", alignItems:"center", gap:8 }}>
          <Icon name="warn" size={16} style={{ color:"#f87171" }}/>
          Remove builder
        </h2>
        <p>This unregisters <span className="mono" style={{ fontWeight:600 }}>{builder.name}</span> from the build queue.</p>
      </div>
      <div className="modal-body">
        {active && (
          <div className="sd-callout sd-callout-danger" style={{ marginBottom:12 }}>
            <Icon name="warn" size={14}/>
            <div style={{ fontSize:12, color:"#fecaca" }}>
              <strong>{builder.slots.used} build{builder.slots.used === 1 ? "" : "s"} in progress on this worker.</strong> Drain it first or those jobs will be re-queued.
            </div>
          </div>
        )}
        <div className="field">
          <label>Type <span className="mono" style={{ color:"#fecaca", fontWeight:700 }}>{builder.name}</span> to confirm</label>
          <input className="input focus-ring mono"
            placeholder={builder.name}
            value={typed}
            onChange={e=>setTyped(e.target.value)}
            autoFocus
            style={{ borderColor: typed && !matches ? "rgba(248,113,113,0.5)" : undefined }}/>
        </div>
      </div>
      <div className="modal-foot">
        <button className="btn btn-ghost focus-ring" onClick={onCancel}>Cancel</button>
        <button className="btn focus-ring" disabled={!matches} onClick={onConfirm}
          style={{ background: matches ? "#dc2626" : "var(--cf-subtle-bg)", color: matches ? "white" : "var(--cf-text-muted)" }}>
          <Icon name="x" size={13}/> Remove builder
        </button>
      </div>
    </>
  );
}

Object.assign(window, { BuildersView });
