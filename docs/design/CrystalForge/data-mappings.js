// Compliance mappings — the reusable-policy abstraction.
// Policy <-> Requirement is an explicit, inspectable relationship. Frameworks own
// requirements; requirements have hierarchy; policies map to zero, one, or many
// requirements across zero, one, or many frameworks. A policy is never "a NIST policy".

const COMPLIANCE_FRAMEWORKS = [
  { id:"nist-800-53", name:"NIST 800-53", version:"Rev 5", hierarchyLabels:["Family","Control","Enhancement"] },
  { id:"disa-stig",   name:"DISA STIG",   version:"Anduril NixOS v1r2", hierarchyLabels:["Group","Rule"] },
  { id:"cis",         name:"CIS Benchmark", version:"NixOS Benchmark 1.0", hierarchyLabels:["Section","Subsection","Recommendation"] },
  { id:"cmmc",        name:"CMMC 2.0",    version:"Level-based practices", hierarchyLabels:["Domain","Practice"] },
];
function frameworkById(id) { return COMPLIANCE_FRAMEWORKS.find(f => f.id === id); }

// Generic requirement node: { id, frameworkId, externalId, title, kind, parentId }
// kind is framework-defined (family/control/enhancement, group/rule, section/subsection/recommendation, domain/practice).
const COMPLIANCE_REQUIREMENTS = [
  // NIST 800-53 — families, then controls/enhancements under them
  { id:"nist-AC", frameworkId:"nist-800-53", externalId:"AC", title:"Access Control", kind:"family", parentId:null },
  { id:"nist-AC-8", frameworkId:"nist-800-53", externalId:"AC-8", title:"System Use Notification", kind:"control", parentId:"nist-AC" },
  { id:"nist-AC-17", frameworkId:"nist-800-53", externalId:"AC-17", title:"Remote Access", kind:"control", parentId:"nist-AC" },
  { id:"nist-AC-17-1", frameworkId:"nist-800-53", externalId:"AC-17(1)", title:"Monitoring and Control", kind:"enhancement", parentId:"nist-AC-17" },
  { id:"nist-AU", frameworkId:"nist-800-53", externalId:"AU", title:"Audit & Accountability", kind:"family", parentId:null },
  { id:"nist-AU-12", frameworkId:"nist-800-53", externalId:"AU-12", title:"Audit Record Generation", kind:"control", parentId:"nist-AU" },
  { id:"nist-SC", frameworkId:"nist-800-53", externalId:"SC", title:"System & Communications Protection", kind:"family", parentId:null },
  { id:"nist-SC-7", frameworkId:"nist-800-53", externalId:"SC-7", title:"Boundary Protection", kind:"control", parentId:"nist-SC" },
  { id:"nist-SC-8", frameworkId:"nist-800-53", externalId:"SC-8", title:"Transmission Confidentiality and Integrity", kind:"control", parentId:"nist-SC" },
  { id:"nist-SC-13", frameworkId:"nist-800-53", externalId:"SC-13", title:"Cryptographic Protection", kind:"control", parentId:"nist-SC" },
  { id:"nist-SC-28", frameworkId:"nist-800-53", externalId:"SC-28", title:"Protection of Information at Rest", kind:"control", parentId:"nist-SC" },
  { id:"nist-CM", frameworkId:"nist-800-53", externalId:"CM", title:"Configuration Management", kind:"family", parentId:null },
  { id:"nist-CM-6", frameworkId:"nist-800-53", externalId:"CM-6", title:"Configuration Settings", kind:"control", parentId:"nist-CM" },
  { id:"nist-IA", frameworkId:"nist-800-53", externalId:"IA", title:"Identification & Authentication", kind:"family", parentId:null },
  { id:"nist-IA-5", frameworkId:"nist-800-53", externalId:"IA-5", title:"Authenticator Management", kind:"control", parentId:"nist-IA" },
  { id:"nist-MP", frameworkId:"nist-800-53", externalId:"MP", title:"Media Protection", kind:"family", parentId:null },
  { id:"nist-MP-7", frameworkId:"nist-800-53", externalId:"MP-7", title:"Media Use", kind:"control", parentId:"nist-MP" },

  // DISA STIG — rules (flat under an implicit benchmark; group omitted where source didn't carry one)
  { id:"stig-V-268137", frameworkId:"disa-stig", externalId:"V-268137", title:"The operating system must not permit direct root logon via SSH.", kind:"rule", parentId:null, cci:"CCI-000770" },
  { id:"stig-V-268142", frameworkId:"disa-stig", externalId:"V-268142", title:"The operating system must terminate idle SSH sessions after 10 minutes.", kind:"rule", parentId:null, cci:"CCI-001133" },
  { id:"stig-V-268089", frameworkId:"disa-stig", externalId:"V-268089", title:"The operating system must use FIPS-validated ciphers for remote access.", kind:"rule", parentId:null, cci:"CCI-000068" },
  { id:"stig-V-268080", frameworkId:"disa-stig", externalId:"V-268080", title:"The operating system must enable the audit daemon.", kind:"rule", parentId:null, cci:"CCI-000018" },
  { id:"stig-V-268078", frameworkId:"disa-stig", externalId:"V-268078", title:"The operating system must enable the built-in firewall.", kind:"rule", parentId:null, cci:"CCI-000366" },
  { id:"stig-V-268082", frameworkId:"disa-stig", externalId:"V-268082", title:"The operating system must display the DoD/USG consent banner.", kind:"rule", parentId:null, cci:"CCI-000048" },
  { id:"stig-V-268168", frameworkId:"disa-stig", externalId:"V-268168", title:"The operating system must use FIPS-validated cryptography.", kind:"rule", parentId:null, cci:"CCI-002450" },
  { id:"stig-V-268144", frameworkId:"disa-stig", externalId:"V-268144", title:"The operating system must protect data at rest with encryption.", kind:"rule", parentId:null, cci:"CCI-001199" },
  { id:"stig-V-268139", frameworkId:"disa-stig", externalId:"V-268139", title:"The operating system must control peripheral access with USBGuard.", kind:"rule", parentId:null, cci:"CCI-001958" },
  { id:"stig-V-268134", frameworkId:"disa-stig", externalId:"V-268134", title:"The operating system must enforce a 15-character minimum password length.", kind:"rule", parentId:null, cci:"CCI-000205" },
  { id:"stig-V-268130", frameworkId:"disa-stig", externalId:"V-268130", title:"The operating system must store only encrypted passwords.", kind:"rule", parentId:null, cci:"CCI-000196" },

  // CIS Benchmark — section -> subsection -> recommendation
  { id:"cis-4", frameworkId:"cis", externalId:"4", title:"Logging and Auditing", kind:"section", parentId:null },
  { id:"cis-4.1", frameworkId:"cis", externalId:"4.1", title:"Configure System Accounting (auditd)", kind:"subsection", parentId:"cis-4" },
  { id:"cis-4.1.1", frameworkId:"cis", externalId:"4.1.1", title:"Ensure auditd is installed and enabled", kind:"recommendation", parentId:"cis-4.1" },
  { id:"cis-5", frameworkId:"cis", externalId:"5", title:"Access, Authentication and Authorization", kind:"section", parentId:null },
  { id:"cis-5.1", frameworkId:"cis", externalId:"5.1", title:"Configure SSH Server", kind:"subsection", parentId:"cis-5" },
  { id:"cis-5.1.8", frameworkId:"cis", externalId:"5.1.8", title:"Ensure SSH root login is disabled", kind:"recommendation", parentId:"cis-5.1" },
  { id:"cis-5.1.10", frameworkId:"cis", externalId:"5.1.10", title:"Ensure SSH warning banner is configured", kind:"recommendation", parentId:"cis-5.1" },
  { id:"cis-5.4", frameworkId:"cis", externalId:"5.4", title:"User Accounts and Environment", kind:"subsection", parentId:"cis-5" },
  { id:"cis-5.4.1", frameworkId:"cis", externalId:"5.4.1", title:"Ensure password quality is configured", kind:"recommendation", parentId:"cis-5.4" },

  // CMMC 2.0 — domain -> practice
  { id:"cmmc-AC", frameworkId:"cmmc", externalId:"AC", title:"Access Control", kind:"domain", parentId:null },
  { id:"cmmc-AC-3.1.12", frameworkId:"cmmc", externalId:"AC.L2-3.1.12", title:"Remote access monitoring and control", kind:"practice", parentId:"cmmc-AC" },
  { id:"cmmc-AU", frameworkId:"cmmc", externalId:"AU", title:"Audit & Accountability", kind:"domain", parentId:null },
  { id:"cmmc-AU-3.3.1", frameworkId:"cmmc", externalId:"AU.L2-3.3.1", title:"System audit records", kind:"practice", parentId:"cmmc-AU" },
  { id:"cmmc-IA", frameworkId:"cmmc", externalId:"IA", title:"Identification & Authentication", kind:"domain", parentId:null },
  { id:"cmmc-IA-3.5.7", frameworkId:"cmmc", externalId:"IA.L2-3.5.7", title:"Enforce minimum password complexity", kind:"practice", parentId:"cmmc-IA" },
  { id:"cmmc-MP", frameworkId:"cmmc", externalId:"MP", title:"Media Protection", kind:"domain", parentId:null },
  { id:"cmmc-MP-3.8.7", frameworkId:"cmmc", externalId:"MP.L2-3.8.7", title:"Control use of removable media", kind:"practice", parentId:"cmmc-MP" },
  { id:"cmmc-SC", frameworkId:"cmmc", externalId:"SC", title:"System & Communications Protection", kind:"domain", parentId:null },
  { id:"cmmc-SC-3.13.11", frameworkId:"cmmc", externalId:"SC.L2-3.13.11", title:"FIPS-validated cryptography", kind:"practice", parentId:"cmmc-SC" },
];

function reqById(id) { return COMPLIANCE_REQUIREMENTS.find(r => r.id === id); }
function reqsOfFramework(frameworkId) { return COMPLIANCE_REQUIREMENTS.filter(r => r.frameworkId === frameworkId); }
function reqChildren(id) { return COMPLIANCE_REQUIREMENTS.filter(r => r.parentId === id); }
function reqBreadcrumb(id) {
  const chain = [];
  let cur = reqById(id);
  while (cur) { chain.unshift(cur); cur = cur.parentId ? reqById(cur.parentId) : null; }
  return chain;
}
function reqTree(frameworkId) {
  const roots = reqsOfFramework(frameworkId).filter(r => !r.parentId);
  const build = (node) => ({ ...node, children: reqChildren(node.id).map(build) });
  return roots.map(build);
}
function reqSearch(frameworkId, query) {
  const q = (query || "").trim().toLowerCase();
  const pool = reqsOfFramework(frameworkId);
  if (!q) return pool;
  return pool.filter(r => r.externalId.toLowerCase().includes(q) || r.title.toLowerCase().includes(q) || (r.cci||"").toLowerCase().includes(q));
}

const RELATIONSHIPS = [
  { id:"implements", label:"Implements", blurb:"The policy directly satisfies this requirement." },
  { id:"supports", label:"Supports", blurb:"The policy contributes to satisfying the requirement but does not satisfy it alone." },
  { id:"provides_evidence", label:"Provides evidence for", blurb:"The policy gathers or produces evidence relevant to determining compliance with the requirement." },
];
function relationshipMeta(id) { return RELATIONSHIPS.find(r => r.id === id) || RELATIONSHIPS[0]; }

let _mapSeq = 1;
function mapId() { return `map-${_mapSeq++}`; }

// Policy <-> Requirement mappings. provenance: "manual" | "imported" | "suggested".
const POLICY_REQUIREMENT_MAPPINGS = [
  // stig-ssh-hardening
  { id:mapId(), policyId:"stig-sshd", requirementId:"stig-V-268137", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-sshd", requirementId:"stig-V-268142", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-sshd", requirementId:"stig-V-268089", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-sshd", requirementId:"nist-AC-17", relationship:"implements", coverage:"full", provenance:"manual",
    rationale:"Disabling root SSH login and enforcing idle timeouts directly satisfies remote access control." },
  { id:mapId(), policyId:"stig-sshd", requirementId:"nist-SC-8", relationship:"supports", coverage:"partial", provenance:"manual",
    rationale:"FIPS-approved ciphers protect the SSH transport, but full SC-8 coverage needs org-wide key management too." },
  { id:mapId(), policyId:"stig-sshd", requirementId:"cis-5.1.8", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-sshd", requirementId:"cmmc-AC-3.1.12", relationship:"supports", coverage:"partial", provenance:"manual" },

  // stig-audit-daemon
  { id:mapId(), policyId:"stig-auditd", requirementId:"stig-V-268080", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-auditd", requirementId:"stig-V-268078", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-auditd", requirementId:"nist-AU-12", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-auditd", requirementId:"nist-SC-7", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-auditd", requirementId:"cis-4.1.1", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-auditd", requirementId:"cmmc-AU-3.3.1", relationship:"implements", coverage:"full", provenance:"manual" },

  // stig-consent-banner
  { id:mapId(), policyId:"stig-banner", requirementId:"stig-V-268082", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-banner", requirementId:"nist-AC-8", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-banner", requirementId:"cis-5.1.10", relationship:"implements", coverage:"full", provenance:"manual" },

  // stig-fips-crypto
  { id:mapId(), policyId:"stig-fips", requirementId:"stig-V-268168", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-fips", requirementId:"stig-V-268144", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-fips", requirementId:"nist-SC-13", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-fips", requirementId:"nist-SC-28", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-fips", requirementId:"cmmc-SC-3.13.11", relationship:"implements", coverage:"full", provenance:"manual" },

  // stig-usbguard
  { id:mapId(), policyId:"stig-usbguard", requirementId:"stig-V-268139", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-usbguard", requirementId:"nist-MP-7", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-usbguard", requirementId:"cmmc-MP-3.8.7", relationship:"implements", coverage:"full", provenance:"manual" },

  // stig-password-policy (current revision, id "stig-pwquality")
  { id:mapId(), policyId:"stig-pwquality", requirementId:"stig-V-268134", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-pwquality", requirementId:"stig-V-268130", relationship:"implements", coverage:"full", provenance:"imported", importedFrom:"DISA XCCDF" },
  { id:mapId(), policyId:"stig-pwquality", requirementId:"nist-IA-5", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-pwquality", requirementId:"cis-5.4.1", relationship:"implements", coverage:"full", provenance:"manual" },
  { id:mapId(), policyId:"stig-pwquality", requirementId:"cmmc-IA-3.5.7", relationship:"implements", coverage:"full", provenance:"manual" },
];

// Suggested (not-yet-accepted) mappings derived from a framework crosswalk — kept
// separate from POLICY_REQUIREMENT_MAPPINGS on purpose; a crosswalk between two
// requirements never silently becomes a policy mapping.
const SUGGESTED_MAPPINGS = [
  { id:"sug-1", policyId:"stig-sshd", requirementId:"nist-CM-6", derivedFrom:"DISA crosswalk (V-268137 → CM-6)" },
];

function mappingsForPolicy(policyId) { return POLICY_REQUIREMENT_MAPPINGS.filter(m => m.policyId === policyId); }
function suggestedForPolicy(policyId) { return SUGGESTED_MAPPINGS.filter(m => m.policyId === policyId); }
function mappingsForRequirement(reqId) { return POLICY_REQUIREMENT_MAPPINGS.filter(m => m.requirementId === reqId); }

// Group a policy's mappings by framework, each entry joined with its requirement + framework record.
function mappingsGroupedByFramework(policyId) {
  const rows = mappingsForPolicy(policyId).map(m => ({ mapping:m, requirement: reqById(m.requirementId), framework: frameworkById(reqById(m.requirementId)?.frameworkId) }));
  const byFw = new Map();
  rows.forEach(r => {
    const key = r.framework?.id || "unknown";
    if (!byFw.has(key)) byFw.set(key, { framework:r.framework, rows:[] });
    byFw.get(key).rows.push(r);
  });
  return Array.from(byFw.values());
}

// Which bundles (by lineage) reference this policy id — "used by N bundles", distinct from "mapped to N requirements".
function bundlesUsingPolicy(policyId) {
  const bundles = (typeof COMPLIANCE_BUNDLES !== "undefined" ? COMPLIANCE_BUNDLES : []).filter(b => (b.policyIds||[]).includes(policyId));
  const lineages = new Set(bundles.map(b => b.lineageId || b.id));
  return { bundles, count: lineages.size };
}

function isDuplicateMapping(policyId, requirementId, excludeId) {
  return POLICY_REQUIREMENT_MAPPINGS.some(m => m.policyId === policyId && m.requirementId === requirementId && m.id !== excludeId);
}

// Requirement coverage for a bundle: every requirement under the bundle's framework,
// derived purely from mappings of the policies actually selected into the bundle —
// never from a `family`/`framework` property living on the policy itself.
function bundleRequirementCoverage(bundle) {
  const fw = COMPLIANCE_FRAMEWORKS.find(f => f.name === bundle.framework);
  if (!fw) return null;
  const policyIds = new Set(bundle.policyIds || []);
  const allReqs = reqsOfFramework(fw.id).filter(r => reqChildren(r.id).length === 0); // leaf requirements only
  const rows = allReqs.map(req => {
    const maps = mappingsForRequirement(req.id).filter(m => policyIds.has(m.policyId));
    let status = "unmapped";
    if (maps.some(m => m.relationship === "implements" && m.coverage === "full")) status = "full";
    else if (maps.length) status = "partial";
    return { requirement:req, mappings:maps, status };
  });
  return {
    framework: fw,
    total: rows.length,
    full: rows.filter(r=>r.status==="full").length,
    partial: rows.filter(r=>r.status==="partial").length,
    unmapped: rows.filter(r=>r.status==="unmapped").length,
    rows,
  };
}

// Split candidate policies for "Add Policies to Bundle": mapped-to-this-framework vs custom additions.
function splitPoliciesForBundleFramework(policies, bundleFrameworkName) {
  const fw = COMPLIANCE_FRAMEWORKS.find(f => f.name === bundleFrameworkName);
  if (!fw) return { mapped: [], other: policies };
  const mapped = [], other = [];
  policies.forEach(p => {
    const hasMapping = mappingsForPolicy(p.id).some(m => reqById(m.requirementId)?.frameworkId === fw.id);
    (hasMapping ? mapped : other).push(p);
  });
  return { mapped, other };
}

Object.assign(window, {
  COMPLIANCE_FRAMEWORKS, COMPLIANCE_REQUIREMENTS, POLICY_REQUIREMENT_MAPPINGS, SUGGESTED_MAPPINGS, RELATIONSHIPS,
  frameworkById, reqById, reqsOfFramework, reqChildren, reqBreadcrumb, reqTree, reqSearch, relationshipMeta,
  mappingsForPolicy, suggestedForPolicy, mappingsForRequirement, mappingsGroupedByFramework, bundlesUsingPolicy,
  isDuplicateMapping, bundleRequirementCoverage, splitPoliciesForBundleFramework, mapId,
});
