// Niks3 cache destination — form, credential modal, storage helpers.
// Write plane (API + mTLS/token) and read plane (substituter + public/Basic/mTLS) are separate.
// Secrets are never reloaded into the browser: edit shows credential STATE only.

function cacheStorageView(c) {
  const s = c.storage;
  if (!s) return { text: "Unavailable", pct: null, unavailable: true };
  if (s.total) return { text: `${s.used}/${s.total} ${s.unit}`, pct: (s.used / s.total) * 100 };
  return { text: `${s.used} ${s.unit}`, pct: null };
}
function cachePathsText(c) { return c.paths == null ? "Unavailable" : Number(c.paths).toLocaleString(); }
window.cacheStorageView = cacheStorageView; window.cachePathsText = cachePathsText;

const N3_AUTH_LABEL = { token: "API token", mtls: "mTLS", public: "Public", basic: "Basic" };
const N3_PARTS = { token: ["API token"], mtls: ["Client certificate", "Private key", "Server CA bundle"], basic: ["Username", "Password"] };

function N3Result({ state }) {
  const m = { verified: ["chip-healthy", "Verified"], failed: ["chip-critical", "Failed"], untested: ["chip-unknown", "Untested"], running: ["chip-unknown", "Testing…"] }[state] || ["chip-unknown", "Untested"];
  return <span className={`chip ${m[0]}`} style={{ fontSize: 10 }}>{state === "running" && <Spinner size={9}/>}{m[1]}</span>;
}
function N3Plane({ title, rows, onTest, busy, disabledReason }) {
  return (
    <div className="n3-plane">
      <div className="n3-plane-head">
        <strong>{title}</strong>
        <button className="btn btn-ghost focus-ring xs" onClick={onTest} disabled={busy || !!disabledReason} title={disabledReason || ""}>{busy ? <><Spinner size={11}/> Testing…</> : <>Test {title.toLowerCase().includes("write") ? "write API" : "read endpoint"}</>}</button>
      </div>
      {rows.map(r => <div key={r.l} className="n3-plane-row"><span>{r.l}</span><N3Result state={r.s}/></div>)}
    </div>
  );
}

// Compact credential row: state + actions. Large PEM inputs live in N3CredModal.
function N3CredRow({ plane, auth, cred, setCred, original, onOpen }) {
  const hasCurrent = !!(original && original.auth === auth && original.configured);
  const usingCurrent = hasCurrent && cred.use === "current";
  const draftOk = cred.draft && cred.draft.auth === auth;
  if (auth === "public") {
    return (
      <div className="n3-cred">
        <div className="n3-cred-main"><span className="n3-cred-state">No credential needed</span></div>
        {original && original.configured && original.auth !== "public" && <div className="n3-cred-warn"><Icon name="warn" size={11}/> Saving removes the stored {plane} credential ({N3_AUTH_LABEL[original.auth]}).</div>}
      </div>
    );
  }
  return (
    <div className="n3-cred">
      <div className="n3-cred-main">
        <span className="n3-cred-state">
          {draftOk
            ? <><Icon name="key" size={12}/> Replacement entered · not saved <span className="n3-cred-sub">{N3_PARTS[auth].filter((p, i) => auth !== "mtls" || i < 2 || cred.draft.hasCa).join(" · ")}</span></>
            : usingCurrent
              ? <><Icon name="check" size={12} style={{ color: "#34d399" }}/> Current configured credential <span className="n3-cred-sub">stored encrypted · never shown</span></>
              : <><Icon name="warn" size={12} style={{ color: "#fbbf24" }}/> Not configured</>}
        </span>
        <div style={{ display: "flex", gap: 6 }}>
          {hasCurrent && !usingCurrent && <button className="btn btn-ghost focus-ring xs" onClick={() => setCred({ ...cred, use: "current", draft: null })}>Use current credential</button>}
          {draftOk && <button className="btn btn-ghost focus-ring xs" onClick={() => setCred({ ...cred, draft: null, use: hasCurrent ? "current" : "none" })}>Discard</button>}
          <button className="btn btn-ghost focus-ring xs" onClick={onOpen}>{draftOk ? "Edit replacement" : hasCurrent ? "Replace" : "Add credential"}</button>
        </div>
      </div>
      {hasCurrent && draftOk && <div className="help" style={{ marginTop: 6 }}>Test uses the replacement. Cancel discards it; Save persists it.</div>}
    </div>
  );
}

function N3CredModal({ plane, auth, cred, hasCurrent, onClose }) {
  const [f, setF] = React.useState({ token: "", cert: "", key: "", ca: "", user: "", pass: "" });
  const set = (k, v) => setF(p => ({ ...p, [k]: v }));
  const ready = auth === "token" ? !!f.token.trim() : auth === "mtls" ? !!(f.cert.trim() && f.key.trim()) : !!(f.user.trim() && f.pass);
  const ta = { fontSize: 11.5, minHeight: 92, resize: "vertical", whiteSpace: "pre" };
  return (
    <div className="modal-backdrop" onClick={() => onClose(null)} style={{ zIndex: 95 }}>
      <div className="modal" onClick={e => e.stopPropagation()} style={{ width: "min(560px,96vw)" }}>
        <div className="modal-head">
          <h2><Icon name="key" size={14} style={{ marginRight: 6, verticalAlign: "text-bottom" }}/>{hasCurrent ? "Replace" : "Add"} {plane} credential · {N3_AUTH_LABEL[auth]}</h2>
          <p>{hasCurrent ? "The current credential is never shown. Leaving this blank keeps it." : "Secrets are encrypted at rest and not shown again."} Nothing is saved until you save the cache.</p>
        </div>
        <div className="modal-body">
          {auth === "token" && <div className="field"><label>API token</label><input type="password" className="input focus-ring mono" style={{ fontSize: 12 }} value={f.token} onChange={e => set("token", e.target.value)} placeholder="•••••••••••••••••"/><div className="help">Bearer token with push permission on the Write / API URL.</div></div>}
          {auth === "mtls" && (<>
            <div className="field"><label>Client certificate</label><textarea className="input focus-ring mono" style={ta} value={f.cert} onChange={e => set("cert", e.target.value)} placeholder="-----BEGIN CERTIFICATE-----"/></div>
            <div className="field"><label>Private key</label><textarea className="input focus-ring mono" style={ta} value={f.key} onChange={e => set("key", e.target.value)} placeholder="-----BEGIN PRIVATE KEY-----"/></div>
            <div className="field"><label>Server CA bundle <span style={{ fontWeight: 400, color: "var(--cf-text-muted)" }}>(optional)</span></label><textarea className="input focus-ring mono" style={ta} value={f.ca} onChange={e => set("ca", e.target.value)} placeholder="-----BEGIN CERTIFICATE-----"/>
              <div className="help">The server CA bundle is used to verify the remote HTTPS server. It is not the CA the server uses to validate your client certificate. The bundle may contain multiple CA certificates.</div></div>
          </>)}
          {auth === "basic" && (<>
            <div className="field"><label>Username</label><input className="input focus-ring" value={f.user} onChange={e => set("user", e.target.value)} autoComplete="off"/></div>
            <div className="field"><label>Password</label><input type="password" className="input focus-ring" value={f.pass} onChange={e => set("pass", e.target.value)} autoComplete="new-password"/><div className="help">Sent as an Authorization header. Never embedded in the URL.</div></div>
          </>)}
        </div>
        <div className="modal-foot">
          <button className="btn btn-ghost focus-ring" onClick={() => onClose(null)}>Cancel</button>
          <button className="btn btn-primary focus-ring" disabled={!ready} onClick={() => onClose({ auth, hasCa: !!f.ca.trim() })}><Icon name="check" size={13}/> Use for this cache</button>
        </div>
      </div>
    </div>
  );
}

function Niks3FormModal({ mode, cache, form, onType, onEnvs, onClose }) {
  const isEdit = mode === "edit" && cache && cache.type === "niks3";
  const o = isEdit ? cache : null;
  const [name, setName] = React.useState(form.name);
  const [enabled, setEnabled] = React.useState(o ? o.enabled !== false : true);
  const [envs, setEnvs] = React.useState(form.environments);
  const [writeUrl, setWriteUrl] = React.useState(o ? o.writeUrl : "");
  const [writeAuth, setWriteAuth] = React.useState(o ? o.writeAuth : "mtls");
  const [readUrl, setReadUrl] = React.useState(o ? o.url : "");
  const [readAuth, setReadAuth] = React.useState(o ? o.readAuth : "basic");
  const [keys, setKeys] = React.useState(o ? [...o.signingKeys] : [""]);
  const [adv, setAdv] = React.useState({ parallel: o?.parallel ?? 8, retries: o?.retries ?? 3, timeout: o?.timeout ?? 600, requireSig: o?.requireSig ?? true });
  const origW = o ? { auth: o.writeAuth, configured: o.writeCredConfigured } : null;
  const origR = o ? { auth: o.readAuth, configured: o.readCredConfigured } : null;
  const [wc, setWc] = React.useState({ use: o && o.writeCredConfigured ? "current" : "none", draft: null });
  const [rc, setRc] = React.useState({ use: o && o.readCredConfigured ? "current" : "none", draft: null });
  const [credModal, setCredModal] = React.useState(null);
  const [disc, setDisc] = React.useState("idle");
  const [discNote, setDiscNote] = React.useState("");
  const [wt, setWt] = React.useState({ api: "untested", auth: "untested", authz: "untested" });
  const [rt, setRt] = React.useState({ access: "untested", keys: "untested" });
  const [busy, setBusy] = React.useState({ w: false, r: false });
  const [section, setSection] = React.useState("dest");

  const credOk = (auth, c, orig) => auth === "public" || (orig && orig.auth === auth && orig.configured && c.use === "current") || (c.draft && c.draft.auth === auth);
  const wOk = credOk(writeAuth, wc, origW), rOk = credOk(readAuth, rc, origR);
  const httpsOk = (u) => /^https:\/\/[^\s/@]+(\/\S*)?$/.test(u.trim());
  const keyList = keys.map(k => k.trim()).filter(Boolean);
  const keyBad = (k) => k.trim() && !/^[^\s:]+:[A-Za-z0-9+/=]{20,}$/.test(k.trim());

  React.useEffect(() => { setWt({ api: "untested", auth: "untested", authz: "untested" }); setDisc("idle"); setDiscNote(""); }, [writeUrl, writeAuth, wc.use, wc.draft]);
  React.useEffect(() => { setRt({ access: "untested", keys: "untested" }); }, [readUrl, readAuth, rc.use, rc.draft, keyList.join("|")]);

  const discover = () => {
    setDisc("running"); setDiscNote("");
    setTimeout(() => {
      const u = writeUrl.toLowerCase(); let r = "ok";
      if (!httpsOk(writeUrl) || /down|unreach/.test(u)) r = "unreachable";
      else if (!wOk) r = "auth";
      else if (/bad|invalid/.test(u)) r = "invalid";
      setDisc(r);
      if (r === "ok") {
        const host = writeUrl.trim().replace(/^https:\/\//, "").replace(/\/.*$/, "");
        const pub = host.replace(/^(push|write|api)\./, "");
        const filled = [];
        if (!readUrl.trim()) { setReadUrl(`https://${pub}`); filled.push("Read / substituter URL"); }
        if (!keyList.length) { setKeys([`${pub}-1:Zm9yLWRlbW8tb25seS1wdWJsaWMta2V5LXBsYWNlaG9sZGVy`]); filled.push("1 signing key"); }
        setDiscNote(filled.length ? `Filled ${filled.join(" and ")}. Review it, then save. Nothing has been saved.` : "Public metadata matches what you entered. Nothing has been saved.");
      }
    }, 900);
  };
  const testWrite = () => {
    setBusy(b => ({ ...b, w: true })); setWt({ api: "running", auth: "running", authz: "running" });
    setTimeout(() => {
      const reach = httpsOk(writeUrl) && !/down|unreach/.test(writeUrl.toLowerCase());
      setWt(!reach ? { api: "failed", auth: "untested", authz: "untested" }
        : !wOk ? { api: "verified", auth: "failed", authz: "untested" }
        : { api: "verified", auth: "verified", authz: "verified" });
      setBusy(b => ({ ...b, w: false }));
    }, 900);
  };
  const testRead = () => {
    setBusy(b => ({ ...b, r: true })); setRt({ access: "running", keys: "running" });
    setTimeout(() => {
      const reach = httpsOk(readUrl) && !/down|unreach/.test(readUrl.toLowerCase());
      setRt({ access: reach && rOk ? "verified" : "failed", keys: !reach || !rOk ? "untested" : keyList.length && !keyList.some(keyBad) ? "verified" : "failed" });
      setBusy(b => ({ ...b, r: false }));
    }, 900);
  };

  const problems = [];
  if (!name.trim()) problems.push("name");
  if (!httpsOk(writeUrl)) problems.push("Write / API URL");
  if (!wOk) problems.push("write credential");
  if (!httpsOk(readUrl)) problems.push("Read / substituter URL");
  if (!rOk) problems.push("read credential");
  if (!keyList.length || keyList.some(keyBad)) problems.push("signing key");
  const toggleEnv = (e) => setEnvs(es => es.includes(e) ? es.filter(x => x !== e) : [...es, e]);
  const discMsg = { running: ["Discovering…", null], ok: ["Discovery successful", "ok"], unreachable: ["Unable to reach API", "bad"], auth: ["Authentication failed", "bad"], invalid: ["Invalid Niks3 metadata", "bad"] }[disc];
  const sections = [
    { id: "dest", label: "Destination", icon: "download", badge: !name.trim() ? "!" : null, warn: !name.trim() },
    { id: "write", label: "Write / API", icon: "upload", badge: !httpsOk(writeUrl) || !wOk ? "!" : N3_AUTH_LABEL[writeAuth], warn: !httpsOk(writeUrl) || !wOk },
    { id: "read", label: "Read / Pull", icon: "link", badge: !httpsOk(readUrl) || !rOk ? "!" : N3_AUTH_LABEL[readAuth], warn: !httpsOk(readUrl) || !rOk },
    { id: "trust", label: "Trust", icon: "shield", badge: keyList.length || "!", warn: !keyList.length },
    { id: "adv", label: "Advanced", icon: "gear" },
  ];
  const AuthSeg = ({ value, set, opts }) => <div className="seg" role="radiogroup" style={{ width: "fit-content" }}>{opts.map(v => <button key={v} className={value === v ? "active" : ""} onClick={() => set(v)}>{N3_AUTH_LABEL[v]}</button>)}</div>;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="pe-shell" onClick={e => e.stopPropagation()}>
        <header className="pe-head">
          <div style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 9, minWidth: 0 }}>
              <Icon name={isEdit ? "gear" : "plus"} size={15} style={{ color: "var(--cf-brand-purple)", flexShrink: 0 }}/>
              <span className="pe-head-title">{name || "Add cache destination"}</span>
              <span className="chip chip-info">Niks3</span>
              {!enabled && <span className="chip chip-unknown">disabled</span>}
            </div>
            <span className="pe-head-sub">{isEdit ? "Update Niks3 destination." : "Register a Niks3 destination. Write and read are configured separately."}</span>
          </div>
          <button className="btn-icon focus-ring" onClick={onClose} aria-label="Close"><Icon name="x" size={16}/></button>
        </header>
        <nav className="pe-rail">
          {sections.map(s => (
            <button key={s.id} className={`pe-rail-item focus-ring${section === s.id ? " active" : ""}`} onClick={() => setSection(s.id)}>
              <Icon name={s.icon} size={13}/><span className="pe-rail-label">{s.label}</span>
              {s.badge != null && <span className={`pe-rail-badge${s.warn ? " warn" : ""}`}>{s.badge}</span>}
            </button>
          ))}
        </nav>
        <div className="pe-body">
          {section === "dest" && (<>
            <div className="pe-sec-head"><h3>Destination</h3><p>Name, type and which environments push here.</p></div>
            <div className="field"><label>Name</label><input className="input focus-ring" value={name} onChange={e => setName(e.target.value)} placeholder="e.g. campground-niks3"/></div>
            <div className="field"><label>Cache type</label>
              <div className="seg" style={{ width: "fit-content", flexWrap: "wrap" }}>
                {[["s3", "S3-compatible"], ["attic", "Attic"], ["nix", "Nix HTTPS"], ["niks3", "Niks3"]].map(([v, l]) => <button key={v} className={v === "niks3" ? "active" : ""} onClick={() => v !== "niks3" && onType(v, { name, environments: envs })}>{l}</button>)}
              </div>
              <div className="help">Niks3 publishes through its own API and serves reads through a read proxy. You never enter the underlying S3 or Garage credentials.</div>
            </div>
            <div className="field"><label>Environment scope</label>
              <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                {ENVIRONMENTS.map(env => { const on = envs.includes(env.name); return (
                  <button key={env.name} className="focus-ring" onClick={() => toggleEnv(env.name)} style={{ padding: "6px 12px", borderRadius: 99, fontSize: 12, fontWeight: 600, border: `1px solid ${on ? env.color : "var(--cf-card-border)"}`, background: on ? `color-mix(in oklab, ${env.color} 14%, var(--cf-card-bg))` : "transparent", color: on ? "var(--cf-text-primary)" : "var(--cf-text-muted)", cursor: "pointer", display: "inline-flex", alignItems: "center", gap: 7, fontFamily: "inherit" }}>
                    <span style={{ width: 8, height: 8, borderRadius: "50%", background: env.color }}/>{env.name}{on && <Icon name="check" size={11}/>}
                  </button>); })}
              </div>
              {envs.length === 0 && <div className="help">Unassigned. Nothing is pushed here until an environment is selected.</div>}
            </div>
            <div className="field"><label className="focus-ring" style={{ display: "flex", gap: 9, cursor: "pointer", margin: 0, textTransform: "none", letterSpacing: 0 }}><input type="checkbox" checked={enabled} onChange={e => setEnabled(e.target.checked)} style={{ accentColor: "var(--cf-brand-purple)" }}/><span style={{ fontSize: 13, fontWeight: 600 }}>Enabled</span></label></div>
          </>)}

          {section === "write" && (<>
            <div className="pe-sec-head"><h3>Write / API</h3><p>Where Crystal Forge's Niks3 client publishes store paths. Niks3 hands the pusher presigned upload URLs.</p></div>
            <div className="field"><label>Write / API URL</label>
              <div style={{ display: "flex", gap: 8 }}>
                <input className="input focus-ring mono" style={{ fontSize: 12, flex: 1 }} value={writeUrl} onChange={e => setWriteUrl(e.target.value)} placeholder="https://push.niks3.example.com"/>
                <button className="btn btn-ghost focus-ring xs" onClick={discover} disabled={disc === "running" || !writeUrl.trim()} title="Reads public Niks3 metadata using the write credential selected below. Does not save.">{disc === "running" ? <><Spinner size={11}/> Discovering…</> : <><Icon name="sync" size={11}/> Discover</>}</button>
              </div>
              {discMsg && disc !== "running" && <div className={`n3-disc ${discMsg[1]}`}><Icon name={discMsg[1] === "ok" ? "check" : "warn"} size={12}/><span><strong>{discMsg[0]}.</strong> {discMsg[1] === "ok" ? discNote : disc === "auth" ? "Add or check the write credential below, then retry. Discover uses the draft credential before saving." : disc === "invalid" ? "The endpoint answered, but not with Niks3 cache metadata." : "Check the URL and network path."}</span></div>}
              {disc === "idle" && <div className="help">Discover fills the Read / substituter URL and signing keys from public Niks3 metadata. It uses the write credential below and does not save the cache.</div>}
            </div>
            <div className="field"><label>Write authentication</label>
              <AuthSeg value={writeAuth} set={setWriteAuth} opts={["token", "mtls"]}/>
              <div style={{ marginTop: 8 }}><N3CredRow plane="write" auth={writeAuth} cred={wc} setCred={setWc} original={origW} onOpen={() => setCredModal("write")}/></div>
            </div>
            <N3Plane title="Write API" busy={busy.w} onTest={testWrite} disabledReason={!httpsOk(writeUrl) ? "Enter a Write / API URL first" : ""}
              rows={[{ l: "API reachable", s: wt.api }, { l: "Authentication", s: wt.auth }, { l: "Write authorization", s: wt.authz }]}/>
            <div className="help">Reachability and TLS alone do not prove write permission. Each line is reported on its own.</div>
          </>)}

          {section === "read" && (<>
            <div className="pe-sec-head"><h3>Read / Pull</h3><p>Where Nix clients substitute from: Nix client → Niks3 read proxy → S3. It can be a different hostname from the write API.</p></div>
            <div className="field"><label>Read / substituter URL</label><input className="input focus-ring mono" style={{ fontSize: 12 }} value={readUrl} onChange={e => setReadUrl(e.target.value)} placeholder="https://niks3.example.com"/><div className="help">Don't put a username or password in the URL.</div></div>
            <div className="field"><label>Read authentication</label>
              <AuthSeg value={readAuth} set={setReadAuth} opts={["public", "basic", "mtls"]}/>
              <div style={{ marginTop: 8 }}><N3CredRow plane="read" auth={readAuth} cred={rc} setCred={setRc} original={origR} onOpen={() => setCredModal("read")}/></div>
              {writeAuth === "mtls" && readAuth !== "basic" && <div className="help">Production topology: write with mTLS, read with Basic username/password.</div>}
            </div>
            <N3Plane title="Read endpoint" busy={busy.r} onTest={testRead} disabledReason={!httpsOk(readUrl) ? "Enter a Read / substituter URL first" : ""}
              rows={[{ l: "Read access", s: rt.access }, { l: "Signing keys", s: rt.keys }]}/>
          </>)}

          {section === "trust" && (<>
            <div className="pe-sec-head"><h3>Trust</h3><p>Nix verifies every path against these keys. This is independent of how you authenticate over the network.</p></div>
            <div className="field"><label>Trusted signing public keys</label>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {keys.map((k, i) => (
                  <div key={i} style={{ display: "flex", gap: 6 }}>
                    <input className="input focus-ring mono" style={{ fontSize: 11.5, flex: 1, borderColor: keyBad(k) ? "#f87171" : undefined }} value={k} onChange={e => setKeys(ks => ks.map((x, j) => j === i ? e.target.value : x))} placeholder="cache.example.com-1:base64-ed25519-public-key"/>
                    <button className="btn-icon focus-ring" aria-label="Remove key" disabled={keys.length === 1 && !k} onClick={() => setKeys(ks => ks.length > 1 ? ks.filter((_, j) => j !== i) : [""])}><Icon name="x" size={13}/></button>
                  </div>
                ))}
              </div>
              {keys.some(keyBad) && <div className="help" style={{ color: "#f87171" }}>Keys look like <span className="mono">name:base64</span>.</div>}
              <button className="btn btn-ghost focus-ring xs" style={{ marginTop: 8 }} onClick={() => setKeys(ks => [...ks, ""])}><Icon name="plus" size={11}/> Add key</button>
              <div className="help">Basic auth or mTLS secures the connection. It does not replace signature verification. Add the old key as well during a rotation.</div>
            </div>
          </>)}

          {section === "adv" && (<>
            <div className="pe-sec-head"><h3>Advanced</h3><p>Defaults suit most installs.</p></div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0,1fr))", gap: 12, alignItems: "start" }}>
              {[["parallel", "Parallel uploads", 1, 64, "concurrent"], ["retries", "Retry attempts", 0, 10, "per path"], ["timeout", "Push timeout", 30, 7200, "seconds"]].map(([k, l, mn, mx, u]) => (
                <div className="field" key={k} style={{ margin: 0, minWidth: 0 }}><label style={{ whiteSpace: "nowrap" }}>{l}</label><input type="number" min={mn} max={mx} className="input focus-ring" style={{ width: "100%" }} value={adv[k]} onChange={e => setAdv(a => ({ ...a, [k]: +e.target.value }))}/><div className="help">{u}</div></div>
              ))}
            </div>
            <div className="field"><label className="focus-ring" style={{ display: "flex", gap: 9, alignItems: "flex-start", cursor: "pointer", margin: 0, textTransform: "none", letterSpacing: 0 }}>
              <input type="checkbox" checked={adv.requireSig} onChange={e => setAdv(a => ({ ...a, requireSig: e.target.checked }))} style={{ accentColor: "var(--cf-brand-purple)", marginTop: 1 }}/>
              <span><span style={{ display: "block", fontSize: 13, fontWeight: 600 }}>Require signatures</span><span className="help" style={{ display: "block", marginTop: 3, fontWeight: 400 }}>Only pull paths signed by a trusted key above.</span></span></label></div>
          </>)}
        </div>
        <footer className="pe-foot">
          <span className="pe-foot-state">
            {problems.length ? <span style={{ color: "#fbbf24" }}>Needs {problems.slice(0, 3).join(", ")}{problems.length > 3 ? ` +${problems.length - 3}` : ""}</span> : <>{name.trim()}<span className="pe-foot-dot">·</span>write {N3_AUTH_LABEL[writeAuth]}<span className="pe-foot-dot">·</span>read {N3_AUTH_LABEL[readAuth]}<span className="pe-foot-dot">·</span>{envs.length} env{envs.length === 1 ? "" : "s"}</>}
          </span>
          <div style={{ display: "flex", gap: 8 }}>
            <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
            <button className="btn btn-primary focus-ring" disabled={problems.length > 0} onClick={onClose}><Icon name="check" size={13}/> {isEdit ? "Save changes" : "Add cache"}</button>
          </div>
        </footer>
      </div>
      {credModal && <N3CredModal plane={credModal} auth={credModal === "write" ? writeAuth : readAuth}
        hasCurrent={credModal === "write" ? !!(origW && origW.auth === writeAuth && origW.configured) : !!(origR && origR.auth === readAuth && origR.configured)}
        onClose={(d) => { const w = credModal === "write"; if (d) (w ? setWc : setRc)({ use: "draft", draft: d }); setCredModal(null); }}/>}
    </div>
  );
}

// Detail-drawer block. Credential STATE only.
function Niks3Details({ cache }) {
  const st = (ok) => <span className={`chip ${ok ? "chip-healthy" : "chip-unknown"}`} style={{ fontSize: 10 }}>{ok ? "Configured" : "Not configured"}</span>;
  return (
    <>
      <section className="panel-section">
        <h3>Write / API</h3>
        <dl className="kv-grid">
          <dt>URL</dt><dd className="mono" style={{ overflowWrap: "anywhere" }}>{cache.writeUrl}</dd>
          <dt>Authentication</dt><dd>{N3_AUTH_LABEL[cache.writeAuth]} {st(cache.writeCredConfigured)}</dd>
        </dl>
      </section>
      <section className="panel-section">
        <h3>Read / pull</h3>
        <dl className="kv-grid">
          <dt>Substituter URL</dt><dd className="mono" style={{ overflowWrap: "anywhere" }}>{cache.url}</dd>
          <dt>Authentication</dt><dd>{N3_AUTH_LABEL[cache.readAuth]} {cache.readAuth !== "public" && st(cache.readCredConfigured)}</dd>
        </dl>
      </section>
      <section className="panel-section">
        <h3>Trusted signing public keys ({cache.signingKeys.length})</h3>
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>{cache.signingKeys.map(k => <div key={k} className="mono" style={{ fontSize: 11, overflowWrap: "anywhere", color: "var(--cf-text-secondary)" }}>{k}</div>)}</div>
      </section>
    </>
  );
}
Object.assign(window, { Niks3FormModal, Niks3Details, cacheStorageView, cachePathsText });
