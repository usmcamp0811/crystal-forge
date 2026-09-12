/* ---------- Config Explorer ----------------------------------------------
   Observational, lazy, path-scoped inspection of an exact target
   (revision + configuration). Layout is an inspector, not a document: a dense
   attribute tree on the left, a persistent detail pane on the right, and one
   meta strip carrying evaluation facts, inventory state, and comparison
   readiness. Nothing expands inline, so rows never jump under the cursor.

   Invariants this UI encodes:
     1. Explorer data is NEVER deployment authority.
     2. Partial data never implies absence.
     3. Waiting for evaluator capacity is not running; progress is never a
        fabricated percentage of an unknown corpus.                          */

function ConfigExplorerTab({ sys, initialRev, onOpenFlake }) {
  const Badge = window.CfgInputBadge, Tray = window.ModuleSourceTray;
  const [summary, setSummary] = React.useState(null);
  const [module, setModule] = React.useState(null);
  const [mode, setMode] = React.useState("browse");

  const gens = React.useMemo(() => window.buildGenerations(sys), [sys.id]);
  const commits = React.useMemo(() => {
    const list = window.buildSystemCommits(sys);
    if (initialRev && !list.some(c => c.sha === initialRev)) {
      return [{ sha: initialRev, message: "(from flake explorer)", author: "—", when: "referenced", current: false, deployed: false, synthetic: true }, ...list];
    }
    return list;
  }, [sys.id, initialRev]);
  const [revMode, setRevMode] = React.useState(initialRev ? "commit" : "generation");
  const [genId, setGenId] = React.useState(null);
  const [commitSha, setCommitSha] = React.useState(initialRev || null);
  React.useEffect(() => { setRevMode(initialRev ? "commit" : "generation"); setGenId(null); setCommitSha(initialRev || null); }, [sys.id, initialRev]);

  const activeGen = (genId !== null && gens.find(g => g.id === genId)) || gens[0];
  const activeCommit = (commitSha && commits.find(c => c.sha === commitSha)) || commits.find(c => c.sha === sys.commit) || commits[0];
  const onCommits = revMode === "commit";
  const rev = onCommits ? activeCommit.sha : (activeGen && activeGen.sha ? activeGen.sha : sys.commit);
  const isHistorical = onCommits ? activeCommit.sha !== sys.commit : !!activeGen && !activeGen.current;
  const deployedHere = gens.some(g => g.sha === rev);

  const [nodes, setNodes] = React.useState({});
  const [open, setOpen] = React.useState({});
  const [sel, setSel] = React.useState(null);
  const [details, setDetails] = React.useState({});
  const [prov, setProv] = React.useState({});
  const target = `${sys.id}|${rev}`;

  React.useEffect(() => { // Phase 0 → 1: new exact target, nothing carries over.
    setNodes({ "": { status: "inspecting" } }); setOpen({}); setSel(null); setDetails({}); setProv({});
    let live = true;
    window.ConfigAPI.treeChildren(sys, rev, "").then(r => { if (live) setNodes(n => ({ ...n, "": r })); });
    window.ConfigAPI.summary(sys, rev).then(s => { if (live) setSummary(s); });
    return () => { live = false; };
  }, [target]);

  const expand = (path) => { // Phase 2: scoped prefix expansion.
    setOpen(o => ({ ...o, [path]: !o[path] }));
    if (nodes[path]) return;
    setNodes(n => ({ ...n, [path]: { status: "inspecting" } }));
    window.ConfigAPI.treeChildren(sys, rev, path).then(r => setNodes(n => ({ ...n, [path]: r })));
  };
  const select = (path) => { // Phase 3: exact option detail, no provenance.
    setSel(path);
    if (details[path]) return;
    setDetails(d => ({ ...d, [path]: { status: "inspecting" } }));
    window.ConfigAPI.treeOption(sys, rev, path).then(r => setDetails(d => ({ ...d, [path]: { status: "succeeded", opt: r } })));
  };
  const loadProv = (path) => { // Phase 4: provenance, explicitly requested.
    setProv(p => ({ ...p, [path]: { status: "inspecting" } }));
    window.ConfigAPI.treeProvenance(sys, rev, path).then(r => setProv(p => ({ ...p, [path]: { status: "succeeded", ...r } })));
  };
  /* Everything actually observed so far, file by file — this is what backs
     the Sources pane when no certified inventory exists. It grows only as
     the user inspects provenance; it is never a preloaded module registry. */
  const observedSources = React.useMemo(() => {
    const byFile = new Map();
    Object.values(prov).forEach(p => {
      if (p.status !== "succeeded") return;
      (p.defs || []).forEach(d => {
        if (d.unknown) return;
        if (!byFile.has(d.file)) byFile.set(d.file, { path: d.file, input: d.input, rev: d.rev, defCount: 0 });
        byFile.get(d.file).defCount++;
      });
    });
    return Array.from(byFile.values());
  }, [prov]);
  const revealPrefix = (p) => { // open every ancestor of a search hit
    const toks = p.split(".");
    const acc = [];
    toks.slice(0, -1).forEach((t, i) => acc.push(toks.slice(0, i + 1).join(".")));
    acc.forEach(a => { if (!nodes[a]) { setNodes(n => ({ ...n, [a]: { status: "inspecting" } })); window.ConfigAPI.treeChildren(sys, rev, a).then(r => setNodes(n => ({ ...n, [a]: r }))); } });
    setOpen(o => { const next = { ...o }; acc.forEach(a => next[a] = true); return next; });
    setMode("browse");
    select(p);
  };

  /* Reverse lookup: options a definition file sets. Complete only when a
     certified inventory exists; otherwise scoped to inspected prefixes. */
  const [srcFilter, setSrcFilter] = React.useState(null);
  React.useEffect(() => { setSrcFilter(null); }, [target]);
  const filterBySource = (m) => {
    setSrcFilter({ file: m.path, input: m.input, status: "inspecting" });
    window.ConfigAPI.sourceOptions(sys, rev, m.path, inspected)
      .then(r => setSrcFilter(f => (f && f.file === m.path ? { ...f, status: "succeeded", ...r } : f)));
  };

  /* Options this host's own modules explicitly configure — the delta
     already materialized at eval time, not a corpus scan. Loaded once when
     the Configured tab is opened. */
  const [configured, setConfigured] = React.useState(null);
  React.useEffect(() => { setConfigured(null); }, [target]);
  React.useEffect(() => {
    if (mode === "configured" && !configured) {
      setConfigured({ status: "inspecting" });
      window.ConfigAPI.configured(sys, rev).then(r => setConfigured({ status: "succeeded", ...r }));
    }
  }, [mode, target]);

  const inspected = React.useMemo(() => Object.keys(nodes).filter(k => nodes[k] && nodes[k].status === "succeeded" && k !== ""), [nodes]);

  const [inv, setInv] = React.useState({ state: "not_requested" });
  React.useEffect(() => {
    setInv(window.ConfigAPI.inventoryStatus(sys, rev));
    const t = setInterval(() => setInv(window.ConfigAPI.inventoryStatus(sys, rev)), 700);
    return () => clearInterval(t);
  }, [target]);
  const complete = inv.state === "succeeded";

  const [q, setQ] = React.useState("");
  const [dq, setDq] = React.useState("");
  const [res, setRes] = React.useState(null);
  const [searching, setSearching] = React.useState(false);
  const reqId = React.useRef(0);
  React.useEffect(() => { const t = setTimeout(() => setDq(q), 240); return () => clearTimeout(t); }, [q]);
  React.useEffect(() => {
    if (mode !== "search") return;
    const id = ++reqId.current;
    setSearching(true);
    window.ConfigAPI.query(sys, { q: dq, filter: "all", offset: 0, limit: 80, rev, scope: complete ? null : inspected })
      .then(r => { if (id === reqId.current) { setRes(r); setSearching(false); } });
  }, [mode, dq, target, complete, inspected.length]);

  const root = nodes[""] || { status: "inspecting" };
  const shortFile = (f) => f.replace(/\/default\.nix$/, "").replace(/\.nix$/, "");

  return (
    <div className="cfgx">
      <div className="cfgx-top">
        <div className="cfgx-target">
          <span className="cfgx-target-label">TARGET</span>
          <span className="mono cfgx-target-path">
            <span className="dim">{sys.flake}#nixosConfigurations.</span>{sys.hostname}<span className="dim">.config</span>
          </span>
        </div>
        <div className="cfgx-top-r">
          <div className="cfgx-rev">
            <div className="seg xs">
              <button className={!onCommits?"active":""} onClick={()=>setRevMode("generation")}>Generations</button>
              <button className={onCommits?"active":""} onClick={()=>setRevMode("commit")}>Commits</button>
            </div>
            {onCommits ? (
              <select className="cfgx-select focus-ring" value={activeCommit.sha} onChange={e=>setCommitSha(e.target.value)}>
                {commits.map(c => <option key={c.sha} value={c.sha}>{c.sha}{c.sha === sys.commit ? " (deployed)" : ""} · {c.when}</option>)}
              </select>
            ) : (
              <select className="cfgx-select focus-ring" value={activeGen ? activeGen.id : ""} onChange={e=>setGenId(Number(e.target.value))}>
                {gens.map(g => <option key={g.id} value={g.id} disabled={!g.sha}>gen #{g.id}{g.current ? " (current)" : ""}{g.sha ? ` · ${g.sha}` : " · no commit"}</option>)}
              </select>
            )}
          </div>
          <span className="cfgx-obs" title="Observational only. Deployment gating uses the policy evaluator, which evaluates this configuration independently — an Explorer cache hit never substitutes for an authoritative policy evaluation.">observational</span>
        </div>
      </div>

      {isHistorical && (
        <div className="cfgx-hist">
          <Icon name="info" size={12}/>
          <div>{onCommits
            ? <>Inspecting what this host <em>would</em> evaluate to at <span className="mono">{rev}</span>{deployedHere ? "" : " — a revision never deployed here"}, not what is running now.</>
            : <>Inspecting generation #{activeGen.id} (<span className="mono">{rev}</span>), not what is running now.</>}</div>
          <button className="cfgx-link" onClick={()=>{ setRevMode("generation"); setGenId(null); setCommitSha(null); }}>back to current</button>
        </div>
      )}

      <div className="cfgx-meta">
        <div className="cfgx-meta-i" title="Whether the primary evaluator has produced a result for this exact target. Config Explorer never initiates or substitutes for primary evaluation — this only reports what already exists."><span>primary eval</span><b className={summary ? "ok" : ""}>{summary ? "complete" : "…"}</b></div>
        <div className="cfgx-meta-i"><span>eval time</span><b className="mono">{summary ? `${summary.facts.evalSeconds}s` : "…"}</b></div>
        <div className="cfgx-meta-i"><span>packages</span><b className="mono">{summary ? summary.facts.packages : "…"}</b></div>
        <div className="cfgx-meta-i"><span>closure</span><b className="mono">{summary ? summary.facts.closure : "…"}</b></div>
        <div className="cfgx-meta-i"><span>carrier</span><b className="mono" title={summary ? summary.facts.drv : ""}>{summary ? summary.facts.drv.replace("/nix/store/","").slice(0,10) + "…" : "…"}</b></div>
        <div className="cfgx-meta-sp"/>
        <InventoryMeta inv={inv} onRequest={()=>{ window.ConfigAPI.requestFullInventory(sys, rev); setInv(window.ConfigAPI.inventoryStatus(sys, rev)); }}/>
        <div className="cfgx-meta-i" title={complete
          ? "A certified complete inventory exists for this revision, so Changed and Drift comparison is meaningful."
          : "Changed and Drift require a certified complete inventory. Scoped Explorer observations cannot establish that nothing changed: incomplete data ≠ zero changes, incomplete data ≠ no drift."}>
          <span>comparison</span><b className={complete ? "ok" : "warn"}>{complete ? "ready" : "unavailable"}</b>
        </div>
      </div>

      <div className="cfgx-tools">
        <div className="seg xs">
          <button className={mode==="browse"?"active":""} onClick={()=>setMode("browse")}>Browse</button>
          <button className={mode==="configured"?"active":""} onClick={()=>setMode("configured")}>Configured</button>
          <button className={mode==="search"?"active":""} onClick={()=>setMode("search")}>Search</button>
        </div>
        <div className="cfgx-search">
          <Icon name="search" size={12}/>
          <input value={q} onChange={e=>{ setQ(e.target.value); setMode("search"); }} placeholder={complete ? "Search all options in this revision…" : "Search inspected options…"}/>
          {q && <button className="btn-icon xs focus-ring" title="Clear" onClick={()=>setQ("")}><Icon name="x" size={12}/></button>}
        </div>
        <div className="cfgx-count mono">
          {mode === "browse"
            ? (root.status === "succeeded" ? `${root.children.length} attrs` : "inspecting…")
            : mode === "configured"
            ? (configured && configured.status === "succeeded" ? `${configured.total} configured` : "inspecting…")
            : (searching ? "searching…" : res ? `${res.total.toLocaleString()} hit${res.total === 1 ? "" : "s"}` : "")}
        </div>
      </div>

      {mode === "search" && (
        <div className={`cfgx-scope ${complete ? "full" : "partial"}`}>
          {complete
            ? <span>Complete search over the certified inventory for <span className="mono">{rev}</span>.</span>
            : <span>Scoped to <b>{inspected.length}</b> inspected prefix{inspected.length === 1 ? "" : "es"}. Paths never inspected are not searched — an absent result does not mean the option is absent.</span>}
        </div>
      )}

      <div className="cfgx-body">
        <div className="cfgx-tree-col">
          {srcFilter && (
            <div className={`cfgx-filter${srcFilter.complete ? " full" : " partial"}`}>
              <span className="cfgx-filter-l">defined by</span>
              <span className="mono cfgx-filter-f">{srcFilter.file}</span>
              <span className="cfgx-filter-n">
                {srcFilter.status === "inspecting" ? "resolving…"
                  : srcFilter.complete
                    ? `${srcFilter.total} option${srcFilter.total === 1 ? "" : "s"}`
                    : `${srcFilter.total} in inspected paths — complete list needs an inventory`}
              </span>
              <button type="button" className="cfgx-link" onClick={()=>setSrcFilter(null)}>clear</button>
            </div>
          )}
          <div className="cfgx-colhead"><span>{srcFilter ? "option" : mode === "configured" ? "configured option" : mode === "browse" ? "config.*" : "match"}</span><span>value</span><span>defined by</span></div>
          <div className="cfgx-scroll">
            {srcFilter ? (
              <div className="cfgx-tree">
                {srcFilter.status === "inspecting" && <div className="cfgx-empty"><span className="cfgx-insp"><i/> resolving definitions…</span></div>}
                {srcFilter.status === "succeeded" && srcFilter.rows.length === 0 && (
                  <div className="cfgx-empty">{srcFilter.complete
                    ? "this file defines no options on this host"
                    : "none of this file's definitions fall in a prefix you have inspected yet — expand more prefixes, or request a complete inventory"}</div>
                )}
                {srcFilter.status === "succeeded" && srcFilter.rows.map(o => (
                  <div key={o.path} className={`cfgx-row hit${sel === o.path ? " sel" : ""}`} onClick={()=>revealPrefix(o.path)}>
                    <span className="cfgx-name mono"><span className="dim">{o.path.split(".").slice(0,-1).join(".")}{o.path.includes(".") ? "." : ""}</span>{o.path.split(".").pop()}</span>
                    <span className="cfgx-val mono">{o.evalError ? <em className="err">not evaluated</em> : <CfgVal v={o.value}/>}</span>
                    <span className="cfgx-by mono" title={o.contributes ? "merged type: this file contributes to the result" : "override-semantic type"}>{o.contributes ? "contributes" : "sets"}</span>
                  </div>
                ))}
              </div>
            ) : mode === "configured" ? (
              <div className="cfgx-tree">
                {(!configured || configured.status === "inspecting") && <div className="cfgx-empty"><span className="cfgx-insp"><i/> inspecting configured options…</span></div>}
                {configured && configured.status === "succeeded" && configured.rows.map(o => (
                  <div key={o.path} className={`cfgx-row hit${sel === o.path ? " sel" : ""}`} onClick={()=>revealPrefix(o.path)}>
                    <span className="cfgx-name mono"><span className="dim">{o.path.split(".").slice(0,-1).join(".")}{o.path.includes(".") ? "." : ""}</span>{o.path.split(".").pop()}</span>
                    <span className="cfgx-val mono">{o.evalError ? <em className="err">not evaluated</em> : <CfgVal v={o.value}/>}</span>
                    <span className={`cfgx-by mono in-${inputKind(o.sourceInput)}`}>{o.sourceInput || "«unknown»"}</span>
                  </div>
                ))}
              </div>
            ) : mode === "browse" ? (
              root.status === "inspecting" ? <div className="cfgx-empty">inspecting root hierarchy…</div>
              : root.status === "failed" ? (
                <div className="cfgx-rootfail">
                  <div className="cfgx-fail-h"><Icon name="warn" size={12}/> root inspection failed</div>
                  <div className="mono cfgx-fail-msg">{root.error}</div>
                  <div className="cfgx-fail-note">No trustworthy root hierarchy could be established, so browsing is unavailable for this target.</div>
                </div>
              ) : (
                <div className="cfgx-tree" role="tree">
                  {root.children.map(c => (
                    <CfgNode key={c.path} node={c} depth={0} nodes={nodes} open={open} sel={sel} details={details} onExpand={expand} onSelect={select}/>
                  ))}
                </div>
              )
            ) : (
              <div className="cfgx-tree">
                {searching && <div className="cfgx-empty">searching cached observations…</div>}
                {!searching && res && res.rows.length === 0 && <div className="cfgx-empty">{complete ? "no options match" : "no match in inspected paths"}</div>}
                {!searching && res && res.rows.map(o => (
                  <div key={o.path} className={`cfgx-row hit${sel === o.path ? " sel" : ""}`} onClick={()=>revealPrefix(o.path)}>
                    <span className="cfgx-name mono"><span className="dim">{o.path.split(".").slice(0,-1).join(".")}{o.path.includes(".") ? "." : ""}</span>{o.path.split(".").pop()}</span>
                    <span className="cfgx-val mono">{o.evalError ? <em className="err">not evaluated</em> : <CfgVal v={o.value}/>}</span>
                    <span className={`cfgx-by mono in-${inputKind(o.sourceInput)}`}>{o.sourceInput || "«unknown»"}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        <Inspector sel={sel} details={details} prov={prov} onProv={loadProv} onModule={setModule} sys={sys} rev={rev} onClear={()=>setSel(null)}
          onFilterSource={filterBySource} srcFilter={srcFilter} observedSources={observedSources}
          onOpenFlake={onOpenFlake} Badge={Badge} shortFile={shortFile} summary={summary} complete={complete}/>
      </div>

      {module && Tray && <Tray sys={sys} mod={module} onOpenFlake={onOpenFlake} onClose={()=>setModule(null)}/>}
    </div>
  );
}

/* Value colouring by Nix value shape — booleans, numbers, strings, lists,
   attrsets, null and store paths each read differently at a glance. */
function valKind(v) {
  if (v == null) return "null";
  const s = String(v).trim();
  if (s === "true" || s === "false") return "bool";
  if (s === "null") return "null";
  if (/^\/nix\/store\//.test(s)) return "store";
  if (/^"/.test(s)) return "str";
  if (/^-?\d+(\.\d+)?$/.test(s)) return "num";
  if (/^\[/.test(s)) return "list";
  if (/^\{/.test(s)) return "attrs";
  if (/^<|^‹/.test(s)) return "fn";
  return "other";
}
function CfgVal({ v }) { return <span className={`v-${valKind(v)}`}>{v}</span>; }
function inputKind(i) {
  if (!i) return "other";
  if (i === "self") return "self";
  if (i === "nixpkgs") return "nixpkgs";
  if (/crystal/.test(i)) return "cf";
  return "other";
}

/* Tree row. A failed prefix reports in place; healthy siblings are unaffected
   and no children are fabricated below it. */
function CfgNode({ node, depth, nodes, open, sel, details, onExpand, onSelect }) {
  const isOpen = !!open[node.path];
  const state = nodes[node.path];
  const pad = 8 + depth * 14;

  if (node.isLeaf) {
    /* The cheap tree request gives identity only (name/path/kind) — not value
       or provenance. Once the user selects this row, its Phase-3 detail lands
       in `details` and the row becomes progressively richer instead of
       fetching anything up front. */
    const det = details[node.path];
    const o = det && det.status === "succeeded" ? det.opt : null;
    return (
      <div className={`cfgx-row${sel === node.path ? " sel" : ""}`} style={{ paddingLeft: pad }} onClick={()=>onSelect(node.path)} role="treeitem" title={`config.${node.path}`}>
        <span className="cfgx-name mono">
          <span className="cfgx-bullet"/>{node.name}
          {o && o.change && <i className="cfgx-dot changed" title="changed vs previous generation"/>}
          {o && o.overridden && <i className="cfgx-dot over" title="more than one definition, override-semantic type: priority selects one"/>}
        </span>
        <span className="cfgx-val mono">
          {det && det.status === "inspecting" ? <span className="cfgx-insp"><i/></span>
            : o ? (o.evalError ? <em className="err">not evaluated</em> : <CfgVal v={o.value}/>)
            : null}
        </span>
        <span className={`cfgx-by mono${o ? ` in-${inputKind(o.sourceInput)}` : ""}`} title={o ? (o.sourceInput || "no _file recorded for this definition") : ""}>
          {o ? (o.sourceInput || "«unknown»") : null}
        </span>
      </div>
    );
  }

  /* A subtree's aggregate option count needs every descendant walked — the
     corpus crawl this API exists to avoid. It is only present on the node
     when a certified inventory already paid that cost; otherwise the branch
     shows no count rather than a fabricated one. */
  const knowsCount = typeof node.optionCount === "number";
  return (
    <React.Fragment>
      <div className={`cfgx-row branch${node.failed ? " failed" : ""}`} style={{ paddingLeft: pad }} onClick={()=>onExpand(node.path)} role="treeitem" aria-expanded={isOpen} title={`config.${node.path}`}>
        <span className="cfgx-name mono">
          <Icon name="chevron-right" size={10} className={`cfgx-caret${isOpen ? " open" : ""}`}/>{node.name}
          {knowsCount && node.changedCount > 0 && <span className="cfgx-chg">{node.changedCount}</span>}
        </span>
        <span className="cfgx-val sub">
          {isOpen && state && state.status === "inspecting" ? <span className="cfgx-insp"><i/> inspecting</span>
            : state && state.status === "failed" ? <span className="err">unavailable</span>
            : knowsCount ? `${node.optionCount.toLocaleString()} opt${node.optionCount === 1 ? "" : "s"}`
            : null}
        </span>
        <span className="cfgx-by"/>
      </div>
      {isOpen && state && state.status === "failed" && (
        <div className="cfgx-branchfail" style={{ marginLeft: pad + 12 }}>
          <div className="mono">{state.error}</div>
          <div>Local to this prefix. Siblings are unaffected; no children are shown here.</div>
        </div>
      )}
      {isOpen && state && state.status === "succeeded" && state.children.map(c => (
        <CfgNode key={c.path} node={c} depth={depth + 1} nodes={nodes} open={open} sel={sel} details={details} onExpand={onExpand} onSelect={onSelect}/>
      ))}
    </React.Fragment>
  );
}

/* Right pane. Idle state carries the module inventory rather than whitespace;
   selected state is the Phase 3 observation plus an explicit Phase 4 fetch. */
function Inspector({ sel, details, prov, onProv, onModule, sys, rev, onClear, onFilterSource, srcFilter, observedSources, onOpenFlake, Badge, shortFile, summary, complete }) {
  const [pane, setPane] = React.useState("option");
  React.useEffect(() => { if (sel) setPane("option"); }, [sel]);
  /* Below a certified inventory, this pane shows only what has actually been
     observed via Inspect provenance — never the full module registry, which
     is a complete-inventory feature. With one, the full per-host source list
     is legitimately free. */
  const srcs = complete ? (summary ? summary.sources : []) : observedSources;
  const showOption = sel && pane === "option";

  const tabs = (
    <div className="cfgx-side-tabs">
      <div className="seg xs">
        <button type="button" className={showOption ? "active" : ""} disabled={!sel} onClick={()=>setPane("option")}>Option</button>
        <button type="button" className={!showOption ? "active" : ""} onClick={()=>setPane("modules")}>Sources <span className="mono">{srcs.length || ""}</span></button>
      </div>
    </div>
  );

  const modulesPane = (
    <React.Fragment>
      <div className="cfgx-side-hint">{complete
        ? <>Every file that defines an option value on this host — one row per <span className="mono">_file</span>, not per logical module: a module split across several files appears as several rows. Select a file to list the options it sets.</>
        : <>Files whose definitions you've actually inspected so far via <span className="mono">Inspect provenance</span> — not a registry of every module on this host, which would need a full crawl. Select one to list the options it sets within prefixes you've inspected.</>}</div>
      <div className="cfgx-mods">
        {srcs.length === 0 && !complete && <div className="cfgx-empty">Nothing observed yet. Select an option and Inspect provenance to populate this list.</div>}
        {srcs.map(m => (
          <div key={m.path} className={`cfgx-mod${srcFilter && srcFilter.file === m.path ? " on" : ""}`}>
            <button type="button" className="cfgx-mod-main focus-ring" onClick={()=>onFilterSource(m)}
              title={`${m.rev ? `${m.input} @ ${m.rev}` : m.input}\nList the options this file sets`}>
              <span className={`cfgx-mod-in in-${inputKind(m.input)}`}>{m.input}</span>
              <span className="mono cfgx-mod-p">{shortFile(m.path)}</span>
              <span className="mono cfgx-mod-n" title={complete
                ? `${m.defCount} definitions on this host`
                : "Definition counts come from a complete inventory. Without one this is the count across the whole revision, shown for reference only."}>{m.defCount}</span>
            </button>
            <button type="button" className="cfgx-mod-src focus-ring" aria-label={`Open source of ${m.path}`} title="Open file source" onClick={()=>onModule(m)}><Icon name="file" size={11}/></button>
          </div>
        ))}
      </div>
    </React.Fragment>
  );

  if (!showOption) return <aside className="cfgx-side">{tabs}{modulesPane}</aside>;

  const det = details[sel], pv = prov[sel], o = det && det.opt;
  const leaf = sel.split(".").pop(), parent = sel.split(".").slice(0, -1).join(".");
  return (
    <aside className="cfgx-side">
      {tabs}
      <div className="cfgx-insp-head">
        <div className="mono cfgx-insp-path"><span className="dim">config.{parent}{parent ? "." : ""}</span>{leaf}</div>
      </div>
      {(!det || det.status === "inspecting") && <div className="cfgx-empty"><span className="cfgx-insp"><i/> inspecting option…</span></div>}
      {o && (
        <div className="cfgx-insp-body">
          <div className="cfgx-kv"><span>type</span><b className="mono cfgx-type">{o.type}</b></div>
          <div className="cfgx-block">
            <div className="cfgx-block-h">value</div>
            {o.evalError
              ? <pre className="cfgx-pre err">{o.evalError}</pre>
              : <pre className={`cfgx-pre v-${valKind(o.value)}`}>{o.value}</pre>}
          </div>
          {o.change && <div className="cfgx-kv"><span>changed</span><b>{o.change.kind}{o.change.reason ? ` — ${o.change.reason}` : ""}</b></div>}
          <div className="cfgx-kv"><span>definitions</span><b className="mono">{o.defCount}</b></div>
          <div className="cfgx-kv" title={o.mergeMode === "merge"
            ? "This option's type merges: every definition contributes to the result, so there is no single winning definition."
            : "This option's type is override-semantic: priority selects one definition and displaces the rest."}>
            <span>merge</span><b className={o.mergeMode === "merge" ? "cfgx-merge" : "cfgx-override"}>{o.mergeNote}</b>
          </div>

          <div className="cfgx-block">
            <div className="cfgx-block-h">provenance{!pv && <span className="cfgx-block-note">costs more than tree navigation</span>}</div>
            {!pv && <button className="cfgx-btn focus-ring" onClick={()=>onProv(sel)}>Inspect provenance</button>}
            {pv && pv.status === "inspecting" && <div className="cfgx-empty"><span className="cfgx-insp"><i/> inspecting provenance…</span></div>}
            {pv && pv.status === "succeeded" && (
              <div className="cfgx-defs">
                {pv.defs.map((d, i) => (
                  <div key={d.file} className={`cfgx-def${d.winning ? " win" : ""}`} role="button" tabIndex={0} onClick={()=>onModule(d)}>
                    <span className="cfgx-def-i mono">{i + 1}</span>
                    <div className="cfgx-def-b">
                      <div className={`mono cfgx-def-f${d.unknown ? " unknown" : ""}`} title={d.unknown ? "The module system recorded no _file for this definition — typically an inline or generated module. Real Nix reports «unknown-file» rather than inventing a path." : d.file}>{d.file}</div>
                      <div className="cfgx-def-m">{Badge && <Badge input={d.input} rev={d.rev} sys={sys} onOpenFlake={onOpenFlake}/>}<span>{d.note}</span></div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
          <div className="cfgx-insp-foot mono">observation cached · {rev} · schema v2{complete ? "" : " · partial inventory"}</div>
        </div>
      )}
    </aside>
  );
}

/* `waiting_for_capacity` is explicitly not running; `running` shows a live
   heartbeat rather than a percentage of an unknown corpus. */
function InventoryMeta({ inv, onRequest }) {
  const s = inv.state;
  if (s === "not_requested") return (
    <div className="cfgx-meta-i" title="Browsing needs no inventory. A complete inventory enables full-corpus search and Changed/Drift comparison. It is never scheduled just because you opened this tab.">
      <span>inventory</span><button className="cfgx-link" onClick={onRequest}>request complete</button>
    </div>
  );
  if (s === "queued") return <div className="cfgx-meta-i" title="Accepted; not yet assigned an evaluator."><span>inventory</span><b className="run"><i className="cfgx-hb"/>queued</b></div>;
  if (s === "waiting_for_capacity") return <div className="cfgx-meta-i" title="Authoritative build and policy evaluation take priority. This is NOT running — no evaluator is executing it and no transaction is held open while it waits."><span>inventory</span><b className="warn">waiting for capacity</b></div>;
  if (s === "running") return <div className="cfgx-meta-i" title="Live heartbeat. No percentage is shown: the corpus size is unknown until the crawl finishes."><span>inventory</span><b className="run"><i className="cfgx-hb live"/>{inv.phase.replace(/_/g, " ")}</b></div>;
  if (s === "succeeded") return <div className="cfgx-meta-i" title="Certified complete inventory for this revision."><span>inventory</span><b className="ok mono">{inv.optionCount.toLocaleString()} options</b></div>;
  return <div className="cfgx-meta-i"><span>inventory</span><b className="warn">failed</b></div>;
}

window.ConfigExplorerTab = ConfigExplorerTab;
