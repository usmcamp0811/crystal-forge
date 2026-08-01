// Signed running-state attestations — agent-reported proof of what's actually running,
// reconciled against deployment authorization records. See docs/attestations.

const ATTESTATION_CLASSIFICATIONS = {
  authorized_current:            { label: "Authorized · current",      color: "#34d399", severity: 0, desc: "Observed artifact matches the latest deployment authorization for this system." },
  authorized_but_evidence_stale: { label: "Authorized · stale evidence",color: "#60a5fa", severity: 1, desc: "Last known-good attestation is older than the freshness interval; artifact was authorized as of last contact." },
  authorized_previous_generation:{ label: "Authorized · prior gen",     color: "#a78bfa", severity: 1, desc: "Booted generation is an earlier authorized generation, not the latest — expected during a pending reboot or rollback window." },
  deployment_pending_reboot:     { label: "Pending reboot",             color: "#fbbf24", severity: 1, desc: "Activation succeeded and profile updated, but the booted generation hasn't caught up yet." },
  activation_failed:             { label: "Activation failed",          color: "#f87171", severity: 3, desc: "Agent reported the deployment did not activate cleanly." },
  unauthorized_artifact:         { label: "Unauthorized artifact",      color: "#ef4444", severity: 4, desc: "Running store path has no matching deployment authorization. Out-of-band change — requires an explicit decision." },
  unknown_artifact:              { label: "Unknown artifact",          color: "#fb923c", severity: 3, desc: "Store path doesn't correspond to any build or eval Crystal Forge has record of." },
  agent_attestation_stale:       { label: "Attestation stale",          color: "#94a3b8", severity: 2, desc: "No attestation received within the freshness window — identity and authorization can't be reconfirmed." },
  agent_identity_invalid:        { label: "Identity invalid",           color: "#dc2626", severity: 4, desc: "Signature verification failed or key/session identity doesn't match the enrolled agent." },
};

function mkAttestation(sys, i, overrides = {}) {
  const now = Date.now();
  const boot = now - (2 + i % 5) * 3600_000;
  return {
    attestation_id: `att-${sys.id}-${i}`,
    system_id: sys.id,
    agent_key_id: `agentkey-${sys.id.slice(-6)}`,
    agent_session_id: `sess-${(1000+i).toString(36)}`,
    boot_id: `boot-${sys.id.slice(-6)}-${i%4}`,
    boot_timestamp: new Date(boot).toISOString(),
    observed_at: new Date(now - (i%7)*600_000).toISOString(),
    monotonic_counter: 4000 + i * 17,
    current_system_store_path: `/nix/store/${(sys.id + i).split("").reduce((a,c)=>((a<<5)-a+c.charCodeAt(0))|0,0).toString(16).replace("-","").padStart(8,"0").slice(0,8)}-nixos-system-${sys.hostname||sys.id}`,
    current_system_nar_hash: `sha256-${(sys.id+"nar"+i).split("").reduce((a,c)=>((a<<5)-a+c.charCodeAt(0))|0,0).toString(16).padStart(12,"a")}`,
    system_profile_store_path: `/nix/var/nix/profiles/system-${100+i}-link`,
    booted_generation: 100 + (i % 3),
    kernel_version: "6.6.32",
    nix_version: "2.24.9",
    agent_version: "0.9.1",
    deployment_authorization_id: `auth-${sys.id}-${100+(i%3)}`,
    deployment_execution_id: `exec-${sys.id}-${100+(i%3)}`,
    activation_source: i % 6 === 0 ? "manual" : "cf-agent",
    payload_digest: `sha256:${(sys.id+"digest"+i).split("").reduce((a,c)=>((a<<5)-a+c.charCodeAt(0))|0,0).toString(16).padStart(16,"0")}`,
    agent_signature: `ed25519:${(sys.id+"sig"+i).split("").reduce((a,c)=>((a<<5)-a+c.charCodeAt(0))|0,0).toString(16).padStart(24,"f")}`,
    ...overrides,
  };
}

// Deterministic per-fleet classification mix so the view has a realistic spread.
const ATTESTATION_RECORDS = (typeof __fx === "function" && __fx("attestations")) || (typeof SYSTEMS !== "undefined" ? SYSTEMS.map((sys, i) => {
  const classKeys = Object.keys(ATTESTATION_CLASSIFICATIONS);
  let cls;
  if (i % 23 === 0) cls = "unauthorized_artifact";
  else if (i % 17 === 0) cls = "unknown_artifact";
  else if (i % 13 === 0) cls = "agent_identity_invalid";
  else if (i % 11 === 0) cls = "agent_attestation_stale";
  else if (i % 9 === 0) cls = "activation_failed";
  else if (i % 7 === 0) cls = "deployment_pending_reboot";
  else if (i % 5 === 0) cls = "authorized_previous_generation";
  else if (i % 4 === 0) cls = "authorized_but_evidence_stale";
  else cls = "authorized_current";
  const now = Date.now();
  const att = mkAttestation(sys, i, {
    observed_at: new Date(now - (cls === "agent_attestation_stale" ? 26 : 1) * 3600_000).toISOString(),
  });
  const isAttention = cls === "unauthorized_artifact" || cls === "unknown_artifact" || cls === "agent_identity_invalid";
  return {
    system_id: sys.id,
    hostname: sys.hostname,
    environment: sys.environment,
    flake: sys.flake,
    classification: cls,
    attestation: att,
    firstObserved: isAttention ? new Date(now - (2 + i%4) * 86_400_000).toISOString() : null,
    lastObserved: isAttention ? att.observed_at : null,
    previousAuthorized: isAttention ? {
      store_path: `/nix/store/8f2a1c9d-nixos-system-${sys.hostname||sys.id}`,
      deployment_authorization_id: `auth-${sys.id}-99`,
      commit: (sys.commit || "a1b2c3d").slice(0,7),
    } : null,
    resolution: null, // { decision: adopt|replace|investigate, by, at, note }
    history: [att],
  };
}) : []);

const ATTESTATION_FRESHNESS_INTERVAL_HOURS = 12;

// Deploy approvals — generated for systems whose assigned policy requires human sign-off
// (builtin "manual" or custom policies like "two-approver") and have a commit ready to promote.
const APPROVAL_QUEUE = (typeof __fx === "function" && __fx("approvals")) || (typeof SYSTEMS !== "undefined" ? SYSTEMS
  .filter((sys, i) => i % 6 === 0 && (sys.deployPolicy === "manual" || sys.deployPolicy === "two-approver" || i % 12 === 0))
  .slice(0, 9)
  .map((sys, i) => {
    const policyId = sys.deployPolicy === "two-approver" ? "two-approver" : "manual";
    const needed = policyId === "two-approver" ? 2 : 1;
    const approvals = i % 4 === 0 && needed > 1 ? [{ by:"j.alvarez@crystal-forge", at: new Date(Date.now()-3600_000).toISOString() }] : [];
    return {
      id: `apr-${sys.id}`,
      system_id: sys.id,
      hostname: sys.hostname,
      environment: sys.environment,
      flake: sys.flake,
      commit: (sys.pendingCommit || sys.commit || "a1b2c3d").slice(0,7),
      requestedBy: i % 3 === 0 ? "auto-promote" : "m.chen@crystal-forge",
      requestedAt: new Date(Date.now() - (1+i)*1800_000).toISOString(),
      policyId, neededApprovals: needed, approvals,
      status: "pending",
    };
  }) : []);

Object.assign(window, { ATTESTATION_CLASSIFICATIONS, ATTESTATION_RECORDS, mkAttestation, ATTESTATION_FRESHNESS_INTERVAL_HOURS, APPROVAL_QUEUE });
