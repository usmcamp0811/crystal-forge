// Server management (admin) — users, roles, OIDC, audit log, server info

function AdminView({ onNavigate, coach, classif, onClassif }) {
  const [tab, setTab] = React.useState("users");
  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">Server Management</h1>
          <p className="page-subtitle">Admin-only · users, access control, audit, and server configuration</p>
        </div>
        <span className="chip chip-critical" style={{ alignSelf:"center" }}><Icon name="shield" size={11}/> admin only</span>
      </div>

      <ServerInfoStrip/>

      <div className="card" style={{ overflow:"hidden" }}>
        <div className="sd-tabs" style={{ padding:"0 16px", borderBottom:"1px solid var(--cf-card-border)" }}>
          {[
            { k:"users", l:"Users",        i:"server" },
            { k:"roles", l:"Roles",        i:"key" },
            { k:"oidc",  l:"OIDC Mappings",i:"link" },
            { k:"jobs",  l:"Background Jobs",i:"sync" },
            { k:"audit", l:"Audit Log",    i:"history" },
            { k:"server",l:"Server",       i:"gear" },
          ].map(t => (
            <button key={t.k} className={`sd-tab focus-ring${tab===t.k?" active":""}`} onClick={()=>setTab(t.k)}>
              <Icon name={t.i} size={12}/> {t.l}
            </button>
          ))}
        </div>
        {tab === "users"  && <AdminUsers/>}
        {tab === "roles"  && <AdminRoles/>}
        {tab === "oidc"   && <AdminOidc/>}
        {tab === "jobs"   && <AdminJobs/>}
        {tab === "audit"  && <AdminAudit/>}
        {tab === "server" && <AdminServer coach={coach} classif={classif} onClassif={onClassif}/>}
      </div>
    </div>
  );
}

function ServerInfoStrip() {
  const s = SERVER_INFO;
  return (
    <div className="stat-strip">
      <div className="stat">
        <span className="stat-accent" style={{ "--stat-color":"#a78bfa" }}/>
        <div className="stat-label">CF Version</div>
        <div className="stat-value" style={{ fontSize:20 }}>{s.version}</div>
        <div className="stat-meta mono">{s.commit} · up {s.uptime}</div>
      </div>
      <div className="stat">
        <span className="stat-accent" style={{ "--stat-color":"#34d399" }}/>
        <div className="stat-label">Auth mode</div>
        <div className="stat-value" style={{ fontSize:16 }}>{s.authMode}</div>
        <div className="stat-meta">{ADMIN_USERS.filter(u=>u.status==="active").length} active users</div>
      </div>
      <div className="stat">
        <span className="stat-accent" style={{ "--stat-color":"#60a5fa" }}/>
        <div className="stat-label">Database</div>
        <div className="stat-value" style={{ fontSize:16, color:"#34d399" }}>{s.dbStatus}</div>
        <div className="stat-meta">{s.dbSize}</div>
      </div>
      <div className="stat">
        <span className="stat-accent" style={{ "--stat-color":"#fbbf24" }}/>
        <div className="stat-label">Active sessions</div>
        <div className="stat-value">{s.sessions}</div>
      </div>
      <div className="stat">
        <span className="stat-accent" style={{ "--stat-color":"#f87171" }}/>
        <div className="stat-label">TLS cert</div>
        <div className="stat-value" style={{ fontSize:20 }}>{s.tlsExpiry}</div>
        <div className="stat-meta">until expiry</div>
      </div>
    </div>
  );
}

/* ── Users tab ── */
function AdminUsers() {
  const [query, setQuery] = React.useState("");
  const [roleFilter, setRoleFilter] = React.useState("all");
  const [editUser, setEditUser] = React.useState(null);
  const [addOpen, setAddOpen] = React.useState(false);

  const users = ADMIN_USERS.filter(u =>
    (roleFilter === "all" || u.role === roleFilter) &&
    (!query || u.name.toLowerCase().includes(query.toLowerCase()) || u.email.toLowerCase().includes(query.toLowerCase()))
  );

  const roleChip = (r) => {
    const def = ROLE_DEFS.find(d => d.role === r);
    return <span className="chip" style={{ background:`color-mix(in oklab, ${def.color} 16%, transparent)`, color:def.color, fontSize:10 }}>{r}</span>;
  };

  return (
    <>
      <div style={{ padding:"12px 16px", borderBottom:"1px solid var(--cf-divider)", display:"flex", gap:10, alignItems:"center", flexWrap:"wrap" }}>
        <div className="filter-search" style={{ maxWidth:260 }}>
          <Icon name="search"/>
          <input className="input focus-ring" placeholder="Search users…" value={query} onChange={e=>setQuery(e.target.value)}/>
        </div>
        <div className="seg">
          {["all","admin","operator","viewer"].map(r => (
            <button key={r} className={roleFilter===r?"active":""} onClick={()=>setRoleFilter(r)}>{r}</button>
          ))}
        </div>
        <span className="filter-count">{users.length} users</span>
        <button className="btn btn-primary focus-ring" style={{ marginLeft:"auto" }} onClick={()=>setAddOpen(true)}>
          <Icon name="plus" size={13}/> Add user
        </button>
      </div>
      <table className="sys-table">
        <thead>
          <tr>
            <th>User</th>
            <th>Role</th>
            <th>Source</th>
            <th>Environments</th>
            <th>MFA</th>
            <th>Status</th>
            <th>Last login</th>
            <th style={{ textAlign:"right" }}> </th>
          </tr>
        </thead>
        <tbody>
          {users.map(u => (
            <tr key={u.id} style={{ cursor:"pointer" }} onClick={()=>setEditUser(u)}>
              <td>
                <div style={{ display:"flex", alignItems:"center", gap:10 }}>
                  <div style={{ width:28, height:28, borderRadius:99, flexShrink:0,
                    background: u.serviceAccount ? "var(--cf-subtle-bg)" : "linear-gradient(135deg,#a78bc4,#654a84)",
                    display:"grid", placeItems:"center", fontSize:11, fontWeight:600,
                    color: u.serviceAccount ? "var(--cf-text-muted)" : "#fff" }}>
                    {u.serviceAccount ? <Icon name="cpu" size={13}/> : u.name.split(" ").map(w=>w[0]).slice(0,2).join("")}
                  </div>
                  <div>
                    <div style={{ fontWeight:600, fontSize:13, display:"flex", alignItems:"center", gap:6 }}>
                      {u.name}
                      {u.serviceAccount && <span className="chip chip-unknown" style={{ fontSize:9 }}>service</span>}
                    </div>
                    <div className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{u.email}</div>
                  </div>
                </div>
              </td>
              <td>{roleChip(u.role)}</td>
              <td><span className="chip chip-unknown" style={{ fontSize:10 }}>{u.source}</span></td>
              <td>
                <div style={{ display:"flex", gap:4, flexWrap:"wrap" }}>
                  {u.envs.includes("all")
                    ? <span className="chip chip-info" style={{ fontSize:10 }}>all</span>
                    : u.envs.map(e => <EnvBadge key={e} env={e}/>)}
                </div>
              </td>
              <td>{u.mfa ? <span className="chip chip-healthy" style={{ fontSize:10 }}>on</span> : <span className="chip chip-warning" style={{ fontSize:10 }}>off</span>}</td>
              <td>{u.status === "active" ? <span className="chip chip-healthy" style={{ fontSize:10 }}>active</span> : <span className="chip chip-unknown" style={{ fontSize:10 }}>disabled</span>}</td>
              <td style={{ fontSize:12, color:"var(--cf-text-muted)" }}>{u.lastLogin}</td>
              <td>
                <div className="row-actions">
                  <button className="btn-icon focus-ring" title="Edit" onClick={e=>{e.stopPropagation(); setEditUser(u);}}><Icon name="gear" size={14}/></button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {(editUser || addOpen) && (
        <AdminUserModal mode={addOpen?"add":"edit"} user={editUser} onClose={()=>{setEditUser(null);setAddOpen(false);}}/>
      )}
    </>
  );
}

function AdminUserModal({ mode, user, onClose }) {
  const isEdit = mode === "edit";
  const [form, setForm] = React.useState(() => isEdit && user ? {
    name:user.name, email:user.email, role:user.role, envs:user.envs, status:user.status, source:user.source,
  } : { name:"", email:"", role:"viewer", envs:["dev"], status:"active", source:"local" });
  const set=(k,v)=>setForm(p=>({...p,[k]:v}));
  const toggleEnv=(e)=>set("envs", form.envs.includes(e)?form.envs.filter(x=>x!==e):[...form.envs.filter(x=>x!=="all"),e]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(560px,96vw)" }}>
        <div className="modal-head">
          <h2><Icon name={isEdit?"gear":"plus"} size={14} style={{marginRight:6,verticalAlign:"text-bottom"}}/>{isEdit?`Edit ${user.name}`:"Add user"}</h2>
          <p>{isEdit ? (user.source==="oidc" ? "OIDC user — role & env come from group mappings unless overridden." : "Local account.") : "Create a local user account."}</p>
        </div>
        <div className="modal-body">
          <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
            <div className="field"><label>Name</label><input className="input focus-ring" value={form.name} onChange={e=>set("name",e.target.value)}/></div>
            <div className="field"><label>Email</label><input className="input focus-ring mono" value={form.email} onChange={e=>set("email",e.target.value)} style={{fontSize:12}}/></div>
          </div>
          <div className="field">
            <label>Role</label>
            <div className="seg" style={{ width:"fit-content" }}>
              {["admin","operator","viewer"].map(r => (
                <button key={r} className={form.role===r?"active":""} onClick={()=>set("role",r)}>{r}</button>
              ))}
            </div>
            <div className="help">{ROLE_DEFS.find(d=>d.role===form.role)?.desc}</div>
          </div>
          <div className="field">
            <label>Environments</label>
            <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
              <button className="focus-ring" onClick={()=>set("envs",["all"])} style={pillStyle(form.envs.includes("all"), "#60a5fa")}>all</button>
              {ENVIRONMENTS.map(env => (
                <button key={env.name} className="focus-ring" onClick={()=>toggleEnv(env.name)} style={pillStyle(form.envs.includes(env.name), env.color)}>
                  <span style={{ width:6, height:6, borderRadius:"50%", background:env.color }}/>{env.name}
                </button>
              ))}
            </div>
          </div>
          {isEdit && user.source==="oidc" && (
            <div className="sd-callout sd-callout-info" style={{ fontSize:11 }}>
              <Icon name="link" size={12}/>
              <div>This user authenticates via OIDC (groups: {user.groups.map(g=><span key={g} className="mono">{g} </span>)}). Manual role/env here overrides the group mapping.</div>
            </div>
          )}
        </div>
        <div className="modal-foot">
          {isEdit && <button className="btn btn-ghost focus-ring" style={{ marginRight:"auto", color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }} onClick={onClose}>{user.status==="active"?"Disable":"Enable"} user</button>}
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" onClick={onClose}><Icon name="check" size={13}/> {isEdit?"Save":"Create"}</button>
        </div>
      </div>
    </div>
  );
}

function pillStyle(on, color) {
  return {
    padding:"4px 10px", borderRadius:99, fontSize:11, cursor:"pointer", fontFamily:"inherit",
    border:`1px solid ${on?color:"var(--cf-card-border)"}`,
    background: on?`color-mix(in oklab, ${color} 14%, var(--cf-card-bg))`:"transparent",
    color: on?color:"var(--cf-text-secondary)",
    display:"inline-flex", alignItems:"center", gap:6,
  };
}

/* ── Roles tab ── */
function AdminRoles() {
  return (
    <div style={{ padding:16, display:"grid", gridTemplateColumns:"repeat(auto-fit, minmax(260px, 1fr))", gap:14 }}>
      {ROLE_DEFS.map(def => (
        <div key={def.role} className="card" style={{ padding:16, borderTop:`3px solid ${def.color}` }}>
          <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", marginBottom:8 }}>
            <span className="chip" style={{ background:`color-mix(in oklab, ${def.color} 16%, transparent)`, color:def.color, fontSize:12, fontWeight:600 }}>{def.role}</span>
            <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{ADMIN_USERS.filter(u=>u.role===def.role).length} users</span>
          </div>
          <div style={{ fontSize:12, color:"var(--cf-text-secondary)", marginBottom:12, lineHeight:1.5 }}>{def.desc}</div>
          <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
            {def.perms.map(p => (
              <div key={p} style={{ display:"flex", alignItems:"center", gap:8, fontSize:12 }}>
                <Icon name="check" size={12} style={{ color:def.color, flexShrink:0 }}/>
                <span>{p}</span>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

/* ── OIDC tab ── */
function AdminOidc() {
  const [editMap, setEditMap] = React.useState(null);
  const [addOpen, setAddOpen] = React.useState(false);
  return (
    <>
      <div style={{ padding:"14px 16px", borderBottom:"1px solid var(--cf-divider)" }}>
        <div className="sd-callout sd-callout-info" style={{ fontSize:12 }}>
          <Icon name="link" size={13}/>
          <div>
            Connected to <span className="mono">{SERVER_INFO.oidcIssuer}</span>. When a user logs in, their IdP groups are matched top-down; the first matching mapping sets their role and environment scope.
          </div>
        </div>
      </div>
      <div style={{ padding:"10px 16px", display:"flex", justifyContent:"flex-end" }}>
        <button className="btn btn-primary focus-ring" onClick={()=>setAddOpen(true)}><Icon name="plus" size={13}/> Add mapping</button>
      </div>
      <table className="sys-table">
        <thead>
          <tr>
            <th style={{ width:60 }}>Priority</th>
            <th>IdP Group</th>
            <th>CF Role</th>
            <th>Environments</th>
            <th>Matched users</th>
            <th style={{ textAlign:"right" }}> </th>
          </tr>
        </thead>
        <tbody>
          {OIDC_MAPPINGS.map(m => {
            const def = ROLE_DEFS.find(d=>d.role===m.role);
            return (
              <tr key={m.id} style={{ cursor:"pointer" }} onClick={()=>setEditMap(m)}>
                <td><span className="mono" style={{ fontSize:12, color:"var(--cf-text-muted)" }}>#{m.priority}</span></td>
                <td><span className="mono" style={{ fontWeight:600, fontSize:13 }}>{m.group}</span></td>
                <td><span className="chip" style={{ background:`color-mix(in oklab, ${def.color} 16%, transparent)`, color:def.color, fontSize:10 }}>{m.role}</span></td>
                <td>
                  <div style={{ display:"flex", gap:4, flexWrap:"wrap" }}>
                    {m.envs.length === 0 ? <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>none</span> :
                     m.envs.includes("all") ? <span className="chip chip-info" style={{ fontSize:10 }}>all</span> :
                     m.envs.map(e => <EnvBadge key={e} env={e}/>)}
                  </div>
                </td>
                <td className="mono" style={{ fontSize:12 }}>{m.users}</td>
                <td>
                  <div className="row-actions">
                    <button className="btn-icon focus-ring" title="Edit" onClick={e=>{e.stopPropagation();setEditMap(m);}}><Icon name="gear" size={14}/></button>
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {(editMap || addOpen) && <AdminOidcModal mode={addOpen?"add":"edit"} mapping={editMap} onClose={()=>{setEditMap(null);setAddOpen(false);}}/>}
    </>
  );
}

function AdminOidcModal({ mode, mapping, onClose }) {
  const isEdit = mode === "edit";
  const [form, setForm] = React.useState(()=> isEdit && mapping ? { group:mapping.group, role:mapping.role, envs:mapping.envs } : { group:"", role:"viewer", envs:[] });
  const set=(k,v)=>setForm(p=>({...p,[k]:v}));
  const toggleEnv=(e)=>set("envs", form.envs.includes(e)?form.envs.filter(x=>x!==e):[...form.envs.filter(x=>x!=="all"),e]);
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(520px,96vw)" }}>
        <div className="modal-head"><h2><Icon name={isEdit?"gear":"plus"} size={14} style={{marginRight:6,verticalAlign:"text-bottom"}}/>{isEdit?"Edit mapping":"Add mapping"}</h2><p>Map an IdP group to a Crystal Forge role and environment scope.</p></div>
        <div className="modal-body">
          <div className="field"><label>IdP group name</label><input className="input focus-ring mono" value={form.group} onChange={e=>set("group",e.target.value)} placeholder="e.g. cf-operators" style={{fontSize:12}}/></div>
          <div className="field">
            <label>CF role</label>
            <div className="seg" style={{ width:"fit-content" }}>
              {["admin","operator","viewer"].map(r => <button key={r} className={form.role===r?"active":""} onClick={()=>set("role",r)}>{r}</button>)}
            </div>
          </div>
          <div className="field">
            <label>Environments</label>
            <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
              <button className="focus-ring" onClick={()=>set("envs",["all"])} style={pillStyle(form.envs.includes("all"),"#60a5fa")}>all</button>
              {ENVIRONMENTS.map(env => (
                <button key={env.name} className="focus-ring" onClick={()=>toggleEnv(env.name)} style={pillStyle(form.envs.includes(env.name),env.color)}>
                  <span style={{ width:6, height:6, borderRadius:"50%", background:env.color }}/>{env.name}
                </button>
              ))}
            </div>
          </div>
        </div>
        <div className="modal-foot">
          {isEdit && <button className="btn btn-ghost focus-ring" style={{ marginRight:"auto", color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }} onClick={onClose}><Icon name="x" size={12}/> Remove</button>}
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" onClick={onClose}><Icon name="check" size={13}/> {isEdit?"Save":"Add"}</button>
        </div>
      </div>
    </div>
  );
}

/* ── Background jobs tab ── */
function AdminJobs() {
  const [jobs, setJobs] = React.useState(BACKGROUND_JOBS);
  const [editJob, setEditJob] = React.useState(null);

  const toggle = (id) => setJobs(js => js.map(j => j.id === id ? { ...j, enabled: !j.enabled, status: !j.enabled ? "healthy" : "disabled", nextRun: !j.enabled ? "scheduled" : "disabled" } : j));

  const statusChip = (j) => {
    if (!j.enabled) return <span className="chip chip-unknown" style={{ fontSize:10 }}>disabled</span>;
    if (j.status === "degraded") return <span className="chip chip-warning" style={{ fontSize:10 }} title={j.note}>degraded</span>;
    return <span className="chip chip-healthy" style={{ fontSize:10 }}>healthy</span>;
  };
  const impactChip = (i) => {
    const m = { low:["chip-healthy","low"], medium:["chip-warning","medium"], high:["chip-critical","high"] };
    const [cls,l] = m[i] || m.low;
    return <span className={`chip ${cls}`} style={{ fontSize:10 }}>{l} load</span>;
  };

  return (
    <>
      <div style={{ padding:"14px 16px", borderBottom:"1px solid var(--cf-divider)" }}>
        <div className="sd-callout sd-callout-info" style={{ fontSize:12 }}>
          <Icon name="sync" size={13}/>
          <div>Scheduled server-side tasks. Crank intervals down for freshness, up to save resources. Cache polling and GC reconciliation can be heavy on large fleets — schedule deliberately.</div>
        </div>
      </div>
      <table className="sys-table">
        <thead>
          <tr>
            <th>Job</th>
            <th>Interval</th>
            <th>Status</th>
            <th>Load</th>
            <th>Last run</th>
            <th>Next run</th>
            <th style={{ textAlign:"right" }}>Enabled</th>
          </tr>
        </thead>
        <tbody>
          {jobs.map(j => (
            <tr key={j.id} style={{ cursor:"pointer" }} onClick={()=>setEditJob(j)}>
              <td>
                <div style={{ fontWeight:600, fontSize:13 }}>{j.name}</div>
                <div style={{ fontSize:11, color:"var(--cf-text-muted)", maxWidth:380 }}>{j.desc}</div>
              </td>
              <td><span className="mono chip chip-unknown" style={{ fontSize:10 }}>{j.interval}</span></td>
              <td>{statusChip(j)}</td>
              <td>{impactChip(j.impact)}</td>
              <td style={{ fontSize:12, color:"var(--cf-text-muted)" }}>{j.lastRun}{j.enabled && j.lastDuration!=="—" && <span style={{ opacity:0.6 }}> · {j.lastDuration}</span>}</td>
              <td style={{ fontSize:12, color:"var(--cf-text-muted)" }}>{j.nextRun}</td>
              <td onClick={e=>e.stopPropagation()}>
                <div style={{ display:"flex", justifyContent:"flex-end", gap:6, alignItems:"center" }}>
                  <button className="btn-icon focus-ring" title="Run now" onClick={()=>{}}><Icon name="sync" size={13}/></button>
                  <label style={{ position:"relative", display:"inline-flex", cursor:"pointer" }}>
                    <input type="checkbox" checked={j.enabled} onChange={()=>toggle(j.id)} style={{ accentColor:"var(--cf-brand-purple)", width:16, height:16 }}/>
                  </label>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {editJob && <AdminJobModal job={editJob} onClose={()=>setEditJob(null)}/>}
    </>
  );
}

function AdminJobModal({ job, onClose }) {
  const [interval, setInterval] = React.useState(job.interval);
  const [enabled, setEnabled] = React.useState(job.enabled);
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(520px,96vw)" }}>
        <div className="modal-head">
          <h2><Icon name="sync" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/> {job.name}</h2>
          <p>{job.desc}</p>
        </div>
        <div className="modal-body">
          <div className="field">
            <label>Run interval</label>
            <select className="input focus-ring" value={interval} onChange={e=>setInterval(e.target.value)} style={{ width:160 }}>
              {JOB_INTERVALS.map(i => <option key={i} value={i}>{i === "never" ? "Manual only" : `Every ${i}`}</option>)}
            </select>
            <div className="help">Lower = fresher data, higher resource use. {job.impact === "medium" && "This job is moderately expensive on large fleets."}</div>
          </div>
          <label style={{ display:"flex", gap:8, alignItems:"center", fontSize:13, cursor:"pointer" }}>
            <input type="checkbox" checked={enabled} onChange={e=>setEnabled(e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
            <span>Enabled</span>
          </label>
          {job.note && (
            <div className="sd-callout sd-callout-warn" style={{ fontSize:11 }}>
              <Icon name="warn" size={12}/>
              <div>Last run note: {job.note}</div>
            </div>
          )}
          <dl className="kv-grid" style={{ marginTop:4 }}>
            <dt>Last run</dt><dd>{job.lastRun} {job.lastDuration!=="—" && <span style={{color:"var(--cf-text-muted)"}}>· {job.lastDuration}</span>}</dd>
            <dt>Next run</dt><dd>{job.nextRun}</dd>
          </dl>
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" style={{ marginRight:"auto" }} onClick={onClose}><Icon name="sync" size={12}/> Run now</button>
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" onClick={onClose}><Icon name="check" size={13}/> Save</button>
        </div>
      </div>
    </div>
  );
}

/* ── Audit tab ── */
function AdminAudit() {
  const [kind, setKind] = React.useState("all");
  const [query, setQuery] = React.useState("");
  const rows = AUDIT_LOG.filter(a =>
    (kind === "all" || a.kind === kind) &&
    (!query || a.actor.toLowerCase().includes(query.toLowerCase()) || a.action.toLowerCase().includes(query.toLowerCase()) || a.target.toLowerCase().includes(query.toLowerCase()))
  );
  const kindChip = (k) => {
    const map = { security:["chip-critical","security"], deploy:["chip-info","deploy"], build:["chip-info","build"], config:["chip-unknown","config"], auth:["chip-warning","auth"] };
    const [cls,label] = map[k] || ["chip-unknown",k];
    return <span className={`chip ${cls}`} style={{ fontSize:10 }}>{label}</span>;
  };
  return (
    <>
      <div style={{ padding:"12px 16px", borderBottom:"1px solid var(--cf-divider)", display:"flex", gap:10, alignItems:"center", flexWrap:"wrap" }}>
        <div className="filter-search" style={{ maxWidth:260 }}>
          <Icon name="search"/>
          <input className="input focus-ring" placeholder="Search actor / action / target…" value={query} onChange={e=>setQuery(e.target.value)}/>
        </div>
        <div className="seg">
          {["all","security","deploy","build","config","auth"].map(k => (
            <button key={k} className={kind===k?"active":""} onClick={()=>setKind(k)}>{k}</button>
          ))}
        </div>
        <span className="filter-count">{rows.length} events</span>
        <button className="btn btn-ghost focus-ring" style={{ marginLeft:"auto" }}><Icon name="download" size={13}/> Export</button>
      </div>
      <table className="sys-table">
        <thead>
          <tr>
            <th>When</th>
            <th>Actor</th>
            <th>Action</th>
            <th>Target</th>
            <th>Category</th>
            <th>Source IP</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(a => (
            <tr key={a.id}>
              <td style={{ fontSize:12, color:"var(--cf-text-muted)", whiteSpace:"nowrap" }}>{a.at}</td>
              <td className="mono" style={{ fontSize:12, fontWeight:600 }}>{a.actor}</td>
              <td className="mono" style={{ fontSize:12, color: a.kind==="security"?"#f87171":"var(--cf-text-primary)" }}>{a.action}</td>
              <td style={{ fontSize:12 }}>{a.target}</td>
              <td>{kindChip(a.kind)}</td>
              <td className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{a.ip}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

/* ── Server tab ── */
/* ── Server tab ── */
function AdminHeartbeat() {
  const [cfg, setCfg] = React.useState(() => JSON.parse(JSON.stringify(HEARTBEAT_CONFIG)));
  const fmt = (sec) => HEARTBEAT_INTERVALS.find(i => i.v === sec)?.l || `${sec}s`;

  const IntervalSelect = ({ value, onChange, placeholder }) => (
    <select className="input focus-ring" value={value ?? ""} onChange={onChange} style={{ width:120 }}>
      {placeholder && <option value="">{placeholder}</option>}
      {HEARTBEAT_INTERVALS.map(i => <option key={i.v} value={i.v}>{i.l}</option>)}
    </select>
  );

  const setOverride = (env, val) => setCfg(c => {
    const o = { ...c.overrides };
    if (val === "") delete o[env]; else o[env] = parseInt(val, 10);
    return { ...c, overrides: o };
  });

  return (
    <>
      <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", marginBottom:4 }}>
        <h3 style={{ margin:0, fontSize:13, fontWeight:600 }}>Agent heartbeat</h3>
        <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>How often agents phone home — drives the Systems heartbeat indicator</span>
      </div>
      <div className="sd-callout sd-callout-info" style={{ fontSize:11, margin:"10px 0 14px" }}>
        <Icon name="server" size={12}/>
        <div>Lower intervals detect drift & outages faster but add load and chatter (costly on metered edge links). Each environment can override the global default.</div>
      </div>

      <div style={{ display:"grid", gridTemplateColumns:"repeat(auto-fit, minmax(220px, 1fr))", gap:16 }}>
        <div className="field">
          <label>Global default</label>
          <IntervalSelect value={cfg.globalIntervalSec} onChange={e=>setCfg(c=>({...c, globalIntervalSec: parseInt(e.target.value,10)}))}/>
          <div className="help">Applied to any environment without an override.</div>
        </div>
        <div className="field">
          <label>Mark stale after</label>
          <select className="input focus-ring" value={cfg.staleMultiplier} onChange={e=>setCfg(c=>({...c, staleMultiplier: parseInt(e.target.value,10)}))} style={{ width:160 }}>
            {[2,3,4].map(m => <option key={m} value={m}>{m} missed ({fmt(cfg.globalIntervalSec*m)})</option>)}
          </select>
        </div>
        <div className="field">
          <label>Mark offline after</label>
          <select className="input focus-ring" value={cfg.offlineMultiplier} onChange={e=>setCfg(c=>({...c, offlineMultiplier: parseInt(e.target.value,10)}))} style={{ width:160 }}>
            {[4,5,8,10].map(m => <option key={m} value={m}>{m} missed ({fmt(cfg.globalIntervalSec*m)})</option>)}
          </select>
        </div>
      </div>

      <div style={{ marginTop:16 }}>
        <div style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.06em", color:"var(--cf-text-muted)", fontWeight:600, marginBottom:8 }}>Per-environment overrides</div>
        <div style={{ display:"flex", flexDirection:"column", gap:0, border:"1px solid var(--cf-divider)", borderRadius:8, overflow:"hidden" }}>
          {ENVIRONMENTS.map((env, i) => {
            const has = cfg.overrides[env.name] != null;
            const eff = has ? cfg.overrides[env.name] : cfg.globalIntervalSec;
            const count = (window.SYSTEMS || []).filter(s => s.environment === env.name).length;
            return (
              <div key={env.name} style={{ display:"flex", alignItems:"center", gap:12, padding:"10px 12px", borderTop: i?"1px solid var(--cf-divider)":"none" }}>
                <span style={{ width:8, height:8, borderRadius:"50%", background:env.color, flexShrink:0 }}/>
                <div style={{ flex:1, minWidth:0 }}>
                  <span className="mono" style={{ fontSize:13, fontWeight:600 }}>{env.name}</span>
                  <span style={{ fontSize:11, color:"var(--cf-text-muted)", marginLeft:8 }}>{count} systems</span>
                </div>
                <span style={{ fontSize:11, color: has ? "var(--cf-brand-purple)" : "var(--cf-text-muted)" }}>
                  {has ? "override" : "inherits global"} · {fmt(eff)}
                </span>
                <IntervalSelect value={has ? cfg.overrides[env.name] : ""} onChange={e=>setOverride(env.name, e.target.value)} placeholder="(global)"/>
                {has && (
                  <button className="btn-icon focus-ring" title="Clear override" onClick={()=>setOverride(env.name, "")}><Icon name="x" size={13}/></button>
                )}
              </div>
            );
          })}
        </div>
      </div>

      <div style={{ display:"flex", justifyContent:"flex-end", gap:8, marginTop:14 }}>
        <button className="btn btn-ghost focus-ring" onClick={()=>setCfg(JSON.parse(JSON.stringify(HEARTBEAT_CONFIG)))}>Reset</button>
        <button className="btn btn-primary focus-ring"><Icon name="check" size={13}/> Save heartbeat config</button>
      </div>
    </>
  );
}

function AdminRetries() {
  const [cfg, setCfg] = React.useState(() => JSON.parse(JSON.stringify(RETRY_CONFIG)));
  const set = (k, v) => setCfg(c => ({ ...c, [k]: v }));
  return (
    <>
      <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", marginBottom:4 }}>
        <h3 style={{ margin:0, fontSize:13, fontWeight:600 }}>Automatic retries</h3>
        <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>How many times a failed build/eval is retried before it's left failed</span>
      </div>
      <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14, marginTop:14 }}>
        <label style={{ display:"flex", flexDirection:"column", gap:5 }}>
          <span style={{ fontSize:12, fontWeight:500 }}>Max build retries</span>
          <select className="input focus-ring" style={{ width:120 }} value={cfg.buildRetries} onChange={e=>set("buildRetries", Number(e.target.value))}>
            {[0,1,2,3,4,5].map(n => <option key={n} value={n}>{n === 0 ? "Never" : n}</option>)}
          </select>
        </label>
        <label style={{ display:"flex", flexDirection:"column", gap:5 }}>
          <span style={{ fontSize:12, fontWeight:500 }}>Max eval retries</span>
          <select className="input focus-ring" style={{ width:120 }} value={cfg.evalRetries} onChange={e=>set("evalRetries", Number(e.target.value))}>
            {[0,1,2,3,4,5].map(n => <option key={n} value={n}>{n === 0 ? "Never" : n}</option>)}
          </select>
        </label>
        <label style={{ display:"flex", flexDirection:"column", gap:5 }}>
          <span style={{ fontSize:12, fontWeight:500 }}>Backoff between attempts</span>
          <select className="input focus-ring" style={{ width:120 }} value={cfg.backoffSec} onChange={e=>set("backoffSec", Number(e.target.value))}>
            {[0,10,30,60,120,300].map(n => <option key={n} value={n}>{n === 0 ? "None" : n < 60 ? `${n}s` : `${n/60}m`}</option>)}
          </select>
        </label>
        <label style={{ display:"flex", gap:9, alignItems:"flex-start", cursor:"pointer", marginTop:22 }}>
          <input type="checkbox" checked={cfg.onlyTransient} onChange={e=>set("onlyTransient", e.target.checked)} style={{ marginTop:2 }}/>
          <span>
            <span style={{ display:"block", fontSize:12, fontWeight:500 }}>Only retry transient failures</span>
            <span style={{ display:"block", fontSize:11, color:"var(--cf-text-muted)" }}>Skip auto-retry for eval/build errors that won't change on their own (e.g. bad derivation, assertion failure)</span>
          </span>
        </label>
      </div>
      <div style={{ display:"flex", justifyContent:"flex-end", gap:8, marginTop:14 }}>
        <button className="btn btn-ghost focus-ring" onClick={()=>setCfg(JSON.parse(JSON.stringify(RETRY_CONFIG)))}>Reset</button>
        <button className="btn btn-primary focus-ring"><Icon name="check" size={13}/> Save retry config</button>
      </div>
    </>
  );
}

function AdminServer({ coach, classif, onClassif }) {
  const s = SERVER_INFO;
  const done = coach ? coach.count : 0;
  const total = coach ? coach.total : 6;
  const cls = classif || { enabled: false, level: "UNCLASSIFIED", text: "" };
  const setCls = (patch) => onClassif && onClassif({ ...cls, ...patch });
  return (
    <div style={{ padding:16, display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
      <div className="card" style={{ padding:16 }}>
        <h3 style={{ margin:"0 0 12px", fontSize:13, fontWeight:600 }}>Build info</h3>
        <dl className="kv-grid">
          <dt>Version</dt><dd className="mono">{s.version}</dd>
          <dt>Commit</dt><dd className="mono">{s.commit}</dd>
          <dt>Uptime</dt><dd>{s.uptime}</dd>
          <dt>Database</dt><dd><span className="chip chip-healthy">{s.dbStatus}</span> <span style={{color:"var(--cf-text-muted)"}}>· {s.dbSize}</span></dd>
        </dl>
      </div>
      <div className="card" style={{ padding:16 }}>
        <h3 style={{ margin:"0 0 12px", fontSize:13, fontWeight:600 }}>Authentication</h3>
        <dl className="kv-grid">
          <dt>Mode</dt><dd>{s.authMode}</dd>
          <dt>OIDC issuer</dt><dd className="mono" style={{ fontSize:11, wordBreak:"break-all", whiteSpace:"normal" }}>{s.oidcIssuer}</dd>
          <dt>Sessions</dt><dd>{s.sessions} active</dd>
          <dt>TLS expiry</dt><dd><span className="chip chip-healthy">{s.tlsExpiry}</span></dd>
        </dl>
      </div>
      <div className="card" style={{ padding:16, gridColumn:"1 / -1" }}>
        <AdminHeartbeat/>
      </div>
      <div className="card" style={{ padding:16, gridColumn:"1 / -1" }}>
        <AdminRetries/>
      </div>
      <div className="card" style={{ padding:16, gridColumn:"1 / -1" }}>
        <div style={{ display:"flex", alignItems:"flex-start", justifyContent:"space-between", gap:12, flexWrap:"wrap", marginBottom: cls.enabled ? 14 : 0 }}>
          <div>
            <h3 style={{ margin:"0 0 4px", fontSize:13, fontWeight:600, display:"flex", alignItems:"center", gap:7 }}>
              <Icon name="shield" size={13}/> Classification banners
            </h3>
            <p style={{ margin:0, fontSize:12, color:"var(--cf-text-muted)", maxWidth:"60ch" }}>
              Display a CNSS/DoD classification marking at the top and bottom of every screen. Required on many DoD / IC information systems.
            </p>
          </div>
          <button
            role="switch"
            aria-checked={cls.enabled}
            className="focus-ring"
            onClick={() => setCls({ enabled: !cls.enabled })}
            style={{
              flexShrink:0, cursor:"pointer", border:"none", padding:0,
              width:44, height:24, borderRadius:999,
              background: cls.enabled ? "var(--cf-brand-purple)" : "var(--cf-subtle-bg)",
              position:"relative", transition:"background 140ms",
            }}>
            <span style={{
              position:"absolute", top:2, left: cls.enabled ? 22 : 2,
              width:20, height:20, borderRadius:"50%", background:"#fff",
              transition:"left 140ms", boxShadow:"0 1px 3px rgba(0,0,0,0.3)",
            }}/>
          </button>
        </div>

        {cls.enabled && (
          <div style={{ borderTop:"1px solid var(--cf-divider)", paddingTop:14, display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
            <div className="field" style={{ margin:0 }}>
              <label>Classification level</label>
              <select className="input focus-ring" value={cls.level} onChange={e=>setCls({ level: e.target.value })}>
                {(window.CLASSIFICATION_LEVELS || []).map(l => <option key={l.id} value={l.id}>{l.label}</option>)}
              </select>
            </div>
            <div className="field" style={{ margin:0 }}>
              <label>Custom marking <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· optional</span></label>
              <input className="input focus-ring" value={cls.text} onChange={e=>setCls({ text: e.target.value })} placeholder="e.g. UNCLASSIFIED//FOUO"/>
            </div>
            <div style={{ gridColumn:"1 / -1" }}>
              <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginBottom:6 }}>Preview</div>
              {(() => {
                const def = (window.CLASSIFICATION_LEVELS || []).find(l => l.id === cls.level) || {};
                const display = (cls.text && cls.text.trim()) ? cls.text.trim().toUpperCase() : (def.label || cls.level);
                return (
                  <div style={{
                    height:24, borderRadius:6, display:"flex", alignItems:"center", justifyContent:"center",
                    fontSize:12, fontWeight:700, letterSpacing:"0.08em", textTransform:"uppercase",
                    background: def.bg, color: def.fg,
                  }}>{display}</div>
                );
              })()}
            </div>
          </div>
        )}
      </div>
      <div className="card" style={{ padding:16, gridColumn:"1 / -1" }}>
        <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:12, flexWrap:"wrap" }}>
          <div>
            <h3 style={{ margin:"0 0 4px", fontSize:13, fontWeight:600, display:"flex", alignItems:"center", gap:7 }}>
              <Icon name="dashboard" size={13}/> Onboarding
            </h3>
            <p style={{ margin:0, fontSize:12, color:"var(--cf-text-muted)" }}>
              The Setup Coach walks admins through first-run configuration. {coach ? `${done} of ${total} steps complete.` : ""}
            </p>
          </div>
          {coach && (
            <div style={{ display:"flex", gap:8, flexWrap:"wrap" }}>
              <button className="btn btn-primary focus-ring" onClick={() => { coach.relaunch(); onNavigate && onNavigate("dashboard"); }}>
                <Icon name="sync" size={13}/> Relaunch Setup Coach
              </button>
              <button className="btn btn-ghost focus-ring" onClick={() => coach.reset()}>Reset progress</button>
            </div>
          )}
        </div>
      </div>
      <div className="card" style={{ padding:16, gridColumn:"1 / -1" }}>
        <h3 style={{ margin:"0 0 12px", fontSize:13, fontWeight:600 }}>Maintenance</h3>
        <div style={{ display:"flex", gap:8, flexWrap:"wrap" }}>
          <button className="btn btn-ghost focus-ring"><Icon name="download" size={13}/> Backup database</button>
          <button className="btn btn-ghost focus-ring"><Icon name="sync" size={13}/> Reload config</button>
          <button className="btn btn-ghost focus-ring"><Icon name="history" size={13}/> Export audit log</button>
          <button className="btn btn-ghost focus-ring" style={{ color:"#fbbf24", borderColor:"rgba(251,191,36,0.3)" }}><Icon name="warn" size={13}/> Invalidate all sessions</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { AdminView });