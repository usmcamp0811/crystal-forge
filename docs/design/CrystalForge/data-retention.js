// Retention & archiving.
//
// Archiving here is a VISIBILITY concern only. An archived build / eval / scan
// keeps its full record: it stays searchable, its drawer still opens, and
// anything that links to it (compliance matrix, POA&M, attestations, audit log)
// resolves exactly as before. The only change is that it drops out of the
// default list so 100k completed builds don't bury the ones you care about.

const RETENTION_KEY = "cf.retention.v1";

const RETENTION_KINDS = [
  { k:"builds", label:"Builds",      desc:"Completed, failed and cancelled builds.", unit:"build" },
  { k:"evals",  label:"Evaluations", desc:"Finished flake evaluations and their policy results.", unit:"evaluation" },
  { k:"scans",  label:"Scan history",desc:"Per-config vulnerability scan results. Latest scan per config is never archived.", unit:"scan" },
];

const RETENTION_DEFAULTS = {
  builds: { enabled:true, age:{ on:true,  days:30 }, perFlake:{ on:true,  n:25 }, cap:{ on:false, n:5000 } },
  evals:  { enabled:true, age:{ on:true,  days:30 }, perFlake:{ on:true,  n:25 }, cap:{ on:false, n:5000 } },
  scans:  { enabled:true, age:{ on:true,  days:90 }, perFlake:{ on:true,  n:10 }, cap:{ on:false, n:2000 } },
  manual: { builds:{ archived:[], restored:[] }, evals:{ archived:[], restored:[] }, scans:{ archived:[], restored:[] } },
  lastSweep: "3h ago",
};

/* ── Age decoration ──────────────────────────────────────────────────────
   Fixtures are ordered newest-first. Spread them over ~2 months on a curve so
   the top of the list is dense (hours) and the tail is genuinely old (weeks),
   which is what makes a retention window mean anything. */
function _cfAgeHours(i) { return 0.35 * Math.pow(i, 1.55); }
function _cfAgo(h) {
  if (h < 1)  return `${Math.max(1, Math.round(h*60))}m ago`;
  if (h < 48) return `${Math.round(h)}h ago`;
  return `${Math.round(h/24)}d ago`;
}
function _cfParseDays(s) {
  if (!s) return 0;
  const m = /(\d+)\s*([mhd])/.exec(String(s));
  if (!m) return 0;
  const n = +m[1];
  return m[2] === "d" ? n : m[2] === "h" ? n/24 : n/1440;
}

(function decorateAges() {
  if (typeof HISTORY_BUILDS !== "undefined") HISTORY_BUILDS.forEach((b, i) => {
    const h = _cfAgeHours(i);
    b.ageHours = h; b.ageDays = h/24; b.queuedAt = _cfAgo(h);
  });
  if (typeof HISTORY_EVALS !== "undefined") HISTORY_EVALS.forEach((e, i) => {
    const h = _cfAgeHours(i);
    e.ageHours = h; e.ageDays = h/24;
    e.completedAt = _cfAgo(h);
    e.startedAt = _cfAgo(h + (parseInt(e.dur, 10) || 60)/3600);
  });
  if (typeof SCAN_COMPLETED_HISTORY !== "undefined") SCAN_COMPLETED_HISTORY.forEach((c, i) => {
    const h = _cfAgeHours(i + 4);
    c.ageHours = h; c.ageDays = h/24;
    c.lastScan = (c.status === "failed" ? "failed " : "") + _cfAgo(h);
  });
  if (typeof SCAN_HISTORY !== "undefined") SCAN_HISTORY.forEach(sys => {
    (sys.commits || []).forEach((c, i) => {
      c.id = `${sys.id}:${c.commit}:${i}`;
      c.flake = sys.id;
      c.ageDays = c.current ? 0 : Math.max(_cfParseDays(c.lastScan), i * 2.4);
    });
  });
})();

/* ── Store ───────────────────────────────────────────────────────────── */
let _cfRetentionState = (function load() {
  const base = JSON.parse(JSON.stringify(RETENTION_DEFAULTS));
  try {
    const raw = localStorage.getItem(RETENTION_KEY);
    if (raw) {
      const saved = JSON.parse(raw);
      ["builds","evals","scans"].forEach(k => { if (saved[k]) base[k] = { ...base[k], ...saved[k] }; });
      if (saved.manual) base.manual = { ...base.manual, ...saved.manual };
    }
  } catch {}
  return base;
})();

const _cfRetentionSubs = new Set();
function _cfRetentionCommit(next) {
  _cfRetentionState = next;
  try { localStorage.setItem(RETENTION_KEY, JSON.stringify(next)); } catch {}
  _cfRetentionSubs.forEach(fn => fn());
}

const cfRetention = {
  get: () => _cfRetentionState,
  subscribe: (fn) => { _cfRetentionSubs.add(fn); return () => _cfRetentionSubs.delete(fn); },
  setRules: (kind, patch) => _cfRetentionCommit({ ..._cfRetentionState, [kind]: { ..._cfRetentionState[kind], ...patch } }),
  reset: (kind) => _cfRetentionCommit({ ..._cfRetentionState, [kind]: JSON.parse(JSON.stringify(RETENTION_DEFAULTS[kind])) }),
  // Manual archive / restore. Both are recorded as overrides so a manual
  // decision survives the nightly sweep in either direction.
  archive: (kind, ids) => {
    const m = _cfRetentionState.manual[kind];
    const next = {
      archived: [...new Set([...m.archived, ...ids])],
      restored: m.restored.filter(id => !ids.includes(id)),
    };
    _cfRetentionCommit({ ..._cfRetentionState, manual: { ..._cfRetentionState.manual, [kind]: next } });
  },
  restore: (kind, ids) => {
    const m = _cfRetentionState.manual[kind];
    const next = {
      archived: m.archived.filter(id => !ids.includes(id)),
      restored: [...new Set([...m.restored, ...ids])],
    };
    _cfRetentionCommit({ ..._cfRetentionState, manual: { ..._cfRetentionState.manual, [kind]: next } });
  },
  clearManual: (kind) => _cfRetentionCommit({
    ..._cfRetentionState,
    manual: { ..._cfRetentionState.manual, [kind]: { archived:[], restored:[] } },
  }),
};

/* ── Rule evaluation ─────────────────────────────────────────────────────
   `list` must be newest-first. Returns a Map of id → reason for every record
   the current rules (plus manual overrides) would hide. */
function cfArchived(kind, list, cfgOverride) {
  const cfg = cfgOverride || _cfRetentionState[kind] || {};
  const man = _cfRetentionState.manual?.[kind] || { archived:[], restored:[] };
  const out = new Map();
  if (cfg.enabled) {
    const seen = {};
    list.forEach((r, i) => {
      const f = r.flake || "—";
      seen[f] = (seen[f] || 0) + 1;
      if (cfg.age?.on && (r.ageDays || 0) > cfg.age.days) out.set(r.id, `older than ${cfg.age.days}d`);
      else if (cfg.perFlake?.on && seen[f] > cfg.perFlake.n) out.set(r.id, `beyond newest ${cfg.perFlake.n} for ${f}`);
      else if (cfg.cap?.on && i >= cfg.cap.n) out.set(r.id, `beyond ${cfg.cap.n.toLocaleString()} record cap`);
    });
  }
  man.archived.forEach(id => out.set(id, "archived manually"));
  man.restored.forEach(id => out.delete(id));
  return out;
}

// How many records each rule would hide on its own — drives the Admin preview.
function cfRetentionPreview(kind, list, cfg) {
  const one = (rule) => cfArchived(kind, list, {
    enabled: true,
    age:      rule === "age"      ? cfg.age      : { on:false },
    perFlake: rule === "perFlake" ? cfg.perFlake : { on:false },
    cap:      rule === "cap"      ? cfg.cap      : { on:false },
  }).size;
  return {
    total: cfArchived(kind, list, { ...cfg, enabled:true }).size,
    age: cfg.age?.on ? one("age") : 0,
    perFlake: cfg.perFlake?.on ? one("perFlake") : 0,
    cap: cfg.cap?.on ? one("cap") : 0,
  };
}

function cfRetentionLists() {
  return {
    builds: typeof HISTORY_BUILDS !== "undefined" ? HISTORY_BUILDS : [],
    evals:  typeof HISTORY_EVALS  !== "undefined" ? HISTORY_EVALS  : [],
    scans:  [
      ...(typeof SCAN_COMPLETED_HISTORY !== "undefined" ? SCAN_COMPLETED_HISTORY : []),
      ...(typeof SCAN_HISTORY !== "undefined" ? SCAN_HISTORY.flatMap(s => s.commits || []) : []),
    ],
  };
}

Object.assign(window, {
  cfRetention, cfArchived, cfRetentionPreview, cfRetentionLists,
  RETENTION_KINDS, RETENTION_DEFAULTS,
});
