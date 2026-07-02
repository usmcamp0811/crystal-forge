// Deploy gate panel + Approvals + Canary progress — used in System Detail → Deploy tab

/* ── Status chip vocab ── */
const GATE_STATUS = {
  allow:   { label: "Allow",   cls: "chip-healthy",  color: "#34d399", icon: "check" },
  warn:    { label: "Warn",    cls: "chip-warning",  color: "#fbbf24", icon: "warn" },
  block:   { label: "Block",   cls: "chip-critical", color: "#f87171", icon: "x" },
  pending: { label: "Pending", cls: "chip-info",     color: "#60a5fa", icon: "sync" },
};

function GateStatusChip({ status }) {
  const s = GATE_STATUS[status] || GATE_STATUS.pending;
  return (
    <span className={`chip ${s.cls}`} style={{ fontWeight: 600 }}>
      <Icon name={s.icon} size={10}/>
      {s.label}
    </span>
  );
}

/* ── Build a synthetic gate evaluation for a (sys, commit) pair ── */
function evaluateGate(sys, commitSha) {
  const policy = (typeof POLICIES !== "undefined" ? POLICIES : []).find(p => p.name === sys.deploymentPolicy);
  if (!policy) return { policy: null, overall: "allow", rules: [] };

  const seed = (sys.id + ":" + (commitSha || sys.commit)).split("").reduce((a, c) => a + c.charCodeAt(0), 0);
  const rand = (k) => ((seed * (k + 1) * 9301) % 233280) / 233280;

  const rules = policy.rules.map((r, i) => {
    const roll = rand(i);
    let status = "allow";
    let reason = "";
    let next = "";

    switch (r.kind) {
      case "eval_passed":
        status = roll < 0.85 ? "allow" : roll < 0.95 ? "warn" : "block";
        reason = status === "allow" ? "Evaluation succeeded for all assigned systems."
               : status === "warn" ? "Evaluation succeeded with deprecation warnings."
               : "Evaluation failed — see eval logs.";
        if (status === "block") next = "Re-evaluate or pick a passing commit.";
        break;
      case "build_succeeded":
        status = roll < 0.8 ? "allow" : roll < 0.92 ? "pending" : "block";
        reason = status === "allow" ? "All derivations built and pushed to cache."
               : status === "pending" ? "12 / 18 derivations built. Waiting for cache push."
               : "1 derivation failed: nginx-1.27.4.";
        if (status === "block") next = "Inspect failed build for nginx-1.27.4.";
        if (status === "pending") next = "Build will complete in ~3 min.";
        break;
      case "cve_block": {
        const cves = sys.cves || { critical: 0, high: 0 };
        const v = r.severity === "critical" ? cves.critical : cves.high;
        if (v <= r.maxAllowed) {
          status = "allow";
          reason = `Found ${v} ${r.severity} CVE${v === 1 ? "" : "s"} (max ${r.maxAllowed}).`;
        } else {
          status = "block";
          reason = `${v} ${r.severity} CVE${v === 1 ? "" : "s"} exceeds limit of ${r.maxAllowed}.`;
          next = `Patch ${r.severity} CVEs or raise the limit.`;
        }
        break;
      }
      case "time_window": {
        const inWindow = roll > 0.4;
        status = inWindow ? "allow" : (r.outsideAction === "warn" ? "warn" : "block");
        reason = inWindow ? `Inside deploy window (${r.from}–${r.to} ${r.tz}).`
                          : `Outside deploy window (${r.from}–${r.to} ${r.tz}).`;
        if (!inWindow) next = "Wait until window opens or override.";
        break;
      }
      case "approval_required": {
        const have = Math.max(0, Math.floor(roll * (r.count + 1)));
        if (have >= r.count) {
          status = "allow";
          reason = `${have} / ${r.count} ${r.role} approvals received.`;
        } else {
          status = "pending";
          reason = `${have} / ${r.count} ${r.role} approvals received.`;
          next = `${r.count - have} more approval${r.count - have === 1 ? "" : "s"} needed.`;
        }
        break;
      }
      case "rollout_percent":
        status = "pending";
        reason = `Canary rollout active — phase 1 of ${Math.ceil(100 / r.percent)}.`;
        next = `${r.observeMin}min observation in progress.`;
        break;
      case "pin_required":
        status = (commitSha === sys.commit) ? "allow" : "block";
        reason = status === "allow" ? "Commit matches the pinned target."
                                    : `System is pinned to ${sys.commit}.`;
        if (status === "block") next = "Change pin in system config or unpin.";
        break;
      default:
        status = "allow";
        reason = "Rule satisfied.";
    }
    return { rule: r, status, reason, next };
  });

  const overall = rules.some(r => r.status === "block") ? "block"
                : rules.some(r => r.status === "pending") ? "pending"
                : rules.some(r => r.status === "warn") ? "warn"
                : "allow";

  // Synth approval data when relevant
  const approvalRule = policy.rules.find(r => r.kind === "approval_required");
  let approvalState = null;
  if (approvalRule) {
    const have = Math.max(0, Math.floor(rand(99) * (approvalRule.count + 1)));
    approvalState = {
      have,
      need: approvalRule.count,
      role: approvalRule.role,
      distinct: approvalRule.distinct !== false,
      expiresIn: `${4 + (seed % 8)}h`,
      approvers: Array.from({ length: have }, (_, j) => ({
        user: ["mreyes", "jpark", "dchen", "kthomas"][j % 4],
        at: `${(j + 1) * 7}m ago`,
        comment: ["LGTM", "Approved — verified eval logs", "Verified", "OK"][j % 4],
      })),
    };
  }

  // Synth canary data when relevant
  const canaryRule = policy.rules.find(r => r.kind === "rollout_percent");
  let canaryState = null;
  if (canaryRule) {
    const total = Math.ceil(100 / canaryRule.percent);
    const phase = 1 + Math.floor(rand(33) * (total - 1));
    canaryState = {
      phase, total,
      percent: canaryRule.percent,
      observeMin: canaryRule.observeMin,
      observeUntil: "14:30 UTC",
      inPhase: 6,
      completed: phase * Math.max(1, Math.floor(SYSTEMS.length / total / 2)),
      failed: rand(11) < 0.15 ? 1 : 0,
      halted: false,
    };
  }

  return { policy, overall, rules, approvalState, canaryState };
}

/* ── Deploy Gate Panel ──
   Drop into the Deploy tab so the operator sees policy decisions before clicking Deploy. */
function DeployGatePanel({ sys, commitSha, userRole = "operator" }) {
  const evalResult = evaluateGate(sys, commitSha);
  if (!evalResult.policy) return null;

  return (
    <section className="card sd-card" style={{ display:"flex", flexDirection:"column", gap:14 }}>
      <div className="sd-card-head">
        <div style={{ display:"flex", alignItems:"center", gap:10 }}>
          <h2>Deploy gate</h2>
          <GateStatusChip status={evalResult.overall}/>
        </div>
        <span className="sd-card-meta">policy: <span className="mono">{evalResult.policy.name}</span></span>
      </div>

      {evalResult.overall === "block" && (
        <div className="sd-callout sd-callout-danger">
          <Icon name="x" size={13}/>
          <div style={{ fontSize:12 }}><strong>Deployment blocked by policy.</strong> Resolve the blocking rules below before proceeding.</div>
        </div>
      )}
      {evalResult.overall === "warn" && (
        <div className="sd-callout sd-callout-warn">
          <Icon name="warn" size={13}/>
          <div style={{ fontSize:12 }}><strong>Deployment allowed with warning.</strong> Review warnings before continuing.</div>
        </div>
      )}
      {evalResult.overall === "pending" && (
        <div className="sd-callout sd-callout-info">
          <Icon name="sync" size={13}/>
          <div style={{ fontSize:12 }}><strong>Waiting on policy gates.</strong> See the cards below for required next actions.</div>
        </div>
      )}

      <div style={{ display:"grid", gridTemplateColumns:"repeat(auto-fill, minmax(280px,1fr))", gap:10 }}>
        {evalResult.rules.map((r, i) => (
          <div key={i} style={{
            padding:"12px 14px",
            border:"1px solid var(--cf-divider)",
            borderRadius:10,
            background: "var(--cf-card-bg)",
            display:"flex", flexDirection:"column", gap:8,
          }}>
            <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:8 }}>
              <span style={{ fontSize:12, fontWeight:600, color:"var(--cf-text-primary)" }}>{ruleDescription(r.rule)}</span>
              <GateStatusChip status={r.status}/>
            </div>
            <div style={{ fontSize:11, color:"var(--cf-text-secondary)", lineHeight:1.5 }}>{r.reason}</div>
            {r.next && (
              <div style={{ fontSize:11, color:"var(--cf-text-muted)", borderTop:"1px solid var(--cf-divider)", paddingTop:6, marginTop:2 }}>
                <strong style={{ color:"var(--cf-text-secondary)" }}>Next:</strong> {r.next}
              </div>
            )}
          </div>
        ))}
      </div>

      {evalResult.approvalState && (
        <ApprovalsWorkspace approvalState={evalResult.approvalState} userRole={userRole}/>
      )}
      {evalResult.canaryState && (
        <CanaryProgress canaryState={evalResult.canaryState}/>
      )}
    </section>
  );
}

/* ── Approvals Workspace ── */
function ApprovalsWorkspace({ approvalState, userRole }) {
  const { have, need, role, distinct, expiresIn, approvers } = approvalState;
  const pct = need > 0 ? (have / need) * 100 : 0;
  const authorized = (role === "admin" && userRole === "admin")
                  || (role === "operator" && (userRole === "operator" || userRole === "admin"))
                  || (role === "any");
  const alreadyApproved = approvers.some(a => a.user === "mreyes"); // current user shim

  return (
    <div style={{
      padding:16,
      border:"1px solid var(--cf-divider)",
      borderRadius:10,
      background: "color-mix(in oklab, var(--cf-page-bg) 50%, var(--cf-card-bg))",
      display:"flex", flexDirection:"column", gap:14,
    }}>
      <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:10, flexWrap:"wrap" }}>
        <div style={{ display:"flex", alignItems:"center", gap:10 }}>
          <h3 style={{ margin:0, fontSize:13, fontWeight:600 }}>Approvals</h3>
          <span className="chip chip-unknown" style={{ fontSize:10 }}>role: {role}</span>
          {distinct && <span className="chip chip-unknown" style={{ fontSize:10 }}>distinct approvers</span>}
          <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>expires in {expiresIn}</span>
        </div>
        <span className="mono" style={{ fontSize:13, fontWeight:600 }}>
          <span style={{ color: have >= need ? "#34d399" : "#fbbf24" }}>{have}</span>
          <span style={{ color:"var(--cf-text-muted)" }}> / {need}</span>
        </span>
      </div>

      <div style={{ height:6, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
        <div style={{ width:`${pct}%`, height:"100%", background: have >= need ? "#34d399" : "#fbbf24", transition:"width 200ms" }}/>
      </div>

      {approvers.length > 0 && (
        <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
          {approvers.map((a, i) => (
            <div key={i} style={{ display:"grid", gridTemplateColumns:"110px 1fr 80px", gap:10, fontSize:12, alignItems:"center", padding:"6px 10px", background:"var(--cf-card-bg)", borderRadius:6 }}>
              <span className="mono" style={{ fontWeight:600 }}>{a.user}</span>
              <span style={{ color:"var(--cf-text-secondary)", overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{a.comment}</span>
              <span style={{ fontSize:11, color:"var(--cf-text-muted)", textAlign:"right" }}>{a.at}</span>
            </div>
          ))}
        </div>
      )}

      <div style={{ display:"flex", alignItems:"center", justifyContent:"flex-end", gap:8 }}>
        {!authorized && (
          <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>
            Your role <span className="mono">{userRole}</span> does not permit this approval.
          </span>
        )}
        {alreadyApproved && (
          <span style={{ fontSize:11, color:"#34d399" }}>
            <Icon name="check" size={10} style={{ verticalAlign:"middle" }}/> You have approved.
          </span>
        )}
        <button
          className={`btn ${authorized && !alreadyApproved ? "btn-primary" : "btn-ghost"} focus-ring`}
          disabled={!authorized || alreadyApproved || have >= need}
          style={(!authorized || alreadyApproved || have >= need) ? { opacity:0.5, cursor:"not-allowed" } : null}
          title={!authorized ? "Your role does not permit this approval"
                : alreadyApproved ? "You have already approved"
                : have >= need ? "Approval threshold met"
                : "Submit approval"}>
          <Icon name="check" size={13}/> Approve
        </button>
      </div>
    </div>
  );
}

/* ── Canary Rollout Progress ── */
function CanaryProgress({ canaryState }) {
  const { phase, total, percent, observeMin, observeUntil, inPhase, completed, failed, halted } = canaryState;

  return (
    <div style={{
      padding:16,
      border:"1px solid var(--cf-divider)",
      borderRadius:10,
      background: "color-mix(in oklab, var(--cf-page-bg) 50%, var(--cf-card-bg))",
      display:"flex", flexDirection:"column", gap:12,
    }}>
      <div style={{ display:"flex", justifyContent:"space-between", alignItems:"center", gap:10, flexWrap:"wrap" }}>
        <div style={{ display:"flex", alignItems:"center", gap:10 }}>
          <h3 style={{ margin:0, fontSize:13, fontWeight:600 }}>Canary rollout</h3>
          <span className="chip chip-info">phase {phase} of {total}</span>
          <span className="chip chip-unknown" style={{ fontSize:10 }}>{percent}% per phase</span>
        </div>
        {halted ? (
          <span className="chip chip-critical">halted</span>
        ) : (
          <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>
            <Icon name="sync" size={10} style={{ verticalAlign:"middle" }}/> Observation in progress until {observeUntil}
          </span>
        )}
      </div>

      {/* Phase progress bar */}
      <div style={{ display:"flex", gap:4 }}>
        {Array.from({ length: total }).map((_, i) => {
          const isDone = i < phase - 1;
          const isCurrent = i === phase - 1;
          return (
            <div key={i} style={{
              flex:1, height:8, borderRadius:4,
              background: isDone ? "#34d399"
                       : isCurrent ? "#60a5fa"
                       : "var(--cf-subtle-bg)",
              opacity: isCurrent ? 0.9 : 1,
              animation: isCurrent ? "pulse 1.6s ease-in-out infinite" : "none",
            }}/>
          );
        })}
      </div>

      {/* Counts */}
      <div style={{ display:"grid", gridTemplateColumns:"repeat(4, 1fr)", gap:8 }}>
        {[
          { label:"In phase",  val:inPhase,   color:"#60a5fa" },
          { label:"Completed", val:completed, color:"#34d399" },
          { label:"Failed",    val:failed,    color: failed > 0 ? "#f87171" : "var(--cf-text-muted)" },
          { label:"Remaining", val:Math.max(0, (SYSTEMS?.length || 30) - completed - inPhase), color:"#9ca3af" },
        ].map(c => (
          <div key={c.label} style={{
            padding:"8px 10px", borderRadius:6, background:"var(--cf-card-bg)",
            display:"flex", flexDirection:"column", gap:2,
          }}>
            <span style={{ fontSize:10, color:"var(--cf-text-muted)", textTransform:"uppercase", letterSpacing:"0.06em" }}>{c.label}</span>
            <span className="mono" style={{ fontSize:18, fontWeight:700, color: c.color, fontVariantNumeric:"tabular-nums" }}>{c.val}</span>
          </div>
        ))}
      </div>

      {/* Observation countdown */}
      <div style={{ padding:"8px 10px", background:"var(--cf-card-bg)", borderRadius:6, display:"flex", alignItems:"center", gap:10 }}>
        <Icon name="history" size={13} style={{ color:"#60a5fa" }}/>
        <span style={{ fontSize:12, color:"var(--cf-text-primary)" }}>Observing phase {phase} for {observeMin} min — until {observeUntil}</span>
        {failed > 0 && (
          <span className="chip chip-warning" style={{ marginLeft:"auto", fontSize:10 }}>1 failure — review before next phase</span>
        )}
      </div>
    </div>
  );
}

Object.assign(window, { DeployGatePanel, ApprovalsWorkspace, CanaryProgress, GateStatusChip, evaluateGate, GATE_STATUS });
