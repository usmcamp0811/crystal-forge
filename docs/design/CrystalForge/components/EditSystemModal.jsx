// Edit system modal — opens from Systems list rows/cards/side-panel and SystemDetail header

function EditSystemModal({ sys, onClose }) {
  const [form, setForm] = React.useState(() => ({
    hostname: sys.hostname,
    fqdn: sys.fqdn,
    environment: sys.environment,
    flake: sys.flake,
    branch: sys.branch,
    deploymentPolicy: sys.deploymentPolicy,
    heartbeatIntervalSec: sys.heartbeatIntervalSec,
    reachability: sys.reachability || "direct",
    serverAddress: sys.serverAddress || sys.ipv4 || "",
    description: "",
    tags: sys.tags.join(", "),
    pinnedCommit: sys.commit,
  }));
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const set = (k, v) => setForm(p => ({ ...p, [k]: v }));

  // Recent commits for the system's flake — used by pinned policy picker
  const pinCommits = React.useMemo(() => [
    { sha: sys.commit,  msg: sys.commitMessage,                author: "mreyes",  when: "2h ago" },
    { sha: "a1f2c31",   msg: "chore: bump nixpkgs to 24.11",   author: "ops-bot", when: "yesterday" },
    { sha: "ffa2b88",   msg: "cve: patch openssl",             author: "ops-bot", when: "3d ago" },
    { sha: "7c1209d",   msg: "fix: restart nginx on cert",     author: "jpark",   when: "5d ago" },
    { sha: "9b3a201",   msg: "feat: prometheus exporter",      author: "dchen",   when: "1w ago" },
    { sha: "44102fa",   msg: "stig: harden sshd defaults",     author: "mreyes",  when: "2w ago" },
  ], [sys.id]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(620px,96vw)", maxHeight:"92vh" }}>
        {confirmDelete ? (
          <DeleteSystemConfirm sys={sys} onCancel={()=>setConfirmDelete(false)} onConfirm={onClose}/>
        ) : (
          <>
            <div className="modal-head">
              <h2>
                <Icon name="gear" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>
                Edit {sys.hostname}
              </h2>
              <p>Update system registration, flake assignment, and deployment policy.</p>
            </div>
            <div className="modal-body" style={{ overflowY:"auto" }}>
              <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
                <div className="field">
                  <label>Hostname</label>
                  <input className="input focus-ring mono" value={form.hostname} onChange={e=>set("hostname",e.target.value)}/>
                </div>
                <div className="field">
                  <label>Environment</label>
                  <select className="input focus-ring" value={form.environment} onChange={e=>set("environment",e.target.value)}>
                    {["production","staging","dev","edge","lab"].map(e=><option key={e}>{e}</option>)}
                  </select>
                </div>
              </div>
              <div className="field">
                <label>FQDN</label>
                <input className="input focus-ring mono" value={form.fqdn} onChange={e=>set("fqdn",e.target.value)}/>
              </div>

              {/* Reachability */}
              <div style={{ marginTop:8, padding:14, border:"1px solid var(--cf-divider)", borderRadius:10, background:"color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg))" }}>
                <div style={{ display:"flex", alignItems:"center", gap:6, marginBottom:10, fontSize:13, fontWeight:600 }}>
                  <Icon name="server" size={13}/> Reachability
                </div>
                <div className="field">
                  <label>How the server reaches this system</label>
                  <div className="seg" style={{ width:"fit-content" }}>
                    {[
                      { v:"direct", l:"Direct / LAN" },
                      { v:"pull",   l:"Agent pull-only" },
                    ].map(o => (
                      <button key={o.v} className={form.reachability===o.v?"active":""} onClick={()=>set("reachability",o.v)}>{o.l}</button>
                    ))}
                  </div>
                  <div className="help">
                    {form.reachability === "direct"
                      ? "Server can open connections to the agent (same LAN / routable / VPN). Enables server-initiated deploys and live log tail."
                      : "Agent is behind NAT/firewall and only reaches out to the server. Deploys are applied when the agent next checks in — no inbound connection."}
                  </div>
                </div>
                {form.reachability === "direct" && (
                  <div className="field" style={{ marginTop:10 }}>
                    <label>Server-reachable address</label>
                    <input className="input focus-ring mono" value={form.serverAddress} onChange={e=>set("serverAddress",e.target.value)} placeholder="10.0.4.12 or host.lan" style={{ fontSize:12 }}/>
                    <div className="help">Address/host the Crystal Forge server uses to reach the agent.</div>
                  </div>
                )}
              </div>

              {/* Flake assignment */}
              <div style={{ marginTop:8, padding:14, border:"1px solid var(--cf-divider)", borderRadius:10, background:"color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg))" }}>
                <div style={{ display:"flex", alignItems:"center", gap:6, marginBottom:10, fontSize:13, fontWeight:600 }}>
                  <Icon name="git" size={13}/> Flake assignment
                </div>
                <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
                  <div className="field">
                    <label>Flake</label>
                    <select className="input focus-ring" value={form.flake} onChange={e=>set("flake",e.target.value)}>
                      {FLAKES.map(f=><option key={f}>{f}</option>)}
                    </select>
                  </div>
                  <div className="field">
                    <label>Branch</label>
                    <input className="input focus-ring mono" value={form.branch} onChange={e=>set("branch",e.target.value)}/>
                  </div>
                </div>
              </div>

              {/* Deployment policy — base mode (single-select). Driven by the deployment-category
                  policies so custom deployment modes added in the Policies view appear here. */}
              <div className="field">
                <label>Deployment mode</label>
                {(() => {
                  const modes = (typeof POLICIES !== "undefined" ? POLICIES : [])
                    .filter(p => (p.category || "deployment") === "deployment");
                  const active = modes.find(m => m.id === form.deploymentPolicy);
                  return (
                    <>
                      <div className="seg" style={{ width:"fit-content", flexWrap:"wrap" }}>
                        {modes.map(p => (
                          <button key={p.id}
                            className={form.deploymentPolicy === p.id ? "active" : ""}
                            onClick={()=>set("deploymentPolicy", p.id)}>
                            {p.name}
                          </button>
                        ))}
                      </div>
                      <div className="help">
                        {form.deploymentPolicy === "manual"
                          ? <>Operator must explicitly approve every deploy. Pending approvals appear in the <Icon name="bell" size={10} style={{ verticalAlign:"middle" }}/> notifications and as an <span className="chip chip-warning" style={{ fontSize:9, padding:"0 5px" }}>awaiting approval</span> chip on the system row.</>
                          : (active ? active.description : "Select how this system picks up new configuration.")}
                      </div>
                    </>
                  );
                })()}
                <div className="help" style={{ marginTop:6, display:"flex", alignItems:"center", gap:6, color:"var(--cf-text-muted)" }}>
                  <Icon name="shield" size={11}/>
                  Additional gate policies (CVE, approvals, STIG…) are enforced per-environment and shown on the system's Compliance tab.
                </div>
              </div>

              {/* Pinned commit picker */}
              {form.deploymentPolicy === "pinned" && (
                <div className="field">
                  <label>Pinned commit</label>
                  <div className="sd-commit-list" style={{ maxHeight: 200 }}>
                    {pinCommits.map(c => {
                      const isSel = form.pinnedCommit === c.sha;
                      return (
                        <button key={c.sha}
                          className={`sd-commit-item focus-ring${isSel ? " selected" : ""}`}
                          onClick={()=>set("pinnedCommit", c.sha)}
                        >
                          <span className="mono sd-commit-sha">{c.sha}</span>
                          <span className="sd-commit-msg">{c.msg}</span>
                          <span className="sd-commit-meta mono">{c.author}</span>
                          <span className="sd-commit-meta">{c.when}</span>
                          {c.sha === sys.commit && <span className="chip chip-info" style={{ fontSize:10 }}>current</span>}
                        </button>
                      );
                    })}
                  </div>
                  <div className="help">
                    System will not auto-advance off this commit. Operators can change the pin or temporarily deploy a different commit from System Detail.
                  </div>
                </div>
              )}

              {/* Heartbeat */}
              <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
                <div className="field">
                  <label>Heartbeat interval</label>
                  <select className="input focus-ring" value={form.heartbeatIntervalSec} onChange={e=>set("heartbeatIntervalSec",parseInt(e.target.value,10))}>
                    <option value={30}>30 seconds</option>
                    <option value={60}>1 minute</option>
                    <option value={90}>90 seconds</option>
                    <option value={120}>2 minutes</option>
                    <option value={300}>5 minutes</option>
                  </select>
                </div>
                <div className="field">
                  <label>Tags <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· free-form labels for grouping &amp; filtering</span></label>
                  <input className="input focus-ring" value={form.tags} onChange={e=>set("tags",e.target.value)} placeholder="e.g. builder, stig-enforced"/>
                  {(() => {
                    const used = form.tags.split(",").map(x=>x.trim()).filter(Boolean);
                    const avail = (typeof allFleetTags === "function" ? allFleetTags() : []).filter(t=>!used.includes(t)).slice(0,8);
                    if (!avail.length) return null;
                    return (
                      <div className="help" style={{ display:"flex", alignItems:"center", gap:6, flexWrap:"wrap", marginTop:6 }}>
                        <span>In use:</span>
                        {avail.map(t => (
                          <button key={t} type="button" className="chip chip-unknown focus-ring" style={{ cursor:"pointer" }}
                            onClick={()=>set("tags", used.concat(t).join(", "))}>#{t}</button>
                        ))}
                      </div>
                    );
                  })()}
                </div>
              </div>

              <div className="field">
                <label>Description / notes</label>
                <textarea className="input focus-ring" rows={2} value={form.description} onChange={e=>set("description",e.target.value)} placeholder="Optional context for operators…" style={{ resize:"vertical" }}/>
              </div>

              <div style={{ marginTop:10, paddingTop:14, borderTop:"1px solid var(--cf-divider)" }}>
                <div style={{ fontSize:11, fontWeight:600, textTransform:"uppercase", letterSpacing:"0.08em", color:"var(--cf-text-muted)", marginBottom:8 }}>Danger zone</div>
                <button className="btn btn-ghost focus-ring" onClick={()=>setConfirmDelete(true)} style={{ color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }}>
                  <Icon name="x" size={12}/> Remove system from registry
                </button>
              </div>
            </div>
            <div className="modal-foot">
              <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
              <button className="btn btn-primary focus-ring" onClick={onClose}>
                <Icon name="check" size={13}/> Save changes
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function DeleteSystemConfirm({ sys, onCancel, onConfirm }) {
  const [typed, setTyped] = React.useState("");
  const matches = typed === sys.hostname;
  return (
    <>
      <div className="modal-head" style={{ background:"rgba(248,113,113,0.06)" }}>
        <h2 style={{ color:"#fecaca", display:"flex", alignItems:"center", gap:8 }}>
          <Icon name="warn" size={16} style={{ color:"#f87171" }}/>
          Remove system from registry
        </h2>
        <p>This unregisters <span className="mono" style={{ fontWeight:600 }}>{sys.hostname}</span> from Crystal Forge. The agent on the host will stop receiving deployments.</p>
      </div>
      <div className="modal-body">
        <div className="sd-callout sd-callout-danger" style={{ flexDirection:"column", alignItems:"stretch" }}>
          <div style={{ display:"flex", gap:10, alignItems:"flex-start" }}>
            <Icon name="warn" size={14}/>
            <div style={{ fontSize:12 }}>
              <div style={{ fontWeight:600, color:"#fecaca", marginBottom:4 }}>What happens</div>
              <ul style={{ margin:0, paddingLeft:18, color:"var(--cf-text-secondary)", lineHeight:1.6 }}>
                <li>Auto-deploy stops; agent will keep running its current generation</li>
                <li>Heartbeat data is retained for 90 days of audit history</li>
                <li>The agent's Ed25519 public key is revoked — re-registration required</li>
                <li>System is removed from environment <span className="mono">{sys.environment}</span></li>
              </ul>
            </div>
          </div>
        </div>
        <div className="field">
          <label>Type <span className="mono" style={{ color:"#fecaca", fontWeight:700 }}>{sys.hostname}</span> to confirm</label>
          <input className="input focus-ring mono"
            placeholder={sys.hostname}
            value={typed}
            onChange={e=>setTyped(e.target.value)}
            autoFocus
            style={{ borderColor: typed && !matches ? "rgba(248,113,113,0.5)" : undefined }}/>
        </div>
      </div>
      <div className="modal-foot">
        <button className="btn btn-ghost focus-ring" onClick={onCancel}>Cancel</button>
        <button className="btn focus-ring" disabled={!matches} onClick={onConfirm}
          style={{ background: matches ? "#dc2626" : "var(--cf-subtle-bg)", color: matches ? "white" : "var(--cf-text-muted)" }}>
          <Icon name="x" size={13}/> Remove system
        </button>
      </div>
    </>
  );
}

Object.assign(window, { EditSystemModal });
