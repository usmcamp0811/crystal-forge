-- Queue user initialization independently from source history. A producer pass
-- can then initialize a fixed user batch without scanning all active accounts.
CREATE TABLE user_notification_initialization_queue (
    user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    initial_last_event_id bigint NOT NULL CHECK (initial_last_event_id >= 0),
    enqueued_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE user_notification_initialization_queue IS
  'Durable active-user work queue. Producers lock and remove at most their global user limit per pass; initial_last_event_id is the first-pass source high-water mark.';

-- Existing accounts can require rolling upgrade work. Start them at zero to
-- preserve every source that is eligible under their existing time boundaries.
INSERT INTO user_notification_initialization_queue(user_id,initial_last_event_id)
SELECT account.id,0
FROM users account
WHERE account.is_active
  AND (
    NOT EXISTS (SELECT 1 FROM user_notification_preferences preference
                WHERE preference.user_id=account.id)
    OR NOT EXISTS (SELECT 1 FROM user_notification_materialization_schedule schedule
                   WHERE schedule.user_id=account.id)
    OR NOT EXISTS (SELECT 1 FROM user_notification_source_cursors cursor
                   WHERE cursor.user_id=account.id)
  )
ON CONFLICT(user_id) DO NOTHING;

CREATE OR REPLACE FUNCTION enqueue_user_notification_initialization()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.is_active AND (TG_OP='INSERT' OR NOT OLD.is_active) THEN
    -- INVARIANT: New eligibility starts after the current commit-ordered source
    -- high-water mark. A concurrent uncommitted source has a higher cursor or
    -- remains below this mark and is therefore still consumed safely.
    INSERT INTO user_notification_initialization_queue(user_id,initial_last_event_id)
    VALUES(NEW.id,COALESCE((SELECT MAX(id) FROM user_notification_source_events),0))
    ON CONFLICT(user_id) DO NOTHING;
  ELSIF TG_OP='UPDATE' AND NOT NEW.is_active THEN
    DELETE FROM user_notification_initialization_queue WHERE user_id=NEW.id;
  END IF;
  RETURN NEW;
END;
$$;

COMMENT ON FUNCTION enqueue_user_notification_initialization() IS
  'Queues new or reactivated users at the current commit-ordered source cursor and removes inactive users before producer selection.';

CREATE TRIGGER enqueue_user_notification_initialization
AFTER INSERT OR UPDATE OF is_active ON users
FOR EACH ROW EXECUTE FUNCTION enqueue_user_notification_initialization();

CREATE INDEX user_notification_initialization_queue_order_idx
ON user_notification_initialization_queue(enqueued_at,user_id);

CREATE INDEX user_notification_materialization_schedule_order_idx
ON user_notification_materialization_schedule(last_serviced_at,user_id);

CREATE INDEX user_notification_immediate_email_cursor_order_idx
ON user_notification_immediate_email_cursors(updated_at,user_id);

-- Each source branch must apply its keyset cursor and limit before the final
-- merge. The merge therefore sorts at most three bounded source batches rather
-- than complete attention, activity, and system histories.
CREATE INDEX attention_occurrences_notification_bootstrap_idx
ON attention_occurrences(opened_at,id)
WHERE category IN ('builds','evals','cves','systems','poams');

CREATE OR REPLACE FUNCTION notification_source_bootstrap_batch(
    p_cursor_at timestamptz,
    p_cursor_kind text,
    p_cursor_id uuid,
    p_limit integer DEFAULT 256
)
RETURNS TABLE(source_kind text,source_id uuid,occurred_at timestamptz)
LANGUAGE sql STABLE AS $$
WITH attention_batch AS MATERIALIZED (
  SELECT 'attention_occurrence'::text AS source_kind,occurrence.id AS source_id,
         occurrence.opened_at AS occurred_at
  FROM attention_occurrences occurrence
  LEFT JOIN systems subject_system ON subject_system.id=CASE
    WHEN occurrence.category='systems'
     AND occurrence.subject_id~*'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    THEN occurrence.subject_id::uuid END
  LEFT JOIN poams subject_poam ON subject_poam.id=CASE
    WHEN occurrence.category='poams'
     AND occurrence.subject_id~*'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    THEN occurrence.subject_id::uuid END
  WHERE occurrence.category IN ('builds','evals','cves','systems','poams')
    AND (occurrence.category IN ('builds','evals','cves')
         OR subject_system.id IS NOT NULL OR subject_poam.id IS NOT NULL)
    AND (p_cursor_at IS NULL OR
         (occurrence.opened_at,'attention_occurrence'::text,occurrence.id)>
         (p_cursor_at,p_cursor_kind,p_cursor_id))
  ORDER BY occurrence.opened_at,occurrence.id
  LIMIT LEAST(GREATEST(p_limit,1),256)
), activity_batch AS MATERIALIZED (
  SELECT 'poam_activity'::text,activity.id,activity.created_at
  FROM poam_activity activity
  JOIN poams poam ON poam.id=activity.poam_id
  WHERE activity.kind='status_changed'
    AND activity.payload->>'to'='awaiting_verification'
    AND (p_cursor_at IS NULL OR
         (activity.created_at,'poam_activity'::text,activity.id)>
         (p_cursor_at,p_cursor_kind,p_cursor_id))
  ORDER BY activity.created_at,activity.id
  LIMIT LEAST(GREATEST(p_limit,1),256)
), system_batch AS MATERIALIZED (
  SELECT 'system_event'::text,event.id,event.occurred_at
  FROM system_events event
  JOIN systems system ON system.id=event.system_id
  WHERE event.event_type='cf_deployment_failed'
    AND (p_cursor_at IS NULL OR
         (event.occurred_at,'system_event'::text,event.id)>
         (p_cursor_at,p_cursor_kind,p_cursor_id))
  ORDER BY event.occurred_at,event.id
  LIMIT LEAST(GREATEST(p_limit,1),256)
)
SELECT * FROM attention_batch
UNION ALL SELECT * FROM activity_batch
UNION ALL SELECT * FROM system_batch
ORDER BY occurred_at,source_kind,source_id
LIMIT LEAST(GREATEST(p_limit,1),256)
$$;

COMMENT ON FUNCTION notification_source_bootstrap_batch(timestamptz,text,uuid,integer) IS
  'Returns at most 256 eligible notification sources after an immutable keyset cursor. Each source history is index-cursor limited before the bounded merge.';

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
      SELECT * FROM notification_source_bootstrap_batch(
        v_cursor_at,v_cursor_kind,v_cursor_id,p_limit)
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
