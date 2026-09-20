// Retention UI — archive toggle + rules popover shared by Builds / Evals,
// and the Admin › Retention tab. Archiving only hides rows; every record
// stays intact and every deep link into one still resolves.

function useRetentionCfg() {
  return React.useSyncExternalStore(cfRetention.subscribe, cfRetention.get);
}

// Returns the archived-id map for `list` plus the include/exclude control.
function useArchive(kind, list) {
  useRetentionCfg();
  const [include, setInclude] = React.useState(false);
  const archived = React.useMemo(() => cfArchived(kind, list), [kind, list, cfRetention.get()]);
  const visible = React.useCallback((rows) => include ? rows : rows.filter(r => !archived.has(r.id)), [include, archived]);
  return {
    archived, include, setInclude,
    isArchived: (r) => archived.has(r.id),
    reason: (r) => archived.get(r.id),
    visible,
    hiddenIn: (rows) => rows.reduce((n, r) => n + (archived.has(r.id) ? 1 : 0), 0),
  };
}

function ArchivedChip({ reason }) {
  return <span className="chip chip-unknown archived-chip" title={reason ? `Archived · ${reason}` : "Archived"}><Icon name="archive" size={10}/> archived</span>;
}

// Filter-bar control: include-archived toggle with the hidden count, plus a
// popover for this record type's rules.
function ArchiveControls({ kind, arc, hidden, label }) {
  const [open, setOpen] = React.useState(false);
  const [pos, setPos] = React.useState(null);
  const btnRef = React.useRef(null);
  const cfg = useRetentionCfg()[kind];
  // The popover is positioned fixed off the button's rect: its anchors live in
  // scrolling toolbars and overflow-hidden cards, which would clip it.
  const place = () => {
    const r = btnRef.current?.getBoundingClientRect();
    if (!r) return;
    setPos({ top: Math.min(r.bottom + 6, window.innerHeight - 300), left: Math.max(8, Math.min(r.right - 296, window.innerWidth - 306)) });
  };
  React.useEffect(() => {
    if (!open) return;
    place();
    const on = () => place();
    window.addEventListener("resize", on);
    window.addEventListener("scroll", on, true);
    return () => { window.removeEventListener("resize", on); window.removeEventListener("scroll", on, true); };
  }, [open]);
  return (
    <div className="arc-controls">
      <button
        className={`btn btn-ghost xs focus-ring${arc.include ? " active-filter" : ""}`}
        onClick={()=>arc.setInclude(v=>!v)}
        title={arc.include ? "Hide archived records again" : `Show ${hidden} archived ${label} in this list`}
      >
        <Icon name="archive" size={12}/> Archived
        {hidden > 0 && <span className="arc-count">{hidden.toLocaleString()}</span>}
      </button>
      <button ref={btnRef} className="btn-icon xs focus-ring" title="Retention rules" onClick={()=>setOpen(o=>!o)}><Icon name="gear" size={13}/></button>
      {open && (
        <>
          <div className="arc-scrim" onClick={()=>setOpen(false)}/>
          <div className="arc-pop" style={pos || { visibility:"hidden" }}>
            <div className="arc-pop-head">
              <span>Retention · {label}</span>
              <button className="btn-icon xs focus-ring" onClick={()=>setOpen(false)}><Icon name="x" size={12}/></button>
            </div>
            <RetentionRules kind={kind} cfg={cfg} compact/>
            <div className="arc-pop-foot">
              <span>Applies everywhere, not just this view.</span>
              <button className="btn btn-ghost xs focus-ring" onClick={()=>cfRetention.reset(kind)}>Reset</button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

// The three rules. Any rule that matches archives the record.
function RetentionRules({ kind, cfg, compact }) {
  const set = (patch) => cfRetention.setRules(kind, patch);
  const rule = (key, on, body, desc) => (
    <div className={`arc-rule${on ? "" : " off"}`}>
      <label className="arc-rule-top">
        <input type="checkbox" checked={on} onChange={e=>set({ [key]: { ...cfg[key], on: e.target.checked } })} disabled={!cfg.enabled}/>
        <span className="arc-rule-body">{body}</span>
      </label>
      {!compact && <div className="arc-rule-desc">{desc}</div>}
    </div>
  );
  const num = (key, field, opts) => (
    <select className="input focus-ring arc-num" value={cfg[key][field]} disabled={!cfg.enabled || !cfg[key].on}
      onChange={e=>set({ [key]: { ...cfg[key], [field]: Number(e.target.value) } })}>
      {opts.map(n => <option key={n} value={n}>{n.toLocaleString()}</option>)}
    </select>
  );
  return (
    <div className="arc-rules">
      <label className="arc-enable">
        <input type="checkbox" checked={cfg.enabled} onChange={e=>set({ enabled: e.target.checked })}/>
        <span>Archive automatically</span>
        <span className="arc-enable-note">nightly sweep</span>
      </label>
      {rule("age", cfg.age.on, <>Older than {num("age","days",[7,14,30,60,90,180,365])} days</>,
        "The main lever. Anything past the window drops out of the default list.")}
      {rule("perFlake", cfg.perFlake.on, <>Keep newest {num("perFlake","n",[5,10,25,50,100,250])} per flake</>,
        "Guarantees recent history for quiet flakes even when the age window has passed.")}
      {rule("cap", cfg.cap.on, <>Hard cap at {num("cap","n",[1000,5000,10000,25000,100000])} records</>,
        "Backstop for a runaway builder. Oldest past the cap archive regardless of age.")}
    </div>
  );
}

/* ── Admin › Retention ─────────────────────────────────────────────── */
function AdminRetention() {
  const state = useRetentionCfg();
  const lists = React.useMemo(cfRetentionLists, []);
  return (
    <div style={{ padding:16, display:"flex", flexDirection:"column", gap:14 }}>
      <div className="arc-intro">
        <Icon name="archive" size={15}/>
        <div>
          <strong>Archiving hides records, it never deletes them.</strong>
          <div>An archived build or evaluation keeps its logs, policy results and signatures. It stays searchable, and anything linking to it — a compliance control, a POA&amp;M item, an attestation — still opens its drawer. Last sweep {state.lastSweep}.</div>
        </div>
      </div>
      {RETENTION_KINDS.map(kind => (
        <RetentionCard key={kind.k} kind={kind} cfg={state[kind.k]} list={lists[kind.k]} manual={state.manual[kind.k]}/>
      ))}
    </div>
  );
}

function RetentionCard({ kind, cfg, list, manual }) {
  const preview = React.useMemo(() => cfRetentionPreview(kind.k, list, cfg), [kind.k, list, cfg]);
  const kept = list.length - preview.total;
  const pct = list.length ? Math.round(preview.total / list.length * 100) : 0;
  return (
    <div className="card arc-card">
      <div className="arc-card-head">
        <div>
          <h3>{kind.label}</h3>
          <p>{kind.desc}</p>
        </div>
        <div className="arc-card-nums">
          <div><span className="arc-big">{kept.toLocaleString()}</span><span>shown</span></div>
          <div><span className="arc-big arc-dim">{preview.total.toLocaleString()}</span><span>archived</span></div>
        </div>
      </div>
      <div className="arc-bar"><span style={{ width:`${100-pct}%` }}/></div>
      <div className="arc-card-body">
        <RetentionRules kind={kind.k} cfg={cfg}/>
        <div className="arc-side">
          <div className="arc-side-head">What each rule catches</div>
          {[["age","Age window",preview.age],["perFlake","Keep newest per flake",preview.perFlake],["cap","Record cap",preview.cap]].map(([k,l,n]) => (
            <div key={k} className="arc-side-row">
              <span>{l}</span>
              <span className={cfg[k].on ? "" : "arc-dim"}>{cfg[k].on ? `${n.toLocaleString()} ${kind.unit}s` : "off"}</span>
            </div>
          ))}
          <div className="arc-side-row arc-side-total">
            <span>Union of all rules</span><span>{preview.total.toLocaleString()}</span>
          </div>
          {(manual.archived.length > 0 || manual.restored.length > 0) && (
            <div className="arc-manual">
              <span>
                {manual.archived.length > 0 && `${manual.archived.length} archived by hand`}
                {manual.archived.length > 0 && manual.restored.length > 0 && " · "}
                {manual.restored.length > 0 && `${manual.restored.length} kept by hand`}
              </span>
              <button className="btn btn-ghost xs focus-ring" onClick={()=>cfRetention.clearManual(kind.k)}>Clear overrides</button>
            </div>
          )}
        </div>
      </div>
      <div className="arc-card-foot">
        <span>Manual overrides always win over the sweep, in both directions.</span>
        <button className="btn btn-ghost focus-ring" onClick={()=>cfRetention.reset(kind.k)}>Reset to defaults</button>
        <button className="btn btn-primary focus-ring"><Icon name="sync" size={13}/> Run sweep now</button>
      </div>
    </div>
  );
}

Object.assign(window, { useRetentionCfg, useArchive, ArchivedChip, ArchiveControls, RetentionRules, AdminRetention, RetentionCard });
