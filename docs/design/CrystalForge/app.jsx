// Main Systems view + Tweaks panel

function SystemsView({ density, defaultView, onDensity, onDefaultView, onOpenDetail, coach, tag = "all", onTag, initialFlake, onClearInitialFlake }) {
  const [view, setView] = React.useState(defaultView);
  const [env, setEnv] = React.useState("all");
  const [status, setStatus] = React.useState("all");
  const [flake, setFlake] = React.useState("all");
  const [query, setQuery] = React.useState("");
  const [selected, setSelected] = React.useState(null);
  const [pendingDeploy, setPendingDeploy] = React.useState(null);
  const [editTarget, setEditTarget] = React.useState(null);
  const [addOpen, setAddOpen] = React.useState(false);
  const setTag = onTag || (() => {});
  React.useEffect(() => {
    if (initialFlake) { setFlake(initialFlake); onClearInitialFlake?.(); }
  }, [initialFlake]);
  const isAttention = (s) => s.health === "critical" || s.health === "offline";
  const flashAttention = useAttentionFlash("systems", SYSTEMS.some(isAttention));

  React.useEffect(() => {setView(defaultView);}, [defaultView]);

  const fleetTags = (typeof allFleetTags === "function" ? allFleetTags() : []);

  const filtered = SYSTEMS.filter((s) => {
    if (env !== "all" && s.environment !== env) return false;
    if (flake !== "all" && s.flake !== flake) return false;
    if (tag !== "all" && !(s.tags || []).includes(tag)) return false;
    if (status !== "all") {
      if (status === "online" && (s.health === "offline" || s.health === "unknown")) return false;
      if (status === "offline" && s.health !== "offline") return false;
      if (status === "warning" && s.health !== "warning" && s.health !== "drifted") return false;
      if (status === "critical" && s.health !== "critical") return false;
    }
    if (query) {
      const q = query.toLowerCase();
      if (!s.hostname.toLowerCase().includes(q) &&
      !s.fqdn.toLowerCase().includes(q) &&
      !s.commit.toLowerCase().includes(q) &&
      !(s.tags || []).some((t) => t.toLowerCase().includes(q)) &&
      !s.flake.toLowerCase().includes(q)) return false;
    }
    return true;
  });

  const counts = {
    total: SYSTEMS.length,
    healthy: SYSTEMS.filter((s) => s.health === "healthy").length,
    warning: SYSTEMS.filter((s) => s.health === "warning" || s.health === "drifted").length,
    critical: SYSTEMS.filter((s) => s.health === "critical").length,
    offline: SYSTEMS.filter((s) => s.health === "offline").length,
    crit_cves: SYSTEMS.reduce((a, s) => a + s.cves.critical, 0)
  };

  const compact = density === "compact";

  return (
    <>
      <div className="page-head">
        <div>
          <h1 className="page-title">Systems</h1>
          <p className="page-subtitle">
            {counts.total} systems · {counts.healthy} healthy · {counts.warning + counts.critical + counts.offline} needing attention
          </p>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-ghost focus-ring"><Icon name="download" size={14} /> Export</button>
          <button className="btn btn-primary focus-ring" data-coach-target="system" onClick={() => setAddOpen(true)}><Icon name="plus" size={14} /> Add system</button>
        </div>
      </div>

      {/* Stat strip */}
      <div className="stat-strip">
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color": "#a78bfa" }} />
          <div className="stat-label">Total</div>
          <div className="stat-value">{counts.total}</div>
          <div className="stat-meta">across 5 environments</div>
          <div className="spark-bar">
            {[
            ["production", counts.total],
            ["staging", counts.total],
            ["dev", counts.total],
            ["edge", counts.total],
            ["lab", counts.total]].
            map(([env]) => {
              const n = SYSTEMS.filter((s) => s.environment === env).length;
              return <div key={env} className="spark-seg" style={{ width: `${n / counts.total * 100}%`, background: ENV_STYLE[env].fg }} title={`${env}: ${n}`} />;
            })}
          </div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color": "#34d399" }} />
          <div className="stat-label">Healthy</div>
          <div className="stat-value" style={{ color: "#34d399" }}>{counts.healthy}</div>
          <div className="stat-meta">{Math.round(counts.healthy / counts.total * 100)}% of fleet</div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color": "#fbbf24" }} />
          <div className="stat-label">Warning / drift</div>
          <div className="stat-value" style={{ color: "#fbbf24" }}>{counts.warning}</div>
          <div className="stat-meta">behind or drifted</div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color": "#f87171" }} />
          <div className="stat-label">Critical / offline</div>
          <div className="stat-value" style={{ color: "#f87171" }}>{counts.critical + counts.offline}</div>
          <div className="stat-meta">{counts.critical} failing · {counts.offline} offline</div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color": "#60a5fa" }} />
          <div className="stat-label">CVEs (critical)</div>
          <div className="stat-value">{counts.crit_cves}</div>
          <div className="stat-meta">across {SYSTEMS.filter((s) => s.cves.critical > 0).length} hosts</div>
        </div>
      </div>

      {/* Filter bar */}
      <div className="filterbar">
        <div className="filter-search">
          <Icon name="search" />
          <input
            className="input focus-ring"
            placeholder="Filter by hostname, commit, or flake…"
            value={query}
            onChange={(e) => setQuery(e.target.value)} />
          
        </div>
        <select className="input filter-select focus-ring" style={{ width: "auto" }} value={env} onChange={(e) => setEnv(e.target.value)}>
          <option value="all">All environments</option>
          {ENVIRONMENTS.map((e) => <option key={e.name} value={e.name}>{e.name}</option>)}
        </select>
        <select className="input filter-select focus-ring" style={{ width: "auto" }} value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="all">All statuses</option>
          <option value="online">Online</option>
          <option value="warning">Warning / drift</option>
          <option value="critical">Critical</option>
          <option value="offline">Offline</option>
        </select>
        <select className="input filter-select focus-ring" style={{ width: "auto" }} value={flake} onChange={(e) => setFlake(e.target.value)}>
          <option value="all">All flakes</option>
          {FLAKES.map((f) => <option key={f} value={f}>{f}</option>)}
        </select>
        <select className="input filter-select focus-ring" style={{ width: "auto" }} value={tag} onChange={(e) => setTag(e.target.value)} title="Filter by tag — free-form labels you assign per system">
          <option value="all">All tags</option>
          {fleetTags.map((t) => <option key={t} value={t}>#{t} · {SYSTEMS.filter((s) => (s.tags || []).includes(t)).length}</option>)}
        </select>
        {tag !== "all" && (
          <button className="btn btn-ghost focus-ring xs" onClick={() => setTag("all")} title="Clear tag filter">
            <Icon name="x" size={11} /> #{tag}
          </button>
        )}

        <div className="seg" role="tablist" aria-label="View mode">
          <button className={view === "cards" ? "active" : ""} onClick={() => setView("cards")}><Icon name="grid" size={12} /> Cards</button>
          <button className={view === "table" ? "active" : ""} onClick={() => setView("table")}><Icon name="rows" size={12} /> Table</button>
        </div>
        <div className="filter-count">{filtered.length} shown</div>
      </div>

      {/* Content */}
      {filtered.length === 0 ?
      <div className="empty">
          <h3>No systems match</h3>
          <div>Try clearing a filter or changing the search.</div>
        </div> :
      view === "cards" ?
      <div className="cards-grid">
          {filtered.map((sys) =>
        <SystemCard
          key={sys.id}
          sys={sys}
          compact={compact}
          flash={flashAttention && isAttention(sys)}
          onOpen={setSelected}
          onDeploy={(s) => onOpenDetail(s, "deploy")}
          onEdit={setEditTarget} />

        )}
        </div> :

      <div className="card" style={{ overflow: "hidden" }}>
          <table className={`sys-table${compact ? " compact" : ""}`}>
            <thead>
              <tr>
                <th style={{ width: "22%" }}>Host</th>
                <th>Env</th>
                <th>Status</th>
                <th>Flake · commit</th>
                <th>Deploy</th>
                <th>CVEs</th>
                <th>Heartbeat</th>
                <th style={{ textAlign: "right" }}> </th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((sys) =>
            <SystemRow
              key={sys.id}
              sys={sys}
              compact={compact}
              flash={flashAttention && isAttention(sys)}
              selected={selected?.id === sys.id}
              onOpen={setSelected}
              onDeploy={(s) => onOpenDetail(s, "deploy")}
              onEdit={setEditTarget} />

            )}
            </tbody>
          </table>
        </div>
      }

      {selected &&
      <SystemPanel
        sys={selected}
        onClose={() => setSelected(null)}
        onTagClick={(t) => { setTag(t); setSelected(null); }}
        onEdit={(s) => {setEditTarget(s);setSelected(null);}}
        onOpenDetail={(s, tab) => {setSelected(null);onOpenDetail(s, tab);}}
        pendingDeploy={pendingDeploy && pendingDeploy.sysId === selected.id ? pendingDeploy : null}
        onClearPending={() => setPendingDeploy(null)} />

      }
      {editTarget &&
      <EditSystemModal sys={editTarget} onClose={() => setEditTarget(null)} />
      }
      {addOpen &&
      <AddSystemModal onClose={() => setAddOpen(false)} coach={coach} />
      }
    </>);

}

function TweaksPanel({ open, onClose, theme, onTheme, density, onDensity, defaultView, onDefaultView, sidebarMode, onSidebarMode, topView, onTopView, coach }) {
  const Row = ({ label, opts, value, onChange }) =>
  <div className="tweaks-row">
      <label>{label}</label>
      <div className="tweaks-opts">
        {opts.map((o) =>
      <button key={o.value} className={value === o.value ? "active" : ""} onClick={() => onChange(o.value)}>
            {o.label}
          </button>
      )}
      </div>
    </div>;

  return (
    <div className={`tweaks${open ? " open" : ""}`}>
      <div className="tweaks-head">
        <strong>Tweaks</strong>
        <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={14} /></button>
      </div>
      <div className="tweaks-body">
        <Row label="View" value={topView} onChange={onTopView} opts={[
        { value: "systems", label: "Systems" },
        { value: "flakes", label: "Flakes" },
        { value: "builds", label: "Builds" },
        { value: "evals", label: "Evals" }]
        } />
        <Row label="Theme" value={theme} onChange={onTheme} opts={[
        { value: "dark", label: "Dark" },
        { value: "light", label: "Light" }]
        } />
        <Row label="Density" value={density} onChange={onDensity} opts={[
        { value: "comfortable", label: "Comfort" },
        { value: "compact", label: "Compact" }]
        } />
        <Row label="Default view" value={defaultView} onChange={onDefaultView} opts={[
        { value: "cards", label: "Cards" },
        { value: "table", label: "Table" }]
        } />
        <Row label="Sidebar" value={sidebarMode} onChange={onSidebarMode} opts={[
        { value: "full", label: "Full" },
        { value: "rail", label: "Rail" }]
        } />
        {coach &&
        <div className="tweaks-row">
          <label>Setup Coach</label>
          <div className="tweaks-opts" style={{ flexWrap: "wrap" }}>
            <button onClick={() => coach.relaunch()}>Relaunch</button>
            <button onClick={() => coach.reset()}>Reset progress</button>
            <button onClick={() => coach.fill()}>Mark all done</button>
          </div>
        </div>
        }
      </div>
    </div>);

}

// App root
function App() {
  const TWEAKS = /*EDITMODE-BEGIN*/{
    "theme": "dark",
    "density": "comfortable",
    "defaultView": "cards",
    "sidebarMode": "full"
  } /*EDITMODE-END*/;

  const [theme, setTheme] = React.useState(TWEAKS.theme);
  const [density, setDensity] = React.useState(TWEAKS.density);
  const [defaultView, setDefaultView] = React.useState(TWEAKS.defaultView);
  const [sidebarMode, setSidebarMode] = React.useState(TWEAKS.sidebarMode);
  const [tweaksOpen, setTweaksOpen] = React.useState(false);
  const [detailSystem, setDetailSystem] = React.useState(null);
  const [sysTag, setSysTag] = React.useState("all");
  const [editTarget, setEditTarget] = React.useState(null);
  const [complianceBundleId, setComplianceBundleId] = React.useState(null);
  const [pendingDeploy, setPendingDeploy] = React.useState(null);
  const [flakeFocus, setFlakeFocus] = React.useState(null);
  const [cacheFocus, setCacheFocus] = React.useState(null);
  const [buildFocus, setBuildFocus] = React.useState(null);
  const [evalFocus, setEvalFocus] = React.useState(null);
  const [sysFlake, setSysFlake] = React.useState(null);
  const [policyFocus, setPolicyFocus] = React.useState(null);
  const [detailTab, setDetailTab] = React.useState("overview");
  const [topView, setTopView] = React.useState("dashboard"); // dashboard | systems | builds | evals | flakes | environments | caches | cves
  const coach = useCoach();
  const [classif, setClassif] = React.useState(() => {
    try { const r = localStorage.getItem("cf.classification"); if (r) return JSON.parse(r); } catch {}
    return { enabled: false, level: "UNCLASSIFIED", text: "" };
  });
  React.useEffect(() => { try { localStorage.setItem("cf.classification", JSON.stringify(classif)); } catch {} }, [classif]);

  const goTo = (v) => { setTopView(v); setDetailSystem(null); };
  const openDetail = (s, tab) => { setDetailSystem(s); setDetailTab(tab || "overview"); };

  React.useEffect(() => {document.documentElement.setAttribute("data-theme", theme);}, [theme]);

  // Edit-mode wire-up
  React.useEffect(() => {
    const handler = (e) => {
      if (e.data?.type === "__activate_edit_mode") setTweaksOpen(true);
      if (e.data?.type === "__deactivate_edit_mode") setTweaksOpen(false);
    };
    window.addEventListener("message", handler);
    window.parent?.postMessage({ type: "__edit_mode_available" }, "*");
    return () => window.removeEventListener("message", handler);
  }, []);

  const persist = (key, val) => {
    window.parent?.postMessage({ type: "__edit_mode_set_keys", edits: { [key]: val } }, "*");
  };

  const sw = {
    theme: (v) => {setTheme(v);persist("theme", v);},
    density: (v) => {setDensity(v);persist("density", v);},
    defaultView: (v) => {setDefaultView(v);persist("defaultView", v);},
    sidebarMode: (v) => {setSidebarMode(v);persist("sidebarMode", v);}
  };

  return (
    <div className="app" style={{ "--sidebar-w": sidebarMode === "rail" ? "64px" : "240px", "--classif-h": classif.enabled ? "24px" : "0px", boxSizing: "border-box", paddingTop: classif.enabled ? 24 : 0, paddingBottom: classif.enabled ? 24 : 0 }}>
      {classif.enabled && <ClassificationBanner level={classif.level} text={classif.text} position="top" />}
      {classif.enabled && <ClassificationBanner level={classif.level} text={classif.text} position="bottom" />}
      <Sidebar rail={sidebarMode === "rail"} topView={topView} onNav={(v) => {setTopView(v);setDetailSystem(null);}} onToggleRail={() => sw.sidebarMode(sidebarMode === "rail" ? "full" : "rail")} />
      <div className="main">
        <Topbar
          theme={theme}
          onTheme={() => sw.theme(theme === "dark" ? "light" : "dark")}
          onTweaks={() => setTweaksOpen((o) => !o)}
          onNavigate={(v) => { setTopView(v); setDetailSystem(null); }}
          crumb={
          topView === "builds" ? { current: "Builds" } :
          topView === "evals" ? { current: "Evaluations" } :
          topView === "flakes" ? { current: "Flakes" } :
          topView === "environments" ? { current: "Environments" } :
          topView === "caches" ? { current: "Caches" } :
          topView === "builders" ? { current: "Builders" } :
          topView === "policies" ? { current: "Policies" } :
          topView === "compliance" ? { current: "Compliance" } :
          topView === "cves" ? { current: "CVEs" } :
          topView === "dashboard" ? { current: "Dashboard" } :
          topView === "admin" ? { current: "Server Management" } :
          topView === "scanning" ? { current: "Scanning" } :
          topView === "profile" ? { current: "Profile" } :
          detailSystem ? { parent: "Systems", current: detailSystem.hostname } :
          { current: "Systems" }
          } />
        
        <div className="content" data-screen-label={detailSystem ? `SystemDetail-${detailSystem.hostname}` : topView}>
          <CoachCallout coach={coach} topView={topView} onNavigate={goTo} />
          {topView === "builds" && <BuildsView focus={buildFocus} onClearFocus={() => setBuildFocus(null)} />}
          {topView === "evals" && <EvalsView focus={evalFocus} onClearFocus={() => setEvalFocus(null)} onOpenSystem={(s) => { setTopView("systems"); openDetail(s); }} onOpenPolicy={(id) => { setPolicyFocus(id); setTopView("policies"); }} />}
          {topView === "flakes" && <FlakesView defaultView={defaultView} focus={flakeFocus} onClearFocus={() => setFlakeFocus(null)} onOpenEval={(c) => { setEvalFocus(c); setTopView("evals"); }} onOpenBuild={(c) => { setBuildFocus(c); setTopView("builds"); }} onOpenSystems={(flakeName) => { setSysFlake(flakeName); setTopView("systems"); }} />}
          {topView === "environments" && <EnvironmentsView defaultView={defaultView} onOpenCache={(c) => { setCacheFocus(c); setTopView("caches"); }} onOpenSystem={(s) => { setTopView("systems"); openDetail(s); }} onOpenBundle={(id) => { setComplianceBundleId(id); setTopView("compliance"); }} />}
          {topView === "caches" && <CachesView focus={cacheFocus} onClearFocus={() => setCacheFocus(null)} onOpenSystem={(s) => { setTopView("systems"); openDetail(s); }} />}
          {topView === "builders" && <BuildersView defaultView={defaultView} />}
          {topView === "policies" && <PoliciesView onOpenSystem={(s)=>{ setTopView("systems"); openDetail(s); }} focus={policyFocus} onClearFocus={() => setPolicyFocus(null)} />}
          {topView === "compliance" && <ComplianceView selectedBundleId={complianceBundleId} onClearBundle={() => setComplianceBundleId(null)} onOpenSystem={(s)=>{ setTopView("systems"); openDetail(s); }}/>}
          {topView === "cves" && <CvesView onOpenSystem={(s)=>{ setTopView("systems"); openDetail(s); }}/>}
          {topView === "dashboard" && <DashboardView onNavigate={(r, focus) => { setTopView(r); setDetailSystem(null); if (focus && r === "evals") setEvalFocus(focus); if (focus && r === "builds") setBuildFocus(focus); }}/>}
          {topView === "admin" && <AdminView onNavigate={(r) => { setTopView(r); setDetailSystem(null); }} coach={coach} classif={classif} onClassif={setClassif}/>}
          {topView === "scanning" && <ScanningView onNavigate={(r) => { setTopView(r); setDetailSystem(null); }}/>}
          {topView === "profile" && <ProfileView prefs={{ theme, onTheme: sw.theme, density, onDensity: sw.density, defaultView, onDefaultView: sw.defaultView, sidebarMode, onSidebarMode: sw.sidebarMode }}/>}
          {topView === "systems" && (
          detailSystem ?
          <SystemDetail
            sys={detailSystem}
            onBack={() => setDetailSystem(null)}
            onNavigate={(view, focusOrBundle, sysFlakeArg) => {
              setTopView(view);
              setDetailSystem(null);
              if (view === "compliance" && focusOrBundle) setComplianceBundleId(focusOrBundle);
              if (view === "evals" && focusOrBundle) setEvalFocus(focusOrBundle);
              if (view === "builds" && focusOrBundle) setBuildFocus(focusOrBundle);
              if (view === "systems" && sysFlakeArg) setSysFlake(sysFlakeArg);
            }}
            onTagFilter={(t) => { setSysTag(t); setTopView("systems"); setDetailSystem(null); }}
            onDeploy={(s) => setPendingDeploy({ sysId: detailSystem.id, commit: s.pendingCommit, at: Date.now() })}
            onEdit={(s) => setEditTarget(s)}
            initialTab={detailTab}
            pendingDeploy={pendingDeploy && pendingDeploy.sysId === detailSystem.id ? pendingDeploy : null}
            onStartPending={(p) => setPendingDeploy({ sysId: detailSystem.id, at: Date.now(), ...p })}
            onClearPending={() => setPendingDeploy(null)} /> :


          <SystemsView
            density={density}
            defaultView={defaultView}
            onDensity={sw.density}
            onDefaultView={sw.defaultView}
            coach={coach}
            tag={sysTag}
            onTag={setSysTag}
            initialFlake={sysFlake}
            onClearInitialFlake={() => setSysFlake(null)}
            onOpenDetail={(s, tab) => openDetail(s, tab)} />)


          }
        </div>
        {editTarget &&
        <EditSystemModal sys={editTarget} onClose={() => setEditTarget(null)} />
        }
      </div>
      <TweaksPanel
        open={tweaksOpen}
        onClose={() => setTweaksOpen(false)}
        theme={theme}
        onTheme={sw.theme}
        density={density}
        onDensity={sw.density}
        defaultView={defaultView}
        onDefaultView={sw.defaultView}
        sidebarMode={sidebarMode}
        onSidebarMode={sw.sidebarMode}
        topView={topView}
        onTopView={(v) => {setTopView(v);setDetailSystem(null);}}
        coach={coach} />
      
      <SetupCoach coach={coach} onNavigate={goTo} />
      <CoachBubble coach={coach} topView={topView} />
    </div>);

}

const root = ReactDOM.createRoot(document.getElementById("root"));
root.render(<App />);