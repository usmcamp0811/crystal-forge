-- Index-friendly evidence-scope lookups.
--
-- Migration 0322 introduced derivations_share_evidence_scope(target, source) and
-- used it as `x.derivation_id = $1 OR derivations_share_evidence_scope($1, x.derivation_id)`.
-- PostgreSQL cannot use an index for the second arm, so each such predicate
-- scans the whole table and calls the function per row. Measured on 200,000
-- derivations and 180,000 scans: 1.0-1.1 s per lookup, 0.6 ms with the peer set
-- below. reusable_physical_build_tx runs the pattern once per admitted
-- derivation, so evaluating one commit paid it once per configuration.
--
-- INVARIANT: derivation_evidence_peers(T) returns exactly the derivations S for
-- which `S = T OR derivations_share_evidence_scope(T, S)` holds. The scope
-- function requires COALESCE(reused_from_derivation_id, id) to be equal on both
-- sides, so every such S is the physical origin P of T or an alias with
-- reused_from_derivation_id = P. The function enumerates that set through the
-- primary key and derivations_reuse_origin_idx and still applies the strict
-- scope predicate to each member. It adds no identity rule and removes none.
CREATE FUNCTION derivation_evidence_peers(target_id integer)
RETURNS SETOF integer LANGUAGE sql STABLE STRICT AS $$
    SELECT peer.id
    FROM derivations target
    JOIN derivations peer
      ON peer.id = COALESCE(target.reused_from_derivation_id, target.id)
      OR peer.reused_from_derivation_id = COALESCE(target.reused_from_derivation_id, target.id)
    WHERE target.id = target_id
      AND (peer.id = target.id OR derivations_share_evidence_scope(target.id, peer.id))
$$;

COMMENT ON FUNCTION derivation_evidence_peers(integer) IS
    'The derivation itself plus every derivation that shares its evidence scope. Equivalent to the OR of equality and derivations_share_evidence_scope, but indexable.';

-- derivation_execution_source returns the target itself unless the target adopted
-- a build job (reused_build_job_id IS NOT NULL): its inner join finds no row for
-- a NULL job and the COALESCE falls back to the target. Calling it only for
-- adopted rows gives an identical result without a function call per derivation.
-- Measured over 200,000 derivations: 1,072 ms before, 70 ms after.
CREATE OR REPLACE VIEW view_derivation_execution_evidence AS
SELECT target.id,target.commit_id,target.derivation_type,target.derivation_name,
       target.derivation_path,target.derivation_target,target.expected_store_path,
       target.cf_agent_enabled,target.policy_requirements_met,target.policy_results,
       target.error_message,target.status_id,target.completed_at,
       target.reused_from_derivation_id,
       target.reused_build_job_id,
       COALESCE(target.reused_from_derivation_id,target.id) AS physical_derivation_id,
       source.id AS execution_derivation_id,source.commit_id AS execution_commit_id,
       source.store_path,
       -- EvaluationStatus IDs describe the selected real job here, not B's
       -- evaluation or the source row's newer failed retry. No status is copied.
       CASE adopted.status WHEN 'success' THEN 10 WHEN 'queued' THEN 7
            WHEN 'building' THEN 8 WHEN 'cancelling' THEN 8
            ELSE source.status_id END AS execution_status_id
FROM derivations target
JOIN derivations source ON source.id=CASE WHEN target.reused_build_job_id IS NULL
                                          THEN target.id
                                          ELSE derivation_execution_source(target.id) END
LEFT JOIN build_jobs adopted ON adopted.id=target.reused_build_job_id AND adopted.derivation_id=source.id;

-- 0322 replaced the unique constraint on derivation_path with a partial unique
-- index that serves only queries stating `reused_from_derivation_id IS NULL`.
-- Lookups by path alone (CVE occurrence matching, inspections) lost their index.
CREATE INDEX derivations_derivation_path_idx ON derivations(derivation_path);

-- reused_build_job_id is a RESTRICT foreign key. Without an index every build
-- job deletion scans derivations.
CREATE INDEX derivations_reused_build_job_idx ON derivations(reused_build_job_id)
    WHERE reused_build_job_id IS NOT NULL;
