-- Queue identity is a commit-ordered high-water mark. Every producer takes the
-- same transaction advisory lock before identity allocation. The lock remains
-- held through commit, so a later id cannot commit before an earlier id and be
-- consumed past an uncommitted row.
CREATE TABLE user_notification_source_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_kind text NOT NULL CHECK (
        source_kind IN ('attention_occurrence', 'poam_activity', 'system_event')
    ),
    source_id uuid NOT NULL,
    occurred_at timestamptz NOT NULL,
    category text NOT NULL CHECK (category IN (
        'deploy_failures', 'build_failures', 'critical_cves',
        'policy_violations', 'heartbeat_lost'
    )),
    source_occurrence_id uuid,
    notification_source_type text NOT NULL,
    notification_source_id text NOT NULL,
    title text NOT NULL,
    summary text NOT NULL,
    route text NOT NULL,
    authorization_scope text NOT NULL CHECK (
        authorization_scope IN ('global', 'environments')
    ),
    authorization_environment_ids uuid[] NOT NULL DEFAULT '{}',
    UNIQUE (source_kind, source_id)
);

CREATE OR REPLACE FUNCTION enqueue_user_notification_source_event()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_poam_id uuid;
    v_poam_number bigint;
    v_poam_title text;
    v_environment_ids uuid[] := '{}';
BEGIN
    -- CONCURRENCY: This transaction-level lock serializes identity allocation
    -- with commit. Consumers may therefore advance a durable id cursor without
    -- skipping a lower id that is still uncommitted.
    PERFORM pg_advisory_xact_lock(433, 237);

    IF TG_TABLE_NAME = 'attention_occurrences' THEN
        IF NEW.category NOT IN ('builds', 'evals', 'cves', 'systems', 'poams') THEN
            RETURN NEW;
        END IF;
        IF NEW.category = 'poams' THEN
            v_poam_id := NEW.subject_id::uuid;
            SELECT poam.human_number, poam.title,
                   COALESCE(array_agg(DISTINCT system.environment_id)
                       FILTER (WHERE system.environment_id IS NOT NULL), '{}')
            INTO v_poam_number, v_poam_title, v_environment_ids
            FROM poams poam
            LEFT JOIN poam_finding_links link
              ON link.poam_id = poam.id AND link.retired_at IS NULL
            LEFT JOIN poam_findings finding ON finding.id = link.finding_id
            LEFT JOIN systems system ON system.id = finding.system_id
            WHERE poam.id = v_poam_id
            GROUP BY poam.id;
            IF v_poam_number IS NULL THEN
                RETURN NEW;
            END IF;
        ELSIF NEW.category = 'systems' THEN
            SELECT COALESCE(array_agg(environment_id)
                       FILTER (WHERE environment_id IS NOT NULL), '{}')
            INTO v_environment_ids
            FROM systems WHERE id = NEW.subject_id::uuid;
        END IF;

        INSERT INTO user_notification_source_events(
            source_kind, source_id, occurred_at, category,
            source_occurrence_id, notification_source_type,
            notification_source_id, title, summary, route,
            authorization_scope, authorization_environment_ids
        ) VALUES (
            'attention_occurrence', NEW.id, NEW.opened_at,
            CASE NEW.category
                WHEN 'builds' THEN 'build_failures'
                WHEN 'evals' THEN 'policy_violations'
                WHEN 'cves' THEN 'critical_cves'
                WHEN 'systems' THEN 'heartbeat_lost'
                ELSE 'policy_violations'
            END,
            NEW.id,
            CASE WHEN NEW.category = 'poams' THEN 'poams' ELSE NEW.category END,
            NEW.subject_id,
            CASE NEW.category
                WHEN 'builds' THEN 'Build failed'
                WHEN 'evals' THEN 'Policy or evaluation failure'
                WHEN 'cves' THEN 'New critical CVE'
                WHEN 'systems' THEN 'Heartbeat lost'
                ELSE 'POAM-' || lpad(v_poam_number::text, 4, '0') || ' overdue'
            END,
            CASE NEW.category
                WHEN 'builds' THEN 'A build entered a failed terminal state.'
                WHEN 'evals' THEN 'An evaluation or policy check entered a failed state.'
                WHEN 'cves' THEN 'A critical CVE attention episode opened.'
                WHEN 'systems' THEN 'A system crossed an offline or lost-heartbeat threshold.'
                ELSE v_poam_title || ' passed its target date.'
            END,
            CASE NEW.category
                WHEN 'builds' THEN '/builds'
                WHEN 'evals' THEN '/evaluations'
                WHEN 'cves' THEN '/cves'
                WHEN 'systems' THEN '/systems'
                ELSE '/compliance?poam=' || v_poam_id::text
            END,
            CASE WHEN NEW.category IN ('builds', 'evals', 'cves')
                 THEN 'global' ELSE 'environments' END,
            v_environment_ids
        ) ON CONFLICT DO NOTHING;
    ELSIF TG_TABLE_NAME = 'poam_activity' THEN
        IF NEW.kind <> 'status_changed'
           OR NEW.payload->>'to' IS DISTINCT FROM 'awaiting_verification' THEN
            RETURN NEW;
        END IF;
        SELECT poam.human_number, poam.title,
               COALESCE(array_agg(DISTINCT system.environment_id)
                   FILTER (WHERE system.environment_id IS NOT NULL), '{}')
        INTO v_poam_number, v_poam_title, v_environment_ids
        FROM poams poam
        LEFT JOIN poam_finding_links link
          ON link.poam_id = poam.id AND link.retired_at IS NULL
        LEFT JOIN poam_findings finding ON finding.id = link.finding_id
        LEFT JOIN systems system ON system.id = finding.system_id
        WHERE poam.id = NEW.poam_id
        GROUP BY poam.id;
        INSERT INTO user_notification_source_events(
            source_kind, source_id, occurred_at, category,
            source_occurrence_id, notification_source_type,
            notification_source_id, title, summary, route,
            authorization_scope, authorization_environment_ids
        ) VALUES (
            'poam_activity', NEW.id, NEW.created_at, 'policy_violations',
            NEW.id, 'poams', NEW.poam_id::text,
            'POAM-' || lpad(v_poam_number::text, 4, '0') || ' awaiting verification',
            v_poam_title || ' is ready for verification.',
            '/compliance?poam=' || NEW.poam_id::text,
            'environments', v_environment_ids
        ) ON CONFLICT DO NOTHING;
    ELSIF TG_TABLE_NAME = 'system_events' THEN
        IF NEW.event_type <> 'cf_deployment_failed' THEN
            RETURN NEW;
        END IF;
        SELECT COALESCE(array_agg(environment_id)
                   FILTER (WHERE environment_id IS NOT NULL), '{}')
        INTO v_environment_ids
        FROM systems WHERE id = NEW.system_id;
        INSERT INTO user_notification_source_events(
            source_kind, source_id, occurred_at, category,
            source_occurrence_id, notification_source_type,
            notification_source_id, title, summary, route,
            authorization_scope, authorization_environment_ids
        ) VALUES (
            'system_event', NEW.id, NEW.occurred_at, 'deploy_failures',
            NULL, 'system_event', NEW.id::text, 'Deployment failed',
            'A deployment entered a failed terminal state.', '/systems',
            'environments', v_environment_ids
        ) ON CONFLICT DO NOTHING;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER enqueue_attention_notification_source
    AFTER INSERT OR UPDATE OF category ON attention_occurrences
    FOR EACH ROW EXECUTE FUNCTION enqueue_user_notification_source_event();
CREATE TRIGGER enqueue_poam_activity_notification_source
    AFTER INSERT OR UPDATE OF kind, payload ON poam_activity
    FOR EACH ROW EXECUTE FUNCTION enqueue_user_notification_source_event();
CREATE TRIGGER enqueue_system_event_notification_source
    AFTER INSERT OR UPDATE OF event_type ON system_events
    FOR EACH ROW EXECUTE FUNCTION enqueue_user_notification_source_event();

-- Existing source history is not replayed in the migration transaction. An
-- unbounded migration-time backfill can hold source tables and make upgrades
-- unsafe. The producer lazily touches at most 256 missing source rows per pass;
-- these triggers then snapshot them through the normal idempotent enqueue path.

CREATE TABLE user_notification_source_cursors (
    user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    last_event_id bigint NOT NULL DEFAULT 0 CHECK (last_event_id >= 0),
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE user_notification_materialization_schedule (
    user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    last_serviced_at timestamptz NOT NULL DEFAULT '-infinity'
);

ALTER TABLE user_notifications
    ADD COLUMN materialization_order bigint GENERATED ALWAYS AS IDENTITY,
    ADD COLUMN authorization_scope text CHECK (
        authorization_scope IS NULL OR authorization_scope IN ('global', 'environments')
    ),
    ADD COLUMN authorization_environment_ids uuid[];

CREATE TABLE user_notification_immediate_email_cursors (
    user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    last_materialization_order bigint NOT NULL DEFAULT 0
        CHECK (last_materialization_order >= 0),
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE OR REPLACE FUNCTION notification_visible_to_user_snapshot(
    p_user_id uuid,
    p_source_type text,
    p_source_id text,
    p_authorization_scope text,
    p_authorization_environment_ids uuid[]
) RETURNS boolean
LANGUAGE SQL
STABLE
AS $$
    SELECT CASE
        WHEN p_authorization_scope IS NULL THEN
            notification_visible_to_user(p_user_id, p_source_type, p_source_id)
        WHEN p_authorization_scope = 'global' THEN EXISTS (
            SELECT 1 FROM users user_account
            JOIN user_role_assignments role ON role.user_id = user_account.id
            WHERE user_account.id = p_user_id AND user_account.is_active
        )
        ELSE EXISTS (
            SELECT 1 FROM users user_account
            JOIN user_role_assignments role ON role.user_id = user_account.id
            WHERE user_account.id = p_user_id AND user_account.is_active
              AND (
                  role.role = 'admin'
                  OR EXISTS (
                      SELECT 1 FROM user_environment_memberships membership
                      WHERE membership.user_id = p_user_id
                        AND membership.environment_id = ANY(
                            COALESCE(p_authorization_environment_ids, '{}')
                        )
                  )
              )
        )
    END;
$$;

CREATE INDEX user_notifications_pending_immediate_email_idx
    ON user_notifications (user_id, materialization_order)
    WHERE email_delivery_eligible;

-- This is both the immediate-delivery lookup index and the final concurrency
-- backstop. The idempotency key remains unique across all delivery types.
CREATE UNIQUE INDEX user_notification_email_deliveries_one_immediate
    ON user_notification_email_deliveries (notification_id)
    WHERE delivery_type = 'immediate';
