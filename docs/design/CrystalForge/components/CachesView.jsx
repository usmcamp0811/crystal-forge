// Caches view — binary cache destinations registry

function CachesView() {
  const [query, setQuery] = React.useState("");
  const [editCache, setEditCache] = React.useState(null);
  const [addOpen, setAddOpen] = React.useState(false);

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
        <span className="filter-count">{caches.length} caches</span>
      </div>

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
            {caches.map(c => <CacheRow key={c.id} cache={c} onEdit={()=>setEditCache(c)}/>)}
          </tbody>
        </table>
      </div>

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

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(620px,96vw)", maxHeight:"92vh" }}>
        <div className="modal-head">
          <h2>
            <Icon name={isEdit ? "gear" : "plus"} size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>
            {isEdit ? `Edit ${cache.name}` : "Add cache destination"}
          </h2>
          <p>{isEdit ? "Update binary cache destination." : "Register a new binary cache destination."}</p>
        </div>
        <div className="modal-body" style={{ overflowY:"auto" }}>
          <div className="field">
            <label>Name</label>
            <input className="input focus-ring" value={form.name} onChange={e=>set("name",e.target.value)} placeholder="e.g. crystal-forge-prod-cache"/>
          </div>
          <div className="field">
            <label>Type</label>
            <div className="seg">
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
          <label style={{ display:"flex", gap:8, alignItems:"center", fontSize:13, cursor:"pointer" }}>
            <input type="checkbox" checked={form.requiresAuth} onChange={e=>set("requiresAuth",e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
            <span>Requires authentication</span>
          </label>
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
                  {testing === "running" ? "Testing…" : testing === "ok" ? "✓ Connected" : testing === "fail" ? "✗ Failed" : "Test"}
                </button>
              </div>
            </div>
          )}

          <div className="field">
            <label>Assigned environments</label>
            <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
              {ENVIRONMENTS.map(env => (
                <button key={env.name}
                  className={`focus-ring`}
                  onClick={() => toggleEnv(env.name)}
                  style={{
                    padding: "4px 10px",
                    borderRadius: 99,
                    fontSize: 11,
                    border: `1px solid ${form.environments.includes(env.name) ? env.color : "var(--cf-card-border)"}`,
                    background: form.environments.includes(env.name)
                      ? `color-mix(in oklab, ${env.color} 14%, var(--cf-card-bg))`
                      : "transparent",
                    color: form.environments.includes(env.name) ? env.color : "var(--cf-text-secondary)",
                    cursor: "pointer",
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 6,
                    fontFamily: "inherit",
                  }}>
                  <span style={{ width:6, height:6, borderRadius:"50%", background: env.color }}/>
                  {env.name}
                </button>
              ))}
            </div>
            <div className="help">Crystal Forge will push builds for systems in these environments to this cache.</div>
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" onClick={onClose}>
            <Icon name="check" size={13}/> {isEdit ? "Save changes" : "Add cache"}
          </button>
        </div>
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
