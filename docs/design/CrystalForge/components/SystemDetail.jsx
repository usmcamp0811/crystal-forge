// Full System Detail view — Overview / Deploy / History / Logs tabs

// Deployment-command progress banner. Deployments are pull-based: the server queues the
// command and the agent picks it up on its next heartbeat, so the operator needs to know
// they're waiting on the agent — not on a stuck server.
function PendingDeployBanner({ stage, stages, commit, sys, kind, gen, onDismiss, onViewLogs }) {
  const idx = stages.indexOf(stage);
  const done = stage === "activated";
  const isRollback = kind === "rollback";
  const targetGen = isRollback ? gen : sys.generation + 1;
  const stepMeta = {
    "queued":    { label: "Queued", sub: `Waiting for ${sys.hostname} agent to check in (heartbeat every ${sys.heartbeatIntervalSec}s)` },
    "picked-up": { label: "Picked up", sub: `Agent fetched the ${isRollback ? "rollback" : "deployment"} command` },
    "applying":  { label: isRollback ? "Reverting" : "Applying", sub: isRollback ? `Switching to generation #${gen} (${commit})` : `Building & switching to ${commit}` },
    "activated": { label: "Activated", sub: `Generation #${targetGen} is live` },
  };
  const cur = stepMeta[stage] || {};
  const verb = isRollback ? "Rollback" : "Deployment";
  return (
    <div className={`deploy-pending${done ? " done" : ""}${isRollback ? " rollback" : ""}`}>
      <div className="deploy-pending-main">
        <div className="deploy-pending-icon">
          {done ? <Icon name="check" size={16} /> : <Spinner size={16} />}
        </div>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div className="deploy-pending-title">
            {done ? `${verb} complete` : `${verb} in progress`}
            <span className="mono deploy-pending-commit">{isRollback ? `#${gen} · ` : ""}{commit}</span>
          </div>
          <div className="deploy-pending-sub">{cur.sub}</div>
        </div>
        <button className="btn btn-ghost xs focus-ring" onClick={onViewLogs}><Icon name="terminal" size={12} /> Logs</button>
        {done && <button className="btn-icon focus-ring" onClick={onDismiss} aria-label="Dismiss"><Icon name="x" size={14} /></button>}
      </div>
      <div className="deploy-steps">
        {stages.map((s, i) => (
          <div key={s} className={`deploy-step${i < idx ? " past" : ""}${i === idx ? " current" : ""}`}>
            <span className="deploy-step-dot">
              {i < idx || done ? <Icon name="check" size={10} /> : i === idx ? <span className="deploy-step-pulse" /> : null}
            </span>
            <span className="deploy-step-label">{stepMeta[s].label}</span>
            {i < stages.length - 1 && <span className="deploy-step-bar" />}
          </div>
        ))}
      </div>
    </div>
  );
}

// Deployment-command lifecycle stage machine (shared by SystemDetail + SystemPanel).
// Pull-based: server queues → agent checks in → applies → activates.
const DEPLOY_STAGES = ["queued", "picked-up", "applying", "activated"];
function useDeployStages(pendingDeploy, onClear) {
  const [stage, setStage] = React.useState(null);
  React.useEffect(() => {
    if (!pendingDeploy) { setStage(null); return; }
    setStage("queued");
    // Pull-based agent: it only checks in on its own heartbeat cadence, so there's
    // almost always a real wait here — never assume it's listening right away.
    const t1 = setTimeout(() => setStage("picked-up"), 15000);
    const t2 = setTimeout(() => setStage("applying"), 17200);
    const t3 = setTimeout(() => setStage("activated"), 20600);
    const t4 = setTimeout(() => onClear?.(), 24300);
    return () => { [t1,t2,t3,t4].forEach(clearTimeout); };
  }, [pendingDeploy?.at]);
  return stage;
}

function SystemDetail({ sys, onBack, onDeploy, onEdit, onNavigate, onTagFilter, onOpenCommit, pendingDeploy, onStartPending, onClearPending, initialTab }) {
  const [tab, setTab] = React.useState(initialTab || "overview");
  const [editSystem, setEditSystem] = React.useState(false);
  const deployStage = useDeployStages(pendingDeploy, onClearPending);
  React.useEffect(() => { setTab(initialTab || "overview"); }, [sys.id, initialTab]);
  const [logsJump, setLogsJump] = React.useState(null);
  const [rollbackTarget, setRollbackTarget] = React.useState(null);
  const [rollbackConfirm, setRollbackConfirm] = React.useState(false);
  const [sshOpen, setSshOpen] = React.useState(false);
  const [rollbackOpen, setRollbackOpen] = React.useState(false);
  const [commitPeek, setCommitPeek] = React.useState(null); // {sha, msg, flake, author, at} | {capture,...}
  const openCommitPeek = (c) => {
    const f = FLAKE_REGISTRY.find(x => x.name === c.flake)
           || FLAKE_REGISTRY.find(x => (FLAKE_COMMITS[x.id] || []).some(k => k.sha === c.sha))
           || FLAKE_REGISTRY[0];
    setCommitPeek({ flake: f, sha: c.capture ? null : c.sha, meta: { msg: c.msg, author: c.author, at: c.at } });
  };
  if (!sys) return null;

  return (
    <div className="sd-root" data-screen-label="SystemDetail">
      {/* Header */}
      <div className="sd-head">
        <div className="sd-crumb">
          <button className="sd-back focus-ring" onClick={onBack} aria-label="Back to systems">
            <Icon name="arrow-left" size={14} />
          </button>
          <span className="sd-crumb-text">
            <span className="sd-crumb-parent">Systems</span>
            <span className="sd-crumb-sep">/</span>
            <span className="sd-crumb-current mono">{sys.hostname}</span>
          </span>
        </div>
        <div className="sd-head-main">
          <div className="sd-title-block">
            <span className="status-dot lg" style={{ "--status-color": sys.statusColor }} />
            <div>
              <h1 className="sd-hostname">{sys.hostname}</h1>
              <div className="sd-fqdn mono">{sys.fqdn}</div>
            </div>
            <EnvBadge env={sys.environment} />
            <StatusChip sys={sys} />
            <DeploymentChip state={sys.deploymentState} />
          </div>
          <div className="sd-head-actions">
            <button className="btn btn-ghost focus-ring" onClick={() => setRollbackOpen(true)}><Icon name="rollback" size={14} /> Rollback</button>
            <button className="btn btn-ghost focus-ring" onClick={() => setSshOpen(true)}><Icon name="terminal" size={14} /> SSH</button>
            <button className="btn btn-ghost focus-ring" onClick={() => onEdit?.(sys)}><Icon name="gear" size={14} /> Edit</button>
            <button className="btn btn-primary focus-ring" onClick={() => setTab("deploy")}>
              <Icon name="deploy" size={14} /> Deploy
            </button>
          </div>
        </div>

        {/* Key metric strip */}
        <div className="sd-metric-strip">
          <div className="sd-metric">
            <div className="sd-metric-label">Heartbeat</div>
            <div className="sd-metric-val">
              <HeartbeatSpinner intervalSec={sys.heartbeatIntervalSec} nextInSec={sys.heartbeatNextInSec} size={36} deployStage={deployStage} deployStartedAt={pendingDeploy?.at}/>
            </div>
          </div>
          <div className="sd-metric">
            <div className="sd-metric-label">Generation</div>
            <div className="sd-metric-val-num">#{sys.generation}</div>
            <div className="sd-metric-sub">activated · {sys.lastHeartbeat}</div>
          </div>
          <div className="sd-metric">
            <div className="sd-metric-label">Uptime</div>
            <div className="sd-metric-val-num">{sys.uptime}</div>
            <div className="sd-metric-sub">{sys.kernel}</div>
          </div>
          <div className="sd-metric">
            <div className="sd-metric-label">CVEs</div>
            <div className="sd-metric-val-num" style={{ color: sys.cves.critical > 0 ? "#f87171" : "#34d399" }}>
              {sys.cves.total}
            </div>
            <div className="sd-metric-sub">{sys.cves.critical} critical · {sys.cves.high} high</div>
          </div>
          <div className="sd-metric">
            <div className="sd-metric-label">Policy</div>
            <div className="sd-metric-val-num mono" style={{ fontSize: 18 }}>{sys.deploymentPolicy}</div>
            <div className="sd-metric-sub">env: {sys.environment}</div>
          </div>
        </div>

        {deployStage && pendingDeploy && (
          <PendingDeployBanner
            stage={deployStage}
            stages={DEPLOY_STAGES}
            commit={pendingDeploy?.commit}
            kind={pendingDeploy?.kind}
            gen={pendingDeploy?.gen}
            sys={sys}
            onDismiss={onClearPending}
            onViewLogs={() => setTab("logs")}
          />
        )}

        {/* Tabs */}
        <div className="sd-tabs" role="tablist">
          {[
            { k: "overview",   l: "Overview",  i: "dashboard" },
            { k: "deploy",     l: "Deploy",    i: "deploy" },
            { k: "history",    l: "History",   i: "history" },
            { k: "logs",       l: "Logs",      i: "terminal" },
            { k: "config",     l: "Config",    i: "file" },
            { k: "cves",       l: "CVEs",      i: "shield", badge: sys.cves.critical > 0 ? sys.cves.critical : null },
            { k: "hardening",  l: "Hardening", i: "key" },
            { k: "compliance", l: "Compliance",i: "shield" },
          ].map(t => (
            <button
              key={t.k}
              role="tab"
              aria-selected={tab === t.k}
              className={`sd-tab focus-ring${tab === t.k ? " active" : ""}`}
              onClick={() => setTab(t.k)}
            >
              <Icon name={t.i} size={13} /> {t.l}
              {t.badge != null && <span className="sd-tab-badge">{t.badge}</span>}
            </button>
          ))}
        </div>
      </div>

      {/* Tab panels */}
      <div className="sd-body">
        {tab === "overview"   && <OverviewTab sys={sys} onViewCves={() => setTab("cves")} onTagFilter={onTagFilter} deployStage={deployStage} pendingCommit={pendingDeploy?.commit} pendingKind={pendingDeploy?.kind} pendingGen={pendingDeploy?.gen} onViewHistory={() => setTab("history")} onOpenCommit={openCommitPeek} onOpenBuild={() => onNavigate?.("builds")} />}
        {tab === "deploy"     && <DeployTab sys={sys} onDeploy={onDeploy} onOpenCommit={openCommitPeek} />}
        {tab === "history"    && <HistoryTab sys={sys} onRollback={(sha,gen)=>{ setRollbackTarget({sha,gen}); setRollbackConfirm(true); }} onLogsJump={(id)=>{ setTab("logs"); setLogsJump({ id, nonce: Date.now() }); }} onOpenCommit={openCommitPeek} />}
        {tab === "logs"       && <LogsTab sys={sys} jump={logsJump} />}
        {tab === "config"     && <ConfigTab sys={sys} />}
        {tab === "cves"       && <CvesTab sys={sys} />}
        {tab === "compliance" && <ComplianceTab sys={sys} onNavigate={onNavigate} />}
        {tab === "hardening"  && <HardeningTab sys={sys} />}
      </div>
      {sshOpen && <SshConnectModal sys={sys} onClose={() => setSshOpen(false)} />}
      {(rollbackConfirm || rollbackOpen) && <RollbackModal sys={sys} targetGen={rollbackTarget?.gen} targetSha={rollbackTarget?.sha} onClose={() => { setRollbackConfirm(false); setRollbackOpen(false); setRollbackTarget(null); }} onConfirm={(g) => { setRollbackConfirm(false); setRollbackOpen(false); setRollbackTarget(null); setTab("overview"); onStartPending?.({ commit: g.sha.substring(0,7), kind: "rollback", gen: g.id }); }} />}
      {commitPeek && <FlakeTray flake={commitPeek.flake} focusSha={commitPeek.sha} focusMeta={commitPeek.meta} onClose={() => setCommitPeek(null)} onEdit={() => {}} />}
    </div>
  );
}

/* ---------- Rollback confirmation ---------- */
function RollbackModal({ sys, targetGen, targetSha, onClose, onConfirm }) {
  const gen = sys.generation;
  // Candidate previous generations the host could roll back to (newest first, excluding current).
  // If a specific targetGen was passed, use that; otherwise show the most recent clean one.
  const candidates = React.useMemo(() => {
    const gens = [];
    for (let i = 1; i <= 10; i++) {
      const g = gen - i;
      if (g > 0) {
        gens.push({
          id: g,
          sha: (Math.random().toString(16).substring(2, 9)),
          msg: i === 1 ? "Previous deployment" : i === 2 ? "Before last policy update" : `Generation ${g}`,
          at: i === 1 ? "2h ago" : i === 2 ? "1d ago" : i === 3 ? "3d ago" : `${Math.ceil(i/2)}d ago`,
          kernel: "6.6." + String(72 - i).padStart(2, "0"),
          by: ["ops-bot", "dchen", "mreyes"][i % 3],
        });
      }
    }
    return gens;
  }, [gen]);
  
  const [targetId, setTargetId] = React.useState(targetGen ?? candidates[0]?.id);
  const target = candidates.find(c => c.id === targetId) || candidates[0];
  const isProd = typeof isProductionEnv === "function" ? isProductionEnv(sys.environment) : (sys.environment === "production");
  const [confirmText, setConfirmText] = React.useState("");

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <div className="modal-head">
          <div>
            <h2>Rollback</h2>
            <div style={{ fontSize: 13, color: "var(--cf-text-secondary)", marginTop: 2 }}>{sys.hostname}</div>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16} /></button>
        </div>
        <div className="modal-body">
          <div className="sd-callout sd-callout-warn" style={{ marginBottom: 16 }}>
            <Icon name="alert-triangle" size={13} />
            <span>Rolling back bypasses the current deployment policy and gate policies. Use only when the current generation is broken.</span>
          </div>

          <div className="field">
            <label>Rollback to</label>
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              {candidates.map(c => (
                <label key={c.id} style={{ display: "flex", gap: 10, padding: 10, border: `1px solid ${targetId === c.id ? "var(--cf-brand-purple)" : "var(--cf-card-border)"}`, borderRadius: 8, cursor: "pointer", background: targetId === c.id ? "color-mix(in oklab, var(--cf-brand-purple) 8%, transparent)" : "transparent" }}>
                  <input type="radio" checked={targetId === c.id} onChange={() => setTargetId(c.id)} />
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: 13, fontWeight: 600, color: "var(--cf-text-primary)" }}>Generation #{c.id}</div>
                    <div style={{ fontSize: 12, color: "var(--cf-text-secondary)", marginTop: 2 }}>{c.msg}</div>
                    <div style={{ fontSize: 11, color: "var(--cf-text-muted)", marginTop: 4, fontFamily: "var(--font-mono)" }}>{c.sha.substring(0, 7)} · {c.at}</div>
                  </div>
                </label>
              ))}
            </div>
          </div>

          {isProd && (
            <div className="field" style={{ marginTop: 16 }}>
              <label>Type the hostname to confirm on production</label>
              <input type="text" placeholder={sys.hostname} value={confirmText} onChange={e => setConfirmText(e.target.value)} style={{ width: "100%", padding: "8px 12px", borderRadius: 6, border: "1px solid var(--cf-card-border)", fontFamily: "var(--font-mono)", fontSize: 13, boxSizing: "border-box" }} />
            </div>
          )}
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-danger focus-ring" disabled={isProd && confirmText !== sys.hostname} onClick={() => onConfirm(target)}>
            Roll back to gen #{target.id}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ---------- SSH connect helper (not yet implemented — shows how to connect manually) ---------- */
function SshConnectModal({ sys, onClose }) {
  const [copied, setCopied] = React.useState(null);
  const target = sys.serverAddress || sys.fqdn || sys.hostname;
  const isPull = sys.reachability === "pull";
  const copy = (text, id) => {
    if (navigator.clipboard) navigator.clipboard.writeText(text).catch(() => {});
    setCopied(id); setTimeout(() => setCopied(c => (c === id ? null : c)), 1500);
  };
  const Cmd = ({ id, children }) => (
    <div className="ssh-cmd">
      <code className="mono">{children}</code>
      <button className="btn btn-ghost xs focus-ring" onClick={() => copy(children, id)}>
        <Icon name={copied === id ? "check" : "file"} size={11} /> {copied === id ? "Copied" : "Copy"}
      </button>
    </div>
  );
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={{ width: "min(560px,96vw)" }}>
        <div className="modal-head">
          <h2><Icon name="terminal" size={14} style={{ marginRight: 6, verticalAlign: "text-bottom" }} /> Connect to {sys.hostname}</h2>
          <p>In-app terminal isn't available yet — connect directly over SSH for now.</p>
        </div>
        <div className="modal-body" style={{ overflowY: "auto" }}>
          <div className="sd-callout sd-callout-warn" style={{ marginBottom: 14 }}>
            <Icon name="warn" size={13} />
            <div style={{ fontSize: 12 }}>Browser-based SSH is on the roadmap. These commands run from your own workstation.</div>
          </div>

          <div className="field"><label>Connect</label></div>
          <Cmd id="ssh">{`ssh root@${target}`}</Cmd>

          {isPull && (
            <div className="help" style={{ marginTop: 8 }}>
              <Icon name="warn" size={11} style={{ verticalAlign: "text-bottom", color: "var(--cf-amber, #fbbf24)" }} /> This host is <strong>pull-only</strong> (behind NAT/firewall). It may only be reachable from inside its network or via a bastion.
            </div>
          )}

          <div className="field" style={{ marginTop: 16 }}><label>Via bastion</label></div>
          <Cmd id="jump">{`ssh -J bastion.${(sys.fqdn || "").split(".").slice(1).join(".") || "example.com"} root@${target}`}</Cmd>

          <div className="field" style={{ marginTop: 16 }}><label>Tail the system journal</label></div>
          <Cmd id="journal">{`ssh root@${target} journalctl -fu crystal-forge-agent`}</Cmd>

          <dl className="kv-grid" style={{ marginTop: 16 }}>
            <dt>Target</dt><dd className="mono">{target}</dd>
            <dt>Environment</dt><dd>{sys.environment}</dd>
            <dt>Reachability</dt><dd>{isPull ? "Agent pull-only" : "Direct / LAN"}</dd>
          </dl>
        </div>
        <div className="modal-foot">
          <button className="btn btn-primary focus-ring" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}

/* ---------- Overview ---------- */
// Build a realistic activity feed from the same event stream that powers History + Logs,
// so the overview reflects what actually happened (and what's happening right now).
function buildActivityFeed(sys, deployStage, pendingCommit, pendingKind, pendingGen) {
  const feed = [];
  // 1) live deployment / rollback, if one is in flight
  if (deployStage) {
    const isRb = pendingKind === "rollback";
    const targetGen = isRb ? pendingGen : sys.generation + 1;
    const map = isRb ? {
      "queued":    { t:`Rollback queued — awaiting agent check-in`, c:"#fbbf24", ic:"rollback", live:true },
      "picked-up": { t:`Agent picked up rollback to gen #${pendingGen}`, c:"#fbbf24", ic:"rollback", live:true },
      "applying":  { t:`Reverting to gen #${pendingGen} (${pendingCommit})`, c:"#fbbf24", ic:"rollback", live:true },
      "activated": { t:`Rolled back — generation #${targetGen} live`, c:"#34d399", ic:"check" },
    } : {
      "queued":    { t:`Deployment queued — awaiting agent check-in`, c:"#a78bfa", ic:"deploy", live:true },
      "picked-up": { t:`Agent picked up deploy ${pendingCommit}`,     c:"#60a5fa", ic:"deploy", live:true },
      "applying":  { t:`Applying ${pendingCommit} — building & switching`, c:"#60a5fa", ic:"deploy", live:true },
      "activated": { t:`Generation #${targetGen} activated`,  c:"#34d399", ic:"check" },
    };
    const m = map[deployStage];
    feed.push({ at:"now", title:m.t, color:m.c, icon:m.ic, live:m.live });
  }
  // 2) latest heartbeat (the always-fresh signal)
  feed.push({ at: sys.lastHeartbeat, title:`Heartbeat received`, color:"#34d399", icon:"activity",
    sub: sys.reachability === "reachable" ? undefined : sys.reachability });
  // 3) recent real events from the deployment history
  const events = (typeof buildHistory === "function" ? buildHistory(sys) : []).slice(0, 8);
  for (const e of events) {
    if (e.type === "startup") {
      feed.push({ at:e.at, title:`System restarted`, sub:`ran ${e.ran} · gen #${e.gen}`, color:"#60a5fa", icon:"power" });
    } else if (e.source === "local") {
      feed.push({ at:e.at, title:`Local rebuild ${e.resolution==="matched"?"(reconciled)":"(out of band)"}`,
        sub:e.msg, color: e.resolution==="matched" ? "#60a5fa" : "#fbbf24", icon:"edit" });
    } else if (e.status === "failed") {
      feed.push({ at:e.at, title:`Deploy failed`, sub:e.msg, color:"#f87171", icon:"x" });
    } else {
      feed.push({ at:e.at, title:`Deployed #${e.gen}`, sub:`${e.sha} · ${e.msg}`, color:"#a78bfa", icon:"deploy" });
    }
  }
  return feed.slice(0, 9);
}

function OverviewTab({ sys, onViewCves, onTagFilter, deployStage, pendingCommit, pendingKind, pendingGen, onViewHistory, onOpenCommit, onOpenBuild }) {
  const [tags, setTags] = React.useState(sys.tags || []);
  const [adding, setAdding] = React.useState(false);
  const [draft, setDraft] = React.useState("");
  const suggestions = (typeof allFleetTags === "function" ? allFleetTags() : []).filter((t) => !tags.includes(t));
  const addTag = (raw) => {
    const v = (raw || "").trim().replace(/^#/, "").replace(/\s+/g, "-").toLowerCase();
    if (v && !tags.includes(v)) setTags([...tags, v]);
    setDraft(""); setAdding(false);
  };
  const removeTag = (t) => setTags(tags.filter((x) => x !== t));
  return (
    <div className="sd-grid sd-grid-overview">
      <section className="card sd-card">
        <div className="sd-card-head">
          <h2>Currently deployed</h2>
          <span className="chip chip-healthy"><Icon name="check" size={10} /> up-to-date</span>
        </div>
        <dl className="kv-grid">
          <dt>Flake</dt><dd className="mono">{sys.flake}</dd>
          <dt>Branch</dt><dd className="mono">{sys.branch}</dd>
          <dt>Commit</dt>
          <dd className="mono">
            <button className="tl-commit-link mono focus-ring" title={`Open ${sys.commit} in Flakes`}
              onClick={() => onOpenCommit?.({ sha: sys.commit, msg: sys.commitMessage, flake: sys.flake, author: sys.deployedBy, at: sys.lastDeployAt })}>
              <Icon name="git" size={11} /> {sys.commit} <Icon name="arrow-right" size={10} />
            </button>
            {" "}
            <button className="tl-commit-link mono focus-ring" title={`Open the build for ${sys.commit}`}
              onClick={() => onOpenBuild?.({ sha: sys.commit, flake: sys.flake, generation: sys.generation })}>
              <Icon name="build" size={11} /> build #{sys.generation} <Icon name="arrow-right" size={10} />
            </button>
          </dd>
          <dt>Message</dt><dd style={{ whiteSpace: "normal", fontFamily: "var(--font-sans)" }}>{sys.commitMessage}</dd>
          <dt>Generation</dt><dd className="mono">#{sys.generation}</dd>
          <dt>NixOS</dt><dd className="mono">{sys.nixosVersion}</dd>
          <dt>Kernel</dt><dd className="mono">{sys.kernel}</dd>
          <dt>Store path</dt>
          <dd className="mono" style={{ fontSize:11, whiteSpace:"normal", wordBreak:"break-all", lineHeight:1.4 }} title={sys.storePath}>
            {sys.storePath}
          </dd>
          {sys.targetStorePath && sys.targetStorePath !== sys.storePath && (
            <>
              <dt style={{ color:"#fbbf24" }}>Target</dt>
              <dd className="mono" style={{ fontSize:11, whiteSpace:"normal", wordBreak:"break-all", lineHeight:1.4, color:"#fbbf24" }} title={sys.targetStorePath}>
                {sys.targetStorePath}
                <span className="chip chip-warning" style={{ marginLeft:6, fontSize:10 }}>drift</span>
              </dd>
            </>
          )}
        </dl>
      </section>

      <section className="card sd-card">
        <div className="sd-card-head">
          <h2>Host</h2>
          <span className="mono" style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{sys.id}</span>
        </div>
        <dl className="kv-grid">
          <dt>Hostname</dt><dd className="mono">{sys.hostname}</dd>
          <dt>FQDN</dt><dd className="mono">{sys.fqdn}</dd>
          <dt>Environment</dt><dd><EnvBadge env={sys.environment} /></dd>
          <dt>Uptime</dt><dd>{sys.uptime}</dd>
          <dt>CPU</dt><dd>{sys.cpu}</dd>
          <dt>Memory</dt><dd>{sys.memGb} GiB</dd>
          <dt>IPv4</dt><dd className="mono" title={sys.ipv4}>{sys.ipv4}</dd>
          <dt>IPv6</dt><dd className="mono" title={sys.ipv6}>{sys.ipv6}</dd>
          <dt>Reachability</dt><dd>
            {sys.reachability === "pull"
              ? <span className="chip chip-warning" title="Behind NAT/firewall — agent checks in; no inbound from server">pull-only</span>
              : <span className="chip chip-healthy" title="Server can reach the agent directly (LAN/routable/VPN)">direct / LAN</span>}
          </dd>
        </dl>
        <div className="hb-panel" style={{ marginTop: 16 }}>
          <HeartbeatSpinner intervalSec={sys.heartbeatIntervalSec} nextInSec={sys.heartbeatNextInSec} size={56} />
        </div>
      </section>

      <section className="card sd-card">
        <div className="sd-card-head">
          <h2>CVE exposure</h2>
          <button className="btn btn-ghost xs focus-ring" onClick={onViewCves}><Icon name="arrow-right" size={11} /> View all</button>
        </div>
        <CveBar cves={sys.cves} />
        <div className="cve-legend" style={{ marginTop: 12 }}>
          <span className="cve-legend-item"><span className="cve-legend-swatch" style={{ background: "#f87171" }} />{sys.cves.critical} critical</span>
          <span className="cve-legend-item"><span className="cve-legend-swatch" style={{ background: "#fbbf24" }} />{sys.cves.high} high</span>
          <span className="cve-legend-item"><span className="cve-legend-swatch" style={{ background: "#9ca3af" }} />{sys.cves.medium} medium</span>
          <span className="cve-legend-item"><span className="cve-legend-swatch" style={{ background: "#4b5563" }} />{sys.cves.low} low</span>
        </div>
        {sys.cves.critical > 0 && (
          <div className="sd-callout sd-callout-danger" style={{ marginTop: 14 }}>
            <Icon name="shield" size={14} />
            <div>
              <strong>{sys.cves.critical} critical CVE{sys.cves.critical === 1 ? "" : "s"}</strong> on this host.
              Review and patch at earliest opportunity.
            </div>
          </div>
        )}
      </section>

      <section className="card sd-card sd-card-wide">
        <div className="sd-card-head">
          <h2>Recent activity</h2>
          <button className="btn btn-ghost xs focus-ring" onClick={onViewHistory}>View all</button>
        </div>
        <div className="timeline sd-timeline">
          {buildActivityFeed(sys, deployStage, pendingCommit, pendingKind, pendingGen).map((e, i) => (
            <div key={i} className={`tl-item${e.live ? " tl-item-live" : ""}`}>
              <span className="tl-dot" style={{ "--status-color": e.color }}>
                {e.live ? <span className="tl-dot-pulse" /> : null}
              </span>
              <div className="tl-body">
                <div className="tl-title" style={{ display:"flex", alignItems:"center", gap:6 }}>
                  <Icon name={e.icon} size={12} style={{ color:e.color, flexShrink:0 }} />
                  <span>{e.title}</span>
                  {e.sub && <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)", overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>· {e.sub}</span>}
                </div>
                <div className="tl-meta">{e.at}</div>
              </div>
            </div>
          ))}
        </div>
      </section>

      <section className="card sd-card">
        <div className="sd-card-head">
          <h2>Tags</h2>
        </div>
        <div className="sd-tag-row">
          {tags.length > 0 ? tags.map(t => (
            <span key={t} className="sd-tag mono sd-tag-chip">
              <button className="sd-tag-label focus-ring" title={`Filter fleet by #${t}`} onClick={() => onTagFilter?.(t)}>#{t}</button>
              <button className="sd-tag-x focus-ring" title="Remove tag" onClick={() => removeTag(t)}><Icon name="x" size={9} /></button>
            </span>
          )) : !adding && <span style={{ color: "var(--cf-text-muted)", fontSize: 13 }}>No tags yet</span>}
          {adding ? (
            <span className="sd-tag-input-wrap">
              <input
                className="sd-tag-input mono focus-ring" autoFocus list="cf-fleet-tags" placeholder="tag…"
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") addTag(draft); if (e.key === "Escape") { setDraft(""); setAdding(false); } }}
                onBlur={() => { if (draft.trim()) addTag(draft); else setAdding(false); }} />
              <datalist id="cf-fleet-tags">{suggestions.map((s) => <option key={s} value={s} />)}</datalist>
            </span>
          ) : (
            <button className="sd-tag sd-tag-add focus-ring" onClick={() => setAdding(true)}><Icon name="plus" size={10} /> add</button>
          )}
        </div>
        <div className="help" style={{ marginTop: 8 }}>
          Free-form labels for your own grouping &amp; filtering — click a tag to slice the fleet by it. They don't affect policies or deployment.
        </div>
      </section>
    </div>
  );
}

/* ---------- Deploy ---------- */
function DeployTab({ sys, onDeploy, onOpenCommit }) {
  const [mode, setMode] = React.useState("commit"); // 'commit' | 'generation'
  const [target, setTarget] = React.useState(null); // {kind, id, label, sub, sha?}
  const [showDiff, setShowDiff] = React.useState(false);

  const commits = React.useMemo(() => {
    const msgs = [
      "chore: bump nixpkgs to 24.11 snapshot",
      "fix: restart nginx on cert renewal",
      "stig: enforce audit rules for sudo",
      "feat: enable prometheus node exporter",
      "cve: patch openssl to 3.3.2",
      "refactor: extract firewall module",
      "fix: postgres role permissions migration",
      "chore: update kernel to 6.6.72",
      "feat: prometheus alertmanager rules",
      "fix: rotate sops keys",
    ];
    return msgs.map((m, i) => ({
      sha: ["a3f8c12","f1d9022","8c4b311","77aef00","3c12889","a22fc08","bc10201","0e9f177","dd55410","e7a1233"][i],
      message: m,
      author: ["mreyes","jpark","dchen","ops-bot"][i % 4],
      when: i === 0 ? "2m ago" : i === 1 ? "18m ago" : i === 2 ? "1h ago" : `${i}h ago`,
      current: i === 2,
      buildStatus: i === 0 ? "building" : i === 1 ? "cached" : "cached",
    }));
  }, [sys.id]);

  // Generation history (all locally activated nixos generations on this system)
  const generations = React.useMemo(() => {
    const gen = sys.generation;
    // origin: 'cf' (deployed via Crystal Forge) | 'local' (manual nixos-rebuild on host) | 'unknown' (closure not in cache, no metadata)
    return [
      { id: gen,     origin: "cf",      sha: sys.commit, flake: sys.flake, msg: sys.commitMessage,            at: "2h ago",     kernel: "6.6.72", by: "mreyes",       current: true,  state: "active" },
      { id: gen - 1, origin: "local",   sha: null,       flake: null,      msg: "Manual rebuild — uncommitted change", at: "8h ago",     kernel: "6.6.72", by: "root@host",   current: false, state: "drift",  driftHint: "modules/services/nginx.nix differs from sys.commit" },
      { id: gen - 2, origin: "cf",      sha: "a1f2c31",  flake: sys.flake, msg: "chore: bump nixpkgs",        at: "yesterday",  kernel: "6.6.72", by: "ops-bot",     current: false, state: "ok" },
      { id: gen - 3, origin: "cf",      sha: "ffa2b88",  flake: sys.flake, msg: "cve: patch openssl",         at: "3d ago",     kernel: "6.6.71", by: "ops-bot",     current: false, state: "ok" },
      { id: gen - 4, origin: "unknown", sha: null,       flake: null,      msg: "Closure not in cache",       at: "5d ago",     kernel: "6.6.71", by: "—",           current: false, state: "unknown" },
      { id: gen - 5, origin: "cf",      sha: "9b3a201",  flake: sys.flake, msg: "feat: prometheus exporter",  at: "1w ago",     kernel: "6.6.70", by: "dchen",       current: false, state: "ok" },
      { id: gen - 6, origin: "cf",      sha: "44102fa",  flake: sys.flake, msg: "stig: harden sshd defaults", at: "2w ago",     kernel: "6.6.68", by: "mreyes",      current: false, state: "ok" },
    ];
  }, [sys.id]);

  const selected = target || (mode === "commit"
    ? { kind: "commit", id: commits[0].sha, label: commits[0].sha, sub: commits[0].message, sha: commits[0].sha }
    : (() => {
        const g = generations.find(x => !x.current) || generations[1];
        return { kind: "generation", id: g.id, label: `gen #${g.id}`, sub: g.msg, sha: g.sha, origin: g.origin };
      })());

  const switchMode = (m) => {
    setMode(m);
    setTarget(null);
  };

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:14 }}>
      {/* Gate panel — shows policy evaluation outcomes for the selected target */}
      <DeployGatePanel sys={sys} commitSha={selected.sha} userRole="operator"/>

      <div className="sd-grid sd-grid-deploy">
      <section className="card sd-card">
        <div className="sd-card-head" style={{ flexDirection: "column", alignItems: "stretch", gap: 12 }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
            <h2>Select target</h2>
            <span className="sd-card-meta mono">{sys.flake}</span>
          </div>
          <div className="seg" style={{ alignSelf: "flex-start" }}>
            <button className={mode === "commit" ? "active" : ""} onClick={() => switchMode("commit")}>
              <Icon name="git" size={12} /> Commit
            </button>
            <button className={mode === "generation" ? "active" : ""} onClick={() => switchMode("generation")}>
              <Icon name="rollback" size={12} /> Generation
            </button>
          </div>
        </div>

        {mode === "commit" ? (
          <div className="sd-commit-list">
            {commits.map(c => {
              const isSel = selected.kind === "commit" && selected.id === c.sha;
              return (
                <button key={c.sha}
                  className={`sd-commit-item focus-ring${isSel ? " selected" : ""}`}
                  onClick={() => setTarget({ kind: "commit", id: c.sha, label: c.sha, sub: c.message, sha: c.sha })}
                >
                  <span className="mono sd-commit-sha sd-commit-sha-link" title={`Open ${c.sha} in Flakes`} onClick={ev => { ev.stopPropagation(); onOpenCommit?.({ sha: c.sha, msg: c.message, flake: sys.flake, author: c.author, at: c.when }); }}><Icon name="git" size={10} /> {c.sha}</span>
                  <span className="sd-commit-msg">{c.message}</span>
                  <span className="sd-commit-meta mono">{c.author}</span>
                  <span className="sd-commit-meta">{c.when}</span>
                  {c.current
                    ? <span className="chip chip-info">deployed</span>
                    : c.buildStatus === "cached"
                      ? <span className="chip chip-healthy">cached</span>
                      : <span className="chip chip-info">building</span>}
                </button>
              );
            })}
          </div>
        ) : (
          <div className="sd-commit-list">
            {generations.map(g => {
              const isSel = selected.kind === "generation" && selected.id === g.id;
              const originBadge = g.origin === "cf"
                ? <span className="chip chip-info" title="Deployed via Crystal Forge"><Icon name="git" size={9}/> commit</span>
                : g.origin === "local"
                  ? <span className="chip chip-warning" title={g.driftHint || "Manual nixos-rebuild on host"}><Icon name="warn" size={9}/> local</span>
                  : <span className="chip chip-unknown" title="Closure no longer in cache or metadata stripped">unknown</span>;
              const shaCell = g.sha
                ? <span className="mono sd-commit-sha sd-commit-sha-link" title={`Open ${g.sha} in Flakes`} onClick={ev => { ev.stopPropagation(); onOpenCommit?.({ sha: g.sha, msg: g.msg, flake: g.flake || sys.flake, author: g.by, at: g.at }); }}><Icon name="git" size={10} /> {g.sha}</span>
                : <span className="mono sd-commit-sha" style={{ color: "var(--cf-text-muted)", fontStyle: "italic" }}>—</span>;
              return (
                <button key={g.id}
                  className={`sd-commit-item focus-ring${isSel ? " selected" : ""}`}
                  style={{ gridTemplateColumns: "60px 80px 1fr auto auto auto auto" }}
                  onClick={() => setTarget({ kind: "generation", id: g.id, label: `gen #${g.id}`, sub: g.msg, sha: g.sha, origin: g.origin })}
                >
                  <span className="mono sd-commit-sha" style={{ color: "var(--cf-brand-purple)" }}>#{g.id}</span>
                  {shaCell}
                  <span className="sd-commit-msg" style={ g.origin !== "cf" ? { color:"var(--cf-text-secondary)", fontStyle:"italic" } : null }>{g.msg}</span>
                  {originBadge}
                  <span className="sd-commit-meta mono">k{g.kernel}</span>
                  <span className="sd-commit-meta">{g.at}</span>
                  {g.current
                    ? <span className="chip chip-healthy">active</span>
                    : <span className="chip chip-unknown">rollback</span>}
                </button>
              );
            })}
          </div>
        )}
      </section>

      <section className="card sd-card sd-deploy-panel">
        <div className="sd-card-head">
          <h2>{selected.kind === "generation" ? "Rollback plan" : "Deployment plan"}</h2>
          <button className="btn btn-ghost xs focus-ring" onClick={() => setShowDiff(v => !v)}>
            <Icon name="file" size={11} /> {showDiff ? "Hide" : "Show"} diff
          </button>
        </div>
        <dl className="kv-grid">
          <dt>Target</dt><dd className="mono">{sys.hostname}</dd>
          <dt>From</dt>
          <dd className="mono">
            gen #{sys.generation} · <span className="sd-commit-sha-link" title={`Open ${sys.commit} in Flakes`} onClick={() => onOpenCommit?.({ sha: sys.commit, msg: sys.commitMessage, flake: sys.flake, author: sys.deployedBy, at: sys.lastDeployAt })}><Icon name="git" size={10} /> {sys.commit}</span>
          </dd>
          <dt>To</dt>
          <dd className="mono">
            {selected.kind === "generation"
              ? <>gen #{selected.id}{selected.sha ? <> · <span className="sd-commit-sha-link" title={`Open ${selected.sha} in Flakes`} onClick={() => onOpenCommit?.({ sha: selected.sha, msg: selected.sub, flake: sys.flake })}><Icon name="git" size={10} /> {selected.sha}</span></> : <span style={{ color:"var(--cf-text-muted)", fontStyle:"italic" }}> · no commit</span>}</>
              : <span className="sd-commit-sha-link" title={`Open ${selected.sha} in Flakes`} onClick={() => onOpenCommit?.({ sha: selected.sha, msg: selected.sub, flake: sys.flake })}><Icon name="git" size={10} /> {selected.sha}</span>}
          </dd>
          {selected.kind === "generation" && selected.origin && (
            <>
              <dt>Origin</dt>
              <dd>
                {selected.origin === "cf" && <span className="chip chip-info">deployed via CF</span>}
                {selected.origin === "local" && <span className="chip chip-warning">local rebuild · drift</span>}
                {selected.origin === "unknown" && <span className="chip chip-unknown">unknown / not in cache</span>}
              </dd>
            </>
          )}
          <dt>Strategy</dt><dd>{selected.kind === "generation" ? "switch_to_generation" : "immediate_persist"}</dd>
          <dt>Policy</dt><dd className="mono">{sys.deploymentPolicy}</dd>
        </dl>

        {showDiff && (
          <pre className="sd-diff">
{selected.kind === "generation"
? `# Switching to existing generation #${selected.id}
# No new build required — closure already on disk
nix-env --switch-generation ${selected.id} --profile /nix/var/nix/profiles/system
/nix/var/nix/profiles/system-${selected.id}-link/bin/switch-to-configuration switch`
: `--- a/nixos/modules/services/nginx.nix
+++ b/nixos/modules/services/nginx.nix
@@ -14,7 +14,7 @@
   services.nginx = {
     enable = true;
-    recommendedTlsSettings = false;
+    recommendedTlsSettings = true;
     virtualHosts.${sys.hostname} = {
       forceSSL = true;
       enableACME = true;`}
          </pre>
        )}

        {(() => {
          if (selected.kind !== "generation") {
            return (
              <div className="sd-callout sd-callout-info">
                <Icon name="check" size={13} />
                <div>Policy check <strong className="mono">{sys.deploymentPolicy}</strong> will run before deploy. No agent disconnect expected.</div>
              </div>
            );
          }
          if (selected.origin === "unknown") {
            return (
              <div className="sd-callout sd-callout-danger">
                <Icon name="warn" size={13} />
                <div><strong>Closure not in cache.</strong> The store path for this generation is no longer available. Rollback will fail unless the closure is re-fetched. Source commit is unknown.</div>
              </div>
            );
          }
          if (selected.origin === "local") {
            return (
              <div className="sd-callout sd-callout-warn">
                <Icon name="warn" size={13} />
                <div><strong>Drift generation.</strong> Built on the host outside Crystal Forge — no commit anchor. Rolling back here brings the box back to that out-of-band state. Crystal Forge cannot reproduce this generation from source.</div>
              </div>
            );
          }
          return (
            <div className="sd-callout sd-callout-warn">
              <Icon name="warn" size={13} />
              <div>Rollback to a prior generation. No build needed — closure is on disk. Heartbeat may pause briefly during activation.</div>
            </div>
          );
        })()}

        <div className="sd-deploy-actions">
          <button className="btn btn-ghost focus-ring">{selected.kind === "generation" ? "Verify closure" : "Dry-run build"}</button>
          <button className="btn btn-primary focus-ring" onClick={() => onDeploy({ ...sys, pendingCommit: selected.sha })}>
            <Icon name={selected.kind === "generation" ? "rollback" : "deploy"} size={13} />
            {selected.kind === "generation" ? ` Switch to gen #${selected.id}` : ` Deploy ${selected.sha}`}
          </button>
        </div>
      </section>
    </div>
    </div>
  );
}

/* ---------- History ---------- */
// Shared event model — consumed by both the timeline (HistoryTab) and the log
// stream (LogsTab) so "view logs" on an event lands on that exact line.
// Each event carries tsMin (minutes ago); labels derive from it so a long-lived
// system can generate an arbitrarily deep, deterministic history.
function _hseed(s) { let h = 2166136261; for (let i=0;i<s.length;i++){ h^=s.charCodeAt(i); h=Math.imul(h,16777619); } return h>>>0; }
function _hrng(a) { return function(){ a|=0; a=a+0x6D2B79F5|0; let t=Math.imul(a^a>>>15,1|a); t=t+Math.imul(t^t>>>7,61|t)^t; return ((t^t>>>14)>>>0)/4294967296; }; }
function relTime(min) {
  if (min < 1) return "just now";
  if (min < 60) return `${Math.round(min)}m ago`;
  const h = min/60; if (h < 24) return `${Math.round(h)}h ago`;
  const d = h/24;   if (d < 7) return `${Math.round(d)}d ago`;
  const w = d/7;    if (w < 8) return `${Math.round(w)}w ago`;
  const mo = d/30;  if (mo < 18) return `${Math.round(mo)}mo ago`;
  return `${(d/365).toFixed(1)}y ago`;
}
function _dur(min) { const h=Math.floor(min/60), m=Math.round(min%60); return h ? `${h}h ${m}m` : `${m}m`; }

function buildHistory(sys) {
  const G = sys.generation;
  const rnd = _hrng(_hseed(sys.id || sys.hostname || "x"));
  const pick = (a) => a[Math.floor(rnd()*a.length)];
  const out = []; let n = 0; const eid = () => "ev" + (n++);
  let tsMin = 0, gen = G;
  // source: "cf" = deployed through Crystal Forge; "local" = nixos-rebuild switch on
  // the host. A local rebuild may later RECONCILE to a pushed commit (resolution:
  // "matched") or stay untracked (resolution: "untracked").

  // ── curated recent head: the demo stories ──
  out.push({ id:eid(), type:"startup", tsMin: tsMin+=14,  ran:"14m",    gen, status:"success", by:"agent" });
  out.push({ id:eid(), type:"startup", tsMin: tsMin+=33,  ran:"32m",    gen, status:"success", by:"agent" });
  out.push({ id:eid(), type:"startup", tsMin: tsMin+=24,  ran:"19m",    gen, status:"success", by:"agent" });
  out.push({ id:eid(), type:"deploy",  source:"cf", tsMin: tsMin+=70,  dur:"43s",     gen, prevGen:gen-1, sha:"2s7cwd3", msg:"feat: grafana node exporter", status:"success", by:"ops-bot" }); gen--;
  out.push({ id:eid(), type:"startup", tsMin: tsMin+=120, ran:"5h 6m",  gen, status:"success", by:"agent" });
  // out-of-band, RECONCILED to a pushed commit (commit+push, then nixos-rebuild switch)
  out.push({ id:eid(), type:"deploy", source:"local", resolution:"matched", tsMin: tsMin+=180, dur:"1m 12s", gen, prevGen:gen-1,
             sha:"4e9a1c2", reconcileMin:2, storePath:"/nix/store/q4f8k2…-nixos-system-"+sys.hostname,
             msg:"nixos-rebuild switch on host", status:"success", by:"jpark@"+sys.hostname }); gen--;
  out.push({ id:eid(), type:"startup", tsMin: tsMin+=130, ran:"2h 10m", gen, status:"success", by:"agent" });
  out.push({ id:eid(), type:"deploy",  source:"cf", tsMin: tsMin+=60,  dur:"2m 11s",  gen, prevGen:gen-1, sha:"7c1209d", msg:"chore: bump nixpkgs lock (openssl CVE)", status:"success", by:"ops-bot" }); gen--;
  out.push({ id:eid(), type:"deploy",  source:"cf", tsMin: tsMin+=1600, dur:"0m 46s", gen:null, prevGen:gen, sha:"a1f2c31", msg:"fix: restart nginx on cert renewal", status:"failed", by:"jpark" });
  out.push({ id:eid(), type:"startup", tsMin: tsMin+=200, ran:"21h 4m", gen, status:"success", by:"agent" });
  out.push({ id:eid(), type:"deploy",  source:"cf", tsMin: tsMin+=2880, dur:"3m 02s", gen, prevGen:gen-1, sha:"91aa7d2", msg:"refactor: firewall module", status:"success", by:"dchen" }); gen--;
  // out-of-band, never committed → untracked
  out.push({ id:eid(), type:"deploy", source:"local", resolution:"untracked", tsMin: tsMin+=2000, dur:"0m 58s", gen, prevGen:gen-1,
             sha:null, storePath:"/nix/store/k1m9p3…-nixos-system-"+sys.hostname,
             msg:"nixos-rebuild switch (local debug — extra logging)", status:"success", by:"root@"+sys.hostname }); gen--;

  // ── procedurally generated older history (deterministic) ──
  const shaPool = ["b73c0aa","5d9e210","c0ffee1","9a8b7c6","de1e7ed","0badf00","1337c0d","face123","ab12cd3","7e4d9f0","3c5a1b8","e0f1a2b"];
  const msgPool = ["chore: bump nixpkgs lock","cve: patch openssl CVE-2026-118","fix: tighten sshd ciphers","feat: enable usbguard allow-list",
    "refactor: split networking module","chore: rotate deploy keys","fix: chrony authoritative servers","feat: node_exporter dashboards",
    "chore: prune old generations","cve: patch curl advisory","fix: luks unlock on data volume","chore: bump agent to 2.4","feat: audit rules expansion"];
  const authors = ["ops-bot","dchen","jpark","mreyes","akumar","ci-runner"];
  while (gen > 1 && out.length < 220) {
    const reboots = Math.floor(rnd()*4);
    for (let r=0;r<reboots;r++) out.push({ id:eid(), type:"startup", tsMin: tsMin+=Math.floor(45+rnd()*900), ran:_dur(30+rnd()*620), gen, status:"success", by:"agent" });
    tsMin += Math.floor(300+rnd()*5200);
    out.push({ id:eid(), type:"deploy", source:"cf", tsMin, dur:`${1+Math.floor(rnd()*3)}m ${Math.floor(rnd()*59)}s`, gen, prevGen:gen-1, sha:pick(shaPool), msg:pick(msgPool), status:"success", by:pick(authors) });
    gen--;
  }
  out.push({ id:eid(), type:"deploy", source:"cf", tsMin: tsMin+=6000, dur:"2m 47s", gen: Math.max(gen,1), prevGen:null, sha:"13a8f01", msg:"chore: initial import", status:"success", by:"mreyes" });

  out.forEach(e => { e.at = relTime(e.tsMin); });
  return out;
}

function HistoryTab({ sys, onRollback, onLogsJump, onOpenCommit }) {
  // Event model: deployments change the generation; startups are reboots of the SAME
  // generation. The timeline makes generation changes prominent and folds routine
  // restarts into collapsible clusters so they don't drown out what actually changed.
  const events = React.useMemo(() => buildHistory(sys), [sys.id]);

  // Fold consecutive startups into clusters; deploys stay standalone.
  const items = React.useMemo(() => {
    const out = []; let run = null;
    for (const e of events) {
      if (e.type === "startup") { (run ||= { kind:"restarts", list:[] }).list.push(e); }
      else { if (run) { out.push(run); run = null; } out.push({ kind:"event", ev:e }); }
    }
    if (run) out.push(run);
    return out;
  }, [events]);

  const [open, setOpen] = React.useState({});
  const deployCount = events.filter(e => e.type === "deploy").length;
  const restartCount = events.filter(e => e.type === "startup").length;

  // Infinite scroll: reveal clustered items a page at a time as the sentinel nears view.
  const PAGE = 14;
  const [count, setCount] = React.useState(PAGE);
  React.useEffect(() => { setCount(PAGE); }, [sys.id]);
  const sentinelRef = React.useRef(null);
  React.useEffect(() => {
    const el = sentinelRef.current;
    if (!el || count >= items.length) return;
    const io = new IntersectionObserver((ents) => {
      if (ents[0].isIntersecting) setCount(c => Math.min(c + PAGE, items.length));
    }, { rootMargin: "320px 0px" });
    io.observe(el);
    return () => io.disconnect();
  }, [items.length, count]);
  const shown = items.slice(0, count);
  const more = count < items.length;

  const statusChip = (s) =>
    s === "success" ? <span className="chip chip-healthy"><Icon name="check" size={10}/> success</span> :
    s === "failed"  ? <span className="chip chip-critical"><Icon name="x" size={10}/> failed</span> :
                      <span className="chip chip-unknown">cancelled</span>;

  // Deploy / local-rebuild / failed node — the prominent generation-changing events.
  const DeployRow = ({ e }) => {
    const failed = e.status === "failed";
    const local = e.source === "local";
    const matched = local && e.resolution === "matched";
    const untracked = local && e.resolution === "untracked";
    const accent = failed ? "var(--cf-red)" : untracked ? "var(--cf-amber)" : matched ? "var(--cf-blue)" : "var(--cf-brand-purple)";
    const kind = failed ? "Deploy failed" : local ? "Local rebuild" : "Deployed";
    const icon = failed ? "x" : local ? "edit" : "deploy";
    return (
      <div className="tl-row">
        <div className="tl-rail">
          <span className="tl-node" style={{ "--node": accent }}>
            <Icon name={icon} size={13}/>
          </span>
        </div>
        <div className="tl-body">
          <div className="tl-card" style={{ "--accent": accent }}>
            <div className="tl-card-head">
              <span className="tl-kind" style={{ color: accent }}>{kind}</span>
              {e.gen != null
                ? <span className="tl-gen">{e.prevGen != null ? <><span className="tl-gen-prev">#{e.prevGen}</span><Icon name="arrow-right" size={11}/></> : null}<strong>#{e.gen}</strong></span>
                : <span className="tl-gen tl-gen-none">no generation activated</span>}
              {local && <span className="tl-badge-oob">out of band</span>}
              {matched && <span className="tl-badge-reconciled"><Icon name="check" size={9}/> reconciled</span>}
              <span className="tl-spacer"/>
              {statusChip(e.status)}
            </div>
            <div className="tl-msg">{e.msg}</div>
            <div className="tl-meta">
              {untracked ? (
                <span className="tl-meta-item tl-untracked" title="Built locally with nixos-rebuild — no matching flake commit">
                  <Icon name="warn" size={11}/> no flake commit · <span className="mono">{e.storePath ? e.storePath.split("/").pop() : "untracked"}</span>
                </span>
              ) : e.sha ? (
                <button className="tl-commit-link mono focus-ring" title={`Open ${e.sha} in Flakes`}
                  onClick={ev => { ev.stopPropagation(); onOpenCommit?.({ sha:e.sha, msg:e.msg, flake:sys.flake, author:e.by, at:e.at }); }}>
                  <Icon name="git" size={11}/> {matched ? "matched " : ""}{e.sha} <Icon name="arrow-right" size={10}/>
                </button>
              ) : null}
              <span className="tl-meta-item"><Icon name="user" size={11}/> {e.by}</span>
              <span className="tl-meta-item">{failed ? "ran" : "built in"} <span className="mono">{e.dur}</span></span>
              <span className="tl-meta-item tl-when">{e.at}</span>
              <span className="tl-spacer"/>
              <div className="row-actions">
                <button className="btn-icon focus-ring" title="Jump to this event in logs" onClick={ev => { ev.stopPropagation(); onLogsJump?.(e.id); }}><Icon name="terminal" size={14}/></button>
                {e.gen != null && <button className="btn-icon focus-ring" title="Rollback to this generation" onClick={ev => { ev.stopPropagation(); onRollback?.(e.sha, e.gen); }}><Icon name="rollback" size={14}/></button>}
              </div>
            </div>
            {matched && (
              <div className="tl-oob-note tl-oob-resolved">
                <Icon name="git" size={12}/>
                <span>Activated on-host out of band, then reconciled to pushed commit <button className="tl-inline-sha mono focus-ring" onClick={ev => { ev.stopPropagation(); onOpenCommit?.({ sha:e.sha, flake:sys.flake, at:e.at }); }}>{e.sha}</button> ~{e.reconcileMin}m later. Config is tracked and reproducible.</span>
              </div>
            )}
            {untracked && (
              <div className="tl-oob-note">
                <Icon name="warn" size={12}/>
                <span>Built on the host, outside Crystal Forge — the running config doesn't map to any tracked flake commit. Capture it to a flake to restore reproducibility.</span>
                <button className="btn btn-ghost xs focus-ring" onClick={ev => { ev.stopPropagation(); onOpenCommit?.({ capture:true, flake:sys.flake, storePath:e.storePath }); }}>Capture to flake</button>
              </div>
            )}
          </div>
        </div>
      </div>
    );
  };

  // A single reboot line within a restart cluster.
  const RestartLine = ({ e }) => (
    <div className="tl-restart-line">
      <span className="tl-restart-dot"/>
      <Icon name="power" size={12} style={{ color:"var(--cf-blue)", flexShrink:0 }}/>
      <span className="tl-restart-label">System restarted</span>
      <span className="tl-restart-sep">·</span>
      <span className="tl-restart-ran">ran <span className="mono">{e.ran}</span></span>
      <span className="tl-spacer"/>
      <span className="tl-when">{e.at}</span>
    </div>
  );

  return (
    <section className="card" style={{ overflow:"hidden" }}>
      <div className="sd-card-head" style={{ padding:"14px 18px" }}>
        <h2>Deployment history</h2>
        <span className="sd-card-meta">{deployCount} deploys · {restartCount} restarts</span>
      </div>

      <div className="tl">
        {shown.map((it, i) => {
          if (it.kind === "event") return <DeployRow key={i} e={it.ev}/>;
          // restart cluster
          const list = it.list;
          if (list.length === 1) {
            return (
              <div className="tl-row" key={i}>
                <div className="tl-rail"><span className="tl-node tl-node-sm" style={{ "--node":"var(--cf-blue)" }}><Icon name="power" size={11}/></span></div>
                <div className="tl-body"><div className="tl-restart-single"><RestartLine e={list[0]}/></div></div>
              </div>
            );
          }
          const isOpen = open[i];
          const gen = list[0].gen;
          return (
            <div className="tl-row" key={i}>
              <div className="tl-rail"><span className="tl-node tl-node-sm" style={{ "--node":"var(--cf-blue)" }}><Icon name="power" size={11}/></span></div>
              <div className="tl-body">
                <button className="tl-cluster focus-ring" onClick={() => setOpen(o => ({ ...o, [i]: !o[i] }))} aria-expanded={isOpen}>
                  <Icon name={isOpen ? "chevron-down" : "chevron-right"} size={14} style={{ color:"var(--cf-text-muted)" }}/>
                  <span className="tl-cluster-count">{list.length} restarts</span>
                  <span className="tl-restart-sep">·</span>
                  <span className="tl-restart-label">generation <span className="mono">#{gen}</span> held steady</span>
                  <span className="tl-spacer"/>
                  <span className="tl-when">{list[list.length-1].at} – {list[0].at}</span>
                </button>
                {isOpen && <div className="tl-cluster-list">{list.map((e, j) => <RestartLine e={e} key={j}/>)}</div>}
              </div>
            </div>
          );
        })}
        {more && (
          <div className="tl-row tl-sentinel" ref={sentinelRef}>
            <div className="tl-rail"><span className="tl-node tl-node-sm tl-node-load"><Icon name="sync" size={11}/></span></div>
            <div className="tl-body"><div className="tl-loadmore">Loading older history… <span className="tl-loadmore-count">{count} of {items.length}</span></div></div>
          </div>
        )}
      </div>
    </section>
  );
}

/* ---------- Logs ---------- */
function LogsTab({ sys, jump }) {
  const [filter, setFilter] = React.useState("all");
  const [tail, setTail] = React.useState(true);
  const [hl, setHl] = React.useState(null);   // highlighted event id
  const [tz, setTz] = React.useState("local"); // "local" | "utc"
  const scrollRef = React.useRef(null);
  const useUTC = tz === "utc";

  // Resolve a human label for the local zone (e.g. "PDT", "GMT+2").
  const localAbbr = React.useMemo(() => {
    try {
      return new Intl.DateTimeFormat(undefined, { timeZoneName: "short" })
        .formatToParts(new Date()).find(p => p.type === "timeZoneName")?.value || "local";
    } catch { return "local"; }
  }, []);
  const tzLabel = useUTC ? "UTC" : localAbbr;

  const p2 = (n) => String(n).padStart(2, "0");
  const dayKey = (d) => useUTC
    ? `${d.getUTCFullYear()}-${p2(d.getUTCMonth()+1)}-${p2(d.getUTCDate())}`
    : `${d.getFullYear()}-${p2(d.getMonth()+1)}-${p2(d.getDate())}`;
  const fmtT = (d) => useUTC
    ? `${p2(d.getUTCHours())}:${p2(d.getUTCMinutes())}:${p2(d.getUTCSeconds())}`
    : `${p2(d.getHours())}:${p2(d.getMinutes())}:${p2(d.getSeconds())}`;
  const today = new Date();
  const TODAY_KEY = dayKey(today);
  const YDAY_KEY = dayKey(new Date(today.getTime() - 86400000));
  const dayLabel = (key) => {
    let label;
    if (key === TODAY_KEY) label = "Today";
    else if (key === YDAY_KEY) label = "Yesterday";
    else { const [y,m,dd] = key.split("-").map(Number); label = new Date(y, m-1, dd).toLocaleDateString(undefined, { weekday:"short", month:"short", day:"numeric", year:"numeric" }); }
    return label;
  };

  // Build the log stream FROM the deployment history so every timeline event has a
  // real line to jump to. Each event expands into a small cluster; the key line
  // carries ev:<id> so "view logs" can scroll straight to it.
  const baseLines = React.useMemo(() => {
    const out = [];
    // Live logs only cover the recent window; jumps target recent events.
    const events = buildHistory(sys).slice(0, 24).reverse(); // oldest → newest
    for (const e of events) {
      const base = new Date(Date.now() - (e.tsMin || 0) * 60000);
      const push = (off, lvl, m, anchor) => {
        const d = new Date(base.getTime() + off * 1000);
        out.push({ lvl, m, t: fmtT(d), d: dayKey(d), sort: d.getTime(), ev: anchor ? e.id : undefined });
      };
      const shortPath = e.storePath ? e.storePath.split("/").pop() : "";
      if (e.type === "startup") {
        push(0, "info", `systemd: reached target multi-user.target`);
        push(2, "info", `agent: boot recorded — generation #${e.gen} (ran ${e.ran})`, true);
        push(4, "info", `heartbeat received (next in ${sys.heartbeatIntervalSec}s)`);
      } else if (e.source === "local") {
        push(0, "warn", `agent: out-of-band activation detected on ${sys.hostname}`);
        push(2, "info", `local: nixos-rebuild switch by ${e.by} — ${e.msg}`);
        if (e.resolution === "matched") {
          push(5, "info", `agent: generation #${e.gen} activated out of band (store-path ${shortPath})`, true);
          push(7, "info", `reconcile: store-path matches pushed commit ${e.sha} — config is tracked`);
        } else {
          push(5, "warn", `agent: generation #${e.gen} activated locally — no flake commit (store-path ${shortPath})`, true);
          push(7, "warn", `drift: running config no longer maps to a tracked flake revision`);
        }
      } else if (e.status === "failed") {
        push(0, "info", `deploy: evaluating ${sys.flake}#nixosConfigurations.${sys.hostname} @ ${e.sha}`);
        push(4, "error", `activation failed: ${e.msg}`, true);
        push(6, "warn", `deploy: rolled back to generation #${e.prevGen}`);
      } else {
        push(0, "info", `deploy: evaluating ${sys.flake}#nixosConfigurations.${sys.hostname} @ ${e.sha}`);
        push(2, "info", `eval: success — derivations resolved, building`);
        push(5, "info", `build: completed in ${e.dur}`);
        push(7, "info", `deploy: activating configuration`);
        push(9, "info", `deploy: generation #${e.gen} activated (${e.sha})`, true);
      }
    }
    out.sort((a,b) => a.sort - b.sort);
    return out;
  }, [sys.id]);

  const [tailLines, setTailLines] = React.useState([]);

  React.useEffect(() => {
    if (!tail) return;
    const id = setInterval(() => {
      setTailLines(prev => {
        const now = new Date();
        const variants = [
          { lvl: "info", m: `heartbeat received (next in ${sys.heartbeatIntervalSec}s)` },
          { lvl: "info", m: `agent: state snapshot dispatched (seq=${Math.floor(Math.random()*10000)})` },
          { lvl: "info", m: `policy: ${sys.deploymentPolicy} — passed` },
        ];
        const v = variants[Math.floor(Math.random() * variants.length)];
        return [...prev, { ...v, t: fmtT(now), d: dayKey(now), sort: now.getTime() }].slice(-40);
      });
    }, 2200);
    return () => clearInterval(id);
  }, [tail, sys.id]);

  const lines = React.useMemo(() => [...baseLines, ...tailLines], [baseLines, tailLines]);
  const filtered = filter === "all" ? lines : lines.filter(l => l.lvl === filter);

  // Jump: when an event id arrives from the History tab, stop tailing, scroll to the
  // anchored line and flash-highlight it.
  React.useEffect(() => {
    if (!jump?.id) return;
    setTail(false);
    setFilter("all");
    const t = setTimeout(() => {
      const box = scrollRef.current;
      const el = box?.querySelector(`[data-ev="${jump.id}"]`);
      if (box && el) {
        box.scrollTop = el.offsetTop - box.clientHeight / 2 + el.clientHeight;
        setHl(jump.id);
        setTimeout(() => setHl(h => (h === jump.id ? null : h)), 2400);
      }
    }, 60);
    return () => clearTimeout(t);
  }, [jump?.id, jump?.nonce]);

  // Tail auto-scroll (only while tailing).
  React.useEffect(() => {
    if (tail && scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [tailLines, tail]);

  return (
    <section className="card sd-logs-card">
      <div className="sd-card-head" style={{ padding: "14px 18px" }}>
        <h2>Live logs</h2>
        <div className="sd-logs-controls">
          <div className="seg seg-tz" title="Timestamp timezone">
            <button className={!useUTC ? "active" : ""} onClick={() => setTz("local")}>{localAbbr}</button>
            <button className={useUTC ? "active" : ""} onClick={() => setTz("utc")}>UTC</button>
          </div>
          <div className="seg">
            {["all", "info", "warn", "error"].map(f => (
              <button key={f} className={filter === f ? "active" : ""} onClick={() => setFilter(f)}>{f}</button>
            ))}
          </div>
          <label className="sd-toggle">
            <input type="checkbox" checked={tail} onChange={e => setTail(e.target.checked)} />
            <span>tail</span>
          </label>
          <button className="btn btn-ghost xs focus-ring" onClick={() => { setTailLines([]); }}>
            Clear
          </button>
          <button className="btn btn-ghost xs focus-ring">
            <Icon name="download" size={11} /> Download
          </button>
        </div>
      </div>
      <div className="sd-log-tzbar"><Icon name="history" size={11}/> Timestamps shown in <strong>{tzLabel}</strong></div>
      <pre ref={scrollRef} className="sd-log-stream">
        {filtered.map((l, i) => {
          const d = new Date(l.sort);
          const dk = dayKey(d);
          const prev = filtered[i - 1];
          const showDay = i === 0 || !prev || dayKey(new Date(prev.sort)) !== dk;
          return (
            <React.Fragment key={i}>
              {showDay && (
                <div className="sd-log-day" role="separator">
                  <span className="sd-log-day-label">{dayLabel(dk)}</span>
                </div>
              )}
              <div className={`sd-log-line sd-log-${l.lvl}${hl && l.ev === hl ? " sd-log-hl" : ""}`} data-ev={l.ev}>
                <span className="sd-log-t">{fmtT(d)}</span>
                <span className="sd-log-lvl">{l.lvl.toUpperCase()}</span>
                <span className="sd-log-m">{l.m}</span>
              </div>
            </React.Fragment>
          );
        })}
        {tail && <div className="sd-log-caret">▍</div>}
      </pre>
    </section>
  );
}

/* ---------- Config ---------- */
function ConfigTab({ sys }) {
  return (
    <div className="sd-grid sd-grid-config">
      <section className="card sd-card">
        <div className="sd-card-head">
          <h2>Rendered module</h2>
          <span className="sd-card-meta mono">{sys.flake}#nixosConfigurations.{sys.hostname}</span>
        </div>
        <pre className="sd-nix">{`{ config, pkgs, lib, ... }: {
  networking.hostName = "${sys.hostname}";
  networking.domain = "${sys.environment}.cf.internal";

  crystal-forge.client = {
    enable = true;
    server_host = "crystal-forge.internal";
    environment = "${sys.environment}";
  };

  crystal-forge.stig = {
    banner.enable = true;
    sshd_hardening.enable = true;
    audit_rules.enable = true;
    # ${sys.environment === "production" ? "28" : "22"} STIG controls active
  };

  services.openssh.enable = true;
  services.prometheus.exporters.node.enable = true;

  deploymentPolicy = "${sys.deploymentPolicy}";
  system.stateVersion = "${sys.nixosVersion.slice(0, 5)}";
}`}</pre>
      </section>

      <section className="card sd-card">
        <div className="sd-card-head">
          <h2>Drift</h2>
          <span className="chip chip-healthy"><Icon name="check" size={10} /> in sync</span>
        </div>
        <div className="sd-drift-row">
          <span className="sd-drift-label">Evaluated config</span>
          <span className="mono sd-drift-val">{sys.commit}</span>
        </div>
        <div className="sd-drift-row">
          <span className="sd-drift-label">Running config</span>
          <span className="mono sd-drift-val">{sys.commit}</span>
        </div>
        <div className="sd-drift-row">
          <span className="sd-drift-label">Agent fingerprint</span>
          <span className="mono sd-drift-val">matches</span>
        </div>
        <div className="sd-callout sd-callout-info" style={{ marginTop: 14 }}>
          <Icon name="check" size={13} />
          <div>No configuration drift detected in the last 7 days.</div>
        </div>
      </section>
    </div>
  );
}

/* ---------- CVEs ---------- */
function CvesTab({ sys }) {
  const cves = React.useMemo(() => {
    const n = Math.min(18, sys.cves.critical + sys.cves.high + Math.min(6, sys.cves.medium));
    const levels = [
      ...Array(sys.cves.critical).fill("critical"),
      ...Array(sys.cves.high).fill("high"),
      ...Array(Math.min(6, sys.cves.medium)).fill("medium"),
    ].slice(0, n);
    const pkgs = ["openssl", "linux-kernel", "curl", "glibc", "systemd", "nginx", "postgresql", "git", "python311"];
    const pkgVersion = {};
    return levels.map((lvl, i) => {
      const pkg = pkgs[i % pkgs.length];
      if (!pkgVersion[pkg]) pkgVersion[pkg] = `${Math.floor(Math.random()*10)}.${Math.floor(Math.random()*20)}.${Math.floor(Math.random()*30)}`;
      return {
        id: `CVE-2025-${String(10000 + Math.floor(Math.random() * 9999)).padStart(5,"0")}`,
        level: lvl,
        pkg,
        version: pkgVersion[pkg],
        score: (lvl === "critical" ? 9 + Math.random() : lvl === "high" ? 7 + Math.random()*2 : 4 + Math.random()*3).toFixed(1),
        fix: Math.random() > 0.3 ? "available" : "pending",
      };
    });
  }, [sys.id]);

  const chipFor = (l) =>
    l === "critical" ? <span className="chip chip-critical">critical</span> :
    l === "high" ? <span className="chip chip-warning">high</span> :
                   <span className="chip chip-unknown">medium</span>;

  // Group by package (mirrors the CVEs view)
  const groups = React.useMemo(() => {
    const sevWeight = { critical: 1000, high: 100, medium: 10, low: 1 };
    const m = new Map();
    cves.forEach(c => { if (!m.has(c.pkg)) m.set(c.pkg, []); m.get(c.pkg).push(c); });
    return [...m.entries()].map(([pkg, list]) => {
      const counts = { critical: 0, high: 0, medium: 0, low: 0 };
      let fixable = 0, maxScore = 0, version = list[0].version;
      list.forEach(c => { counts[c.level] += 1; if (c.fix === "available") fixable += 1; if (parseFloat(c.score) > maxScore) maxScore = parseFloat(c.score); });
      const score = list.reduce((a, c) => a + sevWeight[c.level], 0);
      return { pkg, list, counts, fixable, maxScore, version, score };
    }).sort((a, b) => b.score - a.score);
  }, [cves]);

  const [expanded, setExpanded] = React.useState(null);
  React.useEffect(() => { if (groups.length && expanded == null) setExpanded(groups[0].pkg); }, [groups]);

  return (
    <section className="card" style={{ overflow: "hidden" }}>
      <div className="sd-card-head" style={{ padding: "14px 18px" }}>
        <h2>Vulnerabilities</h2>
        <span className="sd-card-meta">{cves.length} of {sys.cves.total} shown · {groups.length} package{groups.length === 1 ? "" : "s"}</span>
      </div>
      {cves.length === 0 ? (
        <div className="empty">
          <h3>No vulnerabilities detected</h3>
          <div>Last scan: 2h ago.</div>
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 10, padding: 14 }}>
          {groups.map(g => {
            const sevColor = g.counts.critical > 0 ? "#f87171" : g.counts.high > 0 ? "#fbbf24" : g.counts.medium > 0 ? "#60a5fa" : "#9ca3af";
            const isOpen = expanded === g.pkg;
            return (
              <div key={g.pkg} className="card" style={{ overflow: "hidden" }}>
                <button className="focus-ring" onClick={() => setExpanded(isOpen ? null : g.pkg)}
                  style={{ all: "unset", display: "grid", gridTemplateColumns: "24px 1fr auto", alignItems: "center", gap: 14, padding: "12px 16px", cursor: "pointer", width: "100%", boxSizing: "border-box", borderLeft: `3px solid ${sevColor}`, background: isOpen ? "color-mix(in oklab,var(--cf-brand-purple) 6%,var(--cf-card-bg))" : "transparent" }}>
                  <Icon name={isOpen ? "chevron-down" : "chevron-right"} size={14} style={{ color: "var(--cf-text-muted)" }} />
                  <div style={{ minWidth: 0 }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                      <span className="mono" style={{ fontSize: 14, fontWeight: 700 }}>{g.pkg}</span>
                      <span className="mono" style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{g.version}</span>
                      <span style={{ fontSize: 12, color: "var(--cf-text-muted)" }}>{g.list.length} CVE{g.list.length === 1 ? "" : "s"}</span>
                    </div>
                    <div style={{ fontSize: 11, color: "var(--cf-text-secondary)", marginTop: 2 }}>
                      max CVSS {g.maxScore.toFixed(1)} · {g.fixable} patchable · {g.list.length - g.fixable} pending
                    </div>
                  </div>
                  <div style={{ display: "flex", gap: 5, flexWrap: "wrap", justifyContent: "flex-end" }}>
                    {g.counts.critical > 0 && <span className="chip chip-critical" style={{ fontSize: 10 }}>{g.counts.critical} crit</span>}
                    {g.counts.high > 0 && <span className="chip chip-warning" style={{ fontSize: 10 }}>{g.counts.high} high</span>}
                    {g.counts.medium > 0 && <span className="chip chip-unknown" style={{ fontSize: 10 }}>{g.counts.medium} med</span>}
                  </div>
                </button>
                {isOpen && (
                  <table className="sys-table">
                    <thead>
                      <tr>
                        <th>CVE</th>
                        <th>Severity</th>
                        <th>CVSS</th>
                        <th>Fix</th>
                        <th style={{ textAlign: "right" }}> </th>
                      </tr>
                    </thead>
                    <tbody>
                      {g.list.map(c => (
                        <tr key={c.id}>
                          <td className="mono" style={{ color: "var(--cf-text-primary)" }}>{c.id}</td>
                          <td>{chipFor(c.level)}</td>
                          <td className="mono">{c.score}</td>
                          <td>
                            {c.fix === "available"
                              ? <span className="chip chip-healthy">available</span>
                              : <span className="chip chip-unknown">pending</span>}
                          </td>
                          <td>
                            <div className="row-actions">
                              <button className="btn-icon focus-ring" title="Open advisory" onClick={() => window.open(`https://nvd.nist.gov/vuln/detail/${c.id}`, '_blank')}><Icon name="link" size={14} /></button>
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

// Compliance tab — auditor's view: which bundles apply, the system's score per bundle,
// and a click-through to per-control evidence (reuses the Compliance view's drawer).
function ComplianceTab({ sys, onNavigate }) {
  const [drawerBundle, setDrawerBundle] = React.useState(null);
  const bundles = (typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : []);
  const applicable = bundles
    .map(b => ({ bundle: b, rollup: bundleStatusForSystem(b, sys) }))
    .filter(x => x.rollup.applies);

  const scoreColor = (s) => s >= 90 ? "#34d399" : s >= 70 ? "#fbbf24" : "#f87171";

  if (applicable.length === 0) {
    return (
      <div className="empty" style={{ margin:0 }}>
        <h3>No compliance bundles apply</h3>
        <div>No bundle targets the <span className="mono">{sys.environment}</span> environment. Assign one from the Compliance view or environment settings.</div>
      </div>
    );
  }

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:14 }}>
      <div className="sd-callout sd-callout-info">
        <Icon name="shield" size={13}/>
        <div style={{ fontSize:12 }}>
          Evidence of compliance for <span className="mono" style={{ fontWeight:600 }}>{sys.hostname}</span>. Open any bundle to step through its controls and the proof collected for each — config output, systemd unit state, audit results, and waivers.
        </div>
      </div>

      {applicable.map(({ bundle, rollup }) => {
        const compliant = rollup.fail === 0;
        return (
          <div key={bundle.id} className="card" style={{ padding:16 }}>
            <div style={{ display:"flex", alignItems:"flex-start", justifyContent:"space-between", gap:14, flexWrap:"wrap" }}>
              <div style={{ minWidth:0 }}>
                <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                  <span style={{ fontSize:15, fontWeight:650 }}>{bundle.name}</span>
                  <span className="chip chip-info" style={{ fontSize:10 }}>{bundle.framework}</span>
                  <span className="chip chip-unknown" style={{ fontSize:10 }}>{bundle.version}</span>
                  {compliant
                    ? <span className="chip chip-healthy" style={{ fontSize:10 }}><Icon name="check" size={9}/> Compliant</span>
                    : <span className="chip chip-critical" style={{ fontSize:10 }}><Icon name="warn" size={9}/> {rollup.fail} failing</span>}
                </div>
                <div style={{ fontSize:12, color:"var(--cf-text-muted)", marginTop:4 }}>{bundle.policyIds.length} controls · owned by <span className="mono">{bundle.owner}</span></div>
              </div>
              <button className="btn btn-primary focus-ring" onClick={() => setDrawerBundle(bundle)}>
                <Icon name="file" size={13}/> View evidence
              </button>
            </div>

            {/* Score + breakdown */}
            <div style={{ display:"flex", alignItems:"center", gap:16, marginTop:14, flexWrap:"wrap" }}>
              <div style={{ display:"flex", alignItems:"center", gap:10 }}>
                <div style={{ width:120, height:8, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden" }}>
                  <div style={{ width:`${rollup.score}%`, height:"100%", background: scoreColor(rollup.score) }}/>
                </div>
                <span className="mono" style={{ fontSize:14, fontWeight:700, color: scoreColor(rollup.score) }}>{rollup.score}%</span>
              </div>
              <div style={{ display:"flex", gap:14, fontSize:12 }}>
                <span><span className="mono" style={{ fontWeight:700, color:"#34d399" }}>{rollup.pass}</span> <span style={{ color:"var(--cf-text-muted)" }}>pass</span></span>
                <span><span className="mono" style={{ fontWeight:700, color: rollup.warn > 0 ? "#fbbf24" : "var(--cf-text-muted)" }}>{rollup.warn}</span> <span style={{ color:"var(--cf-text-muted)" }}>warn</span></span>
                <span><span className="mono" style={{ fontWeight:700, color: rollup.fail > 0 ? "#f87171" : "var(--cf-text-muted)" }}>{rollup.fail}</span> <span style={{ color:"var(--cf-text-muted)" }}>fail</span></span>
                <span><span className="mono" style={{ fontWeight:700, color: rollup.waiver > 0 ? "#a78bfa" : "var(--cf-text-muted)" }}>{rollup.waiver}</span> <span style={{ color:"var(--cf-text-muted)" }}>waiver</span></span>
              </div>
            </div>
          </div>
        );
      })}

      {drawerBundle && window.ControlsEvidenceDrawer && (
        <window.ControlsEvidenceDrawer
          bundle={drawerBundle}
          sys={sys}
          showSystemLink={false}
          onClose={() => setDrawerBundle(null)}
          onOpenSystem={() => setDrawerBundle(null)}
          onOpenBundle={(b) => { setDrawerBundle(null); onNavigate?.("compliance", b.id); }}
        />
      )}
    </div>
  );
}

Object.assign(window, { SystemDetail });
