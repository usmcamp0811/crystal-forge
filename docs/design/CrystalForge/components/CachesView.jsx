// Caches view — binary cache destinations registry

function CachesView({ focus, onClearFocus, onOpenSystem }) {
  const [query, setQuery] = React.useState("");
  const [viewMode, setViewMode] = React.useState("cards");
  const [editCache, setEditCache] = React.useState(null);
  const [viewCache, setViewCache] = React.useState(null);
  const [addOpen, setAddOpen] = React.useState(false);
  React.useEffect(() => {
    if (focus) {
      const c = CACHE_DESTINATIONS.find(x => x.id === focus || x.name === focus || x.url === focus);
      if (c) setViewCache(c);
      onClearFocus?.();
    }
  }, [focus]);

  const caches = CACHE_DESTINATIONS.filter(c =>
    !query ||
    c.name.toLowerCase().includes(query.toLowerCase()) ||
    c.url.toLowerCase().includes(query.toLowerCase())
  );

  const totals = {
    total: CACHE_DESTINATIONS.length,
    healthy: CACHE_DESTINATIONS.filter(c => c.status === "healthy").length,
    issues: CACHE_DESTINATIONS.filter(c => c.status !== "healthy").length,
    paths: CACHE_DESTINATIONS.reduce((a,c) => a + (c.paths || 0), 0),
  };

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">Caches</h1>
          <p className="page-subtitle">
            {totals.total} destinations · {totals.healthy} healthy · {totals.paths.toLocaleString()} paths cached
          </p>
        </div>
        <button className="btn btn-primary focus-ring" data-coach-target="cache" onClick={() => setAddOpen(true)}>
          <Icon name="plus" size={14}/> Add cache
        </button>
      </div>

      <div className="stat-strip">
        {[
          { label:"Total caches", val: totals.total,   color:"#a78bfa" },
          { label:"Healthy",      val: totals.healthy, color:"#34d399" },
          { label:"Issues",       val: totals.issues,  color:"#fbbf24" },
          { label:"Paths cached", val: totals.paths.toLocaleString(), color:"#60a5fa" },
        ].map(s => (
          <div key={s.label} className="stat">
            <span className="stat-accent" style={{ "--stat-color": s.color }}/>
            <div className="stat-label">{s.label}</div>
            <div className="stat-value">{s.val}</div>
          </div>
        ))}
      </div>

      <div className="filterbar">
        <div className="filter-search" style={{ maxWidth:320 }}>
          <Icon name="search"/>
          <input className="input focus-ring" placeholder="Search caches…" value={query} onChange={e=>setQuery(e.target.value)}/>
        </div>
        <div className="seg">
          <button className={viewMode==="cards"?"active":""} onClick={()=>setViewMode("cards")}><Icon name="grid" size={12}/> Cards</button>
          <button className={viewMode==="table"?"active":""} onClick={()=>setViewMode("table")}><Icon name="rows" size={12}/> Table</button>
        </div>
        <span className="filter-count">{caches.length} caches</span>
      </div>

      {viewMode === "cards" ? (
        <div className="cards-grid">
          {caches.map(c => <CacheCard key={c.id} cache={c} onEdit={()=>setViewCache(c)}/>)}
        </div>
      ) : (
      <div className="card" style={{ overflow:"hidden" }}>
        <table className="sys-table">
          <thead>
            <tr>
              <th>Cache</th>
              <th>Type</th>
              <th>Status</th>
              <th>Storage</th>
              <th>Paths</th>
              <th>Last push</th>
              <th>Environments</th>
              <th style={{ textAlign:"right" }}> </th>
            </tr>
          </thead>
          <tbody>
            {caches.map(c => <CacheRow key={c.id} cache={c} onEdit={()=>setViewCache(c)}/>)}
          </tbody>
        </table>
      </div>
      )}

      {viewCache && (
        <CachePanel cache={viewCache} onClose={() => setViewCache(null)} onEdit={() => setEditCache(viewCache)} onOpenSystem={onOpenSystem} />
      )}
      {(editCache || addOpen) && (
        <CacheFormModal
          mode={addOpen ? "add" : "edit"}
          cache={editCache}
          onClose={() => { setEditCache(null); setAddOpen(false); }}
        />
      )}
    </div>
  );
}

// Card view — mirrors EnvCard's layout/rail/foot pattern for cross-view consistency
function CacheCard({ cache, onEdit }) {
  const status = {
    healthy: { cls:"chip-healthy", color:"#34d399", label:"healthy" },
    warning: { cls:"chip-warning", color:"#fbbf24", label:"warning" },
    error:   { cls:"chip-critical",color:"#f87171", label:"error" },
  }[cache.status] || { cls:"chip-unknown", color:"#6b7280", label:cache.status };
  const typeIcon = { s3:"download", attic:"download", nix:"link" }[cache.type] || "download";
  const pct = cache.storage ? (cache.storage.used / cache.storage.total) * 100 : null;

  return (
    <div className="env-card" onClick={onEdit} style={{ cursor:"pointer" }}>
      <div className="env-card-rail" style={{ background: status.color }}/>
      <div className="env-card-head">
        <div>
          <div className="env-card-title">
            <Icon name={typeIcon} size={13} style={{ opacity:0.7 }}/>
            <span>{cache.name}</span>
            {cache.readonly && <span className="chip chip-info" style={{ fontSize:9 }}>system</span>}
          </div>
          <div className="env-card-desc mono">{cache.url}</div>
        </div>
        <div style={{ display:"flex", gap:4 }}>
          <button className="btn-icon focus-ring" title="Edit" onClick={(e)=>{e.stopPropagation();onEdit();}}>
            <Icon name="gear" size={14}/>
          </button>
        </div>
      </div>

      <div style={{ display:"flex", gap:8, flexWrap:"wrap", padding:"0 16px" }}>
        <span className={`chip ${status.cls}`} title={cache.statusReason || status.label}>
          <span className="chip-dot" style={{ background: status.color }}/>
          {status.label}
        </span>
        <span className="chip chip-unknown mono">{cache.type}</span>
      </div>

      <div style={{ padding:"12px 16px 0" }}>
        {cache.storage ? (
          <>
            <div style={{ fontSize:11, color:"var(--cf-text-secondary)", marginBottom:4 }}>
              <span className="mono">{cache.storage.used}/{cache.storage.total} {cache.storage.unit}</span> used
            </div>
            <div style={{ height:5, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
              <div style={{ width:`${pct}%`, height:"100%", background: pct > 85 ? "#fbbf24" : "#34d399" }}/>
            </div>
          </>
        ) : <div style={{ fontSize:11, color:"var(--cf-text-muted)" }}>No storage data.</div>}
      </div>

      <div className="env-card-foot">
        <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>
          {cache.paths ? cache.paths.toLocaleString() : "—"} paths · {cache.lastPush || "never pushed"}
        </span>
        <div style={{ display:"flex", gap:4, flexWrap:"wrap", justifyContent:"flex-end" }}>
          {cache.environments.length === 0
            ? <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>no environments</span>
            : cache.environments.slice(0, 3).map(env => <EnvBadge key={env} env={env}/>)}
          {cache.environments.length > 3 && <span className="chip chip-unknown" style={{ fontSize:10 }}>+{cache.environments.length - 3}</span>}
        </div>
      </div>
    </div>
  );
}

function CacheRow({ cache, onEdit }) {
  const status = {
    healthy: { cls:"chip-healthy", color:"#34d399", label:"healthy" },
    warning: { cls:"chip-warning", color:"#fbbf24", label:"warning" },
    error:   { cls:"chip-critical",color:"#f87171", label:"error" },
  }[cache.status] || { cls:"chip-unknown", color:"#6b7280", label:cache.status };

  const typeIcon = { s3:"download", attic:"download", nix:"link" }[cache.type] || "download";

  return (
    <tr style={{ cursor:"pointer" }} onClick={onEdit}>
      <td>
        <div style={{ fontWeight:600, fontSize:13, display:"flex", alignItems:"center", gap:6 }}>
          <Icon name={typeIcon} size={12} style={{ opacity:0.6 }}/>
          {cache.name}
          {cache.readonly && <span className="chip chip-info" style={{ fontSize:9 }}>system</span>}
        </div>
        <div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{cache.url}</div>
      </td>
      <td><span className="chip chip-unknown mono" style={{ fontSize:10 }}>{cache.type}</span></td>
      <td>
        <span className={`chip ${status.cls}`} title={cache.statusReason || status.label}>
          <span className="chip-dot" style={{ background: status.color }}/>
          {status.label}
        </span>
      </td>
      <td>
        <div style={{ minWidth:120, height:30, display:"flex", flexDirection:"column", justifyContent:"center", gap:3 }}>
          {cache.storage && (
            <>
              <div style={{ fontSize:11, color:"var(--cf-text-secondary)" }}>
                <span className="mono">{cache.storage.used}/{cache.storage.total} {cache.storage.unit}</span>
              </div>
              <div style={{ height:4, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
                <div style={{
                  width:`${(cache.storage.used / cache.storage.total) * 100}%`,
                  height:"100%",
                  background: cache.storage.used / cache.storage.total > 0.85 ? "#fbbf24" : "#34d399"
                }}/>
              </div>
            </>
          )}
        </div>
      </td>
      <td className="mono" style={{ fontSize:12 }}>{cache.paths ? cache.paths.toLocaleString() : "—"}</td>
      <td style={{ fontSize:12, color:"var(--cf-text-secondary)" }}>{cache.lastPush || "—"}</td>
      <td>
        <div style={{ display:"flex", gap:4, flexWrap:"wrap" }}>
          {cache.environments.length === 0 && <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>none</span>}
          {cache.environments.slice(0, 3).map(env => <EnvBadge key={env} env={env}/>)}
          {cache.environments.length > 3 && (
            <span className="chip chip-unknown" style={{ fontSize:10 }}>+{cache.environments.length - 3}</span>
          )}
        </div>
      </td>
      <td>
        <div className="row-actions">
          <button className="btn-icon focus-ring" title="Edit" onClick={(e)=>{ e.stopPropagation(); onEdit(); }}>
            <Icon name="gear" size={14}/>
          </button>
        </div>
      </td>
    </tr>
  );
}

// Side panel — cache reference peek, with Edit handing off to the form modal
function CachePanel({ cache, onClose, onEdit, onOpenSystem }) {
  const usingSystems = SYSTEMS.filter(s => cache.environments.includes(s.environment));
  const status = {
    healthy: { cls:"chip-healthy", color:"#34d399", label:"healthy" },
    warning: { cls:"chip-warning", color:"#fbbf24", label:"warning" },
    error:   { cls:"chip-critical",color:"#f87171", label:"error" },
  }[cache.status] || { cls:"chip-unknown", color:"#6b7280", label:cache.status };
  const typeIcon = { s3:"download", attic:"download", nix:"link" }[cache.type] || "download";

  return (
    <>
      <div className="side-panel-backdrop" onClick={onClose} />
      <aside className="side-panel" role="dialog" aria-modal="true">
        <div className="panel-head">
          <div className="panel-title">
            <h2>
              <Icon name={typeIcon} size={14} style={{ opacity:0.7 }} />
              {cache.name}
              {cache.readonly && <span className="chip chip-info" style={{ fontSize:9 }}>system</span>}
            </h2>
            <span className="fqdn mono">{cache.url}</span>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close">
            <Icon name="x" size={16} />
          </button>
        </div>
        <div className="panel-body">
          <section className="panel-section">
            <div style={{ display:"flex", gap:8, flexWrap:"wrap" }}>
              <span className={`chip ${status.cls}`} title={cache.statusReason || status.label}>
                <span className="chip-dot" style={{ background: status.color }}/>
                {status.label}
              </span>
              <span className="chip chip-unknown mono">{cache.type}</span>
            </div>
          </section>

          <section className="panel-section">
            <h3>Storage</h3>
            {cache.storage ? (
              <>
                <div style={{ fontSize:12, color:"var(--cf-text-secondary)", marginBottom:6 }}>
                  <span className="mono">{cache.storage.used}/{cache.storage.total} {cache.storage.unit}</span> used
                </div>
                <div style={{ height:6, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
                  <div style={{
                    width:`${(cache.storage.used / cache.storage.total) * 100}%`,
                    height:"100%",
                    background: cache.storage.used / cache.storage.total > 0.85 ? "#fbbf24" : "#34d399"
                  }}/>
                </div>
              </>
            ) : <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>No storage data.</div>}
          </section>

          <section className="panel-section">
            <h3>Details</h3>
            <dl className="kv-grid">
              <dt>Paths cached</dt><dd className="mono">{cache.paths ? cache.paths.toLocaleString() : "—"}</dd>
              <dt>Last push</dt><dd>{cache.lastPush || "—"}</dd>
              <dt>Auth</dt><dd>{cache.requiresAuth ? (cache.credId || "required") : "not required"}</dd>
            </dl>
          </section>

          <section className="panel-section">
            <h3>Environments ({cache.environments.length})</h3>
            <div style={{ display:"flex", gap:6, flexWrap:"wrap" }}>
              {cache.environments.length ? cache.environments.map(env => <EnvBadge key={env} env={env}/>) : <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>none assigned</span>}
            </div>
          </section>

          <section className="panel-section">
            <h3>Systems using this cache ({usingSystems.length})</h3>
            <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
              {usingSystems.slice(0, 8).map(s => (
                <div key={s.id} className="sd-commit-sha-link" style={{ display:"flex", alignItems:"center", gap:8, fontSize:12.5, padding:"3px 4px", margin:"-3px -4px" }} onClick={() => onOpenSystem?.(s)}>
                  <span className="status-dot" style={{ "--status-color": s.statusColor }} />
                  <span className="mono truncate" style={{ flex:1 }}>{s.hostname}</span>
                  <EnvBadge env={s.environment} />
                </div>
              ))}
              {usingSystems.length > 8 && <div style={{ fontSize:11, color:"var(--cf-text-muted)" }}>+{usingSystems.length - 8} more</div>}
              {!usingSystems.length && <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>No systems in an assigned environment yet.</div>}
            </div>
          </section>
        </div>
        <div className="panel-actions">
          <button className="btn btn-primary focus-ring" onClick={onEdit}><Icon name="gear" size={12} /> Edit cache</button>
        </div>
      </aside>
    </>
  );
}

function CacheFormModal({ mode, cache, onClose }) {
  const isEdit = mode === "edit";
  const [form, setForm] = React.useState(() => isEdit && cache ? {
    name: cache.name,
    type: cache.type,
    url: cache.url,
    requiresAuth: cache.requiresAuth,
    credId: cache.credId || "",
    environments: cache.environments || [],
  } : {
    name: "",
    type: "s3",
    url: "",
    requiresAuth: true,
    credId: "",
    environments: [],
  });
  const [testing, setTesting] = React.useState(null);
  const [addCredOpen, setAddCredOpen] = React.useState(false);
  const set = (k,v) => setForm(p => ({ ...p, [k]: v }));
  const toggleEnv = (env) => set("environments", form.environments.includes(env)
    ? form.environments.filter(e => e !== env)
    : [...form.environments, env]);

  const [section, setSection] = React.useState("dest");
  const sections = [
    { id:"dest",  label:"Destination",  icon:"download" },
    { id:"auth",  label:"Credentials",  icon:"key" },
    { id:"envs",  label:"Environments", icon:"grid" },
  ];
  const typeLabel = form.type === "s3" ? "S3" : form.type === "attic" ? "Attic" : "Nix HTTPS";

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="pe-shell" onClick={e=>e.stopPropagation()}>
        <header className="pe-head">
          <div style={{ minWidth:0, display:"flex", flexDirection:"column", gap:3 }}>
            <div style={{ display:"flex", alignItems:"center", gap:9, minWidth:0 }}>
              <Icon name={isEdit ? "gear" : "plus"} size={15} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
              <span className="pe-head-title">{isEdit ? (form.name || cache.name) : (form.name || "Add cache destination")}</span>
              <span className="chip chip-info">{typeLabel}</span>
              {form.requiresAuth && !form.credId && <span className="chip" title="Pick a credential or turn off authentication.">No credential</span>}
              {!form.url.trim() && <span className="chip" title="A cache URL is required.">No URL</span>}
            </div>
            <span className="pe-head-sub">{isEdit ? "Update binary cache destination." : "Register a new binary cache destination."}</span>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={16}/></button>
        </header>

        <nav className="pe-rail">
          {sections.map(s => (
            <button key={s.id} className={`pe-rail-item focus-ring${section===s.id?" active":""}`} onClick={()=>setSection(s.id)}>
              <Icon name={s.icon} size={13}/>
              <span className="pe-rail-label">{s.label}</span>
              {s.id === "dest" && !form.url.trim() && <span className="pe-rail-badge warn">!</span>}
              {s.id === "auth" && <span className={`pe-rail-badge${form.requiresAuth && !form.credId ? " warn" : ""}`}>{form.requiresAuth ? (form.credId || "!") : "none"}</span>}
              {s.id === "envs" && <span className="pe-rail-badge">{form.environments.length}</span>}
            </button>
          ))}
        </nav>

        <div className="pe-body">
          {section === "dest" && (
            <>
              <div className="pe-sec-head">
                <h3>Destination</h3>
                <p>What this cache is called, what kind of store it is, and where it lives.</p>
              </div>
              <div className="field">
                <label>Name</label>
                <input className="input focus-ring" value={form.name} onChange={e=>set("name",e.target.value)} placeholder="e.g. crystal-forge-prod-cache"/>
              </div>
              <div className="field">
                <label>Type</label>
                <div className="seg" style={{ width:"fit-content", flexWrap:"wrap" }}>
                  {[
                    { v:"s3",   l:"S3-compatible" },
                    { v:"attic",l:"Attic" },
                    { v:"nix",  l:"Nix HTTPS" },
                  ].map(o => (
                    <button key={o.v} className={form.type === o.v ? "active" : ""} onClick={()=>set("type", o.v)}>{o.l}</button>
                  ))}
                </div>
              </div>
              <div className="field">
                <label>URL</label>
                <input className="input focus-ring mono" value={form.url} onChange={e=>set("url",e.target.value)} style={{ fontSize:12 }}
                  placeholder={form.type === "s3" ? "s3://bucket?region=us-east-1" : form.type === "attic" ? "attic://host/cache" : "https://cache.nixos.org"}/>
              </div>
            </>
          )}

          {section === "auth" && (
            <>
              <div className="pe-sec-head">
                <h3>Credentials</h3>
                <p>Saved credentials can be reused across caches. Public read-only substituters need none.</p>
              </div>
              <div className="field">
                <label className="focus-ring" style={{ display:"flex", gap:9, alignItems:"flex-start", cursor:"pointer", margin:0, textTransform:"none", letterSpacing:0 }}>
                  <input type="checkbox" checked={form.requiresAuth} onChange={e=>set("requiresAuth",e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)", marginTop:1 }}/>
                  <span style={{ minWidth:0 }}>
                    <span style={{ display:"block", fontSize:13, fontWeight:600 }}>Requires authentication</span>
                    <span className="help" style={{ display:"block", marginTop:3, fontWeight:400 }}>
                      {form.requiresAuth
                        ? "Crystal Forge signs pushes with the credential below."
                        : "Off: anonymous access only — fine for a public read-only substituter."}
                    </span>
                  </span>
                </label>
              </div>
              {form.requiresAuth && (
                <div className="field">
                  <label>Credential</label>
                  <div style={{ display:"flex", gap:8 }}>
                    <select className="input focus-ring" value={form.credId} onChange={e=>{
                      if (e.target.value === "__new__") { setAddCredOpen(true); }
                      else set("credId",e.target.value);
                    }} style={{ flex:1 }}>
                      <option value="">Select a credential…</option>
                      <option value="aws-prod-role">aws-prod-role (IAM role)</option>
                      <option value="aws-staging-role">aws-staging-role (IAM role)</option>
                      <option value="attic-token-dev">attic-token-dev (Attic token)</option>
                      <option value="__new__">+ Add new credential…</option>
                    </select>
                    <button className="btn btn-ghost focus-ring xs" onClick={()=>setTesting("running") || setTimeout(()=>setTesting(Math.random()>0.2?"ok":"fail"),700)} disabled={!form.credId}>
                      {testing === "running" ? <><Spinner size={11}/> Testing…</>
                      : testing === "ok"     ? <><Icon name="check" size={11} style={{color:"#34d399"}}/> Connected</>
                      : testing === "fail"   ? <><Icon name="warn" size={11} style={{color:"#f87171"}}/> Failed</>
                      : <>Test</>}
                    </button>
                  </div>
                </div>
              )}
            </>
          )}

          {section === "envs" && (
            <>
              <div className="pe-sec-head">
                <h3>Assigned environments</h3>
                <p>Crystal Forge pushes builds for systems in these environments to this cache.</p>
              </div>
              <div className="field">
                <label>Environments</label>
                <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
                  {ENVIRONMENTS.map(env => (
                    <button key={env.name}
                      className="focus-ring"
                      onClick={() => toggleEnv(env.name)}
                      style={{
                        padding: "6px 12px",
                        borderRadius: 99,
                        fontSize: 12,
                        fontWeight: 600,
                        border: `1px solid ${form.environments.includes(env.name) ? env.color : "var(--cf-card-border)"}`,
                        background: form.environments.includes(env.name)
                          ? `color-mix(in oklab, ${env.color} 14%, var(--cf-card-bg))`
                          : "transparent",
                        color: form.environments.includes(env.name) ? "var(--cf-text-primary)" : "var(--cf-text-muted)",
                        cursor: "pointer",
                        display: "inline-flex",
                        alignItems: "center",
                        gap: 7,
                        fontFamily: "inherit",
                      }}>
                      <span style={{ width:8, height:8, borderRadius:"50%", background: env.color }}/>
                      {env.name}
                      {form.environments.includes(env.name) && <Icon name="check" size={11}/>}
                    </button>
                  ))}
                </div>
                {form.environments.length === 0 && (
                  <div className="help">Unassigned — nothing is pushed here until an environment is selected.</div>
                )}
              </div>
            </>
          )}
        </div>

        <footer className="pe-foot">
          <span className="pe-foot-state">
            {form.name.trim() || "Unnamed cache"}
            <span className="pe-foot-dot">·</span>
            {typeLabel}
            <span className="pe-foot-dot">·</span>
            {form.requiresAuth ? (form.credId || "credential required") : "no auth"}
            <span className="pe-foot-dot">·</span>
            {form.environments.length} env{form.environments.length === 1 ? "" : "s"}
          </span>
          <div style={{ display:"flex", gap:8 }}>
            <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
            <button className="btn btn-primary focus-ring" onClick={onClose}>
              <Icon name="check" size={13}/> {isEdit ? "Save changes" : "Add cache"}
            </button>
          </div>
        </footer>
      </div>
      {addCredOpen && (
        <CacheCredModal
          type={form.type}
          onClose={(newId) => {
            setAddCredOpen(false);
            if (newId) set("credId", newId);
          }}
        />
      )}
    </div>
  );
}

function CacheCredModal({ type, onClose }) {
  const [kind, setKind] = React.useState(type === "s3" ? "aws-key" : "token");
  const [form, setForm] = React.useState({
    name: "",
    accessKey: "",
    secretKey: "",
    token: "",
    roleArn: "",
  });
  const set = (k,v) => setForm(p => ({ ...p, [k]: v }));

  return (
    <div className="modal-backdrop" onClick={()=>onClose(null)} style={{ zIndex:95 }}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(520px,96vw)" }}>
        <div className="modal-head">
          <h2>
            <Icon name="key" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>
            Add credential
          </h2>
          <p>Saved credentials can be reused across caches. Secrets are encrypted at rest.</p>
        </div>
        <div className="modal-body">
          <div className="field">
            <label>Name</label>
            <input className="input focus-ring" value={form.name} onChange={e=>set("name",e.target.value)} placeholder="e.g. aws-prod-role"/>
          </div>
          <div className="field">
            <label>Type</label>
            <div className="seg">
              {(type === "s3"
                ? [{ v:"aws-key", l:"AWS access key" }, { v:"aws-role", l:"IAM role (IRSA)" }]
                : [{ v:"token", l:"API token" }]
              ).map(o => (
                <button key={o.v} className={kind === o.v ? "active" : ""} onClick={()=>setKind(o.v)}>{o.l}</button>
              ))}
            </div>
          </div>
          {kind === "aws-key" && (
            <>
              <div className="field">
                <label>Access key ID</label>
                <input className="input focus-ring mono" value={form.accessKey} onChange={e=>set("accessKey",e.target.value)} placeholder="AKIA…" style={{ fontSize:12 }}/>
              </div>
              <div className="field">
                <label>Secret access key</label>
                <input type="password" className="input focus-ring mono" value={form.secretKey} onChange={e=>set("secretKey",e.target.value)} placeholder="•••••••••••••••••" style={{ fontSize:12 }}/>
              </div>
            </>
          )}
          {kind === "aws-role" && (
            <div className="field">
              <label>Role ARN</label>
              <input className="input focus-ring mono" value={form.roleArn} onChange={e=>set("roleArn",e.target.value)} placeholder="arn:aws:iam::123456789012:role/cache-pusher" style={{ fontSize:12 }}/>
              <div className="help">Crystal Forge must be running with permission to assume this role.</div>
            </div>
          )}
          {kind === "token" && (
            <div className="field">
              <label>Token</label>
              <input type="password" className="input focus-ring mono" value={form.token} onChange={e=>set("token",e.target.value)} placeholder="•••••••••••••••••" style={{ fontSize:12 }}/>
              <div className="help">Attic / cache-server bearer token with push permission.</div>
            </div>
          )}
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={()=>onClose(null)}>Cancel</button>
          <button className="btn btn-primary focus-ring" disabled={!form.name} onClick={()=>onClose(`cred-${form.name.toLowerCase().replace(/[^a-z0-9]+/g,'-')}`)}>
            <Icon name="check" size={13}/> Save credential
          </button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { CachesView });
