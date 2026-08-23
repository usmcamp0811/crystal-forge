// POA&M — Plan of Action and Milestones.
//
// A POA&M is the *remediation plan* for a known deficiency. It is deliberately NOT an
// evaluation result: a finding with an open POA&M is still a failing finding. The evaluation
// answers "is this true?"; the POA&M answers "what are we doing about it?".
//
// Model: a POA&M links to one or more findings, where a finding is the (system, policy) pair
// that evaluated non-compliant. One remediation effort may cover the same control failing on
// many hosts, or several controls caused by one shared configuration problem.

const POAM_TODAY = "2026-08-22";

const POAM_STATUS = {
  open:                  { label:"Open",                  color:"#9ca3af", blurb:"Deficiency acknowledged; remediation not started." },
  in_progress:           { label:"In Progress",           color:"#60a5fa", blurb:"Remediation work underway." },
  blocked:               { label:"Blocked",               color:"#fbbf24", blurb:"Remediation cannot proceed — dependency or decision pending." },
  awaiting_verification: { label:"Awaiting Verification", color:"#a78bfa", blurb:"Work reported complete; waiting on a passing Crystal Forge evaluation." },
  completed:             { label:"Completed",             color:"#34d399", blurb:"Remediation verified by a passing evaluation and closed." },
};
const POAM_STATUS_ORDER = ["open","in_progress","blocked","awaiting_verification","completed"];

// Demo determinism: the mock derives evaluation status from a hash of (system, policy). These
// overrides pin the specific findings the seeded POA&M items talk about so the walkthrough is
// stable — including one control that now PASSES because its POA&M was remediated and closed.
const POAM_FINDING_STATUS_OVERRIDE = {
  "sys-6::stig-fips": "fail",       // gaia-web-01 — failing, no POA&M yet
  "sys-7::stig-fips": "fail",       // gaia-web-02 — on POAM-0027
  "sys-7::stig-sshd": "fail",       // gaia-web-02 ┐
  "sys-8::stig-sshd": "fail",       // gaia-web-03 ├ one shared POA&M (POAM-0042)
  "sys-9::stig-sshd": "fail",       // gaia-web-04 ┘
  "sys-14::stig-usbguard": "fail",  // stg-web-01 — overdue POAM-0031
  "sys-1::stig-banner": "fail",     // atlas-02 — POAM-0035 awaiting verification, still failing
  "sys-3::stig-auditd": "pass",     // hydra-01 — remediated and verified; POAM-0019 closed
};

const POAMS = [
  {
    id: "POAM-0019",
    title: "Restore auditd rule set on hydra-01",
    severity: "high",
    status: "completed",
    owner: "Platform Team",
    due: "2026-08-08",
    opened: "2026-07-11",
    closed: "2026-08-05",
    plan: "The audit rule module was dropped during the 24.11 nixpkgs bump. Re-add security.audit.rules to the hydra role module, deploy, and confirm the identity/privilege/exec watch rules are loaded.",
    findings: [{ sysId:"sys-3", policyId:"stig-auditd", bundleId:"disa-rhel9-stig" }],
    milestones: [
      { text:"Re-add audit rules to hydra role module", due:"2026-07-18", done:true, doneAt:"2026-07-17" },
      { text:"Deploy to staging and diff auditctl -l", due:"2026-07-25", done:true, doneAt:"2026-07-24" },
      { text:"Deploy to hydra-01", due:"2026-08-01", done:true, doneAt:"2026-07-31" },
      { text:"Verify compliance evaluation passes", due:"2026-08-08", done:true, doneAt:"2026-08-05" },
    ],
    verification: { evalId:"eval-8841", at:"2026-08-05", commit:"5c1de9a2", result:"pass", note:"stig-auditd evaluated pass on hydra-01; audit log excerpt and rendered config collected." },
    activity: [
      { at:"2026-07-11", who:"r.chen", text:"POA&M created from failing finding hydra-01 / stig-auditd." },
      { at:"2026-07-31", who:"platform-bot", text:"Generation 158 activated on hydra-01." },
      { at:"2026-08-01", who:"m.reyes", text:"Marked remediation complete — moved to Awaiting Verification." },
      { at:"2026-08-05", who:"crystal-forge", text:"Evaluation eval-8841 passed. POA&M completed and closed." },
    ],
  },
  {
    id: "POAM-0027",
    title: "Enable FIPS mode and LUKS on gaia-web-02",
    severity: "high",
    status: "in_progress",
    owner: "Platform Team",
    due: "2026-09-30",
    opened: "2026-08-04",
    plan: "Host predates the encrypted-root image. Rebuild with security.enableFIPSMode and a LUKS root, migrate the data volume during the September maintenance window, then re-evaluate.",
    findings: [{ sysId:"sys-7", policyId:"stig-fips", bundleId:"disa-rhel9-stig" }],
    milestones: [
      { text:"Add FIPS + LUKS options to gaia-web role module", due:"2026-08-15", done:true, doneAt:"2026-08-13" },
      { text:"Deploy to stg-web-01 and validate boot", due:"2026-08-29", done:true, doneAt:"2026-08-21" },
      { text:"Schedule data-volume migration window", due:"2026-09-05", done:false },
      { text:"Deploy to gaia-web-02", due:"2026-09-19", done:false },
      { text:"Verify compliance evaluation passes", due:"2026-09-30", done:false },
    ],
    activity: [
      { at:"2026-08-04", who:"r.chen", text:"POA&M created from failing finding gaia-web-02 / stig-fips-crypto." },
      { at:"2026-08-13", who:"j.okafor", text:"Module change merged in infrastructure@e91f2c." },
      { at:"2026-08-21", who:"j.okafor", text:"Staging validation clean — fips_enabled = 1, LUKS root present." },
    ],
  },
  {
    id: "POAM-0031",
    title: "USBguard allow-list for the staging web tier",
    severity: "medium",
    status: "in_progress",
    owner: "Endpoint Team",
    due: "2026-07-15",
    opened: "2026-06-02",
    plan: "usbguard.service is enabled but has no allow-list, so the control fails open. Collect the approved peripheral inventory for the staging lab, generate rules, and apply.",
    findings: [{ sysId:"sys-14", policyId:"stig-usbguard", bundleId:"disa-rhel9-stig" }],
    milestones: [
      { text:"Inventory approved peripherals in staging lab", due:"2026-06-20", done:true, doneAt:"2026-06-18" },
      { text:"Generate usbguard rules from inventory", due:"2026-07-01", done:false },
      { text:"Deploy and verify compliance evaluation", due:"2026-07-15", done:false },
    ],
    activity: [
      { at:"2026-06-02", who:"a.novak", text:"POA&M created from failing finding stg-web-01 / stig-usbguard." },
      { at:"2026-07-16", who:"crystal-forge", text:"Target completion date passed — POA&M is overdue." },
      { at:"2026-08-11", who:"a.novak", text:"Blocked on lab hardware audit; requesting date extension to 2026-09-15." },
    ],
  },
  {
    id: "POAM-0035",
    title: "DoD login banner rollout — atlas-02",
    severity: "low",
    status: "awaiting_verification",
    owner: "Platform Team",
    due: "2026-08-28",
    opened: "2026-08-06",
    plan: "Apply the standard consent banner via environment.etc.\"issue\" and services.openssh.banner, then let the next evaluation collect the console capture as evidence.",
    findings: [{ sysId:"sys-1", policyId:"stig-banner", bundleId:"disa-rhel9-stig" }],
    milestones: [
      { text:"Add banner module to atlas role", due:"2026-08-12", done:true, doneAt:"2026-08-11" },
      { text:"Deploy to atlas-02", due:"2026-08-20", done:true, doneAt:"2026-08-20" },
      { text:"Verify compliance evaluation passes", due:"2026-08-28", done:false },
    ],
    activity: [
      { at:"2026-08-06", who:"r.chen", text:"POA&M created from failing finding atlas-02 / stig-banner." },
      { at:"2026-08-20", who:"j.okafor", text:"Remediation reported complete — moved to Awaiting Verification." },
      { at:"2026-08-21", who:"crystal-forge", text:"Latest evaluation still reports fail — banner not present in /etc/issue on the running generation." },
    ],
  },
  {
    id: "POAM-0042",
    title: "Replace legacy SSH configuration across the gaia web tier",
    severity: "high",
    status: "in_progress",
    owner: "Platform Team",
    due: "2026-10-15",
    opened: "2026-08-12",
    plan: "One shared cause: the gaia-web role still imports the pre-hardening sshd fragment, so PermitRootLogin and the cipher list fail on every host in the tier. Replace the fragment with the hardened module and roll the tier host by host.",
    findings: [
      { sysId:"sys-7", policyId:"stig-sshd", bundleId:"disa-rhel9-stig" },
      { sysId:"sys-8", policyId:"stig-sshd", bundleId:"disa-rhel9-stig" },
      { sysId:"sys-9", policyId:"stig-sshd", bundleId:"disa-rhel9-stig" },
    ],
    milestones: [
      { text:"Write hardened sshd module for gaia-web role", due:"2026-08-22", done:true, doneAt:"2026-08-19" },
      { text:"Canary on gaia-web-04", due:"2026-09-05", done:false },
      { text:"Roll remaining tier hosts", due:"2026-10-01", done:false },
      { text:"Verify compliance evaluation passes on all three hosts", due:"2026-10-15", done:false },
    ],
    activity: [
      { at:"2026-08-12", who:"r.chen", text:"POA&M created from failing finding gaia-web-02 / stig-sshd." },
      { at:"2026-08-12", who:"r.chen", text:"Linked gaia-web-03 and gaia-web-04 — same control, same root cause." },
      { at:"2026-08-19", who:"j.okafor", text:"Hardened module merged; canary scheduled for the 5th." },
    ],
  },
  {
    id: "POAM-0123",
    title: "Migrate hydra-03 off the v1r1 STIG baseline",
    severity: "high",
    status: "blocked",
    owner: "Security Team",
    due: "2026-10-01",
    opened: "2026-05-14",
    plan: "hydra-03 is pinned to the v1r1 revision because its Oracle-compatible kernel module is incompatible with the v1r2 FIPS crypto requirement. Track the vendor rebuild, then move the host to the current baseline and drop the exception.",
    findings: [],
    assignmentRef: { sysId:"sys-5", lineageId:"disa-nixos-stig", bundleId:"disa-rhel9-stig-r1" },
    milestones: [
      { text:"Vendor delivers FIPS-compatible kernel module", due:"2026-09-01", done:false },
      { text:"Rebuild hydra-03 on v1r2 baseline in lab", due:"2026-09-15", done:false },
      { text:"Move assignment to current revision, drop exception", due:"2026-10-01", done:false },
    ],
    activity: [
      { at:"2026-05-14", who:"j.alvarez", text:"POA&M opened alongside the baseline exception for hydra-03." },
      { at:"2026-07-30", who:"j.alvarez", text:"Vendor slipped the module rebuild to Q4 — status Blocked." },
    ],
  },
];

/* ── Finding identity ─────────────────────────────────────────────────────── */
function poamFindingKey(sysId, policyId) { return `${sysId}::${policyId}`; }
function poamSameFinding(f, sysId, policyId) { return f.sysId === sysId && f.policyId === policyId; }

function poamsForFinding(sysId, policyId) {
  return POAMS.filter(p => p.findings.some(f => poamSameFinding(f, sysId, policyId)));
}
// The POA&M a finding is currently *managed* by: prefer an item that is still open.
function poamForFinding(sysId, policyId) {
  const list = poamsForFinding(sysId, policyId);
  return list.find(p => p.status !== "completed") || list[0] || null;
}
function poamsForSystem(sysId) {
  return POAMS.filter(p => p.findings.some(f => f.sysId === sysId) || p.assignmentRef?.sysId === sysId);
}
function poamsForBundle(bundle) {
  const ids = new Set(bundle.policyIds || []);
  return POAMS.filter(p => p.findings.some(f => ids.has(f.policyId)) || p.assignmentRef?.lineageId === (bundle.lineageId || bundle.id));
}
function poamById(id) { return POAMS.find(p => p.id === id) || null; }
// POA&M items that touch one host within one bundle's scope.
function systemBundlePoams(bundle, sysId) {
  const ids = new Set(bundle.policyIds || []);
  const lineage = bundle.lineageId || bundle.id;
  return POAMS.filter(p =>
    p.findings.some(f => f.sysId === sysId && ids.has(f.policyId))
    || (p.assignmentRef?.sysId === sysId && p.assignmentRef?.lineageId === lineage));
}

/* ── Derived state ────────────────────────────────────────────────────────── */
function poamIsOverdue(p) { return p.status !== "completed" && !!p.due && p.due < POAM_TODAY; }
function poamDaysLeft(p) {
  if (!p.due) return null;
  return Math.round((new Date(p.due) - new Date(POAM_TODAY)) / 86400000);
}
function poamMilestoneProgress(p) {
  const total = (p.milestones || []).length;
  const done = (p.milestones || []).filter(m => m.done).length;
  return { done, total, pct: total ? Math.round(done / total * 100) : 0 };
}
function poamCounts(list) {
  return {
    total: list.length,
    open: list.filter(p => p.status !== "completed").length,
    overdue: list.filter(poamIsOverdue).length,
    awaiting: list.filter(p => p.status === "awaiting_verification").length,
    completed: list.filter(p => p.status === "completed").length,
  };
}
function poamShortDate(d) {
  if (!d) return "—";
  const [y, m, day] = d.split("-");
  const mo = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"][Number(m) - 1];
  return `${mo} ${Number(day)}`;
}
// The requirement id an auditor cites — parsed from the STIG rationale where present.
function poamRequirementLabel(policyId) {
  const p = (typeof POLICIES !== "undefined" ? POLICIES : []).find(x => x.id === policyId);
  if (!p) return policyId;
  if (p.vulnId) return p.vulnId;
  const m = (p.rationale || "").match(/V-\d{5,6}/);
  if (m) return m[0];
  return p.srgIds?.[0] || p.name || policyId;
}
function poamSeverityLabel(sev) {
  return { high:"CAT I", medium:"CAT II", low:"CAT III" }[sev] || sev || "—";
}
function poamSeverityColor(sev) {
  return { high:"#f87171", medium:"#fbbf24", low:"#60a5fa" }[sev] || "#9ca3af";
}

/* ── Mutation — in-memory store with a change event so views re-render ───── */
function poamStoreBump() { window.dispatchEvent(new CustomEvent("cf-poam-change")); }

function poamNextId() {
  const max = POAMS.reduce((a, p) => {
    const n = Number((p.id.match(/(\d+)$/) || [])[1] || 0);
    return Math.max(a, n);
  }, 0);
  return `POAM-${String(max + 1).padStart(4, "0")}`;
}

function poamCreate(draft) {
  window.__cfCoach?.complete("poam");
  const item = {
    id: poamNextId(),
    title: draft.title,
    severity: draft.severity || "medium",
    status: draft.status || "open",
    owner: draft.owner || "unassigned",
    due: draft.due || "",
    opened: POAM_TODAY,
    plan: draft.plan || "",
    notes: draft.notes || "",
    findings: draft.findings || [],
    milestones: draft.milestones || [],
    activity: [{ at: POAM_TODAY, who: "you", text: draft.findings?.length
      ? `POA&M created from failing finding ${draft.findings.map(f => f.sysId).length > 1 ? `${draft.findings.length} findings` : poamFindingLabel(draft.findings[0])}.`
      : "POA&M created." }],
  };
  POAMS.unshift(item);
  poamStoreBump();
  return item;
}

function poamFindingLabel(f) {
  const sys = (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).find(s => s.id === f.sysId);
  return `${sys ? sys.hostname : f.sysId} / ${poamRequirementLabel(f.policyId)}`;
}

function poamLinkFinding(poamId, finding) {
  const p = poamById(poamId);
  if (!p) return;
  if (p.findings.some(f => poamSameFinding(f, finding.sysId, finding.policyId))) return;
  p.findings.push(finding);
  p.activity.push({ at: POAM_TODAY, who: "you", text: `Linked finding ${poamFindingLabel(finding)}.` });
  poamStoreBump();
}
function poamUnlinkFinding(poamId, finding) {
  const p = poamById(poamId);
  if (!p) return;
  p.findings = p.findings.filter(f => !poamSameFinding(f, finding.sysId, finding.policyId));
  p.activity.push({ at: POAM_TODAY, who: "you", text: `Unlinked finding ${poamFindingLabel(finding)}.` });
  poamStoreBump();
}
function poamSetStatus(poamId, status, note) {
  const p = poamById(poamId);
  if (!p || p.status === status) return;
  const from = POAM_STATUS[p.status]?.label || p.status;
  p.status = status;
  if (status === "completed") p.closed = POAM_TODAY; else delete p.closed;
  p.activity.push({ at: POAM_TODAY, who: "you", text: note || `Status changed from ${from} to ${POAM_STATUS[status]?.label || status}.` });
  poamStoreBump();
}
function poamSetField(poamId, key, value) {
  const p = poamById(poamId);
  if (!p) return;
  p[key] = value;
  poamStoreBump();
}
function poamToggleMilestone(poamId, idx) {
  const p = poamById(poamId);
  const m = p?.milestones?.[idx];
  if (!m) return;
  m.done = !m.done;
  m.doneAt = m.done ? POAM_TODAY : undefined;
  p.activity.push({ at: POAM_TODAY, who: "you", text: `Milestone ${m.done ? "completed" : "reopened"}: ${m.text}` });
  poamStoreBump();
}
function poamAddMilestone(poamId, text, due) {
  const p = poamById(poamId);
  if (!p || !text.trim()) return;
  p.milestones = p.milestones || [];
  p.milestones.push({ text: text.trim(), due: due || "", done: false });
  p.activity.push({ at: POAM_TODAY, who: "you", text: `Milestone added: ${text.trim()}` });
  poamStoreBump();
}
function poamRemoveMilestone(poamId, idx) {
  const p = poamById(poamId);
  if (!p?.milestones?.[idx]) return;
  p.milestones.splice(idx, 1);
  poamStoreBump();
}
function poamAddNote(poamId, text) {
  const p = poamById(poamId);
  if (!p || !text.trim()) return;
  p.activity.push({ at: POAM_TODAY, who: "you", text: text.trim() });
  poamStoreBump();
}

// Live evaluation status for each linked finding — the POA&M never overrides it.
function poamFindingStatus(finding) {
  const sys = (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).find(s => s.id === finding.sysId);
  const bundle = (typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : []).find(b => b.id === finding.bundleId)
    || (typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : []).find(b => (b.policyIds || []).includes(finding.policyId));
  if (!sys || !bundle || typeof evidenceForControl !== "function") return null;
  return evidenceForControl(bundle, finding.policyId, sys).status;
}
// A POA&M can be closed once every linked finding evaluates clean.
function poamVerificationReady(p) {
  if (!p.findings.length) return false;
  return p.findings.every(f => { const s = poamFindingStatus(f); return s === "pass" || s === "waiver"; });
}

Object.assign(window, {
  POAMS, POAM_STATUS, POAM_STATUS_ORDER, POAM_TODAY, POAM_FINDING_STATUS_OVERRIDE,
  poamFindingKey, poamsForFinding, poamForFinding, poamsForSystem, poamsForBundle, poamById, systemBundlePoams,
  poamIsOverdue, poamDaysLeft, poamMilestoneProgress, poamCounts, poamShortDate,
  poamRequirementLabel, poamSeverityLabel, poamSeverityColor, poamFindingLabel,
  poamStoreBump, poamNextId, poamCreate, poamLinkFinding, poamUnlinkFinding, poamSetStatus,
  poamSetField, poamToggleMilestone, poamAddMilestone, poamRemoveMilestone, poamAddNote,
  poamFindingStatus, poamVerificationReady,
});
