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

// Paginates a list for infinite scroll: renders `count` items, grows by `pageSize` when the
// sentinel scrolls into view within the scroll container. Resets to the first page whenever
// resetKey changes (tab switch, new search/filter).
function useInfiniteScroll(resetKey, pageSize = 30) {
  const [count, setCount] = React.useState(pageSize);
  const [node, setNode] = React.useState(null);
  const sentinelRef = React.useCallback((n) => setNode(n), []);
  React.useEffect(() => { setCount(pageSize); }, [resetKey]);
  React.useEffect(() => {
    if (!node) return;
    const scroller = node.closest(".content") || window;
    const check = () => {
      const rect = node.getBoundingClientRect();
      const scrollerRect = scroller === window
        ? { bottom: window.innerHeight }
        : scroller.getBoundingClientRect();
      if (rect.top < scrollerRect.bottom + 400) setCount(c => c + pageSize);
    };
    check();
    scroller.addEventListener("scroll", check, { passive: true });
    window.addEventListener("resize", check);
    return () => { scroller.removeEventListener("scroll", check); window.removeEventListener("resize", check); };
  }, [node, count, resetKey]);
  return { count, sentinelRef };
}
window.useInfiniteScroll = useInfiniteScroll;

// Marks, per flake, the id of the first (= latest, lists are newest-first) entry in `list`.
function latestPerFlake(list) {
  const seen = new Set(), ids = new Set();
  for (const item of list) { if (item.flake && !seen.has(item.flake)) { seen.add(item.flake); ids.add(item.id); } }
  return ids;
}
window.latestPerFlake = latestPerFlake;

function timeAgoShort(iso) {
  if (!iso) return "";
  const mins = Math.round((Date.now() - new Date(iso).getTime())/60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins/60);
  if (hrs < 48) return `${hrs}h ago`;
  return `${Math.round(hrs/24)}d ago`;
}

// Shared "Import / Export" dropdown — consolidates sharing actions behind one button
// instead of a row of standalone buttons. items: [{label, icon, onClick, danger}] or "divider".
function IOMenu({ label = "Import / Export", icon = "upload", items }) {
  const [open, setOpen] = React.useState(false);
  const ref = React.useRef(null);
  React.useEffect(() => {
    if (!open) return;
    const onDoc = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    const onKey = (e) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onDoc);
    window.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("mousedown", onDoc); window.removeEventListener("keydown", onKey); };
  }, [open]);
  return (
    <div ref={ref} style={{ position:"relative" }}>
      <button className="btn btn-ghost focus-ring" onClick={() => setOpen(v => !v)}>
        <Icon name={icon} size={14}/> {label} <Icon name="chevron-down" size={12}/>
      </button>
      {open && (
        <div className="io-menu">
          {items.map((it, i) => it === "divider"
            ? <div key={i} className="io-menu-divider"/>
            : (
              <button key={i} className="io-menu-item" onClick={() => { setOpen(false); it.onClick(); }} style={it.danger ? { color:"#f87171" } : undefined}>
                <Icon name={it.icon} size={13}/> {it.label}
              </button>
            ))}
        </div>
      )}
    </div>
  );
}
window.IOMenu = IOMenu;

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

// ─── Sidebar badges vs. the notification bell ───
// Two different jobs, on purpose — see docs/alerts-and-notifications.md for the full writeup:
//   Sidebar badge  = live ROLLUP of unresolved state for that section right now (e.g. "6 systems
//                    need attention"). Recomputed from current data every render, auto-clears the
//                    moment the count hits zero OR the operator visits that section (acknowledged).
//                    No history, no per-item dismiss — it's a mirror of "is this section OK?".
//   Notification bell = chronological EVENT LOG — discrete things that happened (a build failed,
//                    a CVE was discovered, a deploy needs approval). Each is its own dismissible
//                    item with a timestamp; it does NOT disappear just because the underlying
//                    condition got fixed — you have to read/act on it. It's the "what happened"
//                    audit trail, the sidebar is the "what's wrong right now" gauge.
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
const _attentionCount = (typeof ATTESTATION_RECORDS !== "undefined" ? ATTESTATION_RECORDS.filter(r => ["unauthorized_artifact","unknown_artifact","agent_identity_invalid"].includes(r.classification) && !r.resolution).length : 0);
const _approvalCount = (typeof APPROVAL_QUEUE !== "undefined" ? APPROVAL_QUEUE.filter(a => a.status === "pending").length : 0);

const NAV = [
  { key: "dashboard", label: "Dashboard", icon: "dashboard", count: null, route: "dashboard" },
  { key: "systems",   label: "Systems",   icon: "server",
    count: (_sysAttention + _attentionCount + _approvalCount) || null, attention: (_sysAttention + _attentionCount + _approvalCount) > 0,
    countTitle: `${_sysAttention} of ${_sysTotal} systems need attention · ${_approvalCount} awaiting deploy approval · ${_attentionCount} unauthorized/unknown artifacts` },
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
  { key: "evals",    label: "Evaluations", icon: "eval",   count: _failedEvals24h || null,  attention: _failedEvals24h > 0,  countTitle: `${_failedEvals24h} failed evaluation${_failedEvals24h===1?"":"s"} in the last 24h`, route: "evals" },
  { key: "builds",   label: "Builds",      icon: "build",  count: _failedBuilds24h || null, attention: _failedBuilds24h > 0, countTitle: `${_failedBuilds24h} failed build${_failedBuilds24h===1?"":"s"} in the last 24h`, route: "builds" },
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

function Sidebar({ rail, topView, onNav, onToggleRail }) {
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
        <div className="brand-mark"><img src="components/cf-logo.png" alt="Crystal Forge" /></div>
        <div style={{ minWidth: 0 }}>
          <div className="brand-name">Crystal Forge</div>
          <div className="brand-sub">v0.3.0 · dev</div>
        </div>
        <button className="sidebar-collapse focus-ring" onClick={onToggleRail}
          title={rail ? "Expand sidebar" : "Collapse sidebar"} aria-label={rail ? "Expand sidebar" : "Collapse sidebar"}>
          <Icon name={rail ? "chevron-right" : "chevron-left"} size={15} />
        </button>
      </div>
      <div className="sidebar-nav-scroll">
      <div className="nav-section-label">Fleet</div>
      {NAV.map(i => <NavItem key={i.key} item={{ ...i, route: i.route || (["systems","flakes","environments"].includes(i.key) ? i.key : undefined) }} />)}
      <div className="nav-section-label">Pipeline</div>
      {NAV_OPS.map(i => <NavItem key={i.key} item={i} />)}
      <div className="nav-section-label">Compliance</div>
      {NAV_COMPLIANCE.map(i => <NavItem key={i.key} item={i} />)}
      <div className="nav-section-label">System</div>
      {NAV_SYS.map(i => <NavItem key={i.key} item={{ ...i, route: i.route }} />)}
      </div>
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

function globalSearch(q) {
  q = q.trim().toLowerCase();
  if (!q) return [];
  const results = [];
  (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).filter(s => s.hostname.toLowerCase().includes(q)).slice(0, 5)
    .forEach(s => results.push({ type: "system", label: s.hostname, sub: `${s.environment} · ${s.flake}`, icon: "server", data: s }));
  (typeof FLAKE_REGISTRY !== "undefined" ? FLAKE_REGISTRY : []).filter(f => f.name.toLowerCase().includes(q)).slice(0, 4)
    .forEach(f => results.push({ type: "flake", label: f.name, sub: `${f.systemCount} systems · ${f.status}`, icon: "git", data: f }));
  (typeof CACHE_DESTINATIONS !== "undefined" ? CACHE_DESTINATIONS : []).filter(c => c.name.toLowerCase().includes(q) || c.url.toLowerCase().includes(q)).slice(0, 4)
    .forEach(c => results.push({ type: "cache", label: c.name, sub: c.url, icon: "cube", data: c }));
  (typeof POLICIES !== "undefined" ? POLICIES : []).filter(p => p.name.toLowerCase().includes(q)).slice(0, 4)
    .forEach(p => results.push({ type: "policy", label: p.name, sub: p.description || "", icon: "shield", data: p }));
  if (q.length >= 3) {
    const seenB = new Set();
    [...(typeof ACTIVE_BUILDS !== "undefined" ? ACTIVE_BUILDS : []), ...(typeof HISTORY_BUILDS !== "undefined" ? HISTORY_BUILDS : [])]
      .filter(b => b.commit && b.commit.toLowerCase().includes(q))
      .forEach(b => { if (seenB.has(b.commit)) return; seenB.add(b.commit); results.push({ type: "build", label: b.commit, sub: `${b.flake} · ${b.status}`, icon: "build", data: b }); });
    const seenE = new Set();
    [...(typeof ACTIVE_EVALS !== "undefined" ? ACTIVE_EVALS : []), ...(typeof HISTORY_EVALS !== "undefined" ? HISTORY_EVALS : [])]
      .filter(e => e.commit && e.commit.toLowerCase().includes(q))
      .forEach(e => { if (seenE.has(e.commit)) return; seenE.add(e.commit); results.push({ type: "eval", label: e.commit, sub: `${e.flake} · ${e.status}`, icon: "eval", data: e }); });
  }
  return results.slice(0, 10);
}

function GlobalSearch({ onResult }) {
  const [query, setQuery] = React.useState("");
  const [open, setOpen] = React.useState(false);
  const [active, setActive] = React.useState(0);
  const wrapRef = React.useRef(null);
  const inputRef = React.useRef(null);
  const results = React.useMemo(() => globalSearch(query), [query]);

  React.useEffect(() => {
    const onDoc = (e) => { if (wrapRef.current && !wrapRef.current.contains(e.target)) setOpen(false); };
    const onKey = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); inputRef.current?.focus(); setOpen(true); }
      else if (e.key === "Escape" && document.activeElement === inputRef.current) { setQuery(""); setOpen(false); inputRef.current?.blur(); }
    };
    document.addEventListener("mousedown", onDoc);
    window.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("mousedown", onDoc); window.removeEventListener("keydown", onKey); };
  }, []);

  const pick = (r) => { onResult(r); setQuery(""); setOpen(false); };

  return (
    <div className="topbar-search" ref={wrapRef}>
      <Icon name="search" />
      <input ref={inputRef} className="input focus-ring" placeholder="Search systems, flakes, commits…"
        value={query}
        onChange={(e) => { setQuery(e.target.value); setOpen(true); setActive(0); }}
        onFocus={() => query && setOpen(true)}
        onKeyDown={(e) => {
          if (!open || !results.length) return;
          if (e.key === "ArrowDown") { e.preventDefault(); setActive(a => Math.min(a + 1, results.length - 1)); }
          else if (e.key === "ArrowUp") { e.preventDefault(); setActive(a => Math.max(a - 1, 0)); }
          else if (e.key === "Enter") { e.preventDefault(); pick(results[active]); }
        }} />
      {!query && <span className="kbd" style={{ position: "absolute", right: 10, top: "50%", transform: "translateY(-50%)" }}>⌘K</span>}
      {open && query && (
        <div className="search-dropdown">
          {results.length === 0 ? (
            <div className="search-empty">No matches for "{query}"</div>
          ) : results.map((r, i) => (
            <div key={r.type + r.label + i} className={`search-result${i === active ? " active" : ""}`}
              onMouseEnter={() => setActive(i)} onMouseDown={(e) => { e.preventDefault(); pick(r); }}>
              <Icon name={r.icon} size={14} />
              <div className="search-result-text">
                <div className="search-result-label mono">{r.label}</div>
                {r.sub && <div className="search-result-sub">{r.sub}</div>}
              </div>
              <span className="search-result-type">{r.type}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function Topbar({ theme, onTheme, onTweaks, crumb, onNavigate, onSearchResult }) {
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

  const NOTIFS = React.useMemo(() => {
    const items = [];
    if (typeof APPROVAL_QUEUE !== "undefined") {
      APPROVAL_QUEUE.filter(a => a.status === "pending").forEach(a => {
        items.push({ id:`apr-${a.id}`, icon:"deploy", color:"#fbbf24",
          title:`${a.hostname} awaiting deploy approval`, sub:`${a.environment} · ${a.policyId} policy · ${a.approvals.length}/${a.neededApprovals} approved`,
          at: timeAgoShort(a.requestedAt), route:"systems", unread:true });
      });
    }
    if (typeof ATTESTATION_RECORDS !== "undefined") {
      ATTESTATION_RECORDS.filter(r => ["unauthorized_artifact","unknown_artifact","agent_identity_invalid"].includes(r.classification) && !r.resolution).forEach(r => {
        const meta = ATTESTATION_CLASSIFICATIONS[r.classification];
        items.push({ id:`att-${r.system_id}`, icon: r.classification==="agent_identity_invalid"?"key":"warn", color: meta.color,
          title:`${meta.label}: ${r.hostname}`, sub:`${r.environment} · ${r.flake} · needs a decision`,
          at: timeAgoShort(r.lastObserved), route:"systems", unread:true });
      });
    }
    if (typeof POAMS !== "undefined") {
      POAMS.forEach(p => {
        if (typeof poamIsOverdue === "function" && poamIsOverdue(p)) {
          items.push({ id:`poam-overdue-${p.id}`, icon:"activity", color:"#f87171",
            title:`${p.id} overdue: ${p.title}`, sub:`${p.owner} · was due ${p.due}`,
            at:"—", route:"compliance", poamId:p.id, unread:true });
        } else if (p.status === "awaiting_verification") {
          items.push({ id:`poam-verify-${p.id}`, icon:"activity", color:"#a78bfa",
            title:`${p.id} awaiting verification: ${p.title}`, sub:`${p.owner} · re-evaluate to confirm the fix`,
            at:"—", route:"compliance", poamId:p.id, unread:true });
        }
      });
    }
    items.push(
      { id:"b1", icon:"build",  color:"#f87171", title:"Build failed: openssl-3.3.2", sub:"hydra-02 · attempt 3", at:"12m ago", route:"builds", unread:true },
      { id:"c1", icon:"shield", color:"#f87171", title:"New critical CVE: CVE-2026-31822", sub:"affects 6 systems · openssl", at:"38m ago", route:"cves", unread:true },
      { id:"h1", icon:"warn",   color:"#fbbf24", title:"Heartbeat lost: edge-fra-01", sub:"no signal for 6h", at:"1h ago", route:"systems", unread:false },
      { id:"e1", icon:"eval",   color:"#34d399", title:"Eval complete: infrastructure@a3f8c12", sub:"12 systems · all policies passed", at:"2h ago", route:"evals", unread:false },
    );
    return items;
  }, []);
  const [dismissedIds, setDismissedIds] = React.useState(() => new Set());
  const visibleNotifs = NOTIFS.filter(n => !dismissedIds.has(n.id));
  const unread = visibleNotifs.filter(n => n.unread).length;

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
      <GlobalSearch onResult={onSearchResult} />
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
              {visibleNotifs.length === 0 && (
                <div style={{ padding:"24px 14px", textAlign:"center", fontSize:12, color:"var(--cf-text-muted)" }}>You're all caught up</div>
              )}
              {visibleNotifs.map(n => (
                <button key={n.id} className={`notif-item focus-ring${n.unread ? " unread" : ""}`}
                  onClick={() => { setNotifOpen(false); onNavigate?.(n.route); if (n.poamId && typeof openPoamDetail === "function") setTimeout(() => openPoamDetail(n.poamId), 60); }}>
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
            <div className="notif-foot" style={{ justifyContent:"space-between" }}>
              <button className="btn btn-ghost focus-ring xs" disabled={visibleNotifs.length===0}
                onClick={() => setDismissedIds(new Set(NOTIFS.map(n => n.id)))}>Dismiss all</button>
              <button className="btn btn-ghost focus-ring xs" onClick={() => { setNotifOpen(false); onNavigate?.("profile"); }}>Notification settings</button>
            </div>
          </div>
        )}
      </div>
      <button className="btn-icon focus-ring" aria-label="Toggle theme" title="Toggle theme" onClick={onTheme}>
        <Icon name={theme === "dark" ? "sun" : "moon"} size={16} />
      </button>
    </div>
  );
}

window.Sidebar = Sidebar;
window.Topbar = Topbar;
