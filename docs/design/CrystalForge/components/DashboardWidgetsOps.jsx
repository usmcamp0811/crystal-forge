// Fleet-ops dashboard widgets — drift, disk/closure, rollback, cache hit rate,
// deploy history, secret expiry, reboot state. Registered into the dashboard's
// renderer table so DashboardView doesn't need a case per widget.
//
// Host-scoped widgets take a `scope` of "all" | "env:<name>" set per-widget in
// Customize, so the same widget can be added twice at different scopes (one per
// environment, say).

function opsScopeFilter(scope) {
  if (!scope || scope === "all") return () => true;
  if (scope.startsWith("env:")) { const env = scope.slice(4); return (sys) => sys.environment === env; }
  return () => true;
}
function opsScopeLabel(scope) {
  if (!scope || scope === "all") return null;
  if (scope.startsWith("env:")) return scope.slice(4);
  return null;
}
function OpsScopeChip({ scope }) {
  const label = opsScopeLabel(scope);
  if (!label) return null;
  return <span className="chip chip-info" style={{ fontSize:9.5, marginLeft:6, flexShrink:0 }}>{label}</span>;
}

function WFleetDrift({ onNavigate, rows, scope }) {
  const all = fleetDriftData();
  const keep = opsScopeFilter(scope);
  const inScope = all.rows.filter(r => keep(r.sys));
  const d = {
    behind: inScope.filter(r => r.behind > 0).sort((a, b) => b.behind - a.behind),
    onHead: inScope.filter(r => r.behind === 0).length,
    unknown: inScope.filter(r => r.unknown).length,
  };
  const shown = d.behind.slice(0, HEIGHT_COUNTS[rows || 1] || 4);
  // sqrt scale: one host 40 commits behind shouldn't flatten every other bar
  // into an illegible sliver. The −N label carries the exact figure.
  const max = Math.sqrt(d.behind[0]?.behind || 1);
  return (
    <>
      <WidgetHeader icon="git" title={<>Configuration Drift<OpsScopeChip scope={scope}/></>} action="Systems →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, lineHeight:1, fontVariantNumeric:"tabular-nums", color: d.behind.length ? "#fbbf24" : "#34d399" }}>{d.behind.length}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>behind HEAD · {d.onHead} current</span>
        </div>
        <div style={{ display:"flex", flexDirection:"column", gap:5 }}>
          {shown.map(({ sys, behind, at }) => (
            <div key={sys.id} className="ops-row" onClick={() => onNavigate("systems")}>
              <span className="status-dot" style={{ "--status-color": sys.statusColor }}/>
              <span className="mono truncate" style={{ flex:1, minWidth:0, fontWeight:600, fontSize:11.5 }}>{sys.hostname}</span>
              <span className="ops-bar" title={`${behind} commits behind ${sys.flake}`}>
                <span style={{ width:`${Math.max(9, (Math.sqrt(behind) / max) * 100)}%`, background: behind > 8 ? "#f87171" : "#fbbf24" }}/>
              </span>
              <span className="mono" style={{ fontSize:11, fontWeight:700, width:34, textAlign:"right", flexShrink:0, color: behind > 8 ? "#f87171" : "#fbbf24" }}>−{behind}</span>
              <span className="mono" style={{ fontSize:10, color:"var(--cf-text-muted)", width:58, textAlign:"right", flexShrink:0 }} title={at?.msg}>{at?.sha || "—"}</span>
            </div>
          ))}
          {shown.length === 0 && <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>{d.onHead + d.unknown === 0 ? "No systems in scope." : "Every host in scope is on its flake's HEAD."}</div>}
        </div>
        {d.unknown > 0 && (
          <div style={{ fontSize:10.5, color:"var(--cf-text-muted)" }}>{d.unknown} host{d.unknown===1?"":"s"} on a flake with no tracked commits</div>
        )}
      </div>
    </>
  );
}

function WClosurePressure({ onNavigate, rows, scope }) {
  const keep = opsScopeFilter(scope);
  const all = closurePressureData().rows.filter(r => keep(r.sys));
  const d = { rows: all, critical: all.filter(r => r.level === "critical").length, warning: all.filter(r => r.level === "warning").length };
  const shown = d.rows.slice(0, HEIGHT_COUNTS[rows || 1] || 4);
  return (
    <>
      <WidgetHeader icon="cube" title={<>Closure & Disk Pressure<OpsScopeChip scope={scope}/></>} action="Systems →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, lineHeight:1, fontVariantNumeric:"tabular-nums", color: d.critical ? "#f87171" : d.warning ? "#fbbf24" : "#34d399" }}>{d.critical + d.warning}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>hosts low on space</span>
        </div>
        {d.critical > 0 && (
          <div style={{ padding:"8px 10px", borderRadius:6, background:"rgba(248,113,113,0.08)", border:"1px solid rgba(248,113,113,0.25)", fontSize:11, color:"#fca5a5" }}>
            {d.critical} under 8% free — activation will fail before it starts
          </div>
        )}
        <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
          {shown.map(r => {
            const color = r.level === "critical" ? "#f87171" : r.level === "warning" ? "#fbbf24" : "#34d399";
            return (
              <div key={r.sys.id} style={{ display:"flex", flexDirection:"column", gap:3, cursor:"pointer" }} onClick={() => onNavigate("systems")}>
                <div style={{ display:"flex", alignItems:"baseline", gap:8, fontSize:11.5 }}>
                  <span className="mono truncate" style={{ fontWeight:600, flex:1, minWidth:0 }}>{r.sys.hostname}</span>
                  <span className="mono" style={{ fontSize:10, color:"var(--cf-text-muted)" }}>{r.closure} GB closure · {r.gens} gens</span>
                  <span className="mono" style={{ fontSize:11, fontWeight:700, color, width:38, textAlign:"right" }}>{r.freePct}%</span>
                </div>
                <div style={{ height:4, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden", display:"flex" }}>
                  <div style={{ width:`${(r.storeUsed / r.diskTotal) * 100}%`, background:color }}/>
                  <div style={{ width:`${((r.used - r.storeUsed) / r.diskTotal) * 100}%`, background:"var(--cf-divider)" }}/>
                </div>
              </div>
            );
          })}
        </div>
        <div style={{ fontSize:10, color:"var(--cf-text-muted)" }}>Solid bar is /nix/store, grey is everything else.</div>
      </div>
    </>
  );
}

function WRollbackReadiness({ onNavigate, scope, rows }) {
  const keep = opsScopeFilter(scope);
  const rowsIn = rollbackReadinessData().rows.filter(r => keep(r.sys));
  const d = { rows: rowsIn, ready: rowsIn.filter(r => r.ready).length, blocked: rowsIn.filter(r => !r.ready) };
  const total = Math.max(1, d.rows.length);
  return (
    <>
      <WidgetHeader icon="rollback" title={<>Rollback Readiness<OpsScopeChip scope={scope}/></>} action="Systems →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, lineHeight:1, fontVariantNumeric:"tabular-nums", color: d.blocked.length ? "#f87171" : "#34d399" }}>{d.blocked.length}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>cannot roll back</span>
        </div>
        <div style={{ display:"flex", height:8, borderRadius:99, overflow:"hidden", background:"var(--cf-subtle-bg)" }}>
          <div style={{ width:`${(d.ready / total) * 100}%`, background:"#34d399" }}/>
          <div style={{ width:`${(d.blocked.length / total) * 100}%`, background:"#f87171" }}/>
        </div>
        <div style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{d.ready} of {total} keep a previous generation on disk</div>
        {d.blocked.slice(0, HEIGHT_COUNTS[rows || 1] || 4).map(r => (
          <div key={r.sys.id} className="ops-row" onClick={() => onNavigate("systems")}>
            <Icon name="warn" size={11} style={{ color:"#f87171", flexShrink:0 }}/>
            <span className="mono truncate" style={{ flex:1, minWidth:0, fontSize:11.5 }}>{r.sys.hostname}</span>
            <span style={{ fontSize:10, color:"var(--cf-text-muted)", flexShrink:0 }}>GC {r.lastGc}</span>
          </div>
        ))}
      </div>
    </>
  );
}

function WCacheHitRate({ onNavigate, rows }) {
  const list = cacheHitTrendData();
  return (
    <>
      <WidgetHeader icon="download" title="Cache Hit Rate" action="Caches →" onAction={() => onNavigate("caches")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:9 }}>
        {list.slice(0, HEIGHT_COUNTS[rows || 1] || 4).map(({ cache, series, now, delta }) => {
          const color = now < 55 ? "#f87171" : now < 78 ? "#fbbf24" : "#34d399";
          const W = 96, H = 22, min = Math.min(...series), max = Math.max(...series), span = Math.max(1, max - min);
          const pts = series.map((v, i) => `${(i / (series.length - 1)) * W},${H - ((v - min) / span) * (H - 3) - 1.5}`).join(" ");
          return (
            <div key={cache.id} style={{ display:"flex", alignItems:"center", gap:10, cursor:"pointer" }} onClick={() => onNavigate("caches")}>
              <span className="mono truncate" style={{ flex:1, minWidth:0, fontSize:11.5 }}>{cache.name}</span>
              <svg width={W} height={H} style={{ flexShrink:0, overflow:"visible" }}>
                <polyline points={pts} fill="none" stroke={color} strokeWidth="1.5" strokeLinejoin="round"/>
              </svg>
              <span className="mono" style={{ fontSize:12, fontWeight:700, color, width:36, textAlign:"right", flexShrink:0 }}>{now}%</span>
              <span className="mono" style={{ fontSize:10, width:40, textAlign:"right", flexShrink:0, color: delta < -2 ? "#f87171" : delta > 2 ? "#34d399" : "var(--cf-text-muted)" }}>
                {delta > 0 ? "+" : ""}{delta}
              </span>
            </div>
          );
        })}
        <div style={{ fontSize:10, color:"var(--cf-text-muted)" }}>Share of needed store paths substituted rather than built · last 24h, hourly · delta vs. 6h ago</div>
      </div>
    </>
  );
}

function WDeployHeatmap({ onNavigate, scope, rows }) {
  const keep = opsScopeFilter(scope);
  const base = deployStateData();
  const rowsIn = base.rows.filter(r => keep(r.sys));
  const shownRows = rowsIn.slice(0, HEIGHT_COUNTS[rows || 1] || 4);
  const hidden = rowsIn.length - shownRows.length;
  const inSync = rowsIn.filter(r => r.behind === 0).length;
  const drifting = rowsIn.filter(r => r.behind >= DRIFT_WARN && r.behind < DRIFT_ALERT).length;
  const stale = rowsIn.filter(r => r.behind >= DRIFT_ALERT).length;

  const cell = (day) => {
    if (day.behind === null) return "var(--cf-subtle-bg)";
    if (day.behind === 0) return "#34d399";
    if (day.behind >= DRIFT_ALERT) return "#f87171";
    // Deepen amber as it approaches the stale threshold.
    return `color-mix(in oklab, #f87171 ${Math.round((day.behind / DRIFT_ALERT) * 55)}%, #fbbf24)`;
  };
  const tip = (day, ago) => {
    const when = ago === 0 ? "today" : `${ago}d ago`;
    if (day.behind === null) return `${when} · drift unknown`;
    const state = day.behind === 0 ? "up to date" : day.behind >= DRIFT_ALERT ? "stale" : "drifting";
    return `${when} · ${day.behind === 0 ? "on HEAD" : `${day.behind} behind`} · ${state}${day.deployed ? " · deployed" : ""}`;
  };

  return (
    <>
      <WidgetHeader icon="grid" title={<>Deploy State<OpsScopeChip scope={scope}/></>} action="Systems →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:8 }}>
        <div style={{ display:"flex", alignItems:"baseline", gap:14, fontSize:12 }}>
          <span style={{ color:"var(--cf-text-muted)" }}>
            <strong style={{ color:"#34d399", fontSize:16, fontVariantNumeric:"tabular-nums" }}>{inSync}</strong> up to date
          </span>
          {drifting > 0 && <span style={{ color:"var(--cf-text-muted)" }}>
            <strong style={{ color:"#fbbf24", fontSize:16, fontVariantNumeric:"tabular-nums" }}>{drifting}</strong> drifting
          </span>}
          {stale > 0 && <span style={{ color:"var(--cf-text-muted)" }}>
            <strong style={{ color:"#f87171", fontSize:16, fontVariantNumeric:"tabular-nums" }}>{stale}</strong> stale
          </span>}
        </div>
        <div style={{ display:"flex", alignItems:"center", gap:8 }}>
          <span style={{ width:104, flexShrink:0, fontSize:9.5, textTransform:"uppercase", letterSpacing:"0.06em", color:"var(--cf-text-muted)" }}>Host</span>
          <div style={{ display:"flex", gap:3, flex:1, minWidth:0 }}>
            {Array.from({ length: base.days }, (_, i) => {
              const ago = base.days - 1 - i;
              return (
                <span key={i} className="mono" style={{ flex:1, minWidth:6, textAlign:"center", fontSize:9, color:"var(--cf-text-muted)", whiteSpace:"nowrap" }}>
                  {ago === 0 ? "today" : ago % 2 === 0 ? `${ago}d` : ""}
                </span>
              );
            })}
          </div>
          <span style={{ width:44, flexShrink:0, fontSize:9.5, textTransform:"uppercase", letterSpacing:"0.06em", textAlign:"right", color:"var(--cf-text-muted)" }}>Behind</span>
        </div>
        <div style={{ display:"flex", flexDirection:"column", gap:3 }}>
          {shownRows.map(r => (
            <div key={r.sys.id} style={{ display:"flex", alignItems:"center", gap:8, cursor:"pointer" }} onClick={() => onNavigate("systems")}>
              <span className="mono truncate" style={{ width:104, flexShrink:0, fontSize:11 }}>{r.sys.hostname}</span>
              <div style={{ display:"flex", gap:3, flex:1, minWidth:0 }}>
                {r.days.map((day, i) => (
                  <span key={i} title={tip(day, base.days - 1 - i)}
                    style={{ flex:1, height:14, borderRadius:3, background:cell(day), minWidth:6,
                      boxShadow: i === base.days - 1 ? "inset 0 0 0 1px var(--cf-text-muted)" : "none" }}/>
                ))}
              </div>
              <span className="mono" style={{ fontSize:10.5, width:44, textAlign:"right", flexShrink:0,
                color: r.behind === null ? "var(--cf-text-muted)" : r.behind >= DRIFT_ALERT ? "#f87171" : r.behind > 0 ? "#fbbf24" : "#34d399" }}>
                {r.behind === null ? "—" : r.behind === 0 ? "0" : `−${r.behind}`}
              </span>
            </div>
          ))}
          {shownRows.length === 0 && <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>No systems in scope.</div>}
        </div>
        <div style={{ display:"flex", alignItems:"center", gap:12, fontSize:10, color:"var(--cf-text-muted)" }}>
          <span style={{ display:"inline-flex", alignItems:"center", gap:4 }}><span style={{ width:9, height:9, borderRadius:2, background:"#34d399" }}/>on HEAD</span>
          <span style={{ display:"inline-flex", alignItems:"center", gap:4 }}><span style={{ width:9, height:9, borderRadius:2, background:"#fbbf24" }}/>1–{DRIFT_ALERT - 1} behind</span>
          <span style={{ display:"inline-flex", alignItems:"center", gap:4 }}><span style={{ width:9, height:9, borderRadius:2, background:"#f87171" }}/>{DRIFT_ALERT}+ behind</span>
          <span style={{ marginLeft:"auto" }}>{hidden > 0 ? `+${hidden} more hosts · ` : ""}one column per day</span>
        </div>
      </div>
    </>
  );
}

function WSecretExpiry({ onNavigate, scope, rows }) {
  const keep = opsScopeFilter(scope);
  const systems = typeof SYSTEMS !== "undefined" ? SYSTEMS : [];
  // Shared secrets have no host, so they only show at "all" scope.
  const list = secretExpiryData().filter(x => {
    if (!x.sysId) return !scope || scope === "all";
    const sys = systems.find(s => s.id === x.sysId);
    return sys ? keep(sys) : false;
  });
  const expired = list.filter(s => s.days < 0);
  const soon = list.filter(s => s.days >= 0 && s.days <= 30);
  return (
    <>
      <WidgetHeader icon="key" title={<>Key & Secret Expiry<OpsScopeChip scope={scope}/></>} action="Admin →" onAction={() => onNavigate("admin")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, lineHeight:1, fontVariantNumeric:"tabular-nums", color: expired.length ? "#f87171" : soon.length ? "#fbbf24" : "#34d399" }}>{expired.length + soon.length}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>expiring within 30d</span>
        </div>
        {expired.length > 0 && (
          <div style={{ padding:"8px 10px", borderRadius:6, background:"rgba(248,113,113,0.08)", border:"1px solid rgba(248,113,113,0.25)", fontSize:11, color:"#fca5a5" }}>
            {expired.length} already expired
          </div>
        )}
        <div style={{ display:"flex", flexDirection:"column", gap:5 }}>
          {list.slice(0, HEIGHT_COUNTS[rows || 1] || 4).map((s, i) => {
            const color = s.days < 0 ? "#f87171" : s.days <= 14 ? "#fbbf24" : "#60a5fa";
            return (
              <div key={i} className="ops-row" title={s.detail}>
                <span className="mono truncate" style={{ flex:1, minWidth:0, fontSize:11.5 }}>{s.scope}</span>
                <span className="chip chip-unknown" style={{ fontSize:9, flexShrink:0 }}>{s.kind}</span>
                <span className="mono" style={{ fontSize:11, fontWeight:700, color, width:52, textAlign:"right", flexShrink:0 }}>
                  {s.days < 0 ? `${-s.days}d over` : `${s.days}d`}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </>
  );
}

function WRebootRequired({ onNavigate, scope, rows }) {
  const keep = opsScopeFilter(scope);
  const rowsIn = rebootRequiredData().rows.filter(r => keep(r.sys));
  const d = { rows: rowsIn, kernel: rowsIn.filter(r => r.reason === "kernel").length };
  return (
    <>
      <WidgetHeader icon="power" title={<>Reboot Required<OpsScopeChip scope={scope}/></>} action="Systems →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline" }}>
          <span style={{ fontSize:32, fontWeight:700, lineHeight:1, fontVariantNumeric:"tabular-nums", color: d.rows.length ? "#60a5fa" : "#34d399" }}>{d.rows.length}</span>
          <span style={{ fontSize:12, color:"var(--cf-text-muted)" }}>awaiting reboot</span>
        </div>
        {d.kernel > 0 && (
          <div style={{ padding:"8px 10px", borderRadius:6, background:"rgba(96,165,250,0.08)", border:"1px solid rgba(96,165,250,0.25)", fontSize:11, color:"#93c5fd" }}>
            {d.kernel} running an older kernel than the activated generation
          </div>
        )}
        <div style={{ display:"flex", flexDirection:"column", gap:5 }}>
          {d.rows.slice(0, HEIGHT_COUNTS[rows || 1] || 4).map(r => (
            <div key={r.sys.id} className="ops-row" onClick={() => onNavigate("systems")} title={`activated: ${r.pending} · running: ${r.running}`}>
              <span className="mono truncate" style={{ flex:1, minWidth:0, fontSize:11.5 }}>{r.sys.hostname}</span>
              <span className="chip chip-unknown" style={{ fontSize:9, flexShrink:0 }}>{r.reason}</span>
              <span className="mono" style={{ fontSize:10.5, color:"var(--cf-text-muted)", width:34, textAlign:"right", flexShrink:0 }}>{r.waitingDays}d</span>
            </div>
          ))}
          {d.rows.length === 0 && <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>Every host in scope is running its activated generation.</div>}
        </div>
      </div>
    </>
  );
}

function WFleetCalendar({ onNavigate, scope, metric, rows }) {
  const keep = opsScopeFilter(scope);
  const { days, systemCount } = React.useMemo(() => fleetCalendarData(keep), [scope]);
  const m = metric || "combined";
  const [hov, setHov] = React.useState(null);
  const valueOf = (d) => m === "compliance" ? d.compliance : m === "drift" ? d.driftHealth : d.combined;

  // Health 0–100 → five steps, so a bad week reads at a glance instead of
  // needing a continuous scale the eye can't resolve.
  const cellColor = (d) => {
    const v = valueOf(d);
    if (v === null) return "var(--cf-subtle-bg)";
    if (v >= 88) return "#34d399";
    if (v >= 76) return "color-mix(in oklab, #34d399 62%, #fbbf24)";
    if (v >= 62) return "#fbbf24";
    if (v >= 48) return "color-mix(in oklab, #fbbf24 45%, #f87171)";
    return "#f87171";
  };
  const tip = (d) => {
    const date = d.date.toLocaleDateString(undefined, { month:"short", day:"numeric", year:"numeric" });
    const parts = [];
    if (m !== "drift") parts.push(`${d.compliance}% controls passing`);
    if (m !== "compliance") parts.push(d.behind === 0 ? "on HEAD" : `${d.behind} commits behind avg`);
    return `${date} · ${parts.join(" · ")}`;
  };

  // Column-major weeks, Sunday at the top — pad the first week so weekdays align.
  const weeks = [];
  let week = new Array(days[0].weekday).fill(null);
  days.forEach(d => {
    week.push(d);
    if (week.length === 7) { weeks.push(week); week = []; }
  });
  if (week.length) { while (week.length < 7) week.push(null); weeks.push(week); }

  // Cell size is driven by column WIDTH (53 columns of 1fr), so extra height can
  // only buy bigger cells by splitting the year into stacked bands — fewer weeks
  // per row means wider columns.
  const bandCount = Math.min(3, Math.max(1, rows || 1));
  const perBand = Math.ceil(weeks.length / bandCount);
  const bands = Array.from({ length: bandCount }, (_, i) => weeks.slice(i * perBand, (i + 1) * perBand))
    .filter(b => b.length);

  const withVal = days.filter(d => valueOf(d) !== null);
  const avg = withVal.length ? Math.round(withVal.reduce((a, d) => a + valueOf(d), 0) / withVal.length) : null;
  const bad = withVal.filter(d => valueOf(d) < 62).length;
  const last30 = withVal.slice(-30);
  const avg30 = last30.length ? Math.round(last30.reduce((a, d) => a + valueOf(d), 0) / last30.length) : null;

  return (
    <>
      <WidgetHeader icon="grid" title={<>{m === "compliance" ? "Compliance Year" : m === "drift" ? "Drift Year" : "Fleet Health Year"}<OpsScopeChip scope={scope}/></>} action="Systems →" onAction={() => onNavigate("systems")}/>
      <div className="dash-w-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
        <div className="fc-outer">
        <div className="fc-summary">
          <span style={{ color:"var(--cf-text-muted)" }}>
            <strong style={{ color:"var(--cf-text-primary)", fontSize:16, fontVariantNumeric:"tabular-nums" }}>{avg30}%</strong> last 30d
          </span>
          <span className="fc-sum-year" style={{ color:"var(--cf-text-muted)" }}>{avg}% year avg</span>
          {bad > 0 && <span style={{ color:"#fbbf24" }}>{bad} day{bad===1?"":"s"} below 62%</span>}
          <span className={hov ? "fc-sum-tail fc-hov" : "fc-sum-tail"}>
            {hov ? tip(hov) : <>{systemCount} system{systemCount===1?"":"s"} · {m === "compliance" ? "controls passing" : m === "drift" ? "drift from HEAD" : "worse of compliance & drift"}</>}
          </span>
        </div>

        {bands.map((band, bi) => (
          <div className="fc-wrap" key={bi} style={{ "--fc-weeks": band.length }}>
            <div className="fc-daylabels">
              {["", "Mon", "", "Wed", "", "Fri", ""].map((lbl, i) => <span key={i}>{lbl}</span>)}
            </div>
            <div className="fc-cols">
              <div className="fc-months" style={{ "--fc-weeks": band.length }}>
                {band.map((wk, i) => {
                  const first = wk.find(Boolean);
                  const prev = i > 0 ? band[i - 1].find(Boolean) : null;
                  const isNew = first && (!prev || prev.date.getMonth() !== first.date.getMonth());
                  return <span key={i}>{isNew && <b>{first.date.toLocaleDateString(undefined, { month:"short" })}</b>}</span>;
                })}
              </div>
              <div className="fc-grid" style={{ "--fc-weeks": band.length }} onMouseLeave={() => setHov(null)}>
                {band.map((wk, wi) => wk.map((d, di) => d
                  ? <span key={`${wi}-${di}`} className="fc-cell" title={tip(d)} onMouseEnter={() => setHov(d)} style={{ background:cellColor(d) }}/>
                  : <span key={`${wi}-${di}`} className="fc-cell fc-cell-empty"/>
                ))}
              </div>
            </div>
          </div>
        ))}

        <div className="fc-legend">
          <span>worse</span>
          {["#f87171", "color-mix(in oklab, #fbbf24 45%, #f87171)", "#fbbf24", "color-mix(in oklab, #34d399 62%, #fbbf24)", "#34d399"].map((c, i) => (
            <span key={i} style={{ width:9, height:9, borderRadius:2, background:c, flexShrink:0 }}/>
          ))}
          <span>better</span>
          <span style={{ marginLeft:"auto" }} className="fc-legend-note">one cell per day · metric &amp; scope in Customize</span>
        </div>
        </div>
      </div>
    </>
  );
}

// Renderer table the dashboard falls through to for these widgets.
window.DASH_WIDGET_RENDERERS = {
  fleetCalendar:     (p) => <WFleetCalendar {...p}/>,
  fleetDrift:        (p) => <WFleetDrift {...p}/>,
  closurePressure:   (p) => <WClosurePressure {...p}/>,
  rollbackReadiness: (p) => <WRollbackReadiness {...p}/>,
  deployHeatmap:     (p) => <WDeployHeatmap {...p}/>,
  rebootRequired:    (p) => <WRebootRequired {...p}/>,
};
