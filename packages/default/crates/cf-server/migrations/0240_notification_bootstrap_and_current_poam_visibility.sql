-- Protect historical notification sources until the bounded upgrade bootstrap
-- has durably copied every eligible source into the notification source queue.
CREATE TABLE user_notification_source_bootstrap_state (
    singleton boolean PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    completed_at timestamptz NOT NULL
);

CREATE OR REPLACE FUNCTION backfill_user_notification_source_events(p_limit integer DEFAULT 256)
RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE v_count bigint;
BEGIN
    -- CONCURRENCY: The producer lock serializes this source scan with trigger
    -- enqueueing. The completion marker and the final absence check therefore
    -- commit atomically with the last bounded batch.
    PERFORM pg_advisory_xact_lock(433,237);
    WITH eligible AS MATERIALIZED (
      SELECT source_kind,source_id,occurred_at FROM (
        SELECT 'attention_occurrence'::text source_kind,occurrence.id source_id,
          occurrence.opened_at occurred_at
        FROM attention_occurrences occurrence
        WHERE occurrence.category IN ('builds','evals','cves')
           OR (occurrence.category='systems' AND EXISTS (
             SELECT 1 FROM systems system WHERE system.id=occurrence.subject_id::uuid))
           OR (occurrence.category='poams' AND EXISTS (
             SELECT 1 FROM poams poam WHERE poam.id=occurrence.subject_id::uuid))
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
    ), missing AS MATERIALIZED (
      SELECT source_kind,source_id
      FROM eligible source
      WHERE NOT EXISTS (SELECT 1 FROM user_notification_source_events queued
        WHERE queued.source_kind=source.source_kind AND queued.source_id=source.source_id)
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
      FROM missing JOIN poam_activity activity ON missing.source_kind='poam_activity' AND activity.id=missing.source_id
      JOIN poams poam ON poam.id=activity.poam_id
      UNION ALL
      SELECT 'system_event',event.id,event.occurred_at,'deploy_failures',event.id,'systems',event.system_id::text,
        'Deployment failed','A deployment entered a failed terminal state.','/systems','environments',
        COALESCE(ARRAY[system.environment_id]::uuid[],'{}')
      FROM missing JOIN system_events event ON missing.source_kind='system_event' AND event.id=missing.source_id
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
      FROM missing JOIN attention_occurrences occurrence
        ON missing.source_kind='attention_occurrence' AND occurrence.id=missing.source_id
      LEFT JOIN poams poam ON occurrence.category='poams' AND poam.id=occurrence.subject_id::uuid
      LEFT JOIN systems subject_system ON occurrence.category='systems' AND subject_system.id=occurrence.subject_id::uuid
      ON CONFLICT DO NOTHING RETURNING 1
    ) SELECT COUNT(*) INTO v_count FROM inserted;

    IF NOT EXISTS (
      SELECT 1 FROM (
        SELECT 'attention_occurrence'::text source_kind,occurrence.id source_id
        FROM attention_occurrences occurrence
        WHERE occurrence.category IN ('builds','evals','cves')
           OR (occurrence.category='systems' AND EXISTS (
             SELECT 1 FROM systems system WHERE system.id=occurrence.subject_id::uuid))
           OR (occurrence.category='poams' AND EXISTS (
             SELECT 1 FROM poams poam WHERE poam.id=occurrence.subject_id::uuid))
        UNION ALL
        SELECT 'poam_activity',activity.id FROM poam_activity activity
        JOIN poams poam ON poam.id=activity.poam_id
        WHERE activity.kind='status_changed'
          AND activity.payload->>'to'='awaiting_verification'
        UNION ALL
        SELECT 'system_event',event.id FROM system_events event
        JOIN systems system ON system.id=event.system_id
        WHERE event.event_type='cf_deployment_failed'
      ) source
      WHERE NOT EXISTS (SELECT 1 FROM user_notification_source_events queued
        WHERE queued.source_kind=source.source_kind AND queued.source_id=source.source_id)
    ) THEN
      INSERT INTO user_notification_source_bootstrap_state(singleton,completed_at)
      VALUES(TRUE,statement_timestamp()) ON CONFLICT(singleton) DO NOTHING;
    END IF;
    RETURN v_count;
END;
$$;

CREATE OR REPLACE FUNCTION cleanup_attention_occurrences(
    resolved_retention INTERVAL DEFAULT INTERVAL '30 days',
    batch_size INT DEFAULT 1000
)
RETURNS TABLE (deleted_occurrences BIGINT, deleted_dismissals BIGINT)
LANGUAGE plpgsql
AS $$
DECLARE deleted_occ BIGINT; deleted_dis BIGINT;
BEGIN
    -- INVARIANT: Queue rows survive source cleanup. Until bounded bootstrap has
    -- proved that history is queued, cleanup must preserve every source row.
    IF NOT EXISTS (SELECT 1 FROM user_notification_source_bootstrap_state WHERE singleton) THEN
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

-- Current authorization includes closure-retired findings, but an unlinked or
-- otherwise retired historical finding no longer contributes current context.
CREATE OR REPLACE VIEW poam_context_systems AS
SELECT link.poam_id,finding.system_id
FROM poam_current_finding_links link
JOIN poam_findings finding ON finding.id=link.finding_id
UNION
SELECT reference.poam_id,system.id
FROM poam_assignment_references reference
JOIN compliance_bundle_assignment_versions version ON version.id=reference.assignment_version_id
JOIN compliance_bundle_assignments assignment ON assignment.id=version.assignment_id
JOIN systems system ON system.id=assignment.system_id
   OR system.environment_id=assignment.environment_id;

CREATE OR REPLACE FUNCTION poam_visible_to_environments(
    v_poam_id uuid,
    v_environment_ids uuid[]
) RETURNS boolean LANGUAGE sql STABLE AS $$
SELECT (EXISTS (SELECT 1 FROM poam_current_finding_links WHERE poam_id=v_poam_id)
        OR EXISTS (SELECT 1 FROM poam_assignment_references WHERE poam_id=v_poam_id))
  AND NOT EXISTS (
    SELECT 1 FROM poam_context_systems context
    JOIN systems system ON system.id=context.system_id
    WHERE context.poam_id=v_poam_id
      AND (system.environment_id IS NULL OR NOT(system.environment_id=ANY(v_environment_ids))))
  AND NOT EXISTS (
    SELECT 1 FROM poam_assignment_references reference
    JOIN compliance_bundle_assignment_versions version ON version.id=reference.assignment_version_id
    JOIN compliance_bundle_assignments assignment ON assignment.id=version.assignment_id
    LEFT JOIN systems assigned_system ON assigned_system.id=assignment.system_id
    WHERE reference.poam_id=v_poam_id
      AND (COALESCE(assignment.environment_id,assigned_system.environment_id) IS NULL
        OR NOT(COALESCE(assignment.environment_id,assigned_system.environment_id)=ANY(v_environment_ids))));
$$;

-- SECURITY: The server is the sole supported persistence writer. These
-- constraints reject malformed or accidental same-transaction POA&M writes.
-- The database owner and superusers are trusted because they can disable or
-- replace triggers and constraints; this design does not resist those actors.
COMMENT ON TABLE compliance_resolved_effective_contexts IS
  'Server-owned resolver output used to validate same-transaction POA&M attestations. The database owner and superusers are trusted.';
COMMENT ON TABLE poam_effective_context_attestations IS
  'Server-written immutable POA&M evidence. Constraints reject malformed writes but do not protect against the trusted database owner or superusers.';
