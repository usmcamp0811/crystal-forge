// CVE view — fleet-wide vulnerabilities

// Multi-select plumbing: the selection lives on CvesView; each table publishes the
// ids it shows (in display order) so Shift-click ranges stay within what you see.
const CveSelCtx = React.createContext(null);
const CveSelIdsCtx = React.createContext([]);
// ⌘/Ctrl/Shift-click on a group header takes every CVE in the group.
function cveGroupClick(sel, list, e) {
  if (!sel || !(e.metaKey || e.ctrlKey || e.shiftKey)) return false;
  e.preventDefault(); e.stopPropagation();
  const all = list.every(c => sel.has(c.id));
  list.forEach(c => sel.toggle(c.id, !all));
  return true;
}
function cveGroupSelN(sel, list) { return sel ? list.filter(c => sel.has(c.id)).length : 0; }

function CvesView({ onOpenSystem, focus, onClearFocus }) {
  const [query, setQuery] = React.useState("");
  const [sevFilter, setSevFilter] = React.useState("all");
  const [fixFilter, setFixFilter] = React.useState("all");
  const [acceptFilter, setAcceptFilter] = React.useState("all");
  const [pkgFilter, setPkgFilter] = React.useState("all");
  const [sort, setSort] = React.useState("severity");
  const [groupMode, setGroupModeState] = React.useState(() => { try { return localStorage.getItem("cf.cves.group") || "package"; } catch { return "package"; } }); // 'package' | 'env' | 'system' | 'flat'
  const setGroupMode = (m) => { setGroupModeState(m); try { localStorage.setItem("cf.cves.group", m); } catch {} };
  const [envFilter, setEnvFilter] = React.useState("all");
  const [expandedPkg, setExpandedPkg] = React.useState(null);
  const [selectedCve, setSelectedCve] = React.useState(null);
  const sel = useMultiSelect();
  const [batchOpen, setBatchOpen] = React.useState(false);
  const [, bumpTriage] = React.useReducer(x => x + 1, 0);
  useAttentionFlash("cves", (CVE_STATS.critical || 0) > 0);
  const seenNew = useCveSeenSet();
  React.useEffect(() => {
    if (!focus) return;
    const c = CVES.find(x => x.id === focus.id) || CVES.find(x => x.pkg === focus.pkg);
    if (c) { setQuery(c.id); setGroupMode("flat"); setSelectedCve(c); }
    else setQuery(focus.id || focus.pkg || "");
    onClearFocus?.();
  }, [focus]);

  const packages = React.useMemo(() => [...new Set(CVES.map((c) => c.pkg))], []);

  let filtered = CVES.filter((c) => {
    if (sevFilter !== "all" && c.severity !== sevFilter) return false;
    if (fixFilter === "available" && c.fix !== "available") return false;
    if (fixFilter === "pending" && c.fix !== "pending") return false;
    if (fixFilter === "exploited" && !c.exploited) return false;
    if (acceptFilter !== "all" && c.acceptance !== acceptFilter) return false;
    if (pkgFilter !== "all" && c.pkg !== pkgFilter) return false;
    if (envFilter !== "all" && !c.affected.some(id => (SYSTEMS.find(s => s.id === id) || {}).environment === envFilter)) return false;
    if (query) {
      const q = query.toLowerCase();
      if (!c.id.toLowerCase().includes(q) &&
      !c.pkg.toLowerCase().includes(q) &&
      !c.title.toLowerCase().includes(q)) return false;
    }
    return true;
  });

  if (sort === "cvss") filtered = [...filtered].sort((a, b) => b.cvss - a.cvss);
  if (sort === "age") filtered = [...filtered].sort((a, b) => a.ageDays - b.ageDays);
  if (sort === "affected") filtered = [...filtered].sort((a, b) => b.affectedCount - a.affectedCount);
  const quickSel = [
    ["Critical", filtered.filter(c => c.severity === "critical")],
    ["High", filtered.filter(c => c.severity === "high")],
    ["Patchable", filtered.filter(c => c.fix === "available")],
    ["Outstanding", filtered.filter(c => c.acceptance === "outstanding")],
  ].filter(([, l]) => l.length);
  const selList = CVES.filter(c => sel.has(c.id));
  const selPkgs = new Set(selList.map(c => c.pkg)).size;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">CVEs</h1>
          <p className="page-subtitle">
            {CVE_STATS.total} vulnerabilities · {CVE_STATS.systemsAffected} systems affected · {CVE_STATS.fixable} have patches
          </p>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-ghost focus-ring"><Icon name="sync" size={14} /> Rescan fleet</button>
          <CveExportMenuButton list={filtered} scope={envFilter !== "all" ? envFilter : null} />
        </div>
      </div>

      <div className="stat-strip">
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color": "#f87171" }} />
          <div className="stat-label">Critical</div>
          <div className="stat-value" style={{ color: "#f87171" }}>{CVE_STATS.critical}</div>
          <div className="stat-meta">{CVE_STATS.exploited} actively exploited</div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color": "#fbbf24" }} />
          <div className="stat-label">High</div>
          <div className="stat-value" style={{ color: "#fbbf24" }}>{CVE_STATS.high}</div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color": "#60a5fa" }} />
          <div className="stat-label">Patchable now</div>
          <div className="stat-value" style={{ color: "#60a5fa" }}>{CVE_STATS.fixable}</div>
          <div className="stat-meta">Just deploy newer flake</div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color": "#a78bfa" }} />
          <div className="stat-label">Accepted risk</div>
          <div className="stat-value" style={{ color: "#a78bfa" }}>{CVE_STATS.accepted + CVE_STATS.scheduled}</div>
          <div className="stat-meta">{CVE_STATS.accepted} accepted · {CVE_STATS.scheduled} scheduled</div>
        </div>
        <div className="stat">
          <span className="stat-accent" style={{ "--stat-color": "#34d399" }} />
          <div className="stat-label">Outstanding</div>
          <div className="stat-value" style={{ color: CVE_STATS.outstanding > 20 ? "#f87171" : "#34d399" }}>{CVE_STATS.outstanding}</div>
          <div className="stat-meta">need triage</div>
        </div>
      </div>

      {/* Insights moved to Dashboard view — keep CVEs page focused on the table */}

      <div className="filterbar">
        <div className="filter-search" style={{ maxWidth: 300 }}>
          <Icon name="search" />
          <input className="input focus-ring" placeholder="Search CVE / package / title…" value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <div className="seg">
          {[
          { v: "all", l: "All" },
          { v: "critical", l: "Critical" },
          { v: "high", l: "High" },
          { v: "medium", l: "Medium" },
          { v: "low", l: "Low" }].
          map((o) =>
          <button key={o.v} className={sevFilter === o.v ? "active" : ""} onClick={() => setSevFilter(o.v)}>{o.l}</button>
          )}
        </div>
        <div className="seg">
          {[
          { v: "all", l: "Any status" },
          { v: "available", l: "Has patch" },
          { v: "pending", l: "No patch" },
          { v: "exploited", l: "Exploited" }].
          map((o) =>
          <button key={o.v} className={fixFilter === o.v ? "active" : ""} onClick={() => setFixFilter(o.v)}>{o.l}</button>
          )}
        </div>
        <div className="seg">
          {[
          { v: "all", l: "Any triage" },
          { v: "outstanding", l: "Outstanding" },
          { v: "scheduled", l: "Scheduled" },
          { v: "accepted", l: "Accepted" }].
          map((o) =>
          <button key={o.v} className={acceptFilter === o.v ? "active" : ""} onClick={() => setAcceptFilter(o.v)}>{o.l}</button>
          )}
        </div>
        <div style={{ position: "relative", maxWidth: 200 }}>
          <input
            list="cve-pkg-list"
            className="input focus-ring mono"
            placeholder="All packages…"
            value={pkgFilter === "all" ? "" : pkgFilter}
            onChange={(e) => setPkgFilter(e.target.value.trim() ? e.target.value.trim() : "all")}
            style={{ fontSize: 12, paddingRight: pkgFilter !== "all" ? 28 : 12 }} />
          
          <datalist id="cve-pkg-list">
            {packages.map((p) => <option key={p} value={p} />)}
          </datalist>
          {pkgFilter !== "all" &&
          <button className="btn-icon focus-ring"
          onClick={() => setPkgFilter("all")}
          title="Clear"
          style={{ position: "absolute", right: 4, top: "50%", transform: "translateY(-50%)", padding: 4 }}>
              <Icon name="x" size={11} />
            </button>
          }
        </div>
        <select className="input filter-select focus-ring" style={{ width: "auto" }} value={envFilter} onChange={(e) => setEnvFilter(e.target.value)}>
          <option value="all">All environments</option>
          {ENVIRONMENTS.map((e) => <option key={e.name} value={e.name}>{e.name}</option>)}
        </select>
        <span className="filter-count" style={{ marginLeft: "auto", marginRight: 0 }}>Group</span>
        <div className="seg">
          {[
          { v: "package", l: "Package" },
          { v: "env", l: "Environment" },
          { v: "system", l: "Env › System" },
          { v: "flat", l: "None" }].
          map((o) =>
          <button key={o.v} className={groupMode === o.v ? "active" : ""} onClick={() => setGroupMode(o.v)}>{o.l}</button>
          )}
        </div>
        <span className="filter-count" style={{ marginLeft: 0, marginRight: 0 }}>Sort</span>
        <div className="seg">
          {[
          { v: "severity", l: "Severity" },
          { v: "cvss", l: "CVSS" },
          { v: "age", l: "Newest" },
          { v: "affected", l: "Most affected" }].
          map((o) =>
          <button key={o.v} className={sort === o.v ? "active" : ""} onClick={() => setSort(o.v)}>{o.l}</button>
          )}
        </div>
      </div>

      <div className="cve-sel-bar">
        <span className="cve-sel-label">Select</span>
        {quickSel.map(([l, list]) => {
          const on = list.every(c => sel.has(c.id));
          return <button key={l} type="button" className={`cve-sel-chip focus-ring${on ? " on" : ""}`} onClick={() => list.forEach(c => sel.toggle(c.id, !on))}>{l} <span className="mono">{list.length}</span></button>;
        })}
        <span style={{ marginLeft: "auto" }}><MultiSelectHint /></span>
      </div>

      <CveSelCtx.Provider value={sel}>
      {groupMode === "package" ?
      <CvePackageGroups
        cves={filtered}
        expanded={expandedPkg}
        onToggle={(p) => setExpandedPkg(expandedPkg === p ? null : p)}
        onSelectCve={setSelectedCve}
        seenNew={seenNew} /> :
      groupMode === "env" || groupMode === "system" ?
      <CveEnvGroups cves={filtered} nested={groupMode === "system"} envFilter={envFilter}
        onSelectCve={setSelectedCve} onOpenSystem={onOpenSystem} seenNew={seenNew} /> :

      <div className="card" style={{ overflow: "hidden" }}>
        <table className="sys-table">
          <thead>
            <tr>
              <th>CVE</th>
              <th>Severity</th>
              <th>CVSS</th>
              <th>Package</th>
              <th>Title</th>
              <th>Affected</th>
              <th>Fix</th>
              <th>Triage</th>
              <th>Age</th>
              <th style={{ textAlign: "right" }}> </th>
            </tr>
          </thead>
          <tbody>
            <CveSelIdsCtx.Provider value={filtered.map(c => c.id)}>
            {filtered.map((cve) => <CveRow key={cve.id} cve={cve} seenNew={seenNew} onOpen={() => setSelectedCve(cve)} />)}
            </CveSelIdsCtx.Provider>
            {filtered.length === 0 &&
            <tr><td colSpan={10} style={{ padding: 24, textAlign: "center", color: "var(--cf-text-muted)", fontSize: 13 }}>No CVEs match the current filters.</td></tr>
            }
          </tbody>
        </table>
      </div>
      }

      </CveSelCtx.Provider>

      <BulkBar count={sel.size} onClear={sel.clear}>
        <span style={{ fontSize: 12, color: "var(--cf-text-muted)" }}>{selPkgs} package{selPkgs === 1 ? "" : "s"}</span>
        <button type="button" className="btn btn-primary xs focus-ring" onClick={() => setBatchOpen(true)}><Icon name="shield" size={12} /> Triage {sel.size} together</button>
      </BulkBar>
      {batchOpen && window.CveBatchTriageModal && selList.length > 0 &&
      <window.CveBatchTriageModal cves={selList} defaultEnvs={envFilter !== "all" ? [envFilter] : null}
        onClose={() => setBatchOpen(false)}
        onSubmit={(out) => {
          selList.forEach(c => { if (out.has(c.id)) cveApplyFleetDispositions(c, out.get(c.id)); });
          setBatchOpen(false); sel.clear(); bumpTriage();
        }} />}

      {selectedCve && <CveDrawer key={selectedCve.id} cve={selectedCve} onClose={() => setSelectedCve(null)} onOpenSystem={onOpenSystem} />}
    </div>);

}

function CveInsights({ onCveClick }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "1.4fr 1fr", gap: 14 }}>
      {/* Top affected by env */}
      <div className="card" style={{ padding: 16 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 12 }}>
          <h3 style={{ margin: 0, fontSize: 13, fontWeight: 600 }}>Top affected systems by environment</h3>
          <span style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>worst 4 per env</span>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          {Object.entries(CVE_INSIGHTS.byEnv).map(([env, list]) =>
          <div key={env}>
              <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
                <EnvBadge env={env} />
                <span style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{list.length} hosts</span>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(180px, 1fr))", gap: 6 }}>
                {list.map(({ sys, counts }) =>
              <div key={sys.id} style={{
                padding: "8px 10px",
                border: "1px solid var(--cf-divider)",
                borderRadius: 8,
                background: "var(--cf-card-bg)",
                display: "flex", flexDirection: "column", gap: 4
              }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12 }}>
                      <span className="status-dot" style={{ "--status-color": sys.statusColor }} />
                      <span className="mono truncate" style={{ fontWeight: 600 }}>{sys.hostname}</span>
                    </div>
                    <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
                      {counts.critical > 0 && <span className="chip chip-critical" style={{ fontSize: 10, padding: "1px 6px" }}>{counts.critical} crit</span>}
                      {counts.high > 0 && <span className="chip chip-warning" style={{ fontSize: 10, padding: "1px 6px" }}>{counts.high} high</span>}
                      {counts.exploited > 0 && <span className="chip chip-critical" style={{ fontSize: 10, padding: "1px 6px" }}>{counts.exploited} exploited</span>}
                    </div>
                  </div>
              )}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Quick-patch + acceptance ratio */}
      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <div className="card" style={{ padding: 16 }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 }}>
            <h3 style={{ margin: 0, fontSize: 13, fontWeight: 600 }}>Quick-patch candidates</h3>
            <span className="chip chip-healthy" style={{ fontSize: 10 }}><Icon name="check" size={10} /> Patches available</span>
          </div>
          <div style={{ fontSize: 11, color: "var(--cf-text-muted)", marginBottom: 10 }}>
            Systems where the next eval would clear at least one CVE.
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {CVE_INSIGHTS.patchableSystems.slice(0, 6).map(({ sys, counts }) =>
            <div key={sys.id} style={{
              display: "flex", alignItems: "center", gap: 10,
              padding: "7px 10px",
              background: "var(--cf-subtle-bg)",
              borderRadius: 6,
              fontSize: 12
            }}>
                <span className="status-dot" style={{ "--status-color": sys.statusColor }} />
                <span className="mono" style={{ fontWeight: 600, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{sys.hostname}</span>
                <span style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{sys.environment}</span>
                <span className="chip chip-healthy" style={{ fontSize: 10 }}>{counts.fixable} fixable</span>
              </div>
            )}
          </div>
        </div>

        <div className="card" style={{ padding: 16 }}>
          <h3 style={{ margin: "0 0 10px", fontSize: 13, fontWeight: 600 }}>Triage status</h3>
          <CveTriageBar />
          <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 4, fontSize: 11 }}>
            <div style={{ display: "flex", justifyContent: "space-between" }}>
              <span><span className="env-health-sw" style={{ background: "#f87171" }} />Outstanding</span>
              <span style={{ color: "var(--cf-text-muted)", fontVariantNumeric: "tabular-nums" }}>{CVE_STATS.outstanding}</span>
            </div>
            <div style={{ display: "flex", justifyContent: "space-between" }}>
              <span><span className="env-health-sw" style={{ background: "#fbbf24" }} />Scheduled</span>
              <span style={{ color: "var(--cf-text-muted)", fontVariantNumeric: "tabular-nums" }}>{CVE_STATS.scheduled}</span>
            </div>
            <div style={{ display: "flex", justifyContent: "space-between" }}>
              <span><span className="env-health-sw" style={{ background: "#a78bfa" }} />Accepted (with justification)</span>
              <span style={{ color: "var(--cf-text-muted)", fontVariantNumeric: "tabular-nums" }}>{CVE_STATS.accepted}</span>
            </div>
          </div>
        </div>
      </div>
    </div>);

}

function CveTriageBar() {
  const total = CVE_STATS.total || 1;
  return (
    <div style={{ display: "flex", height: 8, borderRadius: 99, overflow: "hidden", background: "var(--cf-subtle-bg)" }}>
      <div style={{ width: `${CVE_STATS.outstanding / total * 100}%`, background: "#f87171" }} title={`${CVE_STATS.outstanding} outstanding`} />
      <div style={{ width: `${CVE_STATS.scheduled / total * 100}%`, background: "#fbbf24" }} title={`${CVE_STATS.scheduled} scheduled`} />
      <div style={{ width: `${CVE_STATS.accepted / total * 100}%`, background: "#a78bfa" }} title={`${CVE_STATS.accepted} accepted`} />
    </div>);

}

// Environment / system grouping. A CVE lands in every env (and host) it was found on,
// so group counts can sum past the filtered total — that's the point: per-env exposure.
const CVE_SEV_W = { critical: 1000, high: 100, medium: 10, low: 1 };
function cveTally(list) {
  const t = { critical: 0, high: 0, medium: 0, low: 0, fixable: 0, outstanding: 0, exploited: 0, score: 0 };
  list.forEach((c) => {
    t[c.severity] += 1; t.score += CVE_SEV_W[c.severity];
    if (c.fix === "available") t.fixable += 1;
    if (c.acceptance === "outstanding") t.outstanding += 1;
    if (c.exploited) t.exploited += 1;
  });
  return t;
}
function CveSevChips({ t }) {
  return (
    <div style={{ display: "flex", gap: 5, flexWrap: "wrap", justifyContent: "flex-end" }}>
      {t.exploited > 0 && <span className="chip chip-critical" style={{ fontSize: 10 }}>{t.exploited} exploited</span>}
      {t.critical > 0 && <span className="chip chip-critical" style={{ fontSize: 10 }}>{t.critical} crit</span>}
      {t.high > 0 && <span className="chip chip-warning" style={{ fontSize: 10 }}>{t.high} high</span>}
      {t.medium > 0 && <span className="chip chip-info" style={{ fontSize: 10 }}>{t.medium} med</span>}
      {t.low > 0 && <span className="chip chip-unknown" style={{ fontSize: 10 }}>{t.low} low</span>}
    </div>);
}
function CveTable({ cves, seenNew, onSelectCve, limit, onMore }) {
  const shown = limit ? cves.slice(0, limit) : cves;
  return (
    <>
      <table className="sys-table" style={{ fontSize: 12 }}>
        <thead>
          <tr><th>CVE</th><th>Severity</th><th>CVSS</th><th>Package</th><th>Title</th><th>Affected</th><th>Fix</th><th>Triage</th><th>Age</th><th style={{ textAlign: "right" }}> </th></tr>
        </thead>
        <tbody><CveSelIdsCtx.Provider value={shown.map(c => c.id)}>{shown.map((cve) => <CveRow key={cve.id} cve={cve} seenNew={seenNew} onOpen={() => onSelectCve(cve)} />)}</CveSelIdsCtx.Provider></tbody>
      </table>
      {cves.length > shown.length &&
      <button type="button" className="pv-more pv-more-row focus-ring" onClick={onMore}>
          Show {Math.min(25, cves.length - shown.length)} more · {cves.length - shown.length} hidden
        </button>}
    </>);
}
function CveEnvGroups({ cves, nested, envFilter, onSelectCve, onOpenSystem, seenNew }) {
  const sel = React.useContext(CveSelCtx);
  const [collapsed, setCollapsed] = React.useState({});
  const [limits, setLimits] = React.useState({});
  const sysById = React.useMemo(() => new Map(SYSTEMS.map((s) => [s.id, s])), []);
  const scanByHost = React.useMemo(() => new Map((typeof SCAN_HISTORY !== "undefined" ? SCAN_HISTORY : []).map((s) => [s.hostname, s])), []);
  const envs = React.useMemo(() => {
    const m = new Map();
    cves.forEach((c) => c.affected.forEach((id) => {
      const sys = sysById.get(id);
      if (!sys || (envFilter !== "all" && sys.environment !== envFilter)) return;
      if (!m.has(sys.environment)) m.set(sys.environment, { cves: new Set(), hosts: new Map() });
      const e = m.get(sys.environment);
      e.cves.add(c);
      if (!e.hosts.has(sys.id)) e.hosts.set(sys.id, { sys, cves: [] });
      e.hosts.get(sys.id).cves.push(c);
    }));
    return [...m.entries()].map(([env, e]) => {
      const list = cves.filter((c) => e.cves.has(c));
      const hosts = [...e.hosts.values()].map((h) => ({ ...h, t: cveTally(h.cves) })).sort((a, b) => b.t.score - a.t.score);
      const total = SYSTEMS.filter((s) => s.environment === env).length;
      return { env, list, hosts, total, t: cveTally(list) };
    }).sort((a, b) => b.t.score - a.t.score);
  }, [cves, envFilter]);

  if (envs.length === 0) return <div className="empty" style={{ margin: 0 }}><h3>No CVEs match</h3><div>Try clearing a filter.</div></div>;

  const isOpen = (k, dflt) => collapsed[k] != null ? !collapsed[k] : dflt;
  const toggle = (k, dflt) => setCollapsed((c) => ({ ...c, [k]: isOpen(k, dflt) }));
  const lim = (k) => limits[k] || 10;
  const more = (k) => setLimits((l) => ({ ...l, [k]: lim(k) + 25 }));

  return (
    <div className="card" style={{ overflow: "hidden" }}>
      <div className="pv-groups">
        {envs.map((g, i) => {
          const k = `env:${g.env}`, dflt = i < 3 || envs.length <= 4, open = isOpen(k, dflt);
          const envColor = ENV_STYLE[g.env] && ENV_STYLE[g.env].fg || "#9ca3af";
          const clean = g.total - g.hosts.length;
          return (
            <section key={k} className="pv-group">
              <div className="pv-group-head">
                <button type="button" className="pv-group-toggle focus-ring" aria-expanded={open} title="⌘/Ctrl-click to select every CVE in this environment" onClick={(e) => { if (cveGroupClick(sel, g.list, e)) return; toggle(k, dflt); }}>
                  <Icon name={open ? "chevron-down" : "chevron-right"} size={13} style={{ color: "var(--cf-text-muted)" }} />
                  <span className="pv-dot" style={{ background: envColor }} />
                  <span className="pv-group-name">{g.env}</span>
                  {cveGroupSelN(sel, g.list) > 0 && <span className="chip chip-info" style={{ fontSize: 10 }}>{cveGroupSelN(sel, g.list)} selected</span>}
                  <span className="pv-group-n">{g.list.length} CVE{g.list.length === 1 ? "" : "s"} · {g.hosts.length} of {g.total} host{g.total === 1 ? "" : "s"}</span>
                  <span style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{g.t.fixable} patchable · {g.t.outstanding} outstanding</span>
                </button>
                <CveSevChips t={g.t} />
              </div>
              {open && !nested && <CveTable cves={g.list} seenNew={seenNew} onSelectCve={onSelectCve} limit={lim(k)} onMore={() => more(k)} />}
              {open && nested &&
              <div className="cve-host-groups">
                  {g.hosts.map((h, j) => {
                  const hk = `sys:${h.sys.id}`, hd = j < 2, hOpen = isOpen(hk, hd);
                  const sc = scanByHost.get(h.sys.hostname);
                  return (
                    <div key={hk} className="cve-host-group">
                        <div className="cve-host-head">
                          <button type="button" className="pv-group-toggle focus-ring" aria-expanded={hOpen} title="⌘/Ctrl-click to select every CVE on this host" onClick={(e) => { if (cveGroupClick(sel, h.cves, e)) return; toggle(hk, hd); }}>
                            <Icon name={hOpen ? "chevron-down" : "chevron-right"} size={12} style={{ color: "var(--cf-text-muted)" }} />
                            <span className="status-dot" style={{ "--status-color": h.sys.statusColor }} />
                            <span className="mono" style={{ fontSize: 12.5, fontWeight: 600 }}>{h.sys.hostname}</span>
                            <span className="mono cve-host-flake" style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{h.sys.flake}</span>
                            <span className="pv-group-n">{h.cves.length} CVE{h.cves.length === 1 ? "" : "s"}</span>
                            {cveGroupSelN(sel, h.cves) > 0 && <span className="chip chip-info" style={{ fontSize: 10 }}>{cveGroupSelN(sel, h.cves)} selected</span>}
                          </button>
                          {sc &&
                        <span className="cve-host-fresh" title={`Scan freshness · ${sc.scanned} fresh · ${sc.stale} stale · ${sc.needsBuild} need build · ${sc.unscanned} never scanned`}>
                              <span className="cve-host-bar">
                                <span style={{ flex: sc.scanned, background: "#34d399" }} />
                                <span style={{ flex: sc.stale, background: "#fbbf24" }} />
                                <span style={{ flex: sc.needsBuild, background: "#f59e0b" }} />
                                <span style={{ flex: sc.unscanned, background: "#4b5563" }} />
                              </span>
                              <span className="mono">{sc.scanned}/{sc.totalConfigs} scanned</span>
                            </span>}
                          <CveSevChips t={h.t} />
                          <button className="btn-icon focus-ring" title={`Open ${h.sys.hostname}`} onClick={() => onOpenSystem?.(h.sys)}><Icon name="arrow-right" size={13} /></button>
                        </div>
                        {hOpen && <CveTable cves={h.cves} seenNew={seenNew} onSelectCve={onSelectCve} limit={lim(hk)} onMore={() => more(hk)} />}
                      </div>);
                })}
                  {clean > 0 && <div className="cve-host-clean"><Icon name="check" size={11} /> {clean} other host{clean === 1 ? "" : "s"} in {g.env} with no matching CVEs</div>}
                </div>}
            </section>);
        })}
      </div>
    </div>);
}

// CVEs newly reported (ageDays <= 1) stay flagged until the user actually opens their
// package group and looks at them — that's the dismissal, not a timer.
function cveSeenIds() {
  try { return new Set(JSON.parse(localStorage.getItem("cf.cves.seenNew") || "[]")); } catch { return new Set(); }
}
function cveMarkSeen(ids) {
  if (!ids.length) return;
  const s = cveSeenIds();
  ids.forEach(id => s.add(id));
  try { localStorage.setItem("cf.cves.seenNew", JSON.stringify([...s])); } catch {}
  window.dispatchEvent(new CustomEvent("cf-cve-seen-change"));
}
function useCveSeenSet() {
  const [v, setV] = React.useState(0);
  React.useEffect(() => {
    const h = () => setV(n => n + 1);
    window.addEventListener("cf-cve-seen-change", h);
    return () => window.removeEventListener("cf-cve-seen-change", h);
  }, []);
  return React.useMemo(cveSeenIds, [v]);
}

function CvePackageGroups({ cves, expanded, onToggle, onSelectCve, seenNew }) {
  // Group CVEs by package
  const groups = React.useMemo(() => {
    const m = new Map();
    cves.forEach((c) => {
      if (!m.has(c.pkg)) m.set(c.pkg, []);
      m.get(c.pkg).push(c);
    });
    // Score each group by severity sum
    const sevWeight = { critical: 1000, high: 100, medium: 10, low: 1 };
    return [...m.entries()].map(([pkg, list]) => {
      const counts = { critical: 0, high: 0, medium: 0, low: 0 };
      const systems = new Set();
      let fixable = 0,outstanding = 0,exploited = 0,maxCvss = 0;
      list.forEach((c) => {
        counts[c.severity] += 1;
        c.affected.forEach((s) => systems.add(s));
        if (c.fix === "available") fixable += 1;
        if (c.acceptance === "outstanding") outstanding += 1;
        if (c.exploited) exploited += 1;
        if (c.cvss > maxCvss) maxCvss = c.cvss;
      });
      const score = list.reduce((a, c) => a + sevWeight[c.severity], 0);
      const isNew = list.some(c => c.ageDays <= 1 && !seenNew.has(c.id));
      return { pkg, list, counts, systemsCount: systems.size, fixable, outstanding, exploited, maxCvss, score, isNew };
    }).sort((a, b) => (b.isNew - a.isNew) || (b.score - a.score));
  }, [cves, seenNew]);

  if (groups.length === 0) {
    return <div className="empty" style={{ margin: 0 }}><h3>No CVEs match</h3><div>Try clearing a filter.</div></div>;
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      {groups.map((g) =>
      <CvePackageGroup
        key={g.pkg}
        group={g}
        isExpanded={expanded === g.pkg || (expanded == null && g.isNew)}
        onToggle={() => { onToggle(g.pkg); if (g.isNew) cveMarkSeen(g.list.filter(c => c.ageDays <= 1).map(c => c.id)); }}
        onSelectCve={onSelectCve}
        seenNew={seenNew} />

      )}
    </div>);

}

function CvePackageGroup({ group, isExpanded, onToggle, onSelectCve, seenNew }) {
  const sel = React.useContext(CveSelCtx);
  const selN = cveGroupSelN(sel, group.list);
  const sevColor = group.counts.critical > 0 ? "#f87171" :
  group.counts.high > 0 ? "#fbbf24" :
  group.counts.medium > 0 ? "#60a5fa" : "#9ca3af";
  return (
    <div className={`card${group.isNew ? " cve-pkg-new" : ""}`} style={{ overflow: "hidden" }}>
      <button className="focus-ring" title="⌘/Ctrl-click to select every CVE in this package" onClick={(e) => { if (cveGroupClick(sel, group.list, e)) return; onToggle(); }}
      style={{
        all: "unset", display: "grid",
        boxShadow: selN && selN === group.list.length ? "inset 0 0 0 1px var(--cf-brand-purple)" : "none",
        gridTemplateColumns: "24px 1fr auto auto",
        alignItems: "center", gap: 14,
        padding: "14px 18px",
        cursor: "pointer",
        width: "100%",
        background: isExpanded ? "color-mix(in oklab,var(--cf-brand-purple) 6%,var(--cf-card-bg))" : "transparent",
        borderLeft: `3px solid ${sevColor}`,
        boxSizing: "border-box"
      }}>
        <Icon name={isExpanded ? "chevron-down" : "chevron-right"} size={14} style={{ color: "var(--cf-text-muted)" }} />
        <div style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
            <span className="mono" style={{ fontSize: 14, fontWeight: 700 }}>{group.pkg}</span>
            {group.isNew && <span className="chip cve-new-chip" style={{ fontSize: 10 }}>new</span>}
            {selN > 0 && <span className="chip chip-info" style={{ fontSize: 10 }}>{selN} selected</span>}
            <span style={{ fontSize: 12, color: "var(--cf-text-muted)" }}>{group.list.length} CVE{group.list.length === 1 ? "" : "s"}</span>
            {group.exploited > 0 && <span className="chip chip-critical" style={{ fontSize: 10 }}>{group.exploited} exploited</span>}
          </div>
          <div style={{ fontSize: 11, color: "var(--cf-text-secondary)" }}>
            {group.systemsCount} system{group.systemsCount === 1 ? "" : "s"} affected · {group.fixable} patchable · {group.outstanding} outstanding
          </div>
        </div>
        <div style={{ display: "flex", gap: 5, flexWrap: "wrap", justifyContent: "flex-end" }}>
          {group.counts.critical > 0 && <span className="chip chip-critical" style={{ fontSize: 10 }}>{group.counts.critical} crit</span>}
          {group.counts.high > 0 && <span className="chip chip-warning" style={{ fontSize: 10 }}>{group.counts.high} high</span>}
          {group.counts.medium > 0 && <span className="chip chip-info" style={{ fontSize: 10 }}>{group.counts.medium} med</span>}
          {group.counts.low > 0 && <span className="chip chip-unknown" style={{ fontSize: 10 }}>{group.counts.low} low</span>}
        </div>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 2, minWidth: 96 }}>
          <div style={{ fontSize: 10, color: "var(--cf-text-muted)", textTransform: "uppercase", letterSpacing: "0.06em" }}>Worst CVSS</div>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <div style={{ width: 50, height: 5, background: "var(--cf-subtle-bg)", borderRadius: 99, overflow: "hidden" }}>
              <div style={{ width: `${group.maxCvss / 10 * 100}%`, height: "100%", background: sevColor }} />
            </div>
            <span className="mono" style={{ fontSize: 12, color: "var(--cf-text-primary)", fontWeight: 600 }}>{group.maxCvss.toFixed(1)}</span>
          </div>
        </div>
      </button>

      {isExpanded &&
      <div style={{ borderTop: "1px solid var(--cf-divider)" }}>
          <table className="sys-table" style={{ fontSize: 12 }}>
            <thead>
              <tr>
                <th>CVE</th>
                <th>Severity</th>
                <th>CVSS</th>
                <th>Title</th>
                <th>Affected</th>
                <th>Fix</th>
                <th>Triage</th>
                <th>Age</th>
              </tr>
            </thead>
            <tbody>
              <CveSelIdsCtx.Provider value={group.list.map(c => c.id)}>
              {group.list.map((cve) => <CveRow key={cve.id} cve={cve} seenNew={seenNew} onOpen={() => onSelectCve(cve)} />)}
              </CveSelIdsCtx.Provider>
            </tbody>
          </table>
        </div>
      }
    </div>);

}

function CveRow({ cve, onOpen, seenNew }) {
  const sevCls = { critical: "chip-critical", high: "chip-warning", medium: "chip-info", low: "chip-unknown" }[cve.severity];
  const sevColor = { critical: "#f87171", high: "#fbbf24", medium: "#60a5fa", low: "#9ca3af" }[cve.severity];
  const isNew = cve.ageDays <= 1 && !(seenNew && seenNew.has(cve.id));
  const sel = React.useContext(CveSelCtx);
  const ids = React.useContext(CveSelIdsCtx);
  const checked = sel && sel.has(cve.id);
  return (
    <tr style={{ cursor: "pointer" }} className={`${isNew ? "cve-row-new" : ""}${checked ? " row-checked" : ""}`}
      onClick={(e) => { if (sel && sel.handleClick(e, cve.id, ids)) return; sel && sel.setAnchor(cve.id); if (isNew) cveMarkSeen([cve.id]); onOpen(); }}>
      <td>
        <div className="mono" style={{ fontWeight: 600, fontSize: 13, display: "flex", alignItems: "center", gap: 8 }}>
          {cve.id}
          {isNew && <span className="chip cve-new-chip" style={{ fontSize: 10 }}>new</span>}
          {cve.exploited && <span className="chip chip-critical" style={{ fontSize: 10 }} title="Actively exploited in the wild">exploited</span>}
        </div>
      </td>
      <td>
        <span className={`chip ${sevCls}`}>
          <span className="chip-dot" style={{ background: sevColor }} />
          {cve.severity}
        </span>
      </td>
      <td>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <div style={{ width: 40, height: 5, background: "var(--cf-subtle-bg)", borderRadius: 99, overflow: "hidden" }}>
            <div style={{ width: `${cve.cvss / 10 * 100}%`, height: "100%", background: sevColor }} />
          </div>
          <span className="mono" style={{ fontSize: 12, color: "var(--cf-text-primary)", fontWeight: 600 }}>{cve.cvss.toFixed(1)}</span>
        </div>
      </td>
      <td className="mono" style={{ fontSize: 12 }}>{cve.pkg}</td>
      <td style={{ fontSize: 13, maxWidth: 340 }}>
        <div className="truncate" title={cve.title}>{cve.title}</div>
      </td>
      <td>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <Icon name="server" size={11} style={{ color: "var(--cf-text-muted)" }} />
          <span className="mono" style={{ fontSize: 12, color: cve.affectedCount > 0 ? "var(--cf-text-primary)" : "var(--cf-text-muted)", fontWeight: 600 }}>
            {cve.affectedCount}
          </span>
          <span style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>/ {SYSTEMS.length}</span>
        </div>
      </td>
      <td>
        {cve.fix === "available" ?
        <span className="chip chip-healthy" title={`Fixed in ${cve.fixedIn}`}><Icon name="check" size={10} /> {cve.fixedIn}</span> :
        <span className="chip chip-warning">no patch yet</span>}
      </td>
      <td>
        {cve.acceptance === "accepted" && <span className="chip chip-info" title={cve.justification}>accepted</span>}
        {cve.acceptance === "scheduled" && <span className="chip chip-info" title={cve.justification}>scheduled</span>}
        {cve.acceptance === "outstanding" && <span className="chip chip-critical">outstanding</span>}
        {cve.acceptance === "partial" && <span className="chip chip-warning" title={cve.scopeEnvs ? `Decided for ${cve.scopeEnvs.join(", ")}` : undefined}>partial</span>}
      </td>
      <td style={{ fontSize: 12, color: "var(--cf-text-muted)" }}>{cve.ageDays}d</td>
      <td>
        <div className="row-actions">
          <button className="btn-icon focus-ring" title="Open advisory" onClick={(e) => {e.stopPropagation();}}>
            <Icon name="link" size={14} />
          </button>
          <button className="btn-icon focus-ring" title="Details" onClick={(e) => {e.stopPropagation();onOpen();}}>
            <Icon name="arrow-right" size={14} />
          </button>
        </div>
      </td>
    </tr>);

}

function CveDrawer({ cve, onClose, onOpenSystem }) {
  const sevColor = { critical: "#f87171", high: "#fbbf24", medium: "#60a5fa", low: "#9ca3af" }[cve.severity];
  const affectedSystems = SYSTEMS.filter((s) => cve.affected.includes(s.id));

  // Per-environment dispositions (mock — in real app persists to backend).
  // Legacy mock CVEs carry a single acceptance + scopeEnvs; seed those onto the envs they covered.
  const [dispositions, setDispositions] = React.useState(() => {
    if (cve.dispositions) return cve.dispositions;
    if (!cve.acceptance || cve.acceptance === "outstanding") return {};
    const envs = (cve.scopeEnvs && cve.scopeEnvs.length)
      ? cve.scopeEnvs
      : [...new Set(SYSTEMS.filter(s => cve.affected.includes(s.id)).map(s => s.environment))];
    const seed = {};
    envs.forEach(e => {
      seed[e] = cve.acceptance === "scheduled"
        ? { state:"scheduled", poamId: cve.poamId || null, owner: cve.remediationOwner || "ops-team", due: cve.reviewDate || null, plan: cve.justification, by: cve.justifiedBy, at: cve.justifiedAt }
        : { state:"accepted", justification: cve.justification, reviewDate: cve.reviewDate || null, by: cve.justifiedBy, at: cve.justifiedAt };
    });
    return seed;
  });
  const [showAccept, setShowAccept] = React.useState(false);

  React.useEffect(() => {
    const onKey = (e) => {if (e.key === "Escape") { showAccept ? setShowAccept(false) : onClose(); }};
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, showAccept]);

  const allAffectedEnvs = [...new Set(affectedSystems.map(s => s.environment))];

  // Rollup the list view and the evidence export read off of.
  const dispEnvs = allAffectedEnvs.filter(e => dispositions[e]);
  const states = [...new Set(dispEnvs.map(e => dispositions[e].state))];
  const rollup = dispEnvs.length === 0 ? "outstanding"
    : (dispEnvs.length < allAffectedEnvs.length || states.length > 1) ? "partial"
    : states[0];
  const openEnvs = allAffectedEnvs.filter(e => !dispositions[e]);
  const coveredCount = affectedSystems.filter(s => dispositions[s.environment]).length;

  const applyTriage = (next) => {
    cve.dispositions = next;
    const de = allAffectedEnvs.filter(e => next[e]);
    const st = [...new Set(de.map(e => next[e].state))];
    cve.acceptance = de.length === 0 ? "outstanding"
      : (de.length < allAffectedEnvs.length || st.length > 1) ? "partial" : st[0];
    const first = de.length ? next[de[0]] : null;
    cve.justification = first ? (first.justification || first.plan || null) : null;
    cve.justifiedBy = first ? first.by : null;
    cve.justifiedAt = first ? first.at : null;
    cve.scopeEnvs = de.length ? de : null;
    cve.poamId = de.map(e => next[e].poamId).find(Boolean) || null;
    setDispositions(next);
    setShowAccept(false);
  };
  const revokeEnv = (env) => {
    const next = { ...dispositions };
    delete next[env];
    applyTriage(next);
  };

  // Group affected by environment
  const byEnv = {};
  affectedSystems.forEach((s) => {
    byEnv[s.environment] = byEnv[s.environment] || [];
    byEnv[s.environment].push(s);
  });

  const [maximized, setMaximized] = React.useState(false);
  return (
    <>
      <div className="fl-tray-backdrop" onClick={onClose} />
      <aside className={`fl-tray${maximized?" fl-tray-max":""}`} role="dialog" aria-label={cve.id}>
        <header className="fl-tray-head">
          <div style={{ display: "flex", alignItems: "center", gap: 12, minWidth: 0, flex: 1 }}>
            <Icon name="shield" size={18} style={{ color: sevColor, flexShrink: 0 }} />
            <div style={{ minWidth: 0 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                <span className="mono" style={{ fontWeight: 700, fontSize: 15 }}>{cve.id}</span>
                <span className={`chip ${{ critical: "chip-critical", high: "chip-warning", medium: "chip-info", low: "chip-unknown" }[cve.severity]}`}>
                  <span className="chip-dot" style={{ background: sevColor }} />
                  {cve.severity}
                </span>
                {cve.exploited && <span className="chip chip-critical">exploited in the wild</span>}
              </div>
              <div style={{ fontSize: 12, color: "var(--cf-text-secondary)", marginTop: 3 }}>{cve.title}</div>
            </div>
          </div>
          <div style={{ display: "flex", gap: 6 }}>
            <button className="btn btn-ghost focus-ring xs"
            onClick={() => window.open(cve.advisoryUrl, "_blank", "noopener,noreferrer")}
            title={cve.advisoryUrl}>
              
              <Icon name="link" size={11} /> Advisory
            </button>
            {rollup === "outstanding" ? (
              <button className="btn btn-primary focus-ring xs" onClick={() => setShowAccept(true)}>
                <Icon name="shield" size={11} /> Triage
              </button>
            ) : (
              <button className="btn btn-ghost focus-ring xs" onClick={() => setShowAccept(true)}>
                <Icon name="file" size={11} /> Edit triage
              </button>
            )}
            <button className="btn-icon focus-ring" title={maximized?"Restore":"Expand"} onClick={()=>setMaximized(m=>!m)}><Icon name={maximized?"minimize":"maximize"} size={15}/></button>
            <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16} /></button>
          </div>
        </header>

        {/* Stat band */}
        <div className="ed-stats">
          <div className="ed-stat">
            <div className="ed-stat-label">CVSS</div>
            <div className="ed-stat-val" style={{ color: sevColor }}>{cve.cvss.toFixed(1)}</div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Package</div>
            <div className="ed-stat-val mono" style={{ fontSize: 14 }}>{cve.pkg}</div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Affected</div>
            <div className="ed-stat-val">
              <span>{cve.affectedCount}</span>
              <span style={{ fontSize: 11, color: "var(--cf-text-muted)", fontWeight: 400 }}> / {SYSTEMS.length}</span>
            </div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Fix</div>
            <div className="ed-stat-val" style={{ fontSize: 14 }}>
              {cve.fix === "available" ?
              <span style={{ color: "#34d399" }} className="mono">{cve.fixedIn}</span> :
              <span style={{ color: "#fbbf24" }}>pending</span>}
            </div>
          </div>
          <div className="ed-stat">
            <div className="ed-stat-label">Discovered</div>
            <div className="ed-stat-val" style={{ fontSize: 14 }}>{cve.discoveredAt}</div>
          </div>
        </div>

        {/* Body */}
        <div className="ed-body" style={{ padding: "18px 22px", display: "flex", flexDirection: "column", gap: 18, overflow: "auto" }}>
          {/* Vector */}
          <section>
            <h3 style={{ fontSize: 11, textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--cf-text-muted)", margin: "0 0 8px", fontWeight: 600 }}>CVSS vector</h3>
            <code className="mono" style={{ fontSize: 12, color: "var(--cf-text-primary)", background: "var(--cf-subtle-bg)", padding: "6px 10px", borderRadius: 6, display: "inline-block" }}>
              {cve.vector}
            </code>
          </section>

          {/* Evidence relations — which exact evidence tier each host sits in */}
          {(() => {
            const relCurrent = affectedSystems;
            const relScheduled = affectedSystems.filter(s => s.deploymentState === "behind" || s.pendingCommit);
            const relHistorical = SYSTEMS.filter(s => !cve.affected.includes(s.id)).filter((s, i) => (i + cve.id.length) % 6 === 0).slice(0, 3);
            const relMissing = SYSTEMS.filter(s => !cve.affected.includes(s.id) && affectedSystems.some(a => a.flake === s.flake) && (s.health === "offline" || s.health === "unknown"));
            const Hosts = ({ list }) => list.length === 0
              ? <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>none</span>
              : <span style={{ display:"flex", gap:4, flexWrap:"wrap" }}>{list.slice(0, 4).map(s => <span key={s.id} className="mono cve-rel-host">{s.hostname}</span>)}{list.length > 4 && <span className="cve-rel-host">+{list.length - 4}</span>}</span>;
            const tiers = [
              { k:"current", c:"#34d399", l:"Current", d:"Exact evidence for the running configuration.", list:relCurrent, tag:"authorizes triage" },
              { k:"scheduled", c:"#60a5fa", l:"Scheduled deployment target", d:"Exact evidence for a scheduled target configuration.", list:relScheduled, tag:"read-only" },
              { k:"historical", c:"#9ca3af", l:"Historical", d:"Retained evidence with no current or scheduled authority.", list:relHistorical, tag:"read-only" },
            ];
            return (
              <section data-coach-target="cve-relations">
                <h3 style={{ fontSize: 11, textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--cf-text-muted)", margin: "0 0 8px", fontWeight: 600 }}>Exact evidence</h3>
                <div className="cve-rel">
                  {tiers.map(t => (
                    <div key={t.k} className="cve-rel-row">
                      <span className="cve-rel-dot" style={{ background:t.c }}/>
                      <div style={{ minWidth:0, flex:1 }}>
                        <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                          <strong style={{ fontSize:12 }}>{t.l}</strong>
                          <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{t.list.length} host{t.list.length === 1 ? "" : "s"}</span>
                          <span className={`cve-rel-tag${t.k === "current" ? " on" : ""}`}>{t.tag}</span>
                        </div>
                        <div style={{ fontSize:11, color:"var(--cf-text-secondary)", margin:"2px 0 5px" }}>{t.d}</div>
                        <Hosts list={t.list}/>
                      </div>
                    </div>
                  ))}
                </div>
                <div className="help" style={{ marginTop:8, display:"flex", gap:6, alignItems:"flex-start" }}>
                  <Icon name="warn" size={11} style={{ color:"#fbbf24", flexShrink:0, marginTop:2 }}/>
                  <span><strong style={{ color:"var(--cf-text-primary)" }}>Missing evidence does not mean clean.</strong> {relMissing.length > 0 ? `${relMissing.length} host${relMissing.length === 1 ? "" : "s"} on the same flake ${relMissing.length === 1 ? "has" : "have"} no current exact scan.` : "Hosts without a current exact scan are not listed here and are not clean."} A host can appear in Current and Scheduled when both exact configurations contain this package.</span>
                </div>
              </section>
            );
          })()}

          {/* Triage / acceptance — per environment */}
          <section>
            <div style={{ display:"flex", alignItems:"center", gap:8, margin:"0 0 10px" }}>
              <h3 style={{ fontSize: 11, textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--cf-text-muted)", margin: 0, fontWeight: 600 }}>Triage status</h3>
              {rollup !== "outstanding" && (
                <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>
                  {coveredCount} of {affectedSystems.length} host{affectedSystems.length === 1 ? "" : "s"} dispositioned
                </span>
              )}
              <button className="btn btn-ghost focus-ring xs" style={{ marginLeft:"auto" }} onClick={() => setShowAccept(true)}>
                <Icon name="file" size={10}/> {rollup === "outstanding" ? "Triage" : "Edit"}
              </button>
            </div>

            {rollup === "outstanding" ? (
              <div className="sd-callout sd-callout-warn">
                <Icon name="warn" size={13} />
                <div style={{ fontSize: 12 }}>
                  <strong>Outstanding — needs triage.</strong> Decide per environment: schedule a patch to open a POA&M with an owner and a due date, or accept the risk with a justification. You can do both at once — accept in dev, schedule for prod.
                </div>
              </div>
            ) : (
              <div style={{ display:"flex", flexDirection:"column", gap:8 }}>
                {allAffectedEnvs.map(env => {
                  const d = dispositions[env];
                  const hosts = affectedSystems.filter(s => s.environment === env).length;
                  if (!d) return (
                    <div key={env} style={{ display:"flex", alignItems:"center", gap:10, padding:"10px 12px", borderRadius:9, border:"1px solid rgba(248,113,113,0.3)", background:"rgba(248,113,113,0.06)" }}>
                      <EnvBadge env={env}/>
                      <span className="chip chip-critical" style={{ fontSize:10 }}>outstanding</span>
                      <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{hosts} host{hosts === 1 ? "" : "s"} · no disposition</span>
                      <button className="btn btn-ghost focus-ring xs" style={{ marginLeft:"auto" }} onClick={() => setShowAccept(true)}>Triage</button>
                    </div>
                  );
                  const accepted = d.state === "accepted";
                  const color = accepted ? "167,139,250" : "96,165,250";
                  return (
                    <div key={env} style={{ padding:"11px 12px", borderRadius:9, border:`1px solid rgba(${color},0.3)`, background:`rgba(${color},0.06)` }}>
                      <div style={{ display:"flex", alignItems:"center", gap:8, marginBottom:8 }}>
                        <EnvBadge env={env}/>
                        <span className="chip chip-info" style={{ fontSize:10, background:`rgba(${color},0.18)`, color: accepted ? "#a78bfa" : "#60a5fa" }}>
                          {accepted ? "risk accepted" : "patch scheduled"}
                        </span>
                        <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{hosts} host{hosts === 1 ? "" : "s"}</span>
                        <button className="btn-icon focus-ring" style={{ marginLeft:"auto" }} title={`Revoke disposition for ${env}`} onClick={() => revokeEnv(env)}>
                          <Icon name="x" size={13}/>
                        </button>
                      </div>
                      {(d.justification || d.plan) && (
                        <div style={{ fontSize:12.5, color:"var(--cf-text-primary)", lineHeight:1.5 }}>{d.justification || d.plan}</div>
                      )}
                      {!accepted && d.poamId && (
                        <button className="focus-ring" onClick={() => window.openPoamDetail?.(d.poamId)} style={{
                          all:"unset", cursor:"pointer", display:"flex", alignItems:"center", gap:8, marginTop:9, padding:"7px 10px", borderRadius:7,
                          border:"1px solid var(--cf-divider)", background:"var(--cf-subtle-bg)", width:"100%", boxSizing:"border-box",
                        }}>
                          <Icon name="file" size={12} style={{ color:"var(--cf-text-muted)" }}/>
                          <span style={{ fontSize:12 }}>Tracked by <span className="mono" style={{ fontWeight:600 }}>{d.poamId}</span></span>
                          {d.owner && <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>· {d.owner}</span>}
                          {d.due && <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>· due {d.due}</span>}
                          <Icon name="arrow-right" size={12} style={{ marginLeft:"auto", color:"var(--cf-text-muted)" }}/>
                        </button>
                      )}
                      <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:8, display:"flex", gap:8, alignItems:"center" }}>
                        <Icon name="user" size={11}/>
                        <span>by <span className="mono">{d.by || "—"}</span></span>
                        {d.at && <span>· {d.at}</span>}
                        {accepted && (d.reviewDate
                          ? <span>· review {d.reviewDate}</span>
                          : <span style={{ color:"#fbbf24" }}>· no review date</span>)}
                      </div>
                    </div>
                  );
                })}
                {openEnvs.length > 0 && (
                  <div className="help" style={{ color:"#fbbf24" }}>
                    <Icon name="warn" size={10} style={{ verticalAlign:"middle" }}/> {affectedSystems.length - coveredCount} host{affectedSystems.length - coveredCount === 1 ? "" : "s"} in {openEnvs.join(", ")} remain{affectedSystems.length - coveredCount === 1 ? "s" : ""} outstanding.
                  </div>
                )}
              </div>
            )}
          </section>

          {/* Remediation */}
          <section>
            <h3 style={{ fontSize: 11, textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--cf-text-muted)", margin: "0 0 10px", fontWeight: 600 }}>Remediation</h3>
            {cve.fix === "available" ?
            <div className="sd-callout sd-callout-info">
                <Icon name="check" size={13} />
                <div style={{ fontSize: 12 }}>
                  <div>Fixed in <span className="mono" style={{ fontWeight: 600, color: "#34d399" }}>{cve.pkg}-{cve.fixedIn}</span>. Affected systems will pick up the fix automatically once the upstream flake bumps the package and an eval passes.</div>
                </div>
              </div> :

            <div className="sd-callout sd-callout-danger">
                <Icon name="warn" size={13} />
                <div style={{ fontSize: 12 }}>
                  <strong>No upstream patch yet.</strong> Watch the advisory for updates. Consider applying compensating controls (network isolation, WAF rule) on affected hosts.
                </div>
              </div>
            }
            <dl className="kv-grid" style={{ marginTop: 10 }}>
              <dt>Introduced in</dt><dd className="mono">{cve.pkg}-{cve.introducedIn}</dd>
              <dt>Fixed in</dt><dd className="mono">{cve.fix === "available" ? `${cve.pkg}-${cve.fixedIn}` : "—"}</dd>
              <dt>Advisory</dt><dd className="mono"><a href="#" style={{ color: "var(--cf-brand-purple)" }}>nvd.nist.gov</a></dd>
            </dl>
          </section>

          {/* Affected systems */}
          <section>
            <h3 style={{ fontSize: 11, textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--cf-text-muted)", margin: "0 0 10px", fontWeight: 600 }}>
              Affected systems · {cve.affectedCount}
            </h3>
            {affectedSystems.length === 0 ?
            <div style={{ fontSize: 12, color: "var(--cf-text-muted)", padding: "12px 0" }}>No active systems affected. This CVE may apply to systems no longer in the registry.</div> :

            <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
                {Object.entries(byEnv).map(([env, sysList]) =>
              <div key={env}>
                    <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
                      <EnvBadge env={env} />
                      <span style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{sysList.length} host{sysList.length === 1 ? "" : "s"}</span>
                    </div>
                    <div className="card" style={{ overflow: "hidden", border: "1px solid var(--cf-divider)" }}>
                      <table className="sys-table" style={{ fontSize: 12 }}>
                        <tbody>
                          {sysList.map((sys) =>
                      <tr key={sys.id}>
                              <td style={{ width: "40%" }}>
                                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                                  <span className="status-dot" style={{ "--status-color": sys.statusColor }} />
                                  <span className="mono" style={{ fontWeight: 600 }}>{sys.hostname}</span>
                                </div>
                              </td>
                              <td className="mono" style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{sys.flake}</td>
                              <td className="mono" style={{ fontSize: 11 }}>{sys.commit}</td>
                              <td><DeploymentChip state={sys.deploymentState} /></td>
                              <td style={{ textAlign: "right" }}>
                                <button className="btn-icon focus-ring" title={`Open ${sys.hostname}`}
                          onClick={() => {onClose();onOpenSystem?.(sys);}}>
                                  <Icon name="arrow-right" size={13} />
                                </button>
                              </td>
                            </tr>
                      )}
                        </tbody>
                      </table>
                    </div>
                  </div>
              )}
              </div>
            }
          </section>
        </div>
      </aside>
      {showAccept && (
        <CveTriageModal
          cve={cve}
          affectedSystems={affectedSystems}
          initial={dispositions}
          onClose={() => setShowAccept(false)}
          onSubmit={applyTriage} />
      )}
    </>);

}

// Triage modal. Disposition is per environment, because scope and disposition are the same
// question: you can accept the risk in dev and schedule the patch in prod in one pass. Accepted
// environments produce a waiver; scheduled ones produce a single POA&M covering their hosts.
const CVE_CHOICES = [
  { v:"open",      label:"Leave open" },
  { v:"accepted",  label:"Accept risk" },
  { v:"scheduled", label:"Schedule patch" },
];

function CveTriageModal({ cve, affectedSystems, envSystems, initial, onClose, onSubmit, hostScope, submissionBlocked }) {
  // Opened from a host, the default blast radius is that host alone — deciding for
  // a whole environment from one machine's page is rarely what you meant.
  const [scope, setScope] = React.useState("host");
  const hostScoped = !!hostScope && scope === "host";
  // `affectedSystems` is the set the CVE was actually found on. The env roster is
  // separate: an env-scoped decision covers the environment prospectively, so it
  // must not enumerate unscanned hosts as evidence.
  const envRoster = envSystems || affectedSystems;
  const targetSystems = hostScoped
    ? [hostScope]
    : (hostScope ? affectedSystems.filter(s => s.environment === hostScope.environment) : affectedSystems);
  const envCounts = React.useMemo(() => {
    const m = {};
    targetSystems.forEach(s => { m[s.environment] = (m[s.environment] || 0) + 1; });
    return m;
  }, [targetSystems]);
  const allEnvs = Object.keys(envCounts);
  const hostDisp = (initial && initial.hosts) || {};
  const readInitial = (env) => hostScoped ? hostDisp[hostScope.id] : (initial && initial[env]);

  const [choice, setChoice] = React.useState(() => {
    const o = {};
    allEnvs.forEach(e => { o[e] = (readInitial(e) || {}).state || "open"; });
    return o;
  });
  // Re-seed when the scope flips: the two scopes have independent dispositions.
  React.useEffect(() => {
    const o = {};
    allEnvs.forEach(e => { o[e] = (readInitial(e) || {}).state || "open"; });
    setChoice(o);
  }, [scope]);
  const seeded = allEnvs.map(e => readInitial(e)).filter(Boolean);
  const seedAccepted = seeded.find(d => d.state === "accepted");
  const seedScheduled = seeded.find(d => d.state === "scheduled");

  const [justification, setJustification] = React.useState(seedAccepted ? seedAccepted.justification || "" : "");
  const [reviewDate, setReviewDate] = React.useState(seedAccepted ? seedAccepted.reviewDate || "" : "");
  const people = window.POAM_OWNER_PEOPLE || [];
  const [owner, setOwner] = React.useState(seedScheduled ? seedScheduled.owner || people[0] || "" : people[0] || "");
  const dueDefault = typeof poamDatePlus === "function"
    ? poamDatePlus(cve.severity === "critical" ? 14 : cve.severity === "high" ? 30 : 56) : "";
  const [due, setDue] = React.useState(seedScheduled ? seedScheduled.due || dueDefault : dueDefault);
  const [plan, setPlan] = React.useState(seedScheduled ? seedScheduled.plan || "" : "");
  const [withMilestones, setWithMilestones] = React.useState(!seedScheduled);

  const acceptedEnvs = allEnvs.filter(e => choice[e] === "accepted");
  const scheduledEnvs = allEnvs.filter(e => choice[e] === "scheduled");
  const openEnvs = allEnvs.filter(e => choice[e] === "open");
  const scheduledHosts = targetSystems.filter(s => scheduledEnvs.includes(s.environment));
  const acceptedHosts = targetSystems.filter(s => acceptedEnvs.includes(s.environment));
  const scopeLabel = hostScoped ? hostScope.hostname : null;
  const envWide = !!hostScope && !hostScoped;
  // One phrasing for the fix target, used by the context grid, the plan
  // placeholder and the generated plan text — gated on having a version, not on
  // the advisory merely saying a fix exists.
  const fixTarget = cve.fixedIn || (cve.fix === "available" ? "a patched release" : "a patched release once available");

  const acceptNeedsText = acceptedEnvs.length > 0 && justification.trim().length < 10;
  const scheduleNeedsFields = scheduledEnvs.length > 0 && (!owner || !due);
  // Host-scoped revoke: clearing an existing host override back to "open" is a
  // real change, even though nothing is accepted or scheduled.
  const revoking = hostScoped && seeded.length > 0 && openEnvs.length > 0;
  const touched = acceptedEnvs.length + scheduledEnvs.length > 0 || revoking;
  const canSubmit = touched && !acceptNeedsText && !scheduleNeedsFields && !submissionBlocked;

  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape") { e.stopPropagation(); onClose(); } };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const submit = () => {
    if (!canSubmit) return;
    const next = { ...(initial || {}) };
    // Host-scoped decisions live in their own map so they override the
    // environment default without rewriting it for every other host.
    const hosts = { ...((initial && initial.hosts) || {}) };
    const put = (env, rec) => { if (hostScoped) hosts[hostScope.id] = { ...rec, env }; else next[env] = rec; };
    if (hostScoped) { if (openEnvs.length) delete hosts[hostScope.id]; }
    else openEnvs.forEach(e => { delete next[e]; });

    if (acceptedEnvs.length) {
      acceptedEnvs.forEach(e => {
        put(e, { state:"accepted", justification: justification.trim(), reviewDate: reviewDate || null, by:"mreyes", at:"just now" });
      });
    }
    if (scheduledEnvs.length) {
      let poamId = seedScheduled && seedScheduled.poamId;
      if (!poamId && typeof poamCreate === "function") {
        const where = hostScoped ? hostScope.hostname : scheduledEnvs.join(", ");
        const covers = envWide
          ? `${scheduledEnvs.join(", ")} — every host in the environment, current and future`
          : `${scheduledHosts.length} host${scheduledHosts.length === 1 ? "" : "s"}`;
        const item = poamCreate({
          title: `${cve.id} — patch ${cve.pkg} in ${where}`,
          owner, due,
          severity: cve.severity === "critical" || cve.severity === "high" ? "high" : cve.severity === "medium" ? "medium" : "low",
          status: "open",
          plan: plan.trim() || `Upgrade ${cve.pkg} to ${fixTarget} across ${where}.`,
          // Only hosts the CVE was actually found on are attached as evidence.
          cveRefs: scheduledHosts.map(s => ({ id: cve.id, pkg: cve.pkg, sysId: s.id, hostname: s.hostname })),
          milestones: withMilestones ? poamPatchMilestones({
            due, pkg: cve.pkg, fixAvailable: cve.fix === "available",
            rolloutText: hostScoped ? `Roll out to ${hostScope.hostname}` : `Roll out to ${covers}`,
          }) : [],
        });
        poamId = item.id;
      }
      scheduledEnvs.forEach(e => {
        put(e, { state:"scheduled", poamId, owner, due, plan: plan.trim() || null, by:"mreyes", at:"just now" });
      });
    }
    if (Object.keys(hosts).length) next.hosts = hosts; else delete next.hosts;
    onSubmit(next);
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(720px,95vw)", maxHeight:"92vh" }}>
        <div className="modal-head" style={{ display:"flex", alignItems:"flex-start", justifyContent:"space-between", gap:12 }}>
          <div>
            <h2>Triage {cve.id}</h2>
            <p>{hostScope
              ? "Decide for this host alone, or for every host in its environment."
              : "Decide per environment. Hosts left open stay outstanding until someone dispositions them."}</p>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16}/></button>
        </div>

        <div className="modal-body" style={{ overflowY:"auto", display:"flex", flexDirection:"column", gap:14 }}>
          <div className="poam-ctx">
            <div className="poam-ctx-head">
              <Icon name="shield" size={12}/> Vulnerability
              <span style={{ marginLeft:"auto", fontSize:10.5, color:"var(--cf-text-muted)" }}>carried over automatically</span>
            </div>
            <div className="poam-ctx-grid">
              <div><span>CVE</span><b className="mono">{cve.id}</b></div>
              <div><span>Package</span><b className="mono">{cve.pkg}</b></div>
              <div><span>CVSS</span><b>{cve.cvss.toFixed(1)} <span style={{ fontWeight:400, color:"var(--cf-text-muted)" }}>{cve.severity}</span></b></div>
              <div><span>Affected hosts</span><b>{targetSystems.length}{envWide ? <span style={{ fontWeight:400, color:"var(--cf-text-muted)" }}> of {envRoster.length} in {hostScope.environment}</span> : null}</b></div>
              <div><span>Fix</span><b className="mono">{cve.fixedIn || (cve.fix === "available" ? "available — version pending" : "pending")}</b></div>
              <div><span>Exploited</span><b>{cve.exploited ? "yes — in the wild" : "not observed"}</b></div>
            </div>
          </div>

          {hostScope && (
            <div className="field" style={{ marginTop:0 }}>
              <label>Applies to</label>
              <div className="seg" style={{ width:"fit-content" }}>
                <button className={scope === "host" ? "active" : ""} onClick={()=>setScope("host")}>
                  {hostScope.hostname} only
                </button>
                <button className={scope === "env" ? "active" : ""} onClick={()=>setScope("env")}>
                  All of {hostScope.environment}
                </button>
              </div>
              <div className="help">
                {hostScoped
                  ? "A host-specific decision overrides the environment default for this machine only."
                  : `Covers all ${envRoster.length} host${envRoster.length === 1 ? "" : "s"} in ${hostScope.environment}, including ones added later. Only hosts the CVE was found on are attached as evidence.`}
                {hostScoped && hostDisp[hostScope.id] === undefined && initial && initial[hostScope.environment] && (
                  <> This host currently follows the {hostScope.environment} decision ({initial[hostScope.environment].state}).</>
                )}
              </div>
            </div>
          )}

          <div className="field" style={{ marginTop:0 }}>
            <label>{hostScoped ? "Disposition" : "Disposition by environment"}</label>
            <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
              {allEnvs.map(env => {
                const envColor = (ENV_STYLE[env] && ENV_STYLE[env].fg) || "#9ca3af";
                const c = choice[env];
                return (
                  <div key={env} style={{
                    display:"flex", alignItems:"center", gap:12, padding:"9px 11px", borderRadius:8,
                    border:`1px solid ${c === "open" ? "var(--cf-divider)" : envColor}`,
                    background: c === "open" ? "var(--cf-card-bg)" : `color-mix(in oklab, ${envColor} 7%, var(--cf-card-bg))`,
                  }}>
                    <span style={{ display:"flex", alignItems:"center", gap:8, minWidth:0, flex:1 }}>
                      <span style={{ width:8, height:8, borderRadius:99, background:envColor, flexShrink:0 }}/>
                      <span style={{ fontSize:12.5, fontWeight:600 }}>{hostScoped ? hostScope.hostname : env}</span>
                      <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{hostScoped ? env : `${envCounts[env]} host${envCounts[env] === 1 ? "" : "s"}`}</span>
                    </span>
                    <div className="seg" style={{ flexShrink:0 }}>
                      {CVE_CHOICES.map(o => (
                        <button key={o.v} className={c === o.v ? "active" : ""}
                          onClick={() => setChoice(p => ({ ...p, [env]: o.v }))}>{o.label}</button>
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>
            {openEnvs.length > 0 && touched && !hostScoped && (
              <div className="help" style={{ marginTop:6, color:"#fbbf24" }}>
                <Icon name="warn" size={10} style={{ verticalAlign:"middle" }}/> {openEnvs.join(", ")} stay{openEnvs.length === 1 ? "s" : ""} outstanding.
              </div>
            )}
          </div>

          {scheduledEnvs.length > 0 && (
            <div style={{ padding:"12px 13px", borderRadius:9, border:"1px solid rgba(96,165,250,0.3)", background:"rgba(96,165,250,0.06)", display:"flex", flexDirection:"column", gap:12 }}>
              <div style={{ display:"flex", alignItems:"center", gap:7, fontSize:11.5, fontWeight:600, textTransform:"uppercase", letterSpacing:".06em", color:"#60a5fa" }}>
                <Icon name="plus" size={12}/> POA&M — {scopeLabel || scheduledEnvs.join(", ")} · {envWide ? "all hosts" : `${scheduledHosts.length} host${scheduledHosts.length === 1 ? "" : "s"}`}
              </div>
              <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:12 }}>
                <div className="field" style={{ marginTop:0 }}>
                  <label>Owner</label>
                  {window.PoamOwnerOptions ? (
                    <select className="input focus-ring" value={owner} onChange={e=>setOwner(e.target.value)}>
                      <window.PoamOwnerOptions/>
                    </select>
                  ) : (
                    <input className="input focus-ring" value={owner} onChange={e=>setOwner(e.target.value)}/>
                  )}
                </div>
                <div className="field" style={{ marginTop:0 }}>
                  <label>Target completion</label>
                  <input type="date" className="input focus-ring" value={due} onChange={e=>setDue(e.target.value)}/>
                </div>
              </div>
              <div className="field" style={{ marginTop:0 }}>
                <label>Remediation plan <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· optional now, expected before review</span></label>
                <textarea className="input focus-ring" rows={2} value={plan} onChange={e=>setPlan(e.target.value)}
                  placeholder={`Upgrade ${cve.pkg} to ${fixTarget}, roll out, and verify the scan clears`}
                  style={{ resize:"vertical" }}/>
              </div>
              {!seedScheduled && (
                <label className="poam-check">
                  <input type="checkbox" checked={withMilestones} onChange={e=>setWithMilestones(e.target.checked)}/>
                  <span>Start from standard patch milestones <span style={{ color:"var(--cf-text-muted)" }}>— identify version, staging, rollout, verify scan.</span></span>
                </label>
              )}
              {cve.fix !== "available" && (
                <div className="help" style={{ color:"#fbbf24" }}>
                  <Icon name="warn" size={10} style={{ verticalAlign:"middle" }}/> No upstream patch yet — the first milestone tracks waiting on the advisory.
                </div>
              )}
            </div>
          )}

          {acceptedEnvs.length > 0 && (
            <div style={{ padding:"12px 13px", borderRadius:9, border:"1px solid rgba(167,139,250,0.3)", background:"rgba(167,139,250,0.06)", display:"flex", flexDirection:"column", gap:12 }}>
              <div style={{ display:"flex", alignItems:"center", gap:7, fontSize:11.5, fontWeight:600, textTransform:"uppercase", letterSpacing:".06em", color:"#a78bfa" }}>
                <Icon name="check" size={12}/> Waiver — {scopeLabel || acceptedEnvs.join(", ")} · {envWide ? "all hosts" : `${acceptedHosts.length} host${acceptedHosts.length === 1 ? "" : "s"}`}
              </div>
              <div className="field" style={{ marginTop:0 }}>
                <label>Justification <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· required</span></label>
                <textarea className="input focus-ring" rows={2} value={justification} onChange={e=>setJustification(e.target.value)}
                  placeholder="Why is this acceptable / what is the compensating control?" style={{ resize:"vertical" }}/>
                <div style={{ display:"flex", gap:6, flexWrap:"wrap", marginTop:6 }}>
                  {[
                    "Mitigated by network segmentation; service is internal-only.",
                    "Compensating control via WAF rule.",
                    "Vulnerable code path not reachable in this deployment.",
                    "False positive — upstream backport already applied.",
                  ].map(p => (
                    <button key={p} className="focus-ring" onClick={() => setJustification(p)}
                      style={{ all:"unset", cursor:"pointer", fontSize:10, padding:"3px 8px", borderRadius:99, background:"var(--cf-subtle-bg)", color:"var(--cf-text-secondary)", border:"1px solid var(--cf-divider)" }}>
                      {p.length > 42 ? p.slice(0, 40) + "…" : p}
                    </button>
                  ))}
                </div>
                {acceptNeedsText && justification.length > 0 && <div className="help" style={{ color:"#fbbf24" }}>Add a bit more detail (min 10 chars).</div>}
              </div>
              <div className="field" style={{ marginTop:0, maxWidth:240 }}>
                <label>Review date <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· optional</span></label>
                <input type="date" className="input focus-ring" value={reviewDate} onChange={e=>setReviewDate(e.target.value)}/>
                <div className="help">An acceptance with no review date is what assessors flag most often.</div>
              </div>
            </div>
          )}
        </div>

        <div className="modal-foot">
          <div style={{ marginRight:"auto", fontSize:11.5, color:"var(--cf-text-muted)" }}>
            {submissionBlocked || (!touched ? "Nothing dispositioned yet"
              : revoking ? `${hostScope.hostname} reverts to the ${hostScope.environment} decision`
              : [scheduledEnvs.length ? (seedScheduled ? "updates 1 POA&M" : "creates 1 POA&M") : null,
                 acceptedEnvs.length ? `1 waiver · ${envWide ? "all hosts" : `${acceptedHosts.length} host${acceptedHosts.length === 1 ? "" : "s"}`}` : null,
                 !hostScoped && openEnvs.length ? `${openEnvs.length} env${openEnvs.length === 1 ? "" : "s"} left open` : null,
                 ].filter(Boolean).join(" · "))}
          </div>
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" disabled={!canSubmit}
            style={!canSubmit ? { opacity:0.5, cursor:"not-allowed" } : null} onClick={submit}>
            <Icon name="check" size={13}/> Apply triage
          </button>
        </div>
      </div>
    </div>
  );
}

/* ── Export: same four formats as the POA&M register. OSCAL here is an
   assessment-results document: one observation per CVE, one finding per affected
   host, with the per-environment disposition carried as props. */
function cveExportName(scope, ext) {
  return `cves-${new Date().toISOString().slice(0,10)}${scope ? "-" + String(scope).replace(/\W+/g, "-") : ""}.${ext}`;
}
function cveDisposition(c, env) {
  const d = c.dispositions && c.dispositions[env];
  if (d) return d.state === "accepted" ? "risk accepted" : "patch scheduled" + (d.poamId ? " (" + d.poamId + ")" : "");
  if (!c.dispositions && c.acceptance && c.acceptance !== "outstanding" && (!c.scopeEnvs || c.scopeEnvs.includes(env))) return c.acceptance;
  return "outstanding";
}
function cveCsvRows(list, scope) {
  const rows = [["CVE","Severity","CVSS","Vector","Package","Introduced in","Fixed in","Exploited","Title","Environment","Systems","Disposition","Justification / plan","Discovered","Age (days)","Advisory"]];
  list.forEach(c => {
    const sys = SYSTEMS.filter(s => c.affected.includes(s.id) && (!scope || s.environment === scope));
    const envs = [...new Set(sys.map(s => s.environment))];
    (envs.length ? envs : [""]).forEach(env => {
      const d = c.dispositions && c.dispositions[env];
      rows.push([c.id, c.severity, c.cvss.toFixed(1), c.vector || "", c.pkg, c.introducedIn || "", c.fix === "available" ? c.fixedIn || "available" : "none",
        c.exploited ? "yes" : "no", c.title, env, sys.filter(s => s.environment === env).map(s => s.hostname).join("; "),
        env ? cveDisposition(c, env) : "outstanding", d ? (d.justification || d.plan || "") : (c.justification || ""),
        c.discoveredAt || "", c.ageDays, c.advisoryUrl || ""]);
    });
  });
  return rows;
}
function cveBuildOscal(list, scope) {
  const now = new Date().toISOString();
  const observations = [], findings = [];
  list.forEach(c => {
    const ob = rrUuid("cve-obs::" + c.id);
    observations.push({ uuid: ob, title: c.id, description: c.title, methods: ["TEST"], types: ["finding"], collected: now,
      props: [{ name:"vulnerability-id", value:c.id }, { name:"package", value:c.pkg }, { name:"cvss-score", value:c.cvss.toFixed(1) },
        { name:"cvss-vector", value:c.vector || "none" }, { name:"severity", value:c.severity }, { name:"fixed-in", value:c.fix === "available" ? c.fixedIn || "available" : "none" },
        ...(c.exploited ? [{ name:"known-exploited", value:"true" }] : [])],
      links: c.advisoryUrl ? [{ href:c.advisoryUrl, rel:"reference" }] : [] });
    SYSTEMS.filter(s => c.affected.includes(s.id) && (!scope || s.environment === scope)).forEach(s => {
      findings.push({ uuid: rrUuid("cve-finding::" + c.id + "::" + s.id), title: `${c.id} on ${s.hostname}`, description: c.title,
        props: [{ name:"host", value:s.hostname }, { name:"environment", value:s.environment }, { name:"disposition", value:cveDisposition(c, s.environment) }],
        target: { type:"objective-id", "target-id": c.id, status: { state: cveDisposition(c, s.environment) === "risk accepted" ? "satisfied" : "not-satisfied" } },
        "related-observations": [{ "observation-uuid": ob }] });
    });
  });
  return { "assessment-results": {
    uuid: rrUuid("cve::export::" + now),
    metadata: { title: `Vulnerability assessment results${scope ? " — " + scope : ""}`, "last-modified": now, version: now.slice(0,10), "oscal-version": "1.1.2" },
    "import-ap": { href: "#crystal-forge-scan-policy" },
    results: [{ uuid: rrUuid("cve-result::" + now), title: "Fleet vulnerability scan", start: now, observations, findings }],
  } };
}
function cveRunExport(fmt, list, scope) {
  if (fmt === "csv" || fmt === "excel") {
    const rows = cveCsvRows(list, scope);
    if (fmt === "csv") return rrDownload(cveExportName(scope, "csv"), rows.map(r => r.map(v => `"${String(v ?? "").replace(/"/g, '""')}"`).join(",")).join("\n"), "text/csv");
    const esc = (v) => String(v ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    const body = rows.map((r, i) => `<tr>${r.map(v => `<${i ? "td" : "th"}>${esc(v)}</${i ? "td" : "th"}>`).join("")}</tr>`).join("");
    return rrDownload(cveExportName(scope, "xls"), `<html><head><meta charset="utf-8"><!--[if gte mso 9]><xml><x:ExcelWorkbook><x:ExcelWorksheets><x:ExcelWorksheet><x:Name>CVEs</x:Name><x:WorksheetOptions><x:DisplayGridlines/></x:WorksheetOptions></x:ExcelWorksheet></x:ExcelWorksheets></x:ExcelWorkbook></xml><![endif]--></head><body><table border="1">${body}</table></body></html>`, "application/vnd.ms-excel");
  }
  const doc = cveBuildOscal(list, scope);
  if (fmt === "oscal-json") return rrDownload(cveExportName(scope, "oscal.json"), JSON.stringify(doc, null, 2), "application/json");
  rrDownload(cveExportName(scope, "oscal.xml"), `<?xml version="1.0" encoding="UTF-8"?>\n<oscal-ar xmlns="https://crystalforge.dev/ns/oscal">\n${rrOscalToXml(doc["assessment-results"], "assessment-results", 1)}</oscal-ar>\n`, "application/xml");
}
const CVE_EXPORT_FORMATS = [
  ["oscal-json", "OSCAL JSON", "Standard machine-readable assessment results"],
  ["excel",      "Excel",      "Spreadsheet for review and external workflows"],
  ["csv",        "CSV",        "Flat data for import into other tools"],
  ["oscal-xml",  "OSCAL XML",  "Standards-compatible XML"],
];
function CveExportMenuButton({ list, scope }) {
  const [open, setOpen] = React.useState(false);
  const ref = React.useRef(null);
  React.useEffect(() => {
    if (!open) return;
    const off = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    const esc = (e) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", off); document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", off); document.removeEventListener("keydown", esc); };
  }, [open]);
  return (
    <div className="rr-export" ref={ref}>
      <button type="button" className="btn btn-ghost focus-ring" aria-expanded={open} onClick={() => setOpen(o => !o)}>
        <Icon name="download" size={14}/> Export {list.length} <Icon name="chevron-down" size={11}/>
      </button>
      {open && (
        <div className="rr-export-pop card" role="menu" aria-label="Export CVEs">
          <div className="rr-export-title">Export CVEs{scope ? " · " + scope : ""}</div>
          {CVE_EXPORT_FORMATS.map(([k, l, sub]) => (
            <button key={k} type="button" role="menuitem" className="rr-export-item focus-ring" onClick={() => { cveRunExport(k, list, scope); setOpen(false); }}>
              <span className="rr-export-item-l">{l}</span>
              <span className="rr-export-item-sub">{sub}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

Object.assign(window, { CvesView, CveTriageModal, CVE_CHOICES });
