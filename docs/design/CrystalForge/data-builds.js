// Mock data: systemd hardening, build queue, evaluations

/* ─── systemd service hardening ─── */
const SD_COLS = ["PrivateTmp","PrivateDevices","ProtectSystem","ProtectHome",
  "NoNewPrivileges","CapabilityBoundingSet","MemoryDenyWriteExecute",
  "SystemCallFilter","RestrictNamespaces","ProtectKernelModules",
  "ProtectKernelTunables","ProtectClock","RestrictSUIDSGID","LockPersonality",
  "ProtectControlGroups","PrivateNetwork","PrivateUsers","AmbientCapabilities",
  "ProcSubset","ProtectProc","SystemCallArchitectures","RestrictRealtime"];

const SD_SERVICES = [
  "sshd","nginx","postgresql","crystal-forge-server","crystal-forge-builder",
  "crystal-forge-agent","prometheus-node-exporter","grafana","nix-daemon","redis",
  "systemd-journald","systemd-networkd","systemd-resolved","systemd-logind",
  "auditd","rsyslog","cron","docker","containerd","vault",
  "systemd-udevd","dbus","polkit","avahi-daemon","cups",
  "systemd-oomd","systemd-coredump","fapolicyd","tailscaled","wireguard",
];

const SD_NIX = {
  sshd: `services.openssh.settings = {\n  PrivateTmp = true;\n  NoNewPrivileges = true;\n  CapabilityBoundingSet = "CAP_NET_BIND_SERVICE";\n};`,
  nginx: `systemd.services.nginx.serviceConfig = {\n  PrivateTmp = true;\n  PrivateDevices = true;\n  ProtectSystem = "strict";\n  NoNewPrivileges = true;\n};`,
  default: `# Service uses default systemd unit from upstream\n# Add overrides via:\nsystemd.services.<name>.serviceConfig = {\n  PrivateTmp = true;\n  NoNewPrivileges = true;\n  ProtectSystem = "strict";\n};`,
};

function sdScore(name, i) {
  let s = name.split("").reduce((a,c) => a+c.charCodeAt(0), 0) + i*97;
  const r = () => { s=(s*9301+49297)%233280; return s/233280; };
  // Well-known hardened services
  const good = ["sshd","crystal-forge-server","crystal-forge-builder","crystal-forge-agent","nix-daemon","prometheus-node-exporter"];
  const prob = good.includes(name) ? 0.7 : 0.2 + r()*0.25;
  const enabled = SD_COLS.map(c => r() < prob);
  const pts = enabled.filter(Boolean).length;
  const score = Math.round(pts / SD_COLS.length * 100);
  const risk = score >= 70 ? "OK" : score >= 40 ? "MED" : score >= 15 ? "HIGH" : "VULN";
  return {
    id: `svc-${i}`, name,
    score, risk,
    riskColor: {OK:"#34d399",MED:"#fbbf24",HIGH:"#f97316",VULN:"#f87171"}[risk],
    enabled, // bool[] per SD_COLS
    missing: SD_COLS.length - pts,
    nixSnippet: SD_NIX[name] || SD_NIX.default,
    user: r()<0.15 ? "root" : r()<0.4 ? "nobody" : `svc-${name.split("-")[0]}`,
    notes: Math.floor(r()*5),
  };
}

const HARDENING_SERVICES = SD_SERVICES.map(sdScore);

/* ─── Build workers ─── */
const BUILD_WORKERS = (typeof __fx === "function" && __fx("builds.workers")) || [
  { id:"w1", fingerprint:"SHA256:k7Hn2pQ9xR4mLwT0vBcZ8sJ1aD3eF6gY", registered:true,  name:"reckless-builder", host:"reckless-builder.lab",      arch:"x86_64-linux",   cores:16, mem:64,  slots:{used:0,total:1}, status:"running", load:0.02, lastSeen:"just now", uptimeDays:42,  completed24h:128, failed24h:1,  environments:["lab","dev"],                publicKey:"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH9rk2pQ9xR4mLwT0vBcZ8sJ1aD3eF6gY crystal-forge@reckless-builder" },
  { id:"w2", fingerprint:"SHA256:c04eD8a52f6gH7Lp1qWnM3zX9bV5tR8yK", registered:true,  name:"hydra-01",         host:"hydra-01.production",        arch:"x86_64-linux",   cores:64, mem:256, slots:{used:7,total:8}, status:"running", load:0.91, lastSeen:"2s ago",   uptimeDays:118, completed24h:842, failed24h:12, environments:["production","staging"],    publicKey:"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC04eD8a52f6gH7Lp1qWnM3zX9bV5tR8yK crystal-forge@hydra-01" },
  { id:"w3", fingerprint:"SHA256:9a2b7E60c3d1mQz4kP8xH2vN6rL0tW5yB", registered:true,  name:"hydra-02",         host:"hydra-02.production",        arch:"x86_64-linux",   cores:64, mem:256, slots:{used:5,total:8}, status:"running", load:0.62, lastSeen:"5s ago",   uptimeDays:118, completed24h:617, failed24h:8,  environments:["production"],              publicKey:"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAII9a2b7E60c3d1mQz4kP8xH2vN6rL0tW5yB crystal-forge@hydra-02" },
  { id:"w4", fingerprint:"SHA256:3e8f4A19d7b2ZqA1jK6xP9mN0rT5vL8wH", registered:true,  name:"graviton-01",      host:"build-arm-01.lab",           arch:"aarch64-linux",  cores:16, mem:64,  slots:{used:2,total:4}, status:"paused",  load:0.18, lastSeen:"4m ago",   uptimeDays:18,  completed24h:88,  failed24h:2,  environments:["lab","edge"],              publicKey:"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIN3e8f4A19d7b2ZqA1jK6xP9mN0rT5vL8wH crystal-forge@graviton-01" },
  { id:"w5", fingerprint:"SHA256:6d1c0B75f9e4XaTd2hJ8xP3mN7rL1tV5wK", registered:false, name:"darwin-01",        host:"mac-mini-01.lab",            arch:"aarch64-darwin", cores:8,  mem:16,  slots:{used:0,total:2}, status:"offline", load:0,    lastSeen:"2d ago",   uptimeDays:0,   completed24h:0,   failed24h:0,  environments:["lab"],                     publicKey:"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIO6d1c0B75f9e4XaTd2hJ8xP3mN7rL1tV5wK crystal-forge@darwin-01" },
];

/* ─── Build queue entries ─── */
const BUILD_SYSTEMS = [
  "atlas-01","gaia-web-01","gaia-web-03","orion-db-01","edge-pdx-01",
  "edge-nyc-01","stg-web-01","stg-build-01","dev-node-02","hydra-03",
  "lab-vm-01","atlas-02","gaia-web-02","edge-sgp-01","kepler-api",
];
const BUILD_PKGS = ["linux-6.6.72","openssl-3.3.2","nginx-1.27.4","postgresql-16.4",
  "glibc-2.40","curl-8.10.1","python3-3.11.10","rustc-1.84.0","nodejs-22.13.0",
  "systemd-256.7","grafana-11.4.0","vault-1.18.3","wireguard-tools-1.0"];

const BUILD_STATUS_META = {
  queued:        {label:"Queued",       color:"#a78bfa",cls:"chip-info"},
  building:      {label:"Building",     color:"#60a5fa",cls:"chip-info"},
  "cache-pushing":{label:"Pushing cache",color:"#22d3ee",cls:"chip-info"},
  "cache-pushed":{label:"Cached",       color:"#34d399",cls:"chip-healthy"},
  complete:      {label:"Complete",     color:"#34d399",cls:"chip-healthy"},
  failed:        {label:"Failed",       color:"#f87171",cls:"chip-critical"},
  cancelled:     {label:"Cancelled",    color:"#6b7280",cls:"chip-unknown"},
  stopping:      {label:"Stopping",     color:"#fbbf24",cls:"chip-warning"},
};

function mkBuild(i, forceStatus) {
  let s = i*7919+31; const r=()=>{s=(s*9301+49297)%233280;return s/233280;};
  const statuses = ["building","building","building","queued","queued","stopping","cache-pushing","cache-pushed","complete","complete","failed","cancelled"];
  const status = forceStatus || statuses[i % statuses.length];
  const host = BUILD_SYSTEMS[i % BUILD_SYSTEMS.length];
  const sysName = `nixos-system-${host}`;
  const flake = ["infrastructure","web-services","edge-gateway","build-farm"][Math.floor(r()*4)];
  const worker = ["building","cache-pushing","stopping"].includes(status) ? BUILD_WORKERS[i%3].name : null;
  const hash = Array.from({length:7},()=>"0123456789abcdef"[Math.floor(r()*16)]).join("");
  const totalDerivs = Math.floor(r()*120)+20;
  const prog = status==="building"?r():status==="cache-pushing"?0.9+r()*0.1:["complete","cache-pushed"].includes(status)?1:status==="failed"?r()*0.6:0;
  const builtDerivs = Math.round(totalDerivs*prog);
  const cachedDerivs = Math.round(builtDerivs*(0.4+r()*0.4));
  return {
    id:`bld-${i}`, system:host, name:sysName, flake,
    drv:`/nix/store/${hash}xxxx-${sysName}.drv`,
    commit: Array.from({length:7},()=>"0123456789abcdef"[Math.floor(r()*16)]).join(""),
    status, meta: BUILD_STATUS_META[status]||BUILD_STATUS_META.queued,
    worker, arch:r()<0.15?"aarch64-linux":"x86_64-linux",
    totalDerivs, builtDerivs, cachedDerivs,
    currentPkg: status==="building" ? BUILD_PKGS[Math.floor(r()*BUILD_PKGS.length)] : null,
    queuedAt:["just now","1m ago","4m ago","12m ago","28m ago","1h ago"][i%6],
    dur:["building","cache-pushing"].includes(status)?`${Math.floor(r()*300+10)}s`:
        ["complete","failed","cache-pushed"].includes(status)?`${Math.floor(r()*600+30)}s`:null,
    progress:prog,
    attempts:status==="failed"?Math.floor(r()*2)+2:1,
    logLines:Math.floor(r()*2000+50),
    failedPkg: status==="failed" ? BUILD_PKGS[Math.floor(r()*BUILD_PKGS.length)] : null,
  };
}

const ACTIVE_BUILDS  = (typeof __fx === "function" && __fx("builds.active"))  || [0,1,2,3,4,5].map(i => mkBuild(i));
const HISTORY_BUILDS = (typeof __fx === "function" && __fx("builds.history")) || Array.from({length:40},(_,i) => mkBuild(100+i));

const BUILD_STATS = {
  building: ACTIVE_BUILDS.filter(b=>b.status==="building").length,
  queued:   ACTIVE_BUILDS.filter(b=>b.status==="queued").length,
  failed24h:HISTORY_BUILDS.filter(b=>b.status==="failed").slice(0,8).length,
  workers:  BUILD_WORKERS.filter(w=>w.status==="running").length,
  totalWorkers: BUILD_WORKERS.length,
};

// Exposed to Babel components via window
window.buildSystemHardening = function(sys) {
  return HARDENING_SERVICES;
};

/* ─── Evaluations ─── */
const EVAL_STATUS_META = {
  pending:     {label:"Pending",     color:"#a78bfa",cls:"chip-info"},
  in_progress: {label:"Evaluating",  color:"#60a5fa",cls:"chip-info"},
  cancelling:  {label:"Cancelling",  color:"#fbbf24",cls:"chip-warning"},
  complete:    {label:"Complete",    color:"#34d399",cls:"chip-healthy"},
  failed:      {label:"Failed",      color:"#f87171",cls:"chip-critical"},
  cancelled:   {label:"Cancelled",   color:"#6b7280",cls:"chip-unknown"},
};

const EVAL_FLAKES = ["infrastructure","web-services","edge-gateway","build-farm","lab-nodes"];
const EVAL_BRANCHES = ["main","staging","dev","release/0.3"];

function mkEval(i, isHistoryFlag) {
  let s = i*6271+13; const r=()=>{s=(s*9301+49297)%233280;return s/233280;};
  const aStatuses=["in_progress","in_progress","pending","pending","cancelling"];
  const isHistory = !!isHistoryFlag;
  const flake = EVAL_FLAKES[i%EVAL_FLAKES.length];
  const roll = r();
  const status = isHistory ? (roll<0.6?"complete":roll<0.85?"failed":"cancelled") : aStatuses[i%5];
  const commit = Array.from({length:8},()=>"0123456789abcdef"[Math.floor(r()*16)]).join("");
  const systems = Math.floor(r()*20+3);
  const passed = Math.floor(systems*(r()*0.4+0.5));
  return {
    id:`eval-${i}`, flake, commit,
    branch:EVAL_BRANCHES[i%4],
    status, meta:EVAL_STATUS_META[status]||EVAL_STATUS_META.pending,
    systemCount:systems, policyPass:passed, policyFail:systems-passed,
    queuePos:isHistory?null:i+1,
    startedAt:isHistory?["1m ago","8m ago","1h ago","3h ago","yesterday"][i%5]:
                       ["just now","1m ago","4m ago","10m ago"][i%4],
    completedAt:isHistory?["2m ago","15m ago","1h ago","3h ago","8h ago"][i%5]:null,
    dur:isHistory?`${Math.floor(r()*120+15)}s`:null,
    canCancel:["pending","in_progress"].includes(status),
    canForceCancel:status==="cancelling",
  };
}

const ACTIVE_EVALS  = (typeof __fx === "function" && __fx("evaluations.active")) || [0,1,2,3].map(i => mkEval(i));
const HISTORY_EVALS = (typeof __fx === "function" && __fx("evaluations.history")) || Array.from({length:50},(_,i)=>mkEval(200+i, true));

const EVAL_STATS = {
  active:    ACTIVE_EVALS.length,
  completed: HISTORY_EVALS.filter(e=>e.status==="complete").length,
  failed:    HISTORY_EVALS.filter(e=>e.status==="failed").length,
  total:     ACTIVE_EVALS.length + HISTORY_EVALS.length,
};


/* ─── Eval drilldown — per-system policy matrix + derivation fanout ─── */
const EVAL_HOSTS = ["atlas-01","atlas-02","gaia-web-01","gaia-web-02","orion-db-01","edge-nyc","edge-sgp","hydra-03","lab-rig-01","lab-rig-02","kepler-api","argo-cache","sentinel-01","cygnus-mq","vega-relay","lyra-search","perseus-store","draco-edge","corvus-cdn","aquila-batch"];
const POLICY_LIST = ["stig.cat1","stig.cat2","cve.critical","cve.high","modules.audit","modules.firewall","secrets.sops","fs.encrypted"];
const DERIV_PKGS = ["nixos-system","linux-kernel","systemd","openssh","nginx","postgresql","prometheus-node-exporter","sops-nix","wireguard-tools","node-exporter","grafana-agent","redis","openssl","glibc","coreutils"];

function _h(s){let h=0;for(let i=0;i<s.length;i++)h=(h*31+s.charCodeAt(i))|0;return Math.abs(h);}

function evalSystemMatrix(evalId, n) {
  const hosts = EVAL_HOSTS.slice(0, Math.min(n, EVAL_HOSTS.length));
  return hosts.map((host, i) => {
    const seed = _h(evalId + host);
    const policies = POLICY_LIST.map((p, j) => {
      const v = (seed + j*7) % 17;
      return { name: p, status: v < 13 ? "pass" : v < 15 ? "warn" : "fail" };
    });
    const failCount = policies.filter(p=>p.status==="fail").length;
    return {
      host,
      env: ["production","production","staging","edge","lab"][i%5],
      derivCount: 4 + ((seed>>3) % 8),
      cacheHit: ((seed>>5) % 100) / 100,
      status: failCount > 0 ? "blocked" : ((seed>>1) % 4) === 0 ? "queued" : "ready",
      policies,
    };
  });
}

function evalDerivations(evalId, n) {
  const seed = _h(evalId);
  return Array.from({length: Math.min(n*3+5, 24)}, (_, i) => {
    const pkg = DERIV_PKGS[(seed+i)%DERIV_PKGS.length];
    const cached = ((seed+i*3)%5) < 3;
    return {
      drv: `/nix/store/${(seed+i*131).toString(36).padStart(10,"0").slice(0,10)}-${pkg}-${(seed%99)+1}.${i}.drv`,
      pkg, cached,
      size: cached ? 0 : (5 + ((seed+i*7)%200)),
      depCount: ((seed+i)%6)+1,
    };
  });
}

Object.assign(window, { EVAL_HOSTS, POLICY_LIST, evalSystemMatrix, evalDerivations });
