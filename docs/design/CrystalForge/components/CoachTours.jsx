// Crystal Forge Coach — SECURITY WORKFLOWS track.
// Educational walkthroughs only. Progress recorded here is presentation state
// ("Walkthrough completed"), never security-domain state. The runner navigates and
// opens read-only views (drawers, tabs, unsaved forms); it never submits anything.

const COACH_ROLE_LEVEL = { viewer: 0, operator: 1, admin: 2 };
const COACH_ROLE_LABEL = { viewer: "Viewer", operator: "Operator", admin: "Admin" };

// Deterministic representative records so every demo state renders the same thing.
const coachPick = {
  cve: () => { const L = typeof CVES !== "undefined" ? CVES : []; return L.find(c => c.severity === "critical" && c.acceptance === "outstanding" && c.fix === "available") || L.find(c => c.acceptance === "outstanding") || L[0]; },
  sys: () => { const L = typeof SYSTEMS !== "undefined" ? SYSTEMS : []; return L.find(s => s.cves && s.cves.critical > 0 && s.health !== "offline") || L[0]; },
  bundleFinding: () => {
    const B = typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : [];
    for (const b of B) {
      if (typeof bundleStatusForSystem !== "function") break;
      const s = SYSTEMS.find(x => { const r = bundleStatusForSystem(b, x); return r.applies && r.fail > 0; });
      if (s) return { bundleId: b.id, sysId: s.id };
    }
    return B[0] ? { bundleId: B[0].id, sysId: SYSTEMS[0].id } : null;
  },
  poamOpen: () => { const L = typeof POAMS !== "undefined" ? POAMS : []; return L.find(p => p.status === "in_progress" && ((p.findings || []).length + (p.cveRefs || []).length) > 1) || L.find(p => p.status !== "completed") || L[0]; },
  poamAwaiting: () => { const L = typeof POAMS !== "undefined" ? POAMS : []; return L.find(p => p.status === "awaiting_verification") || L[0]; },
  ra: () => { const L = typeof RISK_ACCEPTANCES !== "undefined" ? RISK_ACCEPTANCES : []; return L.find(r => r.status === "active" && r.kind === "cve") || L[0]; },
};
window.coachPick = coachPick;

// ─── Modules ───
// stop: { id, title, why, doing, important?, figure?, gate?, gatedDoing?, go(), prep[], target, gatedPrep?, gatedTarget? }
// gate = minimum role for the ACTION this stop points at. Below it, the stop still
// runs read-only and shows a permission notice instead of opening the action.
const SECURITY_MODULES = [
  {
    key: "A", title: "Review vulnerability posture", icon: "shield",
    purpose: "Where CVE evidence comes from, what scan status does and doesn't tell you, and which evidence tier you're reading.",
    stops: [
      { id: "A1", title: "Scan status is not a CVE result",
        why: "These counters describe the scan lifecycle: what is running, past its rescan interval, never scanned, or failed. Coverage is the share of tracked configurations that have a result.",
        doing: "Compare Failed and Never scanned against Coverage, then switch between the Active and Completed tabs below.",
        important: "A failed scan is not “no CVEs”, and an unscanned configuration is not clean. Both mean evidence is missing.",
        go: () => ({ view: "scanning" }), target: [".stat-strip|Scanning now"] },
      { id: "A2", title: "Why evidence exists, or doesn't",
        why: "A scan record names the exact flake, commit and configuration it ran against, its lifecycle, and the scanner's own log.",
        doing: "Read the failure in Log, confirm the target identity in Details. Retry scan appears when the failure is retryable. Completed history can be archived and restored.",
        important: "Archiving hides a record from default lists. The result itself is retained.",
        go: () => ({ view: "scanning" }), prep: [{ click: ".stat|Failed" }], target: [".fl-tray|Log|last", ".side-panel", "[role=dialog]"] },
      { id: "A3", title: "Scan schedule", gate: "admin",
        why: "Scanning follows this policy, not every commit. Each row trades builder time against how fresh evidence stays for a class of configuration.",
        doing: "Read each control, then close without saving. The coach never saves the schedule.",
        gatedDoing: "Administrators set this schedule. You can see where it lives; the rules below explain why some configurations have older evidence than others.",
        figure: "schedule",
        go: () => ({ view: "scanning" }), prep: [{ click: [".page-head .btn|Schedule", ".btn|Schedule"] }], target: [".modal|Scan on build"],
        gatedTarget: [".page-head .btn|Schedule", ".btn|Schedule"] },
      { id: "A4", title: "Fleet CVEs",
        why: "One row per CVE and canonical package: severity, whether a fixed version exists, triage state, and how many systems carry it.",
        doing: "Filter by severity or environment, then change Group to package to see which single upgrade removes the most exposure. Open any row for its detail drawer.",
        go: () => ({ view: "cves" }), target: [".stat-strip|Patchable"] },
      { id: "A5", title: "Current, Scheduled, Historical",
        why: "Current exact evidence is the only tier that can be triaged. Scheduled and Historical evidence stay visible for planning and audit but are read-only.",
        doing: "Read which hosts sit in each tier before deciding anything about this CVE.",
        important: "Missing evidence does not mean clean. The same host can appear under Current and Scheduled when both exact configurations contain the package.",
        figure: "relations",
        go: () => ({ view: "cves", cve: coachPick.cve()?.id }), target: ["[data-coach-target=\"cve-relations\"]"] },
      { id: "A6", title: "Three kinds of evidence per system",
        why: "System Detail separates package vulnerabilities, hardening posture and compliance assessment. Each has its own source and its own remediation path.",
        doing: "Open CVEs, Hardening and Compliance in turn on this host.",
        important: "They are not interchangeable. A clean Hardening result says nothing about package CVEs.",
        figure: "tabs",
        go: () => ({ view: "systems", sysId: coachPick.sys()?.id, tab: "cves" }), target: [".sd-tabs|Hardening"] },
    ],
  },
  {
    key: "B", title: "Triage vulnerabilities", icon: "activity", role: "operator",
    purpose: "Deciding per environment whether to leave a CVE open, accept the risk, or schedule a patch — alone or in a batch.",
    stops: [
      { id: "B1", title: "Read the exact finding first",
        why: "The drawer shows the CVE, canonical package, CVSS, fixed version when the advisory publishes one, and the environments and hosts carrying it.",
        doing: "Confirm the fixed version and which environments hold Current evidence before you decide.",
        important: "Values the scanner did not report stay blank. Don't infer them.",
        go: () => ({ view: "cves", cve: coachPick.cve()?.id }), target: [".fl-tray .ed-stats"] },
      { id: "B2", title: "Triage by environment", gate: "operator",
        why: "Each environment gets its own disposition, so you can accept in dev and schedule a patch in production in one pass.",
        doing: "Pick a disposition per environment and read what each requires. Cancel when done; the coach never applies triage.",
        gatedDoing: "Triage requires Operator permission. The three dispositions below are what an Operator chooses between.",
        important: "Accepted risk is not remediation. The CVE stays listed and the finding does not pass.",
        figure: "dispositions",
        go: () => ({ view: "cves", cve: coachPick.cve()?.id }), prep: [{ click: ".fl-tray .btn|Triage" }], target: [".modal"],
        gatedTarget: [".fl-tray .btn|Triage"] },
      { id: "B3", title: "Schedule patch opens a POA&M", gate: "operator",
        why: "Scheduling needs a typed user or group assignee, a target completion and a remediation plan; standard patch milestones are prefilled. Crystal Forge derives the finding identity from the exact source evidence.",
        doing: "Look over the POA&M fields, then Cancel.",
        gatedDoing: "Scheduling requires Operator permission and Current exact evidence.",
        important: "POA&M status is not verification. The CVE stays open until newer exact scan evidence shows it gone.",
        go: () => ({ view: "cves", cve: coachPick.cve()?.id }), prep: [{ click: ".fl-tray .btn|Triage" }, { click: ".modal .seg button|Schedule patch" }], target: [".modal"],
        gatedTarget: [".fl-tray .btn|Triage"] },
      { id: "B4", title: "Select exact pairs deliberately",
        why: "SELECT chips add Critical, High, Patchable or Outstanding rows; ⌘/Ctrl-click a package header to take a whole package. The bar counts selected exact pairs and distinct packages.",
        doing: "Build a selection that genuinely shares one decision, such as the patchable criticals in one package.",
        important: "Batch triage is for identical decisions. Different rationale belongs in a separate batch.",
        go: () => ({ view: "cves" }), prep: [{ click: ".cve-sel-chip|Critical" }], target: [".bulk-bar", ".cve-sel-bar"] },
      { id: "B5", title: "Triage N together", gate: "operator",
        why: "One rationale is written into individually auditable decisions, one per exact pair.",
        doing: "Check the per-environment affected counts and the grouping choice, then Cancel.",
        gatedDoing: "Batch triage requires Operator permission.",
        important: "The batch commits atomically or not at all.",
        figure: "batch",
        go: () => ({ view: "cves" }), prep: [{ click: ".cve-sel-chip|Critical" }, { click: ".bulk-bar .btn|together" }], target: [".modal|together"],
        gatedPrep: [{ click: ".cve-sel-chip|Critical" }], gatedTarget: [".bulk-bar"] },
    ],
  },
  {
    key: "C", title: "Review compliance evidence", icon: "check",
    purpose: "How policies, bundles, versioned assignments and enforcement modes produce the per-control evidence you act on.",
    stops: [
      { id: "C1", title: "Policies are individual criteria",
        why: "Security controls carry framework identity. Group them by NIST 800-53 family, STIG severity (CAT), CCI, SRG category, CMMC level, CIS section or remediation status.",
        doing: "Open the Security controls tab, then change the grouping and watch the same controls regroup.",
        adminNote: "New custom policy and Import / Export are administrator actions.",
        go: () => ({ view: "policies" }), target: [".pol-domain-tabs"] },
      { id: "C2", title: "Bundles organize policies into a baseline",
        why: "Each bundle has a lineage, numbered versions, a publication state, a framework, a score and a set of systems in scope.",
        doing: "Scan the list for publication state and version before comparing scores.",
        adminNote: "Import STIG / XCCDF, New bundle and Export are administrator actions.",
        go: () => ({ view: "compliance" }), target: [".page-head"] },
      { id: "C3", title: "Assignments pin versions by scope",
        why: "A bundle is not applied globally. Each environment's settings pick an exact bundle version, so production can stay on one revision while staging assesses the next. A system can carry its own pin.",
        doing: "Open the bundle list to see each version with its revision and publication state. Close without saving; the coach never changes an assignment.",
        gatedDoing: "Administrators set assignments. You can read the assigned version on the environment and in each bundle's header.",
        gate: "admin",
        figure: "assignments",
        go: () => ({ view: "environments" }), prep: [{ call: () => window.dispatchEvent(new CustomEvent("cf-env-edit", { detail: { section: "policy" } })) }], target: ["[data-coach-target=\"env-bundle-assignment\"] select"],
        gatedPrep: [], gatedTarget: [".page-head"] },
      { id: "C4", title: "Enforce vs Report only",
        why: "Each environment's bundle assignment picks a mode here in its settings. The mode decides whether a failure can block deployment. It never decides whether the failure is real.",
        doing: "Toggle Enforce and Report only to compare what each does, then close without saving. The coach never changes an assignment.",
        gatedDoing: "Administrators set the assignment mode. Operators and Viewers read it on the environment.",
        gate: "admin",
        important: "Report-only FAIL is still FAIL. Creating a POA&M does not change FAIL to PASS.",
        figure: "enforce",
        go: () => ({ view: "environments" }), prep: [{ call: () => window.dispatchEvent(new CustomEvent("cf-env-edit", { detail: { section: "policy" } })) }], target: ["[data-coach-target=\"env-assignment-mode\"]", "[data-coach-target=\"env-bundle-assignment\"]"],
        gatedPrep: [], gatedTarget: [".page-head"] },
      { id: "C5", title: "System matrix",
        why: "Every host in scope with its pass, warning and fail counts and any linked POA&M.",
        doing: "Filter to Failing, then No POA&M to find failures nobody owns yet. Open a failing host.",
        go: () => ({ view: "compliance", bundleId: coachPick.bundleFinding()?.bundleId }), target: [".fl-tray .seg|No POA&M"] },
      { id: "C6", title: "Per-control evidence",
        why: "This is the authoritative place to learn why a control fails: framework release, requirement, policy, exact host, result, source evidence and any remediation already linked.",
        doing: "Step through the controls and read the source evidence, not just the result chip.",
        go: () => { const f = coachPick.bundleFinding(); return f ? { view: "compliance", finding: f } : { view: "compliance" }; }, target: [".fl-tray|Evidence|last", ".fl-tray|last"] },
      { id: "C7", title: "Create remediation from the finding", gate: "operator",
        why: "A POA&M created here is linked to this finding, so ownership and verification follow the evidence.",
        doing: "Find the POA&M action on a failing control. Open it to see what it would record, then cancel.",
        gatedDoing: "Creating a POA&M requires Operator permission.",
        important: "Don't create an unrelated free-floating plan to work around finding ownership. A failing report-only finding still qualifies for a POA&M.",
        go: () => { const f = coachPick.bundleFinding(); return f ? { view: "compliance", finding: f } : { view: "compliance" }; }, target: [".fl-tray button|POA&M|last", ".fl-tray|last"] },
    ],
  },
  {
    key: "D", title: "Manage remediation and accepted risk", icon: "activity", role: "operator",
    purpose: "Working the POA&M register: plans, acceptances, queues, lifecycle, verification and renewals.",
    stops: [
      { id: "D1", title: "One register, two kinds of record",
        why: "Remediation plan: a commitment to fix. Risk acceptance: a decision to let the finding stand for now. They cover the same findings but are different source records.",
        doing: "Switch between Everything, Remediation plans and Risk acceptances.",
        go: () => ({ view: "poams", focus: { kind: "all" } }), target: [".rr-kinds"] },
      { id: "D2", title: "Work queues",
        why: "Queues sort records by what needs a person next: overdue, due in 14 days, awaiting verification, blocked, quiet, unassigned; for acceptances, expired, review soon, no review date.",
        doing: "Click a queue to filter the list. Click again to clear it.",
        important: "Queues are operator work lists. They are not evidence outcomes.",
        figure: "queues",
        go: () => ({ view: "poams", focus: { kind: "all" } }), target: [".pv-main", ".pv-queues"] },
      { id: "D3", title: "Scope and grouping",
        why: "Browse by Environment, Bundle or Owner / Approver. Chips narrow the list; Focus drills into one group without losing the rest.",
        doing: "Pick an environment chip, then clear it.",
        important: "A host-only source decision that recorded no environment is shown without one. Crystal Forge doesn't infer it.",
        go: () => ({ view: "poams", focus: { kind: "all" } }), target: [".rr-scope"] },
      { id: "D4", title: "Remediation plan detail",
        why: "POAM-#### is the human ID. The plan carries linked findings, owner, target completion, risk, milestones and history.",
        doing: "Read the linked findings table: each row keeps its own exact host and evidence identity.",
        important: "A POA&M can group several CVE findings when someone grouped them explicitly. Grouping never merges their evidence.",
        go: () => ({ view: "poams", poamId: coachPick.poamOpen()?.id }), target: [".poam-tray", ".fl-tray|last"] },
      { id: "D5", title: "Lifecycle", gate: "operator",
        why: "Open, In progress, Blocked and Awaiting verification record where the work is. Awaiting verification means remediation is reported complete; the technical finding is still independent.",
        doing: "Look at the status control. Changing status records workflow only.",
        gatedDoing: "Changing status requires Operator permission.",
        important: "POA&M status is not verification.",
        figure: "lifecycle",
        go: () => ({ view: "poams", poamId: coachPick.poamAwaiting()?.id }), target: ["[data-coach-target=\"poam-lifecycle\"]"] },
      { id: "D6", title: "Verify now, then close", gate: "operator",
        why: "Verify now checks authoritative current evidence for every linked finding. Authoritative close is only offered once that check passes.",
        doing: "Read what verification requires below. Verify now is safe to run; the coach does not close the POA&M.",
        gatedDoing: "Verification and closure require Operator permission.",
        figure: "verify",
        go: () => ({ view: "poams", poamId: coachPick.poamAwaiting()?.id }), target: ["[data-coach-target=\"poam-verify\"]"] },
      { id: "D7", title: "Risk acceptance register",
        why: "RA-#### identifies one operator-facing renewal chain. The exact source decision UUID stays the machine and audit identity.",
        doing: "Compare review dates and approvers across acceptances.",
        important: "A renewal can create a new immutable source decision under the same RA number. An unrelated new acceptance gets a new RA number.",
        go: () => ({ view: "poams", focus: { kind: "ra" } }), target: [".rr-kinds"] },
      { id: "D8", title: "Risk acceptance detail",
        why: "Source type, subject, approver, approval time, review deadline, exact scope and justification, plus the source-record identity.",
        doing: "Open Source record to see the typed source and UUID behind this RA number.",
        important: "Risk acceptance records a decision. It does not make a finding pass or mark it remediated.",
        go: () => ({ view: "poams", focus: { kind: "ra", raId: coachPick.ra()?.id } }), target: ["[data-coach-target=\"ra-truth\"]", ".poam-tray"] },
      { id: "D9", title: "Re-review or convert", gate: "operator",
        why: "Re-review renews the acceptance for 90 days. Convert to POA&M starts remediation without rewriting the original evidence; the acceptance stays in audit history.",
        doing: "Decide which applies. The coach won't renew or convert.",
        gatedDoing: "CVE acceptance changes require Operator permission; policy-waiver renewal and conversion can require Admin.",
        important: "There is no universal revoke. Changing course means renewing, converting, or a new decision.",
        go: () => ({ view: "poams", focus: { kind: "ra", raId: coachPick.ra()?.id } }), target: [".rr-tray-foot", ".poam-tray"] },
    ],
  },
  {
    key: "E", title: "Prepare audit evidence", icon: "download",
    purpose: "Which export answers which auditor question, and how acceptance identities survive into OSCAL.",
    stops: [
      { id: "E1", title: "Baseline vs assessed evidence",
        why: "A bundle's XCCDF export is the baseline definition: what is checked. The evidence package is assessed results: what was found, on which hosts, with POA&Ms and acceptances.",
        doing: "Use Import / Export for a bundle's XCCDF; use Export evidence package for an environment or host set.",
        figure: "exports",
        go: () => ({ view: "compliance" }), target: [".page-head .btn|Export evidence package"] },
      { id: "E2", title: "Register export",
        why: "The register exports as OSCAL JSON, Excel, CSV or OSCAL XML. It contains remediation plans and risk acceptances according to the record type and scope you have selected.",
        doing: "Set the record tab and scope first, then export.",
        go: () => ({ view: "poams", focus: { kind: "all" } }), prep: [{ click: ".rr-export .btn" }], target: [".rr-export-pop"] },
      { id: "E3", title: "Two identities per acceptance",
        why: "RA-0042 is the human lifecycle. The typed source and UUID are the exact immutable decision.",
        doing: "When reconciling an OSCAL export, match on source-id, not the RA number.",
        important: "RA-#### never replaces the source UUID.",
        figure: "identities",
        go: () => ({ view: "poams", focus: { kind: "ra", raId: coachPick.ra()?.id } }), target: ["[data-coach-target=\"ra-source\"]", ".poam-tray"] },
    ],
  },
];
window.SECURITY_MODULES = SECURITY_MODULES;

// ─── Runner utilities ───
const coachSleep = (ms) => new Promise(r => setTimeout(r, ms));
function coachFind(t) {
  if (!t) return null;
  for (const a of (Array.isArray(t) ? t : [t])) {
    const [sel, txt, mode] = a.split("|");
    let els;
    try { els = [...document.querySelectorAll(sel)]; } catch { continue; }
    els = els.filter(el => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0 && !el.closest(".coach, .coach-pill, .coach-spot"); });
    if (txt) els = els.filter(el => (el.textContent || "").includes(txt));
    if (els.length) return mode === "last" ? els[els.length - 1] : els[0];
  }
  return null;
}
async function coachWaitFor(t, timeout = 1800) {
  const t0 = Date.now();
  while (Date.now() - t0 < timeout) { const el = coachFind(t); if (el) return el; await coachSleep(90); }
  return null;
}
function coachGated(stop, role) { return !!stop.gate && COACH_ROLE_LEVEL[role] < COACH_ROLE_LEVEL[stop.gate]; }
async function coachRunStop(stop, role) {
  const gated = coachGated(stop, role);
  const go = stop.go ? stop.go() : null;
  if (go && window.cfCoachGo) window.cfCoachGo(go);
  await coachSleep(280);
  for (const s of ((gated ? stop.gatedPrep : stop.prep) || [])) {
    if (s.click) { const el = await coachWaitFor(s.click); if (el) { el.click(); await coachSleep(240); } }
    else if (s.call) { s.call(); await coachSleep(260); }
  }
}
Object.assign(window, { coachFind, coachWaitFor, coachRunStop, coachGated, COACH_ROLE_LEVEL, COACH_ROLE_LABEL });

// ─── Figures (compact, inside the coach card) ───
function CoachFigure({ kind }) {
  const Row = ({ c, l, d, tag }) => (
    <div className="cf-fig-row">
      {c && <span className="cf-fig-dot" style={{ background: c }}/>}
      <div style={{ minWidth: 0, flex: 1 }}><b>{l}</b>{d && <span>{d}</span>}</div>
      {tag && <em>{tag}</em>}
    </div>
  );
  if (kind === "schedule") return (
    <div className="cf-fig">
      <Row l="Scan on build" d="Scan a freshly built exact configuration before deployment."/>
      <Row l="Deployed configs" d="Rescan currently running configurations."/>
      <Row l="Recent configs" d="Rescan recent configurations that are not deployed."/>
      <Row l="Superseded configs" d="Reduce work for superseded configurations."/>
      <Row l="Rebuild to scan old configs" d="Permit policy-driven rebuilds when an archived closure is unavailable."/>
    </div>
  );
  if (kind === "relations") return (
    <div className="cf-fig">
      <Row c="#34d399" l="Current" d="Exact evidence for the running configuration." tag="triage"/>
      <Row c="#60a5fa" l="Scheduled deployment target" d="Exact evidence for a scheduled target." tag="read-only"/>
      <Row c="#9ca3af" l="Historical" d="Retained, no current or scheduled authority." tag="read-only"/>
    </div>
  );
  if (kind === "tabs") return (
    <div className="cf-fig">
      <Row l="CVEs" d="Package vulnerabilities and source-authoritative CVE triage."/>
      <Row l="Hardening" d="Hardening and posture evidence for the selected system state."/>
      <Row l="Compliance" d="Policy and bundle assessment evidence, remediation findings."/>
    </div>
  );
  if (kind === "dispositions") return (
    <div className="cf-fig">
      <Row c="#f87171" l="Leave open" d="No active accepted or scheduled decision for the environment."/>
      <Row c="#a78bfa" l="Accept risk" d="Needs a justification; review date optional. Tolerates the finding. Does not hide it, pass it, or claim remediation."/>
      <Row c="#60a5fa" l="Schedule patch" d="Creates or uses a POA&M. Needs Current exact evidence. Does not mean fixed or verified."/>
    </div>
  );
  if (kind === "batch") return (
    <ul className="cf-fig cf-fig-list">
      <li>Up to <b>100 exact pairs</b> per apply.</li>
      <li>Unavailable or inventory-only pairs stay listed and <b>block Apply</b>.</li>
      <li>Accept risk: one rationale, individually auditable decisions.</li>
      <li>Schedule patch: <b>One POA&M</b>, <b>One per package</b> or <b>One per environment</b>. A shared POA&M keeps each system/CVE/package finding separate.</li>
    </ul>
  );
  if (kind === "assignments") return (
    <div className="cf-fig">
      <div className="cf-fig-table">
        <span>Scope</span><span>Bundle version</span><span>Mode</span>
        <b>production</b><b className="mono">nixos-stig · v1r1</b><b>Enforce</b>
        <b>staging</b><b className="mono">nixos-stig · v1r2</b><b>Enforce</b>
        <b>dev</b><b className="mono">nixos-stig · v1r2</b><b>Report only</b>
      </div>
      <div className="cf-fig-note">Illustrative. Read the real assignment on the environment or system.</div>
    </div>
  );
  if (kind === "enforce") return (
    <div className="cf-fig cf-fig-two">
      <div><b>Enforce</b><span>A failing applicable policy can participate in deployment blocking under the enforcement rules.</span></div>
      <div><b>Report only</b><span>The failure is still FAIL. It produces a finding, supports a waiver or POA&M, retains evidence. It does not block deployment.</span></div>
    </div>
  );
  if (kind === "queues") return (
    <div className="cf-fig cf-fig-two">
      <div><b>Plans</b><span>Overdue · Due in 14 days · Awaiting verification · Blocked · No activity · Unassigned</span></div>
      <div><b>Acceptances</b><span>Expired · Review soon · No review · Accepted</span></div>
    </div>
  );
  if (kind === "lifecycle") return (
    <div className="cf-fig">
      <div className="cf-fig-flow">
        {["Open", "In progress", "Blocked", "Awaiting verification"].map(s => <span key={s}>{s}</span>)}
        <span className="gate">Verify now</span><span className="done">Authoritative close</span>
      </div>
    </div>
  );
  if (kind === "verify") return (
    <ul className="cf-fig cf-fig-list">
      <li>Policy findings: the <b>current assessment</b> decides.</li>
      <li>CVE findings: needs exact scan evidence <b>newer than the finding's baseline</b>.</li>
      <li className="bad">A current occurrence is FAIL.</li>
      <li className="bad">Missing or inconsistent evidence is not PASS.</li>
      <li className="bad">Operator justification or a scanner whitelist is not PASS.</li>
      <li className="good">Only exact absence in sufficiently new authoritative evidence closes a CVE finding.</li>
    </ul>
  );
  if (kind === "exports") return (
    <div className="cf-fig cf-fig-two">
      <div><b>Bundle XCCDF</b><span>Baseline definition: rules and checks.</span></div>
      <div><b>Evidence package</b><span>Assessed results per host, with POA&Ms and acceptances.</span></div>
    </div>
  );
  if (kind === "identities") return (
    <div className="cf-fig">
      <Row l="RA-0042" d="Human, operator-facing acceptance lifecycle. Crystal Forge property in OSCAL." />
      <Row l="cve_risk_acceptance · 7f3c…e21a" d="Typed source + UUID. Exact immutable decision. OSCAL source-id." />
    </div>
  );
  return null;
}
window.CoachFigure = CoachFigure;

// ─── Deterministic demo states ───
// Each maps to one reference render the implementation can be compared against.
const SETUP_ALL = { env: true, flake: true, builder: true, cache: true, system: true, policy: true, compliance: true, poam: true };
const COACH_DEMO_STATES = [
  { id: "setup-first-run",   label: "1 · Admin first-run Setup",            server: { observed: { env: true, flake: true }, agent: "none" }, ui: { track: "setup", panel: "expanded" }, view: "environments" },
  { id: "setup-agent-ack",   label: "1b · Agent reported, awaiting acknowledgement", server: { observed: { env: true, flake: true, builder: true, cache: true, system: true }, agent: "report_received" }, ui: { track: "setup", panel: "expanded" }, view: "systems" },
  { id: "setup-complete",    label: "2 · Setup complete → Explore",         server: { observed: SETUP_ALL, agent: "acknowledged" }, ui: { track: "setup", panel: "expanded" }, view: "dashboard" },
  { id: "security-home",     label: "3 · Security workflow home",           server: { observed: SETUP_ALL, agent: "acknowledged" }, ui: { track: "security", panel: "expanded" }, view: "dashboard", progress: { A: ["A1", "A2", "A3", "A4", "A5", "A6"], B: ["B1", "B2"] } },
  { id: "tour-scanning",     label: "4 · Scanning walkthrough",             tour: ["A", 0] },
  { id: "tour-cve-relations",label: "5 · Fleet CVE evidence relations",     tour: ["A", 4] },
  { id: "tour-cve-triage",   label: "6 · Single CVE triage",                tour: ["B", 1] },
  { id: "tour-batch",        label: "7 · Batch triage",                     tour: ["B", 4] },
  { id: "tour-policies",     label: "8 · Policies / Security controls",     tour: ["C", 0] },
  { id: "tour-matrix",       label: "9 · Bundle system matrix",             tour: ["C", 4] },
  { id: "tour-evidence",     label: "10 · Per-control evidence",            tour: ["C", 5] },
  { id: "tour-register",     label: "11 · POA&M register",                  tour: ["D", 1] },
  { id: "tour-verify",       label: "12 · POA&M verification / closure",    tour: ["D", 5] },
  { id: "tour-ra",           label: "13 · Risk acceptance",                 tour: ["D", 7] },
  { id: "tour-export",       label: "14 · Audit / export",                  tour: ["E", 1] },
  { id: "viewer-readonly",   label: "15 · Viewer read-only walkthrough",    tour: ["B", 1], role: "viewer" },
  { id: "operator-home",     label: "15b · Operator security home",         role: "operator", ui: { track: "security", panel: "expanded" }, view: "cves" },
  { id: "tour-minimized",    label: "16 · Minimized walkthrough",           tour: ["A", 1], ui: { panel: "minimized" } },
  { id: "tour-mobile",       label: "17 · Narrow / mobile walkthrough",     tour: ["B", 0], ui: { forceSheet: true } },
  { id: "tour-light",        label: "Light · Enforce vs Report only",       tour: ["C", 3], theme: "light" },
  { id: "tour-dark-ra",      label: "Dark · RA renew / convert",            tour: ["D", 8], theme: "dark" },
];
window.COACH_DEMO_STATES = COACH_DEMO_STATES;
window.SETUP_ALL = SETUP_ALL;
