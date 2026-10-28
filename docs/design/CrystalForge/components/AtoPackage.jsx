// Compliance evidence package — scope-first export for an audit or authorization package.
//
// The bundle-scoped ExportEvidenceModal answers "give me this benchmark's results".
// This answers the other question, the one a security lead actually starts from:
// "everything an auditor or authorizing official needs for THIS environment (or these
// hosts), in the formats they accept." Scope → contents → formats → generated files.
//
// Framework-neutral by design: the same package serves an RMF ATO renewal, a SOC 2
// evidence request, an ISO 27001 surveillance audit, or an internal control review.
// Nothing here is authoritative on its own: every artifact carries the evaluation
// ids, attestation digests, and POA&M state it was derived from, and the readiness
// panel names what is missing rather than quietly omitting it.

const ATO_OSCAL_VERSION = "1.1.2";

function atoHash(str) {
  let h = 0x811c9dc5;
  for (let i = 0; i < str.length; i++) { h ^= str.charCodeAt(i); h = (h * 0x01000193) >>> 0; }
  return h.toString(16).padStart(8, "0");
}
function atoUuid(seed) {
  const a = atoHash(seed), b = atoHash(seed + "::b"), c = atoHash(seed + "::c"), d = atoHash(seed + "::d");
  return `${a}-${b.slice(0,4)}-4${b.slice(4,7)}-a${c.slice(0,3)}-${c.slice(3,8)}${d.slice(0,7)}`;
}
function atoNow() { return new Date().toISOString(); }
function atoDigest(content) { return `sha256:${atoHash(content)}${atoHash(content + "#2")}`; }
function atoSize(content) {
  const b = new Blob([content]).size;
  return b < 1024 ? `${b} B` : b < 1048576 ? `${(b/1024).toFixed(1)} KB` : `${(b/1048576).toFixed(2)} MB`;
}
// "AC-17(1)" → "ac-17.1" — OSCAL control ids are lowercase dotted.
function atoOscalControlId(externalId) {
  return String(externalId || "").toLowerCase().replace(/\((\w+)\)/g, ".$1").replace(/\s+/g, "");
}
function atoStatusToOscal(status) {
  return status === "pass" ? "satisfied" : status === "waiver" ? "satisfied" : status === "warn" ? "satisfied" : "not-satisfied";
}

/* ── Collection ─────────────────────────────────────────────────────────────── */

function atoScopeSystems(scope) {
  const all = typeof SYSTEMS !== "undefined" ? SYSTEMS : [];
  if (scope.mode === "systems") return all.filter(s => scope.sysIds.includes(s.id));
  return all.filter(s => s.environment === scope.env);
}

function atoCollect(scope) {
  const systems = atoScopeSystems(scope);
  const sysIds = new Set(systems.map(s => s.id));
  const allBundles = typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : [];
  const bundles = allBundles.filter(b => b.publicationState !== "deprecated"
    && systems.some(s => bundleStatusForSystem(b, s).applies));

  const results = [];
  systems.forEach(s => bundles.forEach(b => {
    const rollup = bundleStatusForSystem(b, s);
    if (!rollup.applies) return;
    (b.policyIds || []).forEach(pid => {
      const ev = evidenceForControl(b, pid, s);
      const managedBy = poamForFinding(s.id, pid);
      results.push({
        sysId: s.id, hostname: s.hostname, env: s.environment, commit: s.commit,
        bundleId: b.id, bundleName: b.name, framework: b.framework, bundleVersion: b.version,
        policyId: pid, policyName: ev.policyName, status: ev.status, severity: ev.severity,
        summary: ev.summary, items: ev.items,
        assignmentState: rollup.state || null,
        poamId: managedBy ? managedBy.id : null,
        poamStatus: managedBy ? managedBy.status : null,
        poamDue: managedBy ? managedBy.due : null,
      });
    });
  }));

  const poams = (typeof POAMS !== "undefined" ? POAMS : [])
    .filter(p => p.findings.some(f => sysIds.has(f.sysId)) || (p.cveRefs || []).some(c => sysIds.has(c.sysId)));
  const cves = (typeof CVES !== "undefined" ? CVES : [])
    .filter(c => (c.affected || []).some(id => sysIds.has(id)));
  const attestations = (typeof ATTESTATION_RECORDS !== "undefined" ? ATTESTATION_RECORDS : [])
    .filter(a => sysIds.has(a.system_id));

  const counts = { pass:0, warn:0, fail:0, waiver:0 };
  results.forEach(r => { counts[r.status] = (counts[r.status] || 0) + 1; });
  const unmanaged = results.filter(r => r.status === "fail" && !r.poamId);
  const overdue = poams.filter(p => poamIsOverdue(p));
  const unmappedPolicies = [...new Set(results.map(r => r.policyId))]
    .filter(pid => typeof mappingsForPolicy === "function" && mappingsForPolicy(pid).length === 0);
  const staleAttestations = attestations.filter(a =>
    a.classification === "agent_attestation_stale" || a.classification === "agent_identity_invalid"
    || a.classification === "unauthorized_artifact" || a.classification === "unknown_artifact");

  return {
    scope, systems, bundles, results, poams, cves, attestations,
    counts,
    score: results.length ? Math.round(((counts.pass + counts.waiver) / results.length) * 100) : 0,
    unmanaged, overdue, unmappedPolicies, staleAttestations,
    generatedAt: atoNow(),
  };
}

function atoScopeLabel(scope) {
  return scope.mode === "systems" ? `${scope.sysIds.length} selected system${scope.sysIds.length === 1 ? "" : "s"}` : scope.env;
}
function atoSlug(scope) {
  return scope.mode === "systems" ? `${scope.sysIds.length}-systems` : (slugify(scope.env) || "scope");
}

/* ── OSCAL ──────────────────────────────────────────────────────────────────── */

function atoOscalMetadata(data, title) {
  return {
    title,
    "last-modified": data.generatedAt,
    version: new Date(data.generatedAt).toISOString().slice(0,10),
    "oscal-version": ATO_OSCAL_VERSION,
    roles: [
      { id:"authorizing-official", title:"Authorizing Official" },
      { id:"system-owner", title:"System Owner" },
      { id:"tool-operator", title:"Assessment Tooling", description:"Crystal Forge — configuration authority and evidence collection." },
    ],
    parties: [{ uuid: atoUuid("party::crystal-forge"), type:"organization", name:"Crystal Forge", remarks:"Automated evaluation and evidence collection platform." }],
    props: [
      { name:"scope", ns:"https://crystalforge.dev/ns/oscal", value: atoScopeLabel(data.scope) },
      { name:"host-count", ns:"https://crystalforge.dev/ns/oscal", value: String(data.systems.length) },
    ],
  };
}

// Every control evaluation in scope, grouped by the framework control it maps to.
function atoControlGroups(data) {
  const byControl = new Map();
  data.results.forEach(r => {
    const maps = typeof mappingsForPolicy === "function" ? mappingsForPolicy(r.policyId) : [];
    const reqs = maps.map(m => (typeof reqById === "function" ? reqById(m.requirementId) : null)).filter(Boolean);
    const keys = reqs.length ? reqs.map(q => ({ id: atoOscalControlId(q.externalId), title: q.title, framework: q.frameworkId }))
                             : [{ id: `cf-${r.policyId}`, title: r.policyName, framework: "crystal-forge-local" }];
    keys.forEach(k => {
      if (!byControl.has(k.id)) byControl.set(k.id, { ...k, results: [] });
      byControl.get(k.id).results.push(r);
    });
  });
  return [...byControl.values()].sort((a,b) => a.id.localeCompare(b.id));
}

function atoOscalSsp(data, opts) {
  const groups = atoControlGroups(data);
  const doc = {
    "system-security-plan": {
      uuid: atoUuid("ssp::" + atoScopeLabel(data.scope)),
      metadata: atoOscalMetadata(data, `System Security Plan — ${atoScopeLabel(data.scope)}`),
      "import-profile": { href: "#crystal-forge-derived-profile",
        remarks: "Controls are derived from the compliance bundles assigned to the hosts in scope, resolved through Crystal Forge policy→requirement mappings." },
      "system-characteristics": {
        "system-ids": [{ id: atoSlug(data.scope), "identifier-type": "https://crystalforge.dev/ns/environment" }],
        "system-name": atoScopeLabel(data.scope),
        description: `Hosts under Crystal Forge configuration authority in ${atoScopeLabel(data.scope)}. Configuration is declarative; each host's running artifact is reconciled against its deployment authorization by signed agent attestation.`,
        "security-sensitivity-level": "moderate",
        "system-information": { "information-types": [{ uuid: atoUuid("infotype::" + atoSlug(data.scope)), title:"Operational system configuration and audit records", description:"Configuration state, audit logs, and evaluation evidence for the hosts in scope." }] },
        status: { state: "operational" },
        "authorization-boundary": { description: `${data.systems.length} hosts in ${atoScopeLabel(data.scope)}, built from pinned flake revisions and deployed through Crystal Forge with per-environment approval policy.` },
      },
      "system-implementation": {
        description: "Each host is a component instance of its flake-defined system closure.",
        components: data.bundles.map(b => ({
          uuid: atoUuid("component::" + b.id),
          type: "validation",
          title: b.name,
          description: b.description || `${b.framework} ${b.version} compliance bundle.`,
          props: [
            { name:"framework", ns:"https://crystalforge.dev/ns/oscal", value: b.framework },
            { name:"bundle-digest", ns:"https://crystalforge.dev/ns/oscal", value: b.digest || "unknown" },
            { name:"publication-state", ns:"https://crystalforge.dev/ns/oscal", value: b.publicationState },
          ],
          status: { state: "operational" },
        })),
        "inventory-items": data.systems.map(s => {
          const att = data.attestations.find(a => a.system_id === s.id);
          return {
            uuid: atoUuid("inventory::" + s.id),
            description: `${s.hostname} (${s.environment})`,
            props: [
              { name:"asset-id", value: s.hostname },
              { name:"flake", ns:"https://crystalforge.dev/ns/oscal", value: s.flake || "unknown" },
              { name:"commit", ns:"https://crystalforge.dev/ns/oscal", value: s.commit || "unknown" },
              ...(opts.attestations && att ? [
                { name:"store-path", ns:"https://crystalforge.dev/ns/oscal", value: att.attestation.current_system_store_path },
                { name:"attestation-classification", ns:"https://crystalforge.dev/ns/oscal", value: att.classification },
                { name:"attestation-observed-at", ns:"https://crystalforge.dev/ns/oscal", value: att.attestation.observed_at },
              ] : []),
            ],
          };
        }),
      },
      "control-implementation": {
        description: "Implementation is the deployed NixOS module set; each statement cites the evaluation that produced it.",
        "implemented-requirements": groups.map(g => ({
          uuid: atoUuid("ir::" + g.id),
          "control-id": g.id,
          props: [{ name:"control-title", ns:"https://crystalforge.dev/ns/oscal", value: g.title }],
          statements: [{
            "statement-id": `${g.id}_smt`,
            uuid: atoUuid("stmt::" + g.id),
            description: [...new Set(g.results.map(r => r.policyName))].join("; "),
            "by-components": [...new Set(g.results.map(r => r.bundleId))].map(bid => {
              const rs = g.results.filter(r => r.bundleId === bid);
              const failing = rs.filter(r => r.status === "fail");
              return {
                "component-uuid": atoUuid("component::" + bid),
                uuid: atoUuid("bycomp::" + g.id + "::" + bid),
                description: `${rs.length - failing.length} of ${rs.length} host evaluations satisfied.`,
                "implementation-status": { state: failing.length ? "partial" : "implemented" },
                ...(opts.configEvidence ? { remarks: rs.map(r => {
                  const cfg = (r.items || []).find(i => i.type === "config");
                  return cfg ? `${r.hostname}: ${cfg.ref} (${cfg.hash})` : `${r.hostname}: no rendered config collected`;
                }).join("\n") } : {}),
              };
            }),
          }],
        })),
      },
      ...(opts.poams ? { "back-matter": { resources: [{ uuid: atoUuid("res::poam"), title:"Plan of Action and Milestones",
        description:`${data.poams.filter(p=>p.status!=="completed").length} open items`, rlinks:[{ href:`./poam-${atoSlug(data.scope)}.oscal.json`, "media-type":"application/json" }] }] } } : {}),
    },
  };
  return JSON.stringify(doc, null, 2);
}

function atoOscalSar(data, opts) {
  const groups = atoControlGroups(data);
  const observations = [];
  const findings = [];
  data.results.forEach(r => {
    const obsUuid = atoUuid(`obs::${r.sysId}::${r.bundleId}::${r.policyId}`);
    observations.push({
      uuid: obsUuid,
      title: `${r.hostname} — ${r.policyName}`,
      description: r.summary,
      methods: ["TEST"],
      types: ["control-objective"],
      "subjects": [{ "subject-uuid": atoUuid("inventory::" + r.sysId), type:"inventory-item" }],
      ...(opts.configEvidence && (r.items || []).length ? {
        "relevant-evidence": r.items.filter(i => i.type === "config" || i.type === "audit_log").map(i => ({
          href: `#${i.ref}`, description: `${i.type} via ${i.source}${i.hash ? ` (${i.hash})` : ""}`,
        })),
      } : {}),
      collected: data.generatedAt,
      props: [
        { name:"result", ns:"https://crystalforge.dev/ns/oscal", value: r.status },
        { name:"bundle", ns:"https://crystalforge.dev/ns/oscal", value: r.bundleId },
        ...(r.status === "waiver" && opts.waivers ? [{ name:"waiver", ns:"https://crystalforge.dev/ns/oscal", value:"risk accepted — see relevant evidence" }] : []),
      ],
    });
  });
  groups.forEach(g => {
    const failing = g.results.filter(r => r.status === "fail");
    if (!failing.length) return;
    findings.push({
      uuid: atoUuid("finding::" + g.id),
      title: `${g.id.toUpperCase()} — not satisfied on ${failing.length} host${failing.length===1?"":"s"}`,
      description: [...new Set(failing.map(r => `${r.hostname}: ${r.policyName}`))].join("; "),
      target: {
        type: "objective-id",
        "target-id": g.id,
        description: g.title,
        status: { state: atoStatusToOscal("fail"), reason: "failed" },
      },
      "related-observations": failing.map(r => ({ "observation-uuid": atoUuid(`obs::${r.sysId}::${r.bundleId}::${r.policyId}`) })),
      ...(opts.poams ? { "related-risks": [...new Set(failing.map(r => r.poamId).filter(Boolean))].map(id => ({ "risk-uuid": atoUuid("risk::" + id) })) } : {}),
    });
  });
  const doc = {
    "assessment-results": {
      uuid: atoUuid("sar::" + atoScopeLabel(data.scope)),
      metadata: atoOscalMetadata(data, `Security Assessment Results — ${atoScopeLabel(data.scope)}`),
      "import-ap": { href: `./ssp-${atoSlug(data.scope)}.oscal.json`, remarks:"Assessment plan is implicit: continuous automated evaluation of assigned compliance bundles." },
      results: [{
        uuid: atoUuid("result::" + atoScopeLabel(data.scope)),
        title: `Continuous evaluation snapshot — ${atoScopeLabel(data.scope)}`,
        description: `${data.results.length} control evaluations across ${data.systems.length} hosts and ${data.bundles.length} bundles. Compliance score ${data.score}%.`,
        start: data.generatedAt,
        end: data.generatedAt,
        "reviewed-controls": {
          "control-selections": [{
            description: "Controls resolved from the compliance bundles assigned to the hosts in scope.",
            "include-controls": groups.map(g => ({ "control-id": g.id })),
          }],
        },
        "assessment-log": { entries: [{
          uuid: atoUuid("log::" + data.generatedAt),
          title: "Evidence collected by Crystal Forge",
          start: data.generatedAt,
          description: "Evaluation results, rendered configuration, audit records, and signed running-state attestations collected without host-side manual steps.",
        }] },
        observations,
        findings,
        ...(opts.attestations ? { "local-definitions": { "assessment-assets": { components: data.attestations.map(a => ({
          uuid: atoUuid("attest::" + a.system_id),
          type: "software",
          title: `${a.hostname} running artifact`,
          description: a.attestation.current_system_store_path,
          props: [
            { name:"classification", ns:"https://crystalforge.dev/ns/oscal", value: a.classification },
            { name:"nar-hash", ns:"https://crystalforge.dev/ns/oscal", value: a.attestation.current_system_nar_hash },
            { name:"agent-signature", ns:"https://crystalforge.dev/ns/oscal", value: a.attestation.agent_signature },
            { name:"booted-generation", ns:"https://crystalforge.dev/ns/oscal", value: String(a.attestation.booted_generation) },
          ],
          status: { state: "operational" },
        })) } } } : {}),
      }],
    },
  };
  return JSON.stringify(doc, null, 2);
}

function atoOscalPoam(data) {
  const risks = data.poams.map(p => ({
    uuid: atoUuid("risk::" + p.id),
    title: p.title,
    description: p.plan || "No remediation plan recorded.",
    statement: `Deficiency tracked as ${p.id}. Severity ${poamSeverityLabel(p.severity)}.`,
    status: p.status === "completed" ? "closed" : "open",
    "risk-log": { entries: (p.activity || []).map((a, i) => ({
      uuid: atoUuid(`risklog::${p.id}::${i}`), title: a.text, start: a.at, "logged-by": [{ "party-uuid": atoUuid("party::crystal-forge") }],
    })) },
    deadline: p.due || undefined,
  }));
  const doc = {
    "plan-of-action-and-milestones": {
      uuid: atoUuid("poam::" + atoScopeLabel(data.scope)),
      metadata: atoOscalMetadata(data, `Plan of Action and Milestones — ${atoScopeLabel(data.scope)}`),
      "import-ssp": { href: `./ssp-${atoSlug(data.scope)}.oscal.json` },
      "system-id": { id: atoSlug(data.scope), "identifier-type": "https://crystalforge.dev/ns/environment" },
      observations: [
        ...data.poams.flatMap(p => p.findings.filter(f => data.systems.some(s => s.id === f.sysId)).map(f => ({
          uuid: atoUuid(`poamobs::${p.id}::${f.sysId}::${f.policyId}`),
          title: poamFindingLabel(f),
          description: `Finding managed by ${p.id}. Live evaluation status: ${poamFindingStatus(f)}.`,
          methods: ["TEST"],
          collected: data.generatedAt,
        }))),
        ...data.poams.flatMap(p => (p.cveRefs || []).filter(r => data.systems.some(s => s.id === r.sysId)).map(r => ({
          uuid: atoUuid(`poamcve::${p.id}::${r.sysId}::${r.id}`),
          title: `${r.id} — ${r.hostname}`,
          description: `Vulnerability ${r.id} in package ${r.pkg} on ${r.hostname}, remediation managed by ${p.id}.`,
          methods: ["TEST"],
          types: ["control-objective"],
          subjects: [{ "subject-uuid": atoUuid("inventory::" + r.sysId), type:"inventory-item" }],
          collected: data.generatedAt,
          props: [{ name:"cve", ns:"https://crystalforge.dev/ns/oscal", value: r.id }],
        }))),
      ],
      risks,
      "poam-items": data.poams.map(p => ({
        uuid: atoUuid("poamitem::" + p.id),
        title: `${p.id} — ${p.title}`,
        description: p.plan || "No remediation plan recorded.",
        props: [
          { name:"poam-id", ns:"https://crystalforge.dev/ns/oscal", value: p.id },
          { name:"status", ns:"https://crystalforge.dev/ns/oscal", value: p.status },
          { name:"severity", ns:"https://crystalforge.dev/ns/oscal", value: poamSeverityLabel(p.severity) },
          { name:"owner", ns:"https://crystalforge.dev/ns/oscal", value: p.owner || "unassigned" },
          { name:"scheduled-completion-date", ns:"https://crystalforge.dev/ns/oscal", value: p.due || "none" },
          ...(poamIsOverdue(p) ? [{ name:"overdue", ns:"https://crystalforge.dev/ns/oscal", value:"true" }] : []),
        ],
        "related-observations": [
          ...p.findings.filter(f => data.systems.some(s => s.id === f.sysId))
            .map(f => ({ "observation-uuid": atoUuid(`poamobs::${p.id}::${f.sysId}::${f.policyId}`) })),
          ...(p.cveRefs || []).filter(r => data.systems.some(s => s.id === r.sysId))
            .map(r => ({ "observation-uuid": atoUuid(`poamcve::${p.id}::${r.sysId}::${r.id}`) })),
        ],
        "related-risks": [{ "risk-uuid": atoUuid("risk::" + p.id) }],
        remarks: (p.milestones || []).map(m => `[${m.done ? "x" : " "}] ${m.text} (due ${m.due || "—"}${m.doneAt ? `, completed ${m.doneAt}` : ""})`).join("\n") || undefined,
      })),
    },
  };
  return JSON.stringify(doc, null, 2);
}

/* ── Native, CSV, XCCDF results, printable report ───────────────────────────── */

function atoCfJson(data, opts) {
  return JSON.stringify({
    schema: "crystal-forge/compliance-evidence@1",
    generatedAt: data.generatedAt,
    scope: { ...data.scope, label: atoScopeLabel(data.scope) },
    summary: { hosts: data.systems.length, bundles: data.bundles.length, evaluations: data.results.length, score: data.score, ...data.counts },
    readiness: {
      unmanagedFailingFindings: data.unmanaged.length,
      overduePoams: data.overdue.length,
      unmappedPolicies: data.unmappedPolicies,
      attestationExceptions: data.staleAttestations.length,
    },
    bundles: data.bundles.map(b => ({ id:b.id, name:b.name, framework:b.framework, version:b.version, digest:b.digest, publicationState:b.publicationState, controls:b.policyIds })),
    hosts: data.systems.map(s => ({ id:s.id, hostname:s.hostname, environment:s.environment, flake:s.flake, commit:s.commit })),
    evaluations: data.results.map(r => ({
      host: r.hostname, bundle: r.bundleId, control: r.policyId, controlName: r.policyName,
      status: r.status, severity: r.severity, poam: r.poamId, poamStatus: r.poamStatus,
      ...(opts.configEvidence ? { evidence: (r.items || []).map(i => ({ type:i.type, source:i.source, ref:i.ref, hash:i.hash, value:i.value })) } : {}),
    })),
    ...(opts.poams ? { poams: data.poams.map(p => ({ id:p.id, title:p.title, status:p.status, severity:p.severity, owner:p.owner, due:p.due, opened:p.opened, closed:p.closed || null, plan:p.plan, findings:p.findings, cveRefs:p.cveRefs || [], milestones:p.milestones, overdue: poamIsOverdue(p) })) } : {}),
    ...(opts.cves ? { vulnerabilities: data.cves.map(c => ({ id:c.id, pkg:c.pkg, severity:c.severity, cvss:c.cvss, fix:c.fix, acceptance:c.acceptance, justification:c.justification, affectedInScope: c.affected.filter(id => data.systems.some(s => s.id === id)).length })) } : {}),
    ...(opts.attestations ? { attestations: data.attestations.map(a => ({ host:a.hostname, classification:a.classification, storePath:a.attestation.current_system_store_path, narHash:a.attestation.current_system_nar_hash, observedAt:a.attestation.observed_at, bootedGeneration:a.attestation.booted_generation, signature:a.attestation.agent_signature })) } : {}),
  }, null, 2);
}

function atoCsv(data) {
  const esc = (v) => { const s = String(v ?? ""); return /[",\n]/.test(s) ? `"${s.replace(/"/g,'""')}"` : s; };
  const rows = [["host","environment","framework","bundle","bundle_version","control_id","control_name","status","severity","poam_id","poam_status","poam_due","commit"]];
  data.results.forEach(r => rows.push([r.hostname, r.env, r.framework, r.bundleName, r.bundleVersion, r.policyId, r.policyName, r.status, r.severity, r.poamId || "", r.poamStatus || "", r.poamDue || "", r.commit || ""]));
  return rows.map(cols => cols.map(esc).join(",")).join("\n");
}

function atoXccdfResults(data) {
  const esc = (s) => String(s ?? "").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;");
  const perHost = data.systems.map(s => {
    const rs = data.results.filter(r => r.sysId === s.id);
    const att = data.attestations.find(a => a.system_id === s.id);
    return [
      `  <cdf:TestResult id="cf-result-${esc(s.id)}" start-time="${esc(data.generatedAt)}" end-time="${esc(data.generatedAt)}">`,
      `    <cdf:title>${esc(s.hostname)} — ${esc(atoScopeLabel(data.scope))}</cdf:title>`,
      `    <cdf:target>${esc(s.hostname)}</cdf:target>`,
      `    <cdf:target-facts>`,
      `      <cdf:fact name="urn:crystalforge:fact:environment" type="string">${esc(s.environment)}</cdf:fact>`,
      `      <cdf:fact name="urn:crystalforge:fact:commit" type="string">${esc(s.commit || "unknown")}</cdf:fact>`,
      att ? `      <cdf:fact name="urn:crystalforge:fact:store-path" type="string">${esc(att.attestation.current_system_store_path)}</cdf:fact>` : "",
      `    </cdf:target-facts>`,
      ...rs.map(r => [
        `    <cdf:rule-result idref="${esc(r.policyId)}" severity="${esc(r.severity)}" time="${esc(data.generatedAt)}">`,
        `      <cdf:result>${r.status === "pass" ? "pass" : r.status === "warn" ? "informational" : r.status === "waiver" ? "notapplicable" : "fail"}</cdf:result>`,
        `      <cdf:message severity="info">${esc(r.summary)}</cdf:message>`,
        r.poamId ? `      <cdf:metadata><cf:poam xmlns:cf="urn:crystalforge" id="${esc(r.poamId)}" status="${esc(r.poamStatus)}" due="${esc(r.poamDue || "")}"/></cdf:metadata>` : "",
        `    </cdf:rule-result>`,
      ].filter(Boolean).join("\n")),
      `  </cdf:TestResult>`,
    ].filter(Boolean).join("\n");
  });
  return [
    `<?xml version="1.0" encoding="UTF-8"?>`,
    `<!-- Crystal Forge XCCDF results — ${esc(atoScopeLabel(data.scope))} — generated ${esc(data.generatedAt)} -->`,
    `<cdf:Benchmark xmlns:cdf="http://checklists.nist.gov/xccdf/1.2" id="cf-evidence-${esc(atoSlug(data.scope))}">`,
    `  <cdf:status date="${esc(data.generatedAt.slice(0,10))}">accepted</cdf:status>`,
    `  <cdf:title>Compliance evidence — ${esc(atoScopeLabel(data.scope))}</cdf:title>`,
    `  <cdf:description>Aggregated results for ${data.systems.length} hosts across ${data.bundles.length} compliance bundles.</cdf:description>`,
    ...perHost,
    `</cdf:Benchmark>`,
  ].join("\n");
}

function atoReportHtml(data, opts) {
  const esc = (s) => String(s ?? "").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;");
  const pct = (n) => data.results.length ? Math.round(n / data.results.length * 100) : 0;
  const hostRows = data.systems.map(s => {
    const rs = data.results.filter(r => r.sysId === s.id);
    const fail = rs.filter(r => r.status === "fail").length;
    const att = data.attestations.find(a => a.system_id === s.id);
    return `<tr><td class="m">${esc(s.hostname)}</td><td>${esc(s.environment)}</td><td class="m">${esc(s.commit||"—")}</td><td class="n">${rs.length}</td><td class="n">${rs.length-fail}</td><td class="n ${fail?"bad":""}">${fail}</td><td>${esc(att ? (ATTESTATION_CLASSIFICATIONS[att.classification]?.label || att.classification) : "no attestation")}</td></tr>`;
  }).join("");
  const failRows = data.results.filter(r => r.status === "fail").map(r =>
    `<tr><td class="m">${esc(r.hostname)}</td><td>${esc(r.framework)}</td><td class="m">${esc(r.policyId)}</td><td>${esc(r.policyName)}</td><td>${esc(poamSeverityLabel(r.severity))}</td><td class="m">${r.poamId ? esc(r.poamId) : '<span class="bad">unmanaged</span>'}</td><td>${esc(r.poamDue || "—")}</td></tr>`).join("");
  const poamRows = opts.poams ? data.poams.map(p =>
    `<tr><td class="m">${esc(p.id)}</td><td>${esc(p.title)}</td><td>${esc(POAM_STATUS[p.status]?.label || p.status)}</td><td>${esc(poamSeverityLabel(p.severity))}</td><td>${esc(p.owner||"—")}</td><td class="${poamIsOverdue(p)?"bad":""}">${esc(p.due||"—")}</td><td class="n">${poamMilestoneProgress(p).done}/${poamMilestoneProgress(p).total}</td></tr>`).join("") : "";
  const cveRows = opts.cves ? data.cves.slice(0, 60).map(c =>
    `<tr><td class="m">${esc(c.id)}</td><td class="m">${esc(c.pkg)}</td><td>${esc(c.severity)}</td><td class="n">${c.cvss}</td><td>${esc(c.fix)}</td><td class="n">${c.affected.filter(id => data.systems.some(s=>s.id===id)).length}</td><td>${esc(c.acceptance)}</td></tr>`).join("") : "";
  return `<!doctype html><html><head><meta charset="utf-8"><title>Compliance evidence — ${esc(atoScopeLabel(data.scope))}</title><style>
@page{size:letter;margin:0.7in}
*{box-sizing:border-box}body{font:11pt/1.45 "Helvetica Neue",Arial,sans-serif;color:#111;margin:0}
h1{font-size:24pt;margin:0 0 6px}h2{font-size:13pt;margin:26px 0 8px;padding-bottom:4px;border-bottom:1px solid #ccc}
.sub{color:#555;font-size:10pt;margin-bottom:22px}
.cover{border:1px solid #ccc;padding:18px;margin-bottom:8px;display:grid;grid-template-columns:repeat(4,1fr);gap:14px}
.cover div span{display:block;font-size:8pt;text-transform:uppercase;letter-spacing:.08em;color:#666}
.cover div b{font-size:15pt}
table{width:100%;border-collapse:collapse;font-size:9pt;margin-bottom:6px}
th{text-align:left;border-bottom:1.5px solid #333;padding:5px 6px;font-size:8pt;text-transform:uppercase;letter-spacing:.05em}
td{border-bottom:1px solid #e3e3e3;padding:5px 6px;vertical-align:top}
.m{font-family:"SFMono-Regular",Menlo,monospace;font-size:8.5pt}.n{text-align:right}.bad{color:#b91c1c;font-weight:600}
.note{font-size:9pt;color:#555;border-left:3px solid #999;padding:6px 10px;margin:10px 0}
tr{break-inside:avoid}h2{break-after:avoid}
</style></head><body>
<h1>Compliance evidence package</h1>
<div class="sub">${esc(atoScopeLabel(data.scope))} · generated ${esc(data.generatedAt)} · Crystal Forge</div>
<div class="cover">
<div><span>Hosts</span><b>${data.systems.length}</b></div>
<div><span>Control evaluations</span><b>${data.results.length}</b></div>
<div><span>Compliance score</span><b>${data.score}%</b></div>
<div><span>Open POA&amp;M</span><b>${data.poams.filter(p=>p.status!=="completed").length}</b></div>
<div><span>Satisfied</span><b>${data.counts.pass} <small>(${pct(data.counts.pass)}%)</small></b></div>
<div><span>Warnings</span><b>${data.counts.warn}</b></div>
<div><span>Failing</span><b>${data.counts.fail}</b></div>
<div><span>Waived</span><b>${data.counts.waiver}</b></div>
</div>
<div class="note">Evaluation results are produced by the Crystal Forge policy evaluator against the declarative configuration deployed to each host. A failing control with an open POA&amp;M is still a failing control; the POA&amp;M records what is being done about it.${data.unmanaged.length ? ` <strong>${data.unmanaged.length} failing finding${data.unmanaged.length===1?"":"s"} have no remediation plan.</strong>` : ""}</div>
<h2>Frameworks and bundles in scope</h2>
<table><thead><tr><th>Bundle</th><th>Framework</th><th>Version</th><th>State</th><th>Digest</th><th class="n">Controls</th></tr></thead><tbody>
${data.bundles.map(b=>`<tr><td>${esc(b.name)}</td><td>${esc(b.framework)}</td><td class="m">${esc(b.version)}</td><td>${esc(b.publicationState)}</td><td class="m">${esc(b.digest||"—")}</td><td class="n">${(b.policyIds||[]).length}</td></tr>`).join("")}
</tbody></table>
<h2>Host inventory and posture</h2>
<table><thead><tr><th>Host</th><th>Environment</th><th>Commit</th><th class="n">Evals</th><th class="n">Satisfied</th><th class="n">Failing</th><th>Running-state attestation</th></tr></thead><tbody>${hostRows}</tbody></table>
${failRows ? `<h2>Open deficiencies</h2><table><thead><tr><th>Host</th><th>Framework</th><th>Control</th><th>Requirement</th><th>Severity</th><th>POA&amp;M</th><th>Due</th></tr></thead><tbody>${failRows}</tbody></table>` : `<h2>Open deficiencies</h2><p style="font-size:10pt">No failing control evaluations in scope.</p>`}
${poamRows ? `<h2>Plan of action and milestones</h2><table><thead><tr><th>ID</th><th>Title</th><th>Status</th><th>Severity</th><th>Owner</th><th>Due</th><th class="n">Milestones</th></tr></thead><tbody>${poamRows}</tbody></table>` : ""}
${cveRows ? `<h2>Vulnerability summary</h2><table><thead><tr><th>CVE</th><th>Package</th><th>Severity</th><th class="n">CVSS</th><th>Fix</th><th class="n">Hosts</th><th>Disposition</th></tr></thead><tbody>${cveRows}</tbody></table>${data.cves.length>60?`<p style="font-size:9pt;color:#555">Showing 60 of ${data.cves.length} vulnerabilities affecting hosts in scope; the machine-readable exports carry the full set.</p>`:""}` : ""}
</body></html>`;
}

/* ── Artifact assembly ──────────────────────────────────────────────────────── */

const ATO_FORMATS = {
  oscal: { label:"OSCAL 1.1.2", note:"SSP + Assessment Results + POA&M — the NIST exchange format federal packages are submitted in.", files:3 },
  xccdf: { label:"XCCDF 1.2 results", note:"Per-host rule results. Feeds SCAP viewers and most posture tooling.", files:1 },
  csv:   { label:"CSV summary", note:"One row per host-control. For spreadsheets, auditor evidence requests, and control trackers.", files:1 },
  cfjson:{ label:"Crystal Forge JSON", note:"Native schema — everything collected, including evidence references.", files:1 },
  report:{ label:"Printable report", note:"Cover sheet, host posture, deficiencies, remediation plans. Opens a print view for PDF.", files:1 },
};

function atoBuildArtifacts(data, formats, opts) {
  const slug = atoSlug(data.scope);
  const out = [];
  const push = (name, content, mime, kind) => out.push({ name, content, mime, kind, size: atoSize(content), digest: atoDigest(content) });
  if (formats.oscal) {
    push(`ssp-${slug}.oscal.json`, atoOscalSsp(data, opts), "application/json", "OSCAL system-security-plan");
    push(`sar-${slug}.oscal.json`, atoOscalSar(data, opts), "application/json", "OSCAL assessment-results");
    if (opts.poams) push(`poam-${slug}.oscal.json`, atoOscalPoam(data), "application/json", "OSCAL plan-of-action-and-milestones");
  }
  if (formats.xccdf) push(`results-${slug}.xccdf.xml`, atoXccdfResults(data), "application/xml", "XCCDF 1.2 results");
  if (formats.csv) push(`summary-${slug}.csv`, atoCsv(data), "text/csv", "Control summary");
  if (formats.cfjson) push(`evidence-${slug}.cf.json`, atoCfJson(data, opts), "application/json", "Crystal Forge evidence");
  if (formats.report) push(`report-${slug}.html`, atoReportHtml(data, opts), "text/html", "Printable report");
  const manifest = JSON.stringify({
    schema: "crystal-forge/evidence-manifest@1",
    generatedAt: data.generatedAt,
    scope: { ...data.scope, label: atoScopeLabel(data.scope) },
    hosts: data.systems.map(s => s.hostname),
    bundles: data.bundles.map(b => ({ id:b.id, version:b.version, digest:b.digest })),
    contents: opts,
    readiness: {
      unmanagedFailingFindings: data.unmanaged.length,
      overduePoams: data.overdue.map(p => p.id),
      unmappedPolicies: data.unmappedPolicies,
      attestationExceptions: data.staleAttestations.map(a => ({ host:a.hostname, classification:a.classification })),
    },
    files: out.map(f => ({ name:f.name, kind:f.kind, digest:f.digest })),
  }, null, 2);
  out.push({ name:`manifest-${slug}.json`, content:manifest, mime:"application/json", kind:"Package manifest", size: atoSize(manifest), digest: atoDigest(manifest) });
  return out;
}

function atoDownload(file) {
  downloadFile(file.name, file.content, file.mime);
}
function atoPrintReport(file) {
  const w = window.open("", "_blank");
  if (!w) { atoDownload(file); return false; }
  w.document.write(file.content);
  w.document.close();
  setTimeout(() => { try { w.focus(); w.print(); } catch { /* user can print manually */ } }, 400);
  return true;
}

/* ── UI ─────────────────────────────────────────────────────────────────────── */

function AtoCheck({ on, onClick, title, note, count }) {
  return (
    <button className="focus-ring" onClick={onClick} style={{
      all:"unset", cursor:"pointer", display:"flex", gap:9, alignItems:"flex-start", padding:"8px 10px", borderRadius:8,
      border:`1px solid ${on ? "var(--cf-brand-purple)" : "var(--cf-divider)"}`,
      background: on ? "color-mix(in oklab, var(--cf-brand-purple) 8%, var(--cf-card-bg))" : "var(--cf-card-bg)",
    }}>
      <span style={{ width:15, height:15, borderRadius:4, flexShrink:0, marginTop:1,
        border:`1.5px solid ${on ? "var(--cf-brand-purple)" : "var(--cf-text-muted)"}`,
        background: on ? "var(--cf-brand-purple)" : "transparent",
        display:"flex", alignItems:"center", justifyContent:"center" }}>
        {on && <Icon name="check" size={10} style={{ color:"white" }}/>}
      </span>
      <span style={{ minWidth:0, flex:1 }}>
        <span style={{ display:"flex", gap:6, alignItems:"baseline" }}>
          <span style={{ fontSize:12, fontWeight:600 }}>{title}</span>
          {count != null && <span className="mono" style={{ fontSize:10.5, color:"var(--cf-text-muted)" }}>{count}</span>}
        </span>
        {note && <span style={{ display:"block", fontSize:10.5, color:"var(--cf-text-muted)", lineHeight:1.4, marginTop:2 }}>{note}</span>}
      </span>
    </button>
  );
}

function AtoPackageModal({ initialEnv, initialSysIds, onClose }) {
  usePoamStore();
  const envs = typeof ENVIRONMENTS !== "undefined" ? ENVIRONMENTS : [];
  const [mode, setMode] = React.useState(initialSysIds?.length ? "systems" : "env");
  const [env, setEnv] = React.useState(initialEnv || envs[0]?.name || "");
  const [sysIds, setSysIds] = React.useState(initialSysIds || []);
  const [sysQuery, setSysQuery] = React.useState("");
  const [formats, setFormats] = React.useState({ oscal:true, xccdf:false, csv:true, cfjson:false, report:true });
  const [opts, setOpts] = React.useState({ poams:true, cves:true, attestations:true, configEvidence:true, waivers:true });
  const [built, setBuilt] = React.useState(null);
  const [building, setBuilding] = React.useState(false);

  const scope = React.useMemo(() => ({ mode, env, sysIds }), [mode, env, sysIds]);
  const data = React.useMemo(() => atoCollect(scope), [mode, env, sysIds.join(",")]);
  const fileCount = Object.entries(formats).filter(([,v]) => v)
    .reduce((a, [k]) => a + (k === "oscal" ? (opts.poams ? 3 : 2) : 1), 0) + 1;
  const anyFormat = Object.values(formats).some(Boolean);
  const canBuild = data.systems.length > 0 && anyFormat;

  const toggleFmt = (k) => setFormats(f => ({ ...f, [k]: !f[k] }));
  const toggleOpt = (k) => setOpts(o => ({ ...o, [k]: !o[k] }));
  const toggleSys = (id) => setSysIds(prev => prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id]);

  const allSystems = typeof SYSTEMS !== "undefined" ? SYSTEMS : [];
  const sysMatches = allSystems.filter(s => !sysQuery
    || s.hostname.toLowerCase().includes(sysQuery.toLowerCase())
    || s.environment.toLowerCase().includes(sysQuery.toLowerCase()));

  const build = () => {
    setBuilding(true);
    setTimeout(() => { setBuilt(atoBuildArtifacts(data, formats, opts)); setBuilding(false); }, 450);
  };

  const checks = [
    { ok: data.unmanaged.length === 0, label: data.unmanaged.length ? `${data.unmanaged.length} failing finding${data.unmanaged.length===1?"":"s"} with no remediation plan` : "Every failing finding has a remediation plan",
      hint: "An auditor will ask what the plan is. Exports include these as unmanaged deficiencies." },
    { ok: data.overdue.length === 0, label: data.overdue.length ? `${data.overdue.length} POA&M item${data.overdue.length===1?"":"s"} past their completion date` : "No overdue remediation items",
      hint: "Overdue items are flagged in the OSCAL POA&M and the report." },
    { ok: data.staleAttestations.length === 0, label: data.staleAttestations.length ? `${data.staleAttestations.length} host${data.staleAttestations.length===1?"":"s"} without current running-state proof` : "Running-state attestation current on every host",
      hint: "Without a fresh attestation, the package asserts intended state, not observed state." },
    { ok: data.unmappedPolicies.length === 0, label: data.unmappedPolicies.length ? `${data.unmappedPolicies.length} control${data.unmappedPolicies.length===1?"":"s"} not mapped to a framework requirement` : "All controls map to framework requirements",
      hint: "Unmapped controls export under a local cf- control id instead of a catalog id." },
  ];

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{ width:"min(940px,96vw)", maxHeight:"92vh" }}>
        <div className="modal-head">
          <h2><Icon name="download" size={14} style={{ marginRight:6, verticalAlign:"text-bottom" }}/>Export compliance evidence package</h2>
          <p>Scope it, pick the formats your auditor or authorizing official accepts, and download. One environment is the usual unit of assessment.</p>
        </div>

        {built ? (
          <>
            <div className="modal-body" style={{ overflowY:"auto" }}>
              <div className="sd-callout sd-callout-info" style={{ marginBottom:12 }}>
                <Icon name="check" size={13}/>
                <div style={{ fontSize:12 }}>
                  <div><strong>{built.length} files</strong> · {atoScopeLabel(scope)} · {data.systems.length} hosts · {data.results.length} control evaluations · score {data.score}%</div>
                  <div style={{ marginTop:3, color:"var(--cf-text-muted)" }}>The manifest lists every file with its digest so the package can be verified after transfer.</div>
                </div>
              </div>
              <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
                {built.map(f => (
                  <div key={f.name} style={{ display:"flex", alignItems:"center", gap:10, padding:"9px 11px", border:"1px solid var(--cf-divider)", borderRadius:8, background:"var(--cf-card-bg)" }}>
                    <Icon name={f.mime === "text/html" ? "file" : f.mime === "text/csv" ? "rows" : "file"} size={14} style={{ color:"var(--cf-text-muted)", flexShrink:0 }}/>
                    <div style={{ minWidth:0, flex:1 }}>
                      <div className="mono" style={{ fontSize:12, fontWeight:600 }}>{f.name}</div>
                      <div style={{ fontSize:10.5, color:"var(--cf-text-muted)" }}>{f.kind} · {f.size} · <span className="mono">{f.digest.slice(0,20)}…</span></div>
                    </div>
                    {f.mime === "text/html" && (
                      <button className="btn btn-ghost focus-ring xs" onClick={() => atoPrintReport(f)}>Print / PDF</button>
                    )}
                    <button className="btn btn-ghost focus-ring xs" onClick={() => atoDownload(f)}><Icon name="download" size={12}/> Download</button>
                  </div>
                ))}
              </div>
            </div>
            <div className="modal-foot">
              <button className="btn btn-ghost focus-ring" onClick={() => setBuilt(null)}>Back to options</button>
              <button className="btn btn-primary focus-ring" onClick={() => built.forEach((f, i) => setTimeout(() => atoDownload(f), i * 250))}>
                <Icon name="download" size={13}/> Download all {built.length} files
              </button>
            </div>
          </>
        ) : (
          <>
            <div className="modal-body" style={{ overflowY:"auto", display:"grid", gridTemplateColumns:"minmax(0,1fr) minmax(0,1.15fr)", gap:18 }}>
              <div style={{ display:"flex", flexDirection:"column", gap:14, minWidth:0 }}>
                <div className="field" style={{ marginTop:0 }}>
                  <label>Scope</label>
                  <div className="seg" style={{ width:"fit-content", marginBottom:8 }}>
                    <button className={mode==="env"?"active":""} onClick={()=>setMode("env")}>Environment</button>
                    <button className={mode==="systems"?"active":""} onClick={()=>setMode("systems")}>Pick systems</button>
                  </div>
                  {mode === "env" ? (
                    <div style={{ display:"flex", flexWrap:"wrap", gap:6 }}>
                      {envs.map(e => {
                        const on = env === e.name;
                        const n = allSystems.filter(s => s.environment === e.name).length;
                        return (
                          <button key={e.name} className="focus-ring" onClick={()=>setEnv(e.name)} style={{
                            all:"unset", cursor:"pointer", padding:"6px 11px", borderRadius:99, fontSize:12, fontWeight:600,
                            display:"flex", alignItems:"center", gap:7,
                            border:`1px solid ${on ? (e.dot || e.color || "var(--cf-brand-purple)") : "var(--cf-divider)"}`,
                            background: on ? `color-mix(in oklab, ${e.dot || e.color || "var(--cf-brand-purple)"} 14%, var(--cf-card-bg))` : "var(--cf-card-bg)",
                            color: on ? "var(--cf-text-primary)" : "var(--cf-text-muted)",
                          }}>
                            <span style={{ width:8, height:8, borderRadius:99, background: e.dot || e.color || "#888" }}/>
                            {e.name}
                            <span className="mono" style={{ fontSize:10.5, opacity:0.75 }}>{n}</span>
                          </button>
                        );
                      })}
                    </div>
                  ) : (
                    <>
                      <div style={{ display:"flex", gap:6, marginBottom:6 }}>
                        <input className="input focus-ring" placeholder="Search hosts or environment…" value={sysQuery} onChange={e=>setSysQuery(e.target.value)}/>
                        <button className="btn btn-ghost focus-ring xs" onClick={()=>setSysIds(sysMatches.map(s=>s.id))}>All shown</button>
                        <button className="btn btn-ghost focus-ring xs" onClick={()=>setSysIds([])}>None</button>
                      </div>
                      <div style={{ maxHeight:212, overflowY:"auto", border:"1px solid var(--cf-divider)", borderRadius:8 }}>
                        {sysMatches.map(s => {
                          const on = sysIds.includes(s.id);
                          return (
                            <button key={s.id} className="focus-ring" onClick={()=>toggleSys(s.id)} style={{
                              all:"unset", cursor:"pointer", width:"100%", boxSizing:"border-box", display:"flex", alignItems:"center", gap:9,
                              padding:"6px 9px", borderBottom:"1px solid var(--cf-divider)",
                              background: on ? "color-mix(in oklab, var(--cf-brand-purple) 8%, transparent)" : "transparent",
                            }}>
                              <span style={{ width:14, height:14, borderRadius:3, flexShrink:0,
                                border:`1.5px solid ${on ? "var(--cf-brand-purple)" : "var(--cf-text-muted)"}`,
                                background: on ? "var(--cf-brand-purple)" : "transparent",
                                display:"flex", alignItems:"center", justifyContent:"center" }}>
                                {on && <Icon name="check" size={9} style={{ color:"white" }}/>}
                              </span>
                              <span className="mono truncate" style={{ fontSize:12, flex:1 }}>{s.hostname}</span>
                              <EnvBadge env={s.environment}/>
                            </button>
                          );
                        })}
                        {sysMatches.length === 0 && <div style={{ padding:"10px", fontSize:12, color:"var(--cf-text-muted)" }}>No hosts match “{sysQuery}”.</div>}
                      </div>
                    </>
                  )}
                </div>

                <div className="field">
                  <label>Package contents</label>
                  <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
                    <AtoCheck on onClick={()=>{}} title="Control assessment results" count={`${data.results.length} evals · ${data.bundles.length} bundles`} note="Always included — the assessment itself."/>
                    <AtoCheck on={opts.poams} onClick={()=>toggleOpt("poams")} title="Remediation plans (POA&M)" count={`${data.poams.length}`} note="Open and closed remediation plans for findings in scope."/>
                    <AtoCheck on={opts.attestations} onClick={()=>toggleOpt("attestations")} title="Running-state attestations" count={`${data.attestations.length}`} note="Signed proof of the artifact actually running on each host."/>
                    <AtoCheck on={opts.cves} onClick={()=>toggleOpt("cves")} title="Vulnerability summary" count={`${data.cves.length}`} note="CVEs affecting hosts in scope, with acceptance decisions."/>
                    <AtoCheck on={opts.configEvidence} onClick={()=>toggleOpt("configEvidence")} title="Evidence references" note="Rendered module paths, hashes, and audit-record citations per control."/>
                    <AtoCheck on={opts.waivers} onClick={()=>toggleOpt("waivers")} title="Waivers and risk acceptances" count={`${data.counts.waiver}`} note="Justification, approver, and expiry for each accepted risk."/>
                  </div>
                </div>
              </div>

              <div style={{ display:"flex", flexDirection:"column", gap:14, minWidth:0 }}>
                <div className="field" style={{ marginTop:0 }}>
                  <label>Formats <span style={{ color:"var(--cf-text-muted)", fontWeight:400 }}>· {fileCount} file{fileCount===1?"":"s"} incl. manifest</span></label>
                  <div style={{ display:"flex", flexDirection:"column", gap:6 }}>
                    {Object.entries(ATO_FORMATS).map(([k, m]) => (
                      <AtoCheck key={k} on={!!formats[k]} onClick={()=>toggleFmt(k)} title={m.label} note={m.note}
                        count={k === "oscal" ? `${opts.poams ? 3 : 2} files` : null}/>
                    ))}
                  </div>
                </div>

                <div className="field">
                  <label>Package readiness</label>
                  <div style={{ display:"flex", flexDirection:"column", gap:5 }}>
                    {checks.map((c,i) => (
                      <div key={i} style={{ display:"flex", gap:8, alignItems:"flex-start", fontSize:11.5, padding:"6px 9px", borderRadius:7,
                        background: c.ok ? "color-mix(in oklab, #34d399 8%, transparent)" : "color-mix(in oklab, #fbbf24 10%, transparent)" }}>
                        <Icon name={c.ok ? "check" : "warn"} size={12} style={{ color: c.ok ? "#34d399" : "#fbbf24", flexShrink:0, marginTop:1 }}/>
                        <div style={{ minWidth:0 }}>
                          <div style={{ fontWeight:600 }}>{c.label}</div>
                          {!c.ok && <div style={{ color:"var(--cf-text-muted)", marginTop:1 }}>{c.hint}</div>}
                        </div>
                      </div>
                    ))}
                  </div>
                  <div className="help" style={{ marginTop:6 }}>
                    Warnings do not block the export. They are what an assessor will ask about, so they are also written into the manifest.
                  </div>
                </div>
              </div>
            </div>
            <div className="modal-foot">
              <div style={{ marginRight:"auto", fontSize:11.5, color:"var(--cf-text-muted)" }}>
                {data.systems.length} host{data.systems.length===1?"":"s"} · {data.results.length} evaluations · {data.counts.fail} failing · {data.poams.filter(p=>p.status!=="completed").length} open remediation plan{data.poams.filter(p=>p.status!=="completed").length===1?"":"s"}
              </div>
              <button className="btn btn-ghost focus-ring" onClick={onClose}>Cancel</button>
              <button className="btn btn-primary focus-ring" disabled={!canBuild || building} onClick={build}
                style={!canBuild || building ? { opacity:0.55, cursor:"not-allowed" } : null}>
                {building ? <>Collecting evidence…</> : <><Icon name="download" size={13}/> Generate package</>}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

Object.assign(window, { AtoPackageModal, atoCollect, atoBuildArtifacts, ATO_FORMATS });
