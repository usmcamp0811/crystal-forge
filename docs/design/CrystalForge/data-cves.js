// CVE registry — fleet-wide vulnerabilities aggregated across systems

const CVE_PACKAGES = [
  { pkg: "openssl",      versions: ["3.0.11","3.0.13","3.2.1","3.3.2"] },
  { pkg: "linux-kernel", versions: ["6.6.62","6.6.70","6.6.72","6.1.115"] },
  { pkg: "curl",         versions: ["8.4.0","8.8.0","8.10.1"] },
  { pkg: "glibc",        versions: ["2.38","2.39","2.40"] },
  { pkg: "systemd",      versions: ["254.10","255.4","256.5","256.7"] },
  { pkg: "nginx",        versions: ["1.24.0","1.26.1","1.27.1","1.27.4"] },
  { pkg: "postgresql",   versions: ["15.7","16.2","16.4"] },
  { pkg: "git",          versions: ["2.42.1","2.45.0","2.47.1"] },
  { pkg: "python311",    versions: ["3.11.7","3.11.9","3.11.10"] },
  { pkg: "redis",        versions: ["7.2.4","7.4.0"] },
  { pkg: "vault",        versions: ["1.16.2","1.18.3"] },
  { pkg: "grafana",      versions: ["11.2.0","11.4.0"] },
];

const CVE_TITLES = [
  "Out-of-bounds read in TLS handshake parser",
  "Use-after-free in netfilter table cleanup",
  "Improper bounds check in HTTP/2 frame parser",
  "Stack-based buffer overflow in certificate validation",
  "Integer overflow in compression handler",
  "Heap corruption when parsing malformed packets",
  "Authentication bypass via header injection",
  "Privilege escalation through symlink race",
  "Denial of service via memory exhaustion",
  "Information disclosure via timing side-channel",
  "Server-Side Request Forgery in proxy module",
  "Path traversal in archive extraction",
  "Race condition in shared memory access",
  "Remote code execution via deserialization",
  "Cross-site scripting in error template",
  "SQL injection in admin query endpoint",
];

function _cveSeed(i) { let s = i * 9301 + 49297; return () => { s = (s * 9301 + 49297) % 233280; return s / 233280; }; }

function buildCVEData() {
  const list = [];
  for (let i = 0; i < 48; i++) {
    const r = _cveSeed(i + 1);
    const pkg = CVE_PACKAGES[Math.floor(r() * CVE_PACKAGES.length)];
    const sev = r() < 0.18 ? "critical" : r() < 0.42 ? "high" : r() < 0.72 ? "medium" : "low";
    const cvss = sev === "critical" ? 9.0 + r() :
                 sev === "high" ? 7.0 + r() * 2 :
                 sev === "medium" ? 4.0 + r() * 3 :
                                    1.0 + r() * 3;
    const fixedIn = pkg.versions[Math.min(pkg.versions.length - 1, Math.floor(r() * pkg.versions.length))];
    const introducedIn = pkg.versions[0];
    const fix = sev === "critical" ? (r() < 0.85 ? "available" : "pending") :
                sev === "high"     ? (r() < 0.7  ? "available" : "pending") :
                                     (r() < 0.55 ? "available" : "pending");

    // Affected systems — bias by severity
    const affectFn = r();
    const baseRatio = sev === "critical" ? 0.05 + affectFn * 0.2 :
                      sev === "high" ?     0.08 + affectFn * 0.3 :
                      sev === "medium" ?   0.15 + affectFn * 0.5 :
                                           0.25 + affectFn * 0.55;
    const affected = SYSTEMS.filter((_, si) => ((i * 7 + si * 11) % 100) / 100 < baseRatio);

    const ageDays = Math.floor(r() * 180) + 1;
    const exploited = sev === "critical" && r() < 0.25;
    const id = `CVE-${2024 + Math.floor(r() * 3)}-${String(10000 + Math.floor(r() * 60000)).padStart(5, "0")}`;
    const advisoryUrl = `https://nvd.nist.gov/vuln/detail/${id}`;

    // Justification: 0 = unhandled, 1 = justified (accepted risk), 2 = scheduled
    const justifiedRoll = r();
    let acceptance = "outstanding";
    let justification = null;
    let justifiedBy = null;
    let justifiedAt = null;
    if (justifiedRoll < 0.18) {
      acceptance = "accepted";
      const reasons = [
        "Mitigated by network segmentation; service is internal-only.",
        "Compensating control via WAF rule WAF-2025-447.",
        "Vulnerable code path not reachable in this deployment.",
        "Risk accepted by AO until 2026-08-30 per CR-2026-118.",
        "False positive — upstream backport already applied.",
      ];
      justification = reasons[Math.floor(r() * reasons.length)];
      justifiedBy = ["mreyes","jpark","security-team"][Math.floor(r()*3)];
      justifiedAt = `${Math.floor(r() * 30) + 1}d ago`;
    } else if (justifiedRoll < 0.32) {
      acceptance = "scheduled";
      justification = `Patch scheduled for maintenance window ${Math.floor(r()*4)+1}w from now.`;
      justifiedBy = "ops-team";
      justifiedAt = `${Math.floor(r() * 14) + 1}d ago`;
    }

    list.push({
      id, pkg: pkg.pkg, severity: sev, cvss: parseFloat(cvss.toFixed(1)),
      title: CVE_TITLES[i % CVE_TITLES.length],
      introducedIn, fixedIn,
      fix, ageDays, exploited,
      affected: affected.map(s => s.id),
      affectedCount: affected.length,
      advisoryUrl,
      vector: ["AV:N", "AC:L", sev === "critical" ? "PR:N" : "PR:L", "UI:N", "S:U"].join("/"),
      discoveredAt: `${ageDays}d ago`,
      acceptance, justification, justifiedBy, justifiedAt,
    });
  }
  return list.sort((a, b) => {
    const sevOrder = { critical: 0, high: 1, medium: 2, low: 3 };
    if (sevOrder[a.severity] !== sevOrder[b.severity]) return sevOrder[a.severity] - sevOrder[b.severity];
    return b.cvss - a.cvss;
  });
}

const CVES = (typeof __fx === "function" && __fx("cves.list")) || buildCVEData();

const CVE_STATS = {
  total:    CVES.length,
  critical: CVES.filter(c => c.severity === "critical").length,
  high:     CVES.filter(c => c.severity === "high").length,
  medium:   CVES.filter(c => c.severity === "medium").length,
  low:      CVES.filter(c => c.severity === "low").length,
  exploited:CVES.filter(c => c.exploited).length,
  fixable:  CVES.filter(c => c.fix === "available").length,
  newToday: CVES.filter(c => c.ageDays <= 1).length,
  systemsAffected: new Set(CVES.flatMap(c => c.affected)).size,
  outstanding: CVES.filter(c => c.acceptance === "outstanding").length,
  accepted:    CVES.filter(c => c.acceptance === "accepted").length,
  scheduled:   CVES.filter(c => c.acceptance === "scheduled").length,
};

// Per-environment top affected systems
function buildCveInsights() {
  // sysId -> { critical:n, high:n, total:n, fixable:n, exploited:n }
  const perSys = {};
  CVES.forEach(c => {
    c.affected.forEach(sid => {
      perSys[sid] = perSys[sid] || { critical:0, high:0, medium:0, low:0, total:0, fixable:0, exploited:0 };
      perSys[sid][c.severity] += 1;
      perSys[sid].total += 1;
      if (c.fix === "available") perSys[sid].fixable += 1;
      if (c.exploited) perSys[sid].exploited += 1;
    });
  });

  // Build [{sys, counts}] sorted by risk weight
  const sysScores = Object.entries(perSys).map(([id, counts]) => {
    const sys = SYSTEMS.find(s => s.id === id);
    if (!sys) return null;
    const score = counts.critical * 100 + counts.high * 10 + counts.medium + counts.exploited * 50;
    return { sys, counts, score };
  }).filter(Boolean).sort((a,b) => b.score - a.score);

  // Group by env, top 3 per env
  const byEnv = {};
  sysScores.forEach(({ sys, counts, score }) => {
    byEnv[sys.environment] = byEnv[sys.environment] || [];
    byEnv[sys.environment].push({ sys, counts, score });
  });
  Object.keys(byEnv).forEach(e => { byEnv[e] = byEnv[e].slice(0, 4); });

  // Patchable systems — systems with at least 1 fixable CVE and no compensating "accepted" overrides
  const patchableSystems = sysScores.filter(s => s.counts.fixable > 0).slice(0, 8);

  return { byEnv, patchableSystems, sysScores };
}

const CVE_INSIGHTS = buildCveInsights();

Object.assign(window, { CVES, CVE_STATS, CVE_INSIGHTS });
