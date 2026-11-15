// Deploy tab — "Select target" card. A vertical lane that mirrors the flake-history graph:
// newer commits above the deployed one, older below; generations use the same rail.

function DeployTargetPicker({ approvalSha, approvalText, sys, mode, switchMode, commits, generations, selected, setTarget, showOlder, setShowOlder, onOpenCommit }) {
  const cur = Math.max(0, commits.findIndex(c => c.current));
  const lim = Math.max(8, cur + 4);
  const selIdx = commits.findIndex(c => selected.kind === "commit" && c.sha === selected.id);
  const rows = showOlder ? commits.map((c, i) => [c, i]) : commits.map((c, i) => [c, i]).filter(([c, i]) => i < lim || i === selIdx);
  const hidden = commits.length - rows.length;
  const newer = commits.findIndex(c => c.current);
  const openSha = (e, x) => { e.stopPropagation(); onOpenCommit?.(x); };
  const listRef = React.useRef(null);
  const [fade, setFade] = React.useState({ t: false, b: false });
  const syncFade = () => { const el = listRef.current; if (!el) return; const t = el.scrollTop > 2, b = el.scrollTop + el.clientHeight < el.scrollHeight - 2; setFade(f => f.t === t && f.b === b ? f : { t, b }); };
  React.useLayoutEffect(() => {
    const el = listRef.current; if (!el) return;
    const row = el.querySelector(".tp-row.sel");
    if (row) {
      const top = row.offsetTop, bot = top + row.offsetHeight;
      if (top < el.scrollTop + 8) el.scrollTo({ top: Math.max(0, top - 24), behavior: "smooth" });
      else if (bot > el.scrollTop + el.clientHeight - 8) el.scrollTo({ top: bot - el.clientHeight + 24, behavior: "smooth" });
    }
    syncFade();
  }, [selected.id, mode, showOlder]);

  const commitRow = ([c, i], k) => {
    const isSel = selected.kind === "commit" && selected.id === c.sha;
    const ahead = newer > 0 && i < newer;
    const prev = rows[k - 1];
    const gapAbove = prev && prev[1] !== i - 1;
    return (
      <React.Fragment key={c.sha}>
        {gapAbove && <div className="tp-gap"><span className="tp-lane"><i></i></span><span>{i - prev[1] - 1} commits</span></div>}
        <button className={`tp-row focus-ring${isSel ? " sel" : ""}${c.current ? " cur" : ""}${ahead ? " ahead" : ""}${i === 0 ? " head" : ""}${k === 0 ? " first" : ""}${k === rows.length - 1 && !hidden ? " last" : ""}`}
          onClick={() => setTarget({ kind: "commit", id: c.sha, label: c.sha, sub: c.message, sha: c.sha })}>
          <span className="tp-lane"><i></i><b></b></span>
          <span className="tp-main">
            <span className="tp-msg">{c.message}</span>
            <span className="tp-meta">
              <span className="mono tp-sha" title={`Open ${c.sha} in Flakes`} onClick={e => openSha(e, { sha: c.sha, msg: c.message, flake: sys.flake, author: c.author, at: c.when })}>{c.sha}</span>
              <span>{c.author}</span><span>{c.when}</span>
            </span>
          </span>
          <span className="tp-side">
            {c.current ? <span className="tp-tag tp-tag-cur"><Icon name="server" size={10}/> {sys.hostname}</span>
              : i === 0 ? <span className="tp-tag tp-tag-head">HEAD</span> : null}
            {approvalSha === c.sha && <span className="tp-tag tp-tag-appr" title="Awaiting deploy approval">approval {approvalText}</span>}
            {!c.current && c.buildStatus !== "cached" && <span className="tp-build" title="Building"><Spinner size={10}/> building</span>}
          </span>
        </button>
      </React.Fragment>
    );
  };

  const genRow = (g, k) => {
    const isSel = selected.kind === "generation" && selected.id === g.id;
    const origin = g.origin === "cf" ? null : g.origin === "local" ? "local rebuild" : "not in cache";
    return (
      <button key={g.id} className={`tp-row tp-gen focus-ring o-${g.origin}${isSel ? " sel" : ""}${g.current ? " cur" : ""}${k === 0 ? " first" : ""}${k === generations.length - 1 ? " last" : ""}`}
        title={g.origin === "local" ? (g.driftHint || "Manual nixos-rebuild on host") : g.origin === "unknown" ? "Closure no longer in cache" : undefined}
        onClick={() => setTarget({ kind: "generation", id: g.id, label: `gen #${g.id}`, sub: g.msg, sha: g.sha, origin: g.origin })}>
        <span className="tp-lane"><i></i><b></b></span>
        <span className="tp-main">
          <span className="tp-msg">{g.msg}</span>
          <span className="tp-meta">
            <span className="mono tp-gen-id">#{g.id}</span>
            {g.sha ? <span className="mono tp-sha" title={`Open ${g.sha} in Flakes`} onClick={e => openSha(e, { sha: g.sha, msg: g.msg, flake: g.flake || sys.flake, author: g.by, at: g.at })}>{g.sha}</span> : <span className="tp-nosha">no commit</span>}
            <span>{g.at}</span>
            {origin && <span className={`tp-origin o-${g.origin}`}><Icon name="warn" size={9}/> {origin}</span>}
          </span>
        </span>
        <span className="tp-side">{g.current && <span className="tp-tag tp-tag-cur"><Icon name="server" size={10}/> active</span>}</span>
      </button>
    );
  };

  return (
    <section className="card sd-card tp-card">
      <div className="sd-card-head tp-head">
        <h2>Deploy target</h2>
        <div className="tp-switch" role="tablist">
          <button role="tab" aria-selected={mode === "commit"} className={mode === "commit" ? "on" : ""} onClick={() => switchMode("commit")}><Icon name="git" size={12}/> Commits</button>
          <button role="tab" aria-selected={mode !== "commit"} className={mode !== "commit" ? "on" : ""} onClick={() => switchMode("generation")}><Icon name="rollback" size={12}/> Generations</button>
        </div>
      </div>
      <div className="tp-sub">
        {mode === "commit"
          ? <>{sys.flake} · {newer > 0 ? <><strong className="tp-ahead-n">{newer} newer</strong> than what's deployed</> : "deployed commit is HEAD"}</>
          : <>Generations on {sys.hostname}. Switching needs no build.</>}
      </div>
      <div className={`tp-list${fade.t ? " fade-t" : ""}${fade.b ? " fade-b" : ""}`} ref={listRef} onScroll={syncFade}>
        {mode === "commit" ? rows.map(commitRow) : generations.map(genRow)}
        {mode === "commit" && commits.length > lim && (
          <button className="tp-more focus-ring" onClick={() => setShowOlder(v => !v)}>
            <span className="tp-lane"><i></i></span>{showOlder ? "Show fewer" : `Show ${hidden} older commits`}
          </button>
        )}
      </div>
    </section>
  );
}

window.DeployTargetPicker = DeployTargetPicker;
