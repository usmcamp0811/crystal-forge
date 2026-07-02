// Sidebar + Topbar shell

// ─── Shared multi-select helpers (Builds / Evals / Scanning) ───
function RowCheck({ checked, indeterminate, disabled, onChange }) {
  const ref = React.useRef(null);
  React.useEffect(() => { if (ref.current) ref.current.indeterminate = !!indeterminate && !checked; }, [indeterminate, checked]);
  return (
    <input
      ref={ref}
      type="checkbox"
      className="row-check focus-ring"
      checked={!!checked}
      disabled={disabled}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => { e.stopPropagation(); onChange?.(e.target.checked); }}
    />
  );
}
window.RowCheck = RowCheck;

// Floating action bar that animates up from the bottom when rows are selected.
function BulkBar({ count, onClear, children }) {
  if (!count) return null;
  return (
    <div className="bulk-bar" role="toolbar" aria-label="Bulk actions">
      <span className="bulk-count"><strong>{count}</strong> selected</span>
      <span className="bulk-sep" />
      {children}
      <button className="btn btn-ghost xs focus-ring" onClick={onClear}>Clear</button>
    </div>
  );
}
window.BulkBar = BulkBar;

// Tracks a Set of selected ids with modifier-click (⌘/Ctrl toggle, Shift range) support.
function useMultiSelect(resetKey) {
  const [ids, setIds] = React.useState(() => new Set());
  const anchor = React.useRef(null);
  React.useEffect(() => { setIds(new Set()); anchor.current = null; }, [resetKey]);
  const toggle = React.useCallback((id, on) => {
    setIds(prev => { const n = new Set(prev); (on ?? !n.has(id)) ? n.add(id) : n.delete(id); return n; });
  }, []);
  const set = React.useCallback((arr) => setIds(new Set(arr)), []);
  const clear = React.useCallback(() => { setIds(new Set()); anchor.current = null; }, []);
  // Remember a row as the range anchor even on a plain (non-selecting) click, so a
  // later Shift-click can extend a range from it ("click top, shift-click bottom").
  const setAnchor = React.useCallback((id) => { anchor.current = id; }, []);
  // Returns true if the click was a selection gesture (caller should NOT run its normal click).
  // selectableIds: array of ids, in display order, that are eligible for selection.
  const handleClick = React.useCallback((e, id, selectableIds) => {
    const mod = e.metaKey || e.ctrlKey;
    const shift = e.shiftKey;
    if (!mod && !shift) return false;
    e.preventDefault(); e.stopPropagation();
    if (!selectableIds.includes(id)) return true; // consumed, but row not selectable
    setIds(prev => {
      const n = new Set(prev);
      if (shift && anchor.current != null && selectableIds.includes(anchor.current)) {
        const a = selectableIds.indexOf(anchor.current);
        const b = selectableIds.indexOf(id);
        const [lo, hi] = a < b ? [a, b] : [b, a];
        for (let i = lo; i <= hi; i++) n.add(selectableIds[i]);
      } else {
        n.has(id) ? n.delete(id) : n.add(id);
        anchor.current = id;
      }
      return n;
    });
    return true;
  }, []);
  return { ids, toggle, set, clear, setAnchor, handleClick, has: (id) => ids.has(id), size: ids.size };
}
window.useMultiSelect = useMultiSelect;

// Small inline hint telling users how to multi-select.
function MultiSelectHint() {
  return (
    <span className="ms-hint" title="⌘/Ctrl-click to toggle rows · Shift-click to select a range">
      <kbd>⌘</kbd>/<kbd>⇧</kbd>-click to select
    </span>
  );
}
window.MultiSelectHint = MultiSelectHint;

// ─── Live, ticking duration ───
// We advertise the dashboard as "Live", so in-flight durations must actually tick.
// Parses an initial "147s" / "2m 27s" / "1h 3m" into seconds, anchors a start time
// once, and (when live) re-renders every second counting up from it.
function parseDur(str) {
  if (typeof str === "number") return str;
  if (!str) return 0;
  let s = 0; const re = /(\d+)\s*([hms])/g; let m, hit = false;
  while ((m = re.exec(str))) { hit = true; const n = +m[1]; s += m[2] === "h" ? n*3600 : m[2] === "m" ? n*60 : n; }
  if (!hit) { const n = parseInt(str, 10); if (!isNaN(n)) s = n; }
  return s;
}
function fmtDur(totalSec) {
  totalSec = Math.max(0, Math.floor(totalSec));
  const h = Math.floor(totalSec/3600), mm = Math.floor((totalSec%3600)/60), ss = totalSec%60;
  if (h) return `${h}h ${mm}m`;
  if (mm) return `${mm}m ${String(ss).padStart(2,"0")}s`;
  return `${ss}s`;
}
function LiveDuration({ seconds, dur, live = false, style }) {
  const base = seconds != null ? seconds : parseDur(dur);
  const startRef = React.useRef(null);
  const [, force] = React.useReducer(x => x + 1, 0);
  if (startRef.current == null) startRef.current = Date.now() - base * 1000;
  React.useEffect(() => {
    if (!live) return;
    const id = setInterval(force, 1000);
    return () => clearInterval(id);
  }, [live]);
  const elapsed = live ? (Date.now() - startRef.current) / 1000 : base;
  return (
    <span className={`mono${live ? " live-dur" : ""}`} style={style} title={live ? "Live — counting up" : undefined}>
      {fmtDur(elapsed)}{live && <span className="live-dur-dot" aria-hidden="true" />}
    </span>
  );
}
Object.assign(window, { parseDur, fmtDur, LiveDuration });

// ─── Date-Time Group (DTG) ───
// Mock data uses relative strings ("4m ago", "yesterday"). Anchor them to a fixed
// load time and render an absolute Zulu DTG (DDHHMMZ MON YY) — the DoD-standard
// timestamp, fitting the classification theme. Also offers a plain local fallback.
const _DTG_NOW = Date.now();
function relToDate(str) {
  if (str == null) return null;
  const s = String(str).trim().toLowerCase();
  if (s === "just now" || s === "now") return new Date(_DTG_NOW);
  if (s === "yesterday") return new Date(_DTG_NOW - 86400000);
  const m = /(\d+)\s*(s|sec|m|min|h|hr|hour|d|day|w|wk|week|mo|month|y|yr|year)/.exec(s);
  if (!m) return null;
  const n = +m[1], u = m[2];
  const mult = u.startsWith("s") ? 1e3 : u.startsWith("mo") ? 2.592e9
    : u.startsWith("min") || u === "m" ? 6e4 : u.startsWith("h") ? 3.6e6
    : u.startsWith("d") ? 8.64e7 : u.startsWith("w") ? 6.048e8
    : u.startsWith("y") ? 3.1536e10 : 6e4;
  return new Date(_DTG_NOW - n * mult);
}
const _MON = ["JAN","FEB","MAR","APR","MAY","JUN","JUL","AUG","SEP","OCT","NOV","DEC"];
function toDTG(dateOrRel) {
  const d = dateOrRel instanceof Date ? dateOrRel : relToDate(dateOrRel);
  if (!d || isNaN(d)) return null;
  const p = (n) => String(n).padStart(2, "0");
  return `${p(d.getUTCDate())}${p(d.getUTCHours())}${p(d.getUTCMinutes())}Z ${_MON[d.getUTCMonth()]} ${String(d.getUTCFullYear()).slice(2)}`;
}
function toLocalStamp(dateOrRel) {
  const d = dateOrRel instanceof Date ? dateOrRel : relToDate(dateOrRel);
  if (!d || isNaN(d)) return null;
  return d.toLocaleString(undefined, { month:"short", day:"numeric", year:"numeric", hour:"2-digit", minute:"2-digit" });
}
// Renders the absolute DTG with the relative label + local time in the tooltip.
function DTG({ at, relative }) {
  const dtg = toDTG(at);
  if (!dtg) return <span>{at || "—"}</span>;
  const local = toLocalStamp(at);
  return (
    <span className="mono dtg" title={`${local} local${relative ? ` · ${relative}` : ""}`}>
      {dtg}{relative && <span className="dtg-rel"> · {relative}</span>}
    </span>
  );
}
Object.assign(window, { relToDate, toDTG, toLocalStamp, DTG });

// ─── First-visit attention flash + badge acknowledgment ───
// Visiting a flagged view pulses its attention items once per page load; acknowledging
// it clears the sidebar badge. For most views these happen together on open; for
// Builds/Evals the failures live in a non-default tab, so the badge is acknowledged
// only when that tab is opened (see acknowledgeView).
const _flashedViews = new Set();   // gates flash replay (per page load)
const _ackViews = new Set();       // badge acknowledged → hide it
const _ackListeners = new Set();
function _notifyAck() { _ackListeners.forEach(fn => { try { fn(); } catch {} }); }
function acknowledgeView(key) {
  if (_ackViews.has(key)) return;
  _ackViews.add(key);
  _notifyAck();
}
function useAttentionFlash(key, hasAttention = true, opts = {}) {
  const { ack = true } = opts;
  const [flash, setFlash] = React.useState(false);
  React.useEffect(() => {
    if (!hasAttention || _flashedViews.has(key)) return;
    _flashedViews.add(key);
    if (ack) acknowledgeView(key);
    setFlash(true);
    const t = setTimeout(() => setFlash(false), 3200);
    return () => clearTimeout(t);
  }, []);
  return flash;
}
// Subscribe the sidebar so badges disappear the moment a view is acknowledged.
function useAcknowledgedViews() {
  const [, force] = React.useReducer(x => x + 1, 0);
  React.useEffect(() => { _ackListeners.add(force); return () => _ackListeners.delete(force); }, []);
  return _ackViews;
}
Object.assign(window, { useAttentionFlash, useAcknowledgedViews, acknowledgeView });

// ─── DoD / CNSS classification banners ───
// Standard banner colors per CNSS/DoD marking guidance.
const CLASSIFICATION_LEVELS = [
  { id: "UNCLASSIFIED",      label: "UNCLASSIFIED",      bg: "#007a33", fg: "#ffffff" },
  { id: "CUI",               label: "CUI",               bg: "#502b85", fg: "#ffffff" },
  { id: "CONFIDENTIAL",      label: "CONFIDENTIAL",      bg: "#0033a0", fg: "#ffffff" },
  { id: "SECRET",            label: "SECRET",            bg: "#c8102e", fg: "#ffffff" },
  { id: "TOP SECRET",        label: "TOP SECRET",        bg: "#ff8c00", fg: "#000000" },
  { id: "TOP SECRET//SCI",   label: "TOP SECRET//SCI",   bg: "#fce83a", fg: "#000000" },
];
window.CLASSIFICATION_LEVELS = CLASSIFICATION_LEVELS;

function ClassificationBanner({ level, text, position }) {
  const def = CLASSIFICATION_LEVELS.find(l => l.id === level) || CLASSIFICATION_LEVELS[0];
  const display = (text && text.trim()) ? text.trim().toUpperCase() : def.label;
  return (
    <div className={`classif-banner classif-banner-${position}`} style={{ background: def.bg, color: def.fg }} aria-hidden="true">
      {display}
    </div>
  );
}
window.ClassificationBanner = ClassificationBanner;

// ─── Sidebar badge counts ───
// Every badge is a "needs attention" signal (red) or an informational total (gray),
// and every badge carries a tooltip spelling out exactly what it counts.
function _cnt(list, pred) { return (typeof list !== "undefined" ? list : []).filter(pred).length; }
const _sysAttention = _cnt(typeof SYSTEMS !== "undefined" ? SYSTEMS : [], s => s.health === "critical" || s.health === "offline");
const _sysTotal     = typeof SYSTEMS !== "undefined" ? SYSTEMS.length : 0;
const _flakeErrors  = _cnt(typeof FLAKE_REGISTRY !== "undefined" ? FLAKE_REGISTRY : [], f => f.status === "error");
const _flakeTotal   = typeof FLAKE_REGISTRY !== "undefined" ? FLAKE_REGISTRY.length : 0;
const _envTotal     = typeof ENVIRONMENTS !== "undefined" ? ENVIRONMENTS.length : 0;
const _envAttentionList = (typeof ENVIRONMENTS !== "undefined" ? ENVIRONMENTS : []).filter(e =>
  (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).some(s => s.environment === e.name && (s.health === "critical" || s.health === "offline")));
const _envAttention = _envAttentionList.length;
const _cveCritical  = (typeof CVE_STATS !== "undefined" && CVE_STATS) ? CVE_STATS.critical : 0;

const NAV = [
  { key: "dashboard", label: "Dashboard", icon: "dashboard", count: null, route: "dashboard" },
  { key: "systems",   label: "Systems",   icon: "server",
    count: _sysAttention || null, attention: _sysAttention > 0,
    countTitle: `${_sysAttention} of ${_sysTotal} systems need attention (critical or offline)` },
  { key: "flakes",    label: "Flakes",    icon: "git",
    count: _flakeErrors || null, attention: _flakeErrors > 0,
    countTitle: _flakeErrors > 0 ? `${_flakeErrors} of ${_flakeTotal} flakes failing to sync` : `${_flakeTotal} flakes tracked` },
  { key: "environments", label: "Environments", icon: "env",
    count: _envAttention || null, attention: _envAttention > 0,
    countTitle: _envAttention > 0 ? `${_envAttention} of ${_envTotal} environments have critical or offline systems` : `${_envTotal} deployment environments` },
];

// Count items whose relative timestamp falls within the last `hours` hours.
function _failedWithin(list, hours, tsKey) {
  const cutoff = Date.now() - hours * 3600 * 1000;
  return (list || []).filter(x => {
    if (x.status !== "failed") return false;
    const d = relToDate(x[tsKey] || x.queuedAt || x.startedAt);
    return d && d.getTime() >= cutoff;
  }).length;
}
const _failedBuilds24h = typeof HISTORY_BUILDS !== "undefined" ? _failedWithin(HISTORY_BUILDS, 24, "queuedAt") : 0;
const _failedEvals24h  = typeof HISTORY_EVALS  !== "undefined" ? _failedWithin(HISTORY_EVALS, 24, "completedAt") : 0;

const NAV_OPS = [
  { key: "builds",   label: "Builds",      icon: "build",  count: _failedBuilds24h || null, attention: _failedBuilds24h > 0, countTitle: `${_failedBuilds24h} failed build${_failedBuilds24h===1?"":"s"} in the last 24h`, route: "builds" },
  { key: "evals",    label: "Evaluations", icon: "eval",   count: _failedEvals24h || null,  attention: _failedEvals24h > 0,  countTitle: `${_failedEvals24h} failed evaluation${_failedEvals24h===1?"":"s"} in the last 24h`, route: "evals" },
  { key: "scanning", label: "Scanning",    icon: "shield", count: null, route: "scanning" },
];

const NAV_COMPLIANCE = [
  { key: "cves",       label: "CVEs",       icon: "shield",
    count: _cveCritical || null, attention: _cveCritical > 0,
    countTitle: `${_cveCritical} critical CVEs open across the fleet`, route: "cves" },
  { key: "policies",   label: "Policies",   icon: "file",   route: "policies" },
  { key: "compliance", label: "Compliance", icon: "check",  route: "compliance" },
];

const NAV_SYS = [
  { key: "builders", label: "Builders", icon: "cpu", route: "builders" },
  { key: "caches",   label: "Caches",   icon: "download", route: "caches" },
  { key: "admin",    label: "Server",   icon: "gear", route: "admin" },
];

function Sidebar({ rail, topView, onNav }) {
  const acked = useAcknowledgedViews();
  const NavItem = ({ item }) => {
    const isActive = (item.route && topView === item.route) ||
                     (!item.route && item.key === "systems" && topView === "systems");
    // Attention badges disappear once their view has been visited (acknowledged).
    const showCount = item.count != null && !(item.attention && acked.has(item.key));
    return (
      <div
        className={`nav-item${isActive ? " active" : ""} focus-ring`}
        tabIndex={0}
        title={rail ? item.label : undefined}
        onClick={() => item.route && onNav(item.route)}
        style={{ cursor: item.route ? "pointer" : "default" }}
      >
        <Icon name={item.icon} className="nav-icon" />
        <span className="nav-label">{item.label}</span>
        {showCount && <span className={`nav-count${item.attention ? " nav-count-alert" : ""}`} title={item.countTitle || undefined}>{item.count}</span>}
      </div>
    );
  };
  return (
    <aside className={`sidebar${rail ? " rail" : ""}`}>
      <div className="sidebar-brand">
        <div className="brand-mark">CF</div>
        <div style={{ minWidth: 0 }}>
          <div className="brand-name">Crystal Forge</div>
          <div className="brand-sub">v0.3.0 · dev</div>
        </div>
      </div>
      <div className="nav-section-label">Fleet</div>
      {NAV.map(i => <NavItem key={i.key} item={{ ...i, route: i.route || (["systems","flakes","environments"].includes(i.key) ? i.key : undefined) }} />)}
      <div className="nav-section-label">Pipeline</div>
      {NAV_OPS.map(i => <NavItem key={i.key} item={i} />)}
      <div className="nav-section-label">Compliance</div>
      {NAV_COMPLIANCE.map(i => <NavItem key={i.key} item={i} />)}
      <div className="nav-section-label">System</div>
      {NAV_SYS.map(i => <NavItem key={i.key} item={{ ...i, route: i.route }} />)}
      <div style={{ flex: 1 }} />
      <div
        className={`nav-item${topView === "profile" ? " active" : ""} focus-ring`}
        tabIndex={0}
        onClick={() => onNav("profile")}
        title={rail ? "Mira Reyes" : undefined}
        style={{ margin: "8px 10px", padding: "8px 10px", borderTop: "1px solid var(--cf-divider)", borderRadius: 10, cursor: "pointer", display: "flex", alignItems: "center", gap: 10 }}>
        <div style={{
          width: 28, height: 28, borderRadius: 99,
          background: "linear-gradient(135deg,#f472b6,#6366f1)",
          display: "grid", placeItems: "center",
          color: "#fff", fontSize: 11, fontWeight: 600,
          flexShrink: 0,
        }}>MR</div>
        {!rail && (
          <div style={{ minWidth: 0, flex: 1 }}>
            <div style={{ fontSize: 12, fontWeight: 500, color: "var(--cf-text-primary)" }}>Mira Reyes</div>
            <div style={{ fontSize: 11, color: "var(--cf-text-muted)" }}>admin · acme-prod</div>
          </div>
        )}
        {!rail && <Icon name="gear" size={13} style={{ color: "var(--cf-text-muted)" }} />}
      </div>
    </aside>
  );
}

function Topbar({ theme, onTheme, onTweaks, crumb, onNavigate }) {
  const [notifOpen, setNotifOpen] = React.useState(false);
  const bellRef = React.useRef(null);

  React.useEffect(() => {
    if (!notifOpen) return;
    const onDoc = (e) => { if (bellRef.current && !bellRef.current.contains(e.target)) setNotifOpen(false); };
    const onKey = (e) => { if (e.key === "Escape") setNotifOpen(false); };
    document.addEventListener("mousedown", onDoc);
    window.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("mousedown", onDoc); window.removeEventListener("keydown", onKey); };
  }, [notifOpen]);

  const NOTIFS = [
    { id:1, icon:"deploy", color:"#fbbf24", title:"3 systems awaiting deploy approval", sub:"production · manual policy", at:"2m ago", route:"systems", unread:true },
    { id:2, icon:"build",  color:"#f87171", title:"Build failed: openssl-3.3.2", sub:"hydra-02 · attempt 3", at:"12m ago", route:"builds", unread:true },
    { id:3, icon:"shield", color:"#f87171", title:"New critical CVE: CVE-2026-31822", sub:"affects 6 systems · openssl", at:"38m ago", route:"cves", unread:true },
    { id:4, icon:"warn",   color:"#fbbf24", title:"Heartbeat lost: edge-fra-01", sub:"no signal for 6h", at:"1h ago", route:"systems", unread:false },
    { id:5, icon:"eval",   color:"#34d399", title:"Eval complete: infrastructure@a3f8c12", sub:"12 systems · all policies passed", at:"2h ago", route:"evals", unread:false },
  ];
  const unread = NOTIFS.filter(n => n.unread).length;

  return (
    <div className="topbar">
      <div className="breadcrumbs">
        <span>Fleet</span>
        <span className="sep">/</span>
        {crumb?.parent && (
          <>
            <span>{crumb.parent}</span>
            <span className="sep">/</span>
          </>
        )}
        <span className="crumb-current">{crumb?.current || "Systems"}</span>
      </div>
      <div className="topbar-search">
        <Icon name="search" />
        <input className="input focus-ring" placeholder="Search systems, flakes, commits…" />
        <span className="kbd" style={{ position: "absolute", right: 10, top: "50%", transform: "translateY(-50%)" }}>⌘K</span>
      </div>
      <div ref={bellRef} style={{ position:"relative" }}>
        <button className="btn-icon focus-ring topbar-bell" aria-label="Notifications"
          title="Notifications" onClick={() => setNotifOpen(o => !o)}>
          <Icon name="bell" size={16} />
          {unread > 0 && <span className="topbar-bell-badge">{unread}</span>}
        </button>
        {notifOpen && (
          <div className="notif-panel">
            <div className="notif-head">
              <strong style={{ fontSize:13 }}>Notifications</strong>
              <button className="btn-icon focus-ring" title="Mark all read" style={{ padding:4 }}><Icon name="check" size={13}/></button>
            </div>
            <div className="notif-list">
              {NOTIFS.map(n => (
                <button key={n.id} className={`notif-item focus-ring${n.unread ? " unread" : ""}`}
                  onClick={() => { setNotifOpen(false); onNavigate?.(n.route); }}>
                  <span className="notif-icon" style={{ color:n.color, background:`color-mix(in oklab, ${n.color} 16%, transparent)` }}>
                    <Icon name={n.icon} size={13}/>
                  </span>
                  <span style={{ minWidth:0, flex:1 }}>
                    <span className="notif-title">{n.title}</span>
                    <span className="notif-sub">{n.sub}</span>
                  </span>
                  <span className="notif-at">{n.at}</span>
                </button>
              ))}
            </div>
            <div className="notif-foot">
              <button className="btn btn-ghost focus-ring xs" onClick={() => { setNotifOpen(false); onNavigate?.("profile"); }}>Notification settings</button>
            </div>
          </div>
        )}
      </div>
      <button className="btn-icon focus-ring" aria-label="Toggle theme" title="Toggle theme" onClick={onTheme}>
        <Icon name={theme === "dark" ? "sun" : "moon"} size={16} />
      </button>
      <button className="btn-icon focus-ring" aria-label="Tweaks" title="Tweaks" onClick={onTweaks}>
        <Icon name="tweaks" size={16} />
      </button>
    </div>
  );
}

window.Sidebar = Sidebar;
window.Topbar = Topbar;
