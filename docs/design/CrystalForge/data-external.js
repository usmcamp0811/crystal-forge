// Management modes: managed (agent + deploys), external (scanned, not deployed), reference (configuration only).
// External / reference resources carry `assurance` (evidence about a configuration revision) and never a heartbeat.
const MGMT_META = {
  managed:   { label: "Managed",   long: "Managed system",          icon: "server", desc: "Crystal Forge deploys this system and an agent reports its state." },
  external:  { label: "External",  long: "Externally managed",      icon: "link",   desc: "Another system or team deploys it. Crystal Forge evaluates, builds and scans the configuration." },
  reference: { label: "Reference", long: "Reference configuration", icon: "file",   desc: "A configuration tracked for security assurance. It is not a running machine." },
};
const SCAN_EVIDENCE_META = {
  current:     { label: "Scan current",  cls: "chip-healthy",  color: "#34d399" },
  stale:       { label: "Scan stale",    cls: "chip-warning",  color: "#fbbf24" },
  failed:      { label: "Scan failed",   cls: "chip-critical", color: "#f87171" },
  unavailable: { label: "Scan unavailable", cls: "chip-critical", color: "#f87171" },
  missing:     { label: "No scan",       cls: "chip-unknown",  color: "#9ca3af" },
  scanning:    { label: "Scanning",      cls: "chip-info",     color: "#60a5fa" },
};
function mgmtOf(s) { return (s && s.mgmt) || "managed"; }
function isManagedSys(s) { return mgmtOf(s) === "managed"; }
function managedSystems() { return (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).filter(isManagedSys); }
function assuranceStatus(a) {
  if (a.configState === "missing") return { label: "Config missing", color: "#f87171", chip: "chip-critical", key: "missing-config" };
  if (a.eval.state === "failed")   return { label: "Eval failed",    color: "#f87171", chip: "chip-critical", key: "eval-failed" };
  if (["none", "queued", "building"].includes(a.build.state)) return { label: a.build.state === "building" ? "Building" : "Awaiting build", color: "#60a5fa", chip: "chip-info", key: "awaiting-build" };
  if (a.build.state === "failed")  return { label: "Build failed",   color: "#f87171", chip: "chip-critical", key: "build-failed" };
  const m = SCAN_EVIDENCE_META[a.scan.state] || SCAN_EVIDENCE_META.missing;
  return { label: m.label, color: m.color, chip: m.cls, key: "scan-" + a.scan.state };
}

(function () {
  if (typeof SYSTEMS === "undefined" || typeof buildSystem !== "function") return;
  const hash = (n) => Array.from({ length: 8 }, (_, i) => "0123456789abcdef"[(n * 7 + i * 13 + i * i * n) % 16]).join("");
  const store = (cfg, n) => `/nix/store/${hash(n)}${hash(n + 3)}-nixos-system-${cfg}-24.11.20260401`;
  const defs = [
    { id: 100, mode: "external", name: "vm-photos", env: "production", flake: "web-services", branch: "main", parent: "gaia-web-02", crit: 0, high: 1, med: 4, low: 9, tags: ["microvm"],
      rev: { sha: "c7e1902", msg: "photos: enable immich ML worker", at: "6h ago" },
      eval: { state: "success", at: "6h ago" }, build: { state: "success", at: "6h ago" }, scan: { state: "current", at: "5h ago", rev: "c7e1902" },
      runtime: { source: "host", unit: "microvm@vm-photos.service", unitState: "active", runner: 11, observedAt: "2m ago", matches: true },
      hist: [["scan", "5h ago", "c7e1902", "complete", "0 critical · 1 high"], ["build", "6h ago", "c7e1902", "success", "closure 1.8 GiB"], ["eval", "6h ago", "c7e1902", "success", ""], ["scan", "2d ago", "7eef30a", "complete", "0 critical · 2 high"]] },
    { id: 101, mode: "external", name: "vm-notes", env: "production", flake: "web-services", branch: "main", parent: "gaia-web-02", crit: 0, high: 0, med: 3, low: 5, tags: ["microvm"],
      rev: { sha: "c7e1902", msg: "notes: bump hedgedoc", at: "6h ago" },
      eval: { state: "success", at: "6h ago" }, build: { state: "success", at: "6h ago" }, scan: { state: "current", at: "5h ago", rev: "c7e1902" },
      runtime: null, hostGap: "Host gaia-web-02 has not reported MicroVM observations yet.",
      hist: [["scan", "5h ago", "c7e1902", "complete", "0 critical · 0 high"], ["build", "6h ago", "c7e1902", "success", "closure 1.2 GiB"], ["eval", "6h ago", "c7e1902", "success", ""]] },
    { id: 102, mode: "external", name: "vm-gitea", env: "staging", flake: "infrastructure", branch: "staging", parent: "stg-atlas-01", crit: 1, high: 3, med: 6, low: 11, tags: ["microvm"],
      rev: { sha: "a3f8c12", msg: "gitea: move to postgres 16", at: "1d ago" },
      eval: { state: "success", at: "1d ago" }, build: { state: "success", at: "1d ago" }, scan: { state: "current", at: "20h ago", rev: "a3f8c12" },
      runtime: { source: "host", unit: "microvm@vm-gitea.service", unitState: "active", runner: 12, observedAt: "4m ago", matches: false },
      hist: [["scan", "20h ago", "a3f8c12", "complete", "1 critical · 3 high"], ["build", "1d ago", "a3f8c12", "success", "closure 2.4 GiB"], ["eval", "1d ago", "a3f8c12", "success", ""], ["scan", "9d ago", "ffa2b88", "complete", "1 critical · 4 high"]] },
    { id: 103, mode: "external", name: "fin-sftp-01", env: "production", flake: "infrastructure", branch: "main", parent: null, crit: 0, high: 2, med: 7, low: 14, tags: ["finance-owned"],
      rev: { sha: "a3f8c12", msg: "sftp: restrict chroot to /srv/exchange", at: "3d ago" },
      eval: { state: "success", at: "3d ago" }, build: { state: "success", at: "3d ago" }, scan: { state: "current", at: "6h ago", rev: "a3f8c12" },
      runtime: null, hostGap: "No agent is installed on this server and no parent host is associated.",
      hist: [["scan", "6h ago", "a3f8c12", "complete", "0 critical · 2 high"], ["build", "3d ago", "a3f8c12", "success", "closure 1.1 GiB"], ["eval", "3d ago", "a3f8c12", "success", ""]] },
    { id: 104, mode: "external", name: "vm-legacy-proxy", env: "edge", flake: "edge-gateway", branch: "main", parent: null, crit: 2, high: 4, med: 9, low: 12, tags: [],
      rev: { sha: "9a01fc2", msg: "proxy: pin nginx 1.24", at: "38d ago" },
      eval: { state: "success", at: "38d ago" }, build: { state: "success", at: "38d ago" }, scan: { state: "stale", at: "38d ago", rev: "9a01fc2" },
      runtime: null, hostGap: "No runtime observations have ever been received.",
      hist: [["scan", "38d ago", "9a01fc2", "complete", "2 critical · 4 high"], ["build", "38d ago", "9a01fc2", "success", "closure 0.9 GiB"], ["eval", "38d ago", "9a01fc2", "success", ""]] },
    { id: 105, mode: "external", name: "vm-ci-runner", env: "dev", flake: "build-farm", branch: "dev", parent: "dev-node-02", crit: 0, high: 1, med: 2, low: 4, tags: ["microvm"],
      rev: { sha: "9f0c344", msg: "ci: add cachix post-build hook", at: "2h ago" },
      eval: { state: "success", at: "2h ago" }, build: { state: "success", at: "2h ago" }, scan: { state: "failed", at: "1h ago", rev: "9f0c344", error: "vulnix: derivation output not found in any configured cache", lastGood: { rev: "4c9b11a", at: "3d ago" } },
      runtime: { source: "host", unit: "microvm@vm-ci-runner.service", unitState: "failed", runner: 13, observedAt: "1m ago", matches: null },
      hist: [["scan", "1h ago", "9f0c344", "failed", "derivation output not in cache"], ["build", "2h ago", "9f0c344", "success", "closure 3.1 GiB"], ["eval", "2h ago", "9f0c344", "success", ""], ["scan", "3d ago", "4c9b11a", "complete", "0 critical · 1 high"]] },
    { id: 106, mode: "external", name: "vm-batch-02", env: "lab", flake: "lab-nodes", branch: "main", parent: null, crit: 0, high: 0, med: 1, low: 2, tags: [], configMissing: true,
      rev: { sha: "1b7e5f0", msg: "lab: remove batch hosts", at: "2d ago" },
      eval: { state: "failed", at: "2d ago", error: "flake output 'nixosConfigurations.vm-batch-02' does not exist at 1b7e5f0" }, build: { state: "none" }, scan: { state: "stale", at: "21d ago", rev: "6d1e0a2" },
      runtime: null, hostGap: "No runtime observations.",
      hist: [["eval", "2d ago", "1b7e5f0", "failed", "nixosConfigurations.vm-batch-02 not found"], ["scan", "21d ago", "6d1e0a2", "complete", "0 critical · 0 high"], ["build", "21d ago", "6d1e0a2", "success", "closure 0.7 GiB"]] },
    { id: 110, mode: "reference", name: "ref-golden-workstation", cfg: "golden-workstation", env: "production", flake: "infrastructure", branch: "main", crit: 0, high: 1, med: 5, low: 8, tags: ["baseline"],
      rev: { sha: "a3f8c12", msg: "workstation: stig-enforce screen lock", at: "1d ago" },
      eval: { state: "success", at: "1d ago" }, build: { state: "success", at: "1d ago" }, scan: { state: "current", at: "8h ago", rev: "a3f8c12" },
      hist: [["scan", "8h ago", "a3f8c12", "complete", "0 critical · 1 high"], ["build", "1d ago", "a3f8c12", "success", "closure 6.4 GiB"], ["eval", "1d ago", "a3f8c12", "success", ""], ["scan", "6d ago", "44102fa", "complete", "0 critical · 2 high"]] },
    { id: 111, mode: "reference", name: "ref-server-baseline", cfg: "server-baseline", env: "production", flake: "infrastructure", branch: "main", crit: 0, high: 0, med: 0, low: 0, tags: ["baseline"],
      rev: { sha: "e41b7d0", msg: "baseline: add auditd rules for sudo", at: "40m ago" },
      eval: { state: "success", at: "38m ago" }, build: { state: "building", at: "started 12m ago" }, scan: { state: "missing" },
      hist: [["build", "12m ago", "e41b7d0", "running", "queued on hydra-02"], ["eval", "38m ago", "e41b7d0", "success", ""], ["scan", "2d ago", "a3f8c12", "complete", "0 critical · 0 high"]] },
    { id: 112, mode: "reference", name: "ref-iso-installer", cfg: "iso-installer", env: "lab", flake: "lab-nodes", branch: "main", crit: 0, high: 0, med: 0, low: 0, tags: ["iso"],
      rev: { sha: "1b7e5f0", msg: "iso: include disko and age", at: "5d ago" },
      eval: { state: "success", at: "5d ago" }, build: { state: "success", at: "5d ago" }, scan: { state: "unavailable", at: "5d ago", rev: "1b7e5f0", error: "ISO closures exceed the scanner's size limit (12 GiB). Scanning is not supported for this output type." },
      hist: [["scan", "5d ago", "1b7e5f0", "unavailable", "closure too large"], ["build", "5d ago", "1b7e5f0", "success", "iso 12.6 GiB"], ["eval", "5d ago", "1b7e5f0", "success", ""]] },
    { id: 113, mode: "reference", name: "ref-approved-vm", cfg: "approved-vm", env: "staging", flake: "web-services", branch: "staging", crit: 1, high: 2, med: 4, low: 6, tags: ["baseline"],
      rev: { sha: "2fa8031", msg: "vm: openssl 3.3.1", at: "27d ago" },
      eval: { state: "success", at: "27d ago" }, build: { state: "success", at: "27d ago" }, scan: { state: "stale", at: "27d ago", rev: "2fa8031" },
      hist: [["scan", "27d ago", "2fa8031", "complete", "1 critical · 2 high"], ["build", "27d ago", "2fa8031", "success", "closure 1.5 GiB"], ["eval", "27d ago", "2fa8031", "success", ""]] },
  ];
  defs.forEach((d) => {
    const base = buildSystem([d.name, d.env, d.flake, "unknown", 0], d.id);
    const cfg = d.cfg || d.name;
    const a = {
      configState: d.configMissing ? "missing" : "ok", configName: cfg,
      rev: d.rev, eval: d.eval, build: { ...d.build, storePath: d.build.state === "success" ? store(cfg, d.id) : null },
      scan: d.scan, history: d.hist.map(([kind, at, rev, state, note]) => ({ kind, at, rev, state, note })),
    };
    const st = assuranceStatus(a);
    const mode = d.mode;
    const total = d.crit + d.high + d.med + d.low;
    Object.assign(base, {
      mgmt: mode, assurance: a, configName: cfg,
      fqdn: mode === "external" ? (d.parent ? `guest of ${d.parent}` : "externally administered") : `${d.flake}#${cfg}`,
      branch: d.branch, commit: d.rev.sha, commitMessage: d.rev.msg,
      health: mode, status: st.label, statusColor: st.color, statusChip: st.chip,
      deploymentPolicy: "—", deploymentState: mode, lastHeartbeat: "—", heartbeatAge: null,
      generation: null, uptime: "—", kernel: "linux-6.6.72", cpu: "—", memGb: null, ipv4: "—", ipv6: "—",
      storePath: a.build.storePath, targetStorePath: null, reachability: "none", stig: base.stig,
      tags: d.tags, parentHost: d.parent || null,
      cves: { critical: d.crit, high: d.high, medium: d.med, low: d.low, total },
      events: d.hist.map((h) => ({ at: h[1], title: `${h[0] === "scan" ? "Scan" : h[0] === "build" ? "Build" : "Evaluation"} ${h[3]}`, color: h[3] === "failed" || h[3] === "unavailable" ? "#f87171" : "#60a5fa" })),
    });
    if (d.runtime) {
      base.runtime = { source: "host", parentHost: d.parent, guest: d.name, unit: d.runtime.unit, unitState: d.runtime.unitState,
        runnerPath: d.runtime.matches === false ? store(cfg + "-old", d.id + 50).replace("nixos-system", "microvm-run") : store(cfg, d.id).replace("nixos-system", "microvm-run"),
        observedAt: d.runtime.observedAt, matchesScanned: d.runtime.matches };
    } else base.runtime = null;
    base.hostGap = d.hostGap || null;
    SYSTEMS.push(base);
  });
})();

Object.assign(window, { MGMT_META, SCAN_EVIDENCE_META, mgmtOf, isManagedSys, managedSystems, assuranceStatus });
