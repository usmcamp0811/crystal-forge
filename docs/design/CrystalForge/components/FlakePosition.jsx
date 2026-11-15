// "Where is this system in its flake's history?" — a git-lane showing the commit the
// system's store path was built from, HEAD, and the gap between them.

function flakePositionOf(sys) {
  const mode = typeof mgmtOf === "function" ? mgmtOf(sys) : "managed";
  const flake = (typeof FLAKE_REGISTRY !== "undefined" ? FLAKE_REGISTRY : []).find(f => f.name === sys.flake || f.id === sys.flake);
  const raw = (flake && FLAKE_COMMITS[flake.id]) || [];
  if (!raw.length) return { mode, flake, list: [], unknown: true, reason: "noflake" };
  const drift = typeof fleetDriftData === "function" ? fleetDriftData() : { rows: [] };
  let behind, reason = null;
  if (mode === "managed") {
    const row = drift.rows.find(r => r.sys.id === sys.id);
    behind = row && row.behind != null ? row.behind : 0;
    if (sys.deploymentState === "drift") reason = "untracked";
  } else {
    const a = sys.assurance || {};
    const idx = raw.findIndex(c => c.sha === (a.rev && a.rev.sha));
    behind = idx >= 0 ? idx : Math.min(raw.length - 1, (sys.hostname.length * 7) % 5);
    if (a.configState === "missing") reason = "missing";
  }
  // Fixture commits and system shas come from different generators; pin the system's
  // own sha/message onto its slot so this card agrees with "Currently deployed".
  const list = raw.map((c, i) => i === behind ? { ...c, rawSha: c.sha, sha: sys.commit || c.sha, msg: sys.commitMessage || c.msg } : c);
  const peers = {};
  if (mode === "managed" && flake) drift.rows.forEach(r => {
    if (r.flakeId !== flake.id || r.sys.id === sys.id || r.behind == null || r.sys.deploymentState === "drift") return;
    (peers[r.behind] = peers[r.behind] || []).push(r.sys.hostname);
  });
  return { mode, flake, list, behind, head: list[0], at: list[behind], unknown: !!reason, reason, peers };
}

function fpWindow(len, behind, target) {
  const far = Math.max(behind, target != null ? target : 0);
  const hi = Math.min(len - 1, far + 2);
  const keep = new Set();
  if (far > 9) [0, 1, 2, behind - 2, behind - 1, behind, behind + 1, behind + 2, ...(target != null ? [target - 1, target, target + 1] : [])].forEach(i => i >= 0 && i <= hi && keep.add(i));
  else for (let i = 0; i <= hi; i++) keep.add(i);
  const out = [];
  for (let i = hi; i >= 0; i--) {
    if (keep.has(i)) out.push({ kind: "c", idx: i });
    else if (!out.length || out[out.length - 1].kind !== "gap") out.push({ kind: "gap", idx: i, count: 0 });
    if (!keep.has(i)) out[out.length - 1].count++;
  }
  return out;
}

function FlakePositionCard({ sys, onOpenCommit, onDeployHead, onViewHistory, wide, deploy, pickSha, onPick, attest, approvalSha, approvalText }) {
  const p0 = React.useMemo(() => flakePositionOf(sys), [sys.id]);
  let p = p0;
  const [sel, setSel] = React.useState(null);
  const host = sys.hostname;
  const verb = p.mode === "managed" ? "running" : p.mode === "external" ? "declared" : "tracked";
  const branch = (p.mode === "managed" ? (p.flake && p.flake.branch) || sys.branch : sys.branch || (p.flake && p.flake.branch)) || "main";
  const cls = `card sd-card fp-card${wide ? " sd-card-wide" : ""}`;
  const open = (c) => onOpenCommit?.({ sha: c.sha, msg: c.msg, author: c.author, at: c.at, flake: sys.flake });

  const dep = deploy && deploy.stage && (!p.unknown || p.reason === "untracked") ? deploy : null;
  const orphan = !!dep && p.unknown;
  let tIdx = null;
  if (dep) {
    const s = String(dep.commit || "");
    const hit = (x) => x && s && (x.startsWith(s) || s.startsWith(x));
    p = { ...p0, list: p0.list.slice(), peers: { ...p0.peers } };
    tIdx = p.list.findIndex(c => hit(c.sha) || hit(c.rawSha));
    if (tIdx === p.behind && !hit(sys.commit)) tIdx = -1;
    const realSha = s && s !== "—" && s !== "-";
    if (tIdx < 0 && realSha && p.behind === 0 && dep.kind !== "rollback") {
      // Newer than anything we've synced: show it as a fresh HEAD and step the system back one.
      p.list.unshift({ sha: s, msg: dep.msg || "New commit", author: dep.author || "—", at: "just now" });
      p.behind = 1; p.at = p.list[1]; p.head = p.list[0];
      const sh = {}; Object.keys(p0.peers).forEach(k => sh[+k + 1] = p0.peers[k]); p.peers = sh;
      tIdx = 0;
    }
    if (tIdx < 0) {
      // Target came from a list we can't place exactly; put it on a neighbouring commit and label it with the deployed sha.
      tIdx = dep.kind === "rollback" || p.behind === 0 ? Math.min(p.list.length - 1, p.behind + 1) : 0;
      if (tIdx !== p.behind) p.list[tIdx] = { ...p.list[tIdx], sha: realSha ? s : "uncommitted", msg: realSha ? p.list[tIdx].msg : "Manual rebuild — no commit", noSha: !realSha, rawSha: p.list[tIdx].rawSha || p.list[tIdx].sha };
    }
  }
  const laneRef = React.useRef(null), branchRef = React.useRef(null);
  const [geo, setGeo] = React.useState(null);
  React.useLayoutEffect(() => {
    if (!dep) { setGeo(null); return; }
    const measure = () => {
      const b = branchRef.current, lane = laneRef.current;
      if (!b || !lane) return;
      const br = b.getBoundingClientRect();
      const dot = (i) => { const el = lane.querySelector(`[data-fp-idx="${i}"] .fp-dot`); return el && el.getBoundingClientRect(); };
      const f = dot(orphan ? "orphan" : p.behind), t = dot(tIdx);
      if (!f || !t) return;
      const fx = f.left + f.width / 2 - br.left, tx = t.left + t.width / 2 - br.left;
      const top = Math.max(f.bottom, t.bottom) - br.top + 2;
      const isSame = Math.abs(fx - tx) < 1, H = isSame ? Math.max(40, Math.min(170, fx - 4, br.width - fx - 4)) : 0;
      const cy = f.top + f.height / 2 - br.top;
      const next = isSame ? { l: fx - H, r: br.width - fx - H, top, cy, cx: fx, fr: f.width / 2, same: true, w: br.width, tight: (2 * H - 20) / 5 < 62 } : { l: Math.min(fx, tx), r: br.width - Math.max(fx, tx), top, w: br.width, tight: (Math.abs(fx - tx) - 20) / 5 < 62 };
      setGeo(g => g && g.tight === next.tight && Math.abs(g.l - next.l) < .5 && Math.abs(g.r - next.r) < .5 && Math.abs(g.top - next.top) < .5 && Math.abs(g.w - next.w) < .5 ? g : next);
    };
    measure();
    const ro = new ResizeObserver(measure); laneRef.current && ro.observe(laneRef.current);
    return () => ro.disconnect();
  });
  const landed = dep && dep.stage === "activated";
  const pinAt = landed ? tIdx : orphan ? -1 : p.behind;
  const lo = dep ? Math.min(tIdx, p.behind) : 0, hi = dep ? Math.max(tIdx, p.behind) : -1;
  const travelL = () => false, travelR = () => false;
  let banner = null, stageNodes = null;
  if (dep) {
    const rb = dep.kind === "rollback", sIdx = dep.stages.indexOf(dep.stage), tgtGen = rb ? dep.gen : sys.generation + 1;
    const meta = {
      "queued":    { label: "Queued", sub: `Waiting for ${host} agent to check in (heartbeat every ${sys.heartbeatIntervalSec}s)` },
      "picked-up": { label: "Picked up", sub: `Agent fetched the ${rb ? "rollback" : "deployment"} command` },
      "copying":   { label: "Copying", sub: `Pulling the closure for ${dep.commit} onto ${host} — a failure here stalls the deploy before anything is applied` },
      "applying":  { label: rb ? "Reverting" : "Applying", sub: rb ? `Switching to generation #${dep.gen} (${dep.commit})` : `Building & switching to ${dep.commit}` },
      "activated": { label: "Activated", sub: `Generation #${tgtGen} is live` },
    };
    const dist = Math.abs(tIdx - p.behind);
    banner = (
      <div className="fp-summary fp-dep-summary">
        <div className="fp-dep-text">
          <div className="fp-headline">{landed ? `${rb ? "Rollback" : "Deployment"} complete` : `${rb ? "Rolling back" : "Deploying"} ${host}`} <span className="mono fp-dep-sha">{rb && dep.gen != null ? `#${dep.gen}` : ""}{rb && dep.gen != null && dep.commit && dep.commit !== "—" ? " · " : ""}{dep.commit && dep.commit !== "—" ? dep.commit : rb ? "" : "uncommitted"}</span>{orphan ? <span className="fp-dep-dist">from an untracked config</span> : dist > 0 && <span className="fp-dep-dist">{dist} commit{dist === 1 ? "" : "s"} {tIdx < p.behind ? "forward" : "back"}</span>}</div>
          <div className="fp-sub">{meta[dep.stage]?.sub}</div>
        </div>
        <button className="btn btn-ghost xs focus-ring" onClick={dep.onViewLogs}><Icon name="terminal" size={11}/> Logs</button>
        {landed && <button className="btn-icon focus-ring" onClick={dep.onDismiss} aria-label="Dismiss"><Icon name="x" size={13}/></button>}
      </div>
    );
    stageNodes = dep.stages.map((s, k) => ({ s, k, past: k < sIdx || landed, cur: k === sIdx && !landed, label: meta[s].label, lastStage: k === dep.stages.length - 1 }));
  }
  const chips = (
    <div className="fp-chips">
      {attest && !dep && <span className="fp-attest" title={`First observed ${attest.since}`}><Icon name="warn" size={11}/> {attest.label}<button className="fp-attest-btn focus-ring" onClick={attest.onDecide}>Decide</button></span>}
      {dep ? <span className={`chip ${landed ? "chip-healthy" : "chip-info"}`}>{landed ? <Icon name="check" size={10}/> : <Spinner size={10}/>} {landed ? "deployed" : dep.kind === "rollback" ? "rolling back" : "deploying"}</span>
        : p.unknown ? <span className="chip chip-unknown"><Icon name="warn" size={10}/> not in history</span>
        : p.behind === 0 ? <span className="chip chip-healthy"><Icon name="check" size={10}/> on HEAD</span>
        : <span className="chip chip-warning">{p.behind} behind</span>}
    </div>
  );
  const top = (summary) => <div className="fp-toprow">{summary}{chips}</div>;

  if (p.unknown && !orphan) {
    const lane = p.list.slice(0, 6).reverse();
    const title = p.reason === "untracked" ? `${host} is running a configuration Crystal Forge never evaluated`
      : p.reason === "missing" ? `nixosConfigurations.${sys.assurance?.configName || host} no longer exists in ${sys.flake}`
      : `No history is available for ${sys.flake}`;
    const copy = p.reason === "untracked" ? "The active store path doesn't match the output of any commit on this flake. It was most likely built on the host with nixos-rebuild. The last tracked commit is shown for reference."
      : p.reason === "missing" ? "The configuration was removed or renamed, so there is no current commit to compare against. The last revision that contained it is shown."
      : "The flake hasn't synced, so commits can't be placed yet.";
    return (
      <section className={cls}>
        {top(<div className="fp-summary"><div className="fp-headline">{title}</div><div className="fp-sub">{copy}</div></div>)}
        {lane.length > 0 && (
          <div className="fp-lane-wrap">
            <div className="fp-lane fp-lane-faded">
              {lane.map((c, i) => {
                const idx = lane.length - 1 - i, last = idx === p.behind;
                return (
                  <div key={c.sha} className={`fp-node${last ? " fp-lastknown" : ""}`}>
                    <div className="fp-top fp-top-sha mono">{idx === 0 && <span className="fp-head-tag">HEAD</span>}{c.sha}{last && <span className="fp-lk">last tracked</span>}</div>
                    <div className="fp-rail"><span className={`fp-seg${i === 0 ? " fp-none" : ""}`}></span><button className="fp-dot focus-ring" title={`${c.sha} · ${c.msg}`} onClick={() => open(c)}></button><span className={`fp-seg${idx === 0 ? " fp-none" : ""}`}></span></div>
                    {last && <div className="fp-orphan"><span className="fp-orphan-line"></span><span className="fp-orphan-dot">?</span><span className="fp-pin fp-pin-unknown"><Icon name="server" size={11}/> {host}</span></div>}
                  </div>
                );
              })}
            </div>
          </div>
        )}
        <div className="fp-store-row"><div className="fp-store">
          <span className="fp-store-k">Store path</span>
          <span className="mono fp-store-v" title={sys.storePath}>{sys.storePath || "—"}</span>
          <span className="fp-store-arrow">→</span>
          <span className="fp-store-unknown">no matching evaluation</span>
        </div>
        <div className="fp-actions">
          {p.reason === "untracked" && <button className="btn btn-ghost xs focus-ring" onClick={() => onOpenCommit?.({ capture: true, flake: sys.flake, storePath: sys.storePath })}><Icon name="git" size={11}/> Capture to flake</button>}
          {onViewHistory && <button className="btn btn-ghost xs focus-ring" onClick={onViewHistory}>View history</button>}
        </div></div>
      </section>
    );
  }

  const pickIdx = !dep && pickSha ? p.list.findIndex(c => [c.sha, c.rawSha].some(x => x && (x.startsWith(pickSha) || pickSha.startsWith(x)))) : -1;
  const win = fpWindow(p.list.length, p.behind, dep ? tIdx : pickIdx >= 0 ? pickIdx : null);
  if (orphan) { const k = win.findIndex(n => n.kind === "c" && n.idx === p.behind); win.splice(k + 1, 0, { kind: "orphan", idx: "orphan" }); }
  const tCol = dep ? win.findIndex(n => n.kind === "c" && n.idx === tIdx) : -1;
  const fCol = dep ? win.findIndex(n => orphan ? n.kind === "orphan" : n.kind === "c" && n.idx === p.behind) : -1;
  const c0 = dep ? Math.min(tCol, fCol) : 0, c1 = dep ? Math.max(tCol, fCol) : 0, span = c1 - c0 + 1;
  const fwd = dep && tCol >= fCol;
  const spanMin = Math.max(64, Math.ceil((5 * 72 + 40) / span));
  const same = dep && c0 === c1;
  const b0 = same ? Math.max(0, c0 - 1) : c0, b1 = same ? Math.min(win.length - 1, c0 + 1) : c1;
  const laneStyle = dep ? { display: "grid", gridTemplateColumns: win.map((_, i) => i >= b0 && i <= b1 ? `minmax(${same ? 130 : Math.min(spanMin, 200)}px, ${same ? 3 : 4}fr)` : "minmax(56px, 1fr)").join(" "), gridTemplateRows: "auto auto", rowGap: 14, alignItems: "start" } : undefined;
  const focus = sel != null ? p.list[sel] : orphan ? p.list[tIdx] : pickIdx >= 0 ? p.list[pickIdx] : p.at;
  const focusIdx = sel != null ? sel : orphan ? tIdx : pickIdx >= 0 ? pickIdx : p.behind;
  const rel = !dep && pickIdx >= 0 && focusIdx === pickIdx && pickIdx !== p.behind ? `selected · ${pickIdx < p.behind ? `${p.behind - pickIdx} ahead of` : `${pickIdx - p.behind} before`} ${host}` : orphan ? (focusIdx === tIdx ? `deploy target for ${host}` : `on ${branch}`) : focusIdx === p.behind ? `the commit ${host} is ${verb}` : focusIdx === 0 ? `HEAD · ${p.behind} ahead of ${host}` : focusIdx < p.behind ? `${p.behind - focusIdx} ahead of ${host}` : `${focusIdx - p.behind} before ${host}`;
  return (
    <section className={cls}>
      {top(dep ? banner : <div className="fp-summary">
        <div className="fp-headline">{p.behind === 0 ? <>{host} is on the latest commit of <span className="mono">{sys.flake}/{branch}</span>.</> : <>{host} is <strong className="fp-behind-n">{p.behind} commit{p.behind === 1 ? "" : "s"}</strong> behind <span className="mono">{sys.flake}/{branch}</span>.</>}</div>
        <div className="fp-sub">{p.behind === 0 ? <>The {verb} store path was built from HEAD, <span className="mono">{p.head.sha}</span> ({p.head.at}).</> : <>The {verb} store path was built from <span className="mono">{p.at.sha}</span> ({p.at.at}). HEAD is <span className="mono">{p.head.sha}</span> ({p.head.at}).</>}</div>
      </div>)}
      <div className="fp-lane-wrap">
        <div ref={laneRef} className={`fp-lane${dep ? " fp-deploying" : ""}${onPick && !dep ? " fp-picking" : ""}`} style={laneStyle}>
          {win.map((n, i) => {
            const leftAhead = !orphan && n.idx < p.behind, rightAhead = !orphan && n.idx <= p.behind && n.idx > 0;
            const first = i === 0, last = i === win.length - 1;
            if (n.kind === "orphan") return (
              <div key="orphan" data-fp-idx="orphan" className="fp-node fp-orphan-node" style={{ gridRow: 1, gridColumn: i + 1 }}>
                <div className="fp-top">{landed ? <span className="fp-from-tag">was here</span> : <span className="fp-pin fp-pin-unknown"><Icon name="server" size={11}/> {host}</span>}</div>
                <div className="fp-rail"><span className="fp-seg fp-orphan-seg"></span><button className="fp-dot fp-qdot focus-ring" title={`Untracked · ${sys.storePath || ""}`}>?</button><span className={`fp-seg${last ? " fp-none" : ""}`}></span></div>
                <div className="fp-bot">untracked</div>
              </div>
            );
            if (n.kind === "gap") return (
              <div key={"g" + i} className="fp-node fp-gap" style={dep ? { gridRow: 1, gridColumn: i + 1 } : undefined}>
                <div className="fp-top"></div>
                <div className="fp-rail"><span className={`fp-seg${leftAhead ? " fp-ahead" : ""}${travelL(n.idx) ? " fp-travel" : ""}`}></span><span className="fp-gap-pill">+{n.count}</span><span className={`fp-seg${rightAhead ? " fp-ahead" : ""}${travelR(n.idx - n.count + 1) ? " fp-travel" : ""}`}></span></div>
                <div className="fp-bot">commits</div>
              </div>
            );
            const c = p.list[n.idx], isPick = !dep && !!pickSha && n.idx !== p.behind && [c.sha, c.rawSha].some(x => x && (x.startsWith(pickSha) || pickSha.startsWith(x))), isSys = n.idx === pinAt, isFrom = dep && !orphan && n.idx === p.behind && !isSys, isLast = orphan && n.idx === p.behind && !isSys && !isTgt, isTgt = dep && !landed && n.idx === tIdx && !isSys, isHead = n.idx === 0, peers = p.peers[n.idx];
            return (
              <div key={c.sha + n.idx} data-fp-idx={n.idx} className={`fp-node${isPick ? " fp-picked" : ""}${onPick && !dep ? " fp-pickable" : ""}${c.noSha ? " fp-nosha" : ""}${isSys ? " fp-sys" : ""}${isTgt ? " fp-target" : ""}${isFrom ? " fp-from" : ""}${isHead ? " fp-head" : ""}${focusIdx === n.idx ? " fp-sel" : ""}${n.idx < p.behind ? " fp-is-ahead" : ""}`}
                style={dep ? { gridRow: 1, gridColumn: i + 1 } : undefined}
                onMouseEnter={() => setSel(n.idx)} onMouseLeave={() => setSel(null)}>
                <div className="fp-top">
                  {isSys && <span className={`fp-pin${attest && !dep ? " fp-pin-alert" : ""}`} title={attest && !dep ? attest.label : undefined}><Icon name={attest && !dep ? "warn" : "server"} size={11}/> {host}</span>}
                  {isTgt && <span className="fp-pin fp-pin-target">{dep.kind === "rollback" ? <Icon name="rollback" size={11}/> : <Icon name="deploy" size={11}/>} {dep.kind === "rollback" ? "rollback" : "target"}</span>}
                  {isTgt && isHead && <span className="fp-head-tag">HEAD</span>}
                  {isPick && <span className="fp-pin fp-pin-target"><Icon name="deploy" size={11}/> selected</span>}
                  {isFrom && <span className="fp-from-tag">was here</span>}
                  {isLast && <span className="fp-from-tag">last tracked</span>}
                  {isHead && (dep ? !isTgt : !isSys) && <span className="fp-head-tag">HEAD</span>}
                </div>
                <div className="fp-rail">
                  <span className={`fp-seg${first ? " fp-none" : ""}${leftAhead ? " fp-ahead" : ""}${travelL(n.idx) ? " fp-travel" : ""}`}></span>
                  <button className="fp-dot focus-ring" title={onPick && !dep ? `Select ${c.sha} as deploy target` : `${c.sha} · ${c.msg}`} onClick={() => onPick && !dep ? onPick(c, n.idx === p.behind) : open(c)} onFocus={() => setSel(n.idx)}></button>
                  <span className={`fp-seg${last ? " fp-none" : ""}${rightAhead ? " fp-ahead" : ""}${travelR(n.idx) ? " fp-travel" : ""}`}></span>
                </div>
                <div className="fp-bot mono">{c.sha}</div>
                {!dep && approvalSha && c.sha === approvalSha && <div className="fp-appr" title="Awaiting deploy approval">approval {approvalText}</div>}
                {peers && <div className="fp-peers" title={peers.join(", ")}>+{peers.length} host{peers.length === 1 ? "" : "s"}</div>}
              </div>
            );
          })}
          {dep && (
            <div ref={branchRef} className={`fp-branch${geo && geo.tight ? " tight" : ""}${fwd ? "" : " back"}${same ? " same" : ""}${landed ? " landed" : ""}${dep.kind === "rollback" ? " rb" : ""}`} style={{ gridRow: 2, gridColumn: `${b0 + 1} / ${b1 + 2}`, visibility: geo ? "visible" : "hidden" }}>
              {geo && !geo.same && <span className="fp-u" style={{ left: geo.l, right: geo.r, top: geo.top, height: 10 - geo.top }}></span>}
              {geo && geo.same && (() => {
                const L = geo.l, R = geo.w - geo.r, cx = geo.cx, s = geo.fr + 2, yb = 10 - geo.cy, rad = 12, mid = yb * 0.5;
                const d = `M ${cx - s} 0 C ${cx - s - 34} 0 ${L} ${mid * 0.35} ${L} ${mid} L ${L} ${yb - rad} Q ${L} ${yb} ${L + rad} ${yb} L ${R - rad} ${yb} Q ${R} ${yb} ${R} ${yb - rad} L ${R} ${mid} C ${R} ${mid * 0.35} ${cx + s + 34} 0 ${cx + s} 0`;
                return <svg className="fp-uloop" style={{ top: geo.cy, height: yb + 2 }} width={geo.w} overflow="visible" aria-hidden="true"><path d={d}/></svg>;
              })()}
              <div className="fp-branch-row" style={geo ? { paddingLeft: geo.l + 10, paddingRight: geo.r + 10 } : undefined}>
                {stageNodes.map(st => {
                  const inOn = st.past || st.cur, outOn = st.past;
                  const [lOn, rOn] = fwd ? [inOn, outOn] : [outOn, inOn];
                  return (
                    <div key={st.s} className={`fp-stage${st.past ? " past" : ""}${st.cur ? " cur" : ""}`} title={st.label}>
                      <div className="fp-rail">
                        <span className={`fp-seg${lOn ? " on" : ""}${st.cur && fwd ? " live" : ""}`}></span>
                        <span className="fp-sdot">{st.past ? <Icon name="check" size={9}/> : st.cur ? <span className="deploy-step-pulse"></span> : null}</span>
                        <span className={`fp-seg${rOn ? " on" : ""}${st.cur && !fwd ? " live" : ""}`}></span>
                      </div>
                      <div className="fp-bot">{st.label}</div>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </div>
      <div className="fp-focus">
        <span className="mono fp-focus-sha">{focus.sha}</span>
        <span className="fp-focus-msg">{focus.msg}</span>
        <span className="fp-focus-meta">{focus.author} · {focus.at} · {rel}</span>
        {onPick && !dep && <span className="fp-pick-hint">Click a commit to select it as the deploy target</span>}
      </div>
      <div className="fp-store-row"><div className="fp-store">
        <span className="fp-store-k">Store path</span>
        <span className="mono fp-store-v" title={sys.storePath}>{sys.storePath || "—"}</span>
        <span className="fp-store-arrow">←</span>
        {orphan ? <span className="fp-store-unknown">no matching evaluation</span> : <span className="mono">{p.at.sha} · nixosConfigurations.{sys.assurance?.configName || host}</span>}
      </div>
      {p.behind > 0 && !dep && !onPick && (
        <div className="fp-actions">
          <button className="btn btn-ghost xs focus-ring" onClick={() => open(p.head)}><Icon name="git" size={11}/> Open HEAD</button>
          {p.mode === "managed" && onDeployHead && <button className="btn btn-primary xs focus-ring" onClick={() => onDeployHead(p.head)}><Icon name="deploy" size={11}/> Deploy HEAD</button>}
        </div>
      )}
      </div>
    </section>
  );
}

Object.assign(window, { FlakePositionCard, flakePositionOf });
