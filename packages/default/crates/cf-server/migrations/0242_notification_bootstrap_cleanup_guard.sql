-- Preserve historical notification sources until the bounded bootstrap has
-- reached its durable end cursor. Migration 0241 creates the singleton row
-- before bootstrap completes, so row existence alone is not a completion
-- signal.
CREATE OR REPLACE FUNCTION cleanup_attention_occurrences(
    resolved_retention INTERVAL DEFAULT INTERVAL '30 days',
    batch_size INT DEFAULT 1000
)
RETURNS TABLE (deleted_occurrences BIGINT, deleted_dismissals BIGINT)
LANGUAGE plpgsql
AS $$
DECLARE deleted_occ BIGINT; deleted_dis BIGINT;
BEGIN
    -- INVARIANT: Queue rows survive source cleanup. Cleanup starts only after
    -- bounded bootstrap records completion for all pre-existing source rows.
    IF NOT EXISTS (
      SELECT 1 FROM user_notification_source_bootstrap_state
      WHERE singleton AND completed_at IS NOT NULL
    ) THEN
      RETURN QUERY SELECT 0::bigint,0::bigint;
      RETURN;
    END IF;
    WITH candidates AS (
      SELECT id FROM attention_occurrences
      WHERE resolved_at IS NOT NULL AND resolved_at<NOW()-resolved_retention
      ORDER BY resolved_at LIMIT batch_size
    ), deleted AS (
      DELETE FROM attention_occurrences USING candidates
      WHERE attention_occurrences.id=candidates.id RETURNING attention_occurrences.id
    ) SELECT COUNT(*) INTO deleted_occ FROM deleted;
    WITH candidates AS (
      SELECT dismissal.user_id,dismissal.occurrence_id FROM user_attention_dismissals dismissal
      WHERE NOT EXISTS (SELECT 1 FROM attention_occurrences occurrence
        WHERE occurrence.id=dismissal.occurrence_id) LIMIT batch_size
    ), deleted AS (
      DELETE FROM user_attention_dismissals USING candidates
      WHERE user_attention_dismissals.user_id=candidates.user_id
        AND user_attention_dismissals.occurrence_id=candidates.occurrence_id
      RETURNING user_attention_dismissals.occurrence_id
    ) SELECT COUNT(*) INTO deleted_dis FROM deleted;
    RETURN QUERY SELECT deleted_occ,deleted_dis;
END;
$$;

-- Migration 0241 guarded UUID casts with category predicates. PostgreSQL can
-- evaluate those casts before the predicates, so global numeric subjects and
-- malformed scoped subjects could abort the complete bootstrap batch.
CREATE OR REPLACE FUNCTION backfill_user_notification_source_events(p_limit integer DEFAULT 256)
RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE
    v_count bigint;
    v_cursor_at timestamptz;
    v_cursor_kind text;
    v_cursor_id uuid;
    v_next_at timestamptz;
    v_next_kind text;
    v_next_id uuid;
    v_has_more boolean;
BEGIN
    -- CONCURRENCY: This lock serializes the cursor with trigger enqueueing.
    -- Sources committed behind the cursor are already queued by their trigger.
    PERFORM pg_advisory_xact_lock(433,237);
    SELECT cursor_occurred_at,cursor_source_kind,cursor_source_id
      INTO v_cursor_at,v_cursor_kind,v_cursor_id
      FROM user_notification_source_bootstrap_state
      WHERE singleton=TRUE FOR UPDATE;

    WITH eligible AS MATERIALIZED (
      SELECT source_kind,source_id,occurred_at FROM (
        SELECT 'attention_occurrence'::text source_kind,occurrence.id source_id,
          occurrence.opened_at occurred_at
        FROM attention_occurrences occurrence
        WHERE occurrence.category IN ('builds','evals','cves')
           OR (occurrence.category='systems' AND EXISTS (
             SELECT 1 FROM systems system WHERE system.id=CASE
               WHEN occurrence.category='systems'
                AND occurrence.subject_id~*'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
               THEN occurrence.subject_id::uuid END))
           OR (occurrence.category='poams' AND EXISTS (
             SELECT 1 FROM poams poam WHERE poam.id=CASE
               WHEN occurrence.category='poams'
                AND occurrence.subject_id~*'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
               THEN occurrence.subject_id::uuid END))
        UNION ALL
        SELECT 'poam_activity',activity.id,activity.created_at
        FROM poam_activity activity
        JOIN poams poam ON poam.id=activity.poam_id
        WHERE activity.kind='status_changed'
          AND activity.payload->>'to'='awaiting_verification'
        UNION ALL
        SELECT 'system_event',event.id,event.occurred_at
        FROM system_events event
        JOIN systems system ON system.id=event.system_id
        WHERE event.event_type='cf_deployment_failed'
      ) source
      WHERE v_cursor_at IS NULL
         OR (occurred_at,source_kind,source_id)>(v_cursor_at,v_cursor_kind,v_cursor_id)
      ORDER BY occurred_at,source_kind,source_id
      LIMIT LEAST(GREATEST(p_limit,1),256)
    ), inserted AS (
      INSERT INTO user_notification_source_events(source_kind,source_id,occurred_at,category,
        source_occurrence_id,notification_source_type,notification_source_id,title,summary,route,
        authorization_scope,authorization_environment_ids)
      SELECT 'poam_activity',activity.id,activity.created_at,'policy_violations',activity.id,
        'poams',poam.id::text,'POAM-'||lpad(poam.human_number::text,4,'0')||' awaiting verification',
        poam.title||' is ready for verification.','/compliance?poam='||poam.id::text,'environments',
        COALESCE((SELECT array_agg(DISTINCT system.environment_id) FILTER (WHERE system.environment_id IS NOT NULL)
          FROM poam_context_systems context JOIN systems system ON system.id=context.system_id
          WHERE context.poam_id=poam.id),'{}')
      FROM eligible JOIN poam_activity activity ON eligible.source_kind='poam_activity' AND activity.id=eligible.source_id
      JOIN poams poam ON poam.id=activity.poam_id
      UNION ALL
      SELECT 'system_event',event.id,event.occurred_at,'deploy_failures',event.id,'systems',event.system_id::text,
        'Deployment failed','A deployment entered a failed terminal state.','/systems','environments',
        COALESCE(ARRAY[system.environment_id]::uuid[],'{}')
      FROM eligible JOIN system_events event ON eligible.source_kind='system_event' AND event.id=eligible.source_id
      JOIN systems system ON system.id=event.system_id
      UNION ALL
      SELECT 'attention_occurrence',occurrence.id,occurrence.opened_at,
        CASE occurrence.category WHEN 'builds' THEN 'build_failures' WHEN 'evals' THEN 'policy_violations'
          WHEN 'cves' THEN 'critical_cves' WHEN 'systems' THEN 'heartbeat_lost' ELSE 'policy_violations' END,
        occurrence.id,CASE WHEN occurrence.category='poams' THEN 'poams' ELSE occurrence.category END,
        occurrence.subject_id,
        CASE occurrence.category WHEN 'builds' THEN 'Build failed' WHEN 'evals' THEN 'Policy or evaluation failure'
          WHEN 'cves' THEN 'New critical CVE' WHEN 'systems' THEN 'Heartbeat lost'
          ELSE 'POAM-'||lpad(poam.human_number::text,4,'0')||' overdue' END,
        CASE occurrence.category WHEN 'builds' THEN 'A build entered a failed terminal state.'
          WHEN 'evals' THEN 'An evaluation or policy check entered a failed state.'
          WHEN 'cves' THEN 'A critical CVE attention episode opened.'
          WHEN 'systems' THEN 'A system crossed an offline or lost-heartbeat threshold.'
          ELSE poam.title||' passed its target date.' END,
        CASE occurrence.category WHEN 'builds' THEN '/builds' WHEN 'evals' THEN '/evaluations'
          WHEN 'cves' THEN '/cves' WHEN 'systems' THEN '/systems'
          ELSE '/compliance?poam='||poam.id::text END,
        CASE WHEN occurrence.category IN ('builds','evals','cves') THEN 'global' ELSE 'environments' END,
        CASE WHEN occurrence.category='systems' THEN COALESCE(ARRAY[subject_system.environment_id]::uuid[],'{}')
          WHEN occurrence.category='poams' THEN COALESCE((SELECT array_agg(DISTINCT context_system.environment_id)
            FILTER (WHERE context_system.environment_id IS NOT NULL) FROM poam_context_systems context
            JOIN systems context_system ON context_system.id=context.system_id WHERE context.poam_id=poam.id),'{}')
          ELSE '{}'::uuid[] END
      FROM eligible JOIN attention_occurrences occurrence
        ON eligible.source_kind='attention_occurrence' AND occurrence.id=eligible.source_id
      LEFT JOIN poams poam ON poam.id=CASE
        WHEN occurrence.category='poams'
         AND occurrence.subject_id~*'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        THEN occurrence.subject_id::uuid END
      LEFT JOIN systems subject_system ON subject_system.id=CASE
        WHEN occurrence.category='systems'
         AND occurrence.subject_id~*'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        THEN occurrence.subject_id::uuid END
      ON CONFLICT DO NOTHING RETURNING 1
    ), batch_state AS (
      SELECT COUNT(*) AS source_count,
             (array_agg(occurred_at ORDER BY occurred_at DESC,source_kind DESC,source_id DESC))[1] AS next_at,
             (array_agg(source_kind ORDER BY occurred_at DESC,source_kind DESC,source_id DESC))[1] AS next_kind,
             (array_agg(source_id ORDER BY occurred_at DESC,source_kind DESC,source_id DESC))[1] AS next_id
      FROM eligible
    )
    SELECT (SELECT COUNT(*) FROM inserted),source_count=LEAST(GREATEST(p_limit,1),256),
           next_at,next_kind,next_id
      INTO v_count,v_has_more,v_next_at,v_next_kind,v_next_id
      FROM batch_state;

    IF v_next_at IS NOT NULL THEN
      UPDATE user_notification_source_bootstrap_state
      SET cursor_occurred_at=v_next_at,cursor_source_kind=v_next_kind,
          cursor_source_id=v_next_id,
          completed_at=CASE WHEN v_has_more THEN NULL ELSE statement_timestamp() END
      WHERE singleton=TRUE;
    ELSE
      UPDATE user_notification_source_bootstrap_state
      SET completed_at=statement_timestamp() WHERE singleton=TRUE;
    END IF;
    RETURN v_count;
END;
$$;

-- Bootstrap reads deployment failures in global occurrence order. This
-- partial index keeps each cursor batch proportional to its 256-row limit.
CREATE INDEX IF NOT EXISTS system_events_deployment_failure_bootstrap_idx
ON system_events(occurred_at,id)
WHERE event_type='cf_deployment_failed';

-- The unique constraints already provide these exact access paths. Remove the
-- duplicate non-unique indexes without changing either uniqueness invariant.
DROP INDEX IF EXISTS poam_findings_system_policy_idx;
DROP INDEX IF EXISTS poam_links_poam_active_idx;

-- Relationship pagination uses immutable relationship timestamps and stable
-- IDs. These indexes keep each per-finding or per-assignment page bounded
-- without depending on mutable POA&M metadata.
CREATE INDEX poam_finding_links_history_order_idx
ON poam_finding_links(finding_id,linked_at DESC,id DESC)
INCLUDE(poam_id,retired_at);

CREATE INDEX poam_assignment_references_history_order_idx
ON poam_assignment_references(assignment_version_id,added_at DESC,poam_id DESC);

-- A source-neutral legacy observation has no composite assessment identity.
-- Bind its waiver to the same immutable observation token and snapshot used by
-- creation, verification, and closure instead of fabricating an assessment.
ALTER TABLE finding_waivers
  ALTER COLUMN assessment_id DROP NOT NULL;

ALTER TABLE poam_verification_items
  DROP CONSTRAINT poam_verification_items_accepted_evidence,
  ADD CONSTRAINT poam_verification_items_accepted_evidence CHECK (
    result NOT IN ('pass','waiver') OR (
      policy_version_id IS NOT NULL
      AND derivation_id IS NOT NULL
      AND target_store_path IS NOT NULL
      AND effective_set_digest IS NOT NULL
      AND effective_config_digest IS NOT NULL
      AND effective_config IS NOT NULL
      AND observed_outcome IS NOT NULL
      AND observation_token IS NOT NULL
      AND observation_snapshot IS NOT NULL
      AND assessment_updated_at IS NOT NULL
      AND (
        (assessment_id IS NOT NULL AND effective_context_attestation_id IS NULL)
        OR (assessment_id IS NULL AND effective_context_attestation_id IS NOT NULL)
      )
    )
  );

CREATE OR REPLACE FUNCTION protect_finding_waiver_evidence()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' THEN
        RAISE EXCEPTION 'Finding waiver evidence is immutable';
    END IF;
    IF NEW.finding_id<>OLD.finding_id OR NEW.justification<>OLD.justification
       OR NEW.policy_version_id<>OLD.policy_version_id
       OR NEW.assessment_id IS DISTINCT FROM OLD.assessment_id
       OR NEW.observation_token<>OLD.observation_token
       OR NEW.observation_snapshot IS DISTINCT FROM OLD.observation_snapshot
       OR NEW.created_by<>OLD.created_by
       OR NEW.created_at<>OLD.created_at
       OR (OLD.accepted_by IS NOT NULL AND (NEW.accepted_by IS DISTINCT FROM OLD.accepted_by
           OR NEW.accepted_at IS DISTINCT FROM OLD.accepted_at)) THEN
        RAISE EXCEPTION 'Finding waiver identity and acceptance evidence are immutable';
    END IF;
    RETURN NEW;
END;
$$;

-- Rechecks a source-neutral observation against the current deployed source
-- rows. Effective resolver output is independently bound by the attestation.
CREATE OR REPLACE FUNCTION poam_legacy_observation_is_authoritative(
    item poam_verification_items,
    expected_pass boolean
) RETURNS boolean LANGUAGE sql STABLE AS $$
SELECT EXISTS (
  SELECT 1
  FROM systems system
  JOIN LATERAL (
    SELECT state.store_path,state.timestamp
    FROM system_states state
    WHERE state.hostname=system.hostname
      AND state.store_path IS NOT NULL AND btrim(state.store_path)<>''
    ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
  ) deployed ON deployed.store_path=item.target_store_path
  JOIN derivations derivation
    ON derivation.id=item.derivation_id
   AND derivation.derivation_type='nixos'
   AND COALESCE(derivation.store_path,derivation.expected_store_path)=deployed.store_path
  JOIN deployment_policy_versions policy_version
    ON policy_version.id=item.policy_version_id
   AND policy_version.policy_id=item.policy_lineage_id
  WHERE system.id=item.system_id
    AND item.assessment_updated_at=deployed.timestamp
    AND (
      (policy_version.policy_type IN ('require_packages','custom_check','require_cf_agent')
       AND derivation.policy_results->'assigned'
             ->item.policy_version_id::text->>'passed'=expected_pass::text
       AND jsonb_typeof(derivation.policy_results->'assigned'
             ->item.policy_version_id::text)='object'
       AND (
         NOT (derivation.policy_results->'assigned'
              ->item.policy_version_id::text ? 'details')
         OR jsonb_typeof(derivation.policy_results->'assigned'
              ->item.policy_version_id::text->'details') IN ('string','null')
       )
       AND item.observation_snapshot=jsonb_build_object(
         'source','nix_policy_result',
         'system_id',item.system_id,
         'policy_lineage_id',item.policy_lineage_id,
         'policy_version_id',item.policy_version_id,
         'effective_set_digest',item.effective_set_digest,
         'effective_config_digest',item.effective_config_digest,
         'derivation_id',item.derivation_id,
         'target_store_path',item.target_store_path,
         'passed',expected_pass,
         'details',derivation.policy_results->'assigned'
              ->item.policy_version_id::text->'details'
       ))
      OR
      (policy_version.policy_type='require_cve_check'
       AND EXISTS (
         SELECT 1 FROM cve_scans scan
         WHERE scan.id=(item.observation_snapshot->>'scan_id')::uuid
           AND scan.derivation_id=item.derivation_id
           AND scan.status='completed'
           AND scan.id=(
             SELECT current_scan.id FROM cve_scans current_scan
             WHERE current_scan.derivation_id=item.derivation_id
               AND current_scan.status='completed'
             ORDER BY current_scan.completed_at DESC NULLS LAST,current_scan.id DESC
             LIMIT 1
           )
           AND ((
             scan.critical_count<=COALESCE(
               (item.effective_config->>'max_critical')::bigint,9223372036854775807)
             AND (item.effective_config->>'max_high' IS NULL
               OR scan.high_count<=(item.effective_config->>'max_high')::bigint)
           )=expected_pass)
           AND item.observation_snapshot=jsonb_build_object(
             'source','cve_scan',
             'system_id',item.system_id,
             'policy_lineage_id',item.policy_lineage_id,
             'policy_version_id',item.policy_version_id,
             'effective_set_digest',item.effective_set_digest,
             'effective_config_digest',item.effective_config_digest,
             'derivation_id',item.derivation_id,
             'target_store_path',item.target_store_path,
             'scan_id',scan.id,
             'critical_count',scan.critical_count,
             'high_count',scan.high_count,
             'max_critical',COALESCE(
               (item.effective_config->>'max_critical')::bigint,9223372036854775807),
             'max_high',(item.effective_config->>'max_high')::bigint
           )
       ))
    )
)
$$;

CREATE OR REPLACE FUNCTION poam_effective_attestation_matches(
    item poam_verification_items
) RETURNS boolean LANGUAGE sql STABLE AS $$
SELECT item.effective_context_attestation_id IS NOT NULL
  AND item.observation_token=encode(digest(
    canonical_poam_observation_json(item.observation_snapshot),'sha256'),'hex')
  AND item.effective_config_digest=encode(digest(
    canonical_poam_observation_json(item.effective_config),'sha256'),'hex')
  AND EXISTS (
    SELECT 1 FROM poam_effective_context_attestations attestation
    WHERE attestation.id=item.effective_context_attestation_id
      AND attestation.attempt_id=item.attempt_id
      AND attestation.finding_id=item.finding_id
      AND attestation.system_id=item.system_id
      AND attestation.policy_lineage_id=item.policy_lineage_id
      AND attestation.policy_version_id=item.policy_version_id
      AND attestation.derivation_id=item.derivation_id
      AND attestation.target_store_path=item.target_store_path
      AND attestation.effective_set_digest=item.effective_set_digest
      AND attestation.effective_config_digest=item.effective_config_digest
      AND attestation.effective_config=item.effective_config
      AND attestation.observed_outcome=item.observed_outcome
      AND attestation.observation_token=item.observation_token
      AND attestation.observation_snapshot=item.observation_snapshot
  )
$$;

CREATE OR REPLACE FUNCTION validate_poam_closure_evidence()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.status='completed' AND (
    NOT EXISTS (
      SELECT 1 FROM poam_verification_attempts attempt
      WHERE attempt.id=NEW.closure_attempt_id AND attempt.poam_id=NEW.id
        AND attempt.outcome='accepted' AND attempt.sealed_at IS NOT NULL
    )
    OR NOT EXISTS (
      SELECT 1 FROM poam_verification_items item
      WHERE item.attempt_id=NEW.closure_attempt_id
    )
    OR EXISTS (
      (SELECT link.finding_id FROM poam_finding_links link
       WHERE link.poam_id=NEW.id
         AND link.retirement_reason='closed:'||NEW.closure_attempt_id::text)
      EXCEPT
      (SELECT item.finding_id FROM poam_verification_items item
       WHERE item.attempt_id=NEW.closure_attempt_id)
    )
    OR EXISTS (
      (SELECT item.finding_id FROM poam_verification_items item
       WHERE item.attempt_id=NEW.closure_attempt_id)
      EXCEPT
      (SELECT link.finding_id FROM poam_finding_links link
       WHERE link.poam_id=NEW.id
         AND link.retirement_reason='closed:'||NEW.closure_attempt_id::text)
    )
    OR EXISTS (
      SELECT 1 FROM poam_verification_items item
      WHERE item.attempt_id=NEW.closure_attempt_id
        AND NOT (
          (item.result='pass' AND item.observed_outcome='pass'
           AND item.waiver_id IS NULL AND (
             (item.assessment_id IS NOT NULL
              AND item.effective_context_attestation_id IS NULL
              AND EXISTS (
                SELECT 1 FROM composite_policy_assessments assessment
                WHERE assessment.id=item.assessment_id
                  AND assessment.system_id=item.system_id
                  AND assessment.policy_lineage_id=item.policy_lineage_id
                  AND assessment.policy_version_id=item.policy_version_id
                  AND assessment.derivation_id=item.derivation_id
                  AND assessment.target_store_path=item.target_store_path
                  AND assessment.effective_set_digest=item.effective_set_digest
                  AND assessment.effective_config_digest=item.effective_config_digest
                  AND assessment.effective_config=item.effective_config
                  AND assessment.updated_at=item.assessment_updated_at
                  AND assessment.overall_outcome='pass'
                  AND item.observation_token=encode(digest(
                    canonical_poam_observation_json(item.observation_snapshot),'sha256'),'hex')
                  AND item.observation_snapshot=jsonb_build_object(
                    'assessment',to_jsonb(assessment),
                    'rules',COALESCE((
                      SELECT jsonb_agg(to_jsonb(rule_result)
                        ORDER BY rule_result.ordinal,rule_result.rule_id)
                      FROM composite_policy_rule_results rule_result
                      WHERE rule_result.assessment_id=assessment.id
                    ),'[]'::jsonb)
                  )
              ))
             OR
             (item.assessment_id IS NULL
              AND item.effective_context_attestation_id IS NOT NULL
              AND poam_effective_attestation_matches(item)
              AND poam_legacy_observation_is_authoritative(item,TRUE))
           ))
          OR
          (item.result='waiver' AND item.observed_outcome='fail'
           AND EXISTS (
             SELECT 1 FROM finding_waivers waiver
             WHERE waiver.id=item.waiver_id AND waiver.finding_id=item.finding_id
               AND waiver.assessment_id IS NOT DISTINCT FROM item.assessment_id
               AND waiver.policy_version_id=item.policy_version_id
               AND waiver.observation_token=item.observation_token
               AND waiver.observation_snapshot=item.observation_snapshot
               AND waiver.status='accepted' AND waiver.accepted_at<=CURRENT_TIMESTAMP
               AND (waiver.expires_at IS NULL OR waiver.expires_at>CURRENT_TIMESTAMP)
           )
           AND (
             (item.assessment_id IS NOT NULL
              AND item.effective_context_attestation_id IS NULL
              AND item.observation_snapshot->'assessment'->>'overall_outcome'='fail')
             OR
             (item.assessment_id IS NULL
              AND item.effective_context_attestation_id IS NOT NULL
              AND poam_effective_attestation_matches(item)
              AND poam_legacy_observation_is_authoritative(item,FALSE))
           ))
        )
    )
  ) THEN
    RAISE EXCEPTION 'Completed POA&M closure evidence is incomplete or inconsistent'
      USING ERRCODE='23514',CONSTRAINT='poams_authoritative_closure_evidence';
  END IF;
  RETURN NEW;
END;
$$;
