// Systems view + chips + cards + table + side panel + deploy modal

const ENV_STYLE = {
  production: { bg: "rgba(220,38,38,0.10)",  fg: "#f87171", border: "rgba(248,113,113,0.25)" },
  staging:    { bg: "rgba(217,119,6,0.10)",  fg: "#fbbf24", border: "rgba(251,191,36,0.25)" },
  dev:        { bg: "rgba(37,99,235,0.10)",  fg: "#60a5fa", border: "rgba(96,165,250,0.25)" },
  edge:       { bg: "rgba(15,118,110,0.12)", fg: "#2dd4bf", border: "rgba(45,212,191,0.25)" },
  lab:        { bg: "rgba(124,58,237,0.10)", fg: "#a78bfa", border: "rgba(167,139,250,0.25)" },
};

function envVars(env) {
  const s = ENV_STYLE[env] || ENV_STYLE.dev;
  return { "--env-bg": s.bg, "--env-fg": s.fg, "--env-border": s.border };
}

// HeartbeatSpinner: ring that drains over the expected interval. Goes amber when past
// due (0..interval sec late), red past 2x interval. `size` is diameter in px.
// `deployStage`/`deployStartedAt` let a live deploy borrow this same ring/label instead
// of swapping in a different widget. Each stage has a known simulated duration (the
// agent's own pull cadence — not the fleet-wide heartbeat countdown), so we drain the
// ring across that stage and count down to the next one, same as the idle countdown.
const DEPLOY_STAGE_END_MS   = { queued: 15000, "picked-up": 17200, applying: 20600, activated: 24300 };
const DEPLOY_STAGE_START_MS = { queued: 0,     "picked-up": 15000, applying: 17200, activated: 20600 };
function HeartbeatSpinner({ intervalSec, nextInSec, size = 36, showLabel = true, deployStage, deployStartedAt }) {
  const [now, setNow] = React.useState(() => Date.now());
  const mountRef = React.useRef(Date.now());
  React.useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  // simulate real-time: next-in drops by the elapsed wall-clock since mount.
  const elapsedSince = (now - mountRef.current) / 1000;
  const remaining = nextInSec - elapsedSince;
  const lateBy = -remaining; // positive when overdue
  // progress 0..1 from last heartbeat -> next expected
  const sinceLast = intervalSec - remaining;
  const progress = Math.max(0, Math.min(1, sinceLast / intervalSec));
  // state
  const overdue = remaining < 0;
  const critical = lateBy > intervalSec; // past 2x interval

  const activated = deployStage === "activated";
  const applyingPhase = deployStage === "applying"; // unknown real-world duration — the
  // agent's own heartbeats keep flowing during this phase, so the ring reverts to the
  // normal heartbeat countdown and we just track elapsed time on the side.
  const deploying = deployStage && !activated && !applyingPhase;

  // Countdown within the current deploy stage (queued / picked-up)
  let stageRemainSec = 0, stageProgress = 0;
  if (deploying && deployStartedAt) {
    const stageStart = DEPLOY_STAGE_START_MS[deployStage] ?? 0;
    const stageEnd = DEPLOY_STAGE_END_MS[deployStage] ?? stageStart;
    const stageDurMs = Math.max(1, stageEnd - stageStart);
    const elapsedMs = (now - deployStartedAt) - stageStart;
    stageProgress = Math.max(0, Math.min(1, elapsedMs / stageDurMs));
    stageRemainSec = Math.max(0, (stageDurMs - elapsedMs) / 1000);
  }
  // Elapsed time counts UP through applying, since we don't know when it'll finish.
  let applyingElapsedSec = 0;
  if (applyingPhase && deployStartedAt) {
    applyingElapsedSec = Math.max(0, ((now - deployStartedAt) - DEPLOY_STAGE_START_MS.applying) / 1000);
  }

  const color = activated ? "#34d399"
    : deploying ? "var(--cf-brand-purple)"
    : critical ? "#f87171" : overdue ? "#fbbf24" : "#34d399";
  const trackColor = "rgba(148,163,184,0.18)";

  // SVG ring
  const stroke = Math.max(2, Math.round(size / 12));
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  // Ring drains from full to empty as we count down (to the next heartbeat, or the
  // next deploy stage), then flips to a solid, flashing ring once overdue.
  const dashOffset = activated ? 0 : deploying ? c * stageProgress : (overdue ? 0 : c * progress);

  // label
  const fmt = (s) => {
    const a = Math.abs(Math.round(s));
    if (a < 60) return `${a}s`;
    if (a < 3600) return `${Math.floor(a / 60)}m ${a % 60}s`;
    return `${Math.floor(a / 3600)}h ${Math.floor((a % 3600) / 60)}m`;
  };
  const label = activated ? "activated"
    : deployStage === "queued" ? `picks up in ${fmt(stageRemainSec)}`
    : deployStage === "picked-up" ? `applying in ${fmt(stageRemainSec)}`
    : overdue ? `${fmt(lateBy)} late` : `next in ${fmt(remaining)}`;
  const sub = activated ? "generation live"
    : applyingPhase ? `applying · ${fmt(applyingElapsedSec)} elapsed`
    : deploying ? "deploy in progress" : `every ${fmt(intervalSec)}`;

  return (
    <div className="hb-spinner" title={`Heartbeat ${label} · ${sub}`}>
      <div className={`hb-ring ${overdue ? "hb-overdue" : ""} ${critical ? "hb-critical" : ""}`} style={{ width: size, height: size }}>
        <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
          <circle cx={size/2} cy={size/2} r={r} stroke={trackColor} strokeWidth={stroke} fill="none" />
          <circle
            cx={size/2} cy={size/2} r={r}
            stroke={color} strokeWidth={stroke} fill="none"
            strokeLinecap="round"
            strokeDasharray={c}
            strokeDashoffset={dashOffset}
            transform={`rotate(-90 ${size/2} ${size/2})`}
            style={{ transition: (overdue || deploying) ? "none" : "stroke-dashoffset 0.8s linear, stroke 0.3s" }}
          />
        </svg>
        <span className="hb-pulse" style={{ background: color }} />
      </div>
      {showLabel && (
        <div className="hb-label">
          <div className="hb-label-main" style={{ color }}>{label}</div>
          <div className="hb-label-sub">{sub}</div>
        </div>
      )}
    </div>
  );
}

function EnvBadge({ env }) {
  return (
    <span className="env-badge" style={envVars(env)}>
      <span className="chip-dot" />
      {env}
    </span>
  );
}

function StatusChip({ sys }) {
  return (
    <span className={`chip ${sys.statusChip}`}>
      <span className="chip-dot" style={{ background: sys.statusColor }} />
      {sys.status}
    </span>
  );
}

function CveChips({ cves, compact }) {
  const parts = [];
  if (cves.critical > 0) parts.push(<span key="c" className="chip chip-critical">{cves.critical} crit</span>);
  if (cves.high > 0)     parts.push(<span key="h" className="chip chip-warning">{cves.high} high</span>);
  if (!compact && cves.medium > 0) parts.push(<span key="m" className="chip chip-unknown">{cves.medium} med</span>);
  if (parts.length === 0) parts.push(<span key="ok" className="chip chip-healthy"><Icon name="check" size={10} /> clean</span>);
  return <>{parts}</>;
}

function DeploymentChip({ state }) {
  const map = {
    "up-to-date": ["chip-healthy", "up to date"],
    "behind":     ["chip-warning", "behind"],
    "failed":     ["chip-critical", "deploy failed"],
    "drift":      ["chip-warning", "drift"],
    "deploying":  ["chip-info", "deploying"],
    "unknown":    ["chip-unknown", "unknown"],
  };
  const [cls, label] = map[state] || map.unknown;
  return <span className={`chip ${cls}`}>{label}</span>;
}

// Deploy modal
function DeployModal({ sys, onClose, onQueued }) {
  const [branch, setBranch] = React.useState(sys.branch);
  const [commit, setCommit] = React.useState(sys.commit);
  const [posting, setPosting] = React.useState(false);

  const mockCommits = React.useMemo(() => [
    { sha: sys.commit, msg: sys.commitMessage, rel: "2h ago", current: true },
    { sha: "9a2b1c4f", msg: "bump nixpkgs to 24.11", rel: "8h ago" },
    { sha: "7eef30a1", msg: "harden sshd: disable password auth", rel: "1d ago" },
    { sha: "4c9b11a7", msg: "add grafana exporter to host", rel: "2d ago" },
  ], [sys]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
        <div className="modal-head">
          <h2>Deploy to {sys.hostname}</h2>
          <p>Select a commit from <span className="mono">{sys.flake}</span> to deploy.</p>
        </div>
        <div className="modal-body">
          <div className="field">
            <label>Flake</label>
            <select className="input focus-ring" defaultValue={sys.flake}>
              {FLAKES.map(f => <option key={f}>{f}</option>)}
            </select>
          </div>
          <div className="field">
            <label>Branch</label>
            <select className="input focus-ring" value={branch} onChange={e => setBranch(e.target.value)}>
              <option value="main">main</option>
              <option value="staging">staging</option>
              <option value="dev">dev</option>
            </select>
          </div>
          <div className="field">
            <label>Commit</label>
            <div style={{ display: "flex", flexDirection: "column", gap: 6, maxHeight: 220, overflow: "auto", border: "1px solid var(--cf-card-border)", borderRadius: 10, padding: 6 }}>
              {mockCommits.map(c => (
                <label key={c.sha} style={{
                  display: "flex", alignItems: "center", gap: 10,
                  padding: "8px 10px",
                  borderRadius: 8, cursor: "pointer",
                  background: commit === c.sha ? "var(--cf-subtle-bg)" : "transparent",
                }}>
                  <input type="radio" name="commit" checked={commit === c.sha} onChange={() => setCommit(c.sha)} />
                  <span className="mono" style={{ fontSize: 12, color: "var(--cf-text-primary)" }}>{c.sha}</span>
                  <span style={{ fontSize: 12, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{c.msg}</span>
                  <span style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{c.rel}</span>
                  {c.current && <span className="chip chip-info" style={{ fontSize: 10 }}>current</span>}
                </label>
              ))}
            </div>
            <div className="help">Diff preview will be available after evaluation.</div>
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" onClick={() => { setPosting(true); setTimeout(() => { onQueued?.(commit); onClose(); }, 800); }}>
            {posting ? <><Spinner size={12} /> Queueing…</> : <><Icon name="deploy" size={12} /> Deploy commit</>}
          </button>
        </div>
      </div>
    </div>
  );
}

// Side panel — system detail peek
function SystemPanel({ sys, onClose, onEdit, onOpenDetail, onTagClick, pendingDeploy, onClearPending }) {
  const deployStage = useDeployStages(pendingDeploy, onClearPending);
  return (
    <>
      <div className="side-panel-backdrop" onClick={onClose} />
      <aside className="side-panel" role="dialog" aria-modal="true">
        <div className="panel-head">
          <div className="panel-title">
            <h2>
              <span className="status-dot" style={{ "--status-color": sys.statusColor }} />
              {sys.hostname}
              <StatusChip sys={sys} />
            </h2>
            <span className="fqdn">{sys.fqdn}</span>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close">
            <Icon name="x" size={16} />
          </button>
        </div>
        <div className="panel-body">
          {deployStage && pendingDeploy && (
            <section className="panel-section" style={{ paddingTop: 0 }}>
              <PendingDeployBanner
                stage={deployStage}
                stages={["queued", "picked-up", "copying", "applying", "activated"]}
                commit={pendingDeploy?.commit}
                sys={sys}
                onDismiss={onClearPending}
                onViewLogs={() => onOpenDetail?.(sys)}
              />
            </section>
          )}
          <section className="panel-section">
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              <EnvBadge env={sys.environment} />
              <DeploymentChip state={sys.deploymentState} />
              <span className="chip chip-unknown">policy: {sys.deploymentPolicy}</span>
              {sys.tags.map(t => (
                <button key={t} className="chip chip-unknown sys-tag-chip focus-ring" title={`Filter fleet by #${t}`}
                  onClick={() => onTagClick?.(t)}>#{t}</button>
              ))}
            </div>
          </section>

          <section className="panel-section">
            <h3>Currently deployed</h3>
            <dl className="kv-grid">
              <dt>Flake</dt><dd>{sys.flake}</dd>
              <dt>Branch</dt><dd>{sys.branch}</dd>
              <dt>Commit</dt><dd>{sys.commit}</dd>
              <dt>Message</dt><dd style={{ whiteSpace: "normal", fontFamily: "var(--font-sans)" }}>{sys.commitMessage}</dd>
              <dt>Generation</dt><dd>#{sys.generation}</dd>
              <dt>NixOS</dt><dd>{sys.nixosVersion}</dd>
              <dt>Kernel</dt><dd>{sys.kernel}</dd>
            </dl>
          </section>

          <section className="panel-section">
            <h3>Host</h3>
            <dl className="kv-grid">
              <dt>Uptime</dt><dd>{sys.uptime}</dd>
              <dt>CPU</dt><dd>{sys.cpu}</dd>
              <dt>Memory</dt><dd>{sys.memGb} GiB</dd>
              <dt>IPv4</dt><dd title={sys.ipv4}>{sys.ipv4}</dd>
              <dt>IPv6</dt><dd title={sys.ipv6}>{sys.ipv6}</dd>
              <dt>Last heartbeat</dt><dd>{sys.lastHeartbeat}</dd>
            </dl>
            <div className="hb-panel">
              <HeartbeatSpinner intervalSec={sys.heartbeatIntervalSec} nextInSec={sys.heartbeatNextInSec} size={56} />
            </div>
          </section>

          <section className="panel-section">
            <h3>CVE exposure</h3>
            <CveBar cves={sys.cves} />
            <div className="cve-legend">
              <span className="cve-legend-item"><span className="cve-legend-swatch" style={{ background: "#f87171" }} />{sys.cves.critical} critical</span>
              <span className="cve-legend-item"><span className="cve-legend-swatch" style={{ background: "#fbbf24" }} />{sys.cves.high} high</span>
              <span className="cve-legend-item"><span className="cve-legend-swatch" style={{ background: "#9ca3af" }} />{sys.cves.medium} medium</span>
              <span className="cve-legend-item"><span className="cve-legend-swatch" style={{ background: "#4b5563" }} />{sys.cves.low} low</span>
            </div>
          </section>

          <section className="panel-section">
            <h3>Recent activity</h3>
            <div className="timeline">
              {(typeof buildActivityFeed === "function" ? buildActivityFeed(sys, deployStage, pendingDeploy?.commit) : sys.events).map((e, i) => (
                <div key={i} className={`tl-item${e.live ? " tl-item-live" : ""}`}>
                  <span className="tl-dot" style={{ "--status-color": e.color }}>
                    {e.live ? <span className="tl-dot-pulse" /> : null}
                  </span>
                  <div className="tl-body">
                    <div className="tl-title" style={{ display:"flex", alignItems:"center", gap:6 }}>
                      {e.icon && <Icon name={e.icon} size={12} style={{ color:e.color, flexShrink:0 }} />}
                      <span>{e.title}</span>
                      {e.sub && <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)", overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>· {e.sub}</span>}
                    </div>
                    <div className="tl-meta">{e.at}</div>
                  </div>
                </div>
              ))}
            </div>
          </section>
        </div>
        <div className="panel-actions">
          <button className="btn btn-ghost focus-ring" onClick={() => onOpenDetail?.(sys)}><Icon name="arrow-right" size={12} /> Open full detail</button>
          <button className="btn btn-ghost focus-ring" onClick={() => onEdit?.(sys)}><Icon name="gear" size={12} /> Edit</button>
          <button className="btn btn-primary focus-ring" onClick={() => onOpenDetail?.(sys, "deploy")}><Icon name="deploy" size={12} /> Deploy</button>
        </div>
      </aside>
    </>
  );
}

function CveBar({ cves }) {
  const total = Math.max(cves.total, 1);
  const seg = [
    ["#f87171", cves.critical],
    ["#fbbf24", cves.high],
    ["#9ca3af", cves.medium],
    ["#4b5563", cves.low],
  ];
  return (
    <div className="cve-bar">
      {seg.map(([c, v], i) => v > 0 && (
        <div key={i} className="cve-seg" style={{ background: c, width: `${(v / total) * 100}%` }} />
      ))}
    </div>
  );
}

// System card
function SystemCard({ sys, compact, flash, pendingApproval, onOpen, onDeploy, onEdit }) {
  return (
    <div className={`sys-card${flash ? " attention-flash" : ""}`} style={compact ? { padding: 12, gap: 8 } : null} onClick={() => onOpen(sys)}>
      <div className="status-rail" style={{ "--status-color": sys.statusColor }} />
      <div className="sys-card-head">
        <div className="sys-title">
          <div className="sys-hostname">
            <span className="status-dot" style={{ "--status-color": sys.statusColor }} />
            {sys.hostname}
          </div>
          <div className="sys-fqdn">{sys.fqdn}</div>
        </div>
        <EnvBadge env={sys.environment} />
      </div>

      {!compact && (
        <div className="sys-card-body">
          <div>
            <div className="sys-kv-key">Flake · branch</div>
            <div className="sys-kv-val">{sys.flake} · {sys.branch}</div>
          </div>
          <div>
            <div className="sys-kv-key">Commit</div>
            <div className="sys-kv-val">{sys.commit}</div>
          </div>
          <div>
            <div className="sys-kv-key">Heartbeat</div>
            <div className="sys-kv-val" style={{ fontFamily: "var(--font-sans)", display: "flex", alignItems: "center", gap: 8 }}>
              <HeartbeatSpinner intervalSec={sys.heartbeatIntervalSec} nextInSec={sys.heartbeatNextInSec} size={22} showLabel={false} />
              <span>{sys.lastHeartbeat}</span>
            </div>
          </div>
          <div>
            <div className="sys-kv-key">Policy</div>
            <div className="sys-kv-val" style={{ fontFamily: "var(--font-sans)" }}>{sys.deploymentPolicy}</div>
          </div>
        </div>
      )}

      {compact && (
        <div style={{ display: "flex", gap: 12, fontSize: 12, color: "var(--cf-text-secondary)", flexWrap: "wrap" }}>
          <span className="mono" style={{ color: "var(--cf-text-primary)" }}>{sys.flake}</span>
          <span className="mono">{sys.commit}</span>
          <span>{sys.lastHeartbeat}</span>
        </div>
      )}

      <div className="sys-card-foot">
        <div className="chips-row">
          <StatusChip sys={sys} />
          <DeploymentChip state={sys.deploymentState} />
          <CveChips cves={sys.cves} compact />
          {pendingApproval && (
            <span className="chip chip-warning" style={{ cursor:"pointer" }} title="Deploy awaiting approval" onClick={(e)=>{ e.stopPropagation(); onDeploy(sys); }}>
              <Icon name="deploy" size={10}/> awaiting approval
            </span>
          )}
        </div>
        <button
          className="btn btn-subtle focus-ring"
          style={{ padding: "4px 10px", fontSize: 12 }}
          onClick={(e) => { e.stopPropagation(); onDeploy(sys); }}
        >
          <Icon name="deploy" size={12} /> Deploy
        </button>
      </div>
    </div>
  );
}

// Table row
function SystemRow({ sys, compact, selected, flash, pendingApproval, onOpen, onDeploy, onEdit }) {
  return (
    <tr className={`${selected ? "selected" : ""}${flash ? " attention-flash" : ""}`} onClick={() => onOpen(sys)}>
      <td>
        <div className="sys-host-cell">
          <span className="status-dot" style={{ "--status-color": sys.statusColor }} />
          <div style={{ minWidth: 0 }}>
            <div className="hostname">{sys.hostname}</div>
            <div className="fqdn truncate">{sys.fqdn}</div>
          </div>
        </div>
      </td>
      <td><EnvBadge env={sys.environment} /></td>
      <td>
        <div style={{ display:"flex", gap:5, flexWrap:"wrap", alignItems:"center" }}>
          <StatusChip sys={sys} />
          {pendingApproval && (
            <span className="chip chip-warning" style={{ cursor:"pointer" }} title="Deploy awaiting approval" onClick={(e)=>{ e.stopPropagation(); onDeploy(sys); }}>
              <Icon name="deploy" size={10}/> approval
            </span>
          )}
        </div>
      </td>
      <td>
        <div style={{ display: "flex", flexDirection: "column", lineHeight: 1.3 }}>
          <span className="mono" style={{ fontSize: 12 }}>{sys.flake}</span>
          <span className="mono" style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{sys.commit} · {sys.branch}</span>
        </div>
      </td>
      <td><DeploymentChip state={sys.deploymentState} /></td>
      <td>
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
          <CveChips cves={sys.cves} compact />
        </div>
      </td>
      <td style={{ color: "var(--cf-text-secondary)", fontSize: 12 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <HeartbeatSpinner intervalSec={sys.heartbeatIntervalSec} nextInSec={sys.heartbeatNextInSec} size={20} showLabel={false} />
          <span>{sys.lastHeartbeat}</span>
        </div>
      </td>
      <td>
        <div className="row-actions">
          <button className="btn-icon focus-ring" title="Deploy" onClick={(e) => { e.stopPropagation(); onDeploy(sys); }}>
            <Icon name="deploy" size={14} />
          </button>
          <button className="btn-icon focus-ring" title="Edit" onClick={(e) => { e.stopPropagation(); onEdit?.(sys); }}>
            <Icon name="gear" size={14} />
          </button>
        </div>
      </td>
    </tr>
  );
}

Object.assign(window, {
  EnvBadge, StatusChip, CveChips, DeploymentChip, HeartbeatSpinner, CveBar,
  SystemCard, SystemRow, SystemPanel, DeployModal,
  ENV_STYLE,
});
