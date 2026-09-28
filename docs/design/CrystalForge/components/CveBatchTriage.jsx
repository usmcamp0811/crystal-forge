// Batch triage — one decision applied to a hand-picked group of CVEs on a host.
// Writes exactly the same per-CVE disposition records as CveTriageModal (host
// override map or env default), and POA&Ms are created through poamCreate with
// the same cveRefs shape; the only difference is that one POA&M or waiver can
// carry several CVEs.
const CVE_BATCH_SEV = { critical: 4, high: 3, medium: 2, low: 1, unknown: 0 };
const cveEnvsOf = (c) => [...new Set(SYSTEMS.filter(s => (c.affected || []).includes(s.id)).map(s => s.environment))];
// Fleet CVEs from older mock data carry a single acceptance + scopeEnvs instead of a
// per-env map; seed it the same way the CVE drawer does so nothing already decided is lost.
function cveDispositionsOf(c) {
  if (c.dispositions) return c.dispositions;
  if (!c.acceptance || c.acceptance === "outstanding" || c.acceptance === "partial") return {};
  const envs = c.scopeEnvs && c.scopeEnvs.length ? c.scopeEnvs : cveEnvsOf(c);
  const seed = {};
  envs.forEach(e => {
    seed[e] = c.acceptance === "scheduled"
      ? { state: "scheduled", poamId: c.poamId || null, owner: c.remediationOwner || "ops-team", due: c.reviewDate || null, plan: c.justification, by: c.justifiedBy, at: c.justifiedAt }
      : { state: "accepted", justification: c.justification, reviewDate: c.reviewDate || null, by: c.justifiedBy, at: c.justifiedAt };
  });
  return seed;
}
// Same roll-up the CVE drawer applies after a single triage.
function cveApplyFleetDispositions(c, next) {
  const envs = cveEnvsOf(c);
  c.dispositions = next;
  const de = envs.filter(e => next[e]);
  const st = [...new Set(de.map(e => next[e].state))];
  c.acceptance = de.length === 0 ? "outstanding" : (de.length < envs.length || st.length > 1) ? "partial" : st[0];
  const first = de.length ? next[de[0]] : null;
  c.justification = first ? (first.justification || first.plan || null) : null;
  c.justifiedBy = first ? first.by : null;
  c.justifiedAt = first ? first.at : null;
  c.scopeEnvs = de.length ? de : null;
  c.poamId = de.map(e => next[e].poamId).find(Boolean) || null;
}

// Two entry points: from a host page (sys given — host-only or that host's env, as
// in single triage), or from the fleet CVE view (no sys — pick environments; each
// CVE is recorded per env it was actually found in).
function CveBatchTriageModal({ cves, sys, envSystems, defaultEnvs, onClose, onSubmit }) {
  const fleet = !sys;
  const env = sys ? sys.environment : null;
  const roster = envSystems || (sys ? [sys] : []);
  const [scope, setScope] = React.useState("host");
  const [action, setAction] = React.useState("scheduled");
  const fleetEnvs = React.useMemo(() => {
    if (!fleet) return [];
    const m = new Map();
    cves.forEach(c => SYSTEMS.filter(s => (c.affected || []).includes(s.id)).forEach(s => m.set(s.environment, (m.get(s.environment) || new Set()).add(c.id))));
    return [...m.entries()].map(([e, set]) => ({ env: e, n: set.size })).sort((a, b) => b.n - a.n);
  }, [cves]);
  const [pickedEnvs, setPickedEnvs] = React.useState(() => new Set(defaultEnvs && defaultEnvs.length ? defaultEnvs : fleetEnvs.map(e => e.env)));
  const togglePicked = (e) => setPickedEnvs(p => { const n = new Set(p); n.has(e) ? n.delete(e) : n.add(e); return n; });
  // One (cve, env) pair per decision record we'd write.
  const pairs = React.useMemo(() => fleet
    ? cves.flatMap(c => [...pickedEnvs].map(e => ({ c, env: e, hosts: SYSTEMS.filter(s => s.environment === e && (c.affected || []).includes(s.id)) })).filter(p => p.hosts.length))
    : cves.map(c => ({ c, env, hosts: [sys] })), [cves, pickedEnvs, scope]);
  const existingPair = (p) => {
    const d = cveDispositionsOf(p.c);
    return !fleet && scope === "host" ? d.hosts && d.hosts[sys.id] : d[p.env];
  };
  const existing = (c) => pairs.filter(p => p.c === c).map(existingPair).find(Boolean);
  const [skipTriaged, setSkipTriaged] = React.useState(true);
  const livePairs = skipTriaged ? pairs.filter(p => !existingPair(p)) : pairs;
  const triagedCount = new Set(pairs.filter(existingPair).map(p => p.c.id)).size;
  const targets = cves.filter(c => livePairs.some(p => p.c === c));
  const pkgs = [...new Set(targets.map((c) => c.pkg))];
  const liveEnvs = [...new Set(livePairs.map(p => p.env))];
  const hostCount = new Set(livePairs.flatMap(p => p.hosts.map(h => h.id))).size;
  const maxSev = targets.reduce((m, c) => CVE_BATCH_SEV[c.severity] > CVE_BATCH_SEV[m] ? c.severity : m, "low");

  const [split, setSplit] = React.useState("one");
  const people = window.POAM_OWNER_PEOPLE || [];
  const [owner, setOwner] = React.useState(people[0] || "");
  const dueFor = (sev) => typeof poamDatePlus === "function" ? poamDatePlus(sev === "critical" ? 14 : sev === "high" ? 30 : 56) : "";
  const [due, setDue] = React.useState(() => dueFor(cves.reduce((m, c) => CVE_BATCH_SEV[c.severity] > CVE_BATCH_SEV[m] ? c.severity : m, "low")));
  const [plan, setPlan] = React.useState("");
  const [withMilestones, setWithMilestones] = React.useState(true);
  const [justification, setJustification] = React.useState("");
  const [reviewDate, setReviewDate] = React.useState("");

  const where = fleet ? (liveEnvs.join(", ") || "—") : scope === "host" ? sys.hostname : env;
  const poamCount = action === "scheduled" ? (!targets.length ? 0 : split === "one" ? 1 : split === "env" ? liveEnvs.length : pkgs.length) : 0;
  const needs = action === "accepted" ? justification.trim().length < 10 : !owner || !due;
  const canSubmit = targets.length > 0 && !needs;

  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape") { e.stopPropagation(); onClose(); } };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const submit = () => {
    if (!canSubmit) return;
    const out = new Map();
    const put = (p, rec) => {
      const next = out.get(p.c.id) || { ...cveDispositionsOf(p.c) };
      if (!fleet && scope === "host") next.hosts = { ...(next.hosts || {}), [sys.id]: { ...rec, env: p.env } };
      else next[p.env] = rec;
      out.set(p.c.id, next);
    };
    if (action === "scheduled") {
      const pairGroups = split === "one" ? [livePairs]
        : split === "env" ? liveEnvs.map(e => livePairs.filter(p => p.env === e))
        : pkgs.map(pk => livePairs.filter(p => p.c.pkg === pk));
      pairGroups.forEach((pg) => {
        const g = [...new Set(pg.map(p => p.c))];
        const gWhere = fleet ? [...new Set(pg.map(p => p.env))].join(", ") : where;
        const gp = [...new Set(g.map((c) => c.pkg))];
        const gs = g.reduce((m, c) => CVE_BATCH_SEV[c.severity] > CVE_BATCH_SEV[m] ? c.severity : m, "low");
        const label = g.length === 1 ? g[0].id : `${g.length} CVEs`;
        let poamId = null;
        if (typeof poamCreate === "function") {
          const item = poamCreate({
            title: `${label} — patch ${gp.join(", ")} ${!fleet && scope === "host" ? "on" : "in"} ${gWhere}`,
            owner, due,
            severity: gs === "critical" || gs === "high" ? "high" : gs === "medium" ? "medium" : "low",
            status: "open",
            plan: plan.trim() || `Upgrade ${gp.join(", ")} to patched releases across ${gWhere}; resolves ${g.map((c) => c.id).join(", ")}.`,
            // Only hosts each CVE was actually found on are attached as evidence.
            cveRefs: pg.flatMap(p => p.hosts.map(h => ({ id: p.c.id, pkg: p.c.pkg, sysId: h.id, hostname: h.hostname }))),
            milestones: withMilestones ? poamPatchMilestones({
              due, pkg: gp.join(", "), fixAvailable: g.every((c) => c.fix === "available"),
              rolloutText: !fleet && scope === "host" ? `Roll out to ${sys.hostname}` : `Roll out to ${gWhere} — every host in the environment, current and future`,
            }) : [],
          });
          poamId = item.id;
        }
        pg.forEach((p) => put(p, { state: "scheduled", poamId, owner, due, plan: plan.trim() || null, by: "mreyes", at: "just now" }));
      });
    } else {
      const batch = { id: `WB-${String(Date.now()).slice(-5)}`, size: targets.length };
      livePairs.forEach((p) => put(p, { state: "accepted", justification: justification.trim(), reviewDate: reviewDate || null, batch, by: "mreyes", at: "just now" }));
    }
    onSubmit(out);
  };

  const sevChip = (s) => <span className={`chip ${s === "critical" ? "chip-critical" : s === "high" ? "chip-warning" : "chip-unknown"}`} style={{ fontSize: 10 }}>{s}</span>;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={{ width: "min(720px,95vw)", maxHeight: "92vh" }}>
        <div className="modal-head" style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: 12 }}>
          <div>
            <h2>Triage {cves.length} CVEs together</h2>
            <p>One decision for the whole group. Each CVE still gets its own disposition record{fleet ? " per environment" : ""}, same as triaging it alone.</p>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16} /></button>
        </div>
        <div className="modal-body cve-batch-body" style={{ overflowY: "auto", display: "flex", flexDirection: "column", gap: 14 }}>
          <div className="cve-batch-list">
            {[...new Set(cves.map((c) => c.pkg))].map((p) => (
              <div key={p} className="cve-batch-pkg">
                <span className="mono cve-batch-pkg-name">{p}</span>
                <div className="cve-batch-ids">
                  {cves.filter((c) => c.pkg === p).map((c) => {
                    const skip = skipTriaged && existing(c);
                    return (
                      <span key={c.id} className={`cve-batch-id${skip ? " skip" : ""}`} title={skip ? `Already ${existing(c).state} — skipped` : `${c.severity} · CVSS ${isNaN(c.cvss) ? "—" : c.cvss.toFixed(1)}`}>
                        <span className="cve-batch-dot" data-sev={c.severity} />
                        <span className="mono">{c.id}</span>
                        {skip && <span style={{ fontSize: 10 }}>{existing(c).state}</span>}
                      </span>);
                  })}
                </div>
              </div>
            ))}
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <div className="field" style={{ marginTop: 0, gridColumn: fleet ? "1 / -1" : undefined }}>
              <label>Applies to</label>
              {fleet ? (
                <div className="cve-batch-envs">
                  {fleetEnvs.map(({ env: e, n }) => {
                    const col = (ENV_STYLE[e] && ENV_STYLE[e].fg) || "#9ca3af";
                    return (
                      <button key={e} type="button" className={`cve-sel-chip focus-ring${pickedEnvs.has(e) ? " on" : ""}`} onClick={() => togglePicked(e)}>
                        <span style={{ width: 7, height: 7, borderRadius: 99, background: col }} />{e} <span className="mono">{n} CVE{n === 1 ? "" : "s"}</span>
                      </button>);
                  })}
                </div>
              ) : (
                <div className="seg" style={{ width: "fit-content" }}>
                  <button className={scope === "host" ? "active" : ""} onClick={() => setScope("host")}>{sys.hostname} only</button>
                  <button className={scope === "env" ? "active" : ""} onClick={() => setScope("env")}>All of {env}</button>
                </div>
              )}
            </div>
            <div className="field" style={{ marginTop: 0 }}>
              <label>Disposition</label>
              <div className="seg" style={{ width: "fit-content" }}>
                <button className={action === "scheduled" ? "active" : ""} onClick={() => setAction("scheduled")}>Schedule patch</button>
                <button className={action === "accepted" ? "active" : ""} onClick={() => setAction("accepted")}>Accept risk</button>
              </div>
            </div>
          </div>
          <div className="help" style={{ marginTop: -6, fontSize: 11.5, color: "var(--cf-text-muted)" }}>
            {fleet
              ? `Each CVE is decided for every picked environment it was found in — ${hostCount} host${hostCount === 1 ? "" : "s"} today, plus hosts added later. Environments left unpicked stay as they are.`
              : scope === "host"
              ? "Host-specific decisions override the environment default for this machine only."
              : `Covers all ${roster.length} host${roster.length === 1 ? "" : "s"} in ${env}, including ones added later. Only hosts the CVEs were found on are attached as evidence.`}
          </div>

          {triagedCount > 0 && (
            <label className="poam-check">
              <input type="checkbox" checked={skipTriaged} onChange={(e) => setSkipTriaged(e.target.checked)} />
              <span>Skip {triagedCount} already triaged {fleet ? "in the picked environments" : scope === "host" ? "on this host" : `in ${env}`} <span style={{ color: "var(--cf-text-muted)" }}>— uncheck to overwrite their current decision.</span></span>
            </label>
          )}

          {action === "scheduled" && (
            <div style={{ padding: "12px 13px", borderRadius: 9, border: "1px solid rgba(96,165,250,0.3)", background: "rgba(96,165,250,0.06)", display: "flex", flexDirection: "column", gap: 12 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 7, fontSize: 11.5, fontWeight: 600, textTransform: "uppercase", letterSpacing: ".06em", color: "#60a5fa" }}>
                <Icon name="plus" size={12} /> POA&M — {where} · {targets.length} CVE{targets.length === 1 ? "" : "s"}
              </div>
              {(pkgs.length > 1 || liveEnvs.length > 1) && (
                <div className="field" style={{ marginTop: 0 }}>
                  <label>Group into</label>
                  <div className="seg" style={{ width: "fit-content" }}>
                    <button className={split === "one" ? "active" : ""} onClick={() => setSplit("one")}>One POA&M</button>
                    {pkgs.length > 1 && <button className={split === "package" ? "active" : ""} onClick={() => setSplit("package")}>One per package ({pkgs.length})</button>}
                    {fleet && liveEnvs.length > 1 && <button className={split === "env" ? "active" : ""} onClick={() => setSplit("env")}>One per environment ({liveEnvs.length})</button>}
                  </div>
                </div>
              )}
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
                <div className="field" style={{ marginTop: 0 }}>
                  <label>Owner</label>
                  {window.PoamOwnerOptions
                    ? <select className="input focus-ring" value={owner} onChange={(e) => setOwner(e.target.value)}><window.PoamOwnerOptions /></select>
                    : <input className="input focus-ring" value={owner} onChange={(e) => setOwner(e.target.value)} />}
                </div>
                <div className="field" style={{ marginTop: 0 }}>
                  <label>Target completion</label>
                  <input type="date" className="input focus-ring" value={due} onChange={(e) => setDue(e.target.value)} />
                  <div className="help">Defaulted from the most severe CVE ({maxSev}).</div>
                </div>
              </div>
              <div className="field" style={{ marginTop: 0 }}>
                <label>Remediation plan <span style={{ color: "var(--cf-text-muted)", fontWeight: 400 }}>· optional now, expected before review</span></label>
                <textarea className="input focus-ring" rows={2} value={plan} onChange={(e) => setPlan(e.target.value)}
                  placeholder={`Upgrade ${pkgs.slice(0, 3).join(", ")}${pkgs.length > 3 ? "…" : ""} to patched releases, roll out, and verify the scan clears`} style={{ resize: "vertical" }} />
              </div>
              <label className="poam-check">
                <input type="checkbox" checked={withMilestones} onChange={(e) => setWithMilestones(e.target.checked)} />
                <span>Start from standard patch milestones <span style={{ color: "var(--cf-text-muted)" }}>— identify version, staging, rollout, verify scan.</span></span>
              </label>
            </div>
          )}

          {action === "accepted" && (
            <div style={{ padding: "12px 13px", borderRadius: 9, border: "1px solid rgba(167,139,250,0.3)", background: "rgba(167,139,250,0.06)", display: "flex", flexDirection: "column", gap: 12 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 7, fontSize: 11.5, fontWeight: 600, textTransform: "uppercase", letterSpacing: ".06em", color: "#a78bfa" }}>
                <Icon name="check" size={12} /> Waiver — {where} · {targets.length} CVE{targets.length === 1 ? "" : "s"}
              </div>
              <div className="field" style={{ marginTop: 0 }}>
                <label>Justification <span style={{ color: "var(--cf-text-muted)", fontWeight: 400 }}>· required, applied to every CVE in the group</span></label>
                <textarea className="input focus-ring" rows={2} value={justification} onChange={(e) => setJustification(e.target.value)}
                  placeholder="Why is this acceptable / what is the compensating control?" style={{ resize: "vertical" }} />
                <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginTop: 6 }}>
                  {["Mitigated by network segmentation; service is internal-only.", "Compensating control via WAF rule.", "Vulnerable code path not reachable in this deployment.", "False positive — upstream backport already applied."].map((p) => (
                    <button key={p} className="focus-ring" onClick={() => setJustification(p)}
                      style={{ all: "unset", cursor: "pointer", fontSize: 10, padding: "3px 8px", borderRadius: 99, background: "var(--cf-subtle-bg)", color: "var(--cf-text-secondary)", border: "1px solid var(--cf-divider)" }}>
                      {p.length > 42 ? p.slice(0, 40) + "…" : p}
                    </button>
                  ))}
                </div>
              </div>
              <div className="field" style={{ marginTop: 0, maxWidth: 240 }}>
                <label>Review date <span style={{ color: "var(--cf-text-muted)", fontWeight: 400 }}>· optional</span></label>
                <input type="date" className="input focus-ring" value={reviewDate} onChange={(e) => setReviewDate(e.target.value)} />
              </div>
            </div>
          )}
        </div>
        <div className="modal-foot">
          <div style={{ marginRight: "auto", fontSize: 11.5, color: "var(--cf-text-muted)" }}>
            {targets.length === 0 ? (fleet && pickedEnvs.size === 0 ? "Pick at least one environment" : "Every selected CVE is already triaged")
              : action === "scheduled" ? `creates ${poamCount} POA&M${poamCount === 1 ? "" : "s"} · ${targets.length} CVE${targets.length === 1 ? "" : "s"}`
              : `1 waiver · ${targets.length} CVE${targets.length === 1 ? "" : "s"}`}
            {fleet && targets.length > 0 ? ` · ${liveEnvs.length} env${liveEnvs.length === 1 ? "" : "s"} · ${hostCount} host${hostCount === 1 ? "" : "s"}` : ""}
            {skipTriaged && triagedCount > 0 && targets.length > 0 ? ` · ${triagedCount} skipped` : ""}
          </div>
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" disabled={!canSubmit} style={!canSubmit ? { opacity: 0.5, cursor: "not-allowed" } : null} onClick={submit}>
            <Icon name="check" size={13} /> Apply to {targets.length}
          </button>
        </div>
      </div>
    </div>);
}

Object.assign(window, { CveBatchTriageModal, cveDispositionsOf, cveApplyFleetDispositions });
