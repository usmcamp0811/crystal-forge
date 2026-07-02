// Profile view — user info + preferences (theme, density, layout, notifications)

function ProfileView({ prefs }) {
  const { theme, onTheme, density, onDensity, defaultView, onDefaultView, sidebarMode, onSidebarMode } = prefs;

  const user = {
    name: "Mira Reyes", email: "mreyes@acme.io", role: "admin",
    org: "acme-prod", source: "oidc", groups: ["cf-admins"],
    envs: ["all"], mfa: true, lastLogin: "2m ago", joined: "Jan 2026",
  };

  const [notif, setNotif] = React.useState({
    deployFailed: true, buildFailed: true, criticalCve: true,
    policyFail: true, heartbeatLost: false, weeklyDigest: true,
    channel: "in-app",
  });
  const setN = (k, v) => setNotif(p => ({ ...p, [k]: v }));

  const Seg = ({ value, onChange, opts }) => (
    <div className="seg" style={{ width: "fit-content" }}>
      {opts.map(o => (
        <button key={o.v} className={value === o.v ? "active" : ""} onClick={() => onChange(o.v)}>{o.l}</button>
      ))}
    </div>
  );

  const PrefRow = ({ title, desc, children }) => (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 16, padding: "14px 0", borderBottom: "1px solid var(--cf-divider)" }}>
      <div style={{ minWidth: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 600 }}>{title}</div>
        {desc && <div style={{ fontSize: 11, color: "var(--cf-text-muted)", marginTop: 2 }}>{desc}</div>}
      </div>
      <div style={{ flexShrink: 0 }}>{children}</div>
    </div>
  );

  const Toggle = ({ on, onChange }) => (
    <label style={{ display: "inline-flex", cursor: "pointer" }}>
      <input type="checkbox" checked={on} onChange={e => onChange(e.target.checked)} style={{ accentColor: "var(--cf-brand-purple)", width: 16, height: 16 }} />
    </label>
  );

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">Profile & Preferences</h1>
          <p className="page-subtitle">Personal settings for your Crystal Forge account</p>
        </div>
      </div>

      {/* Identity card */}
      <div className="card" style={{ padding: 20, display: "flex", gap: 18, alignItems: "center", flexWrap: "wrap" }}>
        <div style={{ width: 64, height: 64, borderRadius: 99, background: "linear-gradient(135deg,#f472b6,#6366f1)", display: "grid", placeItems: "center", color: "#fff", fontSize: 22, fontWeight: 700, flexShrink: 0 }}>MR</div>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ fontSize: 18, fontWeight: 700, display: "flex", alignItems: "center", gap: 10 }}>
            {user.name}
            <span className="chip chip-critical" style={{ fontSize: 10 }}>{user.role}</span>
          </div>
          <div className="mono" style={{ fontSize: 12, color: "var(--cf-text-muted)", marginTop: 2 }}>{user.email}</div>
          <div style={{ display: "flex", gap: 8, marginTop: 8, flexWrap: "wrap" }}>
            <span className="chip chip-unknown" style={{ fontSize: 10 }}>{user.source}</span>
            <span className="chip chip-info" style={{ fontSize: 10 }}>{user.org}</span>
            {user.groups.map(g => <span key={g} className="chip chip-unknown mono" style={{ fontSize: 10 }}>{g}</span>)}
            {user.mfa && <span className="chip chip-healthy" style={{ fontSize: 10 }}>MFA on</span>}
          </div>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 6, alignItems: "flex-end" }}>
          <button className="btn btn-ghost focus-ring xs"><Icon name="key" size={11} /> Change password</button>
          <button className="btn btn-ghost focus-ring xs"><Icon name="x" size={11} /> Sign out</button>
        </div>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16, alignItems: "start" }}>
        {/* Appearance */}
        <div className="card" style={{ padding: "8px 18px 14px" }}>
          <h3 style={{ fontSize: 13, fontWeight: 600, margin: "14px 0 4px" }}>Appearance</h3>
          <PrefRow title="Theme" desc="Dark is optimized for long operational use.">
            <Seg value={theme} onChange={onTheme} opts={[{ v: "dark", l: "Dark" }, { v: "light", l: "Light" }]} />
          </PrefRow>
          <PrefRow title="Density" desc="Compact fits more rows per screen.">
            <Seg value={density} onChange={onDensity} opts={[{ v: "comfortable", l: "Comfort" }, { v: "compact", l: "Compact" }]} />
          </PrefRow>
          <PrefRow title="Sidebar" desc="Rail collapses the sidebar to icons.">
            <Seg value={sidebarMode} onChange={onSidebarMode} opts={[{ v: "full", l: "Full" }, { v: "rail", l: "Rail" }]} />
          </PrefRow>
          <PrefRow title="Default systems view" desc="Cards or table when opening Systems.">
            <Seg value={defaultView} onChange={onDefaultView} opts={[{ v: "cards", l: "Cards" }, { v: "table", l: "Table" }]} />
          </PrefRow>
        </div>

        {/* Notifications */}
        <div className="card" style={{ padding: "8px 18px 14px" }}>
          <h3 style={{ fontSize: 13, fontWeight: 600, margin: "14px 0 4px" }}>Notifications</h3>
          <PrefRow title="Deploy failures"><Toggle on={notif.deployFailed} onChange={v => setN("deployFailed", v)} /></PrefRow>
          <PrefRow title="Build failures"><Toggle on={notif.buildFailed} onChange={v => setN("buildFailed", v)} /></PrefRow>
          <PrefRow title="New critical CVEs"><Toggle on={notif.criticalCve} onChange={v => setN("criticalCve", v)} /></PrefRow>
          <PrefRow title="Policy violations"><Toggle on={notif.policyFail} onChange={v => setN("policyFail", v)} /></PrefRow>
          <PrefRow title="Heartbeat lost"><Toggle on={notif.heartbeatLost} onChange={v => setN("heartbeatLost", v)} /></PrefRow>
          <PrefRow title="Weekly digest email"><Toggle on={notif.weeklyDigest} onChange={v => setN("weeklyDigest", v)} /></PrefRow>
          <PrefRow title="Delivery" desc="Where alerts are sent.">
            <Seg value={notif.channel} onChange={v => setN("channel", v)} opts={[{ v: "in-app", l: "In-app" }, { v: "email", l: "Email" }, { v: "both", l: "Both" }]} />
          </PrefRow>
        </div>

        {/* Access summary */}
        <div className="card" style={{ padding: 18 }}>
          <h3 style={{ fontSize: 13, fontWeight: 600, margin: "0 0 12px" }}>Your access</h3>
          <dl className="kv-grid">
            <dt>Role</dt><dd><span className="chip chip-critical" style={{ fontSize: 10 }}>{user.role}</span></dd>
            <dt>Environments</dt><dd><span className="chip chip-info" style={{ fontSize: 10 }}>all</span></dd>
            <dt>Auth source</dt><dd>{user.source} · {user.groups.join(", ")}</dd>
            <dt>Member since</dt><dd>{user.joined}</dd>
            <dt>Last login</dt><dd>{user.lastLogin}</dd>
          </dl>
          <div className="help" style={{ marginTop: 10 }}>Role and environment scope come from your IdP groups. Contact an admin to change them.</div>
        </div>

        {/* Sessions / security */}
        <div className="card" style={{ padding: 18 }}>
          <h3 style={{ fontSize: 13, fontWeight: 600, margin: "0 0 12px" }}>Active sessions</h3>
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {[
              { dev: "MacBook Pro · Chrome", ip: "10.2.4.18", at: "current session", current: true },
              { dev: "iPhone · Safari", ip: "10.5.2.7", at: "2h ago" },
            ].map((s, i) => (
              <div key={i} style={{ display: "flex", alignItems: "center", gap: 10, padding: "8px 10px", background: "var(--cf-subtle-bg)", borderRadius: 8, fontSize: 12 }}>
                <Icon name="server" size={13} style={{ color: "var(--cf-text-muted)" }} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontWeight: 600 }}>{s.dev}</div>
                  <div className="mono" style={{ fontSize: 10, color: "var(--cf-text-muted)" }}>{s.ip} · {s.at}</div>
                </div>
                {s.current ? <span className="chip chip-healthy" style={{ fontSize: 9 }}>this device</span> : <button className="btn btn-ghost focus-ring xs">Revoke</button>}
              </div>
            ))}
          </div>
          <button className="btn btn-ghost focus-ring" style={{ marginTop: 12, color: "#fbbf24", borderColor: "rgba(251,191,36,0.3)" }}><Icon name="warn" size={12} /> Sign out everywhere</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { ProfileView });
