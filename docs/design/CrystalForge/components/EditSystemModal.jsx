// Edit system modal — opens from Systems list rows/cards/side-panel and SystemDetail header

function EditSystemModal({ sys, onClose }) {
  const [form, setForm] = React.useState(() => ({
    hostname: sys.hostname,
    fqdn: sys.fqdn || sys.serverAddress || sys.ipv4 || "",
    environment: sys.environment,
    flake: sys.flake,
    branch: sys.branch,
    deploymentPolicy: sys.deploymentPolicy,
    heartbeatIntervalSec: sys.heartbeatIntervalSec,
    reachability: sys.reachability || "direct",
    description: "",
    tags: sys.tags.join(", "),
    pinnedCommit: sys.commit,
  }));
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const [tab, setTab] = React.useState("general"); // general | deployment | security | danger
  const [rotatingKey, setRotatingKey] = React.useState(false);
  const [keyMode, setKeyMode] = React.useState("generate"); // generate | paste
  const [newPubKey, setNewPubKey] = React.useState("");
  const [generatedKeys, setGeneratedKeys] = React.useState(null); // { pub, priv } once generated
  const [privCopied, setPrivCopied] = React.useState(false);
  const [rotated, setRotated] = React.useState(false);
  const set = (k, v) => setForm(p => ({ ...p, [k]: v }));

  // Mock a fresh ed25519 keypair, client-side, for the "generate for me" path — mirrors
  // cloud-console keypair downloads: shown once, never stored server-side.
  const genKeypair = () => {
    let seed = Date.now() % 2147483647;
    const rnd = () => { seed = (Math.imul(seed, 48271)) % 2147483647; return seed; };
    const b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const chunk = (n) => { let s = ""; for (let i=0;i<n;i++) s += b64[rnd()%64]; return s; };
    const pub = `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5${chunk(43)} crystal-forge@${sys.hostname}`;
    const priv = `-----BEGIN OPENSSH PRIVATE KEY-----\n${chunk(64)}\n${chunk(64)}\n${chunk(32)}\n-----END OPENSSH PRIVATE KEY-----`;
    setGeneratedKeys({ pub, priv });
    setNewPubKey(pub);
  };

  // Mock current fingerprint, deterministic from hostname (real system has no key stored yet).
  const currentFingerprint = React.useMemo(() => {
    let seed = sys.hostname.split("").reduce((a,c)=>a*31+c.charCodeAt(0),7); 
    const b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let s = ""; for (let i=0;i<43;i++){ seed = (Math.imul(seed,1103515245)+12345)>>>0; s += b64[(seed>>>24)%64]; }
    return `SHA256:${s}`;
  }, [sys.hostname]);
  const newFingerprint = React.useMemo(() => {
    const key = newPubKey.trim();
    if (!key || key.length < 20) return null;
    let seed = key.split("").reduce((a,c)=>a*33+c.charCodeAt(0),11);
    const b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let s = ""; for (let i=0;i<43;i++){ seed = (Math.imul(seed,1103515245)+12345)>>>0; s += b64[(seed>>>24)%64]; }
    return `SHA256:${s}`;
  }, [newPubKey]);
  const newKeyValid = /^ssh-(ed25519|rsa|ecdsa)/.test(newPubKey.trim());

  // Recent commits for the system's flake — used by pinned policy picker
  const pinCommits = React.useMemo(() => [
    { sha: sys.commit,  msg: sys.commitMessage,                author: "mreyes",  when: "2h ago" },
    { sha: "a1f2c31",   msg: "chore: bump nixpkgs to 24.11",   author: "ops-bot", when: "yesterday" },
    { sha: "ffa2b88",   msg: "cve: patch openssl",             author: "ops-bot", when: "3d ago" },
    { sha: "7c1209d",   msg: "fix: restart nginx on cert",     author: "jpark",   when: "5d ago" },
    { sha: "9b3a201",   msg: "feat: prometheus exporter",      author: "dchen",   when: "1w ago" },
    { sha: "44102fa",   msg: "stig: harden sshd defaults",     author: "mreyes",  when: "2w ago" },
  ], [sys.id]);

  const SECTIONS = [
    { id:"general",    label:"General",     icon:"server" },
    { id:"deployment", label:"Deployment",  icon:"deploy" },
    { id:"security",   label:"Security",    icon:"key" },
    { id:"danger",     label:"Danger zone", icon:"warn", danger:true },
  ];

  React.useEffect(() => {
    const onKey = (e) => { if (e.key === "Escape" && !confirmDelete) onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, confirmDelete]);

  if (confirmDelete) {
    return (
      <div className="modal-backdrop" onClick={onClose}>
        <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(560px,96vw)" }}>
          <DeleteSystemConfirm sys={sys} onCancel={()=>setConfirmDelete(false)} onConfirm={onClose}/>
        </div>
      </div>
    );
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="pe-shell" onClick={e=>e.stopPropagation()}>
            <header className="pe-head">
              <div style={{ minWidth:0, display:"flex", flexDirection:"column", gap:3 }}>
                <div style={{ display:"flex", alignItems:"center", gap:9, minWidth:0 }}>
                  <Icon name="gear" size={15} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
                  <span className="pe-head-title mono">{form.hostname}</span>
                  <span className="chip chip-info">{form.environment}</span>
                  <span className="chip chip-unknown mono" style={{ fontSize:10 }}>{form.flake}</span>
                </div>
                <span className="pe-head-sub">System registration, flake assignment, deployment policy, and agent identity.</span>
              </div>
              <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={16}/></button>
            </header>

            <nav className="pe-rail">
              {SECTIONS.map(sc => (
                <button key={sc.id} className={`pe-rail-item focus-ring${tab===sc.id?" active":""}`}
                  style={sc.danger && tab!==sc.id ? { color:"#f87171" } : null} onClick={()=>setTab(sc.id)}>
                  <Icon name={sc.icon} size={13}/>
                  <span className="pe-rail-label">{sc.label}</span>
                </button>
              ))}
            </nav>

            <div className="pe-body">
              {tab === "general" && (
              <>
              <div className="pe-sec-head">
                <h3>General</h3>
                <p style={{ margin:0, fontSize:12, color:"var(--cf-text-muted)" }}>Identity, environment, and how the server reaches this host.</p>
              </div>
              <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
                <div className="field">
                  <label>Hostname</label>
                  <input className="input focus-ring mono" value={form.hostname} onChange={e=>set("hostname",e.target.value)}/>
                </div>
                <div className="field" style={{ marginTop:0 }}>
                  <label>Environment</label>
                  <select className="input focus-ring" value={form.environment} onChange={e=>set("environment",e.target.value)}>
                    {["production","staging","dev","edge","lab"].map(e=><option key={e}>{e}</option>)}
                  </select>
                </div>
              </div>
              <div className="field">
                <label>FQDN or address <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· optional</span></label>
                <input className="input focus-ring mono" value={form.fqdn} onChange={e=>set("fqdn",e.target.value)} placeholder="web-server-1.prod.example.com or 10.0.4.12"/>
                <div className="help">
                  {form.reachability === "direct"
                    ? "Where the server reaches this host — a resolvable name or an IP. Leave blank if the hostname alone resolves."
                    : "Recorded for reference only; a pull-only agent connects outbound, so the server never dials this address."}
                </div>
              </div>
              <div className="field">
                <label className="focus-ring" style={{ display:"flex", gap:9, alignItems:"flex-start", cursor:"pointer", margin:0, textTransform:"none", letterSpacing:0 }}>
                  <input type="checkbox" checked={form.reachability === "direct"}
                    onChange={e=>set("reachability", e.target.checked ? "direct" : "pull")}
                    style={{ accentColor:"var(--cf-brand-purple)", marginTop:1 }}/>
                  <span style={{ minWidth:0 }}>
                    <span style={{ display:"block", fontSize:13, fontWeight:600 }}>Reachable by the server directly</span>
                    <span className="help" style={{ display:"block", marginTop:3, fontWeight:400 }}>
                      {form.reachability === "direct"
                        ? "Same LAN / routable / VPN — enables server-initiated deploys and live log tail."
                        : "Off: the agent is behind NAT or a firewall and only reaches out. Deploys apply on its next check-in."}
                    </span>
                  </span>
                </label>
              </div>

              <div style={{ marginTop:8 }}>
                <div className="field" style={{ marginTop:0 }}>
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
              </>
              )}

              {tab === "deployment" && (
              <>
              <div className="pe-sec-head">
                <h3>Deployment</h3>
                <p style={{ margin:0, fontSize:12, color:"var(--cf-text-muted)" }}>Which flake this system tracks, and how it picks up new configuration.</p>
              </div>
              {/* Flake assignment */}
              <div style={{ padding:14, border:"1px solid var(--cf-divider)", borderRadius:10, background:"color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg))" }}>
                <div style={{ display:"flex", alignItems:"center", gap:6, marginBottom:10, fontSize:13, fontWeight:600 }}>
                  <Icon name="git" size={13}/> Flake assignment
                </div>
                <div style={{ display:"grid", gridTemplateColumns:"1fr 1fr", gap:14 }}>
                  <div className="field" style={{ marginTop:0 }}>
                    <label>Flake</label>
                    <select className="input focus-ring" value={form.flake} onChange={e=>set("flake",e.target.value)}>
                      {FLAKES.map(f=><option key={f}>{f}</option>)}
                    </select>
                  </div>
                  <div className="field" style={{ marginTop:0 }}>
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
              <div className="field">
                <label>Heartbeat interval</label>
                <select className="input focus-ring" value={form.heartbeatIntervalSec} onChange={e=>set("heartbeatIntervalSec",parseInt(e.target.value,10))} style={{ width:"fit-content" }}>
                  <option value={30}>30 seconds</option>
                  <option value={60}>1 minute</option>
                  <option value={90}>90 seconds</option>
                  <option value={120}>2 minutes</option>
                  <option value={300}>5 minutes</option>
                </select>
              </div>
              </>
              )}

              {tab === "security" && (
              <>
              <div className="pe-sec-head">
                <h3>Security</h3>
                <p style={{ margin:0, fontSize:12, color:"var(--cf-text-muted)" }}>The Ed25519 key the agent presents on every heartbeat.</p>
              </div>
              <div style={{ marginTop:8, padding:14, border:"1px solid var(--cf-divider)", borderRadius:10, background:"color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg))" }}>
                <div style={{ display:"flex", alignItems:"center", gap:6, marginBottom:10, fontSize:13, fontWeight:600 }}>
                  <Icon name="key" size={13}/> Agent identity
                </div>
                {!rotatingKey ? (
                  <>
                    <div className="field">
                      <label>Current public key fingerprint</label>
                      <div className="mono" style={{ fontSize:12, wordBreak:"break-all", padding:"8px 10px", background:"var(--cf-subtle-bg)", borderRadius:6 }}>
                        {rotated ? newFingerprint : currentFingerprint}
                      </div>
                    </div>
                    {rotated ? (
                      <div className="sd-callout sd-callout-healthy" style={{ marginTop:8 }}>
                        <Icon name="check" size={13}/>
                        <div style={{ fontSize:12 }}>Key rotated. The old key is revoked immediately — the agent will re-register with the new key on its next heartbeat.</div>
                      </div>
                    ) : (
                      <button className="btn btn-ghost focus-ring" onClick={()=>setRotatingKey(true)} style={{ marginTop:4 }}>
                        <Icon name="sync" size={12}/> Rotate key
                      </button>
                    )}
                  </>
                ) : (
                  <>
                    <div className="seg" style={{ width:"fit-content", marginBottom:12 }}>
                      <button className={keyMode==="generate"?"active":""} onClick={()=>{ setKeyMode("generate"); }}>Generate new keypair</button>
                      <button className={keyMode==="paste"?"active":""} onClick={()=>{ setKeyMode("paste"); setGeneratedKeys(null); setNewPubKey(""); }}>Paste existing public key</button>
                    </div>
                    {keyMode === "generate" ? (
                      <div className="field">
                        {!generatedKeys ? (
                          <>
                            <div className="help" style={{ marginTop:0 }}>
                              Generates a new Ed25519 keypair now. The private key is shown once for you to install on the host — Crystal Forge does not keep a copy.
                            </div>
                            <button className="btn btn-ghost focus-ring" onClick={genKeypair} style={{ marginTop:8 }}>
                              <Icon name="key" size={12}/> Generate keypair
                            </button>
                          </>
                        ) : (
                          <>
                            <label>Public key <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· install on the host</span></label>
                            <div className="mono" style={{ fontSize:11, wordBreak:"break-all", padding:"8px 10px", background:"var(--cf-subtle-bg)", borderRadius:6, marginBottom:10 }}>{generatedKeys.pub}</div>
                            <label>Private key <span style={{ color:"#f87171", fontWeight:600 }}>· shown once, copy it now</span></label>
                            <div style={{ position:"relative" }}>
                              <pre className="mono" style={{ margin:0, fontSize:10.5, lineHeight:1.5, whiteSpace:"pre-wrap", wordBreak:"break-all", padding:"8px 10px", background:"var(--cf-subtle-bg)", borderRadius:6, border:"1px solid rgba(248,113,113,0.3)" }}>{generatedKeys.priv}</pre>
                              <button className="btn btn-ghost focus-ring xs" style={{ position:"absolute", top:6, right:6 }}
                                onClick={()=>{ if (navigator.clipboard) navigator.clipboard.writeText(generatedKeys.priv).catch(()=>{}); setPrivCopied(true); setTimeout(()=>setPrivCopied(false),1600); }}>
                                <Icon name={privCopied?"check":"file"} size={11}/> {privCopied ? "Copied" : "Copy"}
                              </button>
                            </div>
                            <div className="help" style={{ marginTop:8 }}>
                              Write the private key to <span className="mono">/var/lib/crystal-forge/host.key</span> on the host before confirming — once rotated, the old key stops being accepted on the agent's next heartbeat.
                            </div>
                          </>
                        )}
                      </div>
                    ) : (
                    <div className="field">
                      <label>New agent public key <span style={{ color:"#f87171" }}>*</span></label>
                      <textarea className="input focus-ring mono" rows={3} value={newPubKey} onChange={e=>setNewPubKey(e.target.value)}
                        placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5… crystal-forge@hostname"
                        style={{ fontSize:11, resize:"vertical" }}/>
                      <div className="help">
                        Generate a new keypair on the host and paste the public half here. The old key is revoked the moment you confirm — the agent must present the new key on its next heartbeat or it will be treated as unrecognized.
                      </div>
                      {newPubKey.trim() && (
                        <div style={{ marginTop:10, padding:"9px 12px", borderRadius:8,
                          border:`1px solid ${newKeyValid ? "rgba(52,211,153,0.3)" : "rgba(248,113,113,0.35)"}`,
                          background: newKeyValid ? "rgba(52,211,153,0.06)" : "rgba(248,113,113,0.06)" }}>
                          {newKeyValid ? (
                            <div style={{ minWidth:0 }}>
                              <div style={{ fontSize:10, textTransform:"uppercase", letterSpacing:"0.06em", color:"var(--cf-text-muted)", fontWeight:600 }}>New fingerprint</div>
                              <div className="mono" style={{ fontSize:11.5, color:"var(--cf-text-primary)", wordBreak:"break-all" }}>{newFingerprint}</div>
                            </div>
                          ) : (
                            <span style={{ fontSize:11.5, color:"#fca5a5" }}>Doesn't look like an SSH public key — expected it to start with <span className="mono">ssh-ed25519</span>.</span>
                          )}
                        </div>
                      )}
                    </div>
                    )}
                    <div style={{ display:"flex", gap:8, marginTop:8 }}>
                      <button className="btn btn-ghost focus-ring" onClick={()=>{ setRotatingKey(false); setNewPubKey(""); setGeneratedKeys(null); }}>Cancel</button>
                      <button className="btn focus-ring" disabled={keyMode==="generate" ? !generatedKeys : !newKeyValid}
                        style={{ background: (keyMode==="generate" ? !!generatedKeys : newKeyValid) ? "#dc2626" : "var(--cf-subtle-bg)", color: (keyMode==="generate" ? !!generatedKeys : newKeyValid) ? "white" : "var(--cf-text-muted)" }}
                        onClick={()=>{ setRotatingKey(false); setRotated(true); }}>
                        <Icon name="key" size={12}/> Revoke old key &amp; rotate
                      </button>
                    </div>
                  </>
                )}
              </div>
              </>
              )}

              {tab === "danger" && (
                <div>
                  <div className="pe-sec-head">
                    <h3>Danger zone</h3>
                    <p style={{ margin:0, fontSize:12, color:"var(--cf-text-muted)" }}>Unregistering stops deploys — the agent keeps running its current generation.</p>
                  </div>
                  <button className="btn btn-ghost focus-ring" onClick={()=>setConfirmDelete(true)} style={{ color:"#f87171", borderColor:"rgba(248,113,113,0.3)" }}>
                    <Icon name="x" size={12}/> Remove system from registry
                  </button>
                </div>
              )}
            </div>
            <footer className="pe-foot">
              <span className="pe-foot-state">
                {form.flake}@{form.branch}
                <span className="pe-foot-dot">·</span>
                {(typeof POLICIES !== "undefined" ? POLICIES : []).find(p => p.id === form.deploymentPolicy)?.name || form.deploymentPolicy}
                <span className="pe-foot-dot">·</span>
                heartbeat {form.heartbeatIntervalSec}s
                {rotated && <><span className="pe-foot-dot">·</span><span style={{ color:"#34d399" }}>key rotated</span></>}
              </span>
              <div style={{ display:"flex", gap:8 }}>
                <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
                <button className="btn btn-primary focus-ring" onClick={onClose}>
                  <Icon name="check" size={13}/> Save changes
                </button>
              </div>
            </footer>
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
