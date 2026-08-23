// POA&M views — Plan of Action and Milestones.
//
// Placement rules this file encodes:
//  · the primary create/link action lives on the FINDING (a failed control on a host), not on
//    a bundle — a bundle is not a deficiency;
//  · a POA&M never changes an evaluation result. FAIL + open POA&M still reads FAIL;
//  · waivers/exceptions stay a separate decision and never share an action surface with these.

function usePoamStore() {
  const [v, setV] = React.useState(0);
  React.useEffect(() => {
    const h = () => setV(n => n + 1);
    window.addEventListener("cf-poam-change", h);
    return () => window.removeEventListener("cf-poam-change", h);
  }, []);
  return v;
}

function openPoamDetail(id) { window.dispatchEvent(new CustomEvent("cf-poam-open", { detail: id })); }

/* ── Chips ────────────────────────────────────────────────────────────────── */
function PoamStatusChip({ poam, showOverdue = true }) {
  const meta = POAM_STATUS[poam.status] || POAM_STATUS.open;
  const overdue = showOverdue && poamIsOverdue(poam);
  return (
    <span style={{ display:"inline-flex", alignItems:"center", gap:5 }}>
      <span className="chip" title={meta.blurb} style={{ fontSize:9.5, color:meta.color, background:`color-mix(in oklab, ${meta.color} 15%, transparent)` }}>{meta.label}</span>
      {overdue && <span className="chip" style={{ fontSize:9.5, color:"#f87171", background:"color-mix(in oklab, #f87171 15%, transparent)", border:"1px solid color-mix(in oklab, #f87171 45%, transparent)" }}>Overdue</span>}
    </span>
  );
}

function PoamSevChip({ severity }) {
  const c = poamSeverityColor(severity);
  return <span className="chip" style={{ fontSize:9.5, color:c, background:`color-mix(in oklab, ${c} 14%, transparent)` }}>{poamSeverityLabel(severity)}</span>;
}

/* ── Finding-level bar: the primary POA&M surface ─────────────────────────── */
function FindingPoamBar({ sysId, policyId, bundleId, evalStatus }) {
  usePoamStore();
  const [creating, setCreating] = React.useState(false);
  const [linking, setLinking] = React.useState(false);
  const finding = { sysId, policyId, bundleId };
  const linked = poamsForFinding(sysId, policyId);
  const active = linked.find(p => p.status !== "completed");
  const historical = linked.filter(p => p.status === "completed");

  // Nothing to show for a clean control that never had a POA&M.
  if (evalStatus !== "fail" && linked.length === 0) return null;

  return (
    <div className="poam-bar">
      <div className="poam-bar-label">
        <Icon name="activity" size={12}/> Remediation
      </div>
      {active ? (
        <button className="poam-ref focus-ring" onClick={() => openPoamDetail(active.id)}>
          <span className="mono" style={{ fontWeight:700 }}>{active.id}</span>
          <span style={{ opacity:0.5 }}>·</span>
          <PoamStatusChip poam={active}/>
          <span style={{ opacity:0.5 }}>·</span>
          <span style={{ color:"var(--cf-text-secondary)" }}>Due {poamShortDate(active.due)}</span>
          <Icon name="chevron-right" size={12} style={{ marginLeft:2, opacity:0.6 }}/>
        </button>
      ) : (
        <div style={{ display:"flex", gap:6, alignItems:"center", flexWrap:"wrap" }}>
          <button className="btn btn-ghost focus-ring xs" onClick={() => setCreating(true)}><Icon name="plus" size={11}/> Create POA&M</button>
          <button className="btn btn-ghost focus-ring xs" onClick={() => setLinking(true)}><Icon name="link" size={11}/> Link existing</button>
          <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>
            {evalStatus === "fail" ? "No remediation plan tracked for this finding." : "No open plan."}
          </span>
        </div>
      )}
      {historical.length > 0 && (
        <div style={{ display:"flex", gap:8, alignItems:"center", flexWrap:"wrap", fontSize:11, color:"var(--cf-text-muted)" }}>
          <span>History</span>
          {historical.map(p => (
            <button key={p.id} className="poam-ref poam-ref-quiet focus-ring" onClick={() => openPoamDetail(p.id)}>
              <span className="mono">{p.id}</span>
              <span style={{ color:"#34d399" }}>closed {poamShortDate(p.closed)}</span>
            </button>
          ))}
        </div>
      )}
      {creating && <PoamCreateModal finding={finding} onClose={() => setCreating(false)} onCreated={(p) => { setCreating(false); openPoamDetail(p.id); }}/>}
      {linking && <PoamLinkModal finding={finding} onClose={() => setLinking(false)} onLinked={(id) => { setLinking(false); openPoamDetail(id); }}/>}
    </div>
  );
}

/* ── Create ───────────────────────────────────────────────────────────────── */
const POAM_OWNERS = ["Platform Team","Security Team","Endpoint Team","Network Team","Database Team"];
const POAM_STD_MILESTONES = [
  { text:"Update NixOS module", offset:14 },
  { text:"Deploy to staging", offset:28 },
  { text:"Validate new configuration", offset:35 },
  { text:"Deploy to production", offset:49 },
  { text:"Verify compliance evaluation passes", offset:56 },
];
function poamDatePlus(days) {
  const d = new Date(POAM_TODAY);
  d.setDate(d.getDate() + days);
  return d.toISOString().slice(0, 10);
}

function PoamCreateModal({ finding, onClose, onCreated }) {
  const sys = SYSTEMS.find(s => s.id === finding.sysId);
  const policy = POLICIES.find(p => p.id === finding.policyId);
  const bundle = COMPLIANCE_BUNDLES.find(b => b.id === finding.bundleId);
  const ev = sys && bundle ? evidenceForControl(bundle, finding.policyId, sys) : null;
  const req = poamRequirementLabel(finding.policyId);

  const [form, setForm] = React.useState({
    title: `${policy?.name || finding.policyId} remediation on ${sys?.hostname || finding.sysId}`,
    owner: POAM_OWNERS[0],
    due: poamDatePlus(56),
    severity: policy?.severity || ev?.severity || "medium",
    status: "open",
    plan: "",
    withMilestones: true,
  });
  const set = (k, v) => setForm(f => ({ ...f, [k]: v }));

  const submit = () => {
    if (!form.title.trim()) return;
    const item = poamCreate({
      title: form.title.trim(),
      owner: form.owner,
      due: form.due,
      severity: form.severity,
      status: form.status,
      plan: form.plan,
      findings: [finding],
      milestones: form.withMilestones ? POAM_STD_MILESTONES.map(m => ({ text:m.text, due:poamDatePlus(m.offset), done:false })) : [],
    });
    onCreated?.(item);
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(720px, 94vw)", maxHeight:"92vh" }}>
        <div className="modal-head" style={{ display:"flex", alignItems:"flex-start", justifyContent:"space-between", gap:12 }}>
          <div>
            <div style={{ fontSize:15, fontWeight:700 }}>Create POA&M</div>
            <div style={{ fontSize:11.5, color:"var(--cf-text-muted)", marginTop:2 }}>A remediation plan for a known deficiency. The finding stays failing until a new evaluation says otherwise.</div>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16}/></button>
        </div>
        <div className="modal-body" style={{ display:"flex", flexDirection:"column", gap:14 }}>
          {/* Context Crystal Forge already knows — read only */}
          <div className="poam-ctx">
            <div className="poam-ctx-head">
              <Icon name="shield" size={12}/> Finding context
              <span style={{ marginLeft:"auto", fontSize:10.5, color:"var(--cf-text-muted)" }}>carried over automatically</span>
            </div>
            <div className="poam-ctx-grid">
              <div><span>System</span><b className="mono">{sys?.hostname || finding.sysId}</b></div>
              <div><span>Requirement</span><b className="mono">{req}</b></div>
              <div><span>Policy</span><b className="mono">{policy?.name || finding.policyId}</b></div>
              <div><span>Framework</span><b>{bundle?.framework} · {bundle?.version}</b></div>
              <div><span>Result</span><b style={{ color:"#f87171", textTransform:"uppercase" }}>{ev?.status || "fail"}</b></div>
              <div><span>Evidence</span><b>{ev?.items.length || 0} collected items</b></div>
            </div>
          </div>

          <div className="field">
            <label>Title</label>
            <input className="input focus-ring" value={form.title} onChange={e=>set("title", e.target.value)}/>
          </div>

          <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr 1fr", gap:12 }}>
            <div className="field">
              <label>Owner</label>
              <select className="input focus-ring" value={form.owner} onChange={e=>set("owner", e.target.value)}>
                {POAM_OWNERS.map(o => <option key={o} value={o}>{o}</option>)}
              </select>
            </div>
            <div className="field">
              <label>Target completion</label>
              <input type="date" className="input focus-ring" value={form.due} onChange={e=>set("due", e.target.value)}/>
            </div>
            <div className="field">
              <label>Risk</label>
              <select className="input focus-ring" value={form.severity} onChange={e=>set("severity", e.target.value)}>
                <option value="high">CAT I — High</option>
                <option value="medium">CAT II — Medium</option>
                <option value="low">CAT III — Low</option>
              </select>
            </div>
          </div>

          <div className="field">
            <label>Remediation plan <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· optional now, expected before review</span></label>
            <textarea className="input focus-ring" rows={3} value={form.plan} onChange={e=>set("plan", e.target.value)}
              placeholder="What will be changed, where, and how it gets verified" style={{ resize:"vertical" }}/>
          </div>

          <label className="poam-check">
            <input type="checkbox" checked={form.withMilestones} onChange={e=>set("withMilestones", e.target.checked)}/>
            <span>Start from the standard remediation milestones <span style={{ color:"var(--cf-text-muted)" }}>— module change, staging, validation, production, verification. Editable after creation.</span></span>
          </label>

          <div className="sd-callout sd-callout-info">
            <Icon name="info" size={13}/>
            <div style={{ fontSize:11.5 }}>This records a plan to fix the deficiency. To formally accept the risk instead, use the waiver flow on the control — the two are not interchangeable.</div>
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" disabled={!form.title.trim()} onClick={submit}>
            <Icon name="plus" size={13}/> Create POA&M
          </button>
        </div>
      </div>
    </div>
  );
}

/* ── Link an existing POA&M ───────────────────────────────────────────────── */
function PoamLinkModal({ finding, onClose, onLinked }) {
  const [q, setQ] = React.useState("");
  const open = POAMS.filter(p => p.status !== "completed" && !p.findings.some(f => f.sysId === finding.sysId && f.policyId === finding.policyId));
  const req = poamRequirementLabel(finding.policyId);
  const ql = q.trim().toLowerCase();
  // Same requirement first — the common case is one control failing across a tier.
  const ranked = open
    .map(p => ({ p, same: p.findings.some(f => f.policyId === finding.policyId) }))
    .filter(({ p }) => !ql || p.id.toLowerCase().includes(ql) || p.title.toLowerCase().includes(ql) || p.owner.toLowerCase().includes(ql))
    .sort((a, b) => (b.same ? 1 : 0) - (a.same ? 1 : 0));

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(680px, 94vw)", maxHeight:"92vh" }}>
        <div className="modal-head" style={{ display:"flex", alignItems:"flex-start", justifyContent:"space-between", gap:12 }}>
          <div>
            <div style={{ fontSize:15, fontWeight:700 }}>Link existing POA&M</div>
            <div style={{ fontSize:11.5, color:"var(--cf-text-muted)", marginTop:2 }}>
              Add <span className="mono">{SYSTEMS.find(s=>s.id===finding.sysId)?.hostname}</span> / <span className="mono">{req}</span> to a remediation effort already in flight.
            </div>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16}/></button>
        </div>
        <div className="modal-body" style={{ display:"flex", flexDirection:"column", gap:10 }}>
          <div className="filter-search" style={{ margin:0 }}>
            <Icon name="search" size={12}/>
            <input className="input focus-ring" autoFocus placeholder="Search by id, title, owner…" value={q} onChange={e=>setQ(e.target.value)}/>
          </div>
          <div style={{ display:"flex", flexDirection:"column", gap:6, maxHeight:340, overflowY:"auto" }}>
            {ranked.length === 0 && <div className="search-empty">No open POA&M items match.</div>}
            {ranked.map(({ p, same }) => (
              <button key={p.id} className="poam-pick focus-ring" onClick={() => { poamLinkFinding(p.id, finding); onLinked?.(p.id); }}>
                <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                  <span className="mono" style={{ fontWeight:700, fontSize:12 }}>{p.id}</span>
                  <PoamStatusChip poam={p}/>
                  <PoamSevChip severity={p.severity}/>
                  {same && <span className="chip" style={{ fontSize:9, color:"var(--cf-brand-purple)", background:"color-mix(in oklab, var(--cf-brand-purple) 16%, transparent)" }}>same requirement</span>}
                </div>
                <div style={{ fontSize:12.5, marginTop:4 }}>{p.title}</div>
                <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:3 }}>
                  {p.owner} · due {poamShortDate(p.due)} · {p.findings.length} finding{p.findings.length===1?"":"s"} linked
                </div>
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

/* ── Detail tray ──────────────────────────────────────────────────────────── */
function PoamDetailTray({ poam, onClose, onOpenFinding }) {
  usePoamStore();
  const [noteDraft, setNoteDraft] = React.useState("");
  const [msDraft, setMsDraft] = React.useState({ text:"", due:"" });
  const [linkOpen, setLinkOpen] = React.useState(false);
  const prog = poamMilestoneProgress(poam);
  const overdue = poamIsOverdue(poam);
  const days = poamDaysLeft(poam);
  const findingStates = poam.findings.map(f => ({ f, status: poamFindingStatus(f) }));
  const ready = poamVerificationReady(poam);

  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const Section = ({ title, right, children }) => (
    <section style={{ borderTop:"1px solid var(--cf-divider)", padding:"14px 18px" }}>
      <div style={{ display:"flex", alignItems:"center", gap:10, marginBottom:10 }}>
        <h3 style={{ margin:0, fontSize:10.5, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", fontWeight:700 }}>{title}</h3>
        <div style={{ marginLeft:"auto" }}>{right}</div>
      </div>
      {children}
    </section>
  );

  return (
    <>
      <div className="poam-tray-backdrop" onClick={onClose}/>
      <aside className="fl-tray poam-tray" style={{ width:"min(960px, 96vw)" }}>
        <header className="fl-tray-head">
          <div style={{ display:"flex", alignItems:"center", gap:12, minWidth:0, flex:1 }}>
            <Icon name="activity" size={18} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
            <div style={{ minWidth:0 }}>
              <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                <span className="mono" style={{ fontWeight:700, fontSize:15 }}>{poam.id}</span>
                <PoamStatusChip poam={poam}/>
                <PoamSevChip severity={poam.severity}/>
              </div>
              <div style={{ fontSize:12, color:"var(--cf-text-secondary)", marginTop:3, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{poam.title}</div>
            </div>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16}/></button>
        </header>

        <div style={{ overflow:"auto", flex:1 }}>
          {/* Plan meta */}
          <div className="poam-meta">
            <div><span>Owner</span>
              <select className="poam-inline-input mono" value={poam.owner} onChange={e=>poamSetField(poam.id, "owner", e.target.value)}>
                {[...new Set([poam.owner, ...POAM_OWNERS])].map(o => <option key={o} value={o}>{o}</option>)}
              </select>
            </div>
            <div><span>Target completion</span>
              <input type="date" className="poam-inline-input mono" value={poam.due || ""} onChange={e=>poamSetField(poam.id, "due", e.target.value)}/>
              <em style={{ color: overdue ? "#f87171" : "var(--cf-text-muted)" }}>
                {poam.status === "completed" ? `closed ${poamShortDate(poam.closed)}` : overdue ? `${Math.abs(days)}d overdue` : `${days}d remaining`}
              </em>
            </div>
            <div><span>Opened</span><b className="mono">{poam.opened}</b></div>
            <div><span>Milestones</span>
              <b className="mono">{prog.done}/{prog.total}</b>
              <div className="poam-progress"><div style={{ width:`${prog.pct}%` }}/></div>
            </div>
          </div>

          {/* Lifecycle */}
          <Section title="Remediation status" right={
            poam.status === "completed"
              ? <button className="btn btn-ghost focus-ring xs" onClick={()=>poamSetStatus(poam.id, "in_progress", "POA&M reopened.")}><Icon name="rollback" size={11}/> Reopen</button>
              : poam.status === "awaiting_verification"
                ? <button className="btn btn-primary focus-ring xs" disabled={!ready}
                    onClick={()=>{ poam.verification = { evalId:`eval-${9000 + POAMS.length}`, at:POAM_TODAY, result:"pass", note:"Closure verified against the latest passing evaluation for every linked finding." }; poamSetStatus(poam.id, "completed", "Closed — every linked finding now evaluates clean."); }}>
                    <Icon name="check" size={11}/> Close POA&M
                  </button>
                : <button className="btn btn-ghost focus-ring xs" onClick={()=>poamSetStatus(poam.id, "awaiting_verification", "Remediation reported complete — awaiting verification.")}>
                    <Icon name="check" size={11}/> Mark remediation complete
                  </button>
          }>
            <div className="seg" style={{ width:"fit-content", marginBottom:10 }}>
              {POAM_STATUS_ORDER.filter(s => s !== "completed").map(s => (
                <button key={s} className={poam.status === s ? "active" : ""} onClick={()=>poamSetStatus(poam.id, s)}
                  style={poam.status === s ? { color:POAM_STATUS[s].color } : undefined}>{POAM_STATUS[s].label}</button>
              ))}
            </div>
            {poam.status === "awaiting_verification" && (
              ready ? (
                <div className="sd-callout" style={{ background:"rgba(52,211,153,0.08)", borderColor:"rgba(52,211,153,0.25)" }}>
                  <Icon name="check" size={13} style={{ color:"#34d399" }}/>
                  <div style={{ fontSize:12 }}>Every linked finding now evaluates clean. Closing the POA&M records the passing evaluation as closure evidence.</div>
                </div>
              ) : (
                <div className="sd-callout sd-callout-warn">
                  <Icon name="warn" size={13}/>
                  <div style={{ fontSize:12 }}>
                    <strong>Not verified yet.</strong> Remediation is reported complete, but Crystal Forge still evaluates {findingStates.filter(x=>x.status==="fail").length} linked finding{findingStates.filter(x=>x.status==="fail").length===1?"":"s"} as failing. The POA&M cannot be closed and the finding stays open.
                  </div>
                </div>
              )
            )}
            {poam.status === "completed" && poam.verification && (
              <div className="poam-verify">
                <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                  <Icon name="check" size={13} style={{ color:"#34d399" }}/>
                  <strong style={{ fontSize:12 }}>Closure verified by evaluation</strong>
                  <span className="mono chip" style={{ fontSize:9.5 }}>{poam.verification.evalId}</span>
                  <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{poam.verification.at}</span>
                </div>
                <div style={{ fontSize:11.5, color:"var(--cf-text-secondary)", marginTop:6 }}>{poam.verification.note}</div>
              </div>
            )}
            {overdue && poam.status !== "completed" && (
              <div className="sd-callout sd-callout-danger" style={{ marginTop:10 }}>
                <Icon name="warn" size={13}/>
                <div style={{ fontSize:12 }}><strong>Overdue by {Math.abs(days)} days.</strong> Target completion was {poam.due}. Revise the date with a documented justification or escalate.</div>
              </div>
            )}
          </Section>

          {/* Findings */}
          <Section title={`Deficiency · ${poam.findings.length} finding${poam.findings.length===1?"":"s"}`} right={
            <button className="btn btn-ghost focus-ring xs" onClick={()=>setLinkOpen(o=>!o)}><Icon name="link" size={11}/> Link finding</button>
          }>
            {poam.findings.length === 0 && !poam.assignmentRef && (
              <div style={{ fontSize:12, color:"var(--cf-text-muted)" }}>No findings linked yet.</div>
            )}
            {poam.findings.length > 0 && (
              <table className="sys-table compact sys-table-dense">
                <thead><tr><th>Host</th><th>Requirement</th><th>Result</th><th style={{ textAlign:"right" }}> </th></tr></thead>
                <tbody>
                  {findingStates.map(({ f, status }) => {
                    const sys = SYSTEMS.find(s => s.id === f.sysId);
                    const c = { pass:"#34d399", warn:"#fbbf24", fail:"#f87171", waiver:"#a78bfa" }[status] || "var(--cf-text-muted)";
                    return (
                      <tr key={f.sysId + f.policyId}>
                        <td><span className="mono" style={{ fontWeight:600 }}>{sys?.hostname || f.sysId}</span>{sys && <span style={{ marginLeft:8 }}><EnvBadge env={sys.environment}/></span>}</td>
                        <td><span className="mono" style={{ fontSize:12 }}>{poamRequirementLabel(f.policyId)}</span> <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{POLICIES.find(p=>p.id===f.policyId)?.name}</span></td>
                        <td><span className="chip" style={{ fontSize:9.5, color:c, background:`color-mix(in oklab, ${c} 14%, transparent)`, textTransform:"uppercase" }}>{status || "unknown"}</span></td>
                        <td style={{ textAlign:"right", whiteSpace:"nowrap" }}>
                          <button className="btn btn-ghost focus-ring xs" onClick={()=>onOpenFinding?.(f)}>Evidence</button>
                          {poam.findings.length > 1 && (
                            <button className="btn-icon focus-ring" title="Unlink finding" onClick={()=>poamUnlinkFinding(poam.id, f)}><Icon name="x" size={12}/></button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
            {poam.assignmentRef && (() => {
              const sys = SYSTEMS.find(s => s.id === poam.assignmentRef.sysId);
              return (
                <div className="poam-ctx" style={{ marginTop:10 }}>
                  <div className="poam-ctx-head"><Icon name="shield" size={12}/> Baseline assignment reference</div>
                  <div style={{ fontSize:12, color:"var(--cf-text-secondary)", padding:"8px 10px" }}>
                    <span className="mono">{sys?.hostname}</span> stays pinned to <span className="mono">{COMPLIANCE_BUNDLES.find(b=>b.id===poam.assignmentRef.bundleId)?.name}</span> while this remediation is tracked. The exception decision itself lives on the assignment.
                  </div>
                </div>
              );
            })()}
            {linkOpen && <PoamFindingPicker poam={poam} onDone={()=>setLinkOpen(false)}/>}
          </Section>

          {/* Plan */}
          <Section title="Remediation plan">
            <textarea className="input focus-ring" rows={4} value={poam.plan || ""} onChange={e=>poamSetField(poam.id, "plan", e.target.value)}
              placeholder="What will be changed, where, and how it gets verified" style={{ resize:"vertical", fontSize:12.5, lineHeight:1.55 }}/>
          </Section>

          {/* Milestones */}
          <Section title={`Milestones · ${prog.done} of ${prog.total} complete`}>
            <div style={{ display:"flex", flexDirection:"column", gap:2 }}>
              {(poam.milestones || []).map((m, i) => {
                const late = !m.done && m.due && m.due < POAM_TODAY;
                return (
                  <div key={i} className="poam-ms">
                    <label className="poam-check" style={{ flex:1, minWidth:0 }}>
                      <input type="checkbox" checked={!!m.done} onChange={()=>poamToggleMilestone(poam.id, i)}/>
                      <span style={{ textDecoration: m.done ? "line-through" : "none", color: m.done ? "var(--cf-text-muted)" : "var(--cf-text-primary)" }}>{m.text}</span>
                    </label>
                    <span className="mono" style={{ fontSize:11, color: late ? "#f87171" : "var(--cf-text-muted)", whiteSpace:"nowrap" }}>
                      {m.done ? `done ${poamShortDate(m.doneAt)}` : m.due ? `due ${poamShortDate(m.due)}` : "no date"}
                    </span>
                    <button className="btn-icon focus-ring" title="Remove milestone" onClick={()=>poamRemoveMilestone(poam.id, i)}><Icon name="trash" size={12}/></button>
                  </div>
                );
              })}
            </div>
            <div style={{ display:"flex", gap:6, marginTop:10 }}>
              <input className="input focus-ring" placeholder="Add a milestone…" value={msDraft.text} onChange={e=>setMsDraft(d=>({ ...d, text:e.target.value }))}
                onKeyDown={e=>{ if (e.key === "Enter" && msDraft.text.trim()) { poamAddMilestone(poam.id, msDraft.text, msDraft.due); setMsDraft({ text:"", due:"" }); } }} style={{ flex:1, fontSize:12 }}/>
              <input type="date" className="input focus-ring mono" value={msDraft.due} onChange={e=>setMsDraft(d=>({ ...d, due:e.target.value }))} style={{ width:150, fontSize:12 }}/>
              <button className="btn btn-ghost focus-ring" disabled={!msDraft.text.trim()} onClick={()=>{ poamAddMilestone(poam.id, msDraft.text, msDraft.due); setMsDraft({ text:"", due:"" }); }}><Icon name="plus" size={12}/></button>
            </div>
          </Section>

          {/* Activity */}
          <Section title="Activity">
            <div style={{ display:"flex", flexDirection:"column", gap:8 }}>
              {[...poam.activity].reverse().map((a, i) => (
                <div key={i} style={{ display:"flex", gap:10, fontSize:12 }}>
                  <span className="mono" style={{ color:"var(--cf-text-muted)", flexShrink:0, width:78 }}>{a.at}</span>
                  <span className="mono" style={{ color:"var(--cf-brand-purple)", flexShrink:0, width:96, overflow:"hidden", textOverflow:"ellipsis" }}>{a.who}</span>
                  <span style={{ color:"var(--cf-text-secondary)" }}>{a.text}</span>
                </div>
              ))}
            </div>
            <div style={{ display:"flex", gap:6, marginTop:12 }}>
              <input className="input focus-ring" placeholder="Add a note…" value={noteDraft} onChange={e=>setNoteDraft(e.target.value)}
                onKeyDown={e=>{ if (e.key === "Enter" && noteDraft.trim()) { poamAddNote(poam.id, noteDraft); setNoteDraft(""); } }} style={{ flex:1, fontSize:12 }}/>
              <button className="btn btn-ghost focus-ring" disabled={!noteDraft.trim()} onClick={()=>{ poamAddNote(poam.id, noteDraft); setNoteDraft(""); }}>Add note</button>
            </div>
          </Section>
        </div>
      </aside>
    </>
  );
}

// Inline picker: other hosts where the same control is currently failing.
function PoamFindingPicker({ poam, onDone }) {
  const policyIds = [...new Set(poam.findings.map(f => f.policyId))];
  const [policyId, setPolicyId] = React.useState(policyIds[0] || "");
  const candidates = React.useMemo(() => {
    if (!policyId) return [];
    const bundle = COMPLIANCE_BUNDLES.find(b => (b.policyIds || []).includes(policyId));
    if (!bundle) return [];
    return SYSTEMS
      .filter(s => bundleStatusForSystem(bundle, s).applies)
      .filter(s => !poam.findings.some(f => f.sysId === s.id && f.policyId === policyId))
      .map(s => ({ sys:s, status: evidenceForControl(bundle, policyId, s).status, bundleId: bundle.id }))
      .filter(x => x.status === "fail");
  }, [policyId, poam.findings.length]);

  return (
    <div className="poam-ctx" style={{ marginTop:10 }}>
      <div className="poam-ctx-head">
        <Icon name="link" size={12}/> Other hosts failing this control
        {policyIds.length > 1 && (
          <select className="poam-inline-input mono" style={{ marginLeft:"auto" }} value={policyId} onChange={e=>setPolicyId(e.target.value)}>
            {policyIds.map(p => <option key={p} value={p}>{poamRequirementLabel(p)}</option>)}
          </select>
        )}
      </div>
      <div style={{ display:"flex", flexWrap:"wrap", gap:6, padding:"8px 10px" }}>
        {candidates.length === 0 && <span style={{ fontSize:11.5, color:"var(--cf-text-muted)" }}>No other host is failing this control right now.</span>}
        {candidates.map(({ sys, bundleId }) => (
          <button key={sys.id} className="btn btn-ghost focus-ring xs" onClick={()=>{ poamLinkFinding(poam.id, { sysId:sys.id, policyId, bundleId }); onDone?.(); }}>
            <Icon name="plus" size={10}/> <span className="mono">{sys.hostname}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

/* ── Roll-ups ─────────────────────────────────────────────────────────────── */
function PoamCountStrip({ list, failCount }) {
  const c = poamCounts(list);
  const noPlan = failCount == null ? null : Math.max(0, failCount - list.filter(p => p.status !== "completed").reduce((a,p)=>a+p.findings.length, 0));
  const cells = [
    failCount != null ? { label:"Open findings", val:failCount, color:"#f87171" } : null,
    { label:"On POA&M", val:c.open, color:"#60a5fa" },
    noPlan != null ? { label:"No POA&M", val:noPlan, color:"#fbbf24" } : null,
    { label:"Overdue", val:c.overdue, color:c.overdue ? "#f87171" : "var(--cf-text-muted)" },
    { label:"Awaiting verification", val:c.awaiting, color:"#a78bfa" },
    { label:"Closed", val:c.completed, color:"#34d399" },
  ].filter(Boolean);
  return (
    <div className="stat-strip stat-strip-flush poam-strip">
      {cells.map(s => (
        <div key={s.label} className="stat">
          <div className="stat-label">{s.label}</div>
          <div className="stat-value" style={{ color:s.color }}>{s.val}</div>
        </div>
      ))}
    </div>
  );
}

function PoamTable({ list, onOpen, emptyNote }) {
  if (list.length === 0) return <div style={{ padding:"18px 16px", fontSize:12, color:"var(--cf-text-muted)" }}>{emptyNote || "No POA&M items."}</div>;
  return (
    <table className="sys-table compact sys-table-dense">
      <colgroup><col style={{ width:104 }}/><col/><col style={{ width:150 }}/><col style={{ width:74 }}/><col style={{ width:170 }}/><col style={{ width:130 }}/><col style={{ width:92 }}/></colgroup>
      <thead><tr><th>POA&M</th><th>Title</th><th>Requirement</th><th>Risk</th><th>Status</th><th>Owner</th><th style={{ textAlign:"right" }}>Due</th></tr></thead>
      <tbody>
        {list.map(p => {
          const reqs = [...new Set(p.findings.map(f => poamRequirementLabel(f.policyId)))];
          const overdue = poamIsOverdue(p);
          return (
            <tr key={p.id} style={{ cursor:"pointer" }} onClick={()=>onOpen(p.id)}>
              <td><span className="mono" style={{ fontWeight:700, fontSize:12 }}>{p.id}</span></td>
              <td>
                <div style={{ fontSize:12.5 }}>{p.title}</div>
                {p.findings.length > 1 && <div style={{ fontSize:10.5, color:"var(--cf-text-muted)", marginTop:2 }}>{p.findings.length} findings across {new Set(p.findings.map(f=>f.sysId)).size} hosts</div>}
              </td>
              <td><span className="mono" style={{ fontSize:11.5 }}>{reqs.length ? reqs.slice(0,2).join(", ") : "baseline assignment"}{reqs.length > 2 ? ` +${reqs.length-2}` : ""}</span></td>
              <td><PoamSevChip severity={p.severity}/></td>
              <td><PoamStatusChip poam={p}/></td>
              <td style={{ fontSize:11.5, color:"var(--cf-text-secondary)" }}>{p.owner}</td>
              <td className="mono" style={{ textAlign:"right", fontSize:11.5, color: overdue ? "#f87171" : "var(--cf-text-muted)", fontWeight: overdue ? 700 : 400 }}>{poamShortDate(p.due)}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

/* ── System compliance tab section ────────────────────────────────────────── */
function SystemPoamSection({ sys }) {
  usePoamStore();
  const [filter, setFilter] = React.useState("open");
  const all = poamsForSystem(sys.id);
  const list = all.filter(p => filter === "all" ? true : filter === "overdue" ? poamIsOverdue(p) : filter === "closed" ? p.status === "completed" : p.status !== "completed");
  const c = poamCounts(all);

  return (
    <div className="card" style={{ padding:0, overflow:"hidden" }}>
      <div style={{ padding:"13px 16px", display:"flex", alignItems:"center", gap:10, flexWrap:"wrap" }}>
        <Icon name="activity" size={14} style={{ color:"var(--cf-brand-purple)" }}/>
        <span style={{ fontSize:14, fontWeight:650 }}>POA&M</span>
        <span style={{ fontSize:11.5, color:"var(--cf-text-muted)" }}>Remediation plans for this host's open deficiencies</span>
        <div className="seg" style={{ marginLeft:"auto" }}>
          {[{ v:"open", l:`Open · ${c.open}` }, { v:"overdue", l:`Overdue · ${c.overdue}` }, { v:"closed", l:`Closed · ${c.completed}` }, { v:"all", l:"All" }].map(o => (
            <button key={o.v} className={filter===o.v?"active":""} onClick={()=>setFilter(o.v)}>{o.l}</button>
          ))}
        </div>
      </div>
      {all.length > 0 && <PoamCountStrip list={all}/>}
      <PoamTable list={list} onOpen={openPoamDetail}
        emptyNote={all.length === 0
          ? "No POA&M items for this host. Open a bundle's evidence and create one from a failing control."
          : "Nothing in this view."}/>
    </div>
  );
}

/* ── Bundle roll-up card ──────────────────────────────────────────────────── */
function BundlePoamRollup({ bundle, failCount, onOpenList }) {
  usePoamStore();
  const list = poamsForBundle(bundle);
  const c = poamCounts(list);
  const onPoamFindings = list.filter(p => p.status !== "completed").reduce((a,p)=>a+p.findings.length, 0);
  const noPlan = Math.max(0, failCount - onPoamFindings);
  return (
    <div style={{ padding:"12px 16px", display:"flex", alignItems:"center", gap:14, flexWrap:"wrap" }}>
      <div style={{ display:"flex", alignItems:"center", gap:9, minWidth:0 }}>
        <Icon name="activity" size={14} style={{ color:"var(--cf-brand-purple)" }}/>
        <div>
          <div style={{ fontSize:13, fontWeight:600 }}>POA&M roll-up</div>
          <div style={{ fontSize:11, color:"var(--cf-text-muted)" }}>Which failing findings have a remediation plan — and which do not</div>
        </div>
      </div>
      <div style={{ display:"flex", gap:16, marginLeft:"auto", flexWrap:"wrap", alignItems:"center" }}>
        {[
          { l:"Open findings", v:failCount, c:"#f87171" },
          { l:"On POA&M", v:onPoamFindings, c:"#60a5fa" },
          { l:"No POA&M", v:noPlan, c: noPlan ? "#fbbf24" : "var(--cf-text-muted)" },
          { l:"Overdue", v:c.overdue, c: c.overdue ? "#f87171" : "var(--cf-text-muted)" },
        ].map(s => (
          <div key={s.l} style={{ textAlign:"right" }}>
            <div className="mono" style={{ fontSize:16, fontWeight:700, color:s.c }}>{s.v}</div>
            <div style={{ fontSize:10, color:"var(--cf-text-muted)", textTransform:"uppercase", letterSpacing:"0.05em" }}>{s.l}</div>
          </div>
        ))}
        <button className="btn btn-ghost focus-ring xs" onClick={onOpenList}>
          <Icon name="arrow-right" size={11}/> {list.length} POA&M item{list.length===1?"":"s"}
        </button>
      </div>
    </div>
  );
}

function BundlePoamBody({ bundle, onOpenFinding }) {
  usePoamStore();
  const [filter, setFilter] = React.useState("open");
  const all = poamsForBundle(bundle);
  const list = all.filter(p => filter === "all" ? true : filter === "overdue" ? poamIsOverdue(p) : filter === "awaiting" ? p.status === "awaiting_verification" : filter === "closed" ? p.status === "completed" : p.status !== "completed");
  const c = poamCounts(all);
  return (
    <div style={{ overflow:"auto", flex:1 }}>
      <div style={{ padding:"14px 18px" }}>
        <div style={{ fontSize:13, color:"var(--cf-text-secondary)", lineHeight:1.55 }}>
          Remediation plans covering controls in <strong>{bundle.name}</strong>. A POA&M does not change a control's result — every finding below still evaluates as it did before.
        </div>
      </div>
      <PoamCountStrip list={all}/>
      <div style={{ padding:"12px 18px", borderTop:"1px solid var(--cf-divider)", display:"flex", gap:10, alignItems:"center", flexWrap:"wrap" }}>
        <div className="seg">
          {[{ v:"open", l:`Open · ${c.open}` }, { v:"overdue", l:`Overdue · ${c.overdue}` }, { v:"awaiting", l:`Awaiting verification · ${c.awaiting}` }, { v:"closed", l:`Closed · ${c.completed}` }, { v:"all", l:"All" }].map(o => (
            <button key={o.v} className={filter===o.v?"active":""} onClick={()=>setFilter(o.v)}>{o.l}</button>
          ))}
        </div>
      </div>
      <PoamTable list={list} onOpen={openPoamDetail} emptyNote="No POA&M items in this view."/>
    </div>
  );
}

/* ── Global host: one tray, openable from anywhere ────────────────────────── */
function PoamDetailHost({ onOpenFinding }) {
  usePoamStore();
  const [id, setId] = React.useState(null);
  React.useEffect(() => {
    const h = (e) => setId(e.detail);
    window.addEventListener("cf-poam-open", h);
    return () => window.removeEventListener("cf-poam-open", h);
  }, []);
  const poam = id ? poamById(id) : null;
  if (!poam) return null;
  return <PoamDetailTray poam={poam} onClose={()=>setId(null)} onOpenFinding={(f)=>{ setId(null); onOpenFinding?.(f); }}/>;
}

Object.assign(window, {
  usePoamStore, openPoamDetail, PoamStatusChip, PoamSevChip, FindingPoamBar,
  PoamCreateModal, PoamLinkModal, PoamDetailTray, PoamFindingPicker, PoamCountStrip,
  PoamTable, SystemPoamSection, BundlePoamRollup, BundlePoamBody, PoamDetailHost,
});
