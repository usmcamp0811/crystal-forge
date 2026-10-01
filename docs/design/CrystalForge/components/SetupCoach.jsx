// Crystal Forge Coach — two tracks in one surface.
//   SETUP               "Is Crystal Forge configured?"   Nine steps. Admin-only. Completion is
//                       SERVER-DERIVED from persisted resources; the browser never completes a step.
//   SECURITY WORKFLOWS  "Do I know how to operate it?"   Five walkthrough modules, every role.
//                       Progress is PRESENTATION state only ("Walkthrough completed").
// Parts: floating panel · setup page callout · setup action bubble · tour card · spotlight · Guide launcher.

const COACH_STEPS = [
  { key:"env", n:1, title:"Create environment", view:"environments", icon:"env", target:"env",
    short:"Define an operational boundary",
    blurb:"Environments are the operational and security boundary for deployment, authorization, compliance assignments and caches. Access grants, bundle versions and cache scope all attach here.",
    completeWhen:"an environment is saved", action:"Add environment" },
  { key:"flake", n:2, title:"Add flake", view:"flakes", icon:"git", target:"flake",
    short:"Register the configuration source",
    blurb:"Crystal Forge monitors and evaluates this configuration source on the server. Builds and exact configurations are scanned according to the configured scan policy.",
    completeWhen:"a flake is registered", action:"Add flake" },
  { key:"builder", n:3, title:"Register builder", view:"builders", icon:"cpu", target:"builder",
    short:"Connect a build worker",
    blurb:"Connect a build worker that builds server-evaluated derivations and performs the build-side security work Crystal Forge assigns. Authoritative evaluation stays on the server. Paste the worker's public key so the server recognizes it.",
    completeWhen:"a builder is registered with its key", action:"Register builder" },
  { key:"cache", n:4, title:"Configure cache", view:"caches", icon:"cube", target:"cache",
    short:"Add a binary cache",
    blurb:"Builders push exact closures here and systems pull them instead of rebuilding. Attic is recommended for production.",
    completeWhen:"a cache is configured", action:"Add cache" },
  { key:"system", n:5, title:"Register system", view:"systems", icon:"server", target:"system",
    short:"Add a host to manage",
    blurb:"Register a NixOS host with its environment, flake and key. Each system is identified by its own key.",
    completeWhen:"a system record is saved", action:"Add system" },
  { key:"agent", n:6, title:"Deploy agent", view:"systems", icon:"deploy", target:null, dependent:true,
    short:"Connect and acknowledge the host",
    blurb:"Install the agent, wait for its first signed report, then complete the administrator acknowledgement.",
    completeWhen:"an administrator acknowledges the agent after its first signed report", action:null },
  { key:"policy", n:7, title:"Create policy", view:"policies", icon:"file", target:"policy",
    short:"Create or import a policy",
    blurb:"Platform policies govern pipeline mechanics such as deployment and approval rules. Security controls carry framework criteria such as STIG rules. Whether a failure blocks deployment depends on enforcement, not on the policy existing.",
    completeWhen:"you create or import a policy lineage", action:"New custom policy" },
  { key:"compliance", n:8, title:"Build compliance bundle", view:"compliance", icon:"shield", target:"bundle",
    short:"Group controls into a baseline",
    blurb:"A bundle collects security controls into a baseline such as a STIG or NIST profile. Bundle versions are reusable; assignments select or pin a version per environment or system.",
    completeWhen:"a compliance bundle is saved", action:"New bundle" },
  { key:"poam", n:9, title:"Track a POA&M", view:"poams", icon:"activity", target:null,
    short:"Plan remediation for a finding",
    blurb:"A POA&M is a remediation plan with an owner, target date and milestones. It can come from failing compliance evidence, a scheduled CVE patch, or converting an accepted risk.",
    completeWhen:"any POA&M exists, in any lifecycle state", action:null },
];
window.COACH_STEPS = COACH_STEPS;

// ─── State ───
// `server` mocks GET /setup/progress. In production it is read, never written, by the coach;
// here creation flows call coach.serverObserve(k) to stand in for "the server now sees a
// persisted resource". `ui` is browser-local presentation state.
const COACH_SERVER_KEY = "cf.setup.server.v2", COACH_UI_KEY = "cf.coach.ui.v2";
const COACH_UI_DEFAULT = { panel:"expanded", track:"setup", role:"admin", calloutHidden:{}, progress:{}, active:null, forceSheet:false, nonce:0 };

function useCoach() {
  const read = (k, d) => { try { const r = localStorage.getItem(k); if (r) return { ...d, ...JSON.parse(r) }; } catch {} return d; };
  const [server, setServer] = React.useState(() => read(COACH_SERVER_KEY, { observed:{ env:true, flake:true }, agent:"none" }));
  const [ui, setUi] = React.useState(() => ({ ...read(COACH_UI_KEY, COACH_UI_DEFAULT), active: null }));
  React.useEffect(() => { try { localStorage.setItem(COACH_SERVER_KEY, JSON.stringify(server)); } catch {} }, [server]);
  React.useEffect(() => { try { localStorage.setItem(COACH_UI_KEY, JSON.stringify({ ...ui, active: null })); } catch {} }, [ui]);

  const isDone = (k) => k === "agent" ? server.agent === "acknowledged" : !!server.observed[k];
  const isLocked = (s) => s.dependent && !isDone("system");
  const current = COACH_STEPS.find(s => !isDone(s.key) && !isLocked(s)) || null;
  const count = COACH_STEPS.filter(s => isDone(s.key)).length;
  const isAdmin = ui.role === "admin";
  const patch = (p) => setUi(u => ({ ...u, ...(typeof p === "function" ? p(u) : p) }));

  const tourStatus = (key) => {
    const m = SECURITY_MODULES.find(x => x.key === key);
    const seen = (ui.progress[key] || []).length;
    return { seen, total: m.stops.length, state: seen === 0 ? "none" : seen >= m.stops.length ? "done" : "progress" };
  };
  const markViewed = (key, id) => patch(u => ({ progress: { ...u.progress, [key]: [...new Set([...(u.progress[key] || []), id])] } }));

  return {
    server, ui, isDone, isLocked, current, count, total: COACH_STEPS.length, allDone: count === COACH_STEPS.length, isAdmin,
    role: ui.role, setRole: (role) => patch(u => ({ role, track: role === "admin" ? u.track : "security" })),
    setPanel: (panel) => patch({ panel }),
    setTrack: (track) => patch({ track }),
    open: (track) => patch(u => ({ panel:"expanded", track: track || (u.role !== "admin" ? "security" : u.track) })),
    relaunch: () => patch({ panel:"expanded", track:"setup", calloutHidden:{} }),
    hideCallout: (view) => patch(u => ({ calloutHidden: { ...u.calloutHidden, [view]: true } })),
    // mock server observation of persisted resources (stand-in for the progress endpoint)
    serverObserve: (k) => setServer(s => ({ ...s, observed: { ...s.observed, [k]: true }, agent: k === "system" && s.agent === "none" ? "awaiting_report" : s.agent })),
    simulateAgentReport: () => setServer(s => ({ ...s, agent: s.observed.system ? "report_received" : s.agent })),
    acknowledgeAgent: () => setServer(s => s.agent === "report_received" ? { ...s, agent:"acknowledged" } : s),
    // walkthroughs (presentation only)
    tourStatus, markViewed,
    active: ui.active,
    startTour: (key, idx = 0) => patch(u => ({ active:{ key, idx }, panel: u.panel === "dismissed" ? "expanded" : u.panel, track:"security", nonce: u.nonce + 1 })),
    goStop: (idx) => patch(u => ({ active: u.active ? { ...u.active, idx } : null, nonce: u.nonce + 1 })),
    rerun: () => patch(u => ({ nonce: u.nonce + 1 })),
    exitTour: () => patch({ active:null }),
    restartWalkthroughs: () => patch({ progress:{}, active:null }),
    applyDemo: (id) => {
      const d = COACH_DEMO_STATES.find(x => x.id === id); if (!d) return;
      setServer(d.server || { observed: SETUP_ALL, agent:"acknowledged" });
      setUi(u => ({ ...COACH_UI_DEFAULT, role: d.role || "admin",
        track: d.tour ? "security" : (d.ui?.track || "security"), panel: d.ui?.panel || "expanded",
        forceSheet: !!d.ui?.forceSheet, progress: d.progress || {},
        active: d.tour ? { key: d.tour[0], idx: d.tour[1] } : null, nonce: u.nonce + 1 }));
      if (d.theme) window.__cfSetTheme?.(d.theme);
      if (!d.tour && d.view) window.cfCoachGo?.({ view: d.view });
    },
  };
}
window.useCoach = useCoach;

function CoachMark({ size = 18 }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 2.5 20.5 7v10L12 21.5 3.5 17V7L12 2.5Z" fill="none" stroke="currentColor" strokeWidth="1.6"/>
      <path d="M12 7.2 16.3 9.6v4.8L12 16.8 7.7 14.4V9.6L12 7.2Z" fill="currentColor" opacity="0.85"/>
    </svg>
  );
}
window.CoachMark = CoachMark;

// Drawers and modals are large; while one is open the coach docks bottom-left, compact.
function useCoachOverlay() {
  const [open, setOpen] = React.useState(false);
  React.useEffect(() => {
    const check = () => setOpen(!!document.querySelector(".modal-backdrop, .fl-tray, .side-panel"));
    check(); const iv = setInterval(check, 350);
    return () => clearInterval(iv);
  }, []);
  return open;
}
function useCoachNarrow() {
  const [n, setN] = React.useState(() => window.innerWidth <= 720);
  React.useEffect(() => { const h = () => setN(window.innerWidth <= 720); window.addEventListener("resize", h); return () => window.removeEventListener("resize", h); }, []);
  return n;
}
function coachReveal(el) {
  let p = el.parentElement;
  while (p && p !== document.body) {
    const cs = getComputedStyle(p);
    if (/(auto|scroll)/.test(cs.overflowY) && p.scrollHeight > p.clientHeight) {
      const r = el.getBoundingClientRect(), pr = p.getBoundingClientRect();
      if (r.top < pr.top + 60 || r.bottom > pr.bottom - 40) p.scrollTop += r.top - pr.top - 110;
      return;
    }
    p = p.parentElement;
  }
}

// Tracks the active stop's target element: runs the stop's navigation once per entry,
// then polls for the element so the spotlight follows layout changes.
function useCoachTour(coach) {
  const a = coach.active;
  const mod = a ? SECURITY_MODULES.find(m => m.key === a.key) : null;
  const stop = mod ? mod.stops[a.idx] : null;
  const [rect, setRect] = React.useState(null);
  const [status, setStatus] = React.useState("idle"); // running | found | missing
  React.useEffect(() => {
    if (!stop) { setRect(null); setStatus("idle"); return; }
    let alive = true, revealed = false, t0 = Date.now();
    setStatus("running"); setRect(null);
    const gated = coachGated(stop, coach.role);
    const target = gated && stop.gatedTarget ? stop.gatedTarget : stop.target;
    let iv;
    coachRunStop(stop, coach.role).then(() => {
      if (!alive) return;
      const tick = () => {
        const el = coachFind(target);
        if (!alive) return;
        if (el) { if (!revealed) { coachReveal(el); revealed = true; } const r = el.getBoundingClientRect(); setRect({ top:r.top, left:r.left, width:r.width, height:r.height }); setStatus("found"); }
        else { setRect(null); setStatus(Date.now() - t0 > 2600 ? "missing" : "running"); }
      };
      tick(); iv = setInterval(tick, 400);
    });
    return () => { alive = false; clearInterval(iv); };
  }, [a?.key, a?.idx, coach.ui.nonce, coach.role]);
  return { mod, stop, rect, status };
}

function CoachSpotlight({ rect }) {
  if (!rect) return null;
  const pad = 6;
  return ReactDOM.createPortal(
    <div className="coach-spot" style={{ top: rect.top - pad, left: rect.left - pad, width: rect.width + pad * 2, height: rect.height + pad * 2 }} aria-hidden="true"/>,
    document.body
  );
}

// ─── Guide launcher (top bar, every authenticated role) ───
function CoachGuideButton({ coach }) {
  const setupOpen = coach.isAdmin && !coach.allDone;
  return (
    <button className="btn btn-ghost focus-ring coach-guide-btn" onClick={() => coach.ui.panel === "expanded" && !coach.active ? coach.setPanel("minimized") : coach.open()}
      title="Open the Crystal Forge Coach: setup and security walkthroughs" aria-label="Guide">
      <Icon name="help" size={15}/><span className="coach-guide-label">Guide</span>
      {setupOpen && <span className="coach-guide-dot" title={`${coach.count} of ${coach.total} setup steps complete`}/>}
    </button>
  );
}
window.CoachGuideButton = CoachGuideButton;

// ─── Floating coach ───
function SetupCoach({ coach, onNavigate }) {
  const overlay = useCoachOverlay();
  const narrow = useCoachNarrow();
  const tour = useCoachTour(coach);
  const sheet = narrow || coach.ui.forceSheet;
  const { panel } = coach.ui;
  if (panel === "dismissed") return null;

  const spot = <CoachSpotlight rect={tour.rect}/>;

  if (tour.stop) {
    if (panel === "minimized") return <>{spot}<CoachPill coach={coach} tour={tour} docked={overlay || sheet}/></>;
    return <>{spot}<CoachTourCard coach={coach} tour={tour} compact={overlay || sheet} sheet={sheet}/></>;
  }
  if (panel === "minimized" || overlay) return <CoachPill coach={coach} docked={overlay || sheet}/>;
  return <CoachPanel coach={coach} onNavigate={onNavigate} sheet={sheet}/>;
}
window.SetupCoach = SetupCoach;

function CoachPill({ coach, tour, docked }) {
  const setupPct = Math.round((coach.count / coach.total) * 100);
  const pct = tour ? Math.round(((tour.stop ? coach.active.idx + 1 : 0) / tour.mod.stops.length) * 100) : coach.isAdmin && !coach.allDone ? setupPct : 100;
  return (
    <button className={`coach-pill focus-ring${docked ? " docked" : ""}`} onClick={() => coach.setPanel("expanded")} title="Open the Coach">
      <span className="coach-pill-ring" style={{ "--p": `${pct}%` }}><CoachMark size={15}/></span>
      <span className="coach-pill-text">
        {tour
          ? <><strong>Walkthrough {coach.active.idx + 1}/{tour.mod.stops.length}</strong><span>{tour.stop.title}</span></>
          : coach.isAdmin && !coach.allDone
            ? <><strong>Setup</strong><span>{coach.count}/{coach.total} reported complete</span></>
            : <><strong>Guide</strong><span>Security walkthroughs</span></>}
      </span>
    </button>
  );
}

function CoachPanel({ coach, onNavigate, sheet }) {
  const track = coach.isAdmin ? coach.ui.track : "security";
  return (
    <div className={`coach${sheet ? " coach-sheet" : ""}`} role="complementary" aria-label="Crystal Forge Coach">
      <div className="coach-head">
        <div className="coach-head-title">
          <span className="coach-head-mark"><CoachMark size={17}/></span>
          <div style={{ minWidth:0 }}>
            <strong>Crystal Forge Coach</strong>
            <div className="coach-head-sub">{track === "setup" ? (coach.allDone ? "All setup complete" : `${coach.count} of ${coach.total} reported complete`) : "Guided security workflows"}</div>
          </div>
        </div>
        <div className="coach-head-actions">
          <button className="coach-link focus-ring" onClick={() => coach.setPanel("minimized")}>Minimize</button>
          <button className="coach-link focus-ring" onClick={() => coach.setPanel("dismissed")} title="Reopen from Guide in the top bar">Close</button>
        </div>
      </div>
      <div className="coach-tabs" role="tablist">
        {coach.isAdmin && (
          <button role="tab" aria-selected={track === "setup"} className={track === "setup" ? "active" : ""} onClick={() => coach.setTrack("setup")}>
            Setup <span className="mono">{coach.count}/{coach.total}</span>
          </button>
        )}
        <button role="tab" aria-selected={track === "security"} className={track === "security" ? "active" : ""} onClick={() => coach.setTrack("security")}>Security workflows</button>
        <span className="coach-role" title="Walkthroughs adapt to your role">{COACH_ROLE_LABEL[coach.role]}</span>
      </div>
      {track === "setup" ? <CoachSetupTrack coach={coach} onNavigate={onNavigate}/> : <CoachSecurityHome coach={coach}/>}
    </div>
  );
}

function CoachSetupTrack({ coach, onNavigate }) {
  const { isDone, isLocked, current, server } = coach;
  return (
    <>
      {coach.allDone ? (
        <div className="coach-done-card">
          <div className="coach-done-icon"><Icon name="check" size={16}/></div>
          <div style={{ minWidth:0 }}>
            <strong>All setup complete</strong>
            <p>The server reports all nine setup steps complete. The Guide stays in the top bar for everyone.</p>
            <button className="btn btn-primary focus-ring xs" onClick={() => coach.setTrack("security")}><Icon name="shield" size={12}/> Explore security workflows</button>
          </div>
        </div>
      ) : (
        <div className="coach-progress" aria-hidden="true">
          {COACH_STEPS.map(s => <span key={s.key} className={`coach-progress-seg${isDone(s.key) ? " done" : ""}${current && current.key === s.key ? " current" : ""}`}/>)}
        </div>
      )}
      <div className="coach-steps">
        {COACH_STEPS.map((s, i) => {
          const done = isDone(s.key), locked = isLocked(s), isCurrent = current && current.key === s.key;
          const status = done ? "done" : locked ? "locked" : isCurrent ? "current" : "pending";
          const agentLine = s.key === "agent" && !done && !locked
            ? server.agent === "report_received" ? "First signed report received · acknowledge to finish" : "Waiting for the agent's first signed report"
            : null;
          return (
            <button key={s.key} className={`coach-step coach-step-${status}`} disabled={locked} onClick={() => onNavigate(s.view)}>
              <span className="coach-step-rail">
                <span className="coach-step-node">{done ? <Icon name="check" size={13}/> : locked ? <Icon name="key" size={11}/> : <span className="coach-step-num">{s.n}</span>}</span>
                {i < COACH_STEPS.length - 1 && <span className="coach-step-line"/>}
              </span>
              <span className="coach-step-body">
                <span className="coach-step-title"><Icon name={s.icon} size={13}/>{s.title}</span>
                <span className="coach-step-status">{done ? "Reported complete" : locked ? "Register a system first" : agentLine || (isCurrent ? s.short : "Not configured")}</span>
                {s.key === "agent" && !done && server.agent === "report_received" && (
                  <span className="btn btn-primary focus-ring xs" role="button" tabIndex={0} style={{ marginTop:6, width:"fit-content" }}
                    onClick={(e) => { e.stopPropagation(); coach.acknowledgeAgent(); }}
                    onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); e.stopPropagation(); coach.acknowledgeAgent(); } }}>
                    <Icon name="check" size={12}/> Acknowledge agent setup
                  </span>
                )}
              </span>
              <span className="coach-step-aff">{done ? <span className="coach-step-tick">✓</span> : locked ? null : <Icon name="chevron-right" size={15}/>}</span>
            </button>
          );
        })}
      </div>
      <div className="coach-foot">
        <span className="coach-foot-note">Completion is reported by the server from saved resources. Opening a page never completes a step. Reopen from <strong>Guide</strong> or <strong>Server Management</strong>.</span>
      </div>
    </>
  );
}

function CoachSecurityHome({ coach }) {
  const lvl = COACH_ROLE_LEVEL[coach.role];
  return (
    <>
      <div className="coach-mods">
        {SECURITY_MODULES.map(m => {
          const st = coach.tourStatus(m.key);
          const gatedStops = m.stops.filter(s => s.gate && COACH_ROLE_LEVEL[s.gate] > lvl).length;
          return (
            <div key={m.key} className={`coach-mod coach-mod-${st.state}`}>
              <div className="coach-mod-top">
                <span className="coach-mod-key mono">{m.key}</span>
                <strong>{m.title}</strong>
              </div>
              <p>{m.purpose}</p>
              <div className="coach-mod-meta">
                <span>{m.stops.length} stops</span>
                {gatedStops > 0 && <span className="coach-mod-gate" title="These stops explain the action without opening it">{gatedStops} read-only for {COACH_ROLE_LABEL[coach.role]}</span>}
                <span className={`coach-mod-state s-${st.state}`}>
                  {st.state === "done" ? "Walkthrough completed" : st.state === "progress" ? `In progress · ${st.seen} of ${st.total} viewed` : "Not started"}
                </span>
                <span style={{ flex:1 }}/>
                {st.state === "done"
                  ? <button className="btn btn-ghost focus-ring xs" onClick={() => coach.startTour(m.key, 0)}>Restart</button>
                  : st.state === "progress"
                    ? <button className="btn btn-primary focus-ring xs" onClick={() => { const i = m.stops.findIndex(s => !(coach.ui.progress[m.key] || []).includes(s.id)); coach.startTour(m.key, Math.max(0, i)); }}>Resume</button>
                    : <button className="btn btn-primary focus-ring xs" onClick={() => coach.startTour(m.key, 0)}>Start</button>}
              </div>
            </div>
          );
        })}
      </div>
      <div className="coach-foot">
        <span className="coach-foot-note">Walkthrough progress is kept in this browser and records only what you've viewed. It never reflects the state of a finding, scan or POA&M. <button className="coach-inline-link" onClick={coach.restartWalkthroughs}>Restart walkthroughs</button></span>
      </div>
    </>
  );
}

function CoachTourCard({ coach, tour, compact, sheet }) {
  const { mod, stop, rect, status } = tour;
  const idx = coach.active.idx, n = mod.stops.length;
  const gated = coachGated(stop, coach.role);
  const [more, setMore] = React.useState(false);
  React.useEffect(() => setMore(false), [stop.id]);
  const next = () => { coach.markViewed(mod.key, stop.id); if (idx < n - 1) coach.goStop(idx + 1); else coach.exitTour(); };
  const back = () => idx > 0 && coach.goStop(idx - 1);
  const exit = () => { coach.exitTour(); coach.setPanel("expanded"); };

  const cardRef = React.useRef(null);
  // Keep the card off the target: pick the corner with the least overlap (default top-right).
  let place = "tr", dock = "bl", dockMax = null;
  if (compact && !sheet && rect) {
    const H = (cardRef.current && cardRef.current.offsetHeight) || 220, W = 340, vw = window.innerWidth, vh = window.innerHeight;
    const side = (parseInt(getComputedStyle(document.querySelector(".app") || document.body).getPropertyValue("--sidebar-w")) || 240) + 16;
    const boxes = { bl:{ l:side, t:vh - 16 - H, r:side + W, b:vh - 16 }, tl:{ l:side, t:72, r:side + W, b:72 + H }, br:{ l:vw - 16 - W, t:vh - 16 - H, r:vw - 16, b:vh - 16 }, tr:{ l:vw - 16 - W, t:72, r:vw - 16, b:72 + H } };
    const area = (b) => Math.max(0, Math.min(b.r, rect.left + rect.width) - Math.max(b.l, rect.left)) * Math.max(0, Math.min(b.b, rect.top + rect.height) - Math.max(b.t, rect.top));
    dock = ["bl", "tl", "br", "tr"].reduce((best, k) => area(boxes[k]) < area(boxes[best]) ? k : best, "bl");
    if (area(boxes[dock]) > 0) {
      const gapTop = rect.top - 72 - 8, gapBottom = vh - 16 - (rect.top + rect.height) - 8;
      const top = gapTop >= gapBottom;
      dock = (dock[1] === "r" ? (top ? "tr" : "br") : (top ? "tl" : "bl"));
      dockMax = Math.max(0, Math.floor(top ? gapTop : gapBottom));
    }
  }
  if (!compact && rect) {
    const W = Math.min(360, window.innerWidth - 40), H = (cardRef.current && cardRef.current.offsetHeight) || 420, vw = window.innerWidth, vh = window.innerHeight;
    const side = (parseInt(getComputedStyle(document.querySelector(".app") || document.body).getPropertyValue("--sidebar-w")) || 240) + 20;
    const boxes = { tr:{ l:vw - 20 - W, t:72, r:vw - 20, b:72 + H }, br:{ l:vw - 20 - W, t:vh - 20 - H, r:vw - 20, b:vh - 20 }, bl:{ l:side, t:vh - 20 - H, r:side + W, b:vh - 20 } };
    const area = (b) => Math.max(0, Math.min(b.r, rect.left + rect.width) - Math.max(b.l, rect.left)) * Math.max(0, Math.min(b.b, rect.top + rect.height) - Math.max(b.t, rect.top));
    place = ["tr", "br", "bl"].reduce((best, k) => area(boxes[k]) < area(boxes[best]) ? k : best, "tr");
  }
  const tight = compact && dockMax != null && dockMax < 140 && !more;
  const showDetail = !compact || more;
  const notice = gated
    ? <div className="coach-gate"><Icon name="key" size={12}/><span><b>{stop.gate === "admin" ? "Administrator" : "Operator"} permission required.</b> {stop.gatedDoing}</span></div>
    : null;

  return (
    <div ref={cardRef} className={`coach coach-tour${compact ? ` coach-dock dock-${dock}${tight ? " dock-tight" : ""}` : ` at-${place}`}${sheet ? " coach-sheet" : ""}`} style={compact && dockMax != null && !more ? { maxHeight: Math.max(dockMax, 96) } : undefined} role="dialog" aria-label={`Security walkthrough: ${stop.title}`}>
      <div className="coach-tour-head">
        <span className="coach-head-mark sm"><CoachMark size={14}/></span>
        <div style={{ minWidth:0, flex:1 }}>
          <div className="coach-callout-eyebrow" style={{ marginBottom:1 }}>{compact ? "Walkthrough" : "Security walkthrough"} · {idx + 1} of {n}</div>
          <div className="coach-tour-mod">{mod.title}</div>
        </div>
        <button className="coach-link focus-ring" onClick={() => coach.setPanel("minimized")}>Minimize</button>
      </div>
      <div className="coach-tour-dots" aria-hidden="true">
        {mod.stops.map((s, i) => <span key={s.id} className={i === idx ? "cur" : (coach.ui.progress[mod.key] || []).includes(s.id) ? "seen" : ""}/>)}
      </div>
      <div className="coach-tour-body">
        <h3>{stop.title}</h3>
        <p className={compact && !more ? "clamp" : ""}>{stop.why}</p>
        {showDetail && (
          <>
            {stop.figure && <CoachFigure kind={stop.figure}/>}
            {notice || <div className="coach-do"><Icon name="arrow-right" size={12}/><span>{stop.doing}</span></div>}
            {stop.adminNote && <div className="coach-admin-note"><Icon name="key" size={11}/><span>{coach.isAdmin ? stop.adminNote : stop.adminNote.replace("are administrator actions", "require Administrator permission")}</span></div>}
            {stop.important && <div className="coach-important"><b>Important</b><span>{stop.important}</span></div>}
          </>
        )}
        {compact && <button className="coach-inline-link" onClick={() => setMore(m => !m)}>{more ? "Less" : "Details"}</button>}
        {status === "missing" && (
          <div className="coach-missing"><Icon name="info" size={12}/><span>The highlighted control isn't on screen. It may have been closed.</span><button className="coach-inline-link" onClick={coach.rerun}>Show me</button></div>
        )}
      </div>
      <div className="coach-tour-foot">
        <button className="btn btn-ghost focus-ring xs" disabled={idx === 0} onClick={back}><Icon name="chevron-left" size={12}/> Back</button>
        <button className="coach-link focus-ring" onClick={exit}>Exit walkthrough</button>
        <span style={{ flex:1 }}/>
        <button className="btn btn-primary focus-ring xs" onClick={next}>{idx === n - 1 ? "Finish" : "Next"} {idx < n - 1 && <Icon name="chevron-right" size={12}/>}</button>
      </div>
    </div>
  );
}

// ─── Setup page callout ───
function CoachCallout({ coach, topView }) {
  if (!coach.isAdmin || coach.active) return null;
  const step = COACH_STEPS.find(s => s.view === topView && !coach.isDone(s.key) && !coach.isLocked(s));
  if (!step || coach.ui.panel === "dismissed" || coach.ui.calloutHidden[topView]) return null;
  const agent = step.key === "agent" ? coach.server.agent : null;
  return (
    <div className="coach-callout" role="status" style={coach.ui.panel === "expanded" && window.innerWidth > 720 ? { marginRight:"min(360px, 42vw)" } : null}>
      <div className="coach-callout-rail"/>
      <div className="coach-callout-icon"><Icon name={step.icon} size={20}/></div>
      <div className="coach-callout-body">
        <div className="coach-callout-eyebrow">Setup · Step {step.n} of {COACH_STEPS.length}</div>
        <div className="coach-callout-title">{step.title}</div>
        <div className="coach-callout-blurb">{step.blurb}</div>
        {agent === "report_received" ? (
          <div className="coach-callout-hint"><Icon name="check" size={12}/> The agent's first signed report arrived. Acknowledge it to finish this step.</div>
        ) : agent ? (
          <div className="coach-callout-hint"><Icon name="clock" size={12}/> Waiting for the agent's first signed report. Nothing to click yet.</div>
        ) : (
          <div className="coach-callout-hint"><Icon name="arrow-right" size={12}/>
            {step.action ? <>Use <strong>{step.action}</strong>. </> : null}Crystal Forge marks this complete when {step.completeWhen}.</div>
        )}
      </div>
      <div className="coach-callout-actions">
        {agent === "report_received" && <button className="btn btn-primary focus-ring xs" onClick={coach.acknowledgeAgent}><Icon name="check" size={12}/> Acknowledge agent setup</button>}
        <button className="coach-link focus-ring" onClick={() => coach.hideCallout(topView)}>Hide</button>
      </div>
    </div>
  );
}
window.CoachCallout = CoachCallout;

// ─── Setup action bubble (minimized panel only) ───
function CoachBubble({ coach, topView }) {
  const step = coach.isAdmin && !coach.active ? COACH_STEPS.find(s => s.view === topView && !coach.isDone(s.key) && !coach.isLocked(s)) : null;
  const targetKey = step && step.target;
  const [pos, setPos] = React.useState(null);
  React.useEffect(() => {
    if (!targetKey || coach.ui.panel !== "minimized" || coach.ui.calloutHidden[topView]) { setPos(null); return; }
    let alive = true;
    const measure = () => {
      const el = document.querySelector(`[data-coach-target="${targetKey}"]`);
      if (!el) { if (alive) setPos(null); return; }
      const r = el.getBoundingClientRect();
      if (alive) setPos({ top: r.bottom + 12, left: Math.min(r.left + r.width / 2, window.innerWidth - 150) });
    };
    measure();
    const iv = setInterval(measure, 600);
    window.addEventListener("resize", measure); window.addEventListener("scroll", measure, true);
    return () => { alive = false; clearInterval(iv); window.removeEventListener("resize", measure); window.removeEventListener("scroll", measure, true); };
  }, [targetKey, topView, coach.ui.panel, JSON.stringify(coach.ui.calloutHidden)]);
  if (!pos || !step) return null;
  return ReactDOM.createPortal(
    <div className="coach-bubble" style={{ top: pos.top, left: pos.left }}>
      <span className="coach-bubble-arrow"/>
      <span className="coach-bubble-eyebrow">Setup · next action</span>
      <span className="coach-bubble-text">Click <strong>{step.action}</strong></span>
    </div>,
    document.body
  );
}
window.CoachBubble = CoachBubble;
