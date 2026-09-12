// Scanning (CVE scan pipeline) mock data

// Scan schedule policy — configurable cadence per config "freshness"
const SCAN_POLICY = {
  onBuild: true,             // always scan freshly-built configs before deploy
  deployedInterval: "6h",    // rescan currently-deployed configs
  recentInterval: "24h",     // configs built in last 30d but not deployed
  archivedInterval: "30d",   // old / superseded configs
  archivedEnabled: true,
  vulnixVersion: "1.10.1",
  dbAge: "2h ago",           // vulnerability DB freshness
};

const SCAN_INTERVALS = ["1h", "6h", "12h", "24h", "7d", "30d", "never"];

// Scan jobs — what's currently being scanned + recent results
function _scanSeed(i) { let s = i*7919+13; return () => { s=(s*9301+49297)%233280; return s/233280; }; }

const SCAN_STATUS_META = {
  scanning:  { label:"Scanning",  color:"#60a5fa", cls:"chip-info" },
  queued:    { label:"Queued",    color:"#a78bfa", cls:"chip-info" },
  // Scanning runs async from building. A config can be known and wanted but have no
  // realised closure yet (build still running, or built on a remote builder and not
  // pushed to a cache we can substitute from). That is a waiting state, not a failure.
  awaiting:  { label:"Awaiting closure", color:"#94a3b8", cls:"chip-unknown" },
  complete:  { label:"Complete",  color:"#34d399", cls:"chip-healthy" },
  failed:    { label:"Failed",    color:"#f87171", cls:"chip-critical" },
  stale:     { label:"Stale",     color:"#fbbf24", cls:"chip-warning" },
  "needs-build": { label:"Needs build", color:"#f59e0b", cls:"chip-warning" },
  unscanned: { label:"Never scanned", color:"#9ca3af", cls:"chip-unknown" },
};

const SCAN_CONFIGS = (typeof __fx === "function" && __fx("scanning.configs")) || [
  // freshness: deployed | recent | archived
  { id:"sc-1",  name:"gaia-web-01",  flake:"web-services",   commit:"c7e1902", freshness:"deployed", status:"scanning", startedAgo:"48s", found:{crit:0,high:2,med:5}, lastScan:"scanning…", trigger:"post-build" },
  { id:"sc-2",  name:"atlas-01",     flake:"infrastructure", commit:"a3f8c12", freshness:"deployed", status:"complete", found:{crit:1,high:3,med:8}, lastScan:"4m ago", trigger:"scheduled" },
  { id:"sc-3",  name:"orion-db-01",  flake:"infrastructure", commit:"a3f8c12", freshness:"deployed", status:"complete", found:{crit:0,high:1,med:4}, lastScan:"12m ago", trigger:"scheduled" },
  { id:"sc-4",  name:"edge-pdx-01",  flake:"edge-gateway",   commit:"4d2a801", freshness:"deployed", status:"stale",    found:{crit:2,high:4,med:9}, lastScan:"9h ago", trigger:"scheduled" },
  { id:"sc-5",  name:"hydra-03",     flake:"build-farm",     commit:"9f0c344", freshness:"recent",   status:"queued",   found:null, lastScan:"pending", trigger:"post-build" },
  { id:"sc-11", name:"gaia-web-02",  flake:"web-services",   commit:"c7e1902", freshness:"deployed", status:"awaiting", found:null, lastScan:"waiting 6m", trigger:"post-build", awaiting:"building", awaitingDetail:"build in progress on hydra-03" },
  { id:"sc-12", name:"edge-sfo-01",  flake:"edge-gateway",   commit:"4d2a801", freshness:"recent",   status:"awaiting", found:null, lastScan:"waiting 41m", trigger:"post-build", awaiting:"not-cached", awaitingDetail:"built on remote builder · not yet pushed to a reachable cache" },
  { id:"sc-6",  name:"stg-web-02",   flake:"web-services",   commit:"2fa8031", freshness:"recent",   status:"complete", found:{crit:0,high:0,med:2}, lastScan:"2h ago", trigger:"scheduled" },
  { id:"sc-7",  name:"gaia-web-03",  flake:"web-services",   commit:"d90c411", freshness:"deployed", status:"failed",   found:null, lastScan:"failed 18m ago", trigger:"scheduled", error:"vulnix: derivation not in store" },
  { id:"sc-8",  name:"lab-vm-01",    flake:"lab-nodes",      commit:"1b7e5f0", freshness:"archived", status:"unscanned",found:null, lastScan:"never", trigger:null },
  { id:"sc-9",  name:"dev-node-02",  flake:"infrastructure", commit:"8c4b311", freshness:"recent",   status:"complete", found:{crit:0,high:1,med:3}, lastScan:"5h ago", trigger:"scheduled" },
  { id:"sc-10", name:"edge-nyc-01",  flake:"edge-gateway",   commit:"9a01fc2", freshness:"archived", status:"stale",    found:{crit:1,high:2,med:6}, lastScan:"21d ago", trigger:"scheduled" },
];

const SCAN_STATS = {
  scanning: SCAN_CONFIGS.filter(s=>s.status==="scanning").length,
  queued:   SCAN_CONFIGS.filter(s=>s.status==="queued").length,
  awaiting: SCAN_CONFIGS.filter(s=>s.status==="awaiting").length,
  stale:    SCAN_CONFIGS.filter(s=>s.status==="stale").length,
  unscanned:SCAN_CONFIGS.filter(s=>s.status==="unscanned").length,
  failed:   SCAN_CONFIGS.filter(s=>s.status==="failed").length,
  coverage: Math.round(SCAN_CONFIGS.filter(s=>s.status==="complete"||s.status==="scanning").length / SCAN_CONFIGS.length * 100),
};

// Recent scan activity feed
const SCAN_ACTIVITY = (typeof __fx === "function" && __fx("scanning.activity")) || [
  { at:"just now", name:"gaia-web-01", event:"Scan started", detail:"post-build trigger · vulnix 1.10.1", color:"#60a5fa", icon:"shield" },
  { at:"4m ago",   name:"atlas-01",   event:"Scan complete", detail:"1 critical, 3 high, 8 medium found", color:"#34d399", icon:"check" },
  { at:"12m ago",  name:"orion-db-01",event:"Scan complete", detail:"1 high, 4 medium · clean of criticals", color:"#34d399", icon:"check" },
  { at:"18m ago",  name:"gaia-web-03",event:"Scan failed",   detail:"derivation not in store — rebuild needed", color:"#f87171", icon:"warn" },
  { at:"1h ago",   name:"vuln-db",    event:"Vulnerability DB updated", detail:"NVD feed synced · 412 new advisories", color:"#a78bfa", icon:"sync" },
  { at:"2h ago",   name:"stg-web-02", event:"Scan complete", detail:"2 medium found", color:"#34d399", icon:"check" },
];

Object.assign(window, { SCAN_POLICY, SCAN_INTERVALS, SCAN_CONFIGS, SCAN_STATS, SCAN_STATUS_META, SCAN_ACTIVITY, scanLogLines });

// Scan log lines (mock) — vulnix output for a config. Failed scans get the real reason
// plus a stack-ish tail, since "why did this fail" is the whole point of opening the log.
function scanLogLines(cfg) {
  const pkgs = ["glibc-2.40","openssl-3.3.2","zlib-1.3.1","systemd-256.7","linux-6.12.4",
    "python3-3.12.7","curl-8.11.0","nginx-1.27.4","openssh-9.9p1","sqlite-3.47.0",
    "libxml2-2.13.4","pcre2-10.44","gnutls-3.8.8","expat-2.6.4"];
  const t0 = 0;
  const L = [];
  let sec = t0;
  const stamp = () => {
    const m = String(Math.floor(sec / 60)).padStart(2,"0");
    const s = String(sec % 60).padStart(2,"0");
    return `00:${m}:${s}`;
  };
  const push = (lvl, m, adv = 1) => { L.push({ t: stamp(), lvl, m }); sec += adv; };

  push("info", `vulnix ${SCAN_POLICY.vulnixVersion} · scan requested for ${cfg.name}`);
  push("info", `trigger: ${cfg.trigger || "manual"} · flake ${cfg.flake} @ ${cfg.commit}`);
  push("info", `resolving nixosConfigurations.${cfg.name}.config.system.build.toplevel`, 2);

  if (cfg.status === "failed") {
    push("info", "querying local store for derivation closure");
    push("warn", `path /nix/store/…-nixos-system-${cfg.name} not present in store`);
    push("warn", "no substituter provided the closure (tried 2 caches)", 2);
    push("error", cfg.error || "vulnix: derivation not available");
    push("error", "  ↳ closure must be built or fetched before scanning", 0);
    push("error", "  ↳ hint: build this config, or run with --no-closure to scan metadata only", 0);
    push("error", `scan aborted after ${sec}s · exit code 1`, 0);
    return L;
  }

  if (cfg.status === "awaiting") {
    push("info", "querying local store for derivation closure");
    if (cfg.awaiting === "building") {
      push("warn", "closure not realised — a build for this derivation is still running");
      push("info", "scan deferred · will start automatically when the build completes", 0);
    } else {
      push("warn", "closure not present in local store");
      push("warn", "no configured substituter has this path yet (tried 2 caches)");
      push("info", `scan deferred · retrying every ${SCAN_INTERVALS?.retry || "5m"} until the closure is available`, 0);
    }
    return L;
  }

  if (cfg.status === "unscanned") {
    push("info", "no scan has been run for this config");
    return L;
  }

  push("info", "derivation closure resolved · 1,284 store paths", 2);
  push("info", `loading vulnerability database (updated ${SCAN_POLICY.dbAge})`, 2);
  push("info", "matching store paths against CVE feed", 1);

  const found = cfg.found || { crit:0, high:0, med:0 };
  const hits = [];
  for (let i = 0; i < found.crit; i++) hits.push(["error", "CRITICAL"]);
  for (let i = 0; i < found.high; i++) hits.push(["warn", "HIGH"]);
  for (let i = 0; i < found.med; i++) hits.push(["warn", "MEDIUM"]);
  hits.forEach(([lvl, sev], i) => {
    const pkg = pkgs[i % pkgs.length];
    const cve = `CVE-2026-${String(1000 + ((i * 137) % 8999)).padStart(4,"0")}`;
    push(lvl, `${sev.padEnd(8)} ${cve}  ${pkg}`, i % 3 === 2 ? 1 : 0);
  });

  if (cfg.status === "scanning") {
    push("info", "matching remaining paths…", 0);
    return L;
  }

  push("info", `scan complete · ${found.crit} critical, ${found.high} high, ${found.med} medium`, 1);
  push("info", `results written · exit code 0`, 0);
  return L;
}

// Per-system scan history — every system, each with its commit scan records.
// "All configs" view groups by system; expanding shows each commit's scan.
function buildScanHistory() {
  const COMMIT_MSGS = ["bump nixpkgs", "stig: audit rules", "cve: patch openssl", "harden sshd", "add node exporter", "fix postgres perms"];
  return (window.SYSTEMS || []).map(sys => {
    let s = sys.hostname.split("").reduce((a,c)=>a+c.charCodeAt(0),0);
    const r = () => { s = (s*9301+49297)%233280; return s/233280; };
    // number of historical configs (commits this system has been on)
    // first prod host gets a long history to demo scrolling
    const longHistory = sys.hostname === "atlas-01";
    const n = longHistory ? 54 : 2 + Math.floor(r()*5);
    const commits = [];
    for (let i = 0; i < n; i++) {
      const isCurrent = i === 0;
      const fresh = isCurrent ? "deployed" : i < 2 ? "recent" : "archived";
      // current always scanned; older may be unscanned/stale/needs-build
      let status;
      if (isCurrent) status = sys.health === "critical" ? "complete" : (r() < 0.15 ? "scanning" : "complete");
      else if (fresh === "recent") status = r() < 0.7 ? "complete" : "stale";
      else status = r() < 0.35 ? "complete" : r() < 0.55 ? "stale" : r() < 0.8 ? "needs-build" : "unscanned";

      // needs-build / unscanned configs have no cached derivation → no findings
      const cached = status !== "needs-build" && status !== "unscanned";
      const hasFindings = status === "complete" || status === "stale";
      const crit = isCurrent ? sys.cves.critical : Math.floor(r()*2);
      const high = hasFindings ? (isCurrent ? sys.cves.high : Math.floor(r()*4)) : 0;
      const med  = hasFindings ? Math.floor(r()*8) : 0;
      commits.push({
        commit: (i===0 ? sys.commit : Array.from({length:7},()=>"0123456789abcdef"[Math.floor(r()*16)]).join("")),
        msg: COMMIT_MSGS[Math.floor(r()*COMMIT_MSGS.length)],
        freshness: fresh,
        current: isCurrent,
        status,
        found: hasFindings ? { crit, high, med } : null,
        cached: hasFindings || isCurrent,
        lastScan: status === "scanning" ? "scanning…" :
                  status === "needs-build" ? "not in cache" :
                  status === "unscanned" ? "never" :
                  isCurrent ? `${Math.floor(r()*30)+1}m ago` :
                  fresh === "recent" ? `${Math.floor(r()*12)+1}h ago` : `${Math.floor(r()*28)+2}d ago`,
        trigger: (status === "unscanned" || status === "needs-build") ? null : (isCurrent && r()<0.4 ? "post-build" : "scheduled"),
      });
    }
    const worst = commits.reduce((acc,c)=>{
      if (!c.found) return acc;
      return { crit: acc.crit + c.found.crit, high: acc.high + c.found.high };
    }, { crit:0, high:0 });
    return {
      id: sys.id,
      hostname: sys.hostname,
      flake: sys.flake,
      environment: sys.environment,
      statusColor: sys.statusColor,
      commits,
      totalConfigs: commits.length,
      scanned: commits.filter(c => c.status === "complete" || c.status === "scanning").length,
      stale: commits.filter(c => c.status === "stale").length,
      needsBuild: commits.filter(c => c.status === "needs-build").length,
      unscanned: commits.filter(c => c.status === "unscanned").length,
      currentCrit: commits[0]?.found?.crit || 0,
      currentHigh: commits[0]?.found?.high || 0,
    };
  });
}
const SCAN_HISTORY = (typeof __fx === "function" && __fx("scanning.history")) || buildScanHistory();
Object.assign(window, { SCAN_HISTORY });
