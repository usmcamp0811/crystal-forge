-- Resume notification-source bootstrap from a durable high-water mark. The
-- prior implementation limited inserts but rescanned all historical sources on
-- every batch while holding the producer lock.
ALTER TABLE user_notification_source_bootstrap_state
    ALTER COLUMN completed_at DROP NOT NULL,
    ADD COLUMN cursor_occurred_at timestamptz,
    ADD COLUMN cursor_source_kind text,
    ADD COLUMN cursor_source_id uuid;

INSERT INTO user_notification_source_bootstrap_state(singleton,completed_at)
VALUES(TRUE,NULL)
ON CONFLICT(singleton) DO NOTHING;

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
      LEFT JOIN poams poam ON occurrence.category='poams' AND poam.id=occurrence.subject_id::uuid
      LEFT JOIN systems subject_system ON occurrence.category='systems' AND subject_system.id=occurrence.subject_id::uuid
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
