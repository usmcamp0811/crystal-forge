// Mock data for Crystal Forge Systems view (~35 systems, mid fleet)

const ENVIRONMENTS = (typeof __fx === "function" && __fx("environments")) || [
  { name: "production", color: "#dc2626", dot: "#ef4444" },
  { name: "staging",    color: "#d97706", dot: "#f59e0b" },
  { name: "dev",        color: "#2563eb", dot: "#3b82f6" },
  { name: "edge",       color: "#0f766e", dot: "#14b8a6" },
  { name: "lab",        color: "#7c3aed", dot: "#8b5cf6" },
];

const FLAKES = [
  "infrastructure",
  "web-services",
  "build-farm",
  "edge-gateway",
  "lab-nodes",
];

function shortHash() {
  const hex = "0123456789abcdef";
  let s = "";
  for (let i = 0; i < 8; i++) s += hex[Math.floor(Math.random() * 16)];
  return s;
}

function rel(minsAgo) {
  if (minsAgo < 1) return "just now";
  if (minsAgo < 60) return `${Math.floor(minsAgo)}m ago`;
  const h = minsAgo / 60;
  if (h < 24) return `${Math.floor(h)}h ago`;
  const d = h / 24;
  if (d < 30) return `${Math.floor(d)}d ago`;
  return `${Math.floor(d / 30)}mo ago`;
}

// Deterministic seed for stable mocks
let _seed = 42;
function rand() { _seed = (_seed * 9301 + 49297) % 233280; return _seed / 233280; }
function pick(arr) { return arr[Math.floor(rand() * arr.length)]; }
function int(min, max) { return Math.floor(rand() * (max - min + 1)) + min; }

const HOSTS = [
  // production (healthy-heavy)
  ["atlas-01",    "production", "infrastructure", "healthy",  0],
  ["atlas-02",    "production", "infrastructure", "healthy",  0],
  ["atlas-03",    "production", "infrastructure", "warning",  2],
  ["hydra-01",    "production", "build-farm",     "healthy",  0],
  ["hydra-02",    "production", "build-farm",     "healthy",  0],
  ["hydra-03",    "production", "build-farm",     "drifted",  1],
  ["gaia-web-01", "production", "web-services",   "healthy",  0],
  ["gaia-web-02", "production", "web-services",   "healthy",  0],
  ["gaia-web-03", "production", "web-services",   "critical", 9],
  ["gaia-web-04", "production", "web-services",   "healthy",  0],
  ["orion-db-01", "production", "infrastructure", "healthy",  1],
  ["orion-db-02", "production", "infrastructure", "warning",  3],
  // staging
  ["stg-atlas-01", "staging", "infrastructure", "healthy", 0],
  ["stg-atlas-02", "staging", "infrastructure", "offline", 0],
  ["stg-web-01",   "staging", "web-services",   "healthy", 2],
  ["stg-web-02",   "staging", "web-services",   "healthy", 0],
  ["stg-build-01", "staging", "build-farm",     "building", 0],
  // dev
  ["dev-node-01", "dev", "infrastructure", "healthy",  0],
  ["dev-node-02", "dev", "infrastructure", "healthy",  1],
  ["dev-node-03", "dev", "web-services",   "healthy",  0],
  ["dev-node-04", "dev", "web-services",   "warning",  1],
  ["dev-lab-01",  "dev", "lab-nodes",      "healthy",  0],
  ["dev-lab-02",  "dev", "lab-nodes",      "unknown",  0],
  // edge
  ["edge-pdx-01", "edge", "edge-gateway", "healthy", 0],
  ["edge-pdx-02", "edge", "edge-gateway", "healthy", 2],
  ["edge-nyc-01", "edge", "edge-gateway", "warning", 4],
  ["edge-nyc-02", "edge", "edge-gateway", "healthy", 0],
  ["edge-fra-01", "edge", "edge-gateway", "offline", 0],
  ["edge-fra-02", "edge", "edge-gateway", "healthy", 1],
  ["edge-sgp-01", "edge", "edge-gateway", "healthy", 0],
  // lab
  ["lab-vm-01",  "lab", "lab-nodes", "healthy", 0],
  ["lab-vm-02",  "lab", "lab-nodes", "healthy", 0],
  ["lab-vm-03",  "lab", "lab-nodes", "drifted", 0],
  ["lab-rig-01", "lab", "lab-nodes", "offline", 0],
  ["lab-rig-02", "lab", "lab-nodes", "healthy", 5],
];

function buildSystem([hostname, env, flake, health, criticalCves], idx) {
  const statusMap = {
    healthy:  { label: "Healthy",   color: "#34d399", chip: "chip-healthy" },
    warning:  { label: "Warning",   color: "#fbbf24", chip: "chip-warning" },
    critical: { label: "Critical",  color: "#f87171", chip: "chip-critical" },
    offline:  { label: "Offline",   color: "#f87171", chip: "chip-critical" },
    drifted:  { label: "Drifted",   color: "#fbbf24", chip: "chip-warning" },
    building: { label: "Deploying", color: "#60a5fa", chip: "chip-info" },
    unknown:  { label: "Unknown",   color: "#6b7280", chip: "chip-unknown" },
  };
  const status = statusMap[health];
  const pol = pick(["manual", "auto_latest", "pinned"]);
  const hbMin = health === "offline" ? int(60 * 6, 60 * 72) :
                health === "unknown" ? int(10, 60) : int(0, 5);
  // expected interval in seconds (agent heartbeat cadence); varies slightly per host
  const hbIntervalSec = pick([60, 90, 120]);
  // seconds until (or past) next expected heartbeat. Healthy hosts -> future; offline -> very past.
  const hbElapsedSec = hbMin * 60 + Math.floor((Math.random() * hbIntervalSec));
  const hbNextInSec = hbIntervalSec - hbElapsedSec;

  const deploy = {
    healthy: "up-to-date",
    warning: rand() < 0.5 ? "behind" : "up-to-date",
    critical: "failed",
    offline: "unknown",
    drifted: "drift",
    building: "deploying",
    unknown: "unknown",
  }[health];

  // CVE totals — critical-heavy hosts have more
  const crit = criticalCves;
  const high = crit > 0 ? int(crit, crit * 3 + 2) : int(0, 4);
  const med = int(0, 18);
  const low = int(0, 30);

  return {
    id: `sys-${idx}`,
    hostname,
    fqdn: `${hostname}.${env}.cf.internal`,
    environment: env,
    flake,
    branch: env === "production" ? "main" : env === "staging" ? "staging" : "dev",
    commit: shortHash(),
    commitMessage: pick([
      "bump nixpkgs to 24.11",
      "harden sshd: disable password auth",
      "add grafana exporter to host",
      "stig: enforce audit rules for sudo",
      "wireguard: add peer for edge-sgp-01",
      "fix: postgres role permissions migration",
      "cve: patch openssl to 3.3.2",
      "feat: enable sops-nix for secrets",
    ]),
    health,
    status: status.label,
    statusColor: status.color,
    statusChip: status.chip,
    deploymentPolicy: pol,
    deploymentState: deploy,
    lastHeartbeat: rel(hbMin),
    heartbeatAge: hbMin,
    heartbeatIntervalSec: hbIntervalSec,
    heartbeatNextInSec: hbNextInSec,
    generation: int(24, 217),
    nixosVersion: pick(["24.11.20260401", "24.11.20260320", "24.05.20260218"]),
    kernel: pick(["linux-6.6.72", "linux-6.6.70", "linux-6.1.115"]),
    storePath: `/nix/store/${shortHash()}${shortHash().slice(0,8)}-nixos-system-${hostname}-24.11.${int(20251101, 20260401)}`,
    targetStorePath: health === "drifted" || health === "warning"
      ? `/nix/store/${shortHash()}${shortHash().slice(0,8)}-nixos-system-${hostname}-24.11.${int(20251101, 20260401)}`
      : null,
    uptime: `${int(1, 42)}d ${int(0, 23)}h`,
    cpu: pick(["Xeon E-2336", "EPYC 7443P", "Ryzen 9 5950X", "Graviton3"]),
    memGb: pick([16, 32, 64, 128, 256]),
    ipv4: `10.${int(0, 4)}.${int(0, 255)}.${int(2, 250)}`,
    ipv6: `fd42:${pick(["a1","b2","c3","d4"])}:${shortHash().slice(0,4)}::${int(1, 99)}`,
    reachability: env === "edge" ? "pull" : pick(["direct", "direct", "direct", "pull"]),
    cves: { critical: crit, high, medium: med, low, total: crit + high + med + low },
    tags: [
      flake === "build-farm" ? "builder" : null,
      env === "production" ? "stig-enforced" : null,
      hostname.includes("db") ? "persistent-data" : null,
    ].filter(Boolean),
    stig: env === "production" ? int(28, 30) : env === "staging" ? int(22, 28) : int(14, 22),
    events: [
      { at: rel(hbMin + 2),   title: `Heartbeat received`, color: "#34d399" },
      { at: rel(int(30, 120)), title: `Deploy ${deploy === "up-to-date" ? "succeeded" : deploy === "failed" ? "failed" : "completed"}`, color: deploy === "failed" ? "#f87171" : "#34d399" },
      { at: rel(int(300, 720)), title: `Evaluation complete`, color: "#60a5fa" },
      { at: rel(int(1200, 2800)), title: `Configuration drift detected`, color: "#fbbf24" },
      { at: rel(int(3000, 9000)), title: `Generation ${int(20, 210)} activated`, color: "#34d399" },
    ],
  };
}

const SYSTEMS = (typeof __fx === "function" && __fx("systems")) || HOSTS.map(buildSystem);

// All distinct tags currently in use across the fleet — for filter dropdowns + suggestions.
function allFleetTags() {
  return [...new Set(SYSTEMS.flatMap(s => s.tags || []))].sort();
}

Object.assign(window, { SYSTEMS, ENVIRONMENTS, FLAKES, allFleetTags });
