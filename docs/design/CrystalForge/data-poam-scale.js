// Fleet-scale POA&M seed. The hand-written items in data-poam.js drive the guided walkthrough;
// these add the volume a real program carries (many environments, hosts, bundles, owners) so the
// POA&M view can be judged at scale. Deterministic: fixed seed, derived only from fixture data.
// Every compliance item is attached to a finding that actually evaluates fail/warn, so bundle and
// host roll-ups stay consistent. Walkthrough findings are never touched.
(function () {
  if (typeof POAMS === "undefined" || typeof SYSTEMS === "undefined" || typeof COMPLIANCE_BUNDLES === "undefined") return;
  let seed = 20260822;
  const rnd = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 4294967296; };
  const pick = (a) => a[Math.floor(rnd() * a.length)];
  const addDays = (d, n) => { const x = new Date(d + "T00:00:00Z"); x.setUTCDate(x.getUTCDate() + n); return x.toISOString().slice(0, 10); };
  const T = POAM_TODAY;

  const taken = new Set(Object.keys(POAM_FINDING_STATUS_OVERRIDE));
  POAMS.forEach(p => p.findings.forEach(f => taken.add(f.sysId + "::" + f.policyId)));
  const policy = (id) => (typeof POLICIES !== "undefined" ? POLICIES : []).find(p => p.id === id) || { id, name: id };
  const statusOf = (b, pid, sys) => { const e = evidenceForControl(b, pid, sys); return typeof e === "string" ? e : e && e.status; };

  const failing = [];
  ["disa-rhel9-stig", "nist-800-53-mod", "iso-9001-qms"].forEach(bid => {
    const b = COMPLIANCE_BUNDLES.find(x => x.id === bid);
    if (!b) return;
    const hosts = SYSTEMS.filter(s => (b.requiredEnvs || []).includes(s.environment));
    b.policyIds.filter(id => id.startsWith("stig-")).forEach(pid => hosts.forEach(sys => {
      const k = sys.id + "::" + pid;
      if (taken.has(k)) return;
      const st = statusOf(b, pid, sys);
      if (st === "fail" || st === "warn") { taken.add(k); failing.push({ sys, pid, bid, k }); }
    }));
  });
  // Keep roughly two thirds on a plan; the rest stay "no POA&M" like a real backlog.
  const chosen = failing.filter(() => rnd() < 0.62);

  // Same control failing on several hosts of one flake is usually one shared cause → one item.
  const byCause = new Map();
  chosen.forEach(f => {
    const key = f.pid + "|" + f.sys.flake + "|" + f.bid;
    if (!byCause.has(key)) byCause.set(key, []);
    byCause.get(key).push(f);
  });
  const units = [];
  byCause.forEach(list => {
    if (list.length > 1 && rnd() < 0.55) units.push(list);
    else list.forEach(f => units.push([f]));
  });

  const TEAM_BY_FLAKE = { "infrastructure": "Platform Team", "build-farm": "Platform Team", "web-services": "Jordan Park", "edge-gateway": "Dana Chen", "lab-nodes": "Dana Chen" };
  const OWNERS = ["Platform Team", "Security Team", "Endpoint Team", "Mira Reyes", "Jordan Park", "Dana Chen", "sre"];
  const STATUS_W = [["open", 0.2], ["in_progress", 0.4], ["blocked", 0.08], ["awaiting_verification", 0.12], ["completed", 0.2]];
  const pickStatus = () => { let r = rnd(), a = 0; for (const [s, w] of STATUS_W) { a += w; if (r < a) return s; } return "open"; };
  const WHO = ["r.chen", "j.okafor", "a.novak", "m.reyes", "j.park", "d.chen"];

  const build = ({ title, severity, owner, findings, cveRefs, plan, hostLabel }) => {
    const status = pickStatus();
    const opened = addDays(T, -Math.floor(8 + rnd() * 140));
    const horizon = severity === "high" ? 45 + rnd() * 90 : severity === "medium" ? 90 + rnd() * 150 : 120 + rnd() * 180;
    const due = addDays(opened, Math.floor(horizon));
    const steps = ["Identify root cause and fix", "Deploy to staging and validate", `Roll out to ${hostLabel}`, "Verify evaluation passes"];
    const doneN = status === "completed" ? 4 : status === "awaiting_verification" ? 3 : status === "open" ? 0 : Math.floor(rnd() * 3);
    const span = (new Date(due) - new Date(opened)) / 86400000;
    const milestones = steps.map((text, i) => {
      const d = addDays(opened, Math.round(span * (i + 1) / 4));
      return i < doneN ? { text, due: d, done: true, doneAt: d < T ? d : T } : { text, due: d, done: false };
    });
    const who = pick(WHO);
    const activity = [{ at: opened, who, text: findings.length ? `POA&M created from ${findings.length > 1 ? `${findings.length} findings` : "failing finding"} ${hostLabel}.` : `POA&M created from ${cveRefs[0].id} on ${hostLabel}.` }];
    // Most items see recent work; a share goes quiet — the "stale" queue exists to catch those.
    const quiet = rnd() < 0.22;
    const lastAt = quiet ? addDays(opened, Math.floor(rnd() * 6)) : addDays(T, -Math.floor(rnd() * 20));
    if (lastAt > opened && lastAt <= T) activity.push({ at: lastAt, who: pick(WHO), text: status === "blocked" ? "Waiting on an upstream dependency — status Blocked." : doneN ? `Milestone complete: ${steps[doneN - 1]}.` : "Plan reviewed; work scheduled." });
    const item = { title, severity, status, owner: status !== "completed" && rnd() < 0.07 ? "unassigned" : owner, due, opened, plan, findings, cveRefs: cveRefs || [], milestones, activity };
    if (status === "completed") {
      const c = addDays(due, -Math.floor(rnd() * 10));
      item.closed = c < T ? c : T;
      item.verification = { at: item.closed, result: "pass", note: "Linked findings evaluated pass; POA&M closed." };
      findings.forEach(f => { POAM_FINDING_STATUS_OVERRIDE[f.sysId + "::" + f.policyId] = "pass"; });
    }
    return item;
  };

  const out = [];
  units.forEach(list => {
    const pol = policy(list[0].pid);
    const sev = ["high", "medium", "low"].includes(pol.severity) ? pol.severity : pick(["high", "medium", "medium", "low"]);
    const flake = list[0].sys.flake;
    const hostLabel = list.length > 1 ? `${list.length} ${flake} hosts` : list[0].sys.hostname;
    out.push(build({
      title: `${pol.name || pol.id} — ${hostLabel}`,
      severity: sev,
      owner: rnd() < 0.6 ? TEAM_BY_FLAKE[flake] || pick(OWNERS) : pick(OWNERS),
      findings: list.map(f => ({ sysId: f.sys.id, policyId: f.pid, bundleId: f.bid })),
      plan: list.length > 1
        ? `Shared cause across the ${flake} role: fix the module once, then roll host by host and re-evaluate.`
        : `Correct the ${pol.name || pol.id} setting in the ${flake} configuration, deploy, and re-evaluate.`,
      hostLabel,
    }));
  });

  // CVE remediation plans — the triage path for hosts outside the STIG-bound environments.
  (typeof CVES !== "undefined" ? CVES : []).filter(c => c.severity === "critical" || c.severity === "high").forEach(cve => {
    const byEnv = {};
    (cve.affected || []).forEach(id => {
      const s = SYSTEMS.find(x => x.id === id);
      if (s && ["dev", "edge", "lab", "staging"].includes(s.environment)) (byEnv[s.environment] = byEnv[s.environment] || []).push(s);
    });
    Object.entries(byEnv).forEach(([env, hosts]) => {
      if (rnd() > 0.45) return;
      const hostLabel = hosts.length > 1 ? `${hosts.length} ${env} hosts` : hosts[0].hostname;
      out.push(build({
        title: `${cve.id} — patch ${cve.pkg} in ${env}`,
        severity: cve.severity === "critical" ? "high" : "medium",
        owner: env === "edge" || env === "lab" ? "Dana Chen" : pick(OWNERS),
        findings: [],
        cveRefs: hosts.map(s => ({ id: cve.id, pkg: cve.pkg, sysId: s.id, hostname: s.hostname })),
        plan: `Upgrade ${cve.pkg} to ${cve.fixedIn || "a patched release"} across ${hostLabel} and confirm the next scan clears.`,
        hostLabel,
      }));
    });
  });

  out.sort((a, b) => a.opened < b.opened ? -1 : a.opened > b.opened ? 1 : 0);
  let n = POAMS.reduce((m, p) => Math.max(m, Number((p.id.match(/(\d+)$/) || [])[1] || 0)), 0);
  out.forEach(p => { p.id = `POAM-${String(++n).padStart(4, "0")}`; POAMS.push(p); });
})();
