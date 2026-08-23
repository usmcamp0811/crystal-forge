// Compliance view — bundle catalog + per-system control evidence + export

function ComplianceView({ onOpenSystem, onOpenPolicy, selectedBundleId, selectedBundleView, onClearBundle, onClearBundleView, selectedFinding, onClearFinding }) {
  usePoamStore();
  const [bundleId, setBundleId] = React.useState(null);
  const [focusPolicy, setFocusPolicy] = React.useState(null);
  const [drawerOpen, setDrawerOpen] = React.useState(false);
  const [drawerView, setDrawerView] = React.useState("overview");
  const [policyDrawerId, setPolicyDrawerId] = React.useState(null);

  React.useEffect(() => {
    if (selectedBundleId) { setBundleId(selectedBundleId); setDrawerOpen(true); setDrawerView(selectedBundleView || "overview"); onClearBundle?.(); onClearBundleView?.(); }
  }, [selectedBundleId]);
  const [selectedSysId, setSelectedSysId] = React.useState(null);
  const [query, setQuery] = React.useState("");
  // Arriving from a POA&M: open the bundle, the host's evidence drawer, and that control.
  React.useEffect(() => {
    if (!selectedFinding) return;
    const b = COMPLIANCE_BUNDLES.find(x => x.id === selectedFinding.bundleId)
      || COMPLIANCE_BUNDLES.find(x => (x.policyIds || []).includes(selectedFinding.policyId));
    if (b) { setBundleId(b.id); setDrawerOpen(true); setDrawerView("overview"); setSelectedSysId(selectedFinding.sysId); setFocusPolicy(selectedFinding.policyId); }
    onClearFinding?.();
  }, [selectedFinding]);
  const [activeFw, setActiveFw] = React.useState("all");
  const [exportOpen, setExportOpen] = React.useState(false);
  const [newBundleOpen, setNewBundleOpen] = React.useState(false);
  const [importOpen, setImportOpen] = React.useState(false);
  const [importBundleOpen, setImportBundleOpen] = React.useState(false);
  const [editBundleOpen, setEditBundleOpen] = React.useState(false);
  const [filter, setFilter] = React.useState("all");
  const [importDraft, setImportDraft] = React.useState(() => { try { return JSON.parse(localStorage.getItem("cf-stig-import-draft") || "null"); } catch { return null; } });
  const checkImportDraft = () => setImportDraft(() => { try { return JSON.parse(localStorage.getItem("cf-stig-import-draft") || "null"); } catch { return null; } });

  const bundle = COMPLIANCE_BUNDLES.find(b => b.id === bundleId);

  const applicableSystems = React.useMemo(() => {
    if (!bundle) return [];
    return SYSTEMS.map(s => ({ sys: s, rollup: bundleStatusForSystem(bundle, s) })).filter(({ rollup }) => rollup.applies);
  }, [bundleId]);

  const stats = React.useMemo(() => {
    const totals = { pass:0, warn:0, fail:0, waiver:0, totalControls:0 };
    applicableSystems.forEach(({ rollup }) => {
      if (!rollup.applies) return;
      totals.pass += rollup.pass;
      totals.warn += rollup.warn;
      totals.fail += rollup.fail;
      totals.waiver += rollup.waiver;
      totals.totalControls += rollup.total;
    });
    const compliantHosts = applicableSystems.filter(s => s.rollup.applies && s.rollup.fail === 0).length;
    return {
      ...totals,
      compliantHosts,
      totalHosts: applicableSystems.length,
      overallScore: totals.totalControls ? Math.round(((totals.pass + totals.waiver) / totals.totalControls) * 100) : 0,
    };
  }, [bundle, applicableSystems]);

  const filteredSystems = applicableSystems.filter(({ sys, rollup }) => {
    if (filter === "all") return true;
    if (filter === "fail") return rollup.fail > 0;
    if (filter === "warn") return rollup.warn > 0 && rollup.fail === 0;
    if (filter === "clean") return rollup.fail === 0 && rollup.warn === 0;
    const ps = bundle ? systemBundlePoams(bundle, sys.id).filter(p => p.status !== "completed") : [];
    if (filter === "poam") return ps.length > 0;
    if (filter === "nopoam") return rollup.fail > 0 && ps.length === 0;
    if (filter === "overdue") return ps.some(poamIsOverdue);
    return true;
  });

  // For drill-in
  const drillSys = SYSTEMS.find(s => s.id === selectedSysId);

  return (
    <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">Compliance</h1>
          <p className="page-subtitle">
            Walk through compliance bundles, review per-control evidence, export for auditors.
          </p>
        </div>
        <div style={{ display:"flex", gap:8 }}>
          <IOMenu items={[
            { label: importDraft ? "Resume STIG import…" : "Import STIG (.xml)", icon:"shield", onClick:() => setImportOpen(true) },
            { label:"Import bundle (.xml / DISA .zip)", icon:"upload", onClick:() => setImportBundleOpen(true) },
            "divider",
            { label:"Export this bundle (XCCDF .xml)", icon:"download", onClick:() => bundle && exportBundle(bundle) },
            { label:"Export evidence report…", icon:"download", onClick:() => setExportOpen(true) },
          ]}/>
          <button className="btn btn-primary focus-ring" data-coach-target="bundle" onClick={() => setNewBundleOpen(true)}>
            <Icon name="plus" size={14}/> New bundle
          </button>
        </div>
      </div>

      {importDraft && !importOpen && (
        <div className="sd-callout sd-callout-warn" style={{ justifyContent:"space-between" }}>
          <div style={{ display:"flex", alignItems:"center", gap:10 }}>
            <Icon name="shield" size={14}/>
            <div style={{ fontSize:12.5 }}>Paused STIG import — <strong>{importDraft.parsed?.title || importDraft.bundleName || "unnamed benchmark"}</strong>, {importDraft.parsed ? `${importDraft.parsed.rules.filter(r=>r.selected).length} of ${importDraft.parsed.rules.length} controls selected` : "in progress"}.</div>
          </div>
          <div style={{ display:"flex", gap:6, flexShrink:0 }}>
            <button className="btn btn-ghost focus-ring xs" onClick={() => { localStorage.removeItem("cf-stig-import-draft"); setImportDraft(null); }}>Discard</button>
            <button className="btn btn-primary focus-ring xs" onClick={() => setImportOpen(true)}>Resume</button>
          </div>
        </div>
      )}

      {/* Dense bundle catalog — every bundle as one scannable row; click to open the detail drawer */}
      <BundleListTable bundles={COMPLIANCE_BUNDLES} query={query} setQuery={setQuery} activeFw={activeFw} setActiveFw={setActiveFw}
        selectedId={bundleId}
        onSelect={(id) => { setBundleId(id); setDrawerOpen(true); setDrawerView("overview"); setSelectedSysId(null); }}/>

      {bundle && drawerOpen && !policyDrawerId && (
        <BundleDetailDrawer
          bundle={bundle}
          stats={stats}
          filter={filter}
          setFilter={setFilter}
          applicableSystems={filteredSystems}
          onClose={() => setDrawerOpen(false)}
          onEdit={() => setEditBundleOpen(true)}
          onSelectRevision={(id) => { setBundleId(id); setSelectedSysId(null); }}
          onOpenSystem={(s) => setSelectedSysId(s.id)}
          onOpenPolicy={setPolicyDrawerId}
          view={drawerView}
          setView={setDrawerView}
        />
      )}

      {drillSys && bundle && (
        <ControlsEvidenceDrawer
          bundle={bundle}
          sys={drillSys}
          focusPolicyId={focusPolicy}
          onClose={() => { setSelectedSysId(null); setFocusPolicy(null); }}
          onOpenSystem={onOpenSystem}
        />
      )}
      {policyDrawerId && (() => { const pol = POLICIES.find(p => p.id === policyDrawerId); return pol && (
        <PolicyDrawer
          policy={pol}
          onClose={() => setPolicyDrawerId(null)}
          onOpenSystem={onOpenSystem}
          onSwitchPolicy={setPolicyDrawerId}
        />
      ); })()}
      {exportOpen && bundle && (
        <ExportEvidenceModal bundle={bundle} stats={stats} onClose={() => setExportOpen(false)}/>
      )}
      {newBundleOpen && (
        <NewBundleModal onClose={() => setNewBundleOpen(false)}/>
      )}
      {importOpen && (
        <ImportStigModal
          onClose={() => { setImportOpen(false); checkImportDraft(); }}
          onComplete={(id) => { setImportOpen(false); checkImportDraft(); setBundleId(id); setDrawerOpen(true); setSelectedSysId(null); }}
        />
      )}
      {importBundleOpen && (
        <ImportBundleModal
          onClose={() => setImportBundleOpen(false)}
          onComplete={(id) => { setImportBundleOpen(false); setBundleId(id); setDrawerOpen(true); setSelectedSysId(null); }}
        />
      )}
      {editBundleOpen && bundle && (
        <NewBundleModal
          bundle={bundle}
          onClose={() => setEditBundleOpen(false)}
          onDelete={() => {
            const idx = COMPLIANCE_BUNDLES.findIndex(b => b.id === bundle.id);
            if (idx >= 0) COMPLIANCE_BUNDLES.splice(idx, 1);
            setDrawerOpen(false);
            setBundleId(null);
            setSelectedSysId(null);
          }}
        />
      )}
    </div>
  );
}

/* ── Left rail: bundle catalog, grouped by lineage ── */
const PUB_STATE_COLOR = { current:"#34d399", accepted:"#60a5fa", deprecated:"#6b7280", draft:"#fbbf24" };
function PubStateChip({ state }) {
  const c = PUB_STATE_COLOR[state] || "#6b7280";
  return <span className="chip" style={{ fontSize:9, padding:"1px 6px", color:c, background:`color-mix(in oklab, ${c} 16%, transparent)` }}>{state}</span>;
}

function scoreDotColor(score) {
  if (score == null) return "var(--cf-text-muted)";
  if (score >= 90) return "#34d399";
  if (score >= 70) return "#fbbf24";
  return "#f87171";
}

function BundleListTable({ bundles, query, setQuery, activeFw, setActiveFw, selectedId, onSelect }) {
  const [pickerFor, setPickerFor] = React.useState(null);

  const frameworks = React.useMemo(() => {
    const counts = new Map();
    bundles.forEach(b => counts.set(b.framework, (counts.get(b.framework) || 0) + 1));
    return Array.from(counts.entries()).sort((a,b) => b[1]-a[1]);
  }, [bundles]);

  const allGroups = React.useMemo(() => groupBundlesByLineage(bundles), [bundles]);
  const q = query.trim().toLowerCase();
  const groups = allGroups.filter(g => {
    if (activeFw !== "all" && g.current.framework !== activeFw) return false;
    if (!q) return true;
    return g.lineageName.toLowerCase().includes(q) || g.current.framework.toLowerCase().includes(q) || g.current.version.toLowerCase().includes(q);
  });

  return (
    <div className="card" style={{ overflow:"hidden" }}>
      <div style={{ padding:"10px 16px", borderBottom:"1px solid var(--cf-card-border)", display:"flex", flexDirection:"column", gap:10 }}>
        <div style={{ display:"flex", gap:6, flexWrap:"nowrap", overflowX:"auto" }}>
          <button className={`cf-fw-chip${activeFw === "all" ? " active" : ""}`} onClick={() => setActiveFw("all")}>All <span>{bundles.length}</span></button>
          {frameworks.map(([fw, count]) => (
            <button key={fw} className={`cf-fw-chip${activeFw === fw ? " active" : ""}`} onClick={() => setActiveFw(fw)}>{fw} <span>{count}</span></button>
          ))}
        </div>
        <div className="q-search" style={{ marginLeft:0, width:"100%", boxSizing:"border-box" }}>
          <Icon name="search" size={13}/>
          <input className="q-search-input" placeholder="Search bundles…" value={query} onChange={e => setQuery(e.target.value)} style={{ flex:1, width:"auto" }}/>
          {query && <span className="q-search-count">{groups.length} of {bundles.length}</span>}
          {query && <button className="btn-icon xs focus-ring" title="Clear search" onClick={()=>setQuery("")}><Icon name="x" size={13}/></button>}
        </div>
      </div>
      {groups.length === 0 ? (
        <div className="q-empty"><Icon name="search" size={20}/><div>No bundles match “{query}”.</div></div>
      ) : (
      <table className="sys-table sys-table-fixed">
        <colgroup>
          <col style={{ width:"38%" }}/><col style={{ width:"16%" }}/><col style={{ width:"18%" }}/>
          <col style={{ width:"18%" }}/><col style={{ width:"10%" }}/>
        </colgroup>
        <thead>
          <tr>
            <th>Bundle</th>
            <th>Framework</th>
            <th>Version</th>
            <th>Score</th>
            <th style={{ textAlign:"right" }}> </th>
          </tr>
        </thead>
        <tbody>
          {groups.map(g => {
            const shown = g.current;
            const multi = g.revisions.length > 1;
            const quick = bundleQuickStats(shown);
            const isSelected = g.revisions.some(r => r.id === selectedId);
            return (
              <tr key={g.lineageId} className={isSelected ? "selected" : ""} onClick={() => onSelect(shown.id)}>
                <td>
                  <div style={{ display:"flex", alignItems:"center", gap:8, minWidth:0 }}>
                    <span style={{ width:7, height:7, borderRadius:"50%", flexShrink:0, background:scoreDotColor(quick.score) }}/>
                    <span style={{ fontWeight:600, fontSize:13, whiteSpace:"nowrap", overflow:"hidden", textOverflow:"ellipsis", minWidth:0 }}>{g.lineageName}</span>
                  </div>
                  <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>{shown.policyIds.length} controls{multi ? ` · ${g.revisions.length} revisions` : ""}</div>
                </td>
                <td><span className="chip chip-info">{shown.framework}</span></td>
                <td>
                  <div className="mono" style={{ fontSize:12 }}>{shown.version}</div>
                  <div style={{ marginTop:3 }}><PubStateChip state={shown.publicationState}/></div>
                </td>
                <td>
                  <span className="mono" style={{ fontSize:13, fontWeight:600, color:scoreDotColor(quick.score) }}>{quick.score != null ? `${quick.score}%` : "—"}</span>
                  <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>{quick.systemCount} system{quick.systemCount === 1 ? "" : "s"}</div>
                </td>
                <td onClick={e=>e.stopPropagation()} style={{ textAlign:"right" }}>
                  <div className="row-actions" style={{ opacity:1, justifyContent:"flex-end" }}>
                    <button className="btn-icon focus-ring" title="View bundle" onClick={() => onSelect(shown.id)}>
                      <Icon name="arrow-right" size={14}/>
                    </button>
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      )}
      {pickerFor && (
        <RevisionPickerModal
          title={pickerFor.lineageName}
          revisions={pickerFor.revisions}
          currentId={pickerFor.current.id}
          selectedId={selectedId}
          onSelect={(id) => { onSelect(id); setPickerFor(null); }}
          onClose={() => setPickerFor(null)}
        />
      )}
    </div>
  );
}

function BundleDetailDrawer({ bundle, stats, filter, setFilter, applicableSystems, onClose, onEdit, onSelectRevision, onOpenSystem, onOpenPolicy, view, setView }) {
  const lineage = React.useMemo(() => groupBundlesByLineage(COMPLIANCE_BUNDLES).find(g => g.revisions.some(r => r.id === bundle.id)), [bundle.id]);
  const [revisionsOpen, setRevisionsOpen] = React.useState(false);
  const coverage = typeof bundleRequirementCoverage === "function" ? bundleRequirementCoverage(bundle) : null;

  return (
    <>
      <div className="fl-tray-backdrop" onClick={onClose}/>
      <aside className="fl-tray" style={{ width:"min(900px, 96vw)" }}>
        <header className="fl-tray-head">
          {view === "coverage" ? (
            <div style={{ display:"flex", alignItems:"center", gap:10, minWidth:0, flex:1 }}>
              <button className="btn-icon focus-ring" onClick={()=>setView("overview")}><Icon name="arrow-left" size={16}/></button>
              <div style={{ minWidth:0 }}>
                <span style={{ fontWeight:700, fontSize:15 }}>Requirement coverage</span>
                <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>{bundle.name}</div>
              </div>
            </div>
          ) : view === "poam" ? (
            <div style={{ display:"flex", alignItems:"center", gap:10, minWidth:0, flex:1 }}>
              <button className="btn-icon focus-ring" onClick={()=>setView("overview")}><Icon name="arrow-left" size={16}/></button>
              <div style={{ minWidth:0 }}>
                <span style={{ fontWeight:700, fontSize:15 }}>POA&amp;M items</span>
                <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>{bundle.name}</div>
              </div>
            </div>
          ) : (
            <div style={{ display:"flex", alignItems:"center", gap:12, minWidth:0, flex:1 }}>
              <Icon name="shield" size={18} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
              <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>Compliance bundle</span>
            </div>
          )}
          <div style={{ display:"flex", gap:6 }}>
            {view === "overview" && <button className="btn btn-ghost focus-ring xs" onClick={onEdit}><Icon name="edit" size={12}/> Edit bundle</button>}
            <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16}/></button>
          </div>
        </header>

        {view === "coverage" ? (
          <RequirementCoverageBody coverage={coverage} onOpenPolicy={onOpenPolicy}/>
        ) : view === "poam" ? (
          <BundlePoamBody bundle={bundle}/>
        ) : (
        <div style={{ overflow:"auto", flex:1 }}>
          <div style={{ padding:"14px 18px" }}>
            <BundleHeader bundle={bundle} stats={stats} onEdit={onEdit}/>
          </div>

          <div className="stat-strip stat-strip-flush" style={{ borderTop:"1px solid var(--cf-divider)" }}>
            <div className="stat">
              <div className="stat-label">Overall score</div>
              <div className="stat-value" style={{ color: stats.overallScore >= 90 ? "#34d399" : stats.overallScore >= 70 ? "#fbbf24" : "#f87171" }}>{stats.overallScore}%</div>
            </div>
            {[
              { label:"Pass",   val:stats.pass,   color:"#34d399" },
              { label:"Warn",   val:stats.warn,   color:"#fbbf24" },
              { label:"Fail",   val:stats.fail,   color:"#f87171" },
              { label:"Waiver", val:stats.waiver, color:"#a78bfa" },
            ].map(s => (
              <div key={s.label} className="stat">
                <div className="stat-label">{s.label}</div>
                <div className="stat-value" style={{ color: s.color }}>{s.val}</div>
              </div>
            ))}
          </div>

          {lineage && lineage.revisions.length > 1 && (
            <div style={{ borderTop:"1px solid var(--cf-divider)", padding:"12px 18px" }}>
              <button className="focus-ring" onClick={()=>setRevisionsOpen(o=>!o)} style={{ all:"unset", cursor:"pointer", display:"flex", alignItems:"center", gap:8, width:"100%" }}>
                <Icon name={revisionsOpen?"chevron-down":"chevron-right"} size={13}/>
                <span style={{ fontSize:13, fontWeight:600 }}>Revisions</span>
                <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{lineage.revisions.length} total</span>
              </button>
              {revisionsOpen && (
              <div style={{ display:"flex", gap:8, flexWrap:"wrap", marginTop:12 }}>
                {lineage.revisions.map(r => {
                  const isSel = r.id === bundle.id;
                  return (
                    <button key={r.id} onClick={() => onSelectRevision(r.id)} className="focus-ring"
                      style={{
                        all:"unset", cursor:"pointer", display:"flex", flexDirection:"column", gap:3,
                        padding:"7px 10px", borderRadius:8,
                        background: isSel ? "color-mix(in oklab,var(--cf-brand-purple) 12%, transparent)" : "var(--cf-subtle-bg)",
                        border: `1px solid ${isSel ? "var(--cf-brand-purple)" : "transparent"}`,
                      }}>
                      <div style={{ display:"flex", alignItems:"center", gap:6 }}>
                        <span className="mono" style={{ fontSize:11.5, fontWeight:600 }}>Rev {r.revision} · {r.version}</span>
                        {r.id === lineage.current.id && <span className="chip" style={{ fontSize:8.5, color:"#34d399", background:"color-mix(in oklab, #34d399 16%, transparent)" }}>Current</span>}
                      </div>
                      <div style={{ display:"flex", alignItems:"center", gap:6 }}>
                        <PubStateChip state={r.publicationState}/>
                        <span style={{ fontSize:10, color:"var(--cf-text-muted)" }}>{r.publishedDate}</span>
                      </div>
                    </button>
                  );
                })}
              </div>
              )}
            </div>
          )}

          <div style={{ borderTop:"1px solid var(--cf-divider)" }}>
            <RequirementCoverageCard coverage={coverage} onOpenCoverage={()=>setView("coverage")}/>
          </div>

          <div style={{ borderTop:"1px solid var(--cf-divider)" }}>
            <BundlePoamRollup bundle={bundle} failCount={stats.fail} onOpenList={()=>setView("poam")}/>
          </div>

          <div style={{ borderTop:"1px solid var(--cf-divider)" }}>
            <BundleDrilldown
              bundle={bundle}
              filter={filter}
              setFilter={setFilter}
              applicableSystems={applicableSystems}
              onOpenSystem={onOpenSystem}
            />
          </div>
        </div>
        )}
      </aside>
    </>
  );
}

function RequirementCoverageCard({ coverage, onOpenCoverage }) {
  if (!coverage) return null;
  const { framework, total, full, partial, unmapped } = coverage;
  if (total === 0) {
    return (
      <div style={{ padding:16, fontSize:12, color:"var(--cf-text-muted)" }}>
        <strong style={{ color:"var(--cf-text-primary)", fontWeight:600, fontSize:13 }}>Requirement coverage</strong>
        <div style={{ marginTop:6 }}>No requirement catalog modeled for {framework.name} yet.</div>
      </div>
    );
  }
  return (
    <div style={{ padding:16 }}>
      <button className="focus-ring" onClick={onOpenCoverage} style={{ all:"unset", cursor:"pointer", display:"flex", width:"100%", alignItems:"center", justifyContent:"space-between", gap:10 }}>
        <div style={{ display:"flex", alignItems:"center", gap:10 }}>
          <span style={{ fontSize:13, fontWeight:600 }}>Requirement coverage</span>
          <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{framework.name} · {total} requirements · derived from mapped policies, not policy tags</span>
        </div>
        <div style={{ display:"flex", gap:6, flexShrink:0, alignItems:"center" }}>
          <span className="chip" style={{ fontSize:9.5, color:"#34d399", background:"color-mix(in oklab, #34d399 16%, transparent)" }}>{full} full</span>
          <span className="chip" style={{ fontSize:9.5, color:"#fbbf24", background:"color-mix(in oklab, #fbbf24 16%, transparent)" }}>{partial} partial</span>
          <span className="chip chip-unknown" style={{ fontSize:9.5 }}>{unmapped} unmapped</span>
          <Icon name="chevron-right" size={13} style={{ color:"var(--cf-text-muted)" }}/>
        </div>
      </button>
    </div>
  );
}

function RequirementCoverageBody({ coverage, onOpenPolicy }) {
  const { total, full, partial, unmapped, rows } = coverage;
  const [query, setQuery] = React.useState("");
  const [statusFilter, setStatusFilter] = React.useState("all");
  const q = query.trim().toLowerCase();
  const filteredRows = rows.filter(r => {
    if (statusFilter !== "all" && r.status !== statusFilter) return false;
    if (!q) return true;
    return r.requirement.externalId.toLowerCase().includes(q) || r.requirement.title.toLowerCase().includes(q);
  });
  const byParent = new Map();
  filteredRows.forEach(r => {
    const top = reqBreadcrumb(r.requirement.id)[0];
    const key = top?.id || "other";
    if (!byParent.has(key)) byParent.set(key, { top, items: [] });
    byParent.get(key).items.push(r);
  });
  const statusColor = { full:"#34d399", partial:"#fbbf24", unmapped:"#6b7280" };

  return (
    <div style={{ display:"flex", flexDirection:"column", flex:1, minHeight:0 }}>
      <div style={{ padding:"12px 18px", borderBottom:"1px solid var(--cf-divider)", display:"flex", gap:10, alignItems:"center", flexWrap:"wrap" }}>
        <div className="seg">
          {[
            { v:"all",      l:`All ${total}` },
            { v:"full",     l:`Full ${full}` },
            { v:"partial",  l:`Partial ${partial}` },
            { v:"unmapped", l:`Unmapped ${unmapped}` },
          ].map(o => (
            <button key={o.v} className={statusFilter === o.v ? "active" : ""} onClick={() => setStatusFilter(o.v)}>{o.l}</button>
          ))}
        </div>
        <div className="q-search" style={{ marginLeft:"auto" }}>
          <Icon name="search" size={13}/>
          <input className="q-search-input" placeholder="Filter requirements…" value={query} onChange={e=>setQuery(e.target.value)}/>
          {query && <button className="btn-icon xs focus-ring" title="Clear" onClick={()=>setQuery("")}><Icon name="x" size={13}/></button>}
        </div>
      </div>

      <div style={{ overflow:"auto", flex:1, padding:"14px 18px", display:"flex", flexDirection:"column", gap:16 }}>
        {Array.from(byParent.values()).map(grp => (
          <div key={grp.top?.id || "other"}>
            {!(grp.items.length === 1 && grp.top?.id === grp.items[0].requirement.id) && (
              <div style={{ fontSize:11.5, fontWeight:700, marginBottom:6 }}>{grp.top ? `${grp.top.externalId} — ${grp.top.title}` : "Other"}</div>
            )}
            <div style={{ display:"flex", flexDirection:"column", gap:4 }}>
              {grp.items.map(({ requirement, mappings, status }) => (
                <div key={requirement.id} style={{ display:"flex", justifyContent:"space-between", gap:10, padding:"6px 9px", background:"var(--cf-subtle-bg)", borderRadius:7 }}>
                  <div style={{ minWidth:0 }}>
                    <span className="mono" style={{ fontSize:11.5, fontWeight:600, whiteSpace:"nowrap", flexShrink:0 }}>{requirement.externalId}</span>
                    <span style={{ fontSize:11, color:"var(--cf-text-secondary)", marginLeft:6 }}>{requirement.title}</span>
                    {mappings.length > 0 && (
                      <div style={{ marginTop:4, display:"flex", gap:6, alignItems:"center", flexWrap:"wrap" }}>
                        <span style={{ fontSize:9.5, fontWeight:600, color:"var(--cf-text-muted)", textTransform:"uppercase", letterSpacing:".03em" }}>Enforced by</span>
                        {mappings.map(m => {
                          const pol = POLICIES.find(p=>p.id===m.policyId);
                          if (!pol) return null;
                          return (
                            <button key={m.policyId} className="focus-ring cf-policy-link" onClick={()=>onOpenPolicy?.(pol.id)}>
                              <Icon name="file" size={10}/>
                              {pol.name}
                              <Icon name="arrow-right" size={10}/>
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                  <span className="chip" style={{ fontSize:9, flexShrink:0, height:"fit-content", color:statusColor[status], background:`color-mix(in oklab, ${statusColor[status]} 16%, transparent)` }}>
                    {status === "full" ? "Fully covered" : status === "partial" ? "Partially covered" : "Unmapped"}
                  </span>
                </div>
              ))}
            </div>
          </div>
        ))}
        {filteredRows.length === 0 && <div style={{ fontSize:12, color:"var(--cf-text-muted)", textAlign:"center", padding:"24px 0" }}>No requirements match.</div>}
      </div>
    </div>
  );
}

/* ── Bundle header ── */
function BundleHeader({ bundle, stats, onEdit }) {
  return (
    <div style={{ display:"flex", flexDirection:"column", gap:10 }}>
      <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:14, flexWrap:"wrap" }}>
        <div>
          <h2 style={{ margin:0, fontSize:18, fontWeight:700 }}>{bundle.name}</h2>
          <div style={{ display:"flex", gap:8, marginTop:6, alignItems:"center", flexWrap:"wrap" }}>
            <span className="chip chip-info">{bundle.framework}</span>
            <span className="chip chip-unknown">{bundle.version}</span>
            <span className="chip chip-unknown">{bundle.layer}</span>
            <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>Owned by <span className="mono">{bundle.owner}</span> · Last reviewed {bundle.lastReview}</span>
          </div>
        </div>
        <div style={{ display:"flex", gap:10, alignItems:"center", flexWrap:"wrap" }}>
          <div style={{ display:"flex", gap:6, flexWrap:"wrap" }}>
            {bundle.requiredEnvs.map(env => <EnvBadge key={env} env={env}/>)}
          </div>
          <button className="btn btn-ghost focus-ring" onClick={onEdit}><Icon name="edit" size={13}/> Edit bundle</button>
        </div>
      </div>
      <p style={{ margin:0, fontSize:13, color:"var(--cf-text-secondary)", lineHeight:1.5 }}>{bundle.description}</p>
    </div>
  );
}

/* ── Controls list + systems matrix ── */
function BundleDrilldown({ bundle, filter, setFilter, applicableSystems, onOpenSystem }) {
  const [envFilter, setEnvFilter] = React.useState("all");
  const envs = React.useMemo(() => [...new Set(applicableSystems.map(({ sys }) => sys.environment))].sort(), [applicableSystems]);
  const envScoped = envFilter === "all" ? applicableSystems : applicableSystems.filter(({ sys }) => sys.environment === envFilter);
  return (
    <div>
      <div style={{ padding:"12px 16px", borderBottom:"1px solid var(--cf-divider)", display:"flex", gap:10, alignItems:"center", flexWrap:"wrap" }}>
        <h3 style={{ margin:0, fontSize:13, fontWeight:600 }}>Systems</h3>
        <div className="seg">
          {[
            { v:"all",   l:"All" },
            { v:"clean", l:"Clean" },
            { v:"warn",  l:"Warning" },
            { v:"fail",  l:"Failing" },
            { v:"poam",  l:"On POA&M" },
            { v:"nopoam", l:"No POA&M" },
            { v:"overdue", l:"Overdue" },
          ].map(o => (
            <button key={o.v} className={filter === o.v ? "active" : ""} onClick={() => setFilter(o.v)}>{o.l}</button>
          ))}
        </div>
        <span className="filter-count">{envScoped.length} host{envScoped.length===1?"":"s"}</span>
        {envs.length > 1 && (
          <select className="input focus-ring" value={envFilter} onChange={e=>setEnvFilter(e.target.value)} style={{ marginLeft:"auto", width:"auto", fontSize:12, padding:"5px 8px" }}>
            <option value="all">All environments</option>
            {envs.map(e => <option key={e} value={e}>{e}</option>)}
          </select>
        )}
      </div>
      <div className="sd-callout sd-callout-info" style={{ margin:"12px 16px 0" }}>
        <Icon name="shield" size={13}/>
        <div style={{ fontSize:12 }}>Select a host to step through its <strong>per-control evidence</strong> — the proof Crystal Forge collected that each control is satisfied.</div>
      </div>
      {bundle.publicationState !== "current" && envScoped.length > 0 && (
        <div className="sd-callout sd-callout-warn" style={{ margin:"10px 16px 0" }}>
          <Icon name="warn" size={13}/>
          <div style={{ fontSize:12 }}>These {envScoped.length} host{envScoped.length===1?"":"s"} are explicitly pinned to this <strong>{bundle.publicationState}</strong> revision rather than tracking current — see each host's assignment reason below.</div>
        </div>
      )}
      <table className="sys-table compact sys-table-dense">
        <colgroup>
          <col style={{ width:"20%" }}/><col style={{ width:82 }}/><col style={{ width:110 }}/><col style={{ width:104 }}/>
          <col style={{ width:54 }}/><col style={{ width:58 }}/><col style={{ width:54 }}/><col style={{ width:64 }}/><col style={{ width:132 }}/><col style={{ width:44 }}/>
        </colgroup>
        <thead>
          <tr>
            <th>Host</th>
            <th>Env</th>
            <th>Assignment</th>
            <th>Score</th>
            <th style={{ textAlign:"right" }}>Pass</th>
            <th style={{ textAlign:"right" }}>Warn</th>
            <th style={{ textAlign:"right" }}>Fail</th>
            <th style={{ textAlign:"right" }}>Waiver</th>
            <th>POA&M</th>
            <th style={{ textAlign:"right" }}> </th>
          </tr>
        </thead>
        <tbody>
          {envScoped.map(({ sys, rollup }) => (
            <tr key={sys.id} style={{ cursor:"pointer" }} onClick={() => onOpenSystem(sys)}>
              <td>
                <div style={{ display:"flex", alignItems:"center", gap:8 }}>
                  <span className="status-dot" style={{ "--status-color": sys.statusColor }}/>
                  <span className="mono" style={{ fontWeight:600, fontSize:13 }}>{sys.hostname}</span>
                </div>
              </td>
              <td><EnvBadge env={sys.environment}/></td>
              <td>
                {(() => {
                  const st = rollup.assignment?.status || "current";
                  const meta = COMPLIANCE_ASSIGNMENT_STATUS[st] || COMPLIANCE_ASSIGNMENT_STATUS.current;
                  return <span className="chip" title={rollup.assignment?.reason || "Tracking current baseline"} style={{ fontSize:9, color:meta.color, background:`color-mix(in oklab, ${meta.color} 14%, transparent)` }}>{meta.label}</span>;
                })()}
              </td>
              <td>
                <div style={{ display:"flex", alignItems:"center", gap:8 }}>
                  <div style={{ width:40, height:5, background:"var(--cf-subtle-bg)", borderRadius:99, overflow:"hidden", flexShrink:0 }}>
                    <div style={{ width:`${rollup.score}%`, height:"100%", background: rollup.score >= 90 ? "#34d399" : rollup.score >= 70 ? "#fbbf24" : "#f87171" }}/>
                  </div>
                  <span className="mono" style={{ fontSize:12, fontWeight:600, color: rollup.score >= 90 ? "#34d399" : rollup.score >= 70 ? "#fbbf24" : "#f87171" }}>{rollup.score}%</span>
                </div>
              </td>
              <td className="mono" style={{ textAlign:"right", color:"#34d399", fontWeight:600 }}>{rollup.pass}</td>
              <td className="mono" style={{ textAlign:"right", color: rollup.warn > 0 ? "#fbbf24" : "var(--cf-text-muted)", fontWeight: rollup.warn > 0 ? 600 : 400 }}>{rollup.warn}</td>
              <td className="mono" style={{ textAlign:"right", color: rollup.fail > 0 ? "#f87171" : "var(--cf-text-muted)", fontWeight: rollup.fail > 0 ? 700 : 400 }}>{rollup.fail}</td>
              <td className="mono" style={{ textAlign:"right", color: rollup.waiver > 0 ? "#a78bfa" : "var(--cf-text-muted)" }}>{rollup.waiver}</td>
              <td onClick={e=>e.stopPropagation()}>
                {(() => {
                  const ps = systemBundlePoams(bundle, sys.id).filter(p => p.status !== "completed");
                  if (ps.length === 0) {
                    if (rollup.fail === 0) return <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>—</span>;
                    return (
                      <button className="poam-tag none focus-ring" style={{ cursor:"pointer" }} title="Open this host's evidence and create a POA&M from the failing control"
                        onClick={() => onOpenSystem(sys)}>+ POA&M</button>
                    );
                  }
                  const overdue = ps.some(poamIsOverdue);
                  return (
                    <button className={`poam-tag${overdue ? " overdue" : ""} focus-ring`} style={{ cursor:"pointer" }} onClick={() => openPoamDetail(ps[0].id)}
                      title={ps.map(p => `${p.id} · ${POAM_STATUS[p.status].label}`).join("\n")}>
                      {ps[0].id}{ps.length > 1 ? ` +${ps.length - 1}` : ""}{overdue ? " ⚠" : ""}
                    </button>
                  );
                })()}
              </td>
              <td onClick={e=>e.stopPropagation()} style={{ textAlign:"right" }}>
                <button className="btn-icon focus-ring" title="View evidence" onClick={() => onOpenSystem(sys)}>
                  <Icon name="arrow-right" size={14}/>
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/* ── Drawer: walk through controls for a host ── */
function ControlsEvidenceDrawer({ bundle, sys, onClose, onOpenSystem, showSystemLink, onOpenBundle, focusPolicyId }) {
  usePoamStore();
  const [activeIdx, setActiveIdx] = React.useState(0);
  const assignment = resolveComplianceAssignment(sys, bundle.lineageId || bundle.id);
  const evidenceList = bundle.policyIds.map(pid => evidenceForControl(bundle, pid, sys));
  const active = evidenceList[activeIdx];

  // Framework-aware grouping: different compliance frameworks organize controls
  // differently, so pick the scheme that matches this bundle instead of always NIST family.
  const frameworkScheme = (() => {
    const f = (bundle.framework || "").toLowerCase();
    if (f.includes("cmmc")) return "cmmc-level";
    if (f.includes("cis")) return "cis-section";
    if (f.includes("stig")) return "severity";
    if (f.includes("800-53") || f.includes("nist")) return "control-family";
    return "control-family";
  })();
  const familyOrder = [...Object.keys(typeof CONTROL_FAMILIES !== "undefined" ? CONTROL_FAMILIES : {}), "ungrouped"];
  const navGroups = frameworkScheme === "severity" ? (() => {
    const order = [["high","CAT I — High"],["medium","CAT II — Medium"],["low","CAT III — Low"],["unrated","Unrated"]];
    return order.map(([sid,label]) => ({ fid:sid, label, indices: bundle.policyIds.map((pid,i)=>i).filter(i => (POLICIES.find(x=>x.id===bundle.policyIds[i])?.severity || "unrated") === sid) })).filter(g=>g.indices.length>0);
  })() : frameworkScheme === "cmmc-level" ? (() => {
    const order = [["l3","Level 3"],["l2","Level 2"],["l1","Level 1"],["unrated","Unrated"]];
    return order.map(([sid,label]) => ({ fid:sid, label, indices: bundle.policyIds.map((pid,i)=>i).filter(i => { const p = POLICIES.find(x=>x.id===bundle.policyIds[i]); return p && cmmcLevelOf(p).id === sid; }) })).filter(g=>g.indices.length>0);
  })() : frameworkScheme === "cis-section" ? (() => {
    const bySection = new Map();
    bundle.policyIds.forEach((pid, i) => {
      const p = POLICIES.find(x=>x.id===pid);
      const key = p?.cisSection ? `Section ${p.cisSection.split(".")[0]}` : "Unmapped";
      if (!bySection.has(key)) bySection.set(key, []);
      bySection.get(key).push(i);
    });
    return Array.from(bySection.entries()).map(([label,indices]) => ({ fid:label, label, indices })).sort((a,b)=>a.label.localeCompare(b.label));
  })() : familyOrder.map(fid => {
    const fam = (typeof CONTROL_FAMILIES !== "undefined" ? CONTROL_FAMILIES : {})[fid];
    const indices = bundle.policyIds.map((pid, i) => i).filter(i => {
      const p = POLICIES.find(x => x.id === bundle.policyIds[i]);
      return (p?.controlFamily || "ungrouped") === fid;
    });
    return { fid, label: fam ? `${fam.id} — ${fam.label}` : "Ungrouped", indices };
  }).filter(g => g.indices.length > 0);
  const visualOrder = navGroups.flatMap(g => g.indices);
  const [collapsed, setCollapsed] = React.useState({});
  const toggleGroup = (fid) => setCollapsed(c => ({ ...c, [fid]: !c[fid] }));
  const [navQuery, setNavQuery] = React.useState("");
  const navQ = navQuery.trim().toLowerCase();
  const navGroupsFiltered = navQ ? navGroups.map(g => ({
    ...g,
    indices: g.indices.filter(i => {
      const ev = evidenceList[i];
      return ev.policyName.toLowerCase().includes(navQ) || ev.status.toLowerCase().includes(navQ);
    }),
  })).filter(g => g.indices.length > 0) : navGroups;

  React.useEffect(() => {
    if (!focusPolicyId) return;
    const i = bundle.policyIds.indexOf(focusPolicyId);
    if (i >= 0) setActiveIdx(i);
  }, [focusPolicyId, bundle.id]);

  React.useEffect(() => {
    const onKey = (e) => {
      if (e.key === "Escape") onClose();
      const pos = Math.max(0, visualOrder.indexOf(activeIdx));
      if (e.key === "j" || e.key === "ArrowDown") { e.preventDefault(); setActiveIdx(visualOrder[Math.min(visualOrder.length - 1, pos + 1)]); }
      if (e.key === "k" || e.key === "ArrowUp")   { e.preventDefault(); setActiveIdx(visualOrder[Math.max(0, pos - 1)]); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [bundle.policyIds.length, onClose, activeIdx, visualOrder]);


  return (
    <>
      <div className="fl-tray-backdrop" onClick={onClose}/>
      <aside className="fl-tray" style={{ width:"min(960px, 96vw)" }}>
        <header className="fl-tray-head">
          <div style={{ display:"flex", alignItems:"center", gap:12, minWidth:0, flex:1 }}>
            <Icon name="shield" size={18} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
            <div style={{ minWidth:0 }}>
              <div style={{ display:"flex", alignItems:"center", gap:8 }}>
                <span style={{ fontWeight:700, fontSize:15 }} className="mono">{sys.hostname}</span>
                <EnvBadge env={sys.environment}/>
                <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>vs</span>
                <span className="chip chip-info">{bundle.name}</span>
              </div>
              <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>
                Stepping through {bundle.policyIds.length} controls · use <kbd className="kbd">j</kbd>/<kbd className="kbd">k</kbd> to navigate
              </div>
            </div>
          </div>
          <div style={{ display:"flex", gap:6 }}>
            {showSystemLink !== false ? (
              <button className="btn btn-ghost focus-ring xs" onClick={() => { onClose(); onOpenSystem?.(sys); }}>
                <Icon name="arrow-right" size={11}/> Open system
              </button>
            ) : (
              <button className="btn btn-ghost focus-ring xs" onClick={() => { onClose(); onOpenBundle?.(bundle); }}>
                <Icon name="arrow-right" size={11}/> View bundle
              </button>
            )}
            <button className="btn-icon focus-ring" onClick={onClose}><Icon name="x" size={16}/></button>
          </div>
        </header>

        <div style={{ display:"grid", gridTemplateColumns:"260px 1fr", flex:1, minHeight:0, overflow:"hidden" }}>
          {/* Left: control nav — grouped by NIST family, filterable */}
          <nav style={{ borderRight:"1px solid var(--cf-divider)", overflowY:"auto", background:"color-mix(in oklab, var(--cf-page-bg) 30%, var(--cf-card-bg))", display:"flex", flexDirection:"column" }}>
            <div style={{ position:"sticky", top:0, zIndex:1, padding:8, background:"color-mix(in oklab, var(--cf-page-bg) 55%, var(--cf-card-bg))", borderBottom:"1px solid var(--cf-divider)" }}>
              <div className="filter-search" style={{ margin:0 }}>
                <Icon name="search" size={12}/>
                <input className="input focus-ring" placeholder="Filter controls…" value={navQuery} onChange={e=>setNavQuery(e.target.value)} style={{ fontSize:11.5, padding:"6px 8px 6px 30px" }}/>
              </div>
            </div>
            {navGroupsFiltered.length === 0 && (
              <div style={{ padding:"20px 14px", fontSize:12, color:"var(--cf-text-muted)", textAlign:"center" }}>No controls match.</div>
            )}
            {(() => { let counter = 0; return navGroupsFiltered.map(grp => {
              const isCollapsed = !!collapsed[grp.fid];
              if (isCollapsed) counter += grp.indices.length;
              return (
              <div key={grp.fid}>
                <button onClick={() => toggleGroup(grp.fid)} className="focus-ring"
                  style={{ all:"unset", cursor:"pointer", display:"flex", alignItems:"center", gap:6, width:"100%", boxSizing:"border-box", padding:"9px 14px 5px", fontSize:9.5, textTransform:"uppercase", letterSpacing:"0.06em", fontWeight:700, color:"var(--cf-text-muted)", position:"sticky", top:0, background:"color-mix(in oklab, var(--cf-page-bg) 55%, var(--cf-card-bg))" }}>
                  <Icon name={isCollapsed ? "chevron-right" : "chevron-down"} size={10}/>
                  <span style={{ flex:1, textAlign:"left" }}>{grp.label} <span className="mono" style={{ opacity:0.7 }}>· {grp.indices.length}</span></span>
                </button>
                {!isCollapsed && grp.indices.map(i => {
                  const ev = evidenceList[i];
                  const n = ++counter;
                  const color = { pass:"#34d399", warn:"#fbbf24", fail:"#f87171", waiver:"#a78bfa" }[ev.status];
                  const isSel = i === activeIdx;
                  return (
                    <button key={ev.policyId}
                      onClick={() => setActiveIdx(i)}
                      className="focus-ring"
                      style={{
                        all:"unset", cursor:"pointer", display:"block",
                        padding:"10px 14px", width:"100%", boxSizing:"border-box",
                        borderLeft:`3px solid ${isSel ? "var(--cf-brand-purple)" : "transparent"}`,
                        background: isSel ? "color-mix(in oklab, var(--cf-brand-purple) 8%, transparent)" : "transparent",
                        borderBottom:"1px solid var(--cf-divider)",
                      }}>
                      <div style={{ display:"flex", justifyContent:"space-between", alignItems:"center", gap:8 }}>
                        <span className="mono" style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{String(n).padStart(2,"0")}</span>
                        <span style={{ display:"flex", alignItems:"center", gap:5 }}>
                          {(() => { const p = poamForFinding(sys.id, ev.policyId); return p ? <span className={`poam-tag${poamIsOverdue(p) ? " overdue" : ""}`} style={{ fontSize:9 }}>POA&M</span> : null; })()}
                          <span style={{ width:8, height:8, borderRadius:"50%", background:color }}/>
                        </span>
                      </div>
                      <div style={{ fontSize:12, color:"var(--cf-text-primary)", marginTop:4, fontWeight: isSel ? 600 : 400, overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>
                        {ev.policyName}
                      </div>
                    </button>
                  );
                })}
              </div>
              );
            }); })()}
          </nav>

          {/* Right: evidence detail */}
          <div style={{ overflow:"auto", padding:20, display:"flex", flexDirection:"column", gap:16 }}>
            {assignment && assignment.status !== "current" && (() => {
              const meta = COMPLIANCE_ASSIGNMENT_STATUS[assignment.status] || COMPLIANCE_ASSIGNMENT_STATUS.current;
              return (
                <div className="sd-callout" style={{ background:`color-mix(in oklab, ${meta.color} 8%, transparent)`, borderColor:`color-mix(in oklab, ${meta.color} 30%, transparent)` }}>
                  <Icon name="warn" size={13} style={{ color:meta.color }}/>
                  <div style={{ fontSize:12 }}>
                    <div><strong style={{ color:meta.color }}>{meta.label}</strong> — pinned to this revision instead of the current baseline.</div>
                    <div style={{ marginTop:4, color:"var(--cf-text-secondary)" }}>{assignment.reason}</div>
                    <div style={{ marginTop:4, display:"flex", gap:12, flexWrap:"wrap", fontSize:11, color:"var(--cf-text-muted)" }}>
                      <span>Approved by <span className="mono">{assignment.approvedBy}</span></span>
                      {assignment.deadline && <span>Migration deadline <span className="mono">{assignment.deadline}</span></span>}
                      {assignment.poam && <span>POA&M <button className="poam-ref poam-ref-quiet focus-ring" onClick={()=>openPoamDetail(assignment.poam)}><span className="mono">{assignment.poam}</span><Icon name="chevron-right" size={10}/></button></span>}
                    </div>
                  </div>
                </div>
              );
            })()}
            <ControlEvidenceCard evidence={active} controlIdx={activeIdx} total={bundle.policyIds.length} sys={sys} bundle={bundle}/>
          </div>
        </div>
      </aside>
    </>
  );
}

function ControlEvidenceCard({ evidence, controlIdx, total, sys, bundle }) {
  const sc = { pass:"#34d399", warn:"#fbbf24", fail:"#f87171", waiver:"#a78bfa" }[evidence.status];
  const sevColor = { high:"#f87171", medium:"#fbbf24", low:"#60a5fa" }[evidence.severity];

  return (
    <>
      <div>
        <div style={{ display:"flex", alignItems:"center", gap:10, flexWrap:"wrap", marginBottom:8 }}>
          <span style={{ fontSize:11, color:"var(--cf-text-muted)" }}>Control {controlIdx + 1} of {total}</span>
          <span className="chip" style={{ color: sc, background:`color-mix(in oklab, ${sc} 14%, transparent)`, border:`1px solid ${sc}` }}>{evidence.status}</span>
          <span className="chip" style={{ color: sevColor, background:`color-mix(in oklab, ${sevColor} 14%, transparent)` }}>{evidence.severity} severity</span>
        </div>
        <h2 style={{ margin:0, fontSize:18, fontWeight:700, fontFamily:"var(--font-mono)" }}>{evidence.policyName}</h2>
        <p style={{ margin:"6px 0 0", fontSize:13, color:"var(--cf-text-secondary)", lineHeight:1.5 }}>{evidence.summary}</p>
      </div>

      {/* Status callout */}
      {evidence.status === "fail" && (
        <div className="sd-callout sd-callout-danger">
          <Icon name="x" size={13}/>
          <div style={{ fontSize:12 }}><strong>Not compliant.</strong> The required configuration is not applied on this host. Investigate via system logs or apply the policy module.</div>
        </div>
      )}
      {evidence.status === "warn" && (
        <div className="sd-callout sd-callout-warn">
          <Icon name="warn" size={13}/>
          <div style={{ fontSize:12 }}><strong>Compliant with warnings.</strong> Auditor may request additional evidence.</div>
        </div>
      )}
      {evidence.status === "waiver" && (
        <div className="sd-callout" style={{ background:"rgba(167,139,250,0.08)", borderColor:"rgba(167,139,250,0.25)" }}>
          <Icon name="file" size={13} style={{ color:"#a78bfa" }}/>
          <div style={{ fontSize:12 }}><strong>Waiver in effect.</strong> Risk accepted with compensating control. See evidence below.</div>
        </div>
      )}

      {/* Remediation — the POA&M lives with the finding. It never changes the result above. */}
      {sys && bundle && (
        <FindingPoamBar sysId={sys.id} policyId={evidence.policyId} bundleId={bundle.id} evalStatus={evidence.status}/>
      )}

      {/* Evidence items */}
      <div>
        <h3 style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", margin:"0 0 8px", fontWeight:600 }}>
          Evidence · {evidence.items.length} item{evidence.items.length === 1 ? "" : "s"}
        </h3>
        <div style={{ display:"flex", flexDirection:"column", gap:10 }}>
          {evidence.items.map((item, i) => {
            const meta = EVIDENCE_TYPES[item.type] || { label:item._label || item.type, icon:"file" };
            return (
              <div key={i} className="ev-item">
                <div className="ev-item-head">
                  <Icon name={meta.icon} size={14} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
                  <div style={{ minWidth:0, flex:1 }}>
                    <div style={{ fontSize:12, fontWeight:600, color:"var(--cf-text-primary)" }}>{item._label || meta.label}</div>
                    <div className="mono" style={{ fontSize:11, color:"var(--cf-text-secondary)", overflow:"hidden", textOverflow:"ellipsis", whiteSpace:"nowrap" }}>{item.ref}</div>
                  </div>
                  <div style={{ fontSize:11, color:"var(--cf-text-muted)", textAlign:"right", whiteSpace:"nowrap", flexShrink:0 }}>
                    <div>{item.at}</div>
                    <div className="mono" style={{ fontSize:10, marginTop:2 }}>{item.source}</div>
                  </div>
                </div>
                {item.artifact && <EvidenceArtifact artifact={item.artifact}/>}
              </div>
            );
          })}
        </div>
      </div>

      {/* Mapping placeholder */}
      <div style={{ padding:12, background:"var(--cf-subtle-bg)", borderRadius:8, fontSize:11, color:"var(--cf-text-secondary)" }}>
        <strong style={{ color:"var(--cf-text-primary)" }}>Framework mapping</strong>
        <span style={{ marginLeft:8 }}>—</span>
        <span className="mono" style={{ marginLeft:8 }}>SRG-OS-{(controlIdx * 31 + 23) % 1000} / CCI-{(controlIdx * 7 + 41) % 9999}</span>
      </div>
    </>
  );
}

/* ── Renders the actual evidence artifact body ── */
function EvidenceArtifact({ artifact }) {
  const [open, setOpen] = React.useState(true);
  const kind = artifact.kind;
  const isShot = kind === "screenshot";

  // Lightweight prompt coloring for terminal-style artifacts
  const renderTerminal = (text) => text.split("\n").map((ln, i) => {
    const isPrompt = ln.startsWith("$ ");
    const isRule = ln.startsWith("----") || ln.startsWith("→") || ln.includes("→ ");
    return (
      <div key={i} style={{ whiteSpace:"pre", color: isPrompt ? "#7dd3fc" : isRule ? "#34d399" : "var(--cf-text-secondary)" }}>{ln || " "}</div>
    );
  });

  return (
    <div className={`ev-art ev-art-${kind}`}>
      <button className="ev-art-bar focus-ring" onClick={() => setOpen(o => !o)}>
        <Icon name={kind === "json" ? "file" : kind === "code" ? "file" : kind === "doc" ? "file" : isShot ? "server" : "terminal"} size={11}/>
        <span className="ev-art-title">{artifact.title}</span>
        <Icon name={open ? "chevron-up" : "chevron-down"} size={13} style={{ marginLeft:"auto" }}/>
      </button>
      {open && (
        isShot ? (
          <div className="ev-shot">
            <div className="ev-shot-bar"><span/><span/><span/></div>
            <pre className="ev-shot-body">{artifact.content}</pre>
          </div>
        ) : (
          <pre className={`ev-art-body ev-art-body-${kind}`}>
            {kind === "terminal" ? renderTerminal(artifact.content) : artifact.content}
          </pre>
        )
      )}
    </div>
  );
}

/* ── Export modal ── */
function ExportEvidenceModal({ bundle, stats, onClose }) {
  const [format, setFormat] = React.useState("oscal");
  const [scope, setScope] = React.useState("all");
  const [includeWaivers, setIncludeWaivers] = React.useState(true);
  const [includeSourceConfig, setIncludeSourceConfig] = React.useState(true);
  const [bundleIds, setBundleIds] = React.useState([bundle.id]);
  const [envs, setEnvs] = React.useState(() => [...bundle.requiredEnvs]);

  const allBundles = (typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : []);
  const selectedBundles = allBundles.filter(b => bundleIds.includes(b.id));

  // Environments available = union of requiredEnvs across selected bundles
  const availableEnvs = React.useMemo(() => {
    const set = new Set();
    selectedBundles.forEach(b => b.requiredEnvs.forEach(e => set.add(e)));
    return (typeof ENVIRONMENTS !== "undefined" ? ENVIRONMENTS : []).map(e => e.name).filter(n => set.has(n));
  }, [bundleIds]);

  // Keep envs valid when bundle selection changes
  React.useEffect(() => {
    setEnvs(prev => {
      const next = prev.filter(e => availableEnvs.includes(e));
      return next.length ? next : [...availableEnvs];
    });
  }, [bundleIds]);

  const toggleBundle = (id) => setBundleIds(prev =>
    prev.includes(id) ? (prev.length > 1 ? prev.filter(x => x !== id) : prev) : [...prev, id]);
  const toggleEnv = (name) => setEnvs(prev =>
    prev.includes(name) ? prev.filter(x => x !== name) : [...prev, name]);

  const [bundleQuery, setBundleQuery] = React.useState("");
  const filteredBundles = allBundles.filter(b =>
    !bundleQuery ||
    b.name.toLowerCase().includes(bundleQuery.toLowerCase()) ||
    (b.framework||"").toLowerCase().includes(bundleQuery.toLowerCase()));

  // Recompute scope stats live from selection
  const computed = React.useMemo(() => {
    let totalHosts = 0, totalControls = 0, pass = 0, warn = 0, fail = 0, waiver = 0;
    const hostSet = new Set();
    selectedBundles.forEach(b => {
      SYSTEMS.filter(s => b.requiredEnvs.includes(s.environment) && envs.includes(s.environment))
        .forEach(s => {
          const r = bundleStatusForSystem(b, s);
          if (!r.applies) return;
          hostSet.add(s.id);
          totalHosts += 1;
          totalControls += r.total; pass += r.pass; warn += r.warn; fail += r.fail; waiver += r.waiver;
        });
    });
    return { uniqueHosts: hostSet.size, hostEvals: totalHosts, totalControls, pass, warn, fail, waiver };
  }, [bundleIds, envs]);

  const formatMeta = {
    oscal: { name:"OSCAL 1.1.2 JSON",  ext:"oscal.json", desc:"NIST OSCAL System Security Plan + Assessment Results for ATO packages." },
    json:  { name:"Crystal Forge JSON", ext:"cf-evidence.json", desc:"Native CF schema — best for re-ingest or custom dashboards." },
    csv:   { name:"CSV summary",        ext:"summary.csv", desc:"Flat per-(host, control) table. Spreadsheet-friendly." },
    pdf:   { name:"PDF report",         ext:"pdf",        desc:"Cover page + per-host summary + evidence index. For auditors." },
    sarif: { name:"SARIF 2.1.0",        ext:"sarif",      desc:"Static analysis exchange format — works with most SAST/posture tools." },
  };

  const filename = (() => {
    const date = new Date().toISOString().slice(0,10);
    const envPart = envs.length === 1 ? envs[0] : envs.length === availableEnvs.length ? "all-envs" : `${envs.length}envs`;
    const bundlePart = bundleIds.length === 1 ? bundle.id : `${bundleIds.length}bundles`;
    return `cf-${bundlePart}-${envPart}-${date}.${formatMeta[format].ext}`;
  })();

  const canExport = bundleIds.length > 0 && envs.length > 0 && computed.hostEvals > 0;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(680px,96vw)", maxHeight:"92vh" }}>
        <div className="modal-head">
          <h2><Icon name="download" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>Export evidence</h2>
          <p>Each environment typically has its own ATO package — select the bundles and environments to scope this export.</p>
        </div>
        <div className="modal-body" style={{ overflowY:"auto" }}>

          <div className="field">
            <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:8 }}>
              <label style={{ margin:0 }}>Compliance bundles <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· {bundleIds.length} of {allBundles.length}</span></label>
              <div style={{ display:"flex", gap:4 }}>
                <button className="focus-ring" onClick={() => setBundleIds(allBundles.map(b => b.id))}
                  style={{ all:"unset", cursor:"pointer", fontSize:11, color:"var(--cf-brand-purple)", padding:"2px 6px" }}>Select all</button>
                <button className="focus-ring" onClick={() => setBundleIds([bundle.id])}
                  style={{ all:"unset", cursor:"pointer", fontSize:11, color:"var(--cf-text-muted)", padding:"2px 6px" }}>Reset</button>
              </div>
            </div>
            {allBundles.length > 4 && (
              <input className="input focus-ring" placeholder="Search bundles…" value={bundleQuery}
                onChange={e=>setBundleQuery(e.target.value)} style={{ marginBottom:8 }}/>
            )}
            <div style={{ display:"flex", flexDirection:"column", gap:6, maxHeight:208, overflowY:"auto", paddingRight:2 }}>
              {filteredBundles.length === 0 && (
                <div style={{ fontSize:12, color:"var(--cf-text-muted)", padding:"8px 2px" }}>No bundles match “{bundleQuery}”.</div>
              )}
              {filteredBundles.map(b => {
                const on = bundleIds.includes(b.id);
                return (
                  <button key={b.id} className="focus-ring" onClick={() => toggleBundle(b.id)}
                    style={{
                      all:"unset", cursor:"pointer", padding:"9px 11px", borderRadius:8,
                      border:`1px solid ${on ? "var(--cf-brand-purple)" : "var(--cf-divider)"}`,
                      background: on ? "color-mix(in oklab, var(--cf-brand-purple) 8%, var(--cf-card-bg))" : "var(--cf-card-bg)",
                      display:"flex", alignItems:"center", gap:10,
                    }}>
                    <span style={{
                      width:16, height:16, borderRadius:4, flexShrink:0,
                      border:`1.5px solid ${on ? "var(--cf-brand-purple)" : "var(--cf-text-muted)"}`,
                      background: on ? "var(--cf-brand-purple)" : "transparent",
                      display:"flex", alignItems:"center", justifyContent:"center",
                    }}>{on && <Icon name="check" size={11} style={{ color:"white" }}/>}</span>
                    <div style={{ minWidth:0, flex:1 }}>
                      <div style={{ fontSize:12, fontWeight:600 }}>{b.name}</div>
                      <div style={{ fontSize:11, color:"var(--cf-text-muted)" }}>{b.framework} · {b.version} · {b.policyIds.length} controls</div>
                    </div>
                    <div style={{ display:"flex", gap:4, flexShrink:0 }}>
                      {b.requiredEnvs.map(e => <EnvBadge key={e} env={e}/>)}
                    </div>
                  </button>
                );
              })}
            </div>
          </div>

          <div className="field">
            <label>Environments {envs.length < availableEnvs.length && <span style={{ color:"var(--cf-brand-purple)", fontWeight:600 }}>· scoped</span>}</label>
            <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
              {availableEnvs.map(name => {
                const on = envs.includes(name);
                const meta = (typeof ENVIRONMENTS !== "undefined" ? ENVIRONMENTS : []).find(e => e.name === name);
                return (
                  <button key={name} className="focus-ring" onClick={() => toggleEnv(name)}
                    style={{
                      all:"unset", cursor:"pointer", padding:"6px 12px", borderRadius:99,
                      border:`1px solid ${on ? (meta?.dot || "var(--cf-brand-purple)") : "var(--cf-divider)"}`,
                      background: on ? `color-mix(in oklab, ${meta?.dot || "var(--cf-brand-purple)"} 14%, var(--cf-card-bg))` : "var(--cf-card-bg)",
                      display:"flex", alignItems:"center", gap:7, fontSize:12, fontWeight:600,
                      color: on ? "var(--cf-text-primary)" : "var(--cf-text-muted)",
                    }}>
                    <span style={{ width:8, height:8, borderRadius:99, background: meta?.dot || "#888" }}/>
                    {name}
                    {on && <Icon name="check" size={11}/>}
                  </button>
                );
              })}
            </div>
            <div className="help" style={{ marginTop:6 }}>
              Export one environment at a time for a focused ATO, or combine several. Only hosts in the selected environments are included.
            </div>
          </div>

          <div className="field">
            <label>Output format</label>
            <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:8 }}>
              {Object.entries(formatMeta).map(([k, m]) => (
                <button key={k}
                  className="focus-ring"
                  onClick={() => setFormat(k)}
                  style={{
                    all:"unset", cursor:"pointer",
                    padding:"10px 12px", borderRadius:8,
                    border: `1px solid ${format === k ? "var(--cf-brand-purple)" : "var(--cf-divider)"}`,
                    background: format === k ? "color-mix(in oklab, var(--cf-brand-purple) 8%, var(--cf-card-bg))" : "var(--cf-card-bg)",
                    display:"flex", flexDirection:"column", gap:4,
                  }}>
                  <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:6 }}>
                    <span style={{ fontSize:12, fontWeight:600 }}>{m.name}</span>
                    {format === k && <Icon name="check" size={12} style={{ color:"var(--cf-brand-purple)" }}/>}
                  </div>
                  <div style={{ fontSize:11, color:"var(--cf-text-muted)", lineHeight:1.4 }}>{m.desc}</div>
                </button>
              ))}
            </div>
          </div>

          <div className="field">
            <label>Host scope</label>
            <div className="seg" style={{ width:"fit-content" }}>
              {[
                { v:"all",  l:`All ${computed.hostEvals} host evals` },
                { v:"fail", l:`Failing only (${computed.fail})` },
                { v:"clean",l:"Compliant only" },
              ].map(o => (
                <button key={o.v} className={scope === o.v ? "active" : ""} onClick={() => setScope(o.v)}>{o.l}</button>
              ))}
            </div>
          </div>

          <div style={{ display:"flex", flexDirection:"column", gap:8 }}>
            <label style={{ display:"flex", gap:8, alignItems:"center", fontSize:13, cursor:"pointer" }}>
              <input type="checkbox" checked={includeWaivers} onChange={e=>setIncludeWaivers(e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
              <span>Include waiver justifications + expiry dates</span>
            </label>
            <label style={{ display:"flex", gap:8, alignItems:"center", fontSize:13, cursor:"pointer" }}>
              <input type="checkbox" checked={includeSourceConfig} onChange={e=>setIncludeSourceConfig(e.target.checked)} style={{ accentColor:"var(--cf-brand-purple)" }}/>
              <span>Include rendered NixOS module source for each control</span>
            </label>
          </div>

          <div className="sd-callout sd-callout-info" style={{ marginTop:10 }}>
            <Icon name="check" size={13}/>
            <div style={{ fontSize:12 }}>
              <div><strong>{bundleIds.length}</strong> bundle{bundleIds.length===1?"":"s"} · <strong>{envs.length}</strong> environment{envs.length===1?"":"s"} · <strong>{computed.uniqueHosts}</strong> host{computed.uniqueHosts===1?"":"s"} · <strong>{computed.totalControls}</strong> control evaluations</div>
              <div style={{ marginTop:4 }}>Filename: <span className="mono" style={{ fontWeight:600 }}>{filename}</span></div>
            </div>
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" onClick={onClose} disabled={!canExport}
            style={!canExport ? { opacity:0.5, cursor:"not-allowed" } : null}>
            <Icon name="download" size={13}/> Download {formatMeta[format].name}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ── Bundle form modal: create or edit ── */
function NewBundleModal({ onClose, bundle: editBundle, onDelete }) {
  const isEdit = !!editBundle;
  const [form, setForm] = React.useState({
    name: editBundle?.name || "",
    framework: editBundle?.framework || "DISA STIG",
    version: editBundle?.version || "",
    description: editBundle?.description || "",
    requiredEnvs: editBundle?.requiredEnvs ? [...editBundle.requiredEnvs] : ["production"],
    policyIds: editBundle?.policyIds ? [...editBundle.policyIds] : [],
  });
  const [query, setQuery] = React.useState("");
  const set = (k, v) => setForm(p => ({ ...p, [k]: v }));
  const [customFrameworks, setCustomFrameworks] = React.useState(() => loadCustomFrameworks());
  const [newFrameworkOpen, setNewFrameworkOpen] = React.useState(false);
  const [newFrameworkName, setNewFrameworkName] = React.useState("");
  const onFrameworkChange = (v) => {
    if (v === "__new__") { setNewFrameworkOpen(true); return; }
    set("framework", v);
  };
  const saveNewFramework = () => {
    const name = newFrameworkName.trim();
    if (!name) return;
    const next = [...customFrameworks, { id: `fw-${Date.now()}`, name }];
    setCustomFrameworks(next);
    saveCustomFrameworks(next);
    set("framework", name);
    setNewFrameworkOpen(false);
    setNewFrameworkName("");
  };

  const policies = (typeof POLICIES !== "undefined" ? POLICIES : []).filter(p => p.publicationState !== "deprecated");
  const filtered = policies.filter(p =>
    !query || p.name.toLowerCase().includes(query.toLowerCase()) || (p.description||"").toLowerCase().includes(query.toLowerCase())
  );
  const togglePolicy = (id) => set("policyIds", form.policyIds.includes(id)
    ? form.policyIds.filter(x => x !== id)
    : [...form.policyIds, id]);
  const toggleEnv = (env) => set("requiredEnvs", form.requiredEnvs.includes(env)
    ? form.requiredEnvs.filter(x => x !== env)
    : [...form.requiredEnvs, env]);

  const canSave = form.name.trim() && form.policyIds.length > 0;
  const [confirmDel, setConfirmDel] = React.useState(false);

  const save = () => {
    if (isEdit) {
      Object.assign(editBundle, {
        name: form.name.trim(), framework: form.framework, version: form.version,
        description: form.description, requiredEnvs: form.requiredEnvs, policyIds: form.policyIds,
        lastReview: "just now",
      });
    } else {
      window.__cfCoach?.complete("compliance");
    }
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(760px,97vw)", maxHeight:"92vh" }}>
        {confirmDel ? (
          <DeleteBundleConfirm bundle={editBundle} onCancel={() => setConfirmDel(false)} onConfirm={() => { onDelete?.(); onClose(); }}/>
        ) : (
        <>
        <div className="modal-head">
          <h2><Icon name="shield" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>{isEdit ? "Edit compliance bundle" : "New compliance bundle"}</h2>
          <p>A bundle represents a standard (a STIG, NIST baseline, or your own) — assembled from granular policies that each assert one thing.</p>
        </div>
        <div className="modal-body" style={{ overflowY:"auto" }}>
          <div style={{ display:"grid", gridTemplateColumns:"2fr 1fr", gap:14 }}>
            <div className="field" style={{ marginTop:0 }}>
              <label>Bundle name</label>
              <input className="input focus-ring" value={form.name} onChange={e=>set("name",e.target.value)} placeholder="e.g. Anduril NixOS STIG (v1r2)"/>
            </div>
            <div className="field" style={{ marginTop:0 }}>
              <label>Version / revision</label>
              <input className="input focus-ring mono" value={form.version} onChange={e=>set("version",e.target.value)} placeholder="v1r5" style={{ fontSize:12 }}/>
            </div>
          </div>
          <div style={{ display:"grid", gridTemplateColumns:"1fr 2fr", gap:14, marginTop:14 }}>
            <div className="field" style={{ marginTop:0 }}>
              <label>Framework</label>
              {newFrameworkOpen ? (
                <div style={{ display:"flex", gap:6 }}>
                  <input className="input focus-ring" autoFocus value={newFrameworkName} onChange={e=>setNewFrameworkName(e.target.value)}
                    placeholder="e.g. Acme Internal Baseline" onKeyDown={e=>{ if(e.key==="Enter") saveNewFramework(); if(e.key==="Escape") setNewFrameworkOpen(false); }}/>
                  <button className="btn btn-ghost focus-ring xs" onClick={saveNewFramework} disabled={!newFrameworkName.trim()}>Add</button>
                  <button className="btn btn-ghost focus-ring xs" onClick={()=>setNewFrameworkOpen(false)}>Cancel</button>
                </div>
              ) : (
                <select className="input focus-ring" value={form.framework} onChange={e=>onFrameworkChange(e.target.value)}>
                  <optgroup label="Standard">
                    {BUILTIN_FRAMEWORKS.map(f => <option key={f}>{f}</option>)}
                  </optgroup>
                  {customFrameworks.length > 0 && (
                    <optgroup label="Custom">
                      {customFrameworks.map(f => <option key={f.name}>{f.name}</option>)}
                    </optgroup>
                  )}
                  <option value="__new__">+ Define new framework…</option>
                </select>
              )}
            </div>
            <div className="field" style={{ marginTop:0 }}>
              <label>Description</label>
              <input className="input focus-ring" value={form.description} onChange={e=>set("description",e.target.value)} placeholder="What this bundle verifies"/>
            </div>
          </div>

          <div className="field">
            <label>Applies to environments</label>
            <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
              {ENVIRONMENTS.map(env => {
                const on = form.requiredEnvs.includes(env.name);
                return (
                  <button key={env.name} className="focus-ring" onClick={()=>toggleEnv(env.name)}
                    style={{
                      padding:"4px 10px", borderRadius:99, fontSize:11, cursor:"pointer",
                      border:`1px solid ${on ? env.color : "var(--cf-card-border)"}`,
                      background: on ? `color-mix(in oklab, ${env.color} 14%, var(--cf-card-bg))` : "transparent",
                      color: on ? env.color : "var(--cf-text-secondary)",
                      display:"inline-flex", alignItems:"center", gap:6, fontFamily:"inherit",
                    }}>
                    <span style={{ width:6, height:6, borderRadius:"50%", background: env.color }}/>
                    {env.name}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Policy picker */}
          <div style={{ padding:14, border:"1px solid var(--cf-divider)", borderRadius:10, background:"color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg))" }}>
            <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", gap:8, marginBottom:10 }}>
              <div style={{ fontSize:13, fontWeight:600, display:"flex", alignItems:"center", gap:6 }}>
                <Icon name="file" size={13}/> Controls in this bundle
                <span className="chip chip-info" style={{ fontSize:10 }}>{form.policyIds.length} selected</span>
              </div>
              <div className="filter-search" style={{ maxWidth:200, margin:0 }}>
                <Icon name="search"/>
                <input className="input focus-ring" placeholder="Filter policies…" value={query} onChange={e=>setQuery(e.target.value)}/>
              </div>
            </div>
            <div style={{ display:"flex", flexDirection:"column", gap:10, maxHeight:280, overflowY:"auto" }}>
              {(() => {
                const { mapped, other } = (typeof splitPoliciesForBundleFramework === "function")
                  ? splitPoliciesForBundleFramework(filtered, form.framework)
                  : { mapped: [], other: filtered };
                const renderRow = (p, custom) => {
                  const on = form.policyIds.includes(p.id);
                  return (
                    <button key={p.id} className="focus-ring" onClick={()=>togglePolicy(p.id)}
                      style={{
                        all:"unset", cursor:"pointer", display:"flex", gap:10, alignItems:"flex-start",
                        padding:"9px 11px", borderRadius:8,
                        border:`1px solid ${on ? "var(--cf-brand-purple)" : "var(--cf-divider)"}`,
                        background: on ? "color-mix(in oklab, var(--cf-brand-purple) 9%, var(--cf-card-bg))" : "var(--cf-card-bg)",
                      }}>
                      <div style={{
                        width:16, height:16, borderRadius:5, flexShrink:0, marginTop:1,
                        border:`1.5px solid ${on ? "var(--cf-brand-purple)" : "var(--cf-card-border)"}`,
                        background: on ? "var(--cf-brand-purple)" : "transparent",
                        display:"grid", placeItems:"center",
                      }}>
                        {on && <Icon name="check" size={11} style={{ color:"#fff" }}/>}
                      </div>
                      <div style={{ minWidth:0, flex:1 }}>
                        <div style={{ display:"flex", alignItems:"center", gap:8, flexWrap:"wrap" }}>
                          <span className="mono" style={{ fontSize:12, fontWeight:600 }}>{p.name}</span>
                          <span className={`chip ${p.type === "builtin" ? "chip-unknown" : "chip-info"}`} style={{ fontSize:9 }}>{p.type}</span>
                          {custom && <span className="chip chip-warning" style={{ fontSize:9 }}>Custom addition</span>}
                        </div>
                        <div style={{ fontSize:11, color:"var(--cf-text-muted)", marginTop:2 }}>{p.description}</div>
                        {custom && <div style={{ fontSize:10, color:"var(--cf-text-muted)", marginTop:2 }}>No mapping to {form.framework || "this framework"}</div>}
                      </div>
                    </button>
                  );
                };
                return (
                  <>
                    {mapped.length > 0 && (
                      <div>
                        <div style={{ fontSize:10.5, fontWeight:600, textTransform:"uppercase", letterSpacing:"0.06em", color:"var(--cf-text-muted)", margin:"2px 0 6px" }}>Mapped to {form.framework || "this framework"}</div>
                        <div style={{ display:"flex", flexDirection:"column", gap:4 }}>{mapped.map(p=>renderRow(p,false))}</div>
                      </div>
                    )}
                    {other.length > 0 && (
                      <div>
                        <div style={{ fontSize:10.5, fontWeight:600, textTransform:"uppercase", letterSpacing:"0.06em", color:"var(--cf-text-muted)", margin:"2px 0 6px" }}>Other reusable policies</div>
                        <div style={{ display:"flex", flexDirection:"column", gap:4 }}>{other.map(p=>renderRow(p,mapped.length>0))}</div>
                      </div>
                    )}
                  </>
                );
              })()}
              {filtered.length === 0 && (
                <div style={{ fontSize:12, color:"var(--cf-text-muted)", padding:"16px 0", textAlign:"center" }}>No policies match. Define new policies in the Policies view.</div>
              )}
            </div>
          </div>

          {form.policyIds.length === 0 && (
            <div className="help" style={{ color:"#fbbf24" }}>
              <Icon name="warn" size={10} style={{ verticalAlign:"middle" }}/> Select at least one policy. A bundle is a collection of policies that together represent a standard.
            </div>
          )}

          {isEdit && (
            <div style={{ marginTop:10, paddingTop:14, borderTop:"1px solid var(--cf-divider)" }}>
              <div style={{ fontSize:11, fontWeight:600, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", marginBottom:8 }}>Danger zone</div>
              <button className="btn btn-ghost focus-ring" onClick={()=>setConfirmDel(true)} style={{ color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }}>
                <Icon name="trash" size={12}/> Delete bundle
              </button>
            </div>
          )}
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary focus-ring" disabled={!canSave} onClick={save}>
            <Icon name="check" size={13}/> {isEdit ? "Save changes" : "Create bundle"}
          </button>
        </div>
        </>
        )}
      </div>
    </div>
  );
}

function DeleteBundleConfirm({ bundle, onCancel, onConfirm }) {
  const [typed, setTyped] = React.useState("");
  const matches = typed === bundle.name;
  const policyCount = bundle.policyIds?.length || 0;
  return (
    <>
      <div className="modal-head" style={{ background:"rgba(248,113,113,0.06)" }}>
        <h2 style={{ color:"#fecaca", display:"flex", alignItems:"center", gap:8 }}>
          <Icon name="warn" size={16} style={{ color:"#f87171" }}/>
          Delete bundle
        </h2>
        <p>This permanently removes the <span className="mono" style={{ fontWeight:600 }}>{bundle.name}</span> compliance bundle.</p>
      </div>
      <div className="modal-body">
        <div className="sd-callout sd-callout-danger" style={{ marginBottom:12 }}>
          <Icon name="warn" size={14}/>
          <div style={{ fontSize:12, color:"#fecaca" }}>
            <ul style={{ margin:0, paddingLeft:16, lineHeight:1.6 }}>
              <li>The bundle and its mapping of {policyCount} polic{policyCount === 1 ? "y" : "ies"} is removed</li>
              <li>Underlying policies are <em>not</em> deleted — they remain in the Policies view</li>
              <li>Systems referencing this bundle for compliance will no longer be gated by it</li>
              <li>Collected evidence history is retained for audit</li>
            </ul>
          </div>
        </div>
        <div className="field">
          <label>Type <span className="mono" style={{ color:"#fecaca", fontWeight:700 }}>{bundle.name}</span> to confirm</label>
          <input className="input focus-ring mono" placeholder={bundle.name} value={typed} onChange={e=>setTyped(e.target.value)} autoFocus/>
        </div>
      </div>
      <div className="modal-foot">
        <button className="btn btn-ghost focus-ring" onClick={onCancel}>Cancel</button>
        <button className="btn focus-ring" disabled={!matches} onClick={onConfirm}
          style={{ background: matches ? "#dc2626" : "var(--cf-subtle-bg)", color: matches ? "white" : "var(--cf-text-muted)" }}>
          <Icon name="trash" size={13}/> Delete bundle
        </button>
      </div>
    </>
  );
}

Object.assign(window, { ComplianceView, ControlsEvidenceDrawer, ControlEvidenceCard, exportBundle, PubStateChip, RequirementCoverageCard });

// ── Community bundle interchange — XCCDF 1.2 + a small Crystal Forge extension,
// per the CF-XCCDF Interchange Profile draft. A bundle exports as one <Benchmark>:
// cf:bundle metadata, a baseline <Profile> selecting every included policy, and one
// <Rule> per policy. Rules backed by a custom_eval assertion get a full cf:custom-check
// (exact round trip); other policy kinds export as human-readable rules only.
function xmlEscape(s) { return String(s ?? "").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;"); }

function policyToXccdfRule(policy) {
  const id = xmlEscape(policy.id);
  const cat = policyCategoryMeta(policy.category || "deployment");
  const severity = policy.severity === "high" ? "high" : policy.severity === "low" ? "low" : policy.severity === "medium" ? "medium" : "unknown";
  const custom = (policy.rules || []).find(r => r.kind === "custom_eval");
  const otherRules = (policy.rules || []).filter(r => r.kind !== "custom_eval");
  const check = custom ? `
    <xccdf:check system="urn:crystal-forge:check-system:policy:1">
      <xccdf:check-content>
        <cf:policy schema-version="1">
          <cf:execution phase="nix-evaluation" strict="${!!custom.strict}"/>
          <cf:implementation>
            <cf:custom-check mode="all" context="nixos-configuration-v1" binding="cfg">
              <cf:rule field-name="${xmlEscape(policy.name)}" strict="${!!custom.strict}">
                <cf:description>${xmlEscape(custom.message || policy.description)}</cf:description>
                <cf:expression language="nix"><![CDATA[${(custom.expr || "").replace(/]]>/g, "]]]]><![CDATA[>")}]]></cf:expression>
              </cf:rule>
            </cf:custom-check>
          </cf:implementation>
        </cf:policy>
      </xccdf:check-content>
    </xccdf:check>` : "";
  return `
  <xccdf:Rule id="xccdf_org.crystalforge_rule_${id}" selected="${policy.enabled !== false}" severity="${severity}">
    <xccdf:status>accepted</xccdf:status>
    <xccdf:title>${xmlEscape(policy.name)}</xccdf:title>
    <xccdf:description>${xmlEscape(policy.description)}</xccdf:description>
    ${policy.rationale ? `<xccdf:rationale>${xmlEscape(policy.rationale)}</xccdf:rationale>` : ""}
    <xccdf:metadata>
      <cf:policy-identity policy-id="urn:cf:policy:${id}" publication-state="accepted">
        <cf:category>${xmlEscape(cat.id)}</cf:category>
        ${otherRules.length ? `<cf:gates>${otherRules.map(r => `<cf:gate>${xmlEscape(ruleDescription(r))}</cf:gate>`).join("")}</cf:gates>` : ""}
      </cf:policy-identity>
    </xccdf:metadata>${check}
  </xccdf:Rule>`;
}

function bundleToXccdf(bundle) {
  const policies = bundle.policyIds.map(pid => POLICIES.find(p => p.id === pid)).filter(Boolean);
  const bid = slugify(bundle.name) || "bundle";
  return `<?xml version="1.0" encoding="UTF-8"?>
<xccdf:Benchmark xmlns:xccdf="http://checklists.nist.gov/xccdf/1.2" xmlns:cf="urn:crystal-forge:xccdf:1" id="xccdf_org.crystalforge_benchmark_${bid}">
  <xccdf:status>accepted</xccdf:status>
  <xccdf:title>${xmlEscape(bundle.name)}</xccdf:title>
  <xccdf:description>${xmlEscape(bundle.description || "")}</xccdf:description>
  <xccdf:version>${xmlEscape(bundle.version || "1.0")}</xccdf:version>
  <xccdf:metadata>
    <cf:bundle schema-version="1" bundle-id="urn:cf:bundle:${bid}" publication-state="accepted">
      <cf:framework name="${xmlEscape(bundle.framework || "Community")}" version="${xmlEscape(bundle.version || "1.0")}"/>
      <cf:layer>${xmlEscape(bundle.layer || "system")}</cf:layer>
      <cf:owner>${xmlEscape(bundle.owner || "")}</cf:owner>
      <cf:required-envs>${(bundle.requiredEnvs||[]).map(e=>xmlEscape(e)).join(",")}</cf:required-envs>
    </cf:bundle>
  </xccdf:metadata>
  <xccdf:Profile id="xccdf_org.crystalforge_profile_${bid}-baseline">
    <xccdf:title>Baseline</xccdf:title>
    <xccdf:metadata><cf:profile-role>baseline</cf:profile-role></xccdf:metadata>
    ${policies.map(p => `<xccdf:select idref="xccdf_org.crystalforge_rule_${xmlEscape(p.id)}" selected="true"/>`).join("\n    ")}
  </xccdf:Profile>
  ${policies.map(policyToXccdfRule).join("\n")}
</xccdf:Benchmark>
`;
}

function exportBundle(bundle) {
  downloadFile(`${slugify(bundle.name)||"bundle"}.xml`, bundleToXccdf(bundle), "application/xml");
}

// ── Parse a CF-XCCDF (or foreign XCCDF) benchmark back into a bundle + policies.
function cfByLocal(root, name) { const out=[]; const walk=(el)=>{ for (const c of el.children) { if (c.localName === name) out.push(c); walk(c); } }; walk(root); return out; }
function cfFirst(el, name) { return cfByLocal(el, name)[0] || null; }
function cfText(el) { return el ? (el.textContent || "").trim() : ""; }

function parseCfXccdf(text) {
  const doc = new DOMParser().parseFromString(text, "application/xml");
  if (doc.querySelector("parsererror")) throw new Error("Not valid XML");
  const bench = doc.documentElement.localName === "Benchmark" ? doc.documentElement : cfFirst(doc.documentElement, "Benchmark");
  if (!bench) throw new Error("No <Benchmark> element — is this an XCCDF document?");
  const cfBundle = cfFirst(bench, "bundle");
  const meta = {
    name: cfText(cfFirst(bench, "title")) || "Imported bundle",
    description: cfText(cfFirst(bench, "description")),
    version: cfText(cfFirst(bench, "version")) || "1.0",
    framework: cfBundle ? (cfFirst(cfBundle, "framework")?.getAttribute("name") || "Community") : "Community",
    layer: cfBundle ? cfText(cfFirst(cfBundle, "layer")) || "system" : "system",
    owner: cfBundle ? cfText(cfFirst(cfBundle, "owner")) : "",
    requiredEnvs: cfBundle ? cfText(cfFirst(cfBundle, "required-envs")).split(",").map(s=>s.trim()).filter(Boolean) : [],
  };
  const ruleEls = cfByLocal(bench, "Rule");
  const policies = ruleEls.map((r, i) => {
    const title = cfText(cfFirst(r, "title")) || `Imported rule ${i+1}`;
    const description = cfText(cfFirst(r, "description")) || title;
    const rationale = cfText(cfFirst(r, "rationale"));
    const severity = r.getAttribute("severity") || "unknown";
    const policyId = cfFirst(r, "policy-identity")?.getAttribute("policy-id") || "";
    const slug = policyId.replace(/^urn:cf:policy:/,"") || slugify(title) || `imported-${i}`;
    const customCheck = cfFirst(r, "custom-check");
    const rules = [];
    if (customCheck) {
      const execEl = cfFirst(r, "execution");
      const strict = execEl ? execEl.getAttribute("strict") === "true" : false;
      cfByLocal(customCheck, "rule").forEach(cr => {
        const expr = cfText(cfFirst(cr, "expression"));
        const msg = cfText(cfFirst(cr, "description")) || description;
        rules.push({ kind:"custom_eval", expr, message: msg, strict: cr.getAttribute("strict") === "true" || strict });
      });
    }
    return {
      id: `custom-import-${slug}`, name: slug, category: "security", description,
      type: "custom", enabled: r.getAttribute("selected") !== "false", severity: ["high","medium","low"].includes(severity) ? severity : "medium",
      rationale, rules, evidence: [],
    };
  });
  return { meta, policies };
}

function ImportBundleModal({ onClose, onComplete }) {
  const [parsed, setParsed] = React.useState(null); // { meta, policies: mapped[] }
  const [error, setError] = React.useState("");
  const [dragOver, setDragOver] = React.useState(false);
  const [name, setName] = React.useState("");
  const fileRef = React.useRef(null);

  const handleFile = async (file) => {
    if (!file) return;
    setError("");
    try {
      const isZip = /\.zip$/i.test(file.name);
      let text, sourceName = file.name;
      if (isZip) {
        if (typeof JSZip === "undefined") throw new Error("Zip support failed to load — try again or extract the XCCDF .xml manually.");
        const zip = await JSZip.loadAsync(file);
        const xmlEntries = Object.values(zip.files).filter(f => !f.dir && /\.xml$/i.test(f.name));
        if (!xmlEntries.length) throw new Error("No XCCDF .xml file found inside this zip.");
        // DISA STIG zips bundle an XCCDF benchmark plus OVAL/CPE/manual-check XML — prefer the one that's actually the benchmark.
        const scored = xmlEntries.map(f => ({ f, score: /xccdf/i.test(f.name) ? 2 : /manual/i.test(f.name) ? 1 : 0 }));
        scored.sort((a,b) => b.score - a.score);
        const entry = scored[0].f;
        text = await entry.async("string");
        sourceName = entry.name;
      } else {
        text = await file.text();
      }
      const isXml = /\.xml$/i.test(sourceName) || text.trim().startsWith("<");
      let data;
      if (isXml) {
        data = parseCfXccdf(text);
      } else {
        const json = JSON.parse(text);
        data = { meta: json.bundle || {}, policies: (json.policies || []).map((ext, i) => externalToPolicy(ext, `bundle-${i}`)) };
      }
      if (!data.policies.length) throw new Error("No policies found in this bundle file.");
      setParsed(data);
      setName(data.meta.name || file.name.replace(/\.(json|xml|zip)$/i,""));
    } catch (e) {
      setError(e.message || "Could not parse this file.");
    }
  };

  const doImport = () => {
    const existing = new Set(POLICIES.map(p => p.id));
    const policyIds = [];
    parsed.policies.forEach(pol => {
      policyIds.push(pol.id);
      if (!existing.has(pol.id)) { POLICIES.push(pol); existing.add(pol.id); }
    });
    const bundleId = slugify(name) || ("bundle-" + Date.now());
    const bundle = {
      id: bundleId, name: name || "Imported bundle",
      framework: parsed.meta.framework || "Community", version: parsed.meta.version || "1.0",
      description: parsed.meta.description || `Imported bundle — ${policyIds.length} controls.`,
      layer: parsed.meta.layer || "system", owner: "imported", lastReview: "just now",
      policyIds, requiredEnvs: parsed.meta.requiredEnvs?.length ? parsed.meta.requiredEnvs : ["production"], imported: true,
    };
    const dup = COMPLIANCE_BUNDLES.findIndex(b => b.id === bundleId);
    if (dup >= 0) COMPLIANCE_BUNDLES.splice(dup, 1, bundle); else COMPLIANCE_BUNDLES.push(bundle);
    onComplete?.(bundleId);
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(600px,96vw)", maxHeight:"92vh" }}>
        <div className="modal-head">
          <h2><Icon name="upload" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>Import a shared bundle</h2>
          <p>Upload a bundle exported from another Crystal Forge instance — an XCCDF 1.2 benchmark with the Crystal Forge extension (CF-XCCDF). Plain STIG XCCDF, a DISA STIG .zip download, and legacy JSON bundle exports are also accepted.</p>
        </div>
        <div className="modal-body" style={{ overflowY:"auto" }}>
          {!parsed ? (
            <div
              onDragOver={e=>{e.preventDefault();setDragOver(true);}}
              onDragLeave={()=>setDragOver(false)}
              onDrop={e=>{e.preventDefault();setDragOver(false);handleFile(e.dataTransfer.files[0]);}}
              onClick={()=>fileRef.current?.click()}
              className="focus-ring"
              style={{
                border:`2px dashed ${dragOver ? "var(--cf-brand-purple)" : "var(--cf-divider)"}`,
                background: dragOver ? "color-mix(in oklab, var(--cf-brand-purple) 7%, var(--cf-card-bg))" : "var(--cf-card-bg)",
                borderRadius:12, padding:"38px 20px", textAlign:"center", cursor:"pointer",
              }}>
              <input ref={fileRef} type="file" accept=".xml,.json,.zip,application/xml,application/zip" style={{ display:"none" }}
                onChange={e=>handleFile(e.target.files[0])}/>
              <Icon name="upload" size={22} style={{ color:"var(--cf-text-muted)" }}/>
              <div style={{ fontSize:14, fontWeight:600, marginTop:8 }}>Drop a bundle .xml or DISA STIG .zip here, or click to browse</div>
            </div>
          ) : (
            <>
              <div className="field">
                <label>Bundle name</label>
                <input className="input focus-ring" value={name} onChange={e=>setName(e.target.value)}/>
              </div>
              <div style={{ fontSize:11, textTransform:"uppercase", letterSpacing:"0.06em", color:"var(--cf-text-muted)", fontWeight:600, margin:"10px 0 6px" }}>
                {parsed.policies.length} polic{parsed.policies.length===1?"y":"ies"} to import
              </div>
              <div style={{ display:"flex", flexDirection:"column", gap:6, maxHeight:280, overflowY:"auto" }}>
                {parsed.policies.map((p, i) => (
                  <div key={i} className="card" style={{ padding:"9px 11px" }}>
                    <div className="mono" style={{ fontSize:12.5, fontWeight:600 }}>{p.name}</div>
                    <div style={{ fontSize:11, color:"var(--cf-text-secondary)" }}>{p.description}</div>
                  </div>
                ))}
              </div>
            </>
          )}
          {error && <div className="sd-callout sd-callout-danger" style={{ marginTop:12 }}><Icon name="warn" size={13}/><div style={{ fontSize:12 }}>{error}</div></div>}
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
          {parsed && <button className="btn btn-primary focus-ring" disabled={!name.trim()} onClick={doImport}><Icon name="check" size={13}/> Create bundle</button>}
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { ImportBundleModal });
