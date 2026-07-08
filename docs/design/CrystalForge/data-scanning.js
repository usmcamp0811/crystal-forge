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
  complete:  { label:"Complete",  color:"#34d399", cls:"chip-healthy" },
  failed:    { label:"Failed",    color:"#f87171", cls:"chip-critical" },
  stale:     { label:"Stale",     color:"#fbbf24", cls:"chip-warning" },
  "needs-build": { label:"Needs build", color:"#f59e0b", cls:"chip-warning" },
  unscanned: { label:"Never scanned", color:"#9ca3af", cls:"chip-unknown" },
};

const SCAN_CONFIGS = (typeof __fx === "function" && __fx("scanning.configs")) || [
  // freshness: deployed | recent | archived
  { id:"sc-1",  name:"gaia-web-01",  flake:"web-services",   commit:"c7e1902", freshness:"deployed", status:"scanning", progress:0.62, found:{crit:0,high:2,med:5}, lastScan:"scanning…", trigger:"post-build" },
  { id:"sc-2",  name:"atlas-01",     flake:"infrastructure", commit:"a3f8c12", freshness:"deployed", status:"complete", found:{crit:1,high:3,med:8}, lastScan:"4m ago", trigger:"scheduled" },
  { id:"sc-3",  name:"orion-db-01",  flake:"infrastructure", commit:"a3f8c12", freshness:"deployed", status:"complete", found:{crit:0,high:1,med:4}, lastScan:"12m ago", trigger:"scheduled" },
  { id:"sc-4",  name:"edge-pdx-01",  flake:"edge-gateway",   commit:"4d2a801", freshness:"deployed", status:"stale",    found:{crit:2,high:4,med:9}, lastScan:"9h ago", trigger:"scheduled" },
  { id:"sc-5",  name:"hydra-03",     flake:"build-farm",     commit:"9f0c344", freshness:"recent",   status:"queued",   found:null, lastScan:"pending", trigger:"post-build" },
  { id:"sc-6",  name:"stg-web-02",   flake:"web-services",   commit:"2fa8031", freshness:"recent",   status:"complete", found:{crit:0,high:0,med:2}, lastScan:"2h ago", trigger:"scheduled" },
  { id:"sc-7",  name:"gaia-web-03",  flake:"web-services",   commit:"d90c411", freshness:"deployed", status:"failed",   found:null, lastScan:"failed 18m ago", trigger:"scheduled", error:"vulnix: derivation not in store" },
  { id:"sc-8",  name:"lab-vm-01",    flake:"lab-nodes",      commit:"1b7e5f0", freshness:"archived", status:"unscanned",found:null, lastScan:"never", trigger:null },
  { id:"sc-9",  name:"dev-node-02",  flake:"infrastructure", commit:"8c4b311", freshness:"recent",   status:"complete", found:{crit:0,high:1,med:3}, lastScan:"5h ago", trigger:"scheduled" },
  { id:"sc-10", name:"edge-nyc-01",  flake:"edge-gateway",   commit:"9a01fc2", freshness:"archived", status:"stale",    found:{crit:1,high:2,med:6}, lastScan:"21d ago", trigger:"scheduled" },
];

const SCAN_STATS = {
  scanning: SCAN_CONFIGS.filter(s=>s.status==="scanning").length,
  queued:   SCAN_CONFIGS.filter(s=>s.status==="queued").length,
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

Object.assign(window, { SCAN_POLICY, SCAN_INTERVALS, SCAN_CONFIGS, SCAN_STATS, SCAN_STATUS_META, SCAN_ACTIVITY });

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
