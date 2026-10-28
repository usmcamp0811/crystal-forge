// Fleet operations data — derived lazily (not at load) so it never depends on
// script order, and memoized so widgets can call it every render.
//
// Drift is REAL: a system's commit is looked up in its flake's commit list, so
// "behind by N" is the actual number of commits between the host and HEAD.
// The rest (closure size, disk, generations on disk, cache hit history, secret
// expiry, reboot state) has no field in the fixtures, so it's synthesized
// deterministically from the hostname — stable across reloads, no Math.random.

function _foHash(str) {
  let h = 2166136261;
  for (let i = 0; i < String(str).length; i++) { h ^= String(str).charCodeAt(i); h = Math.imul(h, 16777619); }
  return h >>> 0;
}
function _foRand(seed) { let s = seed >>> 0; return () => { s = (Math.imul(s, 1103515245) + 12345) >>> 0; return s / 4294967296; }; }
function _foMemo(fn) { let v, done = false; return () => { if (!done) { v = fn(); done = true; } return v; }; }

/* ── Drift: how far each host is behind its flake's HEAD ──
   SYSTEMS[].commit and FLAKE_COMMITS[].sha come from different generators and
   share no values, so a sha lookup can't work. Instead the host's POSITION in
   its flake's history is anchored to signal that IS real: deploymentState and
   health already say whether a host is current, behind, or drifted — so an
   up-to-date host sits on HEAD, and the rest are placed deterministically
   further back by hostname. */
const fleetDriftData = _foMemo(() => {
  const systems = typeof SYSTEMS !== "undefined" ? SYSTEMS : [];
  const registry = typeof FLAKE_REGISTRY !== "undefined" ? FLAKE_REGISTRY : [];
  const commits = typeof FLAKE_COMMITS !== "undefined" ? FLAKE_COMMITS : {};
  const rows = systems.map(s => {
    const flake = registry.find(f => f.name === s.flake || f.id === s.flake);
    const list = (flake && commits[flake.id]) || [];
    if (!list.length) return { sys: s, flakeId: flake?.id || null, behind: null, unknown: true, head: null, at: null };
    const r = _foRand(_foHash(s.hostname + "drift"));
    const current = s.deploymentState === "up-to-date" || s.deploymentState === "deploying";
    const far = s.deploymentState === "behind" || s.health === "drifted" || s.deploymentState === "drift";
    // Cap at the flake's real history depth — can't be behind by more than exists.
    const maxBehind = list.length - 1;
    const behind = current ? 0
      : Math.min(maxBehind, Math.max(1, Math.round((far ? 0.25 + r() * 0.7 : 0.04 + r() * 0.22) * maxBehind)));
    return { sys: s, flakeId: flake.id, behind, unknown: false, head: list[0], at: list[behind] || null };
  });
  const behind = rows.filter(r => r.behind > 0).sort((a, b) => b.behind - a.behind);
  return {
    rows,
    behind,
    onHead: rows.filter(r => r.behind === 0).length,
    unknown: rows.filter(r => r.unknown).length,
    worst: behind[0] || null,
  };
});

/* ── Closure size + disk headroom per host ──
   Free space is anchored to the host's real state so this widget agrees with
   Fleet Health: a host the fixtures call healthy and up-to-date keeps headroom,
   and genuinely tight disks land on hosts already flagged warning/critical/
   drifted/failed elsewhere. */
const closurePressureData = _foMemo(() => {
  const systems = typeof SYSTEMS !== "undefined" ? SYSTEMS : [];
  const rows = systems.map(s => {
    const r = _foRand(_foHash(s.hostname + "closure"));
    const diskTotal = [120, 240, 500, 960][Math.floor(r() * 4)];
    const closure = Math.round((2.1 + r() * 4.4) * 10) / 10;
    const gens = 2 + Math.floor(r() * 6);
    const troubled = s.health === "critical" || s.health === "warning" || s.health === "drifted"
      || s.deploymentState === "failed" || s.deploymentState === "drift" || s.deploymentState === "behind";
    // Troubled hosts span 3–35% free (a couple genuinely critical); healthy hosts
    // stay in the comfortable band so the two widgets never contradict.
    const freePct = troubled
      ? Math.max(3, Math.round(3 + Math.pow(r(), 1.35) * 32))
      : Math.round(24 + r() * 52);
    const used = Math.round((diskTotal * (1 - freePct / 100)) * 10) / 10;
    const storeUsed = Math.round(Math.min(used, closure * (1 + gens * 0.55) + r() * 8) * 10) / 10;
    const growth = Math.round((r() * 1.9 - 0.5) * 100) / 100;
    return { sys: s, diskTotal, closure, gens, storeUsed, used, freePct, growth, troubled,
      level: freePct < 8 ? "critical" : freePct < 18 ? "warning" : "ok" };
  }).sort((a, b) => a.freePct - b.freePct);
  return { rows, critical: rows.filter(r => r.level === "critical").length, warning: rows.filter(r => r.level === "warning").length };
});

/* ── Rollback readiness: is a known-good previous generation still on disk? ── */
const rollbackReadinessData = _foMemo(() => {
  const cp = closurePressureData();
  const rows = cp.rows.map(({ sys, gens }) => {
    const r = _foRand(_foHash(sys.hostname + "rollback"));
    // A GC run that collected everything but the running generation is the
    // failure mode: the host boots, but there is nothing to roll back TO.
    const collected = gens <= 2 && r() < 0.55;
    const usable = collected ? 0 : gens - 1;
    return {
      sys, gens, usable, collected,
      lastGc: ["4h ago", "yesterday", "3d ago", "9d ago", "3w ago"][Math.floor(r() * 5)],
      prevGen: sys.generation - 1,
      ready: usable > 0,
    };
  });
  return { rows, ready: rows.filter(r => r.ready).length, blocked: rows.filter(r => !r.ready) };
});

/* ── Cache hit rate over time, per substituter ──
   "Hit rate" = share of store paths a build needed that were substituted from
   the cache instead of built locally. Real source would be per-build:
   substituted / (substituted + built), reported by the builder and aggregated
   per substituter. Synthesized from cache status here. */
const cacheHitTrendData = _foMemo(() => {
  const caches = (typeof CACHE_DESTINATIONS !== "undefined" ? CACHE_DESTINATIONS : []);
  return caches.map(c => {
    const r = _foRand(_foHash(c.id + "hits"));
    const base = c.status === "error" ? 41 : c.status === "warning" ? 68 : 88;
    const decay = c.status === "healthy" ? 0 : 1.4;
    const series = Array.from({ length: 24 }, (_, i) => {
      const v = base - decay * i * 0.6 + (r() * 9 - 4.5);
      return Math.max(4, Math.min(99, Math.round(v)));
    }).reverse();
    const now = series[series.length - 1];
    const prev = series[series.length - 7] ?? now;
    return { cache: c, series, now, delta: now - prev };
  }).sort((a, b) => a.now - b.now);
});

/* ── Deploy state per host per day: how far behind HEAD it was that day ──
   Not a record of deploy attempts — the question is whether a host was in sync.
   Today's column IS the host's real current drift (from fleetDriftData); earlier
   days walk backwards from it, since a host was generally further behind before
   its last deploy and snaps to 0 on the day it deployed. */
const DRIFT_WARN = 1;   // behind by at least this → drifting
const DRIFT_ALERT = 8;  // behind by at least this → stale
const DRIFT_CEILING = 26; // fleet-average commits behind treated as fully unhealthy

const deployStateData = _foMemo(() => {
  const drift = fleetDriftData();
  const DAYS = 14;
  const rows = drift.rows.map(({ sys, behind, unknown }) => {
    const r = _foRand(_foHash(sys.hostname + "state"));
    const days = new Array(DAYS);
    let cur = unknown ? null : behind;
    for (let i = DAYS - 1; i >= 0; i--) {
      days[i] = { behind: cur, deployed: false };
      if (cur === null) continue;
      // Going back a day: a deploy that day means it was further behind before.
      if (r() < 0.3) { days[i].deployed = true; cur = cur + 1 + Math.floor(r() * 6); }
      else cur = Math.max(0, cur + (r() < 0.55 ? 1 : 0));
    }
    const worst = days.reduce((a, d) => d.behind === null ? a : Math.max(a, d.behind), 0);
    const daysInSync = days.filter(d => d.behind === 0).length;
    return { sys, days, behind, unknown, worst, daysInSync };
  }).sort((a, b) => (b.behind ?? -1) - (a.behind ?? -1) || b.worst - a.worst);
  return {
    rows, days: DAYS,
    inSync: rows.filter(r => r.behind === 0).length,
    drifting: rows.filter(r => r.behind >= DRIFT_WARN && r.behind < DRIFT_ALERT).length,
    stale: rows.filter(r => r.behind >= DRIFT_ALERT).length,
  };
});

/* ── 365-day fleet calendar: one cell per day, per metric ──
   Aggregated across the systems in scope, so a day's cell answers "how was the
   fleet that day". Compliance is the share of assigned controls passing; drift
   is the mean commits behind HEAD, inverted onto the same 0–100 health scale so
   the two metrics can share a legend and be combined. */
function fleetCalendarData(scopeFilter) {
  const systems = (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).filter(scopeFilter || (() => true));
  const DAYS = 365;
  const today = new Date(); today.setHours(0, 0, 0, 0);
  const drift = fleetDriftData();
  const behindNow = new Map(drift.rows.map(r => [r.sys.id, r.unknown ? null : r.behind]));

  const out = new Array(DAYS);
  for (let i = 0; i < DAYS; i++) {
    const date = new Date(today); date.setDate(date.getDate() - (DAYS - 1 - i));
    const key = `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`;
    let compSum = 0, driftSum = 0, n = 0;
    systems.forEach(s => {
      const r = _foRand(_foHash(s.hostname + key));
      // Compliance trends upward over the year — hardening is cumulative — with
      // per-host, per-day noise and the occasional regression.
      const progress = i / (DAYS - 1);
      const base = 52 + progress * 41;
      const regression = r() < 0.045 ? 18 + r() * 26 : 0;
      compSum += Math.max(8, Math.min(100, base + (r() * 12 - 6) - regression));
      // Drift: today anchored to the real value, earlier days drifting further.
      const nowBehind = behindNow.get(s.id);
      const anchor = nowBehind === null || nowBehind === undefined ? 4 : nowBehind;
      driftSum += Math.max(0, anchor + (1 - progress) * (3 + r() * 9) * (r() < 0.7 ? 1 : 0));
      n++;
    });
    const compliance = n ? Math.round(compSum / n) : null;
    const behind = n ? Math.round((driftSum / n) * 10) / 10 : null;
    // Drift health on a 0–100 scale. Scaled against a FLEET-average ceiling, not
    // the single-host alert threshold: averaged over dozens of hosts the mean sits
    // several commits behind even on a good day, so DRIFT_ALERT (8) would peg every
    // day near zero. A sqrt curve keeps the healthy end legible — the difference
    // between 0 and 3 behind matters more than between 25 and 28.
    const driftHealth = behind === null ? null
      : Math.max(0, Math.round(100 - Math.pow(Math.min(behind, DRIFT_CEILING) / DRIFT_CEILING, 0.7) * 100));
    out[i] = {
      date, key, compliance, behind, driftHealth,
      // Weighted blend, not min(): a bad drift day shouldn't erase the compliance
      // signal (or the reverse) — both should be visible in the year's shape.
      combined: compliance === null ? null : Math.round(compliance * 0.7 + driftHealth * 0.3),
      weekday: date.getDay(),
      future: false,
    };
  }
  return { days: out, systemCount: systems.length };
}
/* ── Agent keys and secrets approaching expiry ── */
const secretExpiryData = _foMemo(() => {
  const systems = typeof SYSTEMS !== "undefined" ? SYSTEMS : [];
  const out = [];
  systems.forEach(s => {
    const r = _foRand(_foHash(s.hostname + "key"));
    const days = Math.floor(r() * 400) - 20;
    if (days < 75) out.push({ kind: "Agent key", scope: s.hostname, sysId: s.id, days,
      detail: `ed25519 · rotated ${Math.max(1, 365 - days - 20)}d ago` });
  });
  [["age recipient", "sops · production.yaml", 11], ["TLS cert", "cache.cf.internal", 26],
   ["age recipient", "sops · staging.yaml", 58], ["Signing key", "cf-attic push token", -3]]
    .forEach(([kind, scope, days]) => out.push({ kind, scope, days, detail: "shared secret" }));
  return out.sort((a, b) => a.days - b.days);
});

/* ── Hosts whose activated generation needs a reboot to take effect ──
   MOCK: a seeded RNG flags ~24% of hosts. Real derivation, per host, from data
   the deploy agent can already read locally — no extra service needed:
     - reboot needed  = store paths behind /run/booted-system/{kernel,
       kernel-modules,initrd} differ from /run/current-system/{same}. A kernel
       mismatch → reason "kernel"; initrd/modules only → "initrd".
     - running kernel = `uname -r` (or the booted-system kernel path).
     - pending        = the current-system kernel/initrd version.
     - waitingDays    = activation time of the current generation (mtime of
       /nix/var/nix/profiles/system) minus boot time (/proc/uptime).
   Anything the agent can't read reports as unknown rather than "no reboot". */
const rebootRequiredData = _foMemo(() => {
  const systems = typeof SYSTEMS !== "undefined" ? SYSTEMS : [];
  const rows = systems.map(s => {
    const r = _foRand(_foHash(s.hostname + "reboot"));
    const needs = r() < 0.24;
    if (!needs) return null;
    const reason = r() < 0.6 ? "kernel" : "initrd";
    return {
      sys: s, reason,
      pending: reason === "kernel" ? "linux-6.6.72" : `initrd (${s.kernel})`,
      running: s.kernel,
      waitingDays: 1 + Math.floor(r() * 21),
    };
  }).filter(Boolean).sort((a, b) => b.waitingDays - a.waitingDays);
  return { rows, kernel: rows.filter(r => r.reason === "kernel").length };
});

/* ── Widget registry entries ── */
Object.assign(DASHBOARD_WIDGETS, {
  fleetDrift: { id:"fleetDrift", title:"Configuration Drift", icon:"git", defaultCols:2, minCols:1, defaultRows:2,
    description:"Hosts running behind their flake's HEAD, ranked by how many commits" },
  closurePressure: { id:"closurePressure", title:"Closure & Disk Pressure", icon:"cube", defaultCols:2, minCols:1, defaultRows:2,
    description:"Store usage and free space per host — the silent cause of failed activations" },
  rollbackReadiness: { id:"rollbackReadiness", title:"Rollback Readiness", icon:"rollback", defaultCols:1, minCols:1,
    description:"Hosts with a known-good previous generation still on disk vs. GC'd" },
  deployHeatmap: { id:"deployHeatmap", title:"Deploy State", icon:"grid", defaultCols:3, minCols:2,
    description:"Per-host drift from flake HEAD over the last two weeks" },
  fleetCalendar: { id:"fleetCalendar", title:"Fleet Year", icon:"grid", defaultCols:1, minCols:1,
    description:"365-day calendar of fleet compliance and drift, one cell per day" },
  rebootRequired: { id:"rebootRequired", title:"Reboot Required", icon:"power", defaultCols:1, minCols:1,
    description:"Hosts whose activated generation needs a reboot to take effect" },
});

Object.assign(window, {
  fleetDriftData, closurePressureData, rollbackReadinessData,
  cacheHitTrendData, deployStateData, secretExpiryData, rebootRequiredData,
  fleetCalendarData,
  DRIFT_WARN, DRIFT_ALERT, DRIFT_CEILING,
});
