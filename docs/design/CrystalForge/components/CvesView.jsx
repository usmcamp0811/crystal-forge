// CVE view — fleet-wide vulnerabilities

function CvesView({ onOpenSystem }) {
  const [query, setQuery] = React.useState("");
  const [sevFilter, setSevFilter] = React.useState("all");
  const [fixFilter, setFixFilter] = React.useState("all");
  const [acceptFilter, setAcceptFilter] = React.useState("all");
  const [pkgFilter, setPkgFilter] = React.useState("all");
  const [sort, setSort] = React.useState("severity");
  const [groupMode, setGroupMode] = React.useState("package"); // 'package' | 'flat'
  const [expandedPkg, setExpandedPkg] = React.useState(null);
  const [selectedCve, setSelectedCve] = React.useState(null);
  const flashCrit = useAttentionFlash("cves", (CVE_STATS.critical || 0) > 0);

  const packages = React.useMemo(() => [...new Set(CVES.map((c) => c.pkg))], []);

  let filtered = CVES.filter((c) => {
    if (sevFilter !== "all" && c.severity !== sevFilter) return false;
    if (fixFilter === "available" && c.fix !== "available") return false;
    if (fixFilter === "pending" && c.fix !== "pending") return false;
    if (fixFilter === "exploited" && !c.exploited) return false;
    if (acceptFilter !== "all" && c.acceptance !== acceptFilter) return false;
    if (pkgFilter !== "all" && c.pkg !== pkgFilter) return false;
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
          <button className="btn btn-ghost focus-ring"><Icon name="download" size={14} /> Export report</button>
        </div>
      </div>

      <div className="stat-strip">
        <div className={`stat${flashCrit ? " attention-flash" : ""}`}>
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
        <span className="filter-count" style={{ marginLeft: "auto", marginRight: 0 }}>Group</span>
        <div className="seg">
          <button className={groupMode === "package" ? "active" : ""} onClick={() => setGroupMode("package")}>By package</button>
          <button className={groupMode === "flat" ? "active" : ""} onClick={() => setGroupMode("flat")}>Flat list</button>
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

      {groupMode === "package" ?
      <CvePackageGroups
        cves={filtered}
        expanded={expandedPkg}
        onToggle={(p) => setExpandedPkg(expandedPkg === p ? null : p)}
        onSelectCve={setSelectedCve} /> :


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
            {filtered.map((cve) => <CveRow key={cve.id} cve={cve} onOpen={() => setSelectedCve(cve)} />)}
            {filtered.length === 0 &&
            <tr><td colSpan={10} style={{ padding: 24, textAlign: "center", color: "var(--cf-text-muted)", fontSize: 13 }}>No CVEs match the current filters.</td></tr>
            }
          </tbody>
        </table>
      </div>
      }

      {selectedCve && <CveDrawer cve={selectedCve} onClose={() => setSelectedCve(null)} onOpenSystem={onOpenSystem} />}
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

// dead old stub from prior version below, will be removed by next edit chain
function _removeMe() {}

function CvePackageGroups({ cves, expanded, onToggle, onSelectCve }) {
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
      return { pkg, list, counts, systemsCount: systems.size, fixable, outstanding, exploited, maxCvss, score };
    }).sort((a, b) => b.score - a.score);
  }, [cves]);

  if (groups.length === 0) {
    return <div className="empty" style={{ margin: 0 }}><h3>No CVEs match</h3><div>Try clearing a filter.</div></div>;
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      {groups.map((g) =>
      <CvePackageGroup
        key={g.pkg}
        group={g}
        isExpanded={expanded === g.pkg}
        onToggle={() => onToggle(g.pkg)}
        onSelectCve={onSelectCve} />

      )}
    </div>);

}

function CvePackageGroup({ group, isExpanded, onToggle, onSelectCve }) {
  const sevColor = group.counts.critical > 0 ? "#f87171" :
  group.counts.high > 0 ? "#fbbf24" :
  group.counts.medium > 0 ? "#60a5fa" : "#9ca3af";
  return (
    <div className="card" style={{ overflow: "hidden" }}>
      <button className="focus-ring" onClick={onToggle}
      style={{
        all: "unset", display: "grid",
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
              {group.list.map((cve) => <CveRow key={cve.id} cve={cve} onOpen={() => onSelectCve(cve)} />)}
            </tbody>
          </table>
        </div>
      }
    </div>);

}

function CveRow({ cve, onOpen }) {
  const sevCls = { critical: "chip-critical", high: "chip-warning", medium: "chip-info", low: "chip-unknown" }[cve.severity];
  const sevColor = { critical: "#f87171", high: "#fbbf24", medium: "#60a5fa", low: "#9ca3af" }[cve.severity];
  return (
    <tr style={{ cursor: "pointer" }} onClick={onOpen}>
      <td>
        <div className="mono" style={{ fontWeight: 600, fontSize: 13, display: "flex", alignItems: "center", gap: 8 }}>
          {cve.id}
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

  // Local acceptance state (mock — in real app persists to backend)
  const [acceptance, setAcceptance] = React.useState({
    state: cve.acceptance,
    justification: cve.justification,
    by: cve.justifiedBy,
    at: cve.justifiedAt,
    scopeEnvs: cve.scopeEnvs || null, // null = all affected envs
  });
  const [showAccept, setShowAccept] = React.useState(false);

  React.useEffect(() => {
    const onKey = (e) => {if (e.key === "Escape") { showAccept ? setShowAccept(false) : onClose(); }};
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, showAccept]);

  const allAffectedEnvs = [...new Set(affectedSystems.map(s => s.environment))];

  const applyAcceptance = (payload) => {
    const isPartial = payload.scopeEnvs && payload.scopeEnvs.length < allAffectedEnvs.length;
    cve.acceptance = isPartial ? "partial" : payload.state;
    cve.justification = payload.justification;
    cve.justifiedBy = payload.by;
    cve.justifiedAt = payload.at;
    cve.scopeEnvs = payload.scopeEnvs;
    setAcceptance({ ...payload, state: payload.state });
    setShowAccept(false);
  };
  const revoke = () => {
    cve.acceptance = "outstanding";
    cve.justification = null;
    cve.justifiedBy = null;
    cve.justifiedAt = null;
    cve.scopeEnvs = null;
    setAcceptance({ state: "outstanding", justification: null, by: null, at: null, scopeEnvs: null });
  };

  const coveredEnvs = acceptance.scopeEnvs && acceptance.scopeEnvs.length ? acceptance.scopeEnvs : allAffectedEnvs;
  const coveredCount = affectedSystems.filter(s => coveredEnvs.includes(s.environment)).length;
  const isPartialScope = coveredEnvs.length < allAffectedEnvs.length;

  // Group affected by environment
  const byEnv = {};
  affectedSystems.forEach((s) => {
    byEnv[s.environment] = byEnv[s.environment] || [];
    byEnv[s.environment].push(s);
  });

  return (
    <>
      <div className="fl-tray-backdrop" onClick={onClose} />
      <aside className="fl-tray" role="dialog" aria-label={cve.id}>
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
            {acceptance.state === "outstanding" ? (
              <button className="btn btn-primary focus-ring xs" onClick={() => setShowAccept(true)}>
                <Icon name="check" size={11} /> Accept risk
              </button>
            ) : (
              <button className="btn btn-ghost focus-ring xs" onClick={() => setShowAccept(true)}>
                <Icon name="file" size={11} /> Edit justification
              </button>
            )}
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

          {/* Triage / acceptance */}
          <section>
            <h3 style={{ fontSize: 11, textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--cf-text-muted)", margin: "0 0 10px", fontWeight: 600 }}>Triage status</h3>
            {showAccept ? (
              <CveAcceptForm
                cve={cve}
                affectedSystems={affectedSystems}
                initial={acceptance}
                onCancel={() => setShowAccept(false)}
                onSubmit={applyAcceptance} />
            ) : acceptance.state === "outstanding" ? (
              <div className="sd-callout sd-callout-warn">
                <Icon name="warn" size={13} />
                <div style={{ fontSize: 12 }}>
                  <strong>Outstanding — needs triage.</strong> Patch the affected systems, or accept the risk with a justification. You can scope it to all environments or only specific ones (e.g. accept in dev, keep open in prod).
                </div>
              </div>
            ) : (
              <div style={{ padding: 14, borderRadius: 10, border: "1px solid", borderColor: acceptance.state === "accepted" ? "rgba(167,139,250,0.3)" : "rgba(96,165,250,0.3)", background: acceptance.state === "accepted" ? "rgba(167,139,250,0.07)" : "rgba(96,165,250,0.07)" }}>
                <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 10, marginBottom: 8 }}>
                  <span className="chip chip-info" style={{ background: acceptance.state === "accepted" ? "rgba(167,139,250,0.18)" : undefined, color: acceptance.state === "accepted" ? "#a78bfa" : undefined }}>
                    {acceptance.state === "accepted" ? "Risk accepted" : "Patch scheduled"}
                  </span>
                  <span style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>
                    covers {coveredCount} of {affectedSystems.length} system{affectedSystems.length === 1 ? "" : "s"}
                  </span>
                </div>

                {/* Scope chips */}
                <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginBottom: 10 }}>
                  {allAffectedEnvs.map(env => {
                    const covered = coveredEnvs.includes(env);
                    return covered
                      ? <EnvBadge key={env} env={env} />
                      : <span key={env} className="chip chip-critical" style={{ fontSize: 10 }} title="Still outstanding in this environment">{env} · open</span>;
                  })}
                </div>

                <div style={{ fontSize: 13, color: "var(--cf-text-primary)", lineHeight: 1.5 }}>{acceptance.justification}</div>
                <div style={{ fontSize: 11, color: "var(--cf-text-muted)", marginTop: 8, display: "flex", gap: 8, alignItems: "center" }}>
                  <Icon name="user" size={11} />
                  <span>by <span className="mono">{acceptance.by || "—"}</span></span>
                  {acceptance.at && <span>· {acceptance.at}</span>}
                  <button className="btn btn-ghost focus-ring xs" style={{ marginLeft: "auto" }} onClick={() => setShowAccept(true)}>
                    <Icon name="file" size={10} /> Edit
                  </button>
                  <button className="btn btn-ghost focus-ring xs" onClick={revoke}>
                    <Icon name="x" size={10} /> Revoke
                  </button>
                </div>
                {isPartialScope && (
                  <div className="help" style={{ marginTop: 8, color: "#fbbf24" }}>
                    <Icon name="warn" size={10} style={{ verticalAlign: "middle" }} /> {affectedSystems.length - coveredCount} system{affectedSystems.length - coveredCount === 1 ? "" : "s"} in other environments remain outstanding.
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
    </>);

}

function CveAcceptForm({ cve, affectedSystems, initial, onCancel, onSubmit }) {
  const [state, setState] = React.useState(initial.state === "outstanding" ? "accepted" : initial.state);
  const [justification, setJustification] = React.useState(initial.justification || "");
  const [expiry, setExpiry] = React.useState("");

  // Environments present among affected systems
  const envCounts = React.useMemo(() => {
    const m = {};
    affectedSystems.forEach(s => { m[s.environment] = (m[s.environment] || 0) + 1; });
    return m;
  }, [affectedSystems]);
  const allEnvs = Object.keys(envCounts);

  const [scopeMode, setScopeMode] = React.useState(initial.scopeEnvs && initial.scopeEnvs.length && initial.scopeEnvs.length < allEnvs.length ? "some" : "all");
  const [scopeEnvs, setScopeEnvs] = React.useState(initial.scopeEnvs && initial.scopeEnvs.length ? initial.scopeEnvs : allEnvs);

  const presets = [
    "Mitigated by network segmentation; service is internal-only.",
    "Compensating control via WAF rule.",
    "Vulnerable code path not reachable in this deployment.",
    "Acceptable in non-production; tracked for prod patch.",
    "False positive — upstream backport already applied.",
  ];
  const effectiveEnvs = scopeMode === "all" ? allEnvs : scopeEnvs;
  const coveredCount = affectedSystems.filter(s => effectiveEnvs.includes(s.environment)).length;
  const canSubmit = justification.trim().length >= 10 && effectiveEnvs.length > 0;

  const toggleEnv = (env) => setScopeEnvs(prev => prev.includes(env) ? prev.filter(e => e !== env) : [...prev, env]);

  return (
    <div style={{ padding: 14, borderRadius: 10, border: "1px solid var(--cf-card-border)", background: "var(--cf-card-bg)", display: "flex", flexDirection: "column", gap: 12 }}>

      {/* Scope */}
      <div className="field">
        <label>Apply to</label>
        <div className="seg" style={{ width: "fit-content" }}>
          <button className={scopeMode === "all" ? "active" : ""} onClick={() => setScopeMode("all")}>All environments</button>
          <button className={scopeMode === "some" ? "active" : ""} onClick={() => setScopeMode("some")}>Specific environments</button>
        </div>
        {scopeMode === "some" && (
          <div style={{ display: "flex", flexWrap: "wrap", gap: 6, marginTop: 10 }}>
            {allEnvs.map(env => {
              const on = scopeEnvs.includes(env);
              const envColor = (ENV_STYLE[env] && ENV_STYLE[env].fg) || "#9ca3af";
              return (
                <button key={env} className="focus-ring" onClick={() => toggleEnv(env)}
                  style={{
                    padding: "4px 10px", borderRadius: 99, fontSize: 11, cursor: "pointer", fontFamily: "inherit",
                    border: `1px solid ${on ? envColor : "var(--cf-card-border)"}`,
                    background: on ? `color-mix(in oklab, ${envColor} 16%, var(--cf-card-bg))` : "transparent",
                    color: on ? envColor : "var(--cf-text-secondary)",
                    display: "inline-flex", alignItems: "center", gap: 6,
                  }}>
                  <span style={{ width: 6, height: 6, borderRadius: "50%", background: envColor }} />
                  {env}
                  <span className="mono" style={{ fontSize: 10, opacity: 0.7 }}>{envCounts[env]}</span>
                </button>
              );
            })}
          </div>
        )}
        <div className="help" style={{ marginTop: 6 }}>
          Covers <strong style={{ color: "var(--cf-text-primary)" }}>{coveredCount}</strong> of {affectedSystems.length} affected system{affectedSystems.length === 1 ? "" : "s"}
          {scopeMode === "some" && coveredCount < affectedSystems.length && <> · {affectedSystems.length - coveredCount} remain outstanding</>}.
        </div>
      </div>

      <div className="field">
        <label>Disposition</label>
        <div className="seg" style={{ width: "fit-content" }}>
          <button className={state === "accepted" ? "active" : ""} onClick={() => setState("accepted")}>Accept risk</button>
          <button className={state === "scheduled" ? "active" : ""} onClick={() => setState("scheduled")}>Schedule patch</button>
        </div>
      </div>

      <div className="field">
        <label>Justification</label>
        <textarea className="input focus-ring" rows={3} value={justification} onChange={(e) => setJustification(e.target.value)}
          placeholder="Why is this acceptable / what is the compensating control?" style={{ resize: "vertical" }} />
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginTop: 6 }}>
          {presets.map((p) => (
            <button key={p} className="focus-ring" onClick={() => setJustification(p)}
              style={{ all: "unset", cursor: "pointer", fontSize: 10, padding: "3px 8px", borderRadius: 99, background: "var(--cf-subtle-bg)", color: "var(--cf-text-secondary)", border: "1px solid var(--cf-divider)" }}>
              {p.length > 42 ? p.slice(0, 40) + "…" : p}
            </button>
          ))}
        </div>
        {!canSubmit && justification.length > 0 && justification.trim().length < 10 && <div className="help" style={{ color: "#fbbf24" }}>Add a bit more detail (min 10 chars).</div>}
      </div>

      <div className="field" style={{ maxWidth: 220 }}>
        <label>{state === "scheduled" ? "Target patch date" : "Review / expiry date (optional)"}</label>
        <input type="date" className="input focus-ring" value={expiry} onChange={(e) => setExpiry(e.target.value)} />
      </div>

      <div className="sd-callout sd-callout-info" style={{ fontSize: 11 }}>
        <Icon name="check" size={12} />
        <div>Recorded against your account and attached to each covered system's compliance evidence trail.</div>
      </div>

      <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
        <button className="btn btn-ghost focus-ring" onClick={onCancel}>Cancel</button>
        <button className="btn btn-primary focus-ring" disabled={!canSubmit}
          style={!canSubmit ? { opacity: 0.5, cursor: "not-allowed" } : null}
          onClick={() => onSubmit({ state, justification: justification.trim(), by: "mreyes", at: "just now", expiry, scopeEnvs: effectiveEnvs, scopeMode })}>
          <Icon name="check" size={13} /> {state === "accepted" ? `Accept for ${coveredCount} system${coveredCount === 1 ? "" : "s"}` : `Schedule for ${coveredCount}`}
        </button>
      </div>
    </div>
  );
}

Object.assign(window, { CvesView });