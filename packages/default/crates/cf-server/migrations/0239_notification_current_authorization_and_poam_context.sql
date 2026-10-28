-- Final notification authorization and source-neutral POA&M evidence hardening.

-- Authorization always uses current domain state. Snapshot environment IDs are
-- retained only so queued content survives source cleanup; they never grant
-- access. Deployment events identify their system so event cleanup does not
-- remove the current authorization subject.
UPDATE user_notification_source_events queued
SET notification_source_type = 'systems',
    notification_source_id = event.system_id::text,
    source_occurrence_id = queued.source_id
FROM system_events event
WHERE queued.source_kind = 'system_event'
  AND event.id = queued.source_id;

UPDATE user_notifications notification
SET source_type = 'systems',
    source_id = event.system_id::text
FROM system_events event
WHERE notification.source_type = 'system_event'
  AND event.id::text = notification.source_id;

CREATE OR REPLACE FUNCTION notification_visible_to_user_snapshot(
    p_user_id uuid,
    p_source_type text,
    p_source_id text,
    p_authorization_scope text,
    p_authorization_environment_ids uuid[]
) RETURNS boolean
LANGUAGE sql
STABLE
AS $$
    -- SECURITY: Snapshot scope is content history, not an authorization grant.
    -- POA&M authorization evaluates every current finding and assignment
    -- context through poam_visible_to_environments. System authorization uses
    -- the system's current environment.
    SELECT notification_visible_to_user(p_user_id, p_source_type, p_source_id)
$$;

CREATE OR REPLACE FUNCTION enqueue_user_notification_source_event()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_poam_id uuid;
    v_poam_number bigint;
    v_poam_title text;
    v_environment_ids uuid[] := '{}';
BEGIN
    PERFORM pg_advisory_xact_lock(433, 237);
    IF TG_TABLE_NAME = 'attention_occurrences' THEN
        IF NEW.category NOT IN ('builds', 'evals', 'cves', 'systems', 'poams') THEN RETURN NEW; END IF;
        IF NEW.category = 'poams' THEN
            v_poam_id := NEW.subject_id::uuid;
            SELECT poam.human_number,poam.title,COALESCE(array_agg(DISTINCT system.environment_id)
                FILTER (WHERE system.environment_id IS NOT NULL),'{}')
            INTO v_poam_number,v_poam_title,v_environment_ids
            FROM poams poam LEFT JOIN poam_context_systems context ON context.poam_id=poam.id
            LEFT JOIN systems system ON system.id=context.system_id
            WHERE poam.id=v_poam_id GROUP BY poam.id;
            IF v_poam_number IS NULL THEN RETURN NEW; END IF;
        ELSIF NEW.category = 'systems' THEN
            SELECT COALESCE(array_agg(environment_id) FILTER (WHERE environment_id IS NOT NULL),'{}')
            INTO v_environment_ids FROM systems WHERE id=NEW.subject_id::uuid;
        END IF;
        INSERT INTO user_notification_source_events(
            source_kind,source_id,occurred_at,category,source_occurrence_id,
            notification_source_type,notification_source_id,title,summary,route,
            authorization_scope,authorization_environment_ids)
        VALUES('attention_occurrence',NEW.id,NEW.opened_at,
            CASE NEW.category WHEN 'builds' THEN 'build_failures' WHEN 'evals' THEN 'policy_violations'
              WHEN 'cves' THEN 'critical_cves' WHEN 'systems' THEN 'heartbeat_lost' ELSE 'policy_violations' END,
            NEW.id,CASE WHEN NEW.category='poams' THEN 'poams' ELSE NEW.category END,NEW.subject_id,
            CASE NEW.category WHEN 'builds' THEN 'Build failed' WHEN 'evals' THEN 'Policy or evaluation failure'
              WHEN 'cves' THEN 'New critical CVE' WHEN 'systems' THEN 'Heartbeat lost'
              ELSE 'POAM-'||lpad(v_poam_number::text,4,'0')||' overdue' END,
            CASE NEW.category WHEN 'builds' THEN 'A build entered a failed terminal state.'
              WHEN 'evals' THEN 'An evaluation or policy check entered a failed state.'
              WHEN 'cves' THEN 'A critical CVE attention episode opened.'
              WHEN 'systems' THEN 'A system crossed an offline or lost-heartbeat threshold.'
              ELSE v_poam_title||' passed its target date.' END,
            CASE NEW.category WHEN 'builds' THEN '/builds' WHEN 'evals' THEN '/evaluations'
              WHEN 'cves' THEN '/cves' WHEN 'systems' THEN '/systems'
              ELSE '/compliance?poam='||v_poam_id::text END,
            CASE WHEN NEW.category IN ('builds','evals','cves') THEN 'global' ELSE 'environments' END,
            v_environment_ids) ON CONFLICT DO NOTHING;
    ELSIF TG_TABLE_NAME = 'poam_activity' THEN
        IF NEW.kind<>'status_changed' OR NEW.payload->>'to' IS DISTINCT FROM 'awaiting_verification' THEN RETURN NEW; END IF;
        SELECT poam.human_number,poam.title,COALESCE(array_agg(DISTINCT system.environment_id)
            FILTER (WHERE system.environment_id IS NOT NULL),'{}')
        INTO v_poam_number,v_poam_title,v_environment_ids
        FROM poams poam LEFT JOIN poam_context_systems context ON context.poam_id=poam.id
        LEFT JOIN systems system ON system.id=context.system_id
        WHERE poam.id=NEW.poam_id GROUP BY poam.id;
        INSERT INTO user_notification_source_events(source_kind,source_id,occurred_at,category,
            source_occurrence_id,notification_source_type,notification_source_id,title,summary,route,
            authorization_scope,authorization_environment_ids)
        VALUES('poam_activity',NEW.id,NEW.created_at,'policy_violations',NEW.id,'poams',NEW.poam_id::text,
            'POAM-'||lpad(v_poam_number::text,4,'0')||' awaiting verification',
            v_poam_title||' is ready for verification.','/compliance?poam='||NEW.poam_id::text,
            'environments',v_environment_ids) ON CONFLICT DO NOTHING;
    ELSIF TG_TABLE_NAME = 'system_events' THEN
        IF NEW.event_type<>'cf_deployment_failed' THEN RETURN NEW; END IF;
        SELECT COALESCE(array_agg(environment_id) FILTER (WHERE environment_id IS NOT NULL),'{}')
        INTO v_environment_ids FROM systems WHERE id=NEW.system_id;
        INSERT INTO user_notification_source_events(source_kind,source_id,occurred_at,category,
            source_occurrence_id,notification_source_type,notification_source_id,title,summary,route,
            authorization_scope,authorization_environment_ids)
        VALUES('system_event',NEW.id,NEW.occurred_at,'deploy_failures',NEW.id,'systems',NEW.system_id::text,
            'Deployment failed','A deployment entered a failed terminal state.','/systems',
            'environments',v_environment_ids) ON CONFLICT DO NOTHING;
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION backfill_user_notification_source_events(p_limit integer DEFAULT 256)
RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE v_count bigint;
BEGIN
    -- CONCURRENCY: Acquire the queue lock before reading any source. The
    -- function only takes MVCC reads and inserts queue rows. It never locks or
    -- updates source rows, so it cannot form a queue-lock/source-row cycle with
    -- producer triggers. Queue uniqueness makes interrupted passes retry-safe.
    PERFORM pg_advisory_xact_lock(433,237);
    WITH missing AS MATERIALIZED (
      SELECT source_kind,source_id FROM (
        SELECT 'attention_occurrence'::text source_kind,id source_id,opened_at occurred_at
        FROM attention_occurrences WHERE category IN ('builds','evals','cves','systems','poams')
        UNION ALL
        SELECT 'poam_activity',id,created_at FROM poam_activity
        WHERE kind='status_changed' AND payload->>'to'='awaiting_verification'
        UNION ALL
        SELECT 'system_event',id,occurred_at FROM system_events WHERE event_type='cf_deployment_failed'
      ) source WHERE NOT EXISTS (SELECT 1 FROM user_notification_source_events queued
        WHERE queued.source_kind=source.source_kind AND queued.source_id=source.source_id)
      ORDER BY occurred_at,source_kind,source_id LIMIT LEAST(GREATEST(p_limit,1),256)
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
    RETURN v_count;
END;
$$;

-- The Rust resolver writes its output to a table outside the POA&M DML surface
-- before it inserts an attestation. The attestation trigger accepts only an
-- exact attempt/finding-bound match. This prevents a role that can write POA&M
-- tables, but cannot write this resolver-output table, from self-attesting a
-- chosen policy version, set digest, or effective config.
--
-- PostgreSQL owners and superusers can bypass table privileges and triggers.
-- This boundary does not claim resistance to either actor. Deployments that
-- grant direct SQL access must use a runtime role whose grants exclude this
-- table; using the migration-owner role as an interactive writer voids this
-- boundary.
CREATE TABLE compliance_resolved_effective_contexts (
    attempt_id uuid NOT NULL REFERENCES poam_verification_attempts(id) ON DELETE RESTRICT,
    finding_id uuid NOT NULL,
    system_id uuid NOT NULL,
    policy_lineage_id uuid NOT NULL,
    policy_version_id uuid NOT NULL,
    derivation_id integer NOT NULL,
    target_store_path text NOT NULL,
    effective_set_digest text NOT NULL,
    effective_config_digest text NOT NULL,
    effective_config jsonb NOT NULL,
    observed_outcome text NOT NULL,
    observation_token text NOT NULL,
    observation_snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (attempt_id,finding_id),
    FOREIGN KEY (finding_id,system_id,policy_lineage_id)
      REFERENCES poam_findings(id,system_id,policy_lineage_id) ON DELETE RESTRICT
);

CREATE OR REPLACE FUNCTION protect_compliance_resolved_effective_context()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE v_sealed_at timestamptz;
BEGIN
    IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'Resolved POA&M effective context is immutable'; END IF;
    SELECT sealed_at INTO v_sealed_at FROM poam_verification_attempts
      WHERE id=NEW.attempt_id FOR UPDATE;
    IF v_sealed_at IS NOT NULL THEN
      RAISE EXCEPTION 'A sealed POA&M attempt cannot accept resolved context';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_compliance_resolved_effective_context
  BEFORE INSERT OR UPDATE OR DELETE ON compliance_resolved_effective_contexts
  FOR EACH ROW EXECUTE FUNCTION protect_compliance_resolved_effective_context();

REVOKE INSERT,UPDATE,DELETE ON compliance_resolved_effective_contexts FROM PUBLIC;

CREATE OR REPLACE FUNCTION protect_poam_effective_context_attestation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_sealed_at timestamptz;
BEGIN
    IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'POA&M effective-context attestations are immutable'; END IF;
    SELECT sealed_at INTO v_sealed_at FROM poam_verification_attempts WHERE id=NEW.attempt_id FOR UPDATE;
    IF v_sealed_at IS NOT NULL THEN RAISE EXCEPTION 'A sealed POA&M attempt cannot accept an attestation'; END IF;
    IF NOT EXISTS (
      SELECT 1 FROM compliance_resolved_effective_contexts context
      WHERE context.attempt_id=NEW.attempt_id AND context.finding_id=NEW.finding_id
        AND context.system_id=NEW.system_id
        AND context.policy_lineage_id=NEW.policy_lineage_id
        AND context.policy_version_id=NEW.policy_version_id
        AND context.derivation_id=NEW.derivation_id
        AND context.target_store_path=NEW.target_store_path
        AND context.effective_set_digest=NEW.effective_set_digest
        AND context.effective_config_digest=NEW.effective_config_digest
        AND context.effective_config=NEW.effective_config
        AND context.observed_outcome=NEW.observed_outcome
        AND context.observation_token=NEW.observation_token
        AND context.observation_snapshot=NEW.observation_snapshot
    ) THEN
        RAISE EXCEPTION 'POA&M attestation does not match database-held resolver context';
    END IF;
    RETURN NEW;
END;
$$;
