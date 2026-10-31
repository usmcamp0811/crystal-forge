// Management-mode building blocks: chips, evidence pipeline, runtime observations, mode picker.
const MG_STATE = {
  ok: "#34d399", success: "#34d399", current: "#34d399", complete: "#34d399",
  running: "#60a5fa", building: "#60a5fa", queued: "#60a5fa", scanning: "#60a5fa",
  stale: "#fbbf24", failed: "#f87171", unavailable: "#f87171", missing: "#f87171",
  none: "#6b7280", unknown: "#6b7280",
};
const MG_EXTRA_CONFIGS = {
  infrastructure: ["golden-workstation", "server-baseline", "dev-workstation", "ops-bastion"],
  "web-services": ["approved-vm", "vm-wiki"], "lab-nodes": ["iso-installer"], "edge-gateway": ["edge-sfo-01"], "build-farm": ["vm-ci-runner"],
};
function declaredConfigs(flake) {
  const s = (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).filter((x) => x.flake === flake && !(x.assurance && x.assurance.configState === "missing")).map((x) => x.configName || x.hostname);
  return [...new Set([...s, ...(MG_EXTRA_CONFIGS[flake] || [])])];
}

function MgmtChip({ sys, mode, dot }) {
  const m = mode || mgmtOf(sys);
  const meta = MGMT_META[m];
  return <span className={`chip mg-chip mg-${m}`} title={meta.desc}><Icon name={meta.icon} size={10} />{meta.label}</span>;
}
function ScanEvidenceChip({ sys }) {
  const a = sys.assurance;
  if (!a) return null;
  const m = SCAN_EVIDENCE_META[a.scan.state] || SCAN_EVIDENCE_META.missing;
  return <span className={`chip ${m.cls}`}><span className="chip-dot" style={{ background: m.color }} />{m.label}</span>;
}
function hasCveData(sys) {
  const a = sys.assurance;
  return !a || !["missing", "unavailable"].includes(a.scan.state);
}
function EvidenceCves({ sys, compact }) {
  if (!hasCveData(sys)) return <span className="chip chip-unknown" title="No completed scan for this configuration">no scan</span>;
  const stale = sys.assurance && sys.assurance.scan.state !== "current";
  return <span style={{ display: "inline-flex", gap: 6, flexWrap: "wrap", opacity: stale ? 0.75 : 1 }}><CveChips cves={sys.cves} compact={compact} />{stale && <span className="chip chip-unknown" title="From an earlier or incomplete scan">{sys.assurance.scan.state === "failed" ? "last good" : "historical"}</span>}</span>;
}
function RuntimeLine({ sys }) {
  if (mgmtOf(sys) === "reference") return <span className="mg-dim">Not a running machine</span>;
  const r = sys.runtime;
  if (!r) return <span className="mg-dim">Runtime unknown</span>;
  return <span className="mg-obs"><span className="chip-dot" style={{ background: r.unitState === "active" ? "#60a5fa" : "#fbbf24" }} />host-observed · unit {r.unitState} · {r.observedAt}</span>;
}
function DeploymentCell({ sys }) {
  const m = mgmtOf(sys);
  if (m === "managed") return <DeploymentChip state={sys.deploymentState} />;
  return <span className="chip chip-unknown">{m === "external" ? "deployed externally" : "not deployed"}</span>;
}

function MgmtNotice({ icon = "info", title, children, actions, tone = "info" }) {
  return (
    <div className={`mg-notice mg-notice-${tone}`}>
      <Icon name={icon} size={16} />
      <div className="mg-notice-body"><div className="mg-notice-title">{title}</div>{children && <div className="mg-notice-text">{children}</div>}{actions && <div className="mg-notice-actions">{actions}</div>}</div>
    </div>
  );
}

// Banner above CVE / Hardening / Compliance content
function EvidenceBanner({ sys, what = "Findings" }) {
  const a = sys.assurance; if (!a) return null;
  const m = mgmtOf(sys), s = a.scan, mismatch = s.rev && s.rev !== a.rev.sha;
  let tone = "info", title = `${what} describe configuration ${a.configName} at ${s.rev || a.rev.sha}${s.at ? ` · scanned ${s.at}` : ""}`;
  let text = m === "external" ? "This is evidence about the configuration, not about what the system is running. Runtime state is not verified." : "Evidence for a reference configuration. No machine is associated with it.";
  if (a.configState === "missing") { tone = "danger"; title = `Configuration ${a.configName} no longer exists in the flake`; text = `Showing historical evidence from revision ${s.rev} (${s.at}). Nothing at ${a.rev.sha} can be evaluated.`; }
  else if (s.state === "failed") { tone = "warn"; title = `Scan failed ${s.at}. Showing the last good scan (${s.lastGood.rev}, ${s.lastGood.at})`; text = s.error; }
  else if (s.state === "stale") { tone = "warn"; title = `${what} are from a ${s.at} scan and may be out of date`; text = "The selected revision has changed or the scan is past its freshness window."; }
  else if (mismatch) { tone = "warn"; title = `Scanned revision ${s.rev} differs from selected revision ${a.rev.sha}`; }
  return <MgmtNotice icon={tone === "info" ? "info" : "warn"} tone={tone} title={title}>{text}</MgmtNotice>;
}

// Replaces a tab's body when there is no evidence to show
function EvidenceGate({ sys, what, children }) {
  const a = sys.assurance;
  const [req, setReq] = React.useState(false);
  if (!a || hasCveData(sys)) return <><EvidenceBanner sys={sys} what={what} />{children}</>;
  const st = a.scan.state, building = ["none", "queued", "building"].includes(a.build.state);
  return (
    <div className="mg-empty">
      <Icon name={building ? "build" : "warn"} size={22} />
      <h3>{building ? "Waiting for a build" : st === "unavailable" ? "Scanning isn't available for this configuration" : "No scan evidence yet"}</h3>
      <p>{building ? `${what} appear after the build of revision ${a.rev.sha} completes and is scanned.` : st === "unavailable" ? a.scan.error : `No completed scan exists for ${a.configName} at ${a.rev.sha}.`}</p>
      {!building && st !== "unavailable" && <button className="btn btn-ghost focus-ring" disabled={req} onClick={() => setReq(true)}>{req ? <><Spinner size={12} /> Scan requested</> : <><Icon name="shield" size={12} /> Request scan</>}</button>}
    </div>
  );
}

function NotApplicable({ icon = "info", title, children, actions }) {
  return <div className="mg-empty mg-na"><Icon name={icon} size={22} /><h3>{title}</h3><p>{children}</p>{actions && <div className="mg-notice-actions" style={{ justifyContent: "center" }}>{actions}</div>}</div>;
}

function PipelineStage({ label, state, main, sub, onClick, warn }) {
  const c = MG_STATE[state] || MG_STATE.none;
  return (
    <button className="mg-stage focus-ring" onClick={onClick} disabled={!onClick}>
      <span className="mg-stage-label">{label}</span>
      <span className="mg-stage-main"><span className="chip-dot" style={{ background: c }} />{main}</span>
      <span className="mg-stage-sub">{sub}</span>
      {warn && <span className="mg-stage-warn">{warn}</span>}
    </button>
  );
}

function EvidencePipeline({ sys, onOpenCommit, onNavigate, onViewCves }) {
  const a = sys.assurance, s = a.scan, mism = s.rev && s.rev !== a.rev.sha;
  const ev = a.eval, b = a.build;
  return (
    <div className="mg-pipeline">
      <PipelineStage label="Selected revision" state={a.configState === "missing" ? "failed" : "ok"} main={<span className="mono">{a.rev.sha}</span>} sub={`${sys.branch} · ${a.rev.at}`} warn={a.configState === "missing" ? "config not found" : null}
        onClick={() => onOpenCommit?.({ sha: a.rev.sha, msg: a.rev.msg, flake: sys.flake, at: a.rev.at })} />
      <PipelineStage label="Evaluation" state={ev.state === "success" ? "success" : "failed"} main={ev.state === "success" ? "Evaluated" : "Failed"} sub={ev.at} warn={ev.error} onClick={() => onNavigate?.("evals", { sha: a.rev.sha, flake: sys.flake })} />
      <PipelineStage label="Build" state={b.state === "success" ? "success" : b.state} main={{ success: "Built", building: "Building", queued: "Queued", none: "Not built", failed: "Failed" }[b.state]} sub={b.at || (ev.state === "failed" ? "blocked by evaluation" : "—")} onClick={() => onNavigate?.("builds", { flake: sys.flake })} />
      <PipelineStage label="Scan" state={s.state === "current" ? "success" : s.state} main={(SCAN_EVIDENCE_META[s.state] || {}).label} sub={s.at ? `${s.at} · rev ${s.rev}` : "—"}
        warn={mism ? `scan is for ${s.rev}, not the selected revision` : s.state === "failed" ? "last good scan " + s.lastGood.at : null} onClick={onViewCves} />
    </div>
  );
}

function ObservationRow({ k, children, note }) {
  return <><dt>{k}</dt><dd style={{ whiteSpace: "normal", fontFamily: "var(--font-sans)" }}>{children}{note && <div className="mg-obs-note">{note}</div>}</dd></>;
}
// Host-reported observations, kept apart from verified guest state
function RuntimeCard({ sys, onOpenSystem }) {
  const r = sys.runtime, a = sys.assurance;
  const [phase, setPhase] = React.useState("idle");
  const refresh = () => { setPhase("loading"); setTimeout(() => setPhase(sys.hostname === "vm-gitea" ? "error" : "idle"), 900); };
  const parent = sys.parentHost && SYSTEMS.find((x) => x.hostname === sys.parentHost);
  return (
    <div className="card sd-card">
      <div className="sd-card-head"><h2>Runtime observations</h2>
        {sys.parentHost && <button className="btn btn-ghost xs focus-ring" onClick={refresh} disabled={phase === "loading"}>{phase === "loading" ? <><Spinner size={11} /> Querying host</> : <><Icon name="sync" size={11} /> Refresh</>}</button>}
      </div>
      {phase === "error" && <MgmtNotice icon="warn" tone="warn" title={`${sys.parentHost} did not respond`}>The host query timed out. Showing the last observation from {r.observedAt}.</MgmtNotice>}
      {!r ? (
        <div className="mg-unknown">
          <div className="mg-unknown-title"><span className="chip-dot" style={{ background: "#6b7280" }} /> Runtime state unknown</div>
          <p>{sys.hostGap || "No runtime information is available."} The scan results above describe the configuration at {a.rev.sha}. They do not show what is running.</p>
          {!sys.parentHost && <p className="mg-dim">Associate a parent host in Edit to receive host-reported observations.</p>}
        </div>
      ) : (
        <>
          <div className="mg-obs-head"><Icon name="server" size={12} /> Reported by host <b>{r.parentHost}</b> · {r.observedAt}</div>
          <dl className="kv-grid">
            <ObservationRow k="Parent host">{r.parentHost} <span className="chip chip-unknown" style={{ marginLeft: 6 }}>CF-managed</span>{parent && onOpenSystem && <button className="link-action" style={{ marginLeft: 8 }} onClick={() => onOpenSystem(parent)}>Open host</button>}</ObservationRow>
            <ObservationRow k="Guest name"><span className="mono">{r.guest}</span></ObservationRow>
            <ObservationRow k="Systemd unit"><span className="mono">{r.unit}</span> <span className={`chip ${r.unitState === "active" ? "chip-info" : "chip-warning"}`} style={{ marginLeft: 6 }}>{r.unitState}</span></ObservationRow>
            <ObservationRow k="Runner store path"><span className="mono" style={{ fontSize: 11, wordBreak: "break-all" }}>{r.runnerPath}</span></ObservationRow>
            <ObservationRow k="Compared with scan">
              {r.matchesScanned === true && <span className="chip chip-info">runner path matches scanned build</span>}
              {r.matchesScanned === false && <span className="chip chip-warning">runner path differs from scanned build</span>}
              {r.matchesScanned == null && <span className="chip chip-unknown">not comparable</span>}
            </ObservationRow>
          </dl>
          <div className="mg-unknown">
            <div className="mg-unknown-title"><span className="chip-dot" style={{ background: "#6b7280" }} /> Verified guest state: not available</div>
            <p>{r.unitState === "active" ? "An active systemd unit shows the host started the guest. It does not show the guest is healthy or running the scanned configuration." : "The host reports the unit is not active. The guest's own state is not verified."}{r.matchesScanned === false ? " The host reports a different runner than the scanned build, so the scan may not describe what is running." : ""}</p>
          </div>
        </>
      )}
    </div>
  );
}

function ControlCard({ sys }) {
  const m = mgmtOf(sys), a = sys.assurance;
  return (
    <div className="card sd-card">
      <div className="sd-card-head"><h2>{m === "external" ? "Deployment control" : "Purpose"}</h2><MgmtChip sys={sys} /></div>
      <p className="mg-copy">{m === "external"
        ? (sys.parentHost ? `Deployed by parent host ${sys.parentHost}. Crystal Forge does not deploy, roll back or restart this system.` : "Deployed by its own administrators. Crystal Forge does not deploy or roll back this system.")
        : "A configuration tracked for security assurance. It is built and scanned on each revision, and is never deployed."}</p>
      <dl className="kv-grid">
        <dt>Flake</dt><dd>{sys.flake}</dd>
        <dt>Configuration</dt><dd>nixosConfigurations.{a.configName}</dd>
        <dt>Branch</dt><dd>{sys.branch}</dd>
        <dt>Environment</dt><dd>{sys.environment}</dd>
        {sys.parentHost && <><dt>Parent host</dt><dd>{sys.parentHost}</dd></>}
        {a.build.storePath && <><dt>Built output</dt><dd style={{ wordBreak: "break-all", fontSize: 11 }}>{a.build.storePath}</dd></>}
      </dl>
    </div>
  );
}

function ExternalOverview({ sys, onViewCves, onNavigate, onOpenCommit, onOpenSystem, onViewHistory }) {
  const a = sys.assurance, m = mgmtOf(sys);
  return (
    <div className="mg-overview">
      {a.configState === "missing" && <MgmtNotice icon="warn" tone="danger" title={`nixosConfigurations.${a.configName} is missing at ${a.rev.sha}`} actions={<button className="btn btn-ghost xs focus-ring" onClick={() => onNavigate?.("flakes")}>Open flake</button>}>The configuration was removed or renamed in the flake. Evidence from earlier revisions is kept.</MgmtNotice>}
      <div className="card sd-card">
        <div className="sd-card-head"><h2>Configuration evidence</h2><span className="sd-card-meta">{sys.flake}#{a.configName}</span></div>
        <EvidencePipeline sys={sys} onOpenCommit={onOpenCommit} onNavigate={onNavigate} onViewCves={onViewCves} />
        <div className="mg-copy" style={{ margin: 0 }}>{a.rev.msg}</div>
      </div>
      <div className="mg-cols">
        <ControlCard sys={sys} />
        {m === "external" && <RuntimeCard sys={sys} onOpenSystem={onOpenSystem} />}
        {m === "reference" && (
          <div className="card sd-card">
            <div className="sd-card-head"><h2>Recent evidence</h2><button className="link-action" onClick={onViewHistory}>All history</button></div>
            <div className="timeline">{a.history.slice(0, 4).map((h, i) => <HistoryRow key={i} h={h} />)}</div>
          </div>
        )}
      </div>
    </div>
  );
}

function HistoryRow({ h }) {
  const c = MG_STATE[h.state] || "#6b7280";
  return (
    <div className="tl-item"><span className="tl-dot" style={{ "--status-color": c }} />
      <div className="tl-body"><div className="tl-title"><span style={{ textTransform: "capitalize" }}>{h.kind}</span> <span className="mono" style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>{h.rev}</span> · {h.state}{h.note && <span style={{ color: "var(--cf-text-muted)" }}> · {h.note}</span>}</div><div className="tl-meta">{h.at}</div></div>
    </div>
  );
}
function EvidenceHistory({ sys }) {
  const a = sys.assurance, [kind, setKind] = React.useState("all");
  const rows = a.history.filter((h) => kind === "all" || h.kind === kind);
  return (
    <div className="card sd-card">
      <div className="sd-card-head"><h2>Evidence history</h2>
        <div className="seg">{["all", "eval", "build", "scan"].map((k) => <button key={k} className={kind === k ? "active" : ""} onClick={() => setKind(k)}>{k}</button>)}</div></div>
      <p className="mg-copy" style={{ margin: 0 }}>Evaluations, builds and scans of {a.configName}. Deployments are not recorded because Crystal Forge does not deploy this {mgmtOf(sys) === "external" ? "system" : "configuration"}.</p>
      {rows.length ? <div className="timeline">{rows.map((h, i) => <HistoryRow key={i} h={h} />)}</div> : <div className="mg-dim">No {kind} records.</div>}
    </div>
  );
}

// Compact side-panel body for non-managed resources
function ExternalPanelBody({ sys, onOpenDetail }) {
  const a = sys.assurance;
  return (
    <>
      <section className="panel-section"><div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}><EnvBadge env={sys.environment} /><MgmtChip sys={sys} /><DeploymentCell sys={sys} />{(sys.tags || []).map((t) => <span key={t} className="chip chip-unknown">#{t}</span>)}</div></section>
      <section className="panel-section"><h3>Configuration</h3>
        <dl className="kv-grid"><dt>Flake</dt><dd>{sys.flake}#{a.configName}</dd><dt>Branch</dt><dd>{sys.branch}</dd><dt>Revision</dt><dd>{a.rev.sha}</dd><dt>Message</dt><dd style={{ whiteSpace: "normal", fontFamily: "var(--font-sans)" }}>{a.rev.msg}</dd>{sys.parentHost && <><dt>Parent host</dt><dd>{sys.parentHost}</dd></>}</dl></section>
      <section className="panel-section"><h3>Evidence</h3>
        <dl className="kv-grid"><dt>Evaluation</dt><dd>{a.eval.state} · {a.eval.at}</dd><dt>Build</dt><dd>{a.build.state}{a.build.at ? ` · ${a.build.at}` : ""}</dd><dt>Scan</dt><dd>{a.scan.state}{a.scan.at ? ` · ${a.scan.at}` : ""}</dd></dl></section>
      <section className="panel-section"><h3>Runtime</h3>
        {mgmtOf(sys) === "reference" ? <div className="mg-dim">Not a running machine.</div> : <RuntimeLine sys={sys} />}</section>
      <section className="panel-section"><h3>CVE exposure</h3>{hasCveData(sys) ? <><CveBar cves={sys.cves} /><div style={{ marginTop: 8 }}><EvidenceCves sys={sys} /></div></> : <div className="mg-dim">No completed scan.</div>}</section>
    </>
  );
}

function ModePicker({ value, onChange, current }) {
  const caps = { managed: "Deploys · Agent · Scans", external: "Scans · No deploys · No agent", reference: "Scans · No machine" };
  return (
    <div className="mg-picker" role="radiogroup" aria-label="Management mode">
      {Object.keys(MGMT_META).map((k) => {
        const m = MGMT_META[k];
        return (
          <button key={k} type="button" role="radio" aria-checked={value === k} className={`mg-pick focus-ring mg-${k}${value === k ? " active" : ""}`} onClick={() => onChange(k)}>
            <span className="mg-pick-head"><Icon name={m.icon} size={14} /><b>{m.long}</b>{current === k && <span className="chip chip-unknown" style={{ marginLeft: "auto" }}>current</span>}</span>
            <span className="mg-pick-desc">{m.desc}</span>
            <span className="mg-pick-caps">{caps[k]}</span>
          </button>
        );
      })}
    </div>
  );
}

function FlakeConfigFields({ form, set, mode }) {
  const flakeList = (typeof FLAKES !== "undefined" && FLAKES.length) ? FLAKES : ["infrastructure"];
  const list = declaredConfigs(form.flake);
  const cfg = mode === "managed" ? form.hostname : form.config;
  const found = !cfg || list.includes(cfg);
  return (
    <>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
        <div className="field"><label>Flake</label>
          <select className="input focus-ring" value={form.flake} onChange={(e) => set("flake", e.target.value)}>{flakeList.map((fl) => <option key={fl}>{fl}</option>)}</select></div>
        <div className="field" style={{ marginTop: 0 }}><label>Branch</label><input className="input focus-ring mono" value={form.branch} onChange={(e) => set("branch", e.target.value)} /></div>
      </div>
      {mode !== "managed" && (
        <div className="field"><label>NixOS configuration <span style={{ color: "#f87171" }}>*</span></label>
          <input className="input focus-ring mono" list="mg-cfg-list" value={form.config || ""} onChange={(e) => set("config", e.target.value)} placeholder="e.g. vm-photos" style={!found ? { borderColor: "rgba(248,113,113,0.5)" } : null} />
          <datalist id="mg-cfg-list">{list.map((c) => <option key={c} value={c} />)}</datalist>
          {found ? <div className="help">Crystal Forge evaluates <span className="mono">nixosConfigurations.{form.config || "<name>"}</span> from this flake on every new revision.</div>
            : <div className="help" style={{ color: "#fca5a5" }}><span className="mono">nixosConfigurations.{form.config}</span> was not found on {form.flake}@{form.branch}. Pick a declared configuration: {list.slice(0, 4).join(", ")}.</div>}
        </div>
      )}
      {mode === "managed" && <div className="help">Crystal Forge looks for <span className="mono">nixosConfigurations.{form.hostname || "<hostname>"}</span> in this flake.</div>}
    </>
  );
}

function ParentHostField({ value, onChange }) {
  const hosts = SYSTEMS.filter((s) => mgmtOf(s) === "managed" && s.health !== "offline");
  return (
    <div className="field"><label>Parent host <span style={{ color: "var(--cf-text-muted)", fontWeight: 400 }}>· optional</span></label>
      <select className="input focus-ring mono" value={value || ""} onChange={(e) => onChange(e.target.value)}>
        <option value="">None</option>{hosts.map((h) => <option key={h.id} value={h.hostname}>{h.hostname} ({h.environment})</option>)}
      </select>
      <div className="help">For MicroVMs. A CF-managed host can report whether this guest's unit is running. Crystal Forge never starts, stops or deploys the guest.</div>
    </div>
  );
}

Object.assign(window, { MgmtChip, ScanEvidenceChip, EvidenceCves, RuntimeLine, DeploymentCell, MgmtNotice, EvidenceBanner, EvidenceGate, NotApplicable, EvidencePipeline, RuntimeCard, ExternalOverview, EvidenceHistory, ExternalPanelBody, ModePicker, FlakeConfigFields, ParentHostField, declaredConfigs, hasCveData });
