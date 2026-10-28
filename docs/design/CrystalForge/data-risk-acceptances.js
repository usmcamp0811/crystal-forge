// Risk acceptances — the decision NOT to remediate now, recorded next to the POA&M.
// A POA&M plans a fix toward a target date; an acceptance documents why a deficiency may
// stand, who approved it, what compensates for it, and when it must be reviewed again. Both are
// scoped to the same findings (host × control, or host × CVE), so the POA&M view shows them
// together. An acceptance that lapses is either renewed or converted into a POA&M.
// Deterministic: compliance acceptances come from controls that evaluate "waiver"; CVE
// acceptances come from CVEs triaged as accepted.
(function () {
  if (typeof SYSTEMS === "undefined" || typeof COMPLIANCE_BUNDLES === "undefined") return;
  let seed = 88411;
  const rnd = () => { seed = (Math.imul(seed, 1103515245) + 12345) >>> 0; return seed / 4294967296; };
  const pick = (a) => a[Math.floor(rnd() * a.length)];
  const T = POAM_TODAY;
  const addDays = (d, n) => { const x = new Date(d + "T00:00:00Z"); x.setUTCDate(x.getUTCDate() + n); return x.toISOString().slice(0, 10); };
  const policy = (id) => (typeof POLICIES !== "undefined" ? POLICIES : []).find(p => p.id === id) || { id, name: id };

  const COMPENSATING = [
    "Host is network-isolated on a management VLAN with no inbound routes; access via bastion only.",
    "Service is internal-only behind the edge proxy; WAF rule set blocks the affected request path.",
    "Session recording and MFA on the bastion cover the gap until the module lands.",
    "Workload is ephemeral and rebuilt nightly from the signed image; no persistent state.",
    "Detection rule in the SIEM alerts on the relevant syscall pattern within 5 minutes.",
  ];
  const JUSTIFY = [
    "Vendor dependency blocks the fix; upstream release expected next quarter.",
    "Hardware in this tier cannot support the control without a platform refresh.",
    "Control conflicts with a certified appliance configuration; vendor exception on file.",
    "Operational requirement: the setting breaks the build farm's remote builders.",
    "False positive — the backported fix is present but the scanner keys on the version string.",
  ];
  const APPROVERS = ["Mira Reyes", "Security Team", "Jordan Park", "ISSM · A. Okoro"];

  const list = [];
  const byKey = new Map();
  ["disa-rhel9-stig", "nist-800-53-mod", "iso-9001-qms"].forEach(bid => {
    const b = COMPLIANCE_BUNDLES.find(x => x.id === bid);
    if (!b) return;
    const hosts = SYSTEMS.filter(s => (b.requiredEnvs || []).includes(s.environment));
    b.policyIds.filter(id => id.startsWith("stig-")).forEach(pid => hosts.forEach(sys => {
      const e = evidenceForControl(b, pid, sys);
      if ((e && e.status) !== "waiver") return;
      // Acceptances are decided per control per environment, the same scope the CVE triage uses.
      const k = pid + "|" + sys.environment;
      if (!byKey.has(k)) { byKey.set(k, { pid, bid, env: sys.environment, findings: [] }); list.push(byKey.get(k)); }
      const g = byKey.get(k);
      if (!g.findings.some(f => f.sysId === sys.id)) g.findings.push({ sysId: sys.id, policyId: pid, bundleId: g.bid });
    }));
  });

  const out = [];
  const stamp = (sev) => {
    const approvedAt = addDays(T, -Math.floor(15 + rnd() * 320));
    const r = rnd();
    // Most carry a review date; the ones that don't are what assessors flag first.
    const reviewDate = r < 0.12 ? null : addDays(approvedAt, sev === "high" ? 90 + Math.floor(rnd() * 90) : 180 + Math.floor(rnd() * 185));
    return { approvedAt, reviewDate };
  };
  list.forEach(g => {
    const pol = policy(g.pid);
    const sev = ["high", "medium", "low"].includes(pol.severity) ? pol.severity : pick(["medium", "medium", "low", "high"]);
    const { approvedAt, reviewDate } = stamp(sev);
    const hosts = g.findings.length;
    out.push({
      kind: "compliance", severity: sev, status: "active",
      title: `${pol.name || pol.id} — ${hosts > 1 ? `${hosts} ${g.env} hosts` : SYSTEMS.find(s => s.id === g.findings[0].sysId).hostname}`,
      findings: g.findings, cveRefs: [], env: g.env,
      justification: pick(JUSTIFY), compensating: pick(COMPENSATING),
      approver: pick(APPROVERS), approvedAt, reviewDate,
      history: [{ at: approvedAt, who: "security-team", text: "Risk acceptance approved." }],
    });
  });
  (typeof CVES !== "undefined" ? CVES : []).filter(c => c.acceptance === "accepted").forEach(c => {
    const hosts = SYSTEMS.filter(s => (c.affected || []).includes(s.id));
    if (!hosts.length) return;
    const envs = [...new Set(hosts.map(s => s.environment))];
    envs.forEach(env => {
      const inEnv = hosts.filter(s => s.environment === env);
      const sev = c.severity === "critical" || c.severity === "high" ? "high" : c.severity === "medium" ? "medium" : "low";
      const { approvedAt, reviewDate } = stamp(sev);
      out.push({
        kind: "cve", severity: sev, status: "active",
        title: `${c.id} — ${c.pkg} in ${env}`,
        findings: [], cveRefs: inEnv.map(s => ({ id: c.id, pkg: c.pkg, sysId: s.id, hostname: s.hostname })), env,
        justification: c.justification || pick(JUSTIFY), compensating: pick(COMPENSATING),
        approver: c.justifiedBy === "mreyes" ? "Mira Reyes" : c.justifiedBy === "jpark" ? "Jordan Park" : "Security Team",
        approvedAt, reviewDate,
        history: [{ at: approvedAt, who: c.justifiedBy || "security-team", text: "Risk accepted from CVE triage." }],
      });
    });
  });
  out.sort((a, b) => a.approvedAt < b.approvedAt ? -1 : 1);
  out.forEach((r, i) => { r.id = `RA-${String(i + 1).padStart(4, "0")}`; });

  const RISK_ACCEPTANCES = out;
  const raById = (id) => RISK_ACCEPTANCES.find(r => r.id === id) || null;
  const raDaysLeft = (r) => r.reviewDate ? Math.round((new Date(r.reviewDate) - new Date(T)) / 86400000) : null;
  const raIsExpired = (r) => r.status === "active" && !!r.reviewDate && r.reviewDate < T;
  const raForFinding = (sysId, policyId) => RISK_ACCEPTANCES.find(r => r.findings.some(f => f.sysId === sysId && f.policyId === policyId)) || null;
  const bump = () => (typeof poamStoreBump === "function" ? poamStoreBump() : null);

  function raRenew(id, days) {
    const r = raById(id); if (!r) return;
    r.reviewDate = addDays(T, days);
    r.history.push({ at: T, who: "you", text: `Re-reviewed and renewed until ${r.reviewDate}.` });
    bump();
  }
  function raRevoke(id) {
    const r = raById(id); if (!r) return;
    r.status = "revoked";
    r.history.push({ at: T, who: "you", text: "Acceptance revoked — findings return to outstanding." });
    bump();
  }
  // The usual end of a lapsed acceptance: the deficiency gets a real remediation plan.
  function raConvertToPoam(id) {
    const r = raById(id); if (!r || typeof poamCreate !== "function") return null;
    const item = poamCreate({
      title: r.title, severity: r.severity, owner: "unassigned", status: "open",
      due: addDays(T, r.severity === "high" ? 30 : 90),
      plan: `Replaces risk acceptance ${r.id}. ${r.justification}`,
      findings: r.findings, cveRefs: r.cveRefs,
    });
    r.status = "converted";
    r.poamId = item.id;
    r.history.push({ at: T, who: "you", text: `Converted to ${item.id}.` });
    bump();
    return item;
  }

  Object.assign(window, { RISK_ACCEPTANCES, raById, raDaysLeft, raIsExpired, raForFinding, raRenew, raRevoke, raConvertToPoam });
})();
