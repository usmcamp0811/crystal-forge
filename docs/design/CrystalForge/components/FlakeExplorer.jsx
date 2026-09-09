/* Flake explorer panes — the flake's outputs AT ONE REVISION.
   Rendered inside FlakeTray as sibling tabs to the commit history. Every pane
   is scoped to the selected commit, because outputs are a property of a commit:
   a rewrite can drop hosts, delete modules, or replace the lock wholesale. */

/* Revision bar — states which commit these outputs belong to and what changed
   from the previous one. Present on every output pane so the scope is never
   ambiguous. */
function FxRevBar({ commit, out, onPickCommit }) {
  const d = out.delta;
  return (
    <div className="fx-revbar">
      <div className="fx-revbar-main">
        <span className="fx-revbar-label">Outputs at</span>
        <button className="fx-revbar-sha focus-ring" onClick={onPickCommit} title="Change commit">
          <span className="mono">{out.sha}</span>
          <Icon name="chevron-right" size={11}/>
        </button>
        {commit && <span className="fx-revbar-msg" title={commit.msg}>{commit.msg}</span>}
      </div>
      {d && d.any > 0 && (
        <div className="fx-delta">
          <span className="fx-delta-label">vs <span className="mono">{d.prevSha}</span></span>
          {d.hostsAdded.length > 0 && <span className="fx-delta-chip add" title={d.hostsAdded.join(", ")}>+{d.hostsAdded.length} host{d.hostsAdded.length===1?"":"s"}</span>}
          {d.hostsRemoved.length > 0 && <span className="fx-delta-chip del" title={d.hostsRemoved.join(", ")}>−{d.hostsRemoved.length} host{d.hostsRemoved.length===1?"":"s"}</span>}
          {d.modulesAdded.length > 0 && <span className="fx-delta-chip add" title={d.modulesAdded.join(", ")}>+{d.modulesAdded.length} module{d.modulesAdded.length===1?"":"s"}</span>}
          {d.modulesRemoved.length > 0 && <span className="fx-delta-chip del" title={d.modulesRemoved.join(", ")}>−{d.modulesRemoved.length} module{d.modulesRemoved.length===1?"":"s"}</span>}
          {d.inputsBumped.length > 0 && <span className="fx-delta-chip bump" title={d.inputsBumped.map(i=>`${i.name}: ${i.from} → ${i.to}`).join("\n")}>{d.inputsBumped.length} input{d.inputsBumped.length===1?"":"s"} bumped</span>}
          {d.inputsAdded.length > 0 && <span className="fx-delta-chip add" title={d.inputsAdded.join(", ")}>+{d.inputsAdded.length} input{d.inputsAdded.length===1?"":"s"}</span>}
          {d.inputsRemoved.length > 0 && <span className="fx-delta-chip del" title={d.inputsRemoved.join(", ")}>−{d.inputsRemoved.length} input{d.inputsRemoved.length===1?"":"s"}</span>}
        </div>
      )}
      {d && d.any === 0 && <div className="fx-delta"><span className="fx-delta-label">no output changes vs <span className="mono">{d.prevSha}</span></span></div>}
    </div>
  );
}

function FlakeSystemsPane({ flake, out, commit, onPickCommit, onOpenSystem }) {
  const [scope, setScope] = React.useState("all");
  // Registering a declared-but-unmanaged host: the flake already knows its
  // hostname and flake, so the add workflow opens prefilled.
  const [adding, setAdding] = React.useState(null);
  React.useEffect(() => { setScope("all"); }, [out.sha]);
  const rows = out.systems.filter(s => {
    if (scope === "declared-only") return s.declared && !s.managedSystem;
    if (scope === "orphaned") return !s.declared;
    return true;
  });
  const c = out.counts;
  return (
    <div className="fx-pane">
      <FxRevBar commit={commit} out={out} onPickCommit={onPickCommit}/>
      {out.gutted && (
        <div className="sd-callout sd-callout-warn fx-callout">
          <Icon name="warn" size={13}/>
          <div>This commit declares far fewer outputs than its predecessor. Verify it was intentional before deploying.</div>
        </div>
      )}
      <div className="fx-stats">
        <div className="fx-stat"><span className="fx-stat-n">{c.declared}</span><span className="fx-stat-l">declared here</span></div>
        <div className="fx-stat"><span className="fx-stat-n">{c.managed}</span><span className="fx-stat-l">managed by Forge</span></div>
        <div className={`fx-stat${c.declaredOnly ? " warn" : ""}`}><span className="fx-stat-n">{c.declaredOnly}</span><span className="fx-stat-l">declared, unmanaged</span></div>
        <div className={`fx-stat${c.orphaned ? " crit" : ""}`}><span className="fx-stat-n">{c.orphaned}</span><span className="fx-stat-l">managed, undeclared</span></div>
      </div>
      {c.orphaned > 0 && (
        <div className="sd-callout sd-callout-warn fx-callout">
          <Icon name="warn" size={13}/>
          <div>{c.orphaned} managed host{c.orphaned===1?"":"s"} {c.orphaned===1?"is":"are"} not defined at this commit. Deploying it would leave {c.orphaned===1?"that host":"those hosts"} pinned to an older revision.</div>
        </div>
      )}
      <div className="fx-toolbar">
        <div className="seg">
          <button className={scope==="all"?"active":""} onClick={()=>setScope("all")}>All <span className="seg-n">{out.systems.length}</span></button>
          <button className={scope==="declared-only"?"active":""} onClick={()=>setScope("declared-only")}>Unmanaged <span className="seg-n">{c.declaredOnly}</span></button>
          <button className={scope==="orphaned"?"active":""} onClick={()=>setScope("orphaned")}>Undeclared <span className="seg-n">{c.orphaned}</span></button>
        </div>
      </div>
      <table className="sys-table compact fx-table">
        <colgroup><col style={{width:"34%"}}/><col style={{width:"20%"}}/><col style={{width:"24%"}}/><col style={{width:"22%"}}/></colgroup>
        <thead><tr><th>nixosConfiguration</th><th>Environment</th><th>State</th><th></th></tr></thead>
        <tbody>
          {rows.map(s => (
            <tr key={s.hostname}>
              <td><span className="mono fx-host" title={s.hostname}>{s.hostname}</span></td>
              <td>{s.environment ? <span className="chip chip-unknown fx-chip">{s.environment}</span> : <span className="fx-dim">—</span>}</td>
              <td>
                {!s.declared
                  ? <span className="chip chip-critical fx-chip">undeclared</span>
                  : s.managedSystem
                    ? <span className="chip chip-healthy fx-chip">managed</span>
                    : <span className="chip chip-warning fx-chip">not managed</span>}
                {s.note && <span className="fx-note">{s.note}</span>}
              </td>
              <td className="fx-right">
                {s.managedSystem
                  ? <button className="btn btn-ghost focus-ring xs" onClick={()=>onOpenSystem && onOpenSystem(s.managedSystem, out.sha)}>Open config</button>
                  : <button className="btn btn-ghost focus-ring xs" onClick={()=>setAdding(s)}>Add to Forge</button>}
              </td>
            </tr>
          ))}
          {rows.length === 0 && <tr><td colSpan={4}><div className="fx-empty">Nothing in this category.</div></td></tr>}
        </tbody>
      </table>
      {adding && (
        <AddSystemModal
          prefill={{ hostname: adding.hostname, flake: flake.name, branch: flake.branch }}
          onClose={()=>setAdding(null)}/>
      )}
    </div>
  );
}

function FlakeModulesPane({ flake, out, commit, onPickCommit }) {
  const [open, setOpen] = React.useState(null);
  React.useEffect(() => { setOpen(null); }, [out.sha]);
  const max = Math.max(...out.modules.map(m => m.consumers), 1);
  return (
    <div className="fx-pane">
      <FxRevBar commit={commit} out={out} onPickCommit={onPickCommit}/>
      <div className="fx-pane-note">
        Modules exported as <span className="mono">nixosModules</span> at this commit, ordered by how many declared hosts consume them — the blast radius of a change. Expand a module to see the options it declares.
      </div>
      <table className="sys-table compact fx-table">
        <colgroup><col style={{width:"32%"}}/><col style={{width:"34%"}}/><col style={{width:"10%"}}/><col style={{width:"24%"}}/></colgroup>
        <thead><tr><th>Module</th><th>Sets</th><th className="fx-right">Options</th><th>Consumed by</th></tr></thead>
        <tbody>
          {out.modules.map(m => {
            const isOpen = open === m.path;
            return (
              <React.Fragment key={m.path}>
                <tr className="fx-row" onClick={()=>setOpen(isOpen?null:m.path)}>
                  <td>
                    <div className="fx-mod-cell">
                      <Icon name="chevron-right" size={11} className={`cfg-caret${isOpen?" open":""}`}/>
                      <span className="mono fx-host" title={m.path}>{m.name}</span>
                    </div>
                  </td>
                  <td><span className="fx-desc" title={m.desc}>{m.desc}</span></td>
                  <td className="fx-right mono fx-dim">{m.options.length}</td>
                  <td>
                    <div className="fx-bar-row">
                      <div className="fx-bar"><div className="fx-bar-fill" style={{ width:`${(m.consumers/max)*100}%` }}/></div>
                      <span className="mono fx-bar-n">{m.consumers}</span>
                    </div>
                  </td>
                </tr>
                {isOpen && (
                  <tr className="fx-detail-row"><td colSpan={4}>
                    <div className="fx-detail">
                      <div className="fx-detail-head">
                        <span className="fx-detail-label">Declared options</span>
                        <span className="mono fx-detail-file" title={m.path}>{m.path}</span>
                      </div>
                      <table className="fx-opts">
                        <thead><tr><th>Option</th><th>Type</th><th>Default</th></tr></thead>
                        <tbody>
                          {m.options.map(o => (
                            <tr key={o.path}>
                              <td><span className="mono fx-opt-path" title={o.path}>{o.path}</span></td>
                              <td><span className="fx-dim">{o.type}</span></td>
                              <td><span className="mono fx-dim">{o.default}</span></td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                      <div className="fx-detail-note">
                        Declarations come from the evaluation already run for builds, cached per revision — no per-host cost.
                      </div>
                    </div>
                  </td></tr>
                )}
              </React.Fragment>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function FlakeInputsPane({ flake, out, commit, onPickCommit }) {
  const c = out.counts;
  const bumped = {}; (out.delta ? out.delta.inputsBumped : []).forEach(b => { bumped[b.name] = b; });
  return (
    <div className="fx-pane">
      <FxRevBar commit={commit} out={out} onPickCommit={onPickCommit}/>
      <div className="fx-stats">
        <div className="fx-stat"><span className="fx-stat-n">{c.inputs}</span><span className="fx-stat-l">direct inputs</span></div>
        <div className="fx-stat"><span className="fx-stat-n">{c.transitiveTotal}</span><span className="fx-stat-l">resolved total</span></div>
        <div className={`fx-stat${c.channels > 1 ? " warn" : ""}`}><span className="fx-stat-n">{c.channels}</span><span className="fx-stat-l">nixpkgs channels</span></div>
        <div className={`fx-stat${c.staleInputs ? " warn" : ""}`}><span className="fx-stat-n">{c.staleInputs}</span><span className="fx-stat-l">stale &gt; 90d</span></div>
      </div>
      {c.channels > 1 && (
        <div className="sd-callout sd-callout-info fx-callout">
          <Icon name="info" size={13}/>
          <div>This lock resolves {c.channels} nixpkgs revisions ({out.channels.join(", ")}). Hosts do not all build against the same package set.</div>
        </div>
      )}
      <table className="sys-table compact fx-table">
        <colgroup><col style={{width:"27%"}}/><col style={{width:"31%"}}/><col style={{width:"12%"}}/><col style={{width:"14%"}}/><col style={{width:"16%"}}/></colgroup>
        <thead><tr><th>Input</th><th>Source</th><th>Locked</th><th>Updated</th><th>Follows</th></tr></thead>
        <tbody>
          {out.inputs.map(i => (
            <tr key={i.name}>
              <td>
                <div className="fx-input-cell">
                  <span className="mono fx-host" title={i.name}>{i.name}</span>
                  {i.managed && <span className="chip chip-info fx-chip fx-chip-shrink" title="Crystal Forge also tracks this flake">tracked</span>}
                  {i.flakeFamily === "nixpkgs" && <span className="chip chip-unknown fx-chip fx-chip-shrink" title="nixpkgs channel">channel</span>}
                </div>
              </td>
              <td><span className="mono fx-url" title={i.url}>{i.url}</span></td>
              <td>
                <span className="mono fx-dim">{i.rev}</span>
                {bumped[i.name] && <span className="fx-note" title={`was ${bumped[i.name].from}`}>bumped</span>}
              </td>
              <td><span className={i.staleDays > 90 ? "fx-stale" : "fx-dim"}>{i.updated}</span></td>
              <td>{i.follows ? <span className="mono fx-dim">{i.follows}</span> : <span className="fx-dim">—</span>}{i.transitive > 0 && <span className="fx-note">+{i.transitive} transitive</span>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

Object.assign(window, { FxRevBar, FlakeSystemsPane, FlakeModulesPane, FlakeInputsPane });
