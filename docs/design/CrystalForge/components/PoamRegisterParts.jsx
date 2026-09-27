// POA&M register — shared pieces: the unified index over remediation plans and risk
// acceptances, the scope bar that replaces the old side navigator, the rows table, groups,
// and the risk-acceptance tray. PoamsView.jsx composes these.

const RR_ME = "Mira Reyes";
const RR_SEV = { high:0, medium:1, low:2 };
const RR_RA_STATUS = { active:{ label:"Risk accepted", color:"#a78bfa" }, revoked:{ label:"Revoked", color:"#6b7280" }, converted:{ label:"Converted to POA&M", color:"#60a5fa" } };
const RR_DUE_ORDER = ["Late", "Due this week", "Due in 30 days", "Due in 90 days", "Later", "No date", "Closed"];

function rrAddDays(d, n) { const x = new Date(d + "T00:00:00Z"); x.setUTCDate(x.getUTCDate() + n); return x.toISOString().slice(0, 10); }

function rrIndex() {
  const staleCut = rrAddDays(POAM_TODAY, -30);
  const sysById = new Map(SYSTEMS.map(s => [s.id, s]));
  const bundles = typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : [];
  const scopeOf = (findings, cveRefs, extraSys, extraBundle) => {
    const ids = new Set([...findings.map(f => f.sysId), ...(cveRefs || []).map(c => c.sysId), extraSys].filter(Boolean));
    const systems = [...ids].map(id => sysById.get(id)).filter(Boolean);
    const cves = [...new Set((cveRefs || []).map(c => c.id))];
    const bIds = [...new Set([...findings.map(f => f.bundleId), extraBundle].filter(Boolean))];
    const bs = bIds.map(id => bundles.find(b => b.id === id)).filter(Boolean).map(b => ({ id:b.id, name:b.name }));
    if (!bs.length && cves.length) bs.push({ id:"__cve", name:"CVE remediation" });
    return { systems, envs:[...new Set(systems.map(s => s.environment))], cves, bundles:bs, reqs:[...new Set(findings.map(f => poamRequirementLabel(f.policyId)))] };
  };
  const plans = POAMS.map(p => {
    const sc = scopeOf(p.findings, p.cveRefs, p.assignmentRef?.sysId, p.assignmentRef?.bundleId);
    const open = p.status !== "completed";
    const last = (p.activity || []).reduce((m, a) => a.at > m ? a.at : m, p.opened || "");
    const owner = p.owner && p.owner !== "unassigned" ? p.owner : "unassigned";
    const st = POAM_STATUS[p.status] || POAM_STATUS.open;
    return { kind:"poam", item:p, id:p.id, title:p.title, severity:p.severity, ...sc, open, owner, last, date:p.due,
      days:poamDaysLeft(p), late:poamIsOverdue(p), prog:poamMilestoneProgress(p), stale:open && !!last && last < staleCut,
      unassigned:open && owner === "unassigned", noReview:false, statusKey:p.status, statusLabel:st.label, statusColor:st.color,
      text:[p.id, p.title, owner, ...sc.systems.map(s => s.hostname), ...sc.envs, ...sc.reqs, ...sc.cves].join(" ").toLowerCase() };
  });
  const acc = (typeof RISK_ACCEPTANCES !== "undefined" ? RISK_ACCEPTANCES : []).map(r => {
    const sc = scopeOf(r.findings, r.cveRefs);
    const open = r.status === "active";
    const st = RR_RA_STATUS[r.status] || RR_RA_STATUS.active;
    return { kind:"ra", item:r, id:r.id, title:r.title, severity:r.severity, ...sc, open, owner:r.approver, last:(r.history || []).reduce((m, a) => a.at > m ? a.at : m, r.approvedAt),
      date:r.reviewDate, days:raDaysLeft(r), late:raIsExpired(r), prog:null, stale:false, unassigned:false, noReview:open && !r.reviewDate,
      statusKey:"ra-" + r.status, statusLabel:st.label, statusColor:st.color,
      text:[r.id, r.title, r.approver, r.justification, ...sc.systems.map(s => s.hostname), ...sc.envs, ...sc.reqs, ...sc.cves].join(" ").toLowerCase() };
  });
  return [...plans, ...acc];
}

const RR_Q = {
  late:     { label:"Overdue plans",          color:"#f87171", blurb:"Past target completion",  test:x => x.kind === "poam" && x.late },
  expired:  { label:"Expired acceptances",    color:"#fb923c", blurb:"Renew or plan a fix",     test:x => x.kind === "ra" && x.late },
  soon:     { label:"Coming due",             color:"#fbbf24", blurb:"Plans ≤14d · reviews ≤30d", test:x => x.open && !x.late && x.days != null && x.days <= (x.kind === "poam" ? 14 : 30) },
  soonPlan: { label:"Due in 14 days",         color:"#fbbf24", blurb:"Coming up next",          test:x => x.kind === "poam" && x.open && !x.late && x.days != null && x.days <= 14 },
  soonRa:   { label:"Review in 30 days",      color:"#fbbf24", blurb:"Re-review before expiry", test:x => x.kind === "ra" && x.open && !x.late && x.days != null && x.days <= 30 },
  awaiting: { label:"Awaiting verification",  color:"#a78bfa", blurb:"Re-evaluate to close",    test:x => x.statusKey === "awaiting_verification" },
  blocked:  { label:"Blocked",                color:"#f97316", blurb:"Dependency to clear",     test:x => x.statusKey === "blocked" },
  stale:    { label:"No activity in 30 days", color:"#9ca3af", blurb:"Open but quiet",          test:x => x.stale },
  unassigned:{ label:"Unassigned",            color:"#60a5fa", blurb:"Needs an owner",          test:x => x.unassigned },
  noReview: { label:"No review date",         color:"#60a5fa", blurb:"Assessors flag these",    test:x => x.noReview },
  gaps:     { label:"Missing owner/review",   color:"#60a5fa", blurb:"Unassigned or undated",   test:x => x.unassigned || x.noReview },
  cat1:     { label:"CAT I accepted",         color:"#f87171", blurb:"Highest-risk decisions",  test:x => x.kind === "ra" && x.open && x.severity === "high" },
};
const RR_QUEUES = {
  all:  ["late", "expired", "soon", "awaiting", "blocked", "gaps"],
  poam: ["late", "soonPlan", "awaiting", "blocked", "stale", "unassigned"],
  ra:   ["expired", "soonRa", "noReview", "cat1"],
};

function rrCompare(sort) {
  const sev = (a, b) => (RR_SEV[a.severity] ?? 3) - (RR_SEV[b.severity] ?? 3);
  const due = (a, b) => (a.days ?? 99999) - (b.days ?? 99999);
  const openFirst = (a, b) => a.open === b.open ? 0 : a.open ? -1 : 1;
  if (sort === "due") return (a, b) => openFirst(a, b) || due(a, b);
  if (sort === "severity") return (a, b) => sev(a, b) || due(a, b);
  if (sort === "updated") return (a, b) => a.last < b.last ? 1 : a.last > b.last ? -1 : 0;
  if (sort === "id") return (a, b) => a.id < b.id ? 1 : -1;
  return (a, b) => openFirst(a, b) || (a.late === b.late ? 0 : a.late ? -1 : 1) || sev(a, b) || due(a, b);
}

function rrGroupKeys(x, by) {
  if (by === "type") return [x.kind === "poam" ? "Remediation plans" : "Risk acceptances"];
  if (by === "env") return x.envs.length ? x.envs : ["No host scope"];
  if (by === "system") return x.systems.length ? x.systems.map(s => s.hostname) : ["No host scope"];
  if (by === "bundle") return x.bundles.length ? x.bundles.map(b => b.name) : ["No bundle"];
  if (by === "owner") return [x.owner];
  if (by === "due") return [!x.open ? "Closed" : x.late ? "Late" : x.days == null ? "No date" : x.days <= 7 ? "Due this week" : x.days <= 30 ? "Due in 30 days" : x.days <= 90 ? "Due in 90 days" : "Later"];
  return ["All"];
}

function rrInScope(x, scope) {
  if (!scope) return true;
  if (scope.type === "env") return x.envs.includes(scope.id);
  if (scope.type === "system") return x.systems.some(s => s.id === scope.id);
  if (scope.type === "bundle") return x.bundles.some(b => b.id === scope.id);
  if (scope.type === "owner") return x.owner === scope.id;
  return true;
}
function rrTally(list) { return { n:list.length, late:list.filter(x => x.late).length }; }
function rrEnvDot(name) { const e = (typeof ENVIRONMENTS !== "undefined" ? ENVIRONMENTS : []).find(v => v.name === name); return e ? (e.dot || e.color) : null; }

/* ── Scope bar: breadcrumb + one row of child scopes, instead of a side column ── */
function RegisterScopeBar({ items, scope, onScope, dim, onDim, count, pills, right }) {
  const [moreOpen, setMoreOpen] = React.useState(false);
  const [mq, setMq] = React.useState("");
  const popRef = React.useRef(null);
  React.useEffect(() => {
    if (!moreOpen) return;
    const off = (e) => { if (popRef.current && !popRef.current.contains(e.target)) setMoreOpen(false); };
    const esc = (e) => { if (e.key === "Escape") setMoreOpen(false); };
    document.addEventListener("mousedown", off); document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", off); document.removeEventListener("keydown", esc); };
  }, [moreOpen]);
  React.useEffect(() => { setMoreOpen(false); setMq(""); }, [scope, dim]);

  const scopeSys = scope?.type === "system" ? SYSTEMS.find(s => s.id === scope.id) : null;
  const path = [{ label:"All", scope:null }];
  if (scope?.type === "env") path.push({ label:scope.label, scope });
  if (scopeSys) { path.push({ label:scopeSys.environment, scope:{ type:"env", id:scopeSys.environment, label:scopeSys.environment } }); path.push({ label:scopeSys.hostname, scope, mono:true }); }
  if (scope && ["bundle", "owner"].includes(scope.type)) path.push({ label:scope.label, scope });

  let kids = [];
  if (dim === "env" && !scope) {
    kids = (typeof ENVIRONMENTS !== "undefined" ? ENVIRONMENTS : []).map(e => ({ type:"env", id:e.name, label:e.name, dot:e.dot || e.color, ...rrTally(items.filter(x => x.envs.includes(e.name))) }));
  } else if (dim === "env" && scope?.type === "env") {
    kids = SYSTEMS.filter(s => s.environment === scope.id).map(s => ({ type:"system", id:s.id, label:s.hostname, mono:true, ...rrTally(items.filter(x => x.systems.some(y => y.id === s.id))) }));
  } else if (!scope && dim !== "env") {
    const m = new Map();
    items.forEach(x => (dim === "bundle" ? x.bundles : [{ id:x.owner, name:x.owner }]).forEach(k => { if (!m.has(k.id)) m.set(k.id, { type:dim, id:k.id, label:k.id === "unassigned" ? "Unassigned" : k.name, list:[] }); m.get(k.id).list.push(x); }));
    kids = [...m.values()].map(v => ({ ...v, ...rrTally(v.list) }));
  }
  kids = kids.filter(k => k.n).sort((a, b) => b.late - a.late || b.n - a.n);
  const VISIBLE = 7;
  const shown = kids.slice(0, VISIBLE), rest = kids.slice(VISIBLE);
  const restQ = rest.filter(k => !mq.trim() || k.label.toLowerCase().includes(mq.trim().toLowerCase()));
  const childNoun = dim === "env" ? (scope?.type === "env" ? "hosts" : "environments") : dim === "bundle" ? "bundles" : "owners";

  const Pill = ({ k }) => (
    <button type="button" className="rr-pill focus-ring" onClick={() => onScope({ type:k.type, id:k.id, label:k.label })}>
      {k.dot && <span className="pv-dot" style={{ background:k.dot }}/>}
      <span className={`rr-pill-label${k.mono ? " mono" : ""}`}>{k.label}</span>
      <span className="rr-pill-n">{k.n}</span>
      {k.late > 0 && <span className="rr-pill-late" title={`${k.late} late`}>{k.late}</span>}
    </button>
  );

  return (
    <div className="rr-scope">
      <div className="rr-scope-top">
        <div className="seg xs" role="tablist" aria-label="Browse by">
          {[["env","Environment"],["bundle","Bundle"],["owner","Owner"]].map(([k, l]) => (
            <button key={k} type="button" role="tab" aria-selected={dim === k} className={dim === k ? "active" : ""} onClick={() => onDim(k)}>{l}</button>
          ))}
        </div>
        <nav className="rr-crumb" aria-label="Scope">
          {path.map((c, i) => i < path.length - 1
            ? <React.Fragment key={i}><button type="button" className="pv-link focus-ring" onClick={() => onScope(c.scope)}>{c.label}</button><Icon name="chevron-right" size={11} style={{ color:"var(--cf-text-disabled)" }}/></React.Fragment>
            : <b key={i} className={c.mono ? "mono" : ""}>{c.label}</b>)}
        </nav>
        {pills}
        <span className="pv-count">{count}</span>
        {right}
      </div>
      {kids.length > 0 && (
        <div className="rr-pills" aria-label={`Narrow to ${childNoun}`}>
          {shown.map(k => <Pill key={k.id} k={k}/>)}
          {rest.length > 0 && (
            <div className="rr-more-wrap" ref={popRef}>
              <button type="button" className="rr-pill rr-pill-more focus-ring" aria-expanded={moreOpen} onClick={() => setMoreOpen(o => !o)}>
                +{rest.length} more {childNoun} <Icon name="chevron-down" size={11}/>
              </button>
              {moreOpen && (
                <div className="rr-pop card" role="dialog" aria-label={`More ${childNoun}`}>
                  <div className="filter-search" style={{ maxWidth:"none" }}>
                    <Icon name="search"/>
                    <input autoFocus className="input focus-ring" placeholder={`Find ${childNoun}…`} value={mq} onChange={e => setMq(e.target.value)}/>
                  </div>
                  <div className="rr-pop-list">
                    {restQ.map(k => (
                      <button key={k.id} type="button" className="pv-node focus-ring" onClick={() => onScope({ type:k.type, id:k.id, label:k.label })}>
                        {k.dot && <span className="pv-dot" style={{ background:k.dot }}/>}
                        <span className={`pv-node-label${k.mono ? " mono" : ""}`}>{k.label}</span>
                        {k.late > 0 && <span className="pv-node-od">{k.late}</span>}
                        <span className="pv-node-n">{k.n}</span>
                      </button>
                    ))}
                    {!restQ.length && <div className="pv-nav-empty">No matches.</div>}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/* ── Rows ──────────────────────────────────────────────────────────────── */
function RegisterStatusChip({ x }) {
  if (x.kind === "poam") return <PoamStatusChip poam={x.item} showOverdue={false}/>;
  return <span className="chip" style={{ fontSize:10, color:x.statusColor, background:`color-mix(in oklab, ${x.statusColor} 14%, transparent)` }}>{x.statusLabel}</span>;
}

function RegisterRowsTable({ rows, sel, selectableIds, onOpen }) {
  return (
    <div className="pv-table-wrap">
      <table className="sys-table compact sys-table-dense pv-table">
        <colgroup><col style={{ width:104 }}/><col style={{ width:"40%" }}/><col style={{ width:60 }}/><col style={{ width:116 }}/><col className="pv-c-ms" style={{ width:92 }}/><col className="pv-c-owner" style={{ width:100 }}/><col style={{ width:80 }}/></colgroup>
        <thead><tr>
          <th>ID</th><th>Title</th><th>Risk</th><th>Status</th><th className="pv-c-ms">Progress</th><th className="pv-c-owner">Owner</th><th style={{ textAlign:"right" }} title="Target completion date, or review date for a risk acceptance">Due</th>
        </tr></thead>
        <tbody>
          {rows.map(x => {
            const on = sel.has(x.id), ra = x.kind === "ra";
            const where = x.systems.length === 1 ? `${x.envs[0]} · ${x.systems[0].hostname}` : x.systems.length ? `${x.systems.length} hosts · ${x.envs.join(", ")}` : "No host scope";
            const what = x.reqs.length ? x.reqs[0] + (x.reqs.length > 1 ? ` +${x.reqs.length - 1}` : "") : x.cves.length ? x.cves[0] + (x.cves.length > 1 ? ` +${x.cves.length - 1}` : "") : "baseline assignment";
            const rel = !x.open ? (ra ? x.item.status : "closed")
              : x.late ? (ra ? `expired ${-x.days}d ago` : `${-x.days}d late`)
              : x.noReview ? "no review date"
              : x.days != null && x.days <= (ra ? 30 : 14) ? (ra ? `review in ${x.days}d` : `in ${x.days}d`) : "";
            const relColor = x.late ? "#f87171" : !x.open ? "var(--cf-text-muted)" : "#fbbf24";
            return (
              <tr key={x.id} className={`selectable${on ? " row-checked" : ""}`} style={{ cursor:"pointer" }}
                onMouseDown={(e) => { if (e.shiftKey) e.preventDefault(); }}
                onClick={(e) => { if (sel.handleClick(e, x.id, selectableIds)) return; sel.setAnchor(x.id); onOpen(x); }}>
                <td><span className="mono" style={{ fontWeight:700, fontSize:12 }}>{x.id}</span></td>
                <td>
                  <div className="pv-title" title={ra ? `${x.title}\n\n${x.item.justification}` : x.title}>{x.title}</div>
                  <div className="pv-sub">
                    <span>{where}</span><span className="mono">{what}</span>
                    <span className="pv-sub-owner">{x.unassigned ? "Unassigned" : x.owner}</span>
                    {ra && <span className="rr-just">{x.item.justification}</span>}
                    {x.stale && <span className="pv-quiet">quiet since {poamShortDate(x.last)}</span>}
                  </div>
                </td>
                <td><PoamSevChip severity={x.severity}/></td>
                <td><RegisterStatusChip x={x}/></td>
                <td className="pv-c-ms">
                  {ra
                    ? <span className="rr-appr">approved {poamShortDate(x.item.approvedAt)}</span>
                    : <div className="pv-prog" title={`${x.prog.done} of ${x.prog.total} milestones done`}>
                        <span className="pv-prog-bar"><span style={{ width:`${x.prog.pct}%` }}/></span>
                        <span className="mono pv-prog-t">{x.prog.total ? `${x.prog.done}/${x.prog.total}` : "—"}</span>
                      </div>}
                </td>
                <td className={`pv-c-owner${x.unassigned ? " pv-unassigned" : ""}`} style={{ fontSize:11.5 }}>{x.unassigned ? "Unassigned" : x.owner}</td>
                <td style={{ textAlign:"right" }}>
                  <div className="mono" style={{ fontSize:11.5, color: x.late ? "#f87171" : "var(--cf-text-secondary)", fontWeight: x.late ? 700 : 400 }}>{poamShortDate(x.date)}</div>
                  {rel && <div className="pv-rel" style={{ color:relColor }}>{rel}</div>}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function RegisterGroup({ name, by, items, open, onToggleOpen, onFocus, ...rowProps }) {
  const [limit, setLimit] = React.useState(10);
  const t = rrTally(items);
  const cat1 = items.filter(x => x.open && x.severity === "high").length;
  const segs = [];
  items.forEach(x => { const s = segs.find(v => v.k === x.statusKey); s ? s.n++ : segs.push({ k:x.statusKey, n:1, c:x.statusColor, l:x.statusLabel }); });
  const dot = by === "env" ? rrEnvDot(name) : null;
  return (
    <section className="pv-group">
      <div className="pv-group-head">
        <button type="button" className="pv-group-toggle focus-ring" aria-expanded={open} onClick={onToggleOpen}>
          <Icon name={open ? "chevron-down" : "chevron-right"} size={13} style={{ color:"var(--cf-text-muted)" }}/>
          {dot && <span className="pv-dot" style={{ background:dot }}/>}
          <span className={`pv-group-name${by === "system" ? " mono" : ""}`}>{name}</span>
          <span className="pv-group-n">{items.length}</span>
          {t.late > 0 && <span className="chip chip-critical" style={{ fontSize:10 }}>{t.late} late</span>}
          {cat1 > 0 && <span className="chip" style={{ fontSize:10, color:"#f87171", background:"color-mix(in oklab,#f87171 12%,transparent)" }}>{cat1} CAT I</span>}
        </button>
        <span className="pv-stack" aria-hidden="true">{segs.map(v => <span key={v.k} title={`${v.n} ${v.l}`} style={{ flex:v.n, background:v.c }}/>)}</span>
        {onFocus && <button type="button" className="btn btn-ghost xs focus-ring" onClick={onFocus}>Focus</button>}
      </div>
      {open && (
        <>
          <RegisterRowsTable rows={items.slice(0, limit)} {...rowProps} selectableIds={items.slice(0, limit).map(x => x.id)}/>
          {items.length > limit && (
            <button type="button" className="pv-more pv-more-row focus-ring" onClick={() => setLimit(l => l + 25)}>
              Show {Math.min(25, items.length - limit)} more · {items.length - limit} hidden
            </button>
          )}
        </>
      )}
    </section>
  );
}

/* ── Risk acceptance tray ────────────────────────────────────────────────── */
function RiskAcceptanceTray({ r, onClose, onOpenSystem }) {
  usePoamStore();
  const [confirmRevoke, setConfirmRevoke] = React.useState(false);
  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  const days = raDaysLeft(r), expired = raIsExpired(r), active = r.status === "active";
  const st = RR_RA_STATUS[r.status] || RR_RA_STATUS.active;
  const rows = r.findings.length
    ? r.findings.map(f => ({ sys:SYSTEMS.find(s => s.id === f.sysId), what:poamRequirementLabel(f.policyId), where:(COMPLIANCE_BUNDLES.find(b => b.id === f.bundleId) || {}).name || f.bundleId }))
    : r.cveRefs.map(c => ({ sys:SYSTEMS.find(s => s.id === c.sysId), what:c.id, where:c.pkg }));
  const Section = ({ title, children }) => (
    <section style={{ borderTop:"1px solid var(--cf-divider)", padding:"14px 18px" }}>
      <h3 style={{ margin:"0 0 9px", fontSize:10.5, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", fontWeight:700 }}>{title}</h3>
      {children}
    </section>
  );
  const convert = () => { const item = raConvertToPoam(r.id); if (item) { onClose(); setTimeout(() => openPoamDetail(item.id), 40); } };
  return (
    <>
      <div className="poam-tray-backdrop" onClick={onClose}/>
      <aside className="fl-tray poam-tray" style={{ width:"min(760px, 96vw)" }} role="dialog" aria-label={r.id}>
        <header className="fl-tray-head">
          <div style={{ display:"flex", alignItems:"center", gap:12, minWidth:0, flex:1, overflow:"hidden" }}>
            <Icon name="shield" size={18} style={{ color:"#a78bfa", flexShrink:0 }}/>
            <div style={{ minWidth:0, flex:1, overflow:"hidden" }}>
              <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                <span className="mono" style={{ fontWeight:700, fontSize:15, whiteSpace:"nowrap", flexShrink:0 }}>{r.id}</span>
                <span className="chip" style={{ fontSize:10, color:st.color, background:`color-mix(in oklab, ${st.color} 14%, transparent)`, flexShrink:0 }}>{st.label}</span>
                <PoamSevChip severity={r.severity}/>
                {expired && <span className="chip chip-critical" style={{ fontSize:10, flexShrink:0 }}>review expired</span>}
              </div>
              <div style={{ fontSize:12, color:"var(--cf-text-secondary)", marginTop:3, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{r.title}</div>
            </div>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={16}/></button>
        </header>
        <div style={{ overflow:"auto", flex:1 }}>
          <div className="poam-meta">
            <div><span>Approved by</span><b>{r.approver}</b></div>
            <div><span>Approved</span><b className="mono">{r.approvedAt}</b></div>
            <div><span>Review by</span><b className="mono">{r.reviewDate || "not set"}</b>
              <em style={{ color: expired ? "#f87171" : !r.reviewDate ? "#fbbf24" : "var(--cf-text-muted)" }}>{!r.reviewDate ? "assessors flag undated acceptances" : expired ? `expired ${-days}d ago` : `in ${days}d`}</em></div>
            <div><span>Covers</span><b>{rows.length} host{rows.length === 1 ? "" : "s"}</b><em style={{ color:"var(--cf-text-muted)" }}>{r.env}</em></div>
          </div>
          {r.status === "converted" && r.poamId && (
            <div style={{ padding:"12px 18px 0" }}>
              <button type="button" className="poam-ref focus-ring" onClick={() => { onClose(); setTimeout(() => openPoamDetail(r.poamId), 40); }}>
                <Icon name="activity" size={12}/> Superseded by <span className="mono" style={{ fontWeight:700 }}>{r.poamId}</span> <Icon name="arrow-right" size={11}/>
              </button>
            </div>
          )}
          <Section title="Justification"><div style={{ fontSize:13, lineHeight:1.55 }}>{r.justification}</div></Section>
          <Section title="Compensating control"><div style={{ fontSize:13, lineHeight:1.55 }}>{r.compensating}</div></Section>
          <Section title={`Findings covered · ${rows.length}`}>
            <table className="sys-table compact sys-table-dense" style={{ fontSize:12 }}>
              <tbody>{rows.map((w, i) => (
                <tr key={i} style={{ cursor: onOpenSystem && w.sys ? "pointer" : "default" }} onClick={() => { if (onOpenSystem && w.sys) { onClose(); onOpenSystem(w.sys); } }}>
                  <td className="mono" style={{ fontWeight:600 }}>{w.sys ? w.sys.hostname : "—"}</td>
                  <td className="mono">{w.what}</td>
                  <td style={{ color:"var(--cf-text-muted)" }}>{w.where}</td>
                </tr>))}</tbody>
            </table>
            <div className="help" style={{ marginTop:8 }}>An acceptance does not change a result: these controls still report as waived, not passing.</div>
          </Section>
          <Section title="History">
            <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
              {[...r.history].reverse().map((h, i) => (
                <div key={i} style={{ display:"flex", gap:10, fontSize:12 }}>
                  <span className="mono" style={{ color:"var(--cf-text-muted)", width:80, flexShrink:0 }}>{h.at}</span>
                  <span className="mono" style={{ color:"var(--cf-text-secondary)", width:90, flexShrink:0 }}>{h.who}</span>
                  <span>{h.text}</span>
                </div>
              ))}
            </div>
          </Section>
        </div>
        {active && (
          <footer className="rr-tray-foot">
            {confirmRevoke
              ? <><span style={{ fontSize:12, color:"var(--cf-text-secondary)" }}>Findings return to outstanding.</span>
                  <button type="button" className="btn btn-ghost xs focus-ring" onClick={() => setConfirmRevoke(false)}>Keep</button>
                  <button type="button" className="btn btn-ghost xs focus-ring" style={{ color:"#f87171" }} onClick={() => raRevoke(r.id)}>Confirm revoke</button></>
              : <button type="button" className="btn btn-ghost xs focus-ring" onClick={() => setConfirmRevoke(true)}>Revoke</button>}
            <span style={{ flex:1 }}/>
            <button type="button" className={`btn ${expired ? "btn-ghost" : "btn-primary"} focus-ring`} onClick={() => raRenew(r.id, 90)}><Icon name="clock" size={13}/> Re-review · renew 90 days</button>
            <button type="button" className={`btn ${expired ? "btn-primary" : "btn-ghost"} focus-ring`} onClick={convert}><Icon name="plus" size={13}/> Convert to POA&M</button>
          </footer>
        )}
      </aside>
    </>
  );
}

/* ── Export: CSV / Excel / OSCAL JSON / OSCAL XML ─────────────────────────
   OSCAL is the NIST exchange format most federal POA&M submissions use; CSV/Excel
   cover ad hoc review and import into other trackers. Both formats commit to the
   whole mixed list, so a risk acceptance shows in an OSCAL POA&M export as a
   permanently-mitigated risk rather than being silently dropped. */
function rrDownload(name, content, mime) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([content], { type: mime }));
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}
function rrExportName(list, scope, ext) {
  return `poam-${POAM_TODAY}${scope ? "-" + String(scope.label).replace(/\W+/g, "-") : ""}.${ext}`;
}
function rrCsvRows(list) {
  const rows = [["ID","Type","Title","Risk","Status","Owner / approver","Opened / approved","Due / review","Environments","Systems","Requirements / CVEs","Progress","Justification"]];
  list.forEach(x => rows.push([x.id, x.kind === "poam" ? "POA&M" : "Risk acceptance", x.title, poamSeverityLabel(x.severity), x.statusLabel, x.owner,
    x.kind === "poam" ? x.item.opened : x.item.approvedAt, x.date || "", x.envs.join("; "), x.systems.map(s => s.hostname).join("; "), [...x.reqs, ...x.cves].join("; "),
    x.prog ? `${x.prog.done}/${x.prog.total}` : "", x.kind === "ra" ? x.item.justification : ""]));
  return rows;
}
function rrExportCsv(list, scope) {
  const csv = rrCsvRows(list).map(r => r.map(v => `"${String(v ?? "").replace(/"/g, '""')}"`).join(",")).join("\n");
  rrDownload(rrExportName(list, scope, "csv"), csv, "text/csv");
}
// A plain HTML table saved with an .xls extension opens natively in Excel — no xlsx
// library needed for a flat, one-sheet export.
function rrExportExcel(list, scope) {
  const rows = rrCsvRows(list);
  const esc = (v) => String(v ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const body = rows.map((r, i) => `<tr>${r.map(c => `<${i === 0 ? "th" : "td"}>${esc(c)}</${i === 0 ? "th" : "td"}>`).join("")}</tr>`).join("");
  const html = `<html><head><meta charset="utf-8"><!--[if gte mso 9]><xml><x:ExcelWorkbook><x:ExcelWorksheets><x:ExcelWorksheet><x:Name>POA&amp;M</x:Name><x:WorksheetOptions><x:DisplayGridlines/></x:WorksheetOptions></x:ExcelWorksheet></x:ExcelWorksheets></x:ExcelWorkbook></xml><![endif]--></head><body><table border="1">${body}</table></body></html>`;
  rrDownload(rrExportName(list, scope, "xls"), html, "application/vnd.ms-excel");
}
let _rrUuidSeed = 1;
function rrUuid(key) {
  let h = 2166136261; const s = String(key);
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
  h = h >>> 0;
  const hex = h.toString(16).padStart(8, "0");
  return `${hex.slice(0,8)}-0000-4000-8000-${String(_rrUuidSeed++).padStart(12, "0")}`;
}
function rrBuildOscal(list, scope) {
  const items = list.filter(x => x.kind === "poam").map(x => {
    const p = x.item;
    return {
      uuid: rrUuid("poam-item::" + p.id), title: p.title, description: p.plan || "No remediation plan recorded.",
      props: [
        { name:"crystal-forge-id", value:p.id }, { name:"status", value:p.status }, { name:"severity", value:poamSeverityLabel(p.severity) },
        { name:"owner", value:p.owner || "unassigned" }, { name:"scheduled-completion-date", value:p.due || "none" },
        ...(x.late ? [{ name:"overdue", value:"true" }] : []),
      ],
      "related-findings": x.systems.map(s => ({ "finding-uuid": rrUuid("finding::" + s.id + "::" + p.id) })),
      "remediation-tracking": { milestones: (p.milestones || []).map(m => ({ title:m.text, description:m.done ? `Completed ${m.doneAt || ""}` : `Target ${m.due || "none"}`, props:[{ name:"status", value:m.done ? "completed" : "open" }] })) },
    };
  });
  const risks = list.filter(x => x.kind === "ra").map(x => {
    const r = x.item;
    return {
      uuid: rrUuid("risk::" + r.id), title: r.title, statement: r.justification,
      status: r.status === "active" ? "open" : r.status === "converted" ? "closed" : "closed",
      props: [{ name:"crystal-forge-id", value:r.id }, { name:"disposition", value:"risk-accepted" }, { name:"approver", value:r.approver },
        { name:"approved", value:r.approvedAt }, { name:"review-date", value:r.reviewDate || "none" }, ...(x.late ? [{ name:"review-expired", value:"true" }] : [])],
      characterizations: [{ origin:{ actors:[{ type:"party", "actor-uuid": rrUuid("party::" + r.approver) }] },
        facets: [{ name:"compensating-control", system:"https://crystalforge.dev/ns/oscal", value:r.compensating }] }],
      "risk-log": { entries: (r.history || []).map(h => ({ uuid: rrUuid("log::" + r.id + "::" + h.at + h.text), title:h.text, start:h.at, "logged-by":[{ "party-uuid": rrUuid("party::" + h.who) }] })) },
    };
  });
  const now = new Date().toISOString();
  return { "plan-of-action-and-milestones": {
    uuid: rrUuid("poam::export::" + now),
    metadata: { title: `Plan of Action and Milestones${scope ? " — " + scope.label : ""}`, "last-modified": now, version: now.slice(0,10), "oscal-version": "1.1.2" },
    "system-id": { id: "crystal-forge-poam-register", "identifier-type": "https://crystalforge.dev/ns/registry" },
    "poam-items": items, risks,
  } };
}
function rrOscalToXml(node, tag, indent) {
  const pad = "  ".repeat(indent);
  if (node == null) return "";
  if (Array.isArray(node)) return node.map(v => rrOscalToXml(v, tag, indent)).join("");
  if (typeof node === "object") {
    const inner = Object.entries(node).map(([k, v]) => rrOscalToXml(v, k, indent + 1)).join("");
    return `${pad}<${tag}>\n${inner}${pad}</${tag}>\n`;
  }
  const esc = String(node).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  return `${pad}<${tag}>${esc}</${tag}>\n`;
}
function rrExportOscal(list, scope, xml) {
  const doc = rrBuildOscal(list, scope);
  if (!xml) { rrDownload(rrExportName(list, scope, "oscal.json"), JSON.stringify(doc, null, 2), "application/json"); return; }
  const body = rrOscalToXml(doc["plan-of-action-and-milestones"], "plan-of-action-and-milestones", 1);
  rrDownload(rrExportName(list, scope, "oscal.xml"), `<?xml version="1.0" encoding="UTF-8"?>\n<oscal-poam xmlns="https://crystalforge.dev/ns/oscal">\n${body}</oscal-poam>\n`, "application/xml");
}

const RR_EXPORT_FORMATS = [
  ["oscal-json", "OSCAL JSON", "Standard machine-readable POA&M"],
  ["excel",      "Excel",      "Spreadsheet for review and external workflows"],
  ["csv",        "CSV",        "Flat data for import into other tools"],
  ["oscal-xml",  "OSCAL XML",  "Standards-compatible XML"],
];
function rrRunExport(fmt, list, scope) {
  if (fmt === "csv") rrExportCsv(list, scope);
  else if (fmt === "excel") rrExportExcel(list, scope);
  else if (fmt === "oscal-json") rrExportOscal(list, scope, false);
  else if (fmt === "oscal-xml") rrExportOscal(list, scope, true);
}
function ExportMenuButton({ list, scope }) {
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
        <div className="rr-export-pop card" role="menu" aria-label="Export POA&Ms">
          <div className="rr-export-title">Export POA&Ms</div>
          {RR_EXPORT_FORMATS.map(([k, l, sub]) => (
            <button key={k} type="button" role="menuitem" className="rr-export-item focus-ring" onClick={() => { rrRunExport(k, list, scope); setOpen(false); }}>
              <span className="rr-export-item-l">{l}</span>
              <span className="rr-export-item-sub">{sub}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
Object.assign(window, { ExportMenuButton, rrDownload, rrUuid, rrOscalToXml, rrIndex, RR_Q, RR_QUEUES, RR_ME, RR_DUE_ORDER, rrCompare, rrGroupKeys, rrInScope, rrTally, rrAddDays,
  RegisterScopeBar, RegisterRowsTable, RegisterGroup, RiskAcceptanceTray });

