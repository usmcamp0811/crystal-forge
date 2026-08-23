// Setup Coach — guided first-run onboarding (6 steps), floating panel + page callout + action bubble

const COACH_STEPS = [
  { key:"env", n:1, title:"Create environment", view:"environments", icon:"env", target:"env",
    short:"Define a deployment boundary",
    blurb:"Group systems by deployment domain like production, staging, or dev. Policies and caches attach to environments.",
    action:"Add environment" },
  { key:"flake", n:2, title:"Add flake", view:"flakes", icon:"git", target:"flake",
    short:"Point at your config repo",
    blurb:"Register the Git repo holding your NixOS configs. Crystal Forge evaluates every commit and tracks what's deployed where.",
    action:"Add flake" },
  { key:"builder", n:3, title:"Register builder", view:"builders", icon:"cpu", target:"builder",
    short:"Connect a build worker",
    blurb:"Add a worker that evaluates flakes, builds derivations, and scans for CVEs. Paste its public key so the server recognizes it.",
    action:"Register builder" },
  { key:"cache", n:4, title:"Configure cache", view:"caches", icon:"cube", target:"cache",
    short:"Add a binary cache",
    blurb:"Give systems somewhere to pull prebuilt packages from instead of rebuilding. Attic is recommended for production.",
    action:"Add cache" },
  { key:"system", n:5, title:"Register system", view:"systems", icon:"server", target:"system",
    short:"Add a host to manage",
    blurb:"Register a NixOS host and connect it to an environment and flake. Each system is identified by its own key.",
    action:"Add system" },
  { key:"agent", n:6, title:"Deploy agent", view:"systems", icon:"deploy", target:null, dependent:true,
    short:"Connect the host",
    blurb:"Install the Crystal Forge agent on the host. It reports in over a signed heartbeat — this step completes on its own once the agent checks in.",
    action:null },
  { key:"policy", n:7, title:"Create policy", view:"policies", icon:"file", target:"policy",
    short:"Define a compliance rule",
    blurb:"Write a policy — a NixOS option value or enforcement rule Crystal Forge checks systems against. Import STIG controls or write your own.",
    action:"New custom policy" },
  { key:"compliance", n:8, title:"Build compliance bundle", view:"compliance", icon:"shield", target:"bundle",
    short:"Group policies into an audit bundle",
    blurb:"Bundle policies into a compliance framework like a STIG or NIST baseline, then see pass/fail evidence per system.",
    action:"New bundle" },
  { key:"poam", n:9, title:"Track a POA&M", view:"compliance", icon:"activity", target:null,
    short:"Plan remediation for a failing finding",
    blurb:"When a control fails, open its evidence and create a POA&M — a remediation plan with an owner, target date, and milestones. This step completes on its own once one exists.",
    action:null },
];
window.COACH_STEPS = COACH_STEPS;

// ─── State hook (localStorage-backed) ───
function useCoach() {
  const load = () => {
    try {
      const raw = localStorage.getItem("cf.coach.v1");
      if (raw) return JSON.parse(raw);
    } catch {}
    return { done: ["env", "flake"], panel: "expanded", calloutHidden: {} };
  };
  const [state, setState] = React.useState(load);
  React.useEffect(() => {
    try { localStorage.setItem("cf.coach.v1", JSON.stringify(state)); } catch {}
  }, [state]);

  const doneSet = new Set(state.done);
  const isDone = (k) => doneSet.has(k);
  // a step is locked if it depends on a prerequisite that isn't done
  const isLocked = (s) => s.dependent && !isDone("system");
  // current = first incomplete, unlocked step
  const current = COACH_STEPS.find(s => !isDone(s.key) && !isLocked(s)) || null;

  const api = {
    state,
    doneSet,
    isDone,
    isLocked,
    current,
    count: doneSet.size,
    total: COACH_STEPS.length,
    complete: (k) => setState(p => p.done.includes(k) ? p : { ...p, done: [...p.done, k] }),
    uncomplete: (k) => setState(p => ({ ...p, done: p.done.filter(x => x !== k) })),
    setPanel: (panel) => setState(p => ({ ...p, panel })),
    hideCallout: (view) => setState(p => ({ ...p, calloutHidden: { ...p.calloutHidden, [view]: true } })),
    showCallouts: () => setState(p => ({ ...p, calloutHidden: {} })),
    relaunch: () => setState(p => ({ ...p, panel: "expanded", calloutHidden: {} })),
    reset: () => setState({ done: [], panel: "expanded", calloutHidden: {} }),
    fill: () => setState({ done: COACH_STEPS.map(s => s.key), panel: "minimized", calloutHidden: {} }),
  };
  return api;
}
window.useCoach = useCoach;

// ─── Brand mark (small hexagon) ───
function CoachMark({ size = 18 }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 2.5 20.5 7v10L12 21.5 3.5 17V7L12 2.5Z" fill="none" stroke="currentColor" strokeWidth="1.6"/>
      <path d="M12 7.2 16.3 9.6v4.8L12 16.8 7.7 14.4V9.6L12 7.2Z" fill="currentColor" opacity="0.85"/>
    </svg>
  );
}

// ─── Floating panel ───
function SetupCoach({ coach, onNavigate }) {
  const { state, isDone, isLocked, current, count, total } = coach;
  if (state.panel === "dismissed") return null;

  const pct = Math.round((count / total) * 100);
  const allDone = count === total;

  if (state.panel === "minimized") {
    return (
      <button className="coach-pill focus-ring" onClick={() => coach.setPanel("expanded")} title="Open Setup Coach">
        <span className="coach-pill-ring" style={{ "--p": `${pct}%` }}>
          <CoachMark size={15} />
        </span>
        <span className="coach-pill-text">
          <strong>Setup</strong>
          <span>{allDone ? "Complete" : `${count}/${total}`}</span>
        </span>
      </button>
    );
  }

  return (
    <div className="coach" role="complementary" aria-label="Setup Coach">
      <div className="coach-head">
        <div className="coach-head-title">
          <span className="coach-head-mark"><CoachMark size={17} /></span>
          <div>
            <strong>Setup Coach</strong>
            <div className="coach-head-sub">{allDone ? "All steps complete 🎉" : `${count} of ${total} complete`}</div>
          </div>
        </div>
        <div className="coach-head-actions">
          <button className="coach-link focus-ring" onClick={() => coach.setPanel("minimized")} title="Minimize">Minimize</button>
          <button className="coach-link focus-ring" onClick={() => coach.setPanel("dismissed")} title="Dismiss">Dismiss</button>
        </div>
      </div>

      <div className="coach-progress" aria-hidden="true">
        {COACH_STEPS.map(s => (
          <span key={s.key} className={`coach-progress-seg${isDone(s.key) ? " done" : ""}${current && current.key === s.key ? " current" : ""}`} />
        ))}
      </div>

      <div className="coach-steps">
        {COACH_STEPS.map((s, i) => {
          const done = isDone(s.key);
          const locked = isLocked(s);
          const isCurrent = current && current.key === s.key;
          const status = done ? "done" : locked ? "locked" : isCurrent ? "current" : "pending";
          return (
            <button
              key={s.key}
              className={`coach-step coach-step-${status}`}
              disabled={locked}
              onClick={() => { onNavigate(s.view); }}
            >
              <span className="coach-step-rail">
                <span className="coach-step-node">
                  {done ? <Icon name="check" size={13} />
                    : locked ? <Icon name="key" size={11} />
                    : <span className="coach-step-num">{s.n}</span>}
                </span>
                {i < COACH_STEPS.length - 1 && <span className="coach-step-line" />}
              </span>
              <span className="coach-step-body">
                <span className="coach-step-title">
                  <Icon name={s.icon} size={13} />
                  {s.title}
                </span>
                <span className="coach-step-status">
                  {done ? "Configured"
                    : locked ? "Completes after first system reports in"
                    : isCurrent ? s.short
                    : "Pending"}
                </span>
              </span>
              <span className="coach-step-aff">
                {done ? <span className="coach-step-tick">✓</span>
                  : locked ? null
                  : <Icon name="chevron-right" size={15} />}
              </span>
            </button>
          );
        })}
      </div>

      <div className="coach-foot">
        {allDone
          ? <span className="coach-foot-note"><Icon name="check" size={12} /> You're all set — reopen anytime from Server Management.</span>
          : <span className="coach-foot-note">Reopen anytime from <strong>Server Management</strong>.</span>}
      </div>
    </div>
  );
}
window.SetupCoach = SetupCoach;

// ─── Page callout (top of the current step's destination view) ───
function CoachCallout({ coach, topView, onNavigate }) {
  // find the earliest incomplete, unlocked step whose view matches
  const step = COACH_STEPS.find(s => s.view === topView && !coach.isDone(s.key) && !coach.isLocked(s));
  if (!step) return null;
  if (coach.state.panel === "dismissed") return null;
  if (coach.state.calloutHidden && coach.state.calloutHidden[topView]) return null;

  return (
    <div className="coach-callout" role="status" style={coach.state.panel === "expanded" ? { marginRight: "min(360px, 42vw)" } : null}>
      <div className="coach-callout-rail" />
      <div className="coach-callout-icon"><Icon name={step.icon} size={20} /></div>
      <div className="coach-callout-body">
        <div className="coach-callout-eyebrow">Setup Tour · Step {step.n} of {COACH_STEPS.length}</div>
        <div className="coach-callout-title">{step.title}</div>
        <div className="coach-callout-blurb">{step.blurb}</div>
        {step.action && (
          <div className="coach-callout-hint">
            <Icon name="arrow-right" size={12} />
            Use the <strong>{step.action}</strong> button to continue — the coach marks this step done once it's created.
          </div>
        )}
      </div>
      <div className="coach-callout-actions">
        <button className="btn btn-primary focus-ring xs" onClick={() => coach.complete(step.key)}>
          <Icon name="check" size={12} /> Mark done
        </button>
        <button className="coach-link focus-ring" onClick={() => coach.hideCallout(topView)}>Hide</button>
      </div>
    </div>
  );
}
window.CoachCallout = CoachCallout;

// ─── Anchored action bubble — points at the page's primary action button ───
function CoachBubble({ coach, topView }) {
  const step = COACH_STEPS.find(s => s.view === topView && !coach.isDone(s.key) && !coach.isLocked(s));
  const targetKey = step && step.target;
  const [pos, setPos] = React.useState(null);

  React.useEffect(() => {
    if (!targetKey || coach.state.panel !== "minimized") { setPos(null); return; }
    if (coach.state.calloutHidden && coach.state.calloutHidden[topView]) { setPos(null); return; }
    let alive = true;
    const measure = () => {
      const el = document.querySelector(`[data-coach-target="${targetKey}"]`);
      if (!el) { if (alive) setPos(null); return; }
      const r = el.getBoundingClientRect();
      if (alive) setPos({ top: r.bottom + 12, left: Math.min(r.left + r.width / 2, window.innerWidth - 150) });
    };
    measure();
    const iv = setInterval(measure, 600);
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, true);
    return () => { alive = false; clearInterval(iv); window.removeEventListener("resize", measure); window.removeEventListener("scroll", measure, true); };
  }, [targetKey, topView, coach.state.panel, JSON.stringify(coach.state.calloutHidden)]);

  if (!pos || !step) return null;
  return ReactDOM.createPortal(
    <div className="coach-bubble" style={{ top: pos.top, left: pos.left }}>
      <span className="coach-bubble-arrow" />
      <span className="coach-bubble-eyebrow">Next action</span>
      <span className="coach-bubble-text">Click <strong>{step.action}</strong></span>
    </div>,
    document.body
  );
}
window.CoachBubble = CoachBubble;
