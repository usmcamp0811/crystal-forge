// Environments view — registry of env tiers with system + cache assignments

// Mock env metadata enriching ENVIRONMENTS from data.js
const ENV_META = {
  production: {
    description: "Customer-facing production tier. Strictest deployment policy.",
    isProduction: true,
    cache: "s3://crystal-forge-prod-cache",
    cacheType: "s3",
    requiresApproval: true,
    autoSync: true,
    defaultPolicy: "manual",
    gatePolicyIds: ["cve-gated", "two-approver"],
    complianceBundleId: "disa-rhel9-stig",
    rbac: { admin: ["mreyes"], operator: ["jpark","dchen"], viewer: ["audit-team"] },
  },
  staging: {
    description: "Pre-production validation tier. Mirrors production.",
    isProduction: false,
    cache: "s3://crystal-forge-staging-cache",
    cacheType: "s3",
    requiresApproval: true,
    autoSync: true,
    defaultPolicy: "manual",
    gatePolicyIds: ["cve-gated"],
    complianceBundleId: "nist-800-53-mod",
    rbac: { admin: ["mreyes"], operator: ["jpark","dchen","kthomas"], viewer: ["all"] },
  },
  dev: {
    description: "Development sandbox. Free-form, faster iteration.",
    cache: "attic://cf-attic.dev/dev",
    cacheType: "attic",
    requiresApproval: false,
    autoSync: true,
    defaultPolicy: "auto_latest",
    gatePolicyIds: ["cve-gated"],
    complianceBundleId: "",
    rbac: { admin: ["mreyes","dchen"], operator: ["all-engineers"], viewer: ["all"] },
  },
  edge: {
    description: "Globally distributed edge nodes. WireGuard gateway tier.",
    isProduction: true,
    cache: "s3://crystal-forge-edge-cache",
    cacheType: "s3",
    requiresApproval: true,
    autoSync: true,
    defaultPolicy: "auto_latest",
    rbac: { admin: ["mreyes"], operator: ["dchen"], viewer: ["network-team"] },
  },
  lab: {
    description: "Hardware lab + research nodes. Relaxed posture for testing.",
    cache: null,
    cacheType: "none",
    requiresApproval: false,
    autoSync: false,
    defaultPolicy: "manual",
    rbac: { admin: ["mreyes"], operator: ["lab-ops","research-team"], viewer: ["all"] },
  },
};

function envStats(env) {
  const sys = SYSTEMS.filter(s => s.environment === env);
  return {
    total: sys.length,
    healthy: sys.filter(s => s.health === "healthy").length,
    warning: sys.filter(s => s.health === "warning" || s.health === "drifted").length,
    critical: sys.filter(s => s.health === "critical").length,
    offline: sys.filter(s => s.health === "offline").length,
    cveTotal: sys.reduce((a,s) => a + s.cves.critical + s.cves.high, 0),
    flakes: [...new Set(sys.map(s => s.flake))],
  };
}

// Is this system in a production-flagged environment? Uses the explicit isProduction
// flag on the environment, NOT the environment's name.
function isProductionEnv(envName) {
  const meta = ENV_META[envName];
  return !!(meta && meta.isProduction);
}
window.isProductionEnv = isProductionEnv;

function EnvironmentsView({ defaultView, onOpenCache, onOpenSystem, onOpenBundle }) {
  const [query, setQuery] = React.useState("");
  const [viewMode, setViewMode] = React.useState(defaultView || "cards");
  React.useEffect(() => { if (defaultView) setViewMode(defaultView); }, [defaultView]);
  const [editEnv, setEditEnv] = React.useState(null);
  const [viewEnv, setViewEnv] = React.useState(null);
  const [addOpen, setAddOpen] = React.useState(false);
  const envNeedsAttention = (name) => SYSTEMS.some(s => s.environment === name && (s.health === "critical" || s.health === "offline"));
  const flashAttention = useAttentionFlash("environments", ENVIRONMENTS.some(e => envNeedsAttention(e.name)));

  const envs = ENVIRONMENTS
    .map(e => ({ ...e, ...(ENV_META[e.name] || {}), stats: envStats(e.name) }))
    .filter(e => !query ||
      e.name.toLowerCase().includes(query.toLowerCase()) ||
      (e.description || "").toLowerCase().includes(query.toLowerCase())
    );

  const totals = ENVIRONMENTS.reduce((a, e) => {
    const s = envStats(e.name);
    return {
      systems: a.systems + s.total,
      caches:  a.caches + (ENV_META[e.name]?.cache ? 1 : 0),
      pendingApproval: a.pendingApproval + (ENV_META[e.name]?.requiresApproval ? s.warning : 0),
    };
  }, { systems: 0, caches: 0, pendingApproval: 0 });

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">Environments</h1>
          <p className="page-subtitle">
            {ENVIRONMENTS.length} tiers · {totals.systems} systems · {totals.caches} caches configured
          </p>
        </div>
        <div style={{ display:"flex", gap:8 }}>
          <button className="btn btn-primary focus-ring" data-coach-target="env" onClick={() => setAddOpen(true)}>
            <Icon name="plus" size={14}/> Add environment
          </button>
        </div>
      </div>

      <div className="stat-strip">
        {[
          { label:"Total tiers",   val:ENVIRONMENTS.length, color:"#a78bfa" },
          { label:"Systems",       val:totals.systems,      color:"#60a5fa" },
          { label:"Caches",        val:`${totals.caches}/${ENVIRONMENTS.length}`, color:"#34d399" },
          { label:"Manual policy", val:ENVIRONMENTS.filter(e => ENV_META[e.name]?.defaultPolicy === "manual").length, color:"#fbbf24" },
          { label:"Auto-sync off", val:ENVIRONMENTS.filter(e => ENV_META[e.name]?.autoSync === false).length, color:"#f87171" },
        ].map(s => (
          <div key={s.label} className="stat">
            <span className="stat-accent" style={{ "--stat-color": s.color }}/>
            <div className="stat-label">{s.label}</div>
            <div className="stat-value">{s.val}</div>
          </div>
        ))}
      </div>

      <div className="filterbar">
        <div className="filter-search" style={{ maxWidth: 320 }}>
          <Icon name="search"/>
          <input className="input focus-ring" placeholder="Search environments…" value={query} onChange={e=>setQuery(e.target.value)}/>
        </div>
        <div className="seg">
          <button className={viewMode==="cards"?"active":""} onClick={()=>setViewMode("cards")}><Icon name="grid" size={12}/> Cards</button>
          <button className={viewMode==="table"?"active":""} onClick={()=>setViewMode("table")}><Icon name="rows" size={12}/> Table</button>
        </div>
        <span className="filter-count">{envs.length} environments</span>
      </div>

      {viewMode === "cards" ? (
        <div className="cards-grid">
          {envs.map(env => <EnvCard key={env.name} env={env} flash={flashAttention && envNeedsAttention(env.name)} onEdit={()=>setViewEnv(env)}/>)}
        </div>
      ) : (
        <div className="card" style={{ overflow:"hidden" }}>
          <table className="sys-table">
            <thead>
              <tr>
                <th>Environment</th>
                <th>Systems</th>
                <th>Health</th>
                <th>Deploy</th>
                <th>Enforcement</th>
                <th>Cache</th>
                <th>Auto-sync</th>
                <th>Approval</th>
                <th style={{ textAlign:"right" }}> </th>
              </tr>
            </thead>
            <tbody>
              {envs.map(env => <EnvRow key={env.name} env={env} flash={flashAttention && envNeedsAttention(env.name)} onEdit={()=>setViewEnv(env)}/>)}
            </tbody>
          </table>
        </div>
      )}

      {viewEnv && (
        <EnvPanel env={viewEnv} onClose={() => setViewEnv(null)} onEdit={() => { setEditEnv(viewEnv); }} onOpenCache={onOpenCache} onOpenSystem={onOpenSystem} onOpenBundle={onOpenBundle} />
      )}
      {(editEnv || addOpen) && (
        <EnvFormModal
          mode={addOpen ? "add" : "edit"}
          env={editEnv}
          onClose={() => { setEditEnv(null); setAddOpen(false); }}
        />
      )}
    </div>
  );
}

function EnvRow({ env, onEdit, flash }) {
  const total = env.stats.total || 1;
  return (
    <tr className={flash ? "attention-flash" : ""} style={{ cursor:"pointer" }} onClick={onEdit}>
      <td>
        <div style={{ display:"flex", alignItems:"center", gap:8 }}>
          <span className="env-dot" style={{ background: env.color }}/>
          <div>
            <div className="mono" style={{ fontWeight:600, fontSize:13, display:"flex", alignItems:"center", gap:7 }}>
              {env.name}
              {env.isProduction && <span className="env-prod-badge"><Icon name="shield" size={9}/> PROD</span>}
            </div>
            {env.description && <div style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{env.description}</div>}
          </div>
        </div>
      </td>
      <td className="mono" style={{ fontSize:13 }}>{env.stats.total}</td>
      <td>
        <div style={{ display:"flex", alignItems:"center", gap:6, minWidth:140 }}>
          <div style={{ height:4, flex:1, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden", display:"flex" }}>
            {env.stats.healthy  > 0 && <div style={{ width:`${(env.stats.healthy/total)*100}%`,  background:"#34d399" }}/>}
            {env.stats.warning  > 0 && <div style={{ width:`${(env.stats.warning/total)*100}%`,  background:"#fbbf24" }}/>}
            {env.stats.critical > 0 && <div style={{ width:`${(env.stats.critical/total)*100}%`, background:"#f87171" }}/>}
            {env.stats.offline  > 0 && <div style={{ width:`${(env.stats.offline/total)*100}%`,  background:"#6b7280" }}/>}
          </div>
          <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{env.stats.healthy}/{env.stats.total}</span>
        </div>
      </td>
      <td>
        <span className={`chip ${env.defaultPolicy === "manual" ? "chip-warning" : "chip-healthy"}`}>{env.defaultPolicy || "—"}</span>
      </td>
      <td>
        <div style={{ display:"flex", gap:6, alignItems:"center", flexWrap:"wrap" }}>
          {env.complianceBundleId && (() => {
            const b = (typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : []).find(x => x.id === env.complianceBundleId);
            return b ? <span className="chip chip-info" title={b.name}><Icon name="shield" size={9}/> {b.framework}</span> : null;
          })()}
          {(env.gatePolicyIds || []).length > 0 && (
            <span className="chip chip-unknown" title={(env.gatePolicyIds||[]).join(", ")}>{env.gatePolicyIds.length} gate{env.gatePolicyIds.length === 1 ? "" : "s"}</span>
          )}
          {!env.complianceBundleId && (env.gatePolicyIds || []).length === 0 && (
            <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>—</span>
          )}
        </div>
      </td>
      <td>
        {env.cache
          ? <span className="mono" style={{ fontSize:11 }} title={env.cache}>{env.cache.length > 30 ? env.cache.slice(0,28)+"…" : env.cache}</span>
          : <span style={{ fontSize:11, color:"var(--cf-text-muted)", fontStyle:"italic" }}>none</span>}
      </td>
      <td>{env.autoSync ? <span className="chip chip-healthy">on</span> : <span className="chip chip-unknown">off</span>}</td>
      <td>{env.requiresApproval ? <span className="chip chip-warning">required</span> : <span className="chip chip-healthy">not required</span>}</td>
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

function EnvCard({ env, onEdit, flash }) {
  const total = env.stats.total || 1;
  const healthPct = (env.stats.healthy / total) * 100;
  const warnPct   = (env.stats.warning / total) * 100;
  const critPct   = (env.stats.critical / total) * 100;
  const offPct    = (env.stats.offline / total) * 100;

  return (
    <div className={`env-card${flash ? " attention-flash" : ""}`} onClick={onEdit} style={{ cursor:"pointer" }}>
      <div className="env-card-rail" style={{ background: env.color }}/>
      <div className="env-card-head">
        <div>
          <div className="env-card-title">
            <span className="env-dot" style={{ background: env.color }}/>
            <span>{env.name}</span>
            {env.isProduction && <span className="env-prod-badge"><Icon name="shield" size={9}/> PROD</span>}
          </div>
          {env.description && (
            <div className="env-card-desc">{env.description}</div>
          )}
        </div>
        <div style={{ display:"flex", gap:4 }}>
          <button className="btn-icon focus-ring" title="Edit" onClick={(e)=>{e.stopPropagation();onEdit();}}>
            <Icon name="gear" size={14}/>
          </button>
        </div>
      </div>

      <div className="env-card-stat">
        <div className="env-card-stat-num">{env.stats.total}</div>
        <div className="env-card-stat-label">systems</div>
        <div style={{ flex:1 }}/>
        <div className="env-card-flakes">
          {env.stats.flakes.slice(0,3).map(f => (
            <span key={f} className="chip chip-unknown mono" style={{ fontSize:10 }}>{f}</span>
          ))}
          {env.stats.flakes.length > 3 && (
            <span className="chip chip-unknown" style={{ fontSize:10 }}>+{env.stats.flakes.length - 3}</span>
          )}
        </div>
      </div>

      <div className="env-health-bar">
        {env.stats.healthy  > 0 && <div style={{ width: `${healthPct}%`, background:"#34d399" }} title={`${env.stats.healthy} healthy`}/>}
        {env.stats.warning  > 0 && <div style={{ width: `${warnPct}%`,   background:"#fbbf24" }} title={`${env.stats.warning} warning`}/>}
        {env.stats.critical > 0 && <div style={{ width: `${critPct}%`,   background:"#f87171" }} title={`${env.stats.critical} critical`}/>}
        {env.stats.offline  > 0 && <div style={{ width: `${offPct}%`,    background:"#6b7280" }} title={`${env.stats.offline} offline`}/>}
      </div>
      <div className="env-health-legend">
        {env.stats.healthy  > 0 && <span><span className="env-health-sw" style={{background:"#34d399"}}/>{env.stats.healthy}</span>}
        {env.stats.warning  > 0 && <span><span className="env-health-sw" style={{background:"#fbbf24"}}/>{env.stats.warning}</span>}
        {env.stats.critical > 0 && <span><span className="env-health-sw" style={{background:"#f87171"}}/>{env.stats.critical}</span>}
        {env.stats.offline  > 0 && <span><span className="env-health-sw" style={{background:"#6b7280"}}/>{env.stats.offline}</span>}
        {env.stats.cveTotal > 0 && <span style={{ marginLeft:"auto" }}><Icon name="shield" size={10}/> {env.stats.cveTotal} CVE</span>}
      </div>

      <dl className="env-kv">
        <dt>Deploy</dt>
        <dd><span className={`chip ${env.defaultPolicy === "manual" ? "chip-warning" : "chip-healthy"}`}>{env.defaultPolicy || "—"}</span></dd>
        <dt>Enforcement</dt>
        <dd>
          <div style={{ display:"flex", gap:6, flexWrap:"wrap" }}>
            {env.complianceBundleId && (() => {
              const b = (typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : []).find(x => x.id === env.complianceBundleId);
              return b ? <span className="chip chip-info"><Icon name="shield" size={9}/> {b.framework}</span> : null;
            })()}
            {(env.gatePolicyIds || []).length > 0 && <span className="chip chip-unknown">{env.gatePolicyIds.length} gate{env.gatePolicyIds.length === 1 ? "" : "s"}</span>}
            {!env.complianceBundleId && (env.gatePolicyIds || []).length === 0 && <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>none</span>}
          </div>
        </dd>
        <dt>Cache</dt>
        <dd className="mono truncate" title={env.cache || "no cache"}>
          {env.cache
            ? <><Icon name="download" size={10} style={{ opacity:0.6, marginRight:4 }}/>{env.cache}</>
            : <span style={{ color:"var(--cf-text-muted)", fontStyle:"italic" }}>not configured</span>}
        </dd>
        <dt>Auto-sync</dt>
        <dd>{env.autoSync ? <span className="chip chip-healthy">on</span> : <span className="chip chip-unknown">off</span>}</dd>
        <dt>Approval</dt>
        <dd>{env.requiresApproval ? <span className="chip chip-warning">required</span> : <span className="chip chip-healthy">not required</span>}</dd>
      </dl>

      <div className="env-card-foot">
        <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>
          {Object.values(env.rbac || {}).flat().length} role assignments
        </span>
      </div>
    </div>
  );
}

// Side panel — environment reference peek, with Edit handing off to the form modal
function EnvPanel({ env, onClose, onEdit, onOpenCache, onOpenSystem, onOpenBundle }) {
  const total = env.stats.total || 1;
  const sys = SYSTEMS.filter(s => s.environment === env.name);
  return (
    <>
      <div className="side-panel-backdrop" onClick={onClose} />
      <aside className="side-panel" role="dialog" aria-modal="true">
        <div className="panel-head">
          <div className="panel-title">
            <h2>
              <span className="env-dot" style={{ background: env.color }} />
              {env.name}
              {env.isProduction && <span className="env-prod-badge"><Icon name="shield" size={9}/> PROD</span>}
            </h2>
            {env.description && <span className="fqdn">{env.description}</span>}
          </div>
          <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close">
            <Icon name="x" size={16} />
          </button>
        </div>
        <div className="panel-body">
          <section className="panel-section">
            <div style={{ display:"flex", gap:8, flexWrap:"wrap" }}>
              <span className={`chip ${env.defaultPolicy === "manual" ? "chip-warning" : "chip-healthy"}`}>{env.defaultPolicy || "—"}</span>
              {env.autoSync ? <span className="chip chip-healthy">auto-sync on</span> : <span className="chip chip-unknown">auto-sync off</span>}
              {env.requiresApproval ? <span className="chip chip-warning">approval required</span> : <span className="chip chip-healthy">no approval needed</span>}
            </div>
          </section>

          <section className="panel-section">
            <h3>Health</h3>
            <div className="env-health-bar" style={{ marginBottom:8 }}>
              {env.stats.healthy  > 0 && <div style={{ width: `${(env.stats.healthy/total)*100}%`, background:"#34d399" }} title={`${env.stats.healthy} healthy`}/>}
              {env.stats.warning  > 0 && <div style={{ width: `${(env.stats.warning/total)*100}%`,   background:"#fbbf24" }} title={`${env.stats.warning} warning`}/>}
              {env.stats.critical > 0 && <div style={{ width: `${(env.stats.critical/total)*100}%`,   background:"#f87171" }} title={`${env.stats.critical} critical`}/>}
              {env.stats.offline  > 0 && <div style={{ width: `${(env.stats.offline/total)*100}%`,    background:"#6b7280" }} title={`${env.stats.offline} offline`}/>}
            </div>
            <div className="env-health-legend">
              {env.stats.healthy  > 0 && <span><span className="env-health-sw" style={{background:"#34d399"}}/>{env.stats.healthy} healthy</span>}
              {env.stats.warning  > 0 && <span><span className="env-health-sw" style={{background:"#fbbf24"}}/>{env.stats.warning} warning</span>}
              {env.stats.critical > 0 && <span><span className="env-health-sw" style={{background:"#f87171"}}/>{env.stats.critical} critical</span>}
              {env.stats.offline  > 0 && <span><span className="env-health-sw" style={{background:"#6b7280"}}/>{env.stats.offline} offline</span>}
              {env.stats.cveTotal > 0 && <span style={{ marginLeft:"auto" }}><Icon name="shield" size={10}/> {env.stats.cveTotal} CVE</span>}
            </div>
          </section>

          <section className="panel-section">
            <h3>Configuration</h3>
            <dl className="kv-grid">
              <dt>Cache</dt>
              <dd className="mono truncate" title={env.cache || "no cache"}>
                {env.cache
                  ? <span className="sd-commit-sha-link" title={`Open ${env.cache} in Caches`} onClick={() => onOpenCache?.(env.cache)}><Icon name="download" size={10} /> {env.cache}</span>
                  : <span style={{ color:"var(--cf-text-muted)", fontStyle:"italic" }}>not configured</span>}
              </dd>
              <dt>Enforcement</dt>
              <dd>
                <div style={{ display:"flex", gap:6, flexWrap:"wrap" }}>
                  {env.complianceBundleId && (() => {
                    const b = (typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : []).find(x => x.id === env.complianceBundleId);
                    return b ? <span className="chip chip-info sd-commit-sha-link" title={`Open ${b.name} in Compliance`} onClick={() => onOpenBundle?.(b.id)}><Icon name="shield" size={9}/> {b.framework}</span> : null;
                  })()}
                  {(env.gatePolicyIds || []).length > 0 && <span className="chip chip-unknown">{env.gatePolicyIds.length} gate{env.gatePolicyIds.length === 1 ? "" : "s"}</span>}
                  {!env.complianceBundleId && (env.gatePolicyIds || []).length === 0 && <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>none</span>}
                </div>
              </dd>
              <dt>Role assignments</dt>
              <dd>{Object.values(env.rbac || {}).flat().length}</dd>
            </dl>
          </section>

          <section className="panel-section">
            <h3>Flakes in use</h3>
            <div style={{ display:"flex", gap:6, flexWrap:"wrap" }}>
              {env.stats.flakes.length ? env.stats.flakes.map(f => (
                <span key={f} className="chip chip-unknown mono" style={{ fontSize:11 }}>{f}</span>
              )) : <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>none deployed</span>}
            </div>
          </section>

          <section className="panel-section">
            <h3>Systems ({sys.length})</h3>
            <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
              {sys.slice(0, 8).map(s => (
                <div key={s.id} className="sd-commit-sha-link" style={{ display:"flex", alignItems:"center", gap:8, fontSize:12.5, padding:"3px 4px", margin:"-3px -4px" }} onClick={() => onOpenSystem?.(s)}>
                  <span className="status-dot" style={{ "--status-color": s.statusColor }} />
                  <span className="mono truncate" style={{ flex:1 }}>{s.hostname}</span>
                </div>
              ))}
              {sys.length > 8 && <div style={{ fontSize:11, color:"var(--cf-text-muted)" }}>+{sys.length - 8} more</div>}
              {!sys.length && <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>No systems in this environment yet.</div>}
            </div>
          </section>
        </div>
        <div className="panel-actions">
          <button className="btn btn-primary focus-ring" onClick={onEdit}><Icon name="gear" size={12} /> Edit environment</button>
        </div>
      </aside>
    </>
  );
}

function EnvFormModal({ mode, env, onClose }) {
  const isEdit = mode === "edit";
  const [form, setForm] = React.useState(() => isEdit && env ? {
    name: env.name,
    description: env.description || "",
    color: env.color,
    cacheId: env.cacheId || "",
    cache: env.cache || "",
    cacheType: env.cacheType || "none",
    defaultPolicy: env.defaultPolicy || "manual",
    gatePolicyIds: env.gatePolicyIds || [],
    complianceBundleId: env.complianceBundleId || "",
    autoSync: env.autoSync ?? true,
    requiresApproval: env.requiresApproval ?? true,
    isProduction: env.isProduction ?? false,
  } : {
    name: "",
    description: "",
    color: "#2563eb",
    cacheId: "",
    cache: "",
    cacheType: "s3",
    defaultPolicy: "manual",
    gatePolicyIds: [],
    complianceBundleId: "",
    autoSync: true,
    requiresApproval: true,
    isProduction: false,
  });
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const [addCacheOpen, setAddCacheOpen] = React.useState(false);
  const set = (k, v) => setForm(p => ({ ...p, [k]: v }));

  const COLORS = [
    { name:"red",     value:"#dc2626" },
    { name:"amber",   value:"#d97706" },
    { name:"emerald", value:"#059669" },
    { name:"blue",    value:"#2563eb" },
    { name:"teal",    value:"#0f766e" },
    { name:"violet",  value:"#7c3aed" },
    { name:"pink",    value:"#db2777" },
    { name:"slate",   value:"#475569" },
  ];

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(620px,96vw)", maxHeight:"92vh" }}>
        {confirmDelete ? (
          <DeleteEnvConfirm env={env} onCancel={()=>setConfirmDelete(false)} onConfirm={onClose}/>
        ) : (
          <>
            <div className="modal-head">
              <h2>
                <Icon name={isEdit ? "gear" : "plus"} size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>
                {isEdit ? `Edit ${env.name}` : "Add environment"}
              </h2>
              <p>{isEdit ? "Update environment settings, cache assignment, and deployment policy." : "Create a new environment tier."}</p>
            </div>
            <div className="modal-body" style={{ overflowY:"auto" }}>
              <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
                <div className="field">
                  <label>Name</label>
                  <input className="input focus-ring mono" value={form.name} onChange={e=>set("name",e.target.value)} placeholder="e.g. production"/>
                </div>
                <div className="field">
                  <label>Color</label>
                  <div style={{ display:"flex", gap:6, flexWrap:"wrap", alignItems:"center" }}>
                    {COLORS.map(c => (
                      <button key={c.value}
                        onClick={()=>set("color", c.value)}
                        title={c.name}
                        className="focus-ring"
                        style={{
                          width:28, height:28, borderRadius:8, cursor:"pointer",
                          background: c.value,
                          border: form.color === c.value ? "2px solid var(--cf-text-primary)" : "2px solid transparent",
                          boxShadow: form.color === c.value ? `0 0 0 2px ${c.value}` : "none",
                        }}/>
                    ))}
                    <label className="focus-ring" title="Custom color"
                      style={{
                        width:28, height:28, borderRadius:8, cursor:"pointer",
                        background: COLORS.find(c=>c.value===form.color) ? "var(--cf-subtle-bg)" : form.color,
                        border: !COLORS.find(c=>c.value===form.color) ? "2px solid var(--cf-text-primary)" : "2px dashed var(--cf-card-border)",
                        display:"flex", alignItems:"center", justifyContent:"center",
                        color: COLORS.find(c=>c.value===form.color) ? "var(--cf-text-muted)" : "white",
                      }}>
                      <Icon name="plus" size={12}/>
                      <input type="color" value={form.color} onChange={e=>set("color",e.target.value)}
                        style={{ opacity:0, position:"absolute", width:0, height:0 }}/>
                    </label>
                    <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)", marginLeft:4 }}>{form.color}</span>
                  </div>
                </div>
              </div>

              <div className="field">
                <label>Description</label>
                <input className="input focus-ring" value={form.description} onChange={e=>set("description",e.target.value)} placeholder="What this tier is for"/>
              </div>

              {/* Cache */}
              <div style={{ padding:14, border:"1px solid var(--cf-divider)", borderRadius:10, background:"color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg))" }}>
                <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:6, marginBottom:10 }}>
                  <div style={{ fontSize:13, fontWeight:600, display:"flex", alignItems:"center", gap:6 }}>
                    <Icon name="download" size={13}/> Binary cache
                  </div>
                  <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>Manage caches in the Caches view</span>
                </div>
                <select className="input focus-ring" value={form.cacheId || ""} onChange={e=>{
                  if (e.target.value === "__add__") { setAddCacheOpen(true); }
                  else { set("cacheId", e.target.value); }
                }}>
                  <option value="">No cache assigned</option>
                  {(window.CACHE_DESTINATIONS || []).map(c => (
                    <option key={c.id} value={c.id}>{c.name} ({c.type}) — {c.status}</option>
                  ))}
                  <option value="__add__">+ Add new cache…</option>
                </select>
                {form.cacheId && (() => {
                  const c = (window.CACHE_DESTINATIONS || []).find(x => x.id === form.cacheId);
                  if (!c) return null;
                  return (
                    <div style={{ marginTop:10, padding:10, background:"var(--cf-card-bg)", border:"1px solid var(--cf-divider)", borderRadius:8 }}>
                      <div className="mono" style={{ fontSize:11, color:"var(--cf-text-secondary)", marginBottom:6 }}>{c.url}</div>
                      <div style={{ display:"flex", gap:10, flexWrap:"wrap", fontSize:11, color:"var(--cf-text-muted)" }}>
                        <span className={`chip ${c.status === "healthy" ? "chip-healthy" : c.status === "warning" ? "chip-warning" : "chip-critical"}`}>{c.status}</span>
                        {c.storage && <span>{c.storage.used}/{c.storage.total} {c.storage.unit} used</span>}
                        {c.paths && <span>{c.paths.toLocaleString()} paths</span>}
                      </div>
                    </div>
                  );
                })()}
              </div>

              {/* Deployment behaviour + policy enforcement */}
              <div className="field">
                <label>Default deployment mode</label>
                <div className="seg" style={{ width:"fit-content", flexWrap:"wrap" }}>
                  {(typeof POLICIES !== "undefined" ? POLICIES : [])
                    .filter(p => (p.category || "deployment") === "deployment")
                    .map(p => (
                      <button key={p.id}
                        className={form.defaultPolicy === p.id ? "active" : ""}
                        onClick={()=>set("defaultPolicy", p.id)}>
                        {p.name}
                      </button>
                    ))}
                </div>
                <div className="help">
                  {form.defaultPolicy === "manual" && "New systems in this env default to operator-approved deploys."}
                  {form.defaultPolicy === "auto_latest" && "New systems auto-track the newest passing commit."}
                  {form.defaultPolicy === "pinned" && "New systems start pinned to whatever commit is selected at registration."}
                  {!["manual","auto_latest","pinned"].includes(form.defaultPolicy) && (() => {
                    const p = POLICIES.find(x => x.id === form.defaultPolicy);
                    return p ? p.description : "Default mode new systems in this env start with.";
                  })()}
                </div>
              </div>

              {/* Policy enforcement — scales from a few rules to a full bundle */}
              <div style={{ padding:14, border:"1px solid var(--cf-divider)", borderRadius:10, background:"color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg))" }}>
                <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:6, marginBottom:4 }}>
                  <div style={{ fontSize:13, fontWeight:600, display:"flex", alignItems:"center", gap:6 }}>
                    <Icon name="shield" size={13}/> Policy enforcement
                  </div>
                  <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>Applied to every system in this env</span>
                </div>
                <div className="help" style={{ marginTop:0, marginBottom:12 }}>
                  Pick a few à-la-carte gate policies, or require a full compliance bundle for regulated environments — or both.
                </div>

                {/* À la carte gate policies */}
                <div style={{ fontSize:11, fontWeight:600, color:"var(--cf-text-secondary)", marginBottom:6 }}>Gate policies</div>
                <GatePolicyPicker
                  selected={form.gatePolicyIds}
                  onChange={(ids) => set("gatePolicyIds", ids)}
                />
                <div className="help" style={{ marginBottom:14 }}>
                  {form.gatePolicyIds.length === 0
                    ? "No extra gates — just the deployment behaviour above. Fine for a homelab."
                    : `${form.gatePolicyIds.length} gate ${form.gatePolicyIds.length === 1 ? "policy" : "policies"} must pass before any deploy in this env.`}
                </div>

                {/* Compliance bundle requirement */}
                <div style={{ fontSize:11, fontWeight:600, color:"var(--cf-text-secondary)", marginBottom:6, display:"flex", alignItems:"center", justifyContent:"space-between" }}>
                  <span>Required compliance bundle</span>
                  <span style={{ fontSize:10, color:"var(--cf-text-muted)", fontWeight:400 }}>for regulated / ATO environments</span>
                </div>
                <select className="input focus-ring" value={form.complianceBundleId} onChange={e=>set("complianceBundleId", e.target.value)}>
                  <option value="">None — no compliance bundle required</option>
                  {(typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : []).map(b => (
                    <option key={b.id} value={b.id}>{b.name} ({b.framework})</option>
                  ))}
                </select>
                {form.complianceBundleId && (() => {
                  const b = (COMPLIANCE_BUNDLES || []).find(x => x.id === form.complianceBundleId);
                  if (!b) return null;
                  return (
                    <div className="sd-callout sd-callout-info" style={{ marginTop:10, fontSize:11 }}>
                      <Icon name="shield" size={12}/>
                      <div>
                        Systems must satisfy all <strong>{b.policyIds.length}</strong> controls in <strong>{b.name}</strong>. Non-compliant hosts are blocked from deploy and flagged in the Compliance view.
                      </div>
                    </div>
                  );
                })()}
              </div>

              <label className="env-prod-toggle" style={{ display:"flex", gap:11, alignItems:"flex-start", cursor:"pointer", padding:"11px 13px", border:`1px solid ${form.isProduction ? "color-mix(in oklab, var(--cf-danger-berry) 55%, var(--cf-card-border))" : "var(--cf-card-border)"}`, borderRadius:10, background: form.isProduction ? "color-mix(in oklab, var(--cf-danger-berry) 10%, transparent)" : "transparent", marginBottom:14 }}>
                <input type="checkbox" checked={form.isProduction} onChange={e=>set("isProduction",e.target.checked)} style={{ accentColor:"var(--cf-danger-berry)", marginTop:2 }}/>
                <span style={{ minWidth:0 }}>
                  <span style={{ display:"flex", alignItems:"center", gap:7, fontSize:13, fontWeight:600 }}>
                    <Icon name="shield" size={13} style={{ color: form.isProduction ? "#f87171" : "var(--cf-text-muted)" }}/>
                    Production environment
                  </span>
                  <span style={{ display:"block", fontSize:11.5, color:"var(--cf-text-muted)", marginTop:3, lineHeight:1.45 }}>
                    Flags hosts in this environment as production. Destructive actions (rollback, force-deploy) require a type-to-confirm guard, regardless of the environment's name.
                  </span>
                </span>
              </label>

              <div style={{ display:"flex", gap:18, flexWrap:"wrap" }}>
                <label style={{ display:"flex", gap:8, alignItems:"center", fontSize:13, cursor:"pointer" }}>
                  <input type="checkbox" checked={form.autoSync} onChange={e=>set("autoSync",e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
                  <span>Auto-sync flakes</span>
                </label>
                <label style={{ display:"flex", gap:8, alignItems:"center", fontSize:13, cursor:"pointer" }}>
                  <input type="checkbox" checked={form.requiresApproval} onChange={e=>set("requiresApproval",e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
                  <span>Require approval before deploy</span>
                </label>
              </div>

              {isEdit && (
                <div style={{ marginTop:10, paddingTop:14, borderTop:"1px solid var(--cf-divider)" }}>
                  <div style={{ fontSize:11, fontWeight:600, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", marginBottom:8 }}>Danger zone</div>
                  <button className="btn btn-ghost focus-ring" onClick={()=>setConfirmDelete(true)} style={{ color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }}>
                    <Icon name="x" size={12}/> Remove environment
                  </button>
                  {env.stats.total > 0 && (
                    <div className="help" style={{ marginTop:6 }}>
                      <Icon name="warn" size={10} style={{ color:"#fbbf24", verticalAlign:"middle" }}/> {env.stats.total} system{env.stats.total === 1 ? "" : "s"} currently use this env. Reassign them first.
                    </div>
                  )}
                </div>
              )}
            </div>
            <div className="modal-foot">
              <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
              <button className="btn btn-primary focus-ring" onClick={onClose}>
                <Icon name="check" size={13}/> {isEdit ? "Save changes" : "Add environment"}
              </button>
            </div>
          </>
        )}
      </div>
      {addCacheOpen && (
        <CacheFormModal
          mode="add"
          cache={null}
          onClose={(newCacheId) => {
            setAddCacheOpen(false);
            if (newCacheId) set("cacheId", newCacheId);
          }}
        />
      )}
    </div>
  );
}

function DeleteEnvConfirm({ env, onCancel, onConfirm }) {
  const [typed, setTyped] = React.useState("");
  const matches = typed === env.name;
  const hasSystems = env.stats.total > 0;
  return (
    <>
      <div className="modal-head" style={{ background:"rgba(248,113,113,0.06)" }}>
        <h2 style={{ color:"#fecaca", display:"flex", alignItems:"center", gap:8 }}>
          <Icon name="warn" size={16} style={{ color:"#f87171" }}/>
          Remove environment
        </h2>
        <p>This removes the <span className="mono" style={{ fontWeight:600 }}>{env.name}</span> environment.</p>
      </div>
      <div className="modal-body">
        {hasSystems && (
          <div className="sd-callout sd-callout-danger" style={{ marginBottom:12 }}>
            <Icon name="warn" size={14}/>
            <div style={{ fontSize:12, color:"#fecaca" }}>
              <strong>{env.stats.total} system{env.stats.total === 1 ? "" : "s"} still assigned to this environment.</strong> Reassign them before removing.
            </div>
          </div>
        )}
        <div className="field">
          <label>Type <span className="mono" style={{ color:"#fecaca", fontWeight:700 }}>{env.name}</span> to confirm</label>
          <input className="input focus-ring mono"
            placeholder={env.name}
            value={typed}
            onChange={e=>setTyped(e.target.value)}
            autoFocus
            disabled={hasSystems}
            style={{ borderColor: typed && !matches ? "rgba(248,113,113,0.5)" : undefined }}/>
        </div>
      </div>
      <div className="modal-foot">
        <button className="btn btn-ghost focus-ring" onClick={onCancel}>Cancel</button>
        <button className="btn focus-ring" disabled={!matches || hasSystems} onClick={onConfirm}
          style={{ background: matches && !hasSystems ? "#dc2626" : "var(--cf-subtle-bg)", color: matches && !hasSystems ? "white" : "var(--cf-text-muted)" }}>
          <Icon name="x" size={13}/> Remove environment
        </button>
      </div>
    </>
  );
}

/* Searchable, scalable multi-select for gate policies */
function GatePolicyPicker({ selected, onChange }) {
  const [query, setQuery] = React.useState("");
  const all = (typeof POLICIES !== "undefined" ? POLICIES : []).filter(p => p.type === "custom");
  const byId = Object.fromEntries(all.map(p => [p.id, p]));
  const toggle = (id) => onChange(selected.includes(id) ? selected.filter(x => x !== id) : [...selected, id]);

  const q = query.trim().toLowerCase();
  const matches = all.filter(p =>
    !selected.includes(p.id) &&
    (!q || p.name.toLowerCase().includes(q) || (p.description||"").toLowerCase().includes(q))
  );

  return (
    <div style={{ marginBottom:6 }}>
      {/* Selected chips */}
      {selected.length > 0 && (
        <div style={{ display:"flex", flexWrap:"wrap", gap:6, marginBottom:8 }}>
          {selected.map(id => {
            const p = byId[id];
            if (!p) return null;
            return (
              <span key={id} className="mono" style={{
                padding:"3px 6px 3px 10px", borderRadius:99, fontSize:11,
                border:"1px solid var(--cf-brand-purple)",
                background:"color-mix(in oklab, var(--cf-brand-purple) 14%, var(--cf-card-bg))",
                color:"var(--cf-brand-purple)", display:"inline-flex", alignItems:"center", gap:4,
              }}>
                {p.name}
                <button className="focus-ring" onClick={()=>toggle(id)} title="Remove"
                  style={{ all:"unset", cursor:"pointer", display:"inline-flex", padding:2, borderRadius:99 }}>
                  <Icon name="x" size={10}/>
                </button>
              </span>
            );
          })}
        </div>
      )}
      {/* Search */}
      <div className="filter-search" style={{ maxWidth:"none", margin:0 }}>
        <Icon name="search"/>
        <input className="input focus-ring" placeholder={`Search ${all.length} policies…`} value={query} onChange={e=>setQuery(e.target.value)}/>
      </div>
      {/* Results */}
      {q && (
        <div style={{ marginTop:6, maxHeight:180, overflowY:"auto", border:"1px solid var(--cf-divider)", borderRadius:8 }}>
          {matches.length === 0 ? (
            <div style={{ padding:"10px 12px", fontSize:12, color:"var(--cf-text-muted)" }}>No policies match “{query}”.</div>
          ) : matches.map(p => (
            <button key={p.id} className="focus-ring" onClick={()=>{ toggle(p.id); }}
              style={{
                all:"unset", cursor:"pointer", display:"flex", flexDirection:"column", gap:2,
                padding:"8px 12px", borderBottom:"1px solid var(--cf-divider)", width:"100%", boxSizing:"border-box",
              }}
              onMouseEnter={e=>e.currentTarget.style.background="var(--cf-subtle-bg)"}
              onMouseLeave={e=>e.currentTarget.style.background="transparent"}>
              <span className="mono" style={{ fontSize:12, fontWeight:600 }}>{p.name}</span>
              <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{p.description}</span>
            </button>
          ))}
        </div>
      )}
      {!q && selected.length === 0 && (
        <div className="help" style={{ marginTop:4 }}>Type to search and attach policies. Leave empty for no extra gates.</div>
      )}
    </div>
  );
}

Object.assign(window, { EnvironmentsView });
