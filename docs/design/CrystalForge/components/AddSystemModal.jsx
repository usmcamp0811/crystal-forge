// Register System modal — opens from the "Add system" button on the Systems view.
// Step 5 of onboarding; on success shows the Step 6 "deploy the agent" next-steps panel.

function AddSystemModal({ onClose, coach, prefill }) {
  const envList = (typeof ENVIRONMENTS !== "undefined" && ENVIRONMENTS.length)
    ? ENVIRONMENTS.map(e => e.name) : ["production", "staging", "dev", "edge", "lab"];
  const flakeList = (typeof FLAKES !== "undefined" && FLAKES.length) ? FLAKES : ["infrastructure"];

  const [form, setForm] = React.useState({
    hostname: "",
    fqdn: "",
    environment: envList[0],
    flake: flakeList[0],
    branch: "main",
    deploymentPolicy: "inherit",
    reachability: "direct",
    publicKey: "",
    tags: "",
    // Prefill when the host is already declared in a flake and only needs
    // registering (e.g. opened from the flake explorer's Systems tab).
    ...(prefill || {}),
  });
  const [phase, setPhase] = React.useState("form"); // form | registered
  const set = (k, v) => setForm(p => ({ ...p, [k]: v }));

  // Derive a fingerprint from the pasted public key (mock of ssh-keygen -lf).
  const fingerprint = React.useMemo(() => {
    const key = (form.publicKey || "").trim();
    if (!key || key.length < 20) return null;
    const b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let seed = 0x811c9dc5;
    for (let i = 0; i < key.length; i++) { seed ^= key.charCodeAt(i); seed = Math.imul(seed, 0x01000193) >>> 0; }
    let s = ""; for (let i = 0; i < 43; i++) { seed = (Math.imul(seed, 1103515245) + 12345) >>> 0; s += b64[(seed >>> 24) % 64]; }
    return `SHA256:${s}`;
  }, [form.publicKey]);
  const keyValid = /^ssh-(ed25519|rsa|ecdsa)/.test((form.publicKey || "").trim());
  const canRegister = form.hostname.trim().length > 0 && keyValid;

  const register = () => {
    if (!canRegister) return;
    if (coach) coach.complete("system"); // ticks step 5, unlocks step 6 (deploy agent)
    setPhase("registered");
  };

  const [section, setSection] = React.useState("host");
  const sections = [
    { id:"host",   label:"Host & environment", icon:"server" },
    { id:"agent",  label:"Agent identity",     icon:"key" },
    { id:"flake",  label:"Flake assignment",   icon:"git" },
    { id:"policy", label:"Policy & tags",      icon:"shield" },
  ];

  if (phase === "registered") {
    return (
      <div className="modal-backdrop" onClick={onClose}>
        <div className="modal" onClick={e => e.stopPropagation()} style={{ width: "min(620px,96vw)", maxHeight: "92vh" }}>
          <AgentDeploySteps form={form} fingerprint={fingerprint} onClose={onClose} />
        </div>
      </div>
    );
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="pe-shell" onClick={e => e.stopPropagation()}>
        <header className="pe-head">
          <div style={{ minWidth:0, display:"flex", flexDirection:"column", gap:3 }}>
            <div style={{ display:"flex", alignItems:"center", gap:9, minWidth:0 }}>
              <Icon name="plus" size={15} style={{ color:"var(--cf-brand-purple)", flexShrink:0 }}/>
              <span className="pe-head-title">{form.hostname || "Register system"}</span>
              <span className="chip chip-info">{form.environment}</span>
              <span className="chip chip-unknown mono" style={{ fontSize:10 }}>{form.flake} · {form.branch}</span>
              {!keyValid && <span className="chip" title="An agent public key is required to register.">No agent key</span>}
            </div>
            <span className="pe-head-sub">Add a NixOS host to the fleet and connect it to an environment and flake.</span>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={16}/></button>
        </header>

        <nav className="pe-rail">
          {sections.map(s => (
            <button key={s.id} className={`pe-rail-item focus-ring${section===s.id?" active":""}`} onClick={()=>setSection(s.id)}>
              <Icon name={s.icon} size={13}/>
              <span className="pe-rail-label">{s.label}</span>
              {s.id === "host" && !form.hostname.trim() && <span className="pe-rail-badge warn">!</span>}
              {s.id === "agent" && <span className={`pe-rail-badge${keyValid?"":" warn"}`}>{keyValid ? "ok" : "!"}</span>}
              {s.id === "policy" && form.tags.trim() && <span className="pe-rail-badge">{form.tags.split(",").filter(x=>x.trim()).length}</span>}
            </button>
          ))}
        </nav>

        <div className="pe-body">
          {section === "host" && (
            <>
              <div className="pe-sec-head">
                <h3>Host &amp; environment</h3>
                <p>What the host is called, which tier it belongs to, and how the server reaches it.</p>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
                <div className="field">
                  <label>Hostname <span style={{ color: "#f87171" }}>*</span></label>
                  <input className="input focus-ring mono" value={form.hostname} onChange={e => set("hostname", e.target.value)} placeholder="web-server-1" autoFocus />
                </div>
                <div className="field" style={{ marginTop: 0 }}>
                  <label>Environment</label>
                  <select className="input focus-ring" value={form.environment} onChange={e => set("environment", e.target.value)}>
                    {envList.map(e => <option key={e}>{e}</option>)}
                  </select>
                </div>
              </div>
              <div className="field">
                <label>FQDN or address <span style={{ color: "var(--cf-text-muted)", fontWeight: 400 }}>· optional</span></label>
                <input className="input focus-ring mono" value={form.fqdn} onChange={e => set("fqdn", e.target.value)} placeholder="web-server-1.prod.example.com or 10.0.4.12" />
                <div className="help">
                  {form.reachability === "direct"
                    ? "Where the server reaches this host — a resolvable name or an IP. Leave blank if the hostname alone resolves."
                    : "Recorded for reference only; a pull-only agent connects outbound, so the server never dials this address."}
                </div>
              </div>
              <div className="field">
                <label className="focus-ring" style={{ display: "flex", gap: 9, alignItems: "flex-start", cursor: "pointer", margin: 0, textTransform: "none", letterSpacing: 0 }}>
                  <input type="checkbox" checked={form.reachability === "direct"}
                    onChange={e => set("reachability", e.target.checked ? "direct" : "pull")}
                    style={{ accentColor: "var(--cf-brand-purple)", marginTop: 1 }} />
                  <span style={{ minWidth: 0 }}>
                    <span style={{ display: "block", fontSize: 13, fontWeight: 600 }}>Reachable by the server directly</span>
                    <span className="help" style={{ display: "block", marginTop: 3, fontWeight: 400 }}>
                      {form.reachability === "direct"
                        ? "Same LAN / routable / VPN — enables server-initiated deploys and live log tail."
                        : "Off: the agent is behind NAT or a firewall and only reaches out. Deploys apply on its next check-in."}
                    </span>
                  </span>
                </label>
              </div>
            </>
          )}

          {section === "agent" && (
            <>
              <div className="pe-sec-head">
                <h3>Agent identity</h3>
                <p>The agent generates its own keypair on first start. Paste the public half to register — the private key never leaves the host.</p>
              </div>
              <div className="field">
                <label>Agent public key <span style={{ color: "#f87171" }}>*</span></label>
                <textarea className="input focus-ring mono" rows={4} value={form.publicKey} onChange={e => set("publicKey", e.target.value)}
                  placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5… crystal-forge@hostname"
                  style={{ fontSize: 11, resize: "vertical" }} />
                <div className="help">
                  Grab it from the host: <span className="mono" style={{ fontSize: 10.5 }}>cat /var/lib/crystal-forge/host.key.pub</span>
                </div>
              </div>
              {form.publicKey.trim() && (
                <div className={`sd-callout ${keyValid ? "sd-callout-ok" : "sd-callout-danger"}`}>
                  <Icon name={keyValid ? "key" : "warn"} size={13} style={{ color: keyValid ? "#34d399" : "#f87171", flexShrink: 0 }} />
                  {keyValid ? (
                    <div style={{ minWidth: 0 }}>
                      <div style={{ fontSize: 10, textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--cf-text-muted)", fontWeight: 600 }}>Fingerprint</div>
                      <div className="mono" style={{ fontSize: 11.5, color: "var(--cf-text-primary)", wordBreak: "break-all" }}>{fingerprint}</div>
                    </div>
                  ) : (
                    <div style={{ fontSize: 11.5 }}>Doesn't look like an SSH public key — expected it to start with <span className="mono">ssh-ed25519</span>.</div>
                  )}
                </div>
              )}
            </>
          )}

          {section === "flake" && (
            <>
              <div className="pe-sec-head">
                <h3>Flake assignment</h3>
                <p>Which flake and branch declare this host's configuration.</p>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
                <div className="field">
                  <label>Flake</label>
                  <select className="input focus-ring" value={form.flake} onChange={e => set("flake", e.target.value)}>
                    {flakeList.map(fl => <option key={fl}>{fl}</option>)}
                  </select>
                </div>
                <div className="field" style={{ marginTop: 0 }}>
                  <label>Branch</label>
                  <input className="input focus-ring mono" value={form.branch} onChange={e => set("branch", e.target.value)} />
                </div>
              </div>
              <div className="help">
                Crystal Forge looks for <span className="mono">nixosConfigurations.{form.hostname || "<hostname>"}</span> in this flake.
              </div>
            </>
          )}

          {section === "policy" && (
            <>
              <div className="pe-sec-head">
                <h3>Policy &amp; tags</h3>
                <p>How deploys reach this host, and the labels used to group and filter it.</p>
              </div>
              <div className="field">
                <label>Deployment policy</label>
                <div className="seg" style={{ width: "fit-content", flexWrap: "wrap" }}>
                  {["inherit", "manual", "auto_latest", "pinned"].map(p => (
                    <button key={p} className={form.deploymentPolicy === p ? "active" : ""} onClick={() => set("deploymentPolicy", p)}>{p}</button>
                  ))}
                </div>
                <div className="help">
                  {form.deploymentPolicy === "inherit" && <>Use the default policy of the <span className="mono">{form.environment}</span> environment.</>}
                  {form.deploymentPolicy === "manual" && "Operator must explicitly approve every deploy."}
                  {form.deploymentPolicy === "auto_latest" && "Auto-deploy the newest passing commit on the assigned flake/branch."}
                  {form.deploymentPolicy === "pinned" && "Stay on a chosen commit until manually changed (set the pin from System Detail after registering)."}
                </div>
              </div>
              <div className="field">
                <label>Tags <span style={{ color: "var(--cf-text-muted)", fontWeight: 400 }}>· optional</span></label>
                <input className="input focus-ring" value={form.tags} onChange={e => set("tags", e.target.value)} placeholder="e.g. web, stig-enforced" />
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
            </>
          )}
        </div>

        <footer className="pe-foot">
          <span className="pe-foot-state">
            {form.hostname.trim() ? form.hostname : "Unnamed host"}
            <span className="pe-foot-dot">·</span>
            {form.environment}
            <span className="pe-foot-dot">·</span>
            {keyValid ? "agent key valid" : "agent key required"}
          </span>
          <div style={{ display:"flex", gap:8 }}>
            <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
            <button className="btn btn-primary focus-ring" onClick={register} disabled={!canRegister}>
              <Icon name="check" size={13} /> Register system
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}

// Step 6 — shown after a system is registered.
function AgentDeploySteps({ form, fingerprint, onClose }) {
  const [copied, setCopied] = React.useState(false);
  const snippet = `services.crystal-forge.client = {
  enable = true;
  server_host = "crystal-forge.example.com";
  server_port = 3000;
  # The keypair the agent generated on first start
  private_key = "/var/lib/crystal-forge/host.key";
};`;
  const copy = () => {
    if (navigator.clipboard) navigator.clipboard.writeText(snippet).catch(() => {});
    setCopied(true); setTimeout(() => setCopied(false), 1600);
  };
  return (
    <>
      <div className="modal-head" style={{ background: "rgba(52,211,153,0.06)" }}>
        <h2 style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <Icon name="check" size={16} style={{ color: "#34d399" }} />
          {form.hostname} registered
        </h2>
        <p>One step left — bring the agent online so it reports in.</p>
      </div>
      <div className="modal-body" style={{ overflowY: "auto" }}>
        <dl className="kv-grid" style={{ marginBottom: 14 }}>
          <dt>Environment</dt><dd>{form.environment}</dd>
          <dt>Flake · branch</dt><dd className="mono">{form.flake} · {form.branch}</dd>
          <dt>Fingerprint</dt><dd className="mono" style={{ wordBreak: "break-all" }}>{fingerprint}</dd>
        </dl>

        <div className="sd-callout sd-callout-info" style={{ display: "block", fontSize: 12 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 8, fontWeight: 600, color: "var(--cf-text-secondary)" }}>
            <Icon name="server" size={13} /> Deploy the agent
          </div>
          <ol style={{ margin: 0, paddingLeft: 18, lineHeight: 1.7, color: "var(--cf-text-secondary)" }}>
            <li>Add the agent module to the host's NixOS config:</li>
          </ol>
          <div style={{ position: "relative", marginTop: 8 }}>
            <button className="btn btn-ghost focus-ring xs" onClick={copy} style={{ position: "absolute", top: 6, right: 6 }}>
              <Icon name={copied ? "check" : "file"} size={11} /> {copied ? "Copied" : "Copy"}
            </button>
            <pre className="mono" style={{ margin: 0, fontSize: 11, lineHeight: 1.5, color: "var(--cf-text-secondary)", background: "var(--cf-page-bg)", border: "1px solid var(--cf-divider)", borderRadius: 8, padding: "10px 12px", overflow: "auto", whiteSpace: "pre" }}>{snippet}</pre>
          </div>
          <ol start={2} style={{ margin: "8px 0 0", paddingLeft: 18, lineHeight: 1.7, color: "var(--cf-text-secondary)" }}>
            <li>Apply on the target host: <span className="mono">sudo nixos-rebuild switch</span></li>
            <li>The agent connects and sends its first signed heartbeat — this system flips to <span className="chip chip-healthy" style={{ fontSize: 10 }}>online</span> and the onboarding completes automatically.</li>
          </ol>
        </div>
        <div className="help" style={{ marginTop: 10 }}>
          Until the agent checks in, the system shows as <span className="chip chip-unknown" style={{ fontSize: 10 }}>pending</span> in the registry.
        </div>
      </div>
      <div className="modal-foot">
        <button className="btn btn-primary focus-ring" onClick={onClose}>
          <Icon name="check" size={13} /> Done
        </button>
      </div>
    </>
  );
}

Object.assign(window, { AddSystemModal });
